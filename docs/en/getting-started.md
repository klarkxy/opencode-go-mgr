# Getting Started

A quick reference for building and running OCG Manager locally. The full guide
— installation, gateway usage, configuration, and architecture — lives in the
[root README](../../README.md). This page only covers the dev commands.

## Prerequisites

- **Node.js** v20+ (v24 confirmed)
- **Rust** ≥ 1.85 (edition 2024, MSRV enforced in `src-tauri/Cargo.toml`)
- **Tauri CLI** — already a dev dependency, so `npx tauri ...` works without a global install
- Windows 10/11 with WebView2

## Commands

```bash
# Install frontend deps
npm install

# Frontend-only dev server  →  http://127.0.0.1:30001
npm run dev

# Full Tauri app (frontend + Rust + desktop window)
npx tauri dev

# Type-check + production frontend build
npm run build

# Rust checks / release binary
cd src-tauri && cargo check
cd src-tauri && cargo build --release

# Windows NSIS installer  →  src-tauri/target/release/bundle/nsis/
npx tauri build
```

The embedded gateway starts automatically with the app on its configured port
(default `9042`).

## Next Steps

- [README](../../README.md) — features, gateway usage, configuration, architecture, maintainer notes.
- [logo.md](logo.md) — logo concept, source assets, and usage rules.
