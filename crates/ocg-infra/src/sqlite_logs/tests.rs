use super::*;
use rusqlite::types::Value;

const FORWARD_INSERT_COLUMNS: [&str; 39] = [
    "timestamp",
    "model",
    "account_id",
    "account_name",
    "client_key_id",
    "client_key_name",
    "route_account_id",
    "provider_id",
    "offering_id",
    "credential_account_id",
    "status",
    "http_status",
    "route",
    "prompt_tokens",
    "completion_tokens",
    "cached_tokens",
    "cache_creation_tokens",
    "cost",
    "raw_cost_usd",
    "quota_debit",
    "effective_paid_cost_usd",
    "pricing_revision_id",
    "quota_multiplier",
    "local_adjustment_multiplier",
    "service_tier",
    "cost_state",
    "error_message",
    "request_id",
    "attempt",
    "error_source",
    "error_stage",
    "duration_ms",
    "diagnostic_json",
    "requested_model",
    "resolved_alias",
    "upstream_model",
    "native_cost_value",
    "native_cost_unit",
    "native_cost_currency",
];

const V26_LOG_DDL: &str = "
        CREATE TABLE forward_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            model TEXT NOT NULL,
            account_id TEXT NOT NULL,
            account_name TEXT NOT NULL,
            client_key_id TEXT,
            client_key_name TEXT,
            route_account_id TEXT,
            provider_id TEXT,
            offering_id TEXT,
            credential_account_id TEXT,
            status TEXT NOT NULL,
            http_status INTEGER,
            route TEXT NOT NULL DEFAULT '',
            prompt_tokens INTEGER NOT NULL DEFAULT 0,
            completion_tokens INTEGER NOT NULL DEFAULT 0,
            cached_tokens INTEGER NOT NULL DEFAULT 0,
            cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
            cost REAL NOT NULL DEFAULT 0,
            raw_cost_usd REAL,
            quota_debit REAL,
            effective_paid_cost_usd REAL,
            pricing_revision_id TEXT,
            quota_multiplier REAL,
            local_adjustment_multiplier REAL,
            service_tier TEXT,
            cost_state TEXT NOT NULL DEFAULT 'not_applicable',
            error_message TEXT,
            request_id TEXT,
            attempt INTEGER,
            error_source TEXT,
            error_stage TEXT,
            duration_ms INTEGER,
            diagnostic_json TEXT,
            requested_model TEXT,
            resolved_alias TEXT,
            upstream_model TEXT,
            native_cost_value REAL,
            native_cost_unit TEXT,
            native_cost_currency TEXT
        );
        CREATE TABLE gateway_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            level TEXT NOT NULL,
            category TEXT NOT NULL,
            message TEXT NOT NULL,
            created_at TEXT NOT NULL,
            request_id TEXT,
            attempt INTEGER,
            error_source TEXT,
            error_stage TEXT,
            duration_ms INTEGER,
            diagnostic_json TEXT
        );
    ";

fn v26_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory sqlite");
    conn.execute_batch(V26_LOG_DDL)
        .expect("v26 log tables should create");
    conn
}

fn sentinel_insert_row() -> ForwardLogInsertRow<'static> {
    ForwardLogInsertRow {
        timestamp: "ts-1",
        model: "model-2",
        account_id: "acct-3",
        account_name: "name-4",
        client_key_id: Some("key-5"),
        client_key_name: Some("keyname-6"),
        route_account_id: Some("route-acct-7"),
        provider_id: Some("prov-8"),
        offering_id: Some("off-9"),
        credential_account_id: Some("cred-10"),
        status: "status-11",
        http_status: Some(12),
        route: "proxy",
        prompt_tokens: 14,
        completion_tokens: 15,
        cached_tokens: 16,
        cache_creation_tokens: 17,
        cost: 18.5,
        raw_cost_usd: Some(19.5),
        quota_debit: Some(20.5),
        effective_paid_cost_usd: Some(21.5),
        pricing_revision_id: Some("rev-22"),
        quota_multiplier: Some(23.5),
        local_adjustment_multiplier: Some(24.5),
        service_tier: Some("tier-25"),
        cost_state: "state-26",
        error_message: Some("err-27"),
        request_id: Some("req-28"),
        attempt: Some(29),
        error_source: Some("src-30"),
        error_stage: Some("stage-31"),
        duration_ms: Some(32),
        diagnostic_json: Some("{\"k\":33}"),
        requested_model: Some("req-model-34"),
        resolved_alias: Some("alias-35"),
        upstream_model: Some("up-36"),
        native_cost_value: Some(37.5),
        native_cost_unit: Some("unit-38"),
        native_cost_currency: Some("CUR-39"),
    }
}

fn expected_insert_values() -> [Value; 39] {
    [
        text("ts-1"),
        text("model-2"),
        text("acct-3"),
        text("name-4"),
        text("key-5"),
        text("keyname-6"),
        text("route-acct-7"),
        text("prov-8"),
        text("off-9"),
        text("cred-10"),
        text("status-11"),
        Value::Integer(12),
        text("proxy"),
        Value::Integer(14),
        Value::Integer(15),
        Value::Integer(16),
        Value::Integer(17),
        Value::Real(18.5),
        Value::Real(19.5),
        Value::Real(20.5),
        Value::Real(21.5),
        text("rev-22"),
        Value::Real(23.5),
        Value::Real(24.5),
        text("tier-25"),
        text("state-26"),
        text("err-27"),
        text("req-28"),
        Value::Integer(29),
        text("src-30"),
        text("stage-31"),
        Value::Integer(32),
        text("{\"k\":33}"),
        text("req-model-34"),
        text("alias-35"),
        text("up-36"),
        Value::Real(37.5),
        text("unit-38"),
        text("CUR-39"),
    ]
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

fn select_forward_columns(conn: &Connection, id: i64) -> Vec<Value> {
    let sql = format!(
        "SELECT {} FROM forward_logs WHERE id = ?1",
        FORWARD_INSERT_COLUMNS.join(", ")
    );
    let mut stmt = conn.prepare(&sql).expect("select forward columns");
    stmt.query_row([id], |row| {
        let mut values = Vec::with_capacity(FORWARD_INSERT_COLUMNS.len());
        for index in 0..FORWARD_INSERT_COLUMNS.len() {
            values.push(row.get(index)?);
        }
        Ok(values)
    })
    .expect("forward row should exist")
}

fn parenthesized_lists(sql: &str) -> Vec<Vec<String>> {
    let mut lists = Vec::new();
    let mut rest = sql;
    while let Some(start) = rest.find('(') {
        let after = &rest[start + 1..];
        let end = after.find(')').expect("closing paren");
        lists.push(
            after[..end]
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect(),
        );
        rest = &after[end + 1..];
    }
    lists
}

fn filled_gateway_row() -> GatewayLogInsertRow<'static> {
    GatewayLogInsertRow {
        level: "error",
        category: "gateway",
        message: "upstream failed",
        created_at: "created-at",
        request_id: Some("req-g"),
        attempt: Some(7),
        error_source: Some("upstream"),
        error_stage: Some("read"),
        duration_ms: Some(42),
        diagnostic_json: Some("{\"code\":500}"),
    }
}

fn empty_gateway_row() -> GatewayLogInsertRow<'static> {
    GatewayLogInsertRow {
        level: "info",
        category: "account",
        message: "verified managed account demo",
        created_at: "created-empty",
        request_id: None,
        attempt: None,
        error_source: None,
        error_stage: None,
        duration_ms: None,
        diagnostic_json: None,
    }
}

#[test]
fn insert_forward_log_round_trips_every_column_in_binding_order() {
    let lists = parenthesized_lists(INSERT_FORWARD_LOG_SQL);
    assert_eq!(lists.len(), 2, "{lists:?}");
    assert_eq!(lists[0], FORWARD_INSERT_COLUMNS);
    let expected_placeholders: Vec<String> = (1..=39).map(|index| format!("?{index}")).collect();
    assert_eq!(lists[1], expected_placeholders);

    let conn = v26_conn();
    let row = sentinel_insert_row();
    let id = insert_forward_log(&conn, &row).expect("insert");
    assert_eq!(id, 1);
    assert_eq!(select_forward_columns(&conn, id), expected_insert_values());
}

#[test]
fn insert_gateway_log_round_trips_all_optional_diagnostic_fields() {
    let lists = parenthesized_lists(INSERT_GATEWAY_LOG_SQL);
    assert_eq!(
        lists[0],
        [
            "level",
            "category",
            "message",
            "created_at",
            "request_id",
            "attempt",
            "error_source",
            "error_stage",
            "duration_ms",
            "diagnostic_json",
        ]
    );

    let conn = v26_conn();
    assert_eq!(insert_gateway_log(&conn, &filled_gateway_row()).unwrap(), 1);
    assert_eq!(insert_gateway_log(&conn, &empty_gateway_row()).unwrap(), 1);

    let mut stmt = conn
        .prepare(
            "SELECT level, category, message, created_at, request_id, attempt,
                        error_source, error_stage, duration_ms, diagnostic_json
                 FROM gateway_logs ORDER BY id ASC",
        )
        .unwrap();
    let rows: Vec<[Value; 10]> = stmt
        .query_map([], |row| {
            Ok([
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
            ])
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows[0],
        [
            text("error"),
            text("gateway"),
            text("upstream failed"),
            text("created-at"),
            text("req-g"),
            Value::Integer(7),
            text("upstream"),
            text("read"),
            Value::Integer(42),
            text("{\"code\":500}"),
        ]
    );
    assert_eq!(
        rows[1],
        [
            text("info"),
            text("account"),
            text("verified managed account demo"),
            text("created-empty"),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ]
    );
}

#[test]
fn update_and_patch_missing_rows_return_zero() {
    let conn = v26_conn();
    let id = insert_forward_log(&conn, &sentinel_insert_row()).unwrap();
    let missing_id = id + 99;
    let update = ForwardLogUpdateRow {
        id: missing_id,
        status: "success",
        http_status: Some(200),
        prompt_tokens: 1,
        completion_tokens: 2,
        cached_tokens: 3,
        cache_creation_tokens: 4,
        cost: 5.5,
        raw_cost_usd: Some(5.5),
        quota_debit: Some(5.5),
        effective_paid_cost_usd: Some(5.5),
        pricing_revision_id: Some("rev"),
        quota_multiplier: Some(1.0),
        local_adjustment_multiplier: Some(1.0),
        service_tier: Some("default"),
        cost_state: "priced",
        error_message: Some("nope"),
        error_source: Some("src"),
        error_stage: Some("stage"),
        duration_ms: Some(9),
        diagnostic_json: Some("{}"),
        native_cost_value: Some(5.5),
        native_cost_unit: Some("usd"),
        native_cost_currency: Some("USD"),
    };
    assert_eq!(update_forward_log(&conn, &update).unwrap(), 0);
    assert_eq!(
        patch_forward_log_identity(
            &conn,
            &ForwardLogIdentityPatch {
                id: missing_id,
                requested_model: Some("other"),
                resolved_alias: Some("alias"),
                upstream_model: Some("up"),
                native_cost_value: Some(1.0),
                native_cost_unit: Some("credits"),
                native_cost_currency: Some("CR"),
            },
        )
        .unwrap(),
        0
    );
    assert_eq!(
        select_forward_columns(&conn, id),
        expected_insert_values(),
        "missing-row update/patch must leave the existing row untouched"
    );

    let coalesced = ForwardLogUpdateRow {
        id,
        status: "streaming",
        http_status: None,
        prompt_tokens: 100,
        completion_tokens: 101,
        cached_tokens: 102,
        cache_creation_tokens: 103,
        cost: 0.0,
        raw_cost_usd: None,
        quota_debit: None,
        effective_paid_cost_usd: None,
        pricing_revision_id: None,
        quota_multiplier: None,
        local_adjustment_multiplier: None,
        service_tier: None,
        cost_state: "not_applicable",
        error_message: None,
        error_source: None,
        error_stage: None,
        duration_ms: None,
        diagnostic_json: None,
        native_cost_value: None,
        native_cost_unit: None,
        native_cost_currency: None,
    };
    assert_eq!(update_forward_log(&conn, &coalesced).unwrap(), 1);
    let after = select_forward_columns(&conn, id);
    assert_eq!(after[11], Value::Integer(12), "http_status COALESCE");
    assert_eq!(after[26], text("err-27"), "error_message COALESCE");
    assert_eq!(after[29], text("src-30"), "error_source COALESCE");
    assert_eq!(after[10], text("streaming"));
    assert_eq!(
        patch_forward_log_identity(
            &conn,
            &ForwardLogIdentityPatch {
                id,
                requested_model: Some("patched-model"),
                resolved_alias: Some("patched-alias"),
                upstream_model: Some("patched-up"),
                native_cost_value: Some(9.25),
                native_cost_unit: Some("usd"),
                native_cost_currency: Some("USD"),
            },
        )
        .unwrap(),
        1
    );
    let patched = select_forward_columns(&conn, id);
    assert_eq!(patched[33], text("patched-model"));
    assert_eq!(patched[34], text("patched-alias"));
    assert_eq!(patched[35], text("patched-up"));
    assert_eq!(patched[36], Value::Real(9.25));
    assert_eq!(patched[37], text("usd"));
    assert_eq!(patched[38], text("USD"));
}

#[test]
fn helpers_use_the_caller_connection_and_do_not_commit_a_private_transaction() {
    let conn = v26_conn();
    let tx = conn.unchecked_transaction().unwrap();
    let id = insert_forward_log(&tx, &sentinel_insert_row()).unwrap();
    assert_eq!(insert_gateway_log(&tx, &empty_gateway_row()).unwrap(), 1);
    tx.rollback().unwrap();
    let forward_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM forward_logs WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    let gateway_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM gateway_logs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(forward_count, 0);
    assert_eq!(gateway_count, 0);
}
