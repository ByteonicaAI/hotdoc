mod commands;
mod hotkey;
mod index_state;
mod settings;
mod toggle;
mod tray;
mod window_pos;

use std::sync::{Arc, Mutex, RwLock};

use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
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

    // T1 (post-v1 bundling fix): the index used to be built here, before
    // `.setup()`, using a compile-time `env!("OUT_DIR")` path that only
    // exists on the build machine — a shipped bundle resolved zero packs.
    // Building it now happens inside `.setup()` (below), where
    // `app.path().resource_dir()` is available and resolves to the
    // packs the bundler actually shipped (see `bundle.resources` in
    // tauri.conf.json + `index_state::resource_packs_dir`). We still
    // wrap the connection in `Arc<Mutex<>>` here so `.setup()` can lock
    // it to build the index and then hand the same `Arc` to `AppState`.
    let db = Arc::new(Mutex::new(conn));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                window_pos::center_on_active_monitor(app, &w);
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::search,
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
            commands::set_window_size,
            commands::index_status,
            commands::copy_diagnostics
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Focused(false)) {
                let _ = window.hide();
            }
            if matches!(event, tauri::WindowEvent::Focused(true)) {
                let _ = window.emit("hotdoc://refresh-empty-view", ());
            }
        })
        .setup(move |app| {
            // T1: resolve the shipped-packs dir via the Tauri resource
            // API (only available now that we're inside `.setup()`),
            // then build/reuse the index. `resource_packs_dir` returns
            // `None` (after logging) when the resource dir can't be
            // resolved — `resolve_and_build` (via `load_or_build_index`)
            // falls back to the dev packs dir in that case, it never
            // panics on a missing resource dir.
            let bundled = index_state::resource_packs_dir(app.handle());
            let index = {
                let conn = match db.lock() {
                    Ok(c) => c,
                    Err(_) => {
                        tracing::error!("db mutex poisoned before index build");
                        std::process::exit(1);
                    }
                };
                match index_state::load_or_build_index(&conn, bundled.as_deref()) {
                    Ok(i) => i,
                    Err(e) => {
                        tracing::error!(error = %format!("{e:#}"), "failed to build index");
                        std::process::exit(1);
                    }
                }
            };
            app.manage(index_state::AppState { index: RwLock::new(index), db });

            if let Some(w) = app.get_webview_window("main") {
                // ponytail: FR-L2 / §8.2 — center both axes on the cursor
                // monitor (or primary if cursor unresolvable), accounting for
                // window height. Same path the hotkey/tray/single-instance
                // plugins take on subsequent activations. Falls back to a
                // 60px top margin if the monitor is shorter than the window.
                window_pos::center_on_active_monitor(app.handle(), &w);
            }
            toggle::spawn(app.handle().clone());
            hotkey::register(app.handle(), hotkey::default_combo())?;
            tray::build(app.handle())?;
            // ponytail: FR-T3 first-launch notification. Reads the
            // persisted `first_launch_shown` flag from the settings
            // table; on a fresh install the row is absent, so we fire
            // one notification using the current default hotkey label
            // and then write the flag so subsequent launches are
            // silent. Any failure (notification permission denied,
            // settings DB error, dbus down) is logged and swallowed —
            // a missing hint is never fatal.
            match hotdoc_core::store::open(&db_path) {
                Ok(conn) => {
                    let already_shown =
                        hotdoc_core::store::settings::get(&conn, "first_launch_shown")
                            .ok()
                            .flatten()
                            .is_some();
                    if !already_shown {
                        let combo = crate::hotkey::default_combo();
                        if let Err(e) = app
                            .notification()
                            .builder()
                            .title("Hotdoc")
                            .body(format!("Press {combo} to open Hotdoc"))
                            .show()
                        {
                            tracing::warn!(
                                error = %format!("{e:#}"),
                                "first-launch notification failed"
                            );
                        }
                        if let Err(e) =
                            hotdoc_core::store::settings::set(&conn, "first_launch_shown", "1")
                        {
                            tracing::warn!(
                                error = %format!("{e:#}"),
                                "first-launch: settings::set failed"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %format!("{e:#}"),
                        "first-launch: store::open failed; skipping notification"
                    );
                }
            }
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
