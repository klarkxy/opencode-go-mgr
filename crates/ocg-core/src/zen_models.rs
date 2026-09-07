//! OpenCode Zen Free model-catalog refresh and normalization.

use crate::models::AppConfig;
use chrono::Utc;
use futures_util::StreamExt;
use std::time::Duration;

pub use crate::kernel::zen::{
    ZEN_MODELS_SOURCE_URL, ZenFreeModelCatalog, ZenFreeModelView, model_views, parse_catalog,
    stripped_free_alias,
};

const MAX_BODY_BYTES: usize = 512 * 1024;
const REFRESH_TIMEOUT_SECS: u64 = 30;

pub async fn fetch_catalog(config: &AppConfig) -> Result<ZenFreeModelCatalog, String> {
    let client = crate::http_client::build_no_redirect(config)
        .map_err(|error| format!("failed to build Zen model catalog client: {error}"))?;
    fetch_catalog_at(config, client, ZEN_MODELS_SOURCE_URL).await
}

async fn fetch_catalog_at(
    config: &AppConfig,
    client: reqwest::Client,
    source_url: &str,
) -> Result<ZenFreeModelCatalog, String> {
    let timeout = Duration::from_secs(
        config
            .non_stream_timeout_secs
            .clamp(5, REFRESH_TIMEOUT_SECS),
    );
    let response = tokio::time::timeout(
        Duration::from_secs(REFRESH_TIMEOUT_SECS),
        client
            .get(source_url)
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(timeout)
            .send(),
    )
    .await
    .map_err(|_| "Zen model catalog refresh timed out".to_string())?
    .map_err(|error| format!("Zen model catalog request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Zen model catalog upstream returned HTTP {}",
            status.as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BODY_BYTES as u64)
    {
        return Err("Zen model catalog response is too large".to_string());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("Zen model catalog body failed: {error}"))?;
        if body.len().saturating_add(chunk.len()) > MAX_BODY_BYTES {
            return Err("Zen model catalog response is too large".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(ZenFreeModelCatalog {
        models: parse_catalog(&body)?,
        refreshed_at: Some(Utc::now()),
        source_url: source_url.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, http::HeaderMap, routing::get};

    #[tokio::test]
    async fn fetch_is_keyless_and_filters_the_upstream_catalog() {
        let app = Router::new().route(
            "/models",
            get(|headers: HeaderMap| async move {
                assert!(headers.get(reqwest::header::AUTHORIZATION).is_none());
                assert!(headers.get("x-api-key").is_none());
                axum::Json(serde_json::json!({
                    "object": "list",
                    "data": [
                        {"id": "paid-model"},
                        {"id": "new-coder-free"},
                        {"id": "big-pickle"}
                    ]
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let config = AppConfig {
            proxy_mode: crate::models::ProxyMode::Direct,
            ..AppConfig::default()
        };
        let client = crate::http_client::build_no_redirect(&config).unwrap();
        let catalog = fetch_catalog_at(&config, client, &format!("http://{addr}/models"))
            .await
            .unwrap();
        assert_eq!(catalog.models, ["new-coder-free"]);
        assert_eq!(model_views(&catalog)[0].alias, "new-coder");
    }
}
