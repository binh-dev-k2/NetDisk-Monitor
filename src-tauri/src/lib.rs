#[cfg(feature = "desktop")]
mod autostart;
#[cfg(feature = "desktop")]
mod collector;
pub mod export;
pub mod macos_disk;
pub mod metrics;
#[cfg(feature = "desktop")]
mod persistence;
pub mod preferences;
#[cfg(feature = "desktop")]
mod process_control;
pub mod process_network;
pub mod runtime;
#[cfg(feature = "desktop")]
mod service;
pub mod storage;
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
#[cfg(feature = "desktop")]
use taskbar::TaskbarMode;
#[cfg(feature = "desktop")]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, LogicalSize, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent,
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
    sequence: u64,
    storage_error: Option<String>,
    taskbar_error: Option<String>,
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
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSnapshot {
    process_sequence: u64,
    processes: Vec<ProcessMetric>,
    process_network_error: Option<String>,
}
#[cfg(feature = "desktop")]
struct MonitorState {
    snapshot: Mutex<SystemSnapshot>,
    processes: Mutex<ProcessSnapshot>,
    reset_generation: AtomicU64,
    overlay_settings: Mutex<OverlaySettings>,
    taskbar_mode: Mutex<TaskbarMode>,
    taskbar_active: AtomicBool,
    overlay_pinned: AtomicBool,
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
            taskbar_mode: Mutex::new(TaskbarMode::default()),
            taskbar_active: AtomicBool::new(false),
            overlay_pinned: AtomicBool::new(true),
        }
    }
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn get_snapshot(state: State<'_, MonitorState>) -> Result<DashboardSnapshot, String> {
    let system = state
        .snapshot
        .lock()
        .map_err(|_| "Metric state is unavailable".to_owned())?
        .clone();
    let process = state
        .processes
        .lock()
        .map_err(|_| "Process state is unavailable".to_owned())?
        .clone();
    Ok(DashboardSnapshot { system, process })
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
    with_overlay_window(&app, |window| window.set_always_on_top(pinned))?;
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
fn get_overlay_visible(state: State<'_, MonitorState>, app: AppHandle) -> Result<bool, String> {
    if state.taskbar_active.load(Ordering::Acquire) {
        return Ok(true);
    }
    let window = app
        .get_webview_window("monitor-overlay")
        .ok_or_else(|| "Overlay window is unavailable".to_owned())?;
    window.is_visible().map_err(|error| error.to_string())
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
    with_overlay_window(&app, |window| {
        if visible {
            let _ = window.set_always_on_top(pinned);
            window.show()
        } else {
            window.hide()
        }
    })?;
    let _ = app.emit("overlay://visibility", visible);
    Ok(visible)
}
#[cfg(feature = "desktop")]
#[tauri::command]
// Tauri command arguments are owned deserialization bindings.
#[allow(clippy::needless_pass_by_value)]
fn get_taskbar_mode(state: State<'_, MonitorState>) -> bool {
    state.taskbar_active.load(Ordering::Acquire)
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
    let overlay = app
        .get_webview_window("monitor-overlay")
        .ok_or_else(|| "Overlay window is unavailable".to_owned())?;
    #[cfg(windows)]
    {
        let mode_app = app.clone();
        let active = tauri::async_runtime::spawn_blocking(move || {
            let state = mode_app.state::<MonitorState>();
            let mut mode = state
                .taskbar_mode
                .lock()
                .map_err(|_| "Taskbar mode is unavailable".to_owned())?;
            if enabled {
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
                mode.update(&service::taskbar_text(&snapshot, &settings));
                // Native text measurement determines the full size, even for one item.
                mode.enable(0, 1, 1)?;
            } else {
                mode.disable();
            }
            let active = mode.is_enabled();
            state.taskbar_active.store(active, Ordering::Release);
            Ok::<bool, String>(active)
        })
        .await
        .map_err(|error| error.to_string())??;
        if enabled {
            overlay.hide().map_err(|error| error.to_string())?;
            let _ = app.emit("overlay://visibility", true);
        } else {
            overlay
                .set_always_on_top(state.overlay_pinned.load(Ordering::Acquire))
                .map_err(|error| error.to_string())?;
        }
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
        if enabled {
            overlay.hide().map_err(|error| error.to_string())?;
        }
        app.emit("taskbar://mode", enabled)
            .map_err(|error| error.to_string())?;
        Ok(enabled)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (enabled, overlay, state);
        Err("Native taskbar mode is only available on Windows".to_owned())
    }
}
#[cfg(feature = "desktop")]
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(feature = "desktop")]
fn create_monitor_overlay(app: &tauri::App) -> tauri::Result<()> {
    if app.get_webview_window("monitor-overlay").is_some() {
        return Ok(());
    }

    let (x, y) = app.primary_monitor()?.map_or((20.0, 8.0), |monitor| {
        let width = f64::from(monitor.size().width) / monitor.scale_factor();
        ((width - 336.0).max(0.0) / 2.0, 8.0)
    });
    WebviewWindowBuilder::new(app, "monitor-overlay", WebviewUrl::App("meter.html".into()))
        .title("NetDisk Monitor")
        .inner_size(336.0, 44.0)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .build()?;

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

    path.push(filename);

    std::fs::write(&path, content.as_bytes())
        .map_err(|e| format!("Không thể ghi file CSV: {e}"))?;

    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer.exe")
            .arg("/select,")
            .arg(&path)
            .spawn();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn();
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }

    Ok(path.to_string_lossy().to_string())
}

#[cfg(feature = "desktop")]
/// Runs the desktop lifecycle.
///
/// # Panics
/// Panics if the native application cannot be initialized.
#[allow(clippy::too_many_lines)]
pub fn run() {
    tauri::Builder::default()
        .manage(MonitorState::default())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            match preferences::load(&directory) {
                Ok(Some((flags, opacity))) => {
                    let state = app.state::<MonitorState>();
                    *state.overlay_settings.lock().map_err(|_| "Settings are unavailable")? =
                        OverlaySettings::from_flags(flags, opacity);
                    state.overlay_pinned.store(flags & 128 != 0, Ordering::Release);
                }
                Ok(None) => {}
                Err(error) => {
                    *app.state::<service::RuntimeState>().storage_error.lock()
                        .map_err(|_| "Diagnostics are unavailable")? = Some(error.to_string());
                }
            }
            let open = MenuItem::with_id(app, "open", "Mở dashboard", true, None::<&str>)?;
            let reset = MenuItem::with_id(app, "reset", "Reset tổng phiên", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Thoát", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &reset, &quit])?;
            let tray = TrayIconBuilder::with_id("monitor-tray")
                .icon(
                    app.default_window_icon()
                        .cloned()
                        .ok_or("Missing application tray icon")?,
                )
                .menu(&menu)
                .tooltip("NetDisk Monitor")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show_window(app),
                    "reset" => {
                        let state = app.state::<MonitorState>();
                        state.reset_generation.fetch_add(1, Ordering::AcqRel);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            app.manage(TrayState { _icon: tray, #[cfg(target_os = "macos")] title_pending: AtomicBool::new(false) });
            create_monitor_overlay(app)?;
            if let Some(overlay) = app.get_webview_window("monitor-overlay") {
                overlay.set_always_on_top(app.state::<MonitorState>().overlay_pinned.load(Ordering::Acquire))?;
            }
            service::start(app.handle().clone())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
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
            get_taskbar_mode,
            set_taskbar_mode,
            kill_process,
            get_autostart,
            set_autostart,
            export_csv_file
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .manage(service::RuntimeState::default())
        .build(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("failed to build application: {error}"))
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let runtime = app.state::<service::RuntimeState>();
                let request = runtime.shutdown_gate.request();
                if request != runtime::ShutdownRequest::Exit {
                    api.prevent_exit();
                }
                if request == runtime::ShutdownRequest::Start {
                    let app = app.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        let result = app.state::<service::RuntimeState>().shutdown();
                        if let Ok(mut mode) = app.state::<MonitorState>().taskbar_mode.lock() {
                            mode.disable();
                        }
                        app.state::<service::RuntimeState>().shutdown_gate.complete();
                        match result {
                            Ok(()) => app.exit(0),
                            Err(error) => {
                                // Workers have stopped; a second quit acknowledges unsaved data.
                                if let Ok(mut snapshot) = app.state::<MonitorState>().snapshot.lock() {
                                    snapshot.storage_error = Some(format!("Không lưu được dữ liệu cuối phiên: {error}. Đã dừng đo; chọn Thoát lần nữa để đóng ứng dụng."));
                                    snapshot.sequence = snapshot.sequence.saturating_add(1);
                                    let _ = app.emit("metrics://snapshot", snapshot.clone());
                                }
                                show_window(&app);
                            }
                        }
                    });
                }
            }
        });
}
