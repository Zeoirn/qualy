use std::path::PathBuf;
use crate::utils::constants::APP_NAME;

#[cfg(target_os = "windows")]
use winreg::enums::*;
#[cfg(target_os = "windows")]
use winreg::RegKey;

/// Get the current executable path
fn get_exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("Failed to get executable path: {}", e))
}

// ============================================================================
// Windows Implementation
// ============================================================================

#[cfg(target_os = "windows")]
const RUN_REG_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";

/// Enable auto startup on Windows
#[cfg(target_os = "windows")]
pub fn enable_auto_startup() -> Result<(), String> {
    let exe_path = get_exe_path()?;
    let exe_path_str = exe_path.to_str().ok_or("Failed to convert executable path to string")?;

    // Check if we should start minimized from config authority.
    let config = crate::state::config_writer::current();

    // Always quote the executable path to avoid Unquoted Search Path vulnerabilities
    // when installed in paths with spaces (e.g. C:\Program Files\...).
    let command = if config.start_minimized {
        format!("\"{}\" --minimized", exe_path_str)
    } else {
        format!("\"{}\"", exe_path_str)
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(RUN_REG_KEY, KEY_WRITE)
        .map_err(|e| format!("Failed to open registry key: {}", e))?;

    run_key
        .set_value(APP_NAME, &command)
        .map_err(|e| format!("Failed to set registry value: {}", e))?;

    crate::always_print!("Auto startup enabled: {}", command);
    Ok(())
}

/// Disable auto startup on Windows
#[cfg(target_os = "windows")]
pub fn disable_auto_startup() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(RUN_REG_KEY, KEY_WRITE)
        .map_err(|e| format!("Failed to open registry key: {}", e))?;

    match run_key.delete_value(APP_NAME) {
        Ok(_) => {
            crate::always_print!("Auto startup disabled");
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            crate::always_print!("Auto startup was not enabled");
            Ok(())
        }
        Err(e) => Err(format!("Failed to delete registry value: {}", e)),
    }
}

/// Pull the executable path back out of a registry Run command line.
fn exe_path_from_run_command(command: &str) -> &str {
    let command = command.trim();

    // Quoted form: extract path between first pair of quotes.
    if let Some(rest) = command.strip_prefix('"') {
        return match rest.find('"') {
            Some(end) => &rest[..end],
            None => rest,
        };
    }

    // Unquoted form fallback: if an unquoted command has arguments like `.exe --arg`,
    // split safely after `.exe` if present; otherwise keep full path for paths with spaces.
    if let Some(pos) = command.to_ascii_lowercase().find(".exe ") {
        return &command[..pos + 4];
    }

    command
}

/// Normalizes path slashes and case for Windows path comparison.
#[cfg(target_os = "windows")]
fn normalize_win_path(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

/// Check if auto startup is currently enabled on Windows
#[cfg(target_os = "windows")]
pub fn is_auto_startup_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = match hkcu.open_subkey(RUN_REG_KEY) {
        Ok(key) => key,
        Err(_) => return false,
    };

    match run_key.get_value::<String, _>(APP_NAME) {
        Ok(value) => {
            let current_exe = get_exe_path().unwrap_or_default();
            let current_exe_str = match current_exe.to_str() {
                Some(path) if !path.is_empty() => path,
                _ => return false,
            };

            let reg_exe = exe_path_from_run_command(&value);
            normalize_win_path(reg_exe) == normalize_win_path(current_exe_str)
        }
        Err(_) => false,
    }
}

// ============================================================================
// macOS Implementation (SMAppService)
// ============================================================================

#[cfg(target_os = "macos")]
mod macos {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::msg_send;
    use objc2_foundation::NSError;

    #[link(name = "ServiceManagement", kind = "framework")]
    unsafe extern "C" {}

    const STATUS_ENABLED: isize = 1;
    const STATUS_REQUIRES_APPROVAL: isize = 2;

    fn service_class() -> Result<&'static AnyClass, String> {
        AnyClass::get(c"SMAppService").ok_or_else(|| {
            "Start at login needs macOS 13 or later".to_string()
        })
    }

    fn main_app_service() -> Result<Retained<AnyObject>, String> {
        let class = service_class()?;
        let service: Option<Retained<AnyObject>> = unsafe { msg_send![class, mainAppService] };
        service.ok_or_else(|| "SMAppService.mainApp is unavailable".to_string())
    }

    fn status(service: &AnyObject) -> isize {
        unsafe { msg_send![service, status] }
    }

    pub fn is_enabled() -> bool {
        match main_app_service() {
            Ok(service) => status(&service) == STATUS_ENABLED,
            Err(_) => false,
        }
    }

    pub fn enable() -> Result<(), String> {
        let service = main_app_service()?;
        if status(&service) != STATUS_ENABLED {
            let result: Result<(), Retained<NSError>> = unsafe {
                msg_send![&*service, registerAndReturnError: _]
            };
            result.map_err(|e| format!("Failed to register login item: {}", e.localizedDescription()))?;
        }

        if status(&service) == STATUS_REQUIRES_APPROVAL {
            crate::always_print!("Login item needs approval in System Settings > General > Login Items");
            if let Ok(class) = service_class() {
                unsafe {
                    let _: () = msg_send![class, openSystemSettingsLoginItems];
                }
            }
            return Err("Allow MechvibesDX in System Settings > General > Login Items".to_string());
        }

        crate::always_print!("Auto startup enabled (macOS login item)");
        Ok(())
    }

    pub fn disable() -> Result<(), String> {
        let service = main_app_service()?;
        if status(&service) == 0 {
            crate::always_print!("ℹ️ Auto startup was not enabled");
            return Ok(());
        }
        let result: Result<(), Retained<NSError>> = unsafe {
            msg_send![&*service, unregisterAndReturnError: _]
        };
        result.map_err(|e| format!("Failed to remove login item: {}", e.localizedDescription()))?;
        crate::always_print!("✅ Auto startup disabled");
        Ok(())
    }
}

// ============================================================================
// Linux Implementation (XDG Autostart Desktop Entry)
// ============================================================================

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::path::PathBuf;
    use super::get_exe_path;
    use crate::utils::constants::APP_NAME;

    fn get_autostart_dir() -> Result<PathBuf, String> {
        let base = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => {
                let home = std::env::var_os("HOME").ok_or("Failed to determine HOME directory")?;
                PathBuf::from(home).join(".config")
            }
        };
        Ok(base.join("autostart"))
    }

    fn get_desktop_file_path() -> Result<PathBuf, String> {
        let autostart_dir = get_autostart_dir()?;
        let filename = format!("{}.desktop", APP_NAME.to_lowercase().replace(' ', "-"));
        Ok(autostart_dir.join(filename))
    }

    pub fn is_enabled() -> bool {
        get_desktop_file_path().map(|p| p.exists()).unwrap_or(false)
    }

    pub fn enable() -> Result<(), String> {
        let autostart_dir = get_autostart_dir()?;
        fs::create_dir_all(&autostart_dir)
            .map_err(|e| format!("Failed to create autostart directory: {}", e))?;

        let exe_path = get_exe_path()?;
        let exe_path_str = exe_path.to_str().ok_or("Failed to convert executable path to string")?;
        let config = crate::state::config_writer::current();

        let exec_cmd = if config.start_minimized {
            format!("\"{}\" --minimized", exe_path_str)
        } else {
            format!("\"{}\"", exe_path_str)
        };

        let desktop_entry = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Version=1.0\n\
             Name={name}\n\
             Comment=Launch {name} on startup\n\
             Exec={exec}\n\
             Terminal=false\n\
             Categories=Utility;\n\
             X-GNOME-Autostart-enabled=true\n",
            name = APP_NAME,
            exec = exec_cmd,
        );

        let desktop_file = get_desktop_file_path()?;
        fs::write(&desktop_file, desktop_entry)
            .map_err(|e| format!("Failed to write desktop entry: {}", e))?;

        crate::always_print!("Auto startup enabled (Linux XDG desktop entry)");
        Ok(())
    }

    pub fn disable() -> Result<(), String> {
        let desktop_file = get_desktop_file_path()?;
        if !desktop_file.exists() {
            crate::always_print!("Auto startup was not enabled");
            return Ok(());
        }

        fs::remove_file(&desktop_file)
            .map_err(|e| format!("Failed to remove autostart desktop entry: {}", e))?;

        crate::always_print!("Auto startup disabled");
        Ok(())
    }
}

// ============================================================================
// Public Cross-Platform Facade
// ============================================================================

/// Set auto startup state (enable or disable)
pub fn set_auto_startup(enable: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if enable { enable_auto_startup() } else { disable_auto_startup() }
    }

    #[cfg(target_os = "macos")]
    {
        if enable { macos::enable() } else { macos::disable() }
    }

    #[cfg(target_os = "linux")]
    {
        if enable { linux::enable() } else { linux::disable() }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err("Auto startup is only supported on Windows, macOS, and Linux".to_string())
    }
}

/// Get current auto startup state
pub fn get_auto_startup_state() -> bool {
    #[cfg(target_os = "windows")]
    {
        is_auto_startup_enabled()
    }

    #[cfg(target_os = "macos")]
    {
        macos::is_enabled()
    }

    #[cfg(target_os = "linux")]
    {
        linux::is_enabled()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::exe_path_from_run_command;

    #[cfg(target_os = "macos")]
    #[test]
    fn a_bare_binary_is_not_a_registered_login_item() {
        assert!(!super::get_auto_startup_state());
    }

    #[test]
    fn a_bare_path_is_returned_unchanged() {
        let path = r"C:\Program Files\MechvibesDX\mechvibes-dx.exe";
        assert_eq!(exe_path_from_run_command(path), path);
    }

    #[test]
    fn a_quoted_path_with_the_minimized_flag_yields_just_the_path() {
        let path = r"C:\Program Files\MechvibesDX\mechvibes-dx.exe";
        let command = format!("\"{}\" --minimized", path);
        assert_eq!(exe_path_from_run_command(&command), path);
    }

    #[test]
    fn a_quoted_path_without_arguments_yields_just_the_path() {
        let path = r"C:\Apps\mechvibes-dx.exe";
        assert_eq!(exe_path_from_run_command(&format!("\"{}\"", path)), path);
    }

    #[test]
    fn spaces_in_an_unquoted_path_do_not_truncate_it() {
        let path = r"C:\Program Files\MechvibesDX\mechvibes-dx.exe";
        assert_eq!(exe_path_from_run_command(path), path);
    }

    #[test]
    fn unquoted_path_with_arguments_extracts_executable() {
        let command = r"C:\Tools\mechvibes.exe --minimized";
        assert_eq!(exe_path_from_run_command(command), r"C:\Tools\mechvibes.exe");
    }

    #[test]
    fn both_written_forms_match_the_exe_case_insensitively() {
        let exe = r"C:\Program Files\MechvibesDX\mechvibes-dx.exe";
        let bare = r"c:\program files\mechvibesdx\MECHVIBES-DX.EXE";
        let minimized = format!("\"{}\" --minimized", bare);

        assert!(exe_path_from_run_command(bare).eq_ignore_ascii_case(exe));
        assert!(exe_path_from_run_command(&minimized).eq_ignore_ascii_case(exe));
    }

    #[test]
    fn forward_and_backward_slashes_normalize_consistently() {
        #[cfg(target_os = "windows")]
        {
            let win_path = r"C:\Program Files\MechvibesDX\app.exe";
            let unix_style = "C:/Program Files/MechvibesDX/app.exe";
            assert_eq!(
                super::normalize_win_path(win_path),
                super::normalize_win_path(unix_style)
            );
        }
    }

    #[test]
    fn a_different_executable_does_not_match() {
        let exe = r"C:\Program Files\MechvibesDX\mechvibes-dx.exe";
        let other = "\"C:\\Other\\thing.exe\" --minimized";
        assert!(!exe_path_from_run_command(other).eq_ignore_ascii_case(exe));
    }
}
