use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedOperation {
    pub id: i64,
    pub server_id: String,
    pub method: String,
    pub path: String,
    pub body: Option<String>,
    pub headers: Option<String>,
    pub created_at: String,
}

/// Enqueue a write operation for later replay.
pub fn enqueue(
    conn: &Connection,
    server_id: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
    headers: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO offline_queue (server_id, method, path, body, headers) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![server_id, method, path, body, headers],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Get all pending operations for a server, ordered by creation time.
pub fn get_pending(conn: &Connection, server_id: &str) -> Result<Vec<QueuedOperation>> {
    let mut stmt = conn.prepare(
        "SELECT id, server_id, method, path, body, headers, created_at
         FROM offline_queue
         WHERE server_id = ?1
         ORDER BY created_at ASC",
    )?;
    let ops = stmt
        .query_map([server_id], |row| {
            Ok(QueuedOperation {
                id: row.get(0)?,
                server_id: row.get(1)?,
                method: row.get(2)?,
                path: row.get(3)?,
                body: row.get(4)?,
                headers: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ops)
}

/// Remove a successfully replayed operation.
pub fn dequeue(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM offline_queue WHERE id = ?1", [id])?;
    Ok(())
}

/// Count pending operations for a server.
pub fn pending_count(conn: &Connection, server_id: &str) -> Result<u32> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM offline_queue WHERE server_id = ?1",
        [server_id],
        |row| row.get(0),
    )?;
    Ok(count as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE offline_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id TEXT NOT NULL,
                method TEXT NOT NULL,
                path TEXT NOT NULL,
                body TEXT,
                headers TEXT,
                created_at TEXT DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_enqueue_and_get_pending() {
        let conn = setup_db();

        enqueue(&conn, "s1", "POST", "/api/documents.create", Some(r#"{"title":"Doc"}"#), None).unwrap();
        enqueue(&conn, "s1", "PUT", "/api/documents.update", Some(r#"{"id":"1"}"#), None).unwrap();

        let pending = get_pending(&conn, "s1").unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].method, "POST");
        assert_eq!(pending[1].method, "PUT");
    }

    #[test]
    fn test_dequeue() {
        let conn = setup_db();

        let id = enqueue(&conn, "s1", "POST", "/api/test", None, None).unwrap();
        assert_eq!(pending_count(&conn, "s1").unwrap(), 1);

        dequeue(&conn, id).unwrap();
        assert_eq!(pending_count(&conn, "s1").unwrap(), 0);
    }

    #[test]
    fn test_pending_count_is_per_server() {
        let conn = setup_db();

        enqueue(&conn, "s1", "POST", "/api/a", None, None).unwrap();
        enqueue(&conn, "s1", "POST", "/api/b", None, None).unwrap();
        enqueue(&conn, "s2", "POST", "/api/c", None, None).unwrap();

        assert_eq!(pending_count(&conn, "s1").unwrap(), 2);
        assert_eq!(pending_count(&conn, "s2").unwrap(), 1);
    }

    #[test]
    fn test_fifo_order() {
        let conn = setup_db();

        enqueue(&conn, "s1", "POST", "/api/first", None, None).unwrap();
        enqueue(&conn, "s1", "POST", "/api/second", None, None).unwrap();
        enqueue(&conn, "s1", "POST", "/api/third", None, None).unwrap();

        let pending = get_pending(&conn, "s1").unwrap();
        assert_eq!(pending[0].path, "/api/first");
        assert_eq!(pending[1].path, "/api/second");
        assert_eq!(pending[2].path, "/api/third");
    }
}
