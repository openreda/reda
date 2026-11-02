use std::process::Command;

fn git_short_hash() -> String {
    let out = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .expect("run git");
    assert!(out.status.success(), "tests need a git checkout");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[test]
fn version_reports_cargo_version_dev_and_git_hash_like_the_cli() {
    let out = Command::new(env!("CARGO_BIN_EXE_reda-app"))
        .arg("--version")
        .output()
        .expect("run reda-app");
    assert!(out.status.success());
    let expected = format!(
        "reda-app {}-dev+{}\n",
        env!("CARGO_PKG_VERSION"),
        git_short_hash()
    );
    assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
}
