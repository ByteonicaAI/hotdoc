//! Hotkey rebind + deny-list (spec FR-X1, T9).
//!
//! `validate_hotkey` is a pure function — testable without a Tauri runtime.
//! `rebind` wraps the global-shortcut plugin's `unregister_all` + `on_shortcut`
//! and surfaces two distinct errors: app-level deny-list rejection (this
//! crate's `APP_RESERVED`) and OS-level refusal at registration time (any
//! `Err` from the plugin). Per spec P1-7 the frontend retains the setting
//! value in both cases; only the displayed badge differs.

use anyhow::Result;
use tauri::AppHandle;
use tracing::{info, warn};

use crate::hotkey;

/// App-level reserved shortcuts. These parse and register fine via
/// `global-hotkey` (verified against `global-hotkey-0.8.0/src/hotkey.rs`),
/// but intercepting them would break copy/paste/undo in every other app
/// the user has open. Reject them at the app layer.
/// Linux-only set per spec §10 platform scope.
pub const APP_RESERVED: &[&str] = &["Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+Z", "Ctrl+Y", "Ctrl+A"];

/// Pure validation: returns Err when `combo` is in the app-level deny-list.
/// Caller surfaces the error string verbatim in the UI.
pub fn validate_hotkey(combo: &str) -> Result<(), String> {
    if APP_RESERVED.iter().any(|r| r.eq_ignore_ascii_case(combo.trim())) {
        return Err(format!(
            "{combo} is reserved — rebind it via your desktop environment instead"
        ));
    }
    Ok(())
}

/// Re-register the global hotkey. Distinguishes app-level rejection
/// (this crate's deny-list) from OS-level refusal (the plugin errored
/// at registration time). Both return `Result<(), String>`; the
/// frontend uses the message text to choose the badge color.
pub fn rebind(app: &AppHandle, combo: &str) -> Result<(), String> {
    validate_hotkey(combo)?;
    info!(combo = %combo, "rebinding hotkey");
    hotkey::unregister_current(app);
    hotkey::register(app, combo).map_err(|e| {
        warn!(combo = %combo, error = %format!("{e:#}"), "OS refused hotkey");
        format!("OS refused to register {combo}: {e}. Setting retained — check your desktop environment's shortcut bindings.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebind_rejects_reserved() {
        for combo in APP_RESERVED {
            let err = validate_hotkey(combo).expect_err("reserved should reject");
            assert!(err.contains("reserved"), "msg mentions reserved: {err}");
        }
    }

    #[test]
    fn rebind_accepts_valid_combo() {
        assert!(validate_hotkey("Ctrl+Shift+K").is_ok());
        assert!(validate_hotkey("Alt+Space").is_ok());
        assert!(validate_hotkey("Super+Z").is_ok());
    }

    #[test]
    fn case_insensitive_reserved_match() {
        // global-hotkey's parser is case-insensitive; deny-list should be too.
        assert!(validate_hotkey("ctrl+c").is_err());
        assert!(validate_hotkey("CTRL+V").is_err());
    }

    #[test]
    fn trims_whitespace() {
        assert!(validate_hotkey("  Ctrl+C  ").is_err());
        assert!(validate_hotkey("  Ctrl+Shift+K  ").is_ok());
    }
}
