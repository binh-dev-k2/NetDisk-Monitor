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
        use std::ptr::null_mut;
        use crate::taskbar::wide;
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_SZ,
        };

        let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let value_name = wide("NetDisk Monitor");
        let mut hkey = null_mut();

        unsafe {
            if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
                let mut type_ = 0;
                let mut size = 0;
                let status = RegQueryValueExW(
                    hkey,
                    value_name.as_ptr(),
                    null_mut(),
                    &mut type_,
                    null_mut(),
                    &mut size,
                );
                RegCloseKey(hkey);
                return Ok(status == 0 && type_ == REG_SZ);
            }
        }
        Ok(false)
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

pub fn set_autostart(enabled: bool) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use std::ptr::null_mut;
        use crate::taskbar::wide;
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER,
            KEY_SET_VALUE, REG_SZ,
        };

        let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let value_name = wide("NetDisk Monitor");
        let mut hkey = null_mut();

        unsafe {
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut hkey,
            ) != 0
            {
                return Err("Không thể truy cập Registry Windows".to_owned());
            }

            if enabled {
                let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
                let exe_str = format!("\"{}\"", exe_path.to_string_lossy());
                let exe_wide = wide(&exe_str);
                let byte_len = (exe_wide.len() * 2) as u32;

                let res = RegSetValueExW(
                    hkey,
                    value_name.as_ptr(),
                    0,
                    REG_SZ,
                    exe_wide.as_ptr().cast(),
                    byte_len,
                );
                RegCloseKey(hkey);
                if res == 0 {
                    return Ok(true);
                }
                return Err("Không thể ghi Registry khởi động cùng hệ thống".to_owned());
            } else {
                let res = RegDeleteValueW(hkey, value_name.as_ptr());
                RegCloseKey(hkey);
                let _ = res;
                return Ok(false);
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
                exe_path.to_string_lossy()
            );
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&path, plist_content).map_err(|e| e.to_string())?;
            Ok(true)
        } else {
            if path.exists() {
                let _ = std::fs::remove_file(&path);
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
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&path, desktop_content).map_err(|e| e.to_string())?;
            Ok(true)
        } else {
            if path.exists() {
                let _ = std::fs::remove_file(&path);
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
