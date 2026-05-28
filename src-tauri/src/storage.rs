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
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Helper: create an in-memory DB with the full schema for testing.
    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
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
        ).unwrap();
        conn
    }

    #[test]
    fn test_schema_creates_all_tables() {
        let conn = setup_test_db();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"servers".to_string()), "missing servers table");
        assert!(tables.contains(&"documents".to_string()), "missing documents table");
        assert!(tables.contains(&"api_cache".to_string()), "missing api_cache table");
        assert!(tables.contains(&"offline_queue".to_string()), "missing offline_queue table");
        assert!(tables.contains(&"attachments".to_string()), "missing attachments table");
    }

    #[test]
    fn test_server_crud() {
        let conn = setup_test_db();

        // Insert
        conn.execute(
            "INSERT INTO servers (id, name, url) VALUES (?1, ?2, ?3)",
            ["test-id", "My Server", "https://wiki.example.com"],
        ).unwrap();

        // Read
        let name: String = conn
            .query_row("SELECT name FROM servers WHERE id = 'test-id'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name, "My Server");

        let url: String = conn
            .query_row("SELECT url FROM servers WHERE id = 'test-id'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(url, "https://wiki.example.com");

        // Update
        conn.execute(
            "UPDATE servers SET name = ?1 WHERE id = ?2",
            ["Updated Server", "test-id"],
        ).unwrap();
        let updated_name: String = conn
            .query_row("SELECT name FROM servers WHERE id = 'test-id'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(updated_name, "Updated Server");

        // Delete
        conn.execute("DELETE FROM servers WHERE id = 'test-id'", []).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM servers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_server_cascade_deletes_documents() {
        let conn = setup_test_db();

        // Insert server and document
        conn.execute(
            "INSERT INTO servers (id, name, url) VALUES (?1, ?2, ?3)",
            ["s1", "Server", "https://test.com"],
        ).unwrap();
        conn.execute(
            "INSERT INTO documents (id, server_id, title) VALUES (?1, ?2, ?3)",
            ["d1", "s1", "Test Doc"],
        ).unwrap();

        // Verify document exists
        let doc_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM documents WHERE server_id = 's1'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(doc_count, 1);

        // Delete server should cascade
        conn.execute("DELETE FROM servers WHERE id = 's1'", []).unwrap();
        let doc_count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
            .unwrap();
        assert_eq!(doc_count_after, 0);
    }

    #[test]
    fn test_api_cache_operations() {
        let conn = setup_test_db();

        conn.execute(
            "INSERT INTO servers (id, name, url) VALUES (?1, ?2, ?3)",
            ["s1", "Server", "https://test.com"],
        ).unwrap();

        // Insert cache entry
        conn.execute(
            "INSERT INTO api_cache (key, server_id, method, response) VALUES (?1, ?2, ?3, ?4)",
            ["GET:/api/docs", "s1", "GET", b"{\"ok\":true}"],
        ).unwrap();

        // Read cache
        let response: Vec<u8> = conn
            .query_row(
                "SELECT response FROM api_cache WHERE key = 'GET:/api/docs' AND server_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(response, b"{\"ok\":true}");

        // Upsert (replace)
        conn.execute(
            "INSERT OR REPLACE INTO api_cache (key, server_id, method, response) VALUES (?1, ?2, ?3, ?4)",
            ["GET:/api/docs", "s1", "GET", b"{\"v2\":true}"],
        ).unwrap();
        let updated: Vec<u8> = conn
            .query_row(
                "SELECT response FROM api_cache WHERE key = 'GET:/api/docs' AND server_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(updated, b"{\"v2\":true}");
    }
}
