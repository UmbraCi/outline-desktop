# Outline Desktop

Cross-platform desktop client for [Outline](https://github.com/outline/outline) wiki.

Built with [Tauri 2](https://v2.tauri.app/) — lightweight, fast, secure.

## Features

- Connect to any self-hosted Outline instance
- Multi-server support (switch between servers)
- Offline editing with Y.js CRDT sync
- Native system integration (tray, notifications, keyboard shortcuts)

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 18+
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### Setup

```bash
npm install
```

### Dev

```bash
npm run tauri:dev
```

### Build

```bash
npm run tauri:build
```

## Architecture

See [docs/specs/](docs/specs/) for the design document and [docs/plans/](docs/plans/) for the implementation plan.
