use crate::{DbConnection, ServerConfig};
use anyhow::Result;
use rusqlite::Connection;
use tauri::State;

/// Add a server to the database.
pub fn add_server(conn: &Connection, config: &ServerConfig) -> Result<()> {
    conn.execute(
        "INSERT INTO servers (id, name, url, offline_enabled) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![config.id, config.name, config.url, config.offline_enabled],
    )?;
    Ok(())
}

/// Remove a server from the database.
pub fn remove_server(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM servers WHERE id = ?1", [id])?;
    Ok(())
}

/// List all servers.
pub fn list_servers(conn: &Connection) -> Result<Vec<ServerConfig>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, url, offline_enabled, last_sync, created_at FROM servers ORDER BY created_at"
    )?;
    let servers = stmt
        .query_map([], |row| {
            Ok(ServerConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                offline_enabled: row.get::<_, i64>(3)? != 0,
                last_sync: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(servers)
}

/// Get a single server by ID.
pub fn get_server(conn: &Connection, id: &str) -> Result<Option<ServerConfig>> {
    let result = conn.query_row(
        "SELECT id, name, url, offline_enabled, last_sync, created_at FROM servers WHERE id = ?1",
        [id],
        |row| {
            Ok(ServerConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                offline_enabled: row.get::<_, i64>(3)? != 0,
                last_sync: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    );
    match result {
        Ok(config) => Ok(Some(config)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// --- Tauri commands ---

#[tauri::command]
pub fn get_server_list(db: State<'_, DbConnection>) -> Result<Vec<ServerConfig>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    list_servers(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_server_command(
    db: State<'_, DbConnection>,
    config: ServerConfig,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    add_server(&conn, &config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_server_command(db: State<'_, DbConnection>, id: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    remove_server(&conn, &id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                url TEXT NOT NULL,
                auth_token_encrypted BLOB,
                offline_enabled INTEGER DEFAULT 1,
                last_sync TEXT,
                created_at TEXT DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        conn
    }

    fn make_config(id: &str, name: &str, url: &str) -> ServerConfig {
        ServerConfig {
            id: id.into(),
            name: name.into(),
            url: url.into(),
            offline_enabled: true,
            last_sync: None,
            created_at: String::new(),
        }
    }

    #[test]
    fn test_add_and_list() {
        let conn = setup_db();
        add_server(&conn, &make_config("s1", "Server 1", "https://s1.com")).unwrap();
        add_server(&conn, &make_config("s2", "Server 2", "https://s2.com")).unwrap();
        let servers = list_servers(&conn).unwrap();
        assert_eq!(servers.len(), 2);
    }

    #[test]
    fn test_get_existing() {
        let conn = setup_db();
        add_server(&conn, &make_config("s1", "Server 1", "https://s1.com")).unwrap();
        let found = get_server(&conn, "s1").unwrap().unwrap();
        assert_eq!(found.name, "Server 1");
        assert_eq!(found.url, "https://s1.com");
    }

    #[test]
    fn test_get_missing() {
        let conn = setup_db();
        let result = get_server(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_remove() {
        let conn = setup_db();
        add_server(&conn, &make_config("s1", "Server 1", "https://s1.com")).unwrap();
        assert_eq!(list_servers(&conn).unwrap().len(), 1);
        remove_server(&conn, "s1").unwrap();
        assert_eq!(list_servers(&conn).unwrap().len(), 0);
    }

    #[test]
    fn test_offline_enabled_default() {
        let conn = setup_db();
        let config = ServerConfig {
            id: "s1".into(),
            name: "Test".into(),
            url: "https://test.com".into(),
            offline_enabled: false,
            last_sync: None,
            created_at: String::new(),
        };
        add_server(&conn, &config).unwrap();
        let found = get_server(&conn, "s1").unwrap().unwrap();
        assert!(!found.offline_enabled);
    }
}
