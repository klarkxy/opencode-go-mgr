[简体中文](extending.zh-CN.md)

# Extending OCG Manager

Use one of three explicit extension paths. They are intentionally different;
do not make a new surface look like a Provider merely to reuse a label.

## 1. Provider or Plan: sealed and static

Use this only for an OCG-owned upstream family with a complete routing,
catalog, protocol, key, and failure contract.

1. Add identities and catalog facts in `ocg-domain` (`ids.rs`, `provider.rs`),
   declare one unique static `contract_scope_id` for every Provider/Offering
   contract, and extend `ProviderAdapterKind` exhaustively. Existing scope ids
   are compatibility identities; never reuse or derive them at runtime. Keep
   Custom as `ConfigurableHttp`, not a superclass.
2. Add required protocol rows in `ocg-domain::protocol` and Alias mappings in
   `ocg-gateway::alias`. Never probe protocols on the request path.
3. Implement `resolve_route` in `ocg-core` so it returns an `AttemptSpec`
   only. Adapters cannot own DB, `CoreState`, or a raw reqwest client.
4. Fail closed until control-plane and routing semantics exist, then test the
   domain, gateway, and core boundaries.

The Provider registry remains static and sealed. There is no plugin loader,
dynamic library, user script, or runtime-discovered adapter.
Multiple Offerings may share a Provider identity, but each owns a distinct
contract scope, catalog, evidence, and override state.

## 2. Application connector: local Desktop capability

Use this for a client-side configuration or package integration. Follow the
application-guide/connector boundary: it is process-owned by the Desktop host,
uses documented field ownership, and does not add a service, daemon, remote
sync path, or Provider registry entry.

## 3. External integration: static local-service adapter

Use this for a product-approved service the user deploys locally. It appears in
the static **External Integrations** navigation group below Settings, not in
Providers, Plans, or the Add Account selector.

- Define a narrow typed Dashboard V3 contract and CAS-protected mutations; do
  not add a raw management proxy or arbitrary upstream path/body forwarding.
- Make the ownership boundary explicit. OCG may retain only what it needs to
  connect and route; the external service retains its own OAuth tokens, auth
  files, browser callbacks, internal scheduler, and lifecycle.
- Keep the service local: loopback for Desktop/CLI, or an explicit private
  Compose sibling. Do not create LAN, Internet, cross-node, process-control,
  auto-upgrade, registry, or generic SDK surfaces.
- Reuse OCG's ordering/selection/logging conventions only where the product
  contract calls for it. Do not invent internal accounts, costs, or quotas the
  external service does not expose.

CPA is the first instance of this path. Do not extract a general framework
until a second approved integration proves a shared requirement.

## Dashboard V3 endpoint changes

1. Add or extend DTOs in `dashboard_v3/types.rs` and append new names to
   `CATALOG_TYPE_NAMES`. Do not change existing `$defs` objects.
2. Mount routes in `dashboard_v3/mod.rs`; mutations use
   `parse_mutation_json` and `check_expectation` and preserve secret redaction.
3. Prefer existing persistence/control helpers and keep `dashboard_v3`
   independent of `gateway`.
4. Add a focused integration test, update `src/api/dashboard-v3.ts`, and run
   `pnpm run contract:v3:check`. Retired `/dashboard/api` REST stays retired.

---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](extending.zh-CN.md) · [Docs index](../README.md)
