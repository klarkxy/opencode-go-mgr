# Locate the implementation for a specific maintenance area

| Change | Start here |
| --- | --- |
| Gateway, alias, protocol, key, proxy, usage, catalog | [runtime invariants](../../../../docs/maintainer/runtime-invariants.md), then the named crate source |
| Dashboard/API contract | [dashboard V3](../../../../crates/ocg-core/src/dashboard_v3/), [V3 schema](../../../../schema/dashboard-api-v3.schema.json), [V3 client](../../../../src/api/dashboard-v3.ts), [dashboard API](../../../../docs/maintainer/dashboard-api.md) |
| Vue UI | [views](../../../../src/views/), [components](../../../../src/components/), [domain](../../../../src/domain/), [DESIGN.md](../../../../DESIGN.md) |
| Desktop capability | [Desktop Host](../../../../src-tauri/src/host/), [Tauri startup](../../../../src-tauri/src/lib.rs), [host router](../../../../crates/ocg-core/src/host_router.rs) |
| Provider or Plan | [registry](../../../../crates/ocg-domain/src/provider.rs), [protocol table](../../../../crates/ocg-domain/src/protocol.rs), [alias resolver](../../../../crates/ocg-gateway/src/alias.rs), [dynamic definitions](../../../../crates/ocg-domain/src/dynamic.rs) |
| Application guide/connector | [guide registry](../../../../src/views/application-guides.ts), [Applications view](../../../../src/views/Applications.vue), [Desktop connector Host](../../../../src-tauri/src/host/application_connectors.rs), [native packages](../../../../integrations/), [user guide](../../../../docs/user/applications.md) |
| Static external integration | [runtime invariants](../../../../docs/maintainer/runtime-invariants.md), [extending](../../../../docs/maintainer/extending.md), [Dashboard V3](../../../../crates/ocg-core/src/dashboard_v3/) |
