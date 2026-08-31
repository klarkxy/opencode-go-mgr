use super::*;

#[test]
fn account_ids_are_single_safe_path_components() {
    let id = "018f2f42-4cb7-7ae8-a9a5-935aa89d499b";
    assert_eq!(validate_account_id(id).as_deref(), Ok(id));
    assert_eq!(
        validate_account_id("legacy_account-1").as_deref(),
        Ok("legacy_account-1")
    );
    assert!(validate_account_id("../profiles/other").is_err());
    assert!(validate_account_id("two words").is_err());
}

#[test]
fn target_urls_are_limited_to_signup_and_opencode_hosts() {
    for valid in [
        "https://accounts.google.com/signup",
        "https://github.com/login",
        "https://opencode.ai/zen/go",
        "https://console.opencode.ai/invite?code=test",
        "https://auth.opencode.ai/authorize",
    ] {
        assert!(validate_target_url(valid).is_ok(), "{valid}");
    }
    for invalid in [
        "http://opencode.ai/zen/go",
        "https://opencode.ai.example/zen/go",
        "https://user:pass@opencode.ai/zen/go",
        "https://example.com/",
    ] {
        assert!(validate_target_url(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn browser_args_keep_the_sandbox_and_automation_disabled() {
    let args = browser_args(
        Path::new("/profiles/018f2f42-4cb7-7ae8-a9a5-935aa89d499b"),
        "https://opencode.ai/zen/go",
        false,
    );
    assert!(args.iter().any(|arg| arg == "--no-first-run"));
    assert!(args.iter().any(|arg| arg == "--no-default-browser-check"));
    assert!(args.iter().any(|arg| arg == "--password-store=basic"));
    assert!(!args.iter().any(|arg| arg == "--no-sandbox"));
    assert!(!args.iter().any(|arg| arg.contains("remote-debugging")));
    assert!(!args.iter().any(|arg| arg == "--disable-web-security"));
}

#[test]
fn token_is_created_once_and_reused() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("runtime/control-token");
    let first = ensure_control_token(&path).expect("create token");
    let second = ensure_control_token(&path).expect("read token");
    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
    assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[cfg(unix)]
#[test]
fn profile_symlinks_are_rejected() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().join("profiles");
    fs::create_dir(&root).expect("profile root");
    let account_id = "018f2f42-4cb7-7ae8-a9a5-935aa89d499b";
    std::os::unix::fs::symlink(temp.path(), root.join(account_id)).expect("symlink");
    assert!(prepare_profile(&root, account_id).is_err());
}

#[tokio::test]
async fn protected_routes_require_the_shared_token() {
    use axum::{
        body::to_bytes,
        http::{Method, Request},
    };
    use tower::ServiceExt;

    let temp = tempfile::tempdir().expect("temp dir");
    let config = Config::for_test(temp.path(), PathBuf::from("unused"));
    let app = build_router(AppState::new(config, "a".repeat(64)));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/session")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = to_bytes(response.into_body(), 4096).await.expect("body");
    assert!(String::from_utf8_lossy(&body).contains("invalid browser worker token"));
}

#[tokio::test]
async fn authenticated_session_state_uses_the_control_contract() {
    use axum::{
        body::to_bytes,
        http::{Method, Request, header},
    };
    use tower::ServiceExt;

    let temp = tempfile::tempdir().expect("temp dir");
    let config = Config::for_test(temp.path(), PathBuf::from("unused"));
    let app = build_router(AppState::new(config, "a".repeat(64)));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/session")
                .header(header::AUTHORIZATION, format!("Bearer {}", "a".repeat(64)))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 4096).await.expect("body");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("session JSON");
    assert_eq!(value["status"], "idle");
    assert_eq!(value["active"], false);
    assert!(value["account_id"].is_null());
    assert_eq!(value["vnc_ws_url"], DEFAULT_VNC_WS_URL);
}

#[cfg(unix)]
#[tokio::test]
async fn switching_accounts_gracefully_replaces_the_only_process() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("temp dir");
    let fake_browser = temp.path().join("fake-browser");
    fs::write(
        &fake_browser,
        b"#!/bin/sh\ntrap 'exit 0' TERM INT\nwhile :; do sleep 1; done\n",
    )
    .expect("fake browser");
    fs::set_permissions(&fake_browser, fs::Permissions::from_mode(0o700))
        .expect("fake browser mode");
    let config = Config::for_test(temp.path(), fake_browser);
    let first = "018f2f42-4cb7-7ae8-a9a5-935aa89d499b";
    let second = "018f2f42-4cb7-7ae8-a9a5-935aa89d499c";
    let first_profile = prepare_profile(&config.profile_root, first).expect("first profile");
    let second_profile = prepare_profile(&config.profile_root, second).expect("second profile");
    let mut controller = BrowserController::default();

    assert!(
        !controller
            .start_or_open(&config, first, &first_profile, "https://opencode.ai/zen/go",)
            .await
            .expect("first browser")
    );
    let first_pid = controller.current.as_ref().unwrap().child.id();
    assert!(
        controller
            .start_or_open(
                &config,
                second,
                &second_profile,
                "https://opencode.ai/zen/go",
            )
            .await
            .expect("switch browser")
    );
    let current = controller.current.as_ref().expect("active browser");
    assert_eq!(current.account_id, second);
    assert_ne!(current.child.id(), first_pid);
    controller
        .stop(config.shutdown_timeout)
        .await
        .expect("stop");
    assert!(controller.current.is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn abnormal_browser_exit_reaps_owned_profile_locks_after_the_session_stops() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("temp dir");
    let fake_browser = temp.path().join("fake-browser");
    fs::write(
            &fake_browser,
            b"#!/bin/sh\nfor arg in \"$@\"; do\n  case \"$arg\" in\n    --user-data-dir=*) profile=${arg#--user-data-dir=} ;;\n  esac\ndone\ntouch \"$profile/SingletonLock\" \"$profile/SingletonSocket\" \"$profile/SingletonCookie\"\nsleep 1\nexit 1\n",
        )
        .expect("fake browser");
    fs::set_permissions(&fake_browser, fs::Permissions::from_mode(0o700))
        .expect("fake browser mode");

    let config = Config::for_test(temp.path(), fake_browser);
    let account_id = "018f2f42-4cb7-7ae8-a9a5-935aa89d499b";
    let profile = prepare_profile(&config.profile_root, account_id).expect("profile");
    let mut controller = BrowserController::default();

    controller
        .start_or_open(&config, account_id, &profile, "https://opencode.ai/zen/go")
        .await
        .expect("browser starts before its simulated crash");
    for marker in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        assert!(
            profile.join(marker).exists(),
            "{marker} must remain while running"
        );
    }

    controller.reap_exited().expect("live browser reap");
    assert!(controller.current.is_some());
    for marker in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        assert!(
            profile.join(marker).exists(),
            "{marker} must not be removed early"
        );
    }

    controller
        .current
        .as_mut()
        .expect("active browser")
        .child
        .wait()
        .await
        .expect("simulated browser exit");
    controller.reap_exited().expect("reap crashed browser");
    assert!(controller.current.is_none());
    for marker in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        assert!(
            !profile.join(marker).exists(),
            "{marker} must be removed after the browser exits"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn retired_profile_blocks_a_second_browser_until_its_process_group_exits() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("temp dir");
    let fake_browser = temp.path().join("fake-browser");
    fs::write(
            &fake_browser,
            b"#!/bin/sh\nfor arg in \"$@\"; do\n  case \"$arg\" in\n    --user-data-dir=*) profile=${arg#--user-data-dir=} ;;\n  esac\ndone\nprintf x >> \"$profile/launches\"\ntouch \"$profile/SingletonLock\" \"$profile/SingletonSocket\" \"$profile/SingletonCookie\"\nif [ ! -f \"$profile/first-launch\" ]; then\n  touch \"$profile/first-launch\"\n  sleep 1 &\n  exit 1\nfi\ntrap 'exit 0' TERM INT\nwhile :; do sleep 1; done\n",
        )
        .expect("fake browser");
    fs::set_permissions(&fake_browser, fs::Permissions::from_mode(0o700))
        .expect("fake browser mode");

    let config = Config::for_test(temp.path(), fake_browser);
    let first = "018f2f42-4cb7-7ae8-a9a5-935aa89d499b";
    let second = "018f2f42-4cb7-7ae8-a9a5-935aa89d499c";
    let first_profile = prepare_profile(&config.profile_root, first).expect("first profile");
    let second_profile = prepare_profile(&config.profile_root, second).expect("second profile");
    fs::write(second_profile.join("first-launch"), b"already started")
        .expect("make the second browser stay alive");
    let mut controller = BrowserController::default();

    assert!(
        controller
            .start_or_open(&config, first, &first_profile, "https://opencode.ai/zen/go")
            .await
            .is_err(),
        "the first browser leader exits while a descendant remains"
    );
    assert_eq!(fs::read(first_profile.join("launches")).unwrap(), b"x");
    assert!(controller.current.is_none());
    assert_eq!(controller.retired.len(), 1);

    assert!(
        controller
            .start_or_open(&config, first, &first_profile, "https://opencode.ai/zen/go")
            .await
            .is_err(),
        "the live retired group must block a second browser for the same profile"
    );
    assert_eq!(
        fs::read(first_profile.join("launches")).unwrap(),
        b"x",
        "the blocked retry must not spawn another browser"
    );
    for marker in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        assert!(
            first_profile.join(marker).exists(),
            "{marker} must remain while the retired group is alive"
        );
    }

    controller
        .start_or_open(
            &config,
            second,
            &second_profile,
            "https://opencode.ai/zen/go",
        )
        .await
        .expect("a different profile can still start");
    controller
        .stop(config.shutdown_timeout)
        .await
        .expect("stop second profile");

    tokio::time::sleep(Duration::from_millis(1_100)).await;
    controller
        .start_or_open(&config, first, &first_profile, "https://opencode.ai/zen/go")
        .await
        .expect("first profile can restart after its retired group exits");
    assert_eq!(fs::read(first_profile.join("launches")).unwrap(), b"xx");
    controller
        .stop(config.shutdown_timeout)
        .await
        .expect("stop first profile");
}

#[cfg(unix)]
#[test]
fn worker_restart_recovers_locks_owned_by_a_dead_chromium_process() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().join("profiles");
    let account_id = "018f2f42-4cb7-7ae8-a9a5-935aa89d499b";
    let profile = root.join(account_id);
    fs::create_dir_all(&profile).expect("profile");
    symlink("browser-worker-2147483647", profile.join("SingletonLock")).expect("lock marker");
    fs::write(profile.join("SingletonSocket"), b"stale socket").expect("socket marker");
    fs::write(profile.join("SingletonCookie"), b"stale cookie").expect("cookie marker");

    recover_stale_chromium_profile_locks(&root).expect("recover stale locks");
    for marker in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        assert!(
            fs::symlink_metadata(profile.join(marker)).is_err(),
            "{marker} must be removed for a dead process"
        );
    }
}

#[cfg(unix)]
#[test]
fn worker_restart_keeps_locks_owned_by_a_running_process() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().join("profiles");
    let account_id = "018f2f42-4cb7-7ae8-a9a5-935aa89d499b";
    let profile = root.join(account_id);
    fs::create_dir_all(&profile).expect("profile");
    symlink(
        format!("browser-worker-{}", std::process::id()),
        profile.join("SingletonLock"),
    )
    .expect("lock marker");
    fs::write(profile.join("SingletonSocket"), b"live socket").expect("socket marker");
    fs::write(profile.join("SingletonCookie"), b"live cookie").expect("cookie marker");

    recover_stale_chromium_profile_locks(&root).expect("inspect live locks");
    for marker in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        assert!(
            fs::symlink_metadata(profile.join(marker)).is_ok(),
            "{marker} must remain for a running process"
        );
    }
}
