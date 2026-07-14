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

#[cfg(test)]
mod tests {
    use super::start_gateway_on;
    use crate::crypto::{KeyCipher, StaticKeyCipher};
    use crate::db::Database;
    use crate::state::CoreStateInner;
    use axum::http::StatusCode;
    use serde_json::json;
    use std::fs;
    use std::net::SocketAddr;
    use std::sync::Arc;

    #[tokio::test]
    async fn claude_desktop_routes_are_wired_and_protected() {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be valid")
            .as_nanos();
        dir.push(format!("ocg-claude-desktop-routes-{nanos}"));
        fs::create_dir_all(&dir).expect("test data directory should be created");
        let db = Database::open(dir.clone()).expect("test database should open");
        let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("test"));
        let state =
            Arc::new(CoreStateInner::new(db, dir.clone(), cipher).expect("state should load"));
        let mut config = state.config();
        config.gateway_key = "gateway-test-key".to_string();
        state.set_config(config).expect("test config should save");
        let handle = start_gateway_on(state.clone(), SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("test gateway should start");
        let root = format!("http://127.0.0.1:{}", handle.port);
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("test client should build");

        let gemini_body = json!({
            "contents":[{"role":"user","parts":[{"text":"hello"}]}]
        });
        let gemini_unauthorized = client
            .post(format!("{root}/v1beta/models/minimax-m3:generateContent"))
            .json(&gemini_body)
            .send()
            .await
            .expect("Gemini unauthorized request should complete");
        assert_eq!(gemini_unauthorized.status(), StatusCode::UNAUTHORIZED);

        for path in [
            "/v1beta/models/minimax-m3:generateContent",
            "/v1/models/minimax-m3:streamGenerateContent?alt=sse",
        ] {
            let response = client
                .post(format!("{root}{path}"))
                .header("x-goog-api-key", "gateway-test-key")
                .json(&gemini_body)
                .send()
                .await
                .expect("authorized Gemini generation route should complete");
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        }
        for path in [
            "/v1beta/models/minimax-m3:countTokens",
            "/v1/models/minimax-m3:embedContent",
        ] {
            let response = client
                .post(format!("{root}{path}"))
                .header("x-goog-api-key", "gateway-test-key")
                .json(&gemini_body)
                .send()
                .await
                .expect("unsupported Gemini route should complete");
            assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
            let body: serde_json::Value = response.json().await.expect("Google error JSON");
            assert_eq!(body["error"]["status"], "UNIMPLEMENTED");
        }
        let unknown_action = client
            .post(format!("{root}/v1beta/models/minimax-m3:unknownAction"))
            .header("x-goog-api-key", "gateway-test-key")
            .json(&gemini_body)
            .send()
            .await
            .expect("unknown Gemini action should complete");
        assert_eq!(unknown_action.status(), StatusCode::NOT_FOUND);

        let unauthorized = client
            .get(format!("{root}/claude-desktop/v1/models"))
            .send()
            .await
            .expect("models request should complete");
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let models = client
            .get(format!("{root}/claude-desktop/v1/models"))
            .header("x-api-key", "gateway-test-key")
            .send()
            .await
            .expect("authorized models request should complete");
        assert_eq!(models.status(), StatusCode::OK);
        let models: serde_json::Value =
            models.json().await.expect("models response should be JSON");
        assert_eq!(models["data"][0]["id"], "claude-sonnet-4-6");

        let messages = client
            .post(format!("{root}/claude-desktop/v1/messages"))
            .json(&json!({"model":"claude-sonnet-4-6","max_tokens":1,"messages":[]}))
            .send()
            .await
            .expect("messages request should complete");
        assert_eq!(messages.status(), StatusCode::UNAUTHORIZED);
        let unsupported = client
            .post(format!("{root}/claude-desktop/v1/messages"))
            .header("x-api-key", "gateway-test-key")
            .json(&json!({"model":"claude-unknown","max_tokens":1,"messages":[]}))
            .send()
            .await
            .expect("authorized messages request should complete");
        assert_eq!(unsupported.status(), StatusCode::BAD_REQUEST);

        let dashboard = client
            .get(format!("{root}/dashboard/api/claude-desktop/models"))
            .header("x-forwarded-for", "203.0.113.1")
            .send()
            .await
            .expect("dashboard request should complete");
        assert_eq!(dashboard.status(), StatusCode::UNAUTHORIZED);

        let invalid = client
            .put(format!("{root}/dashboard/api/claude-desktop/models"))
            .json(&json!({"sonnet":"","opus":"","haiku":""}))
            .send()
            .await
            .expect("dashboard update should complete");
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

        let updated = client
            .put(format!("{root}/dashboard/api/claude-desktop/models"))
            .json(&json!({"sonnet":"","opus":"glm-5.2","haiku":""}))
            .send()
            .await
            .expect("dashboard update should complete");
        assert_eq!(updated.status(), StatusCode::OK);
        let updated: serde_json::Value = updated.json().await.expect("update should return JSON");
        assert_eq!(
            updated,
            json!({"sonnet":"glm-5.2","opus":"glm-5.2","haiku":"glm-5.2"})
        );
        let fetched: serde_json::Value = client
            .get(format!("{root}/dashboard/api/claude-desktop/models"))
            .send()
            .await
            .expect("dashboard models request should complete")
            .json()
            .await
            .expect("dashboard models response should be JSON");
        assert_eq!(fetched, updated);

        let _ = handle.shutdown.send(());
        handle.task.await.expect("test gateway should stop");
        drop(state);
        fs::remove_dir_all(dir).expect("test data directory should be removed");
    }
}
