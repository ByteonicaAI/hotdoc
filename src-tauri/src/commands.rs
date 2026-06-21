use tauri::State;
use tauri_plugin_clipboard_manager::ClipboardExt;

use hotdoc_core::index::SearchHit;

use crate::index_state::AppState;

// ponytail: P1-3. The AppState now holds Arc<HotdocIndex>; HotdocIndex::search
// takes &self and is thread-safe via the inner tantivy::IndexReader, so no
// per-call lock is needed. Errors are still returned as Result<T, String>
// so the UI can toast (P1-3 from the prior review).
#[tauri::command]
pub fn search(query: String, state: State<'_, AppState>) -> Result<Vec<SearchHit>, String> {
    state.index.search(&query, 8).map_err(|e| format!("search failed: {e:#}"))
}

#[tauri::command]
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
    Ok(Some(hit.syntax))
}

#[tauri::command]
pub fn hide_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| format!("hide failed: {e}"))
}
