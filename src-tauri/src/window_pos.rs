use tauri::{AppHandle, LogicalPosition, Monitor, Runtime, WebviewWindow};
use tracing::warn;

/// Logical window width, mirroring `app.windows[0].width` in tauri.conf.json.
/// `pub(crate)` so the `set_window_size` IPC command in commands.rs can
/// keep width constant when the frontend asks to resize only height.
pub(crate) const WINDOW_WIDTH: f64 = 720.0;
/// Compact logical height (input + 5 entries + footer). Matches
/// `app.windows[0].height` in tauri.conf.json. Tall state (820) is driven
/// by the frontend via the `set_window_size` IPC command.
const WINDOW_HEIGHT: f64 = 660.0;
/// Minimum gap from the cursor monitor's top — keeps the window off the
/// very top edge on tall secondary displays; ignored if the monitor is
/// shorter than the window.
const MIN_TOP_MARGIN: f64 = 60.0;

/// Center the window on the monitor under the cursor, both axes. Falls
/// back to the primary monitor when the cursor's monitor can't be
/// resolved. Falls back to `MIN_TOP_MARGIN` from the top of whichever
/// monitor is selected when that monitor is shorter than the window.
pub fn center_on_active_monitor<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let monitor = cursor_monitor(app).or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        warn!("no monitor resolved; leaving launcher window position unchanged");
        return;
    };
    let scale = monitor.scale_factor();
    let origin = monitor.position();
    let size = monitor.size();
    let ox = origin.x as f64 / scale;
    let oy = origin.y as f64 / scale;
    let w = size.width as f64 / scale;
    let h = size.height as f64 / scale;
    // x: center horizontally on monitor
    let x = ox + ((w - WINDOW_WIDTH) / 2.0).max(0.0);
    // y: center vertically; clamp to min top margin if window is taller
    // than monitor so it never escapes the visible area entirely.
    let y_raw = oy + ((h - WINDOW_HEIGHT) / 2.0);
    let y = if h >= WINDOW_HEIGHT { y_raw.max(oy + MIN_TOP_MARGIN) } else { oy + MIN_TOP_MARGIN };
    if let Err(e) = window.set_position(LogicalPosition::new(x, y)) {
        warn!(error = %e, x, y, "failed to position launcher window");
    }
}

/// Resolve the monitor the cursor sits on, comparing the physical cursor
/// position against each monitor's physical bounds. Mixing logical
/// (per-monitor scale) and physical coords mis-selected the monitor on
/// mixed-DPI multi-head setups; physical-vs-physical is scale-agnostic.
fn cursor_monitor<R: Runtime>(app: &AppHandle<R>) -> Option<Monitor> {
    let cursor = app.cursor_position().ok()?; // physical
    let (cx, cy) = (cursor.x, cursor.y);
    app.available_monitors().ok()?.into_iter().find(|m| {
        let o = m.position(); // physical
        let sz = m.size(); // physical
        let (ox, oy) = (o.x as f64, o.y as f64);
        let (w, h) = (sz.width as f64, sz.height as f64);
        cx >= ox && cx < ox + w && cy >= oy && cy < oy + h
    })
}
