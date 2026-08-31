//! Native-package connector engine for Pi and DSH.
//!
//! This module deliberately owns only generated, immutable package sources in
//! OCG Manager's data directory. Pi remains responsible for its credential
//! through its native login flow. DSH receives its credential through the
//! separate field-owned configuration engine; this package engine never places
//! a gateway key or environment value in package source, command-line
//! arguments, or diagnostic output.

use ocg_core::application_connectors::{
    ApplicationConnectorAction, ApplicationConnectorChange, ApplicationConnectorCommitResult,
    ApplicationConnectorError, ApplicationConnectorErrorKind, ApplicationConnectorHostRequest,
    ApplicationConnectorId, ApplicationConnectorInspection, ApplicationConnectorPreview,
    ApplicationConnectorResult, ApplicationConnectorStatus,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const PACKAGE_ROOT: &str = "application-connectors/ocg-manager-native-plugin-sources-v1";
const DSH_PACKAGE_NAME: &str = "ocg-manager-dsh";
const DSH_PROFILE: &str = "web";
const MAX_COMMAND_OUTPUT: usize = 16 * 1024;
const MAX_PACKAGE_FILES: usize = 64;
const MAX_PACKAGE_BYTES: u64 = 4 * 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const RECONCILE_TIMEOUT: Duration = Duration::from_secs(2);

/// Placeholder names are intentionally narrow: templates may only depend on
/// the gateway URL and the selected public model aliases.
pub const GATEWAY_URL_PLACEHOLDER: &str = "__OCG_GATEWAY_URL__";
pub const MODELS_JSON_PLACEHOLDER: &str = "__OCG_MODELS_JSON__";
pub const MODELS_LINES_PLACEHOLDER: &str = "__OCG_MODELS_LINES__";
pub const GENERATED_MODELS_PLACEHOLDER: &str = "__OCG_MANAGER_GENERATED_MODELS__";
const DEFAULT_GATEWAY_V1_URL: &str = "http://127.0.0.1:9042/v1";

/// Package files supplied by the application-owned integration templates.
///
/// The host copies these bytes into a digest-named directory rather than
/// modifying a previous package.  Callers normally build this with
/// `include_str!` from `integrations/pi` and `integrations/dsh`.
#[derive(Clone, Copy)]
pub(crate) struct NativePluginTemplates {
    pub pi: &'static [NativePluginTemplateFile],
    pub dsh: &'static [NativePluginTemplateFile],
}

#[derive(Clone, Copy)]
pub(crate) struct NativePluginTemplateFile {
    pub relative_path: &'static str,
    pub contents: &'static str,
}

/// Explicit roots make every filesystem boundary testable without looking at
/// a real client profile.
#[derive(Debug, Clone)]
pub(crate) struct NativePluginRoots {
    pub data_dir: PathBuf,
    pub pi_settings: PathBuf,
    pub dsh_web_manifest: PathBuf,
}

impl NativePluginRoots {
    pub(crate) fn from_home(data_dir: PathBuf, home: &Path) -> Self {
        Self {
            data_dir,
            pi_settings: home.join(".pi/agent/settings.json"),
            dsh_web_manifest: home.join(".dsh/profiles/web/package.json"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativePluginCommand {
    pub executable: PathBuf,
    pub display_executable: String,
    pub args: Vec<OsString>,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct NativePluginCommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Command execution is injected so connector tests cannot execute a local
/// Pi/DSH installation.  Implementations must not append arguments or pass a
/// secret through another channel.
pub(crate) trait NativePluginCommandRunner: Send + Sync {
    fn run(&self, command: &NativePluginCommand) -> Result<NativePluginCommandOutput, String>;
}

#[derive(Default)]
pub(crate) struct ProcessNativePluginCommandRunner;

impl NativePluginCommandRunner for ProcessNativePluginCommandRunner {
    fn run(&self, command: &NativePluginCommand) -> Result<NativePluginCommandOutput, String> {
        #[cfg(windows)]
        {
            run_windows_native_command(command)
        }
        #[cfg(not(windows))]
        {
            let mut process = Command::new(&command.executable);
            process
                .args(&command.args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            #[cfg(unix)]
            process.process_group(0);
            let mut child = process.spawn().map_err(|error| {
                format!("failed to start {}: {error}", command.display_executable)
            })?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| "failed to capture stdout".to_string())?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| "failed to capture stderr".to_string())?;
            let stdout_reader = thread::spawn(move || read_bounded(stdout));
            let stderr_reader = thread::spawn(move || read_bounded(stderr));
            let started = Instant::now();
            let status = loop {
                match child.try_wait().map_err(|error| error.to_string())? {
                    Some(status) => break status,
                    None if started.elapsed() >= command.timeout => {
                        terminate_non_windows_process_tree(&mut child);
                        return Err(format!("{} timed out", command.display_executable));
                    }
                    None => thread::sleep(Duration::from_millis(20)),
                }
            };
            let stdout = stdout_reader
                .join()
                .map_err(|_| "stdout reader panicked".to_string())?;
            let stderr = stderr_reader
                .join()
                .map_err(|_| "stderr reader panicked".to_string())?;
            Ok(NativePluginCommandOutput {
                success: status.success(),
                stdout: redact_output(&stdout),
                stderr: redact_output(&stderr),
            })
        }
    }
}

#[cfg(not(windows))]
fn terminate_non_windows_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;

        let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
struct WindowsProcessTree(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl WindowsProcessTree {
    fn new() -> Result<Self, String> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(format!(
                    "failed to create native package process boundary: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&information as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                let error = std::io::Error::last_os_error();
                CloseHandle(job);
                return Err(format!(
                    "failed to configure native package process boundary: {error}"
                ));
            }
            Ok(Self(job))
        }
    }

    fn assign(&self, process: windows_sys::Win32::Foundation::HANDLE) -> Result<(), String> {
        let assigned = unsafe {
            windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(self.0, process)
        };
        if assigned == 0 {
            return Err(format!(
                "failed to contain native package process tree: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    fn terminate(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
        }
    }
}

#[cfg(windows)]
struct OwnedWindowsHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl OwnedWindowsHandle {
    fn into_file(mut self) -> std::fs::File {
        use std::os::windows::io::FromRawHandle;
        let handle = self.0;
        self.0 = std::ptr::null_mut();
        unsafe { std::fs::File::from_raw_handle(handle as _) }
    }
}

#[cfg(windows)]
impl Drop for OwnedWindowsHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
fn windows_pipe(parent_reads: bool) -> Result<(OwnedWindowsHandle, OwnedWindowsHandle), String> {
    use windows_sys::Win32::Foundation::{HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation};
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::Pipes::CreatePipe;

    unsafe {
        let mut read: HANDLE = std::ptr::null_mut();
        let mut write: HANDLE = std::ptr::null_mut();
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        if CreatePipe(&mut read, &mut write, &attributes, 0) == 0 {
            return Err(format!(
                "failed to create native package output pipe: {}",
                std::io::Error::last_os_error()
            ));
        }
        let read = OwnedWindowsHandle(read);
        let write = OwnedWindowsHandle(write);
        let parent_handle = if parent_reads { read.0 } else { write.0 };
        if SetHandleInformation(parent_handle, HANDLE_FLAG_INHERIT, 0) == 0 {
            return Err(format!(
                "failed to protect native package output pipe: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok((read, write))
    }
}

#[cfg(windows)]
fn run_windows_native_command(
    command: &NativePluginCommand,
) -> Result<NativePluginCommandOutput, String> {
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateProcessW, GetExitCodeProcess,
        PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOW, WaitForSingleObject,
    };

    let (application, mut command_line) = windows_process_command_line(command)?;
    let (stdout_read, stdout_write) = windows_pipe(true)?;
    let (stderr_read, stderr_write) = windows_pipe(true)?;
    let (stdin_read, stdin_write) = windows_pipe(false)?;
    let job = WindowsProcessTree::new()?;
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdInput = stdin_read.0;
    startup.hStdOutput = stdout_write.0;
    startup.hStdError = stderr_write.0;
    let mut process_information: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_SUSPENDED | CREATE_NO_WINDOW,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut process_information,
        )
    };
    if created == 0 {
        return Err(format!(
            "failed to start {}: {}",
            command.display_executable,
            std::io::Error::last_os_error()
        ));
    }
    let process = OwnedWindowsHandle(process_information.hProcess);
    let primary_thread = OwnedWindowsHandle(process_information.hThread);
    job.assign(process.0).inspect_err(|_| {
        job.terminate();
    })?;

    drop(stdout_write);
    drop(stderr_write);
    drop(stdin_read);
    drop(stdin_write);
    let stdout_file = stdout_read.into_file();
    let stderr_file = stderr_read.into_file();
    let stdout_reader = thread::spawn(move || read_bounded(stdout_file));
    let stderr_reader = thread::spawn(move || read_bounded(stderr_file));

    if unsafe { ResumeThread(primary_thread.0) } == u32::MAX {
        job.terminate();
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        return Err(format!(
            "failed to resume {}: {}",
            command.display_executable,
            std::io::Error::last_os_error()
        ));
    }
    drop(primary_thread);

    let started = Instant::now();
    let mut timed_out = false;
    loop {
        match unsafe { WaitForSingleObject(process.0, 20) } {
            WAIT_OBJECT_0 => break,
            WAIT_TIMEOUT if started.elapsed() >= command.timeout => {
                timed_out = true;
                break;
            }
            WAIT_TIMEOUT => {}
            _ => {
                job.terminate();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!(
                    "failed while waiting for {}: {}",
                    command.display_executable,
                    std::io::Error::last_os_error()
                ));
            }
        }
    }
    job.terminate();
    unsafe {
        WaitForSingleObject(process.0, 5_000);
    }
    let stdout = stdout_reader
        .join()
        .map_err(|_| "stdout reader panicked".to_string())?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "stderr reader panicked".to_string())?;
    if timed_out {
        return Err(format!("{} timed out", command.display_executable));
    }
    let mut exit_code = 1u32;
    if unsafe { GetExitCodeProcess(process.0, &mut exit_code) } == 0 {
        return Err(format!(
            "failed to read {} exit status: {}",
            command.display_executable,
            std::io::Error::last_os_error()
        ));
    }
    Ok(NativePluginCommandOutput {
        success: exit_code == 0,
        stdout: redact_output(&stdout),
        stderr: redact_output(&stderr),
    })
}

#[cfg(windows)]
fn windows_process_command_line(
    command: &NativePluginCommand,
) -> Result<(Vec<u16>, Vec<u16>), String> {
    use std::os::windows::ffi::OsStrExt;

    let extension = command
        .executable
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let (application, command_line) =
        if extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat") {
            let command_processor = std::env::var_os("ComSpec")
                .map(PathBuf::from)
                .filter(|path| path.is_file())
                .or_else(|| {
                    std::env::var_os("SystemRoot")
                        .map(PathBuf::from)
                        .map(|root| root.join("System32/cmd.exe"))
                        .filter(|path| path.is_file())
                })
                .ok_or_else(|| "Windows command processor was not found".to_string())?;
            let script = command
                .executable
                .to_str()
                .ok_or_else(|| "native package command path is not Unicode".to_string())?;
            let mut shell_command = String::from("\"");
            for value in std::iter::once(script).chain(
                command
                    .args
                    .iter()
                    .map(|value| value.to_str().unwrap_or("\0")),
            ) {
                if value.contains(['\0', '\r', '\n', '%', '!']) {
                    return Err("native package batch command contains unsafe characters".into());
                }
                shell_command.push_str(&quote_windows_argument_always(value));
                shell_command.push(' ');
            }
            shell_command.pop();
            shell_command.push('"');
            let command_processor_text = command_processor
                .to_str()
                .ok_or_else(|| "Windows command processor path is not Unicode".to_string())?
                .to_string();
            (
                command_processor,
                format!(
                    "{} /d /s /c {}",
                    quote_windows_argument(&command_processor_text),
                    shell_command
                ),
            )
        } else {
            let application_text = command
                .executable
                .to_str()
                .ok_or_else(|| "native package command path is not Unicode".to_string())?;
            let mut command_line = quote_windows_argument(application_text);
            for argument in &command.args {
                let argument = argument
                    .to_str()
                    .ok_or_else(|| "native package command argument is not Unicode".to_string())?;
                if argument.contains('\0') {
                    return Err("native package command argument contains NUL".into());
                }
                command_line.push(' ');
                command_line.push_str(&quote_windows_argument(argument));
            }
            (command.executable.clone(), command_line)
        };
    let mut application = application.as_os_str().encode_wide().collect::<Vec<_>>();
    application.push(0);
    let mut command_line = command_line.encode_utf16().collect::<Vec<_>>();
    command_line.push(0);
    Ok((application, command_line))
}

#[cfg(windows)]
fn quote_windows_argument(value: &str) -> String {
    if !value.is_empty()
        && !value
            .chars()
            .any(|character| character.is_whitespace() || character == '"')
    {
        return value.to_string();
    }
    quote_windows_argument_always(value)
}

#[cfg(windows)]
fn quote_windows_argument_always(value: &str) -> String {
    let mut quoted = String::from("\"");
    let mut backslashes = 0usize;
    for character in value.chars() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }
        if character == '"' {
            quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
            quoted.push('"');
        } else {
            quoted.push_str(&"\\".repeat(backslashes));
            quoted.push(character);
        }
        backslashes = 0;
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(windows)]
impl Drop for WindowsProcessTree {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

/// The native package engine.  It is intentionally independent of the
/// general field-level connector writer; the primary host may delegate Pi/DSH
/// requests to it while retaining the common transaction lock.
pub(crate) struct NativePluginHost {
    roots: NativePluginRoots,
    templates: NativePluginTemplates,
    runner: Arc<dyn NativePluginCommandRunner>,
    pi_executable: Option<PathBuf>,
    dsh_executable: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DshPluginSnapshot {
    Absent,
    Connected(PathBuf),
}

pub(crate) struct DshConnectCompensation {
    before: DshPluginSnapshot,
    expected_after: DshPluginSnapshot,
}

impl NativePluginHost {
    pub(crate) fn new(
        roots: NativePluginRoots,
        templates: NativePluginTemplates,
        runner: Arc<dyn NativePluginCommandRunner>,
        pi_executable: Option<PathBuf>,
        dsh_executable: Option<PathBuf>,
    ) -> Self {
        Self {
            roots,
            templates,
            runner,
            pi_executable,
            dsh_executable,
        }
    }

    pub(crate) fn owns(id: ApplicationConnectorId) -> bool {
        matches!(id, ApplicationConnectorId::Pi | ApplicationConnectorId::Dsh)
    }

    pub(crate) fn inspect(&self, id: ApplicationConnectorId) -> ApplicationConnectorInspection {
        debug_assert!(Self::owns(id));
        let executable = self.resolve_executable(id);
        let target_paths = self.target_paths(id, executable.as_ref());
        let Some(_executable) = executable else {
            return inspection(
                id,
                ApplicationConnectorStatus::NotDetected,
                false,
                Some("native client executable was not found".into()),
                target_paths,
            );
        };

        let installed = self.install_state(id);
        let (status, detail) = match installed {
            InstallState::Connected { .. } => (ApplicationConnectorStatus::Connected, None),
            InstallState::Partial(detail) => (ApplicationConnectorStatus::Partial, Some(detail)),
            InstallState::Absent => (ApplicationConnectorStatus::Ready, None),
        };
        inspection(id, status, true, detail, target_paths)
    }

    pub(crate) fn preview(
        &self,
        request: &ApplicationConnectorHostRequest,
    ) -> ApplicationConnectorResult<ApplicationConnectorPreview> {
        self.ensure_request(request)?;
        let current = self.inspect(request.id);
        if matches!(current.status, ApplicationConnectorStatus::NotDetected) {
            return Err(precondition("native client executable was not found"));
        }
        if matches!(current.status, ApplicationConnectorStatus::Partial) {
            return Err(conflict(
                "native plugin installation is partial; restore it before connecting",
            ));
        }
        let package = match request.action {
            ApplicationConnectorAction::Connect => Some(self.render_package(request)?),
            ApplicationConnectorAction::Restore => None,
        };
        let current_state = self.state_bytes(request.id)?;
        let executable = self
            .resolve_executable(request.id)
            .ok_or_else(|| precondition("native client executable was not found"))?;
        let already_connected = current.status == ApplicationConnectorStatus::Connected;
        let registered_source = match self.install_state(request.id) {
            InstallState::Connected { source } => source,
            InstallState::Absent | InstallState::Partial(_) => None,
        };
        let restore_source = self.restore_source(request.id, request.action)?;
        let changed = match request.action {
            ApplicationConnectorAction::Connect => {
                let desired = package.as_ref().expect("connect has a package");
                !already_connected
                    || !desired.exists_and_matches()
                    || !registered_source
                        .as_deref()
                        .is_some_and(|source| same_lexical_path(source, &desired.path))
            }
            ApplicationConnectorAction::Restore => already_connected,
        };
        let changes = if changed {
            vec![ApplicationConnectorChange {
                field: match request.action {
                    ApplicationConnectorAction::Connect => "Native package".into(),
                    ApplicationConnectorAction::Restore => "Native package installation".into(),
                },
                before: Some(match request.action {
                    ApplicationConnectorAction::Connect if already_connected => "installed".into(),
                    ApplicationConnectorAction::Connect => "not installed".into(),
                    ApplicationConnectorAction::Restore => "installed".into(),
                }),
                after: Some(match request.action {
                    ApplicationConnectorAction::Connect => {
                        "install OCG-managed native package".into()
                    }
                    ApplicationConnectorAction::Restore => "removed".into(),
                }),
                sensitive: false,
            }]
        } else {
            Vec::new()
        };
        Ok(ApplicationConnectorPreview {
            id: request.id,
            action: request.action,
            status: if request.action == ApplicationConnectorAction::Connect && !changed {
                ApplicationConnectorStatus::Connected
            } else {
                ApplicationConnectorStatus::Ready
            },
            fingerprint: fingerprint(
                request,
                &executable,
                &current_state,
                package.as_ref(),
                restore_source.as_deref(),
            ),
            detail: None,
            target_paths: self.target_paths(request.id, Some(&executable)),
            changes,
        })
    }

    pub(crate) fn commit(
        &self,
        request: &ApplicationConnectorHostRequest,
    ) -> ApplicationConnectorResult<ApplicationConnectorCommitResult> {
        let expected = request
            .preview_fingerprint
            .as_deref()
            .ok_or_else(|| invalid("preview fingerprint is required"))?;
        self.commit_with_fingerprint(request, expected)
    }

    pub(crate) fn commit_with_fingerprint(
        &self,
        request: &ApplicationConnectorHostRequest,
        expected: &str,
    ) -> ApplicationConnectorResult<ApplicationConnectorCommitResult> {
        let preview = self.preview(request)?;
        if preview.fingerprint != expected {
            return Err(conflict("native package state changed since preview"));
        }
        if preview.changes.is_empty() {
            return Ok(ApplicationConnectorCommitResult {
                inspection: self.inspect(request.id),
                changed: false,
            });
        }
        let package = match request.action {
            ApplicationConnectorAction::Connect => Some(self.render_package(request)?),
            ApplicationConnectorAction::Restore => None,
        };
        let executable = self
            .resolve_executable(request.id)
            .ok_or_else(|| precondition("native client executable was not found"))?;
        if request.action == ApplicationConnectorAction::Connect {
            package
                .as_ref()
                .expect("connect has a package")
                .materialize()?;
        }
        let restore_source = self.restore_source(request.id, request.action)?;
        let command = native_command(
            request.id,
            executable,
            request.action,
            package.as_ref().map(|package| package.path.as_path()),
            restore_source.as_deref(),
        )?;
        let output = self.runner.run(&command).map_err(internal)?;
        let output = NativePluginCommandOutput {
            success: output.success,
            stdout: redact_output(&output.stdout),
            stderr: redact_output(&output.stderr),
        };
        if !output.success {
            return Err(internal(command_failure(&command, &output)));
        }
        let inspection = self.reconcile_registry(
            request.id,
            request.action,
            package.as_ref().map(|package| package.path.as_path()),
        )?;
        Ok(ApplicationConnectorCommitResult {
            inspection,
            changed: true,
        })
    }

    fn snapshot_dsh_installation(&self) -> ApplicationConnectorResult<DshPluginSnapshot> {
        match self.install_state(ApplicationConnectorId::Dsh) {
            InstallState::Absent => Ok(DshPluginSnapshot::Absent),
            InstallState::Connected {
                source: Some(source),
            } => Ok(DshPluginSnapshot::Connected(source)),
            InstallState::Connected { source: None } => Err(conflict(
                "DSH installation has no registered package source",
            )),
            InstallState::Partial(detail) => Err(conflict(detail)),
        }
    }

    pub(crate) fn prepare_dsh_connect_compensation(
        &self,
        request: &ApplicationConnectorHostRequest,
    ) -> ApplicationConnectorResult<DshConnectCompensation> {
        if request.id != ApplicationConnectorId::Dsh
            || request.action != ApplicationConnectorAction::Connect
        {
            return Err(invalid(
                "DSH compensation can only be prepared for a DSH connect",
            ));
        }
        Ok(DshConnectCompensation {
            before: self.snapshot_dsh_installation()?,
            expected_after: DshPluginSnapshot::Connected(self.render_package(request)?.path),
        })
    }

    pub(crate) fn restore_dsh_installation(
        &self,
        compensation: &DshConnectCompensation,
    ) -> ApplicationConnectorResult<ApplicationConnectorInspection> {
        let current = self.install_state(ApplicationConnectorId::Dsh);
        if dsh_snapshot_matches(&compensation.before, &current) {
            return Ok(self.inspect(ApplicationConnectorId::Dsh));
        }
        if !dsh_snapshot_matches(&compensation.expected_after, &current) {
            return Err(conflict(
                "DSH plugin state changed after installation; compensation stopped without modifying it",
            ));
        }

        let executable = self
            .resolve_executable(ApplicationConnectorId::Dsh)
            .ok_or_else(|| precondition("native client executable was not found"))?;
        let (action, source) = match &compensation.before {
            DshPluginSnapshot::Absent => (ApplicationConnectorAction::Restore, None),
            DshPluginSnapshot::Connected(source) => {
                if !is_owned_package_source(
                    source,
                    &self.roots.data_dir,
                    &package_root(&self.roots.data_dir, ApplicationConnectorId::Dsh),
                    ApplicationConnectorId::Dsh,
                ) {
                    return Err(conflict(
                        "the previous DSH package source is no longer an exact OCG-owned package",
                    ));
                }
                (ApplicationConnectorAction::Connect, Some(source.as_path()))
            }
        };
        let command = native_command(
            ApplicationConnectorId::Dsh,
            executable,
            action,
            source,
            None,
        )?;
        let output = self.runner.run(&command).map_err(internal)?;
        let output = NativePluginCommandOutput {
            success: output.success,
            stdout: redact_output(&output.stdout),
            stderr: redact_output(&output.stderr),
        };
        if !output.success {
            return Err(internal(command_failure(&command, &output)));
        }
        self.reconcile_registry(ApplicationConnectorId::Dsh, action, source)
    }

    fn ensure_request(
        &self,
        request: &ApplicationConnectorHostRequest,
    ) -> ApplicationConnectorResult<()> {
        if !Self::owns(request.id) {
            return Err(invalid("native plugin host only supports Pi and DSH"));
        }
        if request.id == ApplicationConnectorId::Pi && request.secret.is_some() {
            return Err(invalid("Pi stores its gateway key through native login"));
        }
        Ok(())
    }

    fn render_package(
        &self,
        request: &ApplicationConnectorHostRequest,
    ) -> ApplicationConnectorResult<RenderedPackage> {
        let models = models(&request.model_values)?;
        let templates = match request.id {
            ApplicationConnectorId::Pi => self.templates.pi,
            ApplicationConnectorId::Dsh => self.templates.dsh,
            _ => return Err(invalid("native plugin host only supports Pi and DSH")),
        };
        render_package(
            &self.roots.data_dir,
            request.id,
            templates,
            &request.gateway_url,
            &models,
        )
    }

    fn state_bytes(&self, id: ApplicationConnectorId) -> ApplicationConnectorResult<Vec<u8>> {
        match id {
            ApplicationConnectorId::Pi => read_optional(&self.roots.pi_settings),
            ApplicationConnectorId::Dsh => read_optional(&self.roots.dsh_web_manifest),
            _ => unreachable!("caller must use NativePluginHost::owns"),
        }
    }

    fn install_state(&self, id: ApplicationConnectorId) -> InstallState {
        match id {
            ApplicationConnectorId::Pi => inspect_pi(
                &self.roots.pi_settings,
                &self.roots.data_dir,
                &package_root(&self.roots.data_dir, ApplicationConnectorId::Pi),
            ),
            ApplicationConnectorId::Dsh => inspect_dsh(
                &self.roots.dsh_web_manifest,
                &self.roots.data_dir,
                &package_root(&self.roots.data_dir, ApplicationConnectorId::Dsh),
            ),
            _ => unreachable!("caller must use NativePluginHost::owns"),
        }
    }

    fn restore_source(
        &self,
        id: ApplicationConnectorId,
        action: ApplicationConnectorAction,
    ) -> ApplicationConnectorResult<Option<PathBuf>> {
        if action != ApplicationConnectorAction::Restore || id != ApplicationConnectorId::Pi {
            return Ok(None);
        }
        match self.install_state(ApplicationConnectorId::Pi) {
            InstallState::Connected {
                source: Some(source),
            } => Ok(Some(source)),
            InstallState::Connected { source: None } => Err(conflict(
                "Pi installation has no registered native package source",
            )),
            InstallState::Absent => Ok(None),
            InstallState::Partial(detail) => Err(conflict(detail)),
        }
    }

    fn reconcile_registry(
        &self,
        id: ApplicationConnectorId,
        action: ApplicationConnectorAction,
        expected_source: Option<&Path>,
    ) -> ApplicationConnectorResult<ApplicationConnectorInspection> {
        let started = Instant::now();
        loop {
            let state = self.install_state(id);
            let matches = match (&state, action) {
                (
                    InstallState::Connected {
                        source: Some(source),
                    },
                    ApplicationConnectorAction::Connect,
                ) => expected_source.is_some_and(|expected| same_lexical_path(source, expected)),
                (InstallState::Absent, ApplicationConnectorAction::Restore) => true,
                _ => false,
            };
            if matches {
                return Ok(self.inspect(id));
            }
            if started.elapsed() >= RECONCILE_TIMEOUT {
                let detail = match state {
                    InstallState::Partial(detail) => detail,
                    InstallState::Connected { .. } => {
                        "the client registered a different native package source".into()
                    }
                    InstallState::Absent => {
                        "the client registry did not record the native package".into()
                    }
                };
                return Err(precondition(format!(
                    "native package command finished but its registry state was not confirmed: {detail}"
                )));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn resolve_executable(&self, id: ApplicationConnectorId) -> Option<ResolvedExecutable> {
        let configured = match id {
            ApplicationConnectorId::Pi => self.pi_executable.as_deref(),
            ApplicationConnectorId::Dsh => self.dsh_executable.as_deref(),
            _ => return None,
        };
        resolve_executable(configured.unwrap_or_else(|| {
            Path::new(match id {
                ApplicationConnectorId::Pi => "pi",
                ApplicationConnectorId::Dsh => "dsh",
                _ => unreachable!(),
            })
        }))
    }

    fn target_paths(
        &self,
        id: ApplicationConnectorId,
        _executable: Option<&ResolvedExecutable>,
    ) -> Vec<String> {
        vec![
            match id {
                ApplicationConnectorId::Pi => "Pi user package registry".into(),
                ApplicationConnectorId::Dsh => "DSH web profile package registry".into(),
                _ => unreachable!(),
            },
            "OCG-managed native package source".into(),
        ]
    }
}

#[derive(Debug, Clone)]
struct ResolvedExecutable {
    path: PathBuf,
    display: String,
}

#[derive(Debug, Clone)]
struct RenderedPackage {
    trusted_root: PathBuf,
    path: PathBuf,
    files: BTreeMap<PathBuf, Vec<u8>>,
    digest: String,
}

impl RenderedPackage {
    fn exists_and_matches(&self) -> bool {
        if !safe_directory_chain(&self.trusted_root, &self.path) {
            return false;
        }
        package_files_from_disk(&self.path)
            .map(|actual| actual == self.files)
            .unwrap_or(false)
    }

    fn materialize(&self) -> ApplicationConnectorResult<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| internal("invalid native package directory"))?;
        ensure_safe_directory_chain(&self.trusted_root, parent)?;
        match fs::symlink_metadata(&self.path) {
            Ok(_) => {
                if self.exists_and_matches() {
                    return Ok(());
                }
                return Err(conflict(
                    "immutable native package directory does not match its digest",
                ));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                fs::create_dir(&self.path).map_err(internal)?;
            }
            Err(error) => return Err(internal(error)),
        }
        if !safe_directory_chain(&self.trusted_root, &self.path) {
            return Err(conflict(
                "native package directory escaped its trusted root",
            ));
        }
        for (relative, bytes) in &self.files {
            let destination = self.path.join(relative);
            let parent = destination
                .parent()
                .ok_or_else(|| internal("invalid package template path"))?;
            ensure_safe_directory_chain(&self.trusted_root, parent)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .map_err(internal)?;
            file.write_all(bytes).map_err(internal)?;
            file.sync_all().map_err(internal)?;
        }
        if !self.exists_and_matches() {
            return Err(internal("failed to materialize immutable native package"));
        }
        Ok(())
    }
}

fn is_link_or_reparse(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn ensure_safe_directory_chain(
    trusted_root: &Path,
    target: &Path,
) -> ApplicationConnectorResult<()> {
    let trusted_root = canonical_lexical_path(trusted_root).map_err(conflict)?;
    let target = canonical_lexical_path(target).map_err(conflict)?;
    let relative = target
        .strip_prefix(&trusted_root)
        .map_err(|_| conflict("native package directory escaped its trusted root"))?;
    fs::create_dir_all(&trusted_root).map_err(internal)?;
    let canonical_root = fs::canonicalize(&trusted_root).map_err(internal)?;
    let mut current = trusted_root;
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if !metadata.file_type().is_dir() || is_link_or_reparse(&current) {
                    return Err(conflict(
                        "native package directory contains a link or non-directory ancestor",
                    ));
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(internal)?;
            }
            Err(error) => return Err(internal(error)),
        }
        let canonical = fs::canonicalize(&current).map_err(internal)?;
        if !canonical.starts_with(&canonical_root) {
            return Err(conflict(
                "native package directory escaped its trusted root",
            ));
        }
    }
    Ok(())
}

fn safe_directory_chain(trusted_root: &Path, target: &Path) -> bool {
    let Ok(trusted_root) = canonical_lexical_path(trusted_root) else {
        return false;
    };
    let Ok(target) = canonical_lexical_path(target) else {
        return false;
    };
    let Ok(relative) = target.strip_prefix(&trusted_root) else {
        return false;
    };
    let Ok(canonical_root) = fs::canonicalize(&trusted_root) else {
        return false;
    };
    let mut current = trusted_root;
    for component in relative.components() {
        current.push(component.as_os_str());
        let Ok(metadata) = fs::symlink_metadata(&current) else {
            return false;
        };
        if !metadata.file_type().is_dir() || is_link_or_reparse(&current) {
            return false;
        }
        let Ok(canonical) = fs::canonicalize(&current) else {
            return false;
        };
        if !canonical.starts_with(&canonical_root) {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InstallState {
    Absent,
    Connected { source: Option<PathBuf> },
    Partial(String),
}

fn dsh_snapshot_matches(snapshot: &DshPluginSnapshot, state: &InstallState) -> bool {
    match (snapshot, state) {
        (DshPluginSnapshot::Absent, InstallState::Absent) => true,
        (
            DshPluginSnapshot::Connected(expected),
            InstallState::Connected {
                source: Some(current),
            },
        ) => same_lexical_path(expected, current),
        _ => false,
    }
}

fn inspect_pi(settings: &Path, trusted_root: &Path, owned_sources: &Path) -> InstallState {
    match fs::read(settings) {
        Ok(content) => match serde_json::from_slice::<Value>(&content) {
            Ok(settings) => match registered_pi_sources(&settings, owned_sources) {
                Ok(sources) if sources.is_empty() => InstallState::Absent,
                Ok(mut sources) if sources.len() == 1 => {
                    let source = sources.pop_first().expect("one source was checked");
                    if is_owned_package_source(
                        &source,
                        trusted_root,
                        owned_sources,
                        ApplicationConnectorId::Pi,
                    ) {
                        InstallState::Connected {
                            source: Some(source),
                        }
                    } else {
                        InstallState::Partial(
                            "Pi settings reference an OCG package source that no longer exists"
                                .into(),
                        )
                    }
                }
                Ok(_) => InstallState::Partial(
                    "Pi settings reference multiple OCG package sources; restore before connecting"
                        .into(),
                ),
                Err(error) => InstallState::Partial(error),
            },
            Err(error) => InstallState::Partial(format!("invalid Pi settings: {error}")),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => InstallState::Absent,
        Err(error) => InstallState::Partial(format!("could not read Pi settings: {error}")),
    }
}

fn is_owned_package_source(
    source: &Path,
    trusted_root: &Path,
    owned_sources: &Path,
    id: ApplicationConnectorId,
) -> bool {
    let Ok(source) = canonical_lexical_path(source) else {
        return false;
    };
    let Ok(owned_sources) = canonical_lexical_path(owned_sources) else {
        return false;
    };
    if source.parent() != Some(owned_sources.as_path())
        || !safe_directory_chain(trusted_root, &source)
    {
        return false;
    }
    let Some(directory_name) = source.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if directory_name.len() != 24
        || !directory_name
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        return false;
    }
    let Ok(files) = package_files_from_disk(&source) else {
        return false;
    };
    let expected_name = match id {
        ApplicationConnectorId::Pi => "ocg-manager-pi",
        ApplicationConnectorId::Dsh => DSH_PACKAGE_NAME,
        _ => return false,
    };
    let package_name_matches = files
        .get(Path::new("package.json"))
        .and_then(|content| serde_json::from_slice::<Value>(content).ok())
        .and_then(|value| value.get("name").and_then(Value::as_str).map(str::to_owned))
        .is_some_and(|name| name == expected_name);
    let digest = package_digest(id, &files);
    package_name_matches && digest.get(..24) == Some(directory_name)
}

fn package_files_from_disk(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    let metadata = fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("native package source is not a regular directory".into());
    }
    let mut files = BTreeMap::new();
    collect_package_files(root, root, &mut files)?;
    if files.is_empty() {
        return Err("native package source is empty".into());
    }
    Ok(files)
}

fn collect_package_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if is_link_or_reparse(&path) {
            return Err("native package source contains a link".into());
        }
        if metadata.file_type().is_dir() {
            collect_package_files(root, &path, files)?;
            continue;
        }
        if !metadata.file_type().is_file() || metadata.len() > MAX_PACKAGE_BYTES {
            return Err("native package source contains an unsupported file".into());
        }
        if files.len() >= MAX_PACKAGE_FILES {
            return Err("native package source contains too many files".into());
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "native package file escaped its root".to_string())?
            .to_path_buf();
        files.insert(
            relative,
            fs::read(&path).map_err(|error| error.to_string())?,
        );
    }
    Ok(())
}

fn registered_pi_sources(
    settings: &Value,
    owned_sources: &Path,
) -> Result<std::collections::BTreeSet<PathBuf>, String> {
    let owned_sources = canonical_lexical_path(owned_sources)?;
    let mut values = Vec::new();
    collect_json_strings(settings, &mut values);
    let mut sources = std::collections::BTreeSet::new();
    for value in values {
        let Some(candidate) = pi_source_path(value) else {
            continue;
        };
        let candidate = canonical_lexical_path(&candidate)?;
        if candidate.strip_prefix(&owned_sources).is_ok() {
            sources.insert(candidate);
        }
    }
    Ok(sources)
}

fn collect_json_strings<'a>(value: &'a Value, values: &mut Vec<&'a str>) {
    match value {
        Value::String(value) => values.push(value),
        Value::Array(items) => {
            for item in items {
                collect_json_strings(item, values);
            }
        }
        Value::Object(fields) => {
            for value in fields.values() {
                collect_json_strings(value, values);
            }
        }
        _ => {}
    }
}

fn pi_source_path(value: &str) -> Option<PathBuf> {
    let value = value.strip_prefix("file:").unwrap_or(value);
    let value = value.strip_prefix("//").unwrap_or(value);
    #[cfg(windows)]
    let value = value
        .strip_prefix('/')
        .filter(|path| path.as_bytes().get(1) == Some(&b':'))
        .unwrap_or(value);
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

fn canonical_lexical_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("native package source path must be absolute".into());
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return Err("native package source path escapes its root".into());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

fn same_lexical_path(left: &Path, right: &Path) -> bool {
    canonical_lexical_path(left).ok() == canonical_lexical_path(right).ok()
}

fn inspect_dsh(manifest: &Path, trusted_root: &Path, owned_sources: &Path) -> InstallState {
    let content = match fs::read(manifest) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return InstallState::Absent,
        Err(error) => {
            return InstallState::Partial(format!("could not read DSH web profile: {error}"));
        }
    };
    let value: Value = match serde_json::from_slice(&content) {
        Ok(value) => value,
        Err(error) => {
            return InstallState::Partial(format!("invalid DSH web profile manifest: {error}"));
        }
    };
    let dependency = value
        .get("dependencies")
        .and_then(Value::as_object)
        .and_then(|dependencies| dependencies.get(DSH_PACKAGE_NAME));
    let bundle = value
        .pointer("/dsh/profile/bundles")
        .and_then(Value::as_array)
        .is_some_and(|bundles| {
            bundles
                .iter()
                .any(|bundle| bundle.as_str() == Some(DSH_PACKAGE_NAME))
        });
    match (dependency, bundle) {
        (Some(dependency), true) => {
            let Some(spec) = dependency.as_str() else {
                return InstallState::Partial(
                    "DSH has a non-string dependency for the OCG package name".into(),
                );
            };
            let source = match dsh_dependency_source(spec, manifest) {
                Ok(source) => source,
                Err(detail) => return InstallState::Partial(detail),
            };
            if !is_owned_package_source(
                &source,
                trusted_root,
                owned_sources,
                ApplicationConnectorId::Dsh,
            ) {
                return InstallState::Partial(
                    "DSH has a same-name package that is not the exact OCG-managed source".into(),
                );
            }
            InstallState::Connected {
                source: Some(source),
            }
        }
        (None, false) => InstallState::Absent,
        _ => InstallState::Partial(
            "DSH profile has only part of the OCG native plugin registration".into(),
        ),
    }
}

fn dsh_dependency_source(spec: &str, manifest: &Path) -> Result<PathBuf, String> {
    let value = spec
        .strip_prefix("file:")
        .or_else(|| spec.strip_prefix("link:"))
        .ok_or_else(|| "DSH same-name dependency is not a local OCG package source".to_string())?;
    let value = value.strip_prefix("//").unwrap_or(value);
    #[cfg(windows)]
    let value = value
        .strip_prefix('/')
        .filter(|path| path.as_bytes().get(1) == Some(&b':'))
        .unwrap_or(value);
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        manifest
            .parent()
            .ok_or_else(|| "DSH profile manifest has no parent directory".to_string())?
            .join(path)
    };
    canonical_lexical_path(&path)
}

fn models(values: &BTreeMap<String, String>) -> ApplicationConnectorResult<Vec<String>> {
    let raw = values
        .get("models")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| values.get("model").filter(|value| !value.trim().is_empty()))
        .ok_or_else(|| invalid("at least one model is required"))?;
    let models = raw
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if models.is_empty()
        || models
            .iter()
            .any(|model| model.len() > 256 || model.contains('\0'))
    {
        return Err(invalid("models must be non-empty single-line aliases"));
    }
    Ok(models)
}

fn render_package(
    data_dir: &Path,
    id: ApplicationConnectorId,
    templates: &[NativePluginTemplateFile],
    gateway_url: &str,
    models: &[String],
) -> ApplicationConnectorResult<RenderedPackage> {
    if templates.is_empty() {
        return Err(internal("native plugin templates are missing"));
    }
    if gateway_url.is_empty() || gateway_url.contains('\0') {
        return Err(invalid("invalid gateway URL"));
    }
    let gateway_v1_url = gateway_v1_url(gateway_url)?;
    let models_json = serde_json::to_string(&native_models(models)).map_err(internal)?;
    let models_lines = models.join("\n");
    let mut files = BTreeMap::new();
    for template in templates {
        let relative = safe_template_path(template.relative_path)?;
        let quoted_generated_models = format!("\"{GENERATED_MODELS_PLACEHOLDER}\"");
        let rendered = template
            .contents
            .replace(GATEWAY_URL_PLACEHOLDER, gateway_url)
            .replace(DEFAULT_GATEWAY_V1_URL, &gateway_v1_url)
            .replace(MODELS_JSON_PLACEHOLDER, &models_json)
            .replace(MODELS_LINES_PLACEHOLDER, &models_lines)
            .replace(&quoted_generated_models, &models_json)
            .replace(GENERATED_MODELS_PLACEHOLDER, &models_json);
        if rendered.contains(GATEWAY_URL_PLACEHOLDER)
            || rendered.contains(MODELS_JSON_PLACEHOLDER)
            || rendered.contains(MODELS_LINES_PLACEHOLDER)
            || rendered.contains(GENERATED_MODELS_PLACEHOLDER)
        {
            return Err(internal(
                "native plugin template left an unresolved placeholder",
            ));
        }
        if files.insert(relative, rendered.into_bytes()).is_some() {
            return Err(internal("duplicate native plugin template path"));
        }
    }
    let digest = package_digest(id, &files);
    let path = package_root(data_dir, id).join(&digest[..24]);
    Ok(RenderedPackage {
        trusted_root: data_dir.to_path_buf(),
        path,
        files,
        digest,
    })
}

fn gateway_v1_url(gateway_url: &str) -> ApplicationConnectorResult<String> {
    let trimmed = gateway_url.trim_end_matches('/');
    if trimmed.is_empty() || trimmed.contains('\0') {
        return Err(invalid("invalid gateway URL"));
    }
    Ok(if trimmed.ends_with("/v1") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/v1")
    })
}

fn native_models(models: &[String]) -> Vec<Value> {
    models
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "reasoning": false,
                "input": ["text"],
                "contextWindow": 128000,
                "maxTokens": 16384,
            })
        })
        .collect()
}

fn safe_template_path(value: &str) -> ApplicationConnectorResult<PathBuf> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(invalid("unsafe native plugin template path"));
    }
    Ok(path.to_path_buf())
}

fn package_root(data_dir: &Path, id: ApplicationConnectorId) -> PathBuf {
    data_dir.join(PACKAGE_ROOT).join(match id {
        ApplicationConnectorId::Pi => "pi",
        ApplicationConnectorId::Dsh => "dsh",
        _ => unreachable!("caller must use NativePluginHost::owns"),
    })
}

fn package_digest(id: ApplicationConnectorId, files: &BTreeMap<PathBuf, Vec<u8>>) -> String {
    let mut hash = Sha256::new();
    hash.update(b"ocg-manager-native-package-v1\0");
    match id {
        ApplicationConnectorId::Pi => hash.update(b"pi"),
        ApplicationConnectorId::Dsh => hash.update(b"dsh"),
        _ => unreachable!(),
    }
    for (path, bytes) in files {
        hash.update(path.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(bytes);
        hash.update([0]);
    }
    format!("{:x}", hash.finalize())
}

fn native_command(
    id: ApplicationConnectorId,
    executable: ResolvedExecutable,
    action: ApplicationConnectorAction,
    package_path: Option<&Path>,
    restore_source: Option<&Path>,
) -> ApplicationConnectorResult<NativePluginCommand> {
    let args = match (id, action) {
        (ApplicationConnectorId::Pi, ApplicationConnectorAction::Connect) => {
            let package_path =
                package_path.ok_or_else(|| internal("Pi install package is missing"))?;
            vec![
                OsString::from("install"),
                package_path.as_os_str().to_owned(),
            ]
        }
        (ApplicationConnectorId::Pi, ApplicationConnectorAction::Restore) => {
            let restore_source = restore_source
                .ok_or_else(|| precondition("Pi has no OCG native package source to remove"))?;
            vec![
                OsString::from("remove"),
                restore_source.as_os_str().to_owned(),
            ]
        }
        (ApplicationConnectorId::Dsh, ApplicationConnectorAction::Connect) => {
            let package_path =
                package_path.ok_or_else(|| internal("DSH install package is missing"))?;
            vec![
                OsString::from("plugin"),
                OsString::from("--profile"),
                OsString::from(DSH_PROFILE),
                OsString::from("add"),
                dsh_package_argument(package_path),
            ]
        }
        (ApplicationConnectorId::Dsh, ApplicationConnectorAction::Restore) => vec![
            OsString::from("plugin"),
            OsString::from("--profile"),
            OsString::from(DSH_PROFILE),
            OsString::from("remove"),
            OsString::from(DSH_PACKAGE_NAME),
        ],
        _ => unreachable!("caller must use NativePluginHost::owns"),
    };
    Ok(NativePluginCommand {
        executable: executable.path,
        display_executable: executable.display,
        args,
        timeout: COMMAND_TIMEOUT,
    })
}

fn dsh_package_argument(package_path: &Path) -> OsString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        // DSH rc.2 forwards pnpm with `shell: true`. Literal quotes must reach
        // DSH's argv so its second spawn keeps a trusted package path with
        // spaces as one argument.
        let mut quoted = Vec::with_capacity(package_path.as_os_str().encode_wide().count() + 2);
        quoted.push(b'"' as u16);
        quoted.extend(package_path.as_os_str().encode_wide());
        quoted.push(b'"' as u16);
        OsString::from_wide(&quoted)
    }
    #[cfg(not(windows))]
    {
        package_path.as_os_str().to_owned()
    }
}

fn resolve_executable(configured: &Path) -> Option<ResolvedExecutable> {
    if configured.components().count() > 1 || configured.is_absolute() {
        return configured.is_file().then(|| ResolvedExecutable {
            path: configured.to_path_buf(),
            display: configured.display().to_string(),
        });
    }
    let path = std::env::var_os("PATH")?;
    resolve_executable_in_directories(configured, std::env::split_paths(&path))
}

fn resolve_executable_in_directories(
    configured: &Path,
    directories: impl IntoIterator<Item = PathBuf>,
) -> Option<ResolvedExecutable> {
    let names = executable_names(configured);
    for directory in directories {
        for name in &names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(ResolvedExecutable {
                    display: candidate.display().to_string(),
                    path: candidate,
                });
            }
        }
    }
    None
}

fn executable_names(name: &Path) -> Vec<OsString> {
    #[cfg(windows)]
    {
        if name.extension().is_some() {
            vec![name.as_os_str().to_owned()]
        } else {
            vec![
                OsString::from(format!("{}.exe", name.display())),
                OsString::from(format!("{}.cmd", name.display())),
                OsString::from(format!("{}.bat", name.display())),
                name.as_os_str().to_owned(),
            ]
        }
    }
    #[cfg(not(windows))]
    {
        vec![name.as_os_str().to_owned()]
    }
}

fn read_optional(path: &Path) -> ApplicationConnectorResult<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(internal(error)),
    }
}

fn fingerprint(
    request: &ApplicationConnectorHostRequest,
    executable: &ResolvedExecutable,
    state_bytes: &[u8],
    package: Option<&RenderedPackage>,
    restore_source: Option<&Path>,
) -> String {
    let mut hash = Sha256::new();
    hash.update(b"ocg-manager-native-plugin-preview-v1\0");
    hash.update(format!("{:?}\0{:?}\0", request.id, request.action).as_bytes());
    hash.update(executable.path.to_string_lossy().as_bytes());
    hash.update([0]);
    hash.update(sha256(state_bytes).as_bytes());
    hash.update([0]);
    if let Some(package) = package {
        hash.update(package.digest.as_bytes());
        hash.update([0]);
        for (path, bytes) in &package.files {
            hash.update(path.to_string_lossy().as_bytes());
            hash.update([0]);
            hash.update(sha256(bytes).as_bytes());
            hash.update([0]);
        }
    }
    if let Some(restore_source) = restore_source {
        hash.update(restore_source.to_string_lossy().as_bytes());
        hash.update([0]);
    }
    format!("{:x}", hash.finalize())
}

fn read_bounded<R: Read>(mut reader: R) -> String {
    let mut kept = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                let remaining = MAX_COMMAND_OUTPUT.saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..count.min(remaining)]);
            }
        }
    }
    String::from_utf8_lossy(&kept).into_owned()
}

fn redact_output(value: &str) -> String {
    value
        .lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if lower.contains("ocg_manager_api_key")
                || lower.contains("authorization:")
                || lower.contains("api key")
                || lower.contains("apikey")
                || lower.contains("bearer ")
            {
                "[redacted]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn command_failure(command: &NativePluginCommand, output: &NativePluginCommandOutput) -> String {
    let output = if output.stderr.trim().is_empty() {
        output.stdout.trim()
    } else {
        output.stderr.trim()
    };
    if output.is_empty() {
        format!("{} exited unsuccessfully", command.display_executable)
    } else {
        format!(
            "{} exited unsuccessfully: {output}",
            command.display_executable
        )
    }
}

fn inspection(
    id: ApplicationConnectorId,
    status: ApplicationConnectorStatus,
    detected: bool,
    detail: Option<String>,
    target_paths: Vec<String>,
) -> ApplicationConnectorInspection {
    ApplicationConnectorInspection {
        id,
        status,
        automatic: true,
        detected,
        detail,
        target_paths,
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn invalid(message: impl Into<String>) -> ApplicationConnectorError {
    ApplicationConnectorError::new(ApplicationConnectorErrorKind::InvalidRequest, message)
}

fn precondition(message: impl Into<String>) -> ApplicationConnectorError {
    ApplicationConnectorError::new(ApplicationConnectorErrorKind::Precondition, message)
}

fn conflict(message: impl Into<String>) -> ApplicationConnectorError {
    ApplicationConnectorError::new(ApplicationConnectorErrorKind::Conflict, message)
}

fn internal(message: impl std::fmt::Display) -> ApplicationConnectorError {
    ApplicationConnectorError::new(ApplicationConnectorErrorKind::Internal, message.to_string())
}

#[cfg(test)]
mod tests;
