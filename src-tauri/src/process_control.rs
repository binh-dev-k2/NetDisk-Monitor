//! Termination targets an open process object, not a PID checked before opening it.

fn validate_identity(pid: u32, created: u64, identity: &str) -> Result<(), String> {
    if identity != format!("{pid}:{created}") {
        return Err("PID đã được tái sử dụng; hãy cập nhật danh sách".into());
    }
    Ok(())
}

#[cfg(windows)]
mod native_windows {
    use super::validate_identity;
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, FILETIME, HANDLE},
        System::Threading::{
            GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        },
    };

    pub(super) struct OwnedHandle(pub HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    pub(super) fn open_verified(
        pid: u32,
        identity: &str,
        access: u32,
    ) -> Result<OwnedHandle, String> {
        let handle = OwnedHandle(unsafe { OpenProcess(access, 0, pid) });
        if handle.0.is_null() {
            return Err(format!("Không mở được process: {}", unsafe {
                GetLastError()
            }));
        }
        let (mut created, mut ended, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        if unsafe {
            GetProcessTimes(
                handle.0,
                &raw mut created,
                &raw mut ended,
                &raw mut kernel,
                &raw mut user,
            )
        } == 0
        {
            return Err(format!("Không kiểm tra được identity: {}", unsafe {
                GetLastError()
            }));
        }
        let token = u64::from(created.dwHighDateTime) << 32 | u64::from(created.dwLowDateTime);
        validate_identity(pid, token, identity)?;
        Ok(handle)
    }

    pub(super) fn terminate(pid: u32, identity: &str) -> Result<(), String> {
        let handle = open_verified(
            pid,
            identity,
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
        )?;
        if unsafe { TerminateProcess(handle.0, 1) } == 0 {
            return Err(format!("Không đóng được process: {}", unsafe {
                GetLastError()
            }));
        }
        Ok(())
    }

    pub(super) fn executable_path(pid: u32, identity: &str) -> Result<PathBuf, String> {
        let handle = open_verified(pid, identity, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let mut buf = vec![0_u16; 4096];
        let mut len = u32::try_from(buf.len()).unwrap_or(4096);
        if unsafe { QueryFullProcessImageNameW(handle.0, 0, buf.as_mut_ptr(), &raw mut len) } == 0
            || len == 0
        {
            return Err("Không đọc được đường dẫn tệp thực thi".into());
        }
        let slice = &buf[..len as usize];
        Ok(PathBuf::from(OsString::from_wide(slice)))
    }
}

#[cfg(windows)]
pub(crate) fn terminate(pid: u32, identity: &str) -> Result<(), String> {
    native_windows::terminate(pid, identity)
}

#[cfg(windows)]
pub(crate) fn executable_path(pid: u32, identity: &str) -> Result<std::path::PathBuf, String> {
    native_windows::executable_path(pid, identity)
}

#[cfg(not(windows))]
pub(crate) fn terminate(pid: u32, identity: &str) -> Result<(), String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
        ProcessRefreshKind::nothing(),
    );
    let process = system
        .process(Pid::from_u32(pid))
        .ok_or("Tiến trình đã kết thúc")?;
    validate_identity(pid, process.start_time(), identity)?;
    if process.kill() {
        Ok(())
    } else {
        Err("Không có quyền đóng tiến trình".into())
    }
}

#[cfg(not(windows))]
pub(crate) fn executable_path(pid: u32, identity: &str) -> Result<std::path::PathBuf, String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let process = system
        .process(Pid::from_u32(pid))
        .ok_or("Tiến trình đã kết thúc")?;
    validate_identity(pid, process.start_time(), identity)?;
    process
        .exe()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "Không đọc được đường dẫn tệp thực thi".to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "fixture child used by the live termination test"]
    fn fixture_wait() {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }

    #[cfg(windows)]
    #[test]
    fn live_termination_uses_the_checked_native_process_object() {
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "process_control::tests::fixture_wait",
            ])
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        let token = crate::process_network::creation_token(pid).unwrap();
        let stale = super::terminate(pid, &format!("{pid}:{}", token - 1));
        assert!(stale.is_err());
        assert!(child.try_wait().unwrap().is_none());
        let result = super::terminate(pid, &format!("{pid}:{token}"));
        if result.is_err() {
            let _ = child.kill();
        }
        result.unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                panic!("terminated process did not exit");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.wait().unwrap();
    }

    #[test]
    fn exact_creation_identity_rejects_pid_reuse_and_malformed_input() {
        assert!(super::validate_identity(42, 100, "42:100").is_ok());
        for stale in ["42:99", "43:100", "42", "42:100:0", "042:100"] {
            assert!(super::validate_identity(42, 100, stale).is_err());
        }
    }

    #[test]
    fn live_executable_path_verifies_identity_and_returns_binary() {
        let pid = std::process::id();
        #[cfg(windows)]
        let token = crate::process_network::creation_token(pid).unwrap();
        #[cfg(not(windows))]
        let token = 0_u64;
        if token > 0 {
            let valid = format!("{pid}:{token}");
            let path = super::executable_path(pid, &valid).unwrap();
            assert!(path.is_file());
            assert!(super::executable_path(pid, &format!("{pid}:{}", token - 1)).is_err());
        }
    }
}
