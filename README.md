# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="160">
</p>

> A Windows desktop app for managing multiple OpenCode-Go API keys, with a built-in OpenAI-compatible gateway that transparently rotates, fail-overs, and tracks usage across accounts.

[Features](#features) · [For Users](#for-users) · [For Maintainers](#for-maintainers) · [License](#license)

---

## Features

- **Multi-account management** — add, edit, enable/disable, and switch between multiple OpenCode-Go accounts; keys are stored locally and never leave your machine.
- **OpenAI-compatible gateway** — a local `http://127.0.0.1:9042/v1/chat/completions` endpoint that any OpenAI SDK or tool can target.
- **Rotation & fail-over** — sequential / random / round-robin selection; on failure the gateway automatically retries the next available account (up to 5 attempts).
- **Per-account circuit breaker** — escalating cooldowns when an account errors repeatedly; automatic recovery, manual reset.
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

- The gateway implements **only** `POST /v1/chat/completions`. Other OpenAI endpoints (`/models`, `/embeddings`, …) are not routed.
- The request body is passed through unchanged — tools, JSON mode, temperature, `stream`, etc. all work.
- The client's `Authorization` header is consumed for gateway auth and **replaced** with the selected account's OCG key when forwarding upstream. Other request headers are not forwarded.
- SSE streaming responses are relayed byte-for-byte.
- The gateway binds to `127.0.0.1` only — it is not reachable from the network.

### Account selection & fail-over

Set the strategy in **Settings**:

| Strategy | Behavior |
|----------|----------|
| Sequential | Pick the first available account in list order. |
| Random | Pick a random available account. |
| Round-robin | Cycle through available accounts. |

"Available" means enabled **and** not in circuit-breaker cooldown. If the chosen account's request fails (4xx/5xx/timeout), the gateway excludes it and tries the next available account, up to 5 attempts. If all fail you get a `502` with the last error.

### Circuit breaker

Each account tracks consecutive errors independently. When errors accumulate within a time window, the account enters a cooldown and is skipped by the selector.

| Trigger | Cooldown | Meaning |
|---------|----------|---------|
| 6 errors within 30 min | 5 minutes | Likely rate-limit or network blip |
| 6 errors within 5 h | 1 hour | 5h quota likely exhausted |
| 6 errors within 24 h | 1 day | Weekly quota likely exhausted |
| 7 errors within 7 days | Monthly blown | Monthly quota likely exhausted |

- Any **successful** request resets the error counter.
- Short / medium / long cooldowns recover automatically when they expire.
- **Monthly blown** does not auto-recover — use **重置熔断 (Reset circuit)** on the account card, or top up and reset.

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

> ⚠️ **Streaming requests are not cost-tracked.** Only non-streaming responses include a parseable `usage` block, so streaming completions are logged with `0` tokens and `$0.00` cost. If you rely on usage numbers, prefer non-streaming calls, or treat streaming usage as an under-count.

### Built-in browser

On any account card, **打开浏览器 (Open browser)** opens an isolated WebView2 window to the OpenCode-Go console. Each account uses its own profile directory, so cookies/sessions stay separate. Only one browser window is open at a time — opening a new one closes the previous.

Use it to log in, check quota, top up, or copy a key without leaving the app.

### System tray

- The app lives in the Windows tray. **Left-click** the tray icon (or use the right-click menu → **打开管理界面**) to show the window.
- Closing the window **hides** it to the tray — the app and gateway keep running.
- Right-click menu: **打开管理界面** / **查看网关状态** / **退出**. Use **退出** to fully stop the gateway and exit.

### Data & security

- **Data directory:** `%USERPROFILE%\.ocg-mgr\` — contains `data.sqlite` (accounts, logs, circuit state, settings) and `profiles/<account-id>/` (per-account WebView2 data). The uninstaller asks whether to delete it.
- **Key storage:** API keys are obfuscated with a machine-bound transform before being written to `data.sqlite`. This stops casual plaintext inspection but is **not** strong cryptography — it is not a substitute for a key vault. Anyone with read access to your user profile and the binary can recover the keys.
- **Gateway key:** a local secret that gates access to the gateway. Keep it private; anyone who has it (and local access) can spend your accounts' quota.
- **Network:** the gateway binds to `127.0.0.1` only. Do not tunnel it to the public internet without adding your own auth and TLS.
- **No telemetry.** Nothing is sent to any server other than your requests to the OpenCode-Go upstream.
- **Compliance:** the app does not auto-register accounts, auto-recharge, or bypass captchas. Referral codes are display/copy only.

### FAQ

**The dashboard says $0 but I've been using it.** You're probably using `stream: true`. See the [streaming caveat](#usage--cost) — streaming usage isn't recorded.

**I changed the port in Settings but my client still connects to 9042.** Update your client's `base_url` to the new port. The change takes effect after **重启 Gateway (Restart gateway)**.

**An account is stuck in "monthly blown".** Use **重置熔断** on the account card after topping up.

**Can I use this across the LAN / from another machine?** Not as shipped — the gateway binds to `127.0.0.1`. Use a local tunnel (e.g. `ngrok`, `cloudflared`) if you must, and understand the security implications.

### Known limitations

- Only `/v1/chat/completions` is implemented; no `/models`, `/embeddings`, or Anthropic/Gemini protocol conversion.
- Streaming requests are not reflected in usage/cost stats.
- The **测试 (Test)** button on an account only checks that the stored key can be decrypted and shows a masked preview — it does **not** send a probe request to the upstream.
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
│   │       └── gateway/        # Axum router, handler, forwarder, selector, circuit_breaker, cost
│   └── ocg-cli/                # headless CLI binary
│       ├── Cargo.toml
│       └── src/main.rs         # clap: serve / key / circuit / status
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
│  Axum gateway (mod/handler/forwarder/selector/circuit_breaker/cost)                │
│    ├── auth: verify Bearer <gateway-key>                                            │
│    ├── AccountSelector: pick enabled, circuit-healthy account by strategy          │
│    ├── Forwarder: relay body to {upstream}/v1/chat/completions with account's OCG  │
│    │     ├── SSE stream passthrough  ─► client (bytes unchanged)                   │
│    │     └── JSON response           ─► parse usage, estimate cost, log            │
│    ├── CircuitBreaker: record success/error per account, escalate cooldowns        │
│    ├── DB (rusqlite): accounts, settings, logs, circuit_states                      │
│    └── KeyCipher trait: MachineBoundCipher (GUI) | StaticKeyCipher (CLI)           │
│                                                                                     │
│   On failure: exclude account, retry next (≤5 attempts) → 502 if all fail          │
└─────────────────────────────────────────────────────────────────────────────────────┘
  ▲                                                  ▲
  │ Tauri invoke (state.core.*)                      │ CoreState + StaticKeyCipher
  │                                                  │
ocg-manager (Tauri GUI, Windows)             ocg-manager-cli (Linux/macOS/Win/Docker)
├── WebView2 UI (Vue 3 + naive-ui)            ├── serve / key / circuit / status
├── System tray (tray.rs)                     └── uses same gateway + db
└── MachineBoundCipher (default)
```

### Key internals

- **Gateway wiring** — `gateway/mod.rs` builds the Axum router (`/v1/chat/completions` + permissive CORS), binds `127.0.0.1:<port>`, and runs with a graceful-shutdown oneshot. `start_gateway` / `stop_gateway` are called from `lib.rs` (startup) and `commands/gateway.rs` (restart).
- **Request handler** — `gateway/handler.rs` checks the gateway key, then loops up to 5 attempts: select an account (excluding the last failed one), forward, and return on success or exclude-and-retry. Returns `401` / `503` (no accounts) / `502` (all failed).
- **Forwarder** — `gateway/forwarder.rs` decrypts the account key, posts the **raw body** to `{upstream_base_url}/v1/chat/completions` (120 s timeout), then either streams SSE bytes through or parses the JSON `usage` block to estimate cost. Logs every attempt to `forward_logs`; records success/error with the circuit breaker.
- **Selector** — `gateway/selector.rs` lists accounts, filters out disabled / excluded / circuit-unavailable ones, and picks by strategy (sequential = first; random = nano-time index; round-robin = atomic counter).
- **Circuit breaker** — `gateway/circuit_breaker.rs`. `ERROR_THRESHOLD = 6`, `MONTHLY_THRESHOLD = 7`. `evaluate_level` maps the error-count + window to a level; `is_available` is false while `cooldown_until > now` or level is `MonthlyBlown`. Success resets the counter.
- **Cost** — `gateway/cost.rs` holds the price `HashMap`, normalizes model names (lowercase, separators → `-`), and computes `cost = prompt/1e6·in + completion/1e6·out + cached/1e6·cache_read`. Unknown models fall back to a fuzzy match, then a default price.
- **Crypto** — `crypto.rs`. Machine-bound XOR obfuscation seeded from `USERNAME` / `COMPUTERNAME` / `APPDATA`. Documented as **not** cryptographically secure; swap for `aes-gcm` / a KMS if real secrecy is needed.
- **DB** — `db.rs`. SQLite at `<data_dir>/data.sqlite`, schema versioned via `schema_version`. Tables: `accounts`, `settings`, `gateway_logs`, `forward_logs`, `circuit_states`. Indices on `forward_logs(timestamp)` and `forward_logs(account_id)`.
- **State** — `state.rs`. `CoreState` (Arc) lives in `ocg-core` and holds `Mutex<Database>`, `Mutex<AppConfig>`, `Mutex<Option<GatewayHandle>>`, an atomic round-robin counter, `reqwest::Client`, `PathBuf`, and `Arc<dyn KeyCipher>`. Config is serialized to the `settings` table under key `config`; gateway key auto-generated as `ocg-<word>-<word>` on first run. The GUI's `src-tauri/src/state.rs` wraps it in `GuiState { core, current_browser_window }`; Tauri commands access state via `state.core.*`.
- **Tray** — `tray.rs`. Left-click shows the main window; right-click menu opens/status/quit. `lib.rs` intercepts `CloseRequested` to hide instead of close.
- **Tauri commands** — registered in `lib.rs::invoke_handler!`; typed wrappers in `src/api/tauri.ts`. Add a new command in both places.

### Data model

`accounts(id, name, key_cipher, enabled, referral_code, recharge_date, created_at, updated_at)`
`settings(key, value)` — app config stored as JSON under `key = "config"`
`gateway_logs(id, level, category, message, created_at)`
`forward_logs(id, timestamp, model, account_id, account_name, status, http_status, prompt_tokens, completion_tokens, cached_tokens, cost, error_message)`
`circuit_states(account_id, consecutive_errors, first_error_at, last_error_at, cooldown_until, level)`

### Configuration

`AppConfig` (`models.rs`, default values shown):

| Field | Default | Notes |
|-------|---------|-------|
| `gateway_port` | `9042` | Bind port of the local gateway. |
| `gateway_key` | auto `ocg-xxxxxxxx-xxxxxxxx` | Regeneratable from Settings. |
| `selection_strategy` | `sequential` | `sequential` / `random` / `round_robin`. |
| `upstream_base_url` | `https://api.opencode.ai` | OpenCode-Go API base. |
| `auto_start` | `false` | Saved only — not yet wired to OS startup. |

Editable in-app (Settings) or directly in the `settings` table. After changing the port, restart the gateway. `tauri.conf.json` `security.csp` hardcodes `connect-src http://localhost:9042`; update it if you rely on the webview talking to a different port (the gateway is normally hit by external clients, not the webview, so this rarely matters).

### Extending

- **Add a model to the price table** — add an entry in `gateway/cost.rs::price_table()`. Names are normalized (lowercase, ` /_.` → `-`), so match the upstream's model id loosely.
- **Tune circuit thresholds** — `gateway/circuit_breaker.rs`: `ERROR_THRESHOLD`, `MONTHLY_THRESHOLD`, and the windows in `evaluate_level`.
- **Change the default port / upstream** — `models.rs::AppConfig::default()`.
- **Add a Tauri command** — write the `#[tauri::command]` fn under `src-tauri/src/commands/`, register it in `lib.rs::invoke_handler!`, and add a typed wrapper in `src/api/tauri.ts`.

### Testing

- `cargo test --workspace` — runs the `ocg-core` crypto round-trip tests (MachineBoundCipher, StaticKeyCipher).
- No frontend tests. Worth adding: selector strategy, circuit-breaker state machine, cost estimation, and an integration test with a mock upstream.

### Headless CLI (`ocg-manager-cli`)

The same Gateway and account-management code that the GUI uses is also exposed as a cross-platform CLI binary. It has no UI, no tray, no WebView — just a single `data.sqlite` plus an Axum server, so it runs on Linux, macOS, Windows, and inside Docker.

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

# Circuit breaker
ocg-manager-cli circuit reset <account-id>

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

- No automated tests beyond `crypto` round-trip.
- Streaming requests log zero tokens/cost (usage stats cover non-streaming only).
- `test_account` doesn't probe the upstream — it only decrypts and masks the key.
- `auto_start` flag is not wired to an OS autostart entry (no `tauri-plugin-autostart` registered).
- Monthly circuit reset by recharge date is not implemented (manual reset only).
- `reqwest::Client` is constructed per request — no connection pooling.
- Crypto is obfuscation, not AEAD.
- `tauri-plugin-store` is declared as an npm dependency but commented out on the Rust side.

---

## License

MIT
