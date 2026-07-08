# 快速开始

构建和本地运行 OCG Manager 的速查表。完整指南（安装、Gateway 使用、配置、架构）见
[根目录 README](../../README.zh-CN.md)。本页仅列出开发命令。

## 环境要求

- **Node.js** v20+（v24 实测可用）
- **Rust** ≥ 1.85（edition 2024，MSRV 在 `src-tauri/Cargo.toml` 中强制）
- **Tauri CLI**——已作为开发依赖包含，`npx tauri ...` 无需全局安装
- Windows 10/11，已安装 WebView2

## 常用命令

```bash
# 安装前端依赖
npm install

# 仅前端开发服务器  →  http://127.0.0.1:30001
npm run dev

# 完整 Tauri 应用（前端 + Rust + 桌面窗口）
npx tauri dev

# 类型检查 + 前端生产构建
npm run build

# Rust 检查 / Release 二进制
cd src-tauri && cargo check
cd src-tauri && cargo build --release

# Windows NSIS 安装包  →  src-tauri/target/release/bundle/nsis/
npx tauri build
```

内嵌 Gateway 随应用自动启动，监听配置端口（默认 `9042`）。

## 下一步

- [README](../../README.zh-CN.md)——功能特性、Gateway 使用、配置、架构、维护者说明。
- [logo.md](logo.md)——Logo 设计理念、源素材与使用规范。
