# Shared helpers for bash build/CI scripts. Source it, do not execute it:
#
#   source "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh"
#
# Designed for bash 3.2+ (macOS stock bash compatible) without external tools
# like `sort -V` or python.

# Prevent CDPATH from emitting directory paths on `cd`
unset CDPATH

_COMMON_DIR="$(cd -P -- "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
# Try git root first; fallback to relative directory traversal
_REPO_ROOT="$(git -C "$_COMMON_DIR" rev-parse --show-toplevel 2>/dev/null || (cd -P -- "${_COMMON_DIR}/../.." && pwd -P))"

# Helper for logging GitHub Actions errors to stderr
_ci_error() {
  echo "::error::$*" >&2
}

# Helper for standard info logging
_ci_info() {
  echo "::notice::$*"
}

# True if version $1 is >= version $2.
# Compares up to three numeric, dot-separated parts, treating missing parts as 0
# and stripping any suffix ("1.88" == "1.88.0", "1.89.0-nightly" == 1.89.0).
# Strips leading zeros to avoid octal interpretation in bash.
_version_ge() {
  local a="$1" b="$2" i x y
  local -a av bv
  local IFS=.

  # Default missing versions to 0
  [ -z "$a" ] && a="0"
  [ -z "$b" ] && b="0"

  read -r -a av <<<"$a"
  read -r -a bv <<<"$b"

  for i in 0 1 2; do
    x="${av[$i]:-0}"
    y="${bv[$i]:-0}"

    # Strip everything after first non-digit (e.g., "-nightly", "+build")
    x="${x%%[!0-9]*}"
    y="${y%%[!0-9]*}"

    # Default to 0 if empty
    x="${x:-0}"
    y="${y:-0}"

    # Strip leading zeros so bash doesn't treat numbers like 08 as octal
    x="$((10#$x))"
    y="$((10#$y))"

    [ "$x" -gt "$y" ] && return 0
    [ "$x" -lt "$y" ] && return 1
  done

  return 0
}

# Extracts a top-level or [workspace.package] key from Cargo.toml
_extract_cargo_toml_key() {
  local file="$1" key="$2"
  [ -f "$file" ] || return 1

  # Match either:
  # 1. key = "value"
  # 2. Under [workspace.package] or [package]: key = "value"
  awk -v k="$key" '
    BEGIN { in_pkg = 0; found = "" }
    /^\[(package|workspace\.package)\]/ { in_pkg = 1; next }
    /^\[/ { in_pkg = 0 }
    in_pkg && $0 ~ "^[[:space:]]*" k "[[:space:]]*=" {
      if (match($0, /"[^"]+"/)) {
        val = substr($0, RSTART + 1, RLENGTH - 2)
        if (!found) found = val
      }
    }
    # Also support top-level declarations
    !in_pkg && $0 ~ "^[[:space:]]*" k "[[:space:]]*=" {
      if (match($0, /"[^"]+"/)) {
        val = substr($0, RSTART + 1, RLENGTH - 2)
        if (!found) found = val
      }
    }
    END {
      if (found) { print found; exit 0 }
      exit 1
    }
  ' "$file"
}

# Fails unless the Rust in use is at least the `rust-version` declared in
# Cargo.toml - the crate's minimum supported Rust (MSRV floor).
verify_rust_toolchain() {
  local cargo_toml="${_REPO_ROOT}/Cargo.toml"
  local floor got

  if [ ! -f "$cargo_toml" ]; then
    _ci_error "Cargo.toml not found at ${_REPO_ROOT}"
    return 1
  fi

  floor=$(_extract_cargo_toml_key "$cargo_toml" "rust-version") || {
    _ci_error "Cargo.toml has no 'rust-version', so there is no minimum Rust to check against"
    return 1
  }

  if ! command -v rustc >/dev/null 2>&1; then
    _ci_error "rustc is not installed or not in PATH (Cargo.toml requires rust-version ${floor})"
    return 1
  fi

  got=$(rustc --version 2>/dev/null | awk '{print $2}' || true)

  if [ -z "$got" ] || ! _version_ge "$got" "$floor"; then
    _ci_error "Rust ${got:-<unknown>} is active, but Cargo.toml requires rust-version ${floor} or newer."
    echo "  Update it ('rustup update stable'), or check for an active RUSTUP_TOOLCHAIN override." >&2
    return 1
  fi

  echo "Rust toolchain OK: ${got} (satisfies rust-version ${floor} in Cargo.toml)"
}

# The Windows auto-updater picks the first release asset whose name contains
# "x64" AND ends in ".exe". Asserts that no Linux or macOS asset ever matches.
#
# Usage: assert_no_updater_collision <dir>
assert_no_updater_collision() {
  local dir="$1" f name status=0

  if [ ! -d "$dir" ]; then
    _ci_error "Directory '${dir}' does not exist"
    return 1
  fi

  for f in "$dir"/*; do
    [ -e "$f" ] || continue
    name=$(basename "$f" | tr '[:upper:]' '[:lower:]')
    case "$name" in
      *x64*.exe)
        _ci_error "Asset '${name}' collides with the Windows auto-updater filter (contains 'x64' and ends in '.exe')"
        status=1
        ;;
    esac
  done

  return $status
}

# Prints the release tag for a commit, querying the REMOTE rather than local tags.
#
#   resolve_release_tag <commit-sha> [remote]      (remote defaults to origin)
#
# - prints "v<Cargo.toml version>" if that tag points at the commit;
# - prints nothing (status 0) if no v* tag points at it (regular build);
# - prints an error and returns 1 if tags exist but do not match Cargo.toml version.
resolve_release_tag() {
  local input_ref="$1"
  local remote="${2:-origin}"
  local full_sha version refs tags expected

  if [ -z "$input_ref" ]; then
    _ci_error "resolve_release_tag requires a commit SHA or ref"
    return 1
  fi

  # Resolve input ref (branch, tag, HEAD, or short SHA) to a full 40-char SHA
  full_sha=$(git -C "$_REPO_ROOT" rev-parse --verify "${input_ref}^{commit}" 2>/dev/null) || full_sha="$input_ref"

  version=$(_extract_cargo_toml_key "${_REPO_ROOT}/Cargo.toml" "version") || {
    _ci_error "Could not find 'version' in Cargo.toml"
    return 1
  }

  # Fetch tag references from remote
  refs=$(git ls-remote --tags "$remote" 'refs/tags/v*') || {
    _ci_error "Could not list remote tags on '${remote}'"
    return 1
  }

  # Match both lightweight tags and peeled annotated tags (^{})
  tags=$(printf '%s\n' "$refs" | awk -v sha="$full_sha" '
    {
      commit = $1
      name = $2
      sub(/^refs\/tags\//, "", name)
      if (name ~ /\^\{\}$/) {
        sub(/\^\{\}$/, "", name)
        peeled[name] = commit
      } else {
        direct[name] = commit
      }
    }
    END {
      for (n in direct) {
        c = (n in peeled) ? peeled[n] : direct[n]
        if (c == sha) print n
      }
    }' | sort)

  if [ -z "$tags" ]; then
    return 0
  fi

  expected="v${version}"
  if ! printf '%s\n' "$tags" | grep -qx "$expected"; then
    local tag_list
    tag_list=$(printf '%s\n' "$tags" | tr '\n' ' ' | sed 's/[[:space:]]*$//')
    _ci_error "Commit ${input_ref} (${full_sha:0:8}) has tag(s) [${tag_list}], but Cargo.toml says version is ${version} (expected tag '${expected}')."
    return 1
  fi

  echo "$expected"
}
