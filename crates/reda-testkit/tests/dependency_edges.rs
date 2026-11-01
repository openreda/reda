//! Guards the crate layering (docs/spec/v0.1/architecture.md#crates).
//!
//! Every direct dependency of every workspace crate must be listed in
//! `docs/dependency-edges.txt`. A new edge fails this test until the list is regenerated with
//! `REDA_BLESS=1 cargo test -p reda-testkit --test dependency_edges`, so it shows up in review.
//! `reda-testkit` may only ever be a dev-dependency, and blessing does not lift that rule.

use std::path::{Path, PathBuf};
use std::process::Command;

use reda_testkit::assert_golden;
use serde_json::Value;

const ALLOW_LIST: &str = "docs/dependency-edges.txt";
const TESTKIT: &str = "reda-testkit";

const HEADER: &str = "\
# Allowed dependency edges of the workspace crates: `crate -> dependency [kind, target]`.
# Checked against `cargo metadata` by crates/reda-testkit/tests/dependency_edges.rs.
# After adding a dependency, regenerate with
#   REDA_BLESS=1 cargo test -p reda-testkit --test dependency_edges
# and check that the new edge follows the layering in docs/spec/v0.1/architecture.md#crates.
";

/// One declared dependency of a workspace crate, rendered as one allow-list line.
fn edges(metadata: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    for package in metadata["packages"].as_array().expect("packages") {
        let from = package["name"].as_str().expect("package name");
        for dep in package["dependencies"].as_array().expect("dependencies") {
            let to = dep["name"].as_str().expect("dependency name");
            let qualifiers: Vec<&str> = [dep["kind"].as_str(), dep["target"].as_str()]
                .into_iter()
                .flatten()
                .collect();
            lines.push(if qualifiers.is_empty() {
                format!("{from} -> {to}")
            } else {
                format!("{from} -> {to} [{}]", qualifiers.join(", "))
            });
        }
    }
    lines.sort();
    lines.dedup();
    lines
}

/// Every edge onto `reda-testkit` that is not a plain dev-dependency.
fn testkit_violations(metadata: &Value) -> Vec<String> {
    let mut bad = Vec::new();
    for package in metadata["packages"].as_array().expect("packages") {
        let from = package["name"].as_str().expect("package name");
        for dep in package["dependencies"].as_array().expect("dependencies") {
            if dep["name"] == TESTKIT && dep["kind"] != "dev" {
                let kind = dep["kind"].as_str().unwrap_or("normal");
                bad.push(format!(
                    "{from} depends on {TESTKIT} as a {kind} dependency"
                ));
            }
        }
    }
    bad
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/<name> sits two levels below the workspace root")
        .to_path_buf()
}

fn cargo_metadata() -> Value {
    let out = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("run cargo metadata");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("cargo metadata emits JSON")
}

#[test]
fn reda_testkit_is_only_ever_a_dev_dependency() {
    let violations = testkit_violations(&cargo_metadata());
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn dependency_edges_match_the_allow_list() {
    let mut text = HEADER.to_owned();
    for line in edges(&cargo_metadata()) {
        text.push_str(&line);
        text.push('\n');
    }
    assert_golden(workspace_root().join(ALLOW_LIST), text.as_bytes());
}

fn fixture() -> Value {
    serde_json::json!({ "packages": [
        { "name": "reda-cli", "dependencies": [
            { "name": "reda-engine", "kind": null, "target": null },
            { "name": "reda-testkit", "kind": "dev", "target": null },
            { "name": "winapi", "kind": null, "target": "cfg(windows)" },
        ]},
        { "name": "reda-geom", "dependencies": [
            { "name": "reda-testkit", "kind": null, "target": null },
            { "name": "reda-testkit", "kind": "build", "target": null },
        ]},
    ]})
}

#[test]
fn edges_carry_kind_and_target_and_are_sorted() {
    assert_eq!(
        edges(&fixture()),
        [
            "reda-cli -> reda-engine",
            "reda-cli -> reda-testkit [dev]",
            "reda-cli -> winapi [cfg(windows)]",
            "reda-geom -> reda-testkit",
            "reda-geom -> reda-testkit [build]",
        ]
    );
}

#[test]
fn normal_and_build_dependencies_on_testkit_are_violations() {
    assert_eq!(
        testkit_violations(&fixture()),
        [
            "reda-geom depends on reda-testkit as a normal dependency",
            "reda-geom depends on reda-testkit as a build dependency",
        ]
    );
}
