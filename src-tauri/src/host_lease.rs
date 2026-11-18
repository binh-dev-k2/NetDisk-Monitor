//! Fail-closed ownership independent of the focus/activation transport.
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

#[must_use]
pub struct HostLease {
    _file: File,
}

impl HostLease {
    /// Holds the per-data-directory lease until the host runtime is dropped.
    ///
    /// # Errors
    /// Returns filesystem errors or `WouldBlock` if another host owns this directory.
    pub fn acquire(directory: &Path) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("host.lock"))?;
        file.try_lock().map_err(io::Error::from)?;
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Barrier};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "netdisk-host-lease-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn exclusive_owner_releases_without_deleting_the_lock_file() {
        let directory = directory();
        let first = HostLease::acquire(&directory).unwrap();
        assert_eq!(
            HostLease::acquire(&directory).err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(first);
        let second = HostLease::acquire(&directory).unwrap();
        assert_eq!(fs::metadata(directory.join("host.lock")).unwrap().len(), 0);
        drop(second);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn concurrent_startup_has_exactly_one_owner() {
        let directory = directory();
        let barrier = Arc::new(Barrier::new(17));
        let workers: Vec<_> = (0..16)
            .map(|_| {
                let directory = directory.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let lease = HostLease::acquire(&directory);
                    barrier.wait();
                    let won = lease.is_ok();
                    drop(lease);
                    won
                })
            })
            .collect();
        barrier.wait();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .filter(|won| *won)
                .count(),
            1
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
