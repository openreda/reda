//! The data-driven scenarios that need the app: each `.jsonl` script in
//! `crates/reda-ui/tests/scenarios/` is replayed in a REDA window (`egui_kittest`), with
//! frames drawn after every step, through `reda_testkit::run_scenarios_with`: goldens,
//! blessing and placeholders work as for `tests/scenarios/`.
//!
//! A script here may use the app's view Commands, which need a window: `view.cursor.step`,
//! `view.cursor.click` and the nudges `pcb.nudge`, `library.footprint.nudge` and
//! `library.symbol.nudge` (the `input` scenario, docs/spec/v0.3/testing.md#scenarios).
//! What a step makes a Tool do (a click placing a Via), or the edit a nudge runs, is not
//! a line of the script; the goldens show it.

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use reda_testkit::Replayed;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::journal::{self, JournalEntry, ScriptEntry, Source};
use reda_ui::shell::Shell;

fn scenarios() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios")
}

/// Replays `steps` in a window. The Journal is of the script's own steps, not of what the
/// Tools ran on the frames after them.
fn replay(steps: &[ScriptEntry]) -> Result<Replayed, String> {
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
        steps,
    )
    .map_err(|e| e.to_string())?;
    Ok(Replayed {
        files: reda_testkit::project_files(h.state().shell().engine())?,
        journal: journal::export(&ran),
    })
}

#[test]
fn every_app_scenario_replays_to_its_goldens() {
    reda_testkit::run_scenarios_with(scenarios(), replay);
}
