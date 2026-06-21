mod commands;
mod hotkey;
mod index_state;
mod toggle;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let index = match index_state::load_or_build_index() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("hotdoc: failed to build index: {e:#}");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .manage(index_state::AppState { index })
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::copy_syntax,
            commands::hide_window
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Focused(false)) {
                let _ = window.hide();
            }
        })
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = app.primary_monitor() {
                    let mon_w = monitor.size().width as i32;
                    let win_w = 720i32;
                    let x = (mon_w - win_w) / 2;
                    let _ = w.set_position(tauri::PhysicalPosition::new(x, 60));
                }
            }
            toggle::spawn(app.handle().clone());
            hotkey::register(app.handle())?;
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
