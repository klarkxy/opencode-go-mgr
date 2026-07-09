# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="160">
</p>

> A Windows desktop app for managing OpenCode-Go accounts in the GUI, with a built-in OpenAI-compatible gateway that forwards through enabled keys using sequential fallback, cooldown handling, and usage logging.

[Features](#features) · [For Users](#for-users) · [For Maintainers](#for-maintainers) · [License](#license)

---

## Features

- **GUI account management** — add, edit, enable/disable, and switch between multiple OpenCode-Go accounts from the desktop app; keys are stored locally and never leave your machine.
- **OpenAI-compatible gateway** — a local `http://127.0.0.1:9042/v1/chat/completions` endpoint that any OpenAI SDK or tool can target. The gateway consumes the stored enabled keys; it is not an account-management UI or control plane. Also routes `POST /v1/messages` (Anthropic) and `GET /v1/models`.
- **Sequential fallback** — v1 always tries accounts in list/creation order, skipping disabled accounts, accounts in cooldown, and accounts already failed during the current request.
- **Per-account cooldown** — when OpenCode-Go returns 429, the `Resets in X days/hours/min` text is parsed and the account is skipped by the selector for exactly that long. Manual reset is a GUI account-card action.
- **Usage & cost tracking** — per-account 5h / weekly / monthly cost estimated from token counts against the official price table.
- **Built-in browser** — each account opens an isolated WebView2 window to the OpenCode-Go console for login, top-up, and key copying.
- **System tray** — runs in the background; close the window and it stays in the tray.
- **Local-first** — all data in a local SQLite file; no telemetry, no remote servers.

---

## For Users

### Requirements

- Windows 10 or 11 with **WebView2** (preinstalled on most modern Windows; otherwise get it from Microsoft).
- One or more OpenCode-Go API keys.

### Install

Download the latest NSIS installer from the Releases page and run it. It installs per-machine and creates a Start Menu entry plus a desktop shortcut. On uninstall you'll be asked whether to also remove the data directory.

To build from source instead, see [For Maintainers](#build-from-source).

### First run

1. Launch **OCG Manager**. It opens to the **Dashboard** and the gateway starts automatically on port `9042`.
2. Go to **账号管理 (Accounts)** → **添加账号 (Add)**. Enter a name (e.g. `主号`) and paste your OpenCode-Go API key. The key is obfuscated before being written to disk.
3. Add more accounts the same way. Enable the ones you want the gateway to use.
4. On the **Dashboard** or **设置 (Settings)**, copy your **Gateway Key** (format `ocg-xxxxxxxx-xxxxxxxx`). It's generated automatically on first run and can be regenerated any time.

That's it — your gateway is live at `http://127.0.0.1:9042/v1/`.

### Using the gateway

Point any OpenAI-compatible client at the gateway with your Gateway Key:

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

```python
from openai import OpenAI

client = OpenAI(base_url="http://127.0.0.1:9042/v1", api_key="ocg-xxxxxxxx-xxxxxxxx")
stream = client.chat.completions.create(
    model="glm-5.2",
    messages=[{"role": "user", "content": "hello"}],
    stream=True,
)
for chunk in stream:
    print(chunk.choices[0].delta.content or "", end="")
```

Notes:

- The gateway implements `POST /v1/chat/completions`, `POST /v1/messages` (Anthropic SDK shape), and `GET /v1/models`. Other OpenAI endpoints (`/embeddings`, etc.) are not routed.
- The request body is passed through unchanged — tools, JSON mode, temperature, `stream`, etc. all work.
- The client's `Authorization` header is consumed for gateway auth and **replaced** with the selected account's OCG key when forwarding upstream. The Anthropic-style `x-api-key` header is accepted as an auth alias. Gateway-private, hop-by-hop, cookie/proxy, and `accept-encoding` headers are filtered; the upstream request explicitly uses `Accept-Encoding: identity`.
- SSE streaming responses are relayed byte-for-byte.
- The gateway binds to `127.0.0.1` only — it is not reachable from the network.

### Sequential fallback

Account-level management belongs to the GUI (and the CLI's minimal `key` commands for headless use). The gateway only reads the stored account list, selects an enabled key, forwards the request, and records request outcomes such as cooldowns and logs.

There is no random or round-robin strategy in v1. The gateway tries accounts in list/creation order. "Available" means enabled, not in cooldown, and not already failed during the current request.

- `429`: parse the reset window, write `cooldown_until` and `last_error`, then immediately try the next available account.
- `401` / `403`: treat as key-level auth failure and try the next account without writing cooldown.
- `408` / `5xx` / network failure: for non-streaming requests, retry the same account once to preserve same-key cache locality; if it still fails, try the next account. Streaming requests do not do same-key retry.
- Other request-level `4xx`: pass the upstream response through to the client; do not fallback.

There is no fixed attempt cap. The gateway keeps going until no selectable account remains. If every account is in cooldown, the client gets `429` with the soonest reset time; if all available accounts fail for non-cooldown reasons, the client gets `502` with the last error.

### Cooldown

Each account has a `cooldown_until` column populated when OpenCode-Go returns a 429. The `Resets in …` text in the 429 body is parsed by `gateway/limit.rs` (`parse_reset`) and the resulting `Duration` (minutes / hours / days) is written verbatim. The selector skips any account whose `cooldown_until > now`. The gateway writes this request-result state; viewing and manually resetting it is handled by the GUI account page.

| 429 phrase | Cooldown |
|------------|----------|
| `5-hour usage limit reached. Resets in 13min.` | 13 minutes |
| `Weekly usage limit reached. Resets in 4 days.` | 4 days |
| `Monthly usage limit reached. Resets in 13 days.` | 13 days |

- A successful request does not clear cooldown on its own — only **重置冷却 (Reset cooldown)** on the account card does, or letting the timer expire.
- Unknown 429 phrases (no `Resets in` text, or unrecognized unit) leave `cooldown_until` unset; the account is treated as available.

### Usage & cost

The **Dashboard** shows today's / this week's / this month's total cost across all accounts; each account card shows its own 5h / week / month totals. Costs are estimated from token counts in each response using the OpenCode-Go price table:

| Model | Input $/M | Output $/M | Cache read $/M |
|-------|----------:|-----------:|---------------:|
| glm-5.2 | 1.40 | 4.40 | 0.26 |
| glm-5.1 | 1.40 | 4.40 | 0.26 |
| kimi-k2.7-code | 0.95 | 4.00 | 0.19 |
| kimi-k2.6 | 0.95 | 4.00 | 0.16 |
| deepseek-v4-pro | 1.74 | 3.48 | 0.0145 |
| deepseek-v4-flash | 0.14 | 0.28 | 0.0028 |
| mimo-v2.5 | 0.14 | 0.28 | 0.0028 |
| mimo-v2.5-pro | 1.74 | 3.48 | 0.0145 |

OpenCode-Go quota windows for reference: **5h = $12**, **weekly = $30**, **monthly = $60**. The app shows your spend against these but does **not** enforce them.

> ⚠️ **Streaming usage is best-effort.** Non-streaming responses usually include a parseable `usage` block. Streaming responses are logged with token/cost data when the upstream emits a usage chunk; otherwise they finish as `success_no_usage` with `0` tokens and `$0.00` cost. If you rely on exact usage numbers, prefer non-streaming calls or enable usage chunks where your client supports it.

### Built-in browser

On any account card, **打开浏览器 (Open browser)** opens an isolated WebView2 window to the OpenCode-Go console. Each account uses its own profile directory, so cookies/sessions stay separate. Only one browser window is open at a time — opening a new one closes the previous.

Use it to log in, check quota, top up, or copy a key without leaving the app.

### System tray

- The app lives in the Windows tray. **Left-click** the tray icon (or use the right-click menu → **打开管理界面**) to show the window.
- Closing the window **hides** it to the tray — the app and gateway keep running.
- Right-click menu: **打开管理界面** / **查看网关状态** / **退出**. Use **退出** to fully stop the gateway and exit.

### Data & security

- **Data directory:** `%USERPROFILE%\.ocg-mgr\` — contains `data.sqlite` (accounts, logs, cooldown, settings) and `profiles/<account-id>/` (per-account WebView2 data). The uninstaller asks whether to delete it.
- **Key storage:** API keys are obfuscated with a machine-bound transform before being written to `data.sqlite`. This stops casual plaintext inspection but is **not** strong cryptography — it is not a substitute for a key vault. Anyone with read access to your user profile and the binary can recover the keys.
- **Gateway key:** a local secret that gates access to the gateway. Keep it private; anyone who has it (and local access) can spend your accounts' quota.
- **Network:** the gateway binds to `127.0.0.1` only. Do not tunnel it to the public internet without adding your own auth and TLS.
- **No telemetry.** Nothing is sent to any server other than your requests to the OpenCode-Go upstream.
- **Compliance:** the app does not auto-register accounts, auto-recharge, or bypass captchas. Referral codes are display/copy only.

### FAQ

**The dashboard says $0 but I've been using it.** The upstream response probably did not include usage data. For streaming calls, enable usage chunks if your client supports them; otherwise use non-streaming calls for exact dashboard numbers.

**I changed the port in Settings but my client still connects to 9042.** Update your client's `base_url` to the new port. The change takes effect after **重启 Gateway (Restart gateway)**.

**An account is stuck in cooldown.** Use **重置冷却 (Reset cooldown)** on the account card. Cooldowns are not "monthly blown" — they come from the upstream 429 text directly.

**Can I use this across the LAN / from another machine?** Run `ocg-manager-cli serve --admin-port 9091` on the headless machine; the GUI then pushes keys to that endpoint. The admin API binds `127.0.0.1` only, so put a caddy/traefik reverse proxy in front for HTTPS and any client-side auth you want. The local sqlite is always authoritative; remote is a push-mirror, not a control plane.

### Known limitations

- Only `/v1/chat/completions`, `POST /v1/messages`, and `GET /v1/models` are implemented. No `/embeddings` and no Gemini protocol conversion.
- Streaming usage depends on upstream usage chunks; streams without usage are logged as `success_no_usage` with 0 tokens and $0.00 cost.
- The **测试 (Test)** button on an account only checks that the stored key can be decrypted and shows a masked preview — it does **not** send a probe request to the upstream. (The CLI has a separate `key ping` subcommand that does probe.)
- The **开机自启 (Launch on startup)** toggle is saved but not yet wired to the OS.
- Quota windows are displayed, not enforced.
- Single Gateway Key only (multi-key is a future option).

---

## For Maintainers

### Build from source

Prerequisites:

- **Node.js** v20+ (v24 confirmed working)
- **Rust** ≥ 1.85 (edition 2024, MSRV enforced in `src-tauri/Cargo.toml`)
- **Tauri CLI** — included as a dev dependency, so `npx tauri ...` works without a global install.
- Windows 10/11 with WebView2.

```bash
# Frontend deps
npm install

# Frontend-only dev server (http://127.0.0.1:30001)
npm run dev

# Full Tauri app (frontend + Rust + desktop window)
npx tauri dev

# Type-check + production frontend build
npm run build        # = vue-tsc && vite build

# Rust checks / release binary
cd src-tauri && cargo check
cd src-tauri && cargo build --release

# Windows NSIS installer → src-tauri/target/release/bundle/nsis/
npx tauri build
```

### Project structure

```
.
├── assets/logo/                # logo source + generator scripts
├── docs/{en,zh}/               # logo usage + getting-started notes
├── src/                        # Vue 3 + TypeScript frontend
│   ├── api/tauri.ts            # invoke() wrappers + shared TS types
│   ├── views/                  # Dashboard / Accounts / Logs / Settings
│   ├── App.vue                 # shell + sidebar nav
│   ├── main.ts                 # Vue + naive-ui bootstrap
│   └── styles/main.css
├── crates/                     # Rust workspace members
│   ├── ocg-core/               # cross-platform lib: gateway, DB, models, crypto, state
│   │   └── src/
│   │       ├── lib.rs          # re-exports: crypto/db/gateway/models/state
│   │       ├── crypto.rs       # KeyCipher trait + MachineBound/Static ciphers
│   │       ├── db.rs           # SQLite open + migrations + queries
│   │       ├── models.rs       # serde structs, AppConfig, enums
│   │       ├── state.rs        # CoreState, config load/save, gateway-key gen
│   │       └── gateway/        # Axum router, handler, forwarder, selector, limit, cost
│   └── ocg-cli/                # headless CLI binary
│       ├── Cargo.toml
│       └── src/main.rs         # clap: serve / key (list|add|remove|enable|disable|ping) / status
├── src-tauri/                  # Tauri GUI binary (depends on ocg-core)
│   ├── capabilities/default.json   # Tauri v2 permissions for the main window
│   ├── icons/                  # 32/128/256/512 + icon.ico
│   ├── installer.nsh           # NSIS hook: prompt to delete data dir on uninstall
│   ├── src/
│   │   ├── commands/           # Tauri command handlers (account/setting/gateway/log/dashboard/browser)
│   │   ├── state.rs            # GuiState wraps CoreState + current_browser_window
│   │   ├── tray.rs             # system tray setup
│   │   ├── lib.rs              # run() — wires DB, gateway, tray, commands
│   │   └── main.rs             # exe entry → run()
│   ├── Cargo.toml
│   ├── build.rs
│   └── tauri.conf.json
├── Cargo.toml                  # workspace root (members = crates/ocg-core, crates/ocg-cli, src-tauri)
├── package.json
├── vite.config.ts              # dev port 30001, @ alias
├── tsconfig.json
└── index.html
```

### Architecture

```text
Client (OpenAI SDK / curl / any tool)
  │  POST http://127.0.0.1:9042/v1/chat/completions
  │  Authorization: Bearer <gateway-key>
  ▼
┌─────────────────────────── ocg-core (cross-platform lib) ───────────────────────────┐
│  Axum gateway (mod/handler/forwarder/selector/limit/cost)                            │
│    ├── auth: verify Bearer <gateway-key> (or x-api-key)                              │
│    ├── AccountSelector: pick first enabled, non-cooldown account in list order       │
│    ├── Forwarder: relay body to {upstream}/<path> with account's OCG key            │
│    │     ├── SSE stream passthrough  ─► client (bytes unchanged)                     │
│    │     └── JSON response           ─► parse usage, estimate cost, log              │
│    ├── 429 parser (limit.rs): extract `Resets in X` → write cooldown_until          │
│    ├── DB (rusqlite): accounts, settings, logs                                       │
│    └── KeyCipher trait: MachineBoundCipher (GUI) | StaticKeyCipher (CLI)             │
│                                                                                       │
│   Fallback: 429 -> cooldown + next; 408/5xx/network -> same-key retry once, then next │
└─────────────────────────────────────────────────────────────────────────────────────┘
  ▲                                                  ▲
  │ Tauri invoke (state.core.*)                      │ CoreState + StaticKeyCipher
  │                                                  │
ocg-manager (Tauri GUI, Windows)             ocg-manager-cli (Linux/macOS/Win/Docker)
├── Account-management UI                     ├── serve / minimal key store / status
├── WebView2 UI (Vue 3 + naive-ui)            └── uses same gateway + db primitives
├── System tray (tray.rs)
└── MachineBoundCipher (default)
```

### Key internals

- **Gateway wiring** — `gateway/mod.rs` builds the Axum router (`/v1/chat/completions`, `/v1/messages`, `/v1/models` + permissive CORS), binds `127.0.0.1:<port>`, and runs with a graceful-shutdown oneshot. `start_gateway` / `stop_gateway` are called from `lib.rs` (startup) and `commands/gateway.rs` (restart).
- **Request handler** — `gateway/handler.rs` checks the gateway key (Bearer or `x-api-key`), then loops until no selectable account remains. It tracks accounts that failed during the current request, retries the same account once for non-streaming `408` / `5xx` / network failures, falls back immediately for `429` / `401` / `403`, and passes through ordinary request-level `4xx`. Returns `401` / `503` (no accounts) / `429` (all in cooldown, with soonest reset) / `502` (all failed).
- **Forwarder** — `gateway/forwarder.rs` decrypts the account key, posts the **raw body** to `{upstream_base_url}{path}` (120 s timeout), filters gateway-private / hop-by-hop / `accept-encoding` headers, replaces auth with the account's OCG key, and sends `Accept-Encoding: identity`. It then streams SSE bytes through or parses the JSON `usage` block to estimate cost. On a 429 response, `parse_reset` extracts the cooldown duration and writes `accounts.cooldown_until` plus `last_error`. Logs every attempt to `forward_logs`.
- **Selector** — `gateway/selector.rs` lists accounts, filters out disabled / excluded / accounts whose `cooldown_until > now`, and returns the first remaining account. v1 has no random or round-robin strategy.
- **Cooldown parser** — `gateway/limit.rs::parse_reset`. Recognises `Resets in N min|hour|day(s)` in upstream 429 messages; minutes/hours/days are all supported, other units return `None`.
- **Cost** — `gateway/cost.rs` holds the price `HashMap`, normalizes model names (lowercase, separators → `-`), and computes `cost = prompt/1e6·in + completion/1e6·out + cached/1e6·cache_read`. Unknown models fall back to a fuzzy match, then a default price.
- **Crypto** — `crates/ocg-core/src/crypto.rs`. Machine-bound XOR obfuscation seeded from `USERNAME` / `COMPUTERNAME` / `APPDATA`. Documented as **not** cryptographically secure; swap for `aes-gcm` / a KMS if real secrecy is needed.
- **DB** — `crates/ocg-core/src/db.rs`. SQLite at `<data_dir>/data.sqlite`, schema versioned via `schema_version` (currently v2). Tables: `accounts`, `settings`, `gateway_logs`, `forward_logs`. Indices on `forward_logs(timestamp)` and `forward_logs(account_id)`. The v2 migration added `accounts.cooldown_until` and `accounts.last_error`; there is no separate `circuit_states` table.
- **State** — `crates/ocg-core/src/state.rs`. `CoreState` (Arc) lives in `ocg-core` and holds `Mutex<Database>`, `Mutex<AppConfig>`, `Mutex<Option<GatewayHandle>>`, a shared `reqwest::Client` (built once with a 120 s timeout), `PathBuf`, and `Arc<dyn KeyCipher>`. Config is serialized to the `settings` table under key `config`; gateway key auto-generated as `ocg-<word>-<word>` on first run. The GUI's `src-tauri/src/state.rs` wraps it in `GuiState { core, current_browser_window }`; Tauri commands access state via `state.core.*`.
- **Tray** — `src-tauri/src/tray.rs`. Left-click shows the main window; right-click menu opens/status/quit. `lib.rs` intercepts `CloseRequested` to hide instead of close.
- **Tauri commands** — registered in `lib.rs::invoke_handler!`; typed wrappers in `src/api/tauri.ts`. Add a new command in both places.

### Data model

`accounts(id, name, key_cipher, enabled, referral_code, recharge_date, cooldown_until, last_error, created_at, updated_at)` — `cooldown_until` and `last_error` added in schema v2
`settings(key, value)` — app config stored as JSON under `key = "config"`
`gateway_logs(id, level, category, message, created_at)`
`forward_logs(id, timestamp, model, account_id, account_name, status, http_status, prompt_tokens, completion_tokens, cached_tokens, cost, error_message)`

### Configuration

`AppConfig` (`models.rs`, default values shown):

| Field | Default | Notes |
|-------|---------|-------|
| `gateway_port` | `9042` | Bind port of the local gateway. |
| `gateway_key` | auto `ocg-xxxxxxxx-xxxxxxxx` | Regeneratable from Settings. |
| `upstream_base_url` | `https://opencode.ai/zen/go` | OpenCode-Go API base. The path is appended to this in the forwarder; do not include `/v1`. |
| `auto_start` | `false` | Saved only — not yet wired to OS startup. |
| `remote.url` | `` (empty) | When set, the GUI pushes account keys to this remote admin API on every local change. Empty = local-only. |
| `remote.token` | `` | Bearer token for `remote.url`. Generated by `ocg-manager-cli serve --admin-port` on first start. |

Editable in-app (Settings) or directly in the `settings` table. After changing the port, restart the gateway. `tauri.conf.json` `security.csp` hardcodes `connect-src http://localhost:9042`; update it if you rely on the webview talking to a different port (the gateway is normally hit by external clients, not the webview, so this rarely matters).

### Extending

- **Add a model to the price table** — add an entry in `gateway/cost.rs::price_table_cell()`. Names are normalized (lowercase, ` /_.` → `-`), so match the upstream's model id loosely.
- **Tune cooldown parsing** — `gateway/limit.rs::parse_reset`: add a unit arm in the `match` to recognise more 429 phrasings.
- **Change the default port / upstream** — `models.rs::AppConfig::default()`.
- **Add a Tauri command** — write the `#[tauri::command]` fn under `src-tauri/src/commands/`, register it in `lib.rs::invoke_handler!`, and add a typed wrapper in `src/api/tauri.ts`.

### Testing

- `cargo test --workspace` — runs the `ocg-core` tests: `crypto` round-trip (MachineBoundCipher, StaticKeyCipher) and the `gateway::limit::parse_reset` known-message cases.
- No frontend tests. Worth adding: sequential selector behavior, cooldown math, cost estimation, and an integration test with a mock upstream.

### Headless CLI (`ocg-manager-cli`)

The CLI shares the `ocg-core` gateway and database primitives, and adds a minimal headless key-store interface for servers. It is not the GUI account-management surface: no account cards, no browser profiles, no tray, no WebView. It runs as a single `data.sqlite` plus an Axum server on Linux, macOS, Windows, and inside Docker. Pass `--admin-port <u16>` to also expose a remote-sync admin API on `127.0.0.1:<port>` (Bearer-authenticated); put a reverse proxy in front for HTTPS — the binary does not terminate TLS itself.

```bash
# Build
cargo build --release --bin ocg-manager-cli

# Start the gateway (defaults: port 9042, data dir ~/.ocg-mgr-cli)
./target/release/ocg-manager-cli serve

# With overrides
./target/release/ocg-manager-cli --data-dir /var/lib/ocg --encryption-key "$OCG_KEY" serve --port 9042

# Manage keys
ocg-manager-cli key list
ocg-manager-cli key add main sk-ocg-xxxxxxxx
ocg-manager-cli key remove <account-id>
ocg-manager-cli key enable <account-id>
ocg-manager-cli key disable <account-id>

# Probe upstream (real HTTP) — pings one or all enabled accounts
ocg-manager-cli key ping                        # all enabled accounts
ocg-manager-cli key ping <account-id>           # one account
ocg-manager-cli key ping <account-id> --model deepseek-v4-flash --message "hello"

# Show runtime status
ocg-manager-cli status
```

**Encryption key.** The CLI uses `StaticKeyCipher` (not machine-bound), so you can move its data dir to another host and decrypt. Resolution order:

1. `--encryption-key <secret>` argument
2. `OCG_MANAGER_ENCRYPTION_KEY` environment variable
3. `<data-dir>/.encryption-key` file (auto-generated on first run — back it up)

**Data dir.** Default is `~/.ocg-mgr-cli`. Pass `--data-dir` to override. Don't share the directory with the GUI's `%USERPROFILE%/.ocg-mgr/data.sqlite` — the GUI uses `MachineBoundCipher` and the two ciphers are not interchangeable.

### Packaging & release

- `npx tauri build` produces a per-machine **NSIS** installer under `src-tauri/target/release/bundle/nsis/`.
- Release profile (`Cargo.toml`): `opt-level = 3`, `lto = true`, `strip = true`, `panic = "abort"` → a single stripped exe.
- `installer.nsh` adds an uninstall hook that prompts before deleting `%USERPROFILE%\.ocg-mgr`.
- No CI/CD or auto-publish is configured; cut releases manually.

### Known gaps / TODOs

- No automated tests beyond `crypto` round-trip and `parse_reset`.
- Streaming requests log zero tokens/cost (usage stats cover non-streaming only).
- `test_account` doesn't probe the upstream — it only decrypts and masks the key. (The CLI's `key ping` does.)
- `auto_start` flag is not wired to an OS autostart entry (no `tauri-plugin-autostart` registered).
- Monthly cooldown auto-reset by recharge date is not implemented (manual reset only).
- `reqwest::Client` is built once in `CoreStateInner::new` with a 120 s timeout, so connection pooling is per-process and per-host — good enough for now.
- Crypto is obfuscation, not AEAD.
- `tauri-plugin-store` is declared as an npm dependency but commented out on the Rust side.

---

## License

MIT
