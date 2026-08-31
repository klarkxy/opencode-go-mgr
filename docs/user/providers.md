[简体中文](providers.zh-CN.md)

# Providers

Want to connect another upstream or contribute a built-in integration? Start with [Add a Provider](add-provider.md), which includes the upstream HTTP contract and the sealed-registry path.

**Providers** is the supplier control plane — the page you land on when an old
bookmark still ends in `?view=pricing`.

Under the hood it is a static Provider Registry plus a handful of
capability-specific adapters. Custom API is a Configurable HTTP adapter, not a base class
everyone inherits from. Scopes are split like this:

- `Provider(contract_scope_id)` for one exact built-in Provider/Offering
  contract. Existing scope IDs keep their historical Provider-shaped values.
- `CustomEndpoint(account_id)` scopes keep Custom mappings account-owned and
  never editable from this page.

The left rail lists the built-in Provider/Offering contract scopes. The main pane has three tabs:
**Model catalog**, **Pricing**, and **Alias**. The old catalog and
model-contract views are merged into one matrix on the Model catalog tab.

**Alias** is read-only. It aggregates the existing Provider contracts and
account capabilities into public names, with
their routeability and exact upstream identities. It does not create a new
Alias API, store, cache, or editor. A Custom mapping links to the one editor on
**Accounts** with `?view=accounts&account_id=<id>`; loading that link opens the
matching account editor. Closing the editor removes `account_id`; an unknown
account shows a notice and clears the stale parameter.

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
**MiniMax CN**, and **Kimi Code CN** scopes,
the catalog header offers **Restore static
protocol snapshot**. It makes no upstream request, keeps the current model
catalog, clears manual switches and probe evidence, and restores the static
protocol snapshot dated **2026-08-27**. Any current-catalog protocol pair absent
from that static snapshot is left off, except MiniMax CN and Kimi Code CN rows:
their sealed adapters declare Chat Completions as their only supported upstream
protocol.

The compact source line, refresh action, and matrix share one content panel;
there is no separate catalog-summary card and no refresh-account selector.
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
No request-time upstream lookup is performed.

Before the first successful refresh, the built-in static catalog is the initial
preset. After success, the saved official snapshot is authoritative and
replaces that preset. Models newly added by a refresh are visible in the matrix.
For OpenCode Go and Command Code, new protocol cells remain disabled until you
turn one on or a successful Test confirms it. MiniMax CN and Kimi Code CN rows
instead enable their sealed Chat Completions contract immediately; Responses
and Messages stay unsupported. Existing overrides and probe results for
surviving models are preserved. A failed or empty refresh keeps the previous
snapshot.

Custom API continues to use account-owned public-name → upstream-ID mappings;
discovery never silently replaces them. The account form **Fetch models** action
is an unsaved-form helper that returns upstream IDs only. Selecting one imports
an exact `public name = upstream ID` row, without suffix stripping or generated
Aliases. Command Code uses its public official `/models` directory: the GOAT
preset starts enabled, while additional models discovered later start disabled
until you enable their supported protocol in the matrix. It has no separate
Max or account-level GOAT/All mode.

Local catalogs feed resolution without another request-time upstream call.
Built-in Alias authority is static and code-owned: the original OpenCode Go
table supplies Go names, while sealed MiniMax CN, Kimi CN, and selected GOAT
long-name maps supply provider aliases without creating Go routes. Command
removes the Provider namespace and reuses an existing code-owned Alias; known
plan suffixes are removed only when the shorter name is already authorized.
For example, `nvidia/nemotron-3-ultra-550b-a55b` uses Alias
`nemotron-3-ultra`. Saved CN rows activate only their exact sealed map.
Unmatched built-in rows remain exact raw model IDs and are not advertised as
new Aliases; CN mappings keep the upstream ID's exact spelling. A Zen Free row
gains its suffix-stripped Alias only when that Alias is already Go-authorized;
the original `-free` ID remains an exact raw pin,
as described under
[Zen Free models](routing.md#zen-free-models).

If every model/protocol cell for a Provider is off, that Provider contributes
no route. Authenticated downstream `GET /v1/models` publishes only routeable
public names. It omits raw-only identities and raw-name conflicts; an ambiguous
raw identity fails as `ambiguous_model_id` without an upstream request.

OpenCode Go and Zen Free rows have a **Test** button. It probes every protocol
for that model without asking for an account. For each protocol the provider
automatically tries its eligible accounts in saved routing order and stops at
the first success. A Popconfirm warns that these real minimal requests may
consume quota. Command Code GOAT, MiniMax CN, Kimi Code CN, and Custom endpoint
scopes do not show the Test button because their adapters do not expose this V3
protocol-probe operation. Models must belong to the current
provider catalog; all three requested protocol endpoints are then tested,
including for newly fetched models not yet in the static table. Each protocol
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
  `https://commandcode.ai/docs/plans/goat`; subscription price and time-window
  allowance cards are not shown. Each priced model's applied multiplier can be
  edited and saved. The saved provider revision prices later requests; missing
  or ambiguous rows stay unpriced. A refresh asks before replacing edited
  multipliers. This remains separate from OpenCode Go. GOAT account cards use
  those priced OCG request logs for a local `$14 / $35 / $70` window estimate
  with manual baseline correction. It is deliberately not described as
  official usage because Command Code exposes no machine-readable usage API.
- Zen Free has no price (egress-IP-shared free quota).
- Custom API is unpriced: successful forwards log `cost_state=unknown` with
  no quota debit and no official usage refresh.
- MiniMax CN and Kimi Code CN are unpriced in OCG, but their account cards can
  manually read the official subscription windows (`/token_plan/remains` and
  `/usages`). These snapshots are display-only, never auto-polled, and never
  gate inference.

There is no model-level quota pool.

Client requests never probe: at request time the gateway never discovers or
probes. Flow: Alias → account
eligibility → adapter ceiling → saved contract → per-model/per-protocol
effective state → passthrough or conversion. Authenticated `GET /v1/models` and
protected `GET /dashboard/api/v3/application-models` publish only currently
routable public names that have an effective enabled protocol. The Applications picker
stays Go aliases ∩ active pricing and does not include Custom.

---

[User guide index](../USER.md) · [简体中文](providers.zh-CN.md) · [Docs index](../README.md)
