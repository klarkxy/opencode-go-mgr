//! OCG-owned CPA child lifecycle: desktop Host and CLI registrations.
//!
//! Windows starts CREATE_SUSPENDED, assigns a kill-on-close Job Object, then
//! resumes. Unix starts a fresh process group and tears that group down on
//! stop/exit. The Management password is passed only as MANAGEMENT_PASSWORD.
//! This Host never inspects ports or PIDs of processes it did not start.

use super::{
    CpaRuntimeError, CpaRuntimeLogTail, CpaRuntimeProcessHost, CpaRuntimeProcessSpec,
    CpaRuntimeSecret, MAX_LOG_BYTES, append_log_tail,
};
use crate::state::CoreState;
use parking_lot::Mutex;
#[cfg(windows)]
use std::io::Read;
#[cfg(windows)]
use std::path::Path;
#[cfg(any(windows, unix))]
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// Register the OCG-owned CPA process Host. Every desktop or CLI build on
/// Windows/Unix can manage a child the same way; platforms without an
/// official CLIProxyAPI asset fail closed later at install time via
/// `current_cpa_release_asset`, so support is not gated again here.
pub fn register_owned_host(state: &CoreState) {
    #[cfg(any(windows, unix))]
    state.set_cpa_runtime_host(Arc::new(OwnedCpaRuntimeHost::new()));
    let _ = state;
}

#[cfg(windows)]
type OwnedCpaRuntimeHost = WindowsCpaRuntimeHost;
#[cfg(unix)]
type OwnedCpaRuntimeHost = UnixCpaRuntimeHost;

#[cfg(windows)]
struct WindowsCpaRuntimeHost {
    session: Mutex<Option<OwnedSession>>,
    last_logs: Mutex<CpaRuntimeLogTail>,
}

#[cfg(windows)]
impl WindowsCpaRuntimeHost {
    fn new() -> Self {
        Self {
            session: Mutex::new(None),
            last_logs: Mutex::new(CpaRuntimeLogTail {
                stdout: String::new(),
                stderr: String::new(),
            }),
        }
    }
}

#[cfg(windows)]
impl CpaRuntimeProcessHost for WindowsCpaRuntimeHost {
    fn start_owned(&self, spec: &CpaRuntimeProcessSpec) -> Result<(), CpaRuntimeError> {
        if self.owned_running() {
            self.stop_owned()?;
        }
        let session = spawn_owned(spec)?;
        *self.last_logs.lock() = CpaRuntimeLogTail {
            stdout: String::new(),
            stderr: String::new(),
        };
        *self.session.lock() = Some(session);
        Ok(())
    }

    fn stop_owned(&self) -> Result<(), CpaRuntimeError> {
        let Some(session) = self.session.lock().take() else {
            return Ok(());
        };
        let snapshot = session.logs();
        match session.stop() {
            Ok(logs) => {
                *self.last_logs.lock() = logs;
                Ok(())
            }
            Err(error) => {
                *self.last_logs.lock() = snapshot;
                Err(error)
            }
        }
    }

    fn owned_running(&self) -> bool {
        let session = self.session.lock();
        session.as_ref().is_some_and(OwnedSession::is_running)
    }

    fn logs(&self) -> CpaRuntimeLogTail {
        let session = self.session.lock();
        session
            .as_ref()
            .map(OwnedSession::logs)
            .unwrap_or_else(|| self.last_logs.lock().clone())
    }

    fn add_log_secret(&self, secret: &CpaRuntimeSecret) {
        if let Some(session) = self.session.lock().as_ref() {
            session.add_log_secret(secret.expose_to_host().as_bytes());
        }
    }
}

#[cfg(windows)]
struct OwnedSession {
    job: JobObject,
    process: OwnedHandle,
    stdout: Arc<Mutex<String>>,
    stderr: Arc<Mutex<String>>,
    secrets: Arc<Mutex<Vec<Vec<u8>>>>,
    readers: Vec<JoinHandle<()>>,
}

#[cfg(windows)]
impl OwnedSession {
    fn is_running(&self) -> bool {
        const STILL_ACTIVE: u32 = 259;
        let mut code = 0u32;
        unsafe {
            windows_sys::Win32::System::Threading::GetExitCodeProcess(self.process.0, &mut code)
                != 0
                && code == STILL_ACTIVE
        }
    }

    fn logs(&self) -> CpaRuntimeLogTail {
        CpaRuntimeLogTail {
            stdout: self.stdout.lock().clone(),
            stderr: self.stderr.lock().clone(),
        }
    }

    fn add_log_secret(&self, secret: &[u8]) {
        if secret.is_empty() {
            return;
        }
        let mut secrets = self.secrets.lock();
        if !secrets.iter().any(|known| known == secret) {
            secrets.push(secret.to_vec());
            secrets.sort_by_key(|known| std::cmp::Reverse(known.len()));
        }
    }

    fn stop(mut self) -> Result<CpaRuntimeLogTail, CpaRuntimeError> {
        let terminated = self.job.terminate();
        let terminate_error = std::io::Error::last_os_error();
        let wait_result = unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(self.process.0, 5_000)
        };
        let wait_error = std::io::Error::last_os_error();
        match decide_owned_stop(terminated, wait_result) {
            OwnedStopDecision::JoinReaders => {
                for reader in self.readers.drain(..) {
                    let _ = reader.join();
                }
                Ok(self.logs())
            }
            OwnedStopDecision::TerminateFailed => Err(CpaRuntimeError::Failed(format!(
                "failed to terminate owned CPA: {terminate_error}"
            ))),
            OwnedStopDecision::WaitTimedOut => Err(CpaRuntimeError::Failed(
                "owned CPA did not exit within 5 seconds".into(),
            )),
            OwnedStopDecision::WaitFailed => Err(CpaRuntimeError::Failed(format!(
                "failed to wait for owned CPA: {wait_error}"
            ))),
        }
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnedStopDecision {
    JoinReaders,
    TerminateFailed,
    WaitTimedOut,
    WaitFailed,
}

#[cfg(windows)]
fn decide_owned_stop(terminated: bool, wait_result: u32) -> OwnedStopDecision {
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    if !terminated {
        OwnedStopDecision::TerminateFailed
    } else if wait_result == WAIT_OBJECT_0 {
        OwnedStopDecision::JoinReaders
    } else if wait_result == WAIT_TIMEOUT {
        OwnedStopDecision::WaitTimedOut
    } else {
        OwnedStopDecision::WaitFailed
    }
}

#[cfg(windows)]
impl Drop for OwnedSession {
    fn drop(&mut self) {
        let _ = self.job.terminate();
    }
}

#[cfg(windows)]
fn spawn_owned(spec: &CpaRuntimeProcessSpec) -> Result<OwnedSession, CpaRuntimeError> {
    if !spec.executable.is_file() {
        return Err(CpaRuntimeError::Invalid("CPA executable is missing".into()));
    }
    if is_reparse(&spec.executable) || is_reparse(&spec.config_path) {
        return Err(CpaRuntimeError::Invalid(
            "CPA process paths must not be reparse points".into(),
        ));
    }
    let application = wide_z(&spec.executable);
    let command_line = windows_command_line(&spec.executable, &spec.config_path)?;
    let environment = windows_environment(spec.management_password.expose_to_host());
    let secrets = spec
        .log_secrets
        .iter()
        .map(|secret| secret.expose_to_host().as_bytes().to_vec())
        .collect();
    spawn_process(
        &application,
        command_line,
        environment,
        Some(&spec.working_dir),
        secrets,
    )
}

#[cfg(windows)]
fn spawn_process(
    application: &[u16],
    mut command_line: Vec<u16>,
    mut environment: Vec<u16>,
    working_dir: Option<&Path>,
    secrets: Vec<Vec<u8>>,
) -> Result<OwnedSession, CpaRuntimeError> {
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
        EXTENDED_STARTUPINFO_PRESENT, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
        ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute,
    };
    let cwd = working_dir.map(wide_z);
    let (stdout_read, stdout_write) = windows_pipe(true)?;
    let (stderr_read, stderr_write) = windows_pipe(true)?;
    let job = JobObject::new()?;
    let attributes = ProcessAttributeList::new()?;
    let inherited_handles = [stdout_write.0, stderr_write.0];
    if unsafe {
        UpdateProcThreadAttribute(
            attributes.pointer,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            inherited_handles.as_ptr().cast(),
            std::mem::size_of_val(&inherited_handles),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    } == 0
    {
        return Err(CpaRuntimeError::Failed(format!(
            "failed to restrict CPA inherited handles: {}",
            std::io::Error::last_os_error()
        )));
    }
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdOutput = stdout_write.0;
    startup.StartupInfo.hStdError = stderr_write.0;
    startup.lpAttributeList = attributes.pointer;
    let mut information: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_mut_ptr().cast(),
            cwd.as_ref()
                .map(|value| value.as_ptr())
                .unwrap_or(std::ptr::null()),
            &startup.StartupInfo,
            &mut information,
        )
    };
    if created == 0 {
        return Err(CpaRuntimeError::Failed(format!(
            "failed to start CPA: {}",
            std::io::Error::last_os_error()
        )));
    }
    let process = OwnedHandle(information.hProcess);
    let thread = OwnedHandle(information.hThread);
    if let Err(error) = job.assign(process.0) {
        // The process is still suspended and is not in the job, so TerminateJobObject
        // cannot reach it. Kill this exact handle and wait; never look up a PID/port.
        abandon_unassigned_process(&process);
        return Err(error);
    }
    drop(stdout_write);
    drop(stderr_write);
    let stdout = Arc::new(Mutex::new(String::new()));
    let stderr = Arc::new(Mutex::new(String::new()));
    let secrets = Arc::new(Mutex::new(normalize_secrets(secrets)));
    let stdout_reader = spawn_reader(stdout_read, stdout.clone(), secrets.clone());
    let stderr_reader = spawn_reader(stderr_read, stderr.clone(), secrets.clone());
    if unsafe { ResumeThread(thread.0) } == u32::MAX {
        let _ = job.terminate();
        wait_for_process(&process);
        return Err(CpaRuntimeError::Failed(format!(
            "failed to resume CPA: {}",
            std::io::Error::last_os_error()
        )));
    }
    drop(thread);
    Ok(OwnedSession {
        job,
        process,
        stdout,
        stderr,
        secrets,
        readers: vec![stdout_reader, stderr_reader],
    })
}

#[cfg(windows)]
struct ProcessAttributeList {
    _storage: Vec<usize>,
    pointer: windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST,
}

#[cfg(windows)]
impl ProcessAttributeList {
    fn new() -> Result<Self, CpaRuntimeError> {
        use windows_sys::Win32::System::Threading::InitializeProcThreadAttributeList;

        let mut bytes = 0usize;
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(CpaRuntimeError::Failed(format!(
                "failed to size CPA process attributes: {}",
                std::io::Error::last_os_error()
            )));
        }
        let words = bytes.div_ceil(std::mem::size_of::<usize>());
        let mut storage = vec![0usize; words];
        let pointer = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(pointer, 1, 0, &mut bytes) } == 0 {
            return Err(CpaRuntimeError::Failed(format!(
                "failed to initialize CPA process attributes: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(Self {
            _storage: storage,
            pointer,
        })
    }
}

#[cfg(windows)]
impl Drop for ProcessAttributeList {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Threading::DeleteProcThreadAttributeList(self.pointer);
        }
    }
}

#[cfg(windows)]
fn spawn_reader(
    pipe: OwnedHandle,
    buffer: Arc<Mutex<String>>,
    secrets: Arc<Mutex<Vec<Vec<u8>>>>,
) -> JoinHandle<()> {
    let mut file = pipe.into_file();
    thread::spawn(move || {
        let mut redactor = StreamRedactor::new(secrets);
        let mut chunk = [0u8; 4096];
        loop {
            match file.read(&mut chunk) {
                Ok(0) => {
                    let redacted = redactor.finish();
                    let text = String::from_utf8_lossy(&redacted);
                    append_log_tail(&mut buffer.lock(), &text, MAX_LOG_BYTES);
                    break;
                }
                Ok(read) => {
                    let redacted = redactor.push(&chunk[..read]);
                    let text = String::from_utf8_lossy(&redacted);
                    append_log_tail(&mut buffer.lock(), &text, MAX_LOG_BYTES);
                }
                Err(_) => break,
            }
        }
    })
}

#[cfg(unix)]
struct UnixCpaRuntimeHost {
    session: Mutex<Option<UnixOwnedSession>>,
    last_logs: Mutex<CpaRuntimeLogTail>,
}

#[cfg(unix)]
impl UnixCpaRuntimeHost {
    fn new() -> Self {
        Self {
            session: Mutex::new(None),
            last_logs: Mutex::new(CpaRuntimeLogTail {
                stdout: String::new(),
                stderr: String::new(),
            }),
        }
    }
}

#[cfg(unix)]
impl CpaRuntimeProcessHost for UnixCpaRuntimeHost {
    fn start_owned(&self, spec: &CpaRuntimeProcessSpec) -> Result<(), CpaRuntimeError> {
        if self.owned_running() {
            self.stop_owned()?;
        }
        let session = spawn_unix_owned(spec)?;
        *self.last_logs.lock() = CpaRuntimeLogTail {
            stdout: String::new(),
            stderr: String::new(),
        };
        *self.session.lock() = Some(session);
        Ok(())
    }

    fn stop_owned(&self) -> Result<(), CpaRuntimeError> {
        let Some(session) = self.session.lock().take() else {
            return Ok(());
        };
        let snapshot = session.logs();
        match session.stop() {
            Ok(logs) => {
                *self.last_logs.lock() = logs;
                Ok(())
            }
            Err(error) => {
                *self.last_logs.lock() = snapshot;
                Err(error)
            }
        }
    }

    fn owned_running(&self) -> bool {
        let mut session = self.session.lock();
        session.as_mut().is_some_and(UnixOwnedSession::is_running)
    }

    fn logs(&self) -> CpaRuntimeLogTail {
        let session = self.session.lock();
        session
            .as_ref()
            .map(UnixOwnedSession::logs)
            .unwrap_or_else(|| self.last_logs.lock().clone())
    }

    fn add_log_secret(&self, secret: &CpaRuntimeSecret) {
        if let Some(session) = self.session.lock().as_ref() {
            session.add_log_secret(secret.expose_to_host().as_bytes());
        }
    }
}

#[cfg(unix)]
struct UnixOwnedSession {
    child: std::process::Child,
    stdout: Arc<Mutex<String>>,
    stderr: Arc<Mutex<String>>,
    secrets: Arc<Mutex<Vec<Vec<u8>>>>,
    readers: Vec<JoinHandle<()>>,
}

#[cfg(unix)]
impl UnixOwnedSession {
    fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) | Err(_) => false,
        }
    }

    fn logs(&self) -> CpaRuntimeLogTail {
        CpaRuntimeLogTail {
            stdout: self.stdout.lock().clone(),
            stderr: self.stderr.lock().clone(),
        }
    }

    fn add_log_secret(&self, secret: &[u8]) {
        if secret.is_empty() {
            return;
        }
        let mut secrets = self.secrets.lock();
        if !secrets.iter().any(|known| known == secret) {
            secrets.push(secret.to_vec());
            secrets.sort_by_key(|known| std::cmp::Reverse(known.len()));
        }
    }

    fn stop(mut self) -> Result<CpaRuntimeLogTail, CpaRuntimeError> {
        signal_unix_group(&self.child, nix::sys::signal::Signal::SIGTERM)?;
        let started = std::time::Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() >= std::time::Duration::from_secs(5) => {
                    signal_unix_group(&self.child, nix::sys::signal::Signal::SIGKILL)?;
                    self.child.wait().map_err(|error| {
                        CpaRuntimeError::Failed(format!("failed to reap owned CPA: {error}"))
                    })?;
                    break;
                }
                Ok(None) => thread::sleep(std::time::Duration::from_millis(20)),
                Err(error) => {
                    return Err(CpaRuntimeError::Failed(format!(
                        "failed to wait for owned CPA: {error}"
                    )));
                }
            }
        }
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
        Ok(self.logs())
    }
}

#[cfg(unix)]
impl Drop for UnixOwnedSession {
    fn drop(&mut self) {
        let _ = signal_unix_group(&self.child, nix::sys::signal::Signal::SIGKILL);
        let _ = self.child.try_wait();
    }
}

#[cfg(unix)]
fn signal_unix_group(
    child: &std::process::Child,
    signal: nix::sys::signal::Signal,
) -> Result<(), CpaRuntimeError> {
    use nix::sys::signal::killpg;
    use nix::unistd::Pid;
    match killpg(Pid::from_raw(child.id() as i32), signal) {
        Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
        Err(error) => Err(CpaRuntimeError::Failed(format!(
            "failed to signal owned CPA process group: {error}"
        ))),
    }
}

#[cfg(unix)]
fn spawn_unix_owned(spec: &CpaRuntimeProcessSpec) -> Result<UnixOwnedSession, CpaRuntimeError> {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    if !spec.executable.is_file() {
        return Err(CpaRuntimeError::Invalid("CPA executable is missing".into()));
    }
    let exe_meta = std::fs::symlink_metadata(&spec.executable).map_err(|error| {
        CpaRuntimeError::Invalid(format!("CPA executable metadata is unreadable: {error}"))
    })?;
    let config_meta = std::fs::symlink_metadata(&spec.config_path).map_err(|error| {
        CpaRuntimeError::Invalid(format!("CPA config metadata is unreadable: {error}"))
    })?;
    if exe_meta.file_type().is_symlink()
        || config_meta.file_type().is_symlink()
        || exe_meta.file_type().is_fifo()
    {
        return Err(CpaRuntimeError::Invalid(
            "CPA process paths must not be reparse points".into(),
        ));
    }
    let mut command = Command::new(&spec.executable);
    command
        .arg("--config")
        .arg(&spec.config_path)
        .current_dir(&spec.working_dir)
        .env(
            "MANAGEMENT_PASSWORD",
            spec.management_password.expose_to_host(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| CpaRuntimeError::Failed(format!("failed to start owned CPA: {error}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CpaRuntimeError::Failed("failed to capture CPA stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CpaRuntimeError::Failed("failed to capture CPA stderr".into()))?;
    let secrets = Arc::new(Mutex::new(normalize_secrets(
        spec.log_secrets
            .iter()
            .map(|secret| secret.expose_to_host().as_bytes().to_vec())
            .collect(),
    )));
    let stdout_buf = Arc::new(Mutex::new(String::new()));
    let stderr_buf = Arc::new(Mutex::new(String::new()));
    Ok(UnixOwnedSession {
        child,
        readers: vec![
            spawn_unix_reader(stdout, stdout_buf.clone(), secrets.clone()),
            spawn_unix_reader(stderr, stderr_buf.clone(), secrets.clone()),
        ],
        stdout: stdout_buf,
        stderr: stderr_buf,
        secrets,
    })
}

#[cfg(unix)]
fn spawn_unix_reader(
    mut pipe: impl std::io::Read + Send + 'static,
    buffer: Arc<Mutex<String>>,
    secrets: Arc<Mutex<Vec<Vec<u8>>>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut redactor = StreamRedactor::new(secrets);
        let mut chunk = [0u8; 4096];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => {
                    let redacted = redactor.finish();
                    let text = String::from_utf8_lossy(&redacted);
                    append_log_tail(&mut buffer.lock(), &text, MAX_LOG_BYTES);
                    break;
                }
                Ok(read) => {
                    let redacted = redactor.push(&chunk[..read]);
                    let text = String::from_utf8_lossy(&redacted);
                    append_log_tail(&mut buffer.lock(), &text, MAX_LOG_BYTES);
                }
                Err(_) => break,
            }
        }
    })
}

struct StreamRedactor {
    pending: Vec<u8>,
    secrets: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl StreamRedactor {
    fn new(secrets: Arc<Mutex<Vec<Vec<u8>>>>) -> Self {
        Self {
            pending: Vec::new(),
            secrets,
        }
    }

    fn push(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(bytes);
        self.drain(false)
    }

    fn finish(&mut self) -> Vec<u8> {
        self.drain(true)
    }

    fn drain(&mut self, finish: bool) -> Vec<u8> {
        const REDACTED: &[u8] = b"[REDACTED]";
        let mut output = Vec::new();
        let secrets = self.secrets.lock().clone();
        let hold = secrets
            .iter()
            .map(Vec::len)
            .max()
            .unwrap_or(1)
            .saturating_sub(1);
        while !self.pending.is_empty() && (finish || self.pending.len() > hold) {
            if let Some(secret) = secrets
                .iter()
                .find(|secret| self.pending.starts_with(secret))
            {
                output.extend_from_slice(REDACTED);
                self.pending.drain(..secret.len());
            } else {
                output.push(self.pending.remove(0));
            }
        }
        output
    }
}

fn normalize_secrets(mut secrets: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    secrets.retain(|secret| !secret.is_empty());
    secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
    secrets.dedup();
    secrets
}

#[cfg(windows)]
fn windows_command_line(exe: &Path, config: &Path) -> Result<Vec<u16>, CpaRuntimeError> {
    use std::os::windows::ffi::OsStrExt;
    let exe = exe
        .to_str()
        .ok_or_else(|| CpaRuntimeError::Invalid("CPA executable path is not Unicode".into()))?;
    let config = config
        .to_str()
        .ok_or_else(|| CpaRuntimeError::Invalid("CPA config path is not Unicode".into()))?;
    if exe.contains(['\0', '\r', '\n']) || config.contains(['\0', '\r', '\n']) {
        return Err(CpaRuntimeError::Invalid(
            "CPA process path contains unsafe characters".into(),
        ));
    }
    let line = format!(
        "\"{}\" --config \"{}\"",
        exe.replace('"', ""),
        config.replace('"', "")
    );
    let mut wide: Vec<u16> = std::ffi::OsString::from(line).encode_wide().collect();
    wide.push(0);
    Ok(wide)
}

#[cfg(windows)]
fn windows_environment(password: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut pairs: Vec<(std::ffi::OsString, std::ffi::OsString)> = std::env::vars_os()
        .filter(|(key, _)| key != "MANAGEMENT_PASSWORD")
        .collect();
    pairs.push((
        std::ffi::OsString::from("MANAGEMENT_PASSWORD"),
        std::ffi::OsString::from(password),
    ));
    pairs.sort_by(|left, right| {
        left.0
            .to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&right.0.to_string_lossy().to_ascii_lowercase())
    });
    let mut block = Vec::new();
    for (key, value) in pairs {
        block.extend(key.encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

#[cfg(windows)]
fn wide_z(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    wide
}

#[cfg(windows)]
struct JobObject(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
unsafe impl Send for JobObject {}
#[cfg(windows)]
unsafe impl Sync for JobObject {}

#[cfg(windows)]
impl JobObject {
    fn new() -> Result<Self, CpaRuntimeError> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(CpaRuntimeError::Failed(format!(
                    "failed to create CPA job object: {}",
                    std::io::Error::last_os_error()
                )));
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
                return Err(CpaRuntimeError::Failed(format!(
                    "failed to configure CPA job object: {error}"
                )));
            }
            Ok(Self(job))
        }
    }

    fn assign(
        &self,
        process: windows_sys::Win32::Foundation::HANDLE,
    ) -> Result<(), CpaRuntimeError> {
        let assigned = unsafe {
            windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(self.0, process)
        };
        if assigned == 0 {
            return Err(CpaRuntimeError::Failed(format!(
                "failed to assign CPA to job object: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(())
    }

    fn terminate(&self) -> bool {
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1) != 0 }
    }
}

#[cfg(windows)]
fn abandon_unassigned_process(process: &OwnedHandle) {
    use windows_sys::Win32::System::Threading::TerminateProcess;
    unsafe {
        TerminateProcess(process.0, 1);
    }
    wait_for_process(process);
}

#[cfg(windows)]
fn wait_for_process(process: &OwnedHandle) {
    unsafe {
        windows_sys::Win32::System::Threading::WaitForSingleObject(process.0, 5_000);
    }
}

#[cfg(windows)]
impl Drop for JobObject {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
            self.0 = std::ptr::null_mut();
        }
    }
}

#[cfg(windows)]
struct OwnedHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
unsafe impl Send for OwnedHandle {}
#[cfg(windows)]
unsafe impl Sync for OwnedHandle {}

#[cfg(windows)]
impl OwnedHandle {
    fn into_file(mut self) -> std::fs::File {
        use std::os::windows::io::FromRawHandle;
        let handle = self.0;
        self.0 = std::ptr::null_mut();
        unsafe { std::fs::File::from_raw_handle(handle as _) }
    }
}

#[cfg(windows)]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
fn windows_pipe(parent_reads: bool) -> Result<(OwnedHandle, OwnedHandle), CpaRuntimeError> {
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
            return Err(CpaRuntimeError::Failed(format!(
                "failed to create CPA log pipe: {}",
                std::io::Error::last_os_error()
            )));
        }
        let read = OwnedHandle(read);
        let write = OwnedHandle(write);
        let parent = if parent_reads { read.0 } else { write.0 };
        if SetHandleInformation(parent, HANDLE_FLAG_INHERIT, 0) == 0 {
            return Err(CpaRuntimeError::Failed(format!(
                "failed to protect CPA log pipe: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok((read, write))
    }
}

#[cfg(windows)]
fn is_reparse(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    std::fs::symlink_metadata(path)
        .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests;
