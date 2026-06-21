use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use hotdoc_core::cli::TOGGLE_PORT;
use hotdoc_core::index::{HotdocIndex, SearchHit};
use hotdoc_core::pack;

struct AppState {
    index: Mutex<HotdocIndex>,
}

fn bundled_packs_dir() -> PathBuf {
    PathBuf::from(env!("OUT_DIR")).join("bundled-packs")
}

fn dev_packs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("packs").join("curate")
}

fn load_or_build_index() -> anyhow::Result<HotdocIndex> {
    let candidates = [bundled_packs_dir(), dev_packs_dir()];
    for dir in &candidates {
        if dir.is_dir() {
            match pack::load_dir(dir) {
                Ok(packs) if !packs.is_empty() => {
                    eprintln!("hotdoc: loaded {} packs from {}", packs.len(), dir.display());
                    let tmp = std::env::temp_dir().join(format!(
                        "hotdoc-runtime-{}-{}",
                        std::process::id(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or(0)
                    ));
                    return HotdocIndex::build(&packs, &tmp);
                }
                Ok(_) => continue,
                Err(e) => eprintln!("hotdoc: failed to load {}: {e:#}", dir.display()),
            }
        }
    }
    anyhow::bail!("no packs found in bundled-packs or dev packs/curate")
}

#[tauri::command]
fn search(query: String, state: State<'_, AppState>) -> Vec<SearchHit> {
    let idx = state.index.lock().expect("index lock");
    idx.search(&query, 8).unwrap_or_default()
}

#[tauri::command]
fn copy_syntax(query: String, state: State<'_, AppState>, app: tauri::AppHandle) -> Option<String> {
    let idx = state.index.lock().expect("index lock");
    let hit = idx.search(&query, 1).ok()?.first().cloned()?;
    let _ = app.clipboard().write_text(hit.syntax.clone());
    Some(hit.syntax)
}

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

fn should_hide_on_focus_loss(event: &tauri::WindowEvent) -> bool {
    matches!(event, tauri::WindowEvent::Focused(false))
}

fn spawn_toggle_listener<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    std::thread::spawn(move || {
        let sock = match std::net::UdpSocket::bind(("127.0.0.1", TOGGLE_PORT)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "hotdoc: toggle listener bind failed on udp:{TOGGLE_PORT}: {e}. \
                     `hotdoc-cli toggle` will not work; Ctrl+Shift+Space still does."
                );
                return;
            }
        };
        let mut buf = [0u8; 4];
        loop {
            let (len, peer) = match sock.recv_from(&mut buf) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("hotdoc: toggle listener recv error: {e}");
                    continue;
                }
            };
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
                let _ = w.unminimize();
            }
            let _ = sock.send_to(&buf[..len], peer);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let index = match load_or_build_index() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("hotdoc: failed to build index: {e:#}");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .manage(AppState { index: Mutex::new(index) })
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![search, copy_syntax, hide_window])
        .on_window_event(|window, event| {
            if should_hide_on_focus_loss(event) {
                let _ = window.hide();
            }
        })
        .setup(|app| {
            use tauri_plugin_global_shortcut::ShortcutState;
            if let Some(w) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = app.primary_monitor() {
                    let mon_w = monitor.size().width as i32;
                    let win_w = 720i32;
                    let x = (mon_w - win_w) / 2;
                    let _ = w.set_position(tauri::PhysicalPosition::new(x, 60));
                }
            }
            spawn_toggle_listener(app.handle().clone());
            let shortcut = "Ctrl+Shift+Space";
            app.global_shortcut().on_shortcut(shortcut, |app, _scut, event| {
                if event.state == ShortcutState::Pressed {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            })?;
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
            bundled_packs_dir().is_dir() || dev_packs_dir().is_dir(),
            "neither bundled-packs nor packs/curate is present at build/test time"
        );
    }

    #[test]
    fn focus_loss_hides_only_on_blur() {
        assert!(should_hide_on_focus_loss(&tauri::WindowEvent::Focused(false)));
        assert!(!should_hide_on_focus_loss(&tauri::WindowEvent::Focused(true)));
        assert!(!should_hide_on_focus_loss(&tauri::WindowEvent::Destroyed));
    }
}
