[简体中文](applications.zh-CN.md)

# Application Guides And Model Capabilities

For a client that is not listed here, see [Add an Application](add-application.md) for the manual Gateway interface, guide contribution, and optional Desktop connector paths.

## Application Guides

This chapter has copy-ready configuration for 17 clients. Copy the **Key** and
URLs from **Connection Center**, then follow the matching guide. Each guide
lists the protocol, the official documentation URL, setup steps, and example
snippets. The registry lives in `src/views/application-guides.ts`.

Before overwriting any existing configuration file, back up the original.

### Local Desktop connection

Automatic Desktop connectors remain in `src/views/Applications.vue` and the
Desktop Host. They are not on the dashboard rail; the current user path is
manual configuration from Connection Center and this chapter. Claude Code,
Codex, Gemini CLI, OpenCode, OpenClaw, and Hermes use managed field-level
configuration; Pi uses a client-native plugin, while DeepSeek Harness (DSH)
uses a companion plugin plus one field-owned `.env` assignment. Claude Desktop
uses the manual guide only. CLI, Docker, public listeners, and remote nodes
also use the manual path.

All connectors support redacted preview and a reversible operation. Managed
configuration connectors use field-level writes and restore. Codex writes only the OCG-owned provider fields in user-level
`config.toml`; Hermes writes only four `model` fields in `config.yaml` and one
dedicated variable in `.env`. Pi and DSH install or remove only OCG's own package
through the client's package manager; they never store a Key in plugin source.
DSH additionally stores the selected Key only in the dedicated
`OCG_MANAGER_API_KEY` assignment in its home `.env`.

The phase-one installed-client acceptance gate covers five clients end to end:
Codex, Claude Code, OpenCode, Pi, and DSH. Gemini CLI, OpenClaw, and Hermes keep
their managed connector and manual guide.

The connector previews a fixed, field-level OCG patch, then rechecks both the
Dashboard revision and target file fingerprint before committing. A per-client ownership record keeps the
original values of only the fields OCG changed. Restore removes or restores
those fields while preserving unrelated edits; an OCG-owned field changed by
another program is reported as a conflict instead of being overwritten.
Writes use a cross-process lock, sibling temporary files, replacement and
read-back verification. A multi-file failure is compensated or reported as a
recoverable partial state. Preview text, errors, logs and ownership records do
not expose the selected Key.

Malformed, linked, oversized, non-UTF-8, duplicate-key, or unsupported
JSON5/YAML configurations fail closed and keep the manual guide available.
Hermes preserves bytes outside the top-level `model` section; a comment inside
that managed section is rejected instead of being silently discarded.
Close the target client before connecting, then reopen it after commit; OCG
Manager does not terminate applications with active sessions. Hermes manages
the default native profile only. An explicit `HERMES_HOME` wins; on Windows the
native default is `%LOCALAPPDATA%/hermes`, with `~/.hermes` recognized only as a
legacy location. Ambiguous roots are rejected instead of being written twice.

Each client has its own idea of where the API lives:

- Claude Code, Cherry Studio, and Chatbox use the root URL without `/v1`.
- Claude Desktop uses that root plus `/claude-desktop`; its client then calls
  `/claude-desktop/v1/messages` and `/claude-desktop/v1/models`.
- Gemini CLI uses the root URL with `GOOGLE_GENAI_API_VERSION=v1beta`. Its
  remote Base URL must use HTTPS; only `localhost`, `127.0.0.1`, and `[::1]`
  may use HTTP. A non-loopback HTTP root violates this client-side rule.
- Pi, Kimi Code CLI, OpenCode, OpenClaw, Hermes, Cline, Roo Code, and Continue
  use the API Base URL ending in `/v1`.
- VS Code Copilot Chat and WorkBuddy need the full `/v1/chat/completions` URL.
  Codex needs the API Base URL ending in `/v1` plus `wire_api = "responses"`.
  Use `~/.codex/ocg.config.toml` with `codex --profile ocg` (CLI) or merge the
  same provider block into user-level `~/.codex/config.toml` (Desktop / default
  provider). `~/.codex/ocg-model-catalog.json` is optional: skip it to connect.
  Enable `model_catalog_json` only for the picker plus real context windows and
  reasoning levels. A catalog replaces Codex's bundled model list and must
  include the current required fields. Without a catalog, unknown slugs use
  Codex's 272K fallback metadata. Requests always use OCG Manager's Responses
  endpoint.

Codex's connector respects `CODEX_HOME` and otherwise uses
`~/.codex/config.toml`. It owns only root `model`, root `model_provider`, and
the `model_providers.ocg_manager` table. It never reads or modifies
`~/.codex/auth.json`, `openai_base_url`, other provider tables, MCP settings,
permissions, or model catalogs. The selected Key is stored in OCG's provider
table as Codex's `experimental_bearer_token`, so the file must remain private to
the current OS user. Back up separately before mixing this direct connector
with another configuration switcher.

Hermes stores the selected Key only as `OCG_MANAGER_API_KEY` in its `.env` and
references it from `model.api_key`; unrelated YAML sections and environment
variables remain outside connector ownership. Named profiles and container
volumes are not inferred or modified.

Pi installs `ocg-manager-pi` through Pi's package manager and stores the selected
Key through Pi's native provider login. DSH installs `ocg-manager-dsh` into the
`web` profile as an OCG-owned companion plugin. The plugin owns only its fixed
route; OCG field-manages `OCG_MANAGER_API_KEY` in the DSH home `.env`, preserving
all unrelated lines and restoring the original assignment on uninstall. This
leaves base-profile and other bundle providers untouched;
the first phase does not auto-install into TUI, headless, or custom profiles.
Uninstall removes only the OCG-owned package.

The picker list comes from protected `GET /dashboard/api/v3/application-models`:
currently routeable OpenCode Go aliases intersected with the active OpenCode Go
pricing snapshot. Highspeed variants inherit the base model's pricing row. An
empty intersection is `[]`. Authenticated
`GET /v1/models` lists routeable code-owned Go and sealed Provider
Aliases plus eligible Custom declared IDs. Saved Zen `-free` rows publish the suffix-stripped Alias;
Command catalogs may join any code-owned Alias; saved MiniMax/Kimi
rows activate only exact sealed mappings. Both
endpoints are local reads.
They publish only currently routable models that have an
effective enabled protocol. Catalog refresh is an explicit
**Providers** action. A pricing refresh there can change which Go aliases
appear in that intersection.

## Model capabilities

The table below lists verified limits from `src/views/application-guides.ts`
(2026-08-14). Input is what OCG can actually carry. See
[Protocol Conversion](protocol-conversion.md) for the passthrough / conversion
matrix.

| Model | Context | Output | Input | Reasoning | Tools | Efforts |
| --- | ---: | ---: | --- | --- | :---: | --- |
| `grok-4.5` | 500K | 500K | text, image | always | ✓ | low / medium / high (default high) |
| `gpt-5.6-luna` | 1.05M | 128K | text, image | ✓ | ✓ | low / medium / high / max (default medium) |
| `muse-spark-1.2` | 1M | 128K | text, image | ✓ | ✓ | low / medium / high (default high) |
| `muse-spark-1.2-contributor` | 1M | 128K | text, image | ✓ | ✓ | low / medium / high (default high) |
| `glm-5.3` | 1M | 128K | text | ✓ | ✓ | low / high / max (default max) |
| `glm-5.2` | 1M | 128K | text | ✓ | ✓ | high / max (default max) |
| `glm-5.1` | 198K | 32K | text | ✓ | ✓ | — |
| `kimi-k3` | 1M | 128K | text, image, video | always | ✓ | max |
| `kimi-k2.7-code` | 256K | 256K | text, image, video | always | ✓ | — |
| `kimi-k2.6` | 256K | 64K | text, image, video | ✓ | ✓ | — |
| `mimo-v2.5` | 1M | 128K | text, image, audio, video | ✓ | ✓ | — |
| `mimo-v2.5-pro` | 1M | 128K | text | ✓ | ✓ | — |
| `minimax-m3` | 1M | 128K | text, image | ✓ | ✓ | — |
| `minimax-m2.7` | 200K | 128K | text | always | ✓ | — |
| `minimax-m2.7-highspeed` | 200K | 128K | text | always | ✓ | — |
| `minimax-m2.5` | 200K | 64K | text | always | ✓ | — |
| `minimax-m2.5-highspeed` | 200K | 64K | text | always | ✓ | — |
| `qwen3.8-max` | 1M | 128K | text | ✓ | ✓ | — |
| `qwen3.7-max` | 1M | 64K | text | ✓ | ✓ | — |
| `qwen3.7-plus` | 1M | 64K | text, image | ✓ | ✓ | — |
| `qwen3.6-plus` | 1M | 64K | text, image | ✓ | ✓ | — |
| `deepseek-v4-pro` | 1M | 384K | text | ✓ | ✓ | high / max (default high) |
| `deepseek-v4-flash` | 1M | 384K | text | ✓ | ✓ | high / max (default high) |
| `hy3` | 256K | 64K | text | ✓ | ✓ | low / high (default high) |

`muse-spark-1.2` uses Zero Data Retention (ZDR): prompts and completions are
not used for training. `muse-spark-1.2-contributor` is not ZDR; prompts and
completions may be used for training. Select Contributor only for data you are
authorized to use this way. The standard Muse price is measured from live Go
usage because the public Go pricing table lists only Contributor.

Rounded display: 198K = 202,752; 200K = 204,800; 256K = 262,144; 1M = 1,000,000
or 1,048,576. `glm-5.3` limits and token rates follow the dedicated
models.dev `opencode-go/glm-5.3` row (same $1.40 / $4.40 / $0.26 as the
official Go table; Usage remains $15).

Claude Desktop is the exception with durable model mappings: before its
configuration is copied, the selected `sonnet`, `opus`, and `haiku` targets
are saved to SQLite through protected
`GET/PUT /dashboard/api/v3/claude-desktop/models`. Omitted roles
inherit the first configured role, and the three roles cannot all be empty.
Its restore action returns to the mapping loaded or last saved in the current
page.

---

[User guide index](../USER.md) · [简体中文](applications.zh-CN.md) · [Docs index](../README.md)
