//! Independent native taskbar renderer; never touches the supplied `WebView`.
//! Requires `Win32_Graphics_Gdi`, `Win32_System_LibraryLoader`, and `Win32_System_Registry` windows-sys features.
//! UIA fallback uses windows 0.57: `Win32_Foundation`, `Win32_System_Com`,
//! `Win32_UI_Accessibility`. All COM objects stay on the renderer thread.
//! Native GDI output is opaque; `WebView` opacity settings do not apply here.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskbarPhase {
    #[default]
    Disabled,
    Starting,
    Attached,
    Degraded,
    Unsupported,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskbarStatus {
    pub phase: TaskbarPhase,
    pub desired: bool,
    pub attached: bool,
    pub reason: Option<String>,
}

#[cfg(any(windows, test))]
#[must_use]
pub(crate) fn classify_taskbar_error(error: &str) -> TaskbarPhase {
    if error.contains("horizontal taskbar")
        || error.contains("only available on Windows")
        || error.contains("coordinate mismatch")
    {
        TaskbarPhase::Unsupported
    } else {
        TaskbarPhase::Degraded
    }
}

#[cfg(windows)]
#[derive(Default)]
pub struct TaskbarMode {
    text: String,
    left_exclusion: Option<i32>,
    boundary_revision: u64,
    #[cfg(windows)]
    session: Option<native::Session>,
}

#[cfg(windows)]
impl TaskbarMode {
    /// Ignores overlay. Dimensions are physical native pixels. Waits at most 750ms
    /// for worker startup, not attachment; placement failures appear in `last_error()`.
    pub fn enable(&mut self, _overlay: isize, width: i32, height: i32) -> Result<(), String> {
        validate_size(width, height)?;
        #[cfg(windows)]
        {
            if self.is_enabled() {
                return self.resize(width, height);
            }
            if self
                .session
                .as_ref()
                .is_some_and(|session| !session.finished())
            {
                self.shutdown_bounded(std::time::Duration::from_millis(150));
            }
            if let Some(session) = &self.session {
                if !session.finished() {
                    return Err("Native renderer is still shutting down".to_owned());
                }
            }
            self.disable();
            let input = native::Input {
                text: self.text.clone(),
                width,
                height,
                left_exclusion: self.left_exclusion,
                boundary_revision: self.boundary_revision,
            };
            let (session, ready) = native::Session::spawn(input)?;
            self.session = Some(session);
            let session = self.session.as_ref().expect("session was just installed");
            match ready.recv_timeout(std::time::Duration::from_millis(750)) {
                Ok(Ok(())) => {
                    session.activate();
                    Ok(())
                }
                result => {
                    // Never join on this path: preserve the startup timeout.
                    session.stop();
                    let error = match result {
                        Ok(Err(error)) => error,
                        _ => "Native renderer startup timed out or worker exited".to_owned(),
                    };
                    session.error(Some(error.clone()));
                    Err(error)
                }
            }
        }
        #[cfg(not(windows))]
        Err("Native taskbar rendering is only available on Windows".to_owned())
    }

    /// State-only compatibility API. Never resizes `WebView` or Explorer.
    /// Success acknowledges the request; placement errors are asynchronous.
    pub fn resize(&mut self, width: i32, height: i32) -> Result<(), String> {
        validate_size(width, height)?;
        #[cfg(windows)]
        if let Some(session) = &self.session {
            session.change(|input| {
                input.width = width;
                input.height = height;
            });
        }
        Ok(())
    }

    /// Latest-wins text, cached while disabled. Two newline-separated rows of tab-
    /// separated cells; at most 2048 Unicode scalar values and 16 cells per row.
    /// No HWND calls, message posting, or per-sample thread creation.
    pub fn update(&mut self, text: &str) {
        self.text = normalize_text(text);
        #[cfg(windows)]
        if let Some(session) = &self.session {
            session.change(|input| input.text.clone_from(&self.text));
        }
    }

    /// Measured right edge of weather/other left controls relative to `Shell_TrayWnd`.
    /// Some(0) explicitly confirms no left controls; None requests HWND detection.
    /// Re-submit after taskbar geometry/Explorer changes; stale bounds fail closed.
    /// XAML bounds may need UI Automation in the parent. Never guess raw offsets.
    #[allow(dead_code)] // Reserved for a platform-provided measured boundary override.
    pub fn set_left_exclusion(&mut self, right: Option<i32>) -> Result<(), String> {
        if right.is_some_and(|value| value < 0) {
            return Err("Taskbar exclusion must be a nonnegative physical coordinate".to_owned());
        }
        self.left_exclusion = right;
        self.boundary_revision = self.boundary_revision.wrapping_add(1);
        #[cfg(windows)]
        if let Some(session) = &self.session {
            session.change(|input| {
                input.left_exclusion = right;
                input.boundary_revision = self.boundary_revision;
            });
        }
        Ok(())
    }

    /// State-only cooperative stop. Retains at most one retiring worker and joins
    /// only when `is_finished()` is true. HWND cleanup stays on the native worker.
    pub fn disable(&mut self) {
        #[cfg(windows)]
        {
            if let Some(session) = &self.session {
                session.stop();
            }
            if self.session.as_ref().is_some_and(native::Session::finished) {
                if let Some(session) = self.session.take() {
                    session.reap();
                }
            }
        }
    }

    /// Bounded cooperative stop for app shutdown; never waits forever on a stalled shell.
    pub fn shutdown_bounded(&mut self, timeout: std::time::Duration) {
        #[cfg(windows)]
        {
            self.disable();
            let deadline = std::time::Instant::now() + timeout;
            while self
                .session
                .as_ref()
                .is_some_and(|session| !session.finished())
                && std::time::Instant::now() < deadline
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if self.session.as_ref().is_some_and(native::Session::finished) {
                if let Some(session) = self.session.take() {
                    session.reap();
                }
            }
        }
        #[cfg(not(windows))]
        let _ = timeout;
    }

    /// Active worker, not necessarily visible (no room, autohide or empty text).
    pub fn is_enabled(&self) -> bool {
        #[cfg(windows)]
        return self.session.as_ref().is_some_and(native::Session::enabled);
        #[cfg(not(windows))]
        false
    }

    pub fn last_error(&self) -> Option<String> {
        #[cfg(windows)]
        return self.session.as_ref().and_then(native::Session::last_error);
        #[cfg(not(windows))]
        None
    }

    #[must_use]
    pub fn status(&self) -> TaskbarStatus {
        #[cfg(windows)]
        {
            if !self.is_enabled() {
                return TaskbarStatus {
                    phase: TaskbarPhase::Disabled,
                    desired: false,
                    attached: false,
                    reason: None,
                };
            }
            if let Some(reason) = self.last_error() {
                return TaskbarStatus {
                    phase: classify_taskbar_error(&reason),
                    desired: true,
                    attached: false,
                    reason: Some(reason),
                };
            }
            let session = self.session.as_ref();
            if session.is_some_and(native::Session::is_attached) {
                TaskbarStatus {
                    phase: TaskbarPhase::Attached,
                    desired: true,
                    attached: true,
                    reason: None,
                }
            } else if session.is_some_and(native::Session::has_refreshed) {
                TaskbarStatus {
                    phase: TaskbarPhase::Degraded,
                    desired: true,
                    attached: false,
                    reason: Some("Taskbar đang ẩn hoặc chưa có chỉ số hiển thị".to_owned()),
                }
            } else {
                TaskbarStatus {
                    phase: TaskbarPhase::Starting,
                    desired: true,
                    attached: false,
                    reason: None,
                }
            }
        }
        #[cfg(not(windows))]
        TaskbarStatus::default()
    }
}

#[cfg(windows)]
impl Drop for TaskbarMode {
    // Dropping a still-running JoinHandle detaches it; Shared and HWND state remain
    // owned by that worker until it observes stop. Never block the caller on Win32.
    fn drop(&mut self) {
        self.disable();
    }
}

#[cfg(windows)]
fn validate_size(width: i32, height: i32) -> Result<(), String> {
    if width <= 0 || height <= 0 {
        return Err("Native taskbar dimensions must be positive".to_owned());
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(windows)]
mod native {
    use super::{
        cell_width_template, compute_taskbar_slots, is_container_class, native_placement, wide,
        BoundaryRetry, TaskbarSlots,
    };
    use std::{
        ptr::{null, null_mut},
        sync::{
            atomic::{AtomicBool, AtomicU32, Ordering},
            mpsc, Arc, Mutex, OnceLock, PoisonError,
        },
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    };
    use windows::Win32::{
        Foundation::{E_FAIL, HWND as UiaHwnd},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_MULTITHREADED,
        },
        UI::Accessibility::{
            CUIAutomation8, IUIAutomation2, TreeScope_Descendants, TreeScope_Element,
            UIA_AutomationIdPropertyId, UIA_BoundingRectanglePropertyId, UIA_ClassNamePropertyId,
            UIA_IsOffscreenPropertyId, UIA_NativeWindowHandlePropertyId,
        },
    };
    use windows_sys::Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CreateFontW, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC,
            GetMonitorInfoW, GetStockObject, GetSysColor, GetTextExtentPoint32W, InvalidateRect,
            MonitorFromWindow, ReleaseDC, SelectObject, SetBkMode, SetDCBrushColor, SetTextColor,
            COLOR_BTNFACE, COLOR_BTNTEXT, DC_BRUSH, DT_END_ELLIPSIS, DT_NOPREFIX, DT_SINGLELINE,
            DT_VCENTER, HFONT, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, TRANSPARENT,
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Registry::{
                RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ,
                REG_DWORD,
            },
            Threading::GetCurrentThreadId,
        },
        UI::HiDpi::{
            GetDpiForWindow, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumChildWindows,
            FindWindowW, GetClassNameW, GetClientRect, GetForegroundWindow, GetMessageW, GetWindow,
            GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsChild, IsWindow,
            IsWindowVisible, KillTimer, PostThreadMessageW, RegisterClassW,
            SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
            TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, GW_HWNDPREV, HWND_TOPMOST, LWA_ALPHA,
            MA_NOACTIVATE, MSG, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE, WM_ERASEBKGND,
            WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCDESTROY, WM_NULL, WM_PAINT, WM_SETTINGCHANGE,
            WNDCLASSW, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
        },
    };

    const CLASS: &str = "NetDiskMonitor.NativeTaskbar.v1";
    const TICK_MS: u32 = 250;
    const DISCOVERY_INTERVAL: Duration = Duration::from_millis(500);
    const UIA_INTERVAL: Duration = Duration::from_secs(5);

    #[derive(Clone, PartialEq, Eq)]
    pub(super) struct Input {
        pub text: String,
        pub width: i32,
        pub height: i32,
        pub left_exclusion: Option<i32>,
        pub boundary_revision: u64,
    }

    struct Shared {
        input: Mutex<Input>,
        error: Mutex<Option<String>>,
        stop: AtomicBool,
        active: AtomicBool,
        live: AtomicBool,
        attached: AtomicBool,
        has_refreshed: AtomicBool,
        thread_id: AtomicU32,
    }

    pub(super) struct Session {
        shared: Arc<Shared>,
        worker: JoinHandle<()>,
    }

    impl Session {
        pub fn spawn(input: Input) -> Result<(Self, mpsc::Receiver<Result<(), String>>), String> {
            let shared = Arc::new(Shared {
                input: Mutex::new(input),
                error: Mutex::new(None),
                stop: AtomicBool::new(false),
                active: AtomicBool::new(false),
                live: AtomicBool::new(false),
                attached: AtomicBool::new(false),
                has_refreshed: AtomicBool::new(false),
                thread_id: AtomicU32::new(0),
            });
            let (sender, receiver) = mpsc::sync_channel(1);
            let state = Arc::clone(&shared);
            let worker = thread::Builder::new()
                .name("native-taskbar".to_owned())
                .spawn(move || {
                    let _live = LiveGuard(Arc::clone(&state));
                    if let Err(error) = run(&state, &sender) {
                        *state.error.lock().unwrap_or_else(PoisonError::into_inner) =
                            Some(error.clone());
                        let _ = sender.try_send(Err(error));
                    }
                })
                .map_err(|error| format!("Unable to start native renderer: {error}"))?;
            Ok((Self { shared, worker }, receiver))
        }

        pub fn change(&self, change: impl FnOnce(&mut Input)) {
            change(
                &mut self
                    .shared
                    .input
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            );
        }
        pub fn activate(&self) {
            self.shared.active.store(true, Ordering::Release);
        }
        pub fn stop(&self) {
            self.shared.stop.store(true, Ordering::Release);
            let thread_id = self.shared.thread_id.load(Ordering::Acquire);
            if thread_id != 0 && !self.worker.is_finished() {
                unsafe {
                    PostThreadMessageW(thread_id, WM_NULL, 0, 0);
                }
            }
        }
        pub fn enabled(&self) -> bool {
            self.shared.live.load(Ordering::Acquire)
                && self.shared.active.load(Ordering::Acquire)
                && !self.shared.stop.load(Ordering::Acquire)
        }
        pub fn is_attached(&self) -> bool {
            self.shared.attached.load(Ordering::Acquire)
        }
        pub fn has_refreshed(&self) -> bool {
            self.shared.has_refreshed.load(Ordering::Acquire)
        }
        pub fn finished(&self) -> bool {
            self.worker.is_finished()
        }
        pub fn error(&self, error: Option<String>) {
            *self
                .shared
                .error
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = error;
        }
        pub fn last_error(&self) -> Option<String> {
            self.shared
                .error
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
        pub fn reap(self) {
            debug_assert!(self.worker.is_finished());
            let _ = self.worker.join();
        }
    }

    struct LiveGuard(Arc<Shared>);
    impl Drop for LiveGuard {
        fn drop(&mut self) {
            self.0.thread_id.store(0, Ordering::Release);
            self.0.attached.store(false, Ordering::Release);
            self.0.live.store(false, Ordering::Release);
        }
    }

    // A thread timer survives loss of the Explorer parent and our child HWND.
    fn run(
        shared: &Arc<Shared>,
        ready: &mpsc::SyncSender<Result<(), String>>,
    ) -> Result<(), String> {
        let _dpi = DpiContext::new()?;
        let module = register_class()?;
        let _apartment = ComApartment::new()?;
        let timer = unsafe { SetTimer(null_mut(), 0, TICK_MS, None) };
        if timer == 0 {
            return Err(win_error("SetTimer"));
        }
        let _timer = ThreadTimer(timer);
        let mut renderer = Renderer::new(module);
        shared
            .thread_id
            .store(unsafe { GetCurrentThreadId() }, Ordering::Release);
        shared.live.store(true, Ordering::Release);
        if ready.send(Ok(())).is_err() {
            return Ok(());
        }
        let mut message = MSG::default();
        let mut last_tick = elapsed_anchor(Duration::from_millis(u64::from(TICK_MS)));
        while !shared.stop.load(Ordering::Acquire) {
            let result = unsafe { GetMessageW(&raw mut message, null_mut(), 0, 0) };
            if result == -1 {
                return Err(win_error("GetMessageW"));
            }
            if result == 0 || shared.stop.load(Ordering::Acquire) {
                break;
            }
            if shared.active.load(Ordering::Acquire)
                && last_tick.elapsed() >= Duration::from_millis(u64::from(TICK_MS))
            {
                last_tick = Instant::now();
                // Release the input lock before Win32; callbacks never acquire Shared locks.
                let input = shared
                    .input
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone();
                let error = renderer.refresh(&input).err();
                if error.is_some() {
                    renderer.hide();
                }
                shared
                    .attached
                    .store(error.is_none() && renderer.visible, Ordering::Release);
                shared.has_refreshed.store(true, Ordering::Release);
                *shared.error.lock().unwrap_or_else(PoisonError::into_inner) = error;
            }
            unsafe {
                TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
        Ok(())
    }

    fn elapsed_anchor(interval: Duration) -> Instant {
        let now = Instant::now();
        now.checked_sub(interval).unwrap_or(now)
    }

    struct ThreadTimer(usize);
    impl Drop for ThreadTimer {
        fn drop(&mut self) {
            unsafe {
                KillTimer(null_mut(), self.0);
            }
        }
    }

    // Thread-only: does not alter Tauri/WebView awareness or desktop settings.
    struct DpiContext(DPI_AWARENESS_CONTEXT);
    impl DpiContext {
        fn new() -> Result<Self, String> {
            let previous =
                unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
            if previous.is_null() {
                return Err(win_error("SetThreadDpiAwarenessContext"));
            }
            Ok(Self(previous))
        }
    }
    impl Drop for DpiContext {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }

    struct ComApartment;
    impl ComApartment {
        fn new() -> Result<Self, String> {
            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
                .ok()
                .map_err(|error| format!("UIA COM initialization failed: {error}"))?;
            Ok(Self)
        }
    }
    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    struct Automation(IUIAutomation2);
    impl Automation {
        fn new() -> Result<Self, String> {
            unsafe {
                let automation: IUIAutomation2 =
                    CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                        .map_err(|error| format!("UIA provider creation failed: {error}"))?;
                automation
                    .SetConnectionTimeout(250)
                    .and_then(|()| automation.SetTransactionTimeout(250))
                    .map_err(|error| {
                        format!("UIA provider timeout configuration failed: {error}")
                    })?;
                Ok(Self(automation))
            }
        }

        #[allow(
            clippy::too_many_lines,
            reason = "Keeps UIA bounds discovery, XAML filtering and slot extraction in one sequence"
        )]
        fn discover_slots(
            &self,
            parent: HWND,
            own: HWND,
            bar: RECT,
            start_rect: Option<RECT>,
            tray_rect: Option<RECT>,
        ) -> Result<TaskbarSlots, String> {
            let query = || -> windows::core::Result<TaskbarSlots> {
                unsafe {
                    let root = self.0.ElementFromHandle(UiaHwnd(parent as isize))?;
                    let physical = root.CurrentBoundingRectangle()?;
                    if physical.left != bar.left
                        || physical.top != bar.top
                        || physical.right != bar.right
                        || physical.bottom != bar.bottom
                    {
                        return Err(windows::core::Error::new(
                            E_FAIL,
                            "UIA/Win32 coordinate mismatch; taskbar requires per-monitor DPI awareness",
                        ));
                    }
                    // Scan the XAML host subtree, never the Shell tree containing our
                    // sibling HWND. A query condition alone still invokes its provider.
                    let host = find_descendant(parent, &["Windows.UI.Input.InputSite.WindowClass"])
                        .ok_or_else(|| {
                            windows::core::Error::new(
                                E_FAIL,
                                "Cannot identify the native XAML taskbar host",
                            )
                        })?;
                    let host_rect = window_rect(host)
                        .map_err(|error| windows::core::Error::new(E_FAIL, error))?;
                    if !super::uia_host_is_container(
                        [bar.left, bar.top, bar.right, bar.bottom],
                        [
                            host_rect.left,
                            host_rect.top,
                            host_rect.right,
                            host_rect.bottom,
                        ],
                        IsChild(parent, host) != 0,
                    ) || (!own.is_null() && IsChild(host, own) != 0)
                    {
                        return Err(windows::core::Error::new(
                            E_FAIL,
                            "Native XAML taskbar host is not a verified independent container",
                        ));
                    }
                    let scan_root = self.0.ElementFromHandle(UiaHwnd(host as isize))?;
                    let request = self.boundary_cache()?;
                    let condition = self.0.CreateTrueCondition()?;
                    let items =
                        scan_root.FindAllBuildCache(TreeScope_Descendants, &condition, &request)?;
                    let count = items.Length()?;
                    if !(1..=2048).contains(&count) {
                        return Err(windows::core::Error::new(
                            E_FAIL,
                            "UIA taskbar tree is empty or exceeds the bounded scan limit",
                        ));
                    }
                    let bar_w = bar.right - bar.left;
                    let bar_h = bar.bottom - bar.top;
                    let mut rects = visible_native_bounds(parent, own, bar)
                        .map_err(|error| windows::core::Error::new(E_FAIL, error))?;
                    let mut start_left = start_rect.map(|r| r.left - bar.left);
                    let mut tray_left = tray_rect.map(|r| r.left - bar.left);
                    #[cfg(test)]
                    let mut diagnostics = BoundaryDiagnostics::new(&rects);
                    for index in 0..count {
                        let element = items.GetElement(index)?;
                        let class = element.CachedClassName()?.to_string();
                        let handle = element.CachedNativeWindowHandle()?.0;
                        if class == CLASS || (handle != 0 && handle == own as isize) {
                            continue;
                        }
                        #[cfg(test)]
                        diagnostics.record(index, &element, bar)?;
                        if element.CachedIsOffscreen()?.as_bool() {
                            continue;
                        }
                        let rect = element.CachedBoundingRectangle()?;
                        let relative = [
                            rect.left - bar.left,
                            rect.top - bar.top,
                            rect.right - bar.left,
                            rect.bottom - bar.top,
                        ];
                        if relative[2] <= relative[0] || relative[3] <= relative[1] {
                            continue;
                        }
                        let auto_id = element.CachedAutomationId()?.to_string();
                        if auto_id == "StartButton" && start_left.is_none() {
                            start_left = Some(relative[0]);
                        }
                        if (auto_id.contains("SystemTray") || class.contains("SystemTray"))
                            && relative[0] > bar_w / 2
                        {
                            tray_left =
                                Some(tray_left.map_or(relative[0], |prev| prev.min(relative[0])));
                        }
                        if is_container_class(&class)
                            || (relative[2] - relative[0]) >= bar_w * 3 / 4
                        {
                            continue;
                        }
                        rects.push(relative);
                    }
                    let widgets_enabled = widgets_enabled_in_registry();
                    let slots = compute_taskbar_slots(
                        bar_w,
                        bar_h,
                        start_left,
                        tray_left,
                        widgets_enabled,
                        &rects,
                    );
                    #[cfg(test)]
                    if slots.left_slot.is_none() && slots.right_slot.is_none() {
                        let fallback_start = start_rect.unwrap_or_default();
                        diagnostics.report(bar, fallback_start, start_left.unwrap_or(-1), count);
                    }
                    if slots.left_slot.is_none() && slots.right_slot.is_none() {
                        return Err(windows::core::Error::new(
                            E_FAIL,
                            "UIA tree contains ambiguous left-side bounds",
                        ));
                    }
                    Ok(slots)
                }
            };
            query().map_err(|error| {
                format!(
                    "Weather HWND unavailable; native UIA cannot verify free space: {error}; renderer hidden, retrying. Supply measured set_left_exclusion() to override"
                )
            })
        }

        fn boundary_cache(
            &self,
        ) -> windows::core::Result<windows::Win32::UI::Accessibility::IUIAutomationCacheRequest>
        {
            unsafe {
                let request = self.0.CreateCacheRequest()?;
                request.SetTreeScope(TreeScope_Element)?;
                for property in [
                    UIA_AutomationIdPropertyId,
                    UIA_BoundingRectanglePropertyId,
                    UIA_ClassNamePropertyId,
                    UIA_IsOffscreenPropertyId,
                    UIA_NativeWindowHandlePropertyId,
                ] {
                    request.AddProperty(property)?;
                }
                Ok(request)
            }
        }
    }

    #[cfg(test)]
    struct BoundaryDiagnostics {
        native: Vec<[i32; 4]>,
        cached: Vec<String>,
    }

    #[cfg(test)]
    impl BoundaryDiagnostics {
        fn new(native: &[[i32; 4]]) -> Self {
            Self {
                native: native.to_vec(),
                cached: Vec::new(),
            }
        }

        unsafe fn record(
            &mut self,
            index: i32,
            element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
            bar: RECT,
        ) -> windows::core::Result<()> {
            let class: String = element
                .CachedClassName()?
                .to_string()
                .chars()
                .take(256)
                .collect();
            let id: String = element
                .CachedAutomationId()?
                .to_string()
                .chars()
                .take(256)
                .collect();
            let rect = element.CachedBoundingRectangle()?;
            let offscreen = element.CachedIsOffscreen()?.as_bool();
            self.cached.push(format!(
                "{index}: class={class:?}, id={id:?}, relative_rect={:?}, offscreen={offscreen}",
                [
                    rect.left - bar.left,
                    rect.top - bar.top,
                    rect.right - bar.left,
                    rect.bottom - bar.top
                ]
            ));
            Ok(())
        }

        fn report(&self, bar: RECT, start: RECT, start_left: i32, count: i32) {
            eprintln!("UIA boundary rejection: bar={:?}, Start HWND={:?}, start_left={start_left}, cached_count={count}, native_bounds={:?}\ncached_entries:\n{}",
                [bar.left, bar.top, bar.right, bar.bottom],
                [start.left, start.top, start.right, start.bottom], self.native, self.cached.join("\n"));
        }
    }

    fn register_class() -> Result<isize, String> {
        static MODULE: OnceLock<Result<isize, String>> = OnceLock::new();
        MODULE
            .get_or_init(|| {
                let module = unsafe { GetModuleHandleW(null()) };
                if module.is_null() {
                    return Err(win_error("GetModuleHandleW"));
                }
                let name = wide(CLASS);
                let class = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: module,
                    lpszClassName: name.as_ptr(),
                    ..Default::default()
                };
                if unsafe { RegisterClassW(&raw const class) } == 0 {
                    return Err(win_error("RegisterClassW"));
                }
                Ok(module as isize)
            })
            .clone()
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct BarKey {
        hwnd: isize,
        pid: u32,
        left: i32,
        top: i32,
        width: i32,
        height: i32,
    }

    type CachedSlotBounds = (
        BarKey,
        Option<[i32; 4]>,
        Option<[i32; 4]>,
        Result<TaskbarSlots, String>,
    );

    struct Renderer {
        module: isize,
        parent: HWND,
        parent_pid: u32,
        start: HWND,
        tray: HWND,
        window: HWND,
        discovered: Instant,
        boundary: Option<(u64, BarKey, i32)>,
        automation: Option<Automation>,
        auto_slots: Option<CachedSlotBounds>,
        auto_retry: BoundaryRetry,
        paint: Box<Paint>,
        position: Option<(i32, i32, i32, i32)>,
        visible: bool,
        last_input: Option<Input>,
        refreshed: Instant,
        refresh_result: Result<(), String>,
        theme_checked: Instant,
    }

    impl Renderer {
        fn new(module: isize) -> Self {
            Self {
                module,
                parent: null_mut(),
                parent_pid: 0,
                start: null_mut(),
                tray: null_mut(),
                window: null_mut(),
                discovered: elapsed_anchor(DISCOVERY_INTERVAL),
                boundary: None,
                automation: None,
                auto_slots: None,
                auto_retry: BoundaryRetry::default(),
                paint: Box::new(Paint::default()),
                position: None,
                visible: false,
                last_input: None,
                refreshed: elapsed_anchor(DISCOVERY_INTERVAL),
                refresh_result: Ok(()),
                theme_checked: elapsed_anchor(UIA_INTERVAL),
            }
        }

        fn refresh(&mut self, input: &Input) -> Result<(), String> {
            if self.theme_checked.elapsed() >= UIA_INTERVAL {
                self.theme_checked = Instant::now();
                self.paint.refresh_theme();
            }
            if self.last_input.as_ref() == Some(input)
                && self.refreshed.elapsed() < Duration::from_millis(u64::from(TICK_MS))
            {
                if self.paint.dirty && self.visible && !self.window.is_null() {
                    unsafe {
                        InvalidateRect(self.window, null(), 0);
                    }
                    self.paint.dirty = false;
                }
                return self.refresh_result.clone();
            }
            self.refreshed = Instant::now();
            self.last_input = Some(input.clone());
            let result = self.refresh_now(input);
            self.refresh_result.clone_from(&result);
            result
        }

        #[allow(
            clippy::too_many_lines,
            reason = "Keep discovery, fail-closed placement and HWND mutation in one auditable sequence"
        )]
        fn refresh_now(&mut self, input: &Input) -> Result<(), String> {
            if self.discovered.elapsed() >= DISCOVERY_INTERVAL
                || unsafe { IsWindow(self.parent) } == 0
            {
                self.discovered = Instant::now();
                let parent = unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), null()) };
                let mut pid = 0;
                if !parent.is_null() {
                    unsafe {
                        GetWindowThreadProcessId(parent, &raw mut pid);
                    }
                }
                if parent != self.parent || pid != self.parent_pid {
                    self.destroy_window();
                    self.parent = parent;
                    self.parent_pid = pid;
                    self.start = null_mut();
                    self.tray = null_mut();
                }
            }
            if self.parent.is_null() {
                return Err("Windows taskbar was not found; retrying".to_owned());
            }
            let bar = window_rect(self.parent)?;
            let key = BarKey {
                hwnd: self.parent as isize,
                pid: self.parent_pid,
                left: bar.left,
                top: bar.top,
                width: bar.right - bar.left,
                height: bar.bottom - bar.top,
            };
            if key.width < key.height || key.height <= 2 {
                return Err("Native renderer requires a visible horizontal taskbar".to_owned());
            }
            if unsafe { IsWindowVisible(self.parent) } == 0
                || input.text.trim().is_empty()
                || taskbar_obscured_or_autohidden(self.parent, self.window, bar)
            {
                self.hide();
                return Ok(());
            }
            if self.start.is_null() || unsafe { IsWindow(self.start) } == 0 {
                self.start = find_descendant(self.parent, &["Start"]).unwrap_or(null_mut());
            }
            if self.tray.is_null() || unsafe { IsWindow(self.tray) } == 0 {
                self.tray = find_descendant(self.parent, &["TrayNotifyWnd"]).unwrap_or(null_mut());
            }
            let start_rect = (!self.start.is_null())
                .then(|| window_rect(self.start).ok())
                .flatten();
            let tray_rect = (!self.tray.is_null())
                .then(|| window_rect(self.tray).ok())
                .flatten();
            let dpi = i32::try_from(unsafe { GetDpiForWindow(self.parent) })
                .ok()
                .filter(|dpi| *dpi > 0 && *dpi <= 768)
                .ok_or_else(|| "Cannot verify taskbar DPI; renderer hidden".to_owned())?;
            let gap = (8 * dpi / 96).max(1);
            let placement = if let Some(right) = input.left_exclusion {
                let start_rect = start_rect.ok_or_else(|| {
                    "Cannot verify the Start button boundary; native renderer hidden".to_owned()
                })?;
                let start_left = start_rect.left - bar.left;
                if self
                    .boundary
                    .is_none_or(|(revision, _, _)| revision != input.boundary_revision)
                {
                    self.boundary = Some((input.boundary_revision, key, right));
                }
                let (_, anchor, right) = self.boundary.expect("boundary just installed");
                if anchor != key {
                    return Err(
                        "Taskbar geometry changed; refresh set_left_exclusion() with measured bounds"
                            .to_owned(),
                    );
                }
                self.paint.prepare(&input.text, dpi)?;
                let width = input.width.max(self.paint.content_width);
                let height = input.height.max(self.paint.content_height);
                native_placement(key.width, key.height, start_left, right, width, height, gap)
                    .ok_or_else(|| {
                        "Not enough verified space between weather and Start; native renderer hidden"
                            .to_owned()
                    })?
            } else {
                self.boundary = None;
                let slots = self.auto_slots(key, bar, start_rect, tray_rect)?;
                self.select_slot_and_prepare(input, dpi, key, slots, gap)?
            };
            if placement.3 < self.paint.content_height {
                return Err(
                    "Taskbar is too short for the requested text rows; native renderer hidden"
                        .to_owned(),
                );
            }
            let screen_placement = (
                bar.left + placement.0,
                bar.top + placement.1,
                placement.2,
                placement.3,
            );
            if self.window.is_null() || unsafe { IsWindow(self.window) } == 0 {
                self.window = null_mut();
                let class = wide(CLASS);
                let window = unsafe {
                    CreateWindowExW(
                        WS_EX_TOPMOST
                            | WS_EX_TOOLWINDOW
                            | WS_EX_NOACTIVATE
                            | WS_EX_LAYERED
                            | WS_EX_TRANSPARENT,
                        class.as_ptr(),
                        null(),
                        WS_POPUP | WS_CLIPSIBLINGS,
                        screen_placement.0,
                        screen_placement.1,
                        screen_placement.2,
                        screen_placement.3,
                        null_mut(),
                        null_mut(),
                        self.module as _,
                        (&raw mut *self.paint).cast(),
                    )
                };
                if window.is_null() {
                    return Err(win_error("CreateWindowExW(taskbar overlay)"));
                }
                unsafe {
                    SetLayeredWindowAttributes(window, 0, 250, LWA_ALPHA);
                }
                self.window = window;
                self.position = None;
                self.visible = false;
            }
            if self.position != Some(screen_placement) || !is_above_window(self.window, self.parent)
            {
                position_docked_window(self.window, screen_placement)?;
                self.position = Some(screen_placement);
                self.paint.dirty = true;
            }
            if !self.visible {
                unsafe {
                    ShowWindow(self.window, SW_SHOWNOACTIVATE);
                }
                self.visible = true;
                self.paint.dirty = true;
            }
            if self.paint.dirty {
                unsafe {
                    InvalidateRect(self.window, null(), 0);
                }
                self.paint.dirty = false;
            }
            Ok(())
        }

        fn select_slot_and_prepare(
            &mut self,
            input: &Input,
            dpi: i32,
            key: BarKey,
            slots: TaskbarSlots,
            gap: i32,
        ) -> Result<(i32, i32, i32, i32), String> {
            let total_cols = input
                .text
                .lines()
                .take(2)
                .map(|line| line.split('\t').take(16).count())
                .max()
                .unwrap_or(0);
            let left_slot = slots
                .left_slot
                .filter(|&(left, right)| right - left - 2 * gap > 0);
            let right_slot = slots
                .right_slot
                .filter(|&(left, right)| right - left - 2 * gap > 0);
            let (slot_left, slot_right) = match (left_slot, right_slot) {
                (Some((l_left, l_right)), Some((r_left, r_right))) => {
                    let l_avail = l_right - l_left - 2 * gap;
                    let r_avail = r_right - r_left - 2 * gap;
                    let left_fit = self.paint.prepare_fitting(&input.text, dpi, l_avail);
                    if left_fit == Ok(total_cols) {
                        (l_left, l_right)
                    } else {
                        let right_fit = self.paint.prepare_fitting(&input.text, dpi, r_avail);
                        match (&left_fit, &right_fit) {
                            (Ok(lc), Ok(rc)) if *rc > *lc || (*rc == *lc && r_avail > l_avail) => {
                                (r_left, r_right)
                            }
                            (Err(_), Ok(_)) => (r_left, r_right),
                            (Ok(_), _) => {
                                self.paint.prepare_fitting(&input.text, dpi, l_avail)?;
                                (l_left, l_right)
                            }
                            (Err(err_l), Err(_)) => return Err(err_l.clone()),
                        }
                    }
                }
                (Some((l_left, l_right)), None) => {
                    self.paint
                        .prepare_fitting(&input.text, dpi, l_right - l_left - 2 * gap)?;
                    (l_left, l_right)
                }
                (None, Some((r_left, r_right))) => {
                    self.paint
                        .prepare_fitting(&input.text, dpi, r_right - r_left - 2 * gap)?;
                    (r_left, r_right)
                }
                (None, None) => {
                    return Err(
                        "Not enough verified taskbar space; native renderer hidden".to_owned()
                    );
                }
            };
            let width = input.width.max(self.paint.content_width);
            let height = input.height.max(self.paint.content_height);
            native_placement(
                key.width, key.height, slot_right, slot_left, width, height, gap,
            )
            .ok_or_else(|| "Not enough verified taskbar space; native renderer hidden".to_owned())
        }

        fn auto_slots(
            &mut self,
            key: BarKey,
            bar: RECT,
            start: Option<RECT>,
            tray: Option<RECT>,
        ) -> Result<TaskbarSlots, String> {
            let start_bounds = start.map(|r| [r.left, r.top, r.right, r.bottom]);
            let tray_bounds = tray.map(|r| [r.left, r.top, r.right, r.bottom]);
            let matches =
                self.auto_slots
                    .as_ref()
                    .is_some_and(|(anchor, cached_start, cached_tray, _)| {
                        *anchor == key
                            && *cached_start == start_bounds
                            && *cached_tray == tray_bounds
                    });
            if self.auto_retry.query_due(Instant::now(), matches) {
                let result = (|| {
                    if self.automation.is_none() {
                        self.automation = Some(Automation::new()?);
                    }
                    self.automation
                        .as_ref()
                        .expect("automation installed")
                        .discover_slots(self.parent, self.window, bar, start, tray)
                })();
                self.auto_retry.record(Instant::now(), result.is_ok());
                self.auto_slots = Some((key, start_bounds, tray_bounds, result));
            }
            let (anchor, cached_start, cached_tray, result) =
                self.auto_slots.as_ref().expect("UIA cache installed");
            if !self.auto_retry.usable(
                *anchor == key && *cached_start == start_bounds && *cached_tray == tray_bounds,
            ) {
                if let Err(error) = result {
                    return Err(error.clone());
                }
                return Err(
                    "Taskbar geometry changed; renderer hidden until a verified UIA retry"
                        .to_owned(),
                );
            }
            result.clone()
        }

        fn hide(&mut self) {
            if !self.window.is_null() && self.visible {
                unsafe {
                    ShowWindow(self.window, SW_HIDE);
                }
            }
            self.visible = false;
        }
        fn destroy_window(&mut self) {
            if !self.window.is_null() && unsafe { IsWindow(self.window) } != 0 {
                unsafe {
                    DestroyWindow(self.window);
                }
            }
            self.window = null_mut();
            self.visible = false;
            self.position = None;
        }
    }
    impl Drop for Renderer {
        fn drop(&mut self) {
            self.destroy_window();
        }
    }

    fn position_docked_window(window: HWND, placement: (i32, i32, i32, i32)) -> Result<(), String> {
        if unsafe {
            SetWindowPos(
                window,
                HWND_TOPMOST,
                placement.0,
                placement.1,
                placement.2,
                placement.3,
                SWP_NOACTIVATE,
            )
        } == 0
        {
            return Err(win_error("SetWindowPos(taskbar overlay)"));
        }
        Ok(())
    }

    #[cfg(test)]
    fn position_window(window: HWND, placement: (i32, i32, i32, i32)) -> Result<(), String> {
        use windows_sys::Win32::UI::WindowsAndMessaging::HWND_TOP;
        if unsafe {
            SetWindowPos(
                window,
                HWND_TOP,
                placement.0,
                placement.1,
                placement.2,
                placement.3,
                SWP_NOACTIVATE,
            )
        } == 0
        {
            return Err(win_error("SetWindowPos(native child)"));
        }
        Ok(())
    }

    #[derive(Default)]
    pub(super) struct Paint {
        text: String,
        cells: Vec<Vec<Vec<u16>>>,
        widths: Vec<i32>,
        font: HFONT,
        dpi: i32,
        pub(super) fitted_cols: usize,
        use_templates: bool,
        line_height: i32,
        padding: i32,
        pub(super) content_width: i32,
        content_height: i32,
        bg_color: u32,
        text_color: u32,
        theme_initialized: bool,
        dirty: bool,
    }

    fn widgets_enabled_in_registry() -> bool {
        let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced");
        let value = wide("TaskbarDa");
        let mut hkey = null_mut();
        if unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_READ,
                &raw mut hkey,
            )
        } == 0
        {
            let mut reg_type = 0_u32;
            let mut data = 0_u32;
            let mut size = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4);
            let status = unsafe {
                RegQueryValueExW(
                    hkey,
                    value.as_ptr(),
                    null(),
                    &raw mut reg_type,
                    (&raw mut data).cast(),
                    &raw mut size,
                )
            };
            unsafe {
                RegCloseKey(hkey);
            }
            if status == 0 && reg_type == REG_DWORD && size == 4 {
                return data != 0;
            }
        }
        true
    }

    fn query_taskbar_colors() -> (u32, u32) {
        let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        let value = wide("SystemUsesLightTheme");
        let mut hkey = null_mut();
        if unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_READ,
                &raw mut hkey,
            )
        } == 0
        {
            let mut reg_type = 0_u32;
            let mut data = 0_u32;
            let mut size = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4);
            let status = unsafe {
                RegQueryValueExW(
                    hkey,
                    value.as_ptr(),
                    null(),
                    &raw mut reg_type,
                    (&raw mut data).cast(),
                    &raw mut size,
                )
            };
            unsafe {
                RegCloseKey(hkey);
            }
            if status == 0 && reg_type == REG_DWORD && size == 4 {
                return if data != 0 {
                    (0x00F3_F3F3, 0x001A_1A1A)
                } else {
                    (0x0020_2020, 0x00F3_F3F3)
                };
            }
        }
        unsafe { (GetSysColor(COLOR_BTNFACE), GetSysColor(COLOR_BTNTEXT)) }
    }

    impl Paint {
        fn refresh_theme(&mut self) {
            let (bg, text) = query_taskbar_colors();
            if !self.theme_initialized || self.bg_color != bg || self.text_color != text {
                self.bg_color = bg;
                self.text_color = text;
                self.theme_initialized = true;
                self.dirty = true;
            }
        }

        fn ensure_font(&mut self, dpi: i32) -> Result<(), String> {
            if self.dpi == dpi {
                return Ok(());
            }
            let face = wide("Segoe UI");
            let font = unsafe {
                CreateFontW(
                    -12 * dpi / 96,
                    0,
                    0,
                    0,
                    400,
                    0,
                    0,
                    0,
                    1,
                    0,
                    0,
                    5,
                    0,
                    face.as_ptr(),
                )
            };
            if font.is_null() {
                return Err(win_error("CreateFontW"));
            }
            if !self.font.is_null() {
                unsafe {
                    DeleteObject(self.font);
                }
            }
            self.font = font;
            self.dpi = dpi;
            Ok(())
        }

        fn prepare(&mut self, text: &str, dpi: i32) -> Result<(), String> {
            let padding = (4 * dpi / 96).max(1);
            self.prepare_with_options(text, dpi, 16, true, padding)
        }

        pub(super) fn prepare_fitting(
            &mut self,
            text: &str,
            dpi: i32,
            max_width: i32,
        ) -> Result<usize, String> {
            let total_cols = text
                .lines()
                .take(2)
                .map(|line| line.split('\t').take(16).count())
                .max()
                .unwrap_or(0);
            if total_cols == 0 {
                return Err("Empty taskbar metrics text".to_owned());
            }
            let normal_pad = (4 * dpi / 96).max(1);
            self.prepare_with_options(text, dpi, total_cols, true, normal_pad)?;
            if self.content_width <= max_width {
                return Ok(total_cols);
            }
            let tight_pad = (3 * dpi / 96).max(1);
            self.prepare_with_options(text, dpi, total_cols, false, tight_pad)?;
            if self.content_width <= max_width {
                return Ok(total_cols);
            }
            for cols in (1..total_cols).rev() {
                self.prepare_with_options(text, dpi, cols, false, tight_pad)?;
                if self.content_width <= max_width {
                    return Ok(cols);
                }
            }
            Err(format!(
                "Not enough verified taskbar space ({max_width}px available, {}px needed); native renderer hidden",
                self.content_width
            ))
        }

        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "Public input is bounded to 2048 scalars (4096 UTF-16 units), 16 columns, and two rows; GDI takes i32 counts"
        )]
        fn prepare_with_options(
            &mut self,
            text: &str,
            dpi: i32,
            max_cols: usize,
            use_templates: bool,
            padding: i32,
        ) -> Result<(), String> {
            if !self.theme_initialized {
                self.refresh_theme();
            }
            if self.text == text
                && self.dpi == dpi
                && self.fitted_cols == max_cols
                && self.use_templates == use_templates
                && self.padding == padding
            {
                return Ok(());
            }
            self.ensure_font(dpi)?;
            let dc = unsafe { GetDC(null_mut()) };
            if dc.is_null() {
                return Err(win_error("GetDC(text measurement)"));
            }
            let old = unsafe { SelectObject(dc, self.font) };
            let raw_rows: Vec<Vec<&str>> = text
                .lines()
                .take(2)
                .map(|line| line.split('\t').take(16).take(max_cols).collect())
                .collect();
            let cells: Vec<Vec<Vec<u16>>> = raw_rows
                .iter()
                .map(|row| row.iter().map(|cell| wide(cell)).collect())
                .collect();
            let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
            let mut widths = vec![0; columns];
            let mut line_height = 0;
            let mut valid = true;
            let slack = if use_templates {
                0
            } else {
                (2 * dpi / 96).max(1)
            };
            for (row_idx, row) in cells.iter().enumerate() {
                for (column, cell) in row.iter().enumerate() {
                    let mut size = SIZE::default();
                    valid &= unsafe {
                        GetTextExtentPoint32W(
                            dc,
                            cell.as_ptr(),
                            (cell.len() - 1) as i32,
                            &raw mut size,
                        )
                    } != 0;
                    widths[column] = widths[column].max(size.cx + slack);
                    line_height = line_height.max(size.cy);
                    if use_templates {
                        let template = cell_width_template(raw_rows[row_idx][column]);
                        if !template.is_empty() {
                            let tmpl = wide(template);
                            let mut tmpl_size = SIZE::default();
                            if unsafe {
                                GetTextExtentPoint32W(
                                    dc,
                                    tmpl.as_ptr(),
                                    (tmpl.len() - 1) as i32,
                                    &raw mut tmpl_size,
                                )
                            } != 0
                            {
                                widths[column] = widths[column].max(tmpl_size.cx);
                            }
                        }
                    }
                }
            }
            unsafe {
                SelectObject(dc, old);
                ReleaseDC(null_mut(), dc);
            }
            if !valid {
                return Err("Unable to measure native metric text".to_owned());
            }
            self.padding = padding;
            self.fitted_cols = max_cols;
            self.use_templates = use_templates;
            self.line_height = line_height.max(12 * dpi / 96) + self.padding;
            self.content_width =
                widths.iter().sum::<i32>() + (columns as i32 + 1) * self.padding * 2;
            self.content_height = self.line_height * cells.len() as i32 + self.padding * 2;
            text.clone_into(&mut self.text);
            self.cells = cells;
            self.widths = widths;
            self.dirty = true;
            Ok(())
        }

        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "Prepared cells contain at most 4096 UTF-16 units and two rows; native GDI counts are i32"
        )]
        unsafe fn draw(&self, window: HWND) {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(window, &raw mut paint);
            if dc.is_null() {
                return;
            }
            let mut client = RECT::default();
            GetClientRect(window, &raw mut client);
            let bg = if self.theme_initialized {
                self.bg_color
            } else {
                GetSysColor(COLOR_BTNFACE)
            };
            let fg = if self.theme_initialized {
                self.text_color
            } else {
                GetSysColor(COLOR_BTNTEXT)
            };
            SetDCBrushColor(dc, bg);
            FillRect(dc, &raw const client, GetStockObject(DC_BRUSH));
            SetBkMode(dc, TRANSPARENT.cast_signed());
            SetTextColor(dc, fg);
            let old = SelectObject(dc, self.font);
            let top = ((client.bottom - self.content_height) / 2).max(0) + self.padding;
            for (row_number, row) in self.cells.iter().enumerate() {
                let mut x = self.padding;
                for (column, cell) in row.iter().enumerate() {
                    let mut rect = RECT {
                        left: x,
                        top: top + row_number as i32 * self.line_height,
                        right: (x + self.widths[column] + self.padding).min(client.right),
                        bottom: (top + (row_number as i32 + 1) * self.line_height)
                            .min(client.bottom),
                    };
                    DrawTextW(
                        dc,
                        cell.as_ptr(),
                        (cell.len() - 1) as i32,
                        &raw mut rect,
                        DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
                    );
                    x += self.widths[column] + self.padding * 2;
                }
            }
            SelectObject(dc, old);
            EndPaint(window, &raw const paint);
        }
    }
    impl Drop for Paint {
        fn drop(&mut self) {
            if !self.font.is_null() {
                unsafe {
                    DeleteObject(self.font);
                }
            }
        }
    }

    #[allow(
        clippy::cast_possible_wrap,
        reason = "MA_NOACTIVATE is the Win32 constant 3, representable by LRESULT on both targets"
    )]
    unsafe extern "system" fn window_proc(
        window: HWND,
        message: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> LRESULT {
        match message {
            WM_NCCREATE => {
                let create = &*(l as *const CREATESTRUCTW);
                SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
                1
            }
            WM_PAINT => {
                let state = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Paint;
                if state.is_null() {
                    return DefWindowProcW(window, message, w, l);
                }
                (*state).draw(window);
                0
            }
            WM_SETTINGCHANGE => {
                InvalidateRect(window, null(), 0);
                DefWindowProcW(window, message, w, l)
            }
            WM_ERASEBKGND => 1,
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            WM_NCDESTROY => {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                DefWindowProcW(window, message, w, l)
            }
            _ => DefWindowProcW(window, message, w, l),
        }
    }

    fn window_class_name(window: HWND) -> Option<String> {
        let mut name = [0_u16; 256];
        let length = unsafe { GetClassNameW(window, name.as_mut_ptr(), 256) };
        let length = usize::try_from(length).ok()?;
        (length > 0).then(|| String::from_utf16_lossy(&name[..length]))
    }

    fn is_above_window(top_candidate: HWND, below: HWND) -> bool {
        if top_candidate.is_null() || below.is_null() {
            return false;
        }
        let mut cur = unsafe { GetWindow(below, GW_HWNDPREV) };
        let mut steps = 0_usize;
        while !cur.is_null() && steps < 128 {
            if cur == top_candidate {
                return true;
            }
            cur = unsafe { GetWindow(cur, GW_HWNDPREV) };
            steps += 1;
        }
        false
    }

    fn taskbar_obscured_or_autohidden(parent: HWND, own: HWND, bar: RECT) -> bool {
        let monitor = unsafe { MonitorFromWindow(parent, MONITOR_DEFAULTTONEAREST) };
        if !monitor.is_null() {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "MONITORINFO size fits within u32"
            )]
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if unsafe { GetMonitorInfoW(monitor, &raw mut info) } != 0 {
                let m = info.rcMonitor;
                if bar.bottom <= m.top + 4
                    || bar.top >= m.bottom - 4
                    || bar.right <= m.left + 4
                    || bar.left >= m.right - 4
                {
                    return true;
                }
                let fg = unsafe { GetForegroundWindow() };
                if !fg.is_null() && fg != parent && fg != own && unsafe { IsWindowVisible(fg) } != 0
                {
                    let is_shell = window_class_name(fg).is_some_and(|cls| {
                        matches!(
                            cls.as_str(),
                            "Progman"
                                | "WorkerW"
                                | "Shell_TrayWnd"
                                | "SecondaryTrayWnd"
                                | "XamlExplorerHostIslandWindow"
                        )
                    });
                    if !is_shell {
                        if let Ok(fg_rect) = window_rect(fg) {
                            if fg_rect.left <= m.left
                                && fg_rect.top <= m.top
                                && fg_rect.right >= m.right
                                && fg_rect.bottom >= m.bottom
                            {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    fn find_descendant(parent: HWND, classes: &[&str]) -> Option<HWND> {
        struct Search<'a> {
            classes: &'a [&'a str],
            found: HWND,
        }
        unsafe extern "system" fn visit(window: HWND, parameter: LPARAM) -> i32 {
            let search = &mut *(parameter as *mut Search<'_>);
            if window_class_name(window).is_some_and(|cls| search.classes.iter().any(|&c| c == cls))
            {
                search.found = window;
                return 0;
            }
            1
        }
        let mut search = Search {
            classes,
            found: null_mut(),
        };
        unsafe {
            EnumChildWindows(parent, Some(visit), (&raw mut search) as isize);
        }
        (!search.found.is_null()).then_some(search.found)
    }

    // HWND-only enumeration also reserves native controls outside the XAML subtree.
    fn visible_native_bounds(parent: HWND, own: HWND, bar: RECT) -> Result<Vec<[i32; 4]>, String> {
        struct Scan {
            own: HWND,
            bar_w: i32,
            rects: Vec<[i32; 4]>,
            failed: bool,
            count: usize,
        }
        unsafe extern "system" fn visit(window: HWND, parameter: LPARAM) -> i32 {
            let scan = &mut *(parameter as *mut Scan);
            scan.count += 1;
            if scan.count > 1024 {
                scan.failed = true;
                return 0;
            }
            if window == scan.own || IsWindowVisible(window) == 0 {
                return 1;
            }
            if window_class_name(window).is_some_and(|cls| is_container_class(&cls)) {
                return 1;
            }
            let mut rect = RECT::default();
            if GetWindowRect(window, &raw mut rect) == 0 {
                scan.failed = true;
                return 0;
            }
            if rect.right - rect.left >= scan.bar_w * 3 / 4 {
                return 1;
            }
            scan.rects
                .push([rect.left, rect.top, rect.right, rect.bottom]);
            1
        }
        let mut scan = Scan {
            own,
            bar_w: bar.right - bar.left,
            rects: Vec::new(),
            failed: false,
            count: 0,
        };
        unsafe {
            EnumChildWindows(parent, Some(visit), (&raw mut scan) as isize);
        }
        if scan.failed {
            return Err("Unable to verify all native taskbar child bounds".to_owned());
        }
        for rect in &mut scan.rects {
            rect[0] -= bar.left;
            rect[1] -= bar.top;
            rect[2] -= bar.left;
            rect[3] -= bar.top;
        }
        Ok(scan.rects)
    }

    fn window_rect(window: HWND) -> Result<RECT, String> {
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(window, &raw mut rect) } == 0 {
            return Err(win_error("GetWindowRect"));
        }
        Ok(rect)
    }
    fn win_error(operation: &str) -> String {
        format!("{operation} failed (Win32 error {})", unsafe {
            GetLastError()
        })
    }

    #[cfg(test)]
    mod live_tests {
        use super::*;
        use crate::TaskbarMode;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetParent, GWL_EXSTYLE, GWL_STYLE, WS_CHILD,
        };

        fn bounds(rect: RECT) -> [i32; 4] {
            [rect.left, rect.top, rect.right, rect.bottom]
        }

        fn find_top_level_native_window(target_pid: u32) -> Option<HWND> {
            struct Search {
                target_pid: u32,
                found: HWND,
            }
            unsafe extern "system" fn visit(window: HWND, parameter: LPARAM) -> i32 {
                let search = &mut *(parameter as *mut Search);
                if window_class_name(window).as_deref() == Some(CLASS)
                    && IsWindowVisible(window) != 0
                {
                    let mut pid = 0;
                    GetWindowThreadProcessId(window, &raw mut pid);
                    if search.target_pid == 0 || pid == search.target_pid {
                        search.found = window;
                        return 0;
                    }
                }
                1
            }
            let mut search = Search {
                target_pid,
                found: null_mut(),
            };
            unsafe {
                EnumWindows(Some(visit), (&raw mut search) as isize);
            }
            (!search.found.is_null()).then_some(search.found)
        }

        #[test]
        #[ignore = "read-only diagnostic of native taskbar windows on the interactive desktop"]
        fn live_inspect_installed_taskbar() {
            use windows_sys::Win32::UI::HiDpi::{
                GetAwarenessFromDpiAwarenessContext, GetWindowDpiAwarenessContext,
            };
            unsafe fn report_inner(window: HWND, parameter: LPARAM) -> i32 {
                let name = window_class_name(window).unwrap_or_default();
                let mut pid = 0;
                let tid = GetWindowThreadProcessId(window, &raw mut pid);
                let mut client = RECT::default();
                GetClientRect(window, &raw mut client);
                eprintln!("HWND={window:?} class={name} pid={pid} tid={tid} parent={:?} prev={:?} visible={} rect={:?} client={:?} dpi={} awareness={} style={:x} ex={:x}",
                    GetParent(window), GetWindow(window, GW_HWNDPREV), IsWindowVisible(window),
                    window_rect(window).map(bounds), bounds(client), GetDpiForWindow(window),
                    GetAwarenessFromDpiAwarenessContext(GetWindowDpiAwarenessContext(window)),
                GetWindowLongPtrW(window, GWL_STYLE), GetWindowLongPtrW(window, GWL_EXSTYLE));
                if name == CLASS && IsWindowVisible(window) != 0 {
                    let inspection = &mut *(parameter as *mut (usize, bool, bool, HWND));
                    inspection.0 += 1;
                    inspection.1 |= !is_above_window(window, inspection.3);
                }
                1
            }
            unsafe extern "system" fn report(window: HWND, parameter: LPARAM) -> i32 {
                // Keep diagnostic I/O panics from unwinding through Win32's callback ABI.
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    report_inner(window, parameter)
                }))
                .unwrap_or_else(|_| {
                    (*(parameter as *mut (usize, bool, bool, HWND))).2 = true;
                    0
                })
            }
            let _dpi = DpiContext::new().unwrap();
            let parent = unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), null()) };
            assert!(!parent.is_null());
            let mut inspection = (0_usize, false, false, parent);
            let parameter = (&raw mut inspection) as LPARAM;
            unsafe {
                report(parent, parameter);
                EnumChildWindows(parent, Some(report), parameter);
                EnumWindows(Some(report), parameter);
            }
            assert!(!inspection.2, "native inspection callback failed");
            eprintln!(
                "visible monitors={}, occluded={}",
                inspection.0, inspection.1
            );
            assert!(
                !inspection.1,
                "installed monitor is below the taskbar or occluded"
            );
        }

        fn assert_docked_taskbar_window(window: HWND, bar: HWND) {
            assert!(!window.is_null(), "native renderer did not create a window");
            assert!(
                is_above_window(window, bar),
                "native monitor is below the Windows taskbar"
            );
            assert_eq!(window_class_name(window).as_deref(), Some(CLASS));
            let style = unsafe { GetWindowLongPtrW(window, GWL_STYLE) }.cast_unsigned();
            let ex_style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) }.cast_unsigned();
            assert_ne!(style & usize::try_from(WS_POPUP).unwrap(), 0);
            assert_ne!(ex_style & usize::try_from(WS_EX_TOPMOST).unwrap(), 0);
            assert_ne!(ex_style & usize::try_from(WS_EX_LAYERED).unwrap(), 0);
        }

        #[test]
        #[ignore = "creates this renderer's HWND on the current interactive Windows taskbar"]
        fn live_seven_metric_renderer_fits_verified_space() {
            let _dpi = DpiContext::new().unwrap();
            let _apartment = ComApartment::new().unwrap();
            let mut renderer = Renderer::new(register_class().unwrap());
            let input = Input {
                text:
                    "D 999.9K/s\tR 999.9M/s\tRAM 99.9G\t24h 59m\nU 999.9K/s\tW 999.9M/s\tNET 999.9G"
                        .to_owned(),
                width: 1,
                height: 1,
                left_exclusion: None,
                boundary_revision: 0,
            };
            let deadline = Instant::now() + Duration::from_secs(8);
            let result = loop {
                let result = renderer.refresh(&input);
                if result.is_ok() || Instant::now() >= deadline {
                    break result;
                }
                thread::sleep(Duration::from_millis(u64::from(TICK_MS)));
            };
            eprintln!(
                "seven metrics: result={result:?}, content={}x{}, placement={:?}",
                renderer.paint.content_width, renderer.paint.content_height, renderer.position
            );
            result.expect(
                "seven compact metrics must fit or compact into the verified taskbar space",
            );
            assert!(renderer.visible);
        }

        #[test]
        #[ignore = "creates this renderer's HWND on the current interactive Windows taskbar"]
        fn live_native_renderer_parent_bounds_and_two_rows() {
            let _dpi = DpiContext::new().unwrap();
            let _apartment = ComApartment::new().unwrap();
            let mut renderer = Renderer::new(register_class().unwrap());
            let input = Input {
                text: "Down 1 MB/s\tUp 2 KB/s\nRAM 40%\nthird row must not render".to_owned(),
                width: 120,
                height: 1,
                left_exclusion: None,
                boundary_revision: 0,
            };
            let deadline = Instant::now() + Duration::from_secs(8);
            let result = loop {
                let result = renderer.refresh(&input);
                if result.is_ok() || Instant::now() >= deadline {
                    break result;
                }
                thread::sleep(Duration::from_millis(u64::from(TICK_MS)));
            };
            eprintln!("native refresh: {result:?}");
            if !renderer.parent.is_null() {
                eprintln!(
                    "taskbar={:?}, dpi={}",
                    bounds(window_rect(renderer.parent).unwrap()),
                    unsafe { GetDpiForWindow(renderer.parent) }
                );
            }
            result.expect("desktop bounds must be verified, never guessed for acceptance");
            assert_docked_taskbar_window(renderer.window, renderer.parent);
            assert_eq!(renderer.paint.cells.len(), 2);
            assert!(renderer.visible);
            assert_ne!(unsafe { IsWindowVisible(renderer.window) }, 0);
            let bar = window_rect(renderer.parent).unwrap();
            let own = window_rect(renderer.window).unwrap();
            let dpi = i32::try_from(unsafe { GetDpiForWindow(renderer.parent) }).unwrap();
            let gap = (8 * dpi / 96).max(1);
            assert!(own.left >= bar.left && own.right <= bar.right);
            assert!(own.top >= bar.top && own.bottom <= bar.bottom);
            assert!(own.bottom - own.top >= renderer.paint.content_height);
            eprintln!(
                "docked taskbar window={:?}, bar={:?}, gap={}, rows={}",
                bounds(own),
                bounds(bar),
                gap,
                renderer.paint.cells.len()
            );
            let window = renderer.window;
            drop(renderer);
            assert_eq!(
                unsafe { IsWindow(window) },
                0,
                "renderer drop leaked its HWND"
            );
        }

        struct OverlayGuard(HWND);
        impl Drop for OverlayGuard {
            fn drop(&mut self) {
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }

        #[test]
        fn native_position_recovers_sibling_occlusion_without_activation() {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetForegroundWindow, GetWindow, GW_HWNDPREV, HWND_TOP,
            };
            let _dpi = DpiContext::new().unwrap();
            let create = |parent, style| {
                let window = unsafe {
                    CreateWindowExW(
                        WS_EX_NOACTIVATE,
                        wide("STATIC").as_ptr(),
                        null(),
                        style,
                        0,
                        0,
                        400,
                        60,
                        parent,
                        null_mut(),
                        GetModuleHandleW(null()),
                        null(),
                    )
                };
                assert!(!window.is_null());
                OverlayGuard(window)
            };
            let parent = create(null_mut(), 0);
            let cover = create(parent.0, WS_CHILD);
            let child = create(parent.0, WS_CHILD);
            let foreground = unsafe { GetForegroundWindow() };
            assert_eq!(unsafe { GetWindow(child.0, GW_HWNDPREV) }, cover.0);
            for _ in 0..2 {
                position_window(child.0, (40, 5, 160, 40)).unwrap();
                assert!(
                    unsafe { GetWindow(child.0, GW_HWNDPREV) }.is_null(),
                    "monitor is still below the full-size composition sibling"
                );
                assert_eq!(unsafe { GetParent(child.0) }, parent.0);
                assert_eq!(unsafe { GetForegroundWindow() }, foreground);
                let mut client = RECT::default();
                unsafe {
                    GetClientRect(child.0, &raw mut client);
                }
                assert_eq!(bounds(client), [0, 0, 160, 40]);
                unsafe {
                    SetWindowPos(cover.0, HWND_TOP, 0, 0, 400, 60, SWP_NOACTIVATE);
                }
            }
        }

        #[test]
        #[ignore = "runs eight native enable/disable cycles on the current Windows desktop"]
        fn live_enable_disable_finishes_without_parenting_overlay() {
            let overlay = OverlayGuard(unsafe {
                CreateWindowExW(
                    WS_EX_NOACTIVATE,
                    wide("STATIC").as_ptr(),
                    null(),
                    0,
                    20,
                    20,
                    160,
                    80,
                    null_mut(),
                    null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                )
            });
            assert!(
                !overlay.0.is_null(),
                "could not create untouched overlay sentinel"
            );
            let original = bounds(window_rect(overlay.0).unwrap());
            let style = unsafe { GetWindowLongPtrW(overlay.0, GWL_STYLE) };
            let mut mode = TaskbarMode::default();
            mode.update("Down 1 MB/s\tUp 2 KB/s\nRAM 40%\nignored");
            for cycle in 0..8 {
                let started = Instant::now();
                mode.enable(overlay.0 as isize, 120, 1).unwrap();
                let enable_elapsed = started.elapsed();
                assert!(started.elapsed() < Duration::from_secs(1));
                assert!(mode.is_enabled());
                let parent = unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), null()) };
                let deadline = Instant::now() + Duration::from_secs(8);
                let child = loop {
                    if let Some(child) = find_top_level_native_window(std::process::id()) {
                        break child;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "cycle {cycle}: no visible docked taskbar window: {:?}",
                        mode.last_error()
                    );
                    thread::sleep(Duration::from_millis(25));
                };
                assert_docked_taskbar_window(child, parent);
                if cycle == 0 {
                    thread::sleep(UIA_INTERVAL + Duration::from_secs(1));
                    assert_ne!(unsafe { IsWindowVisible(child) }, 0);
                    assert_eq!(mode.last_error(), None, "attached UIA refresh failed");
                    mode.set_left_exclusion(Some(i32::MAX)).unwrap();
                    let hidden_deadline = Instant::now() + Duration::from_secs(2);
                    while unsafe { IsWindowVisible(child) } != 0 {
                        assert!(
                            Instant::now() < hidden_deadline,
                            "unverified bounds did not hide native window"
                        );
                        thread::sleep(Duration::from_millis(10));
                    }
                    assert!(mode.last_error().is_some());
                    mode.set_left_exclusion(None).unwrap();
                    let recovery_deadline = Instant::now() + Duration::from_secs(2);
                    while unsafe { IsWindowVisible(child) } == 0 {
                        assert!(
                            Instant::now() < recovery_deadline,
                            "native detection did not recover: {:?}",
                            mode.last_error()
                        );
                        thread::sleep(Duration::from_millis(10));
                    }
                    eprintln!("attached UIA refresh survived >5s; invalid bounds hid window; verified detection recovered");
                }
                assert!(unsafe { GetParent(overlay.0) }.is_null());
                assert_eq!(bounds(window_rect(overlay.0).unwrap()), original);
                assert_eq!(unsafe { GetWindowLongPtrW(overlay.0, GWL_STYLE) }, style);
                let stopped = Instant::now();
                mode.disable();
                mode.disable();
                assert!(stopped.elapsed() < Duration::from_millis(200));
                assert!(!mode.is_enabled());
                while !mode.session.as_ref().is_none_or(Session::finished) {
                    assert!(
                        stopped.elapsed() < Duration::from_secs(2),
                        "cycle {cycle}: worker did not finish"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
                mode.disable();
                assert!(mode.session.is_none());
                assert_eq!(unsafe { IsWindow(child) }, 0);
                eprintln!("cycle {cycle}: enable={enable_elapsed:?}, cleanup={:?}, window destroyed, overlay unchanged", stopped.elapsed());
            }
        }
    }

    #[cfg(test)]
    mod lifecycle_tests {
        use super::*;
        use crate::TaskbarMode;

        // A controlled stalled worker verifies UI latency without touching Explorer.
        fn stalled_session() -> (Session, mpsc::Sender<()>) {
            let shared = Arc::new(Shared {
                input: Mutex::new(Input {
                    text: String::new(),
                    width: 200,
                    height: 40,
                    left_exclusion: None,
                    boundary_revision: 0,
                }),
                error: Mutex::new(None),
                stop: AtomicBool::new(false),
                active: AtomicBool::new(true),
                live: AtomicBool::new(true),
                attached: AtomicBool::new(false),
                has_refreshed: AtomicBool::new(false),
                thread_id: AtomicU32::new(0),
            });
            let (release, wait) = mpsc::channel();
            let state = Arc::clone(&shared);
            let worker = thread::spawn(move || {
                let _live = LiveGuard(state);
                let _ = wait.recv();
            });
            (Session { shared, worker }, release)
        }

        #[test]
        fn disable_is_nonblocking_and_reenable_rejects_a_retiring_worker() {
            for _ in 0..8 {
                let (session, release) = stalled_session();
                let shared = Arc::clone(&session.shared);
                let mut mode = TaskbarMode::default();
                mode.session = Some(session);
                assert!(mode.is_enabled());
                let started = Instant::now();
                mode.disable();
                mode.disable();
                assert!(started.elapsed() < Duration::from_millis(200));
                assert!(!mode.is_enabled());
                assert!(shared.stop.load(Ordering::Acquire));
                assert!(mode
                    .enable(0, 200, 40)
                    .unwrap_err()
                    .contains("still shutting down"));
                assert!(mode.session.is_some());
                release.send(()).unwrap();
                let deadline = Instant::now() + Duration::from_secs(2);
                while !mode.session.as_ref().unwrap().finished() && Instant::now() < deadline {
                    thread::yield_now();
                }
                assert!(mode.session.as_ref().unwrap().finished());
                mode.disable();
                assert!(mode.session.is_none());
            }
        }

        #[test]
        fn drop_detaches_stalled_worker_and_text_survives_disable() {
            let (session, release) = stalled_session();
            let shared = Arc::clone(&session.shared);
            let mut mode = TaskbarMode::default();
            mode.session = Some(session);
            mode.update("Down 1 MB/s\tRAM 40%");
            mode.disable();
            assert_eq!(mode.text, "Down 1 MB/s\tRAM 40%");
            let started = Instant::now();
            drop(mode);
            assert!(started.elapsed() < Duration::from_millis(200));
            assert!(shared.stop.load(Ordering::Acquire));
            release.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            while shared.live.load(Ordering::Acquire) && Instant::now() < deadline {
                thread::yield_now();
            }
            assert!(!shared.live.load(Ordering::Acquire));
        }

        #[test]
        fn status_reflects_disabled_starting_attached_degraded_and_unsupported() {
            let mut mode = TaskbarMode::default();
            assert_eq!(mode.status().phase, crate::taskbar::TaskbarPhase::Disabled);

            let (session, release) = stalled_session();
            let shared = Arc::clone(&session.shared);
            mode.session = Some(session);
            assert_eq!(mode.status().phase, crate::taskbar::TaskbarPhase::Starting);

            shared.has_refreshed.store(true, Ordering::Release);
            assert_eq!(mode.status().phase, crate::taskbar::TaskbarPhase::Degraded);

            shared.attached.store(true, Ordering::Release);
            assert_eq!(mode.status().phase, crate::taskbar::TaskbarPhase::Attached);

            mode.session.as_ref().unwrap().error(Some(
                "Native renderer requires a visible horizontal taskbar".into(),
            ));
            assert_eq!(
                mode.status().phase,
                crate::taskbar::TaskbarPhase::Unsupported
            );

            mode.disable();
            assert_eq!(mode.status().phase, crate::taskbar::TaskbarPhase::Disabled);
            release.send(()).unwrap();
        }

        #[test]
        fn stop_wakes_native_message_loop_immediately() {
            let mut mode = TaskbarMode::default();
            mode.enable(0, 100, 30).unwrap();
            let started = Instant::now();
            mode.shutdown_bounded(Duration::from_millis(200));
            assert!(started.elapsed() < Duration::from_millis(150));
            assert!(mode.session.is_none());
        }
    }
}

#[cfg(any(windows, test))]
fn cell_width_template(cell: &str) -> &'static str {
    if cell.starts_with("D ")
        || cell.starts_with("U ")
        || cell.starts_with("R ")
        || cell.starts_with("W ")
    {
        "W 999.9M/s"
    } else if cell.starts_with("RAM ") {
        "RAM 99.9G"
    } else if cell.starts_with("NET ") {
        "NET 999.9G"
    } else if cell.contains("h ") && cell.ends_with('m') {
        "999h 59m"
    } else {
        ""
    }
}

#[cfg(any(windows, test))]
#[derive(Default)]
struct BoundaryRetry {
    queried_at: Option<std::time::Instant>,
    due_at: Option<std::time::Instant>,
    failures: u8,
    invalid: bool,
}

#[cfg(any(windows, test))]
impl BoundaryRetry {
    fn query_due(&mut self, now: std::time::Instant, geometry_matches: bool) -> bool {
        if !geometry_matches && !self.invalid {
            if let Some(queried) = self.queried_at {
                self.invalid = true;
                // Reflow may shorten a healthy cooldown, never the 250ms query floor.
                self.due_at = Some(now.max(queried + std::time::Duration::from_millis(250)));
            }
        }
        self.due_at.is_none_or(|due| now >= due)
    }

    fn record(&mut self, now: std::time::Instant, verified: bool) {
        self.queried_at = Some(now);
        self.invalid = !verified;
        self.failures = if verified {
            0
        } else {
            self.failures.saturating_add(1).min(5)
        };
        let millis = match self.failures {
            1 => 250,
            2 => 500,
            3 => 1_000,
            4 => 2_000,
            _ => 5_000,
        };
        self.due_at = Some(now + std::time::Duration::from_millis(millis));
    }

    fn usable(&self, geometry_matches: bool) -> bool {
        geometry_matches && self.queried_at.is_some() && !self.invalid
    }
}

#[cfg(any(windows, test))]
fn native_placement(
    bar_width: i32,
    bar_height: i32,
    start_left: i32,
    weather_right: i32,
    width: i32,
    height: i32,
    gap: i32,
) -> Option<(i32, i32, i32, i32)> {
    if bar_width <= 0
        || bar_height <= 0
        || bar_width < bar_height
        || width <= 0
        || height <= 0
        || gap < 0
        || weather_right < 0
        || start_left > bar_width
        || start_left < 0
    {
        return None;
    }
    let left = weather_right.checked_add(gap)?;
    let right = start_left.checked_sub(gap)?;
    let x = right.checked_sub(width)?;
    if x < left {
        return None;
    }
    let height = height.min(bar_height);
    Some((x, (bar_height - height) / 2, width, height))
}

#[cfg(any(windows, test))]
fn normalize_text(text: &str) -> String {
    let bounded: String = text.chars().take(2048).collect();
    bounded
        .lines()
        .take(2)
        .map(|line| {
            line.split('\t')
                .take(16)
                .map(|cell| cell.chars().filter(|c| !c.is_control()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(any(windows, test))]
fn uia_host_is_container(bar: [i32; 4], host: [i32; 4], is_child: bool) -> bool {
    is_child
        && (host == bar
            || (host[0] == host[2]
                && host[1] == host[3]
                && host[0] >= bar[0]
                && host[0] <= bar[2]
                && host[1] >= bar[1]
                && host[1] <= bar[3]))
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TaskbarSlots {
    left_slot: Option<(i32, i32)>,
    right_slot: Option<(i32, i32)>,
}

#[cfg(any(windows, test))]
fn is_container_class(class: &str) -> bool {
    matches!(
        class,
        "NetDiskMonitor.NativeTaskbar.v1"
            | "ReBarWindow32"
            | "MSTaskSwWClass"
            | "MSTaskListWClass"
            | "WorkerW"
            | "Taskbar.TaskbarFrame"
            | "TaskbarFrame"
            | "TaskbarFrameRepeater"
            | "LandmarkTarget"
            | "Windows.UI.Input.InputSite.WindowClass"
            | "Windows.UI.Composition.DesktopWindowContentBridge"
    )
}

#[cfg(any(windows, test))]
fn compute_taskbar_slots(
    bar_width: i32,
    bar_height: i32,
    start_left: Option<i32>,
    tray_left: Option<i32>,
    widgets_enabled: bool,
    rects: &[[i32; 4]],
) -> TaskbarSlots {
    if bar_width <= 0 || bar_height <= 0 || bar_width < bar_height {
        return TaskbarSlots::default();
    }
    let normalized: Vec<[i32; 4]> = rects
        .iter()
        .filter_map(|&[left, top, end, bottom]| {
            let cl_left = if (-8..0).contains(&left) { 0 } else { left };
            let cl_end = end.min(bar_width);
            if cl_end <= cl_left
                || bottom <= top
                || bottom <= 0
                || top >= bar_height
                || cl_left < 0
                || (cl_left == 0 && cl_end >= bar_width)
                || cl_end - cl_left >= bar_width * 3 / 4
            {
                return None;
            }
            Some([cl_left, top, cl_end, bottom])
        })
        .collect();

    let left_slot = start_left
        .filter(|&s_left| s_left > 80 && s_left <= bar_width)
        .and_then(|s_left| {
            let mut weather_right = 0_i32;
            let mut found_left = false;
            let mut has_right_or_start = false;
            for &[left, _, end, _] in &normalized {
                if left >= s_left {
                    has_right_or_start = true;
                    continue;
                }
                if end <= s_left + 4 {
                    found_left = true;
                    weather_right = weather_right.max(end);
                }
            }
            if found_left {
                Some((weather_right.min(s_left), s_left))
            } else if !widgets_enabled || has_right_or_start {
                Some((0, s_left))
            } else {
                None
            }
        });

    let right_slot = tray_left
        .filter(|&t_left| t_left > 120 && t_left <= bar_width)
        .and_then(|t_left| {
            let mut apps_right = 0_i32;
            for &[left, _, end, _] in &normalized {
                if left < t_left && end <= t_left + 4 {
                    apps_right = apps_right.max(end);
                }
            }
            (apps_right > 0 && apps_right < t_left).then_some((apps_right, t_left))
        });

    TaskbarSlots {
        left_slot,
        right_slot,
    }
}

#[cfg(test)]
fn uia_left_exclusion(width: i32, height: i32, start: i32, rects: &[[i32; 4]]) -> Option<i32> {
    if width <= 0 || height <= 0 || start < 0 || start > width {
        return None;
    }
    let mut right = 0;
    let mut has_left_bound = false;
    for &[left, top, end, bottom] in rects {
        if end <= left || bottom <= top || bottom <= 0 || top >= height || left >= start {
            continue;
        }
        // Full-width containers do not define occupied space; inspect their descendants.
        if left <= 0 && end >= width {
            continue;
        }
        if left < 0 || end > start {
            return None;
        }
        has_left_bound = true;
        right = right.max(end);
    }
    has_left_bound.then_some(right)
}

#[cfg(test)]
mod tests {
    use super::{native_placement, normalize_text, uia_host_is_container, uia_left_exclusion};

    #[test]
    fn boundary_errors_retry_exponentially_without_querying_each_tick() {
        use std::time::{Duration, Instant};
        let mut retry = super::BoundaryRetry::default();
        let mut now = Instant::now();
        assert!(retry.query_due(now, false));
        for millis in [250, 500, 1_000, 2_000, 5_000, 5_000] {
            retry.record(now, false);
            let delay = Duration::from_millis(millis);
            assert!(!retry.usable(true));
            let before_due = (now + delay).checked_sub(Duration::from_millis(1)).unwrap();
            assert!(!retry.query_due(before_due, false));
            now += delay;
            assert!(retry.query_due(now, false));
        }
    }

    #[test]
    fn boundary_geometry_changes_invalidate_proof_and_preserve_the_query_floor() {
        use std::time::{Duration, Instant};
        let mut retry = super::BoundaryRetry::default();
        let now = Instant::now();
        retry.record(now, true);
        assert!(retry.usable(true));
        assert!(!retry.query_due(now + Duration::from_millis(100), false));
        assert!(
            !retry.usable(true),
            "returning to old geometry is not fresh proof"
        );
        for millis in 101..250 {
            assert!(!retry.query_due(now + Duration::from_millis(millis), false));
        }
        assert!(retry.query_due(now + Duration::from_millis(250), false));
    }

    #[test]
    fn boundary_stable_success_uses_five_seconds_and_resets_failure_backoff() {
        use std::time::{Duration, Instant};
        let mut retry = super::BoundaryRetry::default();
        let now = Instant::now();
        for _ in 0..6 {
            retry.record(now, false);
        }
        assert!(!retry.query_due(now + Duration::from_secs(2), false));
        retry.record(now, true);
        assert!(retry.usable(true));
        assert!(!retry.query_due(now + Duration::from_millis(4_999), true));
        assert!(retry.query_due(now + Duration::from_secs(5), true));
        retry.record(now, false);
        assert!(retry.query_due(now + Duration::from_millis(250), true));
    }

    #[test]
    fn native_host_needs_measured_nonoccupying_bounds_and_real_parentage() {
        let bar = [0, 1020, 1920, 1080];
        assert!(uia_host_is_container(bar, [0, 1020, 0, 1020], true));
        assert!(uia_host_is_container(bar, bar, true));
        assert!(!uia_host_is_container(bar, [0, 1020, 1536, 1068], true));
        assert!(!uia_host_is_container(bar, bar, false));
        assert!(!uia_host_is_container(bar, [0, 0, 0, 0], true));
        assert!(!uia_host_is_container(bar, [0, 1020, 0, 1080], true));
    }

    #[test]
    fn uia_reserves_all_visible_left_controls_not_just_weather() {
        assert_eq!(
            uia_left_exclusion(
                1920,
                48,
                800,
                &[
                    [0, 0, 1920, 48],
                    [12, 4, 150, 44],
                    [200, 0, 260, 48],
                    [800, 0, 846, 48]
                ]
            ),
            Some(260)
        );
    }

    #[test]
    fn native_start_does_not_require_a_duplicate_uia_start_element() {
        assert_eq!(uia_left_exclusion(1920, 48, 800, &[[0, 0, 1920, 48]]), None);
        assert_eq!(
            uia_left_exclusion(1920, 48, 800, &[[0, 0, 1920, 48], [8, 0, 198, 48]]),
            Some(198)
        );
        assert_eq!(
            uia_left_exclusion(1920, 48, 800, &[[800, 0, 846, 48]]),
            None
        );
        assert_eq!(
            uia_left_exclusion(1920, 48, 800, &[[200, 0, 900, 48]]),
            None
        );
    }

    #[test]
    fn text_is_two_rows_of_tab_delimited_cells() {
        assert_eq!(
            normalize_text("Down 1 MB/s\tUp 2 KB/s\r\nRAM 40%\nignored"),
            "Down 1 MB/s\tUp 2 KB/s\nRAM 40%"
        );
    }

    #[test]
    fn text_is_bounded_and_keeps_unicode_without_nul() {
        assert_eq!(normalize_text("A\0B"), "AB");
        assert_eq!(normalize_text(&"x".repeat(5000)).len(), 2048);
        assert_eq!(normalize_text("\u{1ec1}\t\u{2193}"), "\u{1ec1}\t\u{2193}");
    }

    #[test]
    fn places_after_weather_and_before_start() {
        assert_eq!(
            native_placement(1920, 48, 800, 160, 248, 40, 8),
            Some((544, 4, 248, 40))
        );
    }

    #[test]
    fn exact_fit_preserves_both_gaps() {
        assert_eq!(
            native_placement(1200, 48, 424, 160, 248, 40, 8),
            Some((168, 4, 248, 40))
        );
    }

    #[test]
    fn insufficient_room_and_left_aligned_start_are_rejected() {
        assert_eq!(native_placement(1200, 48, 423, 160, 248, 40, 8), None);
        assert_eq!(native_placement(1200, 48, 8, 0, 248, 40, 8), None);
    }

    #[test]
    fn overlay_height_is_clamped_to_the_taskbar() {
        assert_eq!(
            native_placement(1920, 48, 800, 160, 248, 164, 8),
            Some((544, 0, 248, 48))
        );
    }

    #[test]
    fn invalid_geometry_and_overflow_are_rejected() {
        for dimensions in [(0, 48), (1920, 0), (1920, -1)] {
            assert_eq!(
                native_placement(dimensions.0, dimensions.1, 800, 160, 248, 40, 8),
                None
            );
        }
        assert_eq!(native_placement(1920, 48, 1921, 0, 248, 40, 8), None);
        assert_eq!(native_placement(1920, 48, 800, -1, 248, 40, 8), None);
        assert_eq!(native_placement(1920, 48, 800, 0, 0, 40, 8), None);
        assert_eq!(
            native_placement(i32::MAX, 48, i32::MAX, i32::MAX, 248, 40, 8),
            None
        );
    }

    #[test]
    fn coordinates_are_taskbar_relative_not_primary_screen_offsets() {
        assert_eq!(
            native_placement(2560, 72, 1100, 240, 372, 60, 12),
            Some((716, 6, 372, 60))
        );
    }

    #[test]
    fn error_classification_distinguishes_unsupported_from_degraded() {
        assert_eq!(
            super::classify_taskbar_error("Native renderer requires a visible horizontal taskbar"),
            super::TaskbarPhase::Unsupported
        );
        assert_eq!(
            super::classify_taskbar_error("Native taskbar mode is only available on Windows"),
            super::TaskbarPhase::Unsupported
        );
        assert_eq!(
            super::classify_taskbar_error(
                "Not enough verified space between weather and Start; native renderer hidden"
            ),
            super::TaskbarPhase::Degraded
        );
    }

    #[test]
    fn slot_templates_stabilize_metric_column_widths() {
        assert_eq!(super::cell_width_template("D 0.0B/s"), "W 999.9M/s");
        assert_eq!(super::cell_width_template("U 12.4K/s"), "W 999.9M/s");
        assert_eq!(super::cell_width_template("RAM 8.1G"), "RAM 99.9G");
        assert_eq!(super::cell_width_template("NET 1.2G"), "NET 999.9G");
        assert_eq!(super::cell_width_template("0h 05m"), "999h 59m");
        assert_eq!(super::cell_width_template("other"), "");
    }

    #[test]
    fn slots_support_centered_start_disabled_widgets_and_right_fallback() {
        use super::{compute_taskbar_slots, TaskbarSlots};

        let centered_weather = compute_taskbar_slots(
            1920,
            60,
            Some(720),
            Some(1680),
            true,
            &[[0, 0, 1920, 60], [8, 4, 198, 56], [720, 4, 1180, 56]],
        );
        assert_eq!(
            centered_weather,
            TaskbarSlots {
                left_slot: Some((198, 720)),
                right_slot: Some((1180, 1680)),
            }
        );

        let centered_no_widgets = compute_taskbar_slots(
            1920,
            60,
            Some(720),
            Some(1680),
            false,
            &[[0, 0, 1920, 60], [-4, 0, 1500, 60]],
        );
        assert_eq!(
            centered_no_widgets,
            TaskbarSlots {
                left_slot: Some((0, 720)),
                right_slot: None,
            }
        );

        let left_aligned_start = compute_taskbar_slots(
            1920,
            60,
            Some(12),
            Some(1680),
            false,
            &[[12, 4, 60, 56], [68, 4, 480, 56], [1680, 4, 1916, 56]],
        );
        assert_eq!(
            left_aligned_start,
            TaskbarSlots {
                left_slot: None,
                right_slot: Some((480, 1680)),
            }
        );
    }

    #[cfg(windows)]
    #[test]
    fn paint_prepare_fitting_compacts_columns_when_width_is_constrained() {
        let mut paint = super::native::Paint::default();
        let text = "D 12.0K/s\tR 1.2M/s\tRAM 8.4G\t0h 12m\nU 3.4K/s\tW 0.5M/s\tNET 1.1G";
        let full = paint.prepare_fitting(text, 120, 1000).unwrap();
        assert_eq!(full, 4);
        let full_w = paint.content_width;

        let compact = paint.prepare_fitting(text, 120, full_w - 20).unwrap();
        assert!((1..=4).contains(&compact));
        assert!(paint.content_width <= full_w - 20);

        let trimmed = paint.prepare_fitting(text, 120, full_w / 2).unwrap();
        assert!((1..4).contains(&trimmed));
        assert!(paint.content_width <= full_w / 2);
        assert_eq!(paint.fitted_cols, trimmed);

        let err = paint.prepare_fitting(text, 120, 12).unwrap_err();
        assert!(err.contains("Not enough verified taskbar space"));
    }
}
