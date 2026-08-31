use super::*;
use super::{V27MigrationFault, v27_test_hooks};
use crate::crypto::{KeyCipher, StaticKeyCipher};
use std::fs;
use std::sync::Arc;

const TEST_HOST_SECRET: &str = "ocg-db-v27-test-host";
const FIXTURE_ACCOUNT_PLAINTEXT: &str = "sk-fixture";

fn test_host_cipher() -> Arc<dyn KeyCipher + Send + Sync> {
    Arc::new(StaticKeyCipher::new(TEST_HOST_SECRET))
}

fn fixture_account_key_cipher() -> String {
    test_host_cipher()
        .encrypt(FIXTURE_ACCOUNT_PLAINTEXT)
        .expect("test host cipher should encrypt fixture account keys")
}

fn open_with_host_cipher(dir: PathBuf) -> Result<Database> {
    Database::open_with_cipher(dir, test_host_cipher())
}

fn assert_fixture_account_cipher(value: &str) {
    assert_eq!(
        test_host_cipher()
            .decrypt(value)
            .expect("fixture account cipher should decrypt with the test host"),
        FIXTURE_ACCOUNT_PLAINTEXT
    );
}

fn temp_data_dir(label: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    dir.push(format!("ocg-db-test-{label}-{nanos}"));
    fs::create_dir_all(&dir).expect("test data dir should be created");
    dir
}

fn create_v21_fixture(dir: &Path, include_reserved_account_conflict: bool) {
    let db = Database::open(dir.to_path_buf()).expect("fixture database should open");
    let mut rollback = account("rollback-account");
    rollback.key_cipher = fixture_account_key_cipher();
    db.create_account(&rollback)
        .expect("representative account should save");
    db.log_forward(&forward_log("rollback-account", "success", 4.25))
        .expect("representative forward log should save");
    if !include_reserved_account_conflict {
        db.conn
            .execute("DELETE FROM accounts WHERE id = ?1", [ZEN_FREE_ACCOUNT_ID])
            .expect("reserved v22 account should be removed from a normal v21 fixture");
    }
    drop(db);

    let conn = Connection::open(dir.join("data.sqlite")).expect("fixture db should reopen");
    conn.execute_batch(
        "PRAGMA foreign_keys=OFF;
             DROP TRIGGER IF EXISTS access_keys_protect_primary_delete;
             DROP TABLE IF EXISTS access_keys;
             CREATE TABLE IF NOT EXISTS sub_gateway_keys (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                key TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                deleted_at TEXT,
                created_at TEXT NOT NULL
             );
             CREATE UNIQUE INDEX IF NOT EXISTS idx_sub_gateway_keys_key
                ON sub_gateway_keys(key) WHERE deleted_at IS NULL AND key <> '';
             DROP INDEX IF EXISTS idx_forward_logs_route_account;
             DROP INDEX IF EXISTS idx_forward_logs_provider_offering;
             DROP INDEX IF EXISTS idx_account_model_capabilities_account;
             DROP TABLE IF EXISTS provider_usage_sync_state;
             DROP TABLE IF EXISTS provider_pricing_snapshots;
             DROP TABLE IF EXISTS credit_balances;
             DROP TABLE IF EXISTS quota_windows;
             DROP TABLE IF EXISTS account_custom_configs;
             DROP TABLE IF EXISTS account_model_capabilities;
             ALTER TABLE accounts DROP COLUMN verification_error;
             ALTER TABLE accounts DROP COLUMN connection_verified_at;
             ALTER TABLE accounts DROP COLUMN verification_status;
             ALTER TABLE forward_logs DROP COLUMN native_cost_currency;
             ALTER TABLE forward_logs DROP COLUMN native_cost_unit;
             ALTER TABLE forward_logs DROP COLUMN native_cost_value;
             ALTER TABLE forward_logs DROP COLUMN upstream_model;
             ALTER TABLE forward_logs DROP COLUMN resolved_alias;
             ALTER TABLE forward_logs DROP COLUMN requested_model;
             ALTER TABLE accounts DROP COLUMN free_alias_enabled;
             ALTER TABLE accounts DROP COLUMN quota_scope;
             ALTER TABLE accounts DROP COLUMN credential_kind;
             ALTER TABLE accounts DROP COLUMN offering_id;
             ALTER TABLE accounts DROP COLUMN provider_id;
             ALTER TABLE forward_logs DROP COLUMN effective_paid_cost_usd;
             ALTER TABLE forward_logs DROP COLUMN quota_debit;
             ALTER TABLE forward_logs DROP COLUMN raw_cost_usd;
             ALTER TABLE forward_logs DROP COLUMN credential_account_id;
             ALTER TABLE forward_logs DROP COLUMN offering_id;
             ALTER TABLE forward_logs DROP COLUMN provider_id;
             ALTER TABLE forward_logs DROP COLUMN route_account_id;
             DELETE FROM schema_version;
             INSERT INTO schema_version (version) VALUES (21);
             PRAGMA foreign_keys=ON;",
    )
    .expect("v21 fixture should be created");
    restore_usage_sync_account_columns(&conn);
}

fn restore_usage_sync_account_columns(conn: &Connection) {
    for (column, definition) in [
        ("usage_sync_last_success_at", "TEXT"),
        ("usage_sync_last_attempt_at", "TEXT"),
        ("usage_sync_next_eligible_at", "TEXT"),
        ("usage_sync_failure_streak", "INTEGER NOT NULL DEFAULT 0"),
        ("usage_sync_last_expedited_at", "TEXT"),
    ] {
        if !table_has_column(conn, "accounts", column).unwrap() {
            conn.execute(
                &format!("ALTER TABLE accounts ADD COLUMN {column} {definition}"),
                [],
            )
            .unwrap();
        }
    }
}

fn create_v20_fixture(dir: &Path, include_reserved_account_conflict: bool) {
    create_v21_fixture(dir, include_reserved_account_conflict);
    let conn = Connection::open(dir.join("data.sqlite")).expect("v21 fixture should reopen");
    for column in USAGE_SYNC_ACCOUNT_COLUMNS {
        if table_has_column(&conn, "accounts", column).unwrap() {
            conn.execute(&format!("ALTER TABLE accounts DROP COLUMN {column}"), [])
                .unwrap();
        }
    }
    conn.execute_batch(
        "DELETE FROM schema_version;
             INSERT INTO schema_version (version) VALUES (20);",
    )
    .expect("v20 fixture should be created");
}

fn pre_v22_backup_paths(dir: &Path) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(dir)
        .expect("fixture directory should be readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with(PRE_V22_BACKUP_FILE_PREFIX) && name.ends_with(".bak")
                })
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn backup_paths_with_prefix(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(dir)
        .expect("fixture directory should be readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(".bak"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn pre_v23_backup_paths(dir: &Path) -> Vec<PathBuf> {
    backup_paths_with_prefix(dir, PRE_V23_BACKUP_FILE_PREFIX)
}

fn pre_v3_backup_paths(dir: &Path) -> Vec<PathBuf> {
    backup_paths_with_prefix(dir, PRE_V3_BACKUP_FILE_PREFIX)
}

fn reverse_current_to_v26(dir: &Path) {
    let path = dir.join("data.sqlite");
    let conn = Connection::open(&path).expect("migrated database should reopen for reverse");
    let primary = conn
        .query_row(
            "SELECT key FROM access_keys WHERE is_primary = 1 LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_default();
    if let Some(json) = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'config'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .unwrap()
    {
        let mut value: serde_json::Value =
            serde_json::from_str(&json).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "gateway_key".to_string(),
                serde_json::Value::String(primary.clone()),
            );
        }
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('config', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [serde_json::to_string(&value).unwrap()],
        )
        .unwrap();
    } else if !primary.is_empty() {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('config', ?1)",
            [serde_json::json!({ "gateway_key": primary }).to_string()],
        )
        .unwrap();
    }
    conn.execute_batch(
        "
            PRAGMA foreign_keys=OFF;
            CREATE TABLE IF NOT EXISTS sub_gateway_keys (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                key TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                deleted_at TEXT,
                created_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_sub_gateway_keys_key
                ON sub_gateway_keys(key) WHERE deleted_at IS NULL AND key <> '';
            INSERT OR IGNORE INTO sub_gateway_keys (id, name, key, enabled, deleted_at, created_at)
                SELECT id, name, key, enabled, deleted_at, created_at
                FROM access_keys WHERE is_primary = 0;
            DROP TRIGGER IF EXISTS access_keys_protect_primary_delete;
            DROP TABLE IF EXISTS access_keys;
            ",
    )
    .expect("access_keys should reverse into sub_gateway_keys");
    restore_usage_sync_account_columns(&conn);
    if table_has_column(&conn, "accounts", "goat_model_access").unwrap() {
        conn.execute_batch("ALTER TABLE accounts DROP COLUMN goat_model_access;")
            .expect("v28 GOAT model access should reverse out of the v26 fixture");
    }
    conn.execute_batch(
        "DELETE FROM schema_version;
             INSERT INTO schema_version (version) VALUES (26);
             PRAGMA foreign_keys=ON;",
    )
    .expect("schema should reverse to v26");
    assert_eq!(schema_version_on(&conn).unwrap(), V26_SCHEMA_VERSION);
}

fn account(id: &str) -> Account {
    Account {
        id: id.into(),
        provider_id: default_provider_id(),
        offering_id: default_offering_id(),
        credential_kind: default_credential_kind(),
        quota_scope: default_quota_scope(),
        name: id.into(),
        username: None,
        password_cipher: None,
        key_cipher: "cipher".into(),
        enabled: true,
        account_type: AccountType::Key,
        setup_step: AccountSetupStep::Ready,
        referral_code: None,
        purchase_date: String::new(),
        expires_on: String::new(),
        cooldown_until: None,
        cooldown_generic_until: None,
        cooldown_5h_until: None,
        cooldown_week_until: None,
        cooldown_month_until: None,
        cooldown_free_until: None,
        last_error: None,
        auth_error: None,
        notes: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn persist_unroutable_draft(db: &Database, plan: BuiltinPlan, id: &str, notes: &str) {
    let mut draft = account(id);
    draft.provider_id = plan.offering.provider_id.to_string();
    draft.offering_id = plan.offering.offering_id.to_string();
    draft.credential_kind = plan.offering.credential_kind;
    draft.quota_scope = plan.offering.quota_scope;
    draft.enabled = false;
    draft.notes = Some(notes.to_string());
    if plan_requires_custom_config(plan) {
        db.create_account_with_contract(
            &draft,
            Some(&AccountCustomConfigInput {
                endpoint_url: "https://api.example.com/v1/chat/completions".into(),
                upstream_protocol: UpstreamProtocolKind::ChatCompletions,
            }),
            &[AccountModelCapabilityInput {
                public_model: "org/model".into(),
                upstream_model: "org/model".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: None,
            }],
        )
        .unwrap();
    } else {
        db.create_account_with_contract(&draft, None, &[]).unwrap();
    }
}

fn leftover_enable(db: &Database, id: &str) {
    let changed = db
        .conn
        .execute("UPDATE accounts SET enabled = 1 WHERE id = ?1", [id])
        .unwrap();
    assert_eq!(changed, 1, "{id}");
}

fn clone_account_row_as_enabled(
    conn: &Connection,
    source_id: &str,
    new_id: &str,
    provider_id: &str,
    offering_id: &str,
) {
    let mut stmt = conn.prepare("PRAGMA table_info(accounts)").unwrap();
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|column| column.unwrap())
        .collect();
    let select_list = columns
        .iter()
        .map(|column| match column.as_str() {
            "id" | "name" => "?1".to_string(),
            "provider_id" => "?2".to_string(),
            "offering_id" => "?3".to_string(),
            "enabled" => "1".to_string(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    conn.execute(
        &format!(
            "INSERT INTO accounts ({cols}) SELECT {select_list} FROM accounts WHERE id = ?4",
            cols = columns.join(", ")
        ),
        params![new_id, provider_id, offering_id, source_id],
    )
    .unwrap();
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SanitationSnapshot {
    enabled: bool,
    name: String,
    notes: Option<String>,
    updated_at: DateTime<Utc>,
    verification: ConnectionVerificationStatus,
    verification_error: Option<String>,
}

fn sanitation_snapshot(db: &Database, id: &str) -> SanitationSnapshot {
    let account = db.get_account(id).unwrap().expect(id);
    let verification = db.account_verification_state(id).unwrap().expect(id);
    SanitationSnapshot {
        enabled: account.enabled,
        name: account.name,
        notes: account.notes,
        updated_at: account.updated_at,
        verification: verification.status,
        verification_error: verification.verification_error,
    }
}

fn forward_log(account_id: &str, status: &str, cost: f64) -> ForwardLog {
    ForwardLog {
        id: 0,
        timestamp: Utc::now(),
        model: "test".into(),
        account_id: account_id.into(),
        account_name: account_id.into(),
        route_account_id: None,
        provider_id: None,
        offering_id: None,
        credential_account_id: None,
        client_key_id: None,
        client_key_name: None,
        status: status.into(),
        http_status: Some(200),
        route: String::new(),
        prompt_tokens: 0,
        completion_tokens: 0,
        cached_tokens: 0,
        cache_creation_tokens: 0,
        cost: Some(cost),
        raw_cost_usd: None,
        quota_debit: None,
        effective_paid_cost_usd: None,
        pricing_revision_id: None,
        quota_multiplier: None,
        local_adjustment_multiplier: None,
        service_tier: None,
        cost_state: "legacy_estimate".into(),
        error_message: None,
        request_id: None,
        attempt: None,
        error_source: None,
        error_stage: None,
        duration_ms: None,
        diagnostic: None,
    }
}

#[test]
fn v24_adds_route_column_and_historical_rows_stay_unlabeled() {
    let dir = temp_data_dir("v24-route-column");
    let db = Database::open(dir.clone()).unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);

    // A row written before the column existed keeps the empty default
    // ("not recorded") — insert it without naming the route column.
    db.conn
        .execute(
            "INSERT INTO forward_logs
                 (timestamp, model, account_id, account_name, status, cost_state)
                 VALUES ('2026-01-01T00:00:00Z', 'glm-5.3', 'a1', 'a1', 'success',
                         'legacy_estimate')",
            [],
        )
        .unwrap();

    let mut modern = forward_log("a1", "success", 0.5);
    modern.route = "proxy".to_string();
    modern.model = "gpt-5.6-luna".to_string();
    db.log_forward(&modern).unwrap();

    let logs = db.list_forward_logs(10).unwrap();
    assert_eq!(logs.len(), 2);
    let historical = logs.iter().find(|log| log.model == "glm-5.3").unwrap();
    assert_eq!(historical.route, "");
    let labeled = logs.iter().find(|log| log.model == "gpt-5.6-luna").unwrap();
    assert_eq!(labeled.route, "proxy");

    // The paginated query surface exposes the same column.
    let page = db
        .query_forward_logs(ForwardLogQueryOptions {
            limit: 10,
            offset: 0,
            status: None,
            account_id: None,
            provider_id: None,
            offering_id: None,
            route_account_id: None,
            credential_account_id: None,
            model: None,
            key_id: None,
            request_id: None,
            start_time: None,
            end_time: None,
            sort_by: None,
            sort_order: None,
        })
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert!(page.items.iter().all(|log| {
        (log.model == "glm-5.3" && log.route.is_empty())
            || (log.model == "gpt-5.6-luna" && log.route == "proxy")
    }));

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v16_migrates_existing_accounts_to_imported_ready_keys() {
    let dir = temp_data_dir("v16-account-lifecycle");
    let path = dir.join("data.sqlite");
    let now = Utc::now().to_rfc3339();
    let conn = Connection::open(&path).expect("fixture db should open");
    conn.execute_batch(
        "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (15);
             CREATE TABLE accounts (
                 id TEXT PRIMARY KEY, name TEXT NOT NULL, username TEXT,
                 password_cipher TEXT, key_cipher TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1, referral_code TEXT,
                 recharge_date TEXT NOT NULL, sort_order INTEGER NOT NULL DEFAULT 0,
                 cooldown_until TEXT, cooldown_generic_until TEXT,
                 cooldown_5h_until TEXT, cooldown_week_until TEXT,
                 cooldown_month_until TEXT, last_error TEXT, auth_error TEXT,
                 created_at TEXT NOT NULL, updated_at TEXT NOT NULL
             );
             CREATE TABLE forward_logs (
                 id INTEGER PRIMARY KEY, timestamp TEXT NOT NULL,
                 cost_state TEXT NOT NULL DEFAULT 'not_applicable', diagnostic_json TEXT
             );
             CREATE TABLE gateway_logs (
                 id INTEGER PRIMARY KEY, created_at TEXT NOT NULL, diagnostic_json TEXT
             );",
    )
    .expect("v15 fixture should be created");
    conn.execute(
        "INSERT INTO accounts
             (id, name, key_cipher, enabled, recharge_date, created_at, updated_at)
             VALUES ('legacy', 'Legacy', ?2, 1, '2026-08-01', ?1, ?1)",
        params![now, fixture_account_key_cipher()],
    )
    .expect("legacy account should be inserted");
    drop(conn);

    let db = open_with_host_cipher(dir.clone()).expect("v16 migration should succeed");
    let legacy = db
        .get_account("legacy")
        .expect("legacy account should load")
        .expect("legacy account should remain");
    assert_eq!(legacy.account_type, AccountType::Key);
    assert_eq!(legacy.setup_step, AccountSetupStep::Ready);
    let version: i64 = db
        .conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version as i32, CURRENT_SCHEMA_VERSION);
    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn managed_setup_requires_order_and_matching_verified_key() {
    let dir = temp_data_dir("managed-setup-state");
    let db = Database::open(dir.clone()).expect("db should open");
    let mut managed = account("managed");
    managed.account_type = AccountType::Managed;
    managed.setup_step = AccountSetupStep::GoogleAccount;
    managed.key_cipher.clear();
    managed.enabled = false;
    db.create_account(&managed).expect("draft should save");

    assert!(
        !db.advance_managed_setup(
            "managed",
            AccountSetupStep::OpencodeRegistration,
            AccountSetupStep::Payment,
        )
        .unwrap()
    );
    for (from, to) in [
        (
            AccountSetupStep::GoogleAccount,
            AccountSetupStep::OpencodeRegistration,
        ),
        (
            AccountSetupStep::OpencodeRegistration,
            AccountSetupStep::Payment,
        ),
    ] {
        assert!(db.advance_managed_setup("managed", from, to).unwrap());
    }
    db.conn
            .execute(
                "UPDATE accounts SET recharge_date = '2000-01-01', usage_month_window_cost_offset = 1 WHERE id = 'managed'",
                [],
            )
            .unwrap();
    assert!(
        db.advance_managed_setup(
            "managed",
            AccountSetupStep::Payment,
            AccountSetupStep::KeyVerification,
        )
        .unwrap()
    );
    let paid = db.get_account("managed").unwrap().unwrap();
    assert_eq!(paid.purchase_date, local_today());
    let month_offset: f64 = db
        .conn
        .query_row(
            "SELECT usage_month_window_cost_offset FROM accounts WHERE id = 'managed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(month_offset, 0.0);
    assert!(
        db.save_managed_key_for_verification("managed", "candidate")
            .unwrap()
    );
    assert!(
        !db.complete_managed_setup_if_key_matches("managed", "stale")
            .unwrap()
    );
    assert!(
        db.complete_managed_setup_if_key_matches("managed", "candidate")
            .unwrap()
    );
    let ready = db.get_account("managed").unwrap().unwrap();
    assert_eq!(ready.setup_step, AccountSetupStep::Ready);
    assert!(ready.enabled);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn managed_key_verification_transaction_rolls_back_after_candidate_write_failure() {
    let dir = temp_data_dir("managed-key-atomic-rollback");
    let db = Database::open(dir.clone()).expect("db should open");
    let mut managed = account("managed-atomic");
    managed.account_type = AccountType::Managed;
    managed.setup_step = AccountSetupStep::KeyVerification;
    managed.key_cipher = "original-cipher".into();
    managed.enabled = false;
    db.create_account(&managed).expect("draft should save");
    let cooldown = (Utc::now() + Duration::hours(2)).to_rfc3339();
    db.conn
        .execute(
            "UPDATE accounts
                 SET auth_error = 'original-auth', last_error = 'original-limit',
                     cooldown_until = ?2, cooldown_generic_until = ?2,
                     verification_error = 'original-verification'
                 WHERE id = ?1",
            params![managed.id, cooldown],
        )
        .expect("rollback sentinel state should save");
    let before = db.get_account(&managed.id).unwrap().unwrap();
    let before_verification = db.account_verification_state(&managed.id).unwrap().unwrap();

    // The first transaction update writes the candidate while leaving the
    // setup step unchanged. This trigger deterministically aborts the
    // following completion update, after that first intended write.
    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_managed_verification_completion
                 BEFORE UPDATE ON accounts
                 WHEN OLD.id = 'managed-atomic'
                      AND OLD.setup_step = 'key_verification'
                      AND NEW.setup_step = 'ready'
                 BEGIN
                     SELECT RAISE(ABORT, 'injected managed verification completion failure');
                 END;",
        )
        .expect("fault trigger should install");

    let error = db
        .commit_managed_key_verification(
            &managed.id,
            &ManagedKeyVerificationCas::from_account(&before),
            "candidate-cipher",
            &ManagedKeyVerificationWrite::Verified {
                rate_limit: None,
                account_name: managed.name.clone(),
            },
        )
        .expect_err("second intended write should fail");
    assert!(
        error
            .to_string()
            .contains("injected managed verification completion failure"),
        "{error:#}"
    );

    let after = db.get_account(&managed.id).unwrap().unwrap();
    let after_verification = db.account_verification_state(&managed.id).unwrap().unwrap();
    assert_eq!(after.key_cipher, before.key_cipher);
    assert_eq!(after.enabled, before.enabled);
    assert_eq!(after.setup_step, before.setup_step);
    assert_eq!(after.auth_error, before.auth_error);
    assert_eq!(after.last_error, before.last_error);
    assert_eq!(after.cooldown_until, before.cooldown_until);
    assert_eq!(after.cooldown_generic_until, before.cooldown_generic_until);
    assert_eq!(after.updated_at, before.updated_at);
    assert_eq!(after_verification.status, before_verification.status);
    assert_eq!(
        after_verification.connection_verified_at,
        before_verification.connection_verified_at
    );
    assert_eq!(
        after_verification.verification_error,
        before_verification.verification_error
    );
    assert!(
        db.list_gateway_logs(10)
            .unwrap()
            .iter()
            .all(|log| !log.message.contains("managed-atomic")),
        "success log must roll back with the account writes"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn managed_key_verification_transaction_rolls_back_if_gateway_audit_insert_aborts() {
    let dir = temp_data_dir("managed-key-audit-rollback");
    let db = Database::open(dir.clone()).expect("db should open");
    let mut managed = account("managed-audit-atomic");
    managed.account_type = AccountType::Managed;
    managed.setup_step = AccountSetupStep::KeyVerification;
    managed.key_cipher = "original-cipher".into();
    managed.enabled = false;
    db.create_account(&managed).expect("draft should save");
    let before = db.get_account(&managed.id).unwrap().unwrap();
    let before_verification = db.account_verification_state(&managed.id).unwrap().unwrap();

    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_managed_verification_audit
                 BEFORE INSERT ON gateway_logs
                 BEGIN
                     SELECT RAISE(ABORT, 'injected managed verification audit failure');
                 END;",
        )
        .expect("fault trigger should install");

    let error = db
        .commit_managed_key_verification(
            &managed.id,
            &ManagedKeyVerificationCas::from_account(&before),
            "candidate-cipher",
            &ManagedKeyVerificationWrite::Verified {
                rate_limit: None,
                account_name: managed.name.clone(),
            },
        )
        .expect_err("gateway audit insert should fail");
    assert!(
        error
            .to_string()
            .contains("injected managed verification audit failure"),
        "{error:#}"
    );

    let after = db.get_account(&managed.id).unwrap().unwrap();
    let after_verification = db.account_verification_state(&managed.id).unwrap().unwrap();
    assert_eq!(after.key_cipher, before.key_cipher);
    assert_eq!(after.enabled, before.enabled);
    assert_eq!(after.setup_step, before.setup_step);
    assert_eq!(after.updated_at, before.updated_at);
    assert_eq!(after_verification.status, before_verification.status);
    assert_eq!(
        after_verification.connection_verified_at,
        before_verification.connection_verified_at
    );
    assert!(
        db.list_gateway_logs(10)
            .unwrap()
            .iter()
            .all(|log| !log.message.contains("managed-audit-atomic")),
        "aborted audit insert must not persist a success log"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

fn forward_log_at(
    account_id: &str,
    status: &str,
    cost: f64,
    timestamp: DateTime<Utc>,
) -> ForwardLog {
    let mut log = forward_log(account_id, status, cost);
    log.timestamp = timestamp;
    log
}

fn finalize_success(db: &Database, account_id: &str, cost: f64, timestamp: DateTime<Utc>) {
    let id = db
        .log_forward(&forward_log_at(account_id, "streaming", 0.0, timestamp))
        .expect("log should insert");
    db.update_forward_log(
        id,
        "success",
        None,
        ForwardMetrics {
            cost,
            cost_state: "priced",
            ..ForwardMetrics::default()
        },
        None,
        None,
    )
    .expect("stream should finalize");
}

fn assert_cost(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

fn create_v6_database(
    dir: &std::path::Path,
    extra_cooldown_columns: &str,
    extra_indexes: &str,
) -> Connection {
    let conn = Connection::open(dir.join("data.sqlite")).expect("v6 db should open");
    conn.execute_batch(&format!(
        "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (6);
             CREATE TABLE accounts (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 key_cipher TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1,
                 referral_code TEXT,
                 recharge_date TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL,
                 cooldown_until TEXT,
                 last_error TEXT,
                 username TEXT,
                 password_cipher TEXT,
                 usage_5h_baseline_percent REAL,
                 usage_5h_anchor_success_cost REAL,
                 usage_week_baseline_percent REAL,
                 usage_week_anchor_success_cost REAL,
                 usage_month_baseline_percent REAL,
                 usage_month_anchor_success_cost REAL,
                 sort_order INTEGER NOT NULL DEFAULT 0
                 {extra_cooldown_columns}
             );
             CREATE TABLE forward_logs (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 timestamp TEXT NOT NULL,
                 model TEXT NOT NULL,
                 account_id TEXT NOT NULL,
                 account_name TEXT NOT NULL,
                 status TEXT NOT NULL,
                 http_status INTEGER,
                 prompt_tokens INTEGER NOT NULL DEFAULT 0,
                 completion_tokens INTEGER NOT NULL DEFAULT 0,
                 cached_tokens INTEGER NOT NULL DEFAULT 0,
                 cost REAL NOT NULL DEFAULT 0,
                 error_message TEXT
             );
             {extra_indexes}"
    ))
    .expect("v6 schema should be created");
    conn
}

#[test]
fn v7_migration_repairs_pr11_pr12_and_combined_v6_databases() {
    let future = (Utc::now() + Duration::days(2)).to_rfc3339();
    for (label, extra_columns, extra_indexes, source_column, error) in [
        (
            "pr11-v6",
            "",
            "CREATE INDEX idx_forward_logs_model ON forward_logs(model);\nCREATE INDEX idx_forward_logs_status ON forward_logs(status);",
            "",
            "5 hour usage limit reached",
        ),
        (
            "pr12-v6",
            ", cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
            "",
            "cooldown_week_until",
            "weekly usage limit reached",
        ),
        (
            "combined-v6",
            ", cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
            "CREATE INDEX idx_forward_logs_model ON forward_logs(model);\nCREATE INDEX idx_forward_logs_status ON forward_logs(status);",
            "cooldown_month_until",
            "monthly usage limit reached",
        ),
        (
            "generic-dev-v6",
            ", cooldown_generic_until TEXT, cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
            "CREATE INDEX idx_forward_logs_model ON forward_logs(model);\nCREATE INDEX idx_forward_logs_status ON forward_logs(status);",
            "cooldown_generic_until",
            "unknown rate limit",
        ),
    ] {
        let dir = temp_data_dir(label);
        let conn = create_v6_database(&dir, extra_columns, extra_indexes);
        conn.execute(
                "INSERT INTO accounts
                 (id, name, key_cipher, recharge_date, created_at, updated_at, cooldown_until, last_error)
                 VALUES ('old', 'old', ?4, '2026-07-01', ?1, ?1, ?2, ?3)",
                params![Utc::now().to_rfc3339(), future, error, fixture_account_key_cipher()],
            )
            .expect("v6 account should be inserted");
        if !source_column.is_empty() {
            conn.execute(
                &format!("UPDATE accounts SET {source_column} = ?1 WHERE id = 'old'"),
                [&future],
            )
            .expect("existing cooldown source should be set");
        }
        drop(conn);

        let db = open_with_host_cipher(dir.clone()).expect("v6 database should migrate");
        let version: i32 = db
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .expect("schema version should load");
        assert_eq!(version, CURRENT_SCHEMA_VERSION, "{label}");
        let account = db
            .get_account("old")
            .expect("account query should work")
            .expect("account should exist");
        assert!(account.cooldown_until.is_some(), "{label}");
        assert!(account.is_cooling_at(Utc::now()), "{label}");
        let indexes: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'index' AND name IN (
                         'idx_forward_logs_model',
                         'idx_forward_logs_status',
                         'idx_forward_logs_time_instant'
                     )",
                [],
                |row| row.get(0),
            )
            .expect("indexes should be queryable");
        assert_eq!(indexes, 3, "{label}");

        drop(db);
        fs::remove_dir_all(dir).expect("test data dir should be removed");
    }
}

#[test]
fn v4_migration_preserves_uncalibrated_usage() {
    let dir = temp_data_dir("v4-migration");
    let conn = Connection::open(dir.join("data.sqlite")).expect("v3 db should open");
    conn.execute_batch(
        "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (3);
             CREATE TABLE accounts (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 key_cipher TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1,
                 referral_code TEXT,
                 recharge_date TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL,
                 cooldown_until TEXT,
                 last_error TEXT,
                 username TEXT,
                 password_cipher TEXT
             );
             CREATE TABLE forward_logs (
                 timestamp TEXT NOT NULL,
                 model TEXT NOT NULL DEFAULT 'test',
                 account_id TEXT NOT NULL,
                 status TEXT NOT NULL,
                 cost REAL NOT NULL DEFAULT 0
             );",
    )
    .expect("v3 schema should be created");
    let now = Utc::now().to_rfc3339();
    conn.execute(
            "INSERT INTO accounts (id, name, key_cipher, created_at, updated_at) VALUES (?1, ?1, ?3, ?2, ?2)",
            params!["old", now, fixture_account_key_cipher()],
        )
        .expect("v3 account should be inserted");
    conn.execute(
            "INSERT INTO forward_logs (timestamp, account_id, status, cost) VALUES (?1, 'old', 'success', 2.5)",
            [Utc::now().to_rfc3339()],
        )
        .expect("v3 usage should be inserted");
    drop(conn);

    let db = open_with_host_cipher(dir.clone()).expect("v3 db should migrate");
    let version: i32 = db
        .conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .expect("schema version should be readable");
    let usage = db.account_usage("old").expect("usage should load");
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        db.get_account("old")
            .expect("account should load")
            .expect("account should exist")
            .purchase_date,
        now[..10]
    );
    assert_cost(usage.window_5h, 2.5);
    assert_cost(usage.window_week, 2.5);
    assert_cost(usage.window_month, 2.5);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn v5_migration_backfills_dates_and_stable_dense_order() {
    let dir = temp_data_dir("v5-migration");
    let conn = Connection::open(dir.join("data.sqlite")).expect("v4 db should open");
    conn.execute_batch(
        "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (4);
             CREATE TABLE accounts (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 key_cipher TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1,
                 referral_code TEXT,
                 recharge_date TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL,
                 cooldown_until TEXT,
                 last_error TEXT,
                 username TEXT,
                 password_cipher TEXT,
                 usage_5h_baseline_percent REAL,
                 usage_5h_anchor_success_cost REAL,
                 usage_week_baseline_percent REAL,
                 usage_week_anchor_success_cost REAL,
                 usage_month_baseline_percent REAL,
                 usage_month_anchor_success_cost REAL
             );
             CREATE TABLE forward_logs (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 timestamp TEXT NOT NULL,
                 model TEXT NOT NULL,
                 account_id TEXT NOT NULL,
                 account_name TEXT NOT NULL,
                 status TEXT NOT NULL,
                 http_status INTEGER,
                 prompt_tokens INTEGER NOT NULL DEFAULT 0,
                 completion_tokens INTEGER NOT NULL DEFAULT 0,
                 cached_tokens INTEGER NOT NULL DEFAULT 0,
                 cost REAL NOT NULL DEFAULT 0,
                 error_message TEXT
             );",
    )
    .expect("v4 schema should be created");
    let shared_created_at = "2026-01-02T01:30:00+02:00";
    for (id, recharge_date, created_at) in [
        ("a", Some("2025-12-31"), shared_created_at),
        ("b", None, shared_created_at),
        ("c", Some(""), shared_created_at),
        ("d", Some("2026-2-3"), "2026-02-04T04:00:00Z"),
    ] {
        conn.execute(
            "INSERT INTO accounts
                 (id, name, key_cipher, recharge_date, created_at, updated_at)
                 VALUES (?1, ?1, ?4, ?2, ?3, ?3)",
            params![id, recharge_date, created_at, fixture_account_key_cipher()],
        )
        .expect("v4 account should be inserted");
    }
    drop(conn);

    let db = open_with_host_cipher(dir.clone()).expect("v4 db should migrate");
    let accounts = db.list_accounts().expect("migrated accounts should load");
    assert_eq!(
        accounts
            .iter()
            .map(|account| account.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c", "d", ZEN_FREE_ACCOUNT_ID]
    );
    assert_eq!(accounts[0].purchase_date, "2025-12-31");
    assert_eq!(accounts[1].purchase_date, "2026-01-01");
    assert_eq!(accounts[2].purchase_date, "2026-01-01");
    assert_eq!(accounts[3].purchase_date, "2026-02-04");
    let sort_orders = db
        .conn
        .prepare("SELECT sort_order FROM accounts ORDER BY sort_order")
        .expect("sort query should prepare")
        .query_map([], |row| row.get::<_, i64>(0))
        .expect("sort query should run")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("sort orders should load");
    assert_eq!(sort_orders, [0, 1, 2, 3, 4]);
    drop(db);

    let reopened = open_with_host_cipher(dir.clone()).expect("migrated db should reopen");
    assert_eq!(
        reopened
            .list_accounts()
            .expect("reopened accounts should load")
            .iter()
            .map(|account| account.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c", "d", ZEN_FREE_ACCOUNT_ID]
    );

    drop(reopened);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn v8_migration_repairs_purchase_dates_written_by_older_binaries() {
    let dir = temp_data_dir("v8-purchase-date-repair");
    let conn = create_v6_database(
        &dir,
        ", cooldown_generic_until TEXT, cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
        "",
    );
    conn.execute("INSERT INTO schema_version (version) VALUES (7)", [])
        .expect("v7 schema version should be recorded");

    let created_at = "2026-01-02T01:30:00+02:00";
    for (id, recharge_date) in [
        ("valid", Some("2025-12-31")),
        ("null", None),
        ("invalid", Some("2026-2-3")),
    ] {
        conn.execute(
            "INSERT INTO accounts
                 (id, name, key_cipher, recharge_date, created_at, updated_at)
                 VALUES (?1, ?1, ?4, ?2, ?3, ?3)",
            params![id, recharge_date, created_at, fixture_account_key_cipher()],
        )
        .expect("legacy account should be inserted");
    }
    drop(conn);

    let db = open_with_host_cipher(dir.clone()).expect("v7 database should migrate");
    let version: i32 = db
        .conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .expect("schema version should load");
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        db.get_account("valid")
            .expect("valid account query should work")
            .expect("valid account should exist")
            .purchase_date,
        "2025-12-31"
    );
    for id in ["null", "invalid"] {
        assert_eq!(
            db.get_account(id)
                .expect("repaired account query should work")
                .expect("repaired account should exist")
                .purchase_date,
            "2026-01-01",
            "{id}"
        );
    }

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn v9_migration_preserves_charged_legacy_errors() {
    let dir = temp_data_dir("v9-charged-error-cost");
    let conn = create_v6_database(
        &dir,
        ", cooldown_generic_until TEXT, cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
        "",
    );
    conn.execute("INSERT INTO schema_version (version) VALUES (7)", [])
        .expect("v7 schema version should be recorded");
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO accounts
             (id, name, key_cipher, recharge_date, created_at, updated_at)
             VALUES ('legacy', 'legacy', ?2, '2026-07-01', ?1, ?1)",
        params![now, fixture_account_key_cipher()],
    )
    .expect("legacy account should be inserted");
    for (status, cost) in [("error", 1.25), ("error", 0.0), ("success", 2.0)] {
        conn.execute(
            "INSERT INTO forward_logs
                 (timestamp, model, account_id, account_name, status, http_status, cost)
                 VALUES (?1, 'glm-5.2', 'legacy', 'legacy', ?2, 200, ?3)",
            params![now, status, cost],
        )
        .expect("legacy forward log should be inserted");
    }
    drop(conn);

    let db = open_with_host_cipher(dir.clone()).expect("v7 database should migrate through v10");
    let states = db
        .conn
        .prepare("SELECT status, cost, cost_state FROM forward_logs ORDER BY id")
        .expect("migrated logs should prepare")
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .expect("migrated logs should query")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("migrated logs should load");
    assert_eq!(
        states,
        [
            ("error".to_string(), 1.25, "legacy_estimate".to_string()),
            ("error".to_string(), 0.0, "not_applicable".to_string()),
            ("success".to_string(), 2.0, "legacy_estimate".to_string()),
        ]
    );
    assert_cost(
        db.account_usage("legacy")
            .expect("legacy usage should load")
            .window_month,
        3.25,
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn v10_migration_repairs_charged_errors_from_original_v9() {
    let dir = temp_data_dir("v10-repair-v9-charged-error-cost");
    let conn = create_v6_database(
        &dir,
        ", cooldown_generic_until TEXT, cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
        "",
    );
    conn.execute_batch(
        "CREATE TABLE pricing_snapshots (
                 revision TEXT PRIMARY KEY,
                 activated_at TEXT NOT NULL,
                 document_updated_at TEXT NOT NULL,
                 source_url TEXT NOT NULL,
                 content_hash TEXT NOT NULL,
                 snapshot_json TEXT NOT NULL
             );
             CREATE INDEX idx_pricing_snapshots_activated
                 ON pricing_snapshots(activated_at DESC);
             ALTER TABLE forward_logs ADD COLUMN pricing_revision_id TEXT;
             ALTER TABLE forward_logs ADD COLUMN quota_multiplier REAL;
             ALTER TABLE forward_logs ADD COLUMN local_adjustment_multiplier REAL;
             ALTER TABLE forward_logs ADD COLUMN cache_creation_tokens INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE forward_logs ADD COLUMN service_tier TEXT;
             ALTER TABLE forward_logs ADD COLUMN cost_state TEXT NOT NULL DEFAULT 'not_applicable';
             INSERT INTO schema_version (version) VALUES (9);",
    )
    .expect("original v9 schema should be created");
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO accounts
             (id, name, key_cipher, recharge_date, created_at, updated_at)
             VALUES ('legacy', 'legacy', ?2, '2026-07-01', ?1, ?1)",
        params![now, fixture_account_key_cipher()],
    )
    .expect("legacy account should be inserted");
    for (cost, cost_state) in [
        (1.25, "not_applicable"),
        (0.0, "not_applicable"),
        (4.0, "unpriced"),
    ] {
        conn.execute(
            "INSERT INTO forward_logs
                 (timestamp, model, account_id, account_name, status, http_status, cost, cost_state)
                 VALUES (?1, 'glm-5.2', 'legacy', 'legacy', 'error', 200, ?2, ?3)",
            params![now, cost, cost_state],
        )
        .expect("original v9 forward log should be inserted");
    }
    drop(conn);

    let db = open_with_host_cipher(dir.clone()).expect("v9 database should migrate through v11");
    let version: i32 = db
        .conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .expect("schema version should load");
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    let states = db
        .conn
        .prepare("SELECT cost, cost_state FROM forward_logs ORDER BY id")
        .expect("migrated logs should prepare")
        .query_map([], |row| {
            Ok((row.get::<_, f64>(0)?, row.get::<_, String>(1)?))
        })
        .expect("migrated logs should query")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("migrated logs should load");
    assert_eq!(
        states,
        [
            (1.25, "legacy_estimate".to_string()),
            (0.0, "not_applicable".to_string()),
            (4.0, "unpriced".to_string()),
        ]
    );
    assert_cost(
        db.account_usage("legacy")
            .expect("legacy usage should load")
            .window_month,
        1.25,
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn account_reads_fallback_after_v8_data_is_corrupted() {
    let dir = temp_data_dir("post-v8-purchase-date-corruption");
    let conn = create_v6_database(
        &dir,
        ", cooldown_generic_until TEXT, cooldown_5h_until TEXT, cooldown_week_until TEXT, cooldown_month_until TEXT",
        "",
    );
    conn.execute("INSERT INTO schema_version (version) VALUES (7)", [])
        .expect("v7 schema version should be recorded");
    drop(conn);

    let db = Database::open(dir.clone()).expect("database should open");
    let version: i32 = db
        .conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .expect("schema version should load");
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    let created_at = DateTime::parse_from_rfc3339("2026-01-02T01:30:00+02:00")
        .expect("fixed timestamp should parse")
        .with_timezone(&Utc);
    for id in ["null", "invalid"] {
        let mut legacy = account(id);
        legacy.purchase_date = "2025-12-31".to_string();
        legacy.created_at = created_at;
        legacy.updated_at = created_at;
        db.create_account(&legacy)
            .expect("account should be created before corruption");
    }
    // v12 重建 accounts 表后 recharge_date 是 NOT NULL（恢复 v1 原始约束），
    // 无法再被 UPDATE 成 NULL；只测试 invalid-text 这一支。
    db.conn
        .execute(
            "UPDATE accounts SET recharge_date = 'not-a-date' WHERE id = 'invalid'",
            [],
        )
        .expect("purchase date should be corrupted to invalid text");

    let accounts = db
        .list_accounts()
        .expect("one corrupt row must not break the account list");
    assert_eq!(accounts.len(), 3);
    // 仅 invalid 被破坏；null 仍持有原始 2025-12-31。
    let invalid_account = accounts
        .iter()
        .find(|a| a.id == "invalid")
        .expect("invalid account should be present");
    assert_eq!(
        invalid_account.purchase_date, "2026-01-01",
        "list_accounts should fall back to default date for corrupted rows"
    );
    let invalid = db
        .get_account("invalid")
        .expect("corrupt account query should work")
        .expect("corrupt account should exist");
    assert_eq!(invalid.purchase_date, "2026-01-01");
    assert_eq!(invalid.expires_on, "2026-02-01");
    let remains_invalid: bool = db
        .conn
        .query_row(
            "SELECT recharge_date = 'not-a-date' FROM accounts WHERE id = 'invalid'",
            [],
            |row| row.get(0),
        )
        .expect("raw purchase date should remain queryable");
    assert!(
        remains_invalid,
        "read fallback must not hide a migration rerun"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn account_creation_defaults_dates_and_appends_to_saved_order() {
    let dir = temp_data_dir("create-order");
    let db = Database::open(dir.clone()).expect("db should open");
    let purchase_date_is_not_null: bool = db
        .conn
        .query_row(
            "SELECT [notnull]
                 FROM pragma_table_info('accounts')
                 WHERE name = 'recharge_date'",
            [],
            |row| row.get(0),
        )
        .expect("fresh account schema should expose purchase date constraints");
    assert!(purchase_date_is_not_null);
    let mut first = account("first");
    first.created_at = Utc::now() + Duration::days(1);
    db.create_account(&first)
        .expect("first account should save");
    let mut second = account("second");
    second.created_at = Utc::now() - Duration::days(1);
    second.purchase_date = "2024-01-31".to_string();
    db.create_account(&second)
        .expect("second account should save");

    let accounts = db.list_accounts().expect("accounts should load");
    assert_eq!(
        accounts
            .iter()
            .map(|account| account.id.as_str())
            .collect::<Vec<_>>(),
        [ZEN_FREE_ACCOUNT_ID, "first", "second"]
    );
    assert_eq!(accounts[1].purchase_date, local_today());
    assert_eq!(
        accounts[1].expires_on,
        purchase_expires_on(&accounts[1].purchase_date)
            .expect("default date should have an expiry")
    );
    assert_eq!(accounts[2].expires_on, "2024-02-29");

    let mut invalid = account("invalid");
    invalid.purchase_date = "2026-2-03".to_string();
    assert!(db.create_account(&invalid).is_err());
    assert!(
        db.get_account("invalid")
            .expect("invalid account lookup should work")
            .is_none()
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn zen_enabled_has_a_dedicated_writer_and_generic_update_is_rejected() {
    let dir = temp_data_dir("zen-enabled-writer");
    let db = Database::open(dir.clone()).expect("db should open");
    db.set_setting("config", r#"{"marker":"before"}"#)
        .expect("initial config should save");
    let zen_before = db
        .get_account(ZEN_FREE_ACCOUNT_ID)
        .expect("Zen lookup should work")
        .expect("Zen singleton should exist");

    let generic = AccountUpdate {
        name: None,
        username: None,
        password: None,
        key: None,
        enabled: Some(!zen_before.enabled),
        referral_code: None,
        purchase_date: None,
        notes: None,
    };
    assert!(
        db.update_account(ZEN_FREE_ACCOUNT_ID, &generic, None, None)
            .is_err(),
        "generic account writers must not bypass the Zen facade"
    );

    db.conn
        .execute_batch(&format!(
            "CREATE TRIGGER reject_zen_provider_settings
                 BEFORE UPDATE OF enabled, free_alias_enabled ON accounts
                 WHEN OLD.id = '{ZEN_FREE_ACCOUNT_ID}'
                 BEGIN
                     SELECT RAISE(ABORT, 'forced Zen settings failure');
                 END;"
        ))
        .expect("failure trigger should install");
    db.set_config(r#"{"marker":"after"}"#)
        .expect("ordinary config should save independently");
    let error = db
        .set_zen_free_enabled(!zen_before.enabled)
        .expect_err("Zen row failure must abort the config write");
    assert!(error.to_string().contains("forced Zen settings failure"));
    assert_eq!(
        db.get_setting("config").unwrap().as_deref(),
        Some(r#"{"marker":"after"}"#)
    );
    let zen_after_failure = db.get_account(ZEN_FREE_ACCOUNT_ID).unwrap().unwrap();
    assert_eq!(zen_after_failure.enabled, zen_before.enabled);

    db.conn
        .execute("DROP TRIGGER reject_zen_provider_settings", [])
        .expect("failure trigger should drop");
    db.set_zen_free_enabled(true)
        .expect("Zen enabled setting should save");
    let zen_after = db.get_account(ZEN_FREE_ACCOUNT_ID).unwrap().unwrap();
    assert!(zen_after.enabled);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn reorder_accounts_validates_atomically_and_persists_dense_order() {
    let dir = temp_data_dir("reorder");
    let db = Database::open(dir.clone()).expect("db should open");
    for id in ["a", "b", "c"] {
        db.create_account(&account(id))
            .expect("account should be created");
    }

    db.reorder_accounts(&[
        "c".into(),
        "a".into(),
        "b".into(),
        ZEN_FREE_ACCOUNT_ID.into(),
    ])
    .expect("valid reorder should save");
    assert_eq!(account_ids(&db), ["c", "a", "b", ZEN_FREE_ACCOUNT_ID]);

    let duplicate = db
        .reorder_accounts(&[
            "c".into(),
            "c".into(),
            "b".into(),
            ZEN_FREE_ACCOUNT_ID.into(),
        ])
        .expect_err("duplicates should fail");
    assert!(matches!(
        duplicate,
        ReorderAccountsError::DuplicateAccountId
    ));
    assert_eq!(account_ids(&db), ["c", "a", "b", ZEN_FREE_ACCOUNT_ID]);

    for stale in [
        vec!["c".into(), "a".into()],
        vec!["c".into(), "a".into(), "missing".into()],
        Vec::<String>::new(),
    ] {
        let error = db
            .reorder_accounts(&stale)
            .expect_err("stale account set should fail");
        assert!(matches!(error, ReorderAccountsError::AccountSetMismatch));
        assert_eq!(account_ids(&db), ["c", "a", "b", ZEN_FREE_ACCOUNT_ID]);
    }

    let sort_orders = db
        .conn
        .prepare("SELECT sort_order FROM accounts ORDER BY sort_order")
        .expect("sort query should prepare")
        .query_map([], |row| row.get::<_, i64>(0))
        .expect("sort query should run")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("sort orders should load");
    assert_eq!(sort_orders, [0, 1, 2, 3]);
    drop(db);

    let reopened = Database::open(dir.clone()).expect("db should reopen");
    assert_eq!(account_ids(&reopened), ["c", "a", "b", ZEN_FREE_ACCOUNT_ID]);
    drop(reopened);

    let empty_dir = temp_data_dir("reorder-empty");
    let empty = Database::open(empty_dir.clone()).expect("empty db should open");
    empty
        .reorder_accounts(&[ZEN_FREE_ACCOUNT_ID.into()])
        .expect("the built-in Zen row is the complete empty-user order");
    drop(empty);

    fs::remove_dir_all(dir).expect("test data dir should be removed");
    fs::remove_dir_all(empty_dir).expect("empty test data dir should be removed");
}

#[test]
fn reorder_accounts_rolls_back_when_an_update_fails_mid_transaction() {
    let dir = temp_data_dir("reorder-write-failure");
    let db = Database::open(dir.clone()).expect("db should open");
    for id in ["a", "b", "c"] {
        db.create_account(&account(id))
            .expect("account should be created");
    }
    db.conn
        .execute_batch(
            "CREATE TRIGGER reject_b_sort_update
                 BEFORE UPDATE OF sort_order ON accounts
                 WHEN NEW.id = 'b'
                 BEGIN
                     SELECT RAISE(ABORT, 'forced reorder failure');
                 END;",
        )
        .expect("failure trigger should be installed");

    let error = db
        .reorder_accounts(&[
            "c".into(),
            "a".into(),
            "b".into(),
            ZEN_FREE_ACCOUNT_ID.into(),
        ])
        .expect_err("the trigger should interrupt the reorder");
    assert!(matches!(error, ReorderAccountsError::Database(_)));
    assert_eq!(account_ids(&db), [ZEN_FREE_ACCOUNT_ID, "a", "b", "c"]);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

fn account_ids(db: &Database) -> Vec<String> {
    db.list_accounts()
        .expect("accounts should load")
        .into_iter()
        .map(|account| account.id)
        .collect()
}

#[test]
fn v22_migration_failure_rolls_back_to_usable_v21_source() {
    let dir = temp_data_dir("v22-atomic-migration");
    create_v21_fixture(&dir, true);

    assert!(Database::open(dir.clone()).is_err());
    let conn = Connection::open(dir.join("data.sqlite")).expect("db should reopen");
    let columns = conn
        .prepare("PRAGMA table_info(accounts)")
        .expect("table info should prepare")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("table info should query")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("columns should load");
    let version: i32 = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .expect("schema version should load");
    let preserved_account: (String, String, i64) = conn
        .query_row(
            "SELECT name, key_cipher, enabled FROM accounts WHERE id = 'rollback-account'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("source account should remain readable");
    let preserved_log: (String, String, String, f64) = conn
        .query_row(
            "SELECT account_id, model, status, cost FROM forward_logs LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("source forward log should remain readable");
    assert!(!columns.iter().any(|name| name == "provider_id"));
    assert_eq!(version, 21);
    assert_eq!(preserved_account.0, "rollback-account");
    assert_eq!(preserved_account.2, 1);
    assert_fixture_account_cipher(&preserved_account.1);
    assert_eq!(
        preserved_log,
        (
            "rollback-account".into(),
            "test".into(),
            "success".into(),
            4.25
        )
    );

    drop(conn);
    let backups_before = pre_v22_backup_paths(&dir);
    assert_eq!(backups_before.len(), 1);
    let backup_bytes = fs::read(&backups_before[0]).expect("rollback backup should be readable");
    assert!(Database::open(dir.clone()).is_err());
    assert_eq!(pre_v22_backup_paths(&dir), backups_before);
    assert_eq!(
        fs::read(&backups_before[0]).expect("rollback backup should remain readable"),
        backup_bytes
    );
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn v20_to_v22_failure_rolls_back_v21_and_v22_writes() {
    let dir = temp_data_dir("v20-v22-atomic-migration");
    create_v20_fixture(&dir, true);

    assert!(Database::open(dir.clone()).is_err());
    let conn = Connection::open(dir.join("data.sqlite")).expect("db should reopen");
    let columns = conn
        .prepare("PRAGMA table_info(accounts)")
        .expect("table info should prepare")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("table info should query")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("columns should load");
    assert!(
        !columns
            .iter()
            .any(|name| name == "usage_sync_last_success_at")
    );
    assert!(!columns.iter().any(|name| name == "provider_id"));
    assert_eq!(schema_version_on(&conn).unwrap(), 20);
    drop(conn);

    let backups_before = pre_v22_backup_paths(&dir);
    assert_eq!(backups_before.len(), 1);
    let backup_bytes = fs::read(&backups_before[0]).expect("backup should be readable");
    let backup = Connection::open_with_flags(&backups_before[0], OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("backup should open read-only");
    assert_eq!(schema_version_on(&backup).unwrap(), 20);
    drop(backup);

    assert!(Database::open(dir.clone()).is_err());
    assert_eq!(pre_v22_backup_paths(&dir), backups_before);
    assert_eq!(
        fs::read(&backups_before[0]).expect("backup should remain readable"),
        backup_bytes
    );
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn clear_account_cooldown_clears_free_window() {
    let dir = temp_data_dir("clear-free-cooldown");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("free-cd"))
        .expect("account should be created");

    let until = Utc::now() + Duration::minutes(30);
    db.set_account_rate_limit(
        "free-cd",
        until,
        r#"{"type":"FreeUsageLimitError","message":"Free usage exceeded"}"#,
        Some(UsageWindowKind::Free),
    )
    .expect("free rate limit should save");

    let cooled = db
        .get_account("free-cd")
        .expect("account should load")
        .expect("account should exist");
    assert!(cooled.cooldown_free_until.is_some());
    assert!(cooled.cooldown_until.is_some());

    db.clear_account_cooldown("free-cd")
        .expect("clear should succeed");
    let cleared = db
        .get_account("free-cd")
        .expect("account should load")
        .expect("account should exist");
    assert!(cleared.cooldown_free_until.is_none());
    assert!(cleared.cooldown_until.is_none());
    assert!(cleared.cooldown_generic_until.is_none());
    assert!(cleared.last_error.is_none());

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn free_channel_cooldown_survives_account_deletion_restart_and_expires() {
    let dir = temp_data_dir("global-free-cooldown");
    let until = Utc::now() + Duration::minutes(30);
    {
        let mut db = Database::open(dir.clone()).expect("db should open");
        db.create_account(&account("free-source"))
            .expect("source account should be created");
        db.set_account_rate_limit(
            "free-source",
            until,
            "free quota exhausted",
            Some(UsageWindowKind::Free),
        )
        .expect("free rate limit should save");
        db.delete_account("free-source")
            .expect("source account should be deleted");
        db.create_account(&account("replacement"))
            .expect("replacement account should be created");

        assert!(db.free_channel_cooldown_until().unwrap().is_some());
    }

    let db = Database::open(dir.clone()).expect("db should reopen");
    assert!(
        db.free_channel_cooldown_until()
            .expect("global cooldown should load")
            .is_some(),
        "deleting every source row and reopening must not clear the IP-wide cooldown"
    );
    assert!(
        db.free_channel_cooldown_until_at(until + Duration::seconds(1))
            .expect("expiry should be evaluated")
            .is_none(),
        "the global gate must reopen after its deadline"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn durable_free_cooldown_expires_at_exact_deadline() {
    let dir = temp_data_dir("free-cooldown-exact-boundary");
    let until = DateTime::from_naive_utc_and_offset(
        NaiveDate::from_ymd_opt(2024, 1, 2)
            .unwrap()
            .and_hms_opt(3, 4, 5)
            .unwrap(),
        Utc,
    );
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("free-source"))
        .expect("source account should be created");
    db.set_account_rate_limit(
        "free-source",
        until,
        "free quota exhausted",
        Some(UsageWindowKind::Free),
    )
    .expect("free rate limit should save");

    let stored = db
        .free_channel_cooldown_until_at(until - Duration::days(1))
        .expect("durable cooldown should load")
        .expect("durable cooldown should be active far before the deadline");
    assert!(
        db.free_channel_cooldown_until_at(stored - Duration::seconds(1))
            .expect("pre-deadline evaluation")
            .is_some(),
        "until > now must keep the durable Free gate closed"
    );
    assert_eq!(
        db.free_channel_cooldown_until_at(stored)
            .expect("exact-deadline evaluation"),
        None,
        "until == now must expire the durable Free gate"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn account_stays_cooling_until_all_windows_expire() {
    let dir = temp_data_dir("multi-window-cooldown");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("multi"))
        .expect("account should be created");

    let now = Utc::now();
    let past_5h = now - Duration::minutes(1);
    let future_week = now + Duration::days(2);
    db.set_account_rate_limit(
        "multi",
        past_5h,
        "5-hour usage limit reached. Resets in 13min.",
        Some(UsageWindowKind::FiveHours),
    )
    .expect("5h rate limit should save");
    db.set_account_rate_limit(
        "multi",
        future_week,
        "weekly usage limit reached. Resets in 4 days.",
        Some(UsageWindowKind::Week),
    )
    .expect("weekly rate limit should save");

    let account = db
        .get_account("multi")
        .expect("account should load")
        .expect("account should exist");
    assert!(account.cooldown_5h_until.is_some_and(|until| until <= now));
    assert!(account.cooldown_week_until.is_some_and(|until| until > now));
    assert!(
        account
            .cooldown_until
            .is_some_and(|until| (until - future_week).num_seconds().abs() < 2)
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn v13_migration_preserves_legacy_manual_usage_calibration() {
    let dir = temp_data_dir("v13-legacy-calibration");
    let db = Database::open(dir.clone()).expect("db should open");
    let mut acct = account("legacy-calibration");
    acct.key_cipher = fixture_account_key_cipher();
    acct.purchase_date = local_today();
    db.create_account(&acct).expect("account should be created");
    finalize_success(&db, "legacy-calibration", 2.0, Utc::now());
    db.conn
        .execute(
            "UPDATE accounts SET
                    usage_5h_baseline_percent = 50,
                    usage_5h_anchor_success_cost = 2,
                    usage_week_baseline_percent = 40,
                    usage_week_anchor_success_cost = 2,
                    usage_month_baseline_percent = 25,
                    usage_month_anchor_success_cost = 2
                 WHERE id = 'legacy-calibration'",
            [],
        )
        .expect("legacy baselines should save");
    finalize_success(&db, "legacy-calibration", 1.0, Utc::now());
    db.conn
        .execute_batch(
            "DELETE FROM schema_version;
                 INSERT INTO schema_version (version) VALUES (10);",
        )
        .expect("legacy schema version should save");
    drop(db);

    let db = open_with_host_cipher(dir.clone()).expect("legacy database should migrate");
    let usage = db
        .account_usage("legacy-calibration")
        .expect("migrated usage should load");
    // Old effective values: 50% * 12 + 1, 40% * 30 + 1,
    // and 25% * 60 + 1. The migration must preserve all three.
    assert_cost(usage.window_5h, 7.0);
    assert_cost(usage.window_week, 13.0);
    assert_cost(usage.window_month, 16.0);

    let (version, remaining_baselines): (i32, i64) = db
        .conn
        .query_row(
            "SELECT
                    (SELECT MAX(version) FROM schema_version),
                    COUNT(*)
                 FROM accounts
                 WHERE usage_5h_baseline_percent IS NOT NULL
                    OR usage_week_baseline_percent IS NOT NULL
                    OR usage_month_baseline_percent IS NOT NULL",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("migration state should load");
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    assert_eq!(remaining_baselines, 0);

    finalize_success(&db, "legacy-calibration", 2.0, Utc::now());
    let usage = db
        .account_usage("legacy-calibration")
        .expect("new usage should accumulate after migration");
    assert_cost(usage.window_5h, 9.0);
    assert_cost(usage.window_week, 15.0);
    assert_cost(usage.window_month, 18.0);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn v14_migrates_v13_logs_and_adds_request_id_indexes() {
    let dir = temp_data_dir("v14-log-diagnostics");
    let db = Database::open(dir.clone()).expect("db should open");
    db.conn
        .execute_batch(
            "DROP INDEX idx_forward_logs_request_id;
                 DROP INDEX idx_gateway_logs_request_id;
                 ALTER TABLE forward_logs DROP COLUMN request_id;
                 ALTER TABLE forward_logs DROP COLUMN attempt;
                 ALTER TABLE forward_logs DROP COLUMN error_source;
                 ALTER TABLE forward_logs DROP COLUMN error_stage;
                 ALTER TABLE forward_logs DROP COLUMN duration_ms;
                 ALTER TABLE forward_logs DROP COLUMN diagnostic_json;
                 ALTER TABLE gateway_logs DROP COLUMN request_id;
                 ALTER TABLE gateway_logs DROP COLUMN attempt;
                 ALTER TABLE gateway_logs DROP COLUMN error_source;
                 ALTER TABLE gateway_logs DROP COLUMN error_stage;
                 ALTER TABLE gateway_logs DROP COLUMN duration_ms;
                 ALTER TABLE gateway_logs DROP COLUMN diagnostic_json;
                 INSERT INTO forward_logs
                    (timestamp, model, account_id, account_name, status, error_message)
                 VALUES ('2026-07-01T00:00:00Z', 'legacy-model', 'legacy', 'Legacy',
                         'client_error', 'legacy error');
                 INSERT INTO gateway_logs (level, category, message, created_at)
                 VALUES ('warn', 'legacy', 'legacy gateway error', '2026-07-01T00:00:00Z');
                 DELETE FROM schema_version;
                 INSERT INTO schema_version (version) VALUES (13);",
        )
        .expect("v13 schema should be prepared");
    drop(db);

    let db = Database::open(dir.clone()).expect("v13 database should migrate");
    let version: i32 = db
        .conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .expect("schema version should load");
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    for index in ["idx_forward_logs_request_id", "idx_gateway_logs_request_id"] {
        let exists: bool = db
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
                [index],
                |row| row.get(0),
            )
            .expect("index state should load");
        assert!(exists, "{index} should exist");
    }
    let forward = db
        .query_forward_logs(ForwardLogQueryOptions {
            limit: 10,
            offset: 0,
            status: None,
            account_id: None,
            provider_id: None,
            offering_id: None,
            route_account_id: None,
            credential_account_id: None,
            model: None,
            key_id: None,
            request_id: None,
            start_time: None,
            end_time: None,
            sort_by: None,
            sort_order: None,
        })
        .expect("legacy forward log should load")
        .items
        .pop()
        .expect("legacy forward log should remain");
    assert_eq!(forward.error_message.as_deref(), Some("legacy error"));
    assert!(forward.request_id.is_none());
    assert!(forward.diagnostic.is_none());
    let gateway = db
        .list_gateway_logs(10)
        .expect("legacy gateway log should load")
        .pop()
        .expect("legacy gateway log should remain");
    assert_eq!(gateway.message, "legacy gateway error");
    assert!(gateway.request_id.is_none());
    assert!(gateway.diagnostic.is_none());

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn v15_migration_adds_nullable_auth_error() {
    let dir = temp_data_dir("v15-auth-error");
    let conn = Connection::open(dir.join("data.sqlite")).expect("legacy db should open");
    conn.execute_batch(
        "CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
             INSERT INTO schema_version (version) VALUES (14);
             CREATE TABLE accounts (id TEXT PRIMARY KEY);
             INSERT INTO accounts (id) VALUES ('legacy');
             CREATE TABLE forward_logs (
                 timestamp TEXT,
                 cost_state TEXT NOT NULL DEFAULT 'not_applicable',
                 diagnostic_json TEXT
             );
             CREATE TABLE gateway_logs (created_at TEXT, diagnostic_json TEXT);",
    )
    .expect("v14 fixture should be created");
    drop(conn);

    let db = Database::open(dir.clone()).expect("v14 database should migrate");
    let (version, auth_error): (i32, Option<String>) = db
        .conn
        .query_row(
            "SELECT (SELECT MAX(version) FROM schema_version), auth_error
                 FROM accounts WHERE id = 'legacy'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("v15 migration state should load");
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    assert!(auth_error.is_none());

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn diagnostic_retention_removes_only_old_json() {
    let dir = temp_data_dir("diagnostic-retention");
    let db = Database::open(dir.clone()).expect("db should open");
    db.conn
        .execute_batch(
            "INSERT INTO forward_logs
                    (timestamp, model, account_id, account_name, status, error_message,
                     request_id, attempt, error_source, error_stage, duration_ms, diagnostic_json)
                 VALUES
                    (datetime('now', '-31 days'), 'old', 'a', 'A', 'client_error', 'keep me',
                     'ocg-old', 1, 'upstream', 'upstream_http', 12, '{\"old\":true}'),
                    (datetime('now', '-29 days'), 'new', 'a', 'A', 'client_error', 'keep new',
                     'ocg-new', 1, 'upstream', 'upstream_http', 13, '{\"new\":true}');
                 INSERT INTO gateway_logs
                    (level, category, message, created_at, request_id, error_source,
                     error_stage, duration_ms, diagnostic_json)
                 VALUES
                    ('warn', 'gateway', 'old gateway', datetime('now', '-31 days'),
                     'ocg-gateway-old', 'client', 'parse', 5, '{\"old\":true}'),
                    ('warn', 'gateway', 'new gateway', datetime('now', '-29 days'),
                     'ocg-gateway-new', 'client', 'parse', 6, '{\"new\":true}');",
        )
        .expect("diagnostic rows should insert");
    drop(db);

    let db = Database::open(dir.clone()).expect("db reopen should apply retention");
    let (old_detail, old_id, old_error, old_source): (Option<String>, String, String, String) = db
        .conn
        .query_row(
            "SELECT diagnostic_json, request_id, error_message, error_source
                 FROM forward_logs WHERE request_id='ocg-old'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("old row should remain");
    assert!(old_detail.is_none());
    assert_eq!(old_id, "ocg-old");
    assert_eq!(old_error, "keep me");
    assert_eq!(old_source, "upstream");
    let new_detail: Option<String> = db
        .conn
        .query_row(
            "SELECT diagnostic_json FROM forward_logs WHERE request_id='ocg-new'",
            [],
            |row| row.get(0),
        )
        .expect("new detail should load");
    assert!(new_detail.is_some());
    let gateway_details: (Option<String>, Option<String>) = db
        .conn
        .query_row(
            "SELECT
                    (SELECT diagnostic_json FROM gateway_logs WHERE request_id='ocg-gateway-old'),
                    (SELECT diagnostic_json FROM gateway_logs WHERE request_id='ocg-gateway-new')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("gateway details should load");
    assert!(gateway_details.0.is_none());
    assert!(gateway_details.1.is_some());

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn fixed_window_5h_starts_at_first_success_and_expires_after_5h() {
    let dir = temp_data_dir("fixed-5h");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("fixed"))
        .expect("account should be created");

    // 第一条成功请求落在 4h 前：固定窗口起点 = 4h 前，倒计时 ≈ 1h
    let ts1 = Utc::now() - Duration::hours(4);
    finalize_success(&db, "fixed", 1.0, ts1);
    // 窗口内的第二条请求：累加
    let ts2 = ts1 + Duration::hours(1);
    finalize_success(&db, "fixed", 2.0, ts2);

    let usage = db.account_usage("fixed").expect("usage should load");
    assert_cost(usage.window_5h, 3.0);
    let reset = usage
        .resets_in_5h
        .expect("5h window reset should be set while window is active");
    let remaining_min = (reset - Utc::now()).num_minutes();
    assert!(
        (55..=65).contains(&remaining_min),
        "expected ~60min remaining, got {remaining_min}"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn fixed_window_treats_exact_end_as_the_next_window_start() {
    let dir = temp_data_dir("fixed-boundary");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("boundary"))
        .expect("account should be created");

    let first = Utc::now() - Duration::hours(5) - Duration::minutes(1);
    let exact_end = first + Duration::hours(5);
    finalize_success(&db, "boundary", 10.0, first);
    finalize_success(&db, "boundary", 2.0, exact_end);

    let usage = db.account_usage("boundary").expect("usage should load");
    assert_cost(usage.window_5h, 2.0);
    assert!(usage.resets_in_5h.is_some());

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn fixed_window_5h_rebuilds_after_expiry_when_new_request_arrives() {
    let dir = temp_data_dir("fixed-5h-rebuild");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("rebuild"))
        .expect("account should be created");

    // 6h 前的第一条请求：窗口已过期
    let ts1 = Utc::now() - Duration::hours(6);
    finalize_success(&db, "rebuild", 10.0, ts1);
    // 1h 前的第二条请求：触发新窗口
    let ts2 = Utc::now() - Duration::hours(1);
    finalize_success(&db, "rebuild", 5.0, ts2);

    let usage = db.account_usage("rebuild").expect("usage should load");
    // 新窗口只包含 ts2 之后：10 已被丢弃，只剩 5
    assert_cost(usage.window_5h, 5.0);
    let reset = usage
        .resets_in_5h
        .expect("5h window reset should be set after rebuild");
    let remaining_min = (reset - Utc::now()).num_minutes();
    assert!(
        (235..=245).contains(&remaining_min),
        "expected ~240min remaining, got {remaining_min}"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn fixed_window_5h_advances_through_multiple_expired_windows_in_one_call() {
    // 复现用户报告的"刷新递减"循环 bug：
    //   4 条间隔 6h 的计费日志（全部已过期）。
    // 旧实现每次刷新只前进一个窗口，前端可见 60→30→13→5.8→0→60+ 循环；
    // 修复后一次调用内连过 4 个过期窗口，next=None 时清空并返回 0，
    // 第二次刷新仍为 0，不再回到最旧日志。
    let dir = temp_data_dir("fixed-5h-multi-expired");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("cycle"))
        .expect("account should be created");

    // ts1 = -24h, ts2 = -18h, ts3 = -12h, ts4 = -6h：每条间隔 6h（> 5h 窗口长度）。
    let ts1 = Utc::now() - Duration::hours(24);
    let ts2 = ts1 + Duration::hours(6);
    let ts3 = ts2 + Duration::hours(6);
    let ts4 = ts3 + Duration::hours(6);
    finalize_success(&db, "cycle", 10.0, ts1);
    finalize_success(&db, "cycle", 5.0, ts2);
    finalize_success(&db, "cycle", 3.0, ts3);
    finalize_success(&db, "cycle", 2.0, ts4);

    // 第一次刷新：应直接走完所有过期窗口，返回 0（无新请求）。
    let usage = db.account_usage("cycle").expect("usage should load");
    assert_cost(usage.window_5h, 0.0);
    assert!(
        usage.resets_in_5h.is_none(),
        "no active window after all expired; resets_in_5h should be None"
    );

    // 第二次刷新：不应回到最旧日志循环重放，仍稳定为 0。
    let usage2 = db.account_usage("cycle").expect("usage should load again");
    assert_cost(usage2.window_5h, 0.0);
    assert!(usage2.resets_in_5h.is_none());

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn fixed_window_5h_finds_active_window_after_multiple_expired() {
    // 多条已过期日志后跟一条近期日志：修复后第一次刷新就应落在有效窗口上，
    // 而不是停在某个过期窗口里返回错误的中间值。
    let dir = temp_data_dir("fixed-5h-active-after-expired");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("active"))
        .expect("account should be created");

    // 三条过期日志间隔 6h，再加一条 1h 前的近期日志。
    let ts1 = Utc::now() - Duration::hours(19);
    let ts2 = ts1 + Duration::hours(6); // -13h
    let ts3 = ts2 + Duration::hours(6); // -7h，仍过期
    let ts4 = Utc::now() - Duration::hours(1); // 近期，落在有效窗口内
    finalize_success(&db, "active", 10.0, ts1);
    finalize_success(&db, "active", 5.0, ts2);
    finalize_success(&db, "active", 3.0, ts3);
    finalize_success(&db, "active", 2.0, ts4);

    // 第一次刷新：连过 3 个过期窗口，落在 ts4 上，只算 ts4 之后的 cost = 2.0。
    let usage = db.account_usage("active").expect("usage should load");
    assert_cost(usage.window_5h, 2.0);
    let reset = usage
        .resets_in_5h
        .expect("5h window reset should be anchored at ts4");
    let remaining_min = (reset - Utc::now()).num_minutes();
    assert!(
        (235..=245).contains(&remaining_min),
        "expected ~240min remaining (anchored at ts4 = now - 1h), got {remaining_min}"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn fixed_window_5h_with_no_usage_returns_zero_and_full_window_remaining() {
    let dir = temp_data_dir("fixed-5h-empty");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("empty"))
        .expect("account should be created");

    let usage = db.account_usage("empty").expect("usage should load");
    assert_cost(usage.window_5h, 0.0);
    // 没用过：倒计时为 None（前端显示"5h0min"由默认值决定）
    assert!(usage.resets_in_5h.is_none());

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn month_window_accumulates_from_purchase_date_to_expires_on() {
    let dir = temp_data_dir("month-window");
    let db = Database::open(dir.clone()).expect("db should open");
    let mut acct = account("monthly");
    acct.purchase_date = "2026-07-01".into();
    db.create_account(&acct).expect("account should be created");

    // 模拟一条历史成功请求（任何时间都算，月窗口从 purchase_date 累计）
    finalize_success(&db, "monthly", 5.0, Utc::now());

    let usage = db.account_usage("monthly").expect("usage should load");
    assert_cost(usage.window_month, 5.0);
    let reset = usage
        .resets_in_month
        .expect("month window reset should be purchase_date + 1 month");
    // 2026-07-01 + 1 自然月 = 2026-08-01 00:00
    let expected = DateTime::parse_from_rfc3339("2026-08-01T00:00:00+00:00")
        .unwrap()
        .with_timezone(&Utc);
    assert!(
        (reset - expected).num_seconds().abs() < 86400,
        "expected ~2026-08-01, got {reset}"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn manual_calibrate_5h_window_sets_started_at_and_cost_offset() {
    let dir = temp_data_dir("calibrate-5h");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("calib"))
        .expect("account should be created");

    // 用户在别处已用 50%，距上游重置还剩 3 小时
    db.calibrate_account_usage("calib", UsageWindowKind::FiveHours, 50.0, Some(180), 12.0)
        .expect("calibrate should save");

    let usage = db.account_usage("calib").expect("usage should load");
    // 5h 限额 12.0，50% = 6.0
    assert_cost(usage.window_5h, 6.0);
    let reset = usage
        .resets_in_5h
        .expect("5h window reset should be set after manual calibrate");
    let remaining_min = (reset - Utc::now()).num_minutes();
    assert!(
        (175..=185).contains(&remaining_min),
        "expected ~180min remaining, got {remaining_min}"
    );

    // 后续网关内的请求累加到偏移之上
    finalize_success(&db, "calib", 1.0, Utc::now());
    let usage = db.account_usage("calib").expect("usage should reload");
    assert_cost(usage.window_5h, 7.0);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn calibrate_subtracts_existing_window_usage_from_offset() {
    // 回归测试：活跃账号（窗口内已有 forward_logs）校准时，
    // offset 必须 = target_cost - actual_cost，否则 compute_fixed_window
    // 返回 offset + actual_cost，显示百分比会高于用户输入。
    let dir = temp_data_dir("calibrate-with-usage");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("active"))
        .expect("account should be created");

    // 1 小时前已用 $3（落在 5h 窗口内）
    let ts = Utc::now() - Duration::hours(1);
    finalize_success(&db, "active", 3.0, ts);

    // 用户说"我在别处用到了 50%"（5h 限额 12.0 → target_cost = 6.0）
    // 期望：offset = 6.0 - 3.0 = 3.0，compute_fixed_window 返回 3.0 + 3.0 = 6.0 = 50%
    // 修复前 bug：offset = 6.0，compute_fixed_window 返回 6.0 + 3.0 = 9.0 = 75%
    // 用 resets_in_minutes=180 让新窗口的 started_at = now + 3h - 5h = now - 2h，
    // 把 1 小时前的 log 稳稳包含进窗口（避开 finalize 与 calibrate 之间的微秒级时序差）。
    db.calibrate_account_usage("active", UsageWindowKind::FiveHours, 50.0, Some(180), 12.0)
        .expect("calibrate should save with existing usage");
    let usage = db.account_usage("active").expect("usage should load");
    assert_cost(usage.window_5h, 6.0);

    // 后续请求继续累加：offset=3.0 + actual=3.0 + new=2.0 = 8.0
    finalize_success(&db, "active", 2.0, Utc::now());
    let usage = db.account_usage("active").expect("usage should reload");
    assert_cost(usage.window_5h, 8.0);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn calibrate_below_actual_usage_allows_negative_offset() {
    // 回归测试（Bug 1.5）：用户校准的百分比低于窗口内实际 cost 时，offset 允许为负数，
    // 让 compute_fixed_window 返回 offset + actual = target_cost，与用户输入一致。
    // 之前 max(0, target - actual) 钳制 + schema CHECK (offset >= 0) 约束让向左拉
    // 滑块时被锁死在实际 cost 对应的百分比（9.0 / 12.0 * 100 = 75%，对应用户看到的 40.2%）。
    let dir = temp_data_dir("calibrate-below-usage");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("clamp"))
        .expect("account should be created");

    // 已用 $9
    let ts = Utc::now() - Duration::hours(1);
    finalize_success(&db, "clamp", 9.0, ts);

    // 用户校准到 20%（target_cost = 2.4，但实际已用 9.0）
    // offset = 2.4 - 9.0 = -6.6；compute_fixed_window 返回 -6.6 + 9.0 = 2.4 = 20%。
    // 用 resets_in_minutes=180 让新窗口的 started_at = now - 2h，把 1 小时前的
    // $9 log 稳稳包含进窗口（避开 finalize 与 calibrate 之间的微秒级时序差）。
    db.calibrate_account_usage("clamp", UsageWindowKind::FiveHours, 20.0, Some(180), 12.0)
        .expect("calibrate below actual usage should allow negative offset");
    let usage = db.account_usage("clamp").expect("usage should load");
    // 显示的 cost = offset(-6.6) + actual(9.0) = 2.4（用户输入的 20%）
    assert_cost(usage.window_5h, 2.4);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn calibrate_month_window_writes_offset_without_started_at() {
    // 回归测试（Bug 2）：月窗口必须支持手动校准。
    // 月窗口不写 started_at 列（起点固定为 purchase_date），只更新 cost_offset。
    // resets_in_minutes 被忽略——窗口由 purchase_date/expires_on 决定。
    let dir = temp_data_dir("calibrate-month");
    let db = Database::open(dir.clone()).expect("db should open");
    let mut acct = account("monthly-calib");
    acct.purchase_date = "2026-07-01".into();
    db.create_account(&acct).expect("account should be created");

    // 已用 $5（落在月窗口内：purchase_date 00:00 起）
    finalize_success(&db, "monthly-calib", 5.0, Utc::now());

    // 用户校准到 50%（月限额 100.0 → target_cost = 50.0）
    // 期望：offset = 50.0 - 5.0 = 45.0；compute_month_window 返回 45.0 + 5.0 = 50.0 = 50%。
    db.calibrate_account_usage("monthly-calib", UsageWindowKind::Month, 50.0, None, 100.0)
        .expect("month window calibrate should save");
    let usage = db
        .account_usage("monthly-calib")
        .expect("usage should load");
    assert_cost(usage.window_month, 50.0);
    // resets_in_month 仍是 purchase_date + 1 自然月（不受 resets_in_minutes 影响）
    let reset = usage
        .resets_in_month
        .expect("month window reset should be purchase_date + 1 month");
    let expected = DateTime::parse_from_rfc3339("2026-08-01T00:00:00+00:00")
        .unwrap()
        .with_timezone(&Utc);
    assert!(
        (reset - expected).num_seconds().abs() < 86400,
        "expected ~2026-08-01, got {reset}"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn changing_purchase_date_resets_month_calibration_offset() {
    let dir = temp_data_dir("month-renewal-reset");
    let db = Database::open(dir.clone()).expect("db should open");
    let new_purchase_date = local_today();
    let old_purchase_date = (Local::now().date_naive() - Duration::days(10))
        .format("%Y-%m-%d")
        .to_string();
    let mut acct = account("monthly-renewal");
    acct.purchase_date = old_purchase_date;
    db.create_account(&acct).expect("account should be created");

    finalize_success(&db, "monthly-renewal", 5.0, Utc::now() - Duration::days(2));
    db.calibrate_account_usage("monthly-renewal", UsageWindowKind::Month, 0.0, None, 100.0)
        .expect("month calibration should save a negative offset");
    assert_cost(
        db.account_usage("monthly-renewal")
            .expect("usage should load")
            .window_month,
        0.0,
    );

    db.update_account(
        "monthly-renewal",
        &AccountUpdate {
            name: None,
            username: None,
            password: None,
            key: None,
            enabled: None,
            referral_code: None,
            purchase_date: Some(new_purchase_date),
            notes: None,
        },
        None,
        None,
    )
    .expect("purchase date should update");
    let offset: f64 = db
        .conn
        .query_row(
            "SELECT usage_month_window_cost_offset FROM accounts WHERE id = ?1",
            ["monthly-renewal"],
            |row| row.get(0),
        )
        .expect("month offset should load");
    assert_cost(offset, 0.0);
    assert_cost(
        db.account_usage("monthly-renewal")
            .expect("renewed usage should load")
            .window_month,
        0.0,
    );

    finalize_success(&db, "monthly-renewal", 2.0, Utc::now());
    assert_cost(
        db.account_usage("monthly-renewal")
            .expect("new cycle usage should load")
            .window_month,
        2.0,
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn replacing_key_clears_auth_error_but_other_updates_preserve_it() {
    let dir = temp_data_dir("auth-error-key-replacement");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("auth-failed"))
        .expect("account should be created");
    let old_key_cipher = db
        .get_account("auth-failed")
        .expect("account should load")
        .expect("account should exist")
        .key_cipher;
    db.set_account_auth_error("auth-failed", Some("upstream auth error 401"))
        .expect("auth error should save");

    let rename = AccountUpdate {
        name: Some("renamed".into()),
        username: None,
        password: None,
        key: None,
        enabled: None,
        referral_code: None,
        purchase_date: None,
        notes: None,
    };
    db.update_account("auth-failed", &rename, None, None)
        .expect("non-key update should save");
    assert!(
        db.get_account("auth-failed")
            .expect("account should load")
            .expect("account should exist")
            .auth_error
            .is_some()
    );

    let no_fields = AccountUpdate {
        name: None,
        username: None,
        password: None,
        key: None,
        enabled: None,
        referral_code: None,
        purchase_date: None,
        notes: None,
    };
    db.update_account("auth-failed", &no_fields, Some("replacement-cipher"), None)
        .expect("key replacement should save");
    assert!(
        db.get_account("auth-failed")
            .expect("account should load")
            .expect("account should exist")
            .auth_error
            .is_none()
    );

    assert!(
        !db.set_account_auth_error_if_key_matches(
            "auth-failed",
            &old_key_cipher,
            Some("late old-key 401"),
        )
        .expect("stale auth response should be ignored")
    );
    assert!(
        db.get_account("auth-failed")
            .expect("account should load")
            .expect("account should exist")
            .auth_error
            .is_none(),
        "a delayed 401 from the old key must not break its replacement"
    );

    assert!(
        db.set_account_auth_error_if_key_matches(
            "auth-failed",
            "replacement-cipher",
            Some("new-key auth error"),
        )
        .expect("current-key auth response should save")
    );
    assert!(
        !db.set_account_auth_error_if_key_matches("auth-failed", &old_key_cipher, None)
            .expect("stale success response should be ignored")
    );
    assert_eq!(
        db.get_account("auth-failed")
            .expect("account should load")
            .expect("account should exist")
            .auth_error
            .as_deref(),
        Some("new-key auth error"),
        "a delayed success from the old key must not recover its replacement"
    );
    assert!(
        db.set_account_auth_error_if_key_matches("auth-failed", "replacement-cipher", None)
            .expect("current-key success should clear auth state")
    );

    let stale_cooldown = Utc::now() + Duration::days(3);
    assert!(
        !db.set_account_rate_limit_if_key_matches(
            "auth-failed",
            &old_key_cipher,
            stale_cooldown,
            "late old-key 429",
            None,
        )
        .expect("stale rate limit should be ignored")
    );
    let stored = db
        .get_account("auth-failed")
        .expect("account should load")
        .expect("account should exist");
    assert!(stored.cooldown_until.is_none());
    assert!(stored.last_error.is_none());

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn calibrate_rejects_reset_outside_fixed_window_without_panicking() {
    let dir = temp_data_dir("calibrate-reset-bounds");
    let db = Database::open(dir.clone()).expect("db should open");
    db.create_account(&account("reset-bounds"))
        .expect("account should be created");

    for (window, minutes) in [
        (UsageWindowKind::FiveHours, -1),
        (UsageWindowKind::FiveHours, 301),
        (UsageWindowKind::Week, 10_081),
        (UsageWindowKind::FiveHours, i64::MAX),
    ] {
        assert!(
            db.calibrate_account_usage("reset-bounds", window, 50.0, Some(minutes), 100.0,)
                .is_err(),
            "{window:?} should reject {minutes} minutes"
        );
    }

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

fn snapshot_limits() -> PricingLimits {
    PricingLimits {
        window_5h: 12.0,
        window_week: 30.0,
        window_month: 100.0,
    }
}

fn usage_calibration(
    rolling_percent: f64,
    weekly_percent: f64,
    monthly_percent: f64,
    rolling_resets_in_minutes: i64,
    weekly_resets_in_minutes: i64,
) -> AccountUsageCalibrationSnapshot {
    AccountUsageCalibrationSnapshot {
        rolling_percent,
        weekly_percent,
        monthly_percent,
        rolling_resets_in_minutes,
        weekly_resets_in_minutes,
    }
}

fn usage_offset_row(db: &Database, id: &str) -> (Option<String>, f64, Option<String>, f64, f64) {
    db.conn
        .query_row(
            "SELECT usage_5h_window_started_at, usage_5h_window_cost_offset,
                        usage_week_window_started_at, usage_week_window_cost_offset,
                        usage_month_window_cost_offset
                 FROM accounts WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .expect("usage offset row should load")
}

#[test]
fn calibrate_account_usage_snapshot_updates_all_three_windows() {
    let dir = temp_data_dir("calibrate-snapshot-ok");
    let db = Database::open(dir.clone()).expect("db should open");
    let mut acct = account("snap-ok");
    acct.purchase_date = "2026-07-01".into();
    db.create_account(&acct).expect("account should be created");
    finalize_success(&db, "snap-ok", 3.0, Utc::now() - Duration::hours(1));

    let limits = snapshot_limits();
    let usage = db
        .calibrate_account_usage_snapshot(
            "snap-ok",
            &usage_calibration(50.0, 20.0, 10.0, 180, 1_440),
            &limits,
        )
        .expect("snapshot calibrate should save");
    assert_cost(usage.window_5h, 6.0);
    assert_cost(usage.window_week, 6.0);
    assert_cost(usage.window_month, 10.0);
    let remaining_5h =
        (usage.resets_in_5h.expect("5h reset should be set") - Utc::now()).num_minutes();
    assert!(
        (175..=185).contains(&remaining_5h),
        "expected ~180min remaining, got {remaining_5h}"
    );
    let remaining_week =
        (usage.resets_in_week.expect("week reset should be set") - Utc::now()).num_minutes();
    assert!(
        (1_435..=1_445).contains(&remaining_week),
        "expected ~1440min remaining, got {remaining_week}"
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn official_usage_sync_rolls_back_baseline_when_success_metadata_fails() {
    let dir = temp_data_dir("official-sync-atomic-failure");
    let db = Database::open(dir.clone()).expect("db should open");
    let mut acct = account("atomic-sync");
    acct.purchase_date = "2026-07-01".into();
    db.create_account(&acct).expect("account should be created");
    let limits = snapshot_limits();
    db.calibrate_account_usage_snapshot(
        "atomic-sync",
        &usage_calibration(10.0, 20.0, 30.0, 120, 1_200),
        &limits,
    )
    .expect("initial baseline should save");
    let previous_success = Utc::now() - Duration::hours(2);
    db.record_account_usage_sync_success(
        "atomic-sync",
        previous_success,
        previous_success + Duration::hours(24),
        false,
    )
    .expect("initial sync metadata should save");
    let before = db
        .account_usage_with_limits("atomic-sync", &limits)
        .expect("initial usage should load");
    let sync_before = db
        .account_usage_sync_state("atomic-sync")
        .expect("initial sync state should load")
        .expect("sync state should exist");

    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_official_sync_metadata
                 BEFORE UPDATE OF last_success_at ON provider_usage_sync_state
                 WHEN NEW.account_id = 'atomic-sync'
                 BEGIN
                    SELECT RAISE(ABORT, 'forced usage sync metadata failure');
                 END;",
        )
        .expect("failure trigger should install");

    let now = Utc::now();
    let result = db.commit_official_usage_sync_success(
        "atomic-sync",
        "cipher",
        &usage_calibration(80.0, 70.0, 60.0, 180, 1_440),
        &limits,
        AccountUsageSyncSuccessMetadata {
            now,
            next_eligible_at: now + Duration::hours(1),
            mark_expedited: true,
        },
    );
    assert!(
        result.is_err(),
        "forced metadata failure must abort the sync"
    );

    let after = db
        .account_usage_with_limits("atomic-sync", &limits)
        .expect("usage should remain readable");
    assert_cost(after.window_5h, before.window_5h);
    assert_cost(after.window_week, before.window_week);
    assert_cost(after.window_month, before.window_month);
    let sync_after = db
        .account_usage_sync_state("atomic-sync")
        .expect("sync state should load")
        .expect("sync state should exist");
    assert_eq!(sync_after.last_success_at, sync_before.last_success_at);
    assert_eq!(sync_after.next_eligible_at, sync_before.next_eligible_at);
    assert_eq!(sync_after.failure_streak, sync_before.failure_streak);
    assert_eq!(sync_after.last_expedited_at, sync_before.last_expedited_at);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn calibrate_account_usage_snapshot_rolls_back_when_second_window_fails() {
    let dir = temp_data_dir("calibrate-snapshot-week-fail");
    let db = Database::open(dir.clone()).expect("db should open");
    let mut acct = account("snap-week");
    acct.purchase_date = "2026-07-01".into();
    db.create_account(&acct).expect("account should be created");
    let limits = snapshot_limits();
    db.calibrate_account_usage_snapshot(
        "snap-week",
        &usage_calibration(10.0, 20.0, 30.0, 100, 200),
        &limits,
    )
    .expect("initial snapshot should save");
    let before = usage_offset_row(&db, "snap-week");
    let before_usage = db
        .account_usage_with_limits("snap-week", &limits)
        .expect("usage should load");

    assert!(
        db.calibrate_account_usage_snapshot(
            "snap-week",
            &usage_calibration(80.0, 90.0, 40.0, 180, 10_081),
            &limits
        )
        .is_err(),
        "weekly minutes outside the 7-day window should fail"
    );

    assert_eq!(usage_offset_row(&db, "snap-week"), before);
    let after = db
        .account_usage_with_limits("snap-week", &limits)
        .expect("usage should reload");
    assert_cost(after.window_5h, before_usage.window_5h);
    assert_cost(after.window_week, before_usage.window_week);
    assert_cost(after.window_month, before_usage.window_month);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn calibrate_account_usage_snapshot_rolls_back_when_third_window_fails() {
    let dir = temp_data_dir("calibrate-snapshot-month-fail");
    let db = Database::open(dir.clone()).expect("db should open");
    let mut acct = account("snap-month");
    acct.purchase_date = "2026-07-01".into();
    db.create_account(&acct).expect("account should be created");
    let limits = snapshot_limits();
    db.calibrate_account_usage_snapshot(
        "snap-month",
        &usage_calibration(10.0, 20.0, 30.0, 100, 200),
        &limits,
    )
    .expect("initial snapshot should save");
    let before = usage_offset_row(&db, "snap-month");
    let before_usage = db
        .account_usage_with_limits("snap-month", &limits)
        .expect("usage should load");

    db.conn
        .execute_batch(
            "CREATE TRIGGER reject_month_calibrate
                 BEFORE UPDATE OF usage_month_window_cost_offset ON accounts
                 WHEN NEW.id = 'snap-month'
                 BEGIN
                     SELECT RAISE(ABORT, 'forced month calibrate failure');
                 END;",
        )
        .expect("failure trigger should be installed");

    assert!(
        db.calibrate_account_usage_snapshot(
            "snap-month",
            &usage_calibration(80.0, 90.0, 40.0, 180, 1_440),
            &limits
        )
        .is_err(),
        "month window trigger should fail the transaction"
    );

    assert_eq!(usage_offset_row(&db, "snap-month"), before);
    let after = db
        .account_usage_with_limits("snap-month", &limits)
        .expect("usage should reload");
    assert_cost(after.window_5h, before_usage.window_5h);
    assert_cost(after.window_week, before_usage.window_week);
    assert_cost(after.window_month, before_usage.window_month);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn soonest_reset_is_minimum_of_each_accounts_latest_active_cooldown() {
    let dir = temp_data_dir("soonest-account-reset");
    let db = Database::open(dir.clone()).expect("db should open");
    for id in ["first", "second"] {
        db.create_account(&account(id))
            .expect("account should be created");
    }

    let now = Utc::now();
    let first_early = now + Duration::hours(1);
    let first_latest = now + Duration::hours(4);
    let second_latest = now + Duration::hours(2);
    db.set_account_rate_limit(
        "first",
        first_early,
        "5-hour usage limit reached",
        Some(UsageWindowKind::FiveHours),
    )
    .expect("first short cooldown should save");
    db.set_account_rate_limit(
        "first",
        first_latest,
        "weekly usage limit reached",
        Some(UsageWindowKind::Week),
    )
    .expect("first long cooldown should save");
    db.set_account_rate_limit("second", second_latest, "unknown rate limit", None)
        .expect("second cooldown should save");

    let reset = db
        .soonest_cooldown_reset()
        .expect("reset query should work")
        .expect("a reset should exist");
    assert!((reset - second_latest).num_seconds().abs() < 2);

    db.set_account_auth_error("second", Some("upstream auth error 401"))
        .expect("auth breaker should save");
    let reset = db
        .soonest_cooldown_reset()
        .expect("reset query should work")
        .expect("an eligible reset should exist");
    assert!((reset - first_latest).num_seconds().abs() < 2);

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn forward_log_time_filter_compares_rfc3339_offsets_by_instant() {
    let dir = temp_data_dir("forward-log-offset-filter");
    let db = Database::open(dir.clone()).expect("db should open");
    db.conn
        .execute(
            "INSERT INTO forward_logs
                 (timestamp, model, account_id, account_name, status, cost)
                 VALUES (?1, 'inside', 'a', 'a', 'success', 1)",
            ["2026-07-17T04:15:00Z"],
        )
        .expect("inside log should save");
    db.conn
        .execute(
            "INSERT INTO forward_logs
                 (timestamp, model, account_id, account_name, status, cost)
                 VALUES (?1, 'outside', 'a', 'a', 'success', 2)",
            ["2026-07-17T03:30:00Z"],
        )
        .expect("outside log should save");

    let page = db
        .query_forward_logs(ForwardLogQueryOptions {
            limit: 20,
            offset: 0,
            status: None,
            account_id: None,
            provider_id: None,
            offering_id: None,
            route_account_id: None,
            credential_account_id: None,
            model: None,
            key_id: None,
            request_id: None,
            start_time: Some("2026-07-17T12:00:00+08:00"),
            end_time: Some("2026-07-17T12:30:00+08:00"),
            sort_by: Some("cost"),
            sort_order: Some("asc"),
        })
        .expect("offset filter should query");
    assert_eq!(page.summary.total_requests, 1);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].model, "inside");

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn forward_logs_can_sort_by_attempt() {
    let dir = temp_data_dir("forward-log-attempt-sort");
    let db = Database::open(dir.clone()).expect("db should open");
    for attempt in [2, 1] {
        db.conn
            .execute(
                "INSERT INTO forward_logs
                     (timestamp, model, account_id, account_name, status, cost, attempt)
                     VALUES ('2026-07-23T00:00:00Z', ?1, 'a', 'a', 'client_error', 0, ?2)",
                params![format!("attempt-{attempt}"), attempt],
            )
            .expect("forward log should save");
    }

    let page = db
        .query_forward_logs(ForwardLogQueryOptions {
            limit: 20,
            offset: 0,
            status: None,
            account_id: None,
            provider_id: None,
            offering_id: None,
            route_account_id: None,
            credential_account_id: None,
            model: None,
            key_id: None,
            request_id: None,
            start_time: None,
            end_time: None,
            sort_by: Some("attempt"),
            sort_order: Some("asc"),
        })
        .expect("attempt sort should query");
    assert_eq!(
        page.items
            .iter()
            .filter_map(|log| log.attempt)
            .collect::<Vec<_>>(),
        [1, 2]
    );

    drop(db);
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

fn attributed_log(account_id: &str, key_id: Option<&str>, cost: f64) -> ForwardLog {
    let mut log = forward_log(account_id, "success", cost);
    log.client_key_id = key_id.map(str::to_string);
    log.client_key_name = key_id.map(|id| format!("Key-{id}"));
    log
}

#[test]
fn forward_logs_filter_by_key_and_unattributed_sentinel() {
    let dir = temp_data_dir("forward-key-filter");
    let db = Database::open(dir.clone()).unwrap();
    db.log_forward(&attributed_log("acct", Some("key-a"), 1.0))
        .unwrap();
    db.log_forward(&attributed_log("acct", Some("key-b"), 2.0))
        .unwrap();
    db.log_forward(&attributed_log("acct", None, 4.0)).unwrap();

    let query = |key_id: Option<&str>| {
        db.query_forward_logs(ForwardLogQueryOptions {
            limit: 50,
            offset: 0,
            status: None,
            account_id: None,
            provider_id: None,
            offering_id: None,
            route_account_id: None,
            credential_account_id: None,
            model: None,
            key_id,
            request_id: None,
            start_time: None,
            end_time: None,
            sort_by: Some("cost"),
            sort_order: Some("asc"),
        })
        .unwrap()
    };

    let all = query(None);
    assert_eq!(all.summary.total_requests, 3);
    assert_eq!(all.items.len(), 3);

    let key_a = query(Some("key-a"));
    assert_eq!(key_a.summary.total_requests, 1);
    assert_eq!(key_a.summary.cost, 1.0);
    assert_eq!(key_a.items[0].client_key_id.as_deref(), Some("key-a"));
    assert_eq!(key_a.items[0].client_key_name.as_deref(), Some("Key-key-a"));

    let unattributed = query(Some(UNATTRIBUTED_KEY_FILTER));
    assert_eq!(unattributed.summary.total_requests, 1);
    assert_eq!(unattributed.summary.cost, 4.0);
    assert!(unattributed.items[0].client_key_id.is_none());

    let keys = db.list_forward_log_keys().unwrap();
    assert_eq!(keys.len(), 2);
    assert!(
        keys.iter()
            .any(|key| key.id == "key-a" && key.name == "Key-key-a")
    );
    assert!(keys.iter().any(|key| key.id == "key-b"));

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn forward_logs_filter_by_provider_attribution_before_pagination() {
    let dir = temp_data_dir("forward-provider-filter");
    let db = Database::open(dir.clone()).unwrap();
    let insert = |model: &str,
                  provider_id: &str,
                  offering_id: &str,
                  route_account_id: &str,
                  credential_account_id: &str| {
        let mut log = forward_log(credential_account_id, "success", 1.0);
        log.model = model.into();
        log.provider_id = Some(provider_id.into());
        log.offering_id = Some(offering_id.into());
        log.route_account_id = Some(route_account_id.into());
        log.credential_account_id = Some(credential_account_id.into());
        db.log_forward(&log).unwrap();
    };

    insert("go-a", "opencode", "go", "go-a", "go-a");
    insert("go-b", "opencode", "go", "go-b", "go-b");
    // A Zen route may deliberately debit an OpenCode credential account.
    insert("zen", "opencode", "zen-free", "zen-free", "go-a");
    // These newer rows would hide OpenCode Go rows if filtering happened
    // after LIMIT/OFFSET.
    insert("goat-a", "goat", "goat", "goat-a", "goat-a");
    insert("goat-b", "goat", "goat", "goat-b", "goat-b");

    let query = |limit: i64,
                 offset: i64,
                 provider_id: Option<&str>,
                 offering_id: Option<&str>,
                 route_account_id: Option<&str>,
                 credential_account_id: Option<&str>| {
        db.query_forward_logs(ForwardLogQueryOptions {
            limit,
            offset,
            status: None,
            account_id: None,
            provider_id,
            offering_id,
            route_account_id,
            credential_account_id,
            model: None,
            key_id: None,
            request_id: None,
            start_time: None,
            end_time: None,
            sort_by: None,
            sort_order: None,
        })
        .unwrap()
    };

    let first_go = query(1, 0, Some("opencode"), Some("go"), None, None);
    assert_eq!(first_go.summary.total_requests, 2);
    assert_eq!(first_go.items[0].model, "go-b");
    let second_go = query(1, 1, Some("opencode"), Some("go"), None, None);
    assert_eq!(second_go.summary.total_requests, 2);
    assert_eq!(second_go.items[0].model, "go-a");

    let routed_zen = query(10, 0, Some("opencode"), None, Some("zen-free"), None);
    assert_eq!(routed_zen.summary.total_requests, 1);
    assert_eq!(routed_zen.items[0].model, "zen");
    assert_eq!(
        routed_zen.items[0].credential_account_id.as_deref(),
        Some("go-a")
    );

    let credential_go_a = query(10, 0, None, None, None, Some("go-a"));
    assert_eq!(credential_go_a.summary.total_requests, 2);
    assert_eq!(
        credential_go_a
            .items
            .iter()
            .map(|log| log.model.as_str())
            .collect::<Vec<_>>(),
        ["zen", "go-a"]
    );

    let goat = query(10, 0, Some("goat"), None, None, None);
    assert_eq!(goat.summary.total_requests, 2);
    assert_eq!(
        goat.items
            .iter()
            .map(|log| log.route_account_id.as_deref())
            .collect::<Vec<_>>(),
        [Some("goat-b"), Some("goat-a")]
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

fn empty_forward_query<'a>() -> ForwardLogQueryOptions<'a> {
    ForwardLogQueryOptions {
        limit: 50,
        offset: 0,
        status: None,
        account_id: None,
        provider_id: None,
        offering_id: None,
        route_account_id: None,
        credential_account_id: None,
        model: None,
        request_id: None,
        start_time: None,
        end_time: None,
        sort_by: None,
        sort_order: None,
        key_id: None,
    }
}

fn insert_identity_log(
    db: &Database,
    log: ForwardLog,
    requested: Option<&str>,
    alias: Option<&str>,
    upstream: Option<&str>,
) -> i64 {
    let id = db.log_forward(&log).unwrap();
    db.set_forward_log_native_attribution(
        id,
        &ForwardLogNativeAttribution {
            requested_model: requested.map(str::to_string),
            resolved_alias: alias.map(str::to_string),
            upstream_model: upstream.map(str::to_string),
            native_cost_value: None,
            native_cost_unit: None,
            native_cost_currency: None,
        },
    )
    .unwrap();
    id
}

fn clear_v23_identity(db: &Database, id: i64) {
    db.conn
        .execute(
            "UPDATE forward_logs
                 SET requested_model = NULL, resolved_alias = NULL, upstream_model = NULL
                 WHERE id = ?1",
            [id],
        )
        .unwrap();
}

#[test]
fn forward_log_model_filter_binds_each_identity_column() {
    let none = empty_forward_query();
    let (sql, params) = forward_log_filter(&none);
    assert!(!sql.to_ascii_lowercase().contains("model"));
    assert!(params.is_empty());

    let empty = ForwardLogQueryOptions {
        model: Some(""),
        ..empty_forward_query()
    };
    let (sql, params) = forward_log_filter(&empty);
    assert!(!sql.to_ascii_lowercase().contains("model"));
    assert!(params.is_empty());

    let filtered = ForwardLogQueryOptions {
        status: Some("success"),
        model: Some("glm-5.2"),
        ..empty_forward_query()
    };
    let (sql, params) = forward_log_filter(&filtered);
    assert!(sql.contains("status = ?"));
    assert!(sql.contains(
        "(model = ? OR requested_model = ? OR resolved_alias = ? OR upstream_model = ?)"
    ));
    assert!(sql.contains(" AND "));
    assert_eq!(params.len(), 5);
    assert_eq!(params[0], Value::Text("success".into()));
    assert!(
        params[1..]
            .iter()
            .all(|value| *value == Value::Text("glm-5.2".into()))
    );
}

#[test]
fn forward_logs_model_filter_matches_each_identity_and_legacy_fallback() {
    let dir = temp_data_dir("forward-model-identity-filter");
    let db = Database::open(dir.clone()).unwrap();

    let mut legacy = forward_log("acct", "success", 1.0);
    legacy.model = "needle".into();
    legacy.prompt_tokens = 1;
    let legacy_id = db.log_forward(&legacy).unwrap();
    clear_v23_identity(&db, legacy_id);

    let mut requested_only = forward_log("acct", "success", 2.0);
    requested_only.model = "legacy-req".into();
    requested_only.prompt_tokens = 2;
    let requested_id = insert_identity_log(
        &db,
        requested_only,
        Some("needle"),
        Some("alias-req"),
        Some("up-req"),
    );

    let mut alias_only = forward_log("acct", "success", 3.0);
    alias_only.model = "legacy-alias".into();
    alias_only.prompt_tokens = 3;
    let alias_id = insert_identity_log(
        &db,
        alias_only,
        Some("req-alias"),
        Some("needle"),
        Some("up-alias"),
    );

    let mut upstream_only = forward_log("acct", "success", 4.0);
    upstream_only.model = "legacy-up".into();
    upstream_only.prompt_tokens = 4;
    let upstream_id = insert_identity_log(
        &db,
        upstream_only,
        Some("req-up"),
        Some("alias-up"),
        Some("needle"),
    );

    let mut empty_v23 = forward_log("acct", "success", 5.0);
    empty_v23.model = "kept-empty".into();
    empty_v23.prompt_tokens = 5;
    insert_identity_log(&db, empty_v23, Some(""), Some(""), Some(""));

    let mut other = forward_log("acct", "success", 100.0);
    other.model = "other-legacy".into();
    other.prompt_tokens = 100;
    insert_identity_log(
        &db,
        other,
        Some("other-req"),
        Some("other-alias"),
        Some("other-up"),
    );

    let mut overlap = forward_log("acct", "success", 6.0);
    overlap.model = "needle".into();
    overlap.prompt_tokens = 6;
    let overlap_id =
        insert_identity_log(&db, overlap, Some("needle"), Some("needle"), Some("needle"));

    let page = db
        .query_forward_logs(ForwardLogQueryOptions {
            model: Some("needle"),
            sort_by: Some("cost"),
            sort_order: Some("asc"),
            ..empty_forward_query()
        })
        .unwrap();
    let ids = page.items.iter().map(|log| log.id).collect::<Vec<_>>();
    assert_eq!(
        ids,
        [legacy_id, requested_id, alias_id, upstream_id, overlap_id]
    );
    let unique = ids.iter().copied().collect::<HashSet<_>>();
    assert_eq!(unique.len(), ids.len());
    assert_eq!(page.summary.total_requests, 5);
    assert_eq!(page.summary.prompt_tokens, 16);
    assert!((page.summary.cost - 16.0).abs() < f64::EPSILON);

    let requested = db
        .query_forward_logs(ForwardLogQueryOptions {
            model: Some("legacy-req"),
            ..empty_forward_query()
        })
        .unwrap();
    assert_eq!(
        requested.items.iter().map(|log| log.id).collect::<Vec<_>>(),
        [requested_id]
    );

    let alias = db
        .query_forward_logs(ForwardLogQueryOptions {
            model: Some("alias-req"),
            ..empty_forward_query()
        })
        .unwrap();
    assert_eq!(
        alias.items.iter().map(|log| log.id).collect::<Vec<_>>(),
        [requested_id]
    );

    let empty_identity = db
        .query_forward_logs(ForwardLogQueryOptions {
            model: Some("kept-empty"),
            ..empty_forward_query()
        })
        .unwrap();
    assert_eq!(empty_identity.summary.total_requests, 1);
    assert_eq!(empty_identity.items[0].model, "kept-empty");

    let missing = db
        .query_forward_logs(ForwardLogQueryOptions {
            model: Some("missing"),
            ..empty_forward_query()
        })
        .unwrap();
    assert!(missing.items.is_empty());
    assert_eq!(missing.summary.total_requests, 0);

    let substring = db
        .query_forward_logs(ForwardLogQueryOptions {
            model: Some("need"),
            ..empty_forward_query()
        })
        .unwrap();
    assert!(substring.items.is_empty());
    assert_eq!(substring.summary.total_requests, 0);

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn forward_logs_model_filter_ands_other_filters_before_pagination() {
    let dir = temp_data_dir("forward-model-combo-filter");
    let db = Database::open(dir.clone()).unwrap();
    let inside = DateTime::parse_from_rfc3339("2026-07-17T04:15:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let outside = DateTime::parse_from_rfc3339("2026-07-17T03:30:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let matching = |suffix: &str, cost: f64| {
        let mut log = forward_log("acct", "success", cost);
        log.model = format!("legacy-{suffix}");
        log.provider_id = Some("opencode".into());
        log.offering_id = Some("go".into());
        log.client_key_id = Some("key-a".into());
        log.client_key_name = Some("Key-a".into());
        log.timestamp = inside;
        log.prompt_tokens = cost as i64;
        insert_identity_log(
            &db,
            log,
            Some("req-other"),
            Some("needle"),
            Some("up-other"),
        )
    };
    let first = matching("a", 1.0);
    let second = matching("b", 2.0);
    let third = matching("c", 3.0);

    let mut wrong_provider = forward_log("acct", "success", 9.0);
    wrong_provider.model = "legacy-provider".into();
    wrong_provider.provider_id = Some("goat".into());
    wrong_provider.offering_id = Some("goat".into());
    wrong_provider.client_key_id = Some("key-a".into());
    wrong_provider.timestamp = inside;
    insert_identity_log(
        &db,
        wrong_provider,
        Some("needle"),
        Some("alias-other"),
        Some("up-other"),
    );

    let mut wrong_key = forward_log("acct", "success", 8.0);
    wrong_key.model = "legacy-key".into();
    wrong_key.provider_id = Some("opencode".into());
    wrong_key.offering_id = Some("go".into());
    wrong_key.client_key_id = Some("key-b".into());
    wrong_key.timestamp = inside;
    insert_identity_log(&db, wrong_key, None, None, Some("needle"));

    let mut wrong_status = forward_log("acct", "error", 7.0);
    wrong_status.model = "needle".into();
    wrong_status.provider_id = Some("opencode".into());
    wrong_status.offering_id = Some("go".into());
    wrong_status.client_key_id = Some("key-a".into());
    wrong_status.timestamp = inside;
    let wrong_status_id = db.log_forward(&wrong_status).unwrap();
    clear_v23_identity(&db, wrong_status_id);

    let mut wrong_time = forward_log("acct", "success", 6.0);
    wrong_time.model = "legacy-time".into();
    wrong_time.provider_id = Some("opencode".into());
    wrong_time.offering_id = Some("go".into());
    wrong_time.client_key_id = Some("key-a".into());
    wrong_time.timestamp = outside;
    insert_identity_log(
        &db,
        wrong_time,
        Some("needle"),
        Some("needle"),
        Some("needle"),
    );

    for index in 0..5 {
        let mut decoy = forward_log("busy", "success", 100.0);
        decoy.model = format!("decoy-{index}");
        decoy.provider_id = Some("opencode".into());
        decoy.offering_id = Some("go".into());
        decoy.client_key_id = Some("key-a".into());
        decoy.timestamp = inside;
        db.log_forward(&decoy).unwrap();
    }

    let filtered = ForwardLogQueryOptions {
        limit: 1,
        offset: 0,
        status: Some("success"),
        provider_id: Some("opencode"),
        offering_id: Some("go"),
        model: Some("needle"),
        key_id: Some("key-a"),
        start_time: Some("2026-07-17T12:00:00+08:00"),
        end_time: Some("2026-07-17T12:30:00+08:00"),
        sort_by: Some("cost"),
        sort_order: Some("asc"),
        ..empty_forward_query()
    };
    let first_page = db.query_forward_logs(filtered).unwrap();
    assert_eq!(first_page.items.len(), 1);
    assert_eq!(first_page.items[0].id, first);
    assert_eq!(first_page.summary.total_requests, 3);
    assert_eq!(first_page.summary.prompt_tokens, 6);
    assert!((first_page.summary.cost - 6.0).abs() < f64::EPSILON);

    let second_page = db
        .query_forward_logs(ForwardLogQueryOptions {
            limit: 1,
            offset: 1,
            status: Some("success"),
            provider_id: Some("opencode"),
            offering_id: Some("go"),
            model: Some("needle"),
            key_id: Some("key-a"),
            start_time: Some("2026-07-17T12:00:00+08:00"),
            end_time: Some("2026-07-17T12:30:00+08:00"),
            sort_by: Some("cost"),
            sort_order: Some("asc"),
            ..empty_forward_query()
        })
        .unwrap();
    assert_eq!(second_page.items.len(), 1);
    assert_eq!(second_page.items[0].id, second);
    assert_eq!(second_page.summary.total_requests, 3);

    let rest = db
        .query_forward_logs(ForwardLogQueryOptions {
            limit: 50,
            offset: 2,
            status: Some("success"),
            provider_id: Some("opencode"),
            offering_id: Some("go"),
            model: Some("needle"),
            key_id: Some("key-a"),
            start_time: Some("2026-07-17T12:00:00+08:00"),
            end_time: Some("2026-07-17T12:30:00+08:00"),
            sort_by: Some("cost"),
            sort_order: Some("asc"),
            ..empty_forward_query()
        })
        .unwrap();
    assert_eq!(
        rest.items.iter().map(|log| log.id).collect::<Vec<_>>(),
        [third]
    );
    let unique = [first, second, third].into_iter().collect::<HashSet<_>>();
    assert_eq!(unique.len(), 3);

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn backfill_attributes_null_rows_in_chunks_with_resume_and_completion() {
    let dir = temp_data_dir("backfill-chunks");
    let db = Database::open(dir.clone()).unwrap();
    for index in 0..7 {
        let mut log = forward_log("acct", "success", index as f64);
        log.client_key_id = (index % 2 == 0).then(|| "already-set".to_string());
        db.log_forward(&log).unwrap();
    }

    // Chunk size 3 covers rowids 1..=7 in three steps; already-attributed
    // rows must never be overwritten by the range update.
    assert!(
        db.backfill_forward_logs_client_key_step("primary", "Primary", 3)
            .unwrap()
    );
    assert!(
        db.backfill_forward_logs_client_key_step("primary", "Primary", 3)
            .unwrap()
    );
    // The final chunk exactly reaches max rowid and records completion
    // in the same call; a further step is a no-op.
    assert!(
        !db.backfill_forward_logs_client_key_step("primary", "Primary", 3)
            .unwrap()
    );
    assert!(
        !db.backfill_forward_logs_client_key_step("primary", "Primary", 3)
            .unwrap()
    );
    assert_eq!(
        db.forward_log_backfill_marker().unwrap().as_deref(),
        Some(BACKFILL_DONE)
    );

    let rows = db.list_forward_logs(100).unwrap();
    assert_eq!(rows.len(), 7);
    for (index, row) in rows.iter().rev().enumerate() {
        if index % 2 == 0 {
            assert_eq!(row.client_key_id.as_deref(), Some("already-set"));
        } else {
            assert_eq!(row.client_key_id.as_deref(), Some("primary"));
            assert_eq!(row.client_key_name.as_deref(), Some("Primary"));
        }
    }

    // New NULL rows written by an older binary (a downgrade window)
    // restart the scan instead of staying "unattributed" forever.
    db.log_forward(&forward_log("acct", "success", 9.0))
        .unwrap();
    assert!(
        db.backfill_forward_logs_client_key_step("primary", "Primary", 3)
            .unwrap()
    );
    while db
        .backfill_forward_logs_client_key_step("primary", "Primary", 3)
        .unwrap()
    {}
    assert_eq!(
        db.forward_log_backfill_marker().unwrap().as_deref(),
        Some(BACKFILL_DONE)
    );
    let late_rows: Vec<_> = db
        .list_forward_logs(100)
        .unwrap()
        .into_iter()
        .filter(|row| row.client_key_name.as_deref() == Some("Primary"))
        .collect();
    assert!(late_rows.iter().any(|row| row.cost == Some(9.0)));

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn backfill_resumes_from_persisted_watermark_after_interruption() {
    let dir = temp_data_dir("backfill-resume");
    let db = Database::open(dir.clone()).unwrap();
    for index in 0..5 {
        db.log_forward(&forward_log("acct", "success", index as f64))
            .unwrap();
    }

    // Simulate a crash after the first chunk: the watermark persists but
    // the remaining rows are still NULL.
    assert!(
        db.backfill_forward_logs_client_key_step("primary", "Primary", 2)
            .unwrap()
    );
    assert_eq!(
        db.forward_log_backfill_marker().unwrap().as_deref(),
        Some("2")
    );
    let partial = db.list_forward_logs(100).unwrap();
    assert_eq!(
        partial
            .iter()
            .filter(|row| row.client_key_id.is_some())
            .count(),
        2
    );

    // A restarted run continues from the watermark instead of
    // rescanning; the last chunk completes the table and records done.
    assert!(
        db.backfill_forward_logs_client_key_step("primary", "Primary", 2)
            .unwrap()
    );
    assert!(
        !db.backfill_forward_logs_client_key_step("primary", "Primary", 2)
            .unwrap()
    );
    assert_eq!(
        db.forward_log_backfill_marker().unwrap().as_deref(),
        Some(BACKFILL_DONE)
    );
    let rows = db.list_forward_logs(100).unwrap();
    assert!(
        rows.iter()
            .all(|row| row.client_key_id.as_deref() == Some("primary"))
    );
    // No row was attributed twice: costs and row counts are unchanged.
    assert_eq!(
        rows.iter().map(|row| row.cost.unwrap_or(0.0)).sum::<f64>() as i64,
        10
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn backfill_restarts_after_done_when_a_downgrade_writes_null_rows() {
    let dir = temp_data_dir("backfill-restart-after-done");
    let db = Database::open(dir.clone()).unwrap();
    db.log_forward(&forward_log("acct", "success", 1.0))
        .unwrap();
    assert!(
        !db.backfill_forward_logs_client_key_step("primary", "Primary", 50)
            .unwrap()
    );
    assert_eq!(
        db.forward_log_backfill_marker().unwrap().as_deref(),
        Some(BACKFILL_DONE)
    );

    // A downgrade window writes fresh rows the way the pre-v18 binary
    // did: without a client key id.
    db.log_forward(&forward_log("acct", "success", 2.0))
        .unwrap();
    db.log_forward(&forward_log("acct", "success", 4.0))
        .unwrap();

    // The completion marker no longer short-circuits: one index probe
    // sees the NULL rows, the scan restarts, and they are attributed.
    assert!(
        db.backfill_forward_logs_client_key_step("primary", "Primary", 1)
            .unwrap()
    );
    while db
        .backfill_forward_logs_client_key_step("primary", "Primary", 50)
        .unwrap()
    {}
    assert_eq!(
        db.forward_log_backfill_marker().unwrap().as_deref(),
        Some(BACKFILL_DONE)
    );
    let rows = db.list_forward_logs(100).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .all(|row| row.client_key_id.as_deref() == Some("primary"))
    );

    // With no fresh NULL rows the marker keeps short-circuiting.
    let mut attributed = forward_log("acct", "success", 8.0);
    attributed.client_key_id = Some("primary".into());
    attributed.client_key_name = Some("Primary".into());
    db.log_forward(&attributed).unwrap();
    assert!(
        !db.backfill_forward_logs_client_key_step("primary", "Primary", 50)
            .unwrap()
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn backfill_completes_inline_for_empty_tables() {
    let dir = temp_data_dir("backfill-empty");
    let db = Database::open(dir.clone()).unwrap();
    assert!(
        !db.backfill_forward_logs_client_key_step("primary", "Primary", 50_000)
            .unwrap()
    );
    assert_eq!(
        db.forward_log_backfill_marker().unwrap().as_deref(),
        Some(BACKFILL_DONE)
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v19_client_key_migration_is_idempotent_and_crash_replay_safe() {
    let dir = temp_data_dir("v19-idempotent");
    let db = Database::open(dir.clone()).unwrap();
    let probe_columns = |conn: &Connection| {
        let mut stmt = conn.prepare("PRAGMA table_info(forward_logs)").unwrap();
        stmt.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert!(probe_columns(&db.conn).contains(&"client_key_id".to_string()));
    assert!(probe_columns(&db.conn).contains(&"client_key_name".to_string()));
    let version: i32 = db
        .conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version, CURRENT_SCHEMA_VERSION);

    let index_exists: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_forward_logs_client_key'",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(index_exists, 1);

    // Replaying migrate (as after a crash between ALTER TABLE and the
    // version bump, or simply a second open) converges without error.
    drop(db);
    let db = Database::open(dir.clone()).unwrap();
    db.migrate().unwrap();
    let version: i32 = db
        .conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version, CURRENT_SCHEMA_VERSION);

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v20_creates_the_sub_gateway_keys_table_idempotently() {
    let dir = temp_data_dir("v20-idempotent");
    let db = Database::open(dir.clone()).unwrap();

    let probe = |conn: &Connection| {
        let table: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'access_keys'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let index: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'index' AND name = 'idx_access_keys_active_key'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let legacy: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'sub_gateway_keys'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        (table, index, legacy)
    };
    assert_eq!(probe(&db.conn), (1, 1, 0));
    let version: i32 = db
        .conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version, CURRENT_SCHEMA_VERSION);

    // Replaying the migration converges to the same shape.
    db.migrate().unwrap();
    assert_eq!(probe(&db.conn), (1, 1, 0));

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v21_adds_usage_sync_columns_with_safe_defaults() {
    let dir = temp_data_dir("v21-usage-sync");
    let db = Database::open(dir.clone()).unwrap();
    let columns = {
        let mut stmt = db.conn.prepare("PRAGMA table_info(accounts)").unwrap();
        stmt.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    for name in USAGE_SYNC_ACCOUNT_COLUMNS {
        assert!(
            !columns.contains(&name.to_string()),
            "v27 must drop leftover {name}"
        );
    }
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);

    let account = account("sync-defaults");
    db.create_account(&account).unwrap();
    let sync = db
        .account_usage_sync_state("sync-defaults")
        .unwrap()
        .unwrap();
    assert!(sync.last_success_at.is_none());
    assert!(sync.last_attempt_at.is_none());
    assert!(sync.next_eligible_at.is_none());
    assert_eq!(sync.failure_streak, 0);
    assert!(sync.last_expedited_at.is_none());

    db.migrate().unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fresh_go_accounts_project_live_provider_quota_windows() {
    let dir = temp_data_dir("fresh-provider-quota");
    let db = Database::open(dir.clone()).unwrap();
    db.create_account(&account("fresh-go")).unwrap();
    assert!(db.list_quota_windows("fresh-go").unwrap().is_empty());

    let limits = SEED_LIMITS;
    db.calibrate_account_usage(
        "fresh-go",
        UsageWindowKind::FiveHours,
        50.0,
        Some(180),
        limits.window_5h,
    )
    .unwrap();
    let windows = db
        .live_opencode_go_quota_windows("fresh-go", &limits)
        .unwrap();
    assert_eq!(windows.len(), 3);
    let rolling = windows
        .iter()
        .find(|window| window.window_kind == QUOTA_WINDOW_FIVE_HOURS)
        .unwrap();
    assert!((rolling.used - limits.window_5h * 0.5).abs() < 1e-9);
    assert_eq!(rolling.limit_value, Some(limits.window_5h));
    assert_eq!(rolling.source, "opencode-go-live");

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v21_to_v22_creates_one_usable_rollback_backup() {
    let dir = temp_data_dir("v21-v22-backup");
    create_v21_fixture(&dir, false);

    let db = open_with_host_cipher(dir.clone()).expect("v21 database should migrate");
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert!(
        db.get_account("rollback-account")
            .expect("migrated account should load")
            .is_some()
    );
    let migrated = db.list_quota_windows("rollback-account").unwrap();
    let migrated_rolling = migrated
        .iter()
        .find(|window| window.window_kind == QUOTA_WINDOW_FIVE_HOURS)
        .unwrap()
        .used;
    db.log_forward(&forward_log("rollback-account", "success", 1.5))
        .unwrap();
    let limits = SEED_LIMITS;
    let live = db
        .live_opencode_go_quota_windows("rollback-account", &limits)
        .unwrap();
    let live_rolling = live
        .iter()
        .find(|window| window.window_kind == QUOTA_WINDOW_FIVE_HOURS)
        .unwrap();
    assert!((live_rolling.used - (migrated_rolling + 1.5)).abs() < 1e-9);
    assert_eq!(
        db.list_quota_windows("rollback-account")
            .unwrap()
            .iter()
            .find(|window| window.window_kind == QUOTA_WINDOW_FIVE_HOURS)
            .unwrap()
            .used,
        migrated_rolling,
        "frozen migration rows must not be the provider API authority"
    );
    drop(db);

    let backups_before = pre_v22_backup_paths(&dir);
    assert_eq!(backups_before.len(), 1);
    let pre_v23 = pre_v23_backup_paths(&dir);
    assert_eq!(pre_v23.len(), 1);
    let pre_v23_backup = Connection::open_with_flags(&pre_v23[0], OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("pre-v23 backup should open");
    assert_eq!(schema_version_on(&pre_v23_backup).unwrap(), 21);
    drop(pre_v23_backup);
    let backup_path = &backups_before[0];
    let backup_name = backup_path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("backup should have a UTF-8 filename");
    let timestamp = backup_name
        .strip_prefix(PRE_V22_BACKUP_FILE_PREFIX)
        .and_then(|name| name.strip_suffix(".bak"))
        .expect("backup should use the v22 rollback name");
    assert_eq!(timestamp.len(), 25);
    assert!(timestamp.bytes().enumerate().all(|(index, byte)| {
        (index == 8 && byte == b'T')
            || (index == 24 && byte == b'Z')
            || !matches!(index, 8 | 24) && byte.is_ascii_digit()
    }));

    let backup = Connection::open_with_flags(backup_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("backup should open read-only");
    assert_eq!(schema_version_on(&backup).unwrap(), 21);
    let backed_up_account: (String, String, i64) = backup
        .query_row(
            "SELECT name, key_cipher, enabled FROM accounts WHERE id = 'rollback-account'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("backup should retain the representative account");
    let backed_up_log: (String, String, String, f64) = backup
        .query_row(
            "SELECT account_id, model, status, cost FROM forward_logs LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("backup should retain the representative forward log");
    assert_eq!(backed_up_account.0, "rollback-account");
    assert_eq!(backed_up_account.2, 1);
    assert_fixture_account_cipher(&backed_up_account.1);
    assert_eq!(
        backed_up_log,
        (
            "rollback-account".into(),
            "test".into(),
            "success".into(),
            4.25
        )
    );
    drop(backup);

    let backup_bytes = fs::read(backup_path).expect("backup should be readable");
    let reopened = open_with_host_cipher(dir.clone()).expect("v22 database should reopen");
    assert_eq!(reopened.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(reopened);
    assert_eq!(pre_v22_backup_paths(&dir), backups_before);
    assert_eq!(
        fs::read(backup_path).expect("backup should remain readable"),
        backup_bytes
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v20_to_v22_creates_verified_source_backup_before_direct_upgrade() {
    let dir = temp_data_dir("v20-v22-backup");
    create_v20_fixture(&dir, false);

    let db = open_with_host_cipher(dir.clone()).expect("v20 database should migrate directly");
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    let account_columns = db
        .conn
        .prepare("PRAGMA table_info(accounts)")
        .expect("table info should prepare")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("table info should query")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("columns should load");
    assert!(
        !account_columns
            .iter()
            .any(|name| name == "usage_sync_last_success_at")
    );
    assert!(account_columns.iter().any(|name| name == "provider_id"));
    drop(db);

    let backups_before = pre_v22_backup_paths(&dir);
    assert_eq!(backups_before.len(), 1);
    let backup_bytes = fs::read(&backups_before[0]).expect("backup should be readable");
    let backup = Connection::open_with_flags(&backups_before[0], OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("backup should open read-only");
    assert_eq!(schema_version_on(&backup).unwrap(), 20);
    assert!(!table_has_column(&backup, "accounts", "provider_id").unwrap());
    assert!(!table_has_column(&backup, "accounts", "usage_sync_last_success_at").unwrap());
    drop(backup);

    let reopened = open_with_host_cipher(dir.clone()).expect("v22 database should reopen");
    assert_eq!(reopened.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(reopened);
    assert_eq!(pre_v22_backup_paths(&dir), backups_before);
    assert_eq!(
        fs::read(&backups_before[0]).expect("backup should remain readable"),
        backup_bytes
    );
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

#[test]
fn draft_v19_libraries_without_notes_gain_the_column_on_reopen() {
    let dir = temp_data_dir("draft-v19-notes-repair");
    let db = Database::open(dir.clone()).unwrap();
    let mut legacy = account("legacy");
    legacy.key_cipher = fixture_account_key_cipher();
    db.create_account(&legacy).unwrap();
    // Unreleased #43 drafts already sat at version 19 (client-key
    // columns + sub-key table) and never received upstream v18 notes.
    db.conn
        .execute_batch(
            "ALTER TABLE accounts DROP COLUMN notes;
                 DELETE FROM schema_version;
                 INSERT INTO schema_version (version) VALUES (19);",
        )
        .expect("draft numbering should be reproducible");
    let notes_before: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('accounts') WHERE name = 'notes'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(notes_before, 0);
    drop(db);

    let db = open_with_host_cipher(dir.clone()).expect("draft database should reopen");
    let (version, notes_after): (i32, i64) = db
        .conn
        .query_row(
            "SELECT
                    (SELECT MAX(version) FROM schema_version),
                    (SELECT COUNT(*) FROM pragma_table_info('accounts') WHERE name = 'notes')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("repaired schema should load");
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    assert_eq!(notes_after, 1);
    db.list_accounts()
        .expect("account reads must survive a missing notes column on the draft");

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sub_gateway_key_crud_and_unique_index_backstop() {
    let dir = temp_data_dir("sub-keys-crud");
    let db = Database::open(dir.clone()).unwrap();
    let now = Utc::now();
    let key = SubGatewayKey {
        id: "sub-1".into(),
        name: "Laptop".into(),
        key: "ocg-laptop".into(),
        enabled: true,
        deleted_at: None,
        created_at: now,
    };
    db.insert_sub_gateway_key(&key).unwrap();
    assert_eq!(db.count_active_sub_gateway_keys().unwrap(), 1);
    let primary_value = db.primary_access_key_value().unwrap().unwrap();
    let collide_primary = SubGatewayKey {
        id: "sub-primary-collide".into(),
        name: "Collide".into(),
        key: primary_value,
        enabled: true,
        deleted_at: None,
        created_at: now,
    };
    assert!(db.insert_sub_gateway_key(&collide_primary).is_err());
    assert_eq!(
        db.list_active_sub_gateway_keys().unwrap(),
        vec![key.clone()]
    );
    assert_eq!(db.list_sub_gateway_keys().unwrap().len(), 1);

    // Duplicate active values are rejected by the partial unique index.
    let duplicate = SubGatewayKey {
        id: "sub-2".into(),
        name: "Twin".into(),
        key: "ocg-laptop".into(),
        enabled: true,
        deleted_at: None,
        created_at: now,
    };
    assert!(db.insert_sub_gateway_key(&duplicate).is_err());

    // Disabled keys keep their plaintext, so they still block duplicates.
    assert!(db.set_sub_gateway_key_enabled("sub-1", false).unwrap());
    assert!(db.insert_sub_gateway_key(&duplicate).is_err());
    assert!(db.sub_gateway_key_value_exists("ocg-laptop").unwrap());
    assert_eq!(
        db.active_sub_gateway_key_values().unwrap(),
        vec!["ocg-laptop".to_string()]
    );

    // Renaming and regenerating address only non-deleted rows.
    assert!(db.rename_sub_gateway_key("sub-1", "Deck").unwrap());
    assert!(
        db.update_sub_gateway_key_value("sub-1", "ocg-deck")
            .unwrap()
    );
    assert!(db.sub_gateway_key_value_exists("ocg-deck").unwrap());

    // Soft delete clears the plaintext; tombstones free the value and do
    // not count as active.
    assert!(db.soft_delete_sub_gateway_key("sub-1", now).unwrap());
    let tombstone = db.get_sub_gateway_key("sub-1").unwrap().unwrap();
    assert!(tombstone.deleted_at.is_some());
    assert!(tombstone.key.is_empty());
    assert!(!tombstone.enabled);
    assert_eq!(db.count_active_sub_gateway_keys().unwrap(), 0);
    assert!(!db.sub_gateway_key_value_exists("ocg-deck").unwrap());
    assert!(!db.rename_sub_gateway_key("sub-1", "Gone").unwrap());
    assert!(!db.set_sub_gateway_key_enabled("sub-1", true).unwrap());
    assert!(!db.soft_delete_sub_gateway_key("sub-1", now).unwrap());

    // The freed value is insertable again, and missing ids report false.
    let recycled = SubGatewayKey {
        id: "sub-3".into(),
        name: "Recycled".into(),
        key: "ocg-deck".into(),
        enabled: true,
        deleted_at: None,
        created_at: now,
    };
    db.insert_sub_gateway_key(&recycled).unwrap();
    assert!(!db.rename_sub_gateway_key("missing", "X").unwrap());
    assert!(db.get_sub_gateway_key("missing").unwrap().is_none());

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn forward_log_keys_resolve_the_latest_name_per_id() {
    let dir = temp_data_dir("log-keys-latest-name");
    let db = Database::open(dir.clone()).unwrap();
    let base = ForwardLog {
        id: 0,
        timestamp: Utc::now(),
        model: "m".into(),
        account_id: "a".into(),
        account_name: "a".into(),
        route_account_id: None,
        provider_id: None,
        offering_id: None,
        credential_account_id: None,
        client_key_id: Some("sub-1".into()),
        client_key_name: Some("Laptop".into()),
        status: "success".into(),
        http_status: Some(200),
        route: String::new(),
        prompt_tokens: 1,
        completion_tokens: 1,
        cached_tokens: 0,
        cache_creation_tokens: 0,
        cost: None,
        raw_cost_usd: None,
        quota_debit: None,
        effective_paid_cost_usd: None,
        pricing_revision_id: None,
        quota_multiplier: None,
        local_adjustment_multiplier: None,
        service_tier: None,
        cost_state: "not_applicable".into(),
        error_message: None,
        request_id: None,
        attempt: None,
        error_source: None,
        error_stage: None,
        duration_ms: None,
        diagnostic: None,
    };
    // A lexicographically "larger" historical name must not win: it was
    // written first, the current name last.
    let mut zzz = base.clone();
    zzz.client_key_name = Some("zzz-old".into());
    db.log_forward(&zzz).unwrap();
    db.log_forward(&base).unwrap();
    let mut renamed = base.clone();
    renamed.client_key_name = Some("Deck".into());
    db.log_forward(&renamed).unwrap();

    let keys = db.list_forward_log_keys().unwrap();
    assert_eq!(keys.len(), 1, "one entry per distinct key id");
    assert_eq!(keys[0].id, "sub-1");
    assert_eq!(keys[0].name, "Deck", "the latest snapshot wins");

    // NULL-name rows fall back to the id label.
    let mut unnamed = base.clone();
    unnamed.client_key_id = Some("ghost".into());
    unnamed.client_key_name = None;
    db.log_forward(&unnamed).unwrap();
    let keys = db.list_forward_log_keys().unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(
        keys.iter().find(|key| key.id == "ghost").unwrap().name,
        "ghost"
    );

    // The list stays purely log-driven: an id with no rows (e.g. the
    // primary key before its first attributed request) never appears.
    assert!(
        !keys
            .iter()
            .any(|key| key.id == "00000000-0000-0000-0000-000000000001")
    );

    // A primary-attributed row with a NULL name resolves to the fixed
    // display name, never the raw id constant.
    let mut unnamed_primary = base.clone();
    unnamed_primary.client_key_id = Some("00000000-0000-0000-0000-000000000001".into());
    unnamed_primary.client_key_name = None;
    db.log_forward(&unnamed_primary).unwrap();
    let keys = db.list_forward_log_keys().unwrap();
    let primary = keys
        .iter()
        .find(|key| key.id == "00000000-0000-0000-0000-000000000001")
        .unwrap();
    assert_eq!(primary.name, "Primary");

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

fn create_v22_fixture(dir: &Path) {
    let db = Database::open(dir.to_path_buf()).expect("fixture database should open");
    let mut v22_account = account("v22-account");
    v22_account.key_cipher = fixture_account_key_cipher();
    db.create_account(&v22_account)
        .expect("representative account should save");
    let mut goat = account("v22-goat");
    goat.key_cipher = fixture_account_key_cipher();
    goat.provider_id = COMMAND_CODE_PROVIDER_ID.to_string();
    goat.offering_id = GOAT_OFFERING_ID.to_string();
    goat.enabled = false;
    db.create_account(&goat)
        .expect("representative GOAT account should save");
    db.log_forward(&forward_log("v22-account", "success", 3.5))
        .expect("representative forward log should save");
    drop(db);

    let conn = Connection::open(dir.join("data.sqlite")).expect("v23 fixture should reopen");
    conn.execute_batch(
        "PRAGMA foreign_keys=OFF;
             DROP TRIGGER IF EXISTS access_keys_protect_primary_delete;
             DROP TABLE IF EXISTS access_keys;
             CREATE TABLE IF NOT EXISTS sub_gateway_keys (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                key TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                deleted_at TEXT,
                created_at TEXT NOT NULL
             );
             CREATE UNIQUE INDEX IF NOT EXISTS idx_sub_gateway_keys_key
                ON sub_gateway_keys(key) WHERE deleted_at IS NULL AND key <> '';
             DROP INDEX IF EXISTS idx_account_model_capabilities_account;
             DROP TABLE IF EXISTS account_custom_configs;
             DROP TABLE IF EXISTS account_model_capabilities;
             ALTER TABLE accounts DROP COLUMN verification_error;
             ALTER TABLE accounts DROP COLUMN connection_verified_at;
             ALTER TABLE accounts DROP COLUMN verification_status;
             ALTER TABLE forward_logs DROP COLUMN native_cost_currency;
             ALTER TABLE forward_logs DROP COLUMN native_cost_unit;
             ALTER TABLE forward_logs DROP COLUMN native_cost_value;
             ALTER TABLE forward_logs DROP COLUMN upstream_model;
             ALTER TABLE forward_logs DROP COLUMN resolved_alias;
             ALTER TABLE forward_logs DROP COLUMN requested_model;
             DELETE FROM schema_version;
             INSERT INTO schema_version (version) VALUES (22);
             UPDATE accounts SET enabled = 1 WHERE id = 'v22-goat';
             PRAGMA foreign_keys=ON;",
    )
    .expect("v22 fixture should be created");
    restore_usage_sync_account_columns(&conn);
}

#[test]
fn v22_to_v23_creates_one_usable_rollback_backup_and_contract_tables() {
    let dir = temp_data_dir("v22-v23-backup");
    create_v22_fixture(&dir);
    assert!(pre_v23_backup_paths(&dir).is_empty());

    let db = open_with_host_cipher(dir.clone()).expect("v22 database should migrate");
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    let go = db
        .account_verification_state("v22-account")
        .unwrap()
        .unwrap();
    assert_eq!(go.status, ConnectionVerificationStatus::NotRequired);
    let goat = db.get_account("v22-goat").unwrap().unwrap();
    assert!(!goat.enabled, "migrated GOAT rows must be fail-closed");
    let goat_state = db.account_verification_state("v22-goat").unwrap().unwrap();
    assert_eq!(goat_state.status, ConnectionVerificationStatus::NotRequired);
    let log_id: i64 = db
        .conn
        .query_row("SELECT id FROM forward_logs LIMIT 1", [], |row| row.get(0))
        .unwrap();
    let attribution = db.forward_log_native_attribution(log_id).unwrap().unwrap();
    assert_eq!(attribution.requested_model.as_deref(), Some("test"));
    assert_eq!(attribution.upstream_model.as_deref(), Some("test"));
    assert_eq!(attribution.native_cost_unit.as_deref(), Some("usd"));
    assert_eq!(attribution.native_cost_currency.as_deref(), Some("USD"));
    drop(db);

    let backups = pre_v23_backup_paths(&dir);
    assert_eq!(backups.len(), 1);
    let backup = Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("pre-v23 backup should open");
    assert_eq!(schema_version_on(&backup).unwrap(), 22);
    assert!(!table_has_column(&backup, "accounts", "verification_status").unwrap());
    drop(backup);

    let reopened = open_with_host_cipher(dir.clone()).expect("v23 database should reopen");
    assert_eq!(reopened.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(reopened);
    assert_eq!(pre_v23_backup_paths(&dir).len(), 1);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn newer_unsupported_schema_is_rejected_without_writes() {
    let dir = temp_data_dir("schema-too-new");
    let db = Database::open(dir.clone()).unwrap();
    let too_new = CURRENT_SCHEMA_VERSION + 1;
    db.conn
        .execute_batch(&format!(
            "DELETE FROM schema_version;
                     INSERT INTO schema_version (version) VALUES ({too_new});"
        ))
        .unwrap();
    drop(db);

    let error = match Database::open(dir.clone()) {
        Ok(_) => panic!("unsupported schema must fail closed"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("newer than this build supports"),
        "{error}"
    );
    let conn = Connection::open(dir.join("data.sqlite")).unwrap();
    assert_eq!(schema_version_on(&conn).unwrap(), too_new);
    drop(conn);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn zen_free_model_catalog_survives_reopen() {
    let dir = temp_data_dir("zen-free-model-catalog");
    let refreshed_at = Utc::now();
    {
        let db = Database::open(dir.clone()).unwrap();
        db.set_zen_free_model_catalog(&crate::kernel::zen::ZenFreeModelCatalog {
            models: vec!["persisted-coder-free".into()],
            refreshed_at: Some(refreshed_at),
            source_url: crate::kernel::zen::ZEN_MODELS_SOURCE_URL.into(),
        })
        .unwrap();
    }
    {
        let db = Database::open(dir.clone()).unwrap();
        let catalog = db.zen_free_model_catalog().unwrap().unwrap();
        assert_eq!(catalog.models, ["persisted-coder-free"]);
        assert_eq!(catalog.refreshed_at, Some(refreshed_at));
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v26_fresh_database_has_contract_tables_and_reopens() {
    let dir = temp_data_dir("v26-fresh");
    let db = Database::open(dir.clone()).unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    let tables: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN (
                    'provider_contract_scopes', 'provider_contract_model_protocols'
                 )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 2);
    db.migrate().unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(db);
    let reopened = Database::open(dir.clone()).unwrap();
    assert_eq!(reopened.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(reopened);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v25_to_v26_backfills_zen_catalog_into_provider_scope() {
    let dir = temp_data_dir("v25-v26-zen-backfill");
    let refreshed_at = Utc::now();
    {
        let db = Database::open(dir.clone()).unwrap();
        db.set_zen_free_model_catalog(&crate::kernel::zen::ZenFreeModelCatalog {
            models: vec!["backfill-coder-free".into()],
            refreshed_at: Some(refreshed_at),
            source_url: crate::kernel::zen::ZEN_MODELS_SOURCE_URL.into(),
        })
        .unwrap();
        db.conn
            .execute_batch(
                "DROP TRIGGER IF EXISTS access_keys_protect_primary_delete;
                     DROP TABLE IF EXISTS access_keys;
                     DROP TABLE IF EXISTS provider_contract_model_protocols;
                     DROP TABLE IF EXISTS provider_contract_scopes;
                     DELETE FROM schema_version;
                     INSERT INTO schema_version (version) VALUES (25);",
            )
            .unwrap();
        assert_eq!(db.schema_version().unwrap(), 25);
    }
    let db = Database::open(dir.clone()).expect("v25 database should migrate to v26");
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    let scope = db
        .load_persisted_scope(&ContractScope::provider(OPENCODE_ZEN_FREE_PROVIDER_ID))
        .unwrap()
        .expect("zen provider scope should be backfilled");
    assert_eq!(scope.catalog_models, ["backfill-coder-free"]);
    assert_eq!(scope.catalog_source, CATALOG_SOURCE_OFFICIAL_ZEN);
    assert!(scope.revision >= 1);
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn provider_and_custom_contract_scopes_are_isolated() {
    let dir = temp_data_dir("v26-scope-isolation");
    let db = Database::open(dir.clone()).unwrap();
    let now = Utc::now();
    let go = ContractScope::provider(OPENCODE_PROVIDER_ID);
    let custom = ContractScope::custom_endpoint("custom-a");
    db.upsert_model_protocol(&PersistedModelProtocol {
        scope: go.clone(),
        model_id: "glm-5.2".into(),
        protocol: UpstreamProtocolKind::ChatCompletions,
        source: ContractEvidenceSource::ProbeConfirmed,
        verified_at: Some(now),
        observed_at: Some(now),
        last_probe_result: Some(ProbeResultKind::Success),
        last_probe_at: Some(now),
        last_probe_error: None,
    })
    .unwrap();
    db.upsert_model_protocol(&PersistedModelProtocol {
        scope: custom.clone(),
        model_id: "local-model".into(),
        protocol: UpstreamProtocolKind::ChatCompletions,
        source: ContractEvidenceSource::Preset,
        verified_at: Some(now),
        observed_at: Some(now),
        last_probe_result: None,
        last_probe_at: None,
        last_probe_error: None,
    })
    .unwrap();
    db.set_model_protocol_overrides(
        &go,
        &[(
            "glm-5.2".into(),
            UpstreamProtocolKind::Messages,
            ProtocolOverrideState::ForceOff,
        )],
        now,
    )
    .unwrap();
    let persisted = db.load_persisted_contracts().unwrap();
    assert!(
        persisted
            .evidence
            .get(&go)
            .unwrap()
            .iter()
            .any(|row| row.model_id == "glm-5.2")
    );
    assert!(
        persisted
            .evidence
            .get(&custom)
            .unwrap()
            .iter()
            .all(|row| row.model_id != "glm-5.2")
    );
    assert!(
        persisted
            .overrides
            .get(&go)
            .unwrap()
            .iter()
            .any(|row| row.model_id == "glm-5.2" && row.state == ProtocolOverrideState::ForceOff)
    );
    assert!(
        persisted
            .overrides
            .get(&custom)
            .map(|rows| rows.iter().all(|row| row.model_id != "glm-5.2"))
            .unwrap_or(true)
    );
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn probe_evidence_and_catalog_mutations_advance_scope_revision_atomically() {
    let dir = temp_data_dir("v26-revision-bump");
    let db = Database::open(dir.clone()).unwrap();
    let now = Utc::now();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);
    assert!(db.load_persisted_scope(&scope).unwrap().is_none());

    let success = db
        .upsert_model_protocol(&PersistedModelProtocol {
            scope: scope.clone(),
            model_id: "grok-4.5".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: ContractEvidenceSource::ProbeConfirmed,
            verified_at: Some(now),
            observed_at: Some(now),
            last_probe_result: Some(ProbeResultKind::Success),
            last_probe_at: Some(now),
            last_probe_error: None,
        })
        .unwrap();
    assert_eq!(success.revision, 2);
    let after_success = db.load_persisted_scope(&scope).unwrap().unwrap();
    assert_eq!(after_success.revision, 2);

    let failure = db
        .upsert_model_protocol(&PersistedModelProtocol {
            scope: scope.clone(),
            model_id: "grok-4.5".into(),
            protocol: UpstreamProtocolKind::Messages,
            source: ContractEvidenceSource::ProbeObserved,
            verified_at: None,
            observed_at: Some(now),
            last_probe_result: Some(ProbeResultKind::Failure),
            last_probe_at: Some(now),
            last_probe_error: Some("upstream 500".into()),
        })
        .unwrap();
    assert_eq!(failure.revision, 3);

    let catalog = db
        .set_contract_catalog(
            &scope,
            &["grok-4.5".into()],
            Some(now),
            crate::provider_contracts::CATALOG_SOURCE_STATIC,
            "",
            now,
        )
        .unwrap();
    assert_eq!(catalog.revision, 4);

    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_probe_write BEFORE INSERT ON provider_contract_model_protocols
                 BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;",
        )
        .unwrap();
    let before_failed = db.load_persisted_scope(&scope).unwrap().unwrap().revision;
    let failed = db.upsert_model_protocol(&PersistedModelProtocol {
        scope: scope.clone(),
        model_id: "glm-5.3".into(),
        protocol: UpstreamProtocolKind::Responses,
        source: ContractEvidenceSource::ProbeObserved,
        verified_at: None,
        observed_at: Some(now),
        last_probe_result: Some(ProbeResultKind::Failure),
        last_probe_at: Some(now),
        last_probe_error: Some("should roll back".into()),
    });
    assert!(failed.is_err());
    let after_failed = db.load_persisted_scope(&scope).unwrap().unwrap();
    assert_eq!(after_failed.revision, before_failed);
    assert!(
        db.load_model_protocol(&scope, "glm-5.3", UpstreamProtocolKind::Responses)
            .unwrap()
            .is_none()
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

fn probe_observation(
    scope: ContractScope,
    model_id: &str,
    protocol: UpstreamProtocolKind,
    now: DateTime<Utc>,
) -> PersistedModelProtocol {
    PersistedModelProtocol {
        scope,
        model_id: model_id.into(),
        protocol,
        source: ContractEvidenceSource::ProbeConfirmed,
        verified_at: Some(now),
        observed_at: Some(now),
        last_probe_result: Some(ProbeResultKind::Success),
        last_probe_at: Some(now),
        last_probe_error: None,
    }
}

#[test]
fn probe_observation_batch_upserts_atomically_and_bumps_scope_once() {
    let dir = temp_data_dir("v26-probe-batch");
    let db = Database::open(dir.clone()).unwrap();
    let now = Utc::now();
    let go = ContractScope::provider(OPENCODE_PROVIDER_ID);
    let custom = ContractScope::custom_endpoint("custom-a");

    let empty = db.upsert_model_protocols(&[]);
    assert!(empty.is_err(), "{empty:?}");
    assert!(empty.unwrap_err().to_string().contains("nonempty"));
    assert!(db.load_persisted_scope(&go).unwrap().is_none());

    let mixed = db.upsert_model_protocols(&[
        probe_observation(
            go.clone(),
            "grok-4.5",
            UpstreamProtocolKind::ChatCompletions,
            now,
        ),
        probe_observation(
            custom.clone(),
            "local-model",
            UpstreamProtocolKind::ChatCompletions,
            now,
        ),
    ]);
    assert!(mixed.is_err(), "{mixed:?}");
    assert!(mixed.unwrap_err().to_string().contains("mix"));
    assert!(db.load_persisted_scope(&go).unwrap().is_none());
    assert!(db.load_persisted_scope(&custom).unwrap().is_none());
    assert!(
        db.load_model_protocol(&go, "grok-4.5", UpstreamProtocolKind::ChatCompletions)
            .unwrap()
            .is_none()
    );

    let persisted = db
        .upsert_model_protocols(&[
            probe_observation(
                go.clone(),
                "grok-4.5",
                UpstreamProtocolKind::ChatCompletions,
                now,
            ),
            probe_observation(go.clone(), "grok-4.5", UpstreamProtocolKind::Responses, now),
        ])
        .unwrap();
    assert_eq!(persisted.revision, 2);
    let after = db.load_persisted_scope(&go).unwrap().unwrap();
    assert_eq!(after.revision, 2);
    assert!(
        db.load_model_protocol(&go, "grok-4.5", UpstreamProtocolKind::ChatCompletions)
            .unwrap()
            .is_some()
    );
    assert!(
        db.load_model_protocol(&go, "grok-4.5", UpstreamProtocolKind::Responses)
            .unwrap()
            .is_some()
    );

    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_second_probe_observation_write
                 BEFORE INSERT ON provider_contract_model_protocols
                 WHEN NEW.protocol = 'messages'
                 BEGIN SELECT RAISE(ABORT, 'injected second observation write failure'); END;",
        )
        .unwrap();
    let before_failed = db.load_persisted_scope(&go).unwrap().unwrap().revision;
    let failed = db.upsert_model_protocols(&[
        probe_observation(
            go.clone(),
            "glm-5.3",
            UpstreamProtocolKind::ChatCompletions,
            now,
        ),
        probe_observation(go.clone(), "glm-5.3", UpstreamProtocolKind::Messages, now),
    ]);
    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(
        db.load_persisted_scope(&go).unwrap().unwrap().revision,
        before_failed
    );
    assert!(
        db.load_model_protocol(&go, "glm-5.3", UpstreamProtocolKind::ChatCompletions)
            .unwrap()
            .is_none()
    );
    assert!(
        db.load_model_protocol(&go, "glm-5.3", UpstreamProtocolKind::Messages)
            .unwrap()
            .is_none()
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v23_persists_verification_custom_config_and_capabilities() {
    let dir = temp_data_dir("v23-contracts");
    let db = Database::open(dir.clone()).unwrap();
    let mut goat = account("goat-draft");
    goat.provider_id = COMMAND_CODE_PROVIDER_ID.to_string();
    goat.offering_id = GOAT_OFFERING_ID.to_string();
    goat.enabled = false;
    db.create_account(&goat).unwrap();
    let goat_state = db
        .account_verification_state("goat-draft")
        .unwrap()
        .unwrap();
    assert_eq!(goat_state.status, ConnectionVerificationStatus::NotRequired);

    let mut custom = account("custom-1");
    custom.provider_id = CUSTOM_PROVIDER_ID.to_string();
    custom.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    custom.enabled = false;
    db.create_account(&custom).unwrap();
    db.upsert_account_custom_config(
        "custom-1",
        &AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        },
        true,
    )
    .unwrap();
    let rejected = db.upsert_account_custom_config(
        "custom-1",
        &AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/messages".into(),
            upstream_protocol: UpstreamProtocolKind::Messages,
        },
        false,
    );
    assert!(
        rejected.is_err(),
        "protocol must stay immutable after create"
    );
    db.replace_account_model_capabilities(
        "custom-1",
        &[AccountModelCapabilityInput {
            public_model: "deepseek/deepseek-v4-flash".into(),
            upstream_model: "deepseek/deepseek-v4-flash".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: Some("manual".into()),
        }],
    )
    .unwrap();
    let capabilities = db.list_account_model_capabilities("custom-1").unwrap();
    assert_eq!(capabilities[0].public_model, "deepseek/deepseek-v4-flash");
    assert_eq!(capabilities[0].upstream_model, "deepseek/deepseek-v4-flash");

    db.set_account_verification(
        "custom-1",
        ConnectionVerificationStatus::Verified,
        Some(Utc::now()),
        None,
    )
    .unwrap();
    db.update_account(
        "custom-1",
        &AccountUpdate {
            key: Some("rotated".into()),
            ..AccountUpdate::default()
        },
        Some("new-cipher"),
        None,
    )
    .unwrap();
    let after_key = db.account_verification_state("custom-1").unwrap().unwrap();
    assert_eq!(after_key.status, ConnectionVerificationStatus::Pending);
    let caps_after_key = db.list_account_model_capabilities("custom-1").unwrap();
    assert_eq!(caps_after_key.len(), 1);
    assert_eq!(caps_after_key[0].public_model, "deepseek/deepseek-v4-flash");

    let unknown = account("unknown");
    let mut unknown = unknown;
    unknown.provider_id = "no-such-provider".into();
    unknown.offering_id = "no-such-offering".into();
    assert!(db.create_account(&unknown).is_err());

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn forward_logs_dual_write_native_usd_attribution() {
    let dir = temp_data_dir("v23-native-logs");
    let db = Database::open(dir.clone()).unwrap();
    db.create_account(&account("priced")).unwrap();
    let mut log = forward_log("priced", "success", 1.25);
    log.raw_cost_usd = Some(1.25);
    log.cost_state = "priced".into();
    let id = db.log_forward(&log).unwrap();
    let attribution = db.forward_log_native_attribution(id).unwrap().unwrap();
    assert_eq!(attribution.native_cost_value, Some(1.25));
    assert_eq!(attribution.native_cost_unit.as_deref(), Some("usd"));
    assert_eq!(attribution.native_cost_currency.as_deref(), Some("USD"));
    assert_eq!(attribution.upstream_model.as_deref(), Some("test"));

    db.set_forward_log_native_attribution(
        id,
        &ForwardLogNativeAttribution {
            requested_model: Some("deepseek-v4-flash".into()),
            resolved_alias: Some("deepseek-v4-flash".into()),
            upstream_model: Some("deepseek/deepseek-v4-flash".into()),
            native_cost_value: Some(12.0),
            native_cost_unit: Some("credits".into()),
            native_cost_currency: None,
        },
    )
    .unwrap();
    let updated = db.forward_log_native_attribution(id).unwrap().unwrap();
    assert_eq!(
        updated.upstream_model.as_deref(),
        Some("deepseek/deepseek-v4-flash")
    );
    assert_eq!(updated.native_cost_unit.as_deref(), Some("credits"));

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn update_forward_log_finalizes_native_usd_with_cost_fields() {
    let dir = temp_data_dir("v23-native-finalize");
    let db = Database::open(dir.clone()).unwrap();
    db.create_account(&account("stream")).unwrap();

    let mut streaming = forward_log("stream", "streaming", 0.0);
    streaming.cost = None;
    streaming.raw_cost_usd = None;
    streaming.cost_state = "not_applicable".into();
    streaming.provider_id = Some(OPENCODE_PROVIDER_ID.to_string());
    streaming.offering_id = Some(GO_OFFERING_ID.to_string());
    let streaming_id = db.log_forward(&streaming).unwrap();
    let preliminary = db
        .forward_log_native_attribution(streaming_id)
        .unwrap()
        .unwrap();
    assert_eq!(preliminary.native_cost_value, None);
    assert_eq!(preliminary.native_cost_unit, None);

    db.update_forward_log(
        streaming_id,
        "success",
        Some(200),
        ForwardMetrics {
            cost: 1.25,
            raw_cost_usd: Some(1.25),
            pricing_provider_id: Some(OPENCODE_PROVIDER_ID.to_string()),
            pricing_offering_id: Some(GO_OFFERING_ID.to_string()),
            cost_state: "priced",
            ..ForwardMetrics::default()
        },
        None,
        None,
    )
    .unwrap();
    let finalized = db
        .forward_log_native_attribution(streaming_id)
        .unwrap()
        .unwrap();
    assert_eq!(finalized.native_cost_value, Some(1.25));
    assert_eq!(finalized.native_cost_unit.as_deref(), Some("usd"));
    assert_eq!(finalized.native_cost_currency.as_deref(), Some("USD"));
    let stored_cost: f64 = db
        .conn
        .query_row(
            "SELECT cost FROM forward_logs WHERE id = ?1",
            [streaming_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!((stored_cost - 1.25).abs() < 1e-9);

    let zero_id = db
        .log_forward(&forward_log("stream", "streaming", 0.0))
        .unwrap();
    assert_eq!(
        db.forward_log_native_attribution(zero_id)
            .unwrap()
            .unwrap()
            .native_cost_value,
        Some(0.0)
    );
    db.update_forward_log(
        zero_id,
        "success",
        None,
        ForwardMetrics {
            cost: 2.5,
            raw_cost_usd: Some(2.5),
            cost_state: "priced",
            ..ForwardMetrics::default()
        },
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        db.forward_log_native_attribution(zero_id)
            .unwrap()
            .unwrap()
            .native_cost_value,
        Some(2.5)
    );

    let mut zen = forward_log("stream", "streaming", 0.0);
    zen.cost = None;
    zen.raw_cost_usd = None;
    zen.cost_state = "not_applicable".into();
    zen.provider_id = Some(OPENCODE_ZEN_FREE_PROVIDER_ID.to_string());
    zen.offering_id = Some(ANONYMOUS_FREE_OFFERING_ID.to_string());
    let zen_id = db.log_forward(&zen).unwrap();
    db.update_forward_log(
        zen_id,
        "success",
        Some(200),
        ForwardMetrics {
            cost: 1.0,
            raw_cost_usd: Some(1.0),
            cost_state: "priced",
            ..ForwardMetrics::default()
        },
        None,
        None,
    )
    .unwrap();
    let zen_native = db.forward_log_native_attribution(zen_id).unwrap().unwrap();
    assert_eq!(zen_native.native_cost_value, Some(0.0));
    assert_eq!(zen_native.native_cost_unit.as_deref(), Some("usd"));
    let zen_cost: (f64, String) = db
        .conn
        .query_row(
            "SELECT cost, cost_state FROM forward_logs WHERE id = ?1",
            [zen_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(zen_cost.0, 0.0);
    assert_eq!(zen_cost.1, "free");

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn create_account_with_contract_is_atomic_on_custom_config_failure() {
    let dir = temp_data_dir("v23-atomic-create");
    let db = Database::open(dir.clone()).unwrap();
    let mut custom = account("custom-atomic");
    custom.provider_id = CUSTOM_PROVIDER_ID.to_string();
    custom.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    custom.enabled = false;
    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_custom_config
                 BEFORE INSERT ON account_custom_configs
                 BEGIN
                     SELECT RAISE(ABORT, 'forced custom config failure');
                 END;",
        )
        .unwrap();

    let error = db
        .create_account_with_contract(
            &custom,
            Some(&AccountCustomConfigInput {
                endpoint_url: "https://api.example.com/v1/chat/completions".into(),
                upstream_protocol: UpstreamProtocolKind::ChatCompletions,
            }),
            &[AccountModelCapabilityInput {
                public_model: "org/model".into(),
                upstream_model: "org/model".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: None,
            }],
        )
        .expect_err("forced custom config failure should abort the create");
    assert!(
        error.to_string().contains("forced custom config failure"),
        "{error}"
    );
    assert!(db.get_account("custom-atomic").unwrap().is_none());
    assert!(db.account_custom_config("custom-atomic").unwrap().is_none());
    assert!(
        db.list_account_model_capabilities("custom-atomic")
            .unwrap()
            .is_empty()
    );

    db.conn
        .execute_batch("DROP TRIGGER fail_custom_config;")
        .unwrap();
    db.create_account_with_contract(
        &custom,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        }),
        &[AccountModelCapabilityInput {
            public_model: "org/model".into(),
            upstream_model: "org/model".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: None,
        }],
    )
    .unwrap();
    assert!(db.get_account("custom-atomic").unwrap().is_some());
    assert!(db.account_custom_config("custom-atomic").unwrap().is_some());

    let mut go = account("go-rejects-custom");
    let rejected = db.create_account_with_contract(
        &go,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        }),
        &[],
    );
    assert!(rejected.is_err(), "non-Custom accounts must reject config");
    assert!(db.get_account("go-rejects-custom").unwrap().is_none());

    go.id = "go-rejects-caps".into();
    go.name = "go-rejects-caps".into();
    let rejected_caps = db.create_account_with_contract(
        &go,
        None,
        &[AccountModelCapabilityInput {
            public_model: "org/model".into(),
            upstream_model: "org/model".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: None,
        }],
    );
    assert!(
        rejected_caps.is_err(),
        "non-Custom accounts must reject capabilities"
    );
    assert!(db.get_account("go-rejects-caps").unwrap().is_none());

    let mut custom_empty = account("custom-empty-caps");
    custom_empty.provider_id = CUSTOM_PROVIDER_ID.to_string();
    custom_empty.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    custom_empty.enabled = false;
    let empty_caps = db.create_account_with_contract(
        &custom_empty,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        }),
        &[],
    );
    assert!(
        empty_caps.is_err(),
        "Custom create must require at least one model capability"
    );
    assert!(db.get_account("custom-empty-caps").unwrap().is_none());

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn account_migration_batch_is_atomic_and_preserves_order() {
    let dir = temp_data_dir("account-migration-batch");
    let db = Database::open(dir.clone()).unwrap();
    let go = account("migration-go");
    let mut custom = account("migration-custom");
    custom.provider_id = CUSTOM_PROVIDER_ID.to_string();
    custom.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    custom.credential_kind = CredentialKind::ApiKey;
    custom.quota_scope = QuotaScope::Key;
    custom.enabled = false;
    let records = vec![
        AccountImportRecord {
            account: go,
            custom_config: None,
            capabilities: Vec::new(),
            verification_status: ConnectionVerificationStatus::NotRequired,
            connection_verified_at: None,
        },
        AccountImportRecord {
            account: custom,
            custom_config: Some(AccountCustomConfigInput {
                endpoint_url: "https://api.example.com/v1/chat/completions".into(),
                upstream_protocol: UpstreamProtocolKind::ChatCompletions,
            }),
            capabilities: vec![AccountModelCapabilityInput {
                public_model: "org/model".into(),
                upstream_model: "org/model".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: Some("import".into()),
            }],
            verification_status: ConnectionVerificationStatus::Pending,
            connection_verified_at: None,
        },
    ];
    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_migration_custom_config
                 BEFORE INSERT ON account_custom_configs
                 BEGIN
                     SELECT RAISE(ABORT, 'forced migration failure');
                 END;",
        )
        .unwrap();
    assert!(db.import_accounts_with_contracts(&records).is_err());
    assert!(db.get_account("migration-go").unwrap().is_none());
    assert!(db.get_account("migration-custom").unwrap().is_none());

    db.conn
        .execute_batch("DROP TRIGGER fail_migration_custom_config;")
        .unwrap();
    db.import_accounts_with_contracts(&records).unwrap();
    let imported = db
        .list_accounts()
        .unwrap()
        .into_iter()
        .filter(|account| account.id.starts_with("migration-"))
        .map(|account| account.id)
        .collect::<Vec<_>>();
    assert_eq!(imported, ["migration-go", "migration-custom"]);
    assert!(
        db.account_custom_config("migration-custom")
            .unwrap()
            .is_some()
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_capability_protocol_must_equal_the_config_protocol() {
    let dir = temp_data_dir("custom-protocol-mismatch");
    let db = Database::open(dir.clone()).unwrap();
    let mut custom = account("custom-protocol");
    custom.provider_id = CUSTOM_PROVIDER_ID.to_string();
    custom.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    custom.enabled = false;
    let mismatch = db.create_account_with_contract(
        &custom,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/messages".into(),
            upstream_protocol: UpstreamProtocolKind::Messages,
        }),
        &[AccountModelCapabilityInput {
            public_model: "org/model".into(),
            upstream_model: "org/model".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: None,
        }],
    );
    assert!(
        mismatch
            .unwrap_err()
            .to_string()
            .contains("must equal account custom_config.upstream_protocol")
    );
    assert!(db.get_account("custom-protocol").unwrap().is_none());

    db.create_account_with_contract(
        &custom,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/messages".into(),
            upstream_protocol: UpstreamProtocolKind::Messages,
        }),
        &[AccountModelCapabilityInput {
            public_model: "org/model".into(),
            upstream_model: "org/model".into(),
            protocol: UpstreamProtocolKind::Messages,
            source: None,
        }],
    )
    .unwrap();
    let stored = db
        .list_account_model_capabilities("custom-protocol")
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].protocol, UpstreamProtocolKind::Messages);

    let rejected = db.replace_account_model_capabilities(
        "custom-protocol",
        &[AccountModelCapabilityInput {
            public_model: "org/other".into(),
            upstream_model: "org/other".into(),
            protocol: UpstreamProtocolKind::Responses,
            source: None,
        }],
    );
    assert!(
        rejected
            .unwrap_err()
            .to_string()
            .contains("must equal account custom_config.upstream_protocol")
    );
    let kept = db
        .list_account_model_capabilities("custom-protocol")
        .unwrap();
    assert_eq!(kept.len(), 1);
    assert!(kept.iter().all(|row| row.public_model == "org/model"));

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_capabilities_allow_shared_upstream_but_reject_duplicate_public_names() {
    let dir = temp_data_dir("custom-model-mapping-uniqueness");
    let db = Database::open(dir.clone()).unwrap();
    let mut custom = account("custom-mapping");
    custom.provider_id = CUSTOM_PROVIDER_ID.to_string();
    custom.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    custom.enabled = false;
    db.create_account_with_contract(
        &custom,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        }),
        &[
            AccountModelCapabilityInput {
                public_model: "public-one".into(),
                upstream_model: "shared-upstream:0731".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: None,
            },
            AccountModelCapabilityInput {
                public_model: "public-two".into(),
                upstream_model: "shared-upstream:0731".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: None,
            },
        ],
    )
    .unwrap();
    let saved = db
        .list_account_model_capabilities("custom-mapping")
        .unwrap();
    assert_eq!(saved.len(), 2);
    assert!(
        saved
            .iter()
            .all(|row| row.upstream_model == "shared-upstream:0731")
    );

    let duplicate = db.replace_account_model_capabilities(
        "custom-mapping",
        &[
            AccountModelCapabilityInput {
                public_model: "Public-One".into(),
                upstream_model: "upstream-a".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: None,
            },
            AccountModelCapabilityInput {
                public_model: "public-one".into(),
                upstream_model: "upstream-b".into(),
                protocol: UpstreamProtocolKind::ChatCompletions,
                source: None,
            },
        ],
    );
    assert!(
        duplicate
            .unwrap_err()
            .to_string()
            .contains("duplicate model capability")
    );
    assert_eq!(
        db.list_account_model_capabilities("custom-mapping")
            .unwrap(),
        saved
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_mutations_repend_but_keep_verified_accounts_enabled() {
    let dir = temp_data_dir("custom-lifecycle-stale");
    let db = Database::open(dir.clone()).unwrap();
    let mut custom = account("custom-stale");
    custom.provider_id = CUSTOM_PROVIDER_ID.to_string();
    custom.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    custom.enabled = false;
    db.create_account_with_contract(
        &custom,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        }),
        &[AccountModelCapabilityInput {
            public_model: "org/model".into(),
            upstream_model: "org/model".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: None,
        }],
    )
    .unwrap();
    db.set_account_verification(
        "custom-stale",
        ConnectionVerificationStatus::Verified,
        Some(Utc::now()),
        Some("previous"),
    )
    .unwrap();
    db.conn
        .execute(
            "UPDATE accounts SET enabled = 1 WHERE id = 'custom-stale'",
            [],
        )
        .unwrap();

    db.upsert_account_custom_config(
        "custom-stale",
        &AccountCustomConfigInput {
            endpoint_url: "https://api.example.net/v2/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        },
        false,
    )
    .unwrap();
    let after_url = db.get_account("custom-stale").unwrap().unwrap();
    let after_url_state = db
        .account_verification_state("custom-stale")
        .unwrap()
        .unwrap();
    assert!(after_url.enabled);
    assert_eq!(
        after_url_state.status,
        ConnectionVerificationStatus::Pending
    );
    assert!(after_url_state.connection_verified_at.is_none());
    assert!(after_url_state.verification_error.is_none());

    db.set_account_verification(
        "custom-stale",
        ConnectionVerificationStatus::Verified,
        Some(Utc::now()),
        None,
    )
    .unwrap();
    db.conn
        .execute(
            "UPDATE accounts SET enabled = 1 WHERE id = 'custom-stale'",
            [],
        )
        .unwrap();
    db.replace_account_model_capabilities(
        "custom-stale",
        &[AccountModelCapabilityInput {
            public_model: "org/other".into(),
            upstream_model: "org/other".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: None,
        }],
    )
    .unwrap();
    let after_caps = db.get_account("custom-stale").unwrap().unwrap();
    let after_caps_state = db
        .account_verification_state("custom-stale")
        .unwrap()
        .unwrap();
    assert!(after_caps.enabled);
    assert_eq!(
        after_caps_state.status,
        ConnectionVerificationStatus::Pending
    );
    assert!(after_caps_state.connection_verified_at.is_none());

    db.set_account_verification(
        "custom-stale",
        ConnectionVerificationStatus::Verified,
        Some(Utc::now()),
        Some("stale"),
    )
    .unwrap();
    db.conn
        .execute(
            "UPDATE accounts SET enabled = 1 WHERE id = 'custom-stale'",
            [],
        )
        .unwrap();
    db.update_account(
        "custom-stale",
        &AccountUpdate {
            key: Some("rotated".into()),
            enabled: Some(true),
            ..AccountUpdate::default()
        },
        Some("new-cipher"),
        None,
    )
    .unwrap();
    let after_key = db.get_account("custom-stale").unwrap().unwrap();
    let after_key_state = db
        .account_verification_state("custom-stale")
        .unwrap()
        .unwrap();
    assert!(after_key.enabled);
    assert_eq!(
        after_key_state.status,
        ConnectionVerificationStatus::Pending
    );
    assert!(after_key_state.connection_verified_at.is_none());
    assert!(after_key_state.verification_error.is_none());
    let caps_after_key = db.list_account_model_capabilities("custom-stale").unwrap();
    assert_eq!(caps_after_key.len(), 1);
    assert_eq!(caps_after_key[0].public_model, "org/other");

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_verification_cas_rejects_stale_key_config_caps_and_delete() {
    let dir = temp_data_dir("custom-verify-cas");
    let mut db = Database::open(dir.clone()).unwrap();
    let mut custom = account("custom-cas");
    custom.provider_id = CUSTOM_PROVIDER_ID.to_string();
    custom.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    custom.enabled = false;
    custom.key_cipher = "cipher-a".into();
    db.create_account_with_contract(
        &custom,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        }),
        &[AccountModelCapabilityInput {
            public_model: "one".into(),
            upstream_model: "one".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: None,
        }],
    )
    .unwrap();

    let contract = db
        .capture_custom_verification_contract("custom-cas")
        .unwrap()
        .unwrap();
    assert_eq!(contract.key_cipher, "cipher-a");
    assert_eq!(contract.capabilities[0].0, "one");

    db.update_account(
        "custom-cas",
        &AccountUpdate {
            key: Some("rotated".into()),
            ..AccountUpdate::default()
        },
        Some("cipher-b"),
        None,
    )
    .unwrap();
    assert!(
        !db.commit_custom_verification_if_contract_matches(
            &contract,
            ConnectionVerificationStatus::Verified,
            Some(Utc::now()),
            None,
        )
        .unwrap()
    );
    assert_eq!(
        db.account_verification_state("custom-cas")
            .unwrap()
            .unwrap()
            .status,
        ConnectionVerificationStatus::Pending
    );

    let after_key = db
        .capture_custom_verification_contract("custom-cas")
        .unwrap()
        .unwrap();
    db.upsert_account_custom_config(
        "custom-cas",
        &AccountCustomConfigInput {
            endpoint_url: "https://api.example.net/v2/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        },
        false,
    )
    .unwrap();
    assert!(
        !db.commit_custom_verification_if_contract_matches(
            &after_key,
            ConnectionVerificationStatus::Verified,
            Some(Utc::now()),
            None,
        )
        .unwrap()
    );

    let after_config = db
        .capture_custom_verification_contract("custom-cas")
        .unwrap()
        .unwrap();
    db.replace_account_model_capabilities(
        "custom-cas",
        &[AccountModelCapabilityInput {
            public_model: "two".into(),
            upstream_model: "two".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: None,
        }],
    )
    .unwrap();
    assert!(
        !db.commit_custom_verification_if_contract_matches(
            &after_config,
            ConnectionVerificationStatus::Verified,
            Some(Utc::now()),
            None,
        )
        .unwrap()
    );

    let matching = db
        .capture_custom_verification_contract("custom-cas")
        .unwrap()
        .unwrap();
    assert!(
        db.commit_custom_verification_if_contract_matches(
            &matching,
            ConnectionVerificationStatus::Verified,
            Some(Utc::now()),
            None,
        )
        .unwrap()
    );
    assert_eq!(
        db.account_verification_state("custom-cas")
            .unwrap()
            .unwrap()
            .status,
        ConnectionVerificationStatus::Verified
    );
    assert!(
        !db.commit_custom_verification_if_contract_matches(
            &matching,
            ConnectionVerificationStatus::Failed,
            None,
            Some("stale"),
        )
        .unwrap()
    );
    assert_eq!(
        db.account_verification_state("custom-cas")
            .unwrap()
            .unwrap()
            .status,
        ConnectionVerificationStatus::Verified
    );

    let mut leftover = account("custom-delete");
    leftover.provider_id = CUSTOM_PROVIDER_ID.to_string();
    leftover.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    leftover.enabled = false;
    db.create_account_with_contract(
        &leftover,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        }),
        &[AccountModelCapabilityInput {
            public_model: "one".into(),
            upstream_model: "one".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: None,
        }],
    )
    .unwrap();
    let deleted_contract = db
        .capture_custom_verification_contract("custom-delete")
        .unwrap()
        .unwrap();
    db.delete_account("custom-delete").unwrap();
    assert!(
        !db.commit_custom_verification_if_contract_matches(
            &deleted_contract,
            ConnectionVerificationStatus::Verified,
            Some(Utc::now()),
            None,
        )
        .unwrap()
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unroutable_catalog_plans_cannot_persist_enabled_true() {
    let dir = temp_data_dir("enablement-gate");
    let db = Database::open(dir.clone()).unwrap();
    let mut go = account("go-enabled");
    go.enabled = true;
    db.create_account(&go).unwrap();
    assert!(db.get_account("go-enabled").unwrap().unwrap().enabled);

    for plan in BUILTIN_PLANS
        .iter()
        .copied()
        .filter(|plan| !plan.routable && plan.offering.singleton_account_id.is_none())
    {
        let id = format!("draft-{}", plan.offering.offering_id);
        let mut draft = account(&id);
        draft.provider_id = plan.offering.provider_id.to_string();
        draft.offering_id = plan.offering.offering_id.to_string();
        draft.credential_kind = plan.offering.credential_kind;
        draft.quota_scope = plan.offering.quota_scope;
        draft.enabled = true;
        let error = db
            .create_account(&draft)
            .expect_err("enabled unroutable create must fail closed");
        assert!(
            error.to_string().contains("not routable"),
            "{}/{}: {error}",
            plan.offering.provider_id,
            plan.offering.offering_id
        );
        assert!(db.get_account(&id).unwrap().is_none());

        draft.enabled = false;
        if plan_requires_custom_config(plan) {
            db.create_account_with_contract(
                &draft,
                Some(&AccountCustomConfigInput {
                    endpoint_url: "https://api.example.com/v1/chat/completions".into(),
                    upstream_protocol: UpstreamProtocolKind::ChatCompletions,
                }),
                &[AccountModelCapabilityInput {
                    public_model: "org/model".into(),
                    upstream_model: "org/model".into(),
                    protocol: UpstreamProtocolKind::ChatCompletions,
                    source: None,
                }],
            )
            .unwrap();
        } else {
            db.create_account_with_contract(&draft, None, &[]).unwrap();
        }
        let stored = db.get_account(&id).unwrap().unwrap();
        assert!(!stored.enabled, "{id} draft must stay disabled");
        let before = stored.updated_at;

        let enable_error = db
            .update_account(
                &id,
                &AccountUpdate {
                    enabled: Some(true),
                    ..AccountUpdate::default()
                },
                None,
                None,
            )
            .expect_err("enable must fail closed");
        assert!(
            enable_error.to_string().contains("not routable"),
            "{id}: {enable_error}"
        );
        let after_reject = db.get_account(&id).unwrap().unwrap();
        assert!(!after_reject.enabled);
        assert_eq!(after_reject.updated_at, before);
        assert_eq!(after_reject.name, stored.name);

        db.update_account(
            &id,
            &AccountUpdate {
                name: Some(format!("{id}-renamed")),
                ..AccountUpdate::default()
            },
            None,
            None,
        )
        .unwrap();
        db.update_account(
            &id,
            &AccountUpdate {
                enabled: Some(false),
                ..AccountUpdate::default()
            },
            None,
            None,
        )
        .unwrap();
        let edited = db.get_account(&id).unwrap().unwrap();
        assert!(!edited.enabled);
        assert_eq!(edited.name, format!("{id}-renamed"));
    }

    db.update_account(
        "go-enabled",
        &AccountUpdate {
            enabled: Some(false),
            ..AccountUpdate::default()
        },
        None,
        None,
    )
    .unwrap();
    db.update_account(
        "go-enabled",
        &AccountUpdate {
            enabled: Some(true),
            ..AccountUpdate::default()
        },
        None,
        None,
    )
    .unwrap();
    assert!(db.get_account("go-enabled").unwrap().unwrap().enabled);

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn open_sanitizes_unroutable_catalog_leftovers_without_touching_go_zen_or_unknown() {
    let unroutable: Vec<_> = BUILTIN_PLANS
        .iter()
        .copied()
        .filter(|plan| !plan.routable)
        .collect();
    assert_eq!(
        unroutable
            .iter()
            .map(|plan| (plan.offering.provider_id, plan.offering.offering_id))
            .collect::<Vec<_>>(),
        Vec::<(&str, &str)>::new()
    );
    assert!(
        builtin_plan(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).is_some_and(|plan| plan.routable)
    );

    let dir = temp_data_dir("unroutable-sanitation");
    let db = Database::open(dir.clone()).unwrap();

    let mut go = account("go-keep");
    go.notes = Some("go-notes".into());
    db.create_account(&go).unwrap();
    leftover_enable(&db, "go-keep");

    let mut unknown = account("unknown-keep");
    unknown.notes = Some("unknown-notes".into());
    db.create_account(&unknown).unwrap();
    leftover_enable(&db, "unknown-keep");
    db.conn
        .execute(
            "UPDATE accounts
                 SET provider_id = 'unknown-provider', offering_id = 'unknown-offering'
                 WHERE id = 'unknown-keep'",
            [],
        )
        .unwrap();

    persist_unroutable_draft(
        &db,
        builtin_plan(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap(),
        "goat-pending",
        "goat-pending-notes",
    );
    leftover_enable(&db, "goat-pending");

    persist_unroutable_draft(
        &db,
        builtin_plan(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap(),
        "goat-verified",
        "goat-verified-notes",
    );
    db.conn
        .execute(
            "UPDATE accounts
                 SET enabled = 1, verification_status = 'verified', verification_error = NULL
                 WHERE id = 'goat-verified'",
            [],
        )
        .unwrap();

    persist_unroutable_draft(
        &db,
        builtin_plan(COMMAND_CODE_PROVIDER_ID, GOAT_OFFERING_ID).unwrap(),
        "goat-failed",
        "goat-failed-notes",
    );
    db.conn
        .execute(
            "UPDATE accounts
                 SET enabled = 1, verification_status = 'failed', verification_error = 'boom'
                 WHERE id = 'goat-failed'",
            [],
        )
        .unwrap();

    persist_unroutable_draft(
        &db,
        builtin_plan(CUSTOM_PROVIDER_ID, CUSTOM_API_OFFERING_ID).unwrap(),
        "draft-api",
        "draft-api-notes",
    );
    leftover_enable(&db, "draft-api");

    let zen_before = sanitation_snapshot(&db, ZEN_FREE_ACCOUNT_ID);
    let go_before = sanitation_snapshot(&db, "go-keep");
    let unknown_before = sanitation_snapshot(&db, "unknown-keep");
    let goat_pending_before = sanitation_snapshot(&db, "goat-pending");
    let goat_verified_before = sanitation_snapshot(&db, "goat-verified");
    let goat_failed_before = sanitation_snapshot(&db, "goat-failed");
    let custom_before = sanitation_snapshot(&db, "draft-api");
    assert!(go_before.enabled);
    assert!(custom_before.enabled);
    assert!(unknown_before.enabled);
    assert!(goat_pending_before.enabled);
    assert!(goat_verified_before.enabled);
    assert!(goat_failed_before.enabled);
    assert_eq!(
        goat_pending_before.verification,
        ConnectionVerificationStatus::NotRequired
    );
    assert_eq!(
        goat_verified_before.verification,
        ConnectionVerificationStatus::Verified
    );
    assert_eq!(
        goat_failed_before.verification,
        ConnectionVerificationStatus::Failed
    );

    drop(db);
    let db = Database::open(dir.clone()).unwrap();

    let zen_after = sanitation_snapshot(&db, ZEN_FREE_ACCOUNT_ID);
    let go_after = sanitation_snapshot(&db, "go-keep");
    let unknown_after = sanitation_snapshot(&db, "unknown-keep");
    assert_eq!(zen_after, zen_before);
    assert_eq!(go_after, go_before);
    assert_eq!(unknown_after, unknown_before);

    let goat_pending_after = sanitation_snapshot(&db, "goat-pending");
    assert_eq!(goat_pending_after, goat_pending_before);
    assert!(goat_pending_after.enabled);
    assert_eq!(
        goat_pending_after.verification,
        ConnectionVerificationStatus::NotRequired
    );

    let goat_verified_after = sanitation_snapshot(&db, "goat-verified");
    assert_eq!(goat_verified_after.name, goat_verified_before.name);
    assert_eq!(goat_verified_after.notes, goat_verified_before.notes);
    assert_eq!(
        goat_verified_after.updated_at,
        goat_verified_before.updated_at
    );
    assert!(goat_verified_after.enabled);
    assert_eq!(
        goat_verified_after.verification,
        ConnectionVerificationStatus::NotRequired
    );

    let goat_failed_after = sanitation_snapshot(&db, "goat-failed");
    assert_eq!(goat_failed_after.name, goat_failed_before.name);
    assert_eq!(goat_failed_after.notes, goat_failed_before.notes);
    assert_eq!(goat_failed_after.updated_at, goat_failed_before.updated_at);
    assert!(goat_failed_after.enabled);
    assert_eq!(
        goat_failed_after.verification,
        ConnectionVerificationStatus::NotRequired
    );
    assert!(goat_failed_after.verification_error.is_none());

    let custom_after = sanitation_snapshot(&db, "draft-api");
    assert_eq!(custom_after, custom_before);
    assert!(
        custom_after.enabled,
        "now-routable Custom leftovers must not be disabled at open"
    );

    let first_pass: Vec<_> = [
        ZEN_FREE_ACCOUNT_ID,
        "go-keep",
        "unknown-keep",
        "goat-pending",
        "goat-verified",
        "goat-failed",
        "draft-api",
    ]
    .into_iter()
    .map(|id| (id.to_string(), sanitation_snapshot(&db, id)))
    .collect();

    drop(db);
    let db = Database::open(dir.clone()).unwrap();
    for (id, expected) in &first_pass {
        assert_eq!(
            sanitation_snapshot(&db, id),
            *expected,
            "second open must be idempotent for {id}"
        );
    }

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v22_open_sanitizes_enabled_unroutable_catalog_rows() {
    let dir = temp_data_dir("v22-unroutable-sanitation");
    create_v22_fixture(&dir);
    let conn = Connection::open(dir.join("data.sqlite")).expect("v22 fixture should reopen");
    for plan in BUILTIN_PLANS.iter().filter(|plan| {
        !(plan.routable
            || plan.offering.provider_id == COMMAND_CODE_PROVIDER_ID
                && plan.offering.offering_id == GOAT_OFFERING_ID)
    }) {
        clone_account_row_as_enabled(
            &conn,
            "v22-goat",
            &format!("v22-{}", plan.offering.offering_id),
            plan.offering.provider_id,
            plan.offering.offering_id,
        );
    }
    clone_account_row_as_enabled(
        &conn,
        "v22-account",
        "v22-unknown",
        "unknown-provider",
        "unknown-offering",
    );
    drop(conn);

    let db = open_with_host_cipher(dir.clone()).expect("v22 database should migrate and sanitize");
    assert!(db.get_account("v22-account").unwrap().unwrap().enabled);
    assert!(db.get_account("v22-unknown").unwrap().unwrap().enabled);
    assert!(
        db.get_account(ZEN_FREE_ACCOUNT_ID)
            .unwrap()
            .unwrap()
            .enabled
    );
    assert!(!db.get_account("v22-goat").unwrap().unwrap().enabled);
    for plan in BUILTIN_PLANS.iter().filter(|plan| {
        !(plan.routable
            || plan.offering.provider_id == COMMAND_CODE_PROVIDER_ID
                && plan.offering.offering_id == GOAT_OFFERING_ID)
    }) {
        let id = format!("v22-{}", plan.offering.offering_id);
        let stored = db.get_account(&id).unwrap().unwrap();
        assert!(!stored.enabled, "{id}");
        assert_eq!(stored.provider_id, plan.offering.provider_id);
        assert_eq!(stored.offering_id, plan.offering.offering_id);
    }
    db.update_account(
        "v22-goat",
        &AccountUpdate {
            name: Some("v22-goat-renamed".into()),
            ..AccountUpdate::default()
        },
        None,
        None,
    )
    .unwrap();
    let renamed = db.get_account("v22-goat").unwrap().unwrap();
    assert!(!renamed.enabled);
    assert_eq!(renamed.name, "v22-goat-renamed");

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v23_migration_failure_rolls_back_to_usable_v22_source_and_backup() {
    let dir = temp_data_dir("v23-atomic-migration");
    create_v22_fixture(&dir);
    let conn = Connection::open(dir.join("data.sqlite")).expect("v22 fixture should reopen");
    conn.execute_batch(
        "CREATE TRIGGER fail_v23_migration
             BEFORE INSERT ON schema_version
             WHEN NEW.version = 23
             BEGIN
                 SELECT RAISE(ABORT, 'forced v23 migration failure');
             END;",
    )
    .expect("fault-injection trigger should install");
    drop(conn);

    assert!(Database::open(dir.clone()).is_err());
    let conn = Connection::open(dir.join("data.sqlite")).expect("db should reopen");
    let columns = conn
        .prepare("PRAGMA table_info(accounts)")
        .expect("table info should prepare")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("table info should query")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("columns should load");
    assert!(!columns.iter().any(|name| name == "verification_status"));
    assert!(!table_exists(&conn, "account_custom_configs").unwrap());
    assert_eq!(schema_version_on(&conn).unwrap(), 22);
    let preserved_account: (String, String, i64) = conn
        .query_row(
            "SELECT name, key_cipher, enabled FROM accounts WHERE id = 'v22-account'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("source account should remain readable");
    let preserved_goat: (String, i64) = conn
        .query_row(
            "SELECT name, enabled FROM accounts WHERE id = 'v22-goat'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("source GOAT account should remain readable");
    let preserved_log: (String, String, String, f64) = conn
        .query_row(
            "SELECT account_id, model, status, cost FROM forward_logs LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("source forward log should remain readable");
    assert_eq!(preserved_account.0, "v22-account");
    assert_eq!(preserved_account.2, 1);
    assert_fixture_account_cipher(&preserved_account.1);
    assert_eq!(preserved_goat, ("v22-goat".into(), 1));
    assert_eq!(
        preserved_log,
        ("v22-account".into(), "test".into(), "success".into(), 3.5)
    );
    drop(conn);

    let backups_before = pre_v23_backup_paths(&dir);
    assert_eq!(backups_before.len(), 1);
    let backup_bytes = fs::read(&backups_before[0]).expect("rollback backup should be readable");
    let backup = Connection::open_with_flags(&backups_before[0], OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("pre-v23 backup should open");
    assert_eq!(schema_version_on(&backup).unwrap(), 22);
    assert!(!table_has_column(&backup, "accounts", "verification_status").unwrap());
    drop(backup);

    assert!(Database::open(dir.clone()).is_err());
    assert_eq!(pre_v23_backup_paths(&dir), backups_before);
    assert_eq!(
        fs::read(&backups_before[0]).expect("rollback backup should remain readable"),
        backup_bytes
    );
    fs::remove_dir_all(dir).expect("test data dir should be removed");
}

struct V27HookGuard;
impl Drop for V27HookGuard {
    fn drop(&mut self) {
        v27_test_hooks::reset();
    }
}

fn arm_v27_fault(point: V27MigrationFault) -> V27HookGuard {
    v27_test_hooks::reset();
    v27_test_hooks::set_fault(Some(point));
    V27HookGuard
}

fn populate_v26_source(dir: &Path) -> (String, String) {
    let db = Database::open(dir.to_path_buf()).expect("fixture database should open");
    let mut v26_account = account("v26-account");
    v26_account.key_cipher = fixture_account_key_cipher();
    db.create_account(&v26_account)
        .expect("representative account should save");
    let now = Utc::now();
    db.insert_sub_gateway_key(&SubGatewayKey {
        id: "sub-v26".into(),
        name: "Laptop".into(),
        key: "ocg-v26-laptop".into(),
        enabled: true,
        deleted_at: None,
        created_at: now,
    })
    .expect("sub key should save");
    let config = serde_json::json!({
        "gateway_port": 9042,
        "gateway_key": "ocg-v26-primary",
        "upstream_base_url": "https://opencode.ai/zen/go",
    });
    db.set_config(&config.to_string())
        .expect("v26 config should persist");
    drop(db);
    reverse_current_to_v26(dir);
    ("ocg-v26-primary".into(), "ocg-v26-laptop".into())
}

#[test]
fn v27_fresh_database_skips_pre_v3_backup_and_has_one_primary() {
    let dir = temp_data_dir("v27-fresh");
    let db = Database::open(dir.clone()).unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert!(pre_v3_backup_paths(&dir).is_empty());
    assert!(table_exists(&db.conn, "access_keys").unwrap());
    assert!(!table_exists(&db.conn, "sub_gateway_keys").unwrap());
    for column in USAGE_SYNC_ACCOUNT_COLUMNS {
        assert!(!table_has_column(&db.conn, "accounts", column).unwrap());
    }
    let primary = db.primary_access_key_value().unwrap().unwrap();
    assert!(!primary.is_empty());
    assert_eq!(db.count_active_sub_gateway_keys().unwrap(), 0);
    db.migrate().unwrap();
    for column in USAGE_SYNC_ACCOUNT_COLUMNS {
        assert!(
            !table_has_column(&db.conn, "accounts", column).unwrap(),
            "replaying migrate must not resurrect {column}"
        );
    }
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_to_v28_adds_goat_model_access_without_replaying_v27() {
    let dir = temp_data_dir("v27-v28-migrate");
    populate_v26_source(&dir);
    let db_path = dir.join("data.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    let cipher = test_host_cipher();
    migrate_to_v27(&conn, &db_path, Some(cipher.as_ref()), false).unwrap();
    assert_eq!(schema_version_on(&conn).unwrap(), V27_SCHEMA_VERSION);
    assert!(!table_has_column(&conn, "accounts", "goat_model_access").unwrap());

    migrate_to_v28(&conn).unwrap();
    assert_eq!(schema_version_on(&conn).unwrap(), 28);
    assert!(table_has_column(&conn, "accounts", "goat_model_access").unwrap());
    let default_value: String = conn
        .query_row(
            "SELECT goat_model_access FROM accounts WHERE id = 'v26-account'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(default_value, "goat");
    for column in USAGE_SYNC_ACCOUNT_COLUMNS {
        assert!(!table_has_column(&conn, "accounts", column).unwrap());
    }

    drop(conn);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v28_to_v29_purges_scnet_accounts_and_acknowledgements() {
    let dir = temp_data_dir("v28-v29-scnet-purge");
    let db = Database::open(dir.clone()).unwrap();
    let mut leftover = account("scnet-leftover");
    leftover.provider_id = OPENCODE_PROVIDER_ID.into();
    leftover.offering_id = GO_OFFERING_ID.into();
    db.create_account(&leftover).unwrap();
    db.conn
        .execute(
            "UPDATE accounts
                 SET provider_id = 'scnet', offering_id = 'scnet-token-plan-basic'
                 WHERE id = 'scnet-leftover'",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "CREATE TABLE account_acknowledgements (
                    account_id TEXT NOT NULL,
                    acknowledgement_id TEXT NOT NULL,
                    version TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    accepted_at TEXT NOT NULL,
                    PRIMARY KEY (account_id, acknowledgement_id)
                )",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO account_acknowledgements
                 (account_id, acknowledgement_id, version, content_hash, accepted_at)
                 VALUES ('scnet-leftover', 'ack-scnet', '1', 'hash', ?1)",
            [Utc::now().to_rfc3339()],
        )
        .unwrap();
    db.conn
        .execute_batch(
            "DELETE FROM schema_version;
                 INSERT OR REPLACE INTO schema_version (version) VALUES (28);",
        )
        .unwrap();
    drop(db);

    let db = Database::open(dir.clone()).unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert!(
        db.get_account("scnet-leftover").unwrap().is_none(),
        "v29 must delete SCNet account rows"
    );
    let ack_table_exists: bool = db
        .conn
        .query_row(
            "SELECT EXISTS(
                    SELECT 1 FROM sqlite_master
                    WHERE type = 'table' AND name = 'account_acknowledgements'
                )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        !ack_table_exists,
        "v29 must drop the account_acknowledgements table"
    );
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v31_to_v32_collapses_custom_protocols_and_disables_the_account() {
    let dir = temp_data_dir("v31-v32-single-protocol");
    let db = Database::open(dir.clone()).unwrap();
    let mut custom = account("custom-v31");
    custom.provider_id = CUSTOM_PROVIDER_ID.to_string();
    custom.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    custom.enabled = false;
    db.create_account_with_contract(
        &custom,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/messages".into(),
            upstream_protocol: UpstreamProtocolKind::Messages,
        }),
        &[AccountModelCapabilityInput {
            public_model: "org/model".into(),
            upstream_model: "org/model".into(),
            protocol: UpstreamProtocolKind::Messages,
            source: None,
        }],
    )
    .unwrap();
    db.conn
        .execute_batch(
            "DROP TABLE account_custom_configs;
                 CREATE TABLE account_custom_configs (
                    account_id TEXT PRIMARY KEY,
                    base_url TEXT NOT NULL,
                    upstream_protocols TEXT NOT NULL,
                    auth_scheme TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
                 );
                 INSERT INTO account_custom_configs (
                    account_id, base_url, upstream_protocols, auth_scheme, created_at, updated_at
                 ) VALUES (
                    'custom-v31', 'https://api.example.com/v1',
                    '[\"messages\",\"responses\",\"chat_completions\"]', 'x_api_key',
                    '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
                 );
                 INSERT INTO account_model_capabilities
                    (account_id, model_id, protocol, source)
                 VALUES ('custom-v31', 'org/model', 'chat_completions', 'manual');
                 UPDATE accounts
                    SET enabled = 1, verification_status = 'verified',
                        connection_verified_at = '2026-01-01T00:00:00Z'
                  WHERE id = 'custom-v31';
                 DELETE FROM schema_version;
                 INSERT OR REPLACE INTO schema_version (version) VALUES (31);",
        )
        .unwrap();
    drop(db);

    let db = Database::open(dir.clone()).unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    let config = db.account_custom_config("custom-v31").unwrap().unwrap();
    assert_eq!(
        config.upstream_protocol,
        UpstreamProtocolKind::ChatCompletions
    );
    assert_eq!(
        config.endpoint_url,
        "https://api.example.com/v1/chat/completions"
    );
    let migrated = db.get_account("custom-v31").unwrap().unwrap();
    assert!(!migrated.enabled);
    let migrated_state = db
        .account_verification_state("custom-v31")
        .unwrap()
        .unwrap();
    assert_eq!(migrated_state.status, ConnectionVerificationStatus::Pending);
    assert!(migrated_state.connection_verified_at.is_none());
    let capabilities = db.list_account_model_capabilities("custom-v31").unwrap();
    assert_eq!(capabilities.len(), 1);
    assert_eq!(
        capabilities[0].protocol,
        UpstreamProtocolKind::ChatCompletions
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v32_to_v33_backfills_public_and_upstream_identities_for_custom_and_goat() {
    let dir = temp_data_dir("v32-v33-model-mapping");
    let db = Database::open(dir.clone()).unwrap();

    let mut custom = account("custom-v32");
    custom.provider_id = CUSTOM_PROVIDER_ID.to_string();
    custom.offering_id = CUSTOM_API_OFFERING_ID.to_string();
    custom.enabled = false;
    db.create_account_with_contract(
        &custom,
        Some(&AccountCustomConfigInput {
            endpoint_url: "https://api.example.com/v1/chat/completions".into(),
            upstream_protocol: UpstreamProtocolKind::ChatCompletions,
        }),
        &[AccountModelCapabilityInput {
            public_model: "custom-public".into(),
            upstream_model: "custom-public".into(),
            protocol: UpstreamProtocolKind::ChatCompletions,
            source: Some("manual".into()),
        }],
    )
    .unwrap();

    let mut goat = account("goat-v32");
    goat.provider_id = COMMAND_CODE_PROVIDER_ID.to_string();
    goat.offering_id = GOAT_OFFERING_ID.to_string();
    goat.enabled = false;
    db.create_account(&goat).unwrap();
    persist_goat_catalog_on(&db.conn, &goat.id, &["goat/model".into()], Some(Utc::now())).unwrap();
    drop(db);

    let conn = Connection::open(dir.join("data.sqlite")).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys = OFF;
             DROP INDEX IF EXISTS idx_account_model_capabilities_account;
             CREATE TABLE account_model_capabilities_v32 (
                account_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                protocol TEXT NOT NULL,
                verified_at TEXT,
                source TEXT NOT NULL DEFAULT 'manual',
                PRIMARY KEY (account_id, model_id, protocol),
                FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
             );
             INSERT INTO account_model_capabilities_v32
                (account_id, model_id, protocol, verified_at, source)
             SELECT account_id, model_id, protocol, verified_at, source
               FROM account_model_capabilities;
             DROP TABLE account_model_capabilities;
             ALTER TABLE account_model_capabilities_v32
                RENAME TO account_model_capabilities;
             CREATE INDEX idx_account_model_capabilities_account
                ON account_model_capabilities(account_id);
             DELETE FROM schema_version;
             INSERT INTO schema_version (version) VALUES (32);
             PRAGMA foreign_keys = ON;",
    )
    .unwrap();
    drop(conn);

    let migrated = Database::open(dir.clone()).unwrap();
    assert_eq!(migrated.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    for (account_id, expected) in [("custom-v32", "custom-public"), ("goat-v32", "goat/model")] {
        let capabilities = migrated
            .list_account_model_capabilities(account_id)
            .unwrap();
        assert_eq!(capabilities.len(), 1);
        assert_eq!(capabilities[0].public_model, expected);
        assert_eq!(capabilities[0].upstream_model, expected);
    }

    drop(migrated);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v33_to_v34_adds_empty_cpa_singleton_configuration_table() {
    let dir = temp_data_dir("v33-v34-cpa");
    let db = Database::open(dir.clone()).unwrap();
    drop(db);
    let conn = Connection::open(dir.join("data.sqlite")).unwrap();
    conn.execute_batch(
        "DROP TABLE cpa_integration;
             DELETE FROM schema_version;
             INSERT INTO schema_version (version) VALUES (33);",
    )
    .unwrap();
    drop(conn);

    let migrated = Database::open(dir.clone()).unwrap();
    assert_eq!(migrated.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert!(table_exists(&migrated.conn, "cpa_integration").unwrap());
    assert!(migrated.cpa_integration().unwrap().is_none());
    drop(migrated);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cpa_singleton_upsert_catalog_and_disconnect_are_idempotent_and_atomic() {
    let dir = temp_data_dir("cpa-singleton-lifecycle");
    let db = open_with_host_cipher(dir.clone()).unwrap();
    let now = Utc::now();
    let mut cpa_account = account(CPA_ACCOUNT_ID);
    cpa_account.provider_id = CPA_PROVIDER_ID.to_string();
    cpa_account.offering_id = CPA_OFFERING_ID.to_string();
    cpa_account.credential_kind = CredentialKind::ApiKey;
    cpa_account.quota_scope = QuotaScope::Key;
    cpa_account.name = CPA_ACCOUNT_NAME.to_string();
    cpa_account.key_cipher = test_host_cipher().encrypt("cpa-inference").unwrap();
    cpa_account.enabled = false;
    cpa_account.account_type = AccountType::Key;
    cpa_account.setup_step = AccountSetupStep::Ready;
    cpa_account.created_at = now;
    cpa_account.updated_at = now;
    let management_cipher = test_host_cipher().encrypt("cpa-management").unwrap();

    db.upsert_cpa_integration(&cpa_account, "http://127.0.0.1:8317", &management_cipher)
        .unwrap();
    cpa_account.enabled = true;
    db.upsert_cpa_integration(&cpa_account, "http://127.0.0.1:9317", &management_cipher)
        .unwrap();
    let record = db.cpa_integration().unwrap().unwrap();
    assert_eq!(record.account_id, CPA_ACCOUNT_ID);
    assert_eq!(record.base_url, "http://127.0.0.1:9317");
    assert_eq!(record.management_key_cipher, management_cipher);
    assert!(db.get_account(CPA_ACCOUNT_ID).unwrap().unwrap().enabled);

    db.conn
        .execute(
            "UPDATE accounts SET auth_error = '401' WHERE id = ?1",
            [CPA_ACCOUNT_ID],
        )
        .unwrap();
    db.upsert_cpa_integration(&cpa_account, "http://127.0.0.1:9317", &management_cipher)
        .unwrap();
    assert_eq!(
        db.get_account(CPA_ACCOUNT_ID)
            .unwrap()
            .unwrap()
            .auth_error
            .as_deref(),
        Some("401"),
        "saving the same inference cipher must preserve an existing breaker"
    );
    cpa_account.key_cipher = test_host_cipher().encrypt("cpa-inference-fixed").unwrap();
    db.upsert_cpa_integration(&cpa_account, "http://127.0.0.1:9317", &management_cipher)
        .unwrap();
    assert!(
        db.get_account(CPA_ACCOUNT_ID)
            .unwrap()
            .unwrap()
            .auth_error
            .is_none(),
        "replacing the inference cipher must clear the stale 401 breaker"
    );

    db.replace_cpa_model_catalog(
        &["gpt-5.6-sol".into(), "unknown-cpa-model".into()],
        "http://127.0.0.1:9317",
        now,
    )
    .unwrap();
    assert_eq!(db.cpa_model_catalog().unwrap().unwrap().models.len(), 2);

    db.delete_cpa_integration().unwrap();
    db.delete_cpa_integration().unwrap();
    assert!(db.cpa_integration().unwrap().is_none());
    assert!(db.cpa_model_catalog().unwrap().is_none());
    assert!(db.get_account(CPA_ACCOUNT_ID).unwrap().is_none());
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v26_to_v27_copies_keys_drops_columns_and_writes_hashed_backup() {
    let dir = temp_data_dir("v26-v27-migrate");
    let (primary, laptop) = populate_v26_source(&dir);
    let cipher_bytes = {
        let conn = Connection::open(dir.join("data.sqlite")).unwrap();
        let key: String = conn
            .query_row(
                "SELECT key_cipher FROM accounts WHERE id = 'v26-account'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);
        key
    };

    let db = open_with_host_cipher(dir.clone()).expect("v26 database should migrate to v27");
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert_eq!(
        db.primary_access_key_value().unwrap().as_deref(),
        Some(primary.as_str())
    );
    let subs = db.list_active_sub_gateway_keys().unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].id, "sub-v26");
    assert_eq!(subs[0].key, laptop);
    assert!(!table_exists(&db.conn, "sub_gateway_keys").unwrap());
    for column in USAGE_SYNC_ACCOUNT_COLUMNS {
        assert!(!table_has_column(&db.conn, "accounts", column).unwrap());
    }
    let stored_cipher: String = db
        .conn
        .query_row(
            "SELECT key_cipher FROM accounts WHERE id = 'v26-account'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        stored_cipher, cipher_bytes,
        "ciphertext bytes must be preserved"
    );
    let config: serde_json::Value =
        serde_json::from_str(&db.get_setting("config").unwrap().unwrap()).unwrap();
    assert_eq!(config["gateway_key"], "");
    drop(db);

    let backups = pre_v3_backup_paths(&dir);
    assert_eq!(backups.len(), 1);
    let backup_name = backups[0]
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap();
    let timestamp = backup_name
        .strip_prefix(PRE_V3_BACKUP_FILE_PREFIX)
        .and_then(|name| name.strip_suffix(".bak"))
        .unwrap();
    assert_eq!(timestamp.len(), 25);
    let backup =
        Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema_version_on(&backup).unwrap(), V26_SCHEMA_VERSION);
    sqlite_quick_check(&backup).unwrap();
    assert!(table_exists(&backup, "sub_gateway_keys").unwrap());
    assert!(table_has_column(&backup, "accounts", "usage_sync_last_success_at").unwrap());
    drop(backup);
    let digest = sha256_file(&backups[0]).unwrap();
    let evidence = fs::read_to_string(format!("{}.sha256", backups[0].display())).unwrap();
    assert!(evidence.starts_with(&digest));
    assert!(evidence.contains(backup_name));

    let reopened = open_with_host_cipher(dir.clone()).unwrap();
    assert_eq!(reopened.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(reopened);
    assert_eq!(pre_v3_backup_paths(&dir), backups);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v21_migrates_through_v26_before_v27_backup() {
    let dir = temp_data_dir("v21-through-v26-v27");
    create_v21_fixture(&dir, false);
    let db = open_with_host_cipher(dir.clone()).expect("v21 database should migrate");
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(db);
    let pre_v3 = pre_v3_backup_paths(&dir);
    assert_eq!(pre_v3.len(), 1);
    let backup = Connection::open_with_flags(&pre_v3[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(schema_version_on(&backup).unwrap(), V26_SCHEMA_VERSION);
    drop(backup);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_fault_before_schema_version_leaves_usable_v26_source() {
    let dir = temp_data_dir("v27-interrupt");
    populate_v26_source(&dir);
    let _guard = arm_v27_fault(V27MigrationFault::BeforeSchemaVersion);
    assert!(open_with_host_cipher(dir.clone()).is_err());
    drop(_guard);
    let conn = Connection::open(dir.join("data.sqlite")).unwrap();
    assert_eq!(schema_version_on(&conn).unwrap(), V26_SCHEMA_VERSION);
    assert!(table_exists(&conn, "sub_gateway_keys").unwrap());
    assert!(!table_exists(&conn, "access_keys").unwrap());
    assert!(table_has_column(&conn, "accounts", "usage_sync_last_success_at").unwrap());
    drop(conn);
    assert_eq!(pre_v3_backup_paths(&dir).len(), 1);
    let db = open_with_host_cipher(dir.clone()).expect("v26 source should still migrate");
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_duplicate_start_converges_on_one_primary() {
    let dir = temp_data_dir("v27-duplicate-start");
    populate_v26_source(&dir);
    let first = dir.clone();
    let second = dir.clone();
    let threads = [
        std::thread::spawn(move || open_with_host_cipher(first)),
        std::thread::spawn(move || open_with_host_cipher(second)),
    ];
    let results = threads
        .into_iter()
        .map(|thread| thread.join().expect("open thread should finish"))
        .collect::<Vec<_>>();
    assert!(
        results.iter().any(|result| result.is_ok()),
        "at least one opener must finish v27: {:?}",
        results
            .iter()
            .map(|result| result
                .as_ref()
                .map(|_| "ok")
                .map_err(|error| error.to_string()))
            .collect::<Vec<_>>()
    );
    for db in results.into_iter().flatten() {
        assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        drop(db);
    }
    let db = open_with_host_cipher(dir.clone()).unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    let count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM access_keys WHERE is_primary = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_wrong_cipher_fails_closed_without_claiming_v27() {
    let dir = temp_data_dir("v27-wrong-cipher");
    let right: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("right-secret"));
    let db = Database::open_with_cipher(dir.clone(), right.clone()).unwrap();
    let mut enc = account("enc-account");
    enc.key_cipher = right.encrypt("sk-live").unwrap();
    enc.password_cipher = Some(right.encrypt("pw-live").unwrap());
    db.create_account(&enc).unwrap();
    drop(db);
    reverse_current_to_v26(&dir);

    struct FailingCipher;
    impl KeyCipher for FailingCipher {
        fn encrypt(&self, plaintext: &str) -> anyhow::Result<String> {
            Ok(plaintext.to_string())
        }
        fn decrypt(&self, _ciphertext: &str) -> anyhow::Result<String> {
            anyhow::bail!("wrong cipher")
        }
    }
    let failing: Arc<dyn KeyCipher + Send + Sync> = Arc::new(FailingCipher);
    assert!(Database::open_with_cipher(dir.clone(), failing).is_err());
    let conn = Connection::open(dir.join("data.sqlite")).unwrap();
    assert_eq!(schema_version_on(&conn).unwrap(), V26_SCHEMA_VERSION);
    drop(conn);

    let recovered = Database::open_with_cipher(dir.clone(), right).unwrap();
    assert_eq!(recovered.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(recovered);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_open_without_cipher_cannot_bypass_ciphertext() {
    let dir = temp_data_dir("v27-open-bypass");
    let cipher: Arc<dyn KeyCipher + Send + Sync> = Arc::new(StaticKeyCipher::new("host-secret"));
    let db = Database::open_with_cipher(dir.clone(), cipher.clone()).unwrap();
    let mut enc = account("enc-bypass");
    enc.key_cipher = cipher.encrypt("sk-bypass").unwrap();
    db.create_account(&enc).unwrap();
    drop(db);
    reverse_current_to_v26(&dir);
    let error = match Database::open(dir.clone()) {
        Ok(_) => panic!("ciphertext must require the host cipher"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("open_with_cipher")
            || error.to_string().contains("host encryption cipher"),
        "{error}"
    );
    let conn = Connection::open(dir.join("data.sqlite")).unwrap();
    assert_eq!(schema_version_on(&conn).unwrap(), V26_SCHEMA_VERSION);
    drop(conn);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_corrupt_account_cipher_fails_before_backup() {
    let dir = temp_data_dir("v27-corrupt-account-cipher");
    populate_v26_source(&dir);
    let conn = Connection::open(dir.join("data.sqlite")).unwrap();
    conn.execute(
        "UPDATE accounts SET key_cipher = '!!!not-base64!!!' WHERE id = 'v26-account'",
        [],
    )
    .unwrap();
    drop(conn);
    let error = match open_with_host_cipher(dir.clone()) {
        Ok(_) => panic!("corrupt account cipher must fail closed"),
        Err(error) => error,
    };
    let message = format!("{error:#}");
    assert!(
        message.contains("key_cipher"),
        "corrupt account cipher must name the column: {message}"
    );
    assert!(
        pre_v3_backup_paths(&dir).is_empty(),
        "corrupt account cipher must fail before the pre-v3 backup"
    );
    let conn = Connection::open(dir.join("data.sqlite")).unwrap();
    assert_eq!(schema_version_on(&conn).unwrap(), V26_SCHEMA_VERSION);
    drop(conn);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_base64_looking_plaintext_access_keys_migrate_without_cipher() {
    let dir = temp_data_dir("v27-b64-plaintext-keys");
    let primary = "ABCDEFGHIJKLMNOPQRSTUVWX";
    let sub = "ZYXWVUTSRQPONMLKJIHGFEDC";
    assert_eq!(primary.len(), 24);
    assert_eq!(sub.len(), 24);
    let db = Database::open(dir.to_path_buf()).unwrap();
    db.insert_sub_gateway_key(&SubGatewayKey {
        id: "sub-b64".into(),
        name: "Laptop".into(),
        key: sub.into(),
        enabled: true,
        deleted_at: None,
        created_at: Utc::now(),
    })
    .unwrap();
    let config = serde_json::json!({
        "gateway_port": 9042,
        "gateway_key": primary,
        "upstream_base_url": "https://opencode.ai/zen/go",
    });
    db.set_config(&config.to_string()).unwrap();
    drop(db);
    reverse_current_to_v26(&dir);
    let db = Database::open(dir.clone()).expect(
        "24-character base64-looking plaintext primary/sub keys must migrate without a host cipher",
    );
    assert_eq!(
        db.primary_access_key_value().unwrap().as_deref(),
        Some(primary)
    );
    let subs = db.list_active_sub_gateway_keys().unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].key, sub);
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_corrupted_source_fails_quick_check_without_claiming_v27() {
    let dir = temp_data_dir("v27-corrupt");
    populate_v26_source(&dir);
    let path = dir.join("data.sqlite");
    fs::write(&path, b"not a sqlite database").unwrap();
    assert!(Database::open(dir.clone()).is_err());
    let raw = fs::read(&path).unwrap();
    assert_eq!(&raw, b"not a sqlite database");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_backup_includes_wal_committed_rows() {
    let dir = temp_data_dir("v27-wal-backup");
    populate_v26_source(&dir);
    let path = dir.join("data.sqlite");
    let writer = Connection::open(&path).unwrap();
    writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    let _mode: String = writer
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .unwrap();
    writer
        .execute(
            "INSERT INTO settings (key, value) VALUES ('wal-marker', 'visible')
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        )
        .unwrap();
    let db = open_with_host_cipher(dir.clone()).expect("WAL source should migrate");
    drop(db);
    drop(writer);
    let backups = pre_v3_backup_paths(&dir);
    assert_eq!(backups.len(), 1);
    let backup =
        Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let marker: String = backup
        .query_row(
            "SELECT value FROM settings WHERE key = 'wal-marker'",
            [],
            |row| row.get(0),
        )
        .expect("VACUUM INTO must include WAL-committed rows");
    assert_eq!(marker, "visible");
    drop(backup);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_vacuum_into_writer_rejects_stale_backup_and_retries() {
    let dir = temp_data_dir("v27-vacuum-race");
    populate_v26_source(&dir);
    v27_test_hooks::reset();
    v27_test_hooks::set_race_during_vacuum(true);
    let _guard = V27HookGuard;
    let db = open_with_host_cipher(dir.clone()).expect("raced VACUUM INTO should retry and finish");
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    drop(db);
    let backups = pre_v3_backup_paths(&dir);
    assert!(
        backups.len() >= 2,
        "the first backup must be rejected and a fresh backup taken, got {backups:?}"
    );
    let accepted = backups.last().expect("accepted backup should exist");
    let backup = Connection::open_with_flags(accepted, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let marker: String = backup
        .query_row(
            "SELECT value FROM settings WHERE key = 'v27-vacuum-race'",
            [],
            |row| row.get(0),
        )
        .expect("accepted backup must contain the row committed during VACUUM INTO");
    assert_eq!(marker, "committed");
    drop(backup);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_set_config_is_atomic_with_the_primary_row() {
    let dir = temp_data_dir("v27-config-atomic");
    let db = Database::open(dir.clone()).unwrap();
    let original = db.primary_access_key_value().unwrap().unwrap();
    let initial = serde_json::json!({
        "gateway_port": 9042,
        "gateway_key": original,
        "upstream_base_url": "https://opencode.ai/zen/go",
        "connect_timeout_secs": 30,
        "non_stream_timeout_secs": 900,
        "stream_idle_timeout_secs": 300,
    });
    db.set_config(&initial.to_string()).unwrap();
    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_primary_update
                 BEFORE UPDATE OF key ON access_keys
                 WHEN NEW.is_primary = 1
                 BEGIN
                     SELECT RAISE(ABORT, 'forced primary update failure');
                 END;",
        )
        .unwrap();
    let rotated = serde_json::json!({
        "gateway_port": 9042,
        "gateway_key": "ocg-rotated-primary",
        "upstream_base_url": "https://opencode.ai/zen/go",
        "connect_timeout_secs": 30,
        "non_stream_timeout_secs": 900,
        "stream_idle_timeout_secs": 300,
    });
    assert!(db.set_config(&rotated.to_string()).is_err());
    assert_eq!(
        db.primary_access_key_value().unwrap().as_deref(),
        Some(original.as_str())
    );
    let stored: serde_json::Value =
        serde_json::from_str(&db.get_setting("config").unwrap().unwrap()).unwrap();
    assert_eq!(stored["gateway_key"], "");
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_primary_row_cannot_be_disabled_or_deleted() {
    let dir = temp_data_dir("v27-primary-protect");
    let db = Database::open(dir.clone()).unwrap();
    assert!(
        !db.set_sub_gateway_key_enabled(PRIMARY_KEY_ID, false)
            .unwrap()
    );
    assert!(
        !db.soft_delete_sub_gateway_key(PRIMARY_KEY_ID, Utc::now())
            .unwrap()
    );
    assert!(
        db.conn
            .execute(
                "UPDATE access_keys SET enabled = 0 WHERE id = ?1",
                [PRIMARY_KEY_ID],
            )
            .is_err()
    );
    assert!(
        db.conn
            .execute("DELETE FROM access_keys WHERE id = ?1", [PRIMARY_KEY_ID])
            .is_err()
    );
    let primary = db.primary_access_key_value().unwrap().unwrap();
    assert!(!primary.is_empty());
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v27_foreign_keys_and_row_conservation_hold() {
    let dir = temp_data_dir("v27-fk-rows");
    populate_v26_source(&dir);
    let before = {
        let conn = Connection::open(dir.join("data.sqlite")).unwrap();
        let accounts: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
            .unwrap();
        let subs: i64 = conn
            .query_row("SELECT COUNT(*) FROM sub_gateway_keys", [], |row| {
                row.get(0)
            })
            .unwrap();
        drop(conn);
        (accounts, subs)
    };
    let db = open_with_host_cipher(dir.clone()).unwrap();
    sqlite_foreign_key_check(&db.conn).unwrap();
    let accounts: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
        .unwrap();
    let keys: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM access_keys", [], |row| row.get(0))
        .unwrap();
    assert_eq!(accounts, before.0);
    assert_eq!(keys, before.1 + 1);
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v31_migration_creates_override_table() {
    let dir = temp_data_dir("v31-migration");
    let db = Database::open(dir.clone()).unwrap();
    db.conn
        .execute_batch(
            "DROP TABLE IF EXISTS provider_contract_model_protocol_overrides;
                 DELETE FROM schema_version;
                 INSERT OR REPLACE INTO schema_version (version) VALUES (30);",
        )
        .unwrap();
    drop(db);

    let db = Database::open(dir.clone()).unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    let table_exists: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'provider_contract_model_protocol_overrides'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table_exists, 1);
    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn model_protocol_override_upsert_and_auto_delete_round_trip() {
    let dir = temp_data_dir("override-roundtrip");
    let db = Database::open(dir.clone()).unwrap();
    let now = Utc::now();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);

    db.set_model_protocol_overrides(
        &scope,
        &[
            (
                "glm-5.2".into(),
                UpstreamProtocolKind::ChatCompletions,
                ProtocolOverrideState::ForceOn,
            ),
            (
                "glm-5.2".into(),
                UpstreamProtocolKind::Messages,
                ProtocolOverrideState::ForceOff,
            ),
        ],
        now,
    )
    .unwrap();

    let persisted = db.load_persisted_contracts().unwrap();
    let overrides = persisted.overrides.get(&scope).unwrap();
    assert_eq!(overrides.len(), 2);
    assert!(overrides.iter().any(|row| row.model_id == "glm-5.2"
        && row.protocol == UpstreamProtocolKind::ChatCompletions
        && row.state == ProtocolOverrideState::ForceOn));
    assert!(overrides.iter().any(|row| row.model_id == "glm-5.2"
        && row.protocol == UpstreamProtocolKind::Messages
        && row.state == ProtocolOverrideState::ForceOff));

    db.set_model_protocol_overrides(
        &scope,
        &[(
            "glm-5.2".into(),
            UpstreamProtocolKind::ChatCompletions,
            ProtocolOverrideState::Auto,
        )],
        now,
    )
    .unwrap();

    let persisted = db.load_persisted_contracts().unwrap();
    let overrides = persisted.overrides.get(&scope).unwrap();
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].protocol, UpstreamProtocolKind::Messages);
    assert_eq!(overrides[0].state, ProtocolOverrideState::ForceOff);

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn catalog_refresh_defaults_only_new_models_off_and_preserves_existing_choices() {
    let dir = temp_data_dir("catalog-refresh-default-off");
    let db = Database::open(dir.clone()).unwrap();
    let now = Utc::now();
    let scope = ContractScope::provider(OPENCODE_PROVIDER_ID);

    db.set_contract_catalog(
        &scope,
        &["grok-4.5".into()],
        None,
        crate::provider_contracts::CATALOG_SOURCE_STATIC,
        "",
        now,
    )
    .unwrap();
    db.set_model_protocol_overrides(
        &scope,
        &[
            (
                "grok-4.5".into(),
                UpstreamProtocolKind::Responses,
                ProtocolOverrideState::ForceOn,
            ),
            (
                "future-go-model".into(),
                UpstreamProtocolKind::Messages,
                ProtocolOverrideState::ForceOn,
            ),
        ],
        now,
    )
    .unwrap();
    let revision_before = db.load_persisted_scope(&scope).unwrap().unwrap().revision;

    let refreshed = db
        .refresh_contract_catalog_with_default_off(
            &scope,
            &["grok-4.5".into()],
            &[
                "grok-4.5".into(),
                "glm-5.2".into(),
                "future-go-model".into(),
            ],
            now,
            crate::provider_contracts::CATALOG_SOURCE_OPENCODE_MODELS,
            "https://opencode.ai/zen/go/v1/models",
        )
        .unwrap();

    assert_eq!(refreshed.revision, revision_before + 1);
    assert_eq!(
        refreshed.catalog_models,
        vec!["grok-4.5", "glm-5.2", "future-go-model"]
    );
    let persisted = db.load_persisted_contracts().unwrap();
    let overrides = persisted.overrides.get(&scope).unwrap();
    assert!(overrides.iter().any(|row| row.model_id == "grok-4.5"
        && row.protocol == UpstreamProtocolKind::Responses
        && row.state == ProtocolOverrideState::ForceOn));
    assert_eq!(
        overrides
            .iter()
            .filter(|row| row.model_id == "grok-4.5")
            .count(),
        1,
        "retained models keep their existing protocol choices"
    );
    for protocol in [
        UpstreamProtocolKind::ChatCompletions,
        UpstreamProtocolKind::Responses,
        UpstreamProtocolKind::Messages,
    ] {
        assert!(overrides.iter().any(|row| row.model_id == "glm-5.2"
            && row.protocol == protocol
            && row.state == ProtocolOverrideState::ForceOff));
    }
    for protocol in [
        UpstreamProtocolKind::ChatCompletions,
        UpstreamProtocolKind::Responses,
    ] {
        assert!(overrides.iter().any(|row| row.model_id == "future-go-model"
            && row.protocol == protocol
            && row.state == ProtocolOverrideState::ForceOff));
    }
    assert!(overrides.iter().any(|row| row.model_id == "future-go-model"
        && row.protocol == UpstreamProtocolKind::Messages
        && row.state == ProtocolOverrideState::ForceOn));

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn command_catalog_reappearing_preset_returns_to_auto_enabled() {
    let dir = temp_data_dir("command-catalog-reappearing-preset");
    let db = Database::open(dir.clone()).unwrap();
    let now = Utc::now();
    let scope = ContractScope::provider(COMMAND_CODE_PROVIDER_ID);
    let preset = COMMAND_CODE_GOAT_INCLUDED_MODEL_IDS[0].to_string();
    let extra = "vendor/future-command-model".to_string();

    db.set_contract_catalog(
        &scope,
        std::slice::from_ref(&preset),
        Some(now),
        CATALOG_SOURCE_COMMAND_CODE_MODELS,
        COMMAND_CODE_GOAT_BASE_URL,
        now,
    )
    .unwrap();
    db.refresh_contract_catalog_with_default_off(
        &scope,
        std::slice::from_ref(&preset),
        std::slice::from_ref(&extra),
        now,
        CATALOG_SOURCE_COMMAND_CODE_MODELS,
        COMMAND_CODE_GOAT_BASE_URL,
    )
    .unwrap();
    db.refresh_contract_catalog_with_default_off(
        &scope,
        std::slice::from_ref(&extra),
        &[extra.clone(), preset.clone()],
        now,
        CATALOG_SOURCE_COMMAND_CODE_MODELS,
        COMMAND_CODE_GOAT_BASE_URL,
    )
    .unwrap();

    let persisted = db.load_persisted_contracts().unwrap();
    let overrides = persisted.overrides.get(&scope).unwrap();
    assert!(
        overrides.iter().all(|row| row.model_id != preset),
        "a GOAT preset must remain Auto when it reappears: {overrides:?}"
    );
    assert!(
        overrides
            .iter()
            .any(|row| { row.model_id == extra && row.state == ProtocolOverrideState::ForceOff })
    );

    drop(db);
    fs::remove_dir_all(dir).unwrap();
}
