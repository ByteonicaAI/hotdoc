use tauri::{AppHandle, LogicalPosition, Monitor, Runtime, WebviewWindow};
use tracing::warn;

/// Logical window width, mirroring `app.windows[0].width` in tauri.conf.json.
const WINDOW_WIDTH: f64 = 720.0;
/// Logical gap from the top edge of the active monitor.
const TOP_MARGIN: f64 = 60.0;

/// Center the window (top-aligned) on the monitor under the cursor, so on a
/// multi-monitor setup it always appears on the screen the user is currently
/// working on. Falls back to the primary monitor when the cursor's monitor
/// can't be resolved.
pub fn center_on_active_monitor<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let monitor = cursor_monitor(app).or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        warn!("no monitor resolved; leaving launcher window position unchanged");
        return;
    };
    // Convert the monitor's physical rect to logical points and hand tauri a
    // LogicalPosition. set_position with physical coordinates is unreliable
    // across monitors (macOS interprets it relative to the window's current
    // monitor), so logical points keep placement deterministic.
    let scale = monitor.scale_factor();
    let origin = monitor.position();
    let size = monitor.size();
    let ox = origin.x as f64 / scale;
    let w = size.width as f64 / scale;
    let x = ox + ((w - WINDOW_WIDTH) / 2.0).max(0.0);
    let y = origin.y as f64 / scale + TOP_MARGIN;
    if let Err(e) = window.set_position(LogicalPosition::new(x, y)) {
        warn!(error = %e, x, y, "failed to position launcher window");
    }
}

/// Resolve the monitor the cursor sits on. `monitor_from_point` is unreliable on
/// macOS/wry (often returns the primary monitor), so we walk the monitor list.
/// macOS reports the cursor in logical points scaled by the *primary* monitor's
/// factor, while each monitor's geometry is its own physical pixels, so convert
/// both to logical points before testing containment — otherwise the cursor
/// never matches a non-primary monitor and we fall back to primary.
fn cursor_monitor<R: Runtime>(app: &AppHandle<R>) -> Option<Monitor> {
    let cursor = app.cursor_position().ok()?;
    let primary_scale =
        app.primary_monitor().ok().flatten().map(|m| m.scale_factor()).unwrap_or(1.0);
    let (cx, cy) = (cursor.x / primary_scale, cursor.y / primary_scale);
    app.available_monitors().ok()?.into_iter().find(|m| {
        let s = m.scale_factor();
        let o = m.position();
        let sz = m.size();
        let (ox, oy) = (o.x as f64 / s, o.y as f64 / s);
        let (w, h) = (sz.width as f64 / s, sz.height as f64 / s);
        cx >= ox && cx < ox + w && cy >= oy && cy < oy + h
    })
}
