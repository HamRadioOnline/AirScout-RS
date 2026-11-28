use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn smoke(flag: &str) -> Output {
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_airscout"))
        .arg(flag)
        .current_dir(home.path())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("APPDATA", home.path().join("roaming"))
        .env("LOCALAPPDATA", home.path().join("local"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .env("XDG_RUNTIME_DIR", home.path().join("runtime"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait().unwrap();
            panic!("airscout {flag} did not exit within 10 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{flag}: {output:?}");
    assert!(
        home.path().read_dir().unwrap().next().is_none(),
        "airscout {flag} wrote files to the disposable home"
    );
    output
}

#[test]
fn version_does_not_start_gui() {
    let output = smoke("--version");
    let version = String::from_utf8(output.stdout).unwrap();
    assert_eq!(version, format!("airscout {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_does_not_start_gui() {
    let output = smoke("--help");
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("Usage:"), "{help}");
    for flag in ["--clean", "--help", "--version"] {
        assert!(help.contains(flag), "{flag} missing: {help}");
    }
}
