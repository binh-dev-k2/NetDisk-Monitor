#[derive(Default)]
pub struct TaskbarMode {
    #[cfg(windows)]
    session: Option<TaskbarSession>,
}

impl TaskbarMode {
    #[cfg(windows)]
    pub fn enable(&mut self, overlay: isize, width: i32, height: i32) -> Result<(), String> {
        if self.session.is_none() {
            self.session = Some(TaskbarSession::attach(overlay, width, height)?);
        }
        Ok(())
    }

    #[cfg(windows)]
    pub fn resize(&mut self, width: i32, height: i32) -> Result<(), String> {
        if let Some(session) = self.session.as_mut() {
            session.resize(width, height)?;
        }
        Ok(())
    }

    pub fn disable(&mut self) {
        #[cfg(windows)]
        if let Some(mut session) = self.session.take() {
            session.restore();
        }
    }

    pub fn is_enabled(&self) -> bool {
        #[cfg(windows)]
        return self.session.is_some();
        #[cfg(not(windows))]
        false
    }
}

impl Drop for TaskbarMode {
    fn drop(&mut self) {
        self.disable();
    }
}

pub fn horizontal_placement(
    task_list_width: i32,
    bar_height: i32,
    monitor_width: i32,
    monitor_height: i32,
) -> (i32, i32, i32) {
    let remaining_width = (task_list_width - monitor_width).max(0);
    let y = (((bar_height - monitor_height) / 2) + (bar_height / 20).max(1)).max(0);
    (remaining_width, remaining_width + 2, y)
}

pub fn windows11_placement(
    taskbar_width: i32,
    taskbar_height: i32,
    start_left: i32,
    notify_x: Option<i32>,
    monitor_width: i32,
    monitor_height: i32,
) -> (i32, i32) {
    let notify_x = notify_x.unwrap_or(taskbar_width - 280);
    // If the Start button left offset is > 120, the taskbar is centered, so place it on the left empty space.
    // Otherwise (if left-aligned), place it to the left of the system tray.
    let is_centered = start_left > 120;
    
    let x = if is_centered {
        // Place it on the far left of the taskbar (typically after Widgets icon, which is at ~150px)
        180
    } else {
        (notify_x - monitor_width - 8).max(0)
    };
    
    let y = (((taskbar_height - monitor_height) / 2) + (taskbar_height / 20).max(1)).max(0);
    (x, y)
}

#[cfg(windows)]
use std::ptr::{null, null_mut};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::{
        FindWindowExW, FindWindowW, GetWindowLongPtrW, GetWindowRect, MoveWindow, SetParent,
        SetWindowLongPtrW, GWL_STYLE, WS_CHILD, WS_POPUP,
    },
};

#[cfg(windows)]
struct TaskbarSession {
    overlay: isize,
    overlay_rect: RECT,
    overlay_style: isize,
    placement: TaskbarPlacement,
}

#[cfg(windows)]
enum TaskbarPlacement {
    Classic {
        task_list: isize,
        task_list_rect: RECT,
        rebar_rect: RECT,
    },
    Windows11,
}

#[cfg(windows)]
impl TaskbarSession {
    fn attach(overlay: isize, width: i32, height: i32) -> Result<Self, String> {
        let taskbar = find_window("Shell_TrayWnd")
            .ok_or_else(|| "Windows taskbar was not found".to_owned())?;
        let overlay_rect = window_rect(overlay)?;
        let taskbar_rect = window_rect(taskbar)?;
        let overlay_style = unsafe { GetWindowLongPtrW(hwnd(overlay), GWL_STYLE) };

        if let Some(start) = find_child(taskbar, "Start") {
            let start_rect = window_rect(start).unwrap_or(taskbar_rect);
            let start_left = start_rect.left - taskbar_rect.left;
            let notify_x = find_child(taskbar, "TrayNotifyWnd")
                .and_then(|notify| window_rect(notify).ok())
                .map(|rect| rect.left - taskbar_rect.left);
            let (x, y) = windows11_placement(
                rect_width(taskbar_rect),
                rect_height(taskbar_rect),
                start_left,
                notify_x,
                width,
                height,
            );
            unsafe {
                SetParent(hwnd(overlay), hwnd(taskbar));
                SetWindowLongPtrW(
                    hwnd(overlay),
                    GWL_STYLE,
                    (overlay_style & !(WS_POPUP as isize)) | WS_CHILD as isize,
                );
            }
            move_window(overlay, x, y, width, height);
            return Ok(Self {
                overlay,
                overlay_rect,
                overlay_style,
                placement: TaskbarPlacement::Windows11,
            });
        }

        let rebar = find_child(taskbar, "ReBarWindow32")
            .or_else(|| find_child(taskbar, "WorkerW"))
            .ok_or_else(|| "Windows taskbar container is unsupported".to_owned())?;
        let task_list = find_child(rebar, "MSTaskSwWClass")
            .or_else(|| find_child(rebar, "MSTaskListWClass"))
            .ok_or_else(|| "Windows task list was not found".to_owned())?;
        let task_list_rect = window_rect(task_list)?;
        let rebar_rect = window_rect(rebar)?;

        unsafe {
            SetParent(hwnd(overlay), hwnd(rebar));
            SetWindowLongPtrW(
                hwnd(overlay),
                GWL_STYLE,
                (overlay_style & !(WS_POPUP as isize)) | WS_CHILD as isize,
            );
        }

        if rect_width(taskbar_rect) >= rect_height(taskbar_rect) {
            let left = task_list_rect.left - rebar_rect.left;
            let (remaining_width, x, y) = horizontal_placement(
                rect_width(task_list_rect),
                rect_height(rebar_rect),
                width,
                height,
            );
            move_window(
                task_list,
                left,
                0,
                remaining_width,
                rect_height(task_list_rect),
            );
            move_window(overlay, left + x, y, width, height);
        } else {
            let top = task_list_rect.top - rebar_rect.top;
            let remaining_height = (rect_height(task_list_rect) - height).max(0);
            let x = ((rect_width(rebar_rect) - width) / 2).max(0);
            move_window(
                task_list,
                0,
                top,
                rect_width(task_list_rect),
                remaining_height,
            );
            move_window(overlay, x, top + remaining_height, width, height);
        }

        Ok(Self {
            overlay,
            overlay_rect,
            overlay_style,
            placement: TaskbarPlacement::Classic {
                task_list,
                task_list_rect,
                rebar_rect,
            },
        })
    }

    fn restore(&mut self) {
        unsafe {
            SetParent(hwnd(self.overlay), null_mut());
            SetWindowLongPtrW(hwnd(self.overlay), GWL_STYLE, self.overlay_style);
        }
        move_window(
            self.overlay,
            self.overlay_rect.left,
            self.overlay_rect.top,
            rect_width(self.overlay_rect),
            rect_height(self.overlay_rect),
        );
        if let TaskbarPlacement::Classic {
            task_list,
            task_list_rect,
            rebar_rect,
        } = self.placement
        {
            move_window(
                task_list,
                task_list_rect.left - rebar_rect.left,
                task_list_rect.top - rebar_rect.top,
                rect_width(task_list_rect),
                rect_height(task_list_rect),
            );
        }
    }

    fn resize(&mut self, width: i32, height: i32) -> Result<(), String> {
        if matches!(&self.placement, TaskbarPlacement::Windows11) {
            position_windows11(self.overlay, width, height)?;
        }
        Ok(())
    }
}

#[cfg(windows)]
fn position_windows11(overlay: isize, width: i32, height: i32) -> Result<(), String> {
    let taskbar =
        find_window("Shell_TrayWnd").ok_or_else(|| "Windows taskbar was not found".to_owned())?;
    let taskbar_rect = window_rect(taskbar)?;
    let start = find_child(taskbar, "Start")
        .ok_or_else(|| "Windows 11 taskbar start button was not found".to_owned())?;
    let start_rect = window_rect(start).unwrap_or(taskbar_rect);
    let start_left = start_rect.left - taskbar_rect.left;
    let notify_x = find_child(taskbar, "TrayNotifyWnd")
        .and_then(|notify| window_rect(notify).ok())
        .map(|rect| rect.left - taskbar_rect.left);
    let (x, y) = windows11_placement(
        rect_width(taskbar_rect),
        rect_height(taskbar_rect),
        start_left,
        notify_x,
        width,
        height,
    );
    move_window(overlay, x, y, width, height);
    Ok(())
}

#[cfg(windows)]
fn hwnd(value: isize) -> HWND {
    value as HWND
}

#[cfg(windows)]
fn find_window(class_name: &str) -> Option<isize> {
    let class_name = wide(class_name);
    let window = unsafe { FindWindowW(class_name.as_ptr(), null()) };
    (!window.is_null()).then_some(window as isize)
}

#[cfg(windows)]
fn find_child(parent: isize, class_name: &str) -> Option<isize> {
    let class_name = wide(class_name);
    let window = unsafe { FindWindowExW(hwnd(parent), null_mut(), class_name.as_ptr(), null()) };
    (!window.is_null()).then_some(window as isize)
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(windows)]
fn window_rect(window: isize) -> Result<RECT, String> {
    let mut rect = RECT::default();
    (unsafe { GetWindowRect(hwnd(window), &mut rect) } != 0)
        .then_some(rect)
        .ok_or_else(|| "Unable to read Windows taskbar geometry".to_owned())
}

#[cfg(windows)]
fn move_window(window: isize, x: i32, y: i32, width: i32, height: i32) {
    unsafe { MoveWindow(hwnd(window), x, y, width, height, 1) };
}

#[cfg(windows)]
fn rect_width(rect: RECT) -> i32 {
    rect.right - rect.left
}
#[cfg(windows)]
fn rect_height(rect: RECT) -> i32 {
    rect.bottom - rect.top
}

#[cfg(test)]
mod tests {
    use super::{horizontal_placement, windows11_placement};

    #[test]
    fn horizontal_taskbar_reserves_space_without_overlapping_app_icons() {
        assert_eq!(horizontal_placement(900, 48, 248, 40), (652, 654, 6));
    }

    #[test]
    fn windows11_taskbar_places_monitor_before_the_notification_area() {
        assert_eq!(
            windows11_placement(1920, 48, 48, Some(1620), 248, 40),
            (1364, 6)
        );
    }

    #[test]
    fn windows11_taskbar_places_monitor_on_left_when_centered() {
        assert_eq!(
            windows11_placement(1920, 48, 800, Some(1620), 248, 40),
            (180, 6)
        );
    }
}
