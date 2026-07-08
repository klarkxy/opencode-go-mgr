pub mod circuit_breaker;
pub mod cost;
pub mod forwarder;
pub mod handler;
pub mod selector;

use crate::state::{AppState, GatewayHandle};
use anyhow::Result;
use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use tokio::sync::oneshot;
use tower_http::cors::{Any, CorsLayer};

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/v1/chat/completions", post(handler::chat_completions))
        .route("/v1/messages", post(handler::messages))
        .route("/v1/models", get(handler::models))
        .layer(cors)
        .with_state(state)
}

pub async fn start_gateway(state: AppState, port: u16) -> Result<GatewayHandle> {
    let app = build_router(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

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
    // Tauri commands (on the tokio runtime) and ExitRequested handler.
    // The spawned task will exit when the graceful-shutdown future resolves.
    // If blocking is needed later, spawn the wait on a dedicated std::thread.
}
