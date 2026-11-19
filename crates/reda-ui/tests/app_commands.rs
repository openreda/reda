//! The app registers its own view Commands (the layout Commands, `panel.show`,
//! `document.open`, `selection.set` and the window Commands the menus bind to) on top of
//! the engine's. They are not in
//! `docs/commands.json`, which is the engine's catalogue, so the engine's Command coverage
//! test does not see them. This test holds them to the same rule: every one is run by a
//! test.

use std::path::{Path, PathBuf};

use reda_ui::engine::command::Registry;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `.rs` and `.jsonl` file under `crates/*/tests/` and `tests/`.
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

/// The ids the app adds to a registry, leaving out those every registry has (`batch`).
fn app_ids() -> Vec<&'static str> {
    let builtin = Registry::new();
    let mut registry = Registry::new();
    reda_ui::layout::register_all(&mut registry);
    reda_ui::selection::register_all(&mut registry);
    reda_ui::bars::register_all(&mut registry);
    reda_ui::help::register_all(&mut registry);
    // The PCB view's Layer, routing and Via span Commands, and the Tools' on the board.
    reda_ui::pcb::register_all(&mut registry);
    registry
        .commands()
        .map(|meta| meta.id)
        .filter(|id| builtin.get(id).is_none())
        .collect()
}

#[test]
fn every_app_command_is_run_by_a_test() {
    let ids = app_ids();
    // The PCB view's and the Stackup editor's Commands are held to the rule too.
    for id in [
        "pcb.layer.step",
        "pcb.layer.activate",
        "pcb.layer.pick",
        "pcb.route.previous_layer",
        "pcb.route.cycle_via",
        "pcb.route.pick_via",
        "pcb.view.via_spans",
        "pcb.stackup.select",
    ] {
        assert!(ids.contains(&id), "{id} is not registered by the app");
    }

    let sources = test_sources();
    let this_file = Path::new(file!()).file_name().unwrap();
    let never_run: Vec<&str> = ids
        .into_iter()
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
        "no test runs these app Commands: {never_run:?}"
    );
}

#[test]
fn app_commands_are_not_in_the_engine_catalogue() {
    let engine = reda_ui::engine::registry();
    for id in app_ids() {
        assert!(
            engine.get(id).is_none(),
            "{id} is both an engine and an app Command"
        );
    }
}
