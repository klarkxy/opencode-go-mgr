use super::*;

#[test]
fn appimage_startup_uses_the_persistent_image_and_rejects_invalid_paths() {
    let dir = std::env::temp_dir().join(format!("ocg-appimage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join("Open Console Gateway.AppImage");
    std::fs::write(&image, b"test image").unwrap();
    let mounted = dir.join(".mount_ocg/usr/bin/ocg-manager");
    let selected =
        linux_startup_executable(mounted.clone(), Some(image.clone().into_os_string())).unwrap();
    assert_eq!(selected, image);
    let encoded = desktop_exec(&image.to_string_lossy());
    assert!(
        linux_desktop_body(&selected).unwrap().contains(&encoded),
        "Windows-hosted AppImage paths must use Exec encoding, got body without {encoded}"
    );
    assert_eq!(
        linux_startup_executable(mounted.clone(), None).unwrap(),
        mounted
    );
    assert!(linux_startup_executable(mounted.clone(), Some("relative.AppImage".into())).is_err());
    assert!(linux_startup_executable(mounted.clone(), Some(dir.clone().into_os_string())).is_err());
    assert!(linux_startup_executable(mounted.clone(), Some(mounted.into_os_string())).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
use std::path::Path;

#[cfg(windows)]
#[test]
fn startup_value_quotes_exe_and_sets_silent_arg() {
    let path = Path::new(r"C:\Program Files\Open Console Gateway\ocg-manager.exe");
    assert_eq!(
        super::startup_value(path),
        r#""C:\Program Files\Open Console Gateway\ocg-manager.exe" --startup"#
    );
}

#[test]
fn macos_launch_agent_contains_exe_and_startup_and_disable_removes_it() {
    let dir = std::env::temp_dir().join(format!("ocg-autostart-macos-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = Path::new("/Applications/Open Console Gateway.app/Contents/MacOS/ocg-manager");
    let path = write_macos_launch_agent(&dir, exe).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("<string>--startup</string>"));
    assert!(body.contains("/Applications/Open Console Gateway.app/Contents/MacOS/ocg-manager"));
    assert!(!body.contains("KeepAlive"));
    remove_macos_launch_agent(&dir).unwrap();
    assert!(!path.exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn linux_desktop_entry_contains_exe_and_startup_and_disable_removes_it() {
    let dir = std::env::temp_dir().join(format!("ocg-autostart-linux-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = Path::new("/opt/Open Console Gateway/ocg-manager");
    let path = write_linux_desktop_entry(&dir, exe).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("[Desktop Entry]"));
    assert!(body.contains("Name=Open Console Gateway\n"));
    assert!(body.contains("Exec=\"/opt/Open Console Gateway/ocg-manager\" --startup\n"));
    remove_linux_desktop_entry(&dir).unwrap();
    assert!(!path.exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn sync_with_writes_the_current_platform_artifact() {
    let root = std::env::temp_dir().join(format!("ocg-autostart-sync-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let exe = if cfg!(target_os = "macos") {
        Path::new("/Applications/Open Console Gateway.app/Contents/MacOS/ocg-manager")
    } else {
        Path::new("/opt/Open Console Gateway/ocg-manager")
    };
    let targets = AutoStartTargets {
        executable: exe.to_path_buf(),
        linux_autostart_dir: root.join("autostart"),
        macos_launch_agents_dir: root.join("LaunchAgents"),
    };
    sync_with(&targets, true).unwrap();
    #[cfg(target_os = "macos")]
    {
        let body = std::fs::read_to_string(targets.macos_launch_agents_dir.join(MACOS_PLIST_NAME))
            .unwrap();
        assert!(body.contains("--startup"));
    }
    #[cfg(target_os = "linux")]
    {
        let body =
            std::fs::read_to_string(targets.linux_autostart_dir.join(LINUX_DESKTOP_NAME)).unwrap();
        assert!(body.contains("--startup"));
    }
    sync_with(&targets, false).unwrap();
    #[cfg(target_os = "macos")]
    assert!(
        !targets
            .macos_launch_agents_dir
            .join(MACOS_PLIST_NAME)
            .exists()
    );
    #[cfg(target_os = "linux")]
    assert!(
        !targets
            .linux_autostart_dir
            .join(LINUX_DESKTOP_NAME)
            .exists()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn desktop_exec_encodes_field_codes_and_quoted_specials() {
    assert_eq!(
        desktop_exec("/opt/Open Console Gateway/ocg-manager"),
        "\"/opt/Open Console Gateway/ocg-manager\" --startup"
    );
    assert_eq!(
        desktop_exec("/opt/OCG%Manager/ocg-manager"),
        "\"/opt/OCG%%Manager/ocg-manager\" --startup"
    );
    assert_eq!(
        desktop_exec(r"/opt/OCG\Manager/ocg-manager"),
        r#""/opt/OCG\\\\Manager/ocg-manager" --startup"#
    );
    assert_eq!(
        desktop_exec("/opt/OCG\"Manager/ocg-manager"),
        r#""/opt/OCG\\"Manager/ocg-manager" --startup"#
    );
    assert_eq!(
        desktop_exec("/opt/OCG$Manager/ocg-manager"),
        r#""/opt/OCG\\$Manager/ocg-manager" --startup"#
    );
    assert_eq!(
        desktop_exec("/opt/OCG`Manager/ocg-manager"),
        "\"/opt/OCG\\\\`Manager/ocg-manager\" --startup"
    );
}

#[test]
fn linux_desktop_entry_encodes_special_paths() {
    let dir = std::env::temp_dir().join(format!(
        "ocg-autostart-linux-special-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = Path::new("/opt/OCG%Manager/ocg-manager");
    let path = write_linux_desktop_entry(&dir, exe).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("Exec=\"/opt/OCG%%Manager/ocg-manager\" --startup\n"));
    assert!(body.contains("X-GNOME-Autostart-enabled=true"));
    remove_linux_desktop_entry(&dir).unwrap();
    let _ = std::fs::remove_dir_all(dir);
}
