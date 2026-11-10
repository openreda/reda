//! The non-modal Select Similar window (docs/spec/v0.1/editing.md#select-similar): opened
//! by Shift+F then a click or from the context menu, rows generated from the object schema
//! in three groups, its options, Apply without closing, the Selection Filter, and Mask
//! Matching really masking: masked objects can't be hit
//! (docs/spec/v0.1/editing.md#highlighting).

use std::path::{Path, PathBuf};

use egui::Modifiers;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::canvas::interact::Pointer;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::commands::similar::{AttributeGroup, SimilarScope, Template};
use reda_ui::engine::model::Uuid;
use reda_ui::schematic_editor::{self, Scale};
use reda_ui::shell::Shell;
use reda_ui::similar::{self, Rule, Show};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const MIL: f64 = 25_400.0;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn shell(dir: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();
    shell
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

/// Places a resistor at (`x`, 0) mil with `value`; returns its Unit.
fn resistor(shell: &mut Shell, x: i64, value: &str) -> Uuid {
    let out = shell
        .run(
            "schematic.component.place",
            json!({"document": SHEET, "part": R_10K,
                   "at": {"x": format!("{x}mil"), "y": 0}, "fields": {"value": value}}),
        )
        .unwrap();
    out["result"]["unit"].as_str().unwrap().parse().unwrap()
}

/// R1 and R2 are 10k, R3 is 4k7, 1000 mil apart.
fn three(shell: &mut Shell) -> [Uuid; 3] {
    [
        resistor(shell, 0, "10k"),
        resistor(shell, 1000, "10k"),
        resistor(shell, 2000, "4k7"),
    ]
}

/// A plain click at (`x`, `y`) mil on the sheet, running what the editor asks for.
fn click(shell: &mut Shell, x: f64, y: f64, modifiers: Modifiers) -> Vec<(String, Value)> {
    let events = [Pointer::Click {
        at: (x * MIL, y * MIL),
        modifiers,
    }];
    let commands = schematic_editor::pointer(shell, sheet(), &events, Scale::at(MIL));
    for (id, params) in &commands {
        shell.run(id, params.clone()).unwrap();
    }
    commands
}

fn selected(shell: &Shell) -> Vec<Uuid> {
    let mut s = shell.selection().objects.clone();
    s.sort();
    s
}

fn sorted(mut v: Vec<Uuid>) -> Vec<Uuid> {
    v.sort();
    v
}

#[test]
fn the_context_menu_opens_the_window_with_schema_rows_in_three_groups() {
    let dir = TempDir::new("similar-window-rows");
    let mut shell = shell(&dir);
    let [r1, ..] = three(&mut shell);
    shell
        .run("selection.similar.window", json!({"objects": [r1]}))
        .unwrap();
    let w = similar::window(&shell).expect("open").clone();
    assert_eq!(w.title, "Select Similar to R1");
    assert_eq!(w.template, Template::Object(r1));
    assert_eq!(w.scopes, [SimilarScope::Sheet, SimilarScope::Project]);
    assert_eq!(w.scope, SimilarScope::Sheet);
    // Kind starts at Same and the rest at Any.
    let ty = w.row("type").unwrap();
    assert_eq!((ty.group, &ty.rule), (AttributeGroup::Kind, &Rule::Same));
    let value = w.row("fields.value").unwrap();
    assert_eq!(value.group, AttributeGroup::ObjectSpecific);
    assert_eq!((value.value.as_str(), &value.rule), ("10k", &Rule::Any));
    assert_eq!(w.row("at.x").unwrap().group, AttributeGroup::Graphical);
    // Options: Select on, Zoom off, Clear Existing on, Mask, Open Properties off.
    assert!(w.select && !w.zoom && w.clear_existing && !w.open_properties);
    assert_eq!(w.show, Show::Mask);
    // Opening it changes nothing yet.
    assert!(shell.highlight().objects.is_empty());
}

#[test]
fn apply_selects_and_masks_the_matches_and_keeps_the_window_open() {
    let dir = TempDir::new("similar-window-apply");
    let mut shell = shell(&dir);
    let [r1, r2, r3] = three(&mut shell);
    shell
        .run("selection.similar.window", json!({"objects": [r1]}))
        .unwrap();
    similar::window_mut(&mut shell)
        .unwrap()
        .set_rule("fields.value", Rule::Same);
    assert_eq!(similar::apply(&mut shell).unwrap(), 2);
    assert_eq!(selected(&shell), sorted(vec![r1, r2]));
    let highlight = shell.highlight().clone();
    assert!(highlight.mask);
    assert!(highlight.objects.contains(&r1) && highlight.objects.contains(&r2));
    assert!(!highlight.objects.contains(&r3));
    assert_eq!(
        similar::window(&shell).unwrap().found,
        Some(2),
        "still open"
    );

    // Masked really masks: R3 can't be hit, so a click on it picks nothing.
    assert!(shell.is_masked(sheet(), r3, None));
    assert!(!shell.is_masked(sheet(), r2, None));
    click(&mut shell, 2000.0, 120.0, Modifiers::NONE);
    assert!(
        !selected(&shell).contains(&r3),
        "a masked object can't be hit"
    );
    click(&mut shell, 1000.0, 120.0, Modifiers::NONE);
    assert_eq!(selected(&shell), [r2], "a matched one can");
    // Shift+C clears the mask and leaves the selection.
    shell.run("view.highlight.clear", json!({})).unwrap();
    assert!(!shell.is_masked(sheet(), r3, None));
    assert_eq!(selected(&shell), [r2]);
    click(&mut shell, 2000.0, 120.0, Modifiers::NONE);
    assert_eq!(selected(&shell), [r3]);
}

#[test]
fn the_options_set_how_the_result_is_shown() {
    let dir = TempDir::new("similar-window-options");
    let mut shell = shell(&dir);
    let [r1, r2, r3] = three(&mut shell);
    shell
        .run("selection.set", json!({"objects": [r3]}))
        .unwrap();
    shell
        .run("selection.similar.window", json!({"objects": [r1]}))
        .unwrap();
    {
        let w = similar::window_mut(&mut shell).unwrap();
        w.set_rule("fields.value", Rule::Pattern("1*".into()));
        w.clear_existing = false;
        w.show = Show::Dim;
    }
    similar::apply(&mut shell).unwrap();
    assert_eq!(selected(&shell), sorted(vec![r1, r2, r3]), "added to R3");
    assert!(!shell.highlight().mask, "dimmed, not masked");
    assert!(shell.highlight().objects.contains(&r1));
    {
        let w = similar::window_mut(&mut shell).unwrap();
        w.set_rule("fields.value", Rule::Different);
        w.clear_existing = true;
        w.show = Show::Normal;
        w.select = false;
    }
    assert_eq!(similar::apply(&mut shell).unwrap(), 1);
    assert_eq!(
        selected(&shell),
        sorted(vec![r1, r2, r3]),
        "Select off keeps it"
    );
    assert!(shell.highlight().objects.is_empty(), "Normal: no highlight");
    // Kind Any finds every object on the sheet, not just Units.
    similar::window_mut(&mut shell)
        .unwrap()
        .set_rule("type", Rule::Any);
    similar::window_mut(&mut shell)
        .unwrap()
        .set_rule("fields.value", Rule::Any);
    assert!(similar::apply(&mut shell).unwrap() >= 3);
}

#[test]
fn shift_f_then_a_click_picks_the_template_instead_of_selecting() {
    let dir = TempDir::new("similar-window-pick");
    let mut shell = shell(&dir);
    let [_, r2, _] = three(&mut shell);
    shell.run("selection.similar.pick", json!({})).unwrap();
    assert!(similar::picking(&shell));
    let ran = click(&mut shell, 1000.0, 120.0, Modifiers::NONE);
    assert!(ran.is_empty(), "the click selects nothing: {ran:?}");
    assert!(!similar::picking(&shell));
    assert_eq!(
        similar::window(&shell).unwrap().template,
        Template::Object(r2)
    );
    // The next click picks as usual.
    click(&mut shell, 1000.0, 120.0, Modifiers::NONE);
    assert_eq!(selected(&shell), [r2]);
}

#[test]
fn the_window_wont_open_for_a_filtered_kind_and_leaves_filtered_kinds_out() {
    let dir = TempDir::new("similar-window-filter");
    let mut shell = shell(&dir);
    let [r1, ..] = three(&mut shell);
    shell
        .run(
            "selection.filter.set",
            json!({"document": SHEET, "kinds": {"unit": false}}),
        )
        .unwrap();
    let err = shell
        .run("selection.similar.window", json!({"objects": [r1]}))
        .unwrap_err();
    assert!(err.message.contains("Selection Filter"), "{}", err.message);
    assert!(similar::window(&shell).is_none());
    shell.run("selection.similar.pick", json!({})).unwrap();
    click(&mut shell, 0.0, 120.0, Modifiers::NONE);
    assert!(similar::window(&shell).is_none());
}

fn harness(dir: &TempDir) -> Harness<'static, App> {
    let shell = shell(dir);
    Harness::builder()
        .with_size([1400.0, 900.0])
        .with_step_dt(0.25)
        .with_max_steps(100)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

#[test]
fn the_window_is_non_modal_and_shift_f_reaches_it() {
    let dir = TempDir::new("similar-window-ui");
    let mut h = harness(&dir);
    let [r1, r2, _] = three(h.state_mut().shell_mut());
    h.run();
    h.state_mut()
        .shell_mut()
        .run("selection.similar.window", json!({"objects": [r1]}))
        .unwrap();
    h.run();
    h.get_by_label("Select Similar to R1");
    // Its rows are generated: Kind, Object Specific and Graphical headings.
    for heading in ["Kind", "Object Specific", "Graphical"] {
        assert!(h.query_by_label(heading).is_some(), "{heading}");
    }
    h.state_mut()
        .shell_mut()
        .run("selection.set", json!({"objects": [r2]}))
        .unwrap();
    h.run();
    // Non-modal: the rest of the window still takes input while it is open.
    h.get_by_role_and_label(Role::Tab, "Sheet 1").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    assert!(
        h.query_by_label("Select Similar to R1").is_some(),
        "Apply keeps it open"
    );
    assert_eq!(similar::window(h.state().shell()).unwrap().found, Some(3));
    h.get_by_role_and_label(Role::Button, "OK").click();
    h.run();
    assert!(
        h.query_by_label("Select Similar to R1").is_none(),
        "OK closes it"
    );
    // Shift+F is bound.
    h.key_press_modifiers(Modifiers::SHIFT, egui::Key::F);
    h.run();
    assert!(similar::picking(h.state().shell()));
}
