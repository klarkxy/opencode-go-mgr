[简体中文](USER.zh-CN.md)

# User Guide

This guide is for people running OCG Manager as a desktop app, a headless gateway, or a Docker service. Chapters follow the order you will actually meet them: install first, troubleshoot later.

## Add integrations

- [Add a Provider](user/add-provider.md) — Create a user-defined Provider, connect Custom API, or contribute a sealed built-in Provider with its complete HTTP and routing contract.
- [Add an Application](user/add-application.md) — Connect an unlisted client to the Gateway, contribute an Applications guide, or add an optional local Desktop connector.

## Chapters

- [What OCG Manager Does](user/overview.md) — Product positioning and the four jobs the gateway performs.
- [Architecture Diagrams](user/architecture.md) — Text maps of one node, a client request, Plans, and the dashboard.
- [Install And First Run](user/install.md) — Windows, macOS, and Linux installers; the SmartScreen ritual included.
- [Connect Your First Client](user/first-client.md) — Copy the Key and base URL, then prove it with one request.
- [Upgrade, Backup, Restore, And Uninstall](user/upgrade-backup.md) — Updater channel, manual upgrade, backup, restore, and uninstall.
- [The Dashboard](user/dashboard.md) — The seven core views, Extensions group, i18n, and Connection Center.
- [Application Guides And Model Capabilities](user/applications.md) — Client tutorials and the model capability table.
- [Accounts](user/accounts.md) — Plans, credentials, ordering, quota behavior, and managed onboarding.
- [Providers](user/providers.md) — Catalog, provider contracts, per-model protocol overrides, and probes.
- [Logs And Settings](user/logs-settings.md) — Request logs, settings, proxy modes, and theme.
- [Gateway Behavior](user/gateway.md) — Endpoints, authentication, aliases, Zen Free, and circuit breakers.
- [Protocol Conversion](user/protocol-conversion.md) — Preferred/supported protocols, passthrough, and conversion limits.
- [Routing, Cost, And Failover](user/routing.md) — Selection order, sticky/round-robin, cost accounting, and failover.
- [CLI](user/cli.md) — Headless CLI archive, data directory, and `serve` / `key` / `status`.
- [Docker](user/docker.md) — GHCR image, Compose setup, browser sidecar, and source builds.
- [External Integrations](user/external-integrations.md) — Local CPA setup, ownership boundaries, routing pool, and disconnect behavior.
- [Data And Security](user/data-security.md) — Data locations, credential storage, and encryption boundaries.
- [Limits](user/limits.md) — What is not implemented, on purpose or otherwise.
- [Troubleshooting](user/troubleshooting.md) — Common first-run, auth, routing, and log problems.

## Reading paths

- **New user** — `overview` → `architecture` → `install` → `first-client` → `accounts` → `providers` → `gateway` → `applications` → `troubleshooting`.
- **Docker / CLI operator** — `overview` → `architecture` → `docker` → `external-integrations` → `cli` → `accounts` → `providers` → `routing` → `logs-settings` → `troubleshooting`.
- **Integration author** — `add-provider` for an upstream; `add-application` for a downstream client.

---

[Docs index](README.md) · [简体中文](USER.zh-CN.md)
