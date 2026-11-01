use std::process::Command;

fn reda(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_reda"))
        .args(args)
        .output()
        .expect("run reda")
}

fn git_short_hash() -> String {
    let out = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .expect("run git");
    assert!(out.status.success(), "tests need a git checkout");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[test]
fn version_reports_cargo_version_dev_and_git_hash() {
    let out = reda(&["--version"]);
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected = format!(
        "reda {}-dev+{}\n",
        env!("CARGO_PKG_VERSION"),
        git_short_hash()
    );
    assert_eq!(stdout, expected);
}

#[test]
fn short_flag_matches_long_flag() {
    assert_eq!(reda(&["-V"]).stdout, reda(&["--version"]).stdout);
}

#[test]
fn unknown_argument_fails_with_usage() {
    let out = reda(&["--frobnicate"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("--frobnicate"), "stderr: {stderr}");
    assert!(stderr.contains("Usage"), "stderr: {stderr}");
}
