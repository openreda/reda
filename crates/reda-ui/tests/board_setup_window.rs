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
        .click();
    h.run();
    retype(&mut h, "Paste area change (%)", "-10");

    page(&mut h, "Zone Connections");
    choose(&mut h, "SMD Pads: Style", "Solid");
    choose(&mut h, "Zone naming scheme", "LXX_NET_PXXX");

    page(&mut h, "Stackup");
    let section = |h: &Harness<'static, App>| {
        h.get_by_role(Role::Image)
            .accesskit_node()
            .label()
            .unwrap_or_default()
    };
    assert!(
        section(&h).contains("Top Copper 0.035 mm") && section(&h).contains("Dielectric 1 1.53 mm"),
        "{}",
        section(&h)
    );
    retype(&mut h, "Top Copper thickness", "2oz");
    // The cross-section and the totals follow as you type.
    assert!(section(&h).contains("Top Copper 0.07 mm"), "{}", section(&h));
    assert!(section(&h).contains("Dielectric 1 1.495 mm"), "{}", section(&h));
    h.get_by_label_contains("total 1.6 mm against a target of 1.6 mm");
    choose(&mut h, "Surface finish", "ENIG");
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
    let mask = board.board_rule(RuleKind::MaskExpansion).values;
    assert_eq!((mask.top.unwrap().0, mask.bottom.unwrap().0), (80_000, 80_000));
    let paste = board.board_rule(RuleKind::PasteExpansion).values;
    assert_eq!((paste.expansion, paste.percent), (None, Some(-10)));
    let zones = board.board_rule(RuleKind::ZoneConnection).values;
    assert_eq!(
        zones.connection(ConnectionTarget::Smd).unwrap().style,
        Some(ConnectionStyle::Solid)
    );
    assert_eq!(board.setup().zone_name_scheme, ZoneNameScheme::LxxNetPxxx);
    let top = board.stackup().copper().next().unwrap().thickness;
    assert_eq!(top.0, 70_000);
    assert_eq!(board.stackup().finish.as_deref(), Some("ENIG"));
    assert_eq!(board.setup().fab_notes.as_deref(), Some("No panel"));
}

#[test]
fn mod_z_inside_board_setup_steps_back_through_pending_edits_first() {
    let (home, work) = (TempDir::new("bsw-home"), TempDir::new("bsw-work"));
    let mut h = harness(home.path(), work.path(), "Win");
    open_board_setup(&mut h, "Win");
    page(&mut h, "DRC");
    h.get_by_label("Courtyard Overlap: Off").click();
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
    assert!(h.query_by_label("Select").is_none(), "the banner takes the Active Bar's place");
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
    for action in ["Save Rules as Fab Preset…", "Export Rules…", "Import Rules…"] {
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
