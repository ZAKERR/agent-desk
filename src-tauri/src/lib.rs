mod config;
mod events;
mod session;
mod sse;
pub mod server;
mod process;
mod adapter;
mod focus;
pub mod tray;
mod remote;
pub mod island;
mod permission;
mod chat;

use std::sync::Arc;
use tauri::Manager;

pub fn run() {
    tracing_subscriber::fmt::init();

    let cfg = config::load_config();
    let port = cfg.manager.port;

    // Prevent duplicate instances: if port is already in use, exit quietly
    if std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok() {
        eprintln!("Agent Desk is already running on port {}. Exiting.", port);
        return;
    }

    let (app_state, tray_rx) = server::AppState::new(cfg);
    let state = Arc::new(app_state);

    // Start the HTTP+SSE server on a background tokio runtime
    let server_state = state.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async {
            server::run_server(server_state).await;
        });
    });

    // Give the HTTP server a moment to bind
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Build Tauri app
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            // Store AppHandle for notifications from api_signal
            let _ = state.app_handle.set(app.handle().clone());

            // Setup system tray
            tray::setup_tray(app, state.clone())?;

            // Setup Dynamic Island window
            if let Some(w) = app.get_webview_window("island") {
                let _ = w.eval(&format!("window.API_PORT={}", port));
                let _ = w.set_skip_taskbar(true);
                island::setup(&w);
            }

            // Tray updater thread: refreshes icon, tooltip, and menu
            let tray_state = state.clone();
            let tray_handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    // Block up to 3s, or wake immediately on signal from api_signal
                    let _ = tray_rx.recv_timeout(std::time::Duration::from_secs(3));

                    if tray_state.app_handle.get().is_some() {
                        let processes = server::scan_and_merge(&tray_state);
                        let status = server::compute_state(&processes);
                        tray::update_tray(&tray_handle, &tray_state, &status, &processes);
                    }
                }
            });

            tracing::info!("Agent Desk running — http://localhost:{}", port);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
