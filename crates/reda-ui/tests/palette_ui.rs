//! The Command Palette in the window: `Mod+Shift+P` opens it, typing narrows it, Enter
//! runs the top entry, and a Command that needs params opens its generated form, whose
//! Run button waits until the fields are valid.

use std::path::Path;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::shell::Shell;
use serde_json::json;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn harness(home: &Path) -> Harness<'static, App> {
    let app = App::new(Shell::new(engine_in(home)));
    Harness::builder()
        .with_size([900.0, 560.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app)
}

fn last_command(h: &Harness<'_, App>) -> Option<(String, serde_json::Value)> {
    h.state()
        .shell()
        .engine()
        .journal()
        .last()
        .map(|e| (e.id.clone(), e.params.clone()))
}

/// The Keymap opened the Palette (`app.dialog.open:command_palette`) and nothing has run
/// since.
fn assert_nothing_ran_since_opening(h: &Harness<'_, App>) {
    assert_eq!(
        last_command(h),
        Some(("app.dialog.open:command_palette".to_owned(), json!({})))
    );
}

fn open_palette(h: &mut Harness<'_, App>) {
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::P);
    h.run();
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Command Palette")
            .is_some(),
        "the Palette did not open"
    );
}

#[test]
fn mod_shift_p_opens_the_palette_and_enter_runs_the_best_match() {
    let home = TempDir::new("palette-home");
    let mut h = harness(home.path());
    h.run();
    open_palette(&mut h);
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text("show journal");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        last_command(&h),
        Some(("panel.show:journal".to_owned(), json!({})))
    );
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Command Palette")
            .is_none()
    );
}

#[test]
fn unavailable_entries_show_why_and_escape_closes_the_palette() {
    let home = TempDir::new("palette-home");
    let mut h = harness(home.path());
    h.run();
    open_palette(&mut h);
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text("save all");
    h.run();
    assert!(h.query_by_label_contains("no Project is open").is_some());
    // Enter on an entry that cannot run does nothing.
    h.key_press(Key::Enter);
    h.run();
    assert_nothing_ran_since_opening(&h);
    h.key_press(Key::Escape);
    h.run();
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Command Palette")
            .is_none()
    );
}

#[test]
fn the_palette_preset_opens_the_palette_too() {
    let home = TempDir::new("palette-home");
    let mut h = harness(home.path());
    h.run();
    h.state_mut()
        .shell_mut()
        .run("app.dialog.open:command_palette", json!({}))
        .unwrap();
    h.run();
    h.run();
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Command Palette")
            .is_some()
    );
}

#[test]
fn a_command_that_needs_params_opens_its_form_which_runs_once_the_fields_fit() {
    let home = TempDir::new("palette-home");
    let mut h = harness(home.path());
    h.run();
    open_palette(&mut h);
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text("set theme");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    // No params: the form opens instead of a run.
    assert_nothing_ran_since_opening(&h);
    assert!(h.query_by_label("Set Theme").is_some());
    assert!(h.query_by_label("Required").is_some());
    assert!(h.get_by_label("Run").accesskit_node().is_disabled());

    h.get_by_role_and_label(Role::ComboBox, "Theme").click();
    h.run();
    h.get_by_label("dark").click();
    h.run();
    assert!(h.query_by_label("Required").is_none());
    h.get_by_label("Run").click();
    h.run();
    assert_eq!(
        last_command(&h),
        Some(("view.theme.set".to_owned(), json!({"theme": "dark"})))
    );
    assert!(h.query_by_label("Set Theme").is_none());
}

#[test]
fn a_form_flags_a_bad_value_under_its_field() {
    let home = TempDir::new("palette-home");
    let mut h = harness(home.path());
    h.run();
    open_palette(&mut h);
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text("show tab");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!(h.query_by_label("Required").is_some());
    let field = h.get_by_role_and_label(Role::TextInput, "Tab");
    field.focus();
    field.type_text("nowhere");
    h.run();
    h.get_by_label("Run").click();
    h.run();
    // The layout refuses an unknown tab: the form stays open and says why.
    assert!(h.query_by_label("Run").is_some());
    // Once in the form, once in the status bar.
    assert_eq!(h.query_all_by_label_contains("is not a tab").count(), 2);
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.query_by_label("Run").is_none());
}

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

/// A window on a Project with R1 and R2 on Sheet 1, joined by the Net SIG, and a PCB.
fn demo(work: &Path) -> Harness<'static, App> {
    let library =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test");
    let engine = Engine::builder()
        .ids(reda_ui::engine::command::SeededIds::new(0))
        .clock(reda_ui::engine::command::FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, "Demo").unwrap();
    for x in ["0mm", "10mm"] {
        shell
            .run(
                "schematic.component.place",
                json!({"document": SHEET, "part": R_10K, "at": {"x": x, "y": 0}}),
            )
            .unwrap();
    }
    shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": [
                {"x": "0mm", "y": "-3.81mm"}, {"x": "10mm", "y": "-3.81mm"}]}),
        )
        .unwrap();
    shell
        .run(
            "schematic.net_label.add",
            json!({"document": SHEET, "name": "SIG", "at": {"x": "5mm", "y": "-3.81mm"}}),
        )
        .unwrap();
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn uuid(s: &str) -> reda_ui::engine::model::Uuid {
    s.parse().unwrap()
}

fn palette_open(h: &Harness<'_, App>) -> bool {
    h.query_by_role_and_label(Role::TextInput, "Command Palette")
        .is_some()
}

fn query(h: &Harness<'_, App>) -> String {
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .accesskit_node()
        .value()
        .unwrap_or_default()
}

/// Opens the Palette empty with `Mod+P`, types `text` and presses Enter.
fn choose(h: &mut Harness<'_, App>, text: &str) {
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text(text);
    h.run();
    h.key_press(Key::Enter);
    h.run();
    h.run();
    assert!(!palette_open(h), "choosing a row closes the Palette");
}

/// The labels of the buttons that start with one of `titles`, in tree order: the
/// Palette's rows.
fn row_labels(h: &Harness<'_, App>, titles: &[&str]) -> Vec<String> {
    h.get_all_by_role(Role::Button)
        .filter_map(|n| n.accesskit_node().label())
        .filter(|l| titles.iter().any(|t| l.starts_with(t)))
        .collect()
}

/// The designators of the selected Units.
fn selected(h: &Harness<'_, App>) -> Vec<String> {
    let shell = h.state().shell();
    let project = shell.engine().project().unwrap();
    let sheet = project.schematic_sheet(uuid(SHEET)).unwrap();
    (shell.selection().objects.iter())
        .filter_map(|o| sheet.unit(*o))
        .map(|u| u.designator.clone())
        .collect()
}

#[test]
fn mod_p_opens_it_empty_in_the_command_field_s_place_without_dimming() {
    let work = TempDir::new("palette-ui-work");
    let mut h = demo(work.path());
    let field = h
        .get_by_label_contains("Search Documents, Components, Nets")
        .rect();
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    h.run();
    assert!(palette_open(&h));
    assert_eq!(query(&h), "");
    let input = h
        .get_by_role_and_label(Role::TextInput, "Command Palette")
        .rect();
    assert!(
        (input.top() - field.top()).abs() < 12.0,
        "{input:?} {field:?}"
    );
    assert!(input.left() <= field.center().x && field.center().x <= input.right());
    // No modal layer: the window behind is not dimmed.
    assert_eq!(h.ctx.memory(|m| m.top_modal_layer()), None);
    // Empty, it shows the Documents.
    assert_eq!(row_labels(&h, &["Sheet 1", "Demo"]).len(), 2);
    h.key_press(Key::Escape);
    h.run();
    assert!(!palette_open(&h));

    // Mod+Shift+P opens it on Commands.
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::P);
    h.run();
    assert_eq!(query(&h), ">");
    h.get_by_label_contains("Save All");
}

#[test]
fn clicking_the_command_field_opens_it_empty_and_a_click_elsewhere_closes_it() {
    let work = TempDir::new("palette-ui-work");
    let mut h = demo(work.path());
    h.get_by_label_contains("Search Documents, Components, Nets")
        .click();
    h.run();
    h.run();
    assert!(palette_open(&h));
    assert_eq!(query(&h), "");
    // A press and release far below the Palette.
    let at = egui::pos2(550.0, 600.0);
    h.hover_at(at);
    h.run();
    h.drag_at(at);
    h.run();
    h.drop_at(at);
    h.run();
    assert!(!palette_open(&h));
}

#[test]
fn no_prefix_goes_to_a_component() {
    let work = TempDir::new("palette-ui-work");
    let mut h = demo(work.path());
    choose(&mut h, "R2");
    assert_eq!(selected(&h), ["R2"]);
    assert_eq!(
        h.state().shell().layout().active_document(),
        Some(uuid(SHEET))
    );
}

#[test]
fn at_selects_and_zooms_to_a_component() {
    let work = TempDir::new("palette-ui-work");
    let mut h = demo(work.path());
    choose(&mut h, "@R1");
    assert_eq!(selected(&h), ["R1"]);
    let ids: Vec<String> = (h.state().shell().engine().journal().iter())
        .map(|e| e.id.clone())
        .collect();
    assert_eq!(ids.last().map(String::as_str), Some("selection.set"));
}

#[test]
fn hash_highlights_a_net() {
    let work = TempDir::new("palette-ui-work");
    let mut h = demo(work.path());
    choose(&mut h, "#SIG");
    assert_eq!(
        last_command(&h),
        Some(("view.highlight".into(), json!({"net": "SIG"})))
    );
    assert_eq!(h.state().shell().highlight().net.as_deref(), Some("SIG"));
}

#[test]
fn colon_opens_a_document() {
    let work = TempDir::new("palette-ui-work");
    let mut h = demo(work.path());
    choose(&mut h, ":Demo");
    assert_eq!(
        last_command(&h),
        Some(("document.open".into(), json!({"document": PCB})))
    );
    assert_eq!(
        h.state().shell().layout().active_document(),
        Some(uuid(PCB))
    );
}

#[test]
fn greater_than_runs_a_command() {
    let work = TempDir::new("palette-ui-work");
    let mut h = demo(work.path());
    choose(&mut h, ">show journal");
    assert_eq!(
        last_command(&h),
        Some(("panel.show:journal".to_owned(), json!({})))
    );
}

#[test]
fn question_mark_lists_the_prefixes_and_choosing_one_fills_it_in() {
    let work = TempDir::new("palette-ui-work");
    let mut h = demo(work.path());
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text("?");
    h.run();
    let rows = row_labels(&h, &[">", "@", "#", ":", "?"]);
    assert_eq!(rows.len(), 5, "{rows:?}");
    for (row, scope) in rows.iter().zip([
        "Commands",
        "Components",
        "Nets",
        "Documents",
        "These prefixes",
    ]) {
        assert!(row.contains(scope), "{rows:?}");
    }
    h.get_all_by_role(Role::Button)
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with('#'))
        })
        .unwrap()
        .click();
    h.run();
    h.run();
    assert_eq!(query(&h), "#");
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text("SIG");
    // The field takes the keyboard back from the row clicked: a few frames of repaint.
    h.run_steps(8);
    assert_eq!(query(&h), "#SIG");
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().shell().highlight().net.as_deref(), Some("SIG"));
}

#[test]
fn empty_it_shows_recent_items_then_the_documents() {
    let work = TempDir::new("palette-ui-work");
    let mut h = demo(work.path());
    choose(&mut h, "#SIG");
    choose(&mut h, "@R2");
    h.key_press_modifiers(Modifiers::COMMAND, Key::P);
    h.run();
    let rows = row_labels(&h, &["R2", "SIG", "Sheet 1", "Demo"]);
    let starts = |i: usize, t: &str| rows.get(i).is_some_and(|l| l.starts_with(t));
    assert!(starts(0, "R2") && starts(1, "SIG"), "{rows:?}");
    assert!(rows[0].contains("recently used"), "{rows:?}");
    assert!(starts(2, "Sheet 1") && starts(3, "Demo"), "{rows:?}");
}
