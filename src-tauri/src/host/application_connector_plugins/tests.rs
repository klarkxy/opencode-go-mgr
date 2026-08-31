use super::*;
use ocg_core::application_connectors::ApplicationConnectorHostOperation;
use std::collections::VecDeque;
use std::sync::Mutex;
use uuid::Uuid;

#[cfg(windows)]
static WINDOWS_PROCESS_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(windows)]
fn lock_windows_process_tests() -> std::sync::MutexGuard<'static, ()> {
    WINDOWS_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

const PI_FILES: &[NativePluginTemplateFile] = &[
    NativePluginTemplateFile {
        relative_path: "package.json",
        contents: r#"{"name":"ocg-manager-pi"}"#,
    },
    NativePluginTemplateFile {
        relative_path: "index.ts",
        contents: "export const url = '__OCG_GATEWAY_URL__'; export const models = __OCG_MODELS_JSON__;",
    },
    NativePluginTemplateFile {
        relative_path: "models.generated.json",
        contents: r#"{"version":1,"models":"__OCG_MANAGER_GENERATED_MODELS__"}"#,
    },
];
const DSH_FILES: &[NativePluginTemplateFile] = &[
    NativePluginTemplateFile {
        relative_path: "package.json",
        contents: r#"{"name":"ocg-manager-dsh","dsh":{"plugin":true}}"#,
    },
    NativePluginTemplateFile {
        relative_path: "index.ts",
        contents: "export const env = 'OCG_MANAGER_API_KEY'; export const models = __OCG_MODELS_JSON__;",
    },
];

#[derive(Default)]
struct FakeRunner {
    commands: Mutex<Vec<NativePluginCommand>>,
    output: Mutex<NativePluginCommandOutput>,
    updates: Mutex<VecDeque<RegistryUpdate>>,
}

enum RegistryUpdate {
    Write(PathBuf, Vec<u8>),
    Remove(PathBuf),
}

impl NativePluginCommandRunner for FakeRunner {
    fn run(&self, command: &NativePluginCommand) -> Result<NativePluginCommandOutput, String> {
        self.commands.lock().unwrap().push(command.clone());
        let output = self.output.lock().unwrap().clone();
        if output.success {
            if let Some(update) = self.updates.lock().unwrap().pop_front() {
                match update {
                    RegistryUpdate::Write(path, bytes) => {
                        fs::create_dir_all(path.parent().ok_or("registry path has no parent")?)
                            .map_err(|error| error.to_string())?;
                        fs::write(path, bytes).map_err(|error| error.to_string())?;
                    }
                    RegistryUpdate::Remove(path) => match fs::remove_file(path) {
                        Ok(()) => {}
                        Err(error) if error.kind() == ErrorKind::NotFound => {}
                        Err(error) => return Err(error.to_string()),
                    },
                }
            }
        }
        Ok(output)
    }
}

struct Fixture {
    root: PathBuf,
    roots: NativePluginRoots,
    pi: PathBuf,
    dsh: PathBuf,
    runner: Arc<FakeRunner>,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ocg-native-plugin-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let pi = root.join("bin/pi.exe");
        let dsh = root.join("bin/dsh.exe");
        fs::create_dir_all(pi.parent().unwrap()).unwrap();
        fs::write(&pi, []).unwrap();
        fs::write(&dsh, []).unwrap();
        let roots = NativePluginRoots {
            data_dir: root.join("data"),
            pi_settings: root.join("pi/settings.json"),
            dsh_web_manifest: root.join("dsh/profiles/web/package.json"),
        };
        let runner = Arc::new(FakeRunner {
            output: Mutex::new(NativePluginCommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            }),
            ..Default::default()
        });
        Self {
            root,
            roots,
            pi,
            dsh,
            runner,
        }
    }

    fn host(&self) -> NativePluginHost {
        NativePluginHost::new(
            self.roots.clone(),
            NativePluginTemplates {
                pi: PI_FILES,
                dsh: DSH_FILES,
            },
            self.runner.clone(),
            Some(self.pi.clone()),
            Some(self.dsh.clone()),
        )
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
            key_id: None,
            secret: None,
            model_values: BTreeMap::from([("models".into(), "first\nsecond".into())]),
            gateway_url: "http://127.0.0.1:9042".into(),
            data_dir: self.roots.data_dir.clone(),
            desktop_executable: None,
            preview_fingerprint: None,
        }
    }

    fn queue_write(&self, path: PathBuf, bytes: Vec<u8>) {
        self.runner
            .updates
            .lock()
            .unwrap()
            .push_back(RegistryUpdate::Write(path, bytes));
    }

    fn queue_remove(&self, path: PathBuf) {
        self.runner
            .updates
            .lock()
            .unwrap()
            .push_back(RegistryUpdate::Remove(path));
    }

    fn dsh_manifest(&self, source: &Path) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "dependencies": {
                (DSH_PACKAGE_NAME): format!("file:{}", source.display())
            },
            "dsh": { "profile": { "bundles": [DSH_PACKAGE_NAME] } }
        }))
        .unwrap()
    }

    fn pi_settings(&self, source: &Path) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "packages": [source.to_string_lossy().to_string()]
        }))
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn models_are_rendered_without_any_key_material() {
    let fixture = Fixture::new();
    let request = fixture.request(
        ApplicationConnectorId::Pi,
        ApplicationConnectorAction::Connect,
    );
    let package = fixture.host().render_package(&request).unwrap();
    let content = String::from_utf8(
        package
            .files
            .get(&PathBuf::from("index.ts"))
            .unwrap()
            .clone(),
    )
    .unwrap();
    assert!(content.contains("first"));
    assert!(content.contains("second"));
    assert!(!content.contains("OCG_MANAGER_API_KEY"));
    assert!(!content.contains("secret"));

    let catalog: serde_json::Value = serde_json::from_slice(
        package
            .files
            .get(&PathBuf::from("models.generated.json"))
            .unwrap(),
    )
    .unwrap();
    let models = catalog["models"].as_array().unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0]["id"], "first");
    assert_eq!(models[1]["id"], "second");
}

#[test]
fn preview_fingerprint_detects_stale_native_state() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let request = fixture.request(
        ApplicationConnectorId::Pi,
        ApplicationConnectorAction::Connect,
    );
    let preview = host.preview(&request).unwrap();
    fs::create_dir_all(fixture.roots.pi_settings.parent().unwrap()).unwrap();
    fs::write(&fixture.roots.pi_settings, "changed").unwrap();
    let mut commit = request;
    commit.preview_fingerprint = Some(preview.fingerprint);
    let error = host.commit(&commit).unwrap_err();
    assert_eq!(error.kind(), ApplicationConnectorErrorKind::Conflict);
    assert!(fixture.runner.commands.lock().unwrap().is_empty());
}

#[test]
fn immutable_package_with_an_extra_file_never_reaches_the_client() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let request = fixture.request(
        ApplicationConnectorId::Dsh,
        ApplicationConnectorAction::Connect,
    );
    let package = host.render_package(&request).unwrap();
    package.materialize().unwrap();
    fs::write(
        package.path.join("unexpected.txt"),
        "not part of the digest",
    )
    .unwrap();
    let preview = host.preview(&request).unwrap();
    let mut commit = request;
    commit.preview_fingerprint = Some(preview.fingerprint);
    let error = host.commit(&commit).unwrap_err();
    assert_eq!(error.kind(), ApplicationConnectorErrorKind::Conflict);
    assert!(fixture.runner.commands.lock().unwrap().is_empty());
}

#[test]
fn dsh_install_and_restore_use_fixed_web_profile_commands() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let request = fixture.request(
        ApplicationConnectorId::Dsh,
        ApplicationConnectorAction::Connect,
    );
    let package = host.render_package(&request).unwrap();
    fixture.queue_write(
        fixture.roots.dsh_web_manifest.clone(),
        fixture.dsh_manifest(&package.path),
    );
    let preview = host.preview(&request).unwrap();
    let mut commit = request;
    commit.preview_fingerprint = Some(preview.fingerprint);
    let result = host.commit(&commit).unwrap();
    assert!(result.changed);
    let commands = fixture.runner.commands.lock().unwrap();
    assert_eq!(
        commands[0].args[..4],
        [
            OsString::from("plugin"),
            OsString::from("--profile"),
            OsString::from("web"),
            OsString::from("add"),
        ]
    );
    #[cfg(windows)]
    {
        let package_argument = commands[0].args[4].to_string_lossy();
        assert!(package_argument.starts_with('"'));
        assert!(package_argument.ends_with('"'));
    }
    drop(commands);

    let restore = fixture.request(
        ApplicationConnectorId::Dsh,
        ApplicationConnectorAction::Restore,
    );
    let restore_preview = host.preview(&restore).unwrap();
    fixture.queue_write(
        fixture.roots.dsh_web_manifest.clone(),
        serde_json::to_vec(&serde_json::json!({
            "dependencies": { "unrelated-package": "1.0.0" },
            "dsh": { "profile": { "bundles": ["unrelated-package"] } }
        }))
        .unwrap(),
    );
    let mut restore_commit = restore;
    restore_commit.preview_fingerprint = Some(restore_preview.fingerprint);
    host.commit(&restore_commit).unwrap();
    let commands = fixture.runner.commands.lock().unwrap();
    assert_eq!(
        commands[1].args,
        vec![
            OsString::from("plugin"),
            OsString::from("--profile"),
            OsString::from("web"),
            OsString::from("remove"),
            OsString::from("ocg-manager-dsh"),
        ]
    );
    drop(commands);
    let preserved: Value =
        serde_json::from_slice(&fs::read(&fixture.roots.dsh_web_manifest).unwrap()).unwrap();
    assert_eq!(
        preserved.pointer("/dependencies/unrelated-package"),
        Some(&Value::String("1.0.0".into()))
    );
    assert_eq!(
        preserved.pointer("/dsh/profile/bundles/0"),
        Some(&Value::String("unrelated-package".into()))
    );
}

#[test]
fn pi_and_dsh_switch_back_to_the_exact_cached_package_source() {
    for id in [ApplicationConnectorId::Pi, ApplicationConnectorId::Dsh] {
        let fixture = Fixture::new();
        let host = fixture.host();
        let mut sources = Vec::new();
        for model in ["model-a", "model-b", "model-a"] {
            let mut request = fixture.request(id, ApplicationConnectorAction::Connect);
            request.model_values.insert("models".into(), model.into());
            let package = host.render_package(&request).unwrap();
            let registry = match id {
                ApplicationConnectorId::Pi => fixture.pi_settings(&package.path),
                ApplicationConnectorId::Dsh => fixture.dsh_manifest(&package.path),
                _ => unreachable!(),
            };
            let registry_path = match id {
                ApplicationConnectorId::Pi => fixture.roots.pi_settings.clone(),
                ApplicationConnectorId::Dsh => fixture.roots.dsh_web_manifest.clone(),
                _ => unreachable!(),
            };
            fixture.queue_write(registry_path, registry);
            let preview = host.preview(&request).unwrap();
            assert_eq!(preview.changes.len(), 1);
            let mut commit = request;
            commit.preview_fingerprint = Some(preview.fingerprint);
            assert!(host.commit(&commit).unwrap().changed);
            sources.push(package.path);
        }
        assert_ne!(sources[0], sources[1]);
        assert_eq!(sources[0], sources[2]);
        match host.install_state(id) {
            InstallState::Connected {
                source: Some(source),
            } => assert!(same_lexical_path(&source, &sources[0])),
            state => panic!("expected exact connected source, got {state:?}"),
        }
        assert_eq!(fixture.runner.commands.lock().unwrap().len(), 3);
    }
}

#[test]
fn dsh_compensation_preserves_external_drift_after_first_install_failure() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let request = fixture.request(
        ApplicationConnectorId::Dsh,
        ApplicationConnectorAction::Connect,
    );
    let compensation = host.prepare_dsh_connect_compensation(&request).unwrap();
    let external = serde_json::to_vec(&serde_json::json!({
        "dependencies": { (DSH_PACKAGE_NAME): "9.9.9-external" },
        "dsh": { "profile": { "bundles": [DSH_PACKAGE_NAME] } }
    }))
    .unwrap();
    fs::create_dir_all(fixture.roots.dsh_web_manifest.parent().unwrap()).unwrap();
    fs::write(&fixture.roots.dsh_web_manifest, &external).unwrap();

    let error = host.restore_dsh_installation(&compensation).unwrap_err();
    assert_eq!(error.kind(), ApplicationConnectorErrorKind::Conflict);
    assert_eq!(fs::read(&fixture.roots.dsh_web_manifest).unwrap(), external);
    assert!(fixture.runner.commands.lock().unwrap().is_empty());
}

#[test]
fn dsh_compensation_preserves_third_owned_source_after_upgrade_failure() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let mut old_request = fixture.request(
        ApplicationConnectorId::Dsh,
        ApplicationConnectorAction::Connect,
    );
    old_request
        .model_values
        .insert("models".into(), "model-old".into());
    let old_package = host.render_package(&old_request).unwrap();
    old_package.materialize().unwrap();
    fs::create_dir_all(fixture.roots.dsh_web_manifest.parent().unwrap()).unwrap();
    fs::write(
        &fixture.roots.dsh_web_manifest,
        fixture.dsh_manifest(&old_package.path),
    )
    .unwrap();

    let mut desired_request = old_request;
    desired_request
        .model_values
        .insert("models".into(), "model-desired".into());
    let compensation = host
        .prepare_dsh_connect_compensation(&desired_request)
        .unwrap();
    let mut external_request = desired_request;
    external_request
        .model_values
        .insert("models".into(), "model-external".into());
    let external_package = host.render_package(&external_request).unwrap();
    external_package.materialize().unwrap();
    let external = fixture.dsh_manifest(&external_package.path);
    fs::write(&fixture.roots.dsh_web_manifest, &external).unwrap();

    let error = host.restore_dsh_installation(&compensation).unwrap_err();
    assert_eq!(error.kind(), ApplicationConnectorErrorKind::Conflict);
    assert_eq!(fs::read(&fixture.roots.dsh_web_manifest).unwrap(), external);
    assert!(fixture.runner.commands.lock().unwrap().is_empty());
}

#[test]
fn pi_restore_removes_the_exact_generated_package_source() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let request = fixture.request(
        ApplicationConnectorId::Pi,
        ApplicationConnectorAction::Connect,
    );
    let package = host.render_package(&request).unwrap();
    package.materialize().unwrap();
    fs::create_dir_all(fixture.roots.pi_settings.parent().unwrap()).unwrap();
    fs::write(
        &fixture.roots.pi_settings,
        serde_json::to_vec(&serde_json::json!({
            "packages": [package.path.to_string_lossy().to_string()]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut restore = fixture.request(
        ApplicationConnectorId::Pi,
        ApplicationConnectorAction::Restore,
    );
    restore
        .model_values
        .insert("models".into(), "replacement-model".into());
    let preview = host.preview(&restore).unwrap();
    let mut commit = restore;
    commit.preview_fingerprint = Some(preview.fingerprint);
    fixture.queue_remove(fixture.roots.pi_settings.clone());
    host.commit(&commit).unwrap();
    let command = fixture.runner.commands.lock().unwrap().pop().unwrap();
    assert_eq!(command.args[0], OsString::from("remove"));
    assert_eq!(
        fs::canonicalize(PathBuf::from(&command.args[1])).unwrap(),
        fs::canonicalize(package.path).unwrap(),
    );
}

#[test]
fn invalid_pi_settings_are_partial_and_block_native_commands() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.roots.pi_settings.parent().unwrap()).unwrap();
    fs::write(&fixture.roots.pi_settings, "not-json").unwrap();
    let host = fixture.host();
    assert_eq!(
        host.inspect(ApplicationConnectorId::Pi).status,
        ApplicationConnectorStatus::Partial
    );
    let request = fixture.request(
        ApplicationConnectorId::Pi,
        ApplicationConnectorAction::Connect,
    );
    assert_eq!(
        host.preview(&request).unwrap_err().kind(),
        ApplicationConnectorErrorKind::Conflict
    );
    assert!(fixture.runner.commands.lock().unwrap().is_empty());
}

#[test]
fn failed_command_is_redacted_and_does_not_write_a_secret() {
    let fixture = Fixture::new();
    *fixture.runner.output.lock().unwrap() = NativePluginCommandOutput {
        success: false,
        stdout: String::new(),
        stderr: "Authorization: Bearer very-secret".into(),
    };
    let host = fixture.host();
    let request = fixture.request(
        ApplicationConnectorId::Pi,
        ApplicationConnectorAction::Connect,
    );
    let preview = host.preview(&request).unwrap();
    let mut commit = request;
    commit.preview_fingerprint = Some(preview.fingerprint);
    let error = host.commit(&commit).unwrap_err();
    assert!(!error.to_string().contains("very-secret"));
    let package_root = package_root(&fixture.roots.data_dir, ApplicationConnectorId::Pi);
    let package_content = fs::read_dir(package_root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(
        !String::from_utf8_lossy(&fs::read(package_content.join("index.ts")).unwrap())
            .contains("very-secret")
    );
}

#[test]
fn commands_never_include_credential_material() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let request = fixture.request(
        ApplicationConnectorId::Pi,
        ApplicationConnectorAction::Connect,
    );
    let package = host.render_package(&request).unwrap();
    fixture.queue_write(
        fixture.roots.pi_settings.clone(),
        fixture.pi_settings(&package.path),
    );
    let preview = host.preview(&request).unwrap();
    let mut commit = request;
    commit.preview_fingerprint = Some(preview.fingerprint);
    host.commit(&commit).unwrap();
    let command = fixture.runner.commands.lock().unwrap().pop().unwrap();
    let rendered = command
        .args
        .iter()
        .map(|value| value.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(!rendered.contains("OCG_MANAGER_API_KEY"));
    assert!(!rendered.contains("secret"));
}

#[test]
fn exit_zero_without_registry_change_is_not_success() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let request = fixture.request(
        ApplicationConnectorId::Pi,
        ApplicationConnectorAction::Connect,
    );
    let preview = host.preview(&request).unwrap();
    let mut commit = request;
    commit.preview_fingerprint = Some(preview.fingerprint);
    let error = host.commit(&commit).unwrap_err();
    assert_eq!(error.kind(), ApplicationConnectorErrorKind::Precondition);
    assert!(
        error
            .to_string()
            .contains("registry state was not confirmed")
    );
    assert_eq!(
        host.inspect(ApplicationConnectorId::Pi).status,
        ApplicationConnectorStatus::Ready
    );
}

#[test]
fn dsh_same_name_package_outside_owned_source_is_never_removed() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.roots.dsh_web_manifest.parent().unwrap()).unwrap();
    fs::write(
        &fixture.roots.dsh_web_manifest,
        serde_json::to_vec(&serde_json::json!({
            "dependencies": { (DSH_PACKAGE_NAME): "1.2.3" },
            "dsh": { "profile": { "bundles": [DSH_PACKAGE_NAME] } }
        }))
        .unwrap(),
    )
    .unwrap();
    let host = fixture.host();
    assert_eq!(
        host.inspect(ApplicationConnectorId::Dsh).status,
        ApplicationConnectorStatus::Partial
    );
    let request = fixture.request(
        ApplicationConnectorId::Dsh,
        ApplicationConnectorAction::Restore,
    );
    assert_eq!(
        host.preview(&request).unwrap_err().kind(),
        ApplicationConnectorErrorKind::Conflict
    );
    assert!(fixture.runner.commands.lock().unwrap().is_empty());
}

#[test]
fn linked_package_ancestor_is_rejected_when_supported() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.roots.data_dir).unwrap();
    let external = fixture.root.join("external-package-root");
    fs::create_dir_all(&external).unwrap();
    let linked = fixture.roots.data_dir.join("application-connectors");
    #[cfg(windows)]
    if std::os::windows::fs::symlink_dir(&external, &linked).is_err() {
        return;
    }
    #[cfg(unix)]
    if std::os::unix::fs::symlink(&external, &linked).is_err() {
        return;
    }
    let host = fixture.host();
    let request = fixture.request(
        ApplicationConnectorId::Pi,
        ApplicationConnectorAction::Connect,
    );
    let package = host.render_package(&request).unwrap();
    assert_eq!(
        package.materialize().unwrap_err().kind(),
        ApplicationConnectorErrorKind::Conflict
    );
    assert_eq!(fs::read_dir(&external).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn unix_timeout_kills_descendants_in_the_process_group() {
    use nix::errno::Errno;
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let script = fixture.root.join("spawn-child.sh");
    let pid_file = fixture.root.join("child.pid");
    fs::write(
        &script,
        "#!/bin/sh\nsleep 30 &\nprintf '%s' \"$!\" > \"$1\"\nsleep 30\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script, permissions).unwrap();
    let command = NativePluginCommand {
        executable: script,
        display_executable: "isolated timeout fixture".into(),
        args: vec![pid_file.clone().into_os_string()],
        timeout: Duration::from_secs(2),
    };
    let error = ProcessNativePluginCommandRunner.run(&command).unwrap_err();
    assert!(error.contains("timed out"));
    let pid = fs::read_to_string(&pid_file)
        .expect("the descendant started before the timeout")
        .parse::<i32>()
        .unwrap();
    let started = Instant::now();
    loop {
        match kill(Pid::from_raw(pid), None) {
            Err(Errno::ESRCH) => break,
            _ if started.elapsed() < Duration::from_secs(2) => {
                thread::sleep(Duration::from_millis(20));
            }
            result => panic!("descendant process remained after timeout: {result:?}"),
        }
    }
}

#[cfg(windows)]
#[test]
fn windows_timeout_kills_batch_descendants_in_the_job() {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let _process_lock = lock_windows_process_tests();
    let fixture = Fixture::new();
    let batch = fixture.root.join("spawn-child.cmd");
    let pid_file = fixture.root.join("child.pid");
    fs::write(
            &batch,
            format!(
                "@echo off\r\npowershell.exe -NoProfile -NonInteractive -Command \"$PID | Set-Content -NoNewline -LiteralPath '{}' ; Start-Sleep -Seconds 30\"\r\n",
                pid_file.display()
            ),
        )
        .unwrap();
    let command = NativePluginCommand {
        executable: batch,
        display_executable: "isolated timeout fixture".into(),
        args: Vec::new(),
        timeout: Duration::from_secs(15),
    };
    let error = ProcessNativePluginCommandRunner.run(&command).unwrap_err();
    assert!(error.contains("timed out"));
    let pid = fs::read_to_string(&pid_file)
        .expect("the descendant started before the timeout")
        .parse::<u32>()
        .unwrap();
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if !process.is_null() {
            let mut exit_code = STILL_ACTIVE as u32;
            assert_ne!(GetExitCodeProcess(process, &mut exit_code), 0);
            CloseHandle(process);
            assert_ne!(exit_code, STILL_ACTIVE as u32);
        }
    }
}

#[cfg(windows)]
#[test]
fn windows_suspended_batch_launch_preserves_fixed_arguments() {
    let _process_lock = lock_windows_process_tests();
    let fixture = Fixture::new();
    let batch = fixture.root.join("echo-arguments.cmd");
    fs::write(&batch, "@echo off\r\necho [%~1][%~2]\r\n").unwrap();
    let output = ProcessNativePluginCommandRunner
        .run(&NativePluginCommand {
            executable: batch,
            display_executable: "isolated argument fixture".into(),
            args: vec![OsString::from("plugin"), OsString::from("path with spaces")],
            timeout: Duration::from_secs(30),
        })
        .unwrap();
    assert!(output.success);
    assert!(output.stdout.contains("[plugin][path with spaces]"));
}

#[cfg(windows)]
#[test]
fn windows_batch_forwarding_preserves_literal_quotes_for_dsh() {
    let _process_lock = lock_windows_process_tests();
    let fixture = Fixture::new();
    let batch = fixture.root.join("forward-arguments.cmd");
    fs::write(
            &batch,
            "@echo off\r\nnode.exe -e \"process.stdout.write(JSON.stringify(process.argv.slice(1)))\" %*\r\n",
        )
        .unwrap();
    let output = ProcessNativePluginCommandRunner
        .run(&NativePluginCommand {
            executable: batch,
            display_executable: "isolated forwarding fixture".into(),
            args: vec![OsString::from("\"path with spaces\"")],
            timeout: Duration::from_secs(30),
        })
        .unwrap();
    assert!(output.success);
    let arguments: Vec<String> = serde_json::from_str(output.stdout.trim()).unwrap();
    assert_eq!(arguments, vec!["\"path with spaces\""]);
}

#[cfg(windows)]
#[test]
fn windows_native_client_discovery_includes_package_manager_shims() {
    assert_eq!(
        executable_names(Path::new("dsh")),
        vec![
            OsString::from("dsh.exe"),
            OsString::from("dsh.cmd"),
            OsString::from("dsh.bat"),
            OsString::from("dsh"),
        ],
    );
}

#[cfg(windows)]
#[test]
fn windows_discovery_prefers_cmd_over_extensionless_npm_shim() {
    let root = std::env::temp_dir().join(format!(
        "ocg-native-plugin-discovery-test-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("dsh"), "#!/bin/sh\n").unwrap();
    fs::write(root.join("dsh.cmd"), "@echo off\r\n").unwrap();

    let resolved = resolve_executable_in_directories(Path::new("dsh"), vec![root.clone()])
        .expect("a package-manager shim should be discovered");
    assert_eq!(resolved.path, root.join("dsh.cmd"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires an installed DSH and an explicit isolated DSH_HOME"]
fn installed_dsh_isolated_plugin_boot() {
    const REAL_DSH_FILES: &[NativePluginTemplateFile] = &[
        NativePluginTemplateFile {
            relative_path: "package.json",
            contents: include_str!("../../../../integrations/dsh/package.json"),
        },
        NativePluginTemplateFile {
            relative_path: "README.md",
            contents: include_str!("../../../../integrations/dsh/README.md"),
        },
        NativePluginTemplateFile {
            relative_path: "index.js",
            contents: include_str!("../../../../integrations/dsh/index.js"),
        },
        NativePluginTemplateFile {
            relative_path: "cordis.patch.yml",
            contents: include_str!("../../../../integrations/dsh/cordis.patch.yml"),
        },
    ];

    struct IsolatedHome(PathBuf);
    impl Drop for IsolatedHome {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    let home = std::env::var_os("DSH_HOME")
        .map(PathBuf::from)
        .expect("set DSH_HOME to a fresh isolated temporary directory");
    let temp = std::env::temp_dir();
    assert!(
        home.starts_with(&temp)
            && home
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with("ocg-dsh-isolated-smoke-")),
        "DSH_HOME must be a fresh ocg-dsh-isolated-smoke-* directory under the OS temp root"
    );
    assert!(!home.exists(), "isolated DSH_HOME must not already exist");
    fs::create_dir_all(&home).unwrap();
    let _cleanup = IsolatedHome(home.clone());
    fs::write(home.join(".env"), "OCG_MANAGER_API_KEY=isolated-fake-key\n").unwrap();

    let executable = resolve_executable(Path::new("dsh"))
        .expect("the installed DSH executable must be discoverable");
    let roots = NativePluginRoots {
        data_dir: home.join("ocg data with spaces"),
        pi_settings: home.join("unused-pi-settings.json"),
        dsh_web_manifest: home.join("profiles/web/package.json"),
    };
    let host = NativePluginHost::new(
        roots.clone(),
        NativePluginTemplates {
            pi: PI_FILES,
            dsh: REAL_DSH_FILES,
        },
        Arc::new(ProcessNativePluginCommandRunner),
        None,
        Some(executable.path.clone()),
    );
    let mut connect = ApplicationConnectorHostRequest {
        operation: ApplicationConnectorHostOperation::Preview,
        id: ApplicationConnectorId::Dsh,
        action: ApplicationConnectorAction::Connect,
        key_id: Some("isolated-key".into()),
        secret: None,
        model_values: BTreeMap::from([("models".into(), "gpt-5.6".into())]),
        gateway_url: "http://127.0.0.1:9042".into(),
        data_dir: roots.data_dir.clone(),
        desktop_executable: None,
        preview_fingerprint: None,
    };
    connect.preview_fingerprint = Some(host.preview(&connect).unwrap().fingerprint);
    let installed = host.commit(&connect).unwrap();
    assert_eq!(
        installed.inspection.status,
        ApplicationConnectorStatus::Connected
    );

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let boot = NativePluginCommand {
        executable: executable.path,
        display_executable: "isolated DSH boot".into(),
        args: vec![
            OsString::from("--profile"),
            OsString::from("web"),
            OsString::from("--no-open"),
            OsString::from("--port"),
            OsString::from(port.to_string()),
        ],
        timeout: Duration::from_secs(15),
    };
    let running = thread::spawn(move || ProcessNativePluginCommandRunner.run(&boot));
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut accepted = false;
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            accepted = true;
            break;
        }
        if running.is_finished() {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let boot_result = running.join().expect("DSH boot runner panicked");
    assert!(
        accepted,
        "isolated DSH never opened its local web port: {boot_result:?}"
    );
    assert!(
        boot_result
            .as_ref()
            .is_err_and(|error| error.contains("timed out")),
        "isolated DSH should remain running until the bounded smoke timeout: {boot_result:?}"
    );

    let mut restore = ApplicationConnectorHostRequest {
        operation: ApplicationConnectorHostOperation::Preview,
        id: ApplicationConnectorId::Dsh,
        action: ApplicationConnectorAction::Restore,
        key_id: None,
        secret: None,
        model_values: BTreeMap::new(),
        gateway_url: "http://127.0.0.1:9042".into(),
        data_dir: roots.data_dir,
        desktop_executable: None,
        preview_fingerprint: None,
    };
    restore.preview_fingerprint = Some(host.preview(&restore).unwrap().fingerprint);
    let removed = host.commit(&restore).unwrap();
    assert_eq!(removed.inspection.status, ApplicationConnectorStatus::Ready);
}
