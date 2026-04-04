mod commands;
mod hosts;

use commands::{AppState, SshPtySpawner};
use rterm_session::SessionManager;
use std::sync::Arc;

fn main() {
    let session_mgr = Arc::new(SessionManager::new("ssh"));
    let spawner = Arc::new(SshPtySpawner);

    tauri::Builder::default()
        .manage(AppState {
            session_mgr,
            spawner,
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_sessions,
            commands::create_session,
            commands::kill_session,
            commands::send_keys,
            commands::get_snapshot,
            commands::get_screen_snapshot,
            commands::resize_session,
            hosts::load_hosts,
            hosts::save_host,
            hosts::delete_host,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
