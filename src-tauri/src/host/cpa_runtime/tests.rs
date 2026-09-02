use super::*;

#[cfg(windows)]
fn utf16_to_string(wide: &[u16]) -> String {
    let end = wide
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

#[cfg(windows)]
#[test]
fn command_line_quotes_paths_and_omits_the_management_password() {
    let line = utf16_to_string(
        &windows_command_line(
            Path::new(r"C:\data\cpa\versions\7.2.147\cli-proxy-api.exe"),
            Path::new(r"C:\data\cpa\config.yaml"),
        )
        .unwrap(),
    );
    assert!(line.contains("--config"));
    assert!(line.contains("cli-proxy-api.exe"));
    assert!(!line.contains("MANAGEMENT_PASSWORD"));
    assert!(!line.contains("secret"));
}

#[cfg(windows)]
#[test]
fn environment_block_sets_management_password() {
    let block = windows_environment("cpa-test-secret");
    let text = String::from_utf16_lossy(&block);
    assert!(text.contains("MANAGEMENT_PASSWORD=cpa-test-secret"));
}

#[cfg(windows)]
#[test]
fn owned_cmd_process_is_job_contained_and_stoppable() {
    use std::os::windows::ffi::OsStrExt;
    let exe = std::path::PathBuf::from(r"C:\Windows\System32\cmd.exe");
    let application: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let command_line: Vec<u16> =
        std::ffi::OsString::from(r"C:\Windows\System32\cmd.exe /c ping -n 20 127.0.0.1")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
    let environment = windows_environment("cpa-test-secret");
    let session = spawn_process(
        &application,
        command_line,
        environment,
        Some(exe.parent().unwrap()),
        vec![b"cpa-test-secret".to_vec()],
    )
    .expect("cmd ping should start");
    assert!(session.is_running());
    let _ = session.stop();
}

#[cfg(windows)]
#[test]
fn stream_redaction_covers_secrets_split_across_chunks() {
    let secrets = Arc::new(Mutex::new(normalize_secrets(vec![
        b"management-secret".to_vec(),
        b"inference-secret".to_vec(),
    ])));
    let mut redactor = StreamRedactor::new(secrets.clone());
    let mut output = redactor.push(b"before management-");
    output.extend(redactor.push(b"secret and inference-se"));
    output.extend(redactor.push(b"cret after"));
    output.extend(redactor.finish());
    let text = String::from_utf8(output).unwrap();
    assert_eq!(text, "before [REDACTED] and [REDACTED] after");

    secrets.lock().push(b"rotated-secret".to_vec());
    let mut output = redactor.push(b" rotated-");
    output.extend(redactor.push(b"secret"));
    output.extend(redactor.finish());
    assert_eq!(String::from_utf8(output).unwrap(), " [REDACTED]");
}

#[cfg(windows)]
fn process_still_active(process: &OwnedHandle) -> bool {
    const STILL_ACTIVE: u32 = 259;
    let mut code = 0u32;
    unsafe {
        windows_sys::Win32::System::Threading::GetExitCodeProcess(process.0, &mut code) != 0
            && code == STILL_ACTIVE
    }
}

#[cfg(windows)]
fn spawn_suspended_unassigned() -> (OwnedHandle, OwnedHandle, JobObject) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
        PROCESS_INFORMATION, STARTUPINFOW,
    };
    let exe = std::path::PathBuf::from(r"C:\Windows\System32\cmd.exe");
    let application: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut command_line: Vec<u16> =
        std::ffi::OsString::from(r"C:\Windows\System32\cmd.exe /c ping -n 20 127.0.0.1")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
    let mut environment = windows_environment("cpa-test-secret");
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut information: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            environment.as_mut_ptr().cast(),
            std::ptr::null(),
            &startup,
            &mut information,
        )
    };
    assert_ne!(created, 0, "CREATE_SUSPENDED cmd should start");
    (
        OwnedHandle(information.hProcess),
        OwnedHandle(information.hThread),
        JobObject::new().expect("job"),
    )
}

#[cfg(windows)]
#[test]
fn unassigned_suspended_process_is_killed_by_its_handle_not_the_job() {
    let (process, thread, job) = spawn_suspended_unassigned();
    job.terminate();
    assert!(
        process_still_active(&process),
        "TerminateJobObject must not reach a process that was never assigned"
    );
    abandon_unassigned_process(&process);
    assert!(
        !process_still_active(&process),
        "the exact created process handle must be terminated and waited"
    );
    drop(thread);
}
