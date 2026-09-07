use super::*;
use crate::crypto::StaticKeyCipher;
use crate::db::Database;
use crate::state::CoreStateInner;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn device_rejects_other_providers_and_stale_requests_without_side_effects() {
    let dir = std::env::temp_dir().join(format!("ocg-device-api-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let state = Arc::new(
        CoreStateInner::new(
            Database::open(dir.clone()).unwrap(),
            dir.clone(),
            Arc::new(StaticKeyCipher::new("device-test")),
        )
        .unwrap(),
    );
    let revision = state.settings_revision();
    let request = |provider: &str, revision: u64| {
        Bytes::from(
            serde_json::to_vec(&json!({
                "provider": provider, "method": "device", "expectedRevision": revision,
                "processGeneration": state.process_generation(),
            }))
            .unwrap(),
        )
    };
    assert!(
        start_oauth(State(state.clone()), request("anthropic", revision))
            .await
            .is_err()
    );
    let stale = start_oauth(State(state.clone()), request("codex", revision + 1))
        .await
        .unwrap_err();
    assert_eq!(stale.body.code, super::super::ERROR_REVISION_CONFLICT);
    // No managed Host/installation: do not dispatch an arbitrary external login.
    assert!(
        start_oauth(State(state.clone()), request("codex", revision))
            .await
            .is_err()
    );
    assert_eq!(state.settings_revision(), revision);
    // Unknown local session must be rejected locally, without needing saved CPA secrets.
    assert!(
        oauth_status(
            State(state.clone()),
            Query(OAuthStatusQuery {
                state: "ocg-device-unknown".into()
            })
        )
        .await
        .is_err()
    );
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}
