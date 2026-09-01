pub mod attempt;
pub mod classify;
pub mod diagnostics;
pub mod executor;
pub mod forwarder;
pub mod free_models;
pub mod handler;
pub mod limit;
pub mod listener;
pub mod materialize;
pub mod protocol;
pub mod protocol_stream;
pub mod provider_adapter;
mod response;
pub mod routing;
pub mod selector;
pub mod wire;

use crate::state::CoreState;

pub use crate::gateway_runtime::GatewayHandle;
use anyhow::Result;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::HeaderName;
use axum::middleware;
use axum::routing::{get, post};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

pub use listener::GatewayLifecycle;
pub use listener::ListenerStopOutcome;

// 1M-token conversations exceed Axum's 2 MiB Bytes default; keep a bounded cap before auth.
const MAX_GATEWAY_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;
const _: () = assert!(MAX_GATEWAY_REQUEST_BODY_BYTES > 2 * 1024 * 1024);

pub(crate) fn inference_router(state: CoreState) -> Router<CoreState> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers([HeaderName::from_static(diagnostics::REQUEST_ID_HEADER)]);

    Router::new()
        .route("/v1/chat/completions", post(handler::chat_completions))
        .route("/v1/responses", post(handler::responses))
        .route("/v1/messages", post(handler::messages))
        .route("/v1/models", get(handler::models))
        .route(
            "/claude-desktop/v1/messages",
            post(handler::claude_desktop_messages),
        )
        .route(
            "/claude-desktop/v1/models",
            get(handler::claude_desktop_models),
        )
        .route(
            "/v1beta/models/{*model_action}",
            post(handler::gemini_model_action),
        )
        .route(
            "/v1/models/{*model_action}",
            post(handler::gemini_model_action),
        )
        .layer(cors)
        .layer(DefaultBodyLimit::max(MAX_GATEWAY_REQUEST_BODY_BYTES))
        .layer(middleware::from_fn_with_state(
            state,
            handler::request_trace_middleware,
        ))
}

pub async fn start_gateway(state: CoreState, port: u16) -> Result<GatewayHandle> {
    start_gateway_on(state, SocketAddr::from(([127, 0, 0, 1], port))).await
}

pub async fn start_gateway_on(state: CoreState, addr: SocketAddr) -> Result<GatewayHandle> {
    crate::usage_sync::ControlPlaneWorkers::ensure_started(state.clone());
    listener::GatewayLifecycle::bind(state, addr).await
}

pub fn stop_gateway(handle: GatewayHandle) {
    listener::GatewayLifecycle::stop(handle);
}

pub async fn stop_gateway_and_wait(handle: GatewayHandle) -> ListenerStopOutcome {
    listener::GatewayLifecycle::stop_and_wait(handle).await
}

/// Listener-only rebind of `state.gateway`. Does not start or cancel
/// process-level control-plane workers.
pub async fn rebind_gateway(state: CoreState, addr: SocketAddr) -> Result<u16> {
    listener::GatewayLifecycle::rebind(state, addr).await
}

#[cfg(test)]
mod tests;
