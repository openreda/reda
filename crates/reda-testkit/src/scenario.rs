//! The data-driven scenario harness: replays every `.jsonl` script in a folder on a fresh
//! [`crate::test_engine`] and compares the resulting Documents with the goldens beside it.
//!
//! A script `<name>.jsonl` is in the Journal format (`reda_engine::journal`). Its goldens
//! are the canonical bytes of every file the open Project would save, under `<name>/`,
//! except `.reda/.gitignore`. The goldens come from `store::encode` after the last step, so
//! a script need not (and should not) run `project.save`.
//!
//! `project.new` writes the new Project to disk, so the harness points every `project.new`
//! step at a temporary folder (see `journal::relocate_new_projects`); the path a script
//! gives, conventionally `/nonexistent`, is only a placeholder.
//!
//! Blessing (`REDA_BLESS=1`) rewrites the goldens, removes stale ones, and rewrites each
//! script as the replay's Journal, so steps written without a `result` get one. The
//! temporary folders are written back as the script's own placeholder paths.

use std::collections::BTreeMap;
use std::path::Path;

use reda_engine::{journal, store};

use crate::{TempDir, read_tree, test_engine};

/// The one file the goldens leave out: its content `*` would ignore itself.
const SKIPPED: &str = ".reda/.gitignore";

/// Runs every scenario script in `dir` and panics listing every failure.
/// With `REDA_BLESS=1` it rewrites the goldens and scripts instead.
#[track_caller]
pub fn run_scenarios(dir: impl AsRef<Path>) {
    let bless = std::env::var_os("REDA_BLESS").is_some_and(|v| v == "1");
    let failures = scenario_failures(dir.as_ref(), bless);
    if !failures.is_empty() {
        panic!(
            "{} scenario failure(s); rerun with REDA_BLESS=1 to accept changed goldens\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

/// Every failure of the scenarios in `dir`, one message each. `bless` rewrites the goldens
/// and scripts of each script that replays, rather than comparing. A folder with no script
/// is a failure.
pub fn scenario_failures(dir: &Path, bless: bool) -> Vec<String> {
    let mut scripts: Vec<_> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "jsonl"))
                .collect()
        })
        .unwrap_or_default();
    scripts.sort();
    if scripts.is_empty() {
        return vec![format!("no .jsonl scenario scripts in {}", dir.display())];
    }
    let mut failures = Vec::new();
    for script in scripts {
        let name = script.file_name().unwrap().to_string_lossy().into_owned();
        if let Err(mut more) = run_one(&script, bless) {
            for failure in &mut more {
                *failure = format!("{name}: {failure}");
            }
            failures.extend(more);
        }
    }
    failures
}

fn run_one(script: &Path, bless: bool) -> Result<(), Vec<String>> {
    let text = std::fs::read_to_string(script).map_err(|e| vec![format!("unreadable: {e}")])?;
    let mut steps = journal::parse(&text).map_err(|e| vec![e.to_string()])?;
    let work = TempDir::new("scenario");
    let placeholders = journal::relocate_new_projects(&mut steps, work.path());
    let mut engine = test_engine();
    journal::replay(&mut engine, &steps).map_err(|e| vec![e.to_string()])?;
    let mut actual = BTreeMap::new();
    if let Some(project) = engine.project() {
        let files = store::encode(project).map_err(|e| vec![format!("encoding: {e}")])?;
        actual.extend(files.into_iter().filter(|(path, _)| path != SKIPPED));
    }

    let golden_dir = script.with_extension("");
    if bless {
        let _ = std::fs::remove_dir_all(&golden_dir);
        for (path, bytes) in &actual {
            let path = golden_dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap())
                .and_then(|()| std::fs::write(&path, bytes))
                .map_err(|e| vec![format!("blessing {}: {e}", path.display())])?;
        }
        let mut exported = journal::export(engine.journal());
        for (i, placeholder) in placeholders.iter().enumerate() {
            let Some(placeholder) = placeholder.as_str() else {
                continue;
            };
            let relocated = work.path().join((i + 1).to_string());
            let escaped = |s: &str| {
                let quoted = serde_json::to_string(s).expect("strings serialise");
                quoted[1..quoted.len() - 1].to_owned()
            };
            exported = exported.replace(
                &escaped(&relocated.to_string_lossy()),
                &escaped(placeholder),
            );
        }
        std::fs::write(script, exported).map_err(|e| vec![format!("rewriting the script: {e}")])?;
        return Ok(());
    }

    let expected = read_tree(&golden_dir);
    let mut failures = Vec::new();
    for (path, bytes) in &actual {
        match expected.get(path) {
            None => failures.push(format!("golden {path} is missing")),
            Some(golden) if golden != bytes => failures.push(format!(
                "golden {path} differs\n--- expected\n{}\n--- actual\n{}",
                String::from_utf8_lossy(golden),
                String::from_utf8_lossy(bytes)
            )),
            Some(_) => {}
        }
    }
    for path in expected.keys().filter(|p| !actual.contains_key(*p)) {
        failures.push(format!(
            "golden {path} is stale: the replay has no such file"
        ));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}
