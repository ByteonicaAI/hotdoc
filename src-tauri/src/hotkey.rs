use anyhow::Result;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+Space";

static REGISTERED_COMBO: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn register(app: &AppHandle, combo: &str) -> Result<()> {
    app.global_shortcut().on_shortcut(combo, |app, _scut, event| {
        if event.state == ShortcutState::Pressed {
            if let Some(w) = app.get_webview_window("main") {
                // On Linux (X11): always_on_top is cleared when the window
                // is unmapped via hide(). Re-asserting it before set_focus()
                // causes the WM to transfer focus on remap. On GNOME/Mutter
                // specifically, _NET_WM_STATE_ABOVE being set at focus-request
                // time is what allows the focus steal to succeed.
                let _ = w.unminimize();
                crate::window_pos::center_on_active_monitor(app, &w);
                let _ = w.show();
                let _ = w.set_always_on_top(true);
                let _ = w.set_focus();
            }
        }
    })?;
    if let Ok(mut guard) = REGISTERED_COMBO.lock() {
        *guard = Some(combo.to_string());
    }
    Ok(())
}

pub fn unregister_current(app: &AppHandle) {
    let combo = REGISTERED_COMBO.lock().ok().and_then(|g| g.clone());
    match combo {
        Some(c) => {
            let _ = app.global_shortcut().unregister(c.as_str());
        }
        None => {
            let _ = app.global_shortcut().unregister_all();
        }
    }
}

pub fn default_combo() -> &'static str {
    DEFAULT_SHORTCUT
}
