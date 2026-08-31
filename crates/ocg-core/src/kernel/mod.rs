//! I/O-free Stage 1 kernels: identities, protocol catalogs, pricing types,
//! and Zen catalog parse/normalize.
//!
//! `catalog`, `ids`, `protocol`, and `zen` live in `ocg_domain` and are
//! re-exported here through explicit facade modules so
//! `ocg_core::kernel::{catalog,ids,protocol,zen}` keep compiling without
//! widening crate-private items. `pricing` stays in this crate so
//! [`pricing::PricingSnapshot`] retains inherent `estimate`/`estimate_at`
//! methods in the host pricing module. The provider catalog lives in
//! `ocg_domain::provider` and is re-exported item-by-item from
//! `crate::provider`, not this kernel module. The account aggregate lives
//! in `ocg_domain::account` and is re-exported item-by-item from
//! `crate::models`, not this kernel module.
//!
//! Domain sources must not import db, state, dashboard, gateway execution,
//! reqwest, rusqlite, tokio, axum, ocg-core, filesystem, clocks, or
//! process/host code.

pub mod catalog;
pub mod ids;
pub mod pricing;
pub mod protocol;
pub mod zen;
