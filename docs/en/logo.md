# OCG Manager Logo

<p align="center">
  <img src="../../assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="160">
</p>

## Concept

The OCG Manager logo is based on the [OpenCode](https://opencode.ai/) icon, extended to express the product's core purpose: **managing multiple OpenCode-Go accounts**.

The design keeps the original rounded-square shape and the inner black/gray split symbol, then stacks three instances with slight offsets. This layered composition visually communicates that the app aggregates, switches, and routes across several keys.

## Source Assets

| File | Description |
|------|-------------|
| `opencode-light.png` | The original OpenCode light icon (dark frame, light inner symbol) |
| `opencode-dark.png` | The original OpenCode dark icon (light frame, dark inner symbol) |
| `ocg_logo_final_transparent.png` | Final layered logo with transparent background |

The original OpenCode icons are from the [homarr-labs/dashboard-icons](https://github.com/homarr-labs/dashboard-icons) CDN.

## Generation

The final logo was produced in two steps:

1. **Layering**: Three `opencode-dark.png` icons were overlapped at decreasing opacity and increasing offset using a Python/Pillow script (`generate_three_white.py`).
2. **Refinement**: The stacked concept was mirrored and passed to `gpt-image-2` to produce a polished, production-ready icon with subtle depth and rounded edges.
3. **Export**: The transparent PNG was squared, scaled, and exported to `src-tauri/icons/` in sizes 32×32, 128×128, 256×256, 512×512, plus a Windows `.ico` (`generate_tauri_icons.py`).

## Usage

- Use `ocg_logo_final_transparent.png` for marketing, documentation, and the app window.
- Use the files in `src-tauri/icons/` for the Windows executable, installer, and system tray.
- Do not distort the logo; always keep the aspect ratio.
