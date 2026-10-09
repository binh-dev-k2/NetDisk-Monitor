//! Independent native taskbar renderer; never touches the supplied `WebView`.
//! Requires `Win32_Graphics_Gdi` and `Win32_System_LibraryLoader` windows-sys features.
//! UIA fallback uses windows 0.57: `Win32_Foundation`, `Win32_System_Com`,
//! `Win32_UI_Accessibility`. All COM objects stay on the renderer thread.
//! Native GDI output is opaque; `WebView` opacity settings do not apply here.

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
    use super::{native_placement, rect_contains, uia_host_is_container, uia_left_exclusion, wide};
    use std::{
        ptr::{null, null_mut},
        sync::{
            atomic::{AtomicBool, Ordering},
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
            GetStockObject, GetSysColor, GetTextExtentPoint32W, InvalidateRect, ReleaseDC,
            SelectObject, SetBkMode, SetDCBrushColor, SetTextColor, COLOR_BTNFACE, COLOR_BTNTEXT,
            DC_BRUSH, DT_END_ELLIPSIS, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, HFONT, PAINTSTRUCT,
            TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::HiDpi::{
            GetDpiForWindow, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumChildWindows,
            FindWindowW, GetClassNameW, GetClientRect, GetMessageW, GetWindowLongPtrW,
            GetWindowRect, GetWindowThreadProcessId, IsChild, IsWindow, IsWindowVisible, KillTimer,
            RegisterClassW, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
            TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, MA_NOACTIVATE, MSG, SWP_NOACTIVATE,
            SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCCREATE,
            WM_NCDESTROY, WM_PAINT, WNDCLASSW, WS_CHILD, WS_EX_NOACTIVATE,
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
        }
        pub fn enabled(&self) -> bool {
            self.shared.live.load(Ordering::Acquire)
                && self.shared.active.load(Ordering::Acquire)
                && !self.shared.stop.load(Ordering::Acquire)
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
                    .SetConnectionTimeout(100)
                    .and_then(|()| automation.SetTransactionTimeout(100))
                    .map_err(|error| {
                        format!("UIA provider timeout configuration failed: {error}")
                    })?;
                Ok(Self(automation))
            }
        }

        fn left_edge(
            &self,
            parent: HWND,
            own: HWND,
            bar: RECT,
            start: RECT,
        ) -> Result<i32, String> {
            let query = || -> windows::core::Result<i32> {
                unsafe {
                    let root = self.0.ElementFromHandle(UiaHwnd(parent as isize))?;
                    let physical = root.CurrentBoundingRectangle()?;
                    if physical.left != bar.left
                        || physical.top != bar.top
                        || physical.right != bar.right
                        || physical.bottom != bar.bottom
                    {
                        return Err(windows::core::Error::new(E_FAIL,
                            "UIA/Win32 coordinate mismatch; taskbar requires per-monitor DPI awareness"));
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
                    if !uia_host_is_container(
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
                    let condition = self.0.CreateTrueCondition()?;
                    let items =
                        scan_root.FindAllBuildCache(TreeScope_Descendants, &condition, &request)?;
                    let count = items.Length()?;
                    if !(1..=256).contains(&count) {
                        return Err(windows::core::Error::new(
                            E_FAIL,
                            "UIA taskbar tree is empty or exceeds the bounded scan limit",
                        ));
                    }
                    let mut rects = visible_native_bounds(parent, own, bar)
                        .map_err(|error| windows::core::Error::new(E_FAIL, error))?;
                    let mut has_start = false;
                    let start_left = start.left - bar.left;
                    for index in 0..count {
                        let element = items.GetElement(index)?;
                        let class = element.CachedClassName()?.to_string();
                        let handle = element.CachedNativeWindowHandle()?.0;
                        if class == CLASS || (handle != 0 && handle == own as isize) {
                            continue;
                        }
                        if element.CachedIsOffscreen()?.as_bool() {
                            continue;
                        }
                        let id = element.CachedAutomationId()?.to_string();
                        let rect = element.CachedBoundingRectangle()?;
                        let relative = [
                            rect.left - bar.left,
                            rect.top - bar.top,
                            rect.right - bar.left,
                            rect.bottom - bar.top,
                        ];
                        if class == "Start" || id == "StartButton" || id == "Start" {
                            has_start |= rect_contains(
                                [start.left, start.top, start.right, start.bottom],
                                [rect.left, rect.top, rect.right, rect.bottom],
                            );
                        }
                        rects.push(relative);
                    }
                    uia_left_exclusion(bar.right - bar.left, bar.bottom - bar.top, start_left, has_start, &rects)
                        .ok_or_else(|| windows::core::Error::new(E_FAIL,
                            "UIA tree lacks a verifiable Start element or contains ambiguous left-side bounds"))
                }
            };
            query().map_err(|error| format!(
                "Weather HWND unavailable; native UIA cannot verify free space: {error}; renderer hidden, retrying. Supply measured set_left_exclusion() to override"))
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
        width: i32,
        height: i32,
    }

    struct Renderer {
        module: isize,
        parent: HWND,
        parent_pid: u32,
        start: HWND,
        weather: HWND,
        weather_discovered: Instant,
        window: HWND,
        discovered: Instant,
        boundary: Option<(u64, BarKey, i32)>,
        automation: Option<Automation>,
        auto_boundary: Option<(BarKey, i32, Instant, Result<i32, String>)>,
        paint: Box<Paint>,
        position: Option<(i32, i32, i32, i32)>,
        visible: bool,
        last_input: Option<Input>,
        refreshed: Instant,
        refresh_result: Result<(), String>,
    }

    impl Renderer {
        fn new(module: isize) -> Self {
            Self {
                module,
                parent: null_mut(),
                parent_pid: 0,
                start: null_mut(),
                weather: null_mut(),
                weather_discovered: elapsed_anchor(UIA_INTERVAL),
                window: null_mut(),
                discovered: elapsed_anchor(DISCOVERY_INTERVAL),
                boundary: None,
                automation: None,
                auto_boundary: None,
                paint: Box::new(Paint::default()),
                position: None,
                visible: false,
                last_input: None,
                refreshed: elapsed_anchor(DISCOVERY_INTERVAL),
                refresh_result: Ok(()),
            }
        }

        fn refresh(&mut self, input: &Input) -> Result<(), String> {
            if self.last_input.as_ref() == Some(input)
                && self.refreshed.elapsed() < DISCOVERY_INTERVAL
            {
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
                    self.weather = null_mut();
                    self.weather_discovered = elapsed_anchor(UIA_INTERVAL);
                }
            }
            if self.parent.is_null() {
                return Err("Windows taskbar was not found; retrying".to_owned());
            }
            let bar = window_rect(self.parent)?;
            let key = BarKey {
                hwnd: self.parent as isize,
                pid: self.parent_pid,
                width: bar.right - bar.left,
                height: bar.bottom - bar.top,
            };
            if key.width < key.height || key.height <= 2 {
                return Err("Native renderer requires a visible horizontal taskbar".to_owned());
            }
            if unsafe { IsWindow(self.start) } == 0 {
                self.start = find_descendant(self.parent, &["Start"]).ok_or_else(|| {
                    "Cannot verify the Start button boundary; native renderer hidden".to_owned()
                })?;
            }
            let start_rect = window_rect(self.start)?;
            let start_left = start_rect.left - bar.left;
            if unsafe { IsWindowVisible(self.parent) } == 0 || input.text.trim().is_empty() {
                self.hide();
                return Ok(());
            }
            let weather_right = if let Some(right) = input.left_exclusion {
                if self
                    .boundary
                    .is_none_or(|(revision, _, _)| revision != input.boundary_revision)
                {
                    self.boundary = Some((input.boundary_revision, key, right));
                }
                let (_, anchor, right) = self.boundary.expect("boundary just installed");
                if anchor != key {
                    return Err("Taskbar geometry changed; refresh set_left_exclusion() with measured bounds".to_owned());
                }
                right
            } else {
                self.boundary = None;
                // XAML weather often has no HWND. Absence is not evidence of free space.
                if self.weather_discovered.elapsed() >= UIA_INTERVAL {
                    self.weather_discovered = Instant::now();
                    self.weather = find_descendant(self.parent, &["DynamicContent2", "Widgets"])
                        .unwrap_or(null_mut());
                }
                if !self.weather.is_null() && unsafe { IsWindow(self.weather) } != 0 {
                    window_rect(self.weather)?.right - bar.left
                } else {
                    let stale = self
                        .auto_boundary
                        .as_ref()
                        .is_none_or(|(_, _, time, _)| time.elapsed() >= UIA_INTERVAL);
                    if stale {
                        let result = (|| {
                            if self.automation.is_none() {
                                self.automation = Some(Automation::new()?);
                            }
                            self.automation
                                .as_ref()
                                .expect("automation installed")
                                .left_edge(self.parent, self.window, bar, start_rect)
                        })();
                        self.auto_boundary = Some((key, start_left, Instant::now(), result));
                    }
                    let (anchor, start, _, result) =
                        self.auto_boundary.as_ref().expect("UIA cache installed");
                    if *anchor != key || *start != start_left {
                        return Err("Taskbar geometry changed; renderer hidden until the throttled UIA boundary refresh (at most 5s)".to_owned());
                    }
                    result.clone()?
                }
            };
            let dpi = i32::try_from(unsafe { GetDpiForWindow(self.parent) })
                .ok()
                .filter(|dpi| *dpi > 0 && *dpi <= 768)
                .ok_or_else(|| "Cannot verify taskbar DPI; renderer hidden".to_owned())?;
            self.paint.prepare(&input.text, dpi)?;
            let gap = (8 * dpi / 96).max(1);
            let width = input.width.max(self.paint.content_width);
            let height = input.height.max(self.paint.content_height);
            let placement = native_placement(
                key.width,
                key.height,
                start_left,
                weather_right,
                width,
                height,
                gap,
            )
            .ok_or_else(|| {
                "Not enough verified space between weather and Start; native renderer hidden"
                    .to_owned()
            })?;
            if placement.3 < self.paint.content_height {
                return Err(
                    "Taskbar is too short for the requested text rows; native renderer hidden"
                        .to_owned(),
                );
            }
            if self.window.is_null() || unsafe { IsWindow(self.window) } == 0 {
                self.window = null_mut();
                let class = wide(CLASS);
                let window = unsafe {
                    CreateWindowExW(
                        WS_EX_NOACTIVATE,
                        class.as_ptr(),
                        null(),
                        WS_CHILD,
                        placement.0,
                        placement.1,
                        placement.2,
                        placement.3,
                        self.parent,
                        null_mut(),
                        self.module as _,
                        (&raw mut *self.paint).cast(),
                    )
                };
                if window.is_null() {
                    return Err(win_error("CreateWindowExW(native child)"));
                }
                self.window = window;
                self.position = None;
                self.visible = false;
            }
            if self.position != Some(placement) {
                if unsafe {
                    SetWindowPos(
                        self.window,
                        null_mut(),
                        placement.0,
                        placement.1,
                        placement.2,
                        placement.3,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    )
                } == 0
                {
                    return Err(win_error("SetWindowPos(native child)"));
                }
                self.position = Some(placement);
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

    #[derive(Default)]
    struct Paint {
        text: String,
        cells: Vec<Vec<Vec<u16>>>,
        widths: Vec<i32>,
        font: HFONT,
        dpi: i32,
        line_height: i32,
        padding: i32,
        content_width: i32,
        content_height: i32,
        dirty: bool,
    }

    impl Paint {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "Public input is bounded to 2048 scalars (4096 UTF-16 units), 16 columns, and two rows; GDI takes i32 counts"
        )]
        fn prepare(&mut self, text: &str, dpi: i32) -> Result<(), String> {
            if self.text == text && self.dpi == dpi {
                return Ok(());
            }
            if self.dpi != dpi {
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
            }
            let dc = unsafe { GetDC(null_mut()) };
            if dc.is_null() {
                return Err(win_error("GetDC(text measurement)"));
            }
            let old = unsafe { SelectObject(dc, self.font) };
            let cells: Vec<Vec<Vec<u16>>> = text
                .lines()
                .take(2)
                .map(|line| line.split('\t').take(16).map(wide).collect())
                .collect();
            let columns = cells.iter().map(Vec::len).max().unwrap_or(0);
            let mut widths = vec![0; columns];
            let mut line_height = 0;
            let mut valid = true;
            for row in &cells {
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
                    widths[column] = widths[column].max(size.cx);
                    line_height = line_height.max(size.cy);
                }
            }
            unsafe {
                SelectObject(dc, old);
                ReleaseDC(null_mut(), dc);
            }
            if !valid {
                return Err("Unable to measure native metric text".to_owned());
            }
            self.padding = (4 * dpi / 96).max(1);
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
            SetDCBrushColor(dc, GetSysColor(COLOR_BTNFACE));
            FillRect(dc, &raw const client, GetStockObject(DC_BRUSH));
            SetBkMode(dc, TRANSPARENT.cast_signed());
            SetTextColor(dc, GetSysColor(COLOR_BTNTEXT));
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
            WM_ERASEBKGND => 1,
            WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
            WM_NCDESTROY => {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                DefWindowProcW(window, message, w, l)
            }
            _ => DefWindowProcW(window, message, w, l),
        }
    }

    fn find_descendant(parent: HWND, classes: &[&str]) -> Option<HWND> {
        struct Search<'a> {
            classes: &'a [&'a str],
            found: HWND,
        }
        unsafe extern "system" fn visit(window: HWND, parameter: LPARAM) -> i32 {
            let search = &mut *(parameter as *mut Search<'_>);
            let mut name = [0u16; 256];
            let length = GetClassNameW(window, name.as_mut_ptr(), 256);
            let Ok(length) = usize::try_from(length) else {
                return 1;
            };
            if length > 0
                && search
                    .classes
                    .iter()
                    .any(|class| class.encode_utf16().eq(name[..length].iter().copied()))
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
            rects: Vec<[i32; 4]>,
            failed: bool,
            count: usize,
        }
        unsafe extern "system" fn visit(window: HWND, parameter: LPARAM) -> i32 {
            let scan = &mut *(parameter as *mut Scan);
            scan.count += 1;
            if scan.count > 256 {
                scan.failed = true;
                return 0;
            }
            if window == scan.own || IsWindowVisible(window) == 0 {
                return 1;
            }
            let mut rect = RECT::default();
            if GetWindowRect(window, &raw mut rect) == 0 {
                scan.failed = true;
                return 0;
            }
            scan.rects
                .push([rect.left, rect.top, rect.right, rect.bottom]);
            1
        }
        let mut scan = Scan {
            own,
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
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetParent, GWL_STYLE};

        fn bounds(rect: RECT) -> [i32; 4] {
            [rect.left, rect.top, rect.right, rect.bottom]
        }

        fn assert_native_child(window: HWND, parent: HWND) {
            assert!(!window.is_null(), "native renderer did not create a child");
            assert_eq!(unsafe { GetParent(window) }, parent);
            let mut class = [0_u16; 128];
            let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), 128) };
            assert_eq!(
                String::from_utf16(&class[..usize::try_from(length).unwrap()]).unwrap(),
                CLASS
            );
            assert_ne!(
                unsafe { GetWindowLongPtrW(window, GWL_STYLE) }
                    & isize::try_from(WS_CHILD).unwrap(),
                0
            );
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
            let result = renderer.refresh(&input);
            eprintln!("native refresh: {result:?}");
            if !renderer.parent.is_null() {
                eprintln!(
                    "taskbar={:?}, dpi={}",
                    bounds(window_rect(renderer.parent).unwrap()),
                    unsafe { GetDpiForWindow(renderer.parent) }
                );
            }
            result.expect("desktop bounds must be verified, never guessed for acceptance");
            assert_native_child(renderer.window, renderer.parent);
            assert_eq!(renderer.paint.cells.len(), 2);
            assert!(renderer.visible);
            assert_ne!(unsafe { IsWindowVisible(renderer.window) }, 0);
            let bar = window_rect(renderer.parent).unwrap();
            let own = window_rect(renderer.window).unwrap();
            let start = window_rect(renderer.start).unwrap();
            let left = if renderer.weather.is_null() {
                renderer
                    .automation
                    .as_ref()
                    .unwrap()
                    .left_edge(renderer.parent, renderer.window, bar, start)
                    .expect("independent live UIA remeasurement must verify occupied bounds")
            } else {
                window_rect(renderer.weather).unwrap().right - bar.left
            };
            let dpi = i32::try_from(unsafe { GetDpiForWindow(renderer.parent) }).unwrap();
            let gap = (8 * dpi / 96).max(1);
            assert!(own.left >= bar.left + left + gap);
            assert!(own.right <= start.left - gap);
            assert!(own.top >= bar.top && own.bottom <= bar.bottom);
            assert!(own.bottom - own.top >= renderer.paint.content_height);
            eprintln!(
                "native child={:?}, Start={:?}, left exclusion={}, gap={}, rows={}",
                bounds(own),
                bounds(start),
                left,
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
                    if let Some(child) = find_descendant(parent, &[CLASS]) {
                        let mut pid = 0;
                        unsafe {
                            GetWindowThreadProcessId(child, &raw mut pid);
                        }
                        if pid == std::process::id() && unsafe { IsWindowVisible(child) } != 0 {
                            break child;
                        }
                    }
                    assert!(
                        Instant::now() < deadline,
                        "cycle {cycle}: no visible native child: {:?}",
                        mode.last_error()
                    );
                    thread::sleep(Duration::from_millis(25));
                };
                assert_native_child(child, parent);
                if cycle == 0 {
                    // Cross the production cache interval while its message loop runs.
                    thread::sleep(UIA_INTERVAL + Duration::from_secs(1));
                    assert_ne!(unsafe { IsWindowVisible(child) }, 0);
                    assert_eq!(mode.last_error(), None, "attached UIA refresh failed");
                    // Deliberately invalid test data is not a claimed measured override.
                    mode.set_left_exclusion(Some(i32::MAX)).unwrap();
                    let hidden_deadline = Instant::now() + Duration::from_secs(2);
                    while unsafe { IsWindowVisible(child) } != 0 {
                        assert!(
                            Instant::now() < hidden_deadline,
                            "unverified bounds did not hide native child"
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
                    eprintln!("attached UIA refresh survived >5s; invalid bounds hid child; verified detection recovered");
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
                eprintln!("cycle {cycle}: enable={enable_elapsed:?}, cleanup={:?}, child destroyed, overlay unchanged", stopped.elapsed());
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
fn rect_contains(outer: [i32; 4], inner: [i32; 4]) -> bool {
    inner[2] > inner[0]
        && inner[3] > inner[1]
        && inner[0] >= outer[0]
        && inner[1] >= outer[1]
        && inner[2] <= outer[2]
        && inner[3] <= outer[3]
}

#[cfg(any(windows, test))]
fn uia_left_exclusion(
    width: i32,
    height: i32,
    start: i32,
    has_start: bool,
    rects: &[[i32; 4]],
) -> Option<i32> {
    if !has_start || width <= 0 || height <= 0 || start < 0 || start > width {
        return None;
    }
    let mut right = 0;
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
        right = right.max(end);
    }
    Some(right)
}

#[cfg(test)]
mod tests {
    use super::{
        native_placement, normalize_text, rect_contains, uia_host_is_container, uia_left_exclusion,
    };

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
    fn xaml_start_inset_is_verified_without_coordinate_tolerance() {
        let native = [626, 1020, 685, 1080];
        assert!(rect_contains(native, [627, 1020, 683, 1080]));
        assert!(rect_contains(native, native));
        assert!(!rect_contains(native, [625, 1020, 683, 1080]));
        assert!(!rect_contains(native, [627, 1019, 683, 1080]));
        assert!(!rect_contains(native, [627, 1020, 686, 1080]));
        assert!(!rect_contains(native, [627, 1020, 683, 1081]));
        assert!(!rect_contains(native, [627, 1020, 627, 1080]));
        assert!(!rect_contains(native, [627, 1020, 683, 1020]));
    }

    #[test]
    fn uia_reserves_all_visible_left_controls_not_just_weather() {
        assert_eq!(
            uia_left_exclusion(
                1920,
                48,
                800,
                true,
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
    fn incomplete_uia_tree_is_not_proof_of_free_space() {
        assert_eq!(
            uia_left_exclusion(1920, 48, 800, false, &[[0, 0, 1920, 48]]),
            None
        );
        assert_eq!(
            uia_left_exclusion(1920, 48, 800, true, &[[800, 0, 846, 48]]),
            Some(0)
        );
        assert_eq!(
            uia_left_exclusion(1920, 48, 800, true, &[[200, 0, 900, 48]]),
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
}
