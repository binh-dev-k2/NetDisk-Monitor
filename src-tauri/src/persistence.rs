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
    let mut store: Option<HistoryStore> = None;
    let mut cursor: Option<PersistenceCursor> = None;
    let mut retry_at = Instant::now();
    let mut retry_delay = Duration::from_secs(1);
    let mut cache_at = Instant::now();
    let mut preferences_at = Instant::now();
    let mut saved_preferences = None;
    let mut io_error = None;
    let mut preference_error = None;
    let mut clock_warning = None;
    let mut deadline = Instant::now();
    loop {
        let running = stop.wait_until(deadline);
        let now = Instant::now();
        if store.is_none() && (now >= retry_at || !running) {
            match HistoryStore::open(&directory) {
                Ok(opened) => {
                    store = Some(opened);
                    cache_at = now;
                    io_error = None;
                }
                Err(error) => {
                    io_error = Some(format!("History: {error}"));
                    retry_at = now + Duration::from_secs(30);
                }
            }
        }
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
            if now >= cache_at || !running {
                match (store.read_minutes(10_080), store.read_days(366)) {
                    (Ok(minutes), Ok(days)) => {
                        if let Ok(mut cache) = runtime.history.lock() {
                            *cache = (minutes, days);
                        }
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
        let encoded = preferences::encode(
            settings.flags(state.overlay_pinned.load(Ordering::Acquire)),
            settings.opacity,
        );
        if saved_preferences != Some(encoded) && (now >= preferences_at || !running) {
            match preferences::save(&directory, &encoded) {
                Ok(()) => {
                    saved_preferences = Some(encoded);
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
        deadline = now + Duration::from_secs(1);
    }
}
