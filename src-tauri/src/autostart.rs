#[cfg(target_os = "macos")]
fn macos_launch_agent_path() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "Không tìm thấy thư mục HOME".to_owned())?;
    Ok(std::path::PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join("com.netdisk.monitor.plist"))
}

#[cfg(target_os = "linux")]
fn linux_autostart_path() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "Không tìm thấy thư mục HOME".to_owned())?;
    Ok(std::path::PathBuf::from(home)
        .join(".config")
        .join("autostart")
        .join("netdisk-monitor.desktop"))
}

pub fn get_autostart() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use crate::taskbar::wide;
        use std::ptr::null_mut;
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_SZ,
        };

        let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let value_name = wide("NetDisk Monitor");
        let mut hkey = null_mut();

        unsafe {
            let opened = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_READ,
                &raw mut hkey,
            );
            if opened == 2 {
                return Ok(false);
            }
            if opened != 0 {
                return Err(format!("Không thể đọc Registry Windows: {opened}"));
            }
            {
                let mut type_ = 0;
                let mut size = 0;
                let status = RegQueryValueExW(
                    hkey,
                    value_name.as_ptr(),
                    null_mut(),
                    &raw mut type_,
                    null_mut(),
                    &raw mut size,
                );
                RegCloseKey(hkey);
                if status == 2 {
                    return Ok(false);
                }
                if status != 0 {
                    return Err(format!("Không thể đọc cấu hình khởi động: {status}"));
                }
                Ok(type_ == REG_SZ)
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let path = macos_launch_agent_path()?;
        Ok(path.exists())
    }
    #[cfg(target_os = "linux")]
    {
        let path = linux_autostart_path()?;
        Ok(path.exists())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    Ok(false)
}

#[cfg(target_os = "windows")]
fn windows_executable_command() -> Result<Vec<u16>, String> {
    use std::os::windows::ffi::OsStrExt;
    let path = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut text = vec![u16::from(b'"')];
    text.extend(path.as_os_str().encode_wide());
    text.extend([u16::from(b'"'), 0]);
    Ok(text)
}

pub fn set_autostart(enabled: bool) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use crate::taskbar::wide;
        use std::ptr::null_mut;
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY_CURRENT_USER,
            KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
        };

        let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let value_name = wide("NetDisk Monitor");
        let mut hkey = null_mut();

        unsafe {
            let exe_wide = if enabled {
                Some(windows_executable_command()?)
            } else {
                None
            };
            if RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                null_mut(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                std::ptr::null(),
                &raw mut hkey,
                null_mut(),
            ) != 0
            {
                return Err("Không thể truy cập Registry Windows".to_owned());
            }

            if let Some(exe_wide) = exe_wide {
                let Ok(byte_len) = u32::try_from(exe_wide.len() * 2) else {
                    RegCloseKey(hkey);
                    return Err("Đường dẫn chương trình quá dài".to_owned());
                };

                let res = RegSetValueExW(
                    hkey,
                    value_name.as_ptr(),
                    0,
                    REG_SZ,
                    exe_wide.as_ptr().cast(),
                    byte_len,
                );
                RegCloseKey(hkey);
                registry_set_result(res, true)
            } else {
                let res = RegDeleteValueW(hkey, value_name.as_ptr());
                RegCloseKey(hkey);
                registry_set_result(res, false)
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let path = macos_launch_agent_path()?;
        if enabled {
            let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
            let plist_content = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n    <key>Label</key>\n    <string>com.netdisk.monitor</string>\n    <key>ProgramArguments</key>\n    <array>\n        <string>{}</string>\n    </array>\n    <key>RunAtLoad</key>\n    <true/>\n</dict>\n</plist>",
                xml_text(&exe_path.to_string_lossy())
            );
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, plist_content).map_err(|e| e.to_string())?;
            Ok(true)
        } else {
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            }
            Ok(false)
        }
    }
    #[cfg(target_os = "linux")]
    {
        let path = linux_autostart_path()?;
        if enabled {
            let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
            let desktop_content = format!(
                "[Desktop Entry]\nType=Application\nName=NetDisk Monitor\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
                exe_path.to_string_lossy()
            );
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, desktop_content).map_err(|e| e.to_string())?;
            Ok(true)
        } else {
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            }
            Ok(false)
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = enabled;
        Ok(false)
    }
}

#[cfg(any(windows, test))]
fn registry_set_result(status: u32, enabled: bool) -> Result<bool, String> {
    if status == 0 || (!enabled && status == 2) {
        Ok(enabled)
    } else {
        Err(format!(
            "Không thể cập nhật khởi động cùng hệ thống: {status}"
        ))
    }
}

#[cfg(any(target_os = "macos", test))]
fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    #[test]
    fn registry_failures_are_not_reported_as_disabled() {
        assert_eq!(super::registry_set_result(0, true), Ok(true));
        assert_eq!(super::registry_set_result(2, false), Ok(false));
        assert!(super::registry_set_result(5, false).is_err());
        assert!(super::registry_set_result(5, true).is_err());
    }
    #[test]
    fn launch_agent_paths_are_xml_escaped() {
        assert_eq!(super::xml_text("/A&B/<app>"), "/A&amp;B/&lt;app&gt;");
    }
}
