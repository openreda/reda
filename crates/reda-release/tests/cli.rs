//! The `reda-release` binary as `.github/workflows/release.yml` calls it.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_reda-release"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run reda-release");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

#[test]
fn plan_prints_github_outputs_for_a_release() {
    // Tags arrive on stdin, one per line, as `git tag --list` prints them.
    let out = run(
        &[
            "plan",
            "--labels",
            "enhancement\nrelease:minor",
            "--message",
            "Finish v0.1 (#80)",
        ],
        "latest\nv0.0.3\nv0.0.2\n",
    );
    assert_eq!(
        stdout(&out),
        "release=true\nversion=0.1.0\ntag=v0.1.0\nprevious_tag=v0.0.3\n"
    );
}

#[test]
fn plan_prints_an_empty_previous_tag_for_the_first_release() {
    let out = run(&["plan", "--message", "First (#1)"], "");
    assert_eq!(
        stdout(&out),
        "release=true\nversion=0.0.1\ntag=v0.0.1\nprevious_tag=\n"
    );
}

#[test]
fn plan_prints_release_false_when_skipping() {
    let out = run(
        &["plan", "--labels", "release:skip", "--message", "Docs (#9)"],
        "v0.0.3\n",
    );
    let text = stdout(&out);
    assert!(text.starts_with("release=false\n"), "{text}");
}

#[test]
fn plan_bump_option_forces_a_release() {
    let out = run(
        &[
            "plan",
            "--message",
            "Docs [skip release]",
            "--bump",
            "patch",
        ],
        "v0.0.3\n",
    );
    assert!(stdout(&out).starts_with("release=true\nversion=0.0.4\n"));
}

#[test]
fn plan_bump_none_is_an_ordinary_push() {
    // workflow_dispatch's input is empty on a push; `none` keeps the push rules.
    for bump in ["", "none"] {
        let out = run(
            &["plan", "--message", "Docs [skip release]", "--bump", bump],
            "v0.0.3\n",
        );
        assert!(stdout(&out).starts_with("release=false\n"), "bump {bump:?}");
    }
}

#[test]
fn notes_reads_changes_from_stdin() {
    let out = run(&["notes", "0.0.5"], "41\tAdd the PCB canvas\n");
    let text = stdout(&out);
    assert!(
        text.starts_with("REDA 0.0.5 is an early tester build."),
        "{text}"
    );
    assert!(text.contains("\n- Add the PCB canvas (#41)\n"), "{text}");
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("reda-release-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn stamp_rewrites_the_manifest_and_lockfile_under_root() {
    let dir = Scratch::new("stamp");
    std::fs::write(
        dir.0.join("Cargo.toml"),
        "[workspace.package]\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.0.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"reda-cli\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();

    let out = run(&["stamp", "0.2.1", "--root", dir.0.to_str().unwrap()], "");
    stdout(&out);

    assert_eq!(
        std::fs::read_to_string(dir.0.join("Cargo.toml")).unwrap(),
        "[workspace.package]\nversion = \"0.2.1\"\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.0.join("Cargo.lock")).unwrap(),
        "version = 4\n\n[[package]]\nname = \"reda-cli\"\nversion = \"0.2.1\"\n"
    );
}

#[test]
fn unknown_subcommand_fails_with_usage() {
    let out = run(&["frobnicate"], "");
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("Usage"), "{stderr}");
}
