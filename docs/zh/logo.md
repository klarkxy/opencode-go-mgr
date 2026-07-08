# OCG Manager Logo

<p align="center">
  <img src="../../assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="160">
</p>

## 设计理念

OCG Manager 的 Logo 基于 [OpenCode](https://opencode.ai/) 图标进行扩展，以表达产品的核心定位：**管理多个 OpenCode-Go 账号**。

设计保留了原版的圆角方形轮廓和内部黑灰分割符号，并将三个相同图形以轻微错位层叠。这种层叠构图直观地传达了应用对多账号的聚合、切换与路由能力。

## 素材来源

| 文件 | 说明 |
|------|------|
| `opencode-light.png` | OpenCode 原版浅色图标（深色外框、浅色内部符号） |
| `opencode-dark.png` | OpenCode 原版深色图标（浅色外框、深色内部符号） |
| `ocg_logo_final_transparent.png` | 最终层叠 Logo，透明背景 |

原始 OpenCode 图标来自 [homarr-labs/dashboard-icons](https://github.com/homarr-labs/dashboard-icons) CDN。

## 生成过程

最终 Logo 分三步生成：

1. **层叠**：使用 Python/Pillow 脚本（`generate_three_white.py`），将三个 `opencode-dark.png` 图标以不同透明度和错位量叠加。
2. **精修**：将层叠概念图水平镜像后，交给 `gpt-image-2` 生成具有立体圆角、更适合产品使用的精致图标。
3. **导出**：将透明 PNG 居中放到正方形画布，缩放后导出到 `src-tauri/icons/`，尺寸包括 32×32、128×128、256×256、512×512，以及 Windows `.ico`（`generate_tauri_icons.py`）。

## 使用规范

- 市场、文档、应用窗口使用 `ocg_logo_final_transparent.png`。
- Windows 可执行文件、安装包、系统托盘使用 `src-tauri/icons/` 下的文件。
- 请勿拉伸或变形 Logo，始终保持原始比例。
