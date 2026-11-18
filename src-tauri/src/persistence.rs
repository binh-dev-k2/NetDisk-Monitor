use crate::{
    preferences,
    runtime::{PersistenceCursor, StopToken},
    service::RuntimeState,
    storage::HistoryStore,
    MonitorState,
};
use std::{
    path::PathBuf,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

// The storage thread owns these handles and keeps the serial retry/checkpoint lifecycle together.
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub(crate) fn run(app: AppHandle, directory: PathBuf, stop: StopToken) {
    let runtime = app.state::<RuntimeState>();
    let health = runtime.health.start(crate::health::WorkerKind::Storage);
    let mut store: Option<HistoryStore> = None;
    let mut cursor: Option<PersistenceCursor> = None;
    let mut retry_at = Instant::now();
    let mut retry_delay = Duration::from_secs(1);
    let mut cache_at = Instant::now();
    let mut history_dirty = false;
    let mut preferences_at = Instant::now();
    let mut saved_preferences = None;
    let mut io_error = None;
    let mut preference_error = None;
    let mut clock_warning = None;
    let mut deadline = Instant::now();
    loop {
        let running = stop.wait_until(deadline);
        let now = Instant::now();
        if store.is_none() && (now >= retry_at || !running || has_clear_requests(&runtime)) {
            match HistoryStore::open(&directory) {
                Ok(opened) => {
                    store = Some(opened);
                    cache_at = now;
                    history_dirty = true;
                    io_error = None;
                }
                Err(error) => {
                    io_error = Some(format!("History: {error}"));
                    retry_at = now + Duration::from_secs(30);
                }
            }
        }
        process_clear_requests(
            &runtime,
            store.as_mut(),
            &mut cursor,
            &mut history_dirty,
            &mut io_error,
            &mut clock_warning,
        );
        if let Some(store) = &mut store {
            if cursor.is_none() {
                if let Some(baseline) = runtime.baseline.lock().ok().and_then(|sample| *sample) {
                    cursor = Some(PersistenceCursor::new(
                        baseline.observed(),
                        store.latest_timestamp_secs(),
                    ));
                }
            }
            if now >= retry_at || !running {
                let latest = runtime.latest.lock().ok().and_then(|sample| *sample);
                if let (Some(cursor), Some(latest)) = (&mut cursor, latest) {
                    let sample = latest.observed();
                    if let Some(pending) = cursor.prepare(sample) {
                        match store.record(pending.timestamp, pending.elapsed_millis, pending.bytes)
                        {
                            Ok(()) => {
                                cursor.accept(sample, &pending);
                                history_dirty = true;
                                io_error = None;
                                clock_warning = pending.clock_adjusted.then(|| "Đồng hồ hệ thống bị lùi; thời gian bucket được giữ ở mốc đã lưu, thời lượng đo vẫn theo đồng hồ monotonic.".to_owned());
                                retry_delay = Duration::from_secs(1);
                            }
                            Err(error) => {
                                // No incoming delta was accepted; the cursor preserves its full backlog.
                                io_error = Some(format!("History: {error}"));
                                retry_at = now + retry_delay;
                                retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
                            }
                        }
                    }
                }
            }
            if (history_dirty && now >= cache_at) || !running {
                match (store.read_minutes(10_080), store.read_days(366)) {
                    (Ok(minutes), Ok(days)) => {
                        if let Ok(mut cache) = runtime.history.lock() {
                            *cache = (minutes, days);
                        }
                        history_dirty = false;
                    }
                    (Err(error), _) | (_, Err(error)) => {
                        io_error = Some(format!("History read: {error}"));
                    }
                }
                cache_at = now + Duration::from_secs(60);
            }
            if !running {
                let mut flush_error = None;
                for attempt in 0..3 {
                    match store.flush() {
                        Ok(()) => {
                            flush_error = None;
                            break;
                        }
                        Err(error) => {
                            flush_error = Some(format!("Không thể lưu lịch sử khi thoát: {error}"));
                        }
                    }
                    std::thread::sleep(Duration::from_millis(100 * (attempt + 1)));
                }
                if flush_error.is_some() {
                    io_error = flush_error;
                }
            }
        }
        let state = app.state::<MonitorState>();
        let settings = state
            .overlay_settings
            .lock()
            .ok()
            .map(|s| s.clone())
            .unwrap_or_default();
        let current_prefs = preferences::StoredPreferences {
            flags: settings.flags(state.overlay_pinned.load(Ordering::Acquire)),
            opacity: settings.opacity,
            overlay_visible: state.overlay_visible.load(Ordering::Acquire),
            taskbar_enabled: state.taskbar_active.load(Ordering::Acquire),
            theme: state.theme.lock().ok().map(|t| *t).unwrap_or_default(),
            overlay_position: state.overlay_position.lock().ok().and_then(|p| *p),
        };
        if saved_preferences != Some(current_prefs) && (now >= preferences_at || !running) {
            match preferences::save_stored(&directory, &current_prefs) {
                Ok(()) => {
                    saved_preferences = Some(current_prefs);
                    preference_error = None;
                }
                Err(error) => {
                    preference_error = Some(format!("Preferences: {error}"));
                    preferences_at = now + Duration::from_secs(30);
                }
            }
        }
        let errors = [
            io_error.as_deref(),
            preference_error.as_deref(),
            clock_warning.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" | ");
        health.warn((!errors.is_empty()).then_some(errors.as_str()));
        health.beat();
        if let Ok(mut status) = runtime.storage_error.lock() {
            *status = (!errors.is_empty()).then_some(errors);
        }
        if !running {
            let final_error = io_error.or(preference_error);
            if let Ok(mut result) = runtime.shutdown_error.lock() {
                *result = final_error;
            }
            break;
        }
        deadline = if has_clear_requests(&runtime) {
            now
        } else {
            now + Duration::from_secs(1)
        };
    }
}

fn has_clear_requests(runtime: &RuntimeState) -> bool {
    runtime
        .history_clear_requests
        .lock()
        .is_ok_and(|queue| !queue.is_empty())
}

fn reset_history_runtime(
    runtime: &RuntimeState,
    cursor: &mut Option<PersistenceCursor>,
    history_dirty: &mut bool,
    io_error: &mut Option<String>,
    clock_warning: &mut Option<String>,
) {
    let anchor = runtime
        .latest
        .lock()
        .ok()
        .and_then(|sample| *sample)
        .or_else(|| runtime.baseline.lock().ok().and_then(|sample| *sample));
    *cursor = anchor.map(|sample| PersistenceCursor::new(sample.observed(), None));
    if let Ok(mut cache) = runtime.history.lock() {
        *cache = (Vec::new(), Vec::new());
    }
    *history_dirty = false;
    *io_error = None;
    *clock_warning = None;
    crate::diagnostics::record("storage", "history cleared");
}

fn process_clear_requests(
    runtime: &RuntimeState,
    store: Option<&mut HistoryStore>,
    cursor: &mut Option<PersistenceCursor>,
    history_dirty: &mut bool,
    io_error: &mut Option<String>,
    clock_warning: &mut Option<String>,
) {
    let waiters = runtime
        .history_clear_requests
        .lock()
        .map(|mut queue| std::mem::take(&mut *queue))
        .unwrap_or_default();
    if waiters.is_empty() {
        return;
    }
    let outcome = match store {
        Some(store) => store
            .clear()
            .map(|()| {
                reset_history_runtime(runtime, cursor, history_dirty, io_error, clock_warning);
            })
            .map_err(|error| format!("Không thể xóa lịch sử: {error}")),
        None => Err(io_error
            .clone()
            .unwrap_or_else(|| "Kho lịch sử hiện không khả dụng".to_owned())),
    };
    for waiter in waiters {
        let _ = waiter.send(outcome.clone());
    }
}
