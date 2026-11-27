//! The data-driven scenarios that need the app: each `.jsonl` script in
//! `crates/reda-ui/tests/scenarios/` is replayed in a REDA window (`egui_kittest`), with
//! frames drawn after every step, and the Project it leaves is compared with the goldens
//! beside the script, as `reda_testkit::run_scenarios` does for `tests/scenarios/`.
//!
//! A script here may use the app's view Commands, which need a window: `view.cursor.step`
//! and `view.cursor.click` (the `input` scenario, docs/spec/v0.3/testing.md#scenarios).
//! What a step makes a Tool do (a click placing a Via) runs on the canvas's next frame,
//! so it is not a line of the script; the goldens show it.
//!
//! `REDA_BLESS=1` rewrites the goldens and each script with its steps' outcomes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use reda_testkit::{TempDir, read_tree};
use reda_ui::App;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::journal::{self, JournalEntry, Source};
use reda_ui::engine::{Engine, store};
use reda_ui::shell::Shell;

/// The one file the goldens leave out: its content `*` would ignore itself.
const SKIPPED: &str = ".reda/.gitignore";

fn scenarios() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios")
}

fn bless() -> bool {
    std::env::var_os("REDA_BLESS").is_some_and(|v| v == "1")
}

/// Replays `script` in a window; the Project files it leaves, and the Journal of the
/// script's own steps.
fn replay(script: &Path) -> Result<(BTreeMap<String, Vec<u8>>, String), String> {
    let text = std::fs::read_to_string(script).map_err(|e| e.to_string())?;
    let mut steps = journal::parse(&text).map_err(|e| e.to_string())?;
    let work = TempDir::new("ui-scenario");
    let placeholders = journal::relocate_new_projects(&mut steps, work.path());
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .version(reda_testkit::TEST_VERSION)
        .user_dirs(None)
        .starter_library(Some(reda_testkit::starter_library()))
        .headless(false)
        .build();
    let mut h = Harness::builder()
        .with_size([1000.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(Shell::new(engine)));
    h.run();
    let mut ran = Vec::new();
    journal::replay_with(
        |id, params| {
            let outcome = h.state_mut().shell_mut().run(id, params.clone());
            // The canvas carries out what the step asked of it.
            h.run();
            ran.push(JournalEntry {
                id: id.to_owned(),
                params,
                source: Source::Cli,
                outcome: outcome.clone(),
            });
            outcome
        },
        &steps,
    )
    .map_err(|e| e.to_string())?;
    let shell = h.state().shell();
    let project = shell
        .engine()
        .project()
        .ok_or("the script opens no Project")?;
    let files = store::encode(project).map_err(|e| e.to_string())?;
    let files = files
        .into_iter()
        .filter(|(path, _)| path != SKIPPED)
        .collect();
    let mut exported = journal::export(&ran);
    for (i, placeholder) in placeholders.iter().enumerate() {
        let Some(placeholder) = placeholder.as_str() else {
            continue;
        };
        let relocated = work.path().join((i + 1).to_string());
        let escaped = |s: &str| {
            let quoted = serde_json::to_string(s).unwrap();
            quoted[1..quoted.len() - 1].to_owned()
        };
        exported = exported.replace(
            &escaped(&relocated.to_string_lossy()),
            &escaped(placeholder),
        );
    }
    Ok((files, exported))
}

#[test]
fn every_app_scenario_replays_to_its_goldens() {
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(scenarios())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    scripts.sort();
    assert!(
        !scripts.is_empty(),
        "no scripts in {}",
        scenarios().display()
    );
    let mut failures = Vec::new();
    for script in scripts {
        let name = script.file_name().unwrap().to_string_lossy().into_owned();
        let (actual, exported) = match replay(&script) {
            Ok(out) => out,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let goldens = script.with_extension("");
        if bless() {
            let _ = std::fs::remove_dir_all(&goldens);
            for (path, bytes) in &actual {
                let path = goldens.join(path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, bytes).unwrap();
            }
            std::fs::write(&script, exported).unwrap();
            continue;
        }
        let expected = read_tree(&goldens);
        for (path, bytes) in &actual {
            match expected.get(path) {
                None => failures.push(format!("{name}: golden {path} is missing")),
                Some(golden) if golden != bytes => failures.push(format!(
                    "{name}: golden {path} differs\n--- expected\n{}\n--- actual\n{}",
                    String::from_utf8_lossy(golden),
                    String::from_utf8_lossy(bytes)
                )),
                Some(_) => {}
            }
        }
        for path in expected.keys().filter(|p| !actual.contains_key(*p)) {
            failures.push(format!("{name}: golden {path} is stale"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failure(s); rerun with REDA_BLESS=1 to accept changed goldens\n{}",
        failures.len(),
        failures.join("\n")
    );
}
