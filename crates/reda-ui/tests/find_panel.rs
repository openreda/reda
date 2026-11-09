//! The Find Panel as a model on a Shell, without a window: a search selects the first
//! match, opens its sheet and zooms to it; F3 and Shift+F3 step through the list; the
//! options (kinds, case, whole word, wildcards, regular expressions), the scope and Mask
//! Matching.

use std::path::Path;

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::Nm;
use reda_ui::highlight::Highlight;
use reda_ui::find::{self, Scope, Syntax};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn shell(dir: &TempDir) -> Shell {
    let library =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .new_project(&dir.path().join("project"), "Demo")
        .unwrap();
    shell
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// Places a 10k resistor on `document` at (`x`, `y`) mil; returns its Unit.
fn place(shell: &mut Shell, document: Uuid, x: i64, y: i64) -> Uuid {
    let out = shell
        .run(
            "schematic.component.place",
            json!({"document": document, "part": R_10K,
                   "at": {"x": format!("{x}mil"), "y": format!("{y}mil")}}),
        )
        .unwrap();
    uuid(&out["result"]["unit"])
}

fn add_sheet(shell: &mut Shell) -> Uuid {
    let added = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    uuid(&added["result"]["uuid"])
}

fn search(shell: &mut Shell, text: &str) {
    find::edit(shell, |p| p.text = text.to_owned());
    find::search(shell);
}

fn labels(shell: &Shell) -> Vec<String> {
    find::state(shell)
        .results()
        .iter()
        .map(|r| r.label())
        .collect()
}

#[test]
fn find_selects_the_match_opens_its_sheet_and_zooms_to_it() {
    let dir = TempDir::new("find");
    let mut shell = shell(&dir);
    place(&mut shell, sheet(), 0, 0);
    let second = add_sheet(&mut shell);
    let r2 = place(&mut shell, second, 2000, 1000);
    shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();

    search(&mut shell, "r2");
    assert_eq!(labels(&shell), ["R2 (Designator) on Sheet 2"]);
    // The first match is shown at once: its sheet, selected and zoomed to.
    assert_eq!(shell.active_document().unwrap().document, second);
    assert_eq!(shell.selection().objects, [r2]);
    let (lo, hi) = shell.document_tab(second).unwrap().zoom_to.unwrap();
    let x = Nm(2000 * 25_400);
    assert!(lo.x <= x && x <= hi.x, "{lo:?}..{hi:?}");

    // By value: both Resistors; going to the first goes back to the first sheet.
    search(&mut shell, "10k");
    assert_eq!(find::state(&shell).results().len(), 2);
    assert_eq!(shell.active_document().unwrap().document, sheet());
    find::go(&mut shell, 1);
    assert_eq!(shell.active_document().unwrap().document, second);

    // Nothing found leaves the selection alone and says so.
    search(&mut shell, "nothing like it");
    assert!(find::state(&shell).results().is_empty());
    assert!(find::state(&shell).searched());
    assert_eq!(shell.selection().objects, [r2]);
}

#[test]
fn f3_and_shift_f3_step_through_the_matches_wrapping_around() {
    let dir = TempDir::new("find-step");
    let mut shell = shell(&dir);
    let units: Vec<Uuid> = (0..3).map(|i| place(&mut shell, sheet(), i * 1000, 0)).collect();
    search(&mut shell, "R");
    find::edit(&mut shell, |p| p.kinds.values = false);
    find::search(&mut shell);
    assert_eq!(shell.selection().objects, [units[0]]);
    shell
        .run("schematic.find.step:next", json!({}))
        .unwrap();
    assert_eq!(shell.selection().objects, [units[1]]);
    assert_eq!(find::state(&shell).current(), Some(1));
    shell
        .run("schematic.find.step:previous", json!({}))
        .unwrap();
    shell
        .run("schematic.find.step:previous", json!({}))
        .unwrap();
    assert_eq!(shell.selection().objects, [units[2]]);
}

#[test]
fn find_options_scope_and_mask_matching() {
    let dir = TempDir::new("find-options");
    let mut shell = shell(&dir);
    let r1 = place(&mut shell, sheet(), 0, 0);
    let r2 = place(&mut shell, sheet(), 1000, 0);
    let second = add_sheet(&mut shell);
    place(&mut shell, second, 0, 0);
    let label = shell
        .run(
            "schematic.net_label.add",
            json!({"document": SHEET, "name": "SDA_1", "at": {"x": "0mil", "y": "2000mil"}}),
        )
        .unwrap();
    let label = uuid(&label["result"]["net_label"]);
    shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();

    // All text, as a regular expression with case.
    find::edit(&mut shell, |p| {
        p.kinds.designators = false;
        p.kinds.values = false;
        p.kinds.nets = false;
        p.syntax = Syntax::Regex;
        p.match_case = true;
    });
    search(&mut shell, "^SDA_[0-9]$");
    assert_eq!(labels(&shell), ["SDA_1 (Net Label) on Sheet 1"]);
    assert_eq!(shell.selection().objects, [label]);
    find::edit(&mut shell, |p| p.text = "(".to_owned());
    find::search(&mut shell);
    assert!(find::state(&shell).error().is_some());

    // Designators as wildcards, on this sheet, then within the selection.
    find::edit(&mut shell, |p| {
        p.kinds = find::Kinds {
            designators: true,
            values: false,
            nets: false,
            text: false,
        };
        p.syntax = Syntax::Wildcard;
        p.scope = Scope::Sheet;
    });
    search(&mut shell, "r?");
    assert_eq!(
        labels(&shell),
        ["R1 (Designator) on Sheet 1", "R2 (Designator) on Sheet 1"]
    );
    shell
        .run("selection.set", json!({"objects": [r2]}))
        .unwrap();
    find::edit(&mut shell, |p| p.scope = Scope::Selection);
    find::search(&mut shell);
    assert_eq!(labels(&shell), ["R2 (Designator) on Sheet 1"]);

    // Mask Matching masks everything but the matches.
    find::edit(&mut shell, |p| {
        p.scope = Scope::Sheet;
        p.mask = true;
    });
    find::search(&mut shell);
    let highlight = shell.engine().view().get::<Highlight>().unwrap();
    assert!(highlight.mask);
    let mut matched = vec![r1, r2];
    matched.sort();
    assert_eq!(highlight.objects, matched);
}
