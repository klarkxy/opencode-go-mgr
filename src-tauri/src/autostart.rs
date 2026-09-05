const STARTUP_ARG: &str = "--startup";
#[cfg(any(target_os = "linux", test))]
const LINUX_DESKTOP_NAME: &str = "ocg-manager.desktop";
#[cfg(any(target_os = "macos", test))]
const MACOS_PLIST_NAME: &str = "com.ocg-manager.plist";
#[cfg(any(target_os = "macos", test))]
const MACOS_LABEL: &str = "com.ocg-manager";

pub fn is_startup_launch() -> bool {
    std::env::args_os().any(|arg| arg == STARTUP_ARG)
}

pub fn sync(enabled: bool) -> anyhow::Result<()> {
    sync_with(&AutoStartTargets::from_current_process()?, enabled)
}

#[derive(Debug, Clone)]
pub(crate) struct AutoStartTargets {
    pub executable: std::path::PathBuf,
    #[allow(dead_code)]
    pub linux_autostart_dir: std::path::PathBuf,
    #[allow(dead_code)]
    pub macos_launch_agents_dir: std::path::PathBuf,
}

impl AutoStartTargets {
    fn from_current_process() -> anyhow::Result<Self> {
        let executable = std::env::current_exe()?;
        #[cfg(target_os = "linux")]
        let executable = linux_startup_executable(executable, std::env::var_os("APPIMAGE"))?;
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .ok_or_else(|| anyhow::anyhow!("HOME is unset"))?;
        let home = std::path::PathBuf::from(home);
        Ok(Self {
            executable,
            linux_autostart_dir: home.join(".config/autostart"),
            macos_launch_agents_dir: home.join("Library/LaunchAgents"),
        })
    }
}

pub(crate) fn sync_with(targets: &AutoStartTargets, enabled: bool) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        let _ = &targets.executable;
        if enabled { enable() } else { disable() }
    }
    #[cfg(target_os = "macos")]
    {
        macos_sync(
            &targets.macos_launch_agents_dir,
            &targets.executable,
            enabled,
        )
    }
    #[cfg(target_os = "linux")]
    {
        linux_sync(&targets.linux_autostart_dir, &targets.executable, enabled)
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = (targets, enabled);
        anyhow::bail!("auto-start is unavailable on this operating system")
    }
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn macos_plist_body(executable: &std::path::Path) -> anyhow::Result<String> {
    let path = unicode_path(executable)?;
    let path = xml_escape(&path);
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{MACOS_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{path}</string>
        <string>{STARTUP_ARG}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
    ))
}

#[cfg(any(target_os = "linux", test))]
fn linux_startup_executable(
    executable: std::path::PathBuf,
    appimage: Option<std::ffi::OsString>,
) -> anyhow::Result<std::path::PathBuf> {
    let Some(appimage) = appimage else {
        return Ok(executable);
    };
    let path = std::path::PathBuf::from(appimage);
    if !path.is_absolute() || !path.is_file() {
        anyhow::bail!("APPIMAGE must point to an existing absolute application file");
    }
    unicode_path(&path)?;
    Ok(path)
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn linux_desktop_body(executable: &std::path::Path) -> anyhow::Result<String> {
    let path = unicode_path(executable)?;
    let exec = desktop_exec(&path);
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=OCG Manager\nExec={exec}\nX-GNOME-Autostart-enabled=true\nHidden=false\n"
    ))
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn write_macos_launch_agent(
    launch_agents_dir: &std::path::Path,
    executable: &std::path::Path,
) -> anyhow::Result<std::path::PathBuf> {
    std::fs::create_dir_all(launch_agents_dir)?;
    let path = launch_agents_dir.join(MACOS_PLIST_NAME);
    std::fs::write(&path, macos_plist_body(executable)?)?;
    Ok(path)
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn remove_macos_launch_agent(launch_agents_dir: &std::path::Path) -> anyhow::Result<()> {
    let path = launch_agents_dir.join(MACOS_PLIST_NAME);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn write_linux_desktop_entry(
    autostart_dir: &std::path::Path,
    executable: &std::path::Path,
) -> anyhow::Result<std::path::PathBuf> {
    std::fs::create_dir_all(autostart_dir)?;
    let path = autostart_dir.join(LINUX_DESKTOP_NAME);
    std::fs::write(&path, linux_desktop_body(executable)?)?;
    Ok(path)
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn remove_linux_desktop_entry(autostart_dir: &std::path::Path) -> anyhow::Result<()> {
    let path = autostart_dir.join(LINUX_DESKTOP_NAME);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn unicode_path(path: &std::path::Path) -> anyhow::Result<String> {
    let value = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("executable path is not Unicode"))?;
    if value.contains(['\0', '\n', '\r']) {
        anyhow::bail!("executable path contains unsafe characters");
    }
    Ok(value.to_string())
}

#[cfg(any(target_os = "macos", test))]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(any(target_os = "linux", test))]
fn desktop_exec(path: &str) -> String {
    format!("\"{}\" {STARTUP_ARG}", path.replace('"', r#"\""#))
}

#[cfg(target_os = "macos")]
fn macos_sync(
    launch_agents_dir: &std::path::Path,
    executable: &std::path::Path,
    enabled: bool,
) -> anyhow::Result<()> {
    if enabled {
        write_macos_launch_agent(launch_agents_dir, executable)?;
    } else {
        remove_macos_launch_agent(launch_agents_dir)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_sync(
    autostart_dir: &std::path::Path,
    executable: &std::path::Path,
    enabled: bool,
) -> anyhow::Result<()> {
    if enabled {
        write_linux_desktop_entry(autostart_dir, executable)?;
    } else {
        remove_linux_desktop_entry(autostart_dir)?;
    }
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

#[cfg(test)]
mod tests;
