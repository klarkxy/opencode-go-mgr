//! Desktop-owned local application connector engine.
//!
//! The engine is deliberately a sealed local-writer implementation. Every
//! automatic connector owns fields, rather than replacing whole user profiles.

use fs2::FileExt;
use ocg_core::application_connectors::*;
use ocg_core::crypto::KeyCipher;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use serde_yaml::{Mapping as YamlMapping, Value as YamlValue};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use super::application_connector_plugins::{
    NativePluginHost, NativePluginRoots, NativePluginTemplateFile, NativePluginTemplates,
    ProcessNativePluginCommandRunner,
};

const MAX_BYTES: u64 = 1024 * 1024;
const STATE_VERSION: u8 = 1;
const JOURNAL_VERSION: u8 = 1;
const REDACTED: &str = "[redacted]";

const PI_PLUGIN_FILES: &[NativePluginTemplateFile] = &[
    NativePluginTemplateFile {
        relative_path: "package.json",
        contents: include_str!("../../../integrations/pi/package.json"),
    },
    NativePluginTemplateFile {
        relative_path: "README.md",
        contents: include_str!("../../../integrations/pi/README.md"),
    },
    NativePluginTemplateFile {
        relative_path: "models.generated.json",
        contents: include_str!("../../../integrations/pi/models.generated.json"),
    },
    NativePluginTemplateFile {
        relative_path: "extensions/ocg-manager.ts",
        contents: include_str!("../../../integrations/pi/extensions/ocg-manager.ts"),
    },
];

const DSH_PLUGIN_FILES: &[NativePluginTemplateFile] = &[
    NativePluginTemplateFile {
        relative_path: "package.json",
        contents: include_str!("../../../integrations/dsh/package.json"),
    },
    NativePluginTemplateFile {
        relative_path: "README.md",
        contents: include_str!("../../../integrations/dsh/README.md"),
    },
    NativePluginTemplateFile {
        relative_path: "index.js",
        contents: include_str!("../../../integrations/dsh/index.js"),
    },
    NativePluginTemplateFile {
        relative_path: "cordis.patch.yml",
        contents: include_str!("../../../integrations/dsh/cordis.patch.yml"),
    },
];

const NATIVE_PLUGIN_TEMPLATES: NativePluginTemplates = NativePluginTemplates {
    pi: PI_PLUGIN_FILES,
    dsh: DSH_PLUGIN_FILES,
};

#[derive(Clone)]
struct Roots {
    home: PathBuf,
    local_app_data: PathBuf,
    hermes_home: Option<PathBuf>,
    codex_home: Option<PathBuf>,
    dsh_home: Option<PathBuf>,
    pi_agent_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DocumentFormat {
    Json,
    Env,
    Toml,
    Yaml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateV1 {
    version: u8,
    connector_id: String,
    documents: Vec<DocumentState>,
    integrity_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentState {
    document_id: String,
    format: DocumentFormat,
    originally_existed: bool,
    fields: Vec<FieldState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldState {
    locator: String,
    sensitive: bool,
    original: StoredOriginal,
    applied: StoredApplied,
    #[serde(default)]
    ancestors: Vec<AncestorState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StoredOriginal {
    Absent,
    Plain { value: String, sha256: String },
    Protected { value: String, sha256: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StoredApplied {
    Plain { value: String },
    Digest { sha256: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AncestorState {
    pointer: String,
    originally_existed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalV1 {
    version: u8,
    connector_id: String,
    files: Vec<JournalFile>,
    integrity_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalFile {
    document_id: String,
    before_existed: bool,
    before_sha256: String,
    after_existed: bool,
    after_sha256: String,
    protected_preimage: String,
}

#[derive(Debug, Clone)]
struct Target {
    id: &'static str,
    path: PathBuf,
    label: &'static str,
    format: DocumentFormat,
}

#[derive(Clone)]
enum DesiredValue {
    Json(Value),
    Env(String),
    Toml(String),
    Yaml(YamlValue),
}

#[derive(Clone)]
struct DesiredField {
    locator: String,
    value: DesiredValue,
    sensitive: bool,
}

#[derive(Clone)]
struct DesiredDocument {
    target: Target,
    fields: Vec<DesiredField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileImage {
    existed: bool,
    bytes: Vec<u8>,
}

struct PlannedFile {
    document_id: String,
    path: PathBuf,
    format: Option<DocumentFormat>,
    before: FileImage,
    after: FileImage,
}

struct MutationPlan {
    id: ApplicationConnectorId,
    files: Vec<PlannedFile>,
    changes: Vec<ApplicationConnectorChange>,
}

impl MutationPlan {
    fn changed(&self) -> bool {
        self.files.iter().any(|file| file.before != file.after)
    }
}

pub fn register(core: &ocg_core::state::CoreState, cipher: Arc<dyn KeyCipher + Send + Sync>) {
    let executable = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ocg-manager"));
    let roots = Roots {
        home: user_home(),
        local_app_data: local_app_data(),
        hermes_home: std::env::var_os("HERMES_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        codex_home: std::env::var_os("CODEX_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        dsh_home: std::env::var_os("DSH_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        pi_agent_dir: std::env::var_os("PI_CODING_AGENT_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
    };
    let host = Arc::new(move |request: ApplicationConnectorHostRequest| {
        execute(&roots, cipher.as_ref(), request)
    });
    core.set_application_connector_host(host, executable);
}

fn execute(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    request: ApplicationConnectorHostRequest,
) -> ApplicationConnectorResult<ApplicationConnectorHostResult> {
    let mut native_plugin_roots =
        NativePluginRoots::from_home(request.data_dir.clone(), &roots.home);
    if let Some(dsh_home) = &roots.dsh_home {
        native_plugin_roots.dsh_web_manifest = dsh_home.join("profiles/web/package.json");
    }
    if let Some(pi_agent_dir) = &roots.pi_agent_dir {
        native_plugin_roots.pi_settings = pi_agent_dir.join("settings.json");
    }
    let native_plugins = NativePluginHost::new(
        native_plugin_roots,
        NATIVE_PLUGIN_TEMPLATES,
        Arc::new(ProcessNativePluginCommandRunner),
        None,
        None,
    );
    execute_with_native_plugins(roots, cipher, request, &native_plugins)
}

fn execute_with_native_plugins(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    request: ApplicationConnectorHostRequest,
    native_plugins: &NativePluginHost,
) -> ApplicationConnectorResult<ApplicationConnectorHostResult> {
    let _lock = acquire_lock(&request.data_dir)?;
    match request.operation {
        ApplicationConnectorHostOperation::List => {
            let inspections = ApplicationConnectorId::ALL
                .into_iter()
                .map(|id| {
                    if id == ApplicationConnectorId::Dsh {
                        inspect_dsh_combined(roots, &request.data_dir, cipher, native_plugins)
                    } else if NativePluginHost::owns(id) {
                        native_plugins.inspect(id)
                    } else {
                        inspect(roots, &request.data_dir, cipher, id)
                    }
                })
                .collect::<Vec<_>>();
            Ok(ApplicationConnectorHostResult::Inspections(inspections))
        }
        ApplicationConnectorHostOperation::Preview => {
            let preview = if request.id == ApplicationConnectorId::Dsh {
                preview_dsh_combined(roots, cipher, native_plugins, &request)?
            } else if NativePluginHost::owns(request.id) {
                native_plugins.preview(&request)?
            } else {
                preview(roots, cipher, &request)?
            };
            Ok(ApplicationConnectorHostResult::Preview(preview))
        }
        ApplicationConnectorHostOperation::Commit => {
            let committed = if request.id == ApplicationConnectorId::Dsh {
                commit_dsh_combined(roots, cipher, native_plugins, &request)?
            } else if NativePluginHost::owns(request.id) {
                native_plugins.commit(&request)?
            } else {
                commit(roots, cipher, &request)?
            };
            Ok(ApplicationConnectorHostResult::Committed(committed))
        }
    }
}

fn inspect_dsh_combined(
    roots: &Roots,
    data_dir: &Path,
    cipher: &dyn KeyCipher,
    native_plugins: &NativePluginHost,
) -> ApplicationConnectorInspection {
    let managed = inspect(roots, data_dir, cipher, ApplicationConnectorId::Dsh);
    let native = native_plugins.inspect(ApplicationConnectorId::Dsh);
    let status = combined_dsh_status(managed.status, native.status);
    let detail = combine_details(managed.detail, native.detail).or_else(|| {
        (status == ApplicationConnectorStatus::Partial)
            .then(|| "DSH plugin and managed credential are out of sync".into())
    });
    ApplicationConnectorInspection {
        id: ApplicationConnectorId::Dsh,
        status,
        automatic: managed.automatic && native.automatic,
        detected: managed.detected || native.detected,
        detail,
        target_paths: combine_target_paths(managed.target_paths, native.target_paths),
    }
}

fn combined_dsh_status(
    managed: ApplicationConnectorStatus,
    native: ApplicationConnectorStatus,
) -> ApplicationConnectorStatus {
    if managed == ApplicationConnectorStatus::Conflict
        || native == ApplicationConnectorStatus::Conflict
    {
        return ApplicationConnectorStatus::Conflict;
    }
    if managed == ApplicationConnectorStatus::Partial
        || native == ApplicationConnectorStatus::Partial
    {
        return ApplicationConnectorStatus::Partial;
    }
    if managed == ApplicationConnectorStatus::Connected
        && native == ApplicationConnectorStatus::Connected
    {
        return ApplicationConnectorStatus::Connected;
    }
    if managed == ApplicationConnectorStatus::Connected
        || native == ApplicationConnectorStatus::Connected
    {
        return ApplicationConnectorStatus::Partial;
    }
    // A fresh DSH installation has an executable but may not have created its
    // profile directory yet. The native package manager owns that bootstrap,
    // so the missing managed .env parent must not disable the first connect.
    if native == ApplicationConnectorStatus::NotDetected {
        return ApplicationConnectorStatus::NotDetected;
    }
    if managed == ApplicationConnectorStatus::NotDetected {
        return ApplicationConnectorStatus::Ready;
    }
    ApplicationConnectorStatus::Ready
}

fn combine_details(first: Option<String>, second: Option<String>) -> Option<String> {
    let mut details = [first, second]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    details.dedup();
    (!details.is_empty()).then(|| details.join("; "))
}

fn combine_target_paths(first: Vec<String>, second: Vec<String>) -> Vec<String> {
    first
        .into_iter()
        .chain(second)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn combined_dsh_fingerprint(managed: &str, native: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"ocg-manager-dsh-combined-preview-v1\0");
    hash.update(managed.as_bytes());
    hash.update([0]);
    hash.update(native.as_bytes());
    format!("{:x}", hash.finalize())
}

fn preview_dsh_combined(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    native_plugins: &NativePluginHost,
    request: &ApplicationConnectorHostRequest,
) -> ApplicationConnectorResult<ApplicationConnectorPreview> {
    let managed = preview(roots, cipher, request)?;
    let native = native_plugins.preview(request)?;
    let status = if request.action == ApplicationConnectorAction::Connect
        && managed.status == ApplicationConnectorStatus::Connected
        && native.status == ApplicationConnectorStatus::Connected
    {
        ApplicationConnectorStatus::Connected
    } else {
        ApplicationConnectorStatus::Ready
    };
    Ok(ApplicationConnectorPreview {
        id: ApplicationConnectorId::Dsh,
        action: request.action,
        status,
        fingerprint: combined_dsh_fingerprint(&managed.fingerprint, &native.fingerprint),
        detail: combine_details(managed.detail, native.detail),
        target_paths: combine_target_paths(managed.target_paths, native.target_paths),
        changes: managed.changes.into_iter().chain(native.changes).collect(),
    })
}

fn commit_dsh_combined(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    native_plugins: &NativePluginHost,
    request: &ApplicationConnectorHostRequest,
) -> ApplicationConnectorResult<ApplicationConnectorCommitResult> {
    let expected = request
        .preview_fingerprint
        .as_deref()
        .ok_or_else(|| invalid("preview fingerprint is required"))?;
    let managed_preview = preview(roots, cipher, request)?;
    let native_preview = native_plugins.preview(request)?;
    if combined_dsh_fingerprint(&managed_preview.fingerprint, &native_preview.fingerprint)
        != expected
    {
        return Err(conflict("DSH connection state changed since preview"));
    }

    let changed = match request.action {
        ApplicationConnectorAction::Connect => {
            // Let DSH initialize a first-time profile before writing the one
            // field-owned .env assignment. If the managed write then fails,
            // remove only the package installed by this operation.
            let native_compensation = native_plugins.prepare_dsh_connect_compensation(request)?;
            let native =
                native_plugins.commit_with_fingerprint(request, &native_preview.fingerprint)?;
            match commit_with_fingerprint(roots, cipher, request, &managed_preview.fingerprint) {
                Ok(managed) => managed.changed || native.changed,
                Err(managed_error) => {
                    if native.changed {
                        let rollback =
                            native_plugins.restore_dsh_installation(&native_compensation);
                        if let Err(rollback_error) = rollback {
                            return Err(internal_message(&format!(
                                "DSH credential setup failed and its plugin installation could not be restored: {managed_error}; {rollback_error}"
                            )));
                        }
                    }
                    return Err(managed_error);
                }
            }
        }
        ApplicationConnectorAction::Restore => {
            let native =
                native_plugins.commit_with_fingerprint(request, &native_preview.fingerprint)?;
            let managed =
                commit_with_fingerprint(roots, cipher, request, &managed_preview.fingerprint)?;
            native.changed || managed.changed
        }
    };

    Ok(ApplicationConnectorCommitResult {
        inspection: inspect_dsh_combined(roots, &request.data_dir, cipher, native_plugins),
        changed,
    })
}

fn inspect(
    roots: &Roots,
    data_dir: &Path,
    cipher: &dyn KeyCipher,
    id: ApplicationConnectorId,
) -> ApplicationConnectorInspection {
    let targets = match targets(roots, id) {
        Ok(targets) => targets,
        Err(error) => {
            return info(
                id,
                ApplicationConnectorStatus::ManualOnly,
                false,
                false,
                Some(error.to_string()),
                &[],
            );
        }
    };
    let detected = targets.iter().any(target_detected);
    if !is_automatic(id) {
        return info(
            id,
            if detected {
                ApplicationConnectorStatus::ManualOnly
            } else {
                ApplicationConnectorStatus::NotDetected
            },
            false,
            detected,
            Some(manual_detail(id).to_string()),
            &targets,
        );
    }

    if journal_path(data_dir, id).exists() && recover_journal(roots, data_dir, cipher, id).is_err()
    {
        return info(
            id,
            ApplicationConnectorStatus::Partial,
            true,
            detected,
            Some("an interrupted connector transaction could not be compensated".into()),
            &targets,
        );
    }

    match load_state(roots, data_dir, cipher, id) {
        Err(error) => info(
            id,
            ApplicationConnectorStatus::Conflict,
            true,
            detected,
            Some(error.to_string()),
            &targets,
        ),
        Ok(Some(state)) => match state_matches(roots, cipher, id, &state) {
            Ok(true) => info(
                id,
                ApplicationConnectorStatus::Connected,
                true,
                true,
                None,
                &targets,
            ),
            Ok(false) => info(
                id,
                ApplicationConnectorStatus::Conflict,
                true,
                detected,
                Some("connector-owned fields changed outside OCG Manager".into()),
                &targets,
            ),
            Err(error) => info(
                id,
                ApplicationConnectorStatus::Conflict,
                true,
                detected,
                Some(error.to_string()),
                &targets,
            ),
        },
        Ok(None) => {
            if !detected {
                return info(
                    id,
                    ApplicationConnectorStatus::NotDetected,
                    true,
                    false,
                    Some("application configuration directory was not detected".into()),
                    &targets,
                );
            }
            match validate_unmanaged_targets(roots, id, &targets) {
                Ok(()) => info(
                    id,
                    ApplicationConnectorStatus::Ready,
                    true,
                    true,
                    None,
                    &targets,
                ),
                Err(error) => info(
                    id,
                    ApplicationConnectorStatus::ManualOnly,
                    false,
                    true,
                    Some(error.to_string()),
                    &targets,
                ),
            }
        }
    }
}

fn info(
    id: ApplicationConnectorId,
    status: ApplicationConnectorStatus,
    automatic: bool,
    detected: bool,
    detail: Option<String>,
    targets: &[Target],
) -> ApplicationConnectorInspection {
    ApplicationConnectorInspection {
        id,
        status,
        automatic,
        detected,
        detail,
        target_paths: targets
            .iter()
            .map(|target| target.label.to_string())
            .collect(),
    }
}

fn preview(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    request: &ApplicationConnectorHostRequest,
) -> ApplicationConnectorResult<ApplicationConnectorPreview> {
    let inspection = inspect(roots, &request.data_dir, cipher, request.id);
    if !inspection.automatic {
        return Ok(ApplicationConnectorPreview {
            id: request.id,
            action: request.action,
            status: inspection.status,
            fingerprint: manual_fingerprint(request),
            detail: inspection.detail,
            target_paths: inspection.target_paths,
            changes: Vec::new(),
        });
    }
    match inspection.status {
        ApplicationConnectorStatus::NotDetected if request.id != ApplicationConnectorId::Dsh => {
            return Err(precondition("application was not detected"));
        }
        ApplicationConnectorStatus::Conflict | ApplicationConnectorStatus::Partial => {
            return Err(conflict("connector state or owned fields are in conflict"));
        }
        _ => {}
    }
    let plan = build_plan(roots, cipher, request)?;
    let status = if request.action == ApplicationConnectorAction::Connect && !plan.changed() {
        ApplicationConnectorStatus::Connected
    } else {
        ApplicationConnectorStatus::Ready
    };
    Ok(ApplicationConnectorPreview {
        id: request.id,
        action: request.action,
        status,
        fingerprint: plan_fingerprint(request, &plan),
        detail: None,
        target_paths: inspection.target_paths,
        changes: plan.changes,
    })
}

fn commit(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    request: &ApplicationConnectorHostRequest,
) -> ApplicationConnectorResult<ApplicationConnectorCommitResult> {
    if !is_automatic(request.id) {
        return Err(precondition(manual_detail(request.id)));
    }
    let expected = request
        .preview_fingerprint
        .as_deref()
        .ok_or_else(|| invalid("preview fingerprint is required"))?;
    commit_with_fingerprint(roots, cipher, request, expected)
}

fn commit_with_fingerprint(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    request: &ApplicationConnectorHostRequest,
    expected: &str,
) -> ApplicationConnectorResult<ApplicationConnectorCommitResult> {
    let plan = build_plan(roots, cipher, request)?;
    if plan_fingerprint(request, &plan) != expected {
        return Err(conflict("configuration changed since preview"));
    }
    if !plan.changed() {
        return Ok(ApplicationConnectorCommitResult {
            inspection: inspect(roots, &request.data_dir, cipher, request.id),
            changed: false,
        });
    }
    apply_transaction(roots, &request.data_dir, cipher, &plan, None)?;
    Ok(ApplicationConnectorCommitResult {
        inspection: inspect(roots, &request.data_dir, cipher, request.id),
        changed: true,
    })
}

fn is_automatic(id: ApplicationConnectorId) -> bool {
    matches!(
        id,
        ApplicationConnectorId::ClaudeCode
            | ApplicationConnectorId::Codex
            | ApplicationConnectorId::Dsh
            | ApplicationConnectorId::GeminiCli
            | ApplicationConnectorId::OpenCode
            | ApplicationConnectorId::OpenClaw
            | ApplicationConnectorId::Hermes
    )
}

fn manual_detail(_id: ApplicationConnectorId) -> &'static str {
    "this connector requires manual configuration"
}

fn manual_fingerprint(request: &ApplicationConnectorHostRequest) -> String {
    let mut hash = Sha256::new();
    hash.update(b"manual-only");
    hash.update(connector_slug(request.id).as_bytes());
    hash.update(format!("{:?}", request.action).as_bytes());
    format!("{:x}", hash.finalize())
}

fn targets(roots: &Roots, id: ApplicationConnectorId) -> ApplicationConnectorResult<Vec<Target>> {
    let json = DocumentFormat::Json;
    let env = DocumentFormat::Env;
    let toml = DocumentFormat::Toml;
    let yaml = DocumentFormat::Yaml;
    match id {
        ApplicationConnectorId::ClaudeCode => Ok(vec![Target {
            id: "claude-settings",
            path: roots.home.join(".claude/settings.json"),
            label: "~/.claude/settings.json",
            format: json,
        }]),
        ApplicationConnectorId::Codex => Ok(vec![Target {
            id: "codex-config",
            path: roots
                .codex_home
                .clone()
                .unwrap_or_else(|| roots.home.join(".codex"))
                .join("config.toml"),
            label: "Codex config.toml",
            format: toml,
        }]),
        ApplicationConnectorId::GeminiCli => Ok(vec![
            Target {
                id: "gemini-settings",
                path: roots.home.join(".gemini/settings.json"),
                label: "~/.gemini/settings.json",
                format: json,
            },
            Target {
                id: "gemini-env",
                path: roots.home.join(".gemini/.env"),
                label: "~/.gemini/.env",
                format: env,
            },
        ]),
        ApplicationConnectorId::OpenCode => Ok(vec![
            Target {
                id: "opencode-settings",
                path: roots.home.join(".config/opencode/opencode.json"),
                label: "~/.config/opencode/opencode.json",
                format: json,
            },
            Target {
                id: "opencode-env",
                path: roots.home.join(".config/opencode/.env"),
                label: "~/.config/opencode/.env",
                format: env,
            },
        ]),
        ApplicationConnectorId::OpenClaw => Ok(vec![
            Target {
                id: "openclaw-settings",
                path: roots.home.join(".openclaw/openclaw.json"),
                label: "~/.openclaw/openclaw.json",
                format: json,
            },
            Target {
                id: "openclaw-env",
                path: roots.home.join(".openclaw/.env"),
                label: "~/.openclaw/.env",
                format: env,
            },
        ]),
        ApplicationConnectorId::Hermes => {
            let root = hermes_root(roots)?;
            Ok(vec![
                Target {
                    id: "hermes-config",
                    path: root.join("config.yaml"),
                    label: "Hermes config.yaml",
                    format: yaml,
                },
                Target {
                    id: "hermes-env",
                    path: root.join(".env"),
                    label: "Hermes .env",
                    format: env,
                },
            ])
        }
        ApplicationConnectorId::Dsh => Ok(vec![Target {
            id: "dsh-env",
            path: roots
                .dsh_home
                .clone()
                .unwrap_or_else(|| roots.home.join(".dsh"))
                .join(".env"),
            label: "DSH .env",
            format: env,
        }]),
        ApplicationConnectorId::Pi => Err(invalid(
            "Pi requests must be handled by the native plugin host",
        )),
    }
}

fn desired_documents(
    roots: &Roots,
    request: &ApplicationConnectorHostRequest,
) -> ApplicationConnectorResult<Vec<DesiredDocument>> {
    let secret = request
        .secret
        .as_ref()
        .map(ApplicationConnectorSecret::expose_to_host)
        .ok_or_else(|| invalid("an enabled access key is required"))?;
    let target_map = targets(roots, request.id)?
        .into_iter()
        .map(|target| (target.id, target))
        .collect::<BTreeMap<_, _>>();
    let target = |id: &str| {
        target_map
            .get(id)
            .cloned()
            .ok_or_else(|| internal_message("connector target registry is incomplete"))
    };
    let primary = primary_model(&request.model_values);
    let models = selected_models(&request.model_values, &primary);
    let json_field = |locator: &str, value: Value, sensitive: bool| DesiredField {
        locator: locator.to_string(),
        value: DesiredValue::Json(value),
        sensitive,
    };
    let env_field = |key: &str, value: String, sensitive: bool| DesiredField {
        locator: key.to_string(),
        value: DesiredValue::Env(value),
        sensitive,
    };
    let toml_field = |locator: &str, value: String, sensitive: bool| DesiredField {
        locator: locator.to_string(),
        value: DesiredValue::Toml(value),
        sensitive,
    };
    let yaml_field = |locator: &str, value: YamlValue, sensitive: bool| DesiredField {
        locator: locator.to_string(),
        value: DesiredValue::Yaml(value),
        sensitive,
    };

    match request.id {
        ApplicationConnectorId::ClaudeCode => {
            let model_value = |name: &str| {
                request
                    .model_values
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| primary.clone())
            };
            let mut fields = vec![
                json_field("/env/ANTHROPIC_BASE_URL", json!(request.gateway_url), false),
                json_field("/env/ANTHROPIC_AUTH_TOKEN", json!(secret), true),
            ];
            for name in [
                "ANTHROPIC_MODEL",
                "ANTHROPIC_DEFAULT_FABLE_MODEL",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                "ANTHROPIC_DEFAULT_SONNET_MODEL",
                "ANTHROPIC_DEFAULT_OPUS_MODEL",
                "CLAUDE_CODE_SUBAGENT_MODEL",
                "ANTHROPIC_CUSTOM_MODEL_OPTION",
            ] {
                fields.push(json_field(
                    &format!("/env/{name}"),
                    json!(model_value(name)),
                    false,
                ));
            }
            fields.push(json_field(
                "/model",
                json!(model_value("ANTHROPIC_MODEL")),
                false,
            ));
            Ok(vec![DesiredDocument {
                target: target("claude-settings")?,
                fields,
            }])
        }
        ApplicationConnectorId::Codex => Ok(vec![DesiredDocument {
            target: target("codex-config")?,
            fields: vec![
                toml_field("model", primary.clone(), false),
                toml_field("model_provider", "ocg_manager".into(), false),
                toml_field(
                    "model_providers.ocg_manager.name",
                    "OCG Manager".into(),
                    false,
                ),
                toml_field(
                    "model_providers.ocg_manager.base_url",
                    format!("{}/v1", request.gateway_url),
                    false,
                ),
                toml_field(
                    "model_providers.ocg_manager.wire_api",
                    "responses".into(),
                    false,
                ),
                toml_field(
                    "model_providers.ocg_manager.experimental_bearer_token",
                    secret.to_string(),
                    true,
                ),
            ],
        }]),
        ApplicationConnectorId::GeminiCli => {
            let overrides = [
                "codebase_investigator",
                "cli_help",
                "generalist",
                "browser_agent",
            ]
            .into_iter()
            .map(|name| (name.to_string(), json!({"modelConfig":{"model":primary}})))
            .collect::<Map<_, _>>();
            Ok(vec![
                DesiredDocument {
                    target: target("gemini-settings")?,
                    fields: vec![
                        json_field("/model/name", json!(primary), false),
                        json_field(
                            "/modelConfigs/customOverrides",
                            json!([{"match":{"overrideScope":"core"},"modelConfig":{"model":primary}}]),
                            false,
                        ),
                        json_field("/agents/overrides", Value::Object(overrides), false),
                    ],
                },
                DesiredDocument {
                    target: target("gemini-env")?,
                    fields: vec![
                        env_field("GEMINI_API_KEY", secret.to_string(), true),
                        env_field("GOOGLE_GEMINI_BASE_URL", request.gateway_url.clone(), false),
                        env_field("GOOGLE_GENAI_API_VERSION", "v1beta".into(), false),
                    ],
                },
            ])
        }
        ApplicationConnectorId::OpenCode => {
            let model_map = models
                .iter()
                .map(|model| (model.clone(), json!({"name":model})))
                .collect::<Map<_, _>>();
            Ok(vec![
                DesiredDocument {
                    target: target("opencode-settings")?,
                    fields: vec![
                        json_field(
                            "/provider/ocg",
                            json!({"npm":"@ai-sdk/openai-compatible","name":"OCG Manager","options":{"baseURL":format!("{}/v1", request.gateway_url),"apiKey":"{env:OCG_API_KEY}"},"models":model_map}),
                            false,
                        ),
                        json_field("/model", json!(format!("ocg/{primary}")), false),
                    ],
                },
                DesiredDocument {
                    target: target("opencode-env")?,
                    fields: vec![env_field("OCG_API_KEY", secret.to_string(), true)],
                },
            ])
        }
        ApplicationConnectorId::OpenClaw => {
            let model_rows = models
                .iter()
                .map(|model| json!({"id":model,"name":model}))
                .collect::<Vec<_>>();
            Ok(vec![
                DesiredDocument {
                    target: target("openclaw-settings")?,
                    fields: vec![
                        json_field("/models/mode", json!("merge"), false),
                        json_field(
                            "/models/providers/ocg",
                            json!({"baseUrl":format!("{}/v1",request.gateway_url),"apiKey":"${CUSTOM_API_KEY}","api":"openai-completions","models":model_rows}),
                            false,
                        ),
                        json_field(
                            "/agents/defaults/model/primary",
                            json!(format!("ocg/{primary}")),
                            false,
                        ),
                    ],
                },
                DesiredDocument {
                    target: target("openclaw-env")?,
                    fields: vec![env_field("CUSTOM_API_KEY", secret.to_string(), true)],
                },
            ])
        }
        ApplicationConnectorId::Hermes => Ok(vec![
            DesiredDocument {
                target: target("hermes-config")?,
                fields: vec![
                    yaml_field("model.default", YamlValue::String(primary), false),
                    yaml_field("model.provider", YamlValue::String("custom".into()), false),
                    yaml_field(
                        "model.base_url",
                        YamlValue::String(format!("{}/v1", request.gateway_url)),
                        false,
                    ),
                    yaml_field(
                        "model.api_key",
                        YamlValue::String("${OCG_MANAGER_API_KEY}".into()),
                        false,
                    ),
                ],
            },
            DesiredDocument {
                target: target("hermes-env")?,
                fields: vec![env_field("OCG_MANAGER_API_KEY", secret.to_string(), true)],
            },
        ]),
        ApplicationConnectorId::Dsh => Ok(vec![DesiredDocument {
            target: target("dsh-env")?,
            fields: vec![env_field("OCG_MANAGER_API_KEY", secret.to_string(), true)],
        }]),
        ApplicationConnectorId::Pi => Err(invalid(
            "Pi requests must be handled by the native plugin host",
        )),
    }
}

fn primary_model(values: &BTreeMap<String, String>) -> String {
    values
        .get("model")
        .or_else(|| values.get("ANTHROPIC_MODEL"))
        .or_else(|| values.values().next())
        .cloned()
        .unwrap_or_else(|| "gpt-5.6".into())
}

fn selected_models(values: &BTreeMap<String, String>, fallback: &str) -> Vec<String> {
    let mut models = values
        .get("models")
        .map(|raw| {
            raw.lines()
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if models.is_empty() {
        models.push(fallback.to_string());
    }
    models.sort();
    models.dedup();
    models
}

fn build_plan(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    request: &ApplicationConnectorHostRequest,
) -> ApplicationConnectorResult<MutationPlan> {
    let old_state = load_state(roots, &request.data_dir, cipher, request.id)?;
    match request.action {
        ApplicationConnectorAction::Connect => {
            let desired = desired_documents(roots, request)?;
            build_connect_plan(roots, cipher, request, old_state.as_ref(), desired)
        }
        ApplicationConnectorAction::Restore => {
            let state = old_state
                .as_ref()
                .ok_or_else(|| precondition("no managed connector state exists"))?;
            if !state_matches(roots, cipher, request.id, state)? {
                return Err(conflict("connector-owned fields changed"));
            }
            build_restore_plan(roots, cipher, request, state)
        }
    }
}

fn build_connect_plan(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    request: &ApplicationConnectorHostRequest,
    old_state: Option<&StateV1>,
    desired_documents: Vec<DesiredDocument>,
) -> ApplicationConnectorResult<MutationPlan> {
    if let Some(state) = old_state {
        if !state_matches(roots, cipher, request.id, state)? {
            return Err(conflict("connector-owned fields changed"));
        }
    }
    validate_desired_documents(roots, request.id, &desired_documents)?;

    let old_documents = old_state
        .map(|state| {
            state
                .documents
                .iter()
                .map(|document| (document.document_id.as_str(), document))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut next_documents = Vec::new();
    let mut files = Vec::new();
    let mut changes = Vec::new();

    for desired in desired_documents {
        let before = read_target(roots, request.id, &desired.target)?;
        let prior = old_documents.get(desired.target.id).copied();
        if let Some(prior) = prior {
            if prior.format != desired.target.format {
                return Err(conflict("connector state format changed"));
            }
        }
        let (after_bytes, document_state, mut document_changes) = match desired.target.format {
            DocumentFormat::Json => patch_json_connect(
                cipher,
                &before,
                prior,
                &desired.fields,
                desired.target.label,
            )?,
            DocumentFormat::Env => patch_env_connect(
                cipher,
                &before,
                prior,
                &desired.fields,
                desired.target.label,
            )?,
            DocumentFormat::Toml => patch_toml_connect(
                cipher,
                &before,
                prior,
                &desired.fields,
                desired.target.label,
            )?,
            DocumentFormat::Yaml => patch_yaml_connect(
                cipher,
                &before,
                prior,
                &desired.fields,
                desired.target.label,
            )?,
        };
        changes.append(&mut document_changes);
        next_documents.push(DocumentState {
            document_id: desired.target.id.to_string(),
            format: desired.target.format,
            originally_existed: prior
                .map(|state| state.originally_existed)
                .unwrap_or(before.existed),
            fields: document_state,
        });
        files.push(PlannedFile {
            document_id: desired.target.id.to_string(),
            path: desired.target.path,
            format: Some(desired.target.format),
            before,
            after: FileImage {
                existed: true,
                bytes: after_bytes,
            },
        });
    }

    let next_state = finalize_state(StateV1 {
        version: STATE_VERSION,
        connector_id: connector_slug(request.id).to_string(),
        documents: next_documents,
        integrity_sha256: String::new(),
    })?;
    let state_target = state_target(&request.data_dir, request.id);
    let state_before = read_internal_file(&state_target.path)?;
    let state_after = serde_json::to_vec_pretty(&next_state).map_err(internal)?;
    files.push(PlannedFile {
        document_id: state_target.id.to_string(),
        path: state_target.path,
        format: None,
        before: state_before,
        after: FileImage {
            existed: true,
            bytes: state_after,
        },
    });
    Ok(MutationPlan {
        id: request.id,
        files,
        changes,
    })
}

fn build_restore_plan(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    request: &ApplicationConnectorHostRequest,
    state: &StateV1,
) -> ApplicationConnectorResult<MutationPlan> {
    validate_state_documents(roots, request.id, state)?;
    let registry = targets(roots, request.id)?
        .into_iter()
        .map(|target| (target.id, target))
        .collect::<BTreeMap<_, _>>();
    let mut files = Vec::new();
    let mut changes = Vec::new();
    for document in &state.documents {
        let target = registry
            .get(document.document_id.as_str())
            .ok_or_else(|| conflict("connector state contains an unknown target"))?;
        let before = read_target(roots, request.id, target)?;
        let (bytes, is_empty, mut document_changes) = match document.format {
            DocumentFormat::Json => {
                let (bytes, empty, changes) =
                    patch_json_restore(cipher, &before, document, target.label)?;
                (bytes, empty, changes)
            }
            DocumentFormat::Env => {
                let (bytes, empty, changes) =
                    patch_env_restore(cipher, &before, document, target.label)?;
                (bytes, empty, changes)
            }
            DocumentFormat::Toml => {
                let (bytes, empty, changes) =
                    patch_toml_restore(cipher, &before, document, target.label)?;
                (bytes, empty, changes)
            }
            DocumentFormat::Yaml => {
                let (bytes, empty, changes) =
                    patch_yaml_restore(cipher, &before, document, target.label)?;
                (bytes, empty, changes)
            }
        };
        changes.append(&mut document_changes);
        let after_existed = document.originally_existed || !is_empty;
        files.push(PlannedFile {
            document_id: target.id.to_string(),
            path: target.path.clone(),
            format: Some(target.format),
            before,
            after: FileImage {
                existed: after_existed,
                // Missing-file images have one canonical byte representation.
                // This keeps journal digests, recovery comparisons, and the
                // post-write byte verification consistent when restore removes
                // a file that was originally absent.
                bytes: if after_existed { bytes } else { Vec::new() },
            },
        });
    }
    let state_target = state_target(&request.data_dir, request.id);
    let state_before = read_internal_file(&state_target.path)?;
    files.push(PlannedFile {
        document_id: state_target.id.to_string(),
        path: state_target.path,
        format: None,
        before: state_before,
        after: FileImage {
            existed: false,
            bytes: Vec::new(),
        },
    });
    Ok(MutationPlan {
        id: request.id,
        files,
        changes,
    })
}

fn patch_json_connect(
    cipher: &dyn KeyCipher,
    before: &FileImage,
    prior: Option<&DocumentState>,
    desired: &[DesiredField],
    label: &str,
) -> ApplicationConnectorResult<(Vec<u8>, Vec<FieldState>, Vec<ApplicationConnectorChange>)> {
    let mut root = parse_json_image(before)?;
    let prior_fields = prior
        .map(|document| {
            document
                .fields
                .iter()
                .map(|field| (field.locator.as_str(), field))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut next = Vec::new();
    let mut changes = Vec::new();
    for field in desired {
        let DesiredValue::Json(desired_value) = &field.value else {
            return Err(internal_message("invalid JSON connector field"));
        };
        let current = json_pointer(&root, &field.locator).cloned();
        let state = if let Some(existing) = prior_fields.get(field.locator.as_str()) {
            let mut retained = (*existing).clone();
            retained.applied = store_applied(&canonical_json(desired_value)?, field.sensitive);
            retained
        } else {
            FieldState {
                locator: field.locator.clone(),
                sensitive: field.sensitive,
                original: store_original(
                    cipher,
                    current.as_ref().map(canonical_json).transpose()?.as_deref(),
                    field.sensitive,
                )?,
                applied: store_applied(&canonical_json(desired_value)?, field.sensitive),
                ancestors: capture_ancestors(&root, &field.locator)?,
            }
        };
        if current.as_ref() != Some(desired_value) || prior.is_none() {
            changes.push(preview_change(
                format!("{label}{}", field.locator),
                current.as_ref().map(canonical_json).transpose()?.as_deref(),
                Some(&canonical_json(desired_value)?),
                field.sensitive,
            ));
        }
        set_json_pointer(&mut root, &field.locator, desired_value.clone())?;
        next.push(state);
    }
    serialize_json(&root).map(|bytes| (bytes, next, changes))
}

fn patch_json_restore(
    cipher: &dyn KeyCipher,
    before: &FileImage,
    state: &DocumentState,
    label: &str,
) -> ApplicationConnectorResult<(Vec<u8>, bool, Vec<ApplicationConnectorChange>)> {
    let mut root = parse_json_image(before)?;
    let mut changes = Vec::new();
    for field in &state.fields {
        let current = json_pointer(&root, &field.locator)
            .cloned()
            .ok_or_else(|| conflict("connector-owned JSON field is missing"))?;
        let original = load_original(cipher, &field.original)?;
        let original_value = original
            .as_deref()
            .map(serde_json::from_str::<Value>)
            .transpose()
            .map_err(|_| conflict("connector JSON baseline is corrupt"))?;
        changes.push(preview_change(
            format!("{label}{}", field.locator),
            Some(&canonical_json(&current)?),
            original.as_deref(),
            field.sensitive,
        ));
        match original_value {
            Some(value) => set_json_pointer(&mut root, &field.locator, value)?,
            None => remove_json_pointer(&mut root, &field.locator)?,
        }
    }
    prune_created_ancestors(&mut root, &state.fields)?;
    let empty = root.as_object().is_some_and(Map::is_empty);
    serialize_json(&root).map(|bytes| (bytes, empty, changes))
}

fn patch_env_connect(
    cipher: &dyn KeyCipher,
    before: &FileImage,
    prior: Option<&DocumentState>,
    desired: &[DesiredField],
    label: &str,
) -> ApplicationConnectorResult<(Vec<u8>, Vec<FieldState>, Vec<ApplicationConnectorChange>)> {
    let text = parse_utf8_image(before)?;
    let mut env = EnvDocument::parse(&text)?;
    let prior_fields = prior
        .map(|document| {
            document
                .fields
                .iter()
                .map(|field| (field.locator.as_str(), field))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut next = Vec::new();
    let mut changes = Vec::new();
    for field in desired {
        let DesiredValue::Env(desired_value) = &field.value else {
            return Err(internal_message("invalid environment connector field"));
        };
        let current_line = env.assignment(&field.locator).map(str::to_string);
        let applied_line = format!("{}={}", field.locator, quote_env(desired_value));
        let state = if let Some(existing) = prior_fields.get(field.locator.as_str()) {
            let mut retained = (*existing).clone();
            retained.applied = store_applied(&applied_line, field.sensitive);
            retained
        } else {
            FieldState {
                locator: field.locator.clone(),
                sensitive: field.sensitive,
                original: store_original(cipher, current_line.as_deref(), field.sensitive)?,
                applied: store_applied(&applied_line, field.sensitive),
                ancestors: Vec::new(),
            }
        };
        if current_line.as_deref() != Some(applied_line.as_str()) || prior.is_none() {
            changes.push(preview_change(
                format!("{label}:{}", field.locator),
                current_line.as_deref(),
                Some(&applied_line),
                field.sensitive,
            ));
        }
        env.set(&field.locator, &applied_line)?;
        next.push(state);
    }
    Ok((env.render().into_bytes(), next, changes))
}

fn patch_env_restore(
    cipher: &dyn KeyCipher,
    before: &FileImage,
    state: &DocumentState,
    label: &str,
) -> ApplicationConnectorResult<(Vec<u8>, bool, Vec<ApplicationConnectorChange>)> {
    let text = parse_utf8_image(before)?;
    let mut env = EnvDocument::parse(&text)?;
    let mut changes = Vec::new();
    for field in &state.fields {
        let current = env
            .assignment(&field.locator)
            .map(str::to_string)
            .ok_or_else(|| conflict("connector-owned environment field is missing"))?;
        let original = load_original(cipher, &field.original)?;
        changes.push(preview_change(
            format!("{label}:{}", field.locator),
            Some(&current),
            original.as_deref(),
            field.sensitive,
        ));
        match original {
            Some(line) => env.set(&field.locator, &line)?,
            None => env.remove(&field.locator)?,
        }
    }
    let rendered = env.render();
    let empty = rendered.trim().is_empty();
    Ok((rendered.into_bytes(), empty, changes))
}

fn patch_toml_connect(
    cipher: &dyn KeyCipher,
    before: &FileImage,
    prior: Option<&DocumentState>,
    desired: &[DesiredField],
    label: &str,
) -> ApplicationConnectorResult<(Vec<u8>, Vec<FieldState>, Vec<ApplicationConnectorChange>)> {
    let mut document = parse_toml_image(before)?;
    let prior_fields = prior
        .map(|document| {
            document
                .fields
                .iter()
                .map(|field| (field.locator.as_str(), field))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut next = Vec::new();
    let mut changes = Vec::new();
    for field in desired {
        let DesiredValue::Toml(desired_value) = &field.value else {
            return Err(internal_message("invalid TOML connector field"));
        };
        let current = toml_string_at(document.as_table(), &field.locator)?;
        let state = if let Some(existing) = prior_fields.get(field.locator.as_str()) {
            let mut retained = (*existing).clone();
            retained.applied = store_applied(desired_value, field.sensitive);
            retained
        } else {
            FieldState {
                locator: field.locator.clone(),
                sensitive: field.sensitive,
                original: store_original(cipher, current.as_deref(), field.sensitive)?,
                applied: store_applied(desired_value, field.sensitive),
                ancestors: capture_toml_ancestors(document.as_table(), &field.locator)?,
            }
        };
        if current.as_deref() != Some(desired_value.as_str()) || prior.is_none() {
            changes.push(preview_change(
                format!("{label}:{}", field.locator),
                current.as_deref(),
                Some(desired_value),
                field.sensitive,
            ));
        }
        toml_set_string(document.as_table_mut(), &field.locator, desired_value)?;
        next.push(state);
    }
    Ok((document.to_string().into_bytes(), next, changes))
}

fn patch_toml_restore(
    cipher: &dyn KeyCipher,
    before: &FileImage,
    state: &DocumentState,
    label: &str,
) -> ApplicationConnectorResult<(Vec<u8>, bool, Vec<ApplicationConnectorChange>)> {
    let mut document = parse_toml_image(before)?;
    let mut changes = Vec::new();
    for field in &state.fields {
        let current = toml_string_at(document.as_table(), &field.locator)?
            .ok_or_else(|| conflict("connector-owned TOML field is missing"))?;
        let original = load_original(cipher, &field.original)?;
        changes.push(preview_change(
            format!("{label}:{}", field.locator),
            Some(&current),
            original.as_deref(),
            field.sensitive,
        ));
        match original {
            Some(value) => toml_set_string(document.as_table_mut(), &field.locator, &value)?,
            None => toml_remove(document.as_table_mut(), &field.locator)?,
        }
    }
    prune_toml_ancestors(document.as_table_mut(), &state.fields)?;
    let empty = document.as_table().is_empty();
    Ok((document.to_string().into_bytes(), empty, changes))
}

fn parse_toml_image(image: &FileImage) -> ApplicationConnectorResult<toml_edit::DocumentMut> {
    if !image.existed || image.bytes.is_empty() {
        return Ok(toml_edit::DocumentMut::new());
    }
    parse_utf8_image(image)?
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| precondition("connector TOML is malformed or contains duplicate keys"))
}

fn dotted_segments(locator: &str) -> ApplicationConnectorResult<Vec<&str>> {
    let segments = locator.split('.').collect::<Vec<_>>();
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| segment.is_empty() || !valid_toml_key(segment))
    {
        return Err(invalid("invalid dotted connector locator"));
    }
    Ok(segments)
}

fn valid_toml_key(key: &str) -> bool {
    key.chars()
        .all(|character| character == '_' || character == '-' || character.is_ascii_alphanumeric())
        && !key.is_empty()
}

fn toml_item_at<'a>(table: &'a toml_edit::Table, segments: &[&str]) -> Option<&'a toml_edit::Item> {
    let item = table.get(segments.first()?);
    if segments.len() == 1 {
        return item;
    }
    item?
        .as_table()
        .and_then(|nested| toml_item_at(nested, &segments[1..]))
}

fn toml_string_at(
    table: &toml_edit::Table,
    locator: &str,
) -> ApplicationConnectorResult<Option<String>> {
    let segments = dotted_segments(locator)?;
    match toml_item_at(table, &segments) {
        None => Ok(None),
        Some(item) => item
            .as_value()
            .and_then(toml_edit::Value::as_str)
            .map(str::to_string)
            .map(Some)
            .ok_or_else(|| conflict("connector TOML field is not a string")),
    }
}

fn toml_set_string(
    table: &mut toml_edit::Table,
    locator: &str,
    value: &str,
) -> ApplicationConnectorResult<()> {
    let segments = dotted_segments(locator)?;
    toml_set_segments(table, &segments, value)
}

fn toml_set_segments(
    table: &mut toml_edit::Table,
    segments: &[&str],
    value: &str,
) -> ApplicationConnectorResult<()> {
    let (head, tail) = segments
        .split_first()
        .ok_or_else(|| invalid("invalid dotted connector locator"))?;
    if tail.is_empty() {
        table.insert(head, toml_edit::value(value));
        return Ok(());
    }
    if table.get(head).is_none() {
        table.insert(head, toml_edit::Item::Table(toml_edit::Table::new()));
    }
    let nested = table
        .get_mut(head)
        .and_then(toml_edit::Item::as_table_mut)
        .ok_or_else(|| conflict("connector TOML field has a non-table parent"))?;
    toml_set_segments(nested, tail, value)
}

fn toml_remove(table: &mut toml_edit::Table, locator: &str) -> ApplicationConnectorResult<()> {
    let segments = dotted_segments(locator)?;
    toml_remove_segments(table, &segments)
}

fn toml_remove_segments(
    table: &mut toml_edit::Table,
    segments: &[&str],
) -> ApplicationConnectorResult<()> {
    let (head, tail) = segments
        .split_first()
        .ok_or_else(|| invalid("invalid dotted connector locator"))?;
    if tail.is_empty() {
        table
            .remove(head)
            .ok_or_else(|| conflict("connector-owned TOML field is missing"))?;
        return Ok(());
    }
    let nested = table
        .get_mut(head)
        .and_then(toml_edit::Item::as_table_mut)
        .ok_or_else(|| conflict("connector-owned TOML field parent is missing"))?;
    toml_remove_segments(nested, tail)
}

fn capture_toml_ancestors(
    table: &toml_edit::Table,
    locator: &str,
) -> ApplicationConnectorResult<Vec<AncestorState>> {
    let segments = dotted_segments(locator)?;
    let mut current = table;
    let mut ancestors = Vec::new();
    for index in 0..segments.len().saturating_sub(1) {
        match current.get(segments[index]) {
            None => {
                for end in index..segments.len() - 1 {
                    ancestors.push(AncestorState {
                        pointer: segments[..=end].join("."),
                        originally_existed: false,
                    });
                }
                break;
            }
            Some(item) => {
                current = item
                    .as_table()
                    .ok_or_else(|| conflict("connector TOML field has a non-table parent"))?;
            }
        }
    }
    Ok(ancestors)
}

fn prune_toml_ancestors(
    table: &mut toml_edit::Table,
    fields: &[FieldState],
) -> ApplicationConnectorResult<()> {
    let mut ancestors = fields
        .iter()
        .flat_map(|field| field.ancestors.iter())
        .filter(|ancestor| !ancestor.originally_existed)
        .map(|ancestor| ancestor.pointer.clone())
        .collect::<Vec<_>>();
    ancestors.sort_by_key(|pointer| std::cmp::Reverse(pointer.len()));
    ancestors.dedup();
    for locator in ancestors {
        let segments = dotted_segments(&locator)?;
        toml_remove_if_empty(table, &segments);
    }
    Ok(())
}

fn toml_remove_if_empty(table: &mut toml_edit::Table, segments: &[&str]) {
    let Some((head, tail)) = segments.split_first() else {
        return;
    };
    if tail.is_empty() {
        if table
            .get(head)
            .and_then(toml_edit::Item::as_table)
            .is_some_and(toml_edit::Table::is_empty)
        {
            table.remove(head);
        }
        return;
    }
    if let Some(nested) = table.get_mut(head).and_then(toml_edit::Item::as_table_mut) {
        toml_remove_if_empty(nested, tail);
    }
}

fn patch_yaml_connect(
    cipher: &dyn KeyCipher,
    before: &FileImage,
    prior: Option<&DocumentState>,
    desired: &[DesiredField],
    label: &str,
) -> ApplicationConnectorResult<(Vec<u8>, Vec<FieldState>, Vec<ApplicationConnectorChange>)> {
    let mut root = parse_yaml_image(before)?;
    let prior_fields = prior
        .map(|document| {
            document
                .fields
                .iter()
                .map(|field| (field.locator.as_str(), field))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut next = Vec::new();
    let mut changes = Vec::new();
    for field in desired {
        let DesiredValue::Yaml(desired_value) = &field.value else {
            return Err(internal_message("invalid YAML connector field"));
        };
        let current = yaml_value_at(&root, &field.locator)?.cloned();
        let desired_text = canonical_yaml(desired_value)?;
        let current_text = current.as_ref().map(canonical_yaml).transpose()?;
        let state = if let Some(existing) = prior_fields.get(field.locator.as_str()) {
            let mut retained = (*existing).clone();
            retained.applied = store_applied(&desired_text, field.sensitive);
            retained
        } else {
            FieldState {
                locator: field.locator.clone(),
                sensitive: field.sensitive,
                original: store_original(cipher, current_text.as_deref(), field.sensitive)?,
                applied: store_applied(&desired_text, field.sensitive),
                ancestors: capture_yaml_ancestors(&root, &field.locator)?,
            }
        };
        if current.as_ref() != Some(desired_value) || prior.is_none() {
            changes.push(preview_change(
                format!("{label}:{}", field.locator),
                current_text.as_deref(),
                Some(&desired_text),
                field.sensitive,
            ));
        }
        yaml_set(&mut root, &field.locator, desired_value.clone())?;
        next.push(state);
    }
    Ok((render_yaml_model_section(before, &root)?, next, changes))
}

fn patch_yaml_restore(
    cipher: &dyn KeyCipher,
    before: &FileImage,
    state: &DocumentState,
    label: &str,
) -> ApplicationConnectorResult<(Vec<u8>, bool, Vec<ApplicationConnectorChange>)> {
    let mut root = parse_yaml_image(before)?;
    let mut changes = Vec::new();
    for field in &state.fields {
        let current = yaml_value_at(&root, &field.locator)?
            .cloned()
            .ok_or_else(|| conflict("connector-owned YAML field is missing"))?;
        let current_text = canonical_yaml(&current)?;
        let original = load_original(cipher, &field.original)?;
        changes.push(preview_change(
            format!("{label}:{}", field.locator),
            Some(&current_text),
            original.as_deref(),
            field.sensitive,
        ));
        match original {
            Some(value) => yaml_set(
                &mut root,
                &field.locator,
                serde_yaml::from_str::<YamlValue>(&value)
                    .map_err(|_| conflict("connector YAML baseline is corrupt"))?,
            )?,
            None => yaml_remove(&mut root, &field.locator)?,
        }
    }
    prune_yaml_ancestors(&mut root, &state.fields)?;
    let empty = root.as_mapping().is_some_and(YamlMapping::is_empty);
    Ok((render_yaml_model_section(before, &root)?, empty, changes))
}

fn parse_yaml_image(image: &FileImage) -> ApplicationConnectorResult<YamlValue> {
    if !image.existed || image.bytes.is_empty() {
        return Ok(YamlValue::Mapping(YamlMapping::new()));
    }
    let text = parse_utf8_image(image)?;
    if yaml_has_anchor_or_alias(&text) {
        return Err(precondition(
            "connector YAML anchors and aliases are not supported; use the manual guide",
        ));
    }
    let root = serde_yaml::from_str::<YamlValue>(&text)
        .map_err(|_| precondition("connector YAML is malformed or contains duplicate keys"))?;
    if !root.is_mapping() {
        return Err(precondition("connector YAML root must be a mapping"));
    }
    Ok(root)
}

fn yaml_has_anchor_or_alias(text: &str) -> bool {
    let characters = text.chars().collect::<Vec<_>>();
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut in_comment = false;
    let mut escaped = false;
    for (index, character) in characters.iter().copied().enumerate() {
        if character == '\n' {
            in_comment = false;
            escaped = false;
            continue;
        }
        if in_comment {
            continue;
        }
        if double_quoted {
            if character == '"' && !escaped {
                double_quoted = false;
            }
            escaped = character == '\\' && !escaped;
            continue;
        }
        if character == '\'' {
            single_quoted = !single_quoted;
            continue;
        }
        if single_quoted {
            continue;
        }
        if character == '"' {
            double_quoted = true;
            continue;
        }
        let previous = index
            .checked_sub(1)
            .and_then(|position| characters.get(position));
        if character == '#' && previous.is_none_or(|previous| previous.is_whitespace()) {
            in_comment = true;
            continue;
        }
        if matches!(character, '&' | '*')
            && previous.is_none_or(|previous| {
                previous.is_whitespace() || matches!(previous, ':' | '[' | '{' | ',')
            })
            && characters
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_alphanumeric() || matches!(next, '_' | '-'))
        {
            return true;
        }
    }
    false
}

fn render_yaml_model_section(
    before: &FileImage,
    root: &YamlValue,
) -> ApplicationConnectorResult<Vec<u8>> {
    let raw = if before.existed {
        parse_utf8_image(before)?
    } else {
        String::new()
    };
    let model_key = yaml_key("model");
    let model = root
        .as_mapping()
        .and_then(|mapping| mapping.get(&model_key));
    let range = find_yaml_top_level_section(&raw, "model");

    if model.is_some() && !raw.trim().is_empty() && range.is_none() {
        return Err(precondition(
            "connector YAML model section uses an unsupported layout; use the manual guide",
        ));
    }

    let replacement = match model {
        Some(model) => {
            let mut section = YamlMapping::new();
            section.insert(model_key, model.clone());
            let text = serde_yaml::to_string(&YamlValue::Mapping(section)).map_err(internal)?;
            if raw.contains("\r\n") {
                text.replace('\n', "\r\n")
            } else {
                text
            }
        }
        None => String::new(),
    };

    match range {
        Some((start, end)) => {
            if yaml_has_comment(&raw[start..end]) {
                return Err(precondition(
                    "connector YAML model section contains comments; use the manual guide to preserve them",
                ));
            }
            let mut output = String::with_capacity(raw.len() + replacement.len());
            output.push_str(&raw[..start]);
            output.push_str(&replacement);
            output.push_str(&raw[end..]);
            Ok(output.into_bytes())
        }
        None if replacement.is_empty() => Ok(raw.into_bytes()),
        None => {
            let mut output = raw;
            let newline = if output.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            if !output.is_empty() && !output.ends_with('\n') {
                output.push_str(newline);
            }
            output.push_str(&replacement);
            Ok(output.into_bytes())
        }
    }
}

fn find_yaml_top_level_section(raw: &str, key: &str) -> Option<(usize, usize)> {
    let mut start = None;
    let mut offset = 0;
    for line in raw.split_inclusive('\n') {
        let body = line
            .strip_suffix('\n')
            .unwrap_or(line)
            .strip_suffix('\r')
            .unwrap_or(line.strip_suffix('\n').unwrap_or(line));
        if let Some(section_start) = start {
            if is_yaml_top_level_key(body, None) {
                return Some((section_start, offset));
            }
        } else if is_yaml_top_level_key(body, Some(key)) {
            start = Some(offset);
        }
        offset += line.len();
    }
    start.map(|section_start| (section_start, raw.len()))
}

fn is_yaml_top_level_key(line: &str, expected: Option<&str>) -> bool {
    if line.is_empty() || line.starts_with([' ', '\t', '#', '-']) {
        return false;
    }
    let Some((key, remainder)) = line.split_once(':') else {
        return false;
    };
    if !remainder.is_empty() && !remainder.starts_with([' ', '\t']) {
        return false;
    }
    expected.is_none_or(|expected| key == expected)
}

fn yaml_has_comment(text: &str) -> bool {
    let characters = text.chars().collect::<Vec<_>>();
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut escaped = false;
    for (index, character) in characters.iter().copied().enumerate() {
        if character == '\n' {
            escaped = false;
            continue;
        }
        if double_quoted {
            if character == '"' && !escaped {
                double_quoted = false;
            }
            escaped = character == '\\' && !escaped;
            continue;
        }
        if character == '\'' {
            single_quoted = !single_quoted;
            continue;
        }
        if single_quoted {
            continue;
        }
        if character == '"' {
            double_quoted = true;
            continue;
        }
        if character == '#'
            && index
                .checked_sub(1)
                .and_then(|position| characters.get(position))
                .is_none_or(|previous| previous.is_whitespace())
        {
            return true;
        }
    }
    false
}

fn canonical_yaml(value: &YamlValue) -> ApplicationConnectorResult<String> {
    serde_yaml::to_string(value)
        .map(|text| text.trim_end().to_string())
        .map_err(internal)
}

fn yaml_segments(locator: &str) -> ApplicationConnectorResult<Vec<&str>> {
    let segments = locator.split('.').collect::<Vec<_>>();
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| segment.is_empty() || !valid_toml_key(segment))
    {
        return Err(invalid("invalid YAML connector locator"));
    }
    Ok(segments)
}

fn yaml_key(key: &str) -> YamlValue {
    YamlValue::String(key.to_string())
}

fn yaml_value_at<'a>(
    root: &'a YamlValue,
    locator: &str,
) -> ApplicationConnectorResult<Option<&'a YamlValue>> {
    let segments = yaml_segments(locator)?;
    let mut current = root;
    for segment in segments {
        current = match current.as_mapping() {
            Some(mapping) => match mapping.get(yaml_key(segment)) {
                Some(value) => value,
                None => return Ok(None),
            },
            None => return Err(conflict("connector YAML field has a non-mapping parent")),
        };
    }
    Ok(Some(current))
}

fn yaml_set(
    root: &mut YamlValue,
    locator: &str,
    value: YamlValue,
) -> ApplicationConnectorResult<()> {
    let segments = yaml_segments(locator)?;
    yaml_set_segments(root, &segments, value)
}

fn yaml_set_segments(
    root: &mut YamlValue,
    segments: &[&str],
    value: YamlValue,
) -> ApplicationConnectorResult<()> {
    let (head, tail) = segments
        .split_first()
        .ok_or_else(|| invalid("invalid YAML connector locator"))?;
    let mapping = root
        .as_mapping_mut()
        .ok_or_else(|| conflict("connector YAML root or parent is not a mapping"))?;
    if tail.is_empty() {
        mapping.insert(yaml_key(head), value);
        return Ok(());
    }
    let entry = mapping
        .entry(yaml_key(head))
        .or_insert_with(|| YamlValue::Mapping(YamlMapping::new()));
    if !entry.is_mapping() {
        return Err(conflict("connector YAML field has a non-mapping parent"));
    }
    yaml_set_segments(entry, tail, value)
}

fn yaml_remove(root: &mut YamlValue, locator: &str) -> ApplicationConnectorResult<()> {
    let segments = yaml_segments(locator)?;
    yaml_remove_segments(root, &segments)
}

fn yaml_remove_segments(root: &mut YamlValue, segments: &[&str]) -> ApplicationConnectorResult<()> {
    let (head, tail) = segments
        .split_first()
        .ok_or_else(|| invalid("invalid YAML connector locator"))?;
    let mapping = root
        .as_mapping_mut()
        .ok_or_else(|| conflict("connector-owned YAML field parent is missing"))?;
    if tail.is_empty() {
        mapping
            .remove(yaml_key(head))
            .ok_or_else(|| conflict("connector-owned YAML field is missing"))?;
        return Ok(());
    }
    let nested = mapping
        .get_mut(yaml_key(head))
        .ok_or_else(|| conflict("connector-owned YAML field parent is missing"))?;
    yaml_remove_segments(nested, tail)
}

fn capture_yaml_ancestors(
    root: &YamlValue,
    locator: &str,
) -> ApplicationConnectorResult<Vec<AncestorState>> {
    let segments = yaml_segments(locator)?;
    let mut current = root;
    let mut ancestors = Vec::new();
    for index in 0..segments.len().saturating_sub(1) {
        match current
            .as_mapping()
            .and_then(|mapping| mapping.get(yaml_key(segments[index])))
        {
            None => {
                for end in index..segments.len() - 1 {
                    ancestors.push(AncestorState {
                        pointer: segments[..=end].join("."),
                        originally_existed: false,
                    });
                }
                break;
            }
            Some(value) if value.is_mapping() => current = value,
            Some(_) => return Err(conflict("connector YAML field has a non-mapping parent")),
        }
    }
    Ok(ancestors)
}

fn prune_yaml_ancestors(
    root: &mut YamlValue,
    fields: &[FieldState],
) -> ApplicationConnectorResult<()> {
    let mut ancestors = fields
        .iter()
        .flat_map(|field| field.ancestors.iter())
        .filter(|ancestor| !ancestor.originally_existed)
        .map(|ancestor| ancestor.pointer.clone())
        .collect::<Vec<_>>();
    ancestors.sort_by_key(|pointer| std::cmp::Reverse(pointer.len()));
    ancestors.dedup();
    for locator in ancestors {
        let segments = yaml_segments(&locator)?;
        yaml_remove_if_empty(root, &segments);
    }
    Ok(())
}

fn yaml_remove_if_empty(root: &mut YamlValue, segments: &[&str]) {
    let Some((head, tail)) = segments.split_first() else {
        return;
    };
    let Some(mapping) = root.as_mapping_mut() else {
        return;
    };
    if tail.is_empty() {
        if mapping
            .get(yaml_key(head))
            .and_then(YamlValue::as_mapping)
            .is_some_and(YamlMapping::is_empty)
        {
            mapping.remove(yaml_key(head));
        }
        return;
    }
    if let Some(nested) = mapping.get_mut(yaml_key(head)) {
        yaml_remove_if_empty(nested, tail);
    }
}

fn preview_change(
    field: String,
    before: Option<&str>,
    after: Option<&str>,
    sensitive: bool,
) -> ApplicationConnectorChange {
    let display = |value: Option<&str>| {
        value.map(|value| {
            if sensitive {
                REDACTED.to_string()
            } else {
                value.to_string()
            }
        })
    };
    ApplicationConnectorChange {
        field,
        before: display(before),
        after: display(after),
        sensitive,
    }
}

fn store_original(
    cipher: &dyn KeyCipher,
    value: Option<&str>,
    sensitive: bool,
) -> ApplicationConnectorResult<StoredOriginal> {
    match value {
        None => Ok(StoredOriginal::Absent),
        Some(value) if sensitive => cipher
            .encrypt(value)
            .map(|protected| StoredOriginal::Protected {
                value: protected,
                sha256: sha256(value.as_bytes()),
            })
            .map_err(internal),
        Some(value) => Ok(StoredOriginal::Plain {
            value: value.to_string(),
            sha256: sha256(value.as_bytes()),
        }),
    }
}

fn load_original(
    cipher: &dyn KeyCipher,
    value: &StoredOriginal,
) -> ApplicationConnectorResult<Option<String>> {
    match value {
        StoredOriginal::Absent => Ok(None),
        StoredOriginal::Plain {
            value,
            sha256: expected,
        } => {
            if &sha256(value.as_bytes()) != expected {
                return Err(conflict("connector baseline digest is invalid"));
            }
            Ok(Some(value.clone()))
        }
        StoredOriginal::Protected {
            value,
            sha256: expected,
        } => {
            let plaintext = cipher
                .decrypt(value)
                .map_err(|_| conflict("connector baseline cannot be decoded"))?;
            if &sha256(plaintext.as_bytes()) != expected {
                return Err(conflict("connector baseline digest is invalid"));
            }
            Ok(Some(plaintext))
        }
    }
}

fn store_applied(value: &str, sensitive: bool) -> StoredApplied {
    if sensitive {
        StoredApplied::Digest {
            sha256: sha256(value.as_bytes()),
        }
    } else {
        StoredApplied::Plain {
            value: value.to_string(),
        }
    }
}

fn applied_matches(state: &StoredApplied, current: &str) -> bool {
    match state {
        StoredApplied::Plain { value } => value == current,
        StoredApplied::Digest { sha256: expected } => expected == &sha256(current.as_bytes()),
    }
}

fn state_matches(
    roots: &Roots,
    cipher: &dyn KeyCipher,
    id: ApplicationConnectorId,
    state: &StateV1,
) -> ApplicationConnectorResult<bool> {
    validate_state_documents(roots, id, state)?;
    let registry = targets(roots, id)?
        .into_iter()
        .map(|target| (target.id, target))
        .collect::<BTreeMap<_, _>>();
    for document in &state.documents {
        let target = registry
            .get(document.document_id.as_str())
            .ok_or_else(|| conflict("connector state contains an unknown target"))?;
        let image = read_target(roots, id, target)?;
        if !image.existed {
            return Ok(false);
        }
        match document.format {
            DocumentFormat::Json => {
                let root = parse_json_image(&image)?;
                for field in &document.fields {
                    let Some(value) = json_pointer(&root, &field.locator) else {
                        return Ok(false);
                    };
                    if !applied_matches(&field.applied, &canonical_json(value)?) {
                        return Ok(false);
                    }
                    // Decode protected originals during inspection. A corrupt
                    // long-lived baseline must fail closed before any write.
                    let _ = load_original(cipher, &field.original)?;
                }
            }
            DocumentFormat::Env => {
                let env = EnvDocument::parse(&parse_utf8_image(&image)?)?;
                for field in &document.fields {
                    let Some(line) = env.assignment(&field.locator) else {
                        return Ok(false);
                    };
                    if !applied_matches(&field.applied, line) {
                        return Ok(false);
                    }
                    let _ = load_original(cipher, &field.original)?;
                }
            }
            DocumentFormat::Toml => {
                let parsed = parse_toml_image(&image)?;
                for field in &document.fields {
                    let Some(value) = toml_string_at(parsed.as_table(), &field.locator)? else {
                        return Ok(false);
                    };
                    if !applied_matches(&field.applied, &value) {
                        return Ok(false);
                    }
                    let _ = load_original(cipher, &field.original)?;
                }
            }
            DocumentFormat::Yaml => {
                let root = parse_yaml_image(&image)?;
                for field in &document.fields {
                    let Some(value) = yaml_value_at(&root, &field.locator)? else {
                        return Ok(false);
                    };
                    if !applied_matches(&field.applied, &canonical_yaml(value)?) {
                        return Ok(false);
                    }
                    let _ = load_original(cipher, &field.original)?;
                }
            }
        }
    }
    Ok(true)
}

fn validate_unmanaged_targets(
    roots: &Roots,
    id: ApplicationConnectorId,
    targets: &[Target],
) -> ApplicationConnectorResult<()> {
    let mut seen = BTreeSet::new();
    for target in targets {
        if !seen.insert(target.path.clone()) {
            return Err(precondition(
                "connector target registry contains duplicate paths",
            ));
        }
        if target.path.exists() {
            let image = read_target(roots, id, target)?;
            match target.format {
                DocumentFormat::Json => {
                    parse_json_image(&image).map_err(|_| {
                        precondition(
                            "existing JSON is malformed, commented, or JSON5; use the manual guide",
                        )
                    })?;
                }
                DocumentFormat::Env => {
                    EnvDocument::parse(&parse_utf8_image(&image)?)?;
                }
                DocumentFormat::Toml => {
                    parse_toml_image(&image)?;
                }
                DocumentFormat::Yaml => {
                    parse_yaml_image(&image)?;
                }
            }
        } else if let Some(parent) = target.path.parent() {
            validate_parent_root(roots, id, parent)?;
        }
    }
    Ok(())
}

fn validate_desired_documents(
    roots: &Roots,
    id: ApplicationConnectorId,
    documents: &[DesiredDocument],
) -> ApplicationConnectorResult<()> {
    let registry = targets(roots, id)?
        .into_iter()
        .map(|target| (target.id, target.path))
        .collect::<BTreeMap<_, _>>();
    let mut document_ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for document in documents {
        if registry.get(document.target.id) != Some(&document.target.path)
            || !document_ids.insert(document.target.id)
            || !paths.insert(document.target.path.clone())
        {
            return Err(precondition(
                "connector target is outside the fixed allowlist",
            ));
        }
        let mut locators = BTreeSet::new();
        if document
            .fields
            .iter()
            .any(|field| !locators.insert(field.locator.as_str()))
        {
            return Err(invalid("connector fields contain duplicates"));
        }
    }
    Ok(())
}

fn validate_state_documents(
    roots: &Roots,
    id: ApplicationConnectorId,
    state: &StateV1,
) -> ApplicationConnectorResult<()> {
    if state.version != STATE_VERSION || state.connector_id != connector_slug(id) {
        return Err(conflict("connector sidecar version or identity is invalid"));
    }
    let registry = targets(roots, id)?
        .into_iter()
        .map(|target| (target.id, target))
        .collect::<BTreeMap<_, _>>();
    let mut documents = BTreeSet::new();
    for document in &state.documents {
        let target = registry
            .get(document.document_id.as_str())
            .ok_or_else(|| conflict("connector sidecar target is outside the allowlist"))?;
        if !documents.insert(document.document_id.as_str()) || target.format != document.format {
            return Err(conflict(
                "connector sidecar contains duplicate or mismatched targets",
            ));
        }
        let mut fields = BTreeSet::new();
        for field in &document.fields {
            if !fields.insert(field.locator.as_str())
                || field.locator.is_empty()
                || (document.format == DocumentFormat::Json && !field.locator.starts_with('/'))
                || (document.format == DocumentFormat::Env && (!valid_env_key(&field.locator)))
                || (document.format == DocumentFormat::Toml
                    && dotted_segments(&field.locator).is_err())
                || (document.format == DocumentFormat::Yaml
                    && yaml_segments(&field.locator).is_err())
            {
                return Err(conflict("connector sidecar contains invalid fields"));
            }
            if !allowed_state_field(id, &document.document_id, &field.locator) {
                return Err(conflict("connector sidecar field is outside the allowlist"));
            }
            match (&field.original, field.sensitive) {
                (StoredOriginal::Protected { sha256, .. }, true) if is_sha256(sha256) => {}
                (StoredOriginal::Absent, _) => {}
                (StoredOriginal::Plain { sha256, .. }, false) if is_sha256(sha256) => {}
                _ => {
                    return Err(conflict(
                        "connector sidecar sensitivity metadata is invalid",
                    ));
                }
            }
            match (&field.applied, field.sensitive) {
                (StoredApplied::Digest { sha256 }, true) if is_sha256(sha256) => {}
                (StoredApplied::Plain { .. }, false) => {}
                _ => return Err(conflict("connector sidecar applied metadata is invalid")),
            }
            if document.format == DocumentFormat::Json {
                let mut ancestors = BTreeSet::new();
                for ancestor in &field.ancestors {
                    if !ancestors.insert(ancestor.pointer.as_str())
                        || pointer_segments(&ancestor.pointer).is_err()
                        || !field
                            .locator
                            .strip_prefix(&ancestor.pointer)
                            .is_some_and(|rest| rest.starts_with('/'))
                    {
                        return Err(conflict("connector sidecar ancestor metadata is invalid"));
                    }
                }
            } else if document.format == DocumentFormat::Env && !field.ancestors.is_empty() {
                return Err(conflict(
                    "connector sidecar environment metadata is invalid",
                ));
            } else if matches!(document.format, DocumentFormat::Toml | DocumentFormat::Yaml) {
                let mut ancestors = BTreeSet::new();
                for ancestor in &field.ancestors {
                    if !ancestors.insert(ancestor.pointer.as_str())
                        || dotted_segments(&ancestor.pointer).is_err()
                        || !field
                            .locator
                            .strip_prefix(&ancestor.pointer)
                            .is_some_and(|rest| rest.starts_with('.'))
                    {
                        return Err(conflict("connector sidecar ancestor metadata is invalid"));
                    }
                }
            }
        }
    }
    Ok(())
}

fn allowed_state_field(id: ApplicationConnectorId, document_id: &str, locator: &str) -> bool {
    match (id, document_id) {
        (ApplicationConnectorId::ClaudeCode, "claude-settings") => matches!(
            locator,
            "/env/ANTHROPIC_BASE_URL"
                | "/env/ANTHROPIC_AUTH_TOKEN"
                | "/env/ANTHROPIC_MODEL"
                | "/env/ANTHROPIC_DEFAULT_FABLE_MODEL"
                | "/env/ANTHROPIC_DEFAULT_HAIKU_MODEL"
                | "/env/ANTHROPIC_DEFAULT_SONNET_MODEL"
                | "/env/ANTHROPIC_DEFAULT_OPUS_MODEL"
                | "/env/CLAUDE_CODE_SUBAGENT_MODEL"
                | "/env/ANTHROPIC_CUSTOM_MODEL_OPTION"
                | "/model"
        ),
        (ApplicationConnectorId::Codex, "codex-config") => matches!(
            locator,
            "model"
                | "model_provider"
                | "model_providers.ocg_manager.name"
                | "model_providers.ocg_manager.base_url"
                | "model_providers.ocg_manager.wire_api"
                | "model_providers.ocg_manager.experimental_bearer_token"
        ),
        (ApplicationConnectorId::GeminiCli, "gemini-settings") => matches!(
            locator,
            "/model/name" | "/modelConfigs/customOverrides" | "/agents/overrides"
        ),
        (ApplicationConnectorId::GeminiCli, "gemini-env") => matches!(
            locator,
            "GEMINI_API_KEY" | "GOOGLE_GEMINI_BASE_URL" | "GOOGLE_GENAI_API_VERSION"
        ),
        (ApplicationConnectorId::OpenCode, "opencode-settings") => {
            matches!(locator, "/provider/ocg" | "/model")
        }
        (ApplicationConnectorId::OpenCode, "opencode-env") => locator == "OCG_API_KEY",
        (ApplicationConnectorId::OpenClaw, "openclaw-settings") => matches!(
            locator,
            "/models/mode" | "/models/providers/ocg" | "/agents/defaults/model/primary"
        ),
        (ApplicationConnectorId::OpenClaw, "openclaw-env") => locator == "CUSTOM_API_KEY",
        (ApplicationConnectorId::Dsh, "dsh-env") => locator == "OCG_MANAGER_API_KEY",
        (ApplicationConnectorId::Hermes, "hermes-config") => matches!(
            locator,
            "model.default" | "model.provider" | "model.base_url" | "model.api_key"
        ),
        (ApplicationConnectorId::Hermes, "hermes-env") => locator == "OCG_MANAGER_API_KEY",
        _ => false,
    }
}

fn finalize_state(mut state: StateV1) -> ApplicationConnectorResult<StateV1> {
    state.integrity_sha256.clear();
    state.integrity_sha256 = sha256(&serde_json::to_vec(&state).map_err(internal)?);
    Ok(state)
}

fn verify_state_integrity(state: &StateV1) -> ApplicationConnectorResult<()> {
    if !is_sha256(&state.integrity_sha256) {
        return Err(conflict("connector sidecar integrity metadata is invalid"));
    }
    let expected = state.integrity_sha256.clone();
    let mut unsigned = state.clone();
    unsigned.integrity_sha256.clear();
    if sha256(&serde_json::to_vec(&unsigned).map_err(internal)?) != expected {
        return Err(conflict("connector sidecar integrity check failed"));
    }
    Ok(())
}

fn load_state(
    roots: &Roots,
    data_dir: &Path,
    cipher: &dyn KeyCipher,
    id: ApplicationConnectorId,
) -> ApplicationConnectorResult<Option<StateV1>> {
    let path = state_path(data_dir, id);
    let image = read_internal_file(&path)?;
    if !image.existed {
        return Ok(None);
    }
    let state: StateV1 = serde_json::from_slice(&image.bytes)
        .map_err(|_| conflict("connector sidecar is corrupt"))?;
    verify_state_integrity(&state)?;
    validate_state_documents(roots, id, &state)?;
    // Validate every protected baseline now, rather than discovering a
    // corrupt value only during restore.
    for field in state.documents.iter().flat_map(|document| &document.fields) {
        let _ = load_original(cipher, &field.original)?;
    }
    Ok(Some(state))
}

fn state_target(data_dir: &Path, id: ApplicationConnectorId) -> Target {
    Target {
        id: "__state",
        path: state_path(data_dir, id),
        label: "OCG Manager connector sidecar",
        format: DocumentFormat::Json,
    }
}

fn state_path(data_dir: &Path, id: ApplicationConnectorId) -> PathBuf {
    connector_root(data_dir).join(format!("{}.state.json", connector_slug(id)))
}

fn journal_path(data_dir: &Path, id: ApplicationConnectorId) -> PathBuf {
    connector_root(data_dir).join(format!("{}.transaction.json", connector_slug(id)))
}

fn connector_root(data_dir: &Path) -> PathBuf {
    data_dir.join("application-connectors")
}

fn connector_slug(id: ApplicationConnectorId) -> &'static str {
    match id {
        ApplicationConnectorId::ClaudeCode => "claude-code",
        ApplicationConnectorId::Codex => "codex",
        ApplicationConnectorId::Dsh => "dsh",
        ApplicationConnectorId::GeminiCli => "gemini-cli",
        ApplicationConnectorId::OpenCode => "opencode",
        ApplicationConnectorId::OpenClaw => "openclaw",
        ApplicationConnectorId::Pi => "pi",
        ApplicationConnectorId::Hermes => "hermes",
    }
}

fn target_detected(target: &Target) -> bool {
    target.path.exists() || target.path.parent().is_some_and(Path::exists)
}

fn user_home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home().join("AppData/Local"))
}

fn hermes_root(roots: &Roots) -> ApplicationConnectorResult<PathBuf> {
    if let Some(path) = &roots.hermes_home {
        return Ok(path.clone());
    }
    let preferred = roots.local_app_data.join("hermes");
    let legacy = roots.home.join(".hermes");
    #[cfg(not(windows))]
    {
        let _ = preferred;
        return Ok(legacy);
    }
    #[cfg(windows)]
    match (preferred.exists(), legacy.exists()) {
        (true, true) => Err(precondition(
            "Hermes configuration is ambiguous: both LOCALAPPDATA/hermes and ~/.hermes exist",
        )),
        (true, false) => Ok(preferred),
        (false, true) => Ok(legacy),
        // Windows Hermes uses LOCALAPPDATA by default. This also makes a
        // first-time connection deterministic without probing a legacy path.
        (false, false) => Ok(preferred),
    }
}

fn acquire_lock(data_dir: &Path) -> ApplicationConnectorResult<File> {
    fs::create_dir_all(data_dir).map_err(internal)?;
    reject_unsafe_metadata(
        data_dir,
        &fs::symlink_metadata(data_dir).map_err(internal)?,
        false,
    )?;
    let root = connector_root(data_dir);
    fs::create_dir_all(&root).map_err(internal)?;
    reject_unsafe_metadata(
        &root,
        &fs::symlink_metadata(&root).map_err(internal)?,
        false,
    )?;
    let path = root.join(".lock");
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(internal)?;
    file.lock_exclusive().map_err(internal)?;
    Ok(file)
}

fn read_target(
    roots: &Roots,
    id: ApplicationConnectorId,
    target: &Target,
) -> ApplicationConnectorResult<FileImage> {
    let expected = targets(roots, id)?
        .into_iter()
        .find(|candidate| candidate.id == target.id)
        .ok_or_else(|| precondition("connector target is outside the fixed allowlist"))?;
    if expected.path != target.path {
        return Err(precondition(
            "connector target is outside the fixed allowlist",
        ));
    }
    validate_target_path(roots, id, &target.path)?;
    read_file_image(&target.path)
}

fn read_internal_file(path: &Path) -> ApplicationConnectorResult<FileImage> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            return Ok(FileImage {
                existed: false,
                bytes: Vec::new(),
            });
        }
        reject_unsafe_metadata(
            parent,
            &fs::symlink_metadata(parent).map_err(internal)?,
            false,
        )?;
    }
    read_file_image(path)
}

fn read_file_image(path: &Path) -> ApplicationConnectorResult<FileImage> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(FileImage {
                existed: false,
                bytes: Vec::new(),
            });
        }
        Err(error) => return Err(internal(error)),
    };
    reject_unsafe_metadata(path, &metadata, true)?;
    let bytes = fs::read(path).map_err(internal)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(conflict("connector target changed while being read"));
    }
    Ok(FileImage {
        existed: true,
        bytes,
    })
}

fn validate_target_path(
    roots: &Roots,
    id: ApplicationConnectorId,
    path: &Path,
) -> ApplicationConnectorResult<()> {
    let root = connector_root_for_id(roots, id)?;
    let parent = path
        .parent()
        .ok_or_else(|| precondition("connector target has no parent"))?;
    validate_parent_against_root(root, parent, id == ApplicationConnectorId::Dsh)
}

fn validate_parent_root(
    roots: &Roots,
    id: ApplicationConnectorId,
    parent: &Path,
) -> ApplicationConnectorResult<()> {
    let root = connector_root_for_id(roots, id)?;
    validate_parent_against_root(root, parent, id == ApplicationConnectorId::Dsh)
}

fn connector_root_for_id(
    roots: &Roots,
    id: ApplicationConnectorId,
) -> ApplicationConnectorResult<&Path> {
    match id {
        ApplicationConnectorId::Hermes => {
            let root = hermes_root(roots)?;
            // `Targets` is constructed from the same resolver. Keep the root
            // in a stable field for validation instead of deriving a second
            // potentially different environment view.
            if roots.hermes_home.as_ref() == Some(&root) {
                return Ok(roots.hermes_home.as_deref().expect("checked above"));
            }
            if root == roots.local_app_data.join("hermes") {
                return Ok(&roots.local_app_data);
            }
            Ok(&roots.home)
        }
        ApplicationConnectorId::Codex => Ok(roots.codex_home.as_deref().unwrap_or(&roots.home)),
        ApplicationConnectorId::Dsh => Ok(roots.dsh_home.as_deref().unwrap_or(&roots.home)),
        _ => Ok(&roots.home),
    }
}

fn validate_parent_against_root(
    root: &Path,
    parent: &Path,
    allow_missing: bool,
) -> ApplicationConnectorResult<()> {
    if !parent.starts_with(root)
        || parent
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(precondition("connector target escapes its fixed root"));
    }
    if !parent.exists() {
        if !allow_missing {
            return Err(precondition(
                "application configuration directory was not detected",
            ));
        }
        // DSH's package manager creates the profile root on first install.
        // Validate the nearest existing ancestor now; the normal transaction
        // preflight revalidates the final parent after the package command.
        let mut ancestor = parent;
        while !ancestor.exists() {
            ancestor = ancestor.parent().ok_or_else(|| {
                precondition("application configuration root has no existing ancestor")
            })?;
        }
        let metadata = fs::symlink_metadata(ancestor).map_err(internal)?;
        reject_unsafe_metadata(ancestor, &metadata, false)?;
        fs::canonicalize(ancestor).map_err(internal)?;
        return Ok(());
    }
    let root_metadata = fs::symlink_metadata(root).map_err(internal)?;
    reject_unsafe_metadata(root, &root_metadata, false)?;
    let canonical_root = fs::canonicalize(root).map_err(internal)?;
    let canonical_parent = fs::canonicalize(parent).map_err(internal)?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(precondition(
            "connector target resolves outside its fixed root",
        ));
    }
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| precondition("connector target escapes its fixed root"))?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        cursor.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&cursor).map_err(internal)?;
        reject_unsafe_metadata(&cursor, &metadata, false)?;
    }
    Ok(())
}

fn reject_unsafe_metadata(
    _path: &Path,
    metadata: &Metadata,
    require_file: bool,
) -> ApplicationConnectorResult<()> {
    if metadata.file_type().is_symlink() || is_reparse(metadata) {
        return Err(precondition(
            "connector target or parent is a link/reparse point",
        ));
    }
    if require_file && !metadata.is_file() {
        return Err(precondition("connector target is not a regular file"));
    }
    if !require_file && !metadata.is_dir() {
        return Err(precondition("connector parent is not a directory"));
    }
    if require_file && metadata.len() > MAX_BYTES {
        return Err(precondition("connector target exceeds the size limit"));
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse(_metadata: &Metadata) -> bool {
    false
}

fn parse_utf8_image(image: &FileImage) -> ApplicationConnectorResult<String> {
    String::from_utf8(image.bytes.clone())
        .map_err(|_| precondition("connector target is not UTF-8"))
}

fn parse_json_image(image: &FileImage) -> ApplicationConnectorResult<Value> {
    if !image.existed || image.bytes.is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_slice(&image.bytes)
        .map_err(|_| precondition("connector JSON is malformed, commented, or JSON5"))?;
    if !value.is_object() {
        return Err(precondition("connector JSON root must be an object"));
    }
    Ok(value)
}

fn serialize_json(value: &Value) -> ApplicationConnectorResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(internal)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn canonical_json(value: &Value) -> ApplicationConnectorResult<String> {
    serde_json::to_string(value).map_err(internal)
}

#[derive(Clone)]
struct EnvLine {
    content: String,
    ending: String,
    key: Option<String>,
}

struct EnvDocument {
    lines: Vec<EnvLine>,
    default_ending: String,
}

impl EnvDocument {
    fn parse(text: &str) -> ApplicationConnectorResult<Self> {
        let default_ending = if text.contains("\r\n") { "\r\n" } else { "\n" }.to_string();
        let mut lines = Vec::new();
        let mut seen = BTreeSet::new();
        let mut offset = 0;
        while offset < text.len() {
            let remaining = &text[offset..];
            let (content, ending, consumed) = if let Some(index) = remaining.find('\n') {
                let line = &remaining[..index];
                if let Some(stripped) = line.strip_suffix('\r') {
                    (stripped, "\r\n", index + 1)
                } else {
                    (line, "\n", index + 1)
                }
            } else {
                (remaining, "", remaining.len())
            };
            let key = parse_env_key(content)?;
            if key.as_ref().is_some_and(|key| !seen.insert(key.clone())) {
                return Err(precondition("connector environment has duplicate fields"));
            }
            lines.push(EnvLine {
                content: content.to_string(),
                ending: ending.to_string(),
                key,
            });
            offset += consumed;
        }
        Ok(Self {
            lines,
            default_ending,
        })
    }

    fn assignment(&self, key: &str) -> Option<&str> {
        self.lines
            .iter()
            .find(|line| line.key.as_deref() == Some(key))
            .map(|line| line.content.as_str())
    }

    fn set(&mut self, key: &str, line: &str) -> ApplicationConnectorResult<()> {
        if !valid_env_key(key) || parse_env_key(line)?.as_deref() != Some(key) {
            return Err(invalid("invalid environment field"));
        }
        if let Some(existing) = self
            .lines
            .iter_mut()
            .find(|existing| existing.key.as_deref() == Some(key))
        {
            existing.content = line.to_string();
            return Ok(());
        }
        if let Some(last) = self.lines.last_mut() {
            if last.ending.is_empty() {
                last.ending = self.default_ending.clone();
            }
        }
        self.lines.push(EnvLine {
            content: line.to_string(),
            ending: self.default_ending.clone(),
            key: Some(key.to_string()),
        });
        Ok(())
    }

    fn remove(&mut self, key: &str) -> ApplicationConnectorResult<()> {
        let position = self
            .lines
            .iter()
            .position(|line| line.key.as_deref() == Some(key))
            .ok_or_else(|| conflict("connector-owned environment field is missing"))?;
        self.lines.remove(position);
        Ok(())
    }

    fn render(&self) -> String {
        let mut output = String::new();
        for line in &self.lines {
            output.push_str(&line.content);
            output.push_str(&line.ending);
        }
        output
    }
}

fn parse_env_key(line: &str) -> ApplicationConnectorResult<Option<String>> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let assignment = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    let (key, _value) = assignment
        .split_once('=')
        .ok_or_else(|| precondition("connector environment contains an unsupported line"))?;
    let key = key.trim();
    if !valid_env_key(key) {
        return Err(precondition(
            "connector environment contains an invalid key",
        ));
    }
    Ok(Some(key.to_string()))
}

fn valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn quote_env(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string cannot fail")
}

fn pointer_segments(pointer: &str) -> ApplicationConnectorResult<Vec<String>> {
    if !pointer.starts_with('/') || pointer.len() < 2 {
        return Err(invalid("invalid JSON pointer"));
    }
    pointer[1..]
        .split('/')
        .map(|segment| {
            let mut decoded = String::new();
            let mut chars = segment.chars();
            while let Some(character) = chars.next() {
                if character != '~' {
                    decoded.push(character);
                    continue;
                }
                match chars.next() {
                    Some('0') => decoded.push('~'),
                    Some('1') => decoded.push('/'),
                    _ => return Err(invalid("invalid JSON pointer escape")),
                }
            }
            if decoded.is_empty() {
                return Err(invalid("empty JSON pointer segment"));
            }
            Ok(decoded)
        })
        .collect()
}

fn json_pointer<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    let segments = pointer_segments(pointer).ok()?;
    let mut current = root;
    for segment in segments {
        current = current.as_object()?.get(&segment)?;
    }
    Some(current)
}

fn set_json_pointer(
    root: &mut Value,
    pointer: &str,
    value: Value,
) -> ApplicationConnectorResult<()> {
    let segments = pointer_segments(pointer)?;
    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        let object = current
            .as_object_mut()
            .ok_or_else(|| conflict("JSON connector field has a non-object parent"))?;
        current = object
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !current.is_object() {
            return Err(conflict("JSON connector field has a non-object parent"));
        }
    }
    current
        .as_object_mut()
        .ok_or_else(|| conflict("JSON connector field has a non-object parent"))?
        .insert(segments.last().expect("non-empty pointer").clone(), value);
    Ok(())
}

fn remove_json_pointer(root: &mut Value, pointer: &str) -> ApplicationConnectorResult<()> {
    let segments = pointer_segments(pointer)?;
    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        current = current
            .as_object_mut()
            .and_then(|object| object.get_mut(segment))
            .ok_or_else(|| conflict("connector-owned JSON field parent is missing"))?;
    }
    current
        .as_object_mut()
        .ok_or_else(|| conflict("connector-owned JSON field parent changed"))?
        .remove(segments.last().expect("non-empty pointer"))
        .ok_or_else(|| conflict("connector-owned JSON field is missing"))?;
    Ok(())
}

fn capture_ancestors(
    root: &Value,
    pointer: &str,
) -> ApplicationConnectorResult<Vec<AncestorState>> {
    let segments = pointer_segments(pointer)?;
    let mut current = root;
    let mut prefix = String::new();
    let mut result = Vec::new();
    for segment in &segments[..segments.len() - 1] {
        prefix.push('/');
        prefix.push_str(&escape_pointer_segment(segment));
        let next = current.as_object().and_then(|object| object.get(segment));
        result.push(AncestorState {
            pointer: prefix.clone(),
            originally_existed: next.is_some(),
        });
        current = match next {
            Some(value) => value,
            None => &Value::Null,
        };
    }
    Ok(result)
}

fn prune_created_ancestors(
    root: &mut Value,
    fields: &[FieldState],
) -> ApplicationConnectorResult<()> {
    let mut candidates = fields
        .iter()
        .flat_map(|field| &field.ancestors)
        .filter(|ancestor| !ancestor.originally_existed)
        .map(|ancestor| ancestor.pointer.clone())
        .collect::<Vec<_>>();
    candidates.sort_by_key(|pointer| std::cmp::Reverse(pointer.matches('/').count()));
    candidates.dedup();
    for pointer in candidates {
        if json_pointer(root, &pointer)
            .and_then(Value::as_object)
            .is_some_and(Map::is_empty)
        {
            remove_json_pointer(root, &pointer)?;
        }
    }
    Ok(())
}

fn escape_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn plan_fingerprint(request: &ApplicationConnectorHostRequest, plan: &MutationPlan) -> String {
    let mut hash = Sha256::new();
    hash.update(b"ocg-application-connector-preview-v1\0");
    hash.update(connector_slug(request.id).as_bytes());
    hash.update(format!("{:?}", request.action).as_bytes());
    hash.update(request.key_id.as_deref().unwrap_or("").as_bytes());
    for (key, value) in &request.model_values {
        hash.update(key.as_bytes());
        hash.update([0]);
        hash.update(value.as_bytes());
        hash.update([0]);
    }
    if let Some(secret) = &request.secret {
        hash.update(sha256(secret.expose_to_host().as_bytes()).as_bytes());
    }
    for file in &plan.files {
        hash.update(file.document_id.as_bytes());
        hash.update([u8::from(file.before.existed)]);
        hash.update(sha256(&file.before.bytes).as_bytes());
        hash.update([u8::from(file.after.existed)]);
        hash.update(sha256(&file.after.bytes).as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn apply_transaction(
    roots: &Roots,
    data_dir: &Path,
    cipher: &dyn KeyCipher,
    plan: &MutationPlan,
    fail_before_index: Option<usize>,
) -> ApplicationConnectorResult<()> {
    apply_transaction_after_journal(roots, data_dir, cipher, plan, fail_before_index, || Ok(()))
}

fn apply_transaction_after_journal<F>(
    roots: &Roots,
    data_dir: &Path,
    cipher: &dyn KeyCipher,
    plan: &MutationPlan,
    fail_before_index: Option<usize>,
    after_journal: F,
) -> ApplicationConnectorResult<()>
where
    F: FnOnce() -> ApplicationConnectorResult<()>,
{
    validate_plan_paths(roots, data_dir, plan)?;
    // All before/after images already exist in memory before the first write.
    let journal = finalize_journal(JournalV1 {
        version: JOURNAL_VERSION,
        connector_id: connector_slug(plan.id).to_string(),
        files: plan
            .files
            .iter()
            .map(|file| {
                let preimage = String::from_utf8(file.before.bytes.clone())
                    .map_err(|_| precondition("transaction preimage is not UTF-8"))?;
                Ok(JournalFile {
                    document_id: file.document_id.clone(),
                    before_existed: file.before.existed,
                    before_sha256: sha256(&file.before.bytes),
                    after_existed: file.after.existed,
                    after_sha256: sha256(&file.after.bytes),
                    protected_preimage: cipher.encrypt(&preimage).map_err(internal)?,
                })
            })
            .collect::<ApplicationConnectorResult<Vec<_>>>()?,
        integrity_sha256: String::new(),
    })?;
    let journal_path = journal_path(data_dir, plan.id);
    let journal_bytes = serde_json::to_vec_pretty(&journal).map_err(internal)?;
    atomic_write(&journal_path, &journal_bytes, true)?;
    verify_journal(&journal_path, plan.id)?;

    if let Err(error) = after_journal().and_then(|()| preflight_plan_before(roots, data_dir, plan))
    {
        // No target write has occurred yet. The journal is safe to remove; if
        // cleanup itself fails it remains recoverable because every target is
        // still required to equal its before-image.
        let _ = remove_internal_file(&journal_path);
        return Err(error);
    }

    let mut original_error = None;
    for (index, file) in plan.files.iter().enumerate() {
        let result = if fail_before_index == Some(index) {
            Err(internal_message("injected connector transaction failure"))
        } else {
            validate_planned_path(roots, data_dir, plan.id, file)?;
            write_image(plan.id, file)
        };
        if let Err(error) = result {
            original_error = Some(error);
            break;
        }
    }
    if let Some(error) = original_error {
        match compensate_plan(roots, data_dir, plan) {
            Ok(()) => {
                remove_internal_file(&journal_path)?;
                return Err(error);
            }
            Err(compensation) => {
                // The retained journal is the durable partial-status marker and
                // contains every exact preimage in protected form.
                return Err(internal_message(&format!(
                    "connector transaction failed and compensation is incomplete: {compensation}"
                )));
            }
        }
    }
    remove_internal_file(&journal_path)?;
    Ok(())
}

fn preflight_plan_before(
    roots: &Roots,
    data_dir: &Path,
    plan: &MutationPlan,
) -> ApplicationConnectorResult<()> {
    for file in &plan.files {
        validate_planned_path(roots, data_dir, plan.id, file)?;
        let current = read_file_image(&file.path)?;
        if current != file.before {
            return Err(conflict(
                "connector target changed after the transaction journal was persisted",
            ));
        }
    }
    Ok(())
}

fn compensate_plan(
    roots: &Roots,
    data_dir: &Path,
    plan: &MutationPlan,
) -> ApplicationConnectorResult<()> {
    let mut classified = Vec::with_capacity(plan.files.len());
    for file in &plan.files {
        validate_planned_path(roots, data_dir, plan.id, file)?;
        let current = read_file_image(&file.path)?;
        let needs_restore = if current == file.before {
            false
        } else if current == file.after {
            true
        } else {
            return Err(conflict(
                "connector compensation found external target drift",
            ));
        };
        classified.push((file, needs_restore));
    }
    for (file, needs_restore) in classified.into_iter().rev() {
        if needs_restore {
            write_exact_image(
                &file.path,
                &file.before,
                file.format,
                document_requires_private(plan.id, &file.document_id),
            )?;
        }
    }
    Ok(())
}

fn recover_journal(
    roots: &Roots,
    data_dir: &Path,
    cipher: &dyn KeyCipher,
    id: ApplicationConnectorId,
) -> ApplicationConnectorResult<()> {
    let path = journal_path(data_dir, id);
    let image = read_internal_file(&path)?;
    if !image.existed {
        return Ok(());
    }
    let journal: JournalV1 = serde_json::from_slice(&image.bytes)
        .map_err(|_| conflict("connector transaction journal is corrupt"))?;
    verify_journal_integrity(&journal)?;
    if journal.version != JOURNAL_VERSION || journal.connector_id != connector_slug(id) {
        return Err(conflict(
            "connector transaction journal identity is invalid",
        ));
    }
    let registry = transaction_registry(roots, data_dir, id)?;
    let mut seen = BTreeSet::new();
    let mut recovery_files = Vec::new();
    for entry in &journal.files {
        if !seen.insert(entry.document_id.as_str())
            || !is_sha256(&entry.before_sha256)
            || !is_sha256(&entry.after_sha256)
            || (!entry.after_existed && entry.after_sha256 != sha256(&[]))
        {
            return Err(conflict(
                "connector transaction journal contains invalid entries",
            ));
        }
        let (target_path, format) = registry
            .get(entry.document_id.as_str())
            .ok_or_else(|| conflict("connector transaction journal target is not allowlisted"))?;
        let plaintext = cipher
            .decrypt(&entry.protected_preimage)
            .map_err(|_| conflict("connector transaction preimage cannot be decoded"))?;
        let bytes = plaintext.into_bytes();
        if sha256(&bytes) != entry.before_sha256 {
            return Err(conflict("connector transaction preimage digest is invalid"));
        }
        if !entry.before_existed && !bytes.is_empty() {
            return Err(conflict(
                "connector transaction before-image existence is invalid",
            ));
        }
        let before = FileImage {
            existed: entry.before_existed,
            bytes,
        };
        let current = if entry.document_id == "__state" {
            let root = connector_root(data_dir);
            reject_unsafe_metadata(
                &root,
                &fs::symlink_metadata(&root).map_err(internal)?,
                false,
            )?;
            read_internal_file(target_path)?
        } else {
            validate_target_path(roots, id, target_path)?;
            read_file_image(target_path)?
        };
        let needs_restore = if current == before {
            false
        } else if image_matches_digest(&current, entry.after_existed, &entry.after_sha256) {
            true
        } else {
            // Classify every target before writing any preimage. A third-party
            // edit is neither a transaction after-image nor an already-restored
            // before-image and must never be overwritten.
            return Err(conflict(
                "connector transaction recovery found external target drift",
            ));
        };
        recovery_files.push((target_path.clone(), before, *format, needs_restore));
    }
    for (target_path, image, format, needs_restore) in recovery_files.into_iter().rev() {
        if !needs_restore {
            continue;
        }
        let allowed = registry.values().any(|(path, _)| path == &target_path);
        if !allowed {
            return Err(conflict("connector recovery target is not allowlisted"));
        }
        if target_path != state_path(data_dir, id) {
            validate_target_path(roots, id, &target_path)?;
        }
        write_exact_image(
            &target_path,
            &image,
            format,
            document_requires_private(id, document_id_for_path(&registry, &target_path)?),
        )?;
    }
    remove_internal_file(&path)
}

fn image_matches_digest(image: &FileImage, existed: bool, expected_sha256: &str) -> bool {
    image.existed == existed && sha256(&image.bytes) == expected_sha256
}

fn document_id_for_path<'a>(
    registry: &'a BTreeMap<String, (PathBuf, Option<DocumentFormat>)>,
    path: &Path,
) -> ApplicationConnectorResult<&'a str> {
    registry
        .iter()
        .find_map(|(id, (candidate, _))| (candidate == path).then_some(id.as_str()))
        .ok_or_else(|| conflict("connector recovery target is not allowlisted"))
}

fn transaction_registry(
    roots: &Roots,
    data_dir: &Path,
    id: ApplicationConnectorId,
) -> ApplicationConnectorResult<BTreeMap<String, (PathBuf, Option<DocumentFormat>)>> {
    let mut registry = targets(roots, id)?
        .into_iter()
        .map(|target| (target.id.to_string(), (target.path, Some(target.format))))
        .collect::<BTreeMap<_, _>>();
    registry.insert("__state".into(), (state_path(data_dir, id), None));
    Ok(registry)
}

fn validate_plan_paths(
    roots: &Roots,
    data_dir: &Path,
    plan: &MutationPlan,
) -> ApplicationConnectorResult<()> {
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for file in &plan.files {
        if !ids.insert(file.document_id.as_str()) || !paths.insert(file.path.clone()) {
            return Err(precondition(
                "connector transaction contains duplicate files",
            ));
        }
        validate_planned_path(roots, data_dir, plan.id, file)?;
    }
    Ok(())
}

fn validate_planned_path(
    roots: &Roots,
    data_dir: &Path,
    id: ApplicationConnectorId,
    file: &PlannedFile,
) -> ApplicationConnectorResult<()> {
    let registry = transaction_registry(roots, data_dir, id)?;
    let (expected_path, expected_format) = registry
        .get(&file.document_id)
        .ok_or_else(|| precondition("connector transaction target is not allowlisted"))?;
    if expected_path != &file.path || expected_format != &file.format {
        return Err(precondition(
            "connector transaction target is not allowlisted",
        ));
    }
    if file.document_id == "__state" {
        let root = connector_root(data_dir);
        reject_unsafe_metadata(
            &root,
            &fs::symlink_metadata(&root).map_err(internal)?,
            false,
        )
    } else {
        validate_target_path(roots, id, &file.path)
    }
}

fn verify_journal(path: &Path, id: ApplicationConnectorId) -> ApplicationConnectorResult<()> {
    let image = read_internal_file(path)?;
    let journal: JournalV1 = serde_json::from_slice(&image.bytes)
        .map_err(|_| internal_message("connector journal write verification failed"))?;
    verify_journal_integrity(&journal)
        .map_err(|_| internal_message("connector journal write verification failed"))?;
    if journal.version != JOURNAL_VERSION || journal.connector_id != connector_slug(id) {
        return Err(internal_message(
            "connector journal write verification failed",
        ));
    }
    Ok(())
}

fn finalize_journal(mut journal: JournalV1) -> ApplicationConnectorResult<JournalV1> {
    journal.integrity_sha256.clear();
    journal.integrity_sha256 = sha256(&serde_json::to_vec(&journal).map_err(internal)?);
    Ok(journal)
}

fn verify_journal_integrity(journal: &JournalV1) -> ApplicationConnectorResult<()> {
    if !is_sha256(&journal.integrity_sha256) {
        return Err(conflict(
            "connector transaction journal integrity is invalid",
        ));
    }
    let expected = journal.integrity_sha256.clone();
    let mut unsigned = journal.clone();
    unsigned.integrity_sha256.clear();
    if sha256(&serde_json::to_vec(&unsigned).map_err(internal)?) != expected {
        return Err(conflict(
            "connector transaction journal integrity check failed",
        ));
    }
    Ok(())
}

fn write_image(id: ApplicationConnectorId, file: &PlannedFile) -> ApplicationConnectorResult<()> {
    write_exact_image(
        &file.path,
        &file.after,
        file.format,
        document_requires_private(id, &file.document_id),
    )
}

fn write_exact_image(
    path: &Path,
    image: &FileImage,
    format: Option<DocumentFormat>,
    private: bool,
) -> ApplicationConnectorResult<()> {
    if image.existed {
        atomic_write(path, &image.bytes, private)?;
    } else {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                reject_unsafe_metadata(path, &metadata, true)?;
                fs::remove_file(path).map_err(internal)?;
                sync_parent(path)?;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(internal(error)),
        }
    }
    let actual = read_file_image(path)?;
    if actual != *image {
        return Err(internal_message("connector byte verification failed"));
    }
    if image.existed {
        match format {
            Some(DocumentFormat::Json) => {
                let _ = parse_json_image(image)?;
            }
            Some(DocumentFormat::Env) => {
                let _ = EnvDocument::parse(&parse_utf8_image(image)?)?;
            }
            Some(DocumentFormat::Toml) => {
                let _ = parse_toml_image(image)?;
            }
            Some(DocumentFormat::Yaml) => {
                let _ = parse_yaml_image(image)?;
            }
            None => {
                let _: StateV1 = serde_json::from_slice(&image.bytes)
                    .map_err(|_| internal_message("connector sidecar verification failed"))?;
            }
        }
    }
    Ok(())
}

fn document_requires_private(id: ApplicationConnectorId, document_id: &str) -> bool {
    document_id == "__state"
        || matches!(
            (id, document_id),
            (ApplicationConnectorId::ClaudeCode, "claude-settings")
                | (ApplicationConnectorId::GeminiCli, "gemini-env")
                | (ApplicationConnectorId::OpenCode, "opencode-env")
                | (ApplicationConnectorId::OpenClaw, "openclaw-env")
                | (ApplicationConnectorId::Codex, "codex-config")
                | (ApplicationConnectorId::Dsh, "dsh-env")
                | (ApplicationConnectorId::Hermes, "hermes-config")
                | (ApplicationConnectorId::Hermes, "hermes-env")
        )
}

struct TempFileGuard(PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn atomic_write(path: &Path, bytes: &[u8], private: bool) -> ApplicationConnectorResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| internal_message("connector target has no parent"))?;
    if !parent.exists() {
        return Err(precondition("connector target parent does not exist"));
    }
    reject_unsafe_metadata(
        parent,
        &fs::symlink_metadata(parent).map_err(internal)?,
        false,
    )?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        reject_unsafe_metadata(path, &metadata, true)?;
    }
    let temp_path = parent.join(format!(
        ".ocg-connector-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let guard = TempFileGuard(temp_path.clone());
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    let mut temp = options.open(&temp_path).map_err(internal)?;
    #[cfg(windows)]
    if private {
        // Windows creates the sibling with inherited ACLs. Tighten and verify
        // it before the first secret byte is written, not merely before the
        // atomic replacement.
        set_private_permissions(&temp_path)?;
    }
    temp.write_all(bytes).map_err(internal)?;
    temp.sync_all().map_err(internal)?;
    sync_temp_permissions(&temp_path, path, private)?;
    drop(temp);
    replace_file(&temp_path, path)?;
    std::mem::forget(guard);
    if private {
        // ReplaceFileW preserves parts of the destination security descriptor.
        // Reapply and verify the private DACL after replacement as well as on
        // the sibling temp file, failing the transaction closed on any drift.
        set_private_permissions(path)?;
    }
    sync_parent(path)?;
    Ok(())
}

#[cfg(unix)]
fn sync_temp_permissions(
    temp_path: &Path,
    destination: &Path,
    private: bool,
) -> ApplicationConnectorResult<()> {
    if private {
        // A connector document containing a plaintext Key must never inherit
        // group/other readability from a pre-existing user file.
        set_private_permissions(temp_path)
    } else if let Ok(metadata) = fs::metadata(destination) {
        fs::set_permissions(temp_path, metadata.permissions()).map_err(internal)
    } else {
        Ok(())
    }
}

#[cfg(not(unix))]
fn sync_temp_permissions(
    temp_path: &Path,
    destination: &Path,
    private: bool,
) -> ApplicationConnectorResult<()> {
    if private {
        // Tighten an existing destination before ReplaceFileW can carry its
        // DACL onto the replacement. The temporary file is private before it
        // ever contains a committed connector image.
        set_private_permissions(temp_path)?;
        if destination.exists() {
            set_private_permissions(destination)?;
        }
        Ok(())
    } else if let Ok(metadata) = fs::metadata(destination) {
        fs::set_permissions(temp_path, metadata.permissions()).map_err(internal)
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> ApplicationConnectorResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(internal)
}

#[cfg(windows)]
fn set_private_permissions(path: &Path) -> ApplicationConnectorResult<()> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, GENERIC_ALL, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        EXPLICIT_ACCESS_W, SE_FILE_OBJECT, SET_ACCESS, SetEntriesInAclW, SetNamedSecurityInfoW,
        TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        CreateWellKnownSid, DACL_SECURITY_INFORMATION, GetTokenInformation, NO_INHERITANCE,
        PROTECTED_DACL_SECURITY_INFORMATION, PSID, SECURITY_MAX_SID_SIZE, TOKEN_QUERY, TOKEN_USER,
        TokenUser, WinLocalSystemSid,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    struct HandleGuard(windows_sys::Win32::Foundation::HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(internal(std::io::Error::last_os_error()));
    }
    let _token = HandleGuard(token);

    let mut needed = 0u32;
    unsafe {
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
    }
    if needed < std::mem::size_of::<TOKEN_USER>() as u32 {
        return Err(internal(anyhow::anyhow!(
            "Windows did not return the current user SID"
        )));
    }
    let word = std::mem::size_of::<usize>();
    let mut user_storage = vec![0usize; (needed as usize).div_ceil(word)];
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            user_storage.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(internal(std::io::Error::last_os_error()));
    }
    let user_sid = unsafe { (*(user_storage.as_ptr().cast::<TOKEN_USER>())).User.Sid };

    let mut system_storage = vec![0usize; (SECURITY_MAX_SID_SIZE as usize).div_ceil(word)];
    let mut system_len = SECURITY_MAX_SID_SIZE;
    let system_sid: PSID = system_storage.as_mut_ptr().cast();
    if unsafe {
        CreateWellKnownSid(
            WinLocalSystemSid,
            std::ptr::null_mut(),
            system_sid,
            &mut system_len,
        )
    } == 0
    {
        return Err(internal(std::io::Error::last_os_error()));
    }

    let trustee = |sid: PSID| TRUSTEE_W {
        pMultipleTrustee: std::ptr::null_mut(),
        MultipleTrusteeOperation: 0,
        TrusteeForm: TRUSTEE_IS_SID,
        TrusteeType: TRUSTEE_IS_USER,
        ptstrName: sid.cast::<u16>(),
    };
    let entries = [
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: GENERIC_ALL,
            grfAccessMode: SET_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: trustee(user_sid),
        },
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: GENERIC_ALL,
            grfAccessMode: SET_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: trustee(system_sid),
        },
    ];
    let mut acl = std::ptr::null_mut();
    let acl_status = unsafe {
        SetEntriesInAclW(
            entries.len() as u32,
            entries.as_ptr(),
            std::ptr::null(),
            &mut acl,
        )
    };
    if acl_status != ERROR_SUCCESS {
        return Err(internal(std::io::Error::from_raw_os_error(
            acl_status as i32,
        )));
    }
    struct LocalGuard(*mut c_void);
    impl Drop for LocalGuard {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    let _acl = LocalGuard(acl.cast());

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            acl,
            std::ptr::null(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(internal(std::io::Error::from_raw_os_error(status as i32)));
    }
    verify_windows_private_dacl(path, user_sid, system_sid)
}

#[cfg(windows)]
fn verify_windows_private_dacl(
    path: &Path,
    user_sid: windows_sys::Win32::Security::PSID,
    system_sid: windows_sys::Win32::Security::PSID,
) -> ApplicationConnectorResult<()> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, GENERIC_ALL, LocalFree};
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, AclSizeInformation, DACL_SECURITY_INFORMATION,
        EqualSid, GetAce, GetAclInformation, GetSecurityDescriptorControl, PSECURITY_DESCRIPTOR,
        SE_DACL_PROTECTED,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut acl = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut acl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(internal(std::io::Error::from_raw_os_error(status as i32)));
    }
    struct LocalGuard(*mut c_void);
    impl Drop for LocalGuard {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    let _descriptor = LocalGuard(descriptor.cast());
    if acl.is_null() {
        return Err(precondition("Windows private DACL is missing"));
    }
    let mut control = 0u16;
    let mut revision = 0u32;
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
        || control & SE_DACL_PROTECTED == 0
    {
        return Err(precondition("Windows private DACL is not protected"));
    }
    let mut info = ACL_SIZE_INFORMATION::default();
    if unsafe {
        GetAclInformation(
            acl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
        || info.AceCount != 2
    {
        return Err(precondition(
            "Windows private DACL contains unexpected access entries",
        ));
    }
    let mut saw_user = false;
    let mut saw_system = false;
    for index in 0..info.AceCount {
        let mut raw_ace: *mut c_void = std::ptr::null_mut();
        if unsafe { GetAce(acl, index, &mut raw_ace) } == 0 || raw_ace.is_null() {
            return Err(internal(std::io::Error::last_os_error()));
        }
        let ace = unsafe { &*(raw_ace.cast::<ACCESS_ALLOWED_ACE>()) };
        if ace.Header.AceType as u32 != ACCESS_ALLOWED_ACE_TYPE
            || ace.Header.AceFlags != 0
            || (ace.Mask != GENERIC_ALL && ace.Mask != FILE_ALL_ACCESS)
        {
            return Err(precondition(
                "Windows private DACL contains an unexpected access rule",
            ));
        }
        let sid = std::ptr::addr_of!(ace.SidStart).cast_mut().cast();
        if unsafe { EqualSid(sid, user_sid) } != 0 {
            if saw_user {
                return Err(precondition("Windows private DACL repeats the user rule"));
            }
            saw_user = true;
        } else if unsafe { EqualSid(sid, system_sid) } != 0 {
            if saw_system {
                return Err(precondition("Windows private DACL repeats the system rule"));
            }
            saw_system = true;
        } else {
            return Err(precondition(
                "Windows private DACL grants access to another identity",
            ));
        }
    }
    if !saw_user || !saw_system {
        return Err(precondition(
            "Windows private DACL does not protect the current user",
        ));
    }
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn set_private_permissions(_path: &Path) -> ApplicationConnectorResult<()> {
    Err(precondition(
        "private connector files are unsupported on this platform",
    ))
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> ApplicationConnectorResult<()> {
    use std::os::windows::ffi::OsStrExt;
    type Bool = i32;
    unsafe extern "system" {
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> Bool;
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> Bool;
    }
    const REPLACEFILE_WRITE_THROUGH: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let destination_exists = destination.exists();
    let source = wide(source);
    let destination = wide(destination);
    let ok = unsafe {
        if destination_exists {
            ReplaceFileW(
                destination.as_ptr(),
                source.as_ptr(),
                std::ptr::null(),
                REPLACEFILE_WRITE_THROUGH,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } else {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if ok == 0 {
        Err(internal(std::io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> ApplicationConnectorResult<()> {
    fs::rename(source, destination).map_err(internal)
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> ApplicationConnectorResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| internal_message("connector target has no parent"))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(internal)
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> ApplicationConnectorResult<()> {
    Ok(())
}

fn remove_internal_file(path: &Path) -> ApplicationConnectorResult<()> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent(path),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(internal(error)),
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn invalid(message: &str) -> ApplicationConnectorError {
    ApplicationConnectorError::new(ApplicationConnectorErrorKind::InvalidRequest, message)
}

fn precondition(message: &str) -> ApplicationConnectorError {
    ApplicationConnectorError::new(ApplicationConnectorErrorKind::Precondition, message)
}

fn conflict(message: &str) -> ApplicationConnectorError {
    ApplicationConnectorError::new(ApplicationConnectorErrorKind::Conflict, message)
}

fn internal(error: impl Into<anyhow::Error>) -> ApplicationConnectorError {
    let error = error.into();
    ApplicationConnectorError::new(ApplicationConnectorErrorKind::Internal, error.to_string())
}

fn internal_message(message: &str) -> ApplicationConnectorError {
    ApplicationConnectorError::new(ApplicationConnectorErrorKind::Internal, message)
}

#[cfg(test)]
mod tests;
