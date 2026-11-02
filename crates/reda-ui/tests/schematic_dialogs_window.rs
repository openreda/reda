//! The schematic's dialogs and graphics Tools in the window, driven with `egui_kittest`:
//! Mod+F opens Find, which goes to the match; the Text Tool's click asks for the words;
//! Design ▸ Re-annotate… previews and applies.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const MIL: f64 = 25_400.0;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn harness(dir: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    for x in [0, 1000, -1000] {
        shell
            .run(
                "schematic.component.place",
                json!({"document": SHEET, "part": R_10K,
                       "at": {"x": format!("{x}mil"), "y": 0}}),
            )
            .unwrap();
    }
    Harness::builder()
        .with_size([1200.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn screen(h: &Harness<'_, App>, x: f64, y: f64) -> Pos2 {
    let rect = h.get_by_role(Role::Canvas).rect();
    let camera = h
        .state()
        .shell()
        .document_tab(sheet())
        .unwrap()
        .camera
        .unwrap();
    camera.screen_point(rect, (x * MIL, y * MIL))
}

fn click(h: &mut Harness<'_, App>, x: f64, y: f64) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.run();
    h.drag_at(at);
    h.run();
    h.drop_at(at);
    h.run();
}

fn designators(h: &Harness<'_, App>) -> Vec<String> {
    let project = h.state().shell().engine().project().unwrap();
    let mut out: Vec<(i64, String)> = project
        .schematic_sheet(sheet())
        .unwrap()
        .units()
        .map(|u| (u.at.x.0, u.designator.clone()))
        .collect();
    out.sort();
    out.into_iter().map(|(_, d)| d).collect()
}

#[test]
fn mod_f_opens_find_which_selects_and_zooms_to_the_match() {
    let dir = TempDir::new("find-window");
    let mut h = harness(&dir);
    h.run();
    click(&mut h, 3000.0, 3000.0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::F);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Find what")
        .type_text("R3");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    h.run();
    let shell = h.state().shell();
    let ids: Vec<&str> = shell
        .engine()
        .journal()
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(shell.selection().objects.len(), 1, "{ids:?}");
    let project = shell.engine().project().unwrap();
    let unit = project
        .schematic_sheet(sheet())
        .unwrap()
        .unit(shell.selection().objects[0])
        .unwrap();
    assert_eq!(unit.designator, "R3");
    h.get_by_label("R3 (Designator) on Sheet 1");
}

#[test]
fn the_text_tool_asks_for_the_words_and_places_them() {
    let dir = TempDir::new("text-window");
    let mut h = harness(&dir);
    h.run();
    click(&mut h, 3000.0, 3000.0);
    h.key_press(Key::T);
    h.run();
    assert_eq!(h.state().shell().tool(), Tool::Text);
    click(&mut h, 2000.0, 2000.0);
    h.get_by_role_and_label(Role::TextInput, "Text")
        .type_text("Power stage");
    h.run();
    h.get_by_label("Place Text").click();
    h.run();
    let project = h.state().shell().engine().project().unwrap();
    let texts: Vec<String> = project
        .schematic_sheet(sheet())
        .unwrap()
        .texts()
        .map(|t| t.text.clone())
        .collect();
    assert_eq!(texts, ["Power stage"]);
}

#[test]
fn design_reannotate_previews_and_applies() {
    let dir = TempDir::new("reannotate-window");
    let mut h = harness(&dir);
    h.run();
    assert_eq!(designators(&h), ["R3", "R1", "R2"]);
    h.get_by_role_and_label(Role::Button, "Design").click();
    h.run();
    h.get_by_label("Re-annotate…").click();
    h.run();
    h.get_by_label("R3 » R1");
    assert_eq!(designators(&h), ["R3", "R1", "R2"]);
    h.get_by_role_and_label(Role::Button, "Re-annotate").click();
    h.run();
    assert_eq!(designators(&h), ["R1", "R2", "R3"]);
    assert!(h.query_by_label("R3 » R1").is_none());
}
