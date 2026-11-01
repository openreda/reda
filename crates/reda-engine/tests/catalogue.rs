//! `docs/commands.json` is the registry's catalogue, and every Command in it is run by a test.

use std::path::{Path, PathBuf};

use reda_testkit::assert_golden;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The catalogue exactly as `reda commands --json` prints it.
fn catalogue_text() -> String {
    let catalogue = reda_engine::registry().catalogue();
    serde_json::to_string_pretty(&catalogue).unwrap() + "\n"
}

#[test]
fn docs_commands_json_is_fresh() {
    // Rerun with REDA_BLESS=1 after changing a Command, and commit the diff.
    assert_golden(
        repo_root().join("docs/commands.json"),
        catalogue_text().as_bytes(),
    );
}

/// Every `.rs` and `.jsonl` file under the test folders: `crates/*/tests/` and `tests/`.
fn test_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path
                .extension()
                .is_some_and(|ext| ext == "rs" || ext == "jsonl")
            {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path, text));
            }
        }
    }
    let root = repo_root();
    let mut out = Vec::new();
    walk(&root.join("tests"), &mut out);
    for krate in std::fs::read_dir(root.join("crates")).unwrap().flatten() {
        walk(&krate.path().join("tests"), &mut out);
    }
    out
}

/// A Command counts as run by a test when a test file names it as a string literal: as
/// `"id"` (a run call, a batch step, a CLI argument) or as `"id:preset"`.
#[test]
fn every_command_is_run_by_a_test() {
    let sources = test_sources();
    let this_file = Path::new(file!()).file_name().unwrap();
    let never_run: Vec<&str> = reda_engine::registry()
        .commands()
        .map(|meta| meta.id)
        .filter(|id| {
            let exact = format!("\"{id}\"");
            let preset = format!("\"{id}:");
            !sources.iter().any(|(path, text)| {
                path.file_name() != Some(this_file)
                    && (text.contains(&exact) || text.contains(&preset))
            })
        })
        .collect();
    assert!(
        never_run.is_empty(),
        "no test runs these Commands: {never_run:?}"
    );
}
