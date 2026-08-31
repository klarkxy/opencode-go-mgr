---
name: OCG Manager Console
colors:
  canvas: "#F7F7F8"
  surface: "#FFFFFF"
  ink: "#18181B"
  muted: "#5F6068"
  primary: "#18181B"
  primary-soft: "#ECECEF"
  success: "#0B6844"
  warning: "#8A4D00"
  error: "#A92742"
  info: "#245DB6"
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

The selector contains seven two-character themes: 默认, 皓白, 曜黑, 藤紫, 霁蓝, 青瓷, and 暖铜. 默认 follows the operating system and resolves to 皓白 or 曜黑; every other theme is fixed. 皓白 stays neutral and 曜黑 uses a pure-black canvas.

Frontmatter colors above are the **皓白** baseline (also the CSS custom-property defaults before JS applies a theme). Runtime tokens live in `src/theme.ts` (`THEME_TOKENS`); light themes share the same success / warning / error / info values for WCAG AA contrast. 曜黑 substitutes its own dark-mode semantic set. Chart series beyond the theme primary use `CHART_PALETTE` in `src/theme.ts` and may keep brighter fixed hues than the status colors.

The four colored themes tint the full environment—canvas, surfaces, raised controls, borders, and interaction states—so they must never collapse into white cards with isolated accent colors.

Use each theme's primary color for active navigation, focus, primary actions, and the first chart series. Use success only for successful or available states; semantic status colors never change meaning between themes.

## Typography

Headings use `{typography.display}`. Interface copy uses `{typography.body}`. API addresses, keys, costs, and other machine-readable values use `{typography.data}` with tabular numerals.

The type scale has six steps, exposed as `--ocg-font-xs` … `--ocg-font-2xl`: 12px for captions and field labels, 13px for secondary text, 14px as the body base, 16px for card titles, 20px for KPI figures and page titles, and 24px reserved for the connection hero. Hierarchy comes from this scale combined with weight and color; never introduce ad-hoc sizes outside the six steps.

## Layout

Use the spacing scale from `{spacing.xs}` through `{spacing.xxl}`. The side rail (horizontal app menu below 1024px) exposes seven fixed core views in this order: Dashboard, Access Keys, Accounts, Providers, Applications, Logs, Settings. A divider below Settings starts the optional **External Integrations** group. Its entries are static, locally supported product surfaces, not Providers, Plans, or dynamic plugins; CPA is the first entry.

The Dashboard order is connection center, KPIs, needs-attention list, then the full-width daily Token chart. Core connection information must stay above the fold and must never be moved into a secondary rail. The connection center is the consume surface: the current Key, copy, and rotate-current stay there, plus a manage action that opens Access Keys. Create, rename, enable, delete, and reset live only on Access Keys. The primary key has no custom-value field; rotation uses the same reset control as sub keys.

Providers is the supplier control plane. A left rail lists built-in `Provider` families. The main pane has three tabs: Model catalog, Pricing, and read-only Alias. The Alias tab aggregates existing Provider contracts and account capabilities into client-facing names plus their current routeability; it is an inspection surface, never an Alias editor, API, store, or cache. Custom mappings are edited only on Accounts: each row pairs the public name clients request with the exact upstream model ID, while the account keeps one protocol for all mappings. A Custom mapping's edit action uses the dashboard deep link `?view=accounts&account_id=<id>` to open that account's editor; it does not create a Providers-side editor.

Every provider scope uses one Model catalog composition: a compact source line and, when the scope has an official catalog, the same **Refresh model catalog** action sit in the panel header; the protocol matrix follows directly without a separate catalog-summary card or account picker. OpenCode Go and Zen Free refresh from their official sources, with the backend selecting any required eligible credential. A persisted official snapshot is authoritative; the static catalog is only the initial preset before the first successful refresh. Models newly added by a refresh appear with every protocol disabled until the user explicitly enables a cell or a successful Test enables it. Existing model overrides and probe state survive refreshes. The matrix has one row per model and one column per upstream protocol (Chat Completions, Responses, Messages). Each cell is a binary switch bound to the effective enabled state, flipping to force on or force off; an enabled switch uses the success color while a disabled one stays neutral, never error red. Column batch actions stay in the column headers as turn-all-on / turn-all-off. Each row ends with a compact Test icon action that probes that model's protocols through backend-managed eligible-account fallback; no account picker is shown. Command Code GOAT is live and refreshes from its official public catalog; GOAT preset rows default on, additional discovered rows default off, and the scope has no generic Provider Probe. Prefer monospace for revision IDs, model IDs, USD rates, and multipliers. Catalog fetch, protocol probes, and OpenCode Go pricing refresh are explicit primary actions, never automatic on page load. The Provider Test action must warn that real minimal requests may be sent through multiple eligible accounts and may consume quota. Accounts keep identity, Key, enablement, usage, and every Custom mapping. Every ready account card has the same compact Test connection utility action. It opens a searchable model table and locks every single or sequential batch test to that exact account with no fallback; test results remain local to the open dialog. The Custom account form exposes one account-wide protocol selector and a compact mapping table of public name to upstream ID. Discovery lists upstream IDs only; importing one creates an exact public-name = upstream-ID row and never strips or synthesizes a suffix. Add Account is a grouped plan list with a detail pane for copy and actions, not a two-column card grid. Backend-owned singletons such as Zen Free are omitted from that list and enabled from the account list.

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
- Don't fetch OpenCode Go pricing, provider catalogs, protocol probes, or GitHub releases without an explicit user action.

## Responsive

At widths below 1024px, replace the sidebar with the horizontal application menu while retaining the Settings divider and External Integrations group. On narrow phones, connection rows remain full width and the mascot becomes a low-opacity background element that cannot cover controls.

## Iteration Guide

Before adding visible copy, ask whether an icon, value, structure, or Tooltip already communicates it. Before adding a component or dependency, reuse Naive UI and the existing native platform capability. When changing colors or type scale, update `src/theme.ts` and this file together, then run `pnpm run design:lint` and `src/theme.test.ts`.
