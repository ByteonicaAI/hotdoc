use std::collections::HashMap;

use tauri::{LogicalSize, State, WebviewWindow};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;
use tracing::{error, info, instrument};

use hotdoc_core::index::SearchHit;
use hotdoc_core::store::packs;
use hotdoc_core::store::pinned::{self, PinnedHit};
use hotdoc_core::store::recents::{self, Recent};
use hotdoc_core::store::search_log;
use hotdoc_core::store::settings;

use crate::index_state;
use crate::index_state::AppState;
use crate::settings as hotkey_settings;

// M5-T2: index is now RwLock<Arc<HotdocIndex>>. Clone the Arc under a
// read lock (cheap), then release the lock before calling search so
// concurrent reads never block each other.
#[tauri::command]
#[instrument(skip(state))]
pub fn search(query: String, state: State<'_, AppState>) -> Result<Vec<SearchHit>, String> {
    let idx = state.index.read().map_err(|_| "index lock poisoned".to_string())?.clone();
    let hits = idx.search(&query, 8).map_err(|e| format!("search failed: {e:#}"))?;
    info!(query = %query, hits = hits.len(), "search");
    Ok(hits)
}

#[tauri::command]
#[instrument(skip(window))]
pub fn hide_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| format!("hide failed: {e}"))
}

// ponytail: spec FR-G1 + audit §4.2. Frontend forwards warn+/error entries
// here so they hit the rolling file log. The Tauri capability system grants
// custom commands in this crate's scope by default; no capabilities
// change needed. Best-effort by design — frontend never crashes if
// logging fails.
#[tauri::command]
#[instrument(skip_all, fields(level = %level))]
pub fn log_error(level: String, msg: String, context: Option<String>) -> Result<(), String> {
    match level.as_str() {
        "error" => {
            error!(target: "frontend", context = context.as_deref().unwrap_or(""), "{}", msg)
        }
        "warn" => {
            tracing::warn!(target: "frontend", context = context.as_deref().unwrap_or(""), "{}", msg)
        }
        "info" => {
            tracing::info!(target: "frontend", context = context.as_deref().unwrap_or(""), "{}", msg)
        }
        _ => return Err(format!("unknown log level: {level}")),
    }
    Ok(())
}

// Recents (spec FR-R1–R4). Thin IPC surface over hotdoc_core::store::recents.
// Frontend never sees rusqlite errors directly — map to a generic string so
// P1-3 logs the failure without surfacing internals.
#[tauri::command]
#[instrument(skip(state))]
pub fn record_recent(
    query: String,
    copied_syntax: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    recents::record(&conn, &query, copied_syntax.as_deref())
        .map_err(|e| format!("record_recent: {e:#}"))
}

#[tauri::command]
#[instrument(skip(state))]
pub fn get_recents(n: usize, state: State<'_, AppState>) -> Result<Vec<Recent>, String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    recents::top_n(&conn, n).map_err(|e| format!("get_recents: {e:#}"))
}

#[tauri::command]
#[instrument(skip(state))]
pub fn clear_recents(state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    recents::clear(&conn).map_err(|e| format!("clear_recents: {e:#}"))?;
    Ok(())
}

// Search log (spec §7.5 Popular, §9.2). Records one row per result
// activation. Honors the recents-enabled toggle (FR-R4) — the search log
// is the same flavor of local activity data as recents, so disabling
// recents disables it too. `first_id`/`clicked_id` may be null for a
// zero-result activation (the query is still logged).
#[tauri::command]
#[instrument(skip(state))]
pub fn record_search(
    query: String,
    first_id: Option<String>,
    clicked_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    if !recents::is_enabled(&conn).map_err(|e| format!("record_search: {e:#}"))? {
        return Ok(());
    }
    search_log::record(&conn, &query, first_id.as_deref(), clicked_id.as_deref())
        .map_err(|e| format!("record_search: {e:#}"))
}

// Index status (spec FR-I4). Counts feed the launcher footer. Read from
// the SQLite mirror (populated by the index pipeline, T16) so this needs
// no index internals.
#[derive(serde::Serialize)]
pub struct IndexStatus {
    pub entry_count: i64,
    pub pack_count: i64,
}

#[tauri::command]
#[instrument(skip(state))]
pub fn index_status(state: State<'_, AppState>) -> Result<IndexStatus, String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    let entry_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .map_err(|e| format!("index_status entries: {e}"))?;
    let pack_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM packs", [], |r| r.get(0))
        .map_err(|e| format!("index_status packs: {e}"))?;
    Ok(IndexStatus { entry_count, pack_count })
}

// Copy diagnostics (spec FR-G2). Builds a REDACTED bundle and writes it
// to the clipboard. Contents: app version, OS/arch, schema version, index
// counts. Deliberately excludes all query/recents/search_log text — the
// bundle is built from static environment + counts only, so no user query
// can leak. Returns the bundle for the toast.
#[tauri::command]
#[instrument(skip(state, app))]
pub fn copy_diagnostics(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    let entry_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0)).unwrap_or(-1);
    let pack_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM packs", [], |r| r.get(0)).unwrap_or(-1);
    let schema_version: String = conn
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
        .unwrap_or_else(|_| "unknown".to_string());
    drop(conn);

    let bundle = format!(
        "Hotdoc diagnostics\n\
         app_version: {}\n\
         os: {} {}\n\
         schema_version: {}\n\
         packs_indexed: {}\n\
         entries_indexed: {}\n\
         (redacted: no query, recents, or search-log contents)\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        schema_version,
        pack_count,
        entry_count,
    );
    app.clipboard()
        .write_text(bundle.clone())
        .map_err(|e| format!("copy_diagnostics clipboard: {e}"))?;
    info!("copied diagnostics bundle");
    Ok(bundle)
}

// Pinned (spec FR-P1–P3). The entries table is unpopulated in M3 (v1.1's
// persistent-index-reuse work will fill it), so `get_pinned` returns []
// until then. The pin/unpin IPC still works — it just won't surface in
// the empty view.
#[tauri::command]
#[instrument(skip(state))]
pub fn pin_entry(entry_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    pinned::add(&conn, &entry_id).map_err(|e| format!("pin_entry: {e:#}"))
}

#[tauri::command]
#[instrument(skip(state))]
pub fn unpin_entry(entry_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    pinned::remove(&conn, &entry_id).map_err(|e| format!("unpin_entry: {e:#}"))?;
    Ok(())
}

#[tauri::command]
#[instrument(skip(state))]
pub fn get_pinned(state: State<'_, AppState>) -> Result<Vec<PinnedHit>, String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    pinned::list(&conn).map_err(|e| format!("get_pinned: {e:#}"))
}

// Packs (T6 command palette: `> <pack_id>` filter validation).
// The `packs` table is unpopulated in M3, so this returns [] until v1.1's
// persistent-index-reuse work lands. The frontend treats [] as "no pack
// filter valid" and falls through to normal search.
#[tauri::command]
#[instrument(skip(state))]
pub fn list_packs(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    packs::list_ids(&conn).map_err(|e| format!("list_packs: {e:#}"))
}

#[tauri::command]
#[instrument(skip(state))]
pub fn list_pack_metas(state: State<'_, AppState>) -> Result<Vec<packs::PackMeta>, String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    packs::list_metas(&conn).map_err(|e| format!("list_pack_metas: {e:#}"))
}

// Settings (spec FR-X1–X4). `set_setting` writes to the `settings` table;
// `set_hotkey` validates against the app-level deny-list (T9) and asks the
// global-shortcut plugin to re-register. OS-refused registrations return a
// distinct error message so the UI can show a different badge per spec P1-7.
#[tauri::command]
#[instrument(skip(state))]
pub fn set_setting(key: String, value: String, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    settings::set(&conn, &key, &value).map_err(|e| format!("set_setting: {e:#}"))
}

#[tauri::command]
#[instrument(skip(state))]
pub fn get_setting(key: String, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    settings::get(&conn, &key).map_err(|e| format!("get_setting: {e:#}"))
}

#[tauri::command]
#[instrument(skip(state))]
pub fn get_all_settings(state: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    settings::all(&conn).map_err(|e| format!("get_all_settings: {e:#}"))
}

#[tauri::command]
#[instrument(skip(app))]
pub fn set_hotkey(combo: String, app: tauri::AppHandle) -> Result<(), String> {
    hotkey_settings::rebind(&app, &combo)
}

#[tauri::command]
#[instrument(skip(app))]
pub fn set_autostart(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
    let mgr = app.autolaunch();
    if enabled {
        mgr.enable().map_err(|e| format!("autostart enable: {e}"))
    } else {
        mgr.disable().map_err(|e| format!("autostart disable: {e}"))
    }
}

// M5-T2: tray "Reload index" → frontend emits hotdoc://reload-index →
// launcher.reloadIndex() calls this command. Build the new index while
// holding the db lock, then release the db lock before acquiring the
// index write lock to avoid any lock-order inversion.
#[tauri::command]
#[instrument(skip(state))]
pub fn rebuild_index(state: State<'_, AppState>) -> Result<usize, String> {
    let new_index = {
        let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
        index_state::reload_index(&conn).map_err(|e| format!("rebuild_index: {e:#}"))?
    };
    let entries = new_index.entry_count();
    *state.index.write().map_err(|_| "index write lock poisoned".to_string())? = new_index;
    info!(entries, "rebuild_index hot-swap complete");
    Ok(entries)
}

// ponytail: SEC-3 / FR-C3. Replaces the @tauri-apps/plugin-opener
// `openUrl` wrapper in the frontend. We deliberately re-check the
// scheme on the Rust side so a future frontend regression (or a
// tampered pack that bypasses the validate-time https gate via some
// other code path) can't `javascript:` or `file:` out of the host.
// Defense-in-depth: the validate_into gate catches poisoned packs at
// load time; this gate catches runtime mistakes. `opener.open_path`
// accepts URLs as well as filesystem paths — xdg-open handles the
// https: scheme transparently (see tray.rs for the existing
// data-folder use of open_path).
#[tauri::command]
#[instrument(skip(app))]
pub fn open_url(url: String, app: tauri::AppHandle) -> Result<(), String> {
    if !hotdoc_core::pack::is_https_url(&url) {
        return Err(format!("open_url: only https: URLs allowed, got {url:?}"));
    }
    app.opener().open_path(url, None::<&str>).map_err(|e| format!("open_url: {e}"))
}

// ponytail: WS-F phase 1 fix — frontend-driven compact/tall snap.
// Settings/About open force tall (820px); everything else is compact
// (660px). Window is centered both axes by center_on_active_monitor on
// subsequent activations; this command only resizes, it does not
// reposition. Height is clamped to [400, 1200] so a tampered frontend
// can't drive the window off-screen.
#[tauri::command]
#[instrument(skip(window))]
pub fn set_window_size(window: WebviewWindow, height: u32) -> Result<(), String> {
    let height = height.clamp(400, 1200) as f64;
    window
        .set_size(LogicalSize::new(crate::window_pos::WINDOW_WIDTH, height))
        .map_err(|e| format!("failed to set window size: {e}"))?;
    Ok(())
}
