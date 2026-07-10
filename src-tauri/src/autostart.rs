const STARTUP_ARG: &str = "--startup";

pub fn is_startup_launch() -> bool {
    std::env::args_os().any(|arg| arg == STARTUP_ARG)
}

#[cfg(windows)]
pub fn sync(enabled: bool) -> anyhow::Result<()> {
    if enabled { enable() } else { disable() }
}

#[cfg(not(windows))]
pub fn sync(_enabled: bool) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const RUN_VALUE: &str = "OCG Manager";

#[cfg(windows)]
fn enable() -> anyhow::Result<()> {
    let value = startup_value(&std::env::current_exe()?);
    run_reg(&[
        "add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &value, "/f",
    ])
}

#[cfg(windows)]
fn disable() -> anyhow::Result<()> {
    if !reg_succeeds(&["query", RUN_KEY, "/v", RUN_VALUE])? {
        return Ok(());
    }
    run_reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"])
}

#[cfg(windows)]
fn startup_value(exe: &std::path::Path) -> String {
    format!("\"{}\" {}", exe.display(), STARTUP_ARG)
}

#[cfg(windows)]
fn reg_succeeds(args: &[&str]) -> anyhow::Result<bool> {
    Ok(reg_command(args).status()?.success())
}

#[cfg(windows)]
fn run_reg(args: &[&str]) -> anyhow::Result<()> {
    let output = reg_command(args).output()?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("failed to update Windows startup entry: {}", stderr.trim());
    }
}

#[cfg(windows)]
fn reg_command(args: &[&str]) -> std::process::Command {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = std::process::Command::new("reg");
    command.args(args).creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn startup_value_quotes_exe_and_sets_silent_arg() {
        let path = std::path::Path::new(r"C:\Program Files\OCG Manager\ocg-manager.exe");
        assert_eq!(
            super::startup_value(path),
            r#""C:\Program Files\OCG Manager\ocg-manager.exe" --startup"#
        );
    }
}
