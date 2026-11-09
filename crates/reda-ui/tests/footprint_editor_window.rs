//! The Footprint editor's depth in the window, driven through AccessKit with
//! `egui_kittest` (library.md#footprint-editor): the Pad model's fields with ratios typed
//! as lengths (and the same rows in Free Pad Properties), `height`, `texts` editing,
//! Paste Array, the Active Bar and Silkscreen Preparation.

use std::path::Path;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{Footprint, LibraryElement, Uuid};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const MINE: &str = "000000aa-0000-4000-8000-000000000000";

fn shell(home: &Path) -> Shell {
    let lib = home.join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{MINE}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&home.join("home"))))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "app.preferences.set",
            json!({"key": "libraries.paths", "value": [lib]}),
        )
        .unwrap();
    shell
}

fn new_footprint(shell: &mut Shell, name: &str) -> Uuid {
    let out = shell
        .run(
            "library.element.new",
            json!({"library": MINE, "kind": "footprint", "name": name}),
        )
        .unwrap();
    out["element"].as_str().unwrap().parse().unwrap()
}

fn harness(shell: Shell) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([1600.0, 1400.0])
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn footprint(h: &Harness<'_, App>, uuid: Uuid) -> Footprint {
    match h.state().shell().engine().element(uuid) {
        Some(LibraryElement::Footprint(f)) => f.clone(),
        other => panic!("no open Footprint: {other:?}"),
    }
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    let out = h.state_mut().shell_mut().run(id, params).unwrap();
    h.run();
    out
}

/// Clicks the last node labelled `label` (menus and dialogs are drawn last).
fn click(h: &mut Harness<'_, App>, label: &str) {
    h.get_all_by_label(label).last().unwrap().click();
    h.run_steps(8);
}

fn type_into(h: &mut Harness<'_, App>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label)
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
}

fn pick(h: &mut Harness<'_, App>, combo: &str, option: &str) {
    h.get_all_by_role_and_label(Role::ComboBox, combo)
        .last()
        .unwrap()
        .scroll_to_me();
    h.run();
    h.get_all_by_role_and_label(Role::ComboBox, combo)
        .last()
        .unwrap()
        .click();
    h.run();
    click(h, option);
}

fn last_entry(h: &Harness<'_, App>) -> (String, Value) {
    let entry = (h.state().shell().engine().journal().iter())
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

fn add_pad(h: &mut Harness<'_, App>, fp: Uuid, number: &str, x: i64) -> Uuid {
    run(
        h,
        "library.footprint.pad.add",
        json!({"footprint": fp, "number": number, "kind": "smd", "shape": "rounded_rect",
               "at": {"x": x, "y": 0}, "width": "2mm", "height": "1mm"}),
    )["result"]["pad"]
        .as_str()
        .or_else(|| None)
        .map(|s| s.parse().unwrap())
        .unwrap_or_else(|| {
            footprint(h, fp)
                .pads
                .iter()
                .find(|p| p.number == number)
                .unwrap()
                .uuid
        })
}

#[test]
fn a_selected_pad_edits_every_pad_field_with_ratios_typed_as_lengths() {
    let home = TempDir::new("fp-window-pad");
    let mut shell = shell(home.path());
    let fp = new_footprint(&mut shell, "Chip");
    let mut h = harness(shell);
    let pad = add_pad(&mut h, fp, "1", 0);
    run(&mut h, "selection.set", json!({"objects": [pad]}));

    // Half the shorter side is 0.5 mm, so a 0.1 mm radius is a 20% ratio.
    type_into(&mut h, "Corner ratio", "0.1mm");
    let (id, params) = last_entry(&h);
    assert_eq!(id, "library.footprint.pad.edit");
    assert_eq!(params["corner_ratio"], 20_000);
    assert_eq!(
        h.get_by_role_and_label(Role::TextInput, "Corner ratio")
            .value()
            .as_deref(),
        Some("20%")
    );

    h.get_by_role_and_label(Role::CheckBox, "Top left").click();
    h.run();
    type_into(&mut h, "Offset X", "0.2");
    pick(&mut h, "Side", "Bottom");
    pick(&mut h, "Mask bottom", "Tent");
    pick(&mut h, "Paste", "Area change");
    type_into(&mut h, "Paste area change", "-20");
    pick(&mut h, "Zone connection", "Solid");
    let p = footprint(&h, fp).pads[0].clone();
    assert_eq!(p.corner_ratio.unwrap().0, 20_000);
    assert!(
        !p.corners
            .unwrap()
            .contains(reda_ui::engine::model::Corner::TopLeft)
    );
    assert_eq!(p.offset.unwrap().x.0, 200_000);
    assert_eq!(p.side, Some(reda_ui::engine::model::Side::Bottom));
    assert_eq!(
        serde_json::to_value(p.mask).unwrap(),
        json!({"bottom": "tent"})
    );
    assert_eq!(
        serde_json::to_value(&p.paste).unwrap(),
        json!({"percent": -20})
    );
    assert_eq!(
        serde_json::to_value(p.zone_connection).unwrap(),
        json!({"style": "solid"})
    );

    // A through-hole Pad shows its hole's rotation and tolerance.
    pick(&mut h, "Kind", "Plated through-hole");
    type_into(&mut h, "Hole height", "0.5");
    type_into(&mut h, "Hole rotation", "90");
    type_into(&mut h, "Hole tolerance +", "0.05");
    let hole = footprint(&h, fp).pads[0].hole.unwrap();
    assert_eq!((hole.width.0, hole.height.0), (800_000, 500_000));
    assert_eq!(hole.rotation.0, 90_000);
    assert_eq!(hole.tolerance.unwrap().plus.0, 50_000);
    pick(&mut h, "Mask from hole edge", "On");
    assert_eq!(
        footprint(&h, fp).pads[0].mask.unwrap().from_hole_edge,
        Some(true)
    );
}

#[test]
fn free_pad_properties_hold_the_same_pad_fields() {
    let (home, work) = (
        TempDir::new("free-pad-props"),
        TempDir::new("free-pad-work"),
    );
    let mut shell = shell(home.path());
    shell.new_project(work.path(), "Demo").unwrap();
    shell
        .run(
            "document.open",
            json!({"document": "00000000-0000-4000-8000-000000000003"}),
        )
        .unwrap();
    let mut h = Harness::builder()
        .with_size([1400.0, 2600.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    let pad = run(
        &mut h,
        "pcb.free_pad.add",
        json!({"kind": "smd", "shape": "rounded_rect", "at": {"x": 0, "y": 0},
               "width": "2mm", "height": "1mm"}),
    )["result"]["uuid"]
        .clone();
    run(&mut h, "selection.set", json!({"objects": [pad]}));
    h.get_by_label("Free Pad");
    // It starts at the 50% default; 0.1 mm is 20% of half the 1 mm side.
    type_into(&mut h, "Corner ratio", "0.1mm");
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.free_pad.edit");
    assert_eq!(params["pad"], pad);
    assert_eq!(params["corner_ratio"], 20_000);
    pick(&mut h, "Mask top", "Expansion");
    type_into(&mut h, "Mask expansion Top", "0.1");
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.free_pad.edit");
    assert_eq!(params["mask"], json!({"top": {"expansion": 100_000}}));
    pick(&mut h, "Side", "Bottom");
    let (_, params) = last_entry(&h);
    assert_eq!(params["side"], "bottom");
    // A Free Pad has no number row.
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Number")
            .is_none()
    );
}
