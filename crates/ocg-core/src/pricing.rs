use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Datelike, Timelike, Utc};
use futures_util::StreamExt;
use reqwest::redirect::{Attempt, Policy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub const SOURCE_URL: &str = "https://opencode.ai/docs/go/";
const SOURCE_HOST: &str = "opencode.ai";
const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const MONTHLY_LIMIT: f64 = 60.0;
const ADJUSTMENT_POLICY_VERSION: &str = "local-v4";

// Audit reference only; the runtime never fetches supplier pricing pages:
// https://platform.minimaxi.com/docs/guides/pricing-paygo

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PricingLimits {
    pub window_5h: f64,
    pub window_week: f64,
    pub window_month: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PricingAdjustment {
    pub label: String,
    pub multiplier: f64,
    pub applies_to: String,
}

#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum PricingTimeWindow {
    #[default]
    Always,
    OffPeak,
    Peak,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PricingModel {
    pub model_id: String,
    pub display_name: String,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: Option<f64>,
    pub usage: f64,
    /// Editable OpenCode Go multiplier applied after the official token rates.
    /// Fresh official snapshots derive it as monthly limit / model Usage.
    pub quota_multiplier: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<i64>,
    /// Official Peak / Off-Peak row. Missing in older snapshots means `always`.
    #[serde(default)]
    pub time_window: PricingTimeWindow,
    pub adjustments: Vec<PricingAdjustment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PricingSnapshot {
    pub revision: String,
    pub activated_at: String,
    pub document_updated_at: String,
    pub source_url: String,
    pub content_hash: String,
    pub limits: PricingLimits,
    pub models: Vec<PricingModel>,
    pub adjustment_policy_version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PricingEstimate {
    pub cost: Option<f64>,
    pub pricing_revision_id: Option<String>,
    pub quota_multiplier: Option<f64>,
    pub local_adjustment_multiplier: Option<f64>,
    pub cost_state: &'static str,
}

impl PricingEstimate {
    fn unpriced(revision: &str) -> Self {
        Self {
            cost: None,
            pricing_revision_id: Some(revision.to_string()),
            quota_multiplier: None,
            local_adjustment_multiplier: None,
            cost_state: "unpriced",
        }
    }

    fn free(revision: &str) -> Self {
        Self {
            cost: None,
            pricing_revision_id: Some(revision.to_string()),
            quota_multiplier: None,
            local_adjustment_multiplier: None,
            cost_state: "free",
        }
    }
}

impl PricingSnapshot {
    pub fn estimate(
        &self,
        model: &str,
        prompt: i64,
        completion: i64,
        cached: i64,
        cache_creation: i64,
        service_tier: Option<&str>,
    ) -> PricingEstimate {
        self.estimate_at(
            model,
            prompt,
            completion,
            cached,
            cache_creation,
            service_tier,
            Utc::now(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn estimate_at(
        &self,
        model: &str,
        prompt: i64,
        completion: i64,
        cached: i64,
        cache_creation: i64,
        service_tier: Option<&str>,
        at: DateTime<Utc>,
    ) -> PricingEstimate {
        let prompt = prompt.max(0) as f64;
        let completion = completion.max(0) as f64;
        let cached = (cached.max(0) as f64).min(prompt);
        let cache_creation = (cache_creation.max(0) as f64).min(prompt - cached);
        let uncached = prompt - cached - cache_creation;
        let normalized = normalize_model_name(model);
        if crate::gateway::free_models::is_free_model(&normalized) {
            return PricingEstimate::free(&self.revision);
        }
        let highspeed = normalized.contains("minimax-m2.7-highspeed")
            || normalized.contains("minimax-m2.5-highspeed");
        let lookup_name = normalized.replace("-highspeed", "");

        let candidates = self
            .models
            .iter()
            .filter(|entry| lookup_name == entry.model_id)
            .filter(|entry| {
                entry
                    .min_input_tokens
                    .is_none_or(|minimum| prompt as i64 >= minimum)
                    && entry
                        .max_input_tokens
                        .is_none_or(|maximum| prompt as i64 <= maximum)
            })
            .collect::<Vec<_>>();
        let selected = select_priced_model(&candidates, at);
        let Some(price) = selected else {
            return PricingEstimate::unpriced(&self.revision);
        };

        // A '-' in the official Cached Write column means there is no separate
        // cache-write price. Cache creation is still new input, so it uses input.
        let cache_write = price.cache_write.unwrap_or(price.input);
        let base = (uncached * price.input
            + completion * price.output
            + cached * price.cache_read
            + cache_creation * cache_write)
            / 1_000_000.0;

        let mut adjusted_input = price.input;
        let mut adjusted_output = price.output;
        let mut adjusted_cache_read = price.cache_read;
        let mut adjusted_cache_write = cache_write;
        if highspeed {
            adjusted_input *= 2.0;
            adjusted_output *= 2.0;
        }
        if price.model_id == "minimax-m3" {
            let mut multiplier = 1.0;
            if prompt > 512_000.0 {
                multiplier *= 2.0;
            }
            if service_tier.is_some_and(|tier| tier.eq_ignore_ascii_case("priority")) {
                multiplier *= 1.5;
            }
            adjusted_input *= multiplier;
            adjusted_output *= multiplier;
            adjusted_cache_read *= multiplier;
            adjusted_cache_write *= multiplier;
        }
        let adjusted = (uncached * adjusted_input
            + completion * adjusted_output
            + cached * adjusted_cache_read
            + cache_creation * adjusted_cache_write)
            / 1_000_000.0;
        let local_adjustment_multiplier = if base > 0.0 { adjusted / base } else { 1.0 };

        PricingEstimate {
            cost: Some(adjusted * price.quota_multiplier),
            pricing_revision_id: Some(self.revision.clone()),
            quota_multiplier: Some(price.quota_multiplier),
            local_adjustment_multiplier: Some(local_adjustment_multiplier),
            cost_state: "priced",
        }
    }
}

pub fn embedded_seed() -> PricingSnapshot {
    let mut models = vec![
        seed_model("grok-4.5", "Grok 4.5", 2.0, 6.0, 0.3, None, 15.0),
        seed_model("glm-5.3", "GLM-5.3", 1.4, 4.4, 0.26, None, 15.0),
        seed_model("glm-5.2", "GLM-5.2", 1.4, 4.4, 0.26, None, 60.0),
        seed_model("glm-5.1", "GLM-5.1", 1.4, 4.4, 0.26, None, 60.0),
        seed_tier_with_usage(
            "gpt-5.6-luna",
            "GPT 5.6 Luna (≤ 272K tokens)",
            0.2,
            1.2,
            0.02,
            Some(0.25),
            15.0,
            None,
            Some(272_000),
        ),
        seed_tier_with_usage(
            "gpt-5.6-luna",
            "GPT 5.6 Luna (> 272K tokens)",
            0.4,
            1.8,
            0.04,
            Some(0.5),
            15.0,
            Some(272_001),
            None,
        ),
        seed_model("kimi-k3", "Kimi K3", 3.0, 15.0, 0.3, None, 15.0),
        seed_model(
            "kimi-k2.7-code",
            "Kimi K2.7 Code",
            0.95,
            4.0,
            0.19,
            None,
            60.0,
        ),
        seed_model("kimi-k2.6", "Kimi K2.6", 0.95, 4.0, 0.16, None, 60.0),
        seed_model("mimo-v2.5", "MiMo V2.5", 0.14, 0.28, 0.0028, None, 60.0),
        seed_model(
            "mimo-v2.5-pro",
            "MiMo V2.5 Pro",
            0.435,
            0.87,
            0.003625,
            None,
            15.0,
        ),
        seed_model("minimax-m3", "MiniMax M3", 0.3, 1.2, 0.06, None, 60.0),
        seed_model(
            "minimax-m2.7",
            "MiniMax M2.7",
            0.3,
            1.2,
            0.06,
            Some(0.375),
            60.0,
        ),
        seed_model(
            "minimax-m2.5",
            "MiniMax M2.5",
            0.3,
            1.2,
            0.06,
            Some(0.375),
            60.0,
        ),
        seed_model(
            "qwen3.8-max",
            "Qwen3.8 Max",
            2.0,
            6.0,
            0.25,
            Some(2.5),
            15.0,
        ),
        seed_model(
            "qwen3.7-max",
            "Qwen3.7 Max",
            2.5,
            7.5,
            0.5,
            Some(3.125),
            60.0,
        ),
        seed_tier(
            "qwen3.7-plus",
            "Qwen3.7 Plus (≤ 256K tokens)",
            0.4,
            1.6,
            0.04,
            0.5,
            None,
            Some(256_000),
        ),
        seed_tier(
            "qwen3.7-plus",
            "Qwen3.7 Plus (> 256K tokens)",
            1.2,
            4.8,
            0.12,
            1.5,
            Some(256_001),
            None,
        ),
        seed_tier(
            "qwen3.6-plus",
            "Qwen3.6 Plus (≤ 256K tokens)",
            0.5,
            3.0,
            0.05,
            0.625,
            None,
            Some(256_000),
        ),
        seed_tier(
            "qwen3.6-plus",
            "Qwen3.6 Plus (> 256K tokens)",
            2.0,
            6.0,
            0.2,
            2.5,
            Some(256_001),
            None,
        ),
        seed_scheduled(
            "deepseek-v4-pro",
            "DeepSeek V4 Pro (Off-Peak)",
            0.66,
            1.98,
            0.022,
            None,
            15.0,
            PricingTimeWindow::OffPeak,
        ),
        seed_scheduled(
            "deepseek-v4-pro",
            "DeepSeek V4 Pro (Peak)",
            1.32,
            3.96,
            0.044,
            None,
            15.0,
            PricingTimeWindow::Peak,
        ),
        seed_scheduled(
            "deepseek-v4-flash",
            "DeepSeek V4 Flash (Off-Peak)",
            0.22,
            0.66,
            0.007,
            None,
            15.0,
            PricingTimeWindow::OffPeak,
        ),
        seed_scheduled(
            "deepseek-v4-flash",
            "DeepSeek V4 Flash (Peak)",
            0.44,
            1.32,
            0.014,
            None,
            15.0,
            PricingTimeWindow::Peak,
        ),
        seed_model(
            "muse-spark-1.2",
            "Muse Spark 1.2",
            0.10,
            0.20,
            0.002,
            None,
            60.0,
        ),
        seed_model(
            "muse-spark-1.2-contributor",
            "Muse Spark 1.2 Contributor",
            0.10,
            0.20,
            0.002,
            None,
            60.0,
        ),
        seed_model("hy3", "Hy3", 0.14, 0.58, 0.035, None, 60.0),
    ];
    apply_official_pricing_policy(&mut models, MONTHLY_LIMIT);
    sort_models(&mut models);
    PricingSnapshot {
        revision: format!("seed-2026-08-16-{ADJUSTMENT_POLICY_VERSION}"),
        activated_at: Utc::now().to_rfc3339(),
        document_updated_at: "2026-08-16T00:00:00.000Z".to_string(),
        source_url: SOURCE_URL.to_string(),
        content_hash: "embedded-opencode-go-2026-08-16".to_string(),
        limits: PricingLimits {
            window_5h: 12.0,
            window_week: 30.0,
            window_month: MONTHLY_LIMIT,
        },
        models,
        adjustment_policy_version: ADJUSTMENT_POLICY_VERSION.to_string(),
    }
}

pub(crate) fn ensure_current_adjustment_policy(mut snapshot: PricingSnapshot) -> PricingSnapshot {
    if snapshot.adjustment_policy_version == ADJUSTMENT_POLICY_VERSION {
        return snapshot;
    }

    // local-v2 and older divided the Go multiplier by a separate supplier-price
    // multiplier for two Pro models. Manual multiplier editing did not exist in
    // those revisions, so repairing them from the official Usage column is safe.
    // local-v3 already stores the correct applied multiplier, while local-v4+
    // may contain user edits and must never be silently rebased by a policy bump.
    if legacy_policy_needs_multiplier_repair(&snapshot.adjustment_policy_version) {
        apply_official_multipliers(&mut snapshot.models, snapshot.limits.window_month);
    }
    add_adjustments(&mut snapshot.models);
    snapshot.adjustment_policy_version = ADJUSTMENT_POLICY_VERSION.to_string();
    snapshot.revision = unique_revision_for_content_hash(&snapshot.content_hash);
    snapshot.activated_at = Utc::now().to_rfc3339();
    snapshot
}

// These are deliberately the only seed rows that can be backfilled into an
// official snapshot. The public Go pricing table lists Contributor but omits
// standard Muse; standard rates come from live Go measurements, not that table.
// Do not turn this into "every embedded seed row": an official removal must
// not silently revive unrelated models forever.
const SEED_COVERAGE_MODEL_IDS: &[&str] = &["muse-spark-1.2", "muse-spark-1.2-contributor"];

/// Append the explicitly allowlisted Muse rows that an existing snapshot does
/// not know about yet. Entries already present — official rows or user-edited
/// multipliers — are never overwritten.
pub(crate) fn ensure_seed_model_coverage(mut snapshot: PricingSnapshot) -> PricingSnapshot {
    let known = snapshot
        .models
        .iter()
        .map(|model| model.model_id.as_str())
        .collect::<HashSet<_>>();
    let mut missing: Vec<PricingModel> = embedded_seed()
        .models
        .into_iter()
        .filter(|model| {
            SEED_COVERAGE_MODEL_IDS.contains(&model.model_id.as_str())
                && !known.contains(model.model_id.as_str())
        })
        .collect();
    if missing.is_empty() {
        return snapshot;
    }
    // Recompute with the snapshot's own monthly limit so the appended rows
    // agree with the rest of the snapshot even if the official limit moved.
    apply_official_multipliers(&mut missing, snapshot.limits.window_month);
    snapshot.models.extend(missing);
    sort_models(&mut snapshot.models);
    snapshot.revision = unique_revision_for_content_hash(&snapshot.content_hash);
    snapshot.activated_at = Utc::now().to_rfc3339();
    snapshot
}

pub(crate) fn stamp_pricing_activation(mut snapshot: PricingSnapshot) -> PricingSnapshot {
    snapshot.revision = unique_revision_for_content_hash(&snapshot.content_hash);
    snapshot.activated_at = Utc::now().to_rfc3339();
    snapshot
}

fn legacy_policy_needs_multiplier_repair(version: &str) -> bool {
    version
        .strip_prefix("local-v")
        .and_then(|value| value.parse::<u32>().ok())
        .is_none_or(|version| version < 3)
}

fn seed_model(
    id: &str,
    name: &str,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: Option<f64>,
    usage: f64,
) -> PricingModel {
    PricingModel {
        model_id: id.to_string(),
        display_name: name.to_string(),
        input,
        output,
        cache_read,
        cache_write,
        usage,
        quota_multiplier: MONTHLY_LIMIT / usage,
        min_input_tokens: None,
        max_input_tokens: None,
        time_window: PricingTimeWindow::Always,
        adjustments: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn seed_scheduled(
    id: &str,
    name: &str,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: Option<f64>,
    usage: f64,
    time_window: PricingTimeWindow,
) -> PricingModel {
    let mut model = seed_model(id, name, input, output, cache_read, cache_write, usage);
    model.time_window = time_window;
    model
}

#[allow(clippy::too_many_arguments)]
fn seed_tier(
    id: &str,
    name: &str,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
    min_input_tokens: Option<i64>,
    max_input_tokens: Option<i64>,
) -> PricingModel {
    seed_tier_with_usage(
        id,
        name,
        input,
        output,
        cache_read,
        Some(cache_write),
        60.0,
        min_input_tokens,
        max_input_tokens,
    )
}

#[allow(clippy::too_many_arguments)]
fn seed_tier_with_usage(
    id: &str,
    name: &str,
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: Option<f64>,
    usage: f64,
    min_input_tokens: Option<i64>,
    max_input_tokens: Option<i64>,
) -> PricingModel {
    let mut model = seed_model(id, name, input, output, cache_read, cache_write, usage);
    model.min_input_tokens = min_input_tokens;
    model.max_input_tokens = max_input_tokens;
    model
}

pub async fn fetch_official_snapshot(config: &crate::models::AppConfig) -> Result<PricingSnapshot> {
    let client = crate::http_client::configured_builder(config)?
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .redirect(Policy::custom(same_source_redirect))
        .build()
        .context("build OpenCode Go pricing client")?;
    let response = client
        .get(SOURCE_URL)
        .send()
        .await
        .context("fetch OpenCode Go pricing page")?
        .error_for_status()
        .context("OpenCode Go pricing page returned an error")?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DOCUMENT_BYTES as u64)
    {
        bail!("OpenCode Go pricing page exceeds 2 MiB");
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read OpenCode Go pricing page")?;
        if bytes.len() + chunk.len() > MAX_DOCUMENT_BYTES {
            bail!("OpenCode Go pricing page exceeds 2 MiB");
        }
        bytes.extend_from_slice(&chunk);
    }
    let html = String::from_utf8(bytes).context("OpenCode Go pricing page is not UTF-8")?;
    parse_official_html(&html)
}

fn same_source_redirect(attempt: Attempt<'_>) -> reqwest::redirect::Action {
    if attempt.previous().len() >= 5 {
        return attempt.error("too many OpenCode Go pricing redirects");
    }
    let url = attempt.url();
    if url.scheme() == "https"
        && url.host_str() == Some(SOURCE_HOST)
        && url.port_or_known_default() == Some(443)
    {
        attempt.follow()
    } else {
        attempt.error("OpenCode Go pricing redirect left the approved HTTPS host")
    }
}

pub fn parse_official_html(html: &str) -> Result<PricingSnapshot> {
    let plain = collapse_whitespace(&strip_tags(html));
    let limits = PricingLimits {
        window_5h: parse_limit(&plain, "5 hour limit")?,
        window_week: parse_limit(&plain, "Weekly limit")?,
        window_month: parse_limit(&plain, "Monthly limit")?,
    };
    if limits.window_5h <= 0.0 || limits.window_week <= 0.0 || limits.window_month <= 0.0 {
        bail!("OpenCode Go usage limits must be positive");
    }
    let tables = extract_tables(html)?;
    let pricing_table = tables
        .iter()
        .find(|table| {
            has_headers(
                table,
                &[
                    "model",
                    "input",
                    "output",
                    "cached read",
                    "cached write",
                    "usage",
                ],
            )
        })
        .ok_or_else(|| anyhow!("OpenCode Go pricing table was not found"))?;
    let endpoint_table = tables
        .iter()
        .find(|table| has_headers(table, &["model", "model id", "endpoint", "ai sdk package"]))
        .ok_or_else(|| anyhow!("OpenCode Go model ID table was not found"))?;

    let mut ids_by_name = HashMap::new();
    let mut seen_model_ids = HashSet::new();
    for row in endpoint_table.iter().skip(1) {
        if row.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }
        if row.len() != 4 {
            bail!("OpenCode Go model ID table contains an incomplete row");
        }
        let key = canonical_display_name(&row[0]);
        let raw_id = row[1].trim();
        let id = normalize_model_name(raw_id);
        if key.is_empty() || id.is_empty() {
            bail!("OpenCode Go model ID table contains an empty model");
        }
        if raw_id != id {
            bail!("OpenCode Go model ID `{raw_id}` is not canonical");
        }
        if !seen_model_ids.insert(id.clone()) {
            bail!("OpenCode Go model ID table contains duplicate model ID {id}");
        }
        if ids_by_name.insert(key, id).is_some() {
            bail!("OpenCode Go model ID table contains duplicate model names");
        }
    }

    let mut models = Vec::new();
    let mut seen_tiers = HashSet::new();
    let mut unpriced_ids = HashSet::new();
    for row in pricing_table.iter().skip(1) {
        if row.len() != 6 {
            bail!("OpenCode Go pricing table contains an incomplete row");
        }
        let display_name = row[0].trim().to_string();
        let id = ids_by_name
            .get(&canonical_display_name(&display_name))
            .cloned()
            .ok_or_else(|| anyhow!("no official model ID found for {display_name}"))?;
        // Official Go docs list limited-time promos such as Ox Alpha Free with
        // dash prices. They stay on `/zen/go` but have no USD rates to ingest.
        if is_unpriced_promo_row(row) {
            unpriced_ids.insert(id);
            continue;
        }
        let (minimum, maximum) = parse_token_tier(&display_name)?;
        let time_window = parse_time_window(&display_name);
        if !seen_tiers.insert((id.clone(), minimum, maximum, time_window)) {
            bail!("OpenCode Go pricing table contains duplicate row for {display_name}");
        }
        let input = parse_dollar(&row[1], false)?
            .ok_or_else(|| anyhow!("{display_name} is missing input price"))?;
        let output = parse_dollar(&row[2], false)?
            .ok_or_else(|| anyhow!("{display_name} is missing output price"))?;
        let cache_read = parse_dollar(&row[3], false)?
            .ok_or_else(|| anyhow!("{display_name} is missing cache-read price"))?;
        let cache_write = parse_dollar(&row[4], true)?;
        let usage = parse_dollar(&row[5], false)?
            .ok_or_else(|| anyhow!("{display_name} is missing Usage"))?;
        if usage <= 0.0 {
            bail!("{display_name} Usage must be positive");
        }
        models.push(PricingModel {
            model_id: id,
            display_name,
            input,
            output,
            cache_read,
            cache_write,
            usage,
            quota_multiplier: limits.window_month / usage,
            min_input_tokens: minimum,
            max_input_tokens: maximum,
            time_window,
            adjustments: Vec::new(),
        });
    }

    if models.is_empty() {
        bail!("OpenCode Go pricing and model ID tables must not be empty");
    }

    let covered = models
        .iter()
        .map(|model| model.model_id.as_str())
        .collect::<HashSet<_>>();
    let missing_prices = seen_model_ids
        .iter()
        .filter(|id| !covered.contains(id.as_str()) && !unpriced_ids.contains(id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_prices.is_empty() {
        bail!(
            "OpenCode Go model ID table contains models without pricing rows: {}",
            missing_prices.join(", ")
        );
    }
    for id in ["qwen3.7-plus", "qwen3.6-plus"] {
        if covered.contains(id) {
            validate_token_tiers(&models, id, 256_000)?;
        }
    }
    if covered.contains("gpt-5.6-luna") {
        validate_token_tiers(&models, "gpt-5.6-luna", 272_000)?;
    }
    validate_time_windows(&models)?;

    let document_updated_at = parse_document_updated_at(html)?;
    let content_hash = format!("{:x}", Sha256::digest(html.as_bytes()));
    // A snapshot revision covers both the official document and the local
    // pricing policy. This prevents a policy update from colliding with an
    // older snapshot when the Go HTML itself is unchanged.
    let revision = revision_for_content_hash(&content_hash);
    apply_official_pricing_policy(&mut models, limits.window_month);
    sort_models(&mut models);

    Ok(PricingSnapshot {
        revision,
        activated_at: Utc::now().to_rfc3339(),
        document_updated_at,
        source_url: SOURCE_URL.to_string(),
        content_hash,
        limits,
        models,
        adjustment_policy_version: ADJUSTMENT_POLICY_VERSION.to_string(),
    })
}

fn validate_token_tiers(models: &[PricingModel], id: &str, boundary: i64) -> Result<()> {
    let tiers = models
        .iter()
        .filter(|model| model.model_id == id)
        .collect::<Vec<_>>();
    let label = format!("{}K", boundary / 1000);
    if tiers.len() != 2
        || !tiers
            .iter()
            .any(|tier| tier.max_input_tokens == Some(boundary))
        || !tiers
            .iter()
            .any(|tier| tier.min_input_tokens == Some(boundary + 1))
    {
        bail!("OpenCode Go {id} must contain complete {label} pricing tiers");
    }
    Ok(())
}

fn add_adjustments(models: &mut [PricingModel]) {
    for model in models {
        model.adjustments.clear();
        match model.model_id.as_str() {
            "minimax-m3" => {
                model.adjustments = vec![
                    PricingAdjustment {
                        label: ">512K input".to_string(),
                        multiplier: 2.0,
                        applies_to: "input,output,cache_read,cache_write".to_string(),
                    },
                    PricingAdjustment {
                        label: "priority service tier".to_string(),
                        multiplier: 1.5,
                        applies_to: "input,output,cache_read,cache_write".to_string(),
                    },
                    PricingAdjustment {
                        label: ">512K + priority".to_string(),
                        multiplier: 3.0,
                        applies_to: "input,output,cache_read,cache_write".to_string(),
                    },
                ];
            }
            "minimax-m2.7" | "minimax-m2.5" => {
                model.adjustments = vec![PricingAdjustment {
                    label: "highspeed alias".to_string(),
                    multiplier: 2.0,
                    applies_to: "input,output".to_string(),
                }];
            }
            _ => {}
        }
    }
}

fn apply_official_pricing_policy(models: &mut [PricingModel], monthly_limit: f64) {
    apply_official_multipliers(models, monthly_limit);
    add_adjustments(models);
}

fn apply_official_multipliers(models: &mut [PricingModel], monthly_limit: f64) {
    for model in models.iter_mut() {
        model.quota_multiplier = monthly_limit / model.usage;
    }
}

fn revision_for_content_hash(content_hash: &str) -> String {
    let prefix = content_hash.chars().take(16).collect::<String>();
    format!("go-{prefix}-{ADJUSTMENT_POLICY_VERSION}")
}

fn unique_revision_for_content_hash(content_hash: &str) -> String {
    format!(
        "{}-{}",
        revision_for_content_hash(content_hash),
        uuid::Uuid::new_v4().simple()
    )
}

fn sort_models(models: &mut [PricingModel]) {
    models.sort_by(|left, right| {
        left.model_id
            .cmp(&right.model_id)
            .then(left.min_input_tokens.cmp(&right.min_input_tokens))
            .then(left.time_window.cmp(&right.time_window))
    });
}

fn select_priced_model<'a>(
    candidates: &[&'a PricingModel],
    at: DateTime<Utc>,
) -> Option<&'a PricingModel> {
    if candidates.is_empty() {
        return None;
    }
    let scheduled = candidates
        .iter()
        .any(|entry| entry.time_window != PricingTimeWindow::Always);
    if scheduled {
        let prefer = if is_official_peak_utc(at) {
            PricingTimeWindow::Peak
        } else {
            PricingTimeWindow::OffPeak
        };
        return candidates
            .iter()
            .copied()
            .find(|entry| entry.time_window == prefer)
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .find(|entry| entry.time_window == PricingTimeWindow::Peak)
            })
            .or_else(|| candidates.first().copied());
    }
    candidates
        .iter()
        .copied()
        .max_by_key(|entry| entry.model_id.len())
}

fn is_official_peak_utc(at: DateTime<Utc>) -> bool {
    // Official Go docs: DeepSeek Peak hours are 01:00-04:00 and 06:00-10:00 UTC,
    // Monday through Friday. Saturday and Sunday are off-peak all day.
    //
    // The weekday is read in UTC rather than on DeepSeek's Beijing clock. That is
    // sound only because both windows end at 10:00 UTC, well before 16:00 UTC --
    // the instant a UTC date and a Beijing date begin to disagree. If a window
    // ever extends past 16:00 UTC this needs to move to Asia/Shanghai.
    if at.weekday().number_from_monday() > 5 {
        return false;
    }
    let minutes = at.hour() * 60 + at.minute();
    (60..240).contains(&minutes) || (360..600).contains(&minutes)
}

fn parse_time_window(name: &str) -> PricingTimeWindow {
    let lower = name.to_ascii_lowercase();
    if lower.contains("off-peak") || lower.contains("off peak") || lower.contains("offpeak") {
        return PricingTimeWindow::OffPeak;
    }
    if lower.contains("peak") {
        return PricingTimeWindow::Peak;
    }
    PricingTimeWindow::Always
}

fn validate_time_windows(models: &[PricingModel]) -> Result<()> {
    let mut windows_by_id: HashMap<String, HashSet<PricingTimeWindow>> = HashMap::new();
    for model in models {
        windows_by_id
            .entry(model.model_id.clone())
            .or_default()
            .insert(model.time_window);
    }
    for (id, windows) in windows_by_id {
        let scheduled = windows.contains(&PricingTimeWindow::Peak)
            || windows.contains(&PricingTimeWindow::OffPeak);
        if !scheduled {
            continue;
        }
        if windows.contains(&PricingTimeWindow::Always) {
            bail!("OpenCode Go {id} mixes scheduled and unscheduled pricing rows");
        }
        if !windows.contains(&PricingTimeWindow::Peak)
            || !windows.contains(&PricingTimeWindow::OffPeak)
        {
            bail!("OpenCode Go {id} must contain both Peak and Off-Peak pricing rows");
        }
    }
    Ok(())
}

pub fn normalize_model_name(name: &str) -> String {
    name.trim().to_lowercase().replace([' ', '_', '/'], "-")
}

fn canonical_display_name(name: &str) -> String {
    let base = name.split('(').next().unwrap_or(name);
    base.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn parse_token_tier(name: &str) -> Result<(Option<i64>, Option<i64>)> {
    // Official Go docs use ≤ / > token tiers (256K for Qwen, 272K for Luna).
    for boundary in [272_000_i64, 256_000, 200_000] {
        let label = format!("{}K", boundary / 1000);
        let label_lower = label.to_ascii_lowercase();
        if !(name.contains(&label) || name.contains(&label_lower)) {
            continue;
        }
        if name.contains('≤') || name.contains("<=") {
            return Ok((None, Some(boundary)));
        }
        if name.contains('>') {
            return Ok((Some(boundary + 1), None));
        }
        bail!("unrecognized token tier in {name}");
    }
    Ok((None, None))
}

fn is_placeholder_price(value: &str) -> bool {
    matches!(value.trim(), "-" | "—" | "–")
}

fn is_unpriced_promo_row(row: &[String]) -> bool {
    row.len() == 6 && row.iter().skip(1).all(|cell| is_placeholder_price(cell))
}

fn parse_dollar(value: &str, allow_dash: bool) -> Result<Option<f64>> {
    let value = value.trim();
    if allow_dash && matches!(value, "-" | "—" | "–") {
        return Ok(None);
    }
    let number = value
        .strip_prefix('$')
        .ok_or_else(|| anyhow!("expected USD value, got {value}"))?
        .replace(',', "");
    let parsed = number
        .parse::<f64>()
        .with_context(|| format!("invalid USD value {value}"))?;
    if !parsed.is_finite() || parsed < 0.0 {
        bail!("USD value must be finite and non-negative");
    }
    Ok(Some(parsed))
}

fn parse_limit(plain: &str, marker: &str) -> Result<f64> {
    let start = plain
        .find(marker)
        .ok_or_else(|| anyhow!("OpenCode Go page is missing {marker}"))?;
    let tail = &plain[start + marker.len()..];
    let dollar = tail
        .find('$')
        .ok_or_else(|| anyhow!("OpenCode Go page is missing USD value after {marker}"))?;
    let value = tail[dollar..]
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("OpenCode Go page is missing USD value after {marker}"))?;
    parse_dollar(
        value.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.'),
        false,
    )?
    .ok_or_else(|| anyhow!("OpenCode Go page is missing USD value after {marker}"))
}

fn parse_document_updated_at(html: &str) -> Result<String> {
    let marker = "title=\"Last updated:\"";
    let start = html
        .find(marker)
        .ok_or_else(|| anyhow!("OpenCode Go page is missing Last updated metadata"))?;
    let tail = &html[start..];
    let datetime = "datetime=\"";
    let value_start = tail
        .find(datetime)
        .ok_or_else(|| anyhow!("OpenCode Go page is missing Last updated datetime"))?
        + datetime.len();
    let value_end = tail[value_start..]
        .find('"')
        .ok_or_else(|| anyhow!("OpenCode Go Last updated datetime is malformed"))?
        + value_start;
    let value = &tail[value_start..value_end];
    chrono::DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("invalid OpenCode Go Last updated datetime {value}"))?;
    Ok(value.to_string())
}

fn has_headers(table: &[Vec<String>], expected: &[&str]) -> bool {
    table.first().is_some_and(|row| {
        let actual = row
            .iter()
            .map(|cell| cell.trim().to_ascii_lowercase())
            .collect::<Vec<_>>();
        actual == expected
    })
}

fn extract_tables(html: &str) -> Result<Vec<Vec<Vec<String>>>> {
    let mut tables = Vec::new();
    let mut remainder = html;
    while let Some(start) = remainder.find("<table") {
        let table = &remainder[start..];
        let end = table
            .find("</table>")
            .ok_or_else(|| anyhow!("OpenCode Go page contains an unterminated table"))?;
        tables.push(extract_rows(&table[..end + "</table>".len()])?);
        remainder = &table[end + "</table>".len()..];
    }
    Ok(tables)
}

fn extract_rows(table: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut remainder = table;
    while let Some(start) = remainder.find("<tr") {
        let row = &remainder[start..];
        let end = row
            .find("</tr>")
            .ok_or_else(|| anyhow!("OpenCode Go page contains an unterminated row"))?;
        rows.push(extract_cells(&row[..end + "</tr>".len()])?);
        remainder = &row[end + "</tr>".len()..];
    }
    Ok(rows)
}

fn extract_cells(row: &str) -> Result<Vec<String>> {
    let mut cells = Vec::new();
    let mut cursor = 0;
    while cursor < row.len() {
        let th = row[cursor..].find("<th").map(|index| (index, "</th>"));
        let td = row[cursor..].find("<td").map(|index| (index, "</td>"));
        let Some((relative, end_tag)) = [th, td].into_iter().flatten().min_by_key(|item| item.0)
        else {
            break;
        };
        let start = cursor + relative;
        let content_start = row[start..]
            .find('>')
            .ok_or_else(|| anyhow!("OpenCode Go page contains a malformed table cell"))?
            + start
            + 1;
        let content_end = row[content_start..]
            .find(end_tag)
            .ok_or_else(|| anyhow!("OpenCode Go page contains an unterminated table cell"))?
            + content_start;
        cells.push(collapse_whitespace(&strip_tags(
            &row[content_start..content_end],
        )));
        cursor = content_end + end_tag.len();
    }
    Ok(cells)
}

fn strip_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut characters = input.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '<' if characters.peek().is_some_and(|next| {
                next.is_ascii_alphabetic() || matches!(next, '/' | '!' | '?')
            }) =>
            {
                in_tag = true
            }
            '>' if in_tag => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    decode_entities(&output)
}

fn decode_entities(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remainder = input;
    while let Some(start) = remainder.find('&') {
        output.push_str(&remainder[..start]);
        let entity = &remainder[start..];
        let Some(end) = entity.find(';') else {
            output.push_str(entity);
            return output;
        };
        let code = &entity[1..end];
        let decoded = match code {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ if code.starts_with("#x") => u32::from_str_radix(&code[2..], 16)
                .ok()
                .and_then(char::from_u32),
            _ if code.starts_with('#') => code[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        if let Some(character) = decoded {
            output.push(character);
        } else {
            output.push_str(&entity[..=end]);
        }
        remainder = &entity[end + 1..];
    }
    output.push_str(remainder);
    output
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{
        embedded_seed, ensure_current_adjustment_policy, ensure_seed_model_coverage,
        fetch_official_snapshot, legacy_policy_needs_multiplier_repair, parse_official_html,
    };
    use chrono::{DateTime, Utc};

    #[test]
    fn seed_coverage_backfills_missing_models_and_prices_them() {
        // A snapshot from a database created before the seed grew: no muse rows.
        let mut snapshot = embedded_seed();
        let previous_revision = snapshot.revision.clone();
        snapshot
            .models
            .retain(|entry| !entry.model_id.starts_with("muse-spark-1.2"));
        assert!(
            snapshot
                .estimate("muse-spark-1.2", 1_000, 100, 0, 0, None)
                .cost
                .is_none()
        );

        let repaired = ensure_seed_model_coverage(snapshot);
        assert_ne!(repaired.revision, previous_revision);
        let estimate = repaired.estimate("muse-spark-1.2", 1_000, 100, 0, 0, None);
        let cost = estimate.cost.expect("muse-spark-1.2 must be priced");
        // 1k uncached input @ $0.10 + 100 output @ $0.20 per M tokens.
        assert!((cost - (1_000.0 * 0.10 + 100.0 * 0.20) / 1_000_000.0).abs() < 1e-9);
        assert!(
            repaired
                .estimate("muse-spark-1.2-contributor", 1_000, 100, 0, 0, None)
                .cost
                .is_some()
        );
    }

    #[test]
    fn seed_coverage_never_overwrites_existing_rows() {
        // A snapshot whose muse-contributor row came from the official table
        // (or carries a user-edited multiplier) must survive the backfill.
        let mut snapshot = embedded_seed();
        snapshot
            .models
            .retain(|entry| entry.model_id != "muse-spark-1.2-contributor");
        let edited = super::PricingModel {
            model_id: "muse-spark-1.2-contributor".to_string(),
            display_name: "Muse Spark 1.2 Contributor".to_string(),
            input: 0.10,
            output: 0.20,
            cache_read: 0.002,
            cache_write: None,
            usage: 60.0,
            quota_multiplier: 2.5,
            min_input_tokens: None,
            max_input_tokens: None,
            time_window: super::PricingTimeWindow::Always,
            adjustments: Vec::new(),
        };
        snapshot.models.push(edited);

        let repaired = ensure_seed_model_coverage(snapshot);
        let row = repaired
            .models
            .iter()
            .find(|entry| entry.model_id == "muse-spark-1.2-contributor")
            .unwrap();
        assert_eq!(row.quota_multiplier, 2.5);
    }

    #[test]
    fn seed_coverage_is_noop_when_every_seed_model_is_present() {
        let snapshot = embedded_seed();
        let repaired = ensure_seed_model_coverage(snapshot.clone());
        assert_eq!(repaired.revision, snapshot.revision);
        assert_eq!(repaired.activated_at, snapshot.activated_at);
    }

    #[test]
    fn seed_coverage_does_not_revive_models_outside_the_muse_allowlist() {
        let mut snapshot = embedded_seed();
        snapshot.models.retain(|entry| entry.model_id != "grok-4.5");

        let repaired = ensure_seed_model_coverage(snapshot);

        assert!(
            repaired
                .models
                .iter()
                .all(|entry| entry.model_id != "grok-4.5")
        );
    }

    #[test]
    fn seed_uses_go_usage_as_quota_multiplier() {
        let snapshot = embedded_seed();
        let grok = snapshot
            .models
            .iter()
            .find(|entry| entry.model_id == "grok-4.5")
            .unwrap();
        let glm = snapshot
            .models
            .iter()
            .find(|entry| entry.model_id == "glm-5.2")
            .unwrap();
        assert_eq!(grok.quota_multiplier, 4.0);
        assert_eq!(glm.quota_multiplier, 1.0);
    }

    #[test]
    fn pro_usage_allowance_is_applied_after_the_official_table_rates() {
        let snapshot = embedded_seed();
        for (model_id, prompt, cached, completion, official_monthly_requests) in [
            ("deepseek-v4-pro", 82_750, 82_000, 290, 5_200.0),
            ("mimo-v2.5-pro", 86_790, 86_000, 305, 16_300.0),
        ] {
            let model = snapshot
                .models
                .iter()
                .find(|entry| entry.model_id == model_id)
                .unwrap();
            assert_eq!(model.usage, 15.0);
            assert_eq!(model.quota_multiplier, 4.0);

            let estimate = snapshot.estimate_at(
                model_id,
                prompt,
                completion,
                cached,
                0,
                None,
                DateTime::parse_from_rfc3339("2026-08-16T12:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            );
            let estimated_monthly_requests = snapshot.limits.window_month / estimate.cost.unwrap();
            assert!(
                (estimated_monthly_requests / official_monthly_requests - 1.0).abs() < 0.01,
                "{model_id}: {estimated_monthly_requests} != {official_monthly_requests}",
            );
            assert_eq!(estimate.quota_multiplier, Some(4.0));
        }

        let grok = snapshot
            .models
            .iter()
            .find(|entry| entry.model_id == "grok-4.5")
            .unwrap();
        assert_eq!(grok.usage, 15.0);
        assert_eq!(grok.quota_multiplier, 4.0);
        assert_eq!(
            snapshot.estimate("grok-4.5", 1_000_000, 0, 0, 0, None).cost,
            Some(8.0)
        );
    }

    #[test]
    fn policy_upgrade_repairs_persisted_pro_quota_multipliers() {
        let mut snapshot = embedded_seed();
        snapshot.adjustment_policy_version = "local-v2".to_string();
        for model in &mut snapshot.models {
            if matches!(model.model_id.as_str(), "deepseek-v4-pro" | "mimo-v2.5-pro") {
                model.quota_multiplier = 1.0;
            }
        }

        let upgraded = ensure_current_adjustment_policy(snapshot);
        for model_id in ["deepseek-v4-pro", "mimo-v2.5-pro"] {
            let model = upgraded
                .models
                .iter()
                .find(|entry| entry.model_id == model_id)
                .unwrap();
            assert_eq!(model.quota_multiplier, 4.0);
        }
    }

    #[test]
    fn legacy_snapshot_json_drops_old_price_multiplier_and_repairs_applied_multiplier() {
        let mut value = serde_json::to_value(embedded_seed()).unwrap();
        value["adjustment_policy_version"] = serde_json::Value::String("local-v2".into());
        for model in value["models"].as_array_mut().unwrap() {
            let object = model.as_object_mut().unwrap();
            if matches!(
                object.get("model_id").and_then(serde_json::Value::as_str),
                Some("deepseek-v4-pro" | "mimo-v2.5-pro")
            ) {
                object.insert("official_price_multiplier".into(), serde_json::json!(4.0));
                object.insert("quota_multiplier".into(), serde_json::json!(1.0));
            }
        }

        let persisted = serde_json::from_value(value).unwrap();
        let upgraded = ensure_current_adjustment_policy(persisted);
        for model_id in ["deepseek-v4-pro", "mimo-v2.5-pro"] {
            let model = upgraded
                .models
                .iter()
                .find(|entry| entry.model_id == model_id)
                .unwrap();
            assert_eq!(model.quota_multiplier, 4.0);
        }
        assert!(
            !serde_json::to_string(&upgraded)
                .unwrap()
                .contains("official_price_multiplier")
        );
    }

    #[test]
    fn editable_policy_versions_are_never_rebased() {
        assert!(legacy_policy_needs_multiplier_repair("local-v2"));
        assert!(!legacy_policy_needs_multiplier_repair("local-v3"));
        assert!(!legacy_policy_needs_multiplier_repair("local-v4"));
        assert!(!legacy_policy_needs_multiplier_repair("local-v99"));

        let mut snapshot = embedded_seed();
        snapshot.adjustment_policy_version = "local-v3".to_string();
        snapshot
            .models
            .iter_mut()
            .filter(|model| model.model_id == "qwen3.7-plus")
            .for_each(|model| model.quota_multiplier = 0.75);
        let upgraded = ensure_current_adjustment_policy(snapshot);
        assert!(
            upgraded
                .models
                .iter()
                .filter(|model| model.model_id == "qwen3.7-plus")
                .all(|model| model.quota_multiplier == 0.75)
        );
    }

    #[test]
    fn minimax_adjustments_follow_local_policy() {
        let snapshot = embedded_seed();
        let at_boundary = snapshot.estimate("minimax-m3", 512_000, 10, 0, 0, None);
        let over_boundary = snapshot.estimate("minimax-m3", 512_001, 10, 0, 0, None);
        assert!((over_boundary.local_adjustment_multiplier.unwrap() - 2.0).abs() < 1e-12);
        assert_eq!(at_boundary.local_adjustment_multiplier, Some(1.0));
        let priority = snapshot.estimate("minimax-m3", 1000, 10, 0, 0, Some("priority"));
        assert!((priority.local_adjustment_multiplier.unwrap() - 1.5).abs() < 1e-12);
        let combined = snapshot.estimate("minimax-m3", 512_001, 10, 0, 0, Some("priority"));
        assert!((combined.local_adjustment_multiplier.unwrap() - 3.0).abs() < 1e-12);
    }

    #[test]
    fn highspeed_only_doubles_input_and_output() {
        let snapshot = embedded_seed();
        let normal = snapshot
            .estimate("minimax-m2.7", 1000, 100, 400, 300, None)
            .cost
            .unwrap();
        let fast = snapshot
            .estimate("minimax-m2.7-highspeed", 1000, 100, 400, 300, None)
            .cost
            .unwrap();
        let expected = (300.0 * 0.60 + 100.0 * 2.40 + 400.0 * 0.06 + 300.0 * 0.375) / 1_000_000.0;
        assert!((fast - expected).abs() < 1e-12);
        assert!(fast < normal * 2.0);
    }

    #[test]
    fn unknown_model_is_unpriced() {
        let estimate = embedded_seed().estimate("future-model", 1000, 100, 0, 0, None);
        assert_eq!(estimate.cost, None);
        assert_eq!(estimate.cost_state, "unpriced");
        let prefixed = embedded_seed().estimate("provider-minimax-m3", 1000, 100, 0, 0, None);
        assert_eq!(prefixed.cost, None);
    }

    #[test]
    fn zen_free_models_do_not_enter_go_quota() {
        for model_id in [
            "mimo-v2.5-free",
            "big-pickle",
            "hy3-free",
            "muse-spark-1.2-contributor-free",
        ] {
            let estimate = embedded_seed().estimate(model_id, 1000, 100, 0, 0, None);
            assert_eq!(estimate.cost, None, "{model_id}");
            assert_eq!(estimate.cost_state, "free", "{model_id}");
            assert_eq!(estimate.quota_multiplier, None, "{model_id}");
        }
        let paid = embedded_seed().estimate("deepseek-v4-flash", 1000, 100, 0, 0, None);
        assert_eq!(paid.cost_state, "priced");
        assert!(paid.cost.is_some());
        let go_named_free = embedded_seed().estimate("ox-alpha-free", 1000, 100, 0, 0, None);
        assert_eq!(go_named_free.cost_state, "unpriced");
        assert_ne!(go_named_free.cost_state, "free");
        let suffix_is_not_zen =
            embedded_seed().estimate("brand-new-promo-free", 1000, 100, 0, 0, None);
        assert_eq!(suffix_is_not_zen.cost_state, "unpriced");
    }

    #[test]
    fn cache_write_dash_falls_back_to_new_input_price() {
        let estimate = embedded_seed().estimate("glm-5.2", 1000, 0, 0, 1000, None);
        assert!((estimate.cost.unwrap() - 0.0014).abs() < 1e-12);
    }

    #[test]
    fn parses_official_fixture() {
        let snapshot =
            parse_official_html(include_str!("../tests/fixtures/opencode-go.html")).unwrap();
        assert_eq!(snapshot.limits.window_5h, 12.0);
        assert_eq!(snapshot.limits.window_week, 30.0);
        assert_eq!(snapshot.limits.window_month, 60.0);
        assert_eq!(snapshot.models.len(), 25);
        assert!(
            snapshot
                .models
                .iter()
                .any(|entry| entry.model_id == "kimi-k3" && entry.quota_multiplier == 4.0)
        );
        for model_id in [
            "deepseek-v4-pro",
            "deepseek-v4-flash",
            "mimo-v2.5-pro",
            "gpt-5.6-luna",
        ] {
            let model = snapshot
                .models
                .iter()
                .find(|entry| entry.model_id == model_id)
                .unwrap();
            assert_eq!(model.quota_multiplier, 4.0);
        }
        assert_eq!(
            snapshot
                .models
                .iter()
                .filter(|entry| entry.model_id == "deepseek-v4-flash")
                .count(),
            2
        );
        assert!(
            snapshot
                .models
                .iter()
                .any(|entry| entry.model_id == "hy3" && entry.quota_multiplier == 1.0)
        );
        assert!(
            !snapshot
                .models
                .iter()
                .any(|entry| entry.model_id == "ox-alpha-free"),
            "dash-priced Go promos must not enter the USD snapshot"
        );
        let luna_tiers = snapshot
            .models
            .iter()
            .filter(|entry| entry.model_id == "gpt-5.6-luna")
            .count();
        assert_eq!(luna_tiers, 2);
    }

    #[test]
    fn deepseek_uses_utc_peak_and_off_peak_rows() {
        let snapshot = embedded_seed();
        // Both instants are on Monday 2026-08-17. They used to sit on Sunday
        // 2026-08-16, which made the `peak` half assert the weekend bug: a
        // Sunday 07:00Z session is off-peak, so it cannot cost 1.76. Keeping
        // this pair on a weekday leaves it testing the hour axis only; the
        // weekday axis is covered by the test below.
        let off_peak = DateTime::parse_from_rfc3339("2026-08-17T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let peak = DateTime::parse_from_rfc3339("2026-08-17T07:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let off = snapshot
            .estimate_at("deepseek-v4-flash", 1_000_000, 0, 0, 0, None, off_peak)
            .cost
            .unwrap();
        let on = snapshot
            .estimate_at("deepseek-v4-flash", 1_000_000, 0, 0, 0, None, peak)
            .cost
            .unwrap();
        assert!((off - 0.88).abs() < 1e-12);
        assert!((on - 1.76).abs() < 1e-12);
    }

    #[test]
    fn deepseek_weekend_is_off_peak_at_every_hour() {
        // Peak is Monday through Friday only, so both weekend days must price
        // at the Off-Peak rows even inside the 01:00-04:00 / 06:00-10:00 windows.
        let snapshot = embedded_seed();
        for at in [
            "2026-08-15T02:00:00Z", // Saturday, inside the first peak window
            "2026-08-15T07:00:00Z", // Saturday, inside the second
            "2026-08-16T02:00:00Z", // Sunday, inside the first
            "2026-08-16T07:00:00Z", // Sunday, inside the second
        ] {
            let when = DateTime::parse_from_rfc3339(at)
                .unwrap()
                .with_timezone(&Utc);
            let cost = snapshot
                .estimate_at("deepseek-v4-flash", 1_000_000, 0, 0, 0, None, when)
                .cost
                .unwrap();
            assert!(
                (cost - 0.88).abs() < 1e-12,
                "{at} is on a weekend and must bill off-peak (0.88), got {cost}"
            );
        }

        // The Friday and Monday either side of that weekend still bill at peak,
        // so this is a weekday boundary and not a blanket disable.
        for at in ["2026-08-14T07:00:00Z", "2026-08-17T07:00:00Z"] {
            let when = DateTime::parse_from_rfc3339(at)
                .unwrap()
                .with_timezone(&Utc);
            let cost = snapshot
                .estimate_at("deepseek-v4-flash", 1_000_000, 0, 0, 0, None, when)
                .cost
                .unwrap();
            assert!(
                (cost - 1.76).abs() < 1e-12,
                "{at} is a weekday peak hour and must bill 1.76, got {cost}"
            );
        }
    }

    #[test]
    fn rejects_incomplete_peak_off_peak_pair() {
        let fixture = include_str!("../tests/fixtures/opencode-go.html").replace(
            "<tr><td>DeepSeek V4 Flash (Peak)</td><td>$0.44</td><td>$1.32</td><td>$0.014</td><td>-</td><td>$15</td></tr>",
            "",
        );
        assert!(
            parse_official_html(&fixture)
                .unwrap_err()
                .to_string()
                .contains("must contain both Peak and Off-Peak")
        );
    }

    #[test]
    fn rejects_model_id_without_a_matching_price_row() {
        let fixture = include_str!("../tests/fixtures/opencode-go.html");
        let incomplete = fixture.replace(
            "<tr><td>Grok 4.5</td><td>$2.00</td><td>$6.00</td><td>$0.30</td><td>-</td><td>$15</td></tr>",
            "",
        );
        assert!(
            parse_official_html(&incomplete)
                .unwrap_err()
                .to_string()
                .contains("model ID table contains models without pricing rows")
        );
    }

    #[test]
    fn accepts_official_model_removal_when_both_tables_still_match() {
        let fixture = include_str!("../tests/fixtures/opencode-go.html")
            .replace(
                "<tr><td>Grok 4.5</td><td>$2.00</td><td>$6.00</td><td>$0.30</td><td>-</td><td>$15</td></tr>",
                "",
            )
            .replace(
                "<tr><td>Grok 4.5</td><td>grok-4.5</td><td>x</td><td>x</td></tr>",
                "",
            );
        let snapshot = parse_official_html(&fixture).unwrap();
        assert!(
            snapshot
                .models
                .iter()
                .all(|model| model.model_id != "grok-4.5")
        );
    }

    #[test]
    fn qwen_tier_validation_is_conditional_on_the_model_being_present() {
        let fixture = include_str!("../tests/fixtures/opencode-go.html")
            .replace(
                "<tr><td>Qwen3.7 Plus (&#x2264; 256K tokens)</td><td>$0.40</td><td>$1.60</td><td>$0.04</td><td>$0.50</td><td>$60</td></tr>",
                "",
            )
            .replace(
                "<tr><td>Qwen3.7 Plus (&gt; 256K tokens)</td><td>$1.20</td><td>$4.80</td><td>$0.12</td><td>$1.50</td><td>$60</td></tr>",
                "",
            )
            .replace(
                "<tr><td>Qwen3.7 Plus</td><td>qwen3.7-plus</td><td>x</td><td>x</td></tr>",
                "",
            );
        let snapshot = parse_official_html(&fixture).unwrap();
        assert!(
            snapshot
                .models
                .iter()
                .all(|model| model.model_id != "qwen3.7-plus")
        );
    }

    #[test]
    fn rejects_structurally_valid_but_empty_catalog() {
        let fixture = r#"
            <p>5 hour limit — $12 of usage</p>
            <p>Weekly limit — $30 of usage</p>
            <p>Monthly limit — $60 of usage</p>
            <table><thead><tr><th>Model</th><th>Input</th><th>Output</th><th>Cached Read</th><th>Cached Write</th><th>Usage</th></tr></thead><tbody></tbody></table>
            <table><thead><tr><th>Model</th><th>Model ID</th><th>Endpoint</th><th>AI SDK Package</th></tr></thead><tbody></tbody></table>
            <time datetime="2026-07-17T15:53:00.000Z">Jul 17, 2026</time>
        "#;
        assert!(
            parse_official_html(fixture)
                .unwrap_err()
                .to_string()
                .contains("must not be empty")
        );
    }

    #[test]
    fn parsed_limit_and_price_changes_drive_dynamic_multiplier() {
        let fixture = include_str!("../tests/fixtures/opencode-go.html")
            .replace(
                "Monthly limit — $60 of usage",
                "Monthly limit — $90 of usage",
            )
            .replace(
                "<tr><td>Kimi K3</td><td>$3.00</td>",
                "<tr><td>Kimi K3</td><td>$3.50</td>",
            );
        let snapshot = parse_official_html(&fixture).unwrap();
        let kimi = snapshot
            .models
            .iter()
            .find(|model| model.model_id == "kimi-k3")
            .unwrap();
        assert_eq!(snapshot.limits.window_month, 90.0);
        assert_eq!(kimi.input, 3.5);
        assert_eq!(kimi.quota_multiplier, 6.0);
    }

    #[test]
    fn accepts_new_models_with_an_official_id_and_complete_prices() {
        let fixture = include_str!("../tests/fixtures/opencode-go.html")
            .replace("\r\n", "\n")
            .replace(
                "</tbody></table>\n<table><thead><tr><th>Model</th><th>Model ID</th>",
                "<tr><td>Future Model</td><td>$1.00</td><td>$2.00</td><td>$0.10</td><td>-</td><td>$60</td></tr></tbody></table>\n<table><thead><tr><th>Model</th><th>Model ID</th>",
            )
            .replace(
                "</tbody></table>\n<footer>",
                "<tr><td>Future Model</td><td>future-model</td><td>x</td><td>x</td></tr></tbody></table>\n<footer>",
            );
        let snapshot = parse_official_html(&fixture).unwrap();
        assert!(
            snapshot
                .models
                .iter()
                .any(|model| model.model_id == "future-model")
        );
    }

    #[test]
    fn rejects_missing_or_reordered_price_columns() {
        let fixture = include_str!("../tests/fixtures/opencode-go.html").replace(
            "<th>Input</th><th>Output</th>",
            "<th>Output</th><th>Input</th>",
        );
        assert!(
            parse_official_html(&fixture)
                .unwrap_err()
                .to_string()
                .contains("pricing table was not found")
        );
    }

    #[test]
    fn rejects_duplicate_price_rows() {
        let fixture = include_str!("../tests/fixtures/opencode-go.html").replace(
            "<tr><td>Grok 4.5</td><td>$2.00</td><td>$6.00</td><td>$0.30</td><td>-</td><td>$15</td></tr>",
            "<tr><td>Grok 4.5</td><td>$2.00</td><td>$6.00</td><td>$0.30</td><td>-</td><td>$15</td></tr><tr><td>Grok 4.5</td><td>$2.00</td><td>$6.00</td><td>$0.30</td><td>-</td><td>$15</td></tr>",
        );
        assert!(
            parse_official_html(&fixture)
                .unwrap_err()
                .to_string()
                .contains("duplicate row")
        );
    }

    #[tokio::test]
    #[ignore = "requires live access to opencode.ai"]
    async fn live_official_document_still_matches_the_parser() {
        let snapshot = fetch_official_snapshot(&crate::models::AppConfig::default())
            .await
            .unwrap();
        assert_eq!(snapshot.source_url, super::SOURCE_URL);
        assert!(snapshot.models.len() >= 18);
    }
}
