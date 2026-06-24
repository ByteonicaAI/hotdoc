use std::net::UdpSocket;

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tracing::{info, warn};

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
                warn!(port = TOGGLE_PORT, error = %e, "toggle listener bind failed; `hotdoc-cli toggle` will not work");
                return;
            }
        };
        info!(port = TOGGLE_PORT, "toggle listener bound");
        let mut buf = [0u8; 4];
        loop {
            let (len, peer) = match sock.recv_from(&mut buf) {
                Ok(v) => v,
                Err(e) => {
                    warn!(error = %e, "toggle listener recv error");
                    continue;
                }
            };
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
                let _ = w.unminimize();
                let _ = app.emit("hotdoc://show", ());
            }
            let _ = sock.send_to(&buf[..len], peer);
        }
    });
}
