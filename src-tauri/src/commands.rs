use tauri::{State, WebviewWindow};
use tauri_plugin_clipboard_manager::ClipboardExt;

use hotdoc_core::index::SearchHit;

use crate::index_state::AppState;

// ponytail: P1-3. Old code returned Vec::new() on any search error and
// Option::None on any copy error, both silently — the UI showed "No
// matches" forever if the index went bad. Errors are now returned as
// Result<T, String> so the frontend can toast them.
#[tauri::command]
pub fn search(query: String, state: State<'_, AppState>) -> Result<Vec<SearchHit>, String> {
    let idx = state.index.lock().map_err(|e| format!("index lock poisoned: {e}"))?;
    idx.search(&query, 8).map_err(|e| format!("search failed: {e:#}"))
}

#[tauri::command]
pub fn copy_syntax(
    query: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let hit = {
        let idx = state.index.lock().map_err(|e| format!("index lock poisoned: {e}"))?;
        idx.search(&query, 1).map_err(|e| format!("search failed: {e:#}"))?.into_iter().next()
    };
    let Some(hit) = hit else { return Ok(None) };
    app.clipboard()
        .write_text(hit.syntax.clone())
        .map_err(|e| format!("clipboard write failed: {e}"))?;
    Ok(Some(hit.syntax))
}

#[tauri::command]
pub fn hide_window(window: WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| format!("hide failed: {e}"))
}
