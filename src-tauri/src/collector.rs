use crate::metrics::{CounterAccumulator, CounterSample};
use crate::metrics_core::VolumeUsage;
use crate::process_network::NetworkCollector;
#[path = "process_tracker.rs"]
mod process_tracker;
pub use process_tracker::ProcessMetric;
pub(crate) use process_tracker::TERMINATED_LIMIT;
use process_tracker::{ProcessObservation, ProcessTracker};
#[cfg(windows)]
use std::path::Path;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};
use sysinfo::{
    CpuRefreshKind, DiskRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessRefreshKind,
    ProcessesToUpdate, System,
};

pub use crate::metrics_core::SystemObservation as Collection;

pub struct SystemCollector {
    system: System,
    networks: Networks,
    disks: Disks,
    network_counters: CounterAccumulator<String>,
    #[cfg(not(target_os = "macos"))]
    disk_counters: CounterAccumulator<OsString>,
    disk_identities: HashMap<PathBuf, OsString>,
    volumes: Vec<VolumeUsage>,
    last_disk_list_refresh: Instant,
    last_wall_sample: SystemTime,
    #[cfg(target_os = "macos")]
    macos_disk_counters: CounterAccumulator<u64>,
}

impl SystemCollector {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        // macOS needs the underlying IOKit driver identity for I/O; storage space uses sysinfo.
        let disk_init_refresh = if cfg!(target_os = "macos") {
            DiskRefreshKind::nothing().with_storage()
        } else {
            DiskRefreshKind::nothing().with_io_usage().with_storage()
        };
        let disks = Disks::new_with_refreshed_list_specifics(disk_init_refresh);
        let disk_identities = disk_identities(&disks);
        let volumes = collect_volumes(&disks, &disk_identities);
        Self {
            system,
            networks: Networks::new_with_refreshed_list(),
            disks,
            network_counters: CounterAccumulator::default(),
            #[cfg(not(target_os = "macos"))]
            disk_counters: CounterAccumulator::default(),
            disk_identities,
            volumes,
            last_disk_list_refresh: Instant::now(),
            last_wall_sample: SystemTime::now(),
            #[cfg(target_os = "macos")]
            macos_disk_counters: CounterAccumulator::default(),
        }
    }

    pub fn collect(&mut self) -> Collection {
        let wall_now = SystemTime::now();
        let wall_jump = wall_now
            .duration_since(self.last_wall_sample)
            .unwrap_or_default()
            >= Duration::from_secs(15);
        self.last_wall_sample = wall_now;

        self.system
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        self.system
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        let raw_cpu = self.system.global_cpu_usage();
        let cpu_usage_percent = if raw_cpu.is_finite() {
            raw_cpu.clamp(0.0, 100.0)
        } else {
            0.0
        };
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

        if wall_jump || self.last_disk_list_refresh.elapsed() >= Duration::from_secs(30) {
            let full_refresh = if cfg!(target_os = "macos") {
                DiskRefreshKind::nothing().with_storage()
            } else {
                DiskRefreshKind::nothing().with_io_usage().with_storage()
            };
            self.disks.refresh_specifics(true, full_refresh);
            self.disk_identities = disk_identities(&self.disks);
            self.volumes = collect_volumes(&self.disks, &self.disk_identities);
            self.last_disk_list_refresh = Instant::now();
        } else if !cfg!(target_os = "macos") {
            let io_refresh = DiskRefreshKind::nothing().with_io_usage();
            for disk in self.disks.list_mut() {
                disk.refresh_specifics(io_refresh);
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
            cpu_usage_percent,
            volumes: self.volumes.clone(),
        }
    }
}

fn collect_volumes(
    disks: &Disks,
    disk_identities: &HashMap<PathBuf, OsString>,
) -> Vec<VolumeUsage> {
    let mut seen = HashSet::new();
    let mut volumes = Vec::new();
    for disk in disks {
        let total_bytes = disk.total_space();
        if total_bytes == 0 {
            continue;
        }
        let dedup_key = disk_identities
            .get(disk.mount_point())
            .cloned()
            .unwrap_or_else(|| disk.mount_point().as_os_str().to_os_string());
        if !seen.insert(dedup_key) {
            continue;
        }
        let available_bytes = disk.available_space();
        volumes.push(VolumeUsage {
            mount_point: disk.mount_point().to_string_lossy().into_owned(),
            name: disk.name().to_string_lossy().into_owned(),
            file_system: disk.file_system().to_string_lossy().into_owned(),
            total_bytes,
            available_bytes,
            used_bytes: total_bytes.saturating_sub(available_bytes),
            is_removable: disk.is_removable(),
        });
    }
    volumes.sort_unstable_by(|left, right| left.mount_point.cmp(&right.mount_point));
    volumes
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
    #[cfg(windows)]
    identity_cache: crate::process_network::PidIdentityCache,
    network_error: Option<String>,
}

impl ProcessCollector {
    pub fn new() -> Self {
        Self {
            tracker: ProcessTracker::default(),
            network: NetworkCollector::new(),
            #[cfg(windows)]
            identity_cache: crate::process_network::PidIdentityCache::default(),
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
        #[cfg(windows)]
        let mut seen_pids = std::collections::HashSet::with_capacity(system.processes().len());
        let mut observations: Vec<_> = system
            .processes()
            .iter()
            .filter_map(|(pid, process)| {
                let raw_pid = pid.as_u32();
                if raw_pid == 0 {
                    return None;
                }
                #[cfg(windows)]
                let creation_token = {
                    seen_pids.insert(raw_pid);
                    self.identity_cache.resolve(raw_pid)
                };
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
        #[cfg(windows)]
        self.identity_cache.retain_seen(&seen_pids);
        let identities = self
            .tracker
            .identities()
            .filter(|identity| !identity.starts_with("0:"))
            .cloned()
            .chain(
                observations
                    .iter()
                    .filter(|obs| obs.creation_token.is_some())
                    .map(ProcessObservation::identity),
            )
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

#[cfg(any(windows, test))]
fn connection_counts(pids: impl IntoIterator<Item = u32>) -> HashMap<String, u32> {
    pids.into_iter().fold(HashMap::new(), |mut counts, pid| {
        *counts.entry(pid.to_string()).or_default() += 1;
        counts
    })
}

#[cfg(windows)]
fn process_network_connections() -> HashMap<String, u32> {
    connection_counts(
        tcp_owner_pids()
            .into_iter()
            .chain(tcp6_owner_pids())
            .chain(udp_owner_pids())
            .chain(udp6_owner_pids()),
    )
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
fn tcp6_owner_pids() -> Vec<u32> {
    use windows_sys::Win32::{
        NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
        },
        Networking::WinSock::AF_INET6,
    };

    unsafe {
        owner_pids_from_table(
            |buffer, size| {
                GetExtendedTcpTable(
                    buffer,
                    size,
                    0,
                    u32::from(AF_INET6),
                    TCP_TABLE_OWNER_PID_ALL,
                    0,
                )
            },
            |row: MIB_TCP6ROW_OWNER_PID| row.dwOwningPid,
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
fn udp6_owner_pids() -> Vec<u32> {
    use windows_sys::Win32::{
        NetworkManagement::IpHelper::{
            GetExtendedUdpTable, MIB_UDP6ROW_OWNER_PID, UDP_TABLE_OWNER_PID,
        },
        Networking::WinSock::AF_INET6,
    };

    unsafe {
        owner_pids_from_table(
            |buffer, size| {
                GetExtendedUdpTable(buffer, size, 0, u32::from(AF_INET6), UDP_TABLE_OWNER_PID, 0)
            },
            |row: MIB_UDP6ROW_OWNER_PID| row.dwOwningPid,
        )
    }
}

#[cfg(any(windows, test))]
unsafe fn owner_pids_from_table<Row: Copy>(
    mut load: impl FnMut(*mut core::ffi::c_void, *mut u32) -> u32,
    owner_pid: impl Fn(Row) -> u32,
) -> Vec<u32> {
    const MAX_TABLE_BYTES: u32 = 16 * 1024 * 1024;
    const INSUFFICIENT_BUFFER: u32 = 122;
    let mut size = 0;
    let initial = load(core::ptr::null_mut(), &raw mut size);
    if initial != 0 && initial != INSUFFICIENT_BUFFER {
        return Vec::new();
    }
    for _ in 0..3 {
        if !(4..=MAX_TABLE_BYTES).contains(&size) {
            return Vec::new();
        }
        let headroom = u32::try_from(16 * core::mem::size_of::<Row>()).unwrap_or(1024);
        let alloc_size = size.saturating_add(headroom).min(MAX_TABLE_BYTES);
        let Ok(buf_len) = usize::try_from(alloc_size) else {
            return Vec::new();
        };
        size = alloc_size;
        let mut buffer = vec![0_u8; buf_len];
        let status = load(buffer.as_mut_ptr().cast(), &raw mut size);
        if status == 0 {
            let count = core::ptr::read_unaligned(buffer.as_ptr().cast::<u32>()) as usize;
            let row_size = core::mem::size_of::<Row>();
            let available = buffer.len().saturating_sub(core::mem::size_of::<u32>()) / row_size;
            return (0..count.min(available))
                .map(|index| {
                    let offset = core::mem::size_of::<u32>() + index * row_size;
                    owner_pid(core::ptr::read_unaligned(
                        buffer.as_ptr().add(offset).cast::<Row>(),
                    ))
                })
                .collect();
        }
        if status != INSUFFICIENT_BUFFER {
            return Vec::new();
        }
    }
    Vec::new()
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

    #[test]
    fn socket_table_retries_on_insufficient_buffer_growth() {
        let mut calls = 0_u32;
        let pids = unsafe {
            super::owner_pids_from_table(
                |buffer, size| {
                    calls += 1;
                    if buffer.is_null() {
                        *size = 8;
                        return 122;
                    }
                    if calls == 2 {
                        *size = 16;
                        return 122;
                    }
                    let slice = std::slice::from_raw_parts_mut(buffer.cast::<u32>(), 4);
                    slice[0] = 3;
                    slice[1] = 101;
                    slice[2] = 202;
                    slice[3] = 101;
                    0
                },
                |pid: u32| pid,
            )
        };
        assert_eq!(calls, 3);
        assert_eq!(pids, vec![101, 202, 101]);
    }

    #[test]
    fn socket_table_rejects_oversized_buffers() {
        let pids = unsafe {
            super::owner_pids_from_table(
                |_buffer, size| {
                    *size = 32 * 1024 * 1024;
                    122
                },
                |pid: u32| pid,
            )
        };
        assert!(pids.is_empty());
    }
}
