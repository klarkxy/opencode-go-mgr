use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct FakeHost {
    running: AtomicBool,
    stops: AtomicUsize,
    logs: Mutex<CpaRuntimeLogTail>,
}

impl Default for FakeHost {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            stops: AtomicUsize::new(0),
            logs: Mutex::new(CpaRuntimeLogTail {
                stdout: String::new(),
                stderr: String::new(),
            }),
        }
    }
}

impl CpaRuntimeProcessHost for FakeHost {
    fn start_owned(&self, _: &CpaRuntimeProcessSpec) -> Result<(), CpaRuntimeError> {
        Ok(())
    }
    fn stop_owned(&self) -> Result<(), CpaRuntimeError> {
        self.running.store(false, Ordering::SeqCst);
        self.stops.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn owned_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    fn logs(&self) -> CpaRuntimeLogTail {
        self.logs.lock().clone()
    }
    fn add_log_secret(&self, _: &CpaRuntimeSecret) {}
}

fn fixture(stdout: &str, running: bool, deadline: Instant) -> (Arc<FakeHost>, Arc<DeviceSession>) {
    let host = Arc::new(FakeHost::default());
    host.running.store(running, Ordering::SeqCst);
    host.logs.lock().stdout = stdout.into();
    let session = Arc::new(DeviceSession {
        state: "ocg-device-test".into(),
        host: host.clone(),
        deadline,
        result: Mutex::new(DeviceResult::default()),
    });
    (host, session)
}

#[test]
fn prompt_requires_official_url_and_complete_safe_code() {
    let prompt = format!("Codex device URL: {DEVICE_URL}\r\nCodex device code: ABCD-1234\r\n");
    assert_eq!(parse_device_code(&prompt).as_deref(), Some("ABCD-1234"));
    assert_eq!(
        parse_device_code(&prompt.replace(DEVICE_URL, "https://example.com")),
        None
    );
    assert_eq!(
        parse_device_code(&prompt.replace("ABCD-1234", "<secret>")),
        None
    );
    assert_eq!(parse_device_code("Codex device code: ABCD-1234\n"), None);
    assert_eq!(
        parse_device_code(&format!(
            "Codex device URL: {DEVICE_URL}\nCodex device code: ABCD"
        )),
        None
    );
}

#[test]
fn authenticated_before_save_is_not_success_and_errors_are_sanitized() {
    let (host, session) = fixture(
        "Codex authentication successful\n",
        true,
        Instant::now() + AUTH_TIMEOUT,
    );
    assert_eq!(session.status().status, "wait");
    host.logs.lock().stderr = "access_token=do-not-expose".into();
    host.running.store(false, Ordering::SeqCst);
    let result = session.status();
    assert_eq!(result.status, "error");
    assert!(!result.error.unwrap().contains("do-not-expose"));
}

#[test]
fn saved_success_stops_helper_and_terminal_result_survives_cancel() {
    let (host, session) = fixture(
        "Authentication saved to private-path\nCodex device authentication successful!\n",
        true,
        Instant::now() + AUTH_TIMEOUT,
    );
    assert_eq!(session.status().status, "ok");
    assert!(!host.owned_running());
    assert!(!session.cancel());
    assert_eq!(session.status().status, "ok");
}

#[test]
fn timeout_and_cancel_terminate_only_device_host() {
    let (host, session) = fixture("", true, Instant::now() - Duration::from_secs(1));
    assert_eq!(session.status().status, "expired");
    assert!(!host.owned_running());
    let (host, session) = fixture("", true, Instant::now() + AUTH_TIMEOUT);
    assert!(session.cancel());
    assert!(!session.cancel());
    assert!(!host.owned_running());
    assert_eq!(session.status().status, "cancelled");
}

#[test]
fn lifecycle_and_abandoned_start_cancel_device_without_stopping_gateway() {
    let gateway = Arc::new(FakeHost::default());
    gateway.running.store(true, Ordering::SeqCst);
    let capabilities = CpaRuntimeCapabilities::new();
    capabilities.set_host(gateway.clone());
    let (helper, session) = fixture("", true, Instant::now() + AUTH_TIMEOUT);
    *capabilities.device.lock() = Some(session.clone());
    {
        let _operation = capabilities.begin_lifecycle_operation("update");
    }
    assert!(!helper.owned_running());
    assert!(gateway.owned_running());
    let (helper, session) = fixture("", true, Instant::now() + AUTH_TIMEOUT);
    drop(PendingStart(Some(session)));
    assert!(!helper.owned_running());
}

#[test]
fn dropping_session_releases_owned_helper() {
    let (host, session) = fixture("", true, Instant::now() + AUTH_TIMEOUT);
    drop(session);
    assert!(!host.owned_running());
}

/// Opt-in real CPA prompt/cancel check, isolated from the user's auth directory.
#[test]
#[ignore = "requires OCG_CPA_DEVICE_SMOKE_EXECUTABLE and network; never completes account authorization"]
fn official_cpa_device_prompt_and_cancel() {
    let executable = PathBuf::from(
        std::env::var_os("OCG_CPA_DEVICE_SMOKE_EXECUTABLE").expect("official CPA executable"),
    );
    let dir = std::env::temp_dir().join(format!("ocg-cpa-device-smoke-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("config.yaml");
    let auth = dir.join("auth");
    fs::write(
        &config_path,
        format!(
            "host: 127.0.0.1\nport: 18317\nauth-dir: '{}'\ndebug: false\nlogging-to-file: false\n",
            auth.display()
        ),
    )
    .unwrap();
    let host = host::new_device_host().unwrap();
    host.start_owned(&CpaRuntimeProcessSpec {
        codex_device_login: true,
        executable,
        config_path,
        working_dir: dir.clone(),
        management_password: CpaRuntimeSecret::new("smoke-management-secret"),
        log_secrets: vec![CpaRuntimeSecret::new("smoke-management-secret")],
    })
    .unwrap();
    let session = Arc::new(DeviceSession {
        state: "ocg-device-smoke".into(),
        host: host.clone(),
        deadline: Instant::now() + Duration::from_secs(30),
        result: Mutex::new(DeviceResult::default()),
    });
    let prompt_deadline = Instant::now() + PROMPT_TIMEOUT;
    let received = loop {
        session.refresh();
        let result = session.result.lock();
        if result.code.is_some() {
            break true;
        }
        if result.terminal.is_some() || Instant::now() >= prompt_deadline {
            break false;
        }
        drop(result);
        std::thread::sleep(Duration::from_millis(100));
    };
    session.cancel();
    assert!(!host.owned_running());
    drop(session);
    drop(host);
    fs::remove_dir_all(&dir).unwrap();
    assert!(
        received,
        "CPA did not emit a device prompt within 15 seconds; inspect network/device support separately"
    );
}
