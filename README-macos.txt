1| ![image](https://github.com/user-attachments/assets/5aa36739-76c8-4a34-9a9b-7e9272927f22)
2| 
3| # Qualy
4| 
5| Play rich keyboard and mouse sounds with every keystroke and click. A polyphonic, low-latency mechanical keyboard sound simulator for Windows, macOS, and Linux. Successor of Mechvibes, now with refined soundpack support and a modern UI.
6| 
7| ## Download
8| 
9| | Platform | Installer | Notes |
10| |----------|-----------|-------|
11| | **Windows** | [`Qualy-*-Setup-x64.exe`](https://github.com/Zeoirn/qualy/releases/latest) | One-click installer with automatic in-app updates (SHA-256 verified). |
12| | **Linux (Debian/Ubuntu)** | [`qualy_*_amd64.deb`](https://github.com/Zeoirn/qualy/releases/latest) | `sudo dpkg -i`. Requires: `sudo usermod -a -G input $USER` + re-log. |
13| | **Linux (any distro)** | [`qualy-*-x86_64.AppImage`](https://github.com/Zeoirn/qualy/releases/latest) | Portable (no install). Requires: `chmod +x` + same input group setup. |
14| | **macOS** | [`qualy-*-macos-arm64.dmg`](https://github.com/Zeoirn/qualy/releases/latest) | Experimental, unsigned. Right-click the app, then Open to bypass Gatekeeper. |
15| 
16| All links point to the [latest release](https://github.com/Zeoirn/qualy/releases/latest); every release also ships a `SHA256SUMS.txt` covering all assets.
17| 
18| ## Features
19| 
20| - **Polyphonic low-latency audio**: Sounds play within ~15ms of keystroke. Resampled at load time (not realtime) for high quality. 32-voice pool with soft eviction prevents clicks and clipping on complex input.
21| - **Works while app is focused**: Dedicated Raw Input worker process on Windows; in-process listeners on Linux/macOS. Keyboard and mouse capture work the same whether Qualy window is active or another app is focused.
22| - **Per-device input filtering** (Windows): Disable specific keyboards or mice in Settings, effective immediately.
23| - **Runtime output device switching**: Change audio output (headphones, speakers, virtual devices) in Settings without restarting.
24| - **Soundpack support**: Import or create custom soundpacks. Drag and drop folders into the app. Supports OGG, WAV, MP3, FLAC. Auto-converts classic Mechvibes packs (V1) to V2 format.
25| - **Ambiance sounds**: Long-playing background audio (rain, coffee shop) that moves with your selected output device.
26| - **Themes and customization**: Light/dark themes, custom logo, background images.
27| - **Tray icon and global hotkey**: `Ctrl+Alt+M` to mute/unmute. Tray icon shows mute state at a glance.
28| - **One-click verified updates** (Windows): When new versions are available, download and install with SHA-256 verification. Choose "Later" to defer, but the verified file stays ready.
29| - **Settings save correctly**: Writable state lives in `%APPDATA%\Qualy` (Windows), `~/.local/share/qualy` (Linux), or `~/Library/Application Support/Qualy` (macOS). Settings persist across restarts.
30| - **Debug section**: Live log viewer in Settings with export button for bug reports. Optional verbose mode adds per-keystroke timing for diagnosing latency.
31| 
32| ## Soundpacks
33| 
34| Soundpacks define the sounds that play for each key and mouse button. The app ships with a curated collection (Classic Mechvibes, Cherry MX Black, etc.). You can add your own.
35| 
36| ### Import a soundpack
37| 
38| 1. Download or create a soundpack folder.
39| 2. In the app, Settings > Soundpacks > "Import Soundpack" (or drag the folder in).
40| 3. Choose which soundpack to load for keyboard and mouse sounds.
41| 
42| ### Create a soundpack
43| 
44| A soundpack is a folder containing:
45| - **config.json** (required): metadata and key mappings
46| - **Audio files** (OGG, WAV, MP3, FLAC): the sounds to play
47| - **icon.png** (optional): a display icon
48| 
49| **Minimal example:**
50| ```
51| my-keyboard-pack/
52| ├── config.json
53| └── typewriter-click.ogg
54| ```
55| 
56| **config.json (V2 format):**
57| ```json
58| {
59|   "id": "typewriter-minimal",
60|   "name": "Typewriter",
61|   "author": "You",
62|   "config_version": "2",
63|   "definition_method": "single",
64|   "audio_file": "typewriter-click.ogg",
64|   "definitions": {
65|     "KeyA": { "timing": [[0, 50]] },
65|     "KeyB": { "timing": [[50, 100]] }
66|   },
67|   "options": { "recommended_volume": 0.8, "random_pitch": false },
68|   "soundpack_type": "Keyboard"
69| }
70| ```
71| 
72| Timing values are `[start_ms, end_ms]` within the audio file. For "single" method, all keys reference the same audio file with different timing windows. For "multi" method, each key can use a different audio file.
73| 
74| For complete documentation on config.json fields, key naming, templates, and troubleshooting, see [Soundpack Config.json V2 Format](docs/soundpack-config-v2.md).
75| 
76| ### Classic Mechvibes packs
77| 
78| Old V1 soundpacks (e.g., from the original Mechvibes) are auto-detected on import. The app automatically converts them to V2 format with no loss of sound quality. Your original files stay intact; the converted copy sits beside them in a generated folder.
79| 
80| ## Platform Notes
81| 
82| ### Windows
83| - SmartScreen may warn on first run (unsigned installer). Click "More info" then "Run anyway" to proceed.
84| - "Start at login" in Settings uses Task Scheduler (admin install required for per-user startup).
85| - Single-instance mutex prevents multiple app windows running at once.
86| 
87| ### Linux
88| - **Input group requirement**: The app reads raw keyboard/mouse events from `/dev/input/event*`, which requires membership in the `input` group:
89|   ```bash
90|   sudo usermod -a -G input $USER
91|   # Log out and log back in for the group change to take effect
92|   ```
| - **Wayland**: rdev (the input listener) supports Wayland as of recent versions; X11 is also fully supported.
| - **AppImage notes**: The `.AppImage` mounts read-only; writable state goes to `~/.local/share/qualy`. No FUSE required to run.
94| 
95| ### macOS
96| - **Experimental**: Not tested on real hardware. Ad-hoc signed, not notarized.
97| - **First launch**: Right-click the app, choose "Open" to bypass Gatekeeper's "unidentified developer" block.
98| - **Permissions**: global key capture needs **both** of these in System Settings > Privacy & Security, enabled for `Qualy` (or for your terminal, if you launch the binary from there):
99|   - **Accessibility**: used while the Qualy window is focused.
100|   - **Input Monitoring**: used while the window is minimized or another app is focused. Without it the app runs normally but is silent unless its window has focus.
101|   - **Check them from the menu bar icon.** Click the Qualy icon: each permission shows "Granted" in green, or a red "Grant …" line that opens System Settings at the right page when clicked.
102| - **After installing a new build**: the app is ad-hoc signed, so macOS ties its permissions to that exact binary. A rebuilt or reinstalled app keeps its toggles on but they stop working. Remove `Qualy.app` and re-add it under both Accessibility and Input Monitoring, then relaunch.
103| - **Start at login**: registers Qualy as a login item (System Settings > General > Login Items), which needs macOS 13 or later and the app installed in `/Applications`. If macOS asks, allow it.
104| - Arm64 (Apple Silicon) only in current builds. Intel builds available on request.
105| 
106| ## Privacy and Telemetry
107| 
108| Qualy sends one anonymous ping per launch to track rough usage. Nothing else leaves your device.
109| 
110| **What is sent (once per launch):**
111| - OS name and version
112| - App version
113| - Your system language (locale)
| - A random session ID (generated fresh each launch, never stored)
115| 
116| **What is never sent:**
117| - Keystrokes, mouse clicks, or any input data
118| - Soundpack names or usage
119| - File paths or personal data
120| - IP-derived identity or persistent identifiers
121| 
122| Telemetry is handled by [Aptabase](https://aptabase.com), an open-source privacy-first service. **Opt out** anytime in Settings > Privacy > "Share anonymous usage stats".
123| 
124| Debug logs stay local until you export them via the Debug button. Key names in logs are always masked (e.g., "KEY_A" instead of actual keystroke data).
125| 
126| ## Building from Source
127| 
128| ### Prerequisites
129| 
130| **All platforms:**
131| - [Rust](https://rustup.rs/) 1.88.0 or later, declared as `rust-version` in `Cargo.toml`. The latest stable works.
132| - [Dioxus CLI](https://dioxuslabs.com/learn/0.7/getting_started) 0.7.10: `cargo install dioxus-cli --version 0.7.10`. If you use [mise](https://mise.jdx.dev/), `mise install` installs the pinned toolchain.
133| 
134| **Windows:**
135| - Visual Studio Build Tools (C++ support)
136| - [Inno Setup 6](https://jrsoftware.org/isinfo.php) (for building installers)
137| 
138| **Linux (Ubuntu/Debian):**
139| ```bash
140| sudo apt-get update
141| sudo apt-get install -y \
142|     libasound2-dev \
143|     pkg-config \
144|     libwebkit2gtk-4.1-dev \
145|     libgtk-3-dev \
146|     libayatana-appindicator3-dev \
147|     librsvg2-dev \
148|     libevdev-dev \
149|     libxdo-dev \
150|     libxi-dev \
151|     libxtst-dev \
152|     libssl-dev
153| ```
154| 
155| **Linux (Fedora/RHEL):**
156| ```bash
157| sudo dnf install -y \
158|     alsa-lib-devel \
159|     pkg-config \
160|     webkit2gtk4.1-devel \
161|     gtk3-devel \
162|     libappindicator-gtk3-devel \
163|     librsvg2-devel \
164|     libevdev-devel \
165|     xdotool-devel \
166|     libXi-devel \
167|     libXtst-devel \
168|     openssl-devel
169| ```
170| 
171| **macOS:**
172| ```bash
173| xcode-select --install
174| ```
| 
| ### Build
| 
| **Development (all platforms):**
| ```bash
| git clone https://github.com/Zeoirn/qualy.git
| cd qualy
| dx serve
| ```
| 
| **Run the release binary directly (macOS/Linux):**
| 
| The app looks for `soundpacks/` next to the executable. `cargo build` copies the bundled packs to `target/release/soundpacks/` for you (see `build.rs`), so the binary runs as is:
| ```bash
| cargo build --release
| ./target/release/qualy
| ```
| To capture a log for a bug report, run it as `RUST_BACKTRACE=1 QUALY_TRACE=1 ./target/release/qualy 2>&1 | tee /tmp/qualy.log`.
| 
| On macOS the permissions apply to the app that launched it. When run from a terminal, grant that terminal app **Accessibility** and **Input Monitoring** in System Settings > Privacy & Security, then relaunch.
| 
| **Release (Windows):**
| ```bash
| .\scripts\build-windows-installer.ps1   # builds first; add -SkipBuild to reuse target\release
| # Output: dist/Qualy-0.8.1-Setup-x64.exe
| ```
| 
| **Release (Linux, AppImage and DEB):**
| ```bash
| cargo install cargo-deb                   # once; or pass --no-deb to skip the .deb
| ./scripts/build-linux-appimage.sh 0.8.1   # builds first; add --skip-build to reuse target/release
| chmod +x dist/qualy-0.8.1-x86_64.AppImage
| sudo dpkg -i dist/qualy_0.8.1_amd64.deb
| # Remember: run 'sudo usermod -a -G input $USER' and re-log before first use
| ```
| 
| **Release (macOS):**
| ```bash
| ./scripts/build-macos-app.sh 0.8.1   # builds first; add --skip-build to reuse target/release
| # Output: dist/qualy-0.8.1-macos-arm64.dmg
| ```
| 
| To run the same compile-and-test gate that CI runs (it also checks that Rust is at least the `rust-version` in `Cargo.toml`): `./scripts/ci-check.sh --release`. CI builds and packages every pull request and release.
| 
| For architecture details, see [docs/system-architecture.md](docs/system-architecture.md). For release procedures and deployment, see [docs/deployment-guide.md](docs/deployment-guide.md).
| 
| ## Troubleshooting
| 
| **No sound playing?**
| - Check if muted (tray icon or `Ctrl+Alt+M`) and that **Allow all sounds** is on in Settings. The setting is saved, so it stays off across restarts.
| - Verify a soundpack is selected in Settings > Soundpacks.
| - Check system volume.
| 
| **macOS: sounds only play while the app window is focused?**
| - Click the Qualy icon in the menu bar: a red "Grant Input Monitoring…" line means it is missing.
| - Input Monitoring is missing or stale. See the macOS permissions notes above: remove and re-add `Qualy` under both Accessibility and Input Monitoring, then relaunch. A relaunch is always needed because macOS ties it to the binary's identity at launch time.
| 
| **Hotkey (`Ctrl+Alt+M`) not working?**
| - Windows: Run as administrator (required for global hotkey).
| - Verify no other app has claimed the same hotkey.
| 
| **Linux: No keyboard input detected?**
| - Verify user is in `input` group: `groups $USER` should list `input`.
| - Re-log in for group changes to take effect.
| - Check device permissions: `ls -la /dev/input/event*` (should show `crw-rw---- root input`).
| 
| **Soundpack won't import?**
| - Ensure `config.json` is valid JSON (use a JSON validator).
| - Check that `definitions` (or `defs`) contains valid key mappings.
| - Verify audio files (OGG, WAV, MP3, FLAC) exist and are readable.
| 
| **Settings reset after restart?**
| - Settings are stored at: `%APPDATA%\Qualy` (Windows), `~/.local/share/qualy` (Linux), or `~/Library/Application Support/Qualy` (macOS). Verify these folders exist and are writable.
| 
| **macOS: Right-click Open not working?**
| - Try `codesign --deep -s - /Applications/Qualy.app` if you trust the build. Otherwise, await an official notarized release.
| 
| ## Credits
| 
| Qualy is built in Rust using [Dioxus](https://dioxuslabs.com/) (0.7), [rodio](https://github.com/RustAudio/rodio) (audio), and [rdev](https://github.com/enigo-rs/rdev) (input capture).
| 
| Based on the original [Mechvibes](https://github.com/hainguyents13/mechvibes) by hainguyents13. Distributed under the MIT License.
| 
| ## License
| 
| MIT License. See [LICENSE](LICENSE) for details.
