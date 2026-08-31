//! Compatibility facade for [`ocg_gateway::attempt`].
//!
//! Crate-private items match the historical `ocg_core::gateway::attempt`
//! surface. The public module path is unchanged; item visibility is not
//! widened. Do not glob-reexport or reexport the module itself.

pub(crate) use ocg_gateway::attempt::{
    AttemptSpec, AttemptTimeouts, AttemptTransportError, CredentialHandle, CredentialResolveError,
    CredentialResolver, ProxyRoutingModel, TransportFailureKind, TransportSendFailure,
    UpstreamAuth,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    #[test]
    fn facade_reexports_gateway_attempt_types_without_widening() {
        assert_eq!(
            TypeId::of::<AttemptSpec>(),
            TypeId::of::<ocg_gateway::attempt::AttemptSpec>()
        );
        assert_eq!(
            TypeId::of::<AttemptTimeouts>(),
            TypeId::of::<ocg_gateway::attempt::AttemptTimeouts>()
        );
        assert_eq!(
            TypeId::of::<AttemptTransportError>(),
            TypeId::of::<ocg_gateway::attempt::AttemptTransportError>()
        );
        assert_eq!(
            TypeId::of::<CredentialHandle>(),
            TypeId::of::<ocg_gateway::attempt::CredentialHandle>()
        );
        assert_eq!(
            TypeId::of::<CredentialResolveError>(),
            TypeId::of::<ocg_gateway::attempt::CredentialResolveError>()
        );
        assert_eq!(
            TypeId::of::<ProxyRoutingModel>(),
            TypeId::of::<ocg_gateway::attempt::ProxyRoutingModel>()
        );
        assert_eq!(
            TypeId::of::<TransportFailureKind>(),
            TypeId::of::<ocg_gateway::attempt::TransportFailureKind>()
        );
        assert_eq!(
            TypeId::of::<TransportSendFailure>(),
            TypeId::of::<ocg_gateway::attempt::TransportSendFailure>()
        );
        assert_eq!(
            TypeId::of::<UpstreamAuth>(),
            TypeId::of::<ocg_gateway::attempt::UpstreamAuth>()
        );
        assert_eq!(
            TypeId::of::<dyn CredentialResolver>(),
            TypeId::of::<dyn ocg_gateway::attempt::CredentialResolver>()
        );
    }
}
