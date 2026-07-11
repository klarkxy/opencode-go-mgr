---
name: OCG Co-pilot Console
colors:
  canvas: "#F6F6FA"
  surface: "#FFFFFF"
  ink: "#181820"
  muted: "#5E5D6A"
  primary: "#6257C8"
  primary-soft: "#ECE9FF"
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

# OCG Co-pilot Console

## Overview

OCG Manager is a local multi-account operations console. Its signature is a first-screen connection center paired with the OpenCode mascot. The interface is compact, technical, calm, and unmistakably operational rather than promotional.

## Colors

Use `{colors.primary}` for active navigation, focus, and primary actions. Use `{colors.success}` only for successful or available states. Dark mode maps canvas, surface, ink, muted, and primary to `#111116`, `#1A1A22`, `#F4F2FA`, `#C7C3D0`, and `#A99CFF` in code.

## Typography

Headings use `{typography.display}`. Interface copy uses `{typography.body}`. API addresses, keys, costs, and other machine-readable values use `{typography.data}` with tabular numerals.

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
- Don't use green as a brand primary color.
- Don't repeat a card title when structure and icons already provide context.
- Don't hide primary connection actions behind menus or secondary navigation.
- Don't use icon-only controls for ambiguous commit or irreversible actions.

## Responsive

At widths below 1024px, replace the sidebar with the horizontal application menu. On narrow phones, connection rows remain full width and the mascot becomes a low-opacity background element that cannot cover controls.

## Iteration Guide

Before adding visible copy, ask whether an icon, value, structure, or Tooltip already communicates it. Before adding a component or dependency, reuse Naive UI and the existing native platform capability.
