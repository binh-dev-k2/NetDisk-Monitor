use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub(crate) const DEFAULT_LOGICAL_WIDTH: f64 = 336.0;
pub(crate) const DEFAULT_LOGICAL_HEIGHT: f64 = 44.0;

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Clamped finite positive physical pixel dimensions fit within i32"
)]
fn scaled_pixels(logical: f64, scale: f64) -> i32 {
    (logical * scale).round().clamp(1.0, 16_384.0) as i32
}

#[must_use]
pub(crate) fn clamp_overlay_position(
    desired: Option<(i32, i32)>,
    monitors: &[(i32, i32, u32, u32)],
    scale: f64,
) -> (i32, i32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let box_w = scaled_pixels(DEFAULT_LOGICAL_WIDTH, scale);
    let box_h = scaled_pixels(DEFAULT_LOGICAL_HEIGHT, scale);

    if let Some((x, y)) = desired {
        for &(left, top, width, height) in monitors {
            let mw = i32::try_from(width).unwrap_or(i32::MAX);
            let mh = i32::try_from(height).unwrap_or(i32::MAX);
            if mw <= 0 || mh <= 0 {
                continue;
            }
            let right = left.saturating_add(mw);
            let bottom = top.saturating_add(mh);
            let overlap_w = x.saturating_add(box_w).min(right) - x.max(left);
            let overlap_h = y.saturating_add(box_h).min(bottom) - y.max(top);
            if overlap_w > 0 && overlap_h > 0 {
                let max_x = right.saturating_sub(box_w).max(left);
                let max_y = bottom.saturating_sub(box_h).max(top);
                return (x.clamp(left, max_x), y.clamp(top, max_y));
            }
        }
    }

    if let Some(&(left, top, width, _)) = monitors.first() {
        let mw = i32::try_from(width).unwrap_or(i32::MAX);
        let offset_x = ((mw - box_w).max(0)) / 2;
        let offset_y = scaled_pixels(8.0, scale);
        return (left.saturating_add(offset_x), top.saturating_add(offset_y));
    }

    (20, 8)
}

fn collect_monitor_rects(app: &AppHandle) -> (Vec<(i32, i32, u32, u32)>, f64) {
    let mut rects = Vec::new();
    let mut fallback_scale = 1.0;
    if let Ok(monitors) = app.available_monitors() {
        for monitor in monitors {
            if rects.is_empty() {
                fallback_scale = monitor.scale_factor();
            }
            let pos = monitor.position();
            let size = monitor.size();
            rects.push((pos.x, pos.y, size.width, size.height));
        }
    }
    if rects.is_empty() {
        if let Ok(Some(primary)) = app.primary_monitor() {
            fallback_scale = primary.scale_factor();
            let pos = primary.position();
            let size = primary.size();
            rects.push((pos.x, pos.y, size.width, size.height));
        }
    }
    (rects, fallback_scale)
}

pub(crate) fn default_overlay_position(app: &AppHandle) -> (i32, i32) {
    let (monitors, scale) = collect_monitor_rects(app);
    clamp_overlay_position(None, &monitors, scale)
}

pub(crate) fn ensure_overlay_window(
    app: &AppHandle,
    visible: bool,
    pinned: bool,
    desired_position: Option<(i32, i32)>,
) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("monitor-overlay") {
        return Ok(window);
    }

    let (monitors, scale) = collect_monitor_rects(app);
    let (x, y) = clamp_overlay_position(desired_position, &monitors, scale);
    let window =
        WebviewWindowBuilder::new(app, "monitor-overlay", WebviewUrl::App("meter.html".into()))
            .title("NetDisk Monitor")
            .visible(false)
            .inner_size(DEFAULT_LOGICAL_WIDTH, DEFAULT_LOGICAL_HEIGHT)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(pinned)
            .skip_taskbar(true)
            .resizable(false)
            .build()
            .map_err(|error| error.to_string())?;
    let _ = window.set_position(PhysicalPosition::new(x, y));
    if visible {
        window.show().map_err(|error| error.to_string())?;
    }
    Ok(window)
}

#[cfg(test)]
mod tests {
    use super::clamp_overlay_position;

    #[test]
    fn valid_position_on_primary_or_negative_secondary_monitor_is_preserved() {
        let monitors = [(0, 0, 1920, 1080), (-1920, 0, 1920, 1080)];
        assert_eq!(
            clamp_overlay_position(Some((200, 16)), &monitors, 1.0),
            (200, 16)
        );
        assert_eq!(
            clamp_overlay_position(Some((-1400, 100)), &monitors, 1.0),
            (-1400, 100)
        );
    }

    #[test]
    fn offscreen_position_after_monitor_disconnect_recovers_to_primary() {
        let monitors = [(0, 0, 1920, 1080)];
        assert_eq!(
            clamp_overlay_position(Some((2500, 120)), &monitors, 1.0),
            ((1920 - 336) / 2, 8)
        );
    }

    #[test]
    fn partial_edge_position_is_clamped_so_controls_stay_reachable() {
        let monitors = [(0, 0, 1920, 1080)];
        assert_eq!(
            clamp_overlay_position(Some((1850, 20)), &monitors, 1.0),
            (1920 - 336, 20)
        );
        assert_eq!(
            clamp_overlay_position(Some((-200, -10)), &monitors, 1.0),
            (0, 0)
        );
    }

    #[test]
    fn empty_monitor_list_uses_safe_fallback() {
        assert_eq!(clamp_overlay_position(Some((900, 900)), &[], 1.0), (20, 8));
    }
}
