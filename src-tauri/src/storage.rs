use anyhow::Result;
use std::path::PathBuf;
use tauri::AppHandle;

const DB_FILENAME: &str = "outline-desktop.db";

fn get_db_path(app_handle: &AppHandle) -> Result<PathBuf> {
    let app_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| anyhow::anyhow!("Failed to get app data dir: {}", e))?;
    std::fs::create_dir_all(&app_dir)?;
    Ok(app_dir.join(DB_FILENAME))
}

pub fn init_database(app_handle: &AppHandle) -> Result<rusqlite::Connection> {
    let db_path = get_db_path(app_handle)?;
    let conn = rusqlite::Connection::open(&db_path)?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS servers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            auth_token_encrypted BLOB,
            offline_enabled INTEGER DEFAULT 1,
            last_sync TEXT,
            created_at TEXT DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS documents (
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,
            yjs_state BLOB,
            title TEXT,
            updated_at TEXT,
            last_modified TEXT,
            FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS api_cache (
            key TEXT NOT NULL,
            server_id TEXT NOT NULL,
            method TEXT NOT NULL DEFAULT 'GET',
            response BLOB,
            updated_at TEXT DEFAULT (datetime('now')),
            PRIMARY KEY (key, server_id),
            FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS offline_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id TEXT NOT NULL,
            method TEXT NOT NULL,
            path TEXT NOT NULL,
            body TEXT,
            headers TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS attachments (
            id TEXT PRIMARY KEY,
            server_id TEXT NOT NULL,
            document_id TEXT,
            local_path TEXT,
            remote_url TEXT,
            content_type TEXT,
            size INTEGER,
            synced INTEGER DEFAULT 0,
            FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
        );",
    )?;

    tracing::info!("Database initialized at {:?}", db_path);
    Ok(conn)
}
