//! The Properties panel's board-object regions in the window, driven through AccessKit
//! with `egui_kittest` (docs/spec/v0.1/pcb.md): free text's style (font, justification,
//! mirroring, knockout and frame), a Dimension's rows, a placed Pad's rows and a placed
//! Footprint's graphics behind their lock, each edit with the Locally Edited mark.

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const PCB: &str = "00000000-0000-4000-8000-000000000003";

fn harness(home: &TempDir, work: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    shell
        .run("document.open", json!({"document": PCB}))
        .unwrap();
    Harness::builder()
        .with_size([1400.0, 2600.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn dirs(name: &str) -> (TempDir, TempDir) {
    (
        TempDir::new(&format!("{name}-home")),
        TempDir::new(&format!("{name}-work")),
    )
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()["result"].clone()
}

fn last_entry(h: &Harness<'_, App>) -> (String, Value) {
    let entry = (h.state().shell().engine().journal().iter())
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

fn select(h: &mut Harness<'_, App>, params: Value) {
    run(h, "selection.set", params);
    h.run();
}

/// Types `text` into the field labelled `label` and presses Enter.
fn type_into(h: &mut Harness<'_, App>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .set_text_value_accesskit(text);
    h.run();
    h.key_press(Key::Enter);
    h.run();
}

/// Opens the drop-down labelled `label` and picks `option`.
fn pick(h: &mut Harness<'_, App>, label: &str, option: &str) {
    h.get_by_role_and_label(Role::ComboBox, label).click();
    h.run();
    h.get_all_by_label(option).last().unwrap().click();
    h.run();
}

fn pcb_text(h: &Harness<'_, App>) -> reda_ui::engine::model::PcbText {
    let pcb = h.state().shell().engine().project().unwrap().pcb().unwrap();
    pcb.texts().next().unwrap().clone()
}

#[test]
fn free_text_style_sets_justification_knockout_and_frame() {
    use reda_ui::engine::model::geom::Nm;
    use reda_ui::engine::model::{TextFrame, TextHJustify};
    let (home, work) = dirs("props-text");
    let mut h = harness(&home, &work);
    let text = run(
        &mut h,
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "REDA", "x": 0, "y": 0}),
    )["uuid"]
        .clone();
    select(&mut h, json!({"objects": [text]}));
    h.get_by_label("Text Style");

    pick(&mut h, "Horizontal", "Right");
    assert_eq!(pcb_text(&h).style.justify.h, TextHJustify::Right);

    h.get_by_label("Knockout").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.text.set");
    assert_eq!(params["knockout"]["margin"], json!(200_000));
    type_into(&mut h, "Knockout margin", "0.5");
    assert_eq!(
        pcb_text(&h).style.knockout.map(|k| k.margin),
        Some(Nm::from_um(500))
    );
    h.get_by_label("Knockout").click();
    h.run();
    assert_eq!(pcb_text(&h).style.knockout, None, "clear_knockout");

    h.get_by_label("Frame").click();
    h.run();
    type_into(&mut h, "Frame width", "3");
    assert_eq!(
        pcb_text(&h).frame,
        Some(TextFrame {
            width: Nm::from_mm(3),
            height: Nm::from_mm(5)
        })
    );
    h.get_by_label("Frame").click();
    h.run();
    assert_eq!(pcb_text(&h).frame, None, "clear_frame");
}

#[test]
fn text_on_a_bottom_layer_can_be_mirrored_and_lists_its_fonts() {
    let (home, work) = dirs("props-text-mirror");
    let mut h = harness(&home, &work);
    let text = run(
        &mut h,
        "pcb.text.add",
        json!({"layer": "silkscreen_bottom", "text": "REDA", "x": 0, "y": 0}),
    )["uuid"]
        .clone();
    select(&mut h, json!({"objects": [text]}));
    let mirrored = pcb_text(&h).mirrored;
    h.get_by_label("Mirrored").click();
    h.run();
    assert_eq!(pcb_text(&h).mirrored, !mirrored);
    // The font row lists the families this machine has, the text's own first.
    let font = pcb_text(&h).style.font;
    h.get_by_role_and_label(Role::ComboBox, "Font").click();
    h.run();
    assert!(!h.get_all_by_label(&font).collect::<Vec<_>>().is_empty());
}

fn dimension(h: &mut Harness<'_, App>) -> Value {
    run(
        h,
        "pcb.dimension.add",
        json!({"layer": "silkscreen_top", "a": {"point": {"x": 0, "y": 0}},
               "b": {"point": {"x": "10mm", "y": 0}}, "offset": "2mm"}),
    )["uuid"]
        .clone()
}

fn the_dimension(h: &Harness<'_, App>) -> reda_ui::engine::model::Dimension {
    let pcb = h.state().shell().engine().project().unwrap().pcb().unwrap();
    pcb.dimensions().next().unwrap().clone()
}

#[test]
fn a_dimension_has_its_rows_in_properties() {
    use reda_ui::engine::model::geom::Nm;
    use reda_ui::engine::model::{ArrowPosition, DimensionFormat, DimensionUnits};
    let (home, work) = dirs("props-dimension");
    let mut h = harness(&home, &work);
    let d = dimension(&mut h);
    select(&mut h, json!({"objects": [d]}));
    h.get_by_label("Dimension");
    assert!(h.query_by_label_contains("Value: 10").is_some());

    pick(&mut h, "Units", "mil");
    assert_eq!(the_dimension(&h).units, DimensionUnits::Mil);
    pick(&mut h, "Format", "12.70 (mm)");
    assert_eq!(
        the_dimension(&h).format,
        DimensionFormat::ValueBracketedUnit
    );
    pick(&mut h, "Arrows", "Outside");
    assert_eq!(the_dimension(&h).arrow_position, ArrowPosition::Outside);
    type_into(&mut h, "Prefix", "L = ");
    assert_eq!(the_dimension(&h).prefix, "L = ");
    type_into(&mut h, "Offset", "! + 1mm");
    assert_eq!(the_dimension(&h).offset, Nm::from_mm(3));
    pick(&mut h, "Text position", "Unidirectional");
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.dimension.set");
    assert_eq!(params["text_position"], "unidirectional");
}

/// A Footprint with two Pads (one with a hole) and a silkscreen line.
fn footprint(h: &mut Harness<'_, App>) -> (Value, Value) {
    let out = run(
        h,
        "test.footprint.add",
        json!({"at": {"x": "20mm", "y": "20mm"}, "designator": "U1",
               "graphics": [{"layer": "silkscreen_top", "width": 150_000,
                   "shape": {"kind": "line", "start": {"x": -2_000_000, "y": 2_000_000},
                             "end": {"x": 2_000_000, "y": 2_000_000}}}],
               "pads": [{"at": {"x": "-1mm", "y": 0}, "width": "1mm", "height": "1mm"},
                        {"at": {"x": "1mm", "y": 0}, "width": "1.5mm", "height": "1.5mm",
                         "kind": "plated_through_hole", "hole": {"width": 800_000, "height": 800_000}}]}),
    );
    let footprint = out["footprint"].clone();
    let pcb = h.state().shell().engine().project().unwrap().pcb().unwrap();
    let f = pcb
        .footprint(footprint.as_str().unwrap().parse().unwrap())
        .unwrap();
    let pad = pcb.footprint_copy(f.footprint).unwrap().footprint.pads[1].uuid;
    (footprint, json!(pad))
}

fn copy(h: &Harness<'_, App>, instance: &Value) -> reda_ui::engine::model::Footprint {
    let pcb = h.state().shell().engine().project().unwrap().pcb().unwrap();
    let f = pcb
        .footprint(instance.as_str().unwrap().parse().unwrap())
        .unwrap();
    pcb.footprint_copy(f.footprint).unwrap().footprint.clone()
}

#[test]
fn a_placed_pad_is_edited_from_properties_and_marked_locally_edited() {
    use reda_ui::engine::model::PadShape;
    use reda_ui::engine::model::geom::Nm;
    let (home, work) = dirs("props-pad");
    let mut h = harness(&home, &work);
    let (instance, pad) = footprint(&mut h);
    select(
        &mut h,
        json!({"pads": [{"footprint": instance, "pad": pad}]}),
    );
    h.get_by_label("Pad");
    assert!(h.query_by_label("Locally edited").is_none());
    type_into(&mut h, "Width", "2");
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.pad.edit");
    assert_eq!(params["pad"], pad);
    assert_eq!(copy(&h, &instance).pads[1].width, Nm::from_mm(2));
    h.get_by_label("Locally edited");
    pick(&mut h, "Shape", "Oval");
    assert_eq!(copy(&h, &instance).pads[1].shape, PadShape::Oval);
    type_into(&mut h, "Hole", "0.9");
    assert_eq!(
        copy(&h, &instance).pads[1].hole.map(|h| h.width),
        Some(Nm::from_um(900))
    );
    // The number stays: Nets come from the schematic.
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Number")
            .is_none()
    );
}

#[test]
fn footprint_graphics_unlock_from_a_lock_icon_then_edit_as_local_edits() {
    use reda_ui::engine::model::FootprintLayer;
    use reda_ui::engine::model::geom::Nm;
    let (home, work) = dirs("props-graphics");
    let mut h = harness(&home, &work);
    let (instance, _) = footprint(&mut h);
    select(&mut h, json!({"objects": [instance]}));
    h.get_by_label("Footprint Graphics");
    assert!(h.query_by_label("Graphics locked").is_some());
    assert!(
        h.query_by_role_and_label(Role::ComboBox, "Layer").is_none(),
        "locked graphics show no rows"
    );

    h.get_by_label("Lock Footprint graphics").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.footprint.unlock_graphics");
    assert_eq!(params, json!({"instances": [instance], "unlocked": true}));

    type_into(&mut h, "Width", "0.3");
    assert_eq!(copy(&h, &instance).graphics[0].width, Nm::from_um(300));
    pick(&mut h, "Layer", "Assembly top");
    assert_eq!(
        copy(&h, &instance).graphics[0].layer,
        FootprintLayer::AssemblyTop
    );
    h.get_by_label("Locally edited");
    h.get_by_label("Delete").click();
    h.run();
    assert!(copy(&h, &instance).graphics.is_empty());
}

#[test]
fn with_protect_locked_primitives_off_locked_graphics_are_edited_anyway() {
    use reda_ui::engine::model::geom::Nm;
    let (home, work) = dirs("props-graphics-unprotected");
    let mut h = harness(&home, &work);
    run(
        &mut h,
        "app.preferences.set",
        json!({"key": "editing.protect_locked_primitives", "value": false}),
    );
    let (instance, _) = footprint(&mut h);
    select(&mut h, json!({"objects": [instance]}));
    type_into(&mut h, "Width", "0.25");
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.footprint.graphic.edit");
    assert_eq!(params["include_locked"], true);
    assert_eq!(copy(&h, &instance).graphics[0].width, Nm::from_um(250));
}
