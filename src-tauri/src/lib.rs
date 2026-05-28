pub mod api_proxy;
pub mod offline_queue;
pub mod server_manager;
pub mod storage;
pub mod sync_engine;
pub mod system;
pub mod ws_proxy;

use api_proxy::HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub offline_enabled: bool,
    pub last_sync: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineStatus {
    pub is_online: bool,
    pub pending_operations: u32,
    pub last_sync: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

pub struct DbConnection(pub Mutex<rusqlite::Connection>);

// Stub commands for DesktopBridge compatibility.
// These prevent runtime errors when Outline calls bridge methods
// that don't have full implementations yet.

#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn restart_app() {}

#[tauri::command]
fn restart_and_install() {}

#[tauri::command]
fn check_for_updates() {}

#[tauri::command]
fn on_titlebar_double_click() {}

#[tauri::command]
fn on_logout() {}

#[tauri::command]
fn add_custom_host(_host: String) {}

#[tauri::command]
fn set_spell_checker_languages(_languages: Vec<String>) {}

#[tauri::command]
fn set_notification_count(_count: u32) {}

#[tauri::command]
fn get_auto_launch() -> bool {
    false
}

#[tauri::command]
fn set_auto_launch(_enabled: bool) {}

#[tauri::command]
fn switch_server(_id: String) {}

#[tauri::command]
fn get_offline_status() -> crate::OfflineStatus {
    crate::OfflineStatus {
        is_online: true,
        pending_operations: 0,
        last_sync: None,
    }
}

#[tauri::command]
fn sync_now() {}

pub fn run() {
    tracing_subscriber::fmt::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Initialize SQLite database
            let conn = storage::init_database(&app_handle)?;
            app.manage(DbConnection(Mutex::new(conn)));

            // Initialize HTTP client for API proxy
            let http_client = HttpClient::new().expect("Failed to create HTTP client");
            app.manage(http_client);

            // Setup system tray
            system::setup_tray(&app_handle)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            server_manager::get_server_list,
            server_manager::add_server_command,
            server_manager::remove_server_command,
            server_manager::open_server_window,
            api_proxy::proxy_fetch,
            get_version,
            restart_app,
            restart_and_install,
            check_for_updates,
            on_titlebar_double_click,
            on_logout,
            add_custom_host,
            set_spell_checker_languages,
            set_notification_count,
            get_auto_launch,
            set_auto_launch,
            switch_server,
            get_offline_status,
            sync_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
