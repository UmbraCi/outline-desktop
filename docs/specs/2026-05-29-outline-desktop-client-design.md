# Outline Desktop Client — Design Document

## Overview

构建一个跨平台（Windows/macOS/Linux）的 Outline 桌面客户端，支持连接自部署的 Outline 服务器，提供原生体验和完整离线编辑能力。

客户端作为**独立项目**维护（`outline-desktop`），通过 Outline 现有 API + WebSocket + DesktopBridge 接口交互，不侵入 Outline 主仓库代码。

## 技术选型

| 决策 | 选择 | 理由 |
|---|---|---|
| 框架 | Tauri 2 | 跨平台、包体积小、性能好、Rust 后端 |
| Web 加载 | 远程加载 + Tauri 代理 | 无需保持版本同步、实现简单 |
| 离线编辑 | 完整离线编辑（CRDT） | Y.js CRDT 无冲突丢失 |
| 多服务器 | 支持切换 | 类 Slack 体验 |
| 架构方式 | WebView 包壳 | 复用现有 Outline Web 应用 |

## 整体架构

```
┌─────────────────────────────────────────────────┐
│                  Tauri 应用                       │
│                                                   │
│  ┌─────────────────────────────────────────────┐ │
│  │  WebView (WebView2/WKWebView/WebKitGTK)    │ │
│  │                                             │ │
│  │  加载 https://wiki.yourcompany.com          │ │
│  │  + 注入 TauriBridge (window.TauriBridge)   │ │
│  │                                             │ │
│  │  Outline Web App 正常运行                    │ │
│  │  通过 TauriBridge 调用本地能力               │ │
│  └─────────────────────────────────────────────┘ │
│                     │ IPC (invoke/handle)          │
│  ┌─────────────────────────────────────────────┐ │
│  │  Tauri Rust 后端                             │ │
│  │                                             │ │
│  │  ├── Server Manager (多服务器连接管理)       │ │
│  │  ├── API Proxy (代理 fetch 请求到远程)      │ │
│  │  ├── WebSocket Proxy (Hocuspocus + Socket.IO)│
│  │  ├── Sync Engine (Y.js CRDT 同步)          │ │
│  │  ├── Local Storage (SQLite + 文件缓存)      │ │
│  │  └── System Integration (通知/托盘/快捷键)  │ │
│  └─────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

## 项目结构

```
D:\code\
├── outline/                  # Outline 主仓库，不修改
│   ├── app/
│   ├── server/
│   └── ...
│
└── outline-desktop/          # 新项目：Tauri 桌面客户端
    ├── src-tauri/            # Tauri Rust 后端
    │   ├── src/
    │   │   ├── main.rs              # 应用入口
    │   │   ├── server_manager.rs    # 多服务器连接管理
    │   │   ├── api_proxy.rs         # API 请求代理与缓存
    │   │   ├── ws_proxy.rs          # WebSocket 代理（Hocuspocus + Socket.IO）
    │   │   ├── sync_engine.rs       # Y.js CRDT 同步引擎
    │   │   ├── storage.rs           # SQLite 本地存储
    │   │   ├── offline_queue.rs     # 离线写操作队列
    │   │   └── system.rs            # 系统集成（托盘/通知/快捷键）
    │   └── Cargo.toml
    ├── src/                  # Tauri 前端（设置界面、服务器列表等）
    │   ├── components/
    │   └── pages/
    ├── package.json
    └── tauri.conf.json
```

## 核心模块设计

### 1. WebView 注入层

Outline 已有 `DesktopBridge` 接口定义（`app/utils/Desktop.ts`）。Tauri 通过 WebView initialization script 注入 `window.TauriBridge`，实现与 Outline 现有 `DesktopBridge` 兼容的 API。

Outline 现有 bridge 接口：

```typescript
interface DesktopBridge {
  redirect(url: string): void
  updateDownloaded(callback: () => void): void
  restartAndInstall(): void
  focus(callback: () => void): void
  blur(callback: () => void): void
  openKeyboardShortcuts(callback: () => void): void
  getAutoLaunch(): Promise<boolean>
  setAutoLaunch(enabled: boolean): void
}
```

Tauri 扩展（在 bridge 对象上额外实现）：

```typescript
interface TauriDesktopBridge extends DesktopBridge {
  // 多服务器管理
  getServerList(): Promise<ServerConfig[]>
  addServer(config: ServerConfig): Promise<void>
  removeServer(id: string): Promise<void>
  switchServer(id: string): Promise<void>

  // 离线与同步
  getOfflineStatus(): Promise<OfflineStatus>
  syncNow(): Promise<void>

  // API 代理（拦截 fetch）
  proxyFetch(path: string, options: RequestInit): Promise<Response>
}
```

注入方式：Tauri 的 `WebviewWindowBuilder::initialization_script()` 注入 JavaScript，在页面加载前执行，将 `window.TauriBridge` 和 fetch 拦截逻辑写入全局作用域。

初始化脚本还需阻止远程 Service Worker 注册，否则 SW 会在网络层拦截请求，绕过我们的 fetch 代理：

```javascript
// 阻止远程 SW 注册，离线缓存由 Tauri 后端控制
Object.defineProperty(navigator, 'serviceWorker', {
  value: undefined, writable: false
});
```

### 2. 服务器连接管理

```rust
struct ServerConfig {
    id: String,              // UUID
    name: String,            // 用户自定义名称
    url: String,             // https://wiki.yourcompany.com
    auth_token: String,      // JWT token（通过 OAuth 流程获取）
    offline_enabled: bool,   // 是否启用离线缓存
    last_sync: DateTime,     // 上次同步时间
}

enum ConnectionStatus {
    Online,
    Offline,
    Syncing,
    Error(String),
}
```

行为：
- 添加服务器时，用户输入服务器 URL（如 `https://wiki.yourcompany.com`）
- WebView 加载该 URL，Outline 的 OAuth 登录流程正常执行
- 登录成功后，Outline 设置 HTTP-only JWT Cookie（同源，自动携带）
- 用户后续访问无需重复登录（Cookie 有效期由服务器配置决定）
- Tauri 后端定期检查 Cookie 有效性，过期时提示重新登录
- 每个服务器对应一个独立的 WebviewWindow 或通过切换 WebView URL 实现
- 系统托盘展示服务器列表及连接状态

### 3. API 代理

WebView 中通过 initialization script 覆盖 `window.fetch` 和 `window.XMLHttpRequest`，将 `/api/*` 请求通过 Tauri IPC (`window.__TAURI__.core.invoke`) 转发到 Rust 后端。其他请求（如静态资源）保持原始 fetch 不变，直接由 WebView 网络栈处理。

```
在线模式:
WebView fetch("/api/documents.list")
  → TauriBridge.proxyFetch("/api/documents.list")
    → Rust: fetch remote https://server/api/documents.list
    → Rust: 缓存响应到 SQLite
    → 返回响应给 WebView

离线模式:
WebView fetch("/api/documents.list")
  → TauriBridge.proxyFetch("/api/documents.list")
    → Rust: 从 SQLite api_cache 表读取
    → 返回缓存响应给 WebView
```

写操作离线队列：
- 离线时的 POST/PUT/PATCH/DELETE 操作存入 `offline_queue` 表
- 联网后按 FIFO 顺序重放
- 重放失败的操作标记并提示用户手动处理

### 4. WebSocket 代理

通过 initialization script 覆盖 `window.WebSocket` 构造函数，拦截到 `/collaboration` 和 `/realtime` 路径的连接，将其转发到 Tauri Rust 后端代理。其他 WebSocket 连接保持原始行为。

```
在线时:
WebView (Y.js Provider) ←→ Tauri Rust WS Proxy ←→ Hocuspocus Server
                              ├── 转发消息
                              └── 持久化 Y.js updates 到 SQLite

离线时:
WebView (Y.js Provider) ←→ Tauri Rust WS Proxy ←→ SQLite (Y.js state)
                              └── 模拟 Hocuspocus 协议响应
```

Tauri 后端需要实现 Hocuspocus 服务端协议的子集：
- `onConnect` — 提供文档的初始 Y.js 状态
- `onChange` — 接收来自 WebView 的 Y.js update 并持久化
- 离线时，Tauri 后端表现为一个本地 Hocuspocus 服务器

Socket.IO 代理：
- 在线时转发实时事件
- 离线时静默忽略（实时事件不需要离线支持）

### 5. Y.js CRDT 同步引擎

```
┌──────────────┐        ┌──────────────┐
│ 本地 Y.js Doc │ ←合并→ │ 远程 Y.js Doc │
│  (SQLite)    │        │ (PostgreSQL)  │
└──────────────┘        └──────────────┘
      CRDT 自动合并，无冲突丢失
```

同步流程：
1. 联网时，获取远程文档的 Y.js binary state
2. 与本地 Y.js state 合并（`Y.applyUpdate(doc, remoteUpdate)`）
3. 合并后的 state 同时写回本地 SQLite 和远程服务器
4. Y.js CRDT 保证最终一致性，最后写入的不会覆盖之前的修改

同步频率：
- 实时：在线编辑时，每次按键后通过 Hocuspocus WebSocket 实时同步
- 定时：每 5 分钟全量同步一次（检查是否有遗漏的文档）
- 手动：用户可点击"立即同步"

### 6. 本地存储（SQLite）

```sql
CREATE TABLE servers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  url TEXT NOT NULL,
  auth_token_encrypted BLOB,
  offline_enabled INTEGER DEFAULT 1,
  last_sync TEXT,
  created_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE documents (
  id TEXT PRIMARY KEY,
  server_id TEXT NOT NULL,
  yjs_state BLOB,
  title TEXT,
  updated_at TEXT,
  last_modified TEXT,
  FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
);

CREATE TABLE api_cache (
  key TEXT NOT NULL,
  server_id TEXT NOT NULL,
  method TEXT NOT NULL DEFAULT 'GET',
  response BLOB,
  updated_at TEXT DEFAULT (datetime('now')),
  PRIMARY KEY (key, server_id),
  FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
);

CREATE TABLE offline_queue (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  server_id TEXT NOT NULL,
  method TEXT NOT NULL,
  path TEXT NOT NULL,
  body TEXT,
  headers TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
);

CREATE TABLE attachments (
  id TEXT PRIMARY KEY,
  server_id TEXT NOT NULL,
  document_id TEXT,
  local_path TEXT,
  remote_url TEXT,
  content_type TEXT,
  size INTEGER,
  synced INTEGER DEFAULT 0,
  FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
);
```

### 7. 系统集成

- **多窗口**：每个服务器连接为独立的 Tauri WebviewWindow
- **系统托盘**：显示服务器列表、连接状态、同步进度
- **全局快捷键**：快速唤起搜索窗口（类 Spotlight）
- **通知**：通过 Tauri notification 插件转发 Outline 通知事件
- **自动更新**：Tauri updater 插件，检查客户端更新
- **开机启动**：Tauri plugin-autostart

## 离线/在线切换流程

```
网络状态变化检测（Tauri 后端定期检查）:

从在线 → 离线:
  1. API 代理切换为读本地 SQLite 缓存
  2. WebSocket 代理切换为本地 Y.js Provider
  3. 写操作入离线队列
  4. 通知 WebView 更新 UI 状态（离线指示器）

从离线 → 在线:
  1. Y.js CRDT 合并（自动，无冲突丢失）
  2. 重放离线写操作队列
  3. API 代理切换为转发到远程服务器
  4. 通知 WebView 更新 UI 状态（同步完成）
  5. 后台全量缓存刷新
```

## 与 Outline 主仓库的协作

客户端作为独立项目，通过 Outline 现有公开接口工作，无需修改 Outline 代码。

Outline 提供的接口：

| 客户端需求 | Outline 接口 | 文件位置 |
|---|---|---|
| 桌面环境检测 | `Desktop.isElectron()` + `window.DesktopBridge` | `app/utils/Desktop.ts` |
| 桌面事件处理 | `DesktopEventHandler.tsx` | `app/components/DesktopEventHandler.tsx` |
| 认证流程 | Cookie-based JWT, `/auth/*` | `server/routes/auth/` |
| API 通信 | `/api/*` REST | `server/routes/api/` |
| 实时协作 | `/collaboration` Hocuspocus WebSocket | `server/services/collaboration.ts` |
| 实时事件 | `/realtime` Socket.IO | `server/services/websockets.ts` |

潜在的上游贡献（可选，不影响客户端基本功能）：
- 扩展 `DesktopBridge` 类型定义，增加 `isTauri` 检测
- 将 `Desktop.isElectron()` 泛化为 `Desktop.isDesktop()`
- 这些改动极小（1-2 个类型文件），可作为 PR 贡献回上游

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|---|---|---|
| Outline 版本升级导致 WebView 不兼容 | 中 | 远程加载方式天然兼容最新版本；客户端仅依赖稳定 API 接口 |
| Hocuspocus 协议变更 | 高 | Tauri WS 代理需要跟进协议变化；可 pin Hocuspocus 客户端版本 |
| Service Worker 缓存策略限制 | 低 | Tauri 后端可补充缓存，不完全依赖 SW |
| 离线队列重放冲突 | 中 | 使用 Y.js CRDT 合并文档内容；非文档操作（如权限变更）重放失败时提示用户 |
| 跨平台 WebView 兼容性 | 中 | WebView2 (Windows) / WKWebView (macOS) / WebKitGTK (Linux) 行为差异需逐平台测试 |

## 实施路线

### Phase 1: 最小可用版本
- Tauri 壳 + WebView 加载远程 Outline
- TauriBridge 注入 + fetch 代理
- 单服务器连接 + OAuth 认证
- 系统托盘 + 基本通知

### Phase 2: 离线支持
- SQLite 本地存储
- API 响应缓存
- 离线写操作队列
- 在线/离线状态检测与切换

### Phase 3: Y.js CRDT 同步
- Hocuspocus WebSocket 代理
- Y.js 文档本地持久化
- CRDT 同步引擎
- 冲突自动解决

### Phase 4: 多服务器 + 完善
- 多服务器切换
- 全局搜索快捷键
- 附件离线缓存
- 自动更新
- 安装包签名与分发
