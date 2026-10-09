//! Termination targets an open process object, not a PID checked before opening it.

fn validate_identity(pid: u32, created: u64, identity: &str) -> Result<(), String> {
    if identity != format!("{pid}:{created}") {
        return Err("PID đã được tái sử dụng; hãy cập nhật danh sách".into());
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn terminate(pid: u32, identity: &str) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, FILETIME, HANDLE},
        System::Threading::{
            GetProcessTimes, OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_TERMINATE,
        },
    };
    struct OwnedHandle(HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let handle = OwnedHandle(unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            0,
            pid,
        )
    });
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
    if unsafe { TerminateProcess(handle.0, 1) } == 0 {
        return Err(format!("Không đóng được process: {}", unsafe {
            GetLastError()
        }));
    }
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn terminate(pid: u32, identity: &str) -> Result<(), String> {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
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
}
