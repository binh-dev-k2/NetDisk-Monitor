use crate::{
    collector::{ProcessCollector, SystemCollector},
    metrics::{CounterSample, MetricsEngine},
    runtime::{ObservedSample, StopToken},
    storage::HistoryRecord,
    MonitorState, OverlaySettings, ProcessSnapshot, SystemSnapshot,
};
use std::{
    sync::{atomic::Ordering, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Copy)]
pub(crate) struct StorageSample {
    counters: CounterSample,
    timestamp: u64,
    elapsed_millis: u64,
}
impl StorageSample {
    pub(crate) fn observed(self) -> ObservedSample {
        ObservedSample {
            timestamp: self.timestamp,
            elapsed_millis: self.elapsed_millis,
            bytes: [
                self.counters.network_down,
                self.counters.network_up,
                self.counters.disk_read,
                self.counters.disk_write,
            ],
        }
    }
}

#[derive(Default)]
pub(crate) struct RuntimeState {
    stop: StopToken,
    storage_stop: StopToken,
    workers: Mutex<Vec<JoinHandle<()>>>,
    storage_worker: Mutex<Option<JoinHandle<()>>>,
    pub(crate) latest: Mutex<Option<StorageSample>>,
    pub(crate) baseline: Mutex<Option<StorageSample>>,
    pub(crate) storage_error: Mutex<Option<String>>,
    pub(crate) history: Mutex<(Vec<HistoryRecord>, Vec<HistoryRecord>)>,
    pub(crate) shutdown_error: Mutex<Option<String>>,
    pub(crate) shutdown_gate: crate::runtime::ShutdownGate,
}

impl RuntimeState {
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
            let mut collector = SystemCollector::new();
            let mut engine = MetricsEngine::default();
            let origin = Instant::now();
            let mut previous = origin;
            let mut deadline = origin;
            let mut sequence = 0_u64;
            let mut generation = 0;
            while system_stop.wait_until(deadline) {
                let state = system_app.state::<MonitorState>();
                let current_generation = state.reset_generation.load(Ordering::Acquire);
                if current_generation != generation {
                    engine.reset_session_totals();
                    generation = current_generation;
                }
                let collection = collector.collect();
                let now = Instant::now();
                let metrics = engine.update(
                    collection.counters,
                    now.duration_since(previous).as_secs_f64(),
                );
                previous = now;
                sequence = sequence.saturating_add(1);
                let runtime = system_app.state::<RuntimeState>();
                let sample = StorageSample {
                    counters: collection.counters,
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    elapsed_millis: u64::try_from(origin.elapsed().as_millis()).unwrap_or(u64::MAX),
                };
                if let Ok(mut baseline) = runtime.baseline.lock() {
                    if baseline.is_none() {
                        *baseline = Some(sample);
                    }
                }
                if let Ok(mut latest) = runtime.latest.lock() {
                    *latest = Some(sample);
                }
                let snapshot = SystemSnapshot {
                    metrics,
                    interfaces: collection.interfaces,
                    total_memory_bytes: collection.total_memory_bytes,
                    used_memory_bytes: collection.used_memory_bytes,
                    disk_available: collection.disk_available,
                    sequence,
                    storage_error: runtime.storage_error.lock().ok().and_then(|e| e.clone()),
                    taskbar_error: None,
                };
                #[cfg(windows)]
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
                    snapshot.taskbar_error = mode.last_error();
                }
                #[cfg(target_os = "macos")]
                if state.taskbar_active.load(Ordering::Acquire) {
                    crate::update_menu_title(
                        &system_app,
                        taskbar_text(&snapshot, &settings).replace(['\n', '\t'], "  "),
                    );
                }
                if let Ok(mut current) = state.snapshot.lock() {
                    *current = snapshot.clone();
                }
                let _ = system_app.emit("metrics://snapshot", snapshot);
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
                let snapshot = ProcessSnapshot {
                    process_sequence: sequence,
                    processes,
                    process_network_error: collector.network_error(),
                };
                if let Ok(mut current) = state.processes.lock() {
                    *current = snapshot.clone();
                }
                let _ = process_app.emit_to("main", "processes://snapshot", snapshot);
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
