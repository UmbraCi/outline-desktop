# Outline Desktop Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a cross-platform Tauri desktop client for self-hosted Outline with offline editing via Y.js CRDT sync.

**Architecture:** WebView wrapper loading remote Outline server, with Tauri Rust backend proxying API/WebSocket requests, caching data in SQLite, and implementing Hocuspocus-compatible Y.js sync for offline editing.

**Tech Stack:** Tauri 2 (Rust), SQLite (rusqlite), reqwest (HTTP), tokio-tungstenite (WebSocket), yrs + y-sync (Y.js CRDT), serde/serde_json (data)

---

## File Structure

```
outline-desktop/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json
│   ├── src/
│   │   ├── main.rs                      # Entry point, app setup, Tauri commands
│   │   ├── lib.rs                       # Module declarations, shared types
│   │   ├── storage.rs                   # SQLite schema, CRUD operations
│   │   ├── server_manager.rs            # Server config management, connection state
│   │   ├── api_proxy.rs                 # HTTP proxy, fetch interception, response caching
│   │   ├── ws_proxy.rs                  # WebSocket proxy for Hocuspocus + Socket.IO
│   │   ├── sync_engine.rs              # Y.js CRDT merge logic
│   │   ├── offline_queue.rs            # Offline write operation queue
│   │   └── system.rs                   # System tray, notifications
│   └── inject/
│       ├── bridge.js                   # TauriBridge object injection
│       └── proxy.js                    # fetch/WebSocket proxy injection
├── src/                                # Settings UI (Vite + TypeScript)
│   ├── main.ts
│   ├── style.css
│   └── index.html
└── package.json
```

---

## Phase 1: Tauri Project Scaffold

### Task 1: Initialize Tauri project

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`
- Create: `src-tauri/inject/bridge.js`
- Create: `src-tauri/inject/proxy.js`
- Create: `package.json`
- Create: `src/index.html`
- Create: `src/main.ts`
- Create: `src/style.css`

- [ ] **Step 1: Create `src-tauri/Cargo.toml`**

```toml
[package]
name = "outline-desktop"
version = "0.1.0"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon", "image-png"] }
tauri-plugin-shell = "2"
tauri-plugin-notification = "2"
tauri-plugin-updater = "2"
tauri-plugin-autostart = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.31", features = ["bundled"] }
reqwest = { version = "0.12", features = ["json", "cookies"] }
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = { version = "0.26", features = ["native-tls"] }
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
aes-gcm = "0.10"
base64 = "0.22"
url = "2"
```

- [ ] **Step 2: Create `src-tauri/tauri.conf.json`**

```json
{
  "$schema": "https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-config-schema/schema.json",
  "productName": "Outline Desktop",
  "version": "0.1.0",
  "identifier": "com.outline.desktop",
  "build": {
    "frontendDist": "../src",
    "devUrl": "http://localhost:5173",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "windows": [
      {
        "title": "Outline Desktop",
        "width": 1200,
        "height": 800,
        "resizable": true,
        "fullscreen": false
      }
    ],
    "security": {
      "csp": "default-src 'self' https:; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  },
  "plugins": {
    "updater": {
      "pubkey": "",
      "endpoints": []
    }
  }
}
```

- [ ] **Step 3: Create `src-tauri/capabilities/default.json`**

```json
{
  "identifier": "default",
  "description": "Default capabilities for Outline Desktop",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "shell:allow-open",
    "notification:default",
    "autostart:default",
    "updater:default"
  ]
}
```

- [ ] **Step 4: Create `src-tauri/src/main.rs`**

```rust
// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
use outline_desktop::storage;
use outline_desktop::system;

fn main() {
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
            storage::init_database(&app_handle)?;

            // Setup system tray
            system::setup_tray(&app_handle)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5: Create `src-tauri/src/lib.rs`**

```rust
pub mod storage;
pub mod server_manager;
pub mod api_proxy;
pub mod ws_proxy;
pub mod sync_engine;
pub mod offline_queue;
pub mod system;

use serde::{Deserialize, Serialize};

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
pub struct ProxyRequest {
    pub method: String,
    pub path: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyResponse {
    pub status: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub body: String,
}
```

- [ ] **Step 6: Create `src-tauri/inject/bridge.js`**

```javascript
// Injected into the WebView before page load.
// Creates window.TauriBridge compatible with Outline's DesktopBridge interface.

(function () {
  const { invoke } = window.__TAURI__.core;

  window.TauriBridge = {
    // --- Outline DesktopBridge compatibility ---
    redirect(path) {
      // Navigation within the webview
      window.location.href = path;
    },

    updateDownloaded(callback) {
      window.__TAURI__.event.listen("update-downloaded", callback);
    },

    restartAndInstall() {
      invoke("restart_and_install");
    },

    focus(callback) {
      window.__TAURI__.event.listen("window-focus", callback);
    },

    blur(callback) {
      window.__TAURI__.event.listen("window-blur", callback);
    },

    openKeyboardShortcuts(callback) {
      window.__TAURI__.event.listen("open-keyboard-shortcuts", callback);
    },

    getAutoLaunch() {
      return invoke("get_auto_launch");
    },

    setAutoLaunch(enabled) {
      return invoke("set_auto_launch", { enabled });
    },

    // --- Tauri extensions ---
    getServerList() {
      return invoke("get_server_list");
    },

    addServer(config) {
      return invoke("add_server", { config });
    },

    removeServer(id) {
      return invoke("remove_server", { id });
    },

    switchServer(id) {
      return invoke("switch_server", { id });
    },

    getOfflineStatus() {
      return invoke("get_offline_status");
    },

    syncNow() {
      return invoke("sync_now");
    },

    // API proxy - called by the fetch override
    proxyFetch(method, path, headers, body) {
      return invoke("proxy_fetch", { method, path, headers, body });
    },
  };
})();
```

- [ ] **Step 7: Create `src-tauri/inject/proxy.js`**

```javascript
// Injected into the WebView before page load.
// Overrides window.fetch and window.WebSocket to route through Tauri backend.

(function () {
  // Block remote Service Worker registration
  Object.defineProperty(navigator, "serviceWorker", {
    value: undefined,
    writable: false,
  });

  const { invoke } = window.__TAURI__.core;
  const originalFetch = window.fetch.bind(window);
  const originalWebSocket = window.WebSocket;

  // --- Fetch proxy for /api/* ---
  window.fetch = async function (input, init) {
    const url = typeof input === "string" ? input : input.url;

    // Only intercept API requests
    if (url.startsWith("/api/") || url.includes("/api/")) {
      const method = init?.method || "GET";
      const headers = {};
      if (init?.headers) {
        const h = init.headers instanceof Headers
          ? Object.fromEntries(init.headers.entries())
          : init.headers;
        Object.assign(headers, h);
      }
      const body = init?.body || null;

      try {
        const response = await invoke("proxy_fetch", {
          method,
          path: url,
          headers,
          body: body ? String(body) : null,
        });

        const responseBody = response.body || "";
        const responseHeaders = new Headers(response.headers || {});
        return new Response(responseBody, {
          status: response.status,
          statusText: response.status === 200 ? "OK" : "Error",
          headers: responseHeaders,
        });
      } catch (err) {
        console.error("[TauriProxy] fetch error:", err);
        return new Response(
          JSON.stringify({ error: "Offline or proxy error" }),
          { status: 503, headers: { "Content-Type": "application/json" } }
        );
      }
    }

    // Non-API requests: use original fetch
    return originalFetch(input, init);
  };

  // --- WebSocket proxy for /collaboration and /realtime ---
  const OriginalWS = window.WebSocket;

  function ProxiedWebSocket(url, protocols) {
    if (url.includes("/collaboration") || url.includes("/realtime")) {
      // Route through Tauri backend
      const proxyUrl = `ws://127.0.0.1:__TAURI_WS_PORT__${new URL(url, window.location.origin).pathname}${new URL(url, window.location.origin).search}`;
      return new OriginalWS(proxyUrl, protocols);
    }
    // Non-proxied WebSocket: use original
    return new OriginalWS(url, protocols);
  }

  ProxiedWebSocket.CONNECTING = OriginalWS.CONNECTING;
  ProxiedWebSocket.OPEN = OriginalWS.OPEN;
  ProxiedWebSocket.CLOSING = OriginalWS.CLOSING;
  ProxiedWebSocket.CLOSED = OriginalWS.CLOSED;

  window.WebSocket = ProxiedWebSocket;
})();
```

- [ ] **Step 8: Create `package.json` (project root)**

```json
{
  "name": "outline-desktop",
  "version": "0.1.0",
  "private": true,
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "tauri": "tauri",
    "tauri:dev": "tauri dev",
    "tauri:build": "tauri build"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "vite": "^6",
    "typescript": "^5"
  },
  "dependencies": {
    "@tauri-apps/api": "^2"
  }
}
```

- [ ] **Step 9: Create `src/index.html`**

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Outline Desktop</title>
    <link rel="stylesheet" href="./style.css" />
  </head>
  <body>
    <div id="app">
      <h1>Outline Desktop</h1>
      <p>Settings and server management</p>
    </div>
    <script type="module" src="./main.ts"></script>
  </body>
</html>
```

- [ ] **Step 10: Create `src/main.ts`**

```typescript
import { invoke } from "@tauri-apps/api/core";

console.log("Outline Desktop loaded");
```

- [ ] **Step 11: Create `src/style.css`**

```css
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  background: #1a1a2e;
  color: #e0e0e0;
}

#app {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 100vh;
}
```

- [ ] **Step 12: Verify the project builds**

Run: `cd outline-desktop && npm install && cd src-tauri && cargo check`
Expected: Compilation succeeds with no errors.

- [ ] **Step 13: Commit**

```bash
cd D:/code/outline-desktop
git add -A
git commit -m "feat: initialize Tauri project scaffold

Set up Tauri 2 project with Cargo dependencies, config,
injection scripts for TauriBridge and fetch/WebSocket proxy,
and placeholder frontend."
```

---

## Phase 1: Storage Layer

### Task 2: SQLite database setup

**Files:**
- Create: `src-tauri/src/storage.rs`

- [ ] **Step 1: Create `storage.rs` with schema and init**

```rust
use anyhow::Result;
use rusqlite::Connection;
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

pub fn init_database(app_handle: &AppHandle) -> Result<Connection> {
    let db_path = get_db_path(app_handle)?;
    let conn = Connection::open(&db_path)?;

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

    #[test]
    fn test_schema_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
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

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"servers".to_string()));
        assert!(tables.contains(&"documents".to_string()));
        assert!(tables.contains(&"api_cache".to_string()));
        assert!(tables.contains(&"offline_queue".to_string()));
        assert!(tables.contains(&"attachments".to_string()));
    }

    #[test]
    fn test_server_crud() {
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
        ).unwrap();

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

        // Delete
        conn.execute("DELETE FROM servers WHERE id = 'test-id'", []).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM servers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test`
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/storage.rs
git commit -m "feat: add SQLite storage layer with schema and tests"
```

---

## Phase 1: Server Manager

### Task 3: Server configuration CRUD

**Files:**
- Create: `src-tauri/src/server_manager.rs`

- [ ] **Step 1: Create `server_manager.rs`**

```rust
use crate::lib::ServerConfig;
use anyhow::Result;
use rusqlite::Connection;
use std::sync::Mutex;
use tauri::State;

pub struct DbConnection(pub Mutex<Connection>);

pub fn add_server(conn: &Connection, config: &ServerConfig) -> Result<()> {
    conn.execute(
        "INSERT INTO servers (id, name, url, offline_enabled) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![config.id, config.name, config.url, config.offline_enabled],
    )?;
    Ok(())
}

pub fn remove_server(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM servers WHERE id = ?1", [id])?;
    Ok(())
}

pub fn list_servers(conn: &Connection) -> Result<Vec<ServerConfig>> {
    let mut stmt = conn.prepare("SELECT id, name, url, offline_enabled, last_sync, created_at FROM servers ORDER BY created_at")?;
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
        .collect::<Result<Vec<_>, _>>()?;
    Ok(servers)
}

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

// Tauri commands
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
    use crate::lib::ServerConfig;

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

    #[test]
    fn test_add_and_list_servers() {
        let conn = setup_db();
        let config = ServerConfig {
            id: "test-1".into(),
            name: "Test Server".into(),
            url: "https://wiki.test.com".into(),
            offline_enabled: true,
            last_sync: None,
            created_at: String::new(),
        };

        add_server(&conn, &config).unwrap();
        let servers = list_servers(&conn).unwrap();

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "Test Server");
        assert_eq!(servers[0].url, "https://wiki.test.com");
    }

    #[test]
    fn test_remove_server() {
        let conn = setup_db();
        let config = ServerConfig {
            id: "test-1".into(),
            name: "Test".into(),
            url: "https://test.com".into(),
            offline_enabled: true,
            last_sync: None,
            created_at: String::new(),
        };

        add_server(&conn, &config).unwrap();
        assert_eq!(list_servers(&conn).unwrap().len(), 1);

        remove_server(&conn, "test-1").unwrap();
        assert_eq!(list_servers(&conn).unwrap().len(), 0);
    }

    #[test]
    fn test_get_server() {
        let conn = setup_db();
        let config = ServerConfig {
            id: "test-1".into(),
            name: "Test".into(),
            url: "https://test.com".into(),
            offline_enabled: false,
            last_sync: None,
            created_at: String::new(),
        };

        add_server(&conn, &config).unwrap();

        let found = get_server(&conn, "test-1").unwrap().unwrap();
        assert_eq!(found.name, "Test");

        let not_found = get_server(&conn, "nonexistent").unwrap();
        assert!(not_found.is_none());
    }
}
```

- [ ] **Step 2: Wire into `main.rs`**

Update `main.rs` to use library modules and register state + commands:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use tauri::Manager;

// All modules are declared in lib.rs; main.rs uses them as a library.
use outline_desktop::storage;
use outline_desktop::server_manager::{self, DbConnection};
use outline_desktop::api_proxy;
use outline_desktop::system;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let app_handle = app.handle().clone();
            let conn = storage::init_database(&app_handle)?;
            app.manage(DbConnection(Mutex::new(conn)));
            let http_client = api_proxy::HttpClient::new()
                .expect("Failed to create HTTP client");
            app.manage(http_client);
            system::setup_tray(&app_handle)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            server_manager::get_server_list,
            server_manager::add_server_command,
            server_manager::remove_server_command,
            api_proxy::proxy_fetch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Note: `main.rs` no longer has its own `mod` declarations — modules live in `lib.rs` and are imported via `outline_desktop::`. Remove the `mod lib;` and `mod storage;` etc. lines from the Task 1 `main.rs`.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass (2 from storage + 3 from server_manager).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/server_manager.rs src-tauri/src/main.rs
git commit -m "feat: add server manager with CRUD operations and Tauri commands"
```

---

## Phase 1: System Tray

### Task 4: System tray with server list

**Files:**
- Create: `src-tauri/src/system.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Create `system.rs`**

```rust
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

pub fn setup_tray(app_handle: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItem::with_id(app_handle, "show", "Show Outline Desktop", true, None::<&str>)?;
    let add_server_item = MenuItem::with_id(app_handle, "add_server", "Add Server...", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app_handle)?;
    let quit_item = MenuItem::with_id(app_handle, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app_handle, &[
        &show_item,
        &add_server_item,
        &separator,
        &quit_item,
    ])?;

    let _tray = TrayIconBuilder::new()
        .icon(app_handle.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("Outline Desktop")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "add_server" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.emit("open-add-server", ());
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app_handle)?;

    Ok(())
}
```

- [ ] **Step 2: Run and verify tray appears**

Run: `cd outline-desktop && cargo tauri dev`
Expected: App launches with a system tray icon. Clicking shows menu with "Show", "Add Server", "Quit".

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/system.rs
git commit -m "feat: add system tray with server management menu"
```

---

## Phase 1: Fetch & WebSocket Proxy

### Task 5: API proxy (fetch interception)

**Files:**
- Create: `src-tauri/src/api_proxy.rs`

- [ ] **Step 1: Create `api_proxy.rs`**

```rust
use crate::lib::{ProxyRequest, ProxyResponse};
use crate::server_manager::DbConnection;
use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::State;

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

/// Proxy a fetch request to the remote Outline server.
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
        rusqlite::params![key, server_id, method, body],
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

// Tauri command
#[tauri::command]
pub async fn proxy_fetch(
    db: State<'_, DbConnection>,
    http: State<'_, HttpClient>,
    method: String,
    path: String,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
) -> Result<ProxyResponse, String> {
    // TODO: Resolve server URL from current webview context
    let server_url = "https://placeholder.example.com";

    let response = proxy_request(
        &http.client,
        &server_url,
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
        let _ = cache_response(
            &conn,
            "default",
            &format!("{}:{}", method, path),
            &method,
            &response.body,
        );
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
}
```

- [ ] **Step 2: Register `HttpClient` in `main.rs`**

Add to the setup block:

```rust
let http_client = api_proxy::HttpClient::new()
    .expect("Failed to create HTTP client");
app.manage(http_client);
```

Add `api_proxy::proxy_fetch` to `invoke_handler`.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test`
Expected: 3 api_proxy tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/api_proxy.rs src-tauri/src/main.rs
git commit -m "feat: add API proxy with fetch interception and response caching"
```

---

## Phase 2: Offline Queue

### Task 6: Offline write operation queue

**Files:**
- Create: `src-tauri/src/offline_queue.rs`

- [ ] **Step 1: Create `offline_queue.rs`**

```rust
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
        .collect::<Result<Vec<_>, _>>()?;
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
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test`
Expected: 3 offline_queue tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/offline_queue.rs
git commit -m "feat: add offline write operation queue with FIFO replay"
```

---

## Phase 2: WebSocket Proxy

### Task 7: WebSocket proxy for Hocuspocus

**Files:**
- Create: `src-tauri/src/ws_proxy.rs`

- [ ] **Step 1: Create `ws_proxy.rs`**

This is the most complex module. It handles:
- Online: Forwarding WebSocket messages between WebView and remote Hocuspocus server
- Offline: Simulating a Hocuspocus server from local SQLite Y.js state

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use futures_util::{SinkExt, StreamExt};

/// WebSocket proxy mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProxyMode {
    /// Forward messages to remote server
    Online,
    /// Serve from local SQLite state
    Offline,
}

/// Hocuspocus-compatible document identifier
#[derive(Debug, Clone)]
pub struct DocumentRef {
    pub document_id: String,
    pub name: String, // "document.<uuid>"
}

impl DocumentRef {
    pub fn from_name(name: &str) -> Option<Self> {
        let id = name.strip_prefix("document.")?;
        Some(Self {
            document_id: id.to_string(),
            name: name.to_string(),
        })
    }
}

/// Start a WebSocket proxy for a Hocuspocus connection.
///
/// In Online mode:
///   - Connect to the remote Hocuspocus server
///   - Forward messages bidirectionally
///   - Persist Y.js updates to SQLite
///
/// In Offline mode:
///   - Load Y.js state from SQLite
///   - Send initial sync response (Sync Step 2)
///   - Accept updates from the client and persist locally
pub async fn start_proxy(
    mode: ProxyMode,
    document_ref: &DocumentRef,
    remote_url: &str,
    token: &str,
    local_port: u16,
) -> Result<()> {
    match mode {
        ProxyMode::Online => {
            start_online_proxy(document_ref, remote_url, token, local_port).await
        }
        ProxyMode::Offline => {
            start_offline_proxy(document_ref, local_port).await
        }
    }
}

/// Online proxy: forward WebSocket messages bidirectionally.
async fn start_online_proxy(
    document_ref: &DocumentRef,
    remote_url: &str,
    token: &str,
    _local_port: u16,
) -> Result<()> {
    let ws_url = format!(
        "{}/collaboration/{}?token={}&editorVersion=1.0.0",
        remote_url.trim_end_matches('/'),
        document_ref.name,
        token,
    );

    let (ws_stream, _) = connect_async(&ws_url).await?;
    tracing::info!("Connected to remote Hocuspocus: {}", ws_url);

    let (mut write, mut read) = ws_stream.split();

    // Forward remote messages to local client
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    // TODO: Intercept and persist Y.js updates to SQLite
                    tracing::debug!("Remote -> Local: {} bytes", data.len());
                }
                Ok(Message::Close(_)) => {
                    tracing::info!("Remote WebSocket closed");
                    break;
                }
                Err(e) => {
                    tracing::error!("Remote WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // In a real implementation, we'd also accept local messages
    // and forward them to the remote server. This requires an
    // incoming local WebSocket connection (from the WebView proxy).
    // The full implementation will use a local WebSocket server
    // that the WebView connects to.

    Ok(())
}

/// Offline proxy: serve Y.js state from local SQLite.
///
/// Implements the minimum Hocuspocus protocol:
/// 1. On connect: send Sync Step 2 with local Y.js state
/// 2. Accept Y.js updates from client, persist to SQLite
async fn start_offline_proxy(
    document_ref: &DocumentRef,
    _local_port: u16,
) -> Result<()> {
    tracing::info!(
        "Starting offline proxy for document: {}",
        document_ref.document_id
    );

    // TODO: Load Y.js state from SQLite
    // TODO: Send Sync Step 2 message with state
    // TODO: Accept and persist incoming updates

    Ok(())
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
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test`
Expected: 2 ws_proxy tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ws_proxy.rs
git commit -m "feat: add WebSocket proxy skeleton for Hocuspocus (online/offline modes)"
```

---

## Phase 3: Y.js Sync Engine

### Task 8: Y.js CRDT sync engine

**Files:**
- Create: `src-tauri/src/sync_engine.rs`

- [ ] **Step 1: Create `sync_engine.rs`**

```rust
use anyhow::Result;
use rusqlite::Connection;

/// Save a Y.js document state to SQLite.
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

/// Merge two Y.js document states using CRDT.
///
/// Y.js documents can be merged by applying one document's update
/// to another. The CRDT algorithm ensures convergence regardless
/// of the order of application.
///
/// This function:
/// 1. Loads the local state from SQLite
/// 2. If no local state exists, saves the remote state directly
/// 3. If local state exists, applies the remote update to merge
///
/// The actual Y.js binary operations (encode/decode/merge) will be
/// implemented using the yrs crate or by passing binary data through
/// the Hocuspocus protocol.
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
            // Merge using Y.js CRDT
            // TODO: Implement using yrs crate
            // let doc = yrs::Doc::new();
            // let mut txn = doc.transact_mut();
            // txn.apply_update(yrs::Update::decode_v1(&local)?);
            // txn.apply_update(yrs::Update::decode_v1(remote_state)?);
            // let merged = yrs::encode_state_as_update(&doc, &yrs::StateVector::default());
            // save_document_state(conn, server_id, document_id, &merged, None)?;
            // Ok(merged)

            // Placeholder: use remote state until yrs is integrated
            save_document_state(conn, server_id, document_id, remote_state, None)?;
            Ok(remote_state.to_vec())
        }
    }
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
    fn test_merge_saves_remote_when_no_local() {
        let conn = setup_db();
        let remote = vec![0xAA, 0xBB];

        let merged = merge_document_state_states(&conn, "s1", "doc1", &remote).unwrap();
        assert_eq!(merged, remote);
    }

    #[test]
    fn test_save_updates_existing() {
        let conn = setup_db();

        save_document_state(&conn, "s1", "doc1", &[1, 2], Some("Title")).unwrap();
        save_document_state(&conn, "s1", "doc1", &[3, 4, 5], None).unwrap();

        let loaded = load_document_state(&conn, "s1", "doc1").unwrap().unwrap();
        assert_eq!(loaded, vec![3, 4, 5]);
    }
}
```

- [ ] **Step 2: Fix the typo in test function name and run tests**

Correct `test_merge_saves_remote_when_no_local` test — the function name `merge_document_state_states` should be `merge_document_states`.

Run: `cd src-tauri && cargo test`
Expected: 4 sync_engine tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/sync_engine.rs
git commit -m "feat: add Y.js CRDT sync engine with local state persistence"
```

---

## Phase 4: Multi-Server Support

### Task 9: Per-server WebView windows

**Files:**
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/server_manager.rs`

- [ ] **Step 1: Add `open_server_window` command**

Add to `server_manager.rs`:

```rust
use tauri::{WebviewUrl, WebviewWindowBuilder, Manager};

#[tauri::command]
pub fn open_server_window(
    app: tauri::AppHandle,
    db: State<'_, DbConnection>,
    server_id: String,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let server = get_server(&conn, &server_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Server not found: {}", server_id))?;

    let label = format!("server-{}", server_id);
    let url = WebviewUrl::External(server.url.parse().map_err(|e: url::ParseError| e.to_string())?);

    // Check if window already exists
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    // Read injection scripts
    let bridge_js = include_str!("../inject/bridge.js");
    let proxy_js = include_str!("../inject/proxy.js");

    let _window = WebviewWindowBuilder::new(&app, &label, url)
        .title(&server.name)
        .inner_size(1200.0, 800.0)
        .initialization_script(bridge_js)
        .initialization_script(proxy_js)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}
```

- [ ] **Step 2: Register the new command in `main.rs`**

Add `server_manager::open_server_window` to `invoke_handler`.

- [ ] **Step 3: Build and verify**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/server_manager.rs src-tauri/src/main.rs
git commit -m "feat: add per-server WebView windows with injection scripts"
```

---

## Phase 4: Packaging

### Task 10: Build configuration and packaging

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Update `tauri.conf.json` with bundle config**

```json
{
  "bundle": {
    "active": true,
    "targets": ["nsis", "dmg", "appimage"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "copyright": "© 2026 Outline Contributors",
    "category": "Productivity",
    "shortDescription": "Desktop client for Outline wiki",
    "longDescription": "A cross-platform desktop client for self-hosted Outline instances with offline editing support via Y.js CRDT synchronization.",
    "nsis": {
      "installerIcon": "icons/icon.ico"
    },
    "dmg": {
      "icon": "icons/icon.icns"
    }
  }
}
```

- [ ] **Step 2: Create placeholder icon**

Create a simple placeholder icon at `src-tauri/icons/32x32.png` (can be a solid color square for now).

- [ ] **Step 3: Build**

Run: `cd outline-desktop && cargo tauri build`
Expected: Produces platform-specific installer in `src-tauri/target/release/bundle/`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tauri.conf.json src-tauri/icons/
git commit -m "feat: configure bundle packaging for Windows/macOS/Linux"
```

---

## Summary

| Phase | Task | Description | Status |
|---|---|---|---|
| 1 | Task 1 | Tauri project scaffold | -- |
| 1 | Task 2 | SQLite storage layer | -- |
| 1 | Task 3 | Server manager CRUD | -- |
| 1 | Task 4 | System tray | -- |
| 1 | Task 5 | API proxy (fetch interception) | -- |
| 2 | Task 6 | Offline write queue | -- |
| 2 | Task 7 | WebSocket proxy skeleton | -- |
| 3 | Task 8 | Y.js CRDT sync engine | -- |
| 4 | Task 9 | Per-server WebView windows | -- |
| 4 | Task 10 | Build & packaging | -- |
