use crate::metrics::{CounterAccumulator, CounterSample};
use crate::process_network::NetworkCollector;
#[path = "process_tracker.rs"]
mod process_tracker;
pub use process_tracker::ProcessMetric;
use process_tracker::{ProcessObservation, ProcessTracker};
#[cfg(windows)]
use std::path::Path;
use std::{
    collections::HashMap,
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};
use sysinfo::{
    DiskRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind, ProcessesToUpdate,
    System,
};

pub struct Collection {
    /// Synthetic cumulative observed bytes, never raw sums of device lifetime counters.
    pub counters: CounterSample,
    pub interfaces: Vec<String>,
    pub total_memory_bytes: u64,
    pub used_memory_bytes: u64,
    pub disk_available: bool,
}

pub struct SystemCollector {
    system: System,
    networks: Networks,
    disks: Disks,
    network_counters: CounterAccumulator<String>,
    disk_counters: CounterAccumulator<OsString>,
    disk_identities: HashMap<PathBuf, OsString>,
    last_disk_list_refresh: Instant,
    #[cfg(target_os = "macos")]
    macos_disk_counters: CounterAccumulator<u64>,
}

impl SystemCollector {
    pub fn new() -> Self {
        // macOS needs the underlying IOKit driver identity, which sysinfo does not expose.
        let disks = if cfg!(target_os = "macos") {
            Disks::new()
        } else {
            Disks::new_with_refreshed_list_specifics(DiskRefreshKind::nothing().with_io_usage())
        };
        let disk_identities = disk_identities(&disks);
        Self {
            system: System::new(),
            networks: Networks::new_with_refreshed_list(),
            disks,
            network_counters: CounterAccumulator::default(),
            disk_counters: CounterAccumulator::default(),
            disk_identities,
            last_disk_list_refresh: Instant::now(),
            #[cfg(target_os = "macos")]
            macos_disk_counters: CounterAccumulator::default(),
        }
    }

    pub fn collect(&mut self) -> Collection {
        self.system
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        // In sysinfo 0.33 both flag values enumerate adapters; true also drops missing ones.
        self.networks.refresh(true);
        let (network_down, network_up) =
            self.network_counters
                .update(self.networks.iter().map(|(name, data)| {
                    (
                        name.clone(),
                        data.total_received(),
                        data.total_transmitted(),
                    )
                }));

        let refreshes = DiskRefreshKind::nothing().with_io_usage();
        if !cfg!(target_os = "macos")
            && self.last_disk_list_refresh.elapsed() >= Duration::from_secs(30)
        {
            self.disks.refresh_specifics(true, refreshes);
            self.disk_identities = disk_identities(&self.disks);
            self.last_disk_list_refresh = Instant::now();
        } else {
            for disk in self.disks.list_mut() {
                disk.refresh_specifics(refreshes);
            }
        }
        #[cfg(not(target_os = "macos"))]
        let (disk_read, disk_write) =
            self.disk_counters
                .update(self.disks.iter().filter_map(|disk| {
                    let identity = self.disk_identities.get(disk.mount_point())?.clone();
                    let usage = disk.usage();
                    Some((identity, usage.total_read_bytes, usage.total_written_bytes))
                }));
        #[cfg(not(target_os = "macos"))]
        let disk_available = !self.disk_identities.is_empty();
        #[cfg(target_os = "macos")]
        let ((disk_read, disk_write), disk_available) = match crate::macos_disk::sample() {
            Ok(drivers) => {
                let available = !drivers.is_empty();
                (self.macos_disk_counters.update(drivers), available)
            }
            Err(_) => (self.macos_disk_counters.update([]), false),
        };
        let mut interfaces: Vec<_> = self.networks.keys().cloned().collect();
        interfaces.sort_unstable();
        Collection {
            counters: CounterSample::new(network_down, network_up, disk_read, disk_write),
            interfaces,
            total_memory_bytes: self.system.total_memory(),
            used_memory_bytes: self.system.used_memory(),
            disk_available,
        }
    }
}

fn disk_identities(disks: &Disks) -> HashMap<PathBuf, OsString> {
    disks
        .iter()
        .filter_map(|disk| {
            disk_identity(disk).map(|identity| (disk.mount_point().to_owned(), identity))
        })
        .collect()
}

#[cfg(windows)]
fn disk_identity(disk: &sysinfo::Disk) -> Option<OsString> {
    volume_identity(disk.mount_point())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn disk_identity(disk: &sysinfo::Disk) -> Option<OsString> {
    // Linux exposes the device name, so multiple mounts share one identity.
    let name = disk.name();
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

#[cfg(target_os = "macos")]
fn disk_identity(_disk: &sysinfo::Disk) -> Option<OsString> {
    // Names are volume labels; multiple volumes can report the same driver counters.
    // Do not merge distinct labels or double-count aliases without a native identity.
    None
}

#[cfg(windows)]
fn volume_identity(mount: &Path) -> Option<OsString> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    #[link(name = "kernel32")]
    extern "system" {
        fn GetVolumeNameForVolumeMountPointW(
            mount: *const u16,
            volume: *mut u16,
            length: u32,
        ) -> i32;
    }
    let mut mount: Vec<u16> = mount.as_os_str().encode_wide().collect();
    if mount.last().copied() != Some(u16::from(b'\\')) {
        mount.push(u16::from(b'\\'));
    }
    mount.push(0);
    let mut volume = [0_u16; 50];
    // sysinfo reports each mount alias separately but queries its volume GUID device.
    // Volume labels are not unique and cannot be used for counter deduplication.
    let success =
        unsafe { GetVolumeNameForVolumeMountPointW(mount.as_ptr(), volume.as_mut_ptr(), 50) };
    if success == 0 {
        return None;
    }
    let length = volume.iter().position(|&value| value == 0)?;
    Some(OsString::from_wide(&volume[..length]))
}

pub struct ProcessCollector {
    tracker: ProcessTracker,
    network: NetworkCollector,
    network_error: Option<String>,
}

impl ProcessCollector {
    pub fn new() -> Self {
        Self {
            tracker: ProcessTracker::default(),
            network: NetworkCollector::new(),
            network_error: None,
        }
    }

    /// Called by the process worker on its own cadence, independently of system sampling.
    pub fn collect(&mut self) -> Vec<ProcessMetric> {
        // Fresh handles identify the current PID owner even when reuse occurs between scans.
        // Counter baselines live in the tracker, not in sysinfo's cached process objects.
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_memory()
                .with_disk_usage(),
        );
        let connections = process_network_connections();
        let mut observations: Vec<_> = system
            .processes()
            .iter()
            .filter_map(|(pid, process)| {
                #[cfg(windows)]
                let creation_token = crate::process_network::creation_token(pid.as_u32());
                #[cfg(not(windows))]
                let creation_token = None;
                // Reject a scan crossing PID reuse between sysinfo and the native lookup.
                if creation_token.is_some_and(|token| {
                    token / 10_000_000 - 11_644_473_600 != process.start_time()
                }) {
                    return None;
                }
                let pid = pid.to_string();
                let usage = process.disk_usage();
                let network_connections = connections.get(&pid).copied().unwrap_or(0);
                Some(ProcessObservation {
                    pid,
                    start_time: process.start_time(),
                    creation_token,
                    name: process.name().to_string_lossy().into_owned(),
                    memory_bytes: process.memory(),
                    read: usage.total_read_bytes,
                    write: usage.total_written_bytes,
                    network_connections,
                    network: None,
                })
            })
            .collect();
        let identities = self
            .tracker
            .identities()
            .cloned()
            .chain(observations.iter().map(ProcessObservation::identity))
            .collect();
        let network = self.network.snapshot(&identities);
        self.network_error.clone_from(&network.error);
        for observation in &mut observations {
            if network.available
                && observation.creation_token.is_some()
                && observation
                    .pid
                    .parse::<u32>()
                    .is_ok_and(|pid| !network.unavailable_pids.contains(&pid))
            {
                observation.network = Some(
                    network
                        .counters
                        .get(&observation.identity())
                        .copied()
                        .unwrap_or([0, 0]),
                );
            }
        }
        self.tracker
            .update_with_network(observations, Instant::now(), &network)
    }

    /// New totals start at the next scan (nominally within 5s); observation age is retained.
    pub fn reset_session_totals(&mut self) {
        self.tracker.reset_session_totals();
    }

    pub fn network_error(&self) -> Option<String> {
        self.network_error.clone()
    }
}

fn connection_counts(pids: impl IntoIterator<Item = u32>) -> HashMap<String, u32> {
    pids.into_iter().fold(HashMap::new(), |mut counts, pid| {
        *counts.entry(pid.to_string()).or_default() += 1;
        counts
    })
}

#[cfg(windows)]
fn process_network_connections() -> HashMap<String, u32> {
    connection_counts(tcp_owner_pids().into_iter().chain(udp_owner_pids()))
}

#[cfg(not(windows))]
fn process_network_connections() -> HashMap<String, u32> {
    HashMap::new()
}

#[cfg(windows)]
fn tcp_owner_pids() -> Vec<u32> {
    use windows_sys::Win32::{
        NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
        },
        Networking::WinSock::AF_INET,
    };

    unsafe {
        owner_pids_from_table(
            |buffer, size| {
                GetExtendedTcpTable(
                    buffer,
                    size,
                    0,
                    u32::from(AF_INET),
                    TCP_TABLE_OWNER_PID_ALL,
                    0,
                )
            },
            |row: MIB_TCPROW_OWNER_PID| row.dwOwningPid,
        )
    }
}

#[cfg(windows)]
fn udp_owner_pids() -> Vec<u32> {
    use windows_sys::Win32::{
        NetworkManagement::IpHelper::{
            GetExtendedUdpTable, MIB_UDPROW_OWNER_PID, UDP_TABLE_OWNER_PID,
        },
        Networking::WinSock::AF_INET,
    };

    unsafe {
        owner_pids_from_table(
            |buffer, size| {
                GetExtendedUdpTable(buffer, size, 0, u32::from(AF_INET), UDP_TABLE_OWNER_PID, 0)
            },
            |row: MIB_UDPROW_OWNER_PID| row.dwOwningPid,
        )
    }
}

#[cfg(windows)]
unsafe fn owner_pids_from_table<Row: Copy>(
    mut load: impl FnMut(*mut core::ffi::c_void, *mut u32) -> u32,
    owner_pid: impl Fn(Row) -> u32,
) -> Vec<u32> {
    let mut size = 0;
    let _ = load(core::ptr::null_mut(), &raw mut size);
    if !(4..=16 * 1024 * 1024).contains(&size) {
        return Vec::new();
    }

    let mut buffer = vec![0_u8; size as usize];
    if load(buffer.as_mut_ptr().cast(), &raw mut size) != 0 {
        return Vec::new();
    }

    let count = core::ptr::read_unaligned(buffer.as_ptr().cast::<u32>()) as usize;
    let row_size = core::mem::size_of::<Row>();
    let available = buffer.len().saturating_sub(core::mem::size_of::<u32>()) / row_size;
    (0..count.min(available))
        .map(|index| {
            let offset = core::mem::size_of::<u32>() + index * row_size;
            owner_pid(core::ptr::read_unaligned(
                buffer.as_ptr().add(offset).cast::<Row>(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{connection_counts, SystemCollector};

    #[test]
    #[ignore = "backend soak; NETDISK_SOAK_SECONDS defaults to 86400, not a WebView soak"]
    fn backend_soak() {
        use crate::{metrics::MetricsEngine, storage::HistoryStore};
        use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
        let seconds = std::env::var("NETDISK_SOAK_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(86_400);
        assert!((10..=604_800).contains(&seconds));
        let directory = std::env::temp_dir().join(format!(
            "netdisk-soak-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut history = HistoryStore::open(&directory).unwrap();
        let mut system = SystemCollector::new();
        let mut processes = super::ProcessCollector::new();
        let mut engine = MetricsEngine::default();
        let origin = Instant::now();
        let mut previous_at = origin;
        let mut previous: Option<[u64; 4]> = None;
        let mut peak_system = Duration::ZERO;
        let mut peak_process = Duration::ZERO;
        let mut peak_memory = 0;
        for tick in 0..=seconds {
            if let Some(remaining) =
                (origin + Duration::from_secs(tick)).checked_duration_since(Instant::now())
            {
                std::thread::sleep(remaining);
            }
            let start = Instant::now();
            let sample = system.collect();
            peak_system = peak_system.max(start.elapsed());
            let now = Instant::now();
            let elapsed = now.duration_since(previous_at);
            let metrics = engine.update(sample.counters, elapsed.as_secs_f64());
            let totals = [
                metrics.session_network_down,
                metrics.session_network_up,
                metrics.session_disk_read,
                metrics.session_disk_write,
            ];
            if let Some(old) = previous {
                let bytes = std::array::from_fn(|index| totals[index] - old[index]);
                history
                    .record(
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        u64::try_from(elapsed.as_millis()).unwrap(),
                        bytes,
                    )
                    .unwrap();
            }
            previous = Some(totals);
            previous_at = now;
            if tick % 5 == 0 {
                let start = Instant::now();
                let records = processes.collect();
                peak_process = peak_process.max(start.elapsed());
                assert!(
                    records.iter().filter(|record| !record.active).count()
                        <= super::process_tracker::TERMINATED_LIMIT
                );
                peak_memory = peak_memory.max(
                    records
                        .iter()
                        .find(|record| record.pid == std::process::id().to_string())
                        .map_or(0, |record| record.memory_bytes),
                );
            }
        }
        history.flush().unwrap();
        assert!(history.read_minutes(usize::MAX).unwrap().len() <= 10_080);
        assert!(history.read_days(usize::MAX).unwrap().len() <= 366);
        let file_bytes = std::fs::metadata(directory.join("history-v1.bin"))
            .unwrap()
            .len();
        assert_eq!(file_bytes, crate::storage::HISTORY_FILE_BYTES);
        eprintln!("backend_soak: elapsed_s={}, system_peak_ms={:.3}, process_peak_ms={:.3}, test_process_peak_ram_bytes={peak_memory}, history_bytes={file_bytes}, network_error={:?}", origin.elapsed().as_secs(), peak_system.as_secs_f64() * 1000.0, peak_process.as_secs_f64() * 1000.0, processes.network_error());
        drop(history);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[ignore = "30-second live Windows sampling probe; run explicitly"]
    fn live_sampling_probe() {
        use std::time::{Duration, Instant};
        let mut system = SystemCollector::new();
        let mut processes = super::ProcessCollector::new();
        let origin = Instant::now();
        let mut system_max = Duration::ZERO;
        let mut process_max = Duration::ZERO;
        let mut process_count = 0;
        let mut own_memory = 0;
        let mut disk_available = false;
        let own_pid = std::process::id().to_string();
        for tick in 0..30_u32 {
            let deadline = origin + Duration::from_secs(u64::from(tick));
            if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                std::thread::sleep(remaining);
            }
            let start = Instant::now();
            let sample = system.collect();
            system_max = system_max.max(start.elapsed());
            disk_available = sample.disk_available;
            if tick % 5 == 0 {
                let start = Instant::now();
                let sample = processes.collect();
                process_max = process_max.max(start.elapsed());
                process_count = sample.len();
                own_memory = sample
                    .iter()
                    .find(|process| process.pid == own_pid)
                    .map_or(0, |process| process.memory_bytes);
            }
        }
        eprintln!("live_probe: system_max_ms={:.2}, process_max_ms={:.2}, process_records={}, test_process_ram_bytes={}, disk_available={}",
            system_max.as_secs_f64() * 1000.0, process_max.as_secs_f64() * 1000.0,
            process_count, own_memory, disk_available);
        assert!(process_count > 0);
        assert!(own_memory > 0);
    }

    #[test]
    fn system_sampling_does_not_enumerate_processes() {
        let mut collector = SystemCollector::new();
        let collection = collector.collect();
        assert!(collector.system.processes().is_empty());
        assert_eq!(collection.counters.network_down, 0);
        assert_eq!(collection.counters.disk_read, 0);
    }

    #[test]
    fn network_connections_are_aggregated_by_owner_pid() {
        let connections = connection_counts([42, 42, 18, 42, 18]);
        assert_eq!(connections.get("42"), Some(&3));
        assert_eq!(connections.get("18"), Some(&2));
    }

    #[cfg(windows)]
    #[test]
    fn windows_disk_identity_is_a_volume_guid_not_a_label() {
        let root = format!("{}\\", std::env::var("SystemDrive").unwrap());
        let identity = super::volume_identity(std::path::Path::new(&root)).unwrap();
        assert!(identity.to_string_lossy().starts_with(r"\\?\Volume{"));
    }
}
