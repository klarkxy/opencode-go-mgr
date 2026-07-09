//! Remote sync glue. Two responsibilities:
//!   1. `startup_pull` — fired once on GUI boot. Pulls every key from the
//!      remote admin API; per-row LWW by `updated_at` (local wins on tie).
//!   2. `push_one`    — fire-and-forget POST/DELETE fired after each local
//!      mutation. No retry queue, no backoff. Failures are warn-logged.
//!
//! ponytail: spawn + drop JoinHandle. If we ever need backpressure or error
//! aggregation, swap to a bounded mpsc + a single drain task — do not grow
//! these functions.

use crate::state::AppState;
use ocg_core::models::Account;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug)]
pub enum PushOp {
    Create,
    Update,
    Delete,
}

#[derive(Deserialize)]
struct RemoteKeyDto {
    id: String,
    name: String,
    key: Option<String>,
    key_cipher: Option<String>,
    enabled: bool,
    referral_code: Option<String>,
    recharge_date: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
struct PushPayload<'a> {
    id: &'a str,
    name: &'a str,
    key: String,
    key_cipher: &'a str,
    enabled: bool,
    referral_code: Option<&'a str>,
    recharge_date: Option<&'a str>,
    created_at: String,
    updated_at: String,
}

/// Pull all keys from the configured remote admin API and merge them into
/// the local database using last-write-wins by `updated_at`. Local entries
/// equal-or-newer than the remote copy are preserved. Returns the number of
/// rows actually merged.
pub async fn startup_pull(state: AppState) -> usize {
    let cfg = state.core.config();
    if cfg.remote.url.is_empty() {
        return 0;
    }
    let base = cfg.remote.url.trim_end_matches('/').to_string();
    if !is_safe_remote_base(&base) {
        let _ = state.core.db.lock().log_gateway(
            "warn",
            "remote_sync",
            "startup pull skipped unsafe remote url",
        );
        return 0;
    }
    let token = cfg.remote.token.clone();
    let url = format!("{}/admin/keys", base);

    let resp = match state
        .core
        .http_client
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = state.core.db.lock().log_gateway(
                "warn",
                "remote_sync",
                &format!("startup pull failed: {}", e),
            );
            return 0;
        }
    };
    if !resp.status().is_success() {
        let _ = state.core.db.lock().log_gateway(
            "warn",
            "remote_sync",
            &format!("startup pull status {}", resp.status()),
        );
        return 0;
    }
    let remote: Vec<RemoteKeyDto> = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            let _ = state.core.db.lock().log_gateway(
                "warn",
                "remote_sync",
                &format!("startup pull decode: {}", e),
            );
            return 0;
        }
    };

    // ponytail: capture the total BEFORE consuming the Vec so the log line
    // can report a real denominator (the old version hardcoded 0).
    let total = remote.len();
    let mut merged = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;
    // ponytail: one db lock for the loop. parking_lot::Mutex serializes
    // regardless, and per-row lock+unlock would also contend with the
    // gateway's hot path. Single scope = simpler + faster.
    let db = state.core.db.lock();
    for r in remote {
        // ponytail: distinguish "row not present" (Ok(None)) from a real
        // lookup error. The old code used `.ok().flatten()` which silently
        // turned Err into None and then ran the create path — re-creating a
        // row the user had just deleted.
        let local = match db.get_account(&r.id) {
            Ok(opt) => opt,
            Err(e) => {
                let _ = db.log_gateway(
                    "warn",
                    "remote_sync",
                    &format!("startup pull lookup {} failed: {}", r.id, e),
                );
                errors += 1;
                continue;
            }
        };
        let wire_updated_at = parse_rfc3339(&r.updated_at).unwrap_or_else(chrono::Utc::now);
        let local_newer = local
            .as_ref()
            .map(|l| l.updated_at >= wire_updated_at)
            .unwrap_or(false);
        if local_newer {
            skipped += 1;
            continue; // 本地为准
        }
        let created_at = parse_rfc3339(&r.created_at).unwrap_or_else(chrono::Utc::now);
        let key_cipher = match r.key.as_deref() {
            Some(key) => match state.core.encrypt_key(key) {
                Ok(cipher) => cipher,
                Err(e) => {
                    let _ = db.log_gateway(
                        "warn",
                        "remote_sync",
                        &format!("startup pull encrypt {} failed: {}", r.id, e),
                    );
                    errors += 1;
                    continue;
                }
            },
            None => r.key_cipher.clone().unwrap_or_default(),
        };
        let account = Account {
            id: r.id.clone(),
            name: r.name.clone(),
            key_cipher,
            enabled: r.enabled,
            referral_code: r.referral_code.clone(),
            recharge_date: r.recharge_date.clone(),
            cooldown_until: None,
            last_error: None,
            created_at,
            updated_at: wire_updated_at,
        };
        // ponytail: use the LWW helper that respects the wire's updated_at
        // AND refuses to overwrite the local cipher. Avoids both the
        // "LWW-by-arrival" bug (db.update_account bumps server clock) and
        // the cross-machine cipher poisoning bug.
        let result = db.merge_account_from_remote(&account.id, &account, wire_updated_at);
        match result {
            Ok(true) => merged += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                let _ = db.log_gateway(
                    "warn",
                    "remote_sync",
                    &format!("startup pull merge {} failed: {}", r.id, e),
                );
                errors += 1;
            }
        }
    }
    let _ = db.log_gateway(
        "info",
        "remote_sync",
        &format!(
            "startup pull merged {} skipped {} errors {} of {}",
            merged, skipped, errors, total
        ),
    );
    merged
}

/// Fire-and-forget push one account to the configured remote. Called after
/// any local mutation (create/update/delete). Failures are warn-logged;
/// there is no retry — the next local mutation will push again.
pub fn push_one(state: AppState, account: Option<Account>, op: PushOp) {
    tauri::async_runtime::spawn(async move {
        let cfg = state.core.config();
        if cfg.remote.url.is_empty() {
            return;
        }
        let base = cfg.remote.url.trim_end_matches('/');
        if !is_safe_remote_base(base) {
            let _ = state.core.db.lock().log_gateway(
                "warn",
                "remote_sync",
                "push skipped unsafe remote url",
            );
            return;
        }
        let token = cfg.remote.token.clone();

        // Ponytail: read snapshot under the config lock; do not hold the lock
        // across the await below.
        let (url, method, body) = match op {
            PushOp::Delete => {
                let id = match account.as_ref() {
                    Some(a) => a.id.clone(),
                    None => return,
                };
                (format!("{}/admin/keys/{}", base, id), "DELETE", None)
            }
            PushOp::Create | PushOp::Update => {
                let a = match account.as_ref() {
                    Some(a) => a,
                    None => return,
                };
                let key = match state.core.decrypt_key(&a.key_cipher) {
                    Ok(key) => key,
                    Err(e) => {
                        let _ = state.core.db.lock().log_gateway(
                            "warn",
                            "remote_sync",
                            &format!("push decrypt {} failed: {}", a.id, e),
                        );
                        return;
                    }
                };
                let payload = PushPayload {
                    id: &a.id,
                    name: &a.name,
                    key,
                    key_cipher: &a.key_cipher,
                    enabled: a.enabled,
                    referral_code: a.referral_code.as_deref(),
                    recharge_date: a.recharge_date.as_deref(),
                    created_at: a.created_at.to_rfc3339(),
                    updated_at: a.updated_at.to_rfc3339(),
                };
                let json = match serde_json::to_string(&payload) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                (format!("{}/admin/keys", base), "POST", Some(json))
            }
        };

        let req = match method {
            "DELETE" => state.core.http_client.delete(&url),
            _ => state.core.http_client.post(&url),
        };
        let req = req.bearer_auth(&token);
        let req = match body {
            Some(b) => req.header("content-type", "application/json").body(b),
            None => req,
        };
        let result = req.send().await;
        let line = match &result {
            Ok(r) if r.status().is_success() => {
                format!("push {:?} ok {}", op, r.status())
            }
            Ok(r) => format!("push {:?} status {}", op, r.status()),
            Err(e) => format!("push {:?} error: {}", op, e),
        };
        let _ = state
            .core
            .db
            .lock()
            .log_gateway("warn", "remote_sync", &line);
    });
}

fn parse_rfc3339(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
}

fn is_safe_remote_base(base: &str) -> bool {
    let Ok(url) = tauri::Url::parse(base) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" => matches!(
            url.host_str(),
            Some("localhost") | Some("127.0.0.1") | Some("::1") | Some("[::1]")
        ),
        _ => false,
    }
}
