use crate::{DbConnection, ProxyResponse};
use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use tauri::State;

/// HTTP client for proxying requests to the remote Outline server.
pub struct HttpClient {
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()?;
        Ok(Self { client })
    }
}

/// Proxy an HTTP request to the remote Outline server.
pub async fn proxy_request(
    client: &reqwest::Client,
    server_url: &str,
    method: &str,
    path: &str,
    headers: Option<&HashMap<String, String>>,
    body: Option<&str>,
) -> Result<ProxyResponse> {
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    let method = reqwest::Method::from_bytes(method.as_bytes())?;

    let mut request = client.request(method, &url);

    if let Some(headers) = headers {
        for (key, value) in headers {
            // Skip host and content-length headers (reqwest sets these)
            let lower = key.to_lowercase();
            if lower == "host" || lower == "content-length" {
                continue;
            }
            request = request.header(key.as_str(), value.as_str());
        }
    }

    if let Some(body) = body {
        request = request.body(body.to_string());
    }

    let response = request.send().await?;
    let status = response.status().as_u16();

    let mut resp_headers = HashMap::new();
    for (key, value) in response.headers() {
        if let Ok(v) = value.to_str() {
            resp_headers.insert(key.to_string(), v.to_string());
        }
    }

    let resp_body = response.text().await?;

    Ok(ProxyResponse {
        status,
        headers: resp_headers,
        body: resp_body,
    })
}

/// Cache an API response to SQLite.
pub fn cache_response(
    conn: &Connection,
    server_id: &str,
    key: &str,
    method: &str,
    body: &str,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO api_cache (key, server_id, method, response) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![key, server_id, method, body.as_bytes()],
    )?;
    Ok(())
}

/// Read a cached API response from SQLite.
pub fn get_cached_response(
    conn: &Connection,
    server_id: &str,
    key: &str,
) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT response FROM api_cache WHERE server_id = ?1 AND key = ?2",
        rusqlite::params![server_id, key],
        |row| row.get::<_, Vec<u8>>(0),
    );

    match result {
        Ok(bytes) => Ok(Some(String::from_utf8(bytes)?)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// --- Tauri command ---

#[tauri::command]
pub async fn proxy_fetch(
    db: State<'_, DbConnection>,
    http: State<'_, HttpClient>,
    method: String,
    path: String,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
) -> Result<ProxyResponse, String> {
    // TODO: Resolve server URL from current webview context.
    // For now, use a placeholder. This will be wired up in Task 9
    // when per-server windows are implemented.
    let server_url = "https://placeholder.example.com";

    let response = proxy_request(
        &http.client,
        server_url,
        &method,
        &path,
        headers.as_ref(),
        body.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Cache GET responses
    if method == "GET" {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        let cache_key = format!("{}:{}", method, path);
        if let Err(e) = cache_response(&conn, "default", &cache_key, &method, &response.body) {
            tracing::warn!("Failed to cache response for {}: {}", cache_key, e);
        }
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE api_cache (
                key TEXT NOT NULL,
                server_id TEXT NOT NULL,
                method TEXT NOT NULL DEFAULT 'GET',
                response BLOB,
                updated_at TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (key, server_id)
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_cache_and_retrieve() {
        let conn = setup_db();
        let body = r#"{"ok":true,"data":[]}"#;

        cache_response(&conn, "server-1", "GET:/api/documents.list", "GET", body).unwrap();
        let cached = get_cached_response(&conn, "server-1", "GET:/api/documents.list").unwrap();

        assert_eq!(cached, Some(body.to_string()));
    }

    #[test]
    fn test_cache_miss() {
        let conn = setup_db();
        let result = get_cached_response(&conn, "server-1", "GET:/api/missing").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_cache_overwrite() {
        let conn = setup_db();

        cache_response(&conn, "server-1", "GET:/api/docs", "GET", "v1").unwrap();
        cache_response(&conn, "server-1", "GET:/api/docs", "GET", "v2").unwrap();

        let cached = get_cached_response(&conn, "server-1", "GET:/api/docs").unwrap();
        assert_eq!(cached, Some("v2".to_string()));
    }

    #[test]
    fn test_cache_per_server() {
        let conn = setup_db();

        cache_response(&conn, "s1", "GET:/api/docs", "GET", "s1-data").unwrap();
        cache_response(&conn, "s2", "GET:/api/docs", "GET", "s2-data").unwrap();

        let s1 = get_cached_response(&conn, "s1", "GET:/api/docs").unwrap();
        let s2 = get_cached_response(&conn, "s2", "GET:/api/docs").unwrap();

        assert_eq!(s1, Some("s1-data".to_string()));
        assert_eq!(s2, Some("s2-data".to_string()));
    }
}
