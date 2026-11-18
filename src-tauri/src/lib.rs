#[cfg(any(feature = "desktop", test))]
mod app_icons;
#[cfg(feature = "desktop")]
mod autostart;
#[cfg(feature = "desktop")]
mod collector;
pub mod diagnostics;
pub mod disk_cleaner;
pub mod export;
pub mod health;
pub mod host_lease;
pub mod macos_disk;
pub mod metrics;
pub mod metrics_core;
#[cfg(feature = "desktop")]
mod overlay;
#[cfg(feature = "desktop")]
mod persistence;
pub mod preferences;
#[cfg(feature = "desktop")]
mod process_control;
pub mod process_network;
pub mod runtime;
#[cfg(feature = "desktop")]
mod service;
pub mod space_analyzer;
pub mod storage;
pub mod storage_info;
#[cfg(feature = "desktop")]
mod taskbar;
#[cfg(feature = "desktop")]
use collector::ProcessMetric;
#[cfg(feature = "desktop")]
use metrics::MetricsSnapshot;
#[cfg(feature = "desktop")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "desktop")]
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
#[cfg(all(feature = "desktop", windows))]
use taskbar::TaskbarMode;
#[cfg(feature = "desktop")]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, LogicalSize, Manager, State, WindowEvent,
};
#[cfg(feature = "desktop")]
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemSnapshot {
    #[serde(flatten)]
    metrics: MetricsSnapshot,
    interfaces: Vec<String>,
    total_memory_bytes: u64,
    used_memory_bytes: u64,
    disk_available: bool,
    cpu_usage_percent: f32,
    volumes: Vec<metrics_core::VolumeUsage>,
    sequence: u64,
    storage_error: Option<String>,
    taskbar_error: Option<String>,
    taskbar_status: taskbar::TaskbarStatus,
    active_process_count: Option<usize>,
    process_network_status_error: Option<String>,
}
#[cfg(feature = "desktop")]
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardSnapshot {
    #[serde(flatten)]
    system: SystemSnapshot,
    #[serde(flatten)]
    process: ProcessSnapshot,
}
#[cfg(feature = "desktop")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeHealthSnapshot {
    process_id: u32,
    ignored_exit_requests: u64,
    workers: Vec<health::WorkerSnapshot>,
    logging: Option<diagnostics::LogStatus>,
}
#[cfg(feature = "desktop")]
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSnapshot {
    process_sequence: u64,
    processes: Vec<ProcessMetric>,
    process_network_error: Option<String>,
    #[serde(skip)]
    active_process_count: usize,
}
#[cfg(feature = "desktop")]
impl ProcessSnapshot {
    fn from_observation(
        process_sequence: u64,
        processes: Vec<ProcessMetric>,
        process_network_error: Option<String>,
    ) -> Self {
        let active_process_count = processes.iter().filter(|process| process.active).count();
        Self {
            process_sequence,
            processes,
            process_network_error,
            active_process_count,
        }
    }

    fn summary(&self) -> (Option<usize>, Option<String>) {
        (
            (self.process_sequence > 0).then_some(self.active_process_count),
            self.process_network_error.clone(),
        )
    }
}
#[cfg(feature = "desktop")]
struct MonitorState {
    snapshot: Mutex<SystemSnapshot>,
    processes: Mutex<ProcessSnapshot>,
    reset_generation: AtomicU64,
    overlay_settings: Mutex<OverlaySettings>,
    overlay_position: Mutex<Option<(i32, i32)>>,
    theme: Mutex<preferences::ThemeMode>,
    icons: Mutex<app_icons::IconCache>,
    #[cfg(windows)]
    taskbar_mode: Mutex<TaskbarMode>,
    taskbar_active: AtomicBool,
    overlay_visible: AtomicBool,
    dashboard_visible: AtomicBool,
    process_view_active: AtomicBool,
    overlay_pinned: AtomicBool,
}

#[cfg(all(test, feature = "desktop"))]
mod scoped_snapshot_tests {
    use super::{
        read_process_snapshot, read_process_summary, read_system_snapshot, DashboardSnapshot,
        MonitorState, ProcessMetric, ProcessSnapshot,
    };

    fn process(active: bool) -> ProcessMetric {
        ProcessMetric {
            pid: "100".into(),
            identity: "100:1".into(),
            start_time: 1,
            active,
            name: "fixture".into(),
            memory_bytes: 0,
            disk_read_bps: 0,
            disk_write_bps: 0,
            session_disk_read: 0,
            session_disk_write: 0,
            uptime_secs: 0,
            network_connections: 0,
            network_available: false,
            network_down_bps: None,
            network_up_bps: None,
            session_network_down: None,
            session_network_up: None,
        }
    }

    #[test]
    fn summary_distinguishes_no_observation_from_an_observed_empty_list() {
        assert_eq!(ProcessSnapshot::default().summary().0, None);
        assert_eq!(
            ProcessSnapshot::from_observation(1, vec![], None)
                .summary()
                .0,
            Some(0)
        );
        let snapshot = ProcessSnapshot::from_observation(
            2,
            vec![process(true), process(false)],
            Some("partial".into()),
        );
        assert_eq!(snapshot.summary(), (Some(1), Some("partial".into())));
        let json = serde_json::to_value(snapshot).unwrap();
        assert!(json.get("activeProcessCount").is_none());
        assert_eq!(json["processSequence"], 2);
        assert_eq!(json["processes"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn system_reader_does_not_depend_on_process_state_or_serialize_rows() {
        let state = MonitorState::default();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = state.processes.lock().unwrap();
            panic!("poison process cache");
        }));
        let json = serde_json::to_value(read_system_snapshot(&state).unwrap()).unwrap();
        assert!(json.get("processes").is_none());
        assert!(json.get("processSequence").is_none());
        assert!(json.get("processNetworkError").is_none());
        assert!(json.get("activeProcessCount").is_some());
        assert_eq!(json["activeProcessCount"], serde_json::Value::Null);
        assert!(json.get("processNetworkStatusError").is_some());
        assert!(read_process_snapshot(&state).is_err());
        assert_eq!(
            read_process_summary(&state),
            (None, Some("Process state is unavailable".to_owned()))
        );
    }

    #[test]
    fn process_reader_does_not_depend_on_system_state_or_serialize_system_fields() {
        let state = MonitorState::default();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = state.snapshot.lock().unwrap();
            panic!("poison system cache");
        }));
        let json = serde_json::to_value(read_process_snapshot(&state).unwrap()).unwrap();
        assert!(json.get("interfaces").is_none());
        assert!(json.get("networkDownBps").is_none());
        assert!(json.get("activeProcessCount").is_none());
        assert!(json.get("processes").is_some());
        assert!(read_system_snapshot(&state).is_err());
    }

    #[test]
    fn legacy_flattened_payload_has_unique_summary_and_process_diagnostics() {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SummaryContract {
            active_process_count: Option<usize>,
            process_network_status_error: Option<String>,
            process_network_error: Option<String>,
        }
        let json = serde_json::to_string(&DashboardSnapshot::default()).unwrap();
        let summary: SummaryContract = serde_json::from_str(&json).unwrap();
        assert_eq!(summary.active_process_count, None);
        assert_eq!(summary.process_network_status_error, None);
        assert_eq!(summary.process_network_error, None);
    }

    #[test]
    fn summary_reader_never_waits_for_a_busy_process_rows_cache() {
        let state = std::sync::Arc::new(MonitorState::default());
        *state.processes.lock().unwrap() =
            ProcessSnapshot::from_observation(1, vec![process(true)], None);
        let held = state.processes.lock().unwrap();
        let reader = state.clone();
        let (send, receive) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            send.send(read_process_summary(&reader)).unwrap();
        });
        let result = receive.recv_timeout(std::time::Duration::from_secs(2));
        drop(held);
        worker.join().unwrap();
        assert_eq!(result.unwrap(), (None, None));
        assert_eq!(read_process_summary(&state), (Some(1), None));
    }
}

#[cfg(feature = "desktop")]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
// Display toggles are independent persisted checkbox settings, not lifecycle states.
#[allow(clippy::struct_excessive_bools)]
struct OverlaySettings {
    show_network_down: bool,
    show_network_up: bool,
    show_disk_read: bool,
    show_disk_write: bool,
    show_session_total: bool,
    show_session_duration: bool,
    show_ram: bool,
    opacity: u8,
}

#[cfg(feature = "desktop")]
impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            show_network_down: true,
            show_network_up: true,
            show_disk_read: false,
            show_disk_write: false,
            show_session_total: false,
            show_session_duration: false,
            show_ram: true,
            opacity: 72,
        }
    }
}

#[cfg(feature = "desktop")]
impl OverlaySettings {
    fn flags(&self, pinned: bool) -> u8 {
        let values = [
            self.show_network_down,
            self.show_network_up,
            self.show_disk_read,
            self.show_disk_write,
            self.show_session_total,
            self.show_session_duration,
            self.show_ram,
            pinned,
        ];
        values.iter().enumerate().fold(0, |flags, (index, value)| {
            flags | (u8::from(*value) << index)
        })
    }
    fn from_flags(flags: u8, opacity: u8) -> Self {
        Self {
            show_network_down: flags & 1 != 0,
            show_network_up: flags & 2 != 0,
            show_disk_read: flags & 4 != 0,
            show_disk_write: flags & 8 != 0,
            show_session_total: flags & 16 != 0,
            show_session_duration: flags & 32 != 0,
            show_ram: flags & 64 != 0,
            opacity,
        }
        .normalized()
    }
    fn normalized(mut self) -> Self {
        self.opacity = self.opacity.clamp(35, 92);
        self
    }
}

#[cfg(feature = "desktop")]
struct TrayState {
    _icon: TrayIcon,
    discard_quit: MenuItem<tauri::Wry>,
    #[cfg(target_os = "macos")]
    title_pending: AtomicBool,
}

#[cfg(all(feature = "desktop", target_os = "macos"))]
fn update_menu_title(app: &AppHandle, title: String) {
    let tray = app.state::<TrayState>();
    if tray.title_pending.swap(true, Ordering::AcqRel) {
        return;
    }
    let callback_app = app.clone();
    if app
        .run_on_main_thread(move || {
            if let Some(tray) = callback_app.tray_by_id("monitor-tray") {
                let _ = tray.set_title(Some(title));
            }
            callback_app
                .state::<TrayState>()
                .title_pending
                .store(false, Ordering::Release);
        })
        .is_err()
    {
        tray.title_pending.store(false, Ordering::Release);
    }
}

#[cfg(feature = "desktop")]
#[cfg(test)]
mod overlay_window_tests {
    use super::overlay_window_size;

    #[test]
    fn overlay_window_size_clamps_to_the_compact_two_column_grid() {
        assert_eq!(overlay_window_size(999.0, 999.0), (336.0, 164.0));
    }
}
#[cfg(feature = "desktop")]
impl Default for MonitorState {
    fn default() -> Self {
        Self {
            snapshot: Mutex::new(SystemSnapshot::default()),
            processes: Mutex::new(ProcessSnapshot::default()),
            reset_generation: AtomicU64::new(0),
            overlay_settings: Mutex::new(OverlaySettings::default()),
            overlay_position: Mutex::new(None),
            theme: Mutex::new(preferences::ThemeMode::System),
            icons: Mutex::new(app_icons::IconCache::default()),
            #[cfg(windows)]
            taskbar_mode: Mutex::new(TaskbarMode::default()),
            taskbar_active: AtomicBool::new(false),
            overlay_visible: AtomicBool::new(true),
            dashboard_visible: AtomicBool::new(false),
            process_view_active: AtomicBool::new(false),
            overlay_pinned: AtomicBool::new(true),
        }
    }
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn get_snapshot(state: State<'_, MonitorState>) -> Result<DashboardSnapshot, String> {
    Ok(DashboardSnapshot {
        system: read_system_snapshot(&state)?,
        process: read_process_snapshot(&state)?,
    })
}
#[cfg(feature = "desktop")]
fn read_system_snapshot(state: &MonitorState) -> Result<SystemSnapshot, String> {
    let system = state
        .snapshot
        .lock()
        .map_err(|_| "Metric state is unavailable".to_owned())?
        .clone();
    Ok(system)
}
#[cfg(feature = "desktop")]
fn read_process_snapshot(state: &MonitorState) -> Result<ProcessSnapshot, String> {
    let process = state
        .processes
        .lock()
        .map_err(|_| "Process state is unavailable".to_owned())?
        .clone();
    Ok(process)
}
#[cfg(feature = "desktop")]
fn read_process_summary(state: &MonitorState) -> (Option<usize>, Option<String>) {
    match state.processes.try_lock() {
        Ok(processes) => processes.summary(),
        Err(std::sync::TryLockError::WouldBlock) => (None, None),
        Err(std::sync::TryLockError::Poisoned(_)) => {
            (None, Some("Process state is unavailable".to_owned()))
        }
    }
}
#[cfg(feature = "desktop")]
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn get_system_snapshot(state: State<'_, MonitorState>) -> Result<SystemSnapshot, String> {
    read_system_snapshot(&state)
}
#[cfg(feature = "desktop")]
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn get_process_snapshot(state: State<'_, MonitorState>) -> Result<ProcessSnapshot, String> {
    read_process_snapshot(&state)
}
#[cfg(feature = "desktop")]
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn get_runtime_health(state: State<'_, service::RuntimeState>) -> RuntimeHealthSnapshot {
    RuntimeHealthSnapshot {
        process_id: std::process::id(),
        ignored_exit_requests: state.shutdown_gate.ignored_requests(),
        workers: state.health.snapshot(),
        logging: state.log_sink.get().map(diagnostics::LogSink::status),
    }
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn reset_session_totals(state: State<'_, MonitorState>) {
    state.reset_generation.fetch_add(1, Ordering::AcqRel);
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn get_overlay_settings(state: State<'_, MonitorState>) -> Result<OverlaySettings, String> {
    state
        .overlay_settings
        .lock()
        .map(|settings| settings.clone())
        .map_err(|_| "Overlay settings are unavailable".to_owned())
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn update_overlay_settings(
    settings: OverlaySettings,
    state: State<'_, MonitorState>,
    app: AppHandle,
) -> Result<OverlaySettings, String> {
    let settings = settings.normalized();
    state
        .overlay_settings
        .lock()
        .map(|mut current| *current = settings.clone())
        .map_err(|_| "Overlay settings are unavailable".to_owned())?;
    app.emit("overlay://settings", settings.clone())
        .map_err(|error| error.to_string())?;
    Ok(settings)
}
#[cfg(feature = "desktop")]
fn overlay_window_size(width: f64, height: f64) -> (f64, f64) {
    let width = if width <= 166.0 { 166.0 } else { 336.0 };
    (width, height.clamp(44.0, 164.0))
}
#[cfg(feature = "desktop")]
fn with_overlay_window(
    app: &AppHandle,
    operation: impl FnOnce(&tauri::WebviewWindow) -> tauri::Result<()>,
) -> Result<(), String> {
    let window = app
        .get_webview_window("monitor-overlay")
        .ok_or_else(|| "Overlay window is unavailable".to_owned())?;
    operation(&window).map_err(|error| error.to_string())
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn resize_overlay(
    width: f64,
    height: f64,
    state: State<'_, MonitorState>,
    app: AppHandle,
) -> Result<(), String> {
    if !width.is_finite() || !height.is_finite() {
        return Err("Invalid overlay dimensions".to_owned());
    }
    if !state.overlay_visible.load(Ordering::Acquire)
        && app.get_webview_window("monitor-overlay").is_none()
    {
        return Ok(());
    }
    let (width, height) = overlay_window_size(width, height);
    let pinned = state.overlay_pinned.load(Ordering::Acquire);
    with_overlay_window(&app, |window| {
        let _ = window.set_always_on_top(pinned);
        window.set_size(LogicalSize::new(width, height))
    })
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn begin_overlay_drag(app: AppHandle) -> Result<(), String> {
    with_overlay_window(&app, tauri::WebviewWindow::start_dragging)
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn set_overlay_pinned(
    pinned: bool,
    state: State<'_, MonitorState>,
    app: AppHandle,
) -> Result<(), String> {
    state.overlay_pinned.store(pinned, Ordering::Release);
    if let Some(window) = app.get_webview_window("monitor-overlay") {
        window
            .set_always_on_top(pinned)
            .map_err(|error| error.to_string())?;
    }
    app.emit("overlay://pinned", pinned)
        .map_err(|e| e.to_string())
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn get_overlay_pinned(state: State<'_, MonitorState>) -> bool {
    state.overlay_pinned.load(Ordering::Acquire)
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn get_overlay_visible(state: State<'_, MonitorState>) -> bool {
    state.overlay_visible.load(Ordering::Acquire)
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn set_overlay_visible(
    visible: bool,
    state: State<'_, MonitorState>,
    app: AppHandle,
) -> Result<bool, String> {
    let pinned = state.overlay_pinned.load(Ordering::Acquire);
    if visible {
        let desired_pos = state.overlay_position.lock().ok().and_then(|pos| *pos);
        let window = overlay::ensure_overlay_window(&app, false, pinned, desired_pos)?;
        let _ = window.set_always_on_top(pinned);
        window.show().map_err(|error| error.to_string())?;
    } else if let Some(window) = app.get_webview_window("monitor-overlay") {
        window.hide().map_err(|error| error.to_string())?;
    }
    state.overlay_visible.store(visible, Ordering::Release);
    let _ = app.emit("overlay://visibility", visible);
    Ok(visible)
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn reset_overlay_position(state: State<'_, MonitorState>, app: AppHandle) -> Result<(), String> {
    *state
        .overlay_position
        .lock()
        .map_err(|_| "Overlay position is unavailable".to_owned())? = None;
    if let Some(window) = app.get_webview_window("monitor-overlay") {
        let (x, y) = overlay::default_overlay_position(&app);
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn set_process_view_active(active: bool, state: State<'_, MonitorState>) {
    state.process_view_active.store(active, Ordering::Release);
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn get_theme(state: State<'_, MonitorState>) -> Result<preferences::ThemeMode, String> {
    state
        .theme
        .lock()
        .map(|theme| *theme)
        .map_err(|_| "Theme state is unavailable".to_owned())
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn set_theme(
    theme: preferences::ThemeMode,
    state: State<'_, MonitorState>,
    app: AppHandle,
) -> Result<preferences::ThemeMode, String> {
    *state
        .theme
        .lock()
        .map_err(|_| "Theme state is unavailable".to_owned())? = theme;
    app.emit("app://theme", theme)
        .map_err(|error| error.to_string())?;
    Ok(theme)
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn get_taskbar_mode(state: State<'_, MonitorState>) -> bool {
    state.taskbar_active.load(Ordering::Acquire)
}
#[cfg(all(feature = "desktop", windows))]
fn apply_windows_taskbar_mode(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    let state = app.state::<MonitorState>();
    let text = if enabled {
        let snapshot = state
            .snapshot
            .lock()
            .map_err(|_| "Snapshot is unavailable")?
            .clone();
        let settings = state
            .overlay_settings
            .lock()
            .map_err(|_| "Settings are unavailable")?
            .clone();
        Some(service::taskbar_text(&snapshot, &settings))
    } else {
        None
    };
    let (active, status) = {
        let mut mode = state
            .taskbar_mode
            .lock()
            .map_err(|_| "Taskbar mode is unavailable".to_owned())?;
        if let Some(text) = text {
            mode.update(&text);
            // Native text measurement determines the full size, even for one item.
            mode.enable(0, 1, 1)?;
        } else {
            mode.disable();
        }
        (mode.is_enabled(), mode.status())
    };
    state.taskbar_active.store(active, Ordering::Release);
    let immediate_snapshot = state.snapshot.lock().ok().map(|mut snapshot| {
        snapshot.taskbar_error.clone_from(&status.reason);
        snapshot.taskbar_status = status;
        snapshot.sequence = snapshot.sequence.saturating_add(1);
        snapshot.clone()
    });
    if state.dashboard_visible.load(Ordering::Acquire) {
        if let Some(snapshot) = immediate_snapshot {
            let _ = app.emit_to("main", "metrics://snapshot", &snapshot);
        }
    }
    Ok(active)
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn set_taskbar_mode(
    enabled: bool,
    state: State<'_, MonitorState>,
    app: AppHandle,
) -> Result<bool, String> {
    #[cfg(windows)]
    {
        let _ = state;
        let mode_app = app.clone();
        let active = tauri::async_runtime::spawn_blocking(move || {
            apply_windows_taskbar_mode(&mode_app, enabled)
        })
        .await
        .map_err(|error| error.to_string())??;
        app.emit("taskbar://mode", active)
            .map_err(|error| error.to_string())?;
        Ok(active)
    }
    #[cfg(target_os = "macos")]
    {
        let title = if enabled {
            let snapshot = state
                .snapshot
                .lock()
                .map_err(|_| "Snapshot is unavailable")?;
            let settings = state
                .overlay_settings
                .lock()
                .map_err(|_| "Settings are unavailable")?;
            service::taskbar_text(&snapshot, &settings).replace(['\n', '\t'], "  ")
        } else {
            String::new()
        };
        update_menu_title(&app, title);
        state.taskbar_active.store(enabled, Ordering::Release);
        app.emit("taskbar://mode", enabled)
            .map_err(|error| error.to_string())?;
        Ok(enabled)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (enabled, state, app);
        Err("Native taskbar mode is only available on Windows".to_owned())
    }
}
#[cfg(feature = "desktop")]
fn show_window(app: &AppHandle) {
    app.state::<MonitorState>()
        .dashboard_visible
        .store(true, Ordering::Release);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(feature = "desktop")]
fn handle_menu_event(app: &AppHandle, event: &tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "open" => show_window(app),
        "reset" => {
            app.state::<MonitorState>()
                .reset_generation
                .fetch_add(1, Ordering::AcqRel);
        }
        "quit" => {
            diagnostics::record("quitRequested", "application menu");
            app.state::<service::RuntimeState>()
                .shutdown_gate
                .authorize_quit();
            app.exit(0);
        }
        "discard-quit" => {
            diagnostics::record(
                "quitWithoutSaving",
                "explicit acknowledgement after failed flush",
            );
            app.state::<service::RuntimeState>()
                .shutdown_gate
                .authorize_finalization();
            app.exit(0);
        }
        "hide-dashboard" => {
            app.state::<MonitorState>()
                .dashboard_visible
                .store(false, Ordering::Release);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
            diagnostics::record("windowClose", "application menu");
        }
        _ => {}
    }
}

#[cfg(feature = "desktop")]
fn install_application_menu(
    app: &tauri::App,
    open: &MenuItem<tauri::Wry>,
    reset: &MenuItem<tauri::Wry>,
    quit: &MenuItem<tauri::Wry>,
    discard: &MenuItem<tauri::Wry>,
) -> tauri::Result<()> {
    use tauri::menu::{PredefinedMenuItem, Submenu};
    let close = MenuItem::with_id(
        app,
        "hide-dashboard",
        "Đóng cửa sổ",
        true,
        Some("CmdOrCtrl+W"),
    )?;
    let application = Submenu::with_items(
        app,
        "NetDisk Monitor",
        true,
        &[open, reset, &close, quit, discard],
    )?;
    let edit = Submenu::with_items(
        app,
        "Sửa",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    app.set_menu(Menu::with_items(app, &[&application, &edit])?)?;
    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn get_history(
    resolution: String,
    limit: usize,
    state: State<'_, service::RuntimeState>,
) -> Result<Vec<storage::HistoryRecord>, String> {
    if resolution != "minute" && resolution != "day" {
        return Err("Invalid history resolution".to_owned());
    }
    state.history(&resolution, limit)
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn kill_process(pid: String, identity: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || kill_process_checked(&pid, &identity))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn get_process_icons(
    items: Vec<app_icons::ProcessIconQuery>,
    app: AppHandle,
) -> Result<std::collections::HashMap<String, Option<String>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<MonitorState>();
        app_icons::resolve_batch(&state.icons, items)
    })
    .await
    .map_err(|e| e.to_string())
}
#[cfg(feature = "desktop")]
fn kill_process_checked(pid: &str, identity: &str) -> Result<(), String> {
    let pid = pid.parse::<u32>().map_err(|_| "PID không hợp lệ")?;
    if pid == std::process::id() {
        return Err("Không thể đóng chính ứng dụng giám sát".to_owned());
    }
    process_control::terminate(pid, identity)
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn get_autostart() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(autostart::get_autostart)
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn set_autostart(enabled: bool) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || autostart::set_autostart(enabled))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn export_csv_file(filename: String, content: String) -> Result<String, String> {
    export::validate_export(&filename, &content)?;
    tauri::async_runtime::spawn_blocking(move || export_csv_checked(&filename, &content))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(feature = "desktop")]
fn export_csv_checked(filename: &str, content: &str) -> Result<String, String> {
    use std::io::Write as _;
    let home_dir = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_or_else(
            |_| std::env::current_dir().unwrap_or_default(),
            std::path::PathBuf::from,
        );

    let mut path = home_dir.join("Downloads");

    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    }

    let tmp_path = path.join(format!(".{filename}.{}.tmp", std::process::id()));
    path.push(filename);

    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp_path, &path)
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("Không thể ghi file CSV: {error}"));
    }

    let _ = tauri_plugin_opener::reveal_item_in_dir(&path);

    Ok(path.to_string_lossy().to_string())
}

#[cfg(feature = "desktop")]
fn collect_storage_overview(app: &AppHandle) -> Result<storage_info::StorageOverview, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let webview_dir = app.path().app_local_data_dir().ok();
    let state = app.state::<MonitorState>();
    let runtime = app.state::<service::RuntimeState>();
    let icon_cache_count = state.icons.lock().map_or(0, |icons| icons.cached_count());
    let terminated_process_count = state.processes.lock().map_or(0, |snap| {
        snap.processes.iter().filter(|proc| !proc.active).count()
    });
    let memory = storage_info::MemoryCacheStats {
        icon_cache_count,
        icon_cache_limit: app_icons::MAX_ICON_CACHE,
        terminated_process_count,
        terminated_process_limit: collector::TERMINATED_LIMIT,
    };
    Ok(storage_info::build_overview(
        &app_data_dir,
        webview_dir.as_deref(),
        runtime.history_counts(),
        memory,
    ))
}

#[cfg(feature = "desktop")]
fn clear_caches_and_legacy(app: &AppHandle, app_data_dir: &std::path::Path) -> Result<(), String> {
    let state = app.state::<MonitorState>();
    if let Ok(mut icons) = state.icons.lock() {
        icons.clear();
    }
    let updated = if let Ok(mut processes) = state.processes.lock() {
        processes.processes.retain(|proc| proc.active);
        processes.active_process_count = processes.processes.len();
        processes.process_sequence = processes.process_sequence.saturating_add(1);
        Some(processes.clone())
    } else {
        None
    };
    state.reset_generation.fetch_add(1, Ordering::AcqRel);
    preferences::remove_legacy_v1_if_migrated(app_data_dir)
        .map_err(|error| format!("Không thể xóa file cấu hình cũ: {error}"))?;
    if state.dashboard_visible.load(Ordering::Acquire)
        && state.process_view_active.load(Ordering::Acquire)
    {
        if let Some(snapshot) = updated {
            let _ = app.emit_to("main", "processes://snapshot", snapshot);
        }
    }
    Ok(())
}

#[cfg(feature = "desktop")]
fn clear_storage_checked(
    target: storage_info::ClearTarget,
    app: &AppHandle,
) -> Result<storage_info::StorageOverview, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let runtime = app.state::<service::RuntimeState>();
    if matches!(
        target,
        storage_info::ClearTarget::Caches | storage_info::ClearTarget::AllClearable
    ) {
        clear_caches_and_legacy(app, &app_data_dir)?;
    }
    if matches!(
        target,
        storage_info::ClearTarget::History | storage_info::ClearTarget::AllClearable
    ) {
        runtime.clear_history()?;
        let _ = app.emit_to("main", "history://updated", ());
    }
    if matches!(
        target,
        storage_info::ClearTarget::Diagnostics | storage_info::ClearTarget::AllClearable
    ) {
        if let Some(sink) = runtime.log_sink.get() {
            sink.clear()?;
        }
    }
    collect_storage_overview(app)
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn scan_storage_info(app: AppHandle) -> Result<storage_info::StorageOverview, String> {
    tauri::async_runtime::spawn_blocking(move || collect_storage_overview(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn clear_storage_data(
    target: storage_info::ClearTarget,
    app: AppHandle,
) -> Result<storage_info::StorageOverview, String> {
    tauri::async_runtime::spawn_blocking(move || clear_storage_checked(target, &app))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn open_storage_folder(target: String, app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = match target.as_str() {
            "app_data" => {
                let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                dir
            }
            "webview" => app.path().app_local_data_dir().map_err(|e| e.to_string())?,
            _ => return Err("Thư mục lưu trữ không hợp lệ".to_owned()),
        };
        if !path.exists() {
            return Err("Thư mục lưu trữ chưa được tạo".to_owned());
        }
        tauri_plugin_opener::open_path(&path, None::<&str>)
            .map_err(|e| format!("Không thể mở thư mục: {e}"))?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn inspect_process_path(
    pid: String,
    identity: String,
    reveal: bool,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let parsed_pid = pid
            .parse::<u32>()
            .map_err(|_| "PID không hợp lệ".to_owned())?;
        let path = process_control::executable_path(parsed_pid, &identity)?;
        if reveal {
            let _ = tauri_plugin_opener::reveal_item_in_dir(&path);
        }
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(feature = "desktop")]
#[tauri::command]
async fn scan_cleanup_rules() -> Result<disk_cleaner::CleanupScanReport, String> {
    tauri::async_runtime::spawn_blocking(disk_cleaner::scan_rules)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn execute_cleanup_rules(
    rules: Vec<disk_cleaner::CleanupRuleId>,
) -> Result<disk_cleaner::CleanupExecutionReport, String> {
    tauri::async_runtime::spawn_blocking(move || disk_cleaner::execute_rules(&rules))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn analyze_disk_space(target: String) -> Result<space_analyzer::SpaceAnalysisReport, String> {
    tauri::async_runtime::spawn_blocking(move || space_analyzer::analyze_space(&target))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
async fn reveal_disk_path(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = std::path::PathBuf::from(&path);
        let meta = std::fs::symlink_metadata(&target)
            .map_err(|_| "Đường dẫn không còn tồn tại".to_owned())?;
        if storage_info::is_reparse_or_symlink(&meta) {
            return Err("Không mở đường dẫn liên kết ngoài".to_owned());
        }
        if meta.is_dir() {
            tauri_plugin_opener::open_path(&target, None::<&str>)
                .map_err(|e| format!("Không thể mở thư mục: {e}"))?;
        } else {
            tauri_plugin_opener::reveal_item_in_dir(&target)
                .map_err(|e| format!("Không thể mở vị trí tệp: {e}"))?;
        }
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(feature = "desktop")]
/// Runs the desktop lifecycle.
///
/// # Panics
/// Panics if the native application cannot be initialized.
#[allow(clippy::too_many_lines)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            diagnostics::record("secondInstance", "existing host retained");
            if runtime::should_focus_existing_instance(&args) {
                show_window(app);
            }
        }))
        .manage(MonitorState::default())
        .plugin(tauri_plugin_opener::init())
        .on_menu_event(|app, event| handle_menu_event(app, &event))
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            let runtime = app.state::<service::RuntimeState>();
            let lease = match host_lease::HostLease::acquire(&directory) {
                Ok(lease) => lease,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    // The focus plugin can race its IPC-window registration. Never start a second writer.
                    runtime.shutdown_gate.authorize_quit();
                    app.handle().exit(0);
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            };
            *runtime.host_lease.lock().map_err(|_| "Host lease is unavailable")? = Some(lease);
            let show_dashboard = runtime::should_focus_existing_instance(&std::env::args().collect::<Vec<_>>());
            let log_owner = diagnostics::LogOwner::start(directory.join("diagnostics"))?;
            let sink = log_owner.sink();
            diagnostics::install_global(sink.clone());
            diagnostics::install_panic_hook();
            let _ = runtime.log_sink.set(sink);
            *runtime.log_owner.lock().map_err(|_| "Log owner is unavailable")? = Some(log_owner);
            diagnostics::record("startup", &format!("version={} pid={}", env!("CARGO_PKG_VERSION"), std::process::id()));
            match preferences::load_stored(&directory) {
                Ok(Some(stored)) => {
                    let state = app.state::<MonitorState>();
                    *state.overlay_settings.lock().map_err(|_| "Settings are unavailable")? =
                        OverlaySettings::from_flags(stored.flags, stored.opacity);
                    state.overlay_pinned.store(stored.flags & 128 != 0, Ordering::Release);
                    state.overlay_visible.store(stored.overlay_visible, Ordering::Release);
                    state.taskbar_active.store(stored.taskbar_enabled, Ordering::Release);
                    *state.theme.lock().map_err(|_| "Theme is unavailable")? = stored.theme;
                    *state.overlay_position.lock().map_err(|_| "Position is unavailable")? =
                        stored.overlay_position;
                }
                Ok(None) => {}
                Err(error) => {
                    *app.state::<service::RuntimeState>().storage_error.lock()
                        .map_err(|_| "Diagnostics are unavailable")? = Some(error.to_string());
                }
            }
            let open = MenuItem::with_id(app, "open", "Mở dashboard", true, None::<&str>)?;
            let reset = MenuItem::with_id(app, "reset", "Reset tổng phiên", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Thoát", true, if cfg!(target_os = "macos") { Some("CmdOrCtrl+Q") } else { None })?;
            let discard_quit = MenuItem::with_id(app, "discard-quit", "Thoát không lưu", false, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &reset, &quit, &discard_quit])?;
            if cfg!(target_os = "macos") { install_application_menu(app, &open, &reset, &quit, &discard_quit)?; }
            let tray = TrayIconBuilder::with_id("monitor-tray")
                .icon(
                    app.default_window_icon()
                        .cloned()
                        .ok_or("Missing application tray icon")?,
                )
                .menu(&menu)
                .tooltip("NetDisk Monitor")
                .build(app)?;
            app.manage(TrayState { _icon: tray, discard_quit, #[cfg(target_os = "macos")] title_pending: AtomicBool::new(false) });
            {
                let state = app.state::<MonitorState>();
                if state.overlay_visible.load(Ordering::Acquire) {
                    let pinned = state.overlay_pinned.load(Ordering::Acquire);
                    let desired_pos = state.overlay_position.lock().ok().and_then(|pos| *pos);
                    let _ = overlay::ensure_overlay_window(app.handle(), true, pinned, desired_pos)?;
                }
            }
            service::start(app.handle().clone())?;
            #[cfg(windows)]
            if app.state::<MonitorState>().taskbar_active.load(Ordering::Acquire) {
                let restore_app = app.handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(error) = apply_windows_taskbar_mode(&restore_app, true) {
                        restore_app
                            .state::<MonitorState>()
                            .taskbar_active
                            .store(false, Ordering::Release);
                        diagnostics::record("taskbarRestoreFailed", &error);
                    }
                });
            }
            if show_dashboard { show_window(app.handle()); }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_system_snapshot,
            get_process_snapshot,
            get_runtime_health,
            reset_session_totals,
            get_overlay_settings,
            get_overlay_pinned,
            get_history,
            update_overlay_settings,
            resize_overlay,
            begin_overlay_drag,
            set_overlay_pinned,
            get_overlay_visible,
            set_overlay_visible,
            reset_overlay_position,
            set_process_view_active,
            get_theme,
            set_theme,
            get_taskbar_mode,
            set_taskbar_mode,
            get_process_icons,
            kill_process,
            get_autostart,
            set_autostart,
            export_csv_file,
            scan_storage_info,
            clear_storage_data,
            open_storage_folder,
            inspect_process_path,
            scan_cleanup_rules,
            execute_cleanup_rules,
            analyze_disk_space,
            reveal_disk_path
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                diagnostics::record("windowClose", window.label());
                api.prevent_close();
                let _ = window.hide();
                let state = window.state::<MonitorState>();
                match window.label() {
                    "main" => state.dashboard_visible.store(false, Ordering::Release),
                    "monitor-overlay" => {
                        state.overlay_visible.store(false, Ordering::Release);
                        let _ = window.app_handle().emit("overlay://visibility", false);
                    }
                    _ => {}
                }
            }
            if let WindowEvent::Moved(position) = event {
                if window.label() == "monitor-overlay" {
                    let state = window.state::<MonitorState>();
                    if state.overlay_visible.load(Ordering::Acquire)
                        && position.x > -30_000
                        && position.y > -30_000
                    {
                        if let Ok(mut saved) = state.overlay_position.lock() {
                            *saved = Some((position.x, position.y));
                        }
                    }
                }
            }
            if let WindowEvent::Destroyed = event {
                diagnostics::record("windowDestroyed", window.label());
            }
        })
        .manage(service::RuntimeState::default())
        .build(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("failed to build application: {error}"))
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                let runtime = app.state::<service::RuntimeState>();
                let request = runtime.shutdown_gate.request();
                if request != runtime::ShutdownRequest::Exit {
                    api.prevent_exit();
                }
                if request == runtime::ShutdownRequest::Ignore && runtime.shutdown_gate.ignored_requests() == 1 {
                    diagnostics::record("exitIgnored", &format!("no quit intent; code={code:?}"));
                }
                if request == runtime::ShutdownRequest::Start {
                    diagnostics::record("shutdownStarted", "stopping producers before storage");
                    let app = app.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        let result = app.state::<service::RuntimeState>().shutdown();
                        #[cfg(windows)]
                        if let Ok(mut mode) = app.state::<MonitorState>().taskbar_mode.lock() {
                            mode.shutdown_bounded(std::time::Duration::from_millis(300));
                        }
                        app.state::<service::RuntimeState>().shutdown_gate.complete();
                        match result {
                            Ok(()) => {
                                diagnostics::record("shutdownComplete", "measurement workers and storage stopped");
                                app.state::<service::RuntimeState>().shutdown_gate.authorize_finalization();
                                app.exit(0);
                            }
                            Err(error) => {
                                diagnostics::record("shutdownFailed", &error);
                                // A distinct action prevents an old queued Quit from acknowledging data loss.
                                if let Some(tray) = app.try_state::<TrayState>() {
                                    let _ = tray.discard_quit.set_enabled(true);
                                }
                                if let Ok(mut snapshot) = app.state::<MonitorState>().snapshot.lock() {
                                    snapshot.storage_error = Some(format!("Không lưu được dữ liệu cuối phiên: {error}. Đã dừng đo; chọn ‘Thoát không lưu’ trong menu khay để xác nhận đóng ứng dụng."));
                                    snapshot.sequence = snapshot.sequence.saturating_add(1);
                                    let _ = app.emit("metrics://snapshot", snapshot.clone());
                                }
                                show_window(&app);
                            }
                        }
                    });
                }
                if request == runtime::ShutdownRequest::Finalize {
                    let app = app.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        let runtime = app.state::<service::RuntimeState>();
                        runtime.stop_diagnostics();
                        runtime.shutdown_gate.finish();
                        runtime.shutdown_gate.authorize_finalization();
                        app.exit(0);
                    });
                }
            }
        });
}
