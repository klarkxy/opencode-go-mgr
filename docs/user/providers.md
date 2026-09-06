[简体中文](providers.zh-CN.md)

# Providers

Want to connect another upstream or contribute a built-in integration? Start with [Add a Provider](add-provider.md), which includes user-defined Providers, Custom API, and the sealed Adapter Registry path.

**Providers** is the supplier control plane — the page you land on when an old
bookmark still ends in `?view=pricing`.

The Adapter Registry stays static and sealed. Built-in Providers and
user-defined Providers share this page, labelled **Built-in** or
**User-defined**. Custom API is a Configurable HTTP adapter used as an
account-owned path. Scopes are split like this:

- `Provider(contract_scope_id)` for one exact built-in Provider contract.
  Existing scope IDs keep their historical Provider-shaped values.
- User-defined Providers persist as typed definitions and bind Configurable
  HTTP. Their Endpoint, protocol, auth kind, and mappings are edited here.
- `CustomEndpoint(account_id)` scopes keep Custom mappings account-owned.
  Edit those mappings on **Accounts**.

The left rail lists built-in contract scopes and user-defined Providers. Built-in
main panes keep two tabs: **Model catalog** and **Pricing**. The **OpenCode Go**
scope adds a third **Other** tab after Pricing for the managed-signup **invite
URL**. It is a user-owned `opencode.ai` / `console.opencode.ai` HTTPS link (not
a sealed origin). Fresh installs may ship a demo default; replace it with your
own link before a real signup. Creating a managed draft can also edit and write
this value back. User-defined panes show configuration, mappings, and
edit/delete. User-defined Providers are unpriced.

**Aliases** is a separate core page because its read-only table spans every
Provider contract, user-defined Provider mapping, and Custom account instead of
the selected Provider. It aggregates existing contracts and account capabilities
into public names with their routeability and exact upstream identities.
Custom mappings stay editable only on **Accounts**.

**Model catalog** is local. The matrix has one row per current catalog model and
three columns — Chat Completions, Responses, and Messages. Each cell is a binary
switch for the effective model/protocol state: turning it on writes `force_on`,
and turning it off writes `force_off`. Column menus can turn a whole protocol
column on or off. The switch updates immediately while the CAS-protected save
runs in the background; only affected cells show saving progress.

Underlying static, preset, and probe evidence remains in the contract, but is
not shown as a separate badge in this compact matrix. `auto` remains the stored
default until an explicit switch or a successful probe writes an override. A
successful provider-level probe pins `force_on`. Failed account attempts are
reported and retained as evidence, but never pin the shared protocol
`force_off`; only an explicit switch can do that.

For the built-in **OpenCode Go**, **Zen Free**, **Command Code GOAT**,
**MiniMax CN**, and **Kimi Code CN** scopes, the catalog header offers
**Restore official protocol baseline**. It makes no upstream request, keeps the
current model catalog, clears manual switches and probe evidence, and restores
the development-time official baseline reviewed on **2026-09-06**. OpenCode
Go and known Zen rows default to the one upstream endpoint documented for each
model. GOAT uses Messages for Anthropic model IDs and Chat Completions for the
other Provider families, with newly discovered non-preset models still off by
default. MiniMax CN and Kimi Code CN both default to Chat Completions and
Messages; neither advertises Responses. Protocols absent from the official
baseline remain off until an administrator explicitly enables a constructible
path or a successful explicit probe records positive evidence.

The compact source line, refresh action, and matrix share one content panel.
Every refreshable scope uses the same action. OpenCode Go refreshes from the
official authenticated model endpoint with a backend-selected eligible Go
account, Zen Free uses the fixed keyless directory
`https://opencode.ai/zen/v1/models`, and Command Code uses its fixed public
official `/models` directory without selecting an account. Refresh is always
explicit.

MiniMax and Kimi require an eligible account Key. MiniMax refreshes
`https://api.minimaxi.com/v1/models`; Kimi refreshes
`https://api.kimi.com/coding/v1/models`. Their saved rows activate only
code-owned sealed mappings; unmatched rows remain exact raw model IDs. MiniMax
maps M3, M2.7/M2.5/M2.1 standard and highspeed variants, and M2 to matching
lowercase kebab Aliases. Kimi maps `kimi-for-coding` → `kimi-k2.7-code`,
`kimi-for-coding-highspeed` → `kimi-k2.7-code-highspeed`, `k3` → `kimi-k3`,
and `k3-256k` → `kimi-k3-256k`. Forwarding retains every exact upstream ID.

Before the first successful refresh, the built-in static catalog is the initial
preset. After success, the saved official snapshot is authoritative and
replaces that preset. Models newly added by a refresh are visible in the matrix.
For OpenCode Go and Command Code, new protocol cells remain disabled until you
turn one on or a successful Test confirms it. MiniMax CN and Kimi Code CN rows
enable their sealed Chat Completions and Messages contracts; Responses stays
unsupported. Existing overrides and probe results for
surviving models are preserved. A failed or empty refresh keeps the previous
snapshot.

Custom API continues to use account-owned public-name → upstream-ID mappings;
discovery never silently replaces them. The account form **Fetch models** action
is an unsaved-form helper that returns upstream IDs only. Selecting one imports
an exact `public name = upstream ID` row. Command Code uses its public official
`/models` directory: the GOAT preset starts enabled, while additional models
discovered later start disabled until you enable their supported protocol in
the matrix.

Local catalogs feed resolution without another request-time upstream call.
Built-in Alias authority is static and code-owned: the original OpenCode Go
table supplies Go names, while sealed MiniMax CN, Kimi CN, and selected GOAT
long-name maps supply provider aliases without creating Go routes. Command
removes the Provider namespace and reuses an existing code-owned Alias; known
plan suffixes are removed only when the shorter name is already authorized.
For example, `nvidia/nemotron-3-ultra-550b-a55b` uses Alias
`nemotron-3-ultra`. Saved CN rows activate only their exact sealed map.
Unmatched Command/MiniMax/Kimi rows remain exact raw model IDs and are not advertised as
new Aliases; CN mappings keep the upstream ID's exact spelling. A Zen Free row
publishes its suffix-stripped Alias from the official `-free` suffix;
the original `-free` ID remains an exact raw pin,
as described under
[Zen Free models](routing.md#zen-free-models).

If every model/protocol cell for a Provider is off, that Provider contributes
no route. Authenticated downstream `GET /v1/models` publishes only routeable
public names. It omits raw-only identities and raw-name conflicts; an ambiguous
raw identity fails as `ambiguous_model_id` without an upstream request.

Every built-in Provider row has a **Test** button.
For each probeable protocol the provider automatically tries its eligible
accounts in saved routing order and stops at the first success. OpenCode Go and
Zen Free use their constructable protocol set. GOAT tests only its sealed native
family path: Messages for Anthropic IDs and Chat Completions otherwise. MiniMax
CN and Kimi Code CN test their sealed Chat Completions and Messages paths.
Custom endpoint tests remain account-owned. Models must belong to the current provider
catalog, including newly fetched models not yet in the static table. A
Popconfirm warns that these real minimal requests may consume quota. Each protocol
result is shown above the matrix with its success, failure, or skipped state,
HTTP status, readable upstream message, and a safe upstream help/billing link
when one is supplied. Every actual account attempt is recorded as a
redacted request log; probe traffic never enters Runtime Logs. One account
failure never disables a protocol that another eligible account can serve.

**Pricing** is scoped to the selected provider. **Refresh price table** only
hits the official source owned by that Provider. OpenCode and Command Code
keep separate revisions and last-good snapshots; one failing does not touch
the other. If a Provider later owns several priced Plans, the same action
refreshes those Plans only. Refresh stays manual:

- OpenCode Go shows revision, documentation timestamp, token rates, `Usage`,
  and the quota-debit multiplier, and can fetch
  `https://opencode.ai/docs/go/` after you press refresh. A failed fetch or
  validation keeps the last successful snapshot. The allowance is not a quota
  pool and does not route requests: it only derives that debit multiplier
  (`monthly limit / Usage`). Saving a temporary override creates a new
  persistent revision for later estimates.
- Command Code GOAT shows its saved official rate snapshot from
  `https://commandcode.ai/docs/plans/goat`. Models with scheduled pricing retain
  the official daily peak windows (01:00–04:00 and 06:00–10:00 UTC) and their
  separate input, output, and cache-read rates. Each priced model's applied
  multiplier can be edited and saved. The saved provider revision prices later
  requests; missing or ambiguous rows stay unpriced. A refresh asks before
  replacing edited multipliers. This remains separate from OpenCode Go. GOAT
  account cards use those priced OCG request logs for a local `$14 / $35 / $70`
  window estimate with manual baseline correction. Command Code exposes no
  machine-readable usage API.
- Zen Free is unpriced (egress-IP-shared free quota).
- Custom API is unpriced: successful forwards log `cost_state=unknown` with
  no quota debit and no official usage refresh.
- Ollama Cloud refreshes the public keyless directory `https://ollama.com/v1/models` without selecting an account. Discovered ids enable Chat Completions immediately; Responses and Messages are unsupported, and there is no protocol-probe entry. A refreshed catalog may append one routeable Ollama mapping to a Go-owned alias only when stripping the `:` tag leaves exactly one catalog match. Date-tagged snapshot ids come from the runtime catalog. Manual pricing refresh reads `https://ollama.com/pricing` (Model / Input / Cached input / Output) into the provider snapshot with quota multiplier `1.0`. New accounts require Pro/Max/Team plus a purchase date. Account cards estimate one monthly USD-Credits window from official per-request usage against that tier; used credit may exceed the soft limit, the bar clamps at 100%, and the meter never writes cooldown or changes routing. Migrated accounts with no billing row stay routeable without a meter.
- MiniMax CN and Kimi Code CN are unpriced in OCG, but their account cards can
  manually read the official subscription windows (`/token_plan/remains` and
  `/usages`). These snapshots are display-only and do not gate inference.

Request-time flow: Alias → account eligibility → adapter ceiling → saved
contract → per-model/per-protocol effective state → passthrough or conversion.
Protocol selection uses the saved contract. Authenticated `GET /v1/models` and
protected `GET /dashboard/api/v3/application-models` publish only currently
routable public names that have an effective enabled protocol. `application-models` stays Go aliases ∩ active pricing and excludes Custom.

---

[User guide index](../USER.md) · [简体中文](providers.zh-CN.md) · [Docs index](../README.md)
