use tauri::State;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tracing::{error, info, instrument};

use hotdoc_core::index::SearchHit;
use hotdoc_core::store::pinned::{self, PinnedHit};
use hotdoc_core::store::recents::{self, Recent};

use crate::index_state::AppState;

// ponytail: P1-3. The AppState now holds Arc<HotdocIndex>; HotdocIndex::search
// takes &self and is thread-safe via the inner tantivy::IndexReader, so no
// per-call lock is needed. Errors are still returned as Result<T, String>
// so the UI can toast (P1-3 from the prior review).
#[tauri::command]
#[instrument(skip(state))]
pub fn search(query: String, state: State<'_, AppState>) -> Result<Vec<SearchHit>, String> {
    let hits = state.index.search(&query, 8).map_err(|e| format!("search failed: {e:#}"))?;
    info!(query = %query, hits = hits.len(), "search");
    Ok(hits)
}

#[tauri::command]
#[instrument(skip(state, app))]
pub fn copy_syntax(
    query: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let hit = state
        .index
        .search(&query, 1)
        .map_err(|e| format!("search failed: {e:#}"))?
        .into_iter()
        .next();
    let Some(hit) = hit else { return Ok(None) };
    app.clipboard()
        .write_text(hit.syntax.clone())
        .map_err(|e| format!("clipboard write failed: {e}"))?;
    info!(query = %query, id = %hit.id, "copy_syntax");
    Ok(Some(hit.syntax))
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
pub fn record_recent(query: String, state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
    recents::record(&conn, &query).map_err(|e| format!("record_recent: {e:#}"))
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
