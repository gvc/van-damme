use std::process::Command;

#[test]
fn test_version_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_van-damme"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_van-damme"))
        .arg("-V")
        .output()
        .expect("failed to run binary");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success());
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}
