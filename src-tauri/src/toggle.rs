use std::net::UdpSocket;

use tauri::{AppHandle, Manager, Runtime};

use hotdoc_core::cli::TOGGLE_PORT;

// ponytail: P1-3. The DE binds a shortcut to 'hotdoc-cli toggle' (the
// vicinae model). hotdoc-cli sends a single UDP byte; this thread
// receives it, shows + focuses the window, acks back. Std-thread is
// fine here because the lifetime matches the AppHandle — when the
// runtime exits, the socket closes and the recv_from returns an error.
// A real graceful-shutdown signal is a P2 (deferred to M2).
pub fn spawn<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || {
        let sock = match UdpSocket::bind(("127.0.0.1", TOGGLE_PORT)) {
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
