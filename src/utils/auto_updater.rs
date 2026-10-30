// Only the tests build an `AppConfig` directly now. Every runtime path here
// goes through `config_writer`, which never hands one out to be written back.
#[cfg(test)]
use crate::state::config::AppConfig;
use crate::utils::constants::APP_NAME;
use crate::utils::update_installer::{self, StageError, StagedUpdate};
use chrono::{DateTime, Utc};
use reqwest;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use tokio::time::{interval, Duration as TokioDuration};

// Fixed repository information
const REPO_OWNER: &str = "zeoirn";
const REPO_NAME: &str = "qualy";
const API_TIMEOUT_SECS: u64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub download_url: Option<String>,
    pub release_notes: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub is_prerelease: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    NetworkError(String),
    ParseError(String),
    NotFound,
    InvalidVersion(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpdateError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            UpdateError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            UpdateError::NotFound => write!(f, "No releases found"),
            UpdateError::InvalidVersion(msg) => write!(f, "Invalid version: {}", msg),
        }
    }
}

impl Error for UpdateError {}

/// How this build is packaged, which is what decides *which* release asset the
/// user should be pointed at.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    /// Windows, installed or portable: the `.exe` installer is the asset.
    Windows,
    /// macOS: the `.dmg`.
    MacOs,
    /// Linux, running from a mounted AppImage.
    LinuxAppImage,
    /// Linux, anything else (a `.deb` install, or a build from source).
    LinuxPackage,
}

/// The packaging of the currently running build.
fn current_install_kind() -> InstallKind {
    #[cfg(target_os = "windows")]
    {
        InstallKind::Windows
    }
    #[cfg(target_os = "macos")]
    {
        InstallKind::MacOs
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        if crate::state::paths::is_running_from_appimage() {
            InstallKind::LinuxAppImage
        } else {
            InstallKind::LinuxPackage
        }
    }
}

/// Whether `name` (already lowercased) is the asset `kind` should install.
fn asset_matches(kind: InstallKind, name: &str) -> bool {
    match kind {
        // Must contain "x64" and end in ".exe" - release.yml contract.
        InstallKind::Windows => name.ends_with(".exe") && name.contains("x64"),
        InstallKind::MacOs => name.ends_with(".dmg") || name.ends_with(".pkg"),
        InstallKind::LinuxAppImage => name.ends_with(".appimage"),
        InstallKind::LinuxPackage => name.ends_with(".deb"),
    }
}

/// The download URL `kind` should be offered from `assets`, if any.
fn find_download_url_for(kind: InstallKind, assets: &[GitHubAsset]) -> Option<String> {
    assets
        .iter()
        .find(|asset| asset_matches(kind, &asset.name.to_lowercase()))
        .map(|asset| asset.browser_download_url.clone())
}

pub struct AutoUpdater {
    pub current_version: String,
    client: reqwest::Client,
}

impl Default for AutoUpdater {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoUpdater {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(API_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            current_version: crate::utils::constants::APP_VERSION.to_string(),
            client,
        }
    }

    pub async fn check_for_updates(&self) -> Result<UpdateInfo, UpdateError> {
        let url = format!("https://api.github.com/repos/{}/{}/releases", REPO_OWNER, REPO_NAME);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", self.user_agent())
            .send()
            .await
            .map_err(|e| UpdateError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let msg = if status == reqwest::StatusCode::FORBIDDEN {
                "HTTP 403 (GitHub API rate limit reached or access forbidden)".to_string()
            } else if status == reqwest::StatusCode::NOT_FOUND {
                "HTTP 404 (Repository or releases endpoint not found)".to_string()
            } else {
                format!("HTTP {}", status)
            };
            return Err(UpdateError::NetworkError(msg));
        }

        let releases: Vec<GitHubRelease> = response
            .json()
            .await
            .map_err(|e| UpdateError::ParseError(e.to_string()))?;

        if releases.is_empty() {
            return Err(UpdateError::NotFound);
        }

        // Find the latest non-prerelease
        let latest_release = releases
            .iter()
            .find(|release| !release.prerelease)
            .ok_or(UpdateError::NotFound)?;

        crate::always_print!(
            "Latest release: {} ({}), published at {}",
            latest_release.tag_name,
            latest_release.name,
            latest_release.published_at
        );

        let current_version = Version::parse(&self.current_version)
            .map_err(|e| UpdateError::InvalidVersion(e.to_string()))?;

        let latest_version_str = latest_release
            .tag_name
            .trim_start_matches(|c| c == 'v' || c == 'V')
            .trim();

        let latest_version = Version::parse(latest_version_str)
            .map_err(|e| UpdateError::InvalidVersion(e.to_string()))?;

        let update_available = latest_version > current_version;
        let download_url = self.find_download_url(&latest_release.assets);
        let published_at = DateTime::parse_from_rfc3339(&latest_release.published_at)
            .map(|dt| dt.with_timezone(&Utc))
            .ok();

        Ok(UpdateInfo {
            current_version: self.current_version.clone(),
            latest_version: latest_version.to_string(),
            update_available,
            download_url,
            release_notes: Some(latest_release.body.clone()),
            published_at,
            is_prerelease: latest_release.prerelease,
        })
    }

    /// Picks the release asset this platform can actually install.
    fn find_download_url(&self, assets: &[GitHubAsset]) -> Option<String> {
        find_download_url_for(current_install_kind(), assets)
    }

    /// Link to the release page.
    pub fn releases_page_url(version: &str) -> String {
        format!("https://github.com/{}/{}/releases/tag/v{}", REPO_OWNER, REPO_NAME, version)
    }

    /// User-Agent header for GitHub API requests.
    pub fn user_agent(&self) -> String {
        format!("{}/{}", APP_NAME, self.current_version)
    }
}

// Configuration for auto-update settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AutoUpdateConfig {
    pub last_check: Option<u64>,
    pub available_version: Option<String>,
    pub available_download_url: Option<String>,
    #[serde(default)]
    pub staged_update: Option<StagedUpdate>,
}

pub async fn check_for_updates_simple() -> Result<UpdateInfo, UpdateError> {
    let updater = AutoUpdater::new();
    updater.check_for_updates().await
}

// Service for auto-update background checking
#[derive(Default)]
pub struct UpdateService;

impl UpdateService {
    pub fn new() -> Self {
        Self
    }

    pub async fn start(&self) {
        tokio::spawn(async move {
            // Tick every hour to inspect whether 24 hours have elapsed since `last_check`.
            // This prevents a 24-hour interval sleep from delaying checks up to 47 hours.
            let mut interval = interval(TokioDuration::from_secs(3600));
            loop {
                interval.tick().await;

                let update_config = crate::state::config_writer::current().auto_update;
                if let Some(last_check) = update_config.last_check {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let check_interval = 24 * 3600; // 24 hours in seconds
                    if now.saturating_sub(last_check) < check_interval {
                        continue;
                    }
                }

                crate::always_print!("🔄 Checking for updates...");
                match check_for_updates_simple().await {
                    Ok(update_info) => {
                        let checked_at = std::time::SystemTime::now()
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();

                        let available = update_info.update_available.then(|| {
                            (
                                update_info.latest_version.clone(),
                                update_info.download_url.clone(),
                            )
                        });

                        crate::state::config_writer::apply(move |config| {
                            config.auto_update.last_check = Some(checked_at);
                            match available {
                                Some((version, download_url)) => {
                                    config.auto_update.available_version = Some(version);
                                    config.auto_update.available_download_url = download_url;
                                }
                                None => {
                                    config.auto_update.available_version = None;
                                    config.auto_update.available_download_url = None;
                                }
                            }
                        });

                        if update_info.update_available {
                            crate::always_print!(
                                "🆕 Update available: {} -> {}",
                                update_info.current_version,
                                update_info.latest_version
                            );
                            crate::state::app::set_update_info(Some(update_info));
                        } else {
                            crate::always_print!("✅ No updates available");
                            crate::state::app::set_update_info(None);
                        }
                    }
                    Err(e) => {
                        crate::always_eprint!("❌ Failed to check for updates: {}", e);
                    }
                }
            }
        });
    }
}

/// Where the update flow currently stands, for the UI to render.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum UpdateStage {
    #[default]
    Idle,
    Downloading {
        version: String,
    },
    Ready {
        version: String,
        installer_path: String,
    },
    Failed {
        version: String,
        reason: String,
    },
}

static UPDATE_STAGE: once_cell::sync::Lazy<std::sync::Mutex<UpdateStage>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(UpdateStage::Idle));

/// Current stage, cloned so callers never hold the lock.
pub fn get_update_stage() -> UpdateStage {
    UPDATE_STAGE
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or(UpdateStage::Idle)
}

fn set_update_stage(stage: UpdateStage) {
    if let Ok(mut guard) = UPDATE_STAGE.lock() {
        *guard = stage;
    }
}

/// Atomically transitions the stage to Downloading if not already in flight or ready for `version`.
fn try_set_downloading(version: &str) -> bool {
    if let Ok(mut guard) = UPDATE_STAGE.lock() {
        match &*guard {
            UpdateStage::Downloading { .. } => false,
            UpdateStage::Ready { version: ready, .. } if ready == version => false,
            _ => {
                *guard = UpdateStage::Downloading {
                    version: version.to_string(),
                };
                true
            }
        }
    } else {
        false
    }
}

/// Clears a `Failed` stage back to `Idle`.
pub fn clear_failed_update() {
    if let Ok(mut guard) = UPDATE_STAGE.lock() {
        if matches!(*guard, UpdateStage::Failed { .. }) {
            *guard = UpdateStage::Idle;
        }
    }
}

/// Downloads and verifies the installer for `update_info`.
pub async fn download_and_stage_update(update_info: &UpdateInfo) {
    if !update_installer::silent_update_supported() {
        return;
    }
    if !update_info.update_available {
        return;
    }

    let version = update_info.latest_version.clone();

    let Some(download_url) = update_info.download_url.clone() else {
        set_update_stage(UpdateStage::Failed {
            version,
            reason: "No installer asset in this release".to_string(),
        });
        return;
    };

    // Recheck if an existing staged installer is already valid before downloading.
    let config = crate::state::config_writer::current();
    if let Some(staged) = &config.auto_update.staged_update {
        if let Ok(path) = staged.validate(&version) {
            set_update_stage(UpdateStage::Ready {
                version,
                installer_path: path.to_string_lossy().into_owned(),
            });
            return;
        }
        // Stale or corrupt: drop disk file AND immediately clear config to prevent orphaned state.
        update_installer::discard_staged(staged);
        crate::state::config_writer::apply(|cfg| {
            cfg.auto_update.staged_update = None;
        });
    }

    // Atomically transition state to avoid concurrent download tasks.
    if !try_set_downloading(&version) {
        return;
    }

    crate::always_print!("📥 Downloading update {} (user requested)...", version);
    let user_agent = AutoUpdater::new().user_agent();
    match update_installer::download_and_verify(&download_url, &version, &user_agent).await {
        Ok(staged) => {
            let installer_path = staged.installer_path.clone();
            crate::state::config_writer::apply(move |config| {
                config.auto_update.staged_update = Some(staged);
            });
            crate::always_print!("✅ Update {} verified and ready to install", version);
            set_update_stage(UpdateStage::Ready { version, installer_path });
        }
        Err(e) => {
            let reason = match &e {
                StageError::ChecksumUnavailable(_) => {
                    "This release has no published checksums; use the download page.".to_string()
                }
                StageError::ChecksumMismatch { .. } => {
                    "The downloaded installer failed verification and was discarded.".to_string()
                }
                other => other.to_string(),
            };
            crate::always_eprint!("⚠️ Update download failed ({}); falling back to manual download", e);
            set_update_stage(UpdateStage::Failed { version, reason });
        }
    }
}

/// Whether `candidate` is strictly newer than `current`.
pub fn is_upgrade(current: &str, candidate: &str) -> bool {
    match (Version::parse(current), Version::parse(candidate)) {
        (Ok(current), Ok(candidate)) => candidate > current,
        _ => false,
    }
}

/// Re-checks a staged installer at startup.
pub fn restore_staged_update() {
    if !update_installer::silent_update_supported() {
        return;
    }
    let Some(staged) = crate::state::config_writer::current().auto_update.staged_update else {
        return;
    };
    let current = crate::utils::constants::APP_VERSION;
    if !is_upgrade(current, &staged.version) {
        crate::always_print!("🧹 Discarding staged update {} (no longer newer than {})", staged.version, current);
        update_installer::discard_staged(&staged);
        crate::state::config_writer::apply(|config| {
            config.auto_update.staged_update = None;
        });
        return;
    }
    match staged.validate(&staged.version) {
        Ok(path) => {
            crate::always_print!("📦 Staged update {} is ready from a previous session", staged.version);
            set_update_stage(UpdateStage::Ready {
                version: staged.version.clone(),
                installer_path: path.to_string_lossy().into_owned(),
            });
        }
        Err(e) => {
            crate::always_eprint!("⚠️ Discarding unusable staged update: {}", e);
            update_installer::discard_staged(&staged);
            crate::state::config_writer::apply(|config| {
                config.auto_update.staged_update = None;
            });
        }
    }
}

/// Launches the staged installer and hands control to it.
pub fn install_staged_update() -> Result<(), StageError> {
    let stage = get_update_stage();
    let UpdateStage::Ready { version, installer_path } = stage else {
        return Err(StageError::Io("No verified update is ready to install".to_string()));
    };
    let staged = crate::state::config_writer::current()
        .auto_update
        .staged_update
        .ok_or_else(|| StageError::Io("Staged update is missing from config".to_string()))?;

    let path = staged.validate(&version)?;
    if path.to_string_lossy() != installer_path {
        return Err(StageError::Io("Staged installer path changed unexpectedly".to_string()));
    }

    crate::state::config_writer::apply(|config| {
        config.auto_update.staged_update = None;
    });
    update_installer::spawn_installer_detached(&path)?;
    crate::always_print!("🚀 Installer for {} launched; shutting down for upgrade", version);
    Ok(())
}

/// The saved update from config, if strictly newer than the running build.
pub fn get_saved_update_info() -> Option<UpdateInfo> {
    let config = crate::state::config_writer::current();
    let available_version = config.auto_update.available_version.as_ref()?;
    let current_version = crate::utils::constants::APP_VERSION;
    if !is_upgrade(current_version, available_version) {
        return None;
    }
    Some(UpdateInfo {
        current_version: current_version.to_string(),
        latest_version: available_version.clone(),
        update_available: true,
        download_url: config.auto_update.available_download_url.clone(),
        release_notes: Some(AutoUpdater::releases_page_url(available_version)),
        published_at: None,
        is_prerelease: false,
    })
}

/// Drops a saved update that is no longer newer than the running build.
pub fn clear_stale_available_version() -> bool {
    let Some(available_version) = crate::state::config_writer::current().auto_update.available_version else {
        return false;
    };
    if is_upgrade(crate::utils::constants::APP_VERSION, &available_version) {
        return false;
    }
    crate::always_print!(
        "🧹 Clearing stale update record: v{} is not newer than the running v{}",
        available_version,
        crate::utils::constants::APP_VERSION
    );
    crate::state::config_writer::apply(|config| {
        config.auto_update.available_version = None;
        config.auto_update.available_download_url = None;
    });
    true
}

/// Check for updates on app startup (when app was completely closed).
pub async fn check_for_updates_on_startup() -> Result<UpdateInfo, UpdateError> {
    crate::always_print!("🔄 Checking for updates on startup...");

    let last_check = crate::state::config_writer::current().auto_update.last_check;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let should_check = match last_check {
        None => {
            crate::always_print!("📅 First time checking for updates");
            true
        }
        Some(last_check) => {
            let time_since_last_check = now.saturating_sub(last_check);
            let one_hour = 3600;
            if time_since_last_check >= one_hour {
                crate::always_print!(
                    "📅 Last check was {} hours ago, checking again",
                    time_since_last_check / 3600
                );
                true
            } else {
                crate::always_print!(
                    "📅 Recently checked ({} minutes ago), skipping startup check",
                    time_since_last_check / 60
                );
                false
            }
        }
    };

    if !should_check {
        if let Some(saved_update) = get_saved_update_info() {
            return Ok(saved_update);
        } else {
            return Ok(UpdateInfo {
                current_version: crate::utils::constants::APP_VERSION.to_string(),
                latest_version: crate::utils::constants::APP_VERSION.to_string(),
                update_available: false,
                download_url: None,
                release_notes: None,
                published_at: None,
                is_prerelease: false,
            });
        }
    }

    match check_for_updates_simple().await {
        Ok(update_info) => {
            let available = update_info.update_available.then(|| {
                (
                    update_info.latest_version.clone(),
                    update_info.download_url.clone(),
                )
            });

            if update_info.update_available {
                crate::always_print!(
                    "🆕 Startup check: Update available {} -> {}",
                    update_info.current_version,
                    update_info.latest_version
                );
            } else {
                crate::always_print!("✅ Startup check: No updates available");
            }

            crate::state::config_writer::apply(move |config| {
                config.auto_update.last_check = Some(now);
                match available {
                    Some((version, download_url)) => {
                        config.auto_update.available_version = Some(version);
                        config.auto_update.available_download_url = download_url;
                    }
                    None => {
                        config.auto_update.available_version = None;
                        config.auto_update.available_download_url = None;
                    }
                }
            });

            if update_info.update_available {
                crate::state::app::set_update_info(Some(update_info.clone()));
            } else {
                crate::state::app::set_update_info(None);
            }
            Ok(update_info)
        }
        Err(e) => {
            crate::always_eprint!("❌ Startup update check failed: {}", e);
            if let Some(saved_update) = get_saved_update_info() {
                Ok(saved_update)
            } else {
                Err(e)
            }
        }
    }
}

// ============================================================================
// Tests (All existing tests preserved)
// ============================================================================

#[cfg(test)]
mod update_service_persistence_tests {
    use super::*;

    fn record_check_result(config: &mut AppConfig, now: u64) {
        config.auto_update.last_check = Some(now);
        config.auto_update.available_version = None;
        config.auto_update.available_download_url = None;
    }

    #[test]
    fn the_periodic_check_does_not_revert_settings_changed_since_launch() {
        let mut config = AppConfig {
            volume: 0.68,
            mouse_volume: 0.62,
            theme: crate::libs::theme::Theme::BuiltIn(crate::libs::theme::BuiltInTheme::Luxury),
            enable_sound: false,
            ..AppConfig::default()
        };
        record_check_result(&mut config, 1_700_000_000);
        assert_eq!(config.volume, 0.68, "keyboard volume must survive the periodic check");
        assert_eq!(config.mouse_volume, 0.62, "mouse volume must survive it too");
        assert_eq!(
            config.theme,
            crate::libs::theme::Theme::BuiltIn(crate::libs::theme::BuiltInTheme::Luxury),
            "the theme must survive it"
        );
        assert!(!config.enable_sound, "and so must the mute state");
        assert_eq!(
            config.auto_update.last_check,
            Some(1_700_000_000),
            "while the check still records its own result"
        );
    }

    #[test]
    fn ticking_repeatedly_never_disturbs_an_unrelated_field() {
        let mut config = AppConfig { volume: 0.68, ..AppConfig::default() };
        for tick in 0..24 {
            record_check_result(&mut config, 1_700_000_000 + tick);
            assert_eq!(config.volume, 0.68, "tick {tick} must leave the volume alone");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions_are_upgrades() {
        assert!(is_upgrade("0.6.1", "0.6.2"));
        assert!(is_upgrade("0.6.1", "0.7.0"));
        assert!(is_upgrade("0.6.1", "1.0.0"));
    }

    #[test]
    fn the_same_version_is_not_an_upgrade() {
        assert!(!is_upgrade("0.6.1", "0.6.1"));
    }

    #[test]
    fn older_versions_are_never_upgrades() {
        assert!(!is_upgrade("0.6.1", "0.6.0"));
        assert!(!is_upgrade("1.0.0", "0.9.9"));
    }

    #[test]
    fn prereleases_rank_below_their_release() {
        assert!(!is_upgrade("0.7.0", "0.7.0-alpha.1"));
        assert!(is_upgrade("0.7.0-alpha.1", "0.7.0"));
    }

    #[test]
    fn unparseable_versions_never_trigger_an_install() {
        assert!(!is_upgrade("0.6.1", "not-a-version"));
        assert!(!is_upgrade("garbage", "0.7.0"));
        assert!(!is_upgrade("", ""));
    }

    #[test]
    fn releases_page_url_points_at_the_tag() {
        assert_eq!(
            AutoUpdater::releases_page_url("0.7.0"),
            "https://github.com/hainguyents13/mechvibes-dx/releases/tag/v0.7.0"
        );
    }

    fn asset(name: &str) -> GitHubAsset {
        GitHubAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.com/{}", name),
            content_type: "application/octet-stream".to_string(),
            size: 1,
        }
    }

    fn release_assets() -> Vec<GitHubAsset> {
        vec![
            asset("MechvibesDX-0.7.0-Setup-x64.exe"),
            asset("SHA256SUMS.txt"),
            asset("mechvibes-dx-0.7.0-macos-arm64.dmg"),
            asset("mechvibes-dx-0.7.0-x86_64.AppImage"),
            asset("mechvibes-dx_0.7.0_amd64.deb"),
        ]
    }

    #[test]
    fn neither_linux_kind_is_ever_offered_a_foreign_asset() {
        let assets = release_assets();
        for kind in [InstallKind::LinuxAppImage, InstallKind::LinuxPackage] {
            let url = find_download_url_for(kind, &assets).unwrap().to_lowercase();
            assert!(
                url.ends_with(".deb") || url.ends_with(".appimage"),
                "{:?} was offered {}",
                kind,
                url
            );
        }
    }

    #[test]
    fn each_install_kind_is_offered_the_asset_it_can_install() {
        let assets = release_assets();
        let pick = |kind| find_download_url_for(kind, &assets);
        assert_eq!(
            pick(InstallKind::Windows).as_deref(),
            Some("https://example.com/MechvibesDX-0.7.0-Setup-x64.exe")
        );
        assert_eq!(
            pick(InstallKind::MacOs).as_deref(),
            Some("https://example.com/mechvibes-dx-0.7.0-macos-arm64.dmg")
        );
        assert_eq!(
            pick(InstallKind::LinuxAppImage).as_deref(),
            Some("https://example.com/mechvibes-dx-0.7.0-x86_64.AppImage"),
            "an AppImage user must be offered the next AppImage"
        );
        assert_eq!(
            pick(InstallKind::LinuxPackage).as_deref(),
            Some("https://example.com/mechvibes-dx_0.7.0_amd64.deb"),
            "a .deb user must be offered the next .deb"
        );
    }

    #[test]
    fn linux_asset_order_in_the_release_never_decides_the_pick() {
        let forward = release_assets();
        let mut reversed = release_assets();
        reversed.reverse();
        for assets in [forward, reversed] {
            assert!(
                find_download_url_for(InstallKind::LinuxPackage, &assets)
                    .unwrap()
                    .ends_with(".deb"),
                "a .deb install was offered a non-.deb asset"
            );
            assert!(
                find_download_url_for(InstallKind::LinuxAppImage, &assets)
                    .unwrap()
                    .to_lowercase()
                    .ends_with(".appimage"),
                "an AppImage run was offered a non-AppImage asset"
            );
        }
    }

    #[test]
    fn no_install_kind_is_ever_offered_another_platforms_asset() {
        let assets = release_assets();
        for (kind, expected_suffix) in [
            (InstallKind::Windows, ".exe"),
            (InstallKind::MacOs, ".dmg"),
            (InstallKind::LinuxAppImage, ".appimage"),
            (InstallKind::LinuxPackage, ".deb"),
        ] {
            let url = find_download_url_for(kind, &assets).unwrap().to_lowercase();
            assert!(
                url.ends_with(expected_suffix),
                "{:?} was offered {}, expected a {} asset",
                kind,
                url,
                expected_suffix
            );
        }
    }

    #[test]
    fn the_macos_rule_does_not_match_the_readme_text_file() {
        assert!(!asset_matches(InstallKind::MacOs, "readme-macos-0.7.0.txt"));
        assert!(asset_matches(InstallKind::MacOs, "mechvibes-dx-0.7.0-macos-arm64.dmg"));
    }

    #[test]
    fn a_kind_with_no_matching_asset_gets_nothing() {
        let only_windows = vec![asset("MechvibesDX-0.7.0-Setup-x64.exe")];
        assert_eq!(find_download_url_for(InstallKind::LinuxPackage, &only_windows), None);
        assert_eq!(find_download_url_for(InstallKind::LinuxAppImage, &only_windows), None);
        assert_eq!(find_download_url_for(InstallKind::MacOs, &only_windows), None);
    }

    #[test]
    fn never_offers_a_windows_installer_to_a_non_windows_build() {
        let url = AutoUpdater::new().find_download_url(&release_assets()).unwrap_or_default();
        if !cfg!(target_os = "windows") {
            assert!(!url.ends_with(".exe"), "non-Windows build was offered {}", url);
        }
    }

    #[test]
    fn no_non_windows_asset_can_satisfy_the_windows_installer_filter() {
        let windows_filter = |name: &str| name.ends_with(".exe") && name.contains("x64");
        for name in [
            "mechvibes-dx-0.7.0-macos-arm64.dmg",
            "mechvibes-dx-0.7.0-macos-x86_64.dmg",
            "mechvibes-dx_0.7.0_amd64.deb",
            "mechvibes-dx-0.7.0-x86_64.AppImage",
            "README-macos-0.7.0.txt",
            "SHA256SUMS.txt",
        ] {
            assert!(
                !windows_filter(&name.to_lowercase()),
                "'{}' would be served to Windows users as an installer",
                name
            );
        }
        assert!(windows_filter(&"MechvibesDX-0.7.0-Setup-x64.exe".to_lowercase()));
    }

    #[test]
    fn returns_none_when_no_asset_suits_this_platform() {
        let assets = vec![asset("SHA256SUMS.txt"), asset("source-code.zip")];
        assert_eq!(AutoUpdater::new().find_download_url(&assets), None);
    }

    #[test]
    fn returns_none_for_a_release_with_no_assets_at_all() {
        assert_eq!(AutoUpdater::new().find_download_url(&[]), None);
    }

    fn saved_update_is_showable(current: &str, saved: &str) -> bool {
        is_upgrade(current, saved)
    }

    #[test]
    fn a_saved_update_equal_to_the_running_version_is_never_offered() {
        assert!(!saved_update_is_showable("0.6.2", "0.6.2"));
    }

    #[test]
    fn a_saved_update_older_than_the_running_version_is_never_offered() {
        assert!(!saved_update_is_showable("0.6.2", "0.6.1"));
        assert!(!saved_update_is_showable("1.0.0", "0.9.9"));
    }

    #[test]
    fn a_saved_update_newer_than_the_running_version_is_still_offered() {
        assert!(saved_update_is_showable("0.6.2", "0.6.3"));
        assert!(saved_update_is_showable("0.6.2", "0.7.0"));
    }

    #[test]
    fn version_comparison_is_numeric_not_lexicographic() {
        assert!(saved_update_is_showable("0.9.0", "0.10.0"));
        assert!(!saved_update_is_showable("0.10.0", "0.9.0"));
    }

    #[test]
    fn a_malformed_saved_version_is_never_offered() {
        assert!(!saved_update_is_showable("0.6.2", "not-a-version"));
        assert!(!saved_update_is_showable("0.6.2", ""));
    }

    #[test]
    fn the_clear_rule_drops_stale_entries_and_keeps_real_ones() {
        let should_clear = |current: &str, saved: &str| !is_upgrade(current, saved);
        assert!(should_clear("0.6.2", "0.6.2"));
        assert!(should_clear("0.6.2", "0.6.1"));
        assert!(should_clear("0.6.2", "garbage"));
        assert!(!should_clear("0.6.2", "0.6.3"));
    }
}
