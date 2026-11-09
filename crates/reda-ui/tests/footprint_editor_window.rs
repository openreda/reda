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

#[test]
fn height_and_texts_are_edited_in_the_inspector() {
    let home = TempDir::new("fp-window-texts");
    let mut shell = shell(home.path());
    let fp = new_footprint(&mut shell, "Chip");
    let mut h = harness(shell);

    type_into(&mut h, "Part height", "1.2");
    assert_eq!(footprint(&h, fp).height.map(|n| n.0), Some(1_200_000));
    let (id, _) = last_entry(&h);
    assert_eq!(id, "library.footprint.edit");

    // A second Designator, on the Assembly Layer: Silkscreen and Assembly both hold one.
    let before = footprint(&h, fp).texts.len();
    h.get_by_role_and_label(Role::ComboBox, "Layer").click();
    h.run();
    click(&mut h, "Assembly top");
    type_into(&mut h, "New text field", "Designator");
    click(&mut h, "Add Text");
    let (id, _) = last_entry(&h);
    assert_eq!(id, "library.footprint.text.add");
    let texts = footprint(&h, fp).texts;
    assert_eq!(texts.len(), before + 1);
    let added = texts.last().unwrap().clone();
    assert_eq!(added.field, "Designator");
    assert_eq!(
        added.layer,
        reda_ui::engine::model::FootprintLayer::AssemblyTop
    );

    // Selecting it opens its rows: every style property but the frame.
    click(&mut h, "Designator · Assembly top");
    type_into(&mut h, "Field", "MPN");
    type_into(&mut h, "X", "1.5");
    type_into(&mut h, "Pen width", "0.2");
    pick(&mut h, "Horizontal", "Left");
    type_into(&mut h, "Knockout margin", "0.3");
    let t = footprint(&h, fp)
        .texts
        .into_iter()
        .find(|t| t.uuid == added.uuid)
        .unwrap();
    assert_eq!(t.field, "MPN");
    assert_eq!(t.at.x.0, 1_500_000);
    assert_eq!(t.style.thickness.0, 200_000);
    assert_eq!(
        t.style.justify.h,
        reda_ui::engine::model::TextHJustify::Left
    );
    assert_eq!(t.style.knockout.unwrap().margin.0, 300_000);

    click(&mut h, "Remove MPN · Assembly top");
    assert!(footprint(&h, fp).texts.iter().all(|t| t.uuid != added.uuid));
    let (id, _) = last_entry(&h);
    assert_eq!(id, "library.footprint.text.remove");
}

#[test]
fn paste_array_repeats_the_selected_pads_as_one_undo_step() {
    let home = TempDir::new("fp-window-array");
    let mut shell = shell(home.path());
    let fp = new_footprint(&mut shell, "Header");
    let mut h = harness(shell);
    let pad = add_pad(&mut h, fp, "01", 0);
    run(&mut h, "selection.set", json!({"objects": [pad]}));
    click(&mut h, "Edit");
    click(&mut h, "Paste Array…");
    h.get_by_label("Paste Array");
    type_into(&mut h, "Count", "3");
    type_into(&mut h, "Number step", "2");
    type_into(&mut h, "Step X", "2.54mm");
    type_into(&mut h, "Step Y", "0");
    h.get_by_role_and_label(Role::CheckBox, "Remove leading zeroes")
        .click();
    h.run();
    h.get_all_by_role_and_label(Role::Button, "Paste")
        .last()
        .unwrap()
        .click();
    h.run_steps(8);
    let (id, params) = last_entry(&h);
    assert_eq!(id, "batch");
    assert_eq!(params["commands"].as_array().unwrap().len(), 3);
    let f = footprint(&h, fp);
    let placed: Vec<(String, i64)> = f
        .pads
        .iter()
        .map(|p| (p.number.clone(), p.at.x.0))
        .collect();
    assert_eq!(
        placed,
        [
            ("01".into(), 0),
            ("3".into(), 2_540_000),
            ("5".into(), 5_080_000),
            ("7".into(), 7_620_000)
        ]
    );
    // The copies keep every field of the original.
    assert!(
        f.pads
            .iter()
            .all(|p| p.shape == f.pads[0].shape && p.width == f.pads[0].width)
    );
    run(&mut h, "edit.undo", json!({"document": fp}));
    assert_eq!(footprint(&h, fp).pads.len(), 1, "one undo step");
}

#[test]
fn silkscreen_preparation_runs_from_the_footprint_editors_tools_menu() {
    let home = TempDir::new("fp-window-silk");
    let mut shell = shell(home.path());
    let fp = new_footprint(&mut shell, "Chip");
    let mut h = harness(shell);
    add_pad(&mut h, fp, "1", 0);
    // A silkscreen line straight across the Pad.
    run(
        &mut h,
        "library.footprint.graphic.add",
        json!({"footprint": fp, "layer": "silkscreen_top",
               "shape": {"kind": "line", "start": {"x": "-3mm", "y": 0}, "end": {"x": "3mm", "y": 0}}}),
    );
    click(&mut h, "Tools");
    click(&mut h, "Silkscreen Preparation");
    click(&mut h, "Run");
    let (id, params) = last_entry(&h);
    assert_eq!(id, "library.footprint.silkscreen.prepare");
    assert_eq!(params["footprint"], json!(fp));
    // The line is clipped clear of the Pad's opening: two pieces now.
    let lines = footprint(&h, fp)
        .graphics
        .iter()
        .filter(|g| g.layer == reda_ui::engine::model::FootprintLayer::SilkscreenTop)
        .count();
    assert_eq!(lines, 2);
}

/// The Active Bar's buttons, by their accessible names, in order.
fn active_bar(kind: reda_ui::bars::DocumentType) -> Vec<String> {
    reda_ui::bars::for_document(Some(kind))
        .active_bar
        .iter()
        .filter_map(|i| match i {
            reda_ui::bars::Item::Action(a) => Some(a.id.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_library_editors_active_bars_hold_the_tools_library_md_lists() {
    use reda_ui::bars::DocumentType;
    assert_eq!(
        active_bar(DocumentType::Footprint),
        [
            "active_bar.select",
            "active_bar.smd_pad",
            "active_bar.th_pad",
            "active_bar.line",
            "active_bar.arc",
            "active_bar.polygon",
            "active_bar.courtyard",
            "active_bar.text",
            "active_bar.origin",
            "active_bar.measure"
        ]
    );
    assert_eq!(
        active_bar(DocumentType::Symbol),
        [
            "active_bar.select",
            "active_bar.pin",
            "active_bar.line",
            "active_bar.rect",
            "active_bar.arc",
            "active_bar.polygon",
            "active_bar.text",
            "active_bar.unit_previous",
            "active_bar.unit_next"
        ]
    );
}

/// Feeds pointer `events` to the Footprint canvas and runs what they ask for.
fn feed(h: &mut Harness<'_, App>, fp: Uuid, events: &[reda_ui::canvas::interact::Pointer]) {
    let shell = h.state_mut().shell_mut();
    for (id, params) in reda_ui::footprint_editor::pointer(shell, fp, events, 10_000.0) {
        shell.run(&id, params).unwrap();
    }
    h.run();
}

fn at(x: f64, y: f64) -> reda_ui::canvas::interact::Pointer {
    reda_ui::canvas::interact::Pointer::Click {
        at: (x * 1e6, y * 1e6),
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn the_footprint_active_bar_tools_place_pads_draw_and_measure() {
    use reda_ui::engine::model::{FootprintLayer, PadKind, Shape};
    let home = TempDir::new("fp-window-bar");
    let mut shell = shell(home.path());
    let fp = new_footprint(&mut shell, "Chip");
    let mut h = harness(shell);

    // Through-hole Pad sets the Pad Tool's kind; SMD Pad sets it back.
    h.get_by_label("Through-Hole Pad").click();
    h.run();
    feed(&mut h, fp, &[at(0.0, 0.0)]);
    h.get_by_label("SMD Pad").click();
    h.run();
    feed(&mut h, fp, &[at(5.0, 0.0)]);
    let kinds: Vec<PadKind> = footprint(&h, fp).pads.iter().map(|p| p.kind).collect();
    assert_eq!(kinds, [PadKind::PlatedThroughHole, PadKind::Smd]);

    // Courtyard draws a closed polygon on the courtyard: Enter closes it.
    h.get_by_label("Courtyard").click();
    h.run();
    feed(
        &mut h,
        fp,
        &[at(-3.0, -2.0), at(8.0, -2.0), at(8.0, 2.0), at(-3.0, 2.0)],
    );
    run(&mut h, "tool.finish", json!({}));
    let court = footprint(&h, fp)
        .graphics
        .into_iter()
        .find(|g| g.layer == FootprintLayer::CourtyardTop)
        .expect("a courtyard polygon");
    assert!(matches!(&court.shape, Shape::Polygon { points } if points.len() == 4));

    // Arc: centre, start, end, on the Silkscreen (a courtyard takes only closed shapes).
    pick(&mut h, "Layer", "Silkscreen top");
    h.get_by_label("Arc").click();
    h.run();
    feed(&mut h, fp, &[at(0.0, 5.0), at(1.0, 5.0), at(0.0, 6.0)]);
    assert!(
        footprint(&h, fp)
            .graphics
            .iter()
            .any(|g| matches!(g.shape, Shape::Arc { .. }))
    );

    // Text places the field the inspector names on the Silkscreen.
    type_into(&mut h, "New text field", "Value");
    h.get_by_label("Text").click();
    h.run();
    let before = footprint(&h, fp).texts.len();
    feed(&mut h, fp, &[at(0.0, -4.0)]);
    let texts = footprint(&h, fp).texts;
    assert_eq!(texts.len(), before + 1);
    assert_eq!(texts.last().unwrap().field, "Value");
    assert_eq!(texts.last().unwrap().layer, FootprintLayer::SilkscreenTop);

    // Measure: two clicks give the distance in the status bar.
    h.get_by_label("Measure").click();
    h.run();
    feed(&mut h, fp, &[at(0.0, 0.0), at(3.0, 4.0)]);
    assert_eq!(
        h.state().shell().note(),
        Some("Distance 5 mm (dx 3 mm, dy 4 mm)")
    );
}

#[test]
fn the_symbol_active_bar_switches_units() {
    let home = TempDir::new("sym-window-units");
    let mut shell = shell(home.path());
    let out = shell
        .run(
            "library.element.new",
            json!({"library": MINE, "kind": "symbol", "name": "Quad"}),
        )
        .unwrap();
    let symbol: Uuid = out["element"].as_str().unwrap().parse().unwrap();
    shell
        .run("library.symbol.unit.add", json!({"symbol": symbol}))
        .unwrap();
    let mut h = harness(shell);
    let units = match h.state().shell().engine().element(symbol) {
        Some(LibraryElement::Symbol(s)) => s.units.iter().map(|u| u.uuid).collect::<Vec<_>>(),
        _ => unreachable!(),
    };
    let shown =
        |h: &Harness<'_, App>| reda_ui::symbol_editor::shown_unit(h.state().shell(), symbol);
    assert_eq!(shown(&h), Some(units[0]));
    h.get_by_label("Next Unit").click();
    h.run();
    assert_eq!(shown(&h), Some(units[1]));
    h.get_by_label("Next Unit").click();
    h.run();
    assert_eq!(shown(&h), Some(units[0]), "it wraps round");
    h.get_by_label("Previous Unit").click();
    h.run();
    assert_eq!(shown(&h), Some(units[1]));
}
