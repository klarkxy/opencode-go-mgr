//! Compatibility facade for [`ocg_gateway::wire`].
//!
//! Keeps the host's wire-normalization paths crate-private while the pure
//! implementation stays in the I/O-free gateway crate.

#[doc(inline)]
pub(crate) use ocg_gateway::wire::WireNormalization;
