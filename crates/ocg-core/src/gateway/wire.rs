//! Compatibility facade for [`ocg_gateway::wire`].
//!
//! Keeps the host's wire-normalization paths crate-private while the pure
//! implementation stays in the I/O-free gateway crate.

#[doc(inline)]
pub(crate) use ocg_gateway::wire::WireNormalization;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facade_reexports_gateway_wire_without_owning_it() {
        assert_eq!(
            std::any::TypeId::of::<WireNormalization>(),
            std::any::TypeId::of::<ocg_gateway::wire::WireNormalization>()
        );
        assert_eq!(ocg_gateway::wire::OLLAMA_CLOUD_MAX_TOKENS_LIMIT, 65_535);
    }
}
