use anyhow::{Context, Result, bail};
use chrono::{Duration as ChronoDuration, Utc};
use reqwest::header::{COOKIE, HeaderValue, LOCATION, USER_AGENT};
use reqwest::{Client, StatusCode, Url};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::browser::browser_profile_paths;
use crate::db::Database;
use crate::models::{AccountType, UsageWindow, UsageWindowKind};
use crate::pricing::PricingLimits;

const AUTH_URL: &str = "https://opencode.ai/auth";
const MAX_REDIRECTS: usize = 8;
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36";
const APPROVED_CONSOLE_HOSTS: &[&str] = &["opencode.ai", "auth.opencode.ai", "console.opencode.ai"];

#[derive(Debug, Clone, PartialEq)]
pub struct ConsoleUsageSnapshot {
    pub window_5h_percent: f64,
    pub window_week_percent: f64,
    pub window_month_percent: f64,
    pub resets_in_5h_minutes: Option<i64>,
    pub resets_in_week_minutes: Option<i64>,
    pub resets_in_month_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsoleUsageRefreshResult {
    pub usage: UsageWindow,
    pub source: &'static str,
}

pub async fn refresh_managed_account_usage(
    db: &parking_lot::Mutex<Database>,
    data_dir: &Path,
    account_id: &str,
    limits: &PricingLimits,
) -> Result<ConsoleUsageRefreshResult> {
    let profile = {
        let db = db.lock();
        let account = db
            .get_account(account_id)?
            .with_context(|| format!("account {account_id} not found"))?;
        if account.account_type != AccountType::Managed || !account.setup_step.is_ready() {
            bail!("only ready managed accounts can refresh console usage");
        }
        first_existing_profile(data_dir, account_id)?
    };
    let cookies = read_opencode_cookies(&profile)?;
    if cookies.is_empty() {
        bail!(
            "browser profile has no OpenCode session cookies; open the OpenCode console once and sign in"
        );
    }

    let snapshot = fetch_console_usage(&cookies).await?;
    let db = db.lock();
    apply_snapshot(&db, account_id, limits, &snapshot)?;
    let usage = db.account_usage_with_limits(account_id, limits)?;
    Ok(ConsoleUsageRefreshResult {
        usage,
        source: "browser_profile_console",
    })
}

fn first_existing_profile(data_dir: &Path, account_id: &str) -> Result<PathBuf> {
    let paths = browser_profile_paths(data_dir, account_id)?;
    paths
        .into_iter()
        .find(|path| path.is_dir())
        .with_context(|| {
            format!(
                "browser profile for account {account_id} is missing; open the console once first"
            )
        })
}

fn apply_snapshot(
    db: &Database,
    account_id: &str,
    limits: &PricingLimits,
    snapshot: &ConsoleUsageSnapshot,
) -> Result<()> {
    let windows = [
        (
            UsageWindowKind::FiveHours,
            snapshot.window_5h_percent,
            snapshot.resets_in_5h_minutes,
            limits.window_5h,
        ),
        (
            UsageWindowKind::Week,
            snapshot.window_week_percent,
            snapshot.resets_in_week_minutes,
            limits.window_week,
        ),
        (
            UsageWindowKind::Month,
            snapshot.window_month_percent,
            snapshot.resets_in_month_minutes,
            limits.window_month,
        ),
    ];
    for (window, percent, resets, limit) in windows {
        if !db.calibrate_account_usage(account_id, window, percent, resets, limit)? {
            bail!("account {account_id} disappeared while refreshing usage");
        }
    }
    Ok(())
}

async fn fetch_console_usage(cookies: &BTreeMap<String, String>) -> Result<ConsoleUsageSnapshot> {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build console usage HTTP client")?;

    let cookie_header = cookie_header_value(cookies)?;
    let mut url = Url::parse(AUTH_URL).expect("auth url is valid");
    let mut html = String::new();

    for _ in 0..MAX_REDIRECTS {
        let response = client
            .get(url.clone())
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(COOKIE, cookie_header.clone())
            .send()
            .await
            .with_context(|| format!("failed to request {}", url))?;
        let status = response.status();
        if matches!(
            status,
            StatusCode::MOVED_PERMANENTLY
                | StatusCode::FOUND
                | StatusCode::SEE_OTHER
                | StatusCode::TEMPORARY_REDIRECT
                | StatusCode::PERMANENT_REDIRECT
        ) {
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .context("OpenCode redirect missing Location")?;
            url = join_console_redirect(&url, location)?;
            continue;
        }
        if !status.is_success() {
            bail!("OpenCode console returned HTTP {}", status.as_u16());
        }
        html = response
            .text()
            .await
            .context("failed to read OpenCode console HTML")?;
        break;
    }

    if html.is_empty() {
        bail!("OpenCode console redirect loop while loading usage");
    }

    // Prefer an explicit Go workspace page when the landing page only links to it.
    if let Some(go_path) = extract_go_workspace_path(&html) {
        let go_url = join_console_redirect(
            &Url::parse("https://opencode.ai").expect("base console url is valid"),
            &go_path,
        )?;
        let response = client
            .get(go_url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(COOKIE, cookie_header)
            .send()
            .await
            .context("failed to load OpenCode Go usage page")?;
        if response.status().is_success() {
            html = response
                .text()
                .await
                .context("failed to read OpenCode Go usage HTML")?;
        }
    }

    parse_console_usage_html(&html)
        .context("could not parse Go usage from the console page; sign in on this profile and open the Go page once")
}

fn join_console_redirect(current: &Url, location: &str) -> Result<Url> {
    let next = current
        .join(location)
        .with_context(|| format!("invalid OpenCode redirect location {location}"))?;
    if next.scheme() != "https"
        || next.port_or_known_default() != Some(443)
        || !next
            .host_str()
            .is_some_and(|host| APPROVED_CONSOLE_HOSTS.contains(&host))
    {
        bail!("OpenCode redirect left the approved HTTPS hosts");
    }
    Ok(next)
}

fn cookie_header_value(cookies: &BTreeMap<String, String>) -> Result<HeaderValue> {
    let mut parts = Vec::with_capacity(cookies.len());
    for (name, value) in cookies {
        if name.is_empty() || value.contains(';') || value.contains(',') {
            continue;
        }
        parts.push(format!("{name}={value}"));
    }
    if parts.is_empty() {
        bail!("no usable OpenCode cookies found in browser profile");
    }
    HeaderValue::from_str(&parts.join("; ")).context("invalid cookie header")
}

pub fn extract_go_workspace_path(html: &str) -> Option<String> {
    let marker = "/workspace/";
    let mut search = html;
    let mut fallback: Option<String> = None;
    while let Some(start) = search.find(marker) {
        let rest = &search[start..];
        let end = rest
            .find(|c: char| ['"', '\'', ' ', '<', '>', '&'].contains(&c))
            .unwrap_or(rest.len());
        let path = &rest[..end];
        let trimmed = path.trim_end_matches('/');
        if trimmed.contains("/go") {
            if trimmed.ends_with("/go") {
                return Some(trimmed.to_string());
            }
            if let Some(idx) = trimmed.find("/go/") {
                return Some(trimmed[..=idx + 2].to_string());
            }
        }
        // "/workspace/wrk_xxx" → append /go
        if fallback.is_none()
            && trimmed.starts_with("/workspace/")
            && trimmed.matches('/').count() == 2
        {
            fallback = Some(format!("{trimmed}/go"));
        }
        search = &rest[marker.len()..];
    }
    fallback
}

pub fn parse_console_usage_html(html: &str) -> Result<ConsoleUsageSnapshot> {
    if let Some(snapshot) = parse_usage_percent_slots(html) {
        return Ok(snapshot);
    }
    if let Some(snapshot) = parse_usage_percent_json(html) {
        return Ok(snapshot);
    }
    bail!("Go usage percentages were not found in the console HTML")
}

fn parse_usage_percent_slots(html: &str) -> Option<ConsoleUsageSnapshot> {
    let mut percents = Vec::new();
    let mut search = html;
    let marker = "data-slot=\"usage-value\"";
    while let Some(idx) = search.find(marker) {
        let after = &search[idx + marker.len()..];
        let Some(gt) = after.find('>') else {
            break;
        };
        let value_part = &after[gt + 1..];
        // Solid SSR wraps values as `<!--$-->0<!--/-->%`.
        let Some(end) = value_part.find("</span>") else {
            search = &value_part[value_part.len().min(1)..];
            continue;
        };
        let raw_chunk = strip_html_comments(&value_part[..end]);
        let raw = raw_chunk.trim().trim_end_matches('%').trim();
        if let Ok(value) = raw.parse::<f64>() {
            percents.push(value.clamp(0.0, 100.0));
        }
        search = &value_part[end + 1..];
        if percents.len() >= 3 {
            break;
        }
    }
    if percents.len() < 3 {
        return None;
    }

    let resets = parse_reset_seconds(html);
    Some(ConsoleUsageSnapshot {
        window_5h_percent: round1(percents[0]),
        window_week_percent: round1(percents[1]),
        window_month_percent: round1(percents[2]),
        resets_in_5h_minutes: resets.first().copied().map(seconds_to_minutes),
        resets_in_week_minutes: resets.get(1).copied().map(seconds_to_minutes),
        resets_in_month_minutes: resets.get(2).copied().map(seconds_to_minutes),
    })
}

fn strip_html_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start + 4..].find("-->") {
            Some(end) => rest = &rest[start + 4 + end + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn parse_usage_percent_json(html: &str) -> Option<ConsoleUsageSnapshot> {
    let keys = ["rollingUsage", "weeklyUsage", "monthlyUsage"];
    let mut percents = Vec::new();
    let mut resets = Vec::new();
    for key in keys {
        let slice = find_usage_object_slice(html, key)?;
        let percent = extract_number_after(&slice, "usagePercent")
            .or_else(|| extract_number_after(&slice, "\"usagePercent\""))?;
        percents.push(percent.clamp(0.0, 100.0));
        if let Some(reset) = extract_number_after(&slice, "resetInSec")
            .or_else(|| extract_number_after(&slice, "\"resetInSec\""))
        {
            resets.push(reset as i64);
        }
    }
    Some(ConsoleUsageSnapshot {
        window_5h_percent: round1(percents[0]),
        window_week_percent: round1(percents[1]),
        window_month_percent: round1(percents[2]),
        resets_in_5h_minutes: resets.first().copied().map(seconds_to_minutes),
        resets_in_week_minutes: resets.get(1).copied().map(seconds_to_minutes),
        resets_in_month_minutes: resets.get(2).copied().map(seconds_to_minutes),
    })
}

fn find_usage_object_slice(html: &str, key: &str) -> Option<String> {
    for needle in [format!("\"{key}\""), key.to_string()] {
        let mut search = html;
        while let Some(idx) = search.find(&needle) {
            // Require a key boundary so "monthlyUsage" does not match inside longer tokens.
            if idx > 0 {
                let prev = search[..idx].chars().next_back().unwrap_or('\0');
                if prev.is_ascii_alphanumeric() || prev == '_' {
                    search = &search[idx + needle.len()..];
                    continue;
                }
            }
            let after_key = &search[idx + needle.len()..];
            let Some(brace) = after_key.find('{') else {
                search = &search[idx + needle.len()..];
                continue;
            };
            // Accept JSON (`: {`) and Solid serialized (`:$R[31]={`) assignments.
            if brace > 48 {
                search = &search[idx + needle.len()..];
                continue;
            }
            let between = after_key[..brace].trim();
            let ok = between.is_empty()
                || between == ":"
                || between == "\":"
                || between.starts_with(':');
            if !ok {
                search = &search[idx + needle.len()..];
                continue;
            }
            let object = &after_key[brace..];
            let end = object.chars().take(500).collect::<String>();
            return Some(end);
        }
    }
    None
}

fn extract_number_after(slice: &str, key: &str) -> Option<f64> {
    let idx = slice.find(key)?;
    let after = &slice[idx + key.len()..];
    let colon = after.find(':')?;
    let mut number = String::new();
    for ch in after[colon + 1..].chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' {
            number.push(ch);
        } else if !number.is_empty() {
            break;
        }
    }
    number.parse().ok()
}

fn parse_reset_seconds(html: &str) -> Vec<i64> {
    // Fallback when only human text is present; leave empty if unknown.
    let mut out = Vec::new();
    let marker = "data-slot=\"reset-time\"";
    let mut search = html;
    while let Some(idx) = search.find(marker) {
        let after = &search[idx + marker.len()..];
        let Some(gt) = after.find('>') else {
            break;
        };
        let value_part = &after[gt + 1..];
        let Some(end) = value_part.find("</span>") else {
            search = &value_part[value_part.len().min(1)..];
            continue;
        };
        let text = strip_html_comments(&value_part[..end]);
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if let Some(seconds) = parse_human_reset_to_seconds(&text) {
            out.push(seconds);
        }
        search = &value_part[end + 1..];
        if out.len() >= 3 {
            break;
        }
    }
    out
}

pub fn parse_human_reset_to_seconds(text: &str) -> Option<i64> {
    let lower = text.to_ascii_lowercase();
    let mut total = 0_i64;
    let mut matched = false;

    // English: "Resets in 2 hours 44 minutes"
    // Chinese: "重置于 2 小时 44 分钟" / "重置于 4 天 13 小时"
    let pairs: &[(&[&str], i64)] = &[
        (&["day", "days", "天"], 86_400),
        (&["hour", "hours", "小时", "小時"], 3_600),
        (&["minute", "minutes", "min", "分钟", "分鐘"], 60),
        (&["second", "seconds", "sec", "秒"], 1),
    ];
    let tokens: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || c == ',' || c == '，')
        .filter(|t| !t.is_empty())
        .collect();
    let mut i = 0;
    while i + 1 < tokens.len() {
        if let Ok(value) = tokens[i].parse::<i64>() {
            let unit = tokens[i + 1];
            let mut advanced = false;
            for (names, seconds) in pairs {
                if names.iter().any(|name| unit.starts_with(name)) {
                    total += value.saturating_mul(*seconds);
                    matched = true;
                    i += 2;
                    advanced = true;
                    break;
                }
            }
            if advanced {
                continue;
            }
        }
        i += 1;
    }
    matched.then_some(total.max(0))
}

fn seconds_to_minutes(seconds: i64) -> i64 {
    (seconds.max(0) + 59) / 60
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn read_opencode_cookies(profile_dir: &Path) -> Result<BTreeMap<String, String>> {
    chrome_cookies::read_host_cookies(
        profile_dir,
        &[
            "opencode.ai",
            ".opencode.ai",
            "auth.opencode.ai",
            "console.opencode.ai",
        ],
    )
}

mod chrome_cookies {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use rusqlite::{Connection, OpenFlags};
    use std::fs;

    pub fn read_host_cookies(
        profile_dir: &Path,
        hosts: &[&str],
    ) -> Result<BTreeMap<String, String>> {
        let cookie_db = find_cookie_db(profile_dir)
            .with_context(|| format!("cookie database missing under {}", profile_dir.display()))?;
        let temp = copy_for_read(&cookie_db)?;
        let key = os_crypt_key(profile_dir).map_err(|error| {
            anyhow::anyhow!(
                "failed to unlock browser cookie key ({error}); close the account browser and retry"
            )
        })?;
        let conn = Connection::open_with_flags(&temp, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("failed to open cookie db {}", temp.display()))?;
        let mut stmt = conn
            .prepare(
                "SELECT host_key, name, value, encrypted_value FROM cookies
                 WHERE host_key = ?1 OR host_key LIKE ?2",
            )
            .context("failed to prepare cookie query")?;

        let mut cookies = BTreeMap::new();
        let mut decrypt_failures = 0_u32;
        for host in hosts {
            let like = if host.starts_with('.') {
                format!("%{host}")
            } else {
                format!("%.{host}")
            };
            let rows = stmt
                .query_map(rusqlite::params![host, like], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                    ))
                })
                .with_context(|| format!("failed to query cookies for {host}"))?;
            for row in rows {
                let (_host_key, name, value, encrypted) = row?;
                if name.is_empty() {
                    continue;
                }
                let plain = if !value.is_empty() {
                    value
                } else {
                    match decrypt_cookie_value(&encrypted, Some(&key)) {
                        Ok(text) => text,
                        Err(_) => {
                            decrypt_failures = decrypt_failures.saturating_add(1);
                            continue;
                        }
                    }
                };
                if !plain.is_empty() {
                    cookies.insert(name, plain);
                }
            }
        }
        let _ = fs::remove_file(&temp);
        if cookies.is_empty() && decrypt_failures > 0 {
            bail!(
                "could not decrypt OpenCode session cookies ({decrypt_failures} encrypted); open the console once, then close the browser and retry"
            );
        }
        Ok(cookies)
    }

    fn find_cookie_db(profile_dir: &Path) -> Option<PathBuf> {
        let candidates = [
            profile_dir.join("Default/Network/Cookies"),
            profile_dir.join("Default/Cookies"),
            profile_dir.join("Network/Cookies"),
            profile_dir.join("Cookies"),
        ];
        candidates.into_iter().find(|path| path.is_file())
    }

    fn copy_for_read(path: &Path) -> Result<PathBuf> {
        let temp = std::env::temp_dir().join(format!(
            "ocg-cookies-{}-{}.db",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        if fs::copy(path, &temp).is_ok() {
            return Ok(temp);
        }
        // Chromium often keeps an exclusive lock; fall back to a shared read.
        let bytes = read_shared(path).with_context(|| {
            format!(
                "cookie database is locked ({}); close the account browser and retry",
                path.display()
            )
        })?;
        fs::write(&temp, bytes)
            .with_context(|| format!("failed to write temp cookie db {}", temp.display()))?;
        Ok(temp)
    }

    fn read_shared(path: &Path) -> Result<Vec<u8>> {
        #[cfg(windows)]
        {
            use std::fs::OpenOptions;
            use std::io::Read;
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_SHARE_ALL: u32 = 0x0000_0007; // READ|WRITE|DELETE
            let mut file = OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_ALL)
                .open(path)
                .with_context(|| format!("failed shared-open of {}", path.display()))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .with_context(|| format!("failed shared-read of {}", path.display()))?;
            Ok(bytes)
        }
        #[cfg(not(windows))]
        {
            fs::read(path).with_context(|| format!("failed to read {}", path.display()))
        }
    }

    fn os_crypt_key(profile_dir: &Path) -> Result<Vec<u8>> {
        let local_state = [
            profile_dir.join("Local State"),
            profile_dir
                .parent()
                .unwrap_or(profile_dir)
                .join("Local State"),
        ]
        .into_iter()
        .find(|path| path.is_file())
        .context("Chromium Local State not found")?;
        let text = fs::read_to_string(&local_state)
            .with_context(|| format!("failed to read {}", local_state.display()))?;
        let json: serde_json::Value =
            serde_json::from_str(&text).context("Local State is not valid JSON")?;
        let Some(encrypted_key) = json
            .pointer("/os_crypt/encrypted_key")
            .and_then(|value| value.as_str())
        else {
            #[cfg(not(windows))]
            {
                return Ok(linux_default_key());
            }
            #[cfg(windows)]
            {
                bail!("os_crypt.encrypted_key missing");
            }
        };
        let mut decoded = BASE64
            .decode(encrypted_key)
            .context("os_crypt.encrypted_key is not valid base64")?;
        if decoded.starts_with(b"DPAPI") {
            decoded.drain(..5);
            return dpapi_unprotect(&decoded);
        }
        if decoded.starts_with(b"v10") || decoded.starts_with(b"v11") {
            #[cfg(not(windows))]
            {
                return Ok(linux_default_key());
            }
            #[cfg(windows)]
            {
                bail!("unsupported os_crypt key format");
            }
        }
        Ok(decoded)
    }

    fn decrypt_cookie_value(encrypted: &[u8], key: Option<&[u8]>) -> Result<String> {
        if encrypted.is_empty() {
            bail!("empty encrypted cookie");
        }
        if encrypted.starts_with(b"v10") || encrypted.starts_with(b"v11") {
            let key = key.context("cookie is encrypted but OS crypt key is unavailable")?;
            #[cfg(not(windows))]
            {
                return decrypt_linux_cookie(encrypted, key);
            }
            return decrypt_aes_gcm(encrypted, key);
        }
        // Legacy DPAPI blob on older Chromium.
        if cfg!(windows) {
            let bytes = dpapi_unprotect(encrypted)?;
            return String::from_utf8(bytes).context("cookie is not utf-8");
        }
        String::from_utf8(encrypted.to_vec()).context("cookie is not utf-8")
    }

    fn decrypt_aes_gcm(encrypted: &[u8], key: &[u8]) -> Result<String> {
        // prefix(3) + nonce(12) + ciphertext + tag(16)
        if encrypted.len() < 3 + 12 + 16 {
            bail!("encrypted cookie is too short");
        }
        let nonce = &encrypted[3..15];
        let ciphertext_and_tag = &encrypted[15..];
        if ciphertext_and_tag.len() < 16 {
            bail!("encrypted cookie payload is too short");
        }
        let (ciphertext, tag) = ciphertext_and_tag.split_at(ciphertext_and_tag.len() - 16);
        let plain = aes_gcm_decrypt(key, nonce, ciphertext, tag)?;
        cookie_value_from_plain(plain)
    }

    /// Chrome 127+ prefixes decrypted cookie bytes with a 32-byte domain hash.
    fn cookie_value_from_plain(plain: Vec<u8>) -> Result<String> {
        if plain.len() > 32 {
            if let Ok(text) = std::str::from_utf8(&plain[32..]) {
                if !text.is_empty() && !text.contains('\0') {
                    return Ok(text.to_string());
                }
            }
        }
        String::from_utf8(plain).context("decrypted cookie is not utf-8")
    }

    #[cfg(test)]
    pub(super) fn cookie_value_from_plain_for_test(plain: Vec<u8>) -> Result<String> {
        cookie_value_from_plain(plain)
    }

    #[cfg(test)]
    #[cfg(not(windows))]
    pub(super) fn decrypt_cookie_value_for_test(encrypted: &[u8], key: &[u8]) -> Result<String> {
        decrypt_cookie_value(encrypted, Some(key))
    }

    #[cfg(not(windows))]
    #[cfg(test)]
    pub(super) fn linux_default_key_for_test() -> Vec<u8> {
        linux_default_key()
    }

    fn aes_gcm_decrypt(key: &[u8], nonce: &[u8], ciphertext: &[u8], tag: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::{Aead, KeyInit, Payload};
        use aes_gcm::{Aes256Gcm, Nonce};
        if key.len() != 32 {
            bail!("unexpected AES key length {}", key.len());
        }
        if nonce.len() != 12 {
            bail!("unexpected AES nonce length {}", nonce.len());
        }
        if tag.len() != 16 {
            bail!("unexpected AES tag length {}", tag.len());
        }
        let cipher = Aes256Gcm::new_from_slice(key).context("invalid AES key")?;
        let mut data = ciphertext.to_vec();
        data.extend_from_slice(tag);
        cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: &data,
                    aad: b"",
                },
            )
            .map_err(|_| anyhow::anyhow!("AES-GCM cookie decryption failed"))
    }

    #[cfg(not(windows))]
    fn linux_default_key() -> Vec<u8> {
        pbkdf2_sha1(b"peanuts", b"saltysalt", 1, 16)
    }

    #[cfg(not(windows))]
    fn linux_empty_key() -> Vec<u8> {
        pbkdf2_sha1(b"", b"saltysalt", 1, 16)
    }

    #[cfg(not(windows))]
    fn decrypt_linux_cookie(encrypted: &[u8], key: &[u8]) -> Result<String> {
        let mut keys = vec![key.to_vec()];
        if encrypted.starts_with(b"v11") {
            keys.push(linux_empty_key());
        }
        for candidate in keys {
            if let Ok(plain) = decrypt_linux_aes_cbc(encrypted, &candidate) {
                return cookie_value_from_plain(plain);
            }
        }
        bail!("Linux Chromium Cookie decryption failed")
    }

    #[cfg(not(windows))]
    fn decrypt_linux_aes_cbc(encrypted: &[u8], key: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aes::{
            Aes128,
            cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray},
        };

        if key.len() != 16 || encrypted.len() < 3 + 16 || (encrypted.len() - 3) % 16 != 0 {
            bail!("invalid Linux Chromium Cookie payload");
        }
        let cipher = Aes128::new_from_slice(key).context("invalid Linux AES key")?;
        let mut previous = [b' '; 16];
        let mut plain = Vec::with_capacity(encrypted.len() - 3);
        for chunk in encrypted[3..].chunks_exact(16) {
            let mut block = GenericArray::clone_from_slice(chunk);
            cipher.decrypt_block(&mut block);
            for (value, previous) in block.iter_mut().zip(previous) {
                *value ^= previous;
            }
            plain.extend_from_slice(&block);
            previous.copy_from_slice(chunk);
        }
        let padding = *plain
            .last()
            .context("Linux Chromium Cookie plaintext is empty")? as usize;
        if padding == 0
            || padding > 16
            || plain.len() < padding
            || !plain[plain.len() - padding..]
                .iter()
                .all(|value| *value as usize == padding)
        {
            bail!("invalid Linux Chromium Cookie padding");
        }
        plain.truncate(plain.len() - padding);
        Ok(plain)
    }

    #[cfg(not(windows))]
    fn pbkdf2_sha1(password: &[u8], salt: &[u8], iterations: u32, length: usize) -> Vec<u8> {
        use sha1::{Digest, Sha1};

        fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; 20] {
            use sha1::{Digest, Sha1};
            let mut key_block = [0_u8; 64];
            if key.len() > key_block.len() {
                key_block[..20].copy_from_slice(&Sha1::digest(key));
            } else {
                key_block[..key.len()].copy_from_slice(key);
            }
            let mut inner = Sha1::new();
            for byte in &mut key_block {
                *byte ^= 0x36;
            }
            inner.update(key_block);
            inner.update(message);
            let inner_hash = inner.finalize();
            for byte in &mut key_block {
                *byte ^= 0x36 ^ 0x5c;
            }
            let mut outer = Sha1::new();
            outer.update(key_block);
            outer.update(inner_hash);
            outer.finalize().into()
        }

        let mut output = Vec::with_capacity(length);
        let blocks = length.div_ceil(20);
        for block in 1..=blocks {
            let mut message = Vec::with_capacity(salt.len() + 4);
            message.extend_from_slice(salt);
            message.extend_from_slice(&(block as u32).to_be_bytes());
            let mut value = hmac_sha1(password, &message);
            let mut accumulated = value;
            for _ in 1..iterations {
                value = hmac_sha1(password, &value);
                for (left, right) in accumulated.iter_mut().zip(value) {
                    *left ^= right;
                }
            }
            output.extend_from_slice(&accumulated);
        }
        output.truncate(length);
        output
    }

    #[cfg(windows)]
    fn dpapi_unprotect(data: &[u8]) -> Result<Vec<u8>> {
        use std::ptr;
        use windows::Win32::Foundation::LocalFree;
        use windows::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptUnprotectData};

        unsafe {
            let input = CRYPT_INTEGER_BLOB {
                cbData: data.len() as u32,
                pbData: data.as_ptr() as *mut u8,
            };
            let mut output = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: ptr::null_mut(),
            };
            CryptUnprotectData(&input, None, None, None, None, 0, &mut output)
                .ok()
                .context("CryptUnprotectData failed")?;
            let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            if !output.pbData.is_null() {
                let _ = LocalFree(Some(windows::Win32::Foundation::HLOCAL(output.pbData as _)));
            }
            Ok(bytes)
        }
    }

    #[cfg(not(windows))]
    fn dpapi_unprotect(data: &[u8]) -> Result<Vec<u8>> {
        // Headless Chromium profiles commonly encrypt cookies with the v10/v11
        // scheme and a key from Local State; plain DPAPI blobs are Windows-only.
        if data.starts_with(b"v10") || data.starts_with(b"v11") {
            bail!("encrypted cookie requires os_crypt key");
        }
        String::from_utf8(data.to_vec())
            .map(|text| text.into_bytes())
            .context("cookie is not utf-8")
    }
}

/// Helper used by tests and callers that only need reset timestamps from now.
pub fn resets_at_from_minutes(minutes: Option<i64>) -> Option<String> {
    minutes.map(|m| (Utc::now() + ChronoDuration::minutes(m.max(0))).to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usage_value_slots_and_chinese_reset_text() {
        let html = r#"
            <div data-slot="usage-item">
              <span data-slot="usage-value">0%</span>
              <span data-slot="reset-time">重置于 2 小时 44 分钟</span>
            </div>
            <div data-slot="usage-item">
              <span data-slot="usage-value">3%</span>
              <span data-slot="reset-time">重置于 4 天 13 小时</span>
            </div>
            <div data-slot="usage-item">
              <span data-slot="usage-value">76%</span>
              <span data-slot="reset-time">重置于 14 天 20 小时</span>
            </div>
        "#;
        let snapshot = parse_console_usage_html(html).unwrap();
        assert_eq!(snapshot.window_5h_percent, 0.0);
        assert_eq!(snapshot.window_week_percent, 3.0);
        assert_eq!(snapshot.window_month_percent, 76.0);
        assert_eq!(snapshot.resets_in_5h_minutes, Some(2 * 60 + 44));
        assert_eq!(snapshot.resets_in_week_minutes, Some(4 * 24 * 60 + 13 * 60));
        assert_eq!(
            snapshot.resets_in_month_minutes,
            Some(14 * 24 * 60 + 20 * 60)
        );
    }

    #[test]
    fn parses_solid_comment_wrapped_usage_slots() {
        let html = r#"
            <span data-slot="usage-value"><!--$-->0<!--/-->%</span>
            <span data-slot="reset-time"><!--$-->重置于<!--/--> <!--$-->1 小时 31 分钟<!--/--></span>
            <span data-slot="usage-value"><!--$-->3<!--/-->%</span>
            <span data-slot="reset-time"><!--$-->重置于<!--/--> <!--$-->4 天 12 小时<!--/--></span>
            <span data-slot="usage-value"><!--$-->76<!--/-->%</span>
            <span data-slot="reset-time"><!--$-->重置于<!--/--> <!--$-->14 天 19 小时<!--/--></span>
        "#;
        let snapshot = parse_console_usage_html(html).unwrap();
        assert_eq!(snapshot.window_5h_percent, 0.0);
        assert_eq!(snapshot.window_week_percent, 3.0);
        assert_eq!(snapshot.window_month_percent, 76.0);
        assert_eq!(snapshot.resets_in_5h_minutes, Some(91));
        assert_eq!(snapshot.resets_in_week_minutes, Some(4 * 24 * 60 + 12 * 60));
    }

    #[test]
    fn parses_json_embedded_usage() {
        let html = r#"{"rollingUsage":{"usagePercent":12,"resetInSec":3600},"weeklyUsage":{"usagePercent":34,"resetInSec":7200},"monthlyUsage":{"usagePercent":56,"resetInSec":10800}}"#;
        let snapshot = parse_console_usage_html(html).unwrap();
        assert_eq!(snapshot.window_5h_percent, 12.0);
        assert_eq!(snapshot.window_week_percent, 34.0);
        assert_eq!(snapshot.window_month_percent, 56.0);
        assert_eq!(snapshot.resets_in_5h_minutes, Some(60));
        assert_eq!(snapshot.resets_in_week_minutes, Some(120));
        assert_eq!(snapshot.resets_in_month_minutes, Some(180));
    }

    #[test]
    fn parses_solid_serialized_usage_objects() {
        let html = r#"={mine:!0,useBalance:!1,rollingUsage:$R[31]={status:"ok",resetInSec:5507,usagePercent:0},weeklyUsage:$R[32]={status:"ok",resetInSec:345600,usagePercent:3},monthlyUsage:$R[33]={status:"ok",resetInSec:1209600,usagePercent:76}}"#;
        let snapshot = parse_console_usage_html(html).unwrap();
        assert_eq!(snapshot.window_5h_percent, 0.0);
        assert_eq!(snapshot.window_week_percent, 3.0);
        assert_eq!(snapshot.window_month_percent, 76.0);
        assert_eq!(snapshot.resets_in_5h_minutes, Some(92));
        assert_eq!(snapshot.resets_in_week_minutes, Some(5760));
        assert_eq!(snapshot.resets_in_month_minutes, Some(20160));
    }

    #[test]
    fn extracts_go_workspace_path() {
        let html = r#"<a href="/workspace/wrk_01ABC/go">Go</a>"#;
        assert_eq!(
            extract_go_workspace_path(html).as_deref(),
            Some("/workspace/wrk_01ABC/go")
        );
        assert_eq!(
            extract_go_workspace_path(r#"href="/workspace/wrk_01ABC""#).as_deref(),
            Some("/workspace/wrk_01ABC/go")
        );
    }

    #[test]
    fn parses_english_reset_phrase() {
        assert_eq!(
            parse_human_reset_to_seconds("Resets in 2 hours 44 minutes"),
            Some(2 * 3600 + 44 * 60)
        );
    }

    #[test]
    fn strips_chrome_domain_hash_prefix_from_cookie_plaintext() {
        let mut plain = vec![0_u8; 32];
        plain.extend_from_slice(b"session-token-value");
        assert_eq!(
            chrome_cookies::cookie_value_from_plain_for_test(plain).unwrap(),
            "session-token-value"
        );
        assert_eq!(
            chrome_cookies::cookie_value_from_plain_for_test(b"legacy-plain".to_vec()).unwrap(),
            "legacy-plain"
        );
    }

    #[test]
    fn console_redirects_reject_cross_origin_and_insecure_targets() {
        let current = Url::parse(AUTH_URL).unwrap();
        assert_eq!(
            join_console_redirect(&current, "https://console.opencode.ai/workspace/wrk/go")
                .unwrap()
                .host_str(),
            Some("console.opencode.ai")
        );
        for location in [
            "https://attacker.example/collect",
            "http://opencode.ai/insecure",
            "https://opencode.ai:444/other",
            "https://opencode.ai.example/other",
        ] {
            assert!(
                join_console_redirect(&current, location).is_err(),
                "{location}"
            );
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn decrypts_linux_basic_chromium_v10_cookie() {
        use aes_gcm::aes::{
            Aes128,
            cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray},
        };

        let key = chrome_cookies::linux_default_key_for_test();
        let mut plain = b"session-token".to_vec();
        let padding = 16 - plain.len() % 16;
        plain.extend(std::iter::repeat_n(padding as u8, padding));
        let cipher = Aes128::new_from_slice(&key).unwrap();
        let mut previous = [b' '; 16];
        let mut encrypted = b"v10".to_vec();
        for chunk in plain.chunks_exact(16) {
            let mut block = GenericArray::clone_from_slice(chunk);
            for (value, previous) in block.iter_mut().zip(previous) {
                *value ^= previous;
            }
            cipher.encrypt_block(&mut block);
            previous.copy_from_slice(&block);
            encrypted.extend_from_slice(&block);
        }
        assert_eq!(
            chrome_cookies::decrypt_cookie_value_for_test(&encrypted, &key).unwrap(),
            "session-token"
        );
    }
}
