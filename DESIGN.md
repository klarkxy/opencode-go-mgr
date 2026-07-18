---
name: OCG Manager Console
colors:
  canvas: "#F7F7F8"
  surface: "#FFFFFF"
  ink: "#18181B"
  muted: "#5F6068"
  primary: "#18181B"
  primary-soft: "#ECECEF"
  success: "#16845B"
  warning: "#A85F00"
  error: "#C33B55"
  info: "#2F6FD4"
typography:
  display: "Bahnschrift, Segoe UI Variable Display, sans-serif"
  body: "Segoe UI Variable Text, Noto Sans SC, Microsoft YaHei UI, sans-serif"
  data: "Cascadia Mono, Consolas, monospace"
spacing:
  xs: 4
  sm: 8
  md: 12
  lg: 16
  xl: 24
  xxl: 32
rounded:
  small: 6
  medium: 10
  large: 14
---

# OCG Manager Console

## Overview

OCG Manager is a local multi-account operations console. Its signature is a first-screen connection center paired with the OpenCode mascot. The interface is compact, technical, calm, and unmistakably operational rather than promotional.

## Colors

The selector contains seven two-character themes: 默认, 皓白, 曜黑, 藤紫, 霁蓝, 青瓷, and 暖铜. 默认 follows the operating system and resolves to 皓白 or 曜黑; every other theme is fixed. 皓白 stays neutral and 曜黑 uses a pure-black canvas. The four colored themes tint the full environment—canvas, surfaces, raised controls, borders, and interaction states—so they must never collapse into white cards with isolated accent colors. Use each theme's primary color for active navigation, focus, primary actions, and the first chart series. Use success only for successful or available states; semantic status colors never change meaning between themes.

## Typography

Headings use `{typography.display}`. Interface copy uses `{typography.body}`. API addresses, keys, costs, and other machine-readable values use `{typography.data}` with tabular numerals. The type scale has six steps: 12px for captions and field labels, 13px for secondary text, 14px as the body base, 16px for card titles, 20px for KPI figures and page titles, and 24px reserved for the connection hero. Hierarchy comes from this scale combined with weight and color; never introduce ad-hoc sizes outside the six steps.

## Layout

Use the spacing scale from `{spacing.xs}` through `{spacing.xxl}`. The Dashboard order is connection center, KPIs, full-width chart, then account overview. Core connection information must stay above the fold and must never be moved into a secondary rail.

## Shapes

Controls use `{rounded.small}` or `{rounded.medium}`. Content panels use `{rounded.large}`. Avoid excessive pills and ornamental cards.

## Components

Utility actions are circular quaternary icon buttons with a Tooltip and an explicit accessible name. Primary commit actions and destructive confirmations retain visible text. Connection rows combine one semantic icon, one monospace value, and only the actions needed for that value.

## Do's & Don'ts

- Do call the access credential “Key”; never display “Gateway Key”.
- Do keep API, Key, and upstream copy actions adjacent to their values.
- Do use icons to reduce repeated labels, while retaining screen-reader labels.
- Do preserve visible keyboard focus and reduced-motion preferences.
- Do keep theme names to two Chinese characters and expose all seven choices in one selector.
- Do give the mascot a subtle light rim only in 曜黑; other themes use the normal shadow.
- Don't reuse the success green as a brand primary color.
- Don't repeat a card title when structure and icons already provide context.
- Don't hide primary connection actions behind menus or secondary navigation.
- Don't use icon-only controls for ambiguous commit or irreversible actions.

## Responsive

At widths below 1024px, replace the sidebar with the horizontal application menu. On narrow phones, connection rows remain full width and the mascot becomes a low-opacity background element that cannot cover controls.

## Iteration Guide

Before adding visible copy, ask whether an icon, value, structure, or Tooltip already communicates it. Before adding a component or dependency, reuse Naive UI and the existing native platform capability.
