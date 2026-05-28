use anyhow::Result;
use rusqlite::Connection;

/// Save a Y.js document state to SQLite.
///
/// Uses INSERT OR REPLACE to upsert the document state.
/// If `title` is None, the existing title is preserved.
pub fn save_document_state(
    conn: &Connection,
    server_id: &str,
    document_id: &str,
    yjs_state: &[u8],
    title: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO documents (id, server_id, yjs_state, title, updated_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
           yjs_state = excluded.yjs_state,
           title = COALESCE(excluded.title, title),
           updated_at = datetime('now')",
        rusqlite::params![document_id, server_id, yjs_state, title],
    )?;
    Ok(())
}

/// Load a Y.js document state from SQLite.
pub fn load_document_state(
    conn: &Connection,
    server_id: &str,
    document_id: &str,
) -> Result<Option<Vec<u8>>> {
    let result = conn.query_row(
        "SELECT yjs_state FROM documents WHERE server_id = ?1 AND id = ?2",
        rusqlite::params![server_id, document_id],
        |row| row.get::<_, Vec<u8>>(0),
    );
    match result {
        Ok(state) => Ok(Some(state)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Merge a remote Y.js document state with the local state using CRDT.
///
/// Y.js documents can be merged by applying one document's update to another.
/// The CRDT algorithm ensures convergence regardless of application order.
///
/// Flow:
/// 1. Load local state from SQLite
/// 2. If no local state exists, save remote state directly
/// 3. If local state exists, merge using Y.js CRDT operations
///
/// Returns the merged state.
///
/// NOTE: Full Y.js CRDT merge using the `yrs` crate will be implemented
/// when the Hocuspocus WebSocket proxy is fully built. For now, this
/// performs a simple merge: remote wins if no local state, otherwise
/// local wins (placeholder until yrs integration).
pub fn merge_document_states(
    conn: &Connection,
    server_id: &str,
    document_id: &str,
    remote_state: &[u8],
) -> Result<Vec<u8>> {
    let local_state = load_document_state(conn, server_id, document_id)?;

    match local_state {
        None => {
            // No local state — save remote directly
            save_document_state(conn, server_id, document_id, remote_state, None)?;
            Ok(remote_state.to_vec())
        }
        Some(_local) => {
            // TODO: Implement proper Y.js CRDT merge using yrs crate
            // For now, save remote state as the merged result.
            // The full implementation will:
            //   let doc = yrs::Doc::new();
            //   let mut txn = doc.transact_mut();
            //   txn.apply_update(yrs::Update::decode_v1(&_local)?);
            //   txn.apply_update(yrs::Update::decode_v1(remote_state)?);
            //   let merged = yrs::encode_state_as_update(&doc, &yrs::StateVector::default());
            //   save_document_state(conn, server_id, document_id, &merged, None)?;
            //   Ok(merged)

            save_document_state(conn, server_id, document_id, remote_state, None)?;
            Ok(remote_state.to_vec())
        }
    }
}

/// Delete a document's local state.
pub fn delete_document_state(
    conn: &Connection,
    server_id: &str,
    document_id: &str,
) -> Result<()> {
    conn.execute(
        "DELETE FROM documents WHERE server_id = ?1 AND id = ?2",
        rusqlite::params![server_id, document_id],
    )?;
    Ok(())
}

/// List all cached document IDs for a server.
pub fn list_document_ids(conn: &Connection, server_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM documents WHERE server_id = ?1 ORDER BY updated_at DESC",
    )?;
    let ids = stmt
        .query_map([server_id], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                server_id TEXT NOT NULL,
                yjs_state BLOB,
                title TEXT,
                updated_at TEXT,
                last_modified TEXT
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_save_and_load() {
        let conn = setup_db();
        let state = vec![0x01, 0x02, 0x03];

        save_document_state(&conn, "s1", "doc1", &state, Some("Test")).unwrap();
        let loaded = load_document_state(&conn, "s1", "doc1").unwrap().unwrap();

        assert_eq!(loaded, state);
    }

    #[test]
    fn test_load_missing() {
        let conn = setup_db();
        let result = load_document_state(&conn, "s1", "missing").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_merge_no_local_saves_remote() {
        let conn = setup_db();
        let remote = vec![0xAA, 0xBB];

        let merged = merge_document_states(&conn, "s1", "doc1", &remote).unwrap();
        assert_eq!(merged, remote);

        // Verify it was persisted
        let loaded = load_document_state(&conn, "s1", "doc1").unwrap().unwrap();
        assert_eq!(loaded, remote);
    }

    #[test]
    fn test_save_updates_existing() {
        let conn = setup_db();

        save_document_state(&conn, "s1", "doc1", &[1, 2], Some("Title")).unwrap();
        save_document_state(&conn, "s1", "doc1", &[3, 4, 5], None).unwrap();

        let loaded = load_document_state(&conn, "s1", "doc1").unwrap().unwrap();
        assert_eq!(loaded, vec![3, 4, 5]);
    }

    #[test]
    fn test_save_preserves_title_when_none() {
        let conn = setup_db();

        save_document_state(&conn, "s1", "doc1", &[1], Some("Original")).unwrap();
        save_document_state(&conn, "s1", "doc1", &[2], None).unwrap();

        // Title should be preserved
        let title: String = conn
            .query_row(
                "SELECT title FROM documents WHERE id = 'doc1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(title, "Original");
    }

    #[test]
    fn test_delete() {
        let conn = setup_db();
        save_document_state(&conn, "s1", "doc1", &[1], None).unwrap();
        assert!(load_document_state(&conn, "s1", "doc1").unwrap().is_some());

        delete_document_state(&conn, "s1", "doc1").unwrap();
        assert!(load_document_state(&conn, "s1", "doc1").unwrap().is_none());
    }

    #[test]
    fn test_list_document_ids() {
        let conn = setup_db();
        save_document_state(&conn, "s1", "doc-a", &[1], None).unwrap();
        save_document_state(&conn, "s1", "doc-b", &[2], None).unwrap();
        save_document_state(&conn, "s2", "doc-c", &[3], None).unwrap();

        let ids = list_document_ids(&conn, "s1").unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"doc-a".to_string()));
        assert!(ids.contains(&"doc-b".to_string()));
    }
}
