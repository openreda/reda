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
fn the_silkscreen_page_commits_both_fabrication_limits_and_their_severities() {
    use reda_ui::engine::model::{DrcCheck, Severity};
    let (home, work) = (TempDir::new("silk-win-home"), TempDir::new("silk-win-work"));
    let mut h = harness(home.path(), work.path(), "Silk");
    open_board_setup(&mut h, "Silk");
    page(&mut h, "Silkscreen");
    retype(&mut h, "Minimum silkscreen stroke width (mm)", "0.18");
    retype(&mut h, "Minimum silkscreen text height (mm)", "1.2");
    for choice in ["Silkscreen width: Error", "Silkscreen text height: Off"] {
        h.get_by_label(choice).scroll_to_me();
        h.run();
        h.get_by_label(choice).click();
        h.run();
    }
    h.get_by_label("OK").click();
    h.run();
    let board = pcb(&h);
    assert_eq!(
        board.board_rule(RuleKind::SilkWidth).values.min.unwrap().0,
        180_000
    );
    assert_eq!(
        board
            .board_rule(RuleKind::SilkTextHeight)
            .values
            .min
            .unwrap()
            .0,
        1_200_000
    );
    assert_eq!(
        board.setup().severities.get(DrcCheck::SilkWidth),
        Severity::Error
    );
    assert_eq!(
        board.setup().severities.get(DrcCheck::SilkTextHeight),
        Severity::Off
    );
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
    assert_eq!(
        h.state().shell().engine().journal()[before].id,
        "pcb.stackup.select"
    );
    assert_eq!(h.state().shell().engine().journal().len(), before + 1);
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        pcb(&h).stackup().copper().next().unwrap().thickness.0,
        70_000
    );
    assert_eq!(h.state().shell().engine().journal().len(), before + 2);
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
    choose(&mut h, "New Via lands", "Remove and keep both ends");
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "pcb.via.add",
            json!({"from":kind.from,"to":kind.to,"at":{"x":0,"y":0}}),
        )
        .unwrap();
    assert_eq!(
        pcb(&h).vias().next().unwrap().unused_layers,
        reda_ui::engine::model::UnusedLayers::RemoveKeepEnds
    );
    let layer = pcb(&h).copper_layers().nth(2).unwrap().uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.layer.set", json!({"layer":layer,"name":"GND"}))
        .unwrap();
    h.run();
    choose(&mut h, "Laser to", "L3 GND");
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
    h.get_by_label("1 graphic on L2 will be deleted.");
    h.state_mut().shell_mut().run("pcb.graphic.add_line",json!({"layer":inner,"from":{"x":0,"y":1000},"to":{"x":500000,"y":1000},"width":100000})).unwrap();
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    h.get_by_label("2 graphics on L2 will be deleted.");
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
fn stackup_shows_the_bare_impedance_and_default_width_on_the_layer_row() {
    let (home, work) = (TempDir::new("z0-home"), TempDir::new("z0-work"));
    let mut h = harness(home.path(), work.path(), "Impedance");
    let board = pcb(&h);
    let bottom = board.copper_layers().last().unwrap().uuid;
    let sheet = board.stackup().dielectrics().next().unwrap().uuid;
    let shell = h.state_mut().shell_mut();
    shell
        .run("pcb.layer.set", json!({"layer":bottom,"role":"plane"}))
        .unwrap();
    shell
        .run(
            "pcb.stackup.set",
            json!({"dielectrics":[{"uuid":sheet,"dk":4400,"thickness":200000}]}),
        )
        .unwrap();
    shell
        .run(
            "pcb.rules.set",
            json!({"kind":"track_width","values":{"preferred":"0.35mm"}}),
        )
        .unwrap();
    shell
        .run(
            "pcb.rules.set",
            json!({"kind":"track_width","scope":{"layers":[bottom]},"values":{"preferred":"0.3mm"}}),
        )
        .unwrap();
    shell
        .run("pcb.stackup.open", json!({"document":board.uuid}))
        .unwrap();
    h.run();
    h.get_by_label("Z0");
    let top = h.get_by_role_and_label(Role::Row, "Top Copper").rect();
    assert!(top.contains(h.get_by_label("50.2 Ω").rect().center()));
    h.get_by_label_contains("without solder mask");
    h.get_by_label_contains("L1 0.35 mm");
    h.get_by_label_contains("L2 0.3 mm");
    h.state_mut()
        .shell_mut()
        .run(
            "pcb.stackup.set",
            json!({"dielectrics":[{"uuid":sheet,"dk":null,"thickness":"1.53mm"}]}),
        )
        .unwrap();
    h.run();
    let rough = h.get_by_label_contains("≈");
    assert!(top.contains(rough.rect().center()));
    assert!(
        rough
            .accesskit_node()
            .description()
            .unwrap()
            .contains("4.5")
    );
    // A three-digit rough estimate, including its unit, must fit the column.
    assert!(h.output().shapes.iter().any(|shape| matches!(
        &shape.shape,
        egui::Shape::Text(text) if text.galley.text().starts_with('≈')
            && text.galley.text().ends_with('Ω') && !text.galley.elided
    )));
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
fn splitting_a_sheet_in_the_cross_section_adds_one_layer_command() {
    let (home, work) = (TempDir::new("section-home"), TempDir::new("section-work"));
    let mut h = harness(home.path(), work.path(), "Section");
    let document = pcb(&h).uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::RadioButton, "Schematic")
        .click();
    h.run();
    let sheet = h
        .get_by_role_and_label(Role::Button, "Select band Dielectric 1")
        .rect();
    h.hover_at(egui::pos2(sheet.left() + 8.0, sheet.center().y));
    h.run();
    h.get_by_role_and_label(Role::Button, "+ Layer").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    assert_eq!(pcb(&h).copper_layers().count(), 3);
    let journal = h.state().shell().engine().journal();
    assert_eq!(
        journal
            .iter()
            .filter(|entry| entry.id == "pcb.layer.add")
            .count(),
        1
    );
}

#[test]
fn dragging_a_via_end_in_the_cross_section_changes_its_span_once_at_release() {
    let (home, work) = (TempDir::new("barrel-home"), TempDir::new("barrel-work"));
    let mut h = harness(home.path(), work.path(), "Barrel");
    let original = pcb(&h).uuid;
    let shell = h.state_mut().shell_mut();
    shell
        .run("document.delete", json!({"document":original}))
        .unwrap();
    shell
        .run("document.add", json!({"kind":"pcb","layers":4}))
        .unwrap();
    let board = pcb(&h);
    let copper: Vec<_> = board.copper_layers().map(|l| l.uuid).collect();
    let shell = h.state_mut().shell_mut();
    shell.run(
        "pcb.stackup.set",
        json!({
            "copper":[{"layer":copper[0], "thickness":"0.003mm"}, {"layer":copper[1], "thickness":"0.003mm"}],
            "dielectrics":[{"uuid":board.stackup().entries[1].uuid(), "thickness":"0.002mm"}],
        }),
    ).unwrap();
    shell.run("pcb.via_type.add", json!({"document":board.uuid,"from":copper[1],"to":copper[0],"name":"Laser","micro":true})).unwrap();
    shell
        .run("pcb.stackup.open", json!({"document":board.uuid}))
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::Button, "Select Via type Laser")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::Button, "Select Via type Laser")
        .click();
    h.run();
    let before = pcb(&h);
    let start = h
        .get_by_role_and_label(Role::Button, "Laser from")
        .rect()
        .center();
    let other = h.get_by_role_and_label(Role::Button, "Laser to").rect();
    assert!(
        !other.contains(start),
        "both ends must be independently draggable on a thin stack"
    );
    let mid = h
        .get_by_role_and_label(Role::Button, "Select band L3")
        .rect();
    let target = egui::pos2(start.x, mid.center().y);
    h.drag_at(start);
    h.run();
    h.hover_at(start.lerp(target, 0.5));
    h.run();
    h.hover_at(target);
    h.run();
    let count = |h: &Harness<'static, App>| {
        h.state()
            .shell()
            .engine()
            .journal()
            .iter()
            .filter(|entry| entry.id == "pcb.via_type.set")
            .count()
    };
    assert_eq!(count(&h), 0, "dragging only previews the destination");
    h.drop_at(target);
    h.run();
    assert_eq!(count(&h), 1);
    assert_eq!(pcb(&h).stackup().via_types[1].from, copper[2]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(pcb(&h).stackup(), before.stackup());
}

#[test]
fn a_thin_dielectric_keeps_its_split_button_while_the_pointer_is_on_it() {
    let home = TempDir::new("thin-sheet-home");
    let work = TempDir::new("thin-sheet-work");
    let mut h = harness(home.path(), work.path(), "First");
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "project.new",
            json!({"path":work.path(),"name":"Thin","layers":4}),
        )
        .unwrap();
    let board = shell.engine().project().unwrap().pcb().unwrap();
    let (doc, sheet) = (board.uuid, board.stackup().entries[1].uuid());
    shell
        .run(
            "pcb.stackup.set",
            json!({"dielectrics":[{"uuid":sheet,"thickness":"0.002mm"}]}),
        )
        .unwrap();
    shell
        .run("pcb.stackup.open", json!({"document":doc}))
        .unwrap();
    h.run();
    let sheet = h
        .get_by_role_and_label(Role::Button, "Select band Dielectric 1")
        .rect();
    h.hover_at(egui::pos2(sheet.left() + 8.0, sheet.center().y));
    h.run();
    let button = h.get_by_role_and_label(Role::Button, "+ Layer").rect();
    let above = egui::pos2(button.center().x, button.top() + 2.);
    assert!(!sheet.contains(above));
    h.hover_at(above);
    h.run();
    h.get_by_role_and_label(Role::Button, "+ Layer").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    assert_eq!(pcb(&h).copper_layers().count(), 5);
}

#[test]
fn the_stackup_tables_and_drawing_share_keyboard_and_pointer_selection() {
    let (home, work) = (
        TempDir::new("section-selection-home"),
        TempDir::new("section-selection-work"),
    );
    let mut h = harness(home.path(), work.path(), "Selection");
    let document = pcb(&h).uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();

    // In a small tab, selecting a band in the right pane selects its table row.
    h.set_size(egui::vec2(1060.0, 640.0));
    h.run();
    let sheet = h
        .get_by_role_and_label(Role::Button, "Select band Dielectric 1")
        .rect();
    let row = h.get_by_role_and_label(Role::Row, "Dielectric 1").rect();
    assert!(row.left() < sheet.left() && sheet.left() < 1060.0);
    let at = egui::pos2(sheet.left() + 4.0, sheet.center().y);
    h.drag_at(at);
    h.drop_at(at);
    h.run();
    assert_eq!(
        h.get_by_role_and_label(Role::Row, "Dielectric 1")
            .accesskit_node()
            .is_selected(),
        Some(true)
    );
    // A keyboard user can select a Via type in the left table.
    h.get_by_role_and_label(Role::Row, "Through").focus();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        h.get_by_role_and_label(Role::Button, "Select Via type Through")
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    assert_eq!(
        h.get_by_role_and_label(Role::Row, "Dielectric 1")
            .accesskit_node()
            .is_selected(),
        Some(false)
    );

    h.get_by_role_and_label(Role::Row, "Top Copper").focus();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        h.get_by_role_and_label(Role::Button, "Select band Top Copper")
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    h.get_by_role_and_label(Role::Button, "Select Via type Through")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::Button, "Select Via type Through")
        .click();
    h.run();
    assert_eq!(
        h.get_by_role_and_label(Role::Row, "Through")
            .accesskit_node()
            .is_selected(),
        Some(true)
    );
}

#[test]
fn the_keyboard_can_split_the_selected_dielectric_and_change_a_via_end() {
    fn enter(h: &mut Harness<'static, App>, role: Role, label: &str) {
        h.get_by_role_and_label(role, label).focus();
        h.run();
        h.key_press(Key::Enter);
        h.run();
    }

    let (home, work) = (
        TempDir::new("section-keys-home"),
        TempDir::new("section-keys-work"),
    );
    let mut h = harness(home.path(), work.path(), "SectionKeys");
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "project.new",
            json!({"name":"Keyed","path":work.path(),"layers":4}),
        )
        .unwrap();
    let board = pcb(&h);
    let document = board.uuid;
    let layers: Vec<_> = board.copper_layers().map(|l| l.uuid).collect();
    let sheet = board.stackup().dielectrics().nth(2).unwrap().uuid;
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "pcb.via_type.add",
            json!({"document":document,"from":layers[0],"to":layers[1],"name":"Blind"}),
        )
        .unwrap();
    shell
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();

    enter(&mut h, Role::ComboBox, "Blind to");
    enter(&mut h, Role::Button, "L3");
    assert_eq!(pcb(&h).stackup().via_types[1].to, layers[2]);
    enter(&mut h, Role::Row, "Dielectric 3");
    enter(&mut h, Role::Button, "Add Layer");
    enter(&mut h, Role::Button, "Apply");
    assert_eq!(pcb(&h).copper_layers().count(), 5);
    let journal = h.state().shell().engine().journal();
    let adds: Vec<_> = journal.iter().filter(|e| e.id == "pcb.layer.add").collect();
    assert_eq!(adds.len(), 1);
    assert_eq!(adds[0].params["split"], json!(sheet));
    assert_eq!(
        journal
            .iter()
            .filter(|e| e.id == "pcb.via_type.set")
            .count(),
        1
    );
}

#[test]
fn schematic_scale_makes_thin_copper_legible_without_changing_the_stackup() {
    let (home, work) = (TempDir::new("scale-home"), TempDir::new("scale-work"));
    let mut h = harness(home.path(), work.path(), "Scale");
    let document = pcb(&h).uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();
    let heights = |h: &Harness<'static, App>| {
        let band = |name| h.get_by_role_and_label(Role::Button, name).rect().height();
        (
            band("Select band Top Copper"),
            band("Select band Dielectric 1"),
        )
    };
    let (copper, sheet) = heights(&h);
    // The default two-Layer board has 35 μm foil and 1.53 mm of dielectric.
    assert!((copper / sheet - 0.035 / 1.53).abs() < 0.001);
    let before = h.state().shell().engine().journal().len();
    h.get_by_role_and_label(Role::RadioButton, "Schematic")
        .click();
    h.run();
    let (thicker, thinner) = heights(&h);
    assert!(thicker > copper * 4.0 && thinner < sheet);
    let journal = h.state().shell().engine().journal();
    assert_eq!(journal.len(), before + 1);
    assert_eq!(journal.last().unwrap().id, "pcb.stackup.scale");
    let through = pcb(&h).stackup().through_type().uuid;
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "pcb.stackup.scale",
            json!({"document":document, "scale":"physical"}),
        )
        .unwrap();
    shell
        .run(
            "pcb.stackup.select",
            json!({"document":document, "entry":through}),
        )
        .unwrap();
    h.run();
    assert_eq!(heights(&h), (copper, sheet));
    assert_eq!(
        h.get_by_role_and_label(Role::Row, "Through")
            .accesskit_node()
            .is_selected(),
        Some(true)
    );
}

#[test]
fn a_small_tab_can_reach_the_bottom_of_a_32_layer_stackup() {
    let (home, work) = (
        TempDir::new("section-32-home"),
        TempDir::new("section-32-work"),
    );
    let mut h = harness(home.path(), work.path(), "Tall");
    h.state_mut()
        .shell_mut()
        .run(
            "project.new",
            json!({"name":"ThirtyTwo","path":work.path(),"layers":32}),
        )
        .unwrap();
    let document = pcb(&h).uuid;
    h.state_mut()
        .shell_mut()
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.set_size(egui::vec2(1060.0, 640.0));
    h.run();

    let name = "Select band Dielectric 31";
    assert!(h.get_by_role_and_label(Role::Button, name).rect().top() > 640.0);
    h.get_by_role_and_label(Role::Button, name).scroll_to_me();
    h.run();
    let sheet = h.get_by_role_and_label(Role::Button, name).rect();
    assert!(sheet.center().y < 640.0);
    h.hover_at(egui::pos2(sheet.left() + 4.0, sheet.center().y));
    h.run();
    assert!(h.query_by_role_and_label(Role::Button, "+ Layer").is_none());
    h.get_by_role_and_label(Role::ComboBox, "Through to")
        .scroll_to_me();
    h.run();
    let span = h.get_by_role_and_label(Role::ComboBox, "Through to");
    assert_eq!(span.value().as_deref(), Some("Bottom Copper"));
    assert!(span.rect().center().y < 640.0);
    assert!(
        h.output().shapes.iter().any(|paint| matches!(&paint.shape,
            egui::Shape::Text(text) if span.rect().contains(text.pos)
                && text.galley.text() == "Bottom Copper" && !text.galley.elided
        )),
        "the closed selector paints the full outer Layer name"
    );
}

#[test]
fn shortened_and_left_out_teardrops_open_their_tracks_on_the_physical_layer() {
    for (section, counts, blocked) in [
        (
            "Shortened Teardrops",
            "0 generated, 1 shortened, 0 left out",
            false,
        ),
        (
            "Teardrops left out",
            "0 generated, 0 shortened, 1 left out",
            true,
        ),
    ] {
        let (home, work) = (
            TempDir::new("teardrop-nav-home"),
            TempDir::new("teardrop-nav-work"),
        );
        let mut h = harness(home.path(), work.path(), "Teardrop");
        let shell = h.state_mut().shell_mut();
        let board = shell.engine().project().unwrap().pcb().unwrap();
        let document = board.uuid;
        let bottom = board.copper_layers().last().unwrap().uuid;
        shell
            .run(
                "pcb.via.add",
                json!({"at":{"x":"5mm","y":"5mm"},"hole":"0.3mm","diameter":"1mm"}),
            )
            .unwrap();
        let track = shell
            .run(
                "pcb.track.add",
                json!({"layer":bottom,"width":"0.1mm","points":[
        {"x":"5mm","y":"5mm"},{"x":"7mm","y":"5mm"}]}),
            )
            .unwrap()["result"]["tracks"][0]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        shell
            .run(
                "pcb.rules.set",
                json!({"kind":"teardrop","values":{"via":{"enabled":true}}}),
            )
            .unwrap();
        // A keepout at the Via's edge leaves the Teardrop out; one beside its flare only
        // shortens it, since a Track that is merely short is not counted as shortened.
        let keepout = if blocked {
            json!([{"x":"5.45mm","y":"4.9mm"},{"x":"5.55mm","y":"4.9mm"},
                   {"x":"5.55mm","y":"5.1mm"},{"x":"5.45mm","y":"5.1mm"}])
        } else {
            json!([{"x":"5.6mm","y":"5.2mm"},{"x":"5.7mm","y":"5.2mm"},
                   {"x":"5.7mm","y":"5.3mm"},{"x":"5.6mm","y":"5.3mm"}])
        };
        shell
            .run(
                "pcb.keepout.add",
                json!({"layer":bottom,"restrict":["tracks"],"vertices":keepout}),
            )
            .unwrap();
        shell.run("pcb.zone.fill", json!({})).unwrap();
        open_board_setup(&mut h, "Teardrop");
        page(&mut h, "Teardrops");
        h.get_by_label(counts);
        h.get_by_label(section).scroll_to_me();
        h.run();
        h.get_by_label(section).click();
        h.run();
        let destination = "L2 · X 5.5 mm · Y 5 mm";
        h.get_by_role_and_label(Role::Button, destination)
            .scroll_to_me();
        h.run();
        h.get_by_role_and_label(Role::Button, destination).click();
        h.run();
        let shell = h.state().shell();
        assert_eq!(shell.layout().focus, Some(TabKey::Document(document)));
        assert_eq!(shell.highlight().objects, [track]);
        assert_eq!(
            shell
                .board_view()
                .active_layer(shell.engine().project().unwrap()),
            Some(bottom)
        );
    }
}

#[test]
fn a_narrow_cross_section_keeps_the_split_action_clear_of_via_barrels() {
    let (home, work) = (TempDir::new("split-home"), TempDir::new("split-work"));
    let mut h = harness(home.path(), work.path(), "First");
    h.state_mut()
        .shell_mut()
        .run(
            "project.new",
            json!({"name":"Split","path":work.path(),"layers":4}),
        )
        .unwrap();
    let board = pcb(&h);
    let copper: Vec<_> = board.copper_layers().map(|l| l.uuid).collect();
    let shell = h.state_mut().shell_mut();
    shell
        .run(
            "pcb.via_type.add",
            json!({"document":board.uuid,"name":"Blind","from":copper[0],"to":copper[1]}),
        )
        .unwrap();
    shell
        .run("pcb.stackup.open", json!({"document":board.uuid}))
        .unwrap();
    h.set_size(egui::vec2(1060.0, 640.0));
    h.run();
    let before = pcb(&h);
    let band = h
        .get_by_role_and_label(Role::Button, "Select band Dielectric 1")
        .rect();
    h.hover_at(egui::pos2(band.left() + 4.0, band.center().y));
    h.run();
    let split = h.get_by_role_and_label(Role::Button, "+ Layer").rect();
    let barrel = h
        .get_by_role_and_label(Role::Button, "Select Via type Through")
        .rect();
    assert!(
        !split.intersects(barrel),
        "split {split:?} overlaps Through {barrel:?}"
    );
    h.hover_at(split.center());
    h.run();
    h.get_by_role_and_label(Role::Button, "+ Layer").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run();
    assert_eq!(pcb(&h).copper_layers().count(), 5);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(pcb(&h).stackup(), before.stackup());
}

#[test]
fn selected_via_type_diameters_support_overrides_inheritance_and_undo() {
    let (home, work) = (
        TempDir::new("diameter-home"),
        TempDir::new("diameter-board"),
    );
    let mut h = harness(home.path(), work.path(), "Diameter");
    let document = pcb(&h).uuid;
    let shell = h.state_mut().shell_mut();
    shell
        .run("pcb.layer.add", json!({"document":document}))
        .unwrap();
    shell
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::Row, "Through").focus();
    h.run();
    h.key_press(Key::Enter);
    h.run();
    for (label, text) in [
        ("Middle diameter (mm)", "0.9"),
        ("End diameter (mm)", "1mm"),
        ("Through pad", "0.8"),
    ] {
        retype(&mut h, label, text);
        h.key_press(Key::Enter);
        h.run();
    }
    let diameter = |h: &Harness<'static, App>| json!(pcb(h).stackup().through_type().diameter);
    let expected = json!({"start":800000,"middle":900000,"end":1000000});
    assert_eq!(diameter(&h), expected);
    let last = h
        .state()
        .shell()
        .engine()
        .journal()
        .iter()
        .rev()
        .find(|e| e.id == "pcb.via_type.set")
        .unwrap();
    assert_eq!(last.params["diameter"], expected);
    for (label, remaining) in [
        (
            "Middle diameter (mm)",
            json!({"start":800000,"end":1000000}),
        ),
        ("End diameter (mm)", json!({"start":800000})),
    ] {
        h.get_by_role_and_label(Role::TextInput, label).focus();
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.key_press(Key::Backspace);
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(diameter(&h), remaining);
    }
    for _ in 0..2 {
        h.state_mut()
            .shell_mut()
            .run("edit.undo", json!({"document":document}))
            .unwrap();
    }
    assert_eq!(diameter(&h), expected);
}

/// Empties the field announced as `label`.
fn clear(h: &mut Harness<'static, App>, label: &str) {
    h.get_by_role_and_label(Role::TextInput, label)
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.key_press(Key::Backspace);
    h.run();
}

fn add_power_class(h: &mut Harness<'static, App>) -> reda_ui::engine::model::Uuid {
    h.state_mut()
        .shell_mut()
        .run("pcb.net_class.add", json!({"name": "Power"}))
        .unwrap()["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

const DRILLED_RATIO: &str = "Maximum Via aspect ratio (depth to hole)";
const MICRO_RATIO: &str = "Maximum micro Via aspect ratio (depth to hole)";
const MICRO_STACK: &str = "Maximum micro Vias stacked";

#[test]
fn the_vias_page_sets_micro_via_limits_aspect_ratios_and_stack_height() {
    use reda_ui::engine::model::geom::Nm;
    use reda_ui::engine::model::{RuleField as F, RuleValue};
    let (home, work) = (TempDir::new("micro-home"), TempDir::new("micro-work"));
    let mut h = harness(home.path(), work.path(), "Micro");
    let power = add_power_class(&mut h);
    open_board_setup(&mut h, "Micro");
    page(&mut h, "Vias");
    h.get_by_label("Drilled Vias");
    h.get_by_label("Micro Vias");
    // A board-wide preferred size belongs to the Via type, for micro Vias too.
    for size in ["Micro Via diameter", "Micro Via hole"] {
        assert!(
            h.query_by_role_and_label(
                Role::TextInput,
                &format!("Board-wide preferred {size} (mm)")
            )
            .is_none()
        );
    }
    for (label, text) in [
        (DRILLED_RATIO, "8.5"),
        ("Board-wide minimum Micro Via diameter (mm)", "0.25"),
        ("Board-wide maximum Micro Via diameter (mm)", "0.35"),
        ("Board-wide minimum Micro Via hole (mm)", "0.1"),
        ("Board-wide maximum Micro Via hole (mm)", "0.15"),
        (MICRO_RATIO, "0.9"),
        (MICRO_STACK, "2"),
        ("Power: Preferred Micro Via diameter", "0.3"),
        ("Power: Preferred Micro Via hole", "0.12"),
        (&format!("Power: {MICRO_RATIO}"), "5.333"),
        (&format!("Power: {MICRO_STACK}"), "1"),
    ] {
        retype(&mut h, label, text);
    }
    h.get_by_label("OK").click();
    h.run();
    let board = pcb(&h);
    let drilled = &board.board_rule(RuleKind::ViaSize).values;
    assert_eq!(
        drilled.get(F::AspectRatioMax),
        Some(RuleValue::MilliRatio(8500))
    );
    let micro = &board.board_rule(RuleKind::MicroviaSize).values;
    let lengths = |values: &reda_ui::engine::model::RuleValues, fields: &[F]| {
        fields.iter().map(|f| values.length(*f)).collect::<Vec<_>>()
    };
    let sizes = [
        F::DiameterMin,
        F::DiameterPreferred,
        F::DiameterMax,
        F::HoleMin,
        F::HolePreferred,
        F::HoleMax,
    ];
    assert_eq!(
        lengths(micro, &sizes),
        [
            Some(Nm(250_000)),
            None,
            Some(Nm(350_000)),
            Some(Nm(100_000)),
            None,
            Some(Nm(150_000)),
        ]
    );
    assert_eq!(
        micro.get(F::AspectRatioMax),
        Some(RuleValue::MilliRatio(900))
    );
    assert_eq!(micro.get(F::MaxStack), Some(RuleValue::Count(2)));
    let class = &board
        .class_rule(RuleKind::MicroviaSize, power)
        .expect("the class gets a micro Via record")
        .values;
    assert_eq!(
        lengths(class, &[F::DiameterPreferred, F::HolePreferred]),
        [Some(Nm(300_000)), Some(Nm(120_000))]
    );
    assert_eq!(
        class.get(F::AspectRatioMax),
        Some(RuleValue::MilliRatio(5333))
    );
    assert_eq!(class.get(F::MaxStack), Some(RuleValue::Count(1)));
    assert!(
        board.class_rule(RuleKind::ViaSize, power).is_none(),
        "a micro Via override leaves the drilled kind alone"
    );

    // Reopened, the values read back as typed, and emptying the optional ones clears them.
    open_board_setup(&mut h, "Micro");
    page(&mut h, "Vias");
    assert_eq!(value(&h, DRILLED_RATIO), "8.5");
    assert_eq!(value(&h, MICRO_RATIO), "0.9");
    assert_eq!(value(&h, &format!("Power: {MICRO_RATIO}")), "5.333");
    for label in [
        DRILLED_RATIO,
        "Board-wide maximum Micro Via hole (mm)",
        MICRO_RATIO,
        MICRO_STACK,
        &format!("Power: {MICRO_STACK}"),
    ] {
        clear(&mut h, label);
    }
    h.get_by_label("OK").click();
    h.run();
    let board = pcb(&h);
    let drilled = &board.board_rule(RuleKind::ViaSize).values;
    assert_eq!(drilled.get(F::AspectRatioMax), None);
    let micro = &board.board_rule(RuleKind::MicroviaSize).values;
    assert_eq!(micro.length(F::HoleMax), None);
    assert_eq!(
        micro.length(F::HoleMin),
        Some(Nm(100_000)),
        "the minimum stays"
    );
    assert_eq!(micro.get(F::AspectRatioMax), None);
    assert_eq!(micro.get(F::MaxStack), None);
    let class = &board
        .class_rule(RuleKind::MicroviaSize, power)
        .unwrap()
        .values;
    assert_eq!(class.get(F::MaxStack), None);
    assert_eq!(
        class.get(F::AspectRatioMax),
        Some(RuleValue::MilliRatio(5333))
    );
}

#[test]
fn a_via_ratio_or_stack_that_is_not_a_positive_number_is_flagged() {
    let (home, work) = (
        TempDir::new("micro-bad-home"),
        TempDir::new("micro-bad-work"),
    );
    let mut h = harness(home.path(), work.path(), "MicroBad");
    open_board_setup(&mut h, "MicroBad");
    page(&mut h, "Vias");
    let ratio = "Give a ratio above 0 with up to three decimals, such as 0.9 or 5.333.";
    for (label, text, message) in [
        (MICRO_RATIO, "0", ratio),
        (DRILLED_RATIO, "1.2345", ratio),
        (DRILLED_RATIO, "8:2", ratio),
        (MICRO_STACK, "0", "Give a whole number from 1 to 255."),
        (MICRO_STACK, "1.5", "Give a whole number from 1 to 255."),
    ] {
        retype(&mut h, label, text);
        h.get_by_role_and_label(Role::TreeItem, "Vias (has invalid values)");
        h.get_by_label(message);
        clear(&mut h, label);
    }
    // A ratio may be written with its `:1`.
    retype(&mut h, DRILLED_RATIO, "10:1");
    h.get_by_role_and_label(Role::TreeItem, "Vias");
    assert_eq!(pending(&h), 1);
}

#[test]
fn the_net_classes_grid_has_micro_via_size_columns() {
    use reda_ui::engine::model::RuleField as F;
    use reda_ui::engine::model::geom::Nm;
    let (home, work) = (
        TempDir::new("micro-grid-home"),
        TempDir::new("micro-grid-work"),
    );
    let mut h = harness(home.path(), work.path(), "MicroGrid");
    let power = add_power_class(&mut h);
    open_board_setup(&mut h, "MicroGrid");
    page(&mut h, "Net Classes");
    h.get_by_label("Default: Micro Via size enabled");
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Default: Preferred Micro Via hole")
            .is_none(),
        "the Default row has no preferred micro Via size"
    );
    retype(&mut h, "Default: Micro Via diameter", "0.2");
    // A minimum typed for a class with no record fills its range.
    retype(&mut h, "Power: Micro Via hole", "0.1");
    assert_eq!(value(&h, "Power: Preferred Micro Via hole"), "0.1");
    assert_eq!(value(&h, "Power: Maximum Micro Via hole"), "0.1");
    h.get_by_label("OK").click();
    h.run();
    let board = pcb(&h);
    assert_eq!(
        board
            .board_rule(RuleKind::MicroviaSize)
            .values
            .length(F::DiameterMin),
        Some(Nm(200_000))
    );
    let class = &board
        .class_rule(RuleKind::MicroviaSize, power)
        .unwrap()
        .values;
    assert_eq!(
        [F::HoleMin, F::HolePreferred, F::HoleMax].map(|f| class.length(f)),
        [Some(Nm(100_000)); 3]
    );
}

#[test]
fn by_layer_resolves_the_drafted_records_before_apply() {
    let (home, work) = (TempDir::new("by-layer-home"), TempDir::new("by-layer-work"));
    let mut h = harness(home.path(), work.path(), "ByLayer");
    add_power_class(&mut h);
    let layers = pcb(&h).copper_layers().count();
    open_board_setup(&mut h, "ByLayer");
    // Clearance is the first page. Neither value is on the board yet.
    retype(&mut h, "Board-wide minimum Clearance (mm)", "0.27");
    retype(&mut h, "Power: Clearance", "0.41");
    h.get_by_role_and_label(Role::DisclosureTriangle, "By Layer")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::DisclosureTriangle, "By Layer")
        .click();
    h.run();
    assert_eq!(
        h.get_all_by_role_and_label(Role::Label, "0.27").count(),
        layers,
        "the Default class resolves the drafted board-wide clearance on every Layer"
    );
    choose(&mut h, "By Layer Net Class", "Power");
    assert_eq!(
        h.get_all_by_role_and_label(Role::Label, "0.41").count(),
        layers,
        "Power resolves its drafted override"
    );
    // Clearing the override falls back to the drafted board-wide value.
    clear(&mut h, "Power: Clearance");
    assert_eq!(
        h.get_all_by_role_and_label(Role::Label, "0.27").count(),
        layers
    );
    assert!(pending(&h) > 0, "nothing is applied yet");
}

#[test]
fn removing_a_layer_confirms_its_objects_and_vias_in_sentences_naming_the_layer() {
    let (home, work) = (
        TempDir::new("layer-sentences-home"),
        TempDir::new("layer-sentences-work"),
    );
    let mut h = harness(home.path(), work.path(), "Sentences");
    let document = pcb(&h).uuid;
    let shell = h.state_mut().shell_mut();
    for _ in 0..2 {
        shell
            .run("pcb.layer.add", json!({"document":document}))
            .unwrap();
    }
    let l: Vec<_> = pcb_of(shell).copper_layers().map(|l| l.uuid).collect();
    shell
        .run("pcb.layer.set", json!({"layer":l[1],"name":"GND"}))
        .unwrap();
    for (from, to) in [(0, 1), (1, 3)] {
        shell
            .run(
                "pcb.via_type.add",
                json!({"document":document,"from":l[from],"to":l[to]}),
            )
            .unwrap();
    }
    // Two Vias collapse to one Layer and go; one is shortened to L3–L4.
    for (from, to, x) in [(0, 1, 0), (0, 1, 2_000_000), (1, 3, 4_000_000)] {
        shell
            .run(
                "pcb.via.add",
                json!({"from":l[from],"to":l[to],"at":{"x":x,"y":0}}),
            )
            .unwrap();
    }
    for y in [0, 1_000_000] {
        shell
            .run(
                "pcb.track.add",
                json!({"layer":l[1],"width":100000,"points":[{"x":0,"y":y},{"x":500000,"y":y}]}),
            )
            .unwrap();
    }
    shell
        .run(
            "pcb.graphic.add_line",
            json!({"layer":l[1],"from":{"x":0,"y":3000000},"to":{"x":500000,"y":3000000},"width":100000}),
        )
        .unwrap();
    shell
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    shell
        .run(
            "pcb.stackup.select",
            json!({"document":document,"entry":l[1]}),
        )
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::Button, "Remove Layer")
        .click();
    h.run();
    h.get_by_label("2 Tracks, 1 graphic on L2 GND will be deleted.");
    h.get_by_label("1 Via shortened, 2 deleted.");
}

#[test]
fn apply_via_types_starts_ticked_only_while_the_board_has_just_its_through_type() {
    let home = TempDir::new("apply-types-default-home");
    let work = TempDir::new("apply-types-default-work");
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
    shell
        .run("pcb.stackup.open", json!({"document":document}))
        .unwrap();
    h.run();
    let ticked = |h: &mut Harness<'static, App>| {
        h.get_by_role_and_label(Role::Button, "Apply Construction…")
            .scroll_to_me();
        h.run();
        h.get_by_role_and_label(Role::Button, "Apply Construction…")
            .click();
        h.run();
        choose(h, "Construction", "Generic · Through only (4 Layers)");
        let toggled = h.get_by_label("Apply Via types").accesskit_node().toggled();
        h.get_by_role_and_label(Role::Button, "Cancel").click();
        h.run();
        toggled
    };
    assert_eq!(ticked(&mut h), Some(egui::accesskit::Toggled::True));
    let l = pcb(&h).copper_layers().map(|l| l.uuid).collect::<Vec<_>>();
    h.state_mut()
        .shell_mut()
        .run(
            "pcb.via_type.add",
            json!({"document":document,"from":l[0],"to":l[1]}),
        )
        .unwrap();
    h.run();
    assert_eq!(ticked(&mut h), Some(egui::accesskit::Toggled::False));
}

fn pcb_of(shell: &Shell) -> PcbDocument {
    shell.engine().project().unwrap().pcb().unwrap().clone()
}
