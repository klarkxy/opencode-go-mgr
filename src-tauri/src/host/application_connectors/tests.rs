use super::*;
use crate::host::application_connector_plugins::{
    NativePluginCommand, NativePluginCommandOutput, NativePluginCommandRunner,
};
use ocg_core::application_connectors::{ApplicationConnectorCommit, ApplicationConnectorHost};
use ocg_core::crypto::StaticKeyCipher;
use ocg_core::db::Database;
use ocg_core::gateway_keys::PRIMARY_KEY_ID;
use ocg_core::state::{CoreState, CoreStateInner};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Fixture {
    root: PathBuf,
    roots: Roots,
    data_dir: PathBuf,
    cipher: StaticKeyCipher,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "ocg-application-connectors-{label}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let home = root.join("home");
        let local_app_data = home.join("AppData/Local");
        let data_dir = root.join("data");
        for directory in [
            &home,
            &local_app_data,
            &data_dir,
            &connector_root(&data_dir),
        ] {
            fs::create_dir_all(directory).unwrap();
        }
        Self {
            root,
            roots: Roots {
                home,
                local_app_data,
                hermes_home: None,
                codex_home: None,
                dsh_home: None,
                pi_agent_dir: None,
            },
            data_dir,
            cipher: StaticKeyCipher::new("connector-tests"),
        }
    }

    fn create_parent(&self, path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
    }

    fn request(
        &self,
        id: ApplicationConnectorId,
        action: ApplicationConnectorAction,
    ) -> ApplicationConnectorHostRequest {
        ApplicationConnectorHostRequest {
            operation: ApplicationConnectorHostOperation::Preview,
            id,
            action,
            key_id: Some("key-id".into()),
            secret: None,
            model_values: BTreeMap::new(),
            gateway_url: "http://127.0.0.1:9042".into(),
            data_dir: self.data_dir.clone(),
            desktop_executable: None,
            preview_fingerprint: None,
        }
    }

    fn target(&self, id: ApplicationConnectorId, target_id: &str) -> Target {
        targets(&self.roots, id)
            .unwrap()
            .into_iter()
            .find(|target| target.id == target_id)
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn json_document(target: Target, locator: &str, value: Value) -> DesiredDocument {
    DesiredDocument {
        target,
        fields: vec![DesiredField {
            locator: locator.into(),
            value: DesiredValue::Json(value),
            sensitive: false,
        }],
    }
}

fn env_document(target: Target, key: &str, value: &str, sensitive: bool) -> DesiredDocument {
    DesiredDocument {
        target,
        fields: vec![DesiredField {
            locator: key.into(),
            value: DesiredValue::Env(value.into()),
            sensitive,
        }],
    }
}

fn toml_document(target: Target, fields: &[(&str, &str, bool)]) -> DesiredDocument {
    DesiredDocument {
        target,
        fields: fields
            .iter()
            .map(|(locator, value, sensitive)| DesiredField {
                locator: (*locator).into(),
                value: DesiredValue::Toml((*value).into()),
                sensitive: *sensitive,
            })
            .collect(),
    }
}

fn yaml_document(target: Target, fields: &[(&str, &str, bool)]) -> DesiredDocument {
    DesiredDocument {
        target,
        fields: fields
            .iter()
            .map(|(locator, value, sensitive)| DesiredField {
                locator: (*locator).into(),
                value: DesiredValue::Yaml(YamlValue::String((*value).into())),
                sensitive: *sensitive,
            })
            .collect(),
    }
}

fn apply_connect(
    fixture: &Fixture,
    request: &ApplicationConnectorHostRequest,
    documents: Vec<DesiredDocument>,
) -> MutationPlan {
    let plan =
        build_connect_plan(&fixture.roots, &fixture.cipher, request, None, documents).unwrap();
    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &plan,
        None,
    )
    .unwrap();
    plan
}

fn persist_test_journal(fixture: &Fixture, plan: &MutationPlan) {
    let journal = finalize_journal(JournalV1 {
        version: JOURNAL_VERSION,
        connector_id: connector_slug(plan.id).to_string(),
        files: plan
            .files
            .iter()
            .map(|file| JournalFile {
                document_id: file.document_id.clone(),
                before_existed: file.before.existed,
                before_sha256: sha256(&file.before.bytes),
                after_existed: file.after.existed,
                after_sha256: sha256(&file.after.bytes),
                protected_preimage: fixture
                    .cipher
                    .encrypt(std::str::from_utf8(&file.before.bytes).unwrap())
                    .unwrap(),
            })
            .collect(),
        integrity_sha256: String::new(),
    })
    .unwrap();
    let path = journal_path(&fixture.data_dir, plan.id);
    atomic_write(&path, &serde_json::to_vec_pretty(&journal).unwrap(), true).unwrap();
}

struct DshHybridFakeRunner {
    manifest: PathBuf,
    env_path: PathBuf,
    poison_env_after_add: AtomicBool,
    commands: Mutex<Vec<Vec<String>>>,
}

impl NativePluginCommandRunner for DshHybridFakeRunner {
    fn run(&self, command: &NativePluginCommand) -> Result<NativePluginCommandOutput, String> {
        let args = command
            .args
            .iter()
            .map(|value| value.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        self.commands.lock().unwrap().push(args.clone());
        if args.iter().any(|value| value == "add") {
            let source = PathBuf::from(
                args.last()
                    .ok_or_else(|| "missing DSH package source".to_string())?
                    .trim_matches('"'),
            );
            fs::create_dir_all(self.manifest.parent().unwrap())
                .map_err(|error| error.to_string())?;
            fs::write(
                &self.manifest,
                serde_json::to_vec(&json!({
                    "dependencies": {
                        "ocg-manager-dsh": format!("file:{}", source.display())
                    },
                    "dsh": { "profile": { "bundles": ["ocg-manager-dsh"] } }
                }))
                .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            if self.poison_env_after_add.swap(false, Ordering::SeqCst) {
                match fs::remove_file(&self.env_path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => return Err(error.to_string()),
                }
                fs::create_dir_all(&self.env_path).map_err(|error| error.to_string())?;
            }
        } else if args.iter().any(|value| value == "remove") {
            match fs::remove_file(&self.manifest) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        } else {
            return Err("unexpected DSH package command".into());
        }
        Ok(NativePluginCommandOutput {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

fn dsh_hybrid_state(
    fixture: &mut Fixture,
    poison_env_after_add: bool,
) -> (CoreState, Arc<DshHybridFakeRunner>) {
    let dsh_home = fixture.roots.home.join("fresh-dsh-home");
    fixture.roots.dsh_home = Some(dsh_home.clone());
    let executable = fixture.root.join("bin/dsh.exe");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, []).unwrap();
    let manifest = dsh_home.join("profiles/web/package.json");
    let runner = Arc::new(DshHybridFakeRunner {
        manifest: manifest.clone(),
        env_path: dsh_home.join(".env"),
        poison_env_after_add: AtomicBool::new(poison_env_after_add),
        commands: Mutex::new(Vec::new()),
    });
    let native_plugins = Arc::new(NativePluginHost::new(
        NativePluginRoots {
            data_dir: fixture.data_dir.clone(),
            pi_settings: fixture.roots.home.join("pi/settings.json"),
            dsh_web_manifest: manifest,
        },
        NATIVE_PLUGIN_TEMPLATES,
        runner.clone(),
        None,
        Some(executable),
    ));
    let cipher: Arc<dyn KeyCipher + Send + Sync> =
        Arc::new(StaticKeyCipher::new("dsh-hybrid-core"));
    let state = Arc::new(
        CoreStateInner::new(
            Database::open(fixture.data_dir.clone()).unwrap(),
            fixture.data_dir.clone(),
            cipher.clone(),
        )
        .unwrap(),
    );
    let roots = fixture.roots.clone();
    let host_plugins = native_plugins.clone();
    let host: ApplicationConnectorHost = Arc::new(move |request| {
        execute_with_native_plugins(&roots, cipher.as_ref(), request, host_plugins.as_ref())
    });
    state.set_application_connector_host(host, "ocg-manager-test.exe".into());
    (state, runner)
}

fn dsh_model_values() -> BTreeMap<String, String> {
    BTreeMap::from([("models".into(), "test-model".into())])
}

#[test]
fn json_existing_baseline_and_unrelated_edits_survive_restore() {
    let fixture = Fixture::new("json-restore");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    fixture.create_parent(&target.path);
    fs::write(
        &target.path,
        br#"{"provider":{"ocg":{"original":true}},"unrelated":"keep"}"#,
    )
    .unwrap();
    apply_connect(
        &fixture,
        &request,
        vec![json_document(
            target.clone(),
            "/provider/ocg",
            json!({"managed":true}),
        )],
    );
    let mut connected: Value = serde_json::from_slice(&fs::read(&target.path).unwrap()).unwrap();
    connected
        .as_object_mut()
        .unwrap()
        .insert("later".into(), json!("preserve"));
    fs::write(&target.path, serialize_json(&connected).unwrap()).unwrap();

    let state = load_state(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::OpenCode,
    )
    .unwrap()
    .unwrap();
    let restore_request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Restore,
    );
    let restore =
        build_restore_plan(&fixture.roots, &fixture.cipher, &restore_request, &state).unwrap();
    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &restore,
        None,
    )
    .unwrap();
    let restored: Value = serde_json::from_slice(&fs::read(&target.path).unwrap()).unwrap();
    assert_eq!(restored["provider"]["ocg"], json!({"original":true}));
    assert_eq!(restored["unrelated"], "keep");
    assert_eq!(restored["later"], "preserve");
    assert!(!state_path(&fixture.data_dir, ApplicationConnectorId::OpenCode).exists());
}

#[test]
fn restore_removes_json_file_that_was_originally_absent() {
    let fixture = Fixture::new("json-restore-absent");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    fixture.create_parent(&target.path);
    apply_connect(
        &fixture,
        &request,
        vec![json_document(target.clone(), "/model", json!("ocg/model"))],
    );
    let state = load_state(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::OpenCode,
    )
    .unwrap()
    .unwrap();
    let restore_request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Restore,
    );
    let restore =
        build_restore_plan(&fixture.roots, &fixture.cipher, &restore_request, &state).unwrap();

    assert!(!restore.files[0].after.existed);
    assert!(restore.files[0].after.bytes.is_empty());
    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &restore,
        None,
    )
    .unwrap();

    assert!(!target.path.exists());
    assert!(!state_path(&fixture.data_dir, ApplicationConnectorId::OpenCode).exists());
}

#[test]
fn reconnect_retains_first_baseline_and_updates_applied_value() {
    let fixture = Fixture::new("reconnect-baseline");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    fixture.create_parent(&target.path);
    fs::write(&target.path, br#"{"model":"user/original"}"#).unwrap();
    apply_connect(
        &fixture,
        &request,
        vec![json_document(target.clone(), "/model", json!("ocg/first"))],
    );
    let first = load_state(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::OpenCode,
    )
    .unwrap()
    .unwrap();
    let first_original = first.documents[0].fields[0].original.clone();
    let reconnect = build_connect_plan(
        &fixture.roots,
        &fixture.cipher,
        &request,
        Some(&first),
        vec![json_document(target, "/model", json!("ocg/second"))],
    )
    .unwrap();
    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &reconnect,
        None,
    )
    .unwrap();
    let second = load_state(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::OpenCode,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        load_original(&fixture.cipher, &first_original).unwrap(),
        load_original(&fixture.cipher, &second.documents[0].fields[0].original).unwrap()
    );
    assert!(applied_matches(
        &second.documents[0].fields[0].applied,
        "\"ocg/second\""
    ));
}

#[test]
fn owned_drift_is_a_conflict() {
    let fixture = Fixture::new("owned-drift");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    fixture.create_parent(&target.path);
    fs::write(&target.path, b"{}\n").unwrap();
    apply_connect(
        &fixture,
        &request,
        vec![json_document(target.clone(), "/model", json!("ocg/first"))],
    );
    fs::write(&target.path, br#"{"model":"someone/else"}"#).unwrap();
    let state = load_state(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::OpenCode,
    )
    .unwrap()
    .unwrap();
    assert!(
        !state_matches(
            &fixture.roots,
            &fixture.cipher,
            ApplicationConnectorId::OpenCode,
            &state
        )
        .unwrap()
    );
    assert_eq!(
        build_connect_plan(
            &fixture.roots,
            &fixture.cipher,
            &request,
            Some(&state),
            vec![json_document(target, "/model", json!("ocg/second"))],
        )
        .err()
        .expect("owned drift must fail")
        .kind(),
        ApplicationConnectorErrorKind::Conflict
    );
}

#[test]
fn sensitive_env_baselines_and_previews_are_redacted() {
    let fixture = Fixture::new("secret-redaction");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-env");
    fixture.create_parent(&target.path);
    fs::write(&target.path, "# keep\nOCG_API_KEY=old-secret\nOTHER=yes\n").unwrap();
    let plan = build_connect_plan(
        &fixture.roots,
        &fixture.cipher,
        &request,
        None,
        vec![env_document(target, "OCG_API_KEY", "new-secret", true)],
    )
    .unwrap();
    assert_eq!(plan.changes[0].before.as_deref(), Some(REDACTED));
    assert_eq!(plan.changes[0].after.as_deref(), Some(REDACTED));
    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &plan,
        None,
    )
    .unwrap();
    let sidecar = fs::read_to_string(state_path(
        &fixture.data_dir,
        ApplicationConnectorId::OpenCode,
    ))
    .unwrap();
    assert!(!sidecar.contains("old-secret"));
    assert!(!sidecar.contains("new-secret"));
    assert!(sidecar.contains("protected"));
    assert!(sidecar.contains("digest"));
}

#[test]
fn env_patch_preserves_comments_order_and_unrelated_lines() {
    let image = FileImage {
        existed: true,
        bytes: b"# first\r\nA=one\r\n\r\nTARGET=old\r\n# last\r\n".to_vec(),
    };
    let cipher = StaticKeyCipher::new("env-test");
    let (bytes, state, _) = patch_env_connect(
        &cipher,
        &image,
        None,
        &[DesiredField {
            locator: "TARGET".into(),
            value: DesiredValue::Env("new value".into()),
            sensitive: false,
        }],
        ".env",
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(bytes.clone()).unwrap(),
        "# first\r\nA=one\r\n\r\nTARGET=\"new value\"\r\n# last\r\n"
    );
    let document = DocumentState {
        document_id: "env".into(),
        format: DocumentFormat::Env,
        originally_existed: true,
        fields: state,
    };
    let (restored, _, _) = patch_env_restore(
        &cipher,
        &FileImage {
            existed: true,
            bytes,
        },
        &document,
        ".env",
    )
    .unwrap();
    assert_eq!(restored, image.bytes);
}

#[test]
fn second_file_failure_compensates_exact_first_file_preimage() {
    let fixture = Fixture::new("transaction-compensation");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let json_target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    let env_target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-env");
    fixture.create_parent(&json_target.path);
    fs::write(&json_target.path, b"{\"unrelated\":1}\n").unwrap();
    fs::write(&env_target.path, b"# untouched\n").unwrap();
    let json_before = fs::read(&json_target.path).unwrap();
    let env_before = fs::read(&env_target.path).unwrap();
    let plan = build_connect_plan(
        &fixture.roots,
        &fixture.cipher,
        &request,
        None,
        vec![
            json_document(json_target.clone(), "/model", json!("ocg/model")),
            env_document(env_target.clone(), "OCG_API_KEY", "secret", true),
        ],
    )
    .unwrap();
    assert!(
        apply_transaction(
            &fixture.roots,
            &fixture.data_dir,
            &fixture.cipher,
            &plan,
            Some(1),
        )
        .is_err()
    );
    assert_eq!(fs::read(&json_target.path).unwrap(), json_before);
    assert_eq!(fs::read(&env_target.path).unwrap(), env_before);
    assert!(!journal_path(&fixture.data_dir, ApplicationConnectorId::OpenCode).exists());
    assert!(!state_path(&fixture.data_dir, ApplicationConnectorId::OpenCode).exists());
}

#[test]
fn post_journal_preflight_rejects_drift_without_writing_any_target() {
    let fixture = Fixture::new("post-journal-preflight");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let json_target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    let env_target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-env");
    fixture.create_parent(&json_target.path);
    fs::write(&json_target.path, b"{\"unrelated\":1}\n").unwrap();
    fs::write(&env_target.path, b"# untouched\n").unwrap();
    let env_before = fs::read(&env_target.path).unwrap();
    let plan = build_connect_plan(
        &fixture.roots,
        &fixture.cipher,
        &request,
        None,
        vec![
            json_document(json_target.clone(), "/model", json!("ocg/model")),
            env_document(env_target.clone(), "OCG_API_KEY", "secret", true),
        ],
    )
    .unwrap();
    let external_edit = b"{\"external\":true}\n".to_vec();

    let error = apply_transaction_after_journal(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &plan,
        None,
        || {
            fs::write(&json_target.path, &external_edit).unwrap();
            Ok(())
        },
    )
    .unwrap_err();

    assert_eq!(error.kind(), ApplicationConnectorErrorKind::Conflict);
    assert_eq!(fs::read(&json_target.path).unwrap(), external_edit);
    assert_eq!(fs::read(&env_target.path).unwrap(), env_before);
    assert!(!state_path(&fixture.data_dir, ApplicationConnectorId::OpenCode).exists());
    assert!(!journal_path(&fixture.data_dir, ApplicationConnectorId::OpenCode).exists());
}

#[test]
fn recovery_restores_after_images_and_accepts_before_images() {
    let fixture = Fixture::new("journal-recovery");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let json_target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    let env_target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-env");
    fixture.create_parent(&json_target.path);
    fs::write(&json_target.path, b"{\"unrelated\":1}\n").unwrap();
    fs::write(&env_target.path, b"# untouched\n").unwrap();
    let plan = build_connect_plan(
        &fixture.roots,
        &fixture.cipher,
        &request,
        None,
        vec![
            json_document(json_target, "/model", json!("ocg/model")),
            env_document(env_target, "OCG_API_KEY", "secret", true),
        ],
    )
    .unwrap();
    persist_test_journal(&fixture, &plan);

    write_exact_image(
        &plan.files[0].path,
        &plan.files[0].after,
        plan.files[0].format,
        document_requires_private(plan.id, &plan.files[0].document_id),
    )
    .unwrap();
    // Every other target is deliberately still its before-image. Recovery
    // must treat those as already restored, not as a reason to fail.
    recover_journal(&fixture.roots, &fixture.data_dir, &fixture.cipher, plan.id).unwrap();

    for file in &plan.files {
        assert_eq!(read_file_image(&file.path).unwrap(), file.before);
    }
    assert!(!journal_path(&fixture.data_dir, plan.id).exists());
}

#[test]
fn recovery_external_drift_writes_nothing_and_reports_partial() {
    let fixture = Fixture::new("journal-external-drift");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let json_target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    let env_target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-env");
    fixture.create_parent(&json_target.path);
    fs::write(&json_target.path, b"{\"unrelated\":1}\n").unwrap();
    fs::write(&env_target.path, b"# untouched\n").unwrap();
    let plan = build_connect_plan(
        &fixture.roots,
        &fixture.cipher,
        &request,
        None,
        vec![
            json_document(json_target, "/model", json!("ocg/model")),
            env_document(env_target, "OCG_API_KEY", "secret", true),
        ],
    )
    .unwrap();
    persist_test_journal(&fixture, &plan);
    write_exact_image(
        &plan.files[0].path,
        &plan.files[0].after,
        plan.files[0].format,
        document_requires_private(plan.id, &plan.files[0].document_id),
    )
    .unwrap();
    let first_after = fs::read(&plan.files[0].path).unwrap();
    let external_edit = b"# external edit\n".to_vec();
    fs::write(&plan.files[1].path, &external_edit).unwrap();

    let error =
        recover_journal(&fixture.roots, &fixture.data_dir, &fixture.cipher, plan.id).unwrap_err();

    assert_eq!(error.kind(), ApplicationConnectorErrorKind::Conflict);
    assert_eq!(fs::read(&plan.files[0].path).unwrap(), first_after);
    assert_eq!(fs::read(&plan.files[1].path).unwrap(), external_edit);
    assert!(journal_path(&fixture.data_dir, plan.id).exists());
    assert_eq!(
        inspect(&fixture.roots, &fixture.data_dir, &fixture.cipher, plan.id,).status,
        ApplicationConnectorStatus::Partial
    );
    assert_eq!(fs::read(&plan.files[0].path).unwrap(), first_after);
    assert_eq!(fs::read(&plan.files[1].path).unwrap(), external_edit);
    assert!(journal_path(&fixture.data_dir, plan.id).exists());
}

#[cfg(unix)]
#[test]
fn unix_sensitive_target_is_tightened_and_nonsensitive_mode_is_preserved() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("private-permissions");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let json_target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    let env_target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-env");
    fixture.create_parent(&json_target.path);
    fs::write(&json_target.path, b"{}\n").unwrap();
    fs::write(&env_target.path, b"# existing\n").unwrap();
    fs::set_permissions(&json_target.path, fs::Permissions::from_mode(0o640)).unwrap();
    fs::set_permissions(&env_target.path, fs::Permissions::from_mode(0o644)).unwrap();
    let plan = build_connect_plan(
        &fixture.roots,
        &fixture.cipher,
        &request,
        None,
        vec![
            json_document(json_target.clone(), "/model", json!("ocg/model")),
            env_document(env_target.clone(), "OCG_API_KEY", "secret", true),
        ],
    )
    .unwrap();

    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &plan,
        None,
    )
    .unwrap();

    assert_eq!(
        fs::metadata(&json_target.path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(
        fs::metadata(&env_target.path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn stale_fingerprint_changes_on_unrelated_file_edit() {
    let fixture = Fixture::new("stale-fingerprint");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    fixture.create_parent(&target.path);
    fs::write(&target.path, b"{\"unrelated\":1}\n").unwrap();
    let desired = || vec![json_document(target.clone(), "/model", json!("ocg/model"))];
    let first =
        build_connect_plan(&fixture.roots, &fixture.cipher, &request, None, desired()).unwrap();
    let first_fingerprint = plan_fingerprint(&request, &first);
    fs::write(&target.path, b"{\"unrelated\":2}\n").unwrap();
    let second =
        build_connect_plan(&fixture.roots, &fixture.cipher, &request, None, desired()).unwrap();
    assert_ne!(first_fingerprint, plan_fingerprint(&request, &second));
}

#[test]
fn commented_json_is_manual_only_until_the_user_repairs_it() {
    let fixture = Fixture::new("manual-gates");
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    fixture.create_parent(&target.path);
    fs::write(&target.path, b"{\n // comment\n provider: {}\n}\n").unwrap();
    let inspection = inspect(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::OpenCode,
    );
    assert_eq!(inspection.status, ApplicationConnectorStatus::ManualOnly);
    assert!(!inspection.automatic);
}

#[test]
fn repeated_connect_is_a_no_op() {
    let fixture = Fixture::new("no-op");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    fixture.create_parent(&target.path);
    fs::write(&target.path, b"{}\n").unwrap();
    let desired = || vec![json_document(target.clone(), "/model", json!("ocg/model"))];
    apply_connect(&fixture, &request, desired());
    let state = load_state(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::OpenCode,
    )
    .unwrap()
    .unwrap();
    let repeated = build_connect_plan(
        &fixture.roots,
        &fixture.cipher,
        &request,
        Some(&state),
        desired(),
    )
    .unwrap();
    assert!(!repeated.changed());
    assert!(repeated.changes.is_empty());
}

#[test]
fn missing_managed_file_is_never_reported_connected() {
    let fixture = Fixture::new("missing-managed");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    fixture.create_parent(&target.path);
    fs::write(&target.path, b"{}\n").unwrap();
    apply_connect(
        &fixture,
        &request,
        vec![json_document(target.clone(), "/model", json!("ocg/model"))],
    );
    fs::remove_file(&target.path).unwrap();
    let inspection = inspect(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::OpenCode,
    );
    assert_eq!(inspection.status, ApplicationConnectorStatus::Conflict);
}

#[test]
fn linked_target_is_rejected_when_platform_allows_it() {
    let fixture = Fixture::new("linked-target");
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    fixture.create_parent(&target.path);
    let outside = fixture.root.join("outside.json");
    fs::write(&outside, b"{}\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &target.path).unwrap();
    #[cfg(windows)]
    if std::os::windows::fs::symlink_file(&outside, &target.path).is_err() {
        return;
    }
    let error = read_target(&fixture.roots, ApplicationConnectorId::OpenCode, &target).unwrap_err();
    assert_eq!(error.kind(), ApplicationConnectorErrorKind::Precondition);
}

#[test]
fn corrupt_sidecar_and_journal_fail_closed() {
    let fixture = Fixture::new("corrupt-state");
    let request = fixture.request(
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::OpenCode, "opencode-settings");
    fixture.create_parent(&target.path);
    fs::write(&target.path, b"{}\n").unwrap();
    apply_connect(
        &fixture,
        &request,
        vec![json_document(target, "/model", json!("ocg/model"))],
    );
    let sidecar_path = state_path(&fixture.data_dir, ApplicationConnectorId::OpenCode);
    let sidecar = fs::read_to_string(&sidecar_path)
        .unwrap()
        .replace("ocg/model", "ocg/tampered");
    fs::write(&sidecar_path, sidecar).unwrap();
    let inspection = inspect(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::OpenCode,
    );
    assert_eq!(inspection.status, ApplicationConnectorStatus::Conflict);

    fs::write(
        journal_path(&fixture.data_dir, ApplicationConnectorId::OpenCode),
        b"{}",
    )
    .unwrap();
    let inspection = inspect(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::OpenCode,
    );
    assert_eq!(inspection.status, ApplicationConnectorStatus::Partial);
}

#[test]
fn codex_toml_preserves_unrelated_comments_redacts_secret_and_restores() {
    let fixture = Fixture::new("codex-toml");
    let request = fixture.request(
        ApplicationConnectorId::Codex,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::Codex, "codex-config");
    fixture.create_parent(&target.path);
    let original = b"# retain this comment\nunrelated = true\n\n[ui]\ncolor = \"violet\"\n";
    fs::write(&target.path, original).unwrap();
    let desired = vec![toml_document(
        target.clone(),
        &[
            ("model", "ocg-model", false),
            ("model_provider", "ocg_manager", false),
            ("model_providers.ocg_manager.name", "OCG Manager", false),
            (
                "model_providers.ocg_manager.base_url",
                "http://127.0.0.1:9042/v1",
                false,
            ),
            ("model_providers.ocg_manager.wire_api", "responses", false),
            (
                "model_providers.ocg_manager.experimental_bearer_token",
                "codex-secret",
                true,
            ),
        ],
    )];
    let plan =
        build_connect_plan(&fixture.roots, &fixture.cipher, &request, None, desired).unwrap();
    assert!(
        plan.changes
            .iter()
            .any(|change| change.sensitive && change.after.as_deref() == Some(REDACTED))
    );
    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &plan,
        None,
    )
    .unwrap();
    let connected = fs::read_to_string(&target.path).unwrap();
    assert!(connected.contains("# retain this comment"));
    assert!(connected.contains("unrelated = true"));
    assert!(connected.contains("experimental_bearer_token = \"codex-secret\""));
    assert!(
        !targets(&fixture.roots, ApplicationConnectorId::Codex)
            .unwrap()
            .iter()
            .any(|candidate| candidate.path.ends_with("auth.json"))
    );
    let sidecar =
        fs::read_to_string(state_path(&fixture.data_dir, ApplicationConnectorId::Codex)).unwrap();
    assert!(!sidecar.contains("codex-secret"));
    let state = load_state(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::Codex,
    )
    .unwrap()
    .unwrap();
    let restore = build_restore_plan(&fixture.roots, &fixture.cipher, &request, &state).unwrap();
    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &restore,
        None,
    )
    .unwrap();
    assert_eq!(fs::read(&target.path).unwrap(), original);
}

#[test]
#[cfg(windows)]
fn windows_private_write_replaces_a_world_readable_acl() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SetFileSecurityW,
    };

    let fixture = Fixture::new("windows-private-acl");
    let path = fixture.root.join("private-config.toml");
    fs::write(&path, b"before").unwrap();
    let sddl: Vec<u16> = "D:(A;;GR;;;WD)".encode_utf16().chain(Some(0)).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let parsed = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(parsed, 0, "world-readable test DACL should parse");
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let applied = unsafe { SetFileSecurityW(wide.as_ptr(), DACL_SECURITY_INFORMATION, descriptor) };
    unsafe {
        LocalFree(descriptor.cast());
    }
    assert_ne!(applied, 0, "world-readable test DACL should apply");

    atomic_write(&path, b"after-secret", true).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"after-secret");
    // The helper performs a full two-ACE/protected-DACL read-back check;
    // calling it again proves ReplaceFileW did not restore the broad ACL.
    set_private_permissions(&path).unwrap();
}

#[test]
#[cfg(windows)]
fn hermes_root_prefers_local_app_data_and_refuses_ambiguity() {
    let mut fixture = Fixture::new("hermes-roots");
    let default = targets(&fixture.roots, ApplicationConnectorId::Hermes).unwrap();
    assert_eq!(
        default[0].path,
        fixture.roots.local_app_data.join("hermes/config.yaml")
    );
    fs::create_dir_all(fixture.roots.home.join(".hermes")).unwrap();
    fs::create_dir_all(fixture.roots.local_app_data.join("hermes")).unwrap();
    assert_eq!(
        targets(&fixture.roots, ApplicationConnectorId::Hermes)
            .unwrap_err()
            .kind(),
        ApplicationConnectorErrorKind::Precondition
    );
    let explicit = fixture.root.join("custom-hermes-home");
    fixture.roots.hermes_home = Some(explicit.clone());
    assert_eq!(
        targets(&fixture.roots, ApplicationConnectorId::Hermes).unwrap()[0].path,
        explicit.join("config.yaml")
    );
}

#[test]
#[cfg(not(windows))]
fn hermes_root_uses_legacy_home_outside_windows() {
    let mut fixture = Fixture::new("hermes-unix-root");
    assert_eq!(
        targets(&fixture.roots, ApplicationConnectorId::Hermes).unwrap()[0].path,
        fixture.roots.home.join(".hermes/config.yaml")
    );
    let explicit = fixture.root.join("custom-hermes-home");
    fixture.roots.hermes_home = Some(explicit.clone());
    assert_eq!(
        targets(&fixture.roots, ApplicationConnectorId::Hermes).unwrap()[0].path,
        explicit.join("config.yaml")
    );
}

#[test]
fn hermes_yaml_preserves_unknown_values_env_and_restores() {
    let fixture = Fixture::new("hermes-yaml");
    let request = fixture.request(
        ApplicationConnectorId::Hermes,
        ApplicationConnectorAction::Connect,
    );
    let config = fixture.target(ApplicationConnectorId::Hermes, "hermes-config");
    let env = fixture.target(ApplicationConnectorId::Hermes, "hermes-env");
    fixture.create_parent(&config.path);
    let original_config = "# keep this unrelated comment\ndisplay:\n  skin: aurora\nmodel:\n  default: old-model\n  provider: old-provider\n  base_url: https://example.invalid/v1?x=one&y=two\n  api_key: old-key\n";
    let original_env = "# preserve\nOTHER=keep\n";
    fs::write(&config.path, original_config).unwrap();
    fs::write(&env.path, original_env).unwrap();
    let plan = build_connect_plan(
        &fixture.roots,
        &fixture.cipher,
        &request,
        None,
        vec![
            yaml_document(
                config.clone(),
                &[
                    ("model.default", "ocg-model", false),
                    ("model.provider", "custom", false),
                    ("model.base_url", "http://127.0.0.1:9042/v1", false),
                    ("model.api_key", "${OCG_MANAGER_API_KEY}", false),
                ],
            ),
            env_document(env.clone(), "OCG_MANAGER_API_KEY", "hermes-secret", true),
        ],
    )
    .unwrap();
    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &plan,
        None,
    )
    .unwrap();
    let connected: YamlValue =
        serde_yaml::from_str(&fs::read_to_string(&config.path).unwrap()).unwrap();
    assert!(
        fs::read_to_string(&config.path)
            .unwrap()
            .starts_with("# keep this unrelated comment\n")
    );
    assert_eq!(
        yaml_value_at(&connected, "display.skin").unwrap(),
        Some(&YamlValue::String("aurora".into()))
    );
    assert_eq!(
        yaml_value_at(&connected, "model.api_key").unwrap(),
        Some(&YamlValue::String("${OCG_MANAGER_API_KEY}".into()))
    );
    assert!(
        fs::read_to_string(&env.path)
            .unwrap()
            .contains("OCG_MANAGER_API_KEY=\"hermes-secret\"")
    );
    let state = load_state(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        ApplicationConnectorId::Hermes,
    )
    .unwrap()
    .unwrap();
    let restore = build_restore_plan(&fixture.roots, &fixture.cipher, &request, &state).unwrap();
    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &restore,
        None,
    )
    .unwrap();
    let restored: YamlValue =
        serde_yaml::from_str(&fs::read_to_string(&config.path).unwrap()).unwrap();
    assert_eq!(
        yaml_value_at(&restored, "display.skin").unwrap(),
        Some(&YamlValue::String("aurora".into()))
    );
    assert_eq!(
        yaml_value_at(&restored, "model.default").unwrap(),
        Some(&YamlValue::String("old-model".into()))
    );
    assert_eq!(fs::read_to_string(&env.path).unwrap(), original_env);
    assert!(
        fs::read_to_string(&config.path)
            .unwrap()
            .starts_with("# keep this unrelated comment\n")
    );
}

#[test]
fn hermes_yaml_rejects_duplicates_and_anchors_without_rejecting_plain_urls() {
    let fixture = Fixture::new("hermes-yaml-strict");
    for invalid_yaml in [
        "model: &base\n  default: x\ncopy: *base\n",
        "model: one\nmodel: two\n",
    ] {
        let image = FileImage {
            existed: true,
            bytes: invalid_yaml.as_bytes().to_vec(),
        };
        assert_eq!(
            parse_yaml_image(&image).unwrap_err().kind(),
            ApplicationConnectorErrorKind::Precondition
        );
    }
    let plain_url = FileImage {
            existed: true,
            bytes: b"model:\n  base_url: https://example.invalid/v1?x=one&y=two\n  literal: '*not-an-alias'\n"
                .to_vec(),
        };
    assert!(parse_yaml_image(&plain_url).is_ok());
    assert!(targets(&fixture.roots, ApplicationConnectorId::Hermes).is_ok());

    let commented_model = FileImage {
        existed: true,
        bytes: b"display:\n  skin: keep\nmodel:\n  # must not be lost\n  default: old\n".to_vec(),
    };
    let parsed = parse_yaml_image(&commented_model).unwrap();
    assert_eq!(
        render_yaml_model_section(&commented_model, &parsed)
            .unwrap_err()
            .kind(),
        ApplicationConnectorErrorKind::Precondition
    );
}

#[test]
fn dsh_env_owns_one_redacted_assignment_and_restores_exact_bytes() {
    let fixture = Fixture::new("dsh-env-roundtrip");
    let request = fixture.request(
        ApplicationConnectorId::Dsh,
        ApplicationConnectorAction::Connect,
    );
    let target = fixture.target(ApplicationConnectorId::Dsh, "dsh-env");
    fixture.create_parent(&target.path);
    let original = b"# keep\r\nOTHER=yes\r\nOCG_MANAGER_API_KEY=old-secret\r\n";
    fs::write(&target.path, original).unwrap();

    let plan = apply_connect(
        &fixture,
        &request,
        vec![env_document(
            target.clone(),
            "OCG_MANAGER_API_KEY",
            "new-secret",
            true,
        )],
    );
    assert_eq!(plan.changes.len(), 1);
    assert_eq!(plan.changes[0].before.as_deref(), Some(REDACTED));
    assert_eq!(plan.changes[0].after.as_deref(), Some(REDACTED));
    let state =
        fs::read_to_string(state_path(&fixture.data_dir, ApplicationConnectorId::Dsh)).unwrap();
    assert!(!state.contains("old-secret"));
    assert!(!state.contains("new-secret"));
    assert_eq!(
        inspect(
            &fixture.roots,
            &fixture.data_dir,
            &fixture.cipher,
            ApplicationConnectorId::Dsh,
        )
        .status,
        ApplicationConnectorStatus::Connected
    );

    let restore = fixture.request(
        ApplicationConnectorId::Dsh,
        ApplicationConnectorAction::Restore,
    );
    let restore_plan = build_plan(&fixture.roots, &fixture.cipher, &restore).unwrap();
    apply_transaction(
        &fixture.roots,
        &fixture.data_dir,
        &fixture.cipher,
        &restore_plan,
        None,
    )
    .unwrap();
    assert_eq!(fs::read(&target.path).unwrap(), original);
}

#[test]
fn dsh_combined_status_requires_both_plugin_and_credential() {
    assert_eq!(
        combined_dsh_status(
            ApplicationConnectorStatus::Connected,
            ApplicationConnectorStatus::Connected,
        ),
        ApplicationConnectorStatus::Connected
    );
    assert_eq!(
        combined_dsh_status(
            ApplicationConnectorStatus::Connected,
            ApplicationConnectorStatus::Ready,
        ),
        ApplicationConnectorStatus::Partial
    );
    assert_eq!(
        combined_dsh_status(
            ApplicationConnectorStatus::Ready,
            ApplicationConnectorStatus::Conflict,
        ),
        ApplicationConnectorStatus::Conflict
    );
    assert_ne!(
        combined_dsh_fingerprint("managed-a", "native"),
        combined_dsh_fingerprint("managed-b", "native")
    );
}

#[test]
fn dsh_combined_first_connect_bootstraps_missing_home_and_restores() {
    let mut fixture = Fixture::new("dsh-hybrid-first-connect");
    let (state, runner) = dsh_hybrid_state(&mut fixture, false);
    let dsh_home = fixture.roots.dsh_home.as_ref().unwrap();
    assert!(!dsh_home.exists());

    let before = state
        .application_connectors()
        .unwrap()
        .into_iter()
        .find(|item| item.id == ApplicationConnectorId::Dsh)
        .unwrap();
    assert_eq!(before.status, ApplicationConnectorStatus::Ready);
    assert!(before.detected);

    let model_values = dsh_model_values();
    let preview = state
        .preview_application_connector(
            ApplicationConnectorId::Dsh,
            ApplicationConnectorAction::Connect,
            Some(PRIMARY_KEY_ID),
            model_values.clone(),
        )
        .unwrap();
    let committed = state
        .commit_application_connector(ApplicationConnectorCommit {
            id: ApplicationConnectorId::Dsh,
            action: ApplicationConnectorAction::Connect,
            key_id: Some(PRIMARY_KEY_ID.into()),
            model_values: model_values.clone(),
            preview_fingerprint: preview.fingerprint,
        })
        .unwrap();
    assert_eq!(
        committed.inspection.status,
        ApplicationConnectorStatus::Connected
    );
    let env_path = dsh_home.join(".env");
    let secret = state.db.lock().primary_access_key_value().unwrap().unwrap();
    assert_eq!(
        fs::read_to_string(&env_path).unwrap(),
        format!(
            "OCG_MANAGER_API_KEY={}\n",
            serde_json::to_string(&secret).unwrap()
        )
    );
    assert!(dsh_home.join("profiles/web/package.json").is_file());
    assert!(
        !fs::read_to_string(state_path(&fixture.data_dir, ApplicationConnectorId::Dsh))
            .unwrap()
            .contains(&secret)
    );

    let restore_preview = state
        .preview_application_connector(
            ApplicationConnectorId::Dsh,
            ApplicationConnectorAction::Restore,
            None,
            model_values.clone(),
        )
        .unwrap();
    let restored = state
        .commit_application_connector(ApplicationConnectorCommit {
            id: ApplicationConnectorId::Dsh,
            action: ApplicationConnectorAction::Restore,
            key_id: None,
            model_values,
            preview_fingerprint: restore_preview.fingerprint,
        })
        .unwrap();
    assert_eq!(
        restored.inspection.status,
        ApplicationConnectorStatus::Ready
    );
    assert!(!env_path.exists());
    assert!(!dsh_home.join("profiles/web/package.json").exists());
    let commands = runner.commands.lock().unwrap();
    assert_eq!(commands.len(), 2);
    assert!(commands[0].iter().any(|value| value == "add"));
    assert!(commands[1].iter().any(|value| value == "remove"));
}

#[test]
fn dsh_combined_connect_removes_new_plugin_when_credential_write_fails() {
    let mut fixture = Fixture::new("dsh-hybrid-connect-compensation");
    let (state, runner) = dsh_hybrid_state(&mut fixture, true);
    let model_values = dsh_model_values();
    let preview = state
        .preview_application_connector(
            ApplicationConnectorId::Dsh,
            ApplicationConnectorAction::Connect,
            Some(PRIMARY_KEY_ID),
            model_values.clone(),
        )
        .unwrap();
    let error = state
        .commit_application_connector(ApplicationConnectorCommit {
            id: ApplicationConnectorId::Dsh,
            action: ApplicationConnectorAction::Connect,
            key_id: Some(PRIMARY_KEY_ID.into()),
            model_values,
            preview_fingerprint: preview.fingerprint,
        })
        .unwrap_err();
    assert_eq!(error.kind(), ApplicationConnectorErrorKind::Precondition);
    let dsh_home = fixture.roots.dsh_home.as_ref().unwrap();
    assert!(!dsh_home.join("profiles/web/package.json").exists());
    assert!(!state_path(&fixture.data_dir, ApplicationConnectorId::Dsh).exists());
    let commands = runner.commands.lock().unwrap();
    assert_eq!(commands.len(), 2);
    assert!(commands[0].iter().any(|value| value == "add"));
    assert!(commands[1].iter().any(|value| value == "remove"));
}

#[test]
fn dsh_combined_upgrade_restores_previous_plugin_when_credential_write_fails() {
    let mut fixture = Fixture::new("dsh-hybrid-upgrade-compensation");
    let (state, runner) = dsh_hybrid_state(&mut fixture, false);
    let first_values = BTreeMap::from([("models".into(), "model-a".into())]);
    let first_preview = state
        .preview_application_connector(
            ApplicationConnectorId::Dsh,
            ApplicationConnectorAction::Connect,
            Some(PRIMARY_KEY_ID),
            first_values.clone(),
        )
        .unwrap();
    state
        .commit_application_connector(ApplicationConnectorCommit {
            id: ApplicationConnectorId::Dsh,
            action: ApplicationConnectorAction::Connect,
            key_id: Some(PRIMARY_KEY_ID.into()),
            model_values: first_values,
            preview_fingerprint: first_preview.fingerprint,
        })
        .unwrap();
    let manifest = fixture
        .roots
        .dsh_home
        .as_ref()
        .unwrap()
        .join("profiles/web/package.json");
    let first_manifest: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    let first_source = first_manifest["dependencies"]["ocg-manager-dsh"]
        .as_str()
        .unwrap()
        .to_string();

    runner.poison_env_after_add.store(true, Ordering::SeqCst);
    let second_values = BTreeMap::from([("models".into(), "model-b".into())]);
    let second_preview = state
        .preview_application_connector(
            ApplicationConnectorId::Dsh,
            ApplicationConnectorAction::Connect,
            Some(PRIMARY_KEY_ID),
            second_values.clone(),
        )
        .unwrap();
    let error = state
        .commit_application_connector(ApplicationConnectorCommit {
            id: ApplicationConnectorId::Dsh,
            action: ApplicationConnectorAction::Connect,
            key_id: Some(PRIMARY_KEY_ID.into()),
            model_values: second_values,
            preview_fingerprint: second_preview.fingerprint,
        })
        .unwrap_err();
    assert_eq!(error.kind(), ApplicationConnectorErrorKind::Precondition);

    let restored: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    let restored_source = restored["dependencies"]["ocg-manager-dsh"]
        .as_str()
        .unwrap()
        .strip_prefix("file:")
        .unwrap();
    assert_eq!(
        fs::canonicalize(Path::new(restored_source)).unwrap(),
        fs::canonicalize(Path::new(first_source.strip_prefix("file:").unwrap())).unwrap()
    );
    let commands = runner.commands.lock().unwrap();
    assert_eq!(commands.len(), 3);
    assert!(
        commands
            .iter()
            .all(|args| args.iter().any(|value| value == "add"))
    );
    assert!(state_path(&fixture.data_dir, ApplicationConnectorId::Dsh).exists());
}

#[test]
fn managed_config_engine_has_exactly_seven_automatic_writers() {
    let fixture = Fixture::new("seven-matrix");
    let managed_ids = [
        ApplicationConnectorId::ClaudeCode,
        ApplicationConnectorId::Codex,
        ApplicationConnectorId::Dsh,
        ApplicationConnectorId::GeminiCli,
        ApplicationConnectorId::OpenCode,
        ApplicationConnectorId::OpenClaw,
        ApplicationConnectorId::Hermes,
    ];
    let inspections = managed_ids
        .into_iter()
        .map(|id| inspect(&fixture.roots, &fixture.data_dir, &fixture.cipher, id))
        .collect::<Vec<_>>();
    assert_eq!(inspections.len(), 7);
    assert_eq!(inspections.iter().filter(|item| item.automatic).count(), 7);
    for id in managed_ids {
        assert!(
            inspections
                .iter()
                .find(|item| item.id == id)
                .unwrap()
                .automatic
        );
    }
}
