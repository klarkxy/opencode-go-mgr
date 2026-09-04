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
