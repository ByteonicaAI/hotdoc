mod commands;
mod hotkey;
mod index_state;
mod settings;
mod toggle;
mod tray;

use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};
use tracing::info;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(e) = hotdoc_core::logging::init() {
        eprintln!("hotdoc: logging init failed: {e:#}");
    }
    let db_path = hotdoc_core::store::default_db_path().expect("default db path");
    // FR-I5: migration failure → delete the corrupt DB and start fresh.
    let conn = {
        let try_open = || -> hotdoc_core::store::Result<rusqlite::Connection> {
            let c = hotdoc_core::store::open(&db_path)?;
            hotdoc_core::store::migrate(&c)?;
            Ok(c)
        };
        match try_open() {
            Ok(c) => c,
            Err(hotdoc_core::store::StoreError::MigrationFailed { step, ref message }) => {
                tracing::error!(step, message, "migration failed — deleting DB and rebuilding");
                let _ = std::fs::remove_file(&db_path);
                let c = hotdoc_core::store::open(&db_path).expect("open fresh DB");
                hotdoc_core::store::migrate(&c).expect("migrate fresh DB");
                c
            }
            Err(e) => panic!("open store: {e:#}"),
        }
    };
    info!(path = %db_path.display(), "store opened");

    // ponytail: build the index first (T16 — populates packs/entries
    // tables on the same conn), then wrap the conn in Arc<Mutex<>>.
    // The resolver takes &Connection by reference; we hand it the
    // un-wrapped conn here.
    let index = match index_state::load_or_build_index(&conn) {
        Ok(i) => i,
        Err(e) => {
            tracing::error!(error = %format!("{e:#}"), "failed to build index");
            std::process::exit(1);
        }
    };

    let popularity_map = hotdoc_core::store::popularity::weighted_counts(
        &conn,
        hotdoc_core::store::time::unix_now_ms(),
    )
    .unwrap_or_default();
    let db = Arc::new(Mutex::new(conn));

    tauri::Builder::default()
        .manage(index_state::AppState { index, db, popularity_map: Arc::new(popularity_map) })
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::copy_syntax,
            commands::hide_window,
            commands::log_error,
            commands::record_recent,
            commands::get_recents,
            commands::clear_recents,
            commands::pin_entry,
            commands::unpin_entry,
            commands::get_pinned,
            commands::list_packs,
            commands::list_pack_metas,
            commands::set_setting,
            commands::get_setting,
            commands::get_all_settings,
            commands::set_hotkey,
            commands::set_autostart,
            commands::rebuild_index,
            commands::open_url,
            commands::record_search,
            commands::get_popular,
            commands::index_status,
            commands::copy_diagnostics,
            commands::set_window_height
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Focused(false)) {
                let _ = window.hide();
            }
            if matches!(event, tauri::WindowEvent::Focused(true)) {
                let _ = window.emit("hotdoc://refresh-empty-view", ());
            }
        })
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = app.primary_monitor() {
                    // ponytail: FR-L2 / §8.2 — center-top, 60px logical from
                    // the top. monitor.size() is physical; the 720px window
                    // width and 60px margin are logical, so scale them by
                    // the monitor's factor before the physical-pixel math,
                    // else the window is off-center on fractional-scale
                    // displays.
                    let scale = monitor.scale_factor();
                    let mon_w = monitor.size().width as f64;
                    let win_w = 720.0 * scale;
                    let x = ((mon_w - win_w) / 2.0).max(0.0) as i32;
                    let y = (60.0 * scale) as i32;
                    let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                }
            }
            toggle::spawn(app.handle().clone());
            hotkey::register(app.handle(), hotkey::default_combo())?;
            tray::build(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_packs_or_dev_packs_present() {
        assert!(
            index_state::bundled_packs_dir().is_dir() || index_state::dev_packs_dir().is_dir(),
            "neither bundled-packs nor packs/curate is present at build/test time"
        );
    }

    #[test]
    fn focus_loss_hides_only_on_blur() {
        assert!(focus_loss_hides(&tauri::WindowEvent::Focused(false)));
        assert!(!focus_loss_hides(&tauri::WindowEvent::Focused(true)));
        assert!(!focus_loss_hides(&tauri::WindowEvent::Destroyed));
    }

    fn focus_loss_hides(event: &tauri::WindowEvent) -> bool {
        matches!(event, tauri::WindowEvent::Focused(false))
    }
}
