use anyhow::Result;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

// ponytail: spec FR-L1. M1-R2 (Wayland global-shortcut failure) is
// already mitigated by the toggle listener in toggle.rs — Ctrl+Shift+Space
// still works on X11/XWayland for free.
const SHORTCUT: &str = "Ctrl+Shift+Space";

pub fn register(app: &AppHandle) -> Result<()> {
    app.global_shortcut().on_shortcut(SHORTCUT, |app, _scut, event| {
        if event.state == ShortcutState::Pressed {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    })?;
    Ok(())
}
