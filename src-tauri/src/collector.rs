use crate::metrics::CounterSample;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};
use sysinfo::{Networks, ProcessesToUpdate, System};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetric {
    pub pid: String,
    pub name: String,
    pub memory_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub session_disk_read: u64,
    pub session_disk_write: u64,
    pub uptime_secs: u64,
    pub network_connections: u32,
    pub network_down_bps: u64,
    pub network_up_bps: u64,
}

#[derive(Clone, Copy)]
struct DiskCounters {
    read: u64,
    write: u64,
}
pub struct Collection {
    pub counters: CounterSample,
    pub processes: Vec<ProcessMetric>,
    pub interfaces: Vec<String>,
}
pub struct SystemCollector {
    system: System,
    networks: Networks,
    process_session_baselines: HashMap<String, DiskCounters>,
    process_observed_at: HashMap<String, Instant>,
    process_history: HashMap<String, ProcessMetric>,
}
impl SystemCollector {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            networks: Networks::new_with_refreshed_list(),
            process_session_baselines: HashMap::new(),
            process_observed_at: HashMap::new(),
            process_history: HashMap::new(),
        }
    }

    pub fn reset_session_totals(&mut self) {
        self.process_session_baselines.clear();
        self.process_history.clear();
    }
    pub fn collect(&mut self) -> Collection {
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        self.networks.refresh(true);
        let (network_down, network_up) = self.networks.values().fold((0, 0), |(down, up), data| {
            (down + data.total_received(), up + data.total_transmitted())
        });
        let mut disk_read = 0;
        let mut disk_write = 0;
        let mut observed_pids = HashSet::new();
        let network_connections = process_network_connections();
        let mut processes = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| {
                let usage = process.disk_usage();
                disk_read += usage.total_read_bytes;
                disk_write += usage.total_written_bytes;
                let pid = pid.to_string();
                observed_pids.insert(pid.clone());
                let uptime_secs = self
                    .process_observed_at
                    .entry(pid.clone())
                    .or_insert_with(Instant::now)
                    .elapsed()
                    .as_secs();
                let baseline =
                    self.process_session_baselines
                        .entry(pid.clone())
                        .or_insert(DiskCounters {
                            read: usage.total_read_bytes,
                            write: usage.total_written_bytes,
                        });
                let network_connections = network_connections.get(&pid).copied().unwrap_or(0);
                let metric = ProcessMetric {
                    pid,
                    name: process.name().to_string_lossy().into_owned(),
                    memory_bytes: process.memory(),
                    disk_read_bps: usage.read_bytes,
                    disk_write_bps: usage.written_bytes,
                    session_disk_read: process_session_bytes(usage.total_read_bytes, baseline.read),
                    session_disk_write: process_session_bytes(
                        usage.total_written_bytes,
                        baseline.write,
                    ),
                    uptime_secs,
                    network_connections,
                    network_down_bps: 0,
                    network_up_bps: 0,
                };
                self.process_history.insert(metric.pid.clone(), metric.clone());
                metric
            })
            .collect::<Vec<_>>();

        self.process_observed_at
            .retain(|pid, _| observed_pids.contains(pid));

        // Add terminated processes that have recorded session activity
        let mut inactive_processes = Vec::new();
        for (pid, historic) in &self.process_history {
            if !observed_pids.contains(pid) {
                if historic.session_disk_read > 0 || historic.session_disk_write > 0 {
                    let mut terminated = historic.clone();
                    terminated.memory_bytes = 0;
                    terminated.disk_read_bps = 0;
                    terminated.disk_write_bps = 0;
                    terminated.network_connections = 0;
                    terminated.network_down_bps = 0;
                    terminated.network_up_bps = 0;
                    inactive_processes.push(terminated);
                }
            }
        }
        processes.extend(inactive_processes);

        processes.sort_unstable_by(|left, right| {
            let left_active = left.disk_read_bps + left.disk_write_bps;
            let right_active = right.disk_read_bps + right.disk_write_bps;
            if left_active != right_active {
                right_active.cmp(&left_active)
            } else {
                let left_session = left.session_disk_read + left.session_disk_write;
                let right_session = right.session_disk_read + right.session_disk_write;
                right_session.cmp(&left_session)
            }
        });

        Collection {
            counters: CounterSample::new(network_down, network_up, disk_read, disk_write),
            processes,
            interfaces: self.networks.keys().map(ToString::to_string).collect(),
        }
    }
}

fn process_session_bytes(total: u64, baseline: u64) -> u64 {
    total.saturating_sub(baseline)
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
                GetExtendedTcpTable(buffer, size, 0, AF_INET as u32, TCP_TABLE_OWNER_PID_ALL, 0)
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
                GetExtendedUdpTable(buffer, size, 0, AF_INET as u32, UDP_TABLE_OWNER_PID, 0)
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
    let _ = load(core::ptr::null_mut(), &mut size);
    if size < core::mem::size_of::<u32>() as u32 {
        return Vec::new();
    }

    let mut buffer = vec![0_u8; size as usize];
    if load(buffer.as_mut_ptr().cast(), &mut size) != 0 {
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
fn observed_duration_secs(observed_at: Instant, current: Instant) -> u64 {
    current.duration_since(observed_at).as_secs()
}

#[cfg(test)]
mod tests {
    use super::{connection_counts, observed_duration_secs, process_session_bytes};
    use std::time::{Duration, Instant};

    #[test]
    fn session_bytes_start_from_the_first_observed_counter() {
        assert_eq!(process_session_bytes(1_700, 1_000), 700);
    }

    #[test]
    fn process_duration_starts_when_monitor_first_observes_it() {
        let observed_at = Instant::now();
        let current = observed_at + Duration::from_secs(15);

        assert_eq!(observed_duration_secs(observed_at, current), 15);
    }

    #[test]
    fn network_connections_are_aggregated_by_owner_pid() {
        let connections = connection_counts([42, 42, 18, 42, 18]);

        assert_eq!(connections.get("42"), Some(&3));
        assert_eq!(connections.get("18"), Some(&2));
    }
}
