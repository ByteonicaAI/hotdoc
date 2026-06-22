//! System tray icon (spec FR-T1–T3).
//!
//! Right-click menu: Preferences / Reload index / Open data folder / Quit.
//! Left-click is NOT implemented on Linux — `tauri-2.11.3/src/tray/mod.rs:66`
//! marks `TrayIconEvent::Click` as "Linux: Unsupported. The event is not
//! emitted even though the icon is shown and will still show a context
//! menu on right click." The launcher window opens via the global hotkey
//! or the CLI toggle, not via tray-left-click.

use anyhow::Result;
use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Wry};
use tauri_plugin_opener::OpenerExt;
use tracing::info;

/// Menu item ids. Exposed as `pub const` so a unit test can pin them —
/// accidental rename would break user muscle memory and the
/// `hotdoc://open-settings` listener wiring.
pub const ID_PREFS: &str = "prefs";
pub const ID_RELOAD: &str = "reload";
pub const ID_OPEN_FOLDER: &str = "open_folder";
pub const ID_DIAGNOSTICS: &str = "diagnostics";
pub const ID_QUIT: &str = "quit";

const TOOLTIP: &str = "Hotdoc — Ctrl+Shift+Space";

// Embed the 32×32 icon at compile time. `include_bytes!` resolves
// relative to the source file; src-tauri/icons/32x32.png is the same
// file the bundle uses.
const ICON_BYTES: &[u8] = include_bytes!("../icons/32x32.png");

/// Build and attach the tray icon + context menu. Idempotent within a
/// single Tauri app instance.
pub fn build(app: &AppHandle<Wry>) -> Result<()> {
    let prefs = MenuItemBuilder::with_id(ID_PREFS, "Preferences").build(app)?;
    let reload = MenuItemBuilder::with_id(ID_RELOAD, "Reload index").build(app)?;
    let open_folder = MenuItemBuilder::with_id(ID_OPEN_FOLDER, "Open data folder").build(app)?;
    let diagnostics = MenuItemBuilder::with_id(ID_DIAGNOSTICS, "Copy diagnostics").build(app)?;
    let quit = MenuItemBuilder::with_id(ID_QUIT, "Quit").build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;

    let menu: Menu<Wry> = MenuBuilder::new(app)
        .items(&[&prefs, &reload, &open_folder, &diagnostics, &sep, &quit])
        .build()?;

    let icon = Image::from_bytes(ICON_BYTES)?;

    let _tray = TrayIconBuilder::with_id("hotdoc-tray")
        .icon(icon)
        .tooltip(TOOLTIP)
        .menu(&menu)
        .on_menu_event(|app, event| {
            info!(id = %event.id().as_ref(), "tray menu click");
            match event.id().as_ref() {
                ID_PREFS => {
                    let _ = app.emit("hotdoc://open-settings", ());
                }
                ID_RELOAD => {
                    // ponytail: T13 — tray "Reload index" now wired.
                    // Frontend listens for this event and calls the
                    // `rebuild_index` IPC command. The command deletes
                    // the persistent tantivy dir, rebuilds from the
                    // on-disk packs, and re-populates the SQLite
                    // `packs`/`entries` tables (T16). A toast in the
                    // launcher confirms the entry count after rebuild.
                    let _ = app.emit("hotdoc://reload-index", ());
                }
                ID_OPEN_FOLDER => {
                    if let Some(dir) = hotdoc_core::store::default_db_path()
                        .ok()
                        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    {
                        let _ =
                            app.opener().open_path(dir.to_string_lossy().to_string(), None::<&str>);
                    }
                }
                ID_DIAGNOSTICS => {
                    // ponytail: FR-G2 — frontend listens, calls the
                    // `copy_diagnostics` IPC (redacted bundle → clipboard)
                    // and toasts. Kept off the tray thread because the
                    // command needs AppState (the db handle).
                    let _ = app.emit("hotdoc://copy-diagnostics", ());
                }
                ID_QUIT => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_menu_id_is_quit() {
        // Guards the frontend's `on_menu_event` switch and the
        // user-visible menu order against accidental rename.
        assert_eq!(ID_QUIT, "quit");
        assert_eq!(ID_PREFS, "prefs");
        assert_eq!(ID_RELOAD, "reload");
        assert_eq!(ID_OPEN_FOLDER, "open_folder");
        assert_eq!(ID_DIAGNOSTICS, "diagnostics");
    }
}
