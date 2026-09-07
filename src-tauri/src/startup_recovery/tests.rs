use super::windows::*;
use std::os::windows::process::CommandExt;
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Fixture {
    child: Child,
    root: std::path::PathBuf,
    port: u16,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn start(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ocg-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join(name);
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let ready = root.join("ready");
        let child = Command::new(exe)
            .args([
                "--ignored",
                "--exact",
                "startup_recovery::tests::listener_child",
            ])
            .env("OCG_RECOVERY_TEST_READY", &ready)
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut fixture = Self {
            child,
            root,
            port: 0,
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(text) = std::fs::read_to_string(&ready)
                && let Ok(port) = text.parse()
            {
                fixture.port = port;
                return fixture;
            }
            assert!(
                fixture.child.try_wait().unwrap().is_none(),
                "listener helper exited"
            );
            assert!(Instant::now() < deadline, "listener helper timed out");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
#[ignore = "subprocess fixture invoked only by recovery tests"]
fn listener_child() {
    let ready = std::env::var_os("OCG_RECOVERY_TEST_READY").expect("fixture only");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    std::fs::write(ready, listener.local_addr().unwrap().port().to_string()).unwrap();
    std::thread::sleep(Duration::from_secs(60));
    drop(listener);
}

#[test]
fn identified_cli_can_be_stopped_and_port_rebound() {
    let mut fixture = Fixture::start("ocg-manager-cli.exe");
    let occupant = inspect(fixture.port).unwrap().unwrap();
    assert_eq!(occupant.pid, fixture.child.id());
    assert!(occupant.is_cli());
    occupant.stop(fixture.port).unwrap();
    assert!(fixture.child.wait().unwrap().code().is_some());
    let replacement = TcpListener::bind(("127.0.0.1", fixture.port)).unwrap();
    assert_eq!(
        listener_pid(fixture.port).unwrap(),
        Some(std::process::id())
    );
    drop(replacement);
}

#[test]
fn unrelated_listener_is_identified_but_never_stopped() {
    let mut fixture = Fixture::start("unrelated-server.exe");
    let occupant = inspect(fixture.port).unwrap().unwrap();
    assert!(!occupant.is_cli());
    assert!(occupant.stop(fixture.port).is_err());
    assert!(fixture.child.try_wait().unwrap().is_none());
    assert_eq!(
        listener_pid(fixture.port).unwrap(),
        Some(fixture.child.id())
    );
}

#[test]
fn changed_port_ownership_prevents_cleanup() {
    let mut fixture = Fixture::start("ocg-manager-cli.exe");
    let occupant = inspect(fixture.port).unwrap().unwrap();
    let different = TcpListener::bind("127.0.0.1:0").unwrap();
    assert!(
        occupant
            .stop(different.local_addr().unwrap().port())
            .is_err()
    );
    assert!(fixture.child.try_wait().unwrap().is_none());
}

#[test]
fn self_listener_cannot_be_a_cleanup_target() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    assert_eq!(listener_pid(port).unwrap(), Some(std::process::id()));
    assert!(inspect(port).is_err());
}
