pub mod cost;
pub mod forwarder;
pub mod handler;
pub mod limit;
pub mod protocol;
pub mod protocol_stream;
pub mod selector;

use crate::state::{CoreState, GatewayHandle};
use anyhow::Result;
use axum::Router;
use axum::routing::{get, post};
use std::net::SocketAddr;
use tokio::sync::oneshot;
use tower_http::cors::{Any, CorsLayer};

pub fn build_router(state: CoreState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let gateway_api = Router::new()
        .route("/v1/chat/completions", post(handler::chat_completions))
        .route("/v1/responses", post(handler::responses))
        .route("/v1/messages", post(handler::messages))
        .route("/v1/models", get(handler::models))
        .layer(cors);

    Router::new()
        .merge(gateway_api)
        .nest(
            "/dashboard/api",
            crate::dashboard::api_router(state.clone()),
        )
        .route("/dashboard", get(crate::dashboard::serve_index))
        .route("/dashboard/", get(crate::dashboard::serve_index))
        .route(
            "/dashboard/assets/{*path}",
            get(crate::dashboard::serve_asset),
        )
        .with_state(state)
}

pub async fn start_gateway(state: CoreState, port: u16) -> Result<GatewayHandle> {
    start_gateway_on(state, SocketAddr::from(([127, 0, 0, 1], port))).await
}

pub async fn start_gateway_on(state: CoreState, addr: SocketAddr) -> Result<GatewayHandle> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    state.set_dashboard_local_mode(local_addr.ip().is_loopback());
    let app = build_router(state);
    let port = local_addr.port();

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        let server = axum::serve(listener, app);
        let server = server.with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        if let Err(e) = server.await {
            eprintln!("gateway server error: {}", e);
        }
    });

    Ok(GatewayHandle {
        port,
        shutdown: shutdown_tx,
        task: handle,
    })
}

pub fn stop_gateway(handle: GatewayHandle) {
    let _ = handle.shutdown.send(());
    // ponytail: don't block_on the JoinHandle — stop_gateway is called from
    // tokio runtime contexts and ExitRequested handlers.
    // The spawned task will exit when the graceful-shutdown future resolves.
    // If blocking is needed later, spawn the wait on a dedicated std::thread.
}
