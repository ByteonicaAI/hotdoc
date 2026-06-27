use tauri::{AppHandle, PhysicalPosition, Runtime, WebviewWindow};

/// Logical window width, mirroring `app.windows[0].width` in tauri.conf.json.
const WINDOW_WIDTH: f64 = 720.0;
/// Logical gap from the top edge of the active monitor.
const TOP_MARGIN: f64 = 60.0;

/// Center the window (top-aligned) on the monitor under the cursor, so on a
/// multi-monitor setup it always appears on the screen the user is currently
/// working on. Falls back to the primary monitor when the cursor's monitor
/// can't be resolved.
pub fn center_on_active_monitor<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    // Monitor geometry is in physical pixels; scale the logical width and
    // margin by the monitor's factor so the window stays centered on
    // fractional-scale displays.
    let scale = monitor.scale_factor();
    let origin = monitor.position();
    let size = monitor.size();
    let win_w = WINDOW_WIDTH * scale;
    let x = origin.x + ((size.width as f64 - win_w) / 2.0).max(0.0) as i32;
    let y = origin.y + (TOP_MARGIN * scale) as i32;
    let _ = window.set_position(PhysicalPosition::new(x, y));
}
