use std::process::Command;

#[test]
fn test_version_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_vd"))
        .arg("--version")
        .output()
        .expect("failed to run binary");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains("van-damme"));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_version_short_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_vd"))
        .arg("-V")
        .output()
        .expect("failed to run binary");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_process_hook_writes_to_debug_log() {
    let output = Command::new(env!("CARGO_BIN_EXE_vd"))
        .arg("process-hook")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin
                    .write_all(br#"{"session_id":"test-123","hook_event_name":"Stop"}"#)
                    .unwrap();
            }
            child.wait_with_output()
        })
        .expect("failed to run binary");
    assert!(output.status.success());
    let log_path = dirs::home_dir()
        .unwrap()
        .join(".van-damme")
        .join("debug.log");
    let contents = std::fs::read_to_string(&log_path).unwrap();
    assert!(contents.contains("test-123"));
    assert!(contents.contains("Stop"));
}

#[test]
fn test_add_dir_with_explicit_path() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_string_lossy().to_string();
    let output = Command::new(env!("CARGO_BIN_EXE_vd"))
        .args(["add-dir", &dir])
        .output()
        .expect("failed to run binary");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains("Added"));
    assert!(stdout.contains("recent directories"));
}

#[test]
fn test_add_dir_defaults_to_cwd() {
    let output = Command::new(env!("CARGO_BIN_EXE_vd"))
        .arg("add-dir")
        .output()
        .expect("failed to run binary");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains("Added"));
}

#[test]
fn test_process_hook_invalid_json_exits_zero() {
    let output = Command::new(env!("CARGO_BIN_EXE_vd"))
        .arg("process-hook")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(b"not json").unwrap();
            }
            child.wait_with_output()
        })
        .expect("failed to run binary");
    assert!(output.status.success());
}

#[test]
fn test_process_hook_empty_stdin_exits_zero() {
    let output = Command::new(env!("CARGO_BIN_EXE_vd"))
        .arg("process-hook")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            drop(child.stdin.take());
            child.wait_with_output()
        })
        .expect("failed to run binary");
    assert!(output.status.success());
}

#[test]
fn test_install_subcommand() {
    let output = Command::new(env!("CARGO_BIN_EXE_vd"))
        .arg("install")
        .output()
        .expect("failed to run binary");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains("vd install complete"));
}

#[test]
fn test_uninstall_subcommand() {
    // Install first, then uninstall
    Command::new(env!("CARGO_BIN_EXE_vd"))
        .arg("install")
        .output()
        .expect("failed to run binary");

    let output = Command::new(env!("CARGO_BIN_EXE_vd"))
        .arg("uninstall")
        .output()
        .expect("failed to run binary");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains("vd uninstall complete"));
}
