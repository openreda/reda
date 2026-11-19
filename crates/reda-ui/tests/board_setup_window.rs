//! Board Setup's later pages and actions in the window, driven through AccessKit with
//! `egui_kittest`: Mask & Paste, Zone Connections and Stackup, `Mod+Z` over pending
//! edits, Show on board with its banner, and the Fab Preset actions.

use std::path::Path;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{
    ConnectionStyle, ConnectionTarget, PcbDocument, RuleKind, ZoneNameScheme,
};
use reda_ui::layout::{SettingsWindow, TabKey};
use reda_ui::shell::Shell;
use serde_json::json;

fn harness(home: &Path, work: &Path, name: &str) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .commands(reda_testkit::probe_commands)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, name).unwrap();
    let mut h = Harness::builder()
        .with_size([1200.0, 820.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn open_board_setup(h: &mut Harness<'static, App>, _project: &str) {
    h.state_mut().open_board_setup();
    h.run();
}

fn page(h: &mut Harness<'static, App>, title: &str) {
    h.get_by_role_and_label(Role::TreeItem, title)
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::TreeItem, title).click();
    h.run();
}

fn pcb(h: &Harness<'static, App>) -> PcbDocument {
    h.state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .clone()
}

/// Replaces the text of the field announced as `label` with `text`.
fn retype(h: &mut Harness<'static, App>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label)
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
}

fn value(h: &Harness<'static, App>, label: &str) -> String {
    h.get_by_role_and_label(Role::TextInput, label)
        .value()
        .unwrap_or_default()
}

/// Picks `option` from the drop-down announced as `label`.
fn choose(h: &mut Harness<'static, App>, label: &str, option: &str) {
    h.get_by_role_and_label(Role::ComboBox, label)
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::ComboBox, label).click();
    h.run();
    h.get_all_by_label(option).last().unwrap().click();
    h.run();
}

fn pending(h: &Harness<'static, App>) -> usize {
    h.state()
        .settings_windows()
        .dialog(SettingsWindow::BoardSetup)
        .map_or(0, |d| d.steps().len())
}

#[test]
fn mask_paste_zone_connections_and_stackup_pages_edit_and_ok_commits_them() {
    let (home, work) = (TempDir::new("bsw-home"), TempDir::new("bsw-work"));
    let mut h = harness(home.path(), work.path(), "Win");
    open_board_setup(&mut h, "Win");

    page(&mut h, "Mask & Paste");
    h.get_by_label("Link top and bottom");
    retype(&mut h, "Top mask expansion (mm)", "0.08");
    assert_eq!(value(&h, "Bottom mask expansion (mm)"), "0.08", "linked");
    h.get_by_role_and_label(Role::RadioButton, "By area (%)")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::RadioButton, "By area (%)")
        .click();
    h.run();
    retype(&mut h, "Paste area change (%)", "-10");

    page(&mut h, "Zone Connections");
    choose(&mut h, "SMD Pads: Style", "Solid");
    choose(&mut h, "Zone naming scheme", "LXX_NET_PXXX");

    page(&mut h, "Stackup");
    h.get_by_label("2 copper Layers · 1.6 mm ordered");
    h.get_by_role_and_label(Role::Button, "Open Stackup editor");
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Top Copper thickness")
            .is_none()
    );
    h.get_by_role_and_label(Role::MultilineTextInput, "Fab Notes text")
        .click();
    h.run();
    h.get_by_role_and_label(Role::MultilineTextInput, "Fab Notes text")
        .type_text("No panel");
    h.run();

    h.get_by_label("OK").click();
    h.run();
    assert!(h.query_by_label("Board Setup").is_none(), "OK closes");
    let board = pcb(&h);
    let mask = &board.board_rule(RuleKind::MaskExpansion).values;
    assert_eq!(
        (mask.top.unwrap().0, mask.bottom.unwrap().0),
        (80_000, 80_000)
    );
    let paste = &board.board_rule(RuleKind::PasteExpansion).values;
    assert_eq!((paste.expansion, paste.percent), (None, Some(-10)));
    let zones = &board.board_rule(RuleKind::ZoneConnection).values;
    assert_eq!(
        zones.connection(ConnectionTarget::Smd).unwrap().style,
        Some(ConnectionStyle::Solid)
    );
    assert_eq!(board.setup().zone_name_scheme, ZoneNameScheme::LxxNetPxxx);
    assert_eq!(board.setup().fab_notes.as_deref(), Some("No panel"));
}

#[test]
fn mod_z_inside_board_setup_steps_back_through_pending_edits_first() {
    let (home, work) = (TempDir::new("bsw-home"), TempDir::new("bsw-work"));
    let mut h = harness(home.path(), work.path(), "Win");
    open_board_setup(&mut h, "Win");
    page(&mut h, "DRC");
    h.get_by_label("Courtyard Overlap: Off").scroll_to_me();
    h.run();
    h.get_by_label("Courtyard Overlap: Off").click();
    h.run();
    h.get_by_label("Clearance: Warning").scroll_to_me();
    h.run();
    h.get_by_label("Clearance: Warning").click();
    h.run();
    assert_eq!(pending(&h), 2);
    let journal = h.state().shell().engine().journal().len();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(pending(&h), 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(pending(&h), 0);
    assert_eq!(
        h.state().shell().engine().journal().len(),
        journal,
        "the undo stack was left alone"
    );
}

#[test]
fn show_on_board_raises_the_board_view_only_with_a_banner_and_back_returns() {
    let (home, work) = (TempDir::new("bsw-home"), TempDir::new("bsw-work"));
    let mut h = harness(home.path(), work.path(), "Win");
    let track = {
        let shell = h.state_mut().shell_mut();
        let out = shell
            .run(
                "pcb.track.add",
                json!({"layer": "copper_top", "width": "0.25mm",
                       "points": [{"x": "10mm", "y": "10mm"}, {"x": "15mm", "y": "10mm"}]}),
            )
            .unwrap();
        out["result"]["tracks"][0].as_str().unwrap().to_owned()
    };
    h.run();
    open_board_setup(&mut h, "Win");
    // A pending edit, which Show on board keeps.
    h.get_by_label("Clearance: Warning").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Show board-wide Clearance on board")
        .click();
    h.step();
    let raised = h.output().viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::Focus);
    assert!(raised, "the main window comes forward");
    h.run();
    let pcb_uuid = pcb(&h).uuid;
    assert_eq!(h.state().shell().shown_on_board(), Some(1));
    assert_eq!(
        h.state().shell().layout().focus,
        Some(TabKey::Document(pcb_uuid))
    );
    assert_eq!(
        h.state().shell().highlight().objects,
        [track.parse().unwrap()]
    );
    h.get_by_label("1 object affected (view only)");
    assert!(
        h.query_by_label("Select").is_none(),
        "the banner takes the Active Bar's place"
    );
    // View-only: an edit key does nothing.
    h.state_mut()
        .shell_mut()
        .run("selection.set", json!({"objects": [track]}))
        .unwrap();
    h.run();
    h.key_press(Key::Delete);
    h.run();
    assert_eq!(pcb(&h).tracks().count(), 1, "nothing was deleted");

    h.get_by_label("Back to Board Setup").click();
    h.run();
    assert_eq!(h.state().shell().shown_on_board(), None);
    assert!(h.state().shell().highlight().objects.is_empty());
    assert!(h.query_by_label("Back to Board Setup").is_none());
    assert_eq!(pending(&h), 1, "Board Setup kept its pending edit");
}

#[test]
fn reset_page_previews_row_by_row_and_save_rules_as_fab_preset_adds_a_preset() {
    let (home, work) = (TempDir::new("bsw-home"), TempDir::new("bsw-work"));
    let mut h = harness(home.path(), work.path(), "Win");
    open_board_setup(&mut h, "Win");
    page(&mut h, "Track Width");
    let field = "Board-wide minimum Track width (mm)";
    let seeded = value(&h, field);
    retype(&mut h, field, "0.5");
    // Acting on the applied rules waits until nothing is pending.
    for action in [
        "Save Rules as Fab Preset…",
        "Export Rules…",
        "Import Rules…",
    ] {
        assert!(
            h.get_by_role_and_label(Role::Button, action)
                .accesskit_node()
                .is_disabled()
        );
    }
    h.get_by_label("Reset Page to Fab Preset").click();
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Take Track width");
    h.get_by_label("Use These Rules").click();
    h.run();
    assert_eq!(value(&h, field), seeded);
    assert_eq!(pending(&h), 0);

    h.get_by_label("Save Rules as Fab Preset…").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Fab Preset name")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Fab Preset name")
        .type_text("House");
    h.run();
    h.get_by_label("Save Preset").click();
    h.run();
    h.get_by_label("Apply House Rules…");
    let presets: Vec<String> = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .fab_presets()
        .map(|p| p.name.clone())
        .collect();
    assert!(presets.contains(&"House".to_owned()), "{presets:?}");
}

#[test]
fn board_setup_opens_the_boards_stackup_tab() {
    let (home, work) = (TempDir::new("stack-home"), TempDir::new("stack-work"));
    let mut h = harness(home.path(), work.path(), "Layered");
    open_board_setup(&mut h, "Layered");
    page(&mut h, "Stackup");
    h.get_by_role_and_label(Role::Button, "Open Stackup editor")
        .click();
    h.run();
    let focused = h.state().shell().layout().focus.as_ref().unwrap();
    assert_eq!(focused.to_string(), format!("stackup/{}", pcb(&h).uuid));
    assert_eq!(focused.to_string().parse::<TabKey>().unwrap(), *focused);
    assert_eq!(
        h.state().shell().undo_scope(None),
        Some(reda_ui::engine::model::Scope::Document(pcb(&h).uuid))
    );
}

#[test]
fn stackup_field_commits_copper_thickness_once_and_rejects_an_overfull_board() {
    let (home, work) = (TempDir::new("se-home"), TempDir::new("se-work"));
    let mut h = harness(home.path(), work.path(), "Copper");
    let document = pcb(&h).uuid;
    let sheet = pcb(&h).stackup().dielectrics().next().unwrap().uuid;
    h.state_mut()
        .shell_mut()
        .run(
            "pcb.stackup.set",
            json!({"dielectrics":[{"uuid":sheet, "thickness":null}]}),
        )
        .unwrap();
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document": document}))
        .unwrap();
    h.run();
    let before = h.state().shell().engine().journal().len();
    retype(&mut h, "Top Copper thickness", "2oz");
    assert_eq!(h.state().shell().engine().journal().len(), before);
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        pcb(&h).stackup().copper().next().unwrap().thickness.0,
        70_000
    );
    assert_eq!(h.state().shell().engine().journal().len(), before + 1);
    retype(&mut h, "Finished thickness (mm)", "0.05");
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(pcb(&h).stackup().thickness.0, 1_600_000);
    assert_eq!(value(&h, "Finished thickness (mm)"), "0.05");
    assert!(h.query_all_by_label_contains("blank dielectric").count() > 0);
}

#[test]
fn dielectric_rows_edit_material_measurements_and_keep_one_sheet_in_each_gap() {
    let (home, work) = (TempDir::new("sd-home"), TempDir::new("sd-work"));
    let mut h = harness(home.path(), work.path(), "Dielectrics");
    let document = pcb(&h).uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document": document}))
        .unwrap();
    h.run();
    retype(&mut h, "Dielectric 1 name", "Top prepreg");
    h.key_press(Key::Enter);
    h.run();
    choose(&mut h, "Top prepreg kind", "Prepreg");
    assert!(
        h.get_by_role_and_label(Role::Button, "Remove sheet")
            .accesskit_node()
            .is_disabled()
    );
    for (label, text) in [
        ("Material", "7628"),
        ("Dk", "4.21"),
        ("Df", "0.0123"),
        ("Test frequency (Hz)", "1000000000"),
    ] {
        retype(&mut h, label, text);
        h.key_press(Key::Enter);
        h.run();
    }
    let board = pcb(&h);
    let sheet = board.stackup().dielectrics().next().unwrap();
    assert_eq!(sheet.name.as_deref(), Some("Top prepreg"));
    assert_eq!(sheet.material.as_deref(), Some("7628"));
    assert_eq!(
        (sheet.dk, sheet.df, sheet.test_frequency),
        (Some(4210), Some(12_300), Some(1_000_000_000))
    );
    assert_eq!(sheet.kind, reda_ui::engine::model::DielectricKind::Prepreg);
    h.get_by_role_and_label(Role::Button, "Add sheet").click();
    h.run();
    assert_eq!(pcb(&h).stackup().dielectrics().count(), 2);
    h.get_by_role_and_label(Role::Button, "Remove sheet")
        .click();
    h.run();
    assert_eq!(pcb(&h).stackup().dielectrics().count(), 1);
}

#[test]
fn applying_a_saved_layout_keeps_the_stackup_visible_and_reopening_restores_it() {
    let (home, work) = (
        TempDir::new("layout-stack-home"),
        TempDir::new("layout-stack-work"),
    );
    let mut h = harness(home.path(), work.path(), "Layout");
    let doc = pcb(&h).uuid;
    let key = TabKey::Stackup(doc);
    let shell = h.state_mut().shell_mut();
    shell.run("document.open", json!({"document":doc})).unwrap();
    shell
        .run("pcb.stackup.open", json!({"document":doc}))
        .unwrap();
    shell.run("layout.save", json!({"name":"editor"})).unwrap();
    shell.run("layout.apply:editor", json!({})).unwrap();
    assert!(shell.layout().is_shown(&key));
    shell.run("project.close", json!({})).unwrap();
    shell
        .open_project(&work.path().join("Layout/Layout.reda"))
        .unwrap();
    assert!(shell.layout().is_shown(&key));
    shell
        .run("document.delete", json!({"document":doc}))
        .unwrap();
    assert!(shell.layout().place(&key).is_none());
}

#[test]
fn the_layers_and_board_forms_edit_an_odd_boards_inner_layer_and_finish() {
    let (home, work) = (TempDir::new("sf-home"), TempDir::new("sf-work"));
    let mut h = harness(home.path(), work.path(), "First");
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "project.new",
            json!({"path":work.path(),"name":"Odd", "layers":3}),
        )
        .unwrap();
    let document = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();
    h.get_by_label("Not symmetric");
    retype(&mut h, "L2 name", "Ground");
    h.key_press(Key::Enter);
    h.run();
    choose(&mut h, "L2 Ground kind", "Plane");
    choose(&mut h, "Surface finish", "ENIG");
    for (label, text) in [
        ("Solder mask colour", "Purple"),
        ("Silkscreen colour", "White"),
    ] {
        retype(&mut h, label, text);
        h.key_press(Key::Enter);
        h.run();
    }
    let board = pcb(&h);
    let layer = board.copper_layers().nth(1).unwrap();
    assert_eq!(layer.name.as_deref(), Some("Ground"));
    assert_eq!(layer.role, reda_ui::engine::model::CopperRole::Plane);
    let stack = board.stackup();
    assert_eq!(stack.finish.as_deref(), Some("ENIG"));
    assert_eq!(stack.mask_color.as_deref(), Some("Purple"));
    assert_eq!(stack.silk_color.as_deref(), Some("White"));
}

#[test]
fn stackup_table_names_and_moves_used_via_types_via_the_preview() {
    let (home, work) = (
        TempDir::new("via-table-home"),
        TempDir::new("via-table-project"),
    );
    let mut h = harness(home.path(), work.path(), "ViaTable");
    let old = pcb(&h).uuid;
    let shell = h.state_mut().shell_mut();
    shell
        .run("document.delete", json!({"document":old}))
        .unwrap();
    shell
        .run("document.add", json!({"kind":"pcb","layers":4}))
        .unwrap();
    let document = pcb(&h).uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();
    let button = h.get_by_role_and_label(Role::Button, "Add Via type");
    button.scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::Button, "Add Via type")
        .click();
    h.run();
    let board = pcb(&h);
    let kind = board.stackup().via_types[1].clone();
    retype(&mut h, "Blind 1:2 name", "Laser");
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        pcb(&h).stackup().via_types[1].name.as_deref(),
        Some("Laser")
    );
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "pcb.via.add",
            json!({"from":kind.from,"to":kind.to,"at":{"x":0,"y":0}}),
        )
        .unwrap();
    choose(&mut h, "Laser to", "L3");
    h.get_by_role_and_label(Role::Button, "Move Vias").click();
    h.run();
    let board = pcb(&h);
    assert_eq!(
        board.copper_number(board.vias().next().unwrap().span.to),
        Some(3)
    );
}

#[test]
fn a_net_class_can_exclude_the_through_via_in_board_setup() {
    let (home, work) = (
        TempDir::new("via-class-home"),
        TempDir::new("via-class-project"),
    );
    let mut h = harness(home.path(), work.path(), "ClassVias");
    let original = pcb(&h).uuid;
    let shell = h.state_mut().shell_mut();
    shell
        .run("document.delete", json!({"document":original}))
        .unwrap();
    shell
        .run("document.add", json!({"kind":"pcb","layers":4}))
        .unwrap();
    let b = pcb(&h);
    let l: Vec<_> = b.copper_layers().map(|l| l.uuid).collect();
    let shell = h.state_mut().shell_mut();
    let kind = shell
        .run(
            "pcb.via_type.add",
            json!({"document":b.uuid,"from":l[0],"to":l[1]}),
        )
        .unwrap()["result"]["uuid"]
        .clone();
    let class = shell
        .run("pcb.net_class.add", json!({"name":"Laser"}))
        .unwrap()["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    open_board_setup(&mut h, "ClassVias");
    page(&mut h, "Net Classes");
    h.get_by_label("Select or drag Laser").click();
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Allow Through")
        .click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(
        serde_json::to_value(pcb(&h).class_rule(RuleKind::ViaSpan, class)).unwrap()["values"]["allowed"],
        json!([kind])
    );
}

#[test]
fn layer_table_previews_add_and_delete_then_commits_each_once() {
    let (home, work) = (
        TempDir::new("layer-table-home"),
        TempDir::new("layer-table-work"),
    );
    let mut h = harness(home.path(), work.path(), "LayerTable");
    let doc = pcb(&h).uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document":doc}))
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::Button, "Add Layer").click();
    h.run();
    h.get_by_label("Via types stretched: 1");
    assert_eq!(pcb(&h).copper_layers().count(), 2);
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    let inner = pcb(&h).copper_layers().nth(1).unwrap().uuid;
    h.state_mut()
        .shell_mut()
        .run(
            "pcb.graphic.add_line",
            json!({"layer":inner,"from":{"x":0,"y":0},"to":{"x":500000,"y":0},"width":100000}),
        )
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::Button, "Remove Layer")
        .click();
    h.run();
    h.get_by_label("Graphics deleted: 1");
    h.state_mut().shell_mut().run("pcb.graphic.add_line",json!({"layer":inner,"from":{"x":0,"y":1000},"to":{"x":500000,"y":1000},"width":100000})).unwrap();
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    h.get_by_label("Graphics deleted: 2");
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    h.run();
    assert_eq!(pcb(&h).graphics().count(), 2);
    h.get_by_role_and_label(Role::Button, "Remove Layer")
        .click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    assert_eq!(pcb(&h).graphics().count(), 0);
    h.state_mut()
        .shell_mut()
        .run("edit.undo", json!({"document":doc}))
        .unwrap();
    assert_eq!(pcb(&h).graphics().count(), 2);
    assert_eq!(pcb(&h).copper_layers().count(), 3);
    let entries = h.state().shell().engine().journal();
    for id in ["pcb.layer.add", "pcb.layer.remove"] {
        assert_eq!(entries.iter().filter(|entry| entry.id == id).count(), 1);
    }
}

#[test]
fn the_stackup_editor_shows_a_modified_construction_and_saves_the_board_as_a_new_build() {
    let (home, work) = (
        TempDir::new("build-editor-home"),
        TempDir::new("build-editor-work"),
    );
    let mut h = harness(home.path(), work.path(), "Board");
    let document = pcb(&h).uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.set", json!({"thickness":"1.5mm"}))
        .unwrap();
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();
    h.get_by_label("Modified from 2 Layer");
    h.get_by_role_and_label(Role::Button, "Save as Construction…")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::Button, "Save as Construction…")
        .click();
    h.run();
    retype(&mut h, "Construction name", "Bench");
    h.get_by_role_and_label(Role::Button, "Save Construction")
        .click();
    h.run();
    let preset = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .fab_presets()
        .next()
        .unwrap()
        .preset();
    assert_eq!(preset.construction("Bench").unwrap().thickness.0, 1_500_000);
}

#[test]
fn via_types_table_marks_a_drill_below_the_minimum_and_a_land_above_the_maximum() {
    let (home, work) = (
        TempDir::new("via-limits-home"),
        TempDir::new("via-limits-project"),
    );
    let mut h = harness(home.path(), work.path(), "ViaLimits");
    let old = pcb(&h).uuid;
    let shell = h.state_mut().shell_mut();
    shell
        .run("document.delete", json!({"document":old}))
        .unwrap();
    shell
        .run("document.add", json!({"kind":"pcb","layers":4}))
        .unwrap();
    let board = pcb(&h);
    let document = board.uuid;
    let layers: Vec<_> = board.copper_layers().map(|l| l.uuid).collect();
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "pcb.via_type.add",
            json!({"document":document, "name":"Laser", "micro":true,
        "from":layers[2], "to":layers[0], "diameter":{"start":"0.5mm","middle":"0.8mm"}}),
        )
        .unwrap();
    shell
        .run(
            "pcb.rules.set",
            json!({"kind":"via_size","values":{"hole":{"min":"0.4mm"}}}),
        )
        .unwrap();
    shell
        .run(
            "pcb.rules.set",
            json!({"kind":"microvia_size","values":{"diameter":{"max":"0.7mm"}}}),
        )
        .unwrap();
    shell
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();
    let warning = |name| format!("{name} has a drill or pad outside the board’s size limits.");
    h.get_by_label(&warning("Through"));
    h.get_by_label(&warning("Laser"));
    h.state_mut()
        .shell_mut()
        .run(
            "pcb.rules.set",
            json!({"kind":"microvia_size","enabled":false}),
        )
        .unwrap();
    h.run();
    h.get_by_label(&warning("Through"));
    assert!(h.query_by_label(&warning("Laser")).is_none());
}

#[test]
fn a_plane_role_offers_an_outline_zone_only_until_one_is_created() {
    let (home, work) = (TempDir::new("plane-ui-home"), TempDir::new("plane-ui-work"));
    let mut h = harness(home.path(), work.path(), "First");
    let shell = h.state_mut().shell_mut();
    shell.run("project.new",json!({"name":"Planes","path":work.path(),"layers":4,"board":{"width":"20mm","height":"20mm"}})).unwrap();
    let gnd = shell.run("test.net.add", json!({"name":"GND"})).unwrap()["result"]["uuid"].clone();
    let board = pcb(&h);
    let layer = board.copper_layers().nth(1).unwrap().uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document":board.uuid}))
        .unwrap();
    h.run();
    choose(&mut h, "L2 kind", "Plane");
    assert_eq!(pcb(&h).zones().count(), 0);
    h.get_by_role_and_label(Role::Button, "Fill Layer with plane…")
        .click();
    h.run();
    choose(&mut h, "Plane Net", "GND");
    h.get_by_role_and_label(Role::Button, "Create Zone").click();
    h.run();
    let board = pcb(&h);
    let zone = board.zones().next().unwrap();
    assert!(zone.follows_outline);
    assert_eq!(zone.layer, layer);
    assert_eq!(json!(zone.net), gnd);
    choose(&mut h, "L2 kind", "Signal");
    choose(&mut h, "L2 kind", "Plane");
    assert_eq!(pcb(&h).zones().count(), 1);
    assert!(
        h.query_by_role_and_label(Role::Button, "Fill Layer with plane…")
            .is_none()
    );
    h.get_by_role_and_label(Role::Row, "L2").click_secondary();
    h.run();
    h.get_by_label("Fill Layer with plane…").click();
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    assert_eq!(pcb(&h).zones().count(), 1);
}

#[test]
fn apply_construction_preselects_empty_middle_layers_and_revert_restores_the_named_build() {
    let home = TempDir::new("apply-editor-home");
    let work = TempDir::new("apply-editor-work");
    let mut h = harness(home.path(), work.path(), "Board");
    let board = pcb(&h);
    let document = board.uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    let preset = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .fab_presets()
        .next()
        .unwrap()
        .uuid;
    h.state_mut()
        .shell_mut()
        .run(
            "pcb.stackup.apply_construction",
            json!({"document":document,"preset":preset,"construction":"6 Layer"}),
        )
        .unwrap();
    let l: Vec<_> = pcb(&h).copper_layers().map(|v| v.uuid).collect();
    h.state_mut()
        .shell_mut()
        .run(
            "pcb.text.add",
            json!({"layer":l[2],"text":"GND","x":0,"y":0}),
        )
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply Construction…")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply Construction…")
        .click();
    h.run();
    choose(&mut h, "Construction", "Generic · 4 Layer (4 Layers)");
    h.get_by_label("L3 · 1 object");
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    assert_eq!(
        pcb(&h).copper_layers().map(|l| l.uuid).collect::<Vec<_>>(),
        [l[0], l[2], l[4], l[5]]
    );
    assert_eq!(pcb(&h).texts().count(), 1);
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.set", json!({"thickness":"1.55mm"}))
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::Button, "Revert")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::Button, "Revert").click();
    h.run();
    assert_eq!(pcb(&h).stackup().thickness.0, 1_600_000);

    let recording = reda_ui::engine::journal::export(h.state().shell().engine().journal());
    let mut steps = reda_ui::engine::journal::parse(&recording).unwrap();
    let into = TempDir::new("apply-editor-replay");
    reda_ui::engine::journal::relocate_new_projects(&mut steps, into.path());
    let mut replay = Shell::new(Engine::builder().headless(false).user_dirs(None).build());
    reda_ui::engine::journal::replay_with(|id, params| replay.run(id, params), &steps).unwrap();
    let restored = replay.engine().project().unwrap().pcb().unwrap();
    assert_eq!(restored.copper_layers().count(), 4);
    assert_eq!(
        restored.copper_number(restored.texts().next().unwrap().layer),
        Some(2)
    );
    assert_eq!(restored.stackup().thickness.0, 1_600_000);
}

#[test]
fn choosing_construction_via_types_shows_the_unmatched_vias_before_replacing_the_table() {
    let home = TempDir::new("apply-types-ui-home");
    let work = TempDir::new("apply-types-ui-work");
    let mut h = harness(home.path(), work.path(), "Board");
    let document = pcb(&h).uuid;
    let preset = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .fab_presets()
        .next()
        .unwrap()
        .uuid;
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "pcb.stackup.apply_construction",
            json!({"document":document,"preset":preset,"construction":"4 Layer"}),
        )
        .unwrap();
    shell
        .run(
            "pcb.stackup.save_as_construction",
            json!({"document":document,"preset":preset,"name":"Through only"}),
        )
        .unwrap();
    let l: Vec<_> = shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    shell
        .run(
            "pcb.via_type.add",
            json!({"document":document,"from":l[0],"to":l[1]}),
        )
        .unwrap();
    shell
        .run(
            "pcb.via.add",
            json!({"from":l[0],"to":l[1],"at":{"x":0,"y":0}}),
        )
        .unwrap();
    shell
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply Construction…")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply Construction…")
        .click();
    h.run();
    choose(&mut h, "Construction", "Generic · Through only (4 Layers)");
    let checkbox = h.get_by_label("Apply Via types");
    assert_eq!(
        checkbox.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::False)
    );
    checkbox.click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    h.get_by_label("Vias left without a type: 1");
    assert_eq!(pcb(&h).stackup().via_types.len(), 2);
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    assert_eq!(pcb(&h).stackup().via_types.len(), 1);
}
