//! Observed TCP/UDP bytes from an owned real-time kernel ETW session.
//! No ETL file, no global NT Kernel Logger takeover, no inferred PID allocation.
//! Counts only v2 send/receive observations, not NIC wire bytes or all protocols.
//! Retransmit/copy events are excluded. Schema, capacity or global ETW loss makes
//! `available` false for this collector's lifetime. Attribution gaps disqualify
//! only their PID via `unavailable_pids`; other PIDs remain usable with a warning.
//! Each cache is bounded to 8,192 entries. Snapshot retention removes PID gaps
//! absent from the caller's identities after returning their final warning;
//! a disqualified PID is not queried per packet again while it remains retained.
//! One serialized callback, one nonblocking process-liveness syscall and hash-map
//! work per packet remain. First observations open a process handle; terminated
//! handles require a time query. High packet rates can overrun the finite ETW
//! buffers; losses are queried each second, so detection is not instantaneous.
//! A fixed app GUID and machine-wide ownership mutex permit one bounded orphan
//! recovery attempt on restart. Query/permission/foreign-owner failures require
//! operator intervention; no automatic recovery or lossless 24/7 claim is made.
//! Legacy PID-named / auto-GUID sessions are deliberately not adopted or stopped.
use std::collections::{HashMap, HashSet};

#[cfg(any(all(windows, feature = "desktop"), test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transport {
    Tcp,
    Udp,
}
#[cfg(any(all(windows, feature = "desktop"), test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Packet {
    pid: u32,
    bytes: u64,
    download: bool,
}

#[cfg(any(all(windows, feature = "desktop"), test))]
fn decode_packet(_transport: Transport, version: u8, opcode: u8, payload: &[u8]) -> Option<Packet> {
    // Microsoft v2 MOF: PID/size at 0/4, followed by two addresses and two ports.
    // Validate the common 20-byte IPv4 / 44-byte IPv6 prefix; unused trailing
    // connection fields differ by transport and native pointer width.
    // https://learn.microsoft.com/en-us/windows/win32/etw/tcpip
    // https://learn.microsoft.com/en-us/windows/win32/etw/udpip
    // Connect, retransmit and copy events must not inflate byte totals.
    let download = match opcode {
        10 | 26 => false,
        11 | 27 => true,
        _ => return None,
    };
    let prefix = if opcode < 26 { 20 } else { 44 };
    if version != 2 || payload.len() < prefix {
        return None;
    }
    let pid = u32::from_le_bytes(payload[..4].try_into().ok()?);
    let bytes = u64::from(u32::from_le_bytes(payload[4..8].try_into().ok()?));
    Some(Packet {
        pid,
        bytes,
        download,
    })
}
#[cfg(any(all(windows, feature = "desktop"), test))]
fn belongs_to_process(created: u64, ended: Option<u64>, event: u64) -> bool {
    event >= created && ended.is_none_or(|end| event <= end)
}

#[cfg(any(all(windows, feature = "desktop"), test))]
fn retain_process(pid: u32, created: u64, live: bool, identities: &HashSet<String>) -> bool {
    identities.contains(&format!("{pid}:{created}")) || live
}

#[cfg(any(all(windows, feature = "desktop"), test))]
struct CounterBank {
    counters: HashMap<String, [u64; 2]>,
    limit: usize,
}
#[cfg(any(all(windows, feature = "desktop"), test))]
impl CounterBank {
    fn new(limit: usize) -> Self {
        Self {
            counters: HashMap::new(),
            limit,
        }
    }
    fn add(&mut self, pid: u32, created: u64, download: bool, bytes: u64) -> bool {
        let identity = format!("{pid}:{created}");
        if !self.counters.contains_key(&identity) && self.counters.len() >= self.limit {
            return false;
        }
        let counter = self.counters.entry(identity).or_default();
        let index = usize::from(!download);
        let Some(next) = counter[index].checked_add(bytes) else {
            return false;
        };
        counter[index] = next;
        true
    }
    fn retain(&mut self, identities: &HashSet<String>) {
        self.counters
            .retain(|identity, _| identities.contains(identity));
    }
}

#[derive(Default)]
pub(crate) struct NetworkSnapshot {
    pub counters: HashMap<String, [u64; 2]>,
    pub unavailable_pids: HashSet<u32>,
    pub available: bool,
    pub error: Option<String>,
}

#[cfg(all(windows, feature = "desktop"))]
pub(crate) use native::{creation_token, NetworkCollector};
#[cfg(not(all(windows, feature = "desktop")))]
pub(crate) struct NetworkCollector;
#[cfg(not(all(windows, feature = "desktop")))]
impl NetworkCollector {
    pub fn new() -> Self {
        Self
    }
    #[allow(clippy::unused_self)] // Preserve the uniform platform collector API.
    pub fn snapshot(&self, _identities: &HashSet<String>) -> NetworkSnapshot {
        NetworkSnapshot {
            error: Some("Network theo PID chưa có collector native trên nền tảng này".to_owned()),
            ..NetworkSnapshot::default()
        }
    }
}

#[cfg(all(windows, feature = "desktop"))]
mod native {
    use super::{
        belongs_to_process, decode_packet, retain_process, CounterBank, NetworkSnapshot, Transport,
    };
    use crate::runtime::StopToken;
    use std::{
        collections::{HashMap, HashSet},
        marker::PhantomData,
        ptr::null,
        rc::Rc,
        sync::{Arc, Mutex},
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    };
    use windows_sys::{
        core::GUID,
        Win32::{
            Foundation::{
                CloseHandle, GetLastError, SetLastError, ERROR_ALREADY_EXISTS, ERROR_MORE_DATA,
                ERROR_NOT_ALL_ASSIGNED, ERROR_WMI_INSTANCE_NOT_FOUND, FILETIME, HANDLE, LUID,
                WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
            },
            Security::{
                AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES,
                SE_PRIVILEGE_ENABLED, SE_SYSTEM_PROFILE_NAME, TOKEN_ADJUST_PRIVILEGES,
                TOKEN_PRIVILEGES, TOKEN_QUERY,
            },
            System::{
                Diagnostics::Etw::{
                    CloseTrace, ControlTraceW, OpenTraceW, ProcessTrace, StartTraceW,
                    CONTROLTRACE_HANDLE, EVENT_RECORD, EVENT_TRACE_CONTROL_QUERY,
                    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_FLAG_NETWORK_TCPIP, EVENT_TRACE_LOGFILEW,
                    EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE,
                    EVENT_TRACE_SYSTEM_LOGGER_MODE, PROCESSTRACE_HANDLE,
                    PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME,
                    WNODE_FLAG_TRACED_GUID,
                },
                Threading::{
                    CreateMutexW, GetCurrentProcess, GetProcessTimes, OpenProcess,
                    OpenProcessToken, ReleaseMutex, WaitForSingleObject,
                    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
                },
            },
        },
    };
    const TCP: GUID = GUID::from_u128(0x9a280ac0_c8e0_11d1_84e2_00c04fb998a2);
    const UDP: GUID = GUID::from_u128(0xbf3a50c5_a9c9_4988_a005_2df0b7c80f80);
    const APP_GUID: GUID = GUID::from_u128(0x465ef2b7_758e_4ee8_99bf_2a7fa7f085ec);
    const SESSION_NAME: &str = "NetDisk Monitor Network v1";
    #[cfg(test)]
    const SYSTEM: GUID = GUID::from_u128(0x9e814aad_3204_11d2_9a82_006008a86939);
    const LIMIT: usize = 8192;

    // HANDLE is opaque, never dereferenced. Integer storage allows ownership to
    // move with Context between threads; every access is serialized by its mutex.
    struct OwnedHandle(isize);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0 as HANDLE);
            }
        }
    }

    struct SessionOwner {
        handle: OwnedHandle,
        owns_mutex: bool,
        // Windows mutex ownership belongs to the acquiring thread.
        _thread_bound: PhantomData<Rc<()>>,
    }
    impl SessionOwner {
        fn acquire() -> Result<Self, String> {
            Self::acquire_named(windows_sys::core::w!(
                "Global\\NetDiskMonitor.NetworkETW-v1"
            ))
        }
        fn acquire_named(name: *const u16) -> Result<Self, String> {
            let raw = unsafe { CreateMutexW(null(), 0, name) };
            if raw.is_null() {
                return Err(format!("ETW ownership mutex creation: {}", unsafe {
                    GetLastError()
                }));
            }
            let handle = OwnedHandle(raw as isize);
            match unsafe { WaitForSingleObject(raw, 0) } {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self {
                    handle,
                    owns_mutex: true,
                    _thread_bound: PhantomData,
                }),
                WAIT_TIMEOUT => Err(
                    "ETW collector already owned by an active app instance; no session was stopped"
                        .into(),
                ),
                status => Err(format!(
                    "ETW ownership mutex wait: {status}, error={}",
                    unsafe { GetLastError() }
                )),
            }
        }
    }
    impl Drop for SessionOwner {
        fn drop(&mut self) {
            if self.owns_mutex {
                unsafe { ReleaseMutex(self.handle.0 as HANDLE) };
            }
        }
    }
    struct ProcessHandle {
        raw: OwnedHandle,
        created: u64,
        ended: Option<u64>,
    }
    impl ProcessHandle {
        fn open(pid: u32) -> Option<Self> {
            let raw = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    pid,
                )
            };
            if raw.is_null() {
                return None;
            }
            let mut handle = Self {
                raw: OwnedHandle(raw as isize),
                created: 0,
                ended: None,
            };
            let (created, _) = handle.times()?;
            handle.created = created;
            Some(handle)
        }
        fn times(&self) -> Option<(u64, Option<u64>)> {
            process_times(self.raw.0 as HANDLE)
        }
        fn live(&self) -> bool {
            unsafe { WaitForSingleObject(self.raw.0 as HANDLE, 0) == WAIT_TIMEOUT }
        }
        fn matches(&mut self, event: u64) -> bool {
            if event < self.created {
                return false;
            }
            if let Some(end) = self.ended {
                return belongs_to_process(self.created, Some(end), event);
            }
            match unsafe { WaitForSingleObject(self.raw.0 as HANDLE, 0) } {
                WAIT_TIMEOUT => true,
                WAIT_OBJECT_0 => {
                    let Some((_, Some(end))) = self.times() else {
                        return false;
                    };
                    self.ended = Some(end);
                    belongs_to_process(self.created, Some(end), event)
                }
                _ => false,
            }
        }
    }
    fn process_times(raw: HANDLE) -> Option<(u64, Option<u64>)> {
        let (mut created, mut ended, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        if unsafe {
            GetProcessTimes(
                raw,
                &raw mut created,
                &raw mut ended,
                &raw mut kernel,
                &raw mut user,
            )
        } == 0
        {
            return None;
        }
        let ticks =
            |time: FILETIME| u64::from(time.dwHighDateTime) << 32 | u64::from(time.dwLowDateTime);
        let end = ticks(ended);
        Some((ticks(created), (end != 0).then_some(end)))
    }
    pub(crate) fn creation_token(pid: u32) -> Option<u64> {
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if raw.is_null() {
            return None;
        }
        let owned = OwnedHandle(raw as isize);
        process_times(owned.0 as HANDLE).map(|(created, _)| created)
    }

    struct ProfilePrivilege {
        token: OwnedHandle,
        previous: TOKEN_PRIVILEGES,
    }
    impl ProfilePrivilege {
        fn enable() -> Result<Self, String> {
            let mut token = std::ptr::null_mut();
            if unsafe {
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                    &raw mut token,
                )
            } == 0
            {
                return Err(format!("OpenProcessToken ETW: {}", unsafe {
                    GetLastError()
                }));
            }
            let token = OwnedHandle(token as isize);
            let mut luid = LUID::default();
            if unsafe { LookupPrivilegeValueW(null(), SE_SYSTEM_PROFILE_NAME, &raw mut luid) } == 0
            {
                return Err(format!("LookupPrivilegeValue ETW: {}", unsafe {
                    GetLastError()
                }));
            }
            let requested = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            let mut previous = TOKEN_PRIVILEGES::default();
            let mut returned = 0;
            // This can only enable privileges already in the token, never elevate or request UAC.
            unsafe {
                SetLastError(0);
            }
            let success = unsafe {
                AdjustTokenPrivileges(
                    token.0 as HANDLE,
                    0,
                    &raw const requested,
                    u32::try_from(std::mem::size_of::<TOKEN_PRIVILEGES>())
                        .expect("small native struct"),
                    &raw mut previous,
                    &raw mut returned,
                )
            };
            let error = unsafe { GetLastError() };
            if success == 0 || error != 0 {
                return Err(if error == ERROR_NOT_ALL_ASSIGNED {
                    "ETW requires SeSystemProfilePrivilege already present in this token; reopen as Administrator. No UAC was requested.".to_owned()
                } else {
                    format!("AdjustTokenPrivileges ETW: {error}")
                });
            }
            Ok(Self { token, previous })
        }
    }
    impl Drop for ProfilePrivilege {
        fn drop(&mut self) {
            if self.previous.PrivilegeCount != 0 {
                unsafe {
                    AdjustTokenPrivileges(
                        self.token.0 as HANDLE,
                        0,
                        &raw const self.previous,
                        0,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    );
                }
            }
        }
    }

    struct Context {
        bank: CounterBank,
        handles: HashMap<u32, ProcessHandle>,
        ready: bool,
        error: Option<String>,
        handle_limit: usize,
        dropped_packets: u64,
        dropped_bytes: u64,
        last_drop: Option<&'static str>,
        unavailable_pids: HashSet<u32>,
    }
    impl Context {
        fn fail(&mut self, error: String) {
            self.error = Some(error);
            self.ready = false;
        }
        fn drop_packet(&mut self, bytes: u64, cause: &'static str) {
            self.dropped_packets = self.dropped_packets.saturating_add(1);
            self.dropped_bytes = self.dropped_bytes.saturating_add(bytes);
            self.last_drop = Some(cause);
        }
        fn error(&self) -> Option<String> {
            let loss = self.last_drop.map(|cause| {
                format!(
                    "ETW gaps: dropped {} packets / {} known bytes ({cause})",
                    self.dropped_packets, self.dropped_bytes
                )
            });
            match (&self.error, loss) {
                (Some(error), Some(loss)) => Some(format!("{error}; {loss}")),
                (Some(error), None) => Some(error.clone()),
                (None, loss) => loss,
            }
        }
        fn drop_global(&mut self, bytes: u64, cause: &'static str) {
            self.drop_packet(bytes, cause);
            self.fail(format!("ETW global failure: {cause}"));
        }
        fn drop_pid(&mut self, pid: u32, bytes: u64, cause: &'static str) {
            if !self.unavailable_pids.contains(&pid)
                && self.unavailable_pids.len() >= self.handle_limit
            {
                self.drop_global(bytes, "unavailable PID capacity");
                return;
            }
            self.unavailable_pids.insert(pid);
            self.drop_packet(bytes, cause);
        }
        fn attribute(&mut self, packet: super::Packet, timestamp: u64) {
            if self.unavailable_pids.contains(&packet.pid) {
                self.drop_packet(packet.bytes, "PID already disqualified");
                return;
            }
            if let Some(handle) = self.handles.get_mut(&packet.pid) {
                if handle.matches(timestamp) {
                    let created = handle.created;
                    self.count(packet, created);
                    return;
                }
            }
            self.handles.remove(&packet.pid);
            if self.handles.len() >= self.handle_limit {
                self.drop_global(packet.bytes, "process handle capacity");
                return;
            }
            let Some(mut handle) = ProcessHandle::open(packet.pid) else {
                self.drop_pid(
                    packet.pid,
                    packet.bytes,
                    "process unavailable or access denied",
                );
                return;
            };
            if !handle.matches(timestamp) {
                self.drop_pid(
                    packet.pid,
                    packet.bytes,
                    "event outside the held process lifetime",
                );
                return;
            }
            let created = handle.created;
            self.handles.insert(packet.pid, handle);
            self.count(packet, created);
        }
        fn count(&mut self, packet: super::Packet, created: u64) {
            if !self
                .bank
                .add(packet.pid, created, packet.download, packet.bytes)
            {
                self.drop_global(packet.bytes, "counter capacity or overflow");
            }
        }
    }
    pub(crate) struct NetworkCollector {
        shared: Arc<Mutex<Context>>,
        stop: StopToken,
        worker: Option<JoinHandle<()>>,
    }
    impl NetworkCollector {
        pub fn new() -> Self {
            let shared = Arc::new(Mutex::new(Context {
                bank: CounterBank::new(LIMIT),
                handles: HashMap::new(),
                ready: false,
                error: None,
                handle_limit: LIMIT,
                dropped_packets: 0,
                dropped_bytes: 0,
                last_drop: None,
                unavailable_pids: HashSet::new(),
            }));
            let stop = StopToken::default();
            let worker_shared = shared.clone();
            let worker_stop = stop.clone();
            let worker = thread::Builder::new()
                .name("monitor-etw-controller".into())
                .spawn(move || {
                    if let Err(error) = run(&worker_shared, &worker_stop) {
                        if let Ok(mut context) = worker_shared.lock() {
                            context.fail(error);
                        }
                    }
                });
            let worker = match worker {
                Ok(worker) => Some(worker),
                Err(error) => {
                    if let Ok(mut context) = shared.lock() {
                        context.fail(error.to_string());
                    }
                    None
                }
            };
            Self {
                shared,
                stop,
                worker,
            }
        }
        pub fn snapshot(&self, identities: &HashSet<String>) -> NetworkSnapshot {
            let Ok(mut context) = self.shared.lock() else {
                return NetworkSnapshot {
                    error: Some("ETW state unavailable".into()),
                    ..NetworkSnapshot::default()
                };
            };
            let error = context.error();
            let result = NetworkSnapshot {
                counters: context.bank.counters.clone(),
                unavailable_pids: context.unavailable_pids.clone(),
                available: context.ready && context.error.is_none(),
                error,
            };
            context.bank.retain(identities);
            context.handles.retain(|pid, handle| {
                retain_process(*pid, handle.created, handle.live(), identities)
            });
            let retained_pids: HashSet<u32> = identities
                .iter()
                .filter_map(|identity| identity.split_once(':')?.0.parse().ok())
                .collect();
            context
                .unavailable_pids
                .retain(|pid| retained_pids.contains(pid));
            result
        }
    }
    impl Drop for NetworkCollector {
        fn drop(&mut self) {
            self.stop.stop();
            // Parent owns this on its process worker, never the UI thread.
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    // Strings are stored in the same aligned allocation as EVENT_TRACE_PROPERTIES.
    struct Properties(Vec<u64>);
    impl Properties {
        fn new(name: &[u16]) -> Self {
            let bytes = std::mem::size_of::<EVENT_TRACE_PROPERTIES>() + name.len() * 2;
            let mut buffer = Self(vec![0; bytes.div_ceil(8)]);
            let pointer = buffer.0.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>();
            unsafe {
                (*pointer).Wnode.BufferSize =
                    u32::try_from(buffer.0.len() * 8).expect("bounded name");
                // Stable app identity permits verified orphan recovery, never SYSTEM takeover.
                (*pointer).Wnode.Guid = APP_GUID;
                (*pointer).Wnode.ClientContext = 1;
                (*pointer).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
                (*pointer).BufferSize = 64;
                (*pointer).MinimumBuffers = 16;
                (*pointer).MaximumBuffers = 64;
                (*pointer).LogFileMode =
                    EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_SYSTEM_LOGGER_MODE;
                (*pointer).FlushTimer = 1;
                (*pointer).EnableFlags = EVENT_TRACE_FLAG_NETWORK_TCPIP;
                (*pointer).LoggerNameOffset =
                    u32::try_from(std::mem::size_of::<EVENT_TRACE_PROPERTIES>())
                        .expect("small struct");
                std::ptr::copy_nonoverlapping(
                    name.as_ptr(),
                    // The u64 allocation and native struct's even size align the UTF-16 tail.
                    buffer
                        .0
                        .as_mut_ptr()
                        .cast::<u16>()
                        .add(std::mem::size_of::<EVENT_TRACE_PROPERTIES>() / 2),
                    name.len(),
                );
            }
            buffer
        }
        fn pointer(&mut self) -> *mut EVENT_TRACE_PROPERTIES {
            self.0.as_mut_ptr().cast()
        }
        fn for_control() -> Self {
            // Microsoft documents maximum 1,024-character session and file names.
            let structure = std::mem::size_of::<EVENT_TRACE_PROPERTIES>();
            let name_bytes = 1024 * 2;
            let bytes = structure + name_bytes * 2;
            let mut buffer = Self(vec![0; bytes.div_ceil(8)]);
            unsafe {
                let properties = &mut *buffer.pointer();
                properties.Wnode.BufferSize =
                    u32::try_from(buffer.0.len() * 8).expect("bounded control buffer");
                properties.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
                properties.LoggerNameOffset = u32::try_from(structure).expect("small struct");
                properties.LogFileNameOffset =
                    u32::try_from(structure + name_bytes).expect("bounded offset");
            }
            buffer
        }
    }

    fn recover_orphan(owner: &SessionOwner, name: &[u16]) -> Result<(), String> {
        let mut properties = Properties::for_control();
        let status = unsafe {
            ControlTraceW(
                CONTROLTRACE_HANDLE::default(),
                name.as_ptr(),
                properties.pointer(),
                EVENT_TRACE_CONTROL_QUERY,
            )
        };
        if status == ERROR_WMI_INSTANCE_NOT_FOUND {
            return Ok(());
        }
        if status != 0 {
            return Err(format!("ETW orphan query failed ({status}); no session was stopped. Manual ownership verification is required."));
        }
        let queried = unsafe { &*properties.pointer() };
        if !may_recover_owned_session(owner.owns_mutex, &queried.Wnode.Guid) {
            return Err("ETW session name belongs to a foreign GUID; no session was stopped. Manual resolution is required.".into());
        }
        // WNODE_HEADER.HistoricalContext is the returned session handle.
        // Never stop by name: a replacement with the same name must not be targeted.
        let controller = CONTROLTRACE_HANDLE {
            Value: unsafe { queried.Wnode.Anonymous1.HistoricalContext },
        };
        if controller.Value == 0 {
            return Err("ETW orphan query returned no handle; no session was stopped".into());
        }
        properties = Properties::for_control();
        let status = unsafe {
            ControlTraceW(
                controller,
                null(),
                properties.pointer(),
                EVENT_TRACE_CONTROL_QUERY,
            )
        };
        if status == ERROR_WMI_INSTANCE_NOT_FOUND {
            return Ok(());
        }
        if status != 0
            || !may_recover_owned_session(owner.owns_mutex, unsafe {
                &(*properties.pointer()).Wnode.Guid
            })
        {
            return Err(format!(
                "ETW orphan handle verification failed ({status}); no session was stopped"
            ));
        }
        properties = Properties::for_control();
        let status = unsafe {
            ControlTraceW(
                controller,
                null(),
                properties.pointer(),
                EVENT_TRACE_CONTROL_STOP,
            )
        };
        if matches!(status, 0 | ERROR_MORE_DATA | ERROR_WMI_INSTANCE_NOT_FOUND) {
            Ok(())
        } else {
            Err(format!(
                "ETW owned orphan stop failed ({status}); manual cleanup may be required"
            ))
        }
    }

    struct Session {
        name: Vec<u16>,
        properties: Properties,
        controller: CONTROLTRACE_HANDLE,
        trace: Option<PROCESSTRACE_HANDLE>,
        active: bool,
    }
    impl Session {
        fn start(owner: &SessionOwner) -> Result<Self, String> {
            let name: Vec<u16> = SESSION_NAME.encode_utf16().chain(Some(0)).collect();
            let mut properties = Properties::new(&name);
            let mut controller = CONTROLTRACE_HANDLE::default();
            let mut status =
                unsafe { StartTraceW(&raw mut controller, name.as_ptr(), properties.pointer()) };
            if status == ERROR_ALREADY_EXISTS {
                recover_orphan(owner, &name)?;
                properties = Properties::new(&name);
                controller = CONTROLTRACE_HANDLE::default();
                // One retry only, including an orphan that disappeared during query.
                status = unsafe {
                    StartTraceW(&raw mut controller, name.as_ptr(), properties.pointer())
                };
            }
            if status != 0 {
                return Err(if status == 5 || status == 1314 {
                    "Đo network theo PID cần quyền Administrator; mở lại app bằng Run as administrator để bật ETW.".into()
                } else if status == ERROR_ALREADY_EXISTS {
                    "ETW name or application GUID remains in use after one recovery attempt (183); no additional sessions were stopped. Manual ownership verification is required.".into()
                } else {
                    format!("StartTrace ETW: {status}")
                });
            }
            Ok(Self {
                name,
                properties,
                controller,
                trace: None,
                active: true,
            })
        }
        fn open(&mut self, shared: &Arc<Mutex<Context>>) -> Result<PROCESSTRACE_HANDLE, String> {
            let mut log = EVENT_TRACE_LOGFILEW {
                LoggerName: self.name.as_mut_ptr(),
                Context: Arc::as_ptr(shared).cast_mut().cast(),
                ..EVENT_TRACE_LOGFILEW::default()
            };
            log.Anonymous1.ProcessTraceMode =
                PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
            log.Anonymous2.EventRecordCallback = Some(packet_callback);
            let trace = unsafe { OpenTraceW(&raw mut log) };
            if trace.Value == u64::MAX {
                return Err(format!("OpenTrace ETW: {}", unsafe { GetLastError() }));
            }
            self.trace = Some(trace);
            Ok(trace)
        }
        fn control(&mut self, operation: u32) -> u32 {
            self.properties = Properties::for_control();
            unsafe {
                ControlTraceW(
                    self.controller,
                    null(),
                    self.properties.pointer(),
                    operation,
                )
            }
        }
        fn query_error(&mut self, status: u32) -> Option<String> {
            let statistics = unsafe { &*self.properties.pointer() };
            (status != 0 || statistics.EventsLost != 0 || statistics.RealTimeBuffersLost != 0).then(
                || {
                    format!(
                        "ETW incomplete: control={status}, eventsLost={}, buffersLost={}",
                        statistics.EventsLost, statistics.RealTimeBuffersLost
                    )
                },
            )
        }
        fn stop(&mut self) -> Result<(), String> {
            if !self.active {
                return Ok(());
            }
            let status = self.control(EVENT_TRACE_CONTROL_STOP);
            if matches!(status, 0 | ERROR_MORE_DATA | ERROR_WMI_INSTANCE_NOT_FOUND) {
                self.active = false;
            }
            self.query_error(status).map_or(Ok(()), Err)
        }
        fn close(&mut self) -> Result<(), String> {
            let Some(trace) = self.trace.take() else {
                return Ok(());
            };
            let status = unsafe { CloseTrace(trace) };
            // ERROR_CTX_CLOSE_PENDING means callbacks are finishing; their context stays held.
            if status == 0 || status == 7007 {
                Ok(())
            } else {
                Err(format!("CloseTrace ETW: {status}"))
            }
        }
    }
    impl Drop for Session {
        fn drop(&mut self) {
            let _ = self.stop();
            let _ = self.close();
        }
    }
    fn consume(trace: PROCESSTRACE_HANDLE, shared: &Arc<Mutex<Context>>) {
        // Keep the callback context alive until ProcessTrace has returned.
        let status = unsafe { ProcessTrace(&raw const trace, 1, null(), null()) };
        if status != 0 && status != 1223 {
            if let Ok(mut context) = shared.lock() {
                context.fail(format!("ProcessTrace ETW: {status}"));
            }
        }
    }
    fn run(shared: &Arc<Mutex<Context>>, stop: &StopToken) -> Result<(), String> {
        // Declared first, released last: held through session stop and consumer join.
        let owner = SessionOwner::acquire()?;
        let _privilege = ProfilePrivilege::enable()?;
        let mut session = Session::start(&owner)?;
        let trace = session.open(shared)?;
        let consumer_shared = Arc::clone(shared);
        let consumer = thread::Builder::new()
            .name("monitor-etw-events".into())
            .spawn(move || consume(trace, &consumer_shared))
            .map_err(|error| error.to_string())?;
        if let Ok(mut context) = shared.lock() {
            context.ready = true;
        }
        while stop.wait_until(Instant::now() + Duration::from_secs(1)) && !consumer.is_finished() {
            let status = session.control(EVENT_TRACE_CONTROL_QUERY);
            if let Some(error) = session.query_error(status) {
                if let Ok(mut context) = shared.lock() {
                    context.fail(error);
                }
            }
        }
        if consumer.is_finished() && stop.wait_until(Instant::now()) {
            if let Ok(mut context) = shared.lock() {
                context.fail("ETW consumer stopped unexpectedly".into());
            }
        }
        let stopped = session.stop();
        // Even if stopping the session fails, close the consumer before joining it.
        let closed = session.close();
        let joined = consumer.join();
        if let Ok(mut context) = shared.lock() {
            context.ready = false;
        }
        stopped?;
        closed?;
        joined.map_err(|_| "ETW consumer panicked".to_owned())
    }

    unsafe extern "system" fn packet_callback(record: *mut EVENT_RECORD) {
        let Some(record) = record.as_ref() else {
            return;
        };
        let Some(shared) = record.UserContext.cast::<Mutex<Context>>().as_ref() else {
            return;
        };
        let provider = record.EventHeader.ProviderId;
        let transport = if same_guid(&provider, &TCP) {
            Transport::Tcp
        } else if same_guid(&provider, &UDP) {
            Transport::Udp
        } else {
            return;
        };
        let descriptor = record.EventHeader.EventDescriptor;
        if !matches!(descriptor.Opcode, 10 | 11 | 26 | 27) {
            return;
        }
        let Ok(mut context) = shared.lock() else {
            return;
        };
        if record.UserData.is_null() || descriptor.Version != 2 {
            context.drop_global(0, "unsupported network schema or null payload");
            return;
        }
        let payload = std::slice::from_raw_parts(
            record.UserData.cast::<u8>(),
            usize::from(record.UserDataLength),
        );
        let Some(packet) = decode_packet(transport, descriptor.Version, descriptor.Opcode, payload)
        else {
            context.drop_global(0, "truncated network payload");
            return;
        };
        // Without RAW_TIMESTAMP, ProcessTrace converts QPC to FILETIME system ticks.
        let Ok(timestamp) = u64::try_from(record.EventHeader.TimeStamp) else {
            context.drop_pid(packet.pid, packet.bytes, "negative converted timestamp");
            return;
        };
        context.attribute(packet, timestamp);
    }

    fn same_guid(first: &GUID, second: &GUID) -> bool {
        first.data1 == second.data1
            && first.data2 == second.data2
            && first.data3 == second.data3
            && first.data4 == second.data4
    }

    fn may_recover_owned_session(owns_mutex: bool, session_guid: &GUID) -> bool {
        owns_mutex && same_guid(session_guid, &APP_GUID)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            io::{Read, Write},
            net::{TcpListener, TcpStream, UdpSocket},
        };

        #[test]
        fn orphan_recovery_requires_mutex_and_application_guid() {
            assert!(may_recover_owned_session(true, &APP_GUID));
            assert!(!may_recover_owned_session(false, &APP_GUID));
            for foreign in [SYSTEM, TCP, UDP, GUID::from_u128(0)] {
                assert!(!may_recover_owned_session(true, &foreign));
            }
        }

        #[test]
        fn session_properties_use_a_stable_application_guid() {
            let mut properties = Properties::new(&[b'N'.into(), 0]);
            assert!(same_guid(
                unsafe { &(*properties.pointer()).Wnode.Guid },
                &APP_GUID
            ));
        }

        fn test_mutex_name(label: &str) -> Vec<u16> {
            format!(
                "Local\\NetDiskMonitor.ETW-test-{label}-{}",
                std::process::id()
            )
            .encode_utf16()
            .chain(Some(0))
            .collect()
        }

        #[test]
        fn ownership_mutex_rejects_another_thread_without_waiting() {
            let name = test_mutex_name("contention");
            let owner = SessionOwner::acquire_named(name.as_ptr()).unwrap();
            let other_name = name.clone();
            let rejected = thread::spawn(move || {
                SessionOwner::acquire_named(other_name.as_ptr())
                    .is_err_and(|error| error.contains("active app instance"))
            })
            .join()
            .unwrap();
            assert!(rejected);
            drop(owner);
            let _next = SessionOwner::acquire_named(name.as_ptr()).unwrap();
        }

        #[test]
        fn ownership_mutex_accepts_a_dead_owner() {
            let name = test_mutex_name("abandonment");
            let other_name = name.clone();
            let raw = thread::spawn(move || {
                let owner = SessionOwner::acquire_named(other_name.as_ptr()).unwrap();
                let raw = owner.handle.0;
                // Simulate abrupt thread exit; the parent closes this retained handle.
                std::mem::forget(owner);
                raw
            })
            .join()
            .unwrap();
            let _retained = OwnedHandle(raw);
            let _recovered = SessionOwner::acquire_named(name.as_ptr()).unwrap();
        }

        #[test]
        fn control_buffer_is_bounded_and_starts_without_claiming_ownership() {
            let mut buffer = Properties::for_control();
            let properties = unsafe { &*buffer.pointer() };
            assert!(buffer.0.len() * 8 < 8192);
            assert!(same_guid(&properties.Wnode.Guid, &GUID::from_u128(0)));
            assert!(properties.LoggerNameOffset < properties.LogFileNameOffset);
            assert!(properties.LogFileNameOffset < properties.Wnode.BufferSize);
        }

        #[test]
        #[ignore = "needs an already elevated Windows token; captures owned TCP/UDP IPv4/IPv6 loopback traffic"]
        fn live_permission_and_loopback_capture() {
            let pid = std::process::id();
            let identity = format!("{pid}:{}", creation_token(pid).unwrap());
            let identities = HashSet::from([identity.clone()]);
            let collector = NetworkCollector::new();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let snapshot = collector.snapshot(&identities);
                if !snapshot.available {
                    if let Some(error) = snapshot.error {
                        if error.contains("SeSystemProfilePrivilege")
                            || error.contains("Administrator")
                        {
                            println!(
                                "ETW permission rejection verified; live capture NOT run: {error}"
                            );
                            return;
                        }
                        panic!("ETW startup failed: {error}");
                    }
                }
                if snapshot.available {
                    break;
                }
                assert!(Instant::now() < deadline, "ETW startup did not complete");
                thread::sleep(Duration::from_millis(50));
            }
            for (name, tcp, ipv6) in [
                ("TCP IPv4", true, false),
                ("UDP IPv4", false, false),
                ("TCP IPv6", true, true),
                ("UDP IPv6", false, true),
            ] {
                let before = collector
                    .snapshot(&identities)
                    .counters
                    .get(&identity)
                    .copied()
                    .unwrap_or_default();
                let bytes = loopback(tcp, ipv6);
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let snapshot = collector.snapshot(&identities);
                    assert!(snapshot.available, "{name}: {:?}", snapshot.error);
                    assert!(
                        !snapshot.unavailable_pids.contains(&pid),
                        "own PID attribution failed: {:?}",
                        snapshot.error
                    );
                    let after = snapshot
                        .counters
                        .get(&identity)
                        .copied()
                        .unwrap_or_default();
                    if after[0].saturating_sub(before[0]) >= bytes
                        && after[1].saturating_sub(before[1]) >= bytes
                    {
                        println!(
                            "{name}: observed down={} up={} (own loopback payload={bytes})",
                            after[0] - before[0],
                            after[1] - before[1]
                        );
                        break;
                    }
                    assert!(Instant::now() < deadline, "{name}: capture insufficient: before={before:?}, after={after:?}, expected at least {bytes}");
                    thread::sleep(Duration::from_millis(50));
                }
            }
            let shared = Arc::clone(&collector.shared);
            drop(collector);
            let context = shared.lock().unwrap();
            assert!(!context.ready);
            assert!(context.error.is_none(), "cleanup: {:?}", context.error);
        }
        fn loopback(tcp: bool, ipv6: bool) -> u64 {
            let address = if ipv6 { "[::1]:0" } else { "127.0.0.1:0" };
            let payload = [0x5a; 1024];
            let mut received = [0; 1024];
            let timeout = Some(Duration::from_secs(5));
            if tcp {
                let listener = TcpListener::bind(address).unwrap();
                let mut client = TcpStream::connect_timeout(
                    &listener.local_addr().unwrap(),
                    Duration::from_secs(5),
                )
                .unwrap();
                let (mut server, _) = listener.accept().unwrap();
                for stream in [&client, &server] {
                    stream.set_read_timeout(timeout).unwrap();
                    stream.set_write_timeout(timeout).unwrap();
                    stream.set_nodelay(true).unwrap();
                }
                for _ in 0..32 {
                    client.write_all(&payload).unwrap();
                    server.read_exact(&mut received).unwrap();
                    assert_eq!(received, payload);
                    server.write_all(&payload).unwrap();
                    client.read_exact(&mut received).unwrap();
                    assert_eq!(received, payload);
                }
            } else {
                let client = UdpSocket::bind(address).unwrap();
                let server = UdpSocket::bind(address).unwrap();
                for socket in [&client, &server] {
                    socket.set_read_timeout(timeout).unwrap();
                    socket.set_write_timeout(timeout).unwrap();
                }
                for _ in 0..32 {
                    assert_eq!(
                        client
                            .send_to(&payload, server.local_addr().unwrap())
                            .unwrap(),
                        payload.len()
                    );
                    assert_eq!(server.recv_from(&mut received).unwrap().0, payload.len());
                    assert_eq!(received, payload);
                    assert_eq!(
                        server
                            .send_to(&payload, client.local_addr().unwrap())
                            .unwrap(),
                        payload.len()
                    );
                    assert_eq!(client.recv_from(&mut received).unwrap().0, payload.len());
                    assert_eq!(received, payload);
                }
            }
            32 * 1024 * 2
        }
        #[test]
        fn drop_waits_for_controller_cleanup() {
            use std::sync::atomic::{AtomicBool, Ordering};
            let mut collector = collector();
            let stop = collector.stop.clone();
            let cleaned = Arc::new(AtomicBool::new(false));
            let worker_cleaned = Arc::clone(&cleaned);
            collector.worker = Some(thread::spawn(move || {
                assert!(!stop.wait_until(Instant::now() + Duration::from_secs(60)));
                thread::sleep(Duration::from_millis(30));
                worker_cleaned.store(true, Ordering::Release);
            }));
            drop(collector);
            assert!(cleaned.load(Ordering::Acquire));
        }
        fn collector() -> NetworkCollector {
            NetworkCollector {
                shared: Arc::new(Mutex::new(Context {
                    bank: CounterBank::new(LIMIT),
                    handles: HashMap::new(),
                    ready: true,
                    error: None,
                    handle_limit: LIMIT,
                    dropped_packets: 0,
                    dropped_bytes: 0,
                    last_drop: None,
                    unavailable_pids: HashSet::new(),
                })),
                stop: StopToken::default(),
                worker: None,
            }
        }
        fn send(collector: &NetworkCollector, pid: u32, timestamp: u64) {
            let mut payload = [0_u8; 64];
            payload[..4].copy_from_slice(&pid.to_le_bytes());
            payload[4..8].copy_from_slice(&12_u32.to_le_bytes());
            let mut record = EVENT_RECORD::default();
            record.EventHeader.ProviderId = TCP;
            record.EventHeader.EventDescriptor.Version = 2;
            record.EventHeader.EventDescriptor.Opcode = 11;
            record.EventHeader.TimeStamp = i64::try_from(timestamp).unwrap();
            record.UserData = payload.as_mut_ptr().cast();
            record.UserDataLength = 64;
            record.UserContext = Arc::as_ptr(&collector.shared).cast_mut().cast();
            unsafe {
                packet_callback(&raw mut record);
            }
        }
        #[test]
        fn owned_system_logger_does_not_use_the_global_kernel_guid() {
            let mut properties = Properties::new(&[b'N'.into(), 0]);
            assert!(!same_guid(
                unsafe { &(*properties.pointer()).Wnode.Guid },
                &SYSTEM
            ));
        }
        #[test]
        fn unattributable_packets_disqualify_only_the_affected_pid() {
            let collector = collector();
            send(&collector, 0, 1);
            let snapshot = collector.snapshot(&HashSet::new());
            assert!(snapshot.available);
            assert_eq!(snapshot.unavailable_pids, HashSet::from([0]));
            assert!(snapshot.error.is_some());
            assert!(snapshot.counters.is_empty());
        }
        #[test]
        fn a_known_pid_stays_available_after_an_unattributable_packet() {
            let collector = collector();
            let pid = std::process::id();
            let created = creation_token(pid).unwrap();
            let identity = format!("{pid}:{created}");
            send(&collector, 0, 1);
            send(&collector, pid, created + 1);
            let snapshot = collector.snapshot(&HashSet::from([identity.clone()]));
            assert!(snapshot.available);
            assert!(snapshot.unavailable_pids.contains(&0));
            assert!(!snapshot.unavailable_pids.contains(&pid));
            assert_eq!(snapshot.counters[&identity], [12, 0]);
        }
        #[test]
        fn unavailable_pid_capacity_failure_is_global() {
            let collector = collector();
            collector.shared.lock().unwrap().handle_limit = 1;
            send(&collector, 0, 1);
            send(&collector, u32::MAX, 1);
            let snapshot = collector.snapshot(&HashSet::new());
            assert!(!snapshot.available);
            assert_eq!(snapshot.unavailable_pids.len(), 1);
            assert!(snapshot.error.unwrap().contains("capacity"));
        }
        #[test]
        fn pid_gaps_are_retained_only_while_the_caller_retains_that_pid() {
            let collector = collector();
            let retained = HashSet::from(["0:1".to_owned()]);
            send(&collector, 0, 1);
            assert!(collector.snapshot(&retained).unavailable_pids.contains(&0));
            send(&collector, 0, 2);
            let snapshot = collector.snapshot(&retained);
            assert!(snapshot.available);
            assert_eq!(snapshot.unavailable_pids, HashSet::from([0]));
            assert_eq!(collector.shared.lock().unwrap().dropped_packets, 2);
            collector.snapshot(&HashSet::new());
            assert!(collector.shared.lock().unwrap().unavailable_pids.is_empty());
            assert!(collector
                .snapshot(&HashSet::new())
                .unavailable_pids
                .is_empty());
        }
        #[test]
        fn unsupported_schema_is_a_global_failure_not_a_pid_gap() {
            let collector = collector();
            let mut payload = [0; 64];
            let mut record = EVENT_RECORD::default();
            record.EventHeader.ProviderId = UDP;
            record.EventHeader.EventDescriptor.Version = 3;
            record.EventHeader.EventDescriptor.Opcode = 27;
            record.UserData = payload.as_mut_ptr().cast();
            record.UserDataLength = 64;
            record.UserContext = Arc::as_ptr(&collector.shared).cast_mut().cast();
            unsafe {
                packet_callback(&raw mut record);
            }
            let snapshot = collector.snapshot(&HashSet::new());
            assert!(!snapshot.available);
            assert!(snapshot.unavailable_pids.is_empty());
            assert!(snapshot.error.unwrap().contains("schema"));
        }
        #[test]
        fn event_and_realtime_buffer_losses_are_global_failures() {
            for (events, buffers) in [(1, 0), (0, 1)] {
                let collector = collector();
                let mut session = Session {
                    name: vec![0],
                    properties: Properties::new(&[0]),
                    controller: CONTROLTRACE_HANDLE::default(),
                    trace: None,
                    active: false,
                };
                unsafe {
                    (*session.properties.pointer()).EventsLost = events;
                    (*session.properties.pointer()).RealTimeBuffersLost = buffers;
                }
                let error = session.query_error(0).unwrap();
                collector.shared.lock().unwrap().fail(error);
                assert!(!collector.snapshot(&HashSet::new()).available);
            }
        }
        #[test]
        fn process_handle_cache_is_bounded_before_opening_a_new_handle() {
            let collector = collector();
            collector.shared.lock().unwrap().handle_limit = 0;
            let pid = std::process::id();
            send(&collector, pid, creation_token(pid).unwrap() + 1);
            assert!(collector.shared.lock().unwrap().handles.is_empty());
            assert!(!collector.snapshot(&HashSet::new()).available);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminated_handles_are_retained_by_the_real_pid_not_zero() {
        let identities = HashSet::from(["42:100".to_owned()]);
        assert!(retain_process(42, 100, false, &identities));
        assert!(!retain_process(0, 100, false, &identities));
        assert!(!retain_process(42, 200, false, &identities));
        assert!(retain_process(42, 200, true, &identities));
    }
    #[test]
    fn a_pid_size_prefix_is_not_a_complete_network_event() {
        for transport in [Transport::Tcp, Transport::Udp] {
            for opcode in [10, 11, 26, 27] {
                assert!(decode_packet(transport, 2, opcode, &[0; 8]).is_none());
            }
        }
    }
    #[test]
    fn tcp_udp_v2_ipv4_ipv6_prefixes_decode_pid_size_and_direction() {
        for transport in [Transport::Tcp, Transport::Udp] {
            for opcode in [10, 11, 26, 27] {
                let prefix = if opcode < 26 { 20 } else { 44 };
                let mut payload = vec![0; prefix];
                payload[..4].copy_from_slice(&0x0102_0304_u32.to_le_bytes());
                payload[4..8].copy_from_slice(&0x1122_3344_u32.to_le_bytes());
                assert_eq!(
                    decode_packet(transport, 2, opcode, &payload),
                    Some(Packet {
                        pid: 0x0102_0304,
                        bytes: 0x1122_3344,
                        download: matches!(opcode, 11 | 27),
                    })
                );
                assert!(decode_packet(transport, 2, opcode, &payload[..prefix - 1]).is_none());
            }
        }
    }
    #[test]
    fn payload_pid_is_used_instead_of_the_etw_header_pid() {
        let mut payload = vec![0; 64];
        payload[..4].copy_from_slice(&42_u32.to_le_bytes());
        payload[4..8].copy_from_slice(&2048_u32.to_le_bytes());
        assert_eq!(
            decode_packet(Transport::Tcp, 2, 11, &payload),
            Some(Packet {
                pid: 42,
                bytes: 2048,
                download: true
            })
        );
        assert_eq!(
            decode_packet(Transport::Udp, 2, 26, &payload),
            Some(Packet {
                pid: 42,
                bytes: 2048,
                download: false
            })
        );
    }
    #[test]
    fn unsupported_events_are_never_guessed() {
        assert!(decode_packet(Transport::Tcp, 0, 11, &[0; 64]).is_none());
        assert!(decode_packet(Transport::Tcp, 2, 14, &[0; 64]).is_none());
        assert!(decode_packet(Transport::Tcp, 2, 10, &[0; 7]).is_none());
    }
    #[test]
    fn a_reused_pid_cannot_receive_an_older_event() {
        assert!(!belongs_to_process(100, None, 99));
        assert!(!belongs_to_process(100, Some(120), 121));
        assert!(belongs_to_process(100, Some(120), 110));
    }
    #[test]
    fn counters_are_bounded_and_keep_creation_identity() {
        let mut bank = CounterBank::new(2);
        assert!(bank.add(42, 100, true, 10));
        assert!(bank.add(42, 200, false, 20));
        assert_eq!(bank.counters.get("42:100"), Some(&[10, 0]));
        assert_eq!(bank.counters.get("42:200"), Some(&[0, 20]));
        assert!(!bank.add(9, 300, true, 1));
        bank.retain(&std::collections::HashSet::from(["42:200".to_owned()]));
        assert!(bank.add(9, 300, true, 1));
    }
    #[test]
    fn overflow_does_not_wrap_or_partially_accept() {
        let mut bank = CounterBank::new(1);
        assert!(bank.add(1, 100, true, u64::MAX));
        assert!(!bank.add(1, 100, true, 1));
        assert_eq!(bank.counters["1:100"][0], u64::MAX);
    }
}
