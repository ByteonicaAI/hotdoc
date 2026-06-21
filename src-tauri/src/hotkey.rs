use anyhow::Result;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+Space";

pub fn register(app: &AppHandle, combo: &str) -> Result<()> {
    app.global_shortcut().on_shortcut(combo, |app, _scut, event| {
        if event.state == ShortcutState::Pressed {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    })?;
    Ok(())
}

pub fn unregister_current(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}

pub fn default_combo() -> &'static str {
    DEFAULT_SHORTCUT
}
