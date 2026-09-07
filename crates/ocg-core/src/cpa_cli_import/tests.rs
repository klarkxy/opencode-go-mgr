use super::*;

fn jwt(claims: Value) -> String {
    format!(
        "e30.{}.test",
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap())
    )
}

fn codex() -> Value {
    json!({"auth_mode":"chatgpt", "tokens":{
        "access_token":jwt(json!({"exp":2000000000})),
        "id_token":jwt(json!({"email":"test@example.invalid"})),
        "refresh_token":"fake-refresh", "account_id":"fake-account"
    },"OPENAI_API_KEY":null,"unrelated_secret":"must-not-be-uploaded"})
}

#[test]
fn codex_maps_only_oauth_fields_and_has_stable_nonsecret_filename() {
    let source = codex();
    let credential = normalize(CpaOAuthProvider::Codex, &source).unwrap();
    assert_eq!(credential.payload["type"], "codex");
    assert_eq!(credential.payload["account_id"], "fake-account");
    assert_eq!(credential.payload["refresh_token"], "fake-refresh");
    assert_eq!(credential.payload["email"], "test@example.invalid");
    assert!(credential.payload.get("tokens").is_none());
    assert!(credential.payload.get("unrelated_secret").is_none());
    assert!(!credential.name.contains("fake-account"));
    assert!(!credential.name.contains("fake-refresh"));
    let mut refreshed = source;
    refreshed["tokens"]["refresh_token"] = json!("rotated-refresh");
    assert_eq!(
        credential.name,
        normalize(CpaOAuthProvider::Codex, &refreshed).unwrap().name
    );
}

#[test]
fn codex_rejects_api_keys_missing_refresh_and_malformed_identity_without_echoing_secrets() {
    assert!(
        normalize(
            CpaOAuthProvider::Codex,
            &json!({"auth_mode":"apikey","OPENAI_API_KEY":"secret"})
        )
        .is_err()
    );
    let mut source = codex();
    source["tokens"]["refresh_token"] = Value::Null;
    assert!(normalize(CpaOAuthProvider::Codex, &source).is_err());
    source = codex();
    source["tokens"]["id_token"] = json!("private-malformed-value");
    let error = normalize(CpaOAuthProvider::Codex, &source).err().unwrap();
    assert!(!error.contains("private-malformed-value"));
}

#[test]
fn claude_maps_refreshable_inference_login_and_converts_millisecond_expiry() {
    let source = json!({"claudeAiOauth": {"accessToken":"access", "refreshToken":"refresh", "expiresAt":2000000000000i64, "scopes":["user:inference","user:profile"]}, "apiKey":"not-oauth"});
    let credential = normalize(CpaOAuthProvider::Anthropic, &source).unwrap();
    assert_eq!(credential.cpa_provider, "claude");
    assert_eq!(
        credential.payload["expired"],
        timestamp(2000000000).unwrap()
    );
    assert_eq!(credential.payload["access_token"], "access");
    assert!(credential.payload.get("apiKey").is_none());
    let mut wrong_scope = source;
    wrong_scope["claudeAiOauth"]["scopes"] = json!(["user:profile"]);
    assert!(normalize(CpaOAuthProvider::Anthropic, &wrong_scope).is_err());
}

#[test]
fn kimi_maps_official_wire_expiry_and_drops_unrelated_settings() {
    let source = json!({"access_token":"kimi-access", "refresh_token":"kimi-refresh", "expires_at":2000000000.5,
        "scope":"kimi-code", "token_type":"Bearer", "expires_in":3600, "api_key":"must-not-copy"});
    let credential = normalize(CpaOAuthProvider::Kimi, &source).unwrap();
    assert_eq!(credential.cpa_provider, "kimi");
    assert_eq!(
        credential.payload["expired"],
        timestamp(2000000000).unwrap()
    );
    assert_eq!(credential.payload["scope"], "kimi-code");
    assert!(credential.payload.get("api_key").is_none());
    assert!(credential.payload.get("device_id").is_none());
}

fn grok() -> Value {
    json!({ GROK_AUTH_ENTRY: {"auth_mode":"oidc", "oidc_issuer":"https://auth.x.ai", "oidc_client_id":GROK_CLIENT_ID,
        "key":"grok-access", "refresh_token":"grok-refresh", "expires_at":"2033-05-18T03:33:20Z",
        "email":"grok@example.invalid", "principal_id":"grok-user", "base_url":"https://untrusted.invalid"},
        "other-client": {"key":"not-selected"} })
}

#[test]
fn grok_selects_only_bound_oidc_entry_and_never_copies_endpoint_overrides() {
    let credential = normalize(CpaOAuthProvider::Xai, &grok()).unwrap();
    assert_eq!(credential.cpa_provider, "xai");
    assert_eq!(credential.payload["auth_kind"], "oauth");
    assert_eq!(credential.payload["access_token"], "grok-access");
    assert_eq!(credential.payload["sub"], "grok-user");
    assert!(credential.payload.get("base_url").is_none());
    for (field, value) in [
        ("oidc_issuer", "https://other.invalid"),
        ("oidc_client_id", "wrong-client"),
        ("auth_mode", "api_key"),
    ] {
        let mut incompatible = grok();
        incompatible[GROK_AUTH_ENTRY][field] = json!(value);
        assert!(normalize(CpaOAuthProvider::Xai, &incompatible).is_err());
    }
    assert!(normalize(CpaOAuthProvider::Xai, &json!({"key":"api-key"})).is_err());
}

#[test]
fn antigravity_has_explicit_unsupported_discovery_and_cannot_be_imported() {
    let roots = roots();
    let sources = roots.discover();
    let source = sources
        .iter()
        .find(|s| s.provider == CpaOAuthProvider::Antigravity)
        .unwrap();
    assert!(!source.supported);
    assert!(!source.available);
    assert!(source.reason.is_some());
    assert!(roots.read(CpaOAuthProvider::Antigravity).is_err());
    fs::remove_dir_all(roots.home).unwrap();
}

#[test]
#[ignore = "explicit local credential read; set OCG_CPA_CLI_IMPORT_PROBE=1; never uploads credentials"]
fn local_cli_formats_parse_without_upload() {
    assert_eq!(
        std::env::var("OCG_CPA_CLI_IMPORT_PROBE").as_deref(),
        Ok("1")
    );
    let roots = CliRoots::from_env().unwrap();
    let mut checked = 0;
    for source in roots
        .discover()
        .into_iter()
        .filter(|s| s.supported && s.available)
    {
        let credential = roots
            .read(source.provider)
            .unwrap_or_else(|error| panic!("{}: {error}", source.source));
        assert!(
            credential
                .payload
                .get("refresh_token")
                .and_then(Value::as_str)
                .is_some()
        );
        println!("{}: compatible file parsed (no upload)", source.source);
        checked += 1;
    }
    assert!(
        checked > 0,
        "no local credential files were available to check"
    );
}

#[tokio::test]
#[ignore = "requires OCG_CPA_IMPORT_SMOKE_EXECUTABLE; isolated CPA with synthetic credentials only"]
async fn official_cpa_accepts_converted_cli_formats() {
    use crate::cpa::CpaClient;
    use std::process::{Child, Command, Stdio};
    struct StopChild(Child);
    impl Drop for StopChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let executable =
        std::env::var_os("OCG_CPA_IMPORT_SMOKE_EXECUTABLE").expect("official CPA executable");
    let roots = roots();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config = roots.home.join("config.yaml");
    fs::write(&config, format!("host: 127.0.0.1\nport: {port}\nauth-dir: '{}'\ndebug: false\nlogging-to-file: false\nremote-management:\n  allow-remote: false\n  disable-control-panel: true\napi-keys:\n  - import-smoke-inference\n", roots.home.join("auth").display())).unwrap();
    let mut command = Command::new(executable);
    command
        .arg("--config")
        .arg(&config)
        .current_dir(&roots.home)
        .env("MANAGEMENT_PASSWORD", "import-smoke-management")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let child = StopChild(command.spawn().unwrap());
    let client = CpaClient::new(
        &Default::default(),
        &format!("http://127.0.0.1:{port}"),
        "import-smoke-management".into(),
        "import-smoke-inference".into(),
        false,
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while client.health().await.is_err() {
        assert!(
            std::time::Instant::now() < deadline,
            "isolated CPA did not start"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let sources = [
        (CpaOAuthProvider::Codex, codex()),
        (
            CpaOAuthProvider::Anthropic,
            json!({"claudeAiOauth":{"accessToken":"fake-claude-access", "refreshToken":"fake-claude-refresh", "scopes":["user:inference"], "expiresAt":2000000000000i64}}),
        ),
        (
            CpaOAuthProvider::Kimi,
            json!({"access_token":"fake-kimi-access", "refresh_token":"fake-kimi-refresh", "scope":"kimi-code", "token_type":"Bearer", "expires_at":2000000000}),
        ),
        (CpaOAuthProvider::Xai, grok()),
    ];
    for (provider, source) in sources {
        let credential = normalize(provider, &source).unwrap();
        client.upload_cli_credential(&credential).await.unwrap();
        let (_, accounts) = client.accounts().await.unwrap();
        assert!(
            accounts
                .iter()
                .any(|account| account.name == credential.name
                    && account.provider == credential.cpa_provider),
            "converted provider missing from CPA account list"
        );
    }
    assert_eq!(client.accounts().await.unwrap().1.len(), 4);
    drop(child);
    fs::remove_dir_all(roots.home).unwrap();
}

fn roots() -> CliRoots {
    let home = std::env::temp_dir().join(format!("ocg-cli-import-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&home).unwrap();
    let home = fs::canonicalize(home).unwrap();
    CliRoots {
        codex: home.join("codex"),
        claude: home.join("claude"),
        kimi: home.join("kimi"),
        grok: home.join("grok"),
        home,
    }
}

#[test]
fn discovery_does_not_parse_credentials_and_explicit_read_preserves_source() {
    let roots = roots();
    fs::create_dir_all(&roots.codex).unwrap();
    let path = roots.codex.join("auth.json");
    fs::write(&path, "not json private-value").unwrap();
    assert!(
        roots
            .discover()
            .iter()
            .find(|s| s.provider == CpaOAuthProvider::Codex)
            .unwrap()
            .available
    );
    assert!(roots.read(CpaOAuthProvider::Codex).is_err());
    let bytes = serde_json::to_vec(&codex()).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(roots.read(CpaOAuthProvider::Codex).is_ok());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(
        !roots
            .discover()
            .iter()
            .find(|s| s.provider == CpaOAuthProvider::Anthropic)
            .unwrap()
            .available
    );
    fs::remove_dir_all(roots.home).unwrap();
}

#[test]
fn source_rejects_oversize_directory_and_relative_paths() {
    let roots = roots();
    fs::create_dir_all(&roots.codex).unwrap();
    let path = roots.codex.join("auth.json");
    fs::write(&path, vec![b'x'; MAX_FILE_BYTES as usize + 1]).unwrap();
    assert!(roots.read(CpaOAuthProvider::Codex).is_err());
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(roots.read(CpaOAuthProvider::Codex).is_err());
    assert!(checked_metadata(Path::new("auth.json")).is_err());
    fs::remove_dir_all(roots.home).unwrap();
}

#[cfg(unix)]
#[test]
fn linked_source_is_rejected() {
    let roots = roots();
    fs::create_dir_all(&roots.codex).unwrap();
    let real = roots.home.join("other.json");
    fs::write(&real, serde_json::to_vec(&codex()).unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, roots.codex.join("auth.json")).unwrap();
    assert!(roots.read(CpaOAuthProvider::Codex).is_err());
    fs::remove_dir_all(roots.home).unwrap();
}

#[cfg(any(windows, unix))]
#[test]
fn substituted_leaf_after_metadata_check_is_not_followed() {
    let roots = roots();
    fs::create_dir_all(&roots.codex).unwrap();
    let path = roots.codex.join("auth.json");
    let target = roots.home.join("unrelated.json");
    fs::write(&path, b"original").unwrap();
    fs::write(&target, b"unrelated-secret").unwrap();
    checked_metadata(&path).unwrap();
    fs::remove_file(&path).unwrap();
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&target, &path);
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&target, &path);
    if let Err(error) = linked {
        fs::remove_dir_all(roots.home).unwrap();
        #[cfg(windows)]
        if error.raw_os_error() == Some(1314) {
            eprintln!("symlink test needs Windows developer mode");
            return;
        }
        panic!("could not construct link fixture: {error}");
    }
    assert!(open_source(&path).is_err());
    fs::remove_dir_all(roots.home).unwrap();
}

#[cfg(any(windows, unix))]
#[test]
fn substituted_ancestor_after_metadata_check_is_not_followed() {
    let roots = roots();
    fs::create_dir_all(&roots.codex).unwrap();
    let path = roots.codex.join("auth.json");
    let target = roots.home.join("unrelated");
    fs::create_dir(&target).unwrap();
    fs::write(&path, b"original").unwrap();
    fs::write(target.join("auth.json"), b"unrelated-secret").unwrap();
    checked_metadata(&path).unwrap();
    fs::rename(&roots.codex, roots.home.join("original-dir")).unwrap();
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&target, &roots.codex);
    #[cfg(windows)]
    let linked = windows_junction(&target, &roots.codex);
    if let Err(error) = linked {
        fs::remove_dir_all(roots.home).unwrap();
        #[cfg(windows)]
        if error.raw_os_error() == Some(1314) {
            eprintln!("symlink test needs Windows developer mode");
            return;
        }
        panic!("could not construct link fixture: {error}");
    }
    assert!(open_source(&path).is_err());
    fs::remove_dir_all(roots.home).unwrap();
}

#[cfg(windows)]
fn windows_junction(target: &Path, link: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    // Junction creation needs no developer-mode privilege. Literal PowerShell
    // quoting keeps fixture paths as data; only fixture creation uses a shell.
    let quote = |path: &Path| {
        path.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('\'', "''")
    };
    let script = format!(
        "New-Item -ItemType Junction -Path '{}' -Target '{}' -ErrorAction Stop | Out-Null",
        quote(link),
        quote(target)
    );
    let bytes = script
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let result = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .creation_flags(0x08000000)
        .output()?;
    if result.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("junction fixture creation failed"))
    }
}
