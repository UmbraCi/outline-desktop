use anyhow::Result;
use serde::{Deserialize, Serialize};

/// WebSocket proxy mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProxyMode {
    /// Forward messages to remote server
    Online,
    /// Serve from local SQLite state
    Offline,
}

/// Hocuspocus-compatible document reference
#[derive(Debug, Clone)]
pub struct DocumentRef {
    pub document_id: String,
    pub name: String, // "document.<uuid>"
}

impl DocumentRef {
    /// Parse a document reference from a Hocuspocus document name.
    /// Returns None if the name doesn't match the expected format.
    pub fn from_name(name: &str) -> Option<Self> {
        let id = name.strip_prefix("document.")?;
        if id.is_empty() {
            return None;
        }
        Some(Self {
            document_id: id.to_string(),
            name: name.to_string(),
        })
    }

    /// Parse from a WebSocket URL path like "/collaboration/document.abc-123"
    pub fn from_path(path: &str) -> Option<Self> {
        let name = path.strip_prefix("/collaboration/")?;
        Self::from_name(name)
    }
}

/// Connection parameters extracted from the WebSocket URL query string.
#[derive(Debug, Clone)]
pub struct ConnectionParams {
    pub token: String,
    pub editor_version: Option<String>,
}

impl ConnectionParams {
    pub fn from_query(query: &str) -> Self {
        let mut token = String::new();
        let mut editor_version = None;

        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            match parts.next() {
                Some("token") => token = parts.next().unwrap_or("").to_string(),
                Some("editorVersion") => editor_version = parts.next().map(String::from),
                _ => {}
            }
        }

        Self {
            token,
            editor_version,
        }
    }
}

/// Configuration for a WebSocket proxy session.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    pub document_ref: DocumentRef,
    pub remote_url: String,
    pub connection_params: ConnectionParams,
}

impl ProxyConfig {
    /// Build the remote WebSocket URL for online mode.
    pub fn remote_ws_url(&self) -> String {
        format!(
            "{}/collaboration/{}?token={}&editorVersion={}",
            self.remote_url.trim_end_matches('/'),
            self.document_ref.name,
            urlencoding(&self.connection_params.token),
            self.connection_params
                .editor_version
                .as_deref()
                .unwrap_or("1.0.0"),
        )
    }
}

fn urlencoding(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_ref_from_name() {
        let doc = DocumentRef::from_name("document.abc-123").unwrap();
        assert_eq!(doc.document_id, "abc-123");
        assert_eq!(doc.name, "document.abc-123");
    }

    #[test]
    fn test_document_ref_invalid() {
        assert!(DocumentRef::from_name("invalid-name").is_none());
        assert!(DocumentRef::from_name("document").is_none());
        assert!(DocumentRef::from_name("document.").is_none());
    }

    #[test]
    fn test_document_ref_from_path() {
        let doc = DocumentRef::from_path("/collaboration/document.my-doc-42").unwrap();
        assert_eq!(doc.document_id, "my-doc-42");
    }

    #[test]
    fn test_document_ref_from_path_invalid() {
        assert!(DocumentRef::from_path("/realtime/socket").is_none());
        assert!(DocumentRef::from_path("/collaboration/").is_none());
    }

    #[test]
    fn test_connection_params_from_query() {
        let params = ConnectionParams::from_query("token=jwt-token-123&editorVersion=2.0.0");
        assert_eq!(params.token, "jwt-token-123");
        assert_eq!(params.editor_version, Some("2.0.0".to_string()));
    }

    #[test]
    fn test_connection_params_missing_version() {
        let params = ConnectionParams::from_query("token=jwt-abc");
        assert_eq!(params.token, "jwt-abc");
        assert_eq!(params.editor_version, None);
    }

    #[test]
    fn test_proxy_config_remote_url() {
        let config = ProxyConfig {
            mode: ProxyMode::Online,
            document_ref: DocumentRef::from_name("document.doc-1").unwrap(),
            remote_url: "https://wiki.example.com".to_string(),
            connection_params: ConnectionParams {
                token: "my-jwt".to_string(),
                editor_version: Some("1.5.0".to_string()),
            },
        };

        let url = config.remote_ws_url();
        assert!(url.starts_with("https://wiki.example.com/collaboration/document.doc-1"));
        assert!(url.contains("token=my-jwt"));
        assert!(url.contains("editorVersion=1.5.0"));
    }
}
