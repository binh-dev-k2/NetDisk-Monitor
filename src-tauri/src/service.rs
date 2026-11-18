use crate::{
    collector::{ProcessCollector, SystemCollector},
    health::{WorkerHealth, WorkerKind},
    metrics_core::{StorageSample, SystemReducer},
    runtime::StopToken,
    storage::HistoryRecord,
    MonitorState, OverlaySettings, ProcessSnapshot, SystemSnapshot,
};
use std::{
    sync::{atomic::Ordering, Arc, Mutex, OnceLock},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub(crate) struct RuntimeState {
    pub(crate) host_lease: Mutex<Option<crate::host_lease::HostLease>>,
    pub(crate) health: Arc<WorkerHealth>,
    pub(crate) log_sink: OnceLock<crate::diagnostics::LogSink>,
    pub(crate) log_owner: Mutex<Option<crate::diagnostics::LogOwner>>,
    stop: StopToken,
    storage_stop: StopToken,
    workers: Mutex<Vec<JoinHandle<()>>>,
    storage_worker: Mutex<Option<JoinHandle<()>>>,
    pub(crate) latest: Mutex<Option<StorageSample>>,
    pub(crate) baseline: Mutex<Option<StorageSample>>,
    pub(crate) storage_error: Mutex<Option<String>>,
    pub(crate) history: Mutex<(Vec<HistoryRecord>, Vec<HistoryRecord>)>,
    pub(crate) history_clear_requests: Mutex<Vec<std::sync::mpsc::SyncSender<Result<(), String>>>>,
    pub(crate) shutdown_error: Mutex<Option<String>>,
    pub(crate) shutdown_gate: crate::runtime::ShutdownGate,
}

impl RuntimeState {
    // Only called by the blocking shutdown task, never under a UI-visible lock.
    pub fn stop_diagnostics(&self) {
        let owner = self
            .log_owner
            .lock()
            .ok()
            .and_then(|mut owner| owner.take());
        if let Some(mut owner) = owner {
            let _ = owner.shutdown(Duration::from_millis(500));
        }
    }
    pub fn history(&self, resolution: &str, limit: usize) -> Result<Vec<HistoryRecord>, String> {
        let history = self
            .history
            .lock()
            .map_err(|_| "History cache is unavailable")?;
        let records = if resolution == "minute" {
            &history.0
        } else {
            &history.1
        };
        Ok(records
            .iter()
            .take(limit.min(records.len()))
            .cloned()
            .collect())
    }

    pub fn history_counts(&self) -> (usize, usize) {
        self.history
            .lock()
            .map(|history| (history.0.len(), history.1.len()))
            .unwrap_or((0, 0))
    }

    pub fn clear_history(&self) -> Result<(), String> {
        let running = self
            .storage_worker
            .lock()
            .ok()
            .is_some_and(|worker| worker.is_some());
        if !running {
            return Err("Luồng lưu trữ lịch sử chưa chạy".to_owned());
        }
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
        self.history_clear_requests
            .lock()
            .map_err(|_| "Hàng đợi xóa lịch sử không khả dụng".to_owned())?
            .push(done_tx);
        self.storage_stop.wake();
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "Hết thời gian chờ xóa lịch sử".to_owned())?
    }

    pub fn shutdown(&self) -> Result<(), String> {
        self.stop.stop();
        let workers = self
            .workers
            .lock()
            .map(|mut w| std::mem::take(&mut *w))
            .unwrap_or_default();
        let mut panic = false;
        for worker in workers {
            panic |= worker.join().is_err();
        }
        // Producers finish before the persistence owner takes its final sample.
        self.storage_stop.stop();
        if let Ok(mut worker) = self.storage_worker.lock() {
            if let Some(worker) = worker.take() {
                panic |= worker.join().is_err();
            }
        }
        if panic {
            return Err("A monitoring worker stopped unexpectedly".to_owned());
        }
        self.shutdown_error
            .lock()
            .map_err(|_| "Shutdown result is unavailable")?
            .clone()
            .map_or(Ok(()), Err)
    }
}

// Startup owns the app handle and rolls back all started workers as one lifecycle.
#[allow(
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::items_after_statements
)]
pub(crate) fn start(app: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    #[allow(clippy::items_after_statements)]
    let directory = app.path().app_data_dir()?;
    let runtime = app.state::<RuntimeState>();
    struct StartupGuard<'a> {
        runtime: &'a RuntimeState,
        armed: bool,
    }
    #[allow(clippy::items_after_statements)]
    impl Drop for StartupGuard<'_> {
        fn drop(&mut self) {
            if self.armed {
                let _ = self.runtime.shutdown();
            }
        }
    }
    let mut guard = StartupGuard {
        runtime: &runtime,
        armed: true,
    };
    let stop = runtime.stop.clone();
    let system_app = app.clone();
    let system_stop = stop.clone();
    let system = thread::Builder::new()
        .name("monitor-system".into())
        .spawn(move || {
            let health = system_app
                .state::<RuntimeState>()
                .health
                .start(WorkerKind::System);
            let mut collector = SystemCollector::new();
            let origin = Instant::now();
            let mut reducer = SystemReducer::new(origin);
            let mut deadline = origin;
            let mut last_taskbar_diag: Option<(crate::taskbar::TaskbarPhase, Option<String>)> =
                None;
            while system_stop.wait_until(deadline) {
                let state = system_app.state::<MonitorState>();
                let current_generation = state.reset_generation.load(Ordering::Acquire);
                let collection = collector.collect();
                let now = Instant::now();
                let frame = reducer.observe(
                    collection,
                    now,
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    current_generation,
                );
                let runtime = system_app.state::<RuntimeState>();
                let sample = frame.storage;
                if let Ok(mut baseline) = runtime.baseline.lock() {
                    if baseline.is_none() {
                        *baseline = Some(sample);
                    }
                }
                if let Ok(mut latest) = runtime.latest.lock() {
                    *latest = Some(sample);
                }
                let (active_process_count, process_network_status_error) =
                    crate::read_process_summary(&state);
                let snapshot = SystemSnapshot {
                    metrics: frame.metrics,
                    interfaces: frame.interfaces,
                    total_memory_bytes: frame.total_memory_bytes,
                    used_memory_bytes: frame.used_memory_bytes,
                    disk_available: frame.disk_available,
                    cpu_usage_percent: frame.cpu_usage_percent,
                    volumes: frame.volumes,
                    sequence: frame.sequence,
                    storage_error: runtime.storage_error.lock().ok().and_then(|e| e.clone()),
                    taskbar_error: None,
                    taskbar_status: crate::taskbar::TaskbarStatus::default(),
                    active_process_count,
                    process_network_status_error,
                };
                let mut snapshot = snapshot;
                let settings = state
                    .overlay_settings
                    .lock()
                    .ok()
                    .map(|s| s.clone())
                    .unwrap_or_default();
                #[cfg(windows)]
                if let Ok(mut mode) = state.taskbar_mode.try_lock() {
                    state
                        .taskbar_active
                        .store(mode.is_enabled(), Ordering::Release);
                    if mode.is_enabled() {
                        mode.update(&taskbar_text(&snapshot, &settings));
                    }
                    let status = mode.status();
                    snapshot.taskbar_error.clone_from(&status.reason);
                    snapshot.taskbar_status = status;
                }
                #[cfg(target_os = "macos")]
                {
                    let active = state.taskbar_active.load(Ordering::Acquire);
                    if active {
                        crate::update_menu_title(
                            &system_app,
                            taskbar_text(&snapshot, &settings).replace(['\n', '\t'], "  "),
                        );
                    }
                    snapshot.taskbar_status = crate::taskbar::TaskbarStatus {
                        phase: if active {
                            crate::taskbar::TaskbarPhase::Attached
                        } else {
                            crate::taskbar::TaskbarPhase::Disabled
                        },
                        desired: active,
                        attached: active,
                        reason: None,
                    };
                }
                #[cfg(not(any(windows, target_os = "macos")))]
                {
                    let active = state.taskbar_active.load(Ordering::Acquire);
                    snapshot.taskbar_status = crate::taskbar::TaskbarStatus {
                        phase: if active {
                            crate::taskbar::TaskbarPhase::Unsupported
                        } else {
                            crate::taskbar::TaskbarPhase::Disabled
                        },
                        desired: active,
                        attached: false,
                        reason: active
                            .then(|| "Native taskbar mode is only available on Windows".to_owned()),
                    };
                    snapshot
                        .taskbar_error
                        .clone_from(&snapshot.taskbar_status.reason);
                }
                let current_diag = (
                    snapshot.taskbar_status.phase,
                    snapshot.taskbar_status.reason.clone(),
                );
                if last_taskbar_diag.as_ref() != Some(&current_diag) {
                    crate::diagnostics::record(
                        "taskbar",
                        &format!(
                            "phase={:?} attached={} reason={}",
                            snapshot.taskbar_status.phase,
                            snapshot.taskbar_status.attached,
                            snapshot.taskbar_status.reason.as_deref().unwrap_or("none")
                        ),
                    );
                    last_taskbar_diag = Some(current_diag);
                }
                let dashboard_visible = state.dashboard_visible.load(Ordering::Acquire);
                let overlay_visible = state.overlay_visible.load(Ordering::Acquire);
                if dashboard_visible || overlay_visible {
                    if let Ok(mut current) = state.snapshot.lock() {
                        *current = snapshot.clone();
                    }
                    if dashboard_visible {
                        let _ = system_app.emit_to("main", "metrics://snapshot", &snapshot);
                    }
                    if overlay_visible {
                        let _ =
                            system_app.emit_to("monitor-overlay", "metrics://snapshot", &snapshot);
                    }
                } else if let Ok(mut current) = state.snapshot.lock() {
                    *current = snapshot;
                }
                health.beat();
                deadline += Duration::from_secs(1);
                if deadline <= now {
                    deadline = now + Duration::from_secs(1);
                }
            }
        })?;
    runtime
        .workers
        .lock()
        .map_err(|_| "Worker state is unavailable")?
        .push(system);

    let process_app = app.clone();
    let process_stop = stop.clone();
    let process = thread::Builder::new()
        .name("monitor-processes".into())
        .spawn(move || {
            let health = process_app
                .state::<RuntimeState>()
                .health
                .start(WorkerKind::Processes);
            let mut collector = ProcessCollector::new();
            let mut sequence = 0_u64;
            let mut generation = 0;
            let mut deadline = Instant::now();
            while process_stop.wait_until(deadline) {
                let state = process_app.state::<MonitorState>();
                let current_generation = state.reset_generation.load(Ordering::Acquire);
                if current_generation != generation {
                    collector.reset_session_totals();
                    generation = current_generation;
                }
                let processes = collector.collect();
                sequence = sequence.saturating_add(1);
                let snapshot = ProcessSnapshot::from_observation(
                    sequence,
                    processes,
                    collector.network_error(),
                );
                let process_warn = snapshot.process_network_error.as_deref().filter(|msg| {
                    !msg.contains("SeSystemProfilePrivilege") && !msg.starts_with("ETW gaps:")
                });
                health.warn(process_warn);
                let should_emit = state.dashboard_visible.load(Ordering::Acquire)
                    && state.process_view_active.load(Ordering::Acquire);
                if should_emit {
                    if let Ok(mut current) = state.processes.lock() {
                        *current = snapshot.clone();
                    }
                    let _ = process_app.emit_to("main", "processes://snapshot", snapshot);
                } else if let Ok(mut current) = state.processes.lock() {
                    *current = snapshot;
                }
                health.beat();
                deadline += Duration::from_secs(5);
                if deadline <= Instant::now() {
                    deadline = Instant::now() + Duration::from_secs(5);
                }
            }
        })?;
    runtime
        .workers
        .lock()
        .map_err(|_| "Worker state is unavailable")?
        .push(process);

    let storage_app = app.clone();
    let storage_stop = runtime.storage_stop.clone();
    let storage = thread::Builder::new()
        .name("monitor-storage".into())
        .spawn(move || {
            crate::persistence::run(storage_app, directory, storage_stop);
        })?;
    *runtime
        .storage_worker
        .lock()
        .map_err(|_| "Storage worker state is unavailable")? = Some(storage);
    guard.armed = false;
    Ok(())
}

// Floating point is used only for compact display formatting, not byte accumulation.
#[allow(clippy::cast_precision_loss)]
fn rate(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B/s", "K/s", "M/s", "G/s", "T/s"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1}{}", UNITS[unit])
}

#[allow(clippy::cast_precision_loss)]
pub(crate) fn taskbar_text(snapshot: &SystemSnapshot, settings: &OverlaySettings) -> String {
    let m = &snapshot.metrics;
    let mut items = Vec::with_capacity(7);
    if settings.show_network_down {
        items.push(format!("D {}", rate(m.network_down_bps)));
    }
    if settings.show_network_up {
        items.push(format!("U {}", rate(m.network_up_bps)));
    }
    if settings.show_disk_read {
        items.push(format!(
            "R {}",
            if snapshot.disk_available {
                rate(m.disk_read_bps)
            } else {
                "N/A".into()
            }
        ));
    }
    if settings.show_disk_write {
        items.push(format!(
            "W {}",
            if snapshot.disk_available {
                rate(m.disk_write_bps)
            } else {
                "N/A".into()
            }
        ));
    }
    if settings.show_ram {
        items.push(format!(
            "RAM {:.1}G",
            snapshot.used_memory_bytes as f64 / 1_073_741_824.0
        ));
    }
    if settings.show_session_total {
        items.push(format!(
            "NET {:.1}G",
            m.session_network_down.saturating_add(m.session_network_up) as f64 / 1_073_741_824.0
        ));
    }
    if settings.show_session_duration {
        items.push(format!(
            "{}h {:02}m",
            m.session_duration_secs / 3600,
            (m.session_duration_secs / 60) % 60
        ));
    }
    if items.is_empty() {
        return String::new();
    }
    let mut rows = [Vec::new(), Vec::new()];
    for (index, item) in items.into_iter().enumerate() {
        rows[index % 2].push(item);
    }
    if rows[1].is_empty() {
        rows[0].join("\t")
    } else {
        format!("{}\n{}", rows[0].join("\t"), rows[1].join("\t"))
    }
}
