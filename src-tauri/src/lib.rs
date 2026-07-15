#[cfg(feature = "desktop")]
mod collector;
pub mod metrics;
#[cfg(feature = "desktop")]
mod taskbar;
#[cfg(feature = "desktop")]
use collector::{ProcessMetric, SystemCollector};
use metrics::{MetricsEngine, MetricsSnapshot};
use serde::{Deserialize, Serialize};
#[cfg(feature = "desktop")]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
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
struct DashboardSnapshot {
    #[serde(flatten)]
    metrics: MetricsSnapshot,
    processes: Vec<ProcessMetric>,
    interfaces: Vec<String>,
}
#[cfg(feature = "desktop")]
struct MonitorState {
    engine: Mutex<MetricsEngine>,
    snapshot: Mutex<DashboardSnapshot>,
    reset_process_sessions: AtomicBool,
    overlay_settings: Mutex<OverlaySettings>,
    taskbar_mode: Mutex<TaskbarMode>,
    overlay_pinned: AtomicBool,
}

#[cfg(feature = "desktop")]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlaySettings {
    show_network_down: bool,
    show_network_up: bool,
    show_disk_read: bool,
    show_disk_write: bool,
    show_session_total: bool,
    show_session_duration: bool,
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
            opacity: 72,
        }
    }
}

#[cfg(feature = "desktop")]
impl OverlaySettings {
    fn normalized(mut self) -> Self {
        self.opacity = self.opacity.clamp(35, 92);
        self
    }
}

#[cfg(feature = "desktop")]
struct TrayState {
    _icon: TrayIcon,
}

#[cfg(feature = "desktop")]
#[cfg(test)]
mod overlay_window_tests {
    use super::overlay_window_size;

    #[test]
    fn overlay_window_size_clamps_to_the_compact_two_column_grid() {
        assert_eq!(overlay_window_size(999.0, 999.0), (336.0, 124.0));
    }
}
#[cfg(feature = "desktop")]
impl Default for MonitorState {
    fn default() -> Self {
        Self {
            engine: Mutex::new(MetricsEngine::default()),
            snapshot: Mutex::new(DashboardSnapshot::default()),
            reset_process_sessions: AtomicBool::new(false),
            overlay_settings: Mutex::new(OverlaySettings::default()),
            taskbar_mode: Mutex::new(TaskbarMode::default()),
            overlay_pinned: AtomicBool::new(true),
        }
    }
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn get_snapshot(state: State<'_, MonitorState>) -> Result<DashboardSnapshot, String> {
    state
        .snapshot
        .lock()
        .map(|snapshot| snapshot.clone())
        .map_err(|_| "Metric state is unavailable".to_owned())
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn reset_session_totals(state: State<'_, MonitorState>) -> Result<(), String> {
    state
        .engine
        .lock()
        .map(|mut engine| engine.reset_session_totals())
        .map_err(|_| "Metric engine is unavailable".to_owned())?;
    state.reset_process_sessions.store(true, Ordering::Release);
    Ok(())
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn get_overlay_settings(state: State<'_, MonitorState>) -> Result<OverlaySettings, String> {
    state
        .overlay_settings
        .lock()
        .map(|settings| settings.clone())
        .map_err(|_| "Overlay settings are unavailable".to_owned())
}
#[cfg(feature = "desktop")]
#[tauri::command]
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
    app.emit_to("monitor-overlay", "overlay://settings", settings.clone())
        .map_err(|error| error.to_string())?;
    Ok(settings)
}
#[cfg(feature = "desktop")]
fn overlay_window_size(width: f64, height: f64) -> (f64, f64) {
    let width = if width <= 166.0 { 166.0 } else { 336.0 };
    (width, height.clamp(44.0, 124.0))
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
fn resize_overlay(
    width: f64,
    height: f64,
    state: State<'_, MonitorState>,
    app: AppHandle,
) -> Result<(), String> {
    let mut taskbar_mode = state
        .taskbar_mode
        .lock()
        .map_err(|_| "Taskbar mode is unavailable".to_owned())?;
    if taskbar_mode.is_enabled() {
        #[cfg(windows)]
        {
            let window = app
                .get_webview_window("monitor-overlay")
                .ok_or_else(|| "Overlay window is unavailable".to_owned())?;
            let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
            let physical_width = (width * scale_factor).round() as i32;
            let physical_height = (height * scale_factor).round() as i32;
            return taskbar_mode.resize(physical_width, physical_height);
        }
        #[cfg(not(windows))]
        return Ok(());
    }
    drop(taskbar_mode);
    let (width, height) = overlay_window_size(width, height);
    let pinned = state.overlay_pinned.load(Ordering::Acquire);
    with_overlay_window(&app, |window| {
      let _ = window.set_always_on_top(pinned);
      window.set_size(LogicalSize::new(width, height))
    })
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn begin_overlay_drag(app: AppHandle) -> Result<(), String> {
    with_overlay_window(&app, tauri::WebviewWindow::start_dragging)
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn set_overlay_pinned(pinned: bool, state: State<'_, MonitorState>, app: AppHandle) -> Result<(), String> {
    state.overlay_pinned.store(pinned, Ordering::Release);
    with_overlay_window(&app, |window| window.set_always_on_top(pinned))
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn get_overlay_visible(app: AppHandle) -> Result<bool, String> {
    let window = app
        .get_webview_window("monitor-overlay")
        .ok_or_else(|| "Overlay window is unavailable".to_owned())?;
    window.is_visible().map_err(|error| error.to_string())
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn set_overlay_visible(
    visible: bool,
    state: State<'_, MonitorState>,
    app: AppHandle,
) -> Result<bool, String> {
    if state
        .taskbar_mode
        .lock()
        .map_err(|_| "Taskbar mode is unavailable".to_owned())?
        .is_enabled()
    {
        return Err("Turn off taskbar mode before hiding the desktop overlay".to_owned());
    }
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
fn get_taskbar_mode(state: State<'_, MonitorState>) -> Result<bool, String> {
    state
        .taskbar_mode
        .lock()
        .map(|mode| mode.is_enabled())
        .map_err(|_| "Taskbar mode is unavailable".to_owned())
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn set_taskbar_mode(
    enabled: bool,
    state: State<'_, MonitorState>,
    app: AppHandle,
) -> Result<bool, String> {
    let overlay = app
        .get_webview_window("monitor-overlay")
        .ok_or_else(|| "Overlay window is unavailable".to_owned())?;
    let mut mode = state
        .taskbar_mode
        .lock()
        .map_err(|_| "Taskbar mode is unavailable".to_owned())?;
    #[cfg(windows)]
    {
        if enabled {
            let hwnd = overlay.hwnd().map_err(|error| error.to_string())?;
            let scale_factor = overlay.scale_factor().map_err(|error| error.to_string())?;
            let initial_width = (200.0 * scale_factor).round() as i32;
            let initial_height = (42.0 * scale_factor).round() as i32;
            overlay.show().map_err(|error| error.to_string())?;
            overlay
                .set_always_on_top(false)
                .map_err(|error| error.to_string())?;
            if let Err(error) = mode.enable(hwnd.0 as isize, initial_width, initial_height) {
                let _ = overlay.set_always_on_top(true);
                return Err(error);
            }
        } else {
            mode.disable();
            overlay
                .set_always_on_top(true)
                .map_err(|error| error.to_string())?;
        }
        app.emit_to("monitor-overlay", "taskbar://mode", mode.is_enabled())
            .map_err(|error| error.to_string())?;
        return Ok(mode.is_enabled());
    }
    #[cfg(not(windows))]
    {
        let _ = (enabled, overlay, mode);
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

    let (x, y) = app
        .primary_monitor()?
        .map(|monitor| {
            let width = monitor.size().width;
            (f64::from(width.saturating_sub(336)) / 2.0, 8.0)
        })
        .unwrap_or((20.0, 8.0));
    WebviewWindowBuilder::new(app, "monitor-overlay", WebviewUrl::App("meter.html".into()))
        .title("NetDisk Monitor")
        .inner_size(336.0, 44.0)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .build()?;
    Ok(())
}
#[cfg(feature = "desktop")]
fn start_collector(app: AppHandle) {
    thread::spawn(move || {
        let mut collector = SystemCollector::new();
        let mut previous = Instant::now();
        loop {
            let state = app.state::<MonitorState>();
            if state.reset_process_sessions.swap(false, Ordering::AcqRel) {
                collector.reset_session_totals();
            }
            let collection = collector.collect();
            let elapsed = previous.elapsed().as_secs_f64();
            previous = Instant::now();
            let metrics = match state.engine.lock() {
                Ok(mut engine) => engine.update(collection.counters, elapsed),
                Err(_) => break,
            };
            let mut processes = collection.processes;
            let total_connections: u32 = processes.iter().map(|p| p.network_connections).sum();
            if total_connections > 0 {
                for p in &mut processes {
                    let ratio = p.network_connections as f64 / total_connections as f64;
                    p.network_down_bps = (metrics.network_down_bps as f64 * ratio).round() as u64;
                    p.network_up_bps = (metrics.network_up_bps as f64 * ratio).round() as u64;
                }
            }
            let snapshot = DashboardSnapshot {
                metrics,
                processes,
                interfaces: collection.interfaces,
            };
            if let Ok(mut current) = state.snapshot.lock() {
                *current = snapshot.clone();
            }
            let _ = app.emit("metrics://snapshot", snapshot);
            thread::sleep(Duration::from_secs(1));
        }
    });
}
#[cfg(feature = "desktop")]
pub fn run() {
    tauri::Builder::default()
        .manage(MonitorState::default())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
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
                        if let Ok(mut engine) = state.engine.lock() {
                            engine.reset_session_totals();
                        }
                        state.reset_process_sessions.store(true, Ordering::Release);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            app.manage(TrayState { _icon: tray });
            create_monitor_overlay(app)?;
            start_collector(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            reset_session_totals,
            get_overlay_settings,
            update_overlay_settings,
            resize_overlay,
            begin_overlay_drag,
            set_overlay_pinned,
            get_overlay_visible,
            set_overlay_visible,
            get_taskbar_mode,
            set_taskbar_mode
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("failed to run application: {error}"));
}
