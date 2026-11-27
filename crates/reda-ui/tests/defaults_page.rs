//! Preferences ▸ Defaults (docs/spec/v0.3/input-and-defaults.md#the-defaults-page): the
//! kinds by editor with a filter, each kind's fields generated from the schema, a length
//! kept in the unit it was typed in, and Reset, Save As… and Load… for one kind or all.

use std::path::Path;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::bridge::Bridge;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::preferences;
use reda_ui::preferences_dialog::{Kind, Outcome, PageId, PreferencesDialog, defaults_fields};
use reda_ui::settings::FileRequest;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn shell(home: &Path) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    Shell::new(engine)
}

fn value(shell: &Shell, key: &str) -> Value {
    preferences::value_of(shell.preferences(), key).unwrap().1
}

/// Each heading of the kinds list with its kinds' titles.
fn listed(d: &PreferencesDialog, shell: &Shell) -> Vec<(String, Vec<String>)> {
    d.defaults_groups(shell)
        .into_iter()
        .map(|g| (g.title, g.kinds.into_iter().map(|k| k.title).collect()))
        .collect()
}

fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn the_kinds_are_listed_by_editor_and_the_filter_narrows_them() {
    let home = TempDir::new("defaults-page-kinds");
    let shell = shell(home.path());
    let mut d = PreferencesDialog::on(PageId::DEFAULTS);
    let shapes = ["Line", "Rectangle", "Arc", "Circle", "Polygon"];
    let mut board_and_footprint = owned(&shapes);
    board_and_footprint.push("Text".into());
    let mut symbol = owned(&["Pin"]);
    symbol.extend(owned(&shapes));
    assert_eq!(
        listed(&d, &shell),
        [
            ("PCB and Footprint".to_owned(), board_and_footprint.clone()),
            (
                "PCB".to_owned(),
                owned(&["Dimension", "Free Pad", "Zone", "Keepout", "Via"])
            ),
            ("Footprint".to_owned(), owned(&["Pad", "Text"])),
            ("Symbol".to_owned(), symbol),
            ("Symbol and Schematic".to_owned(), owned(&["Text"])),
            ("Schematic".to_owned(), owned(&["Rectangle", "Port"])),
        ]
    );
    // By kind name.
    d.defaults.filter = "pad".into();
    assert_eq!(
        listed(&d, &shell),
        [
            ("PCB".to_owned(), owned(&["Free Pad"])),
            ("Footprint".to_owned(), owned(&["Pad"])),
        ]
    );
    // By a field's Layer kind: only the kinds with a set per Layer kind.
    d.defaults.filter = "Courtyard".into();
    assert_eq!(
        listed(&d, &shell),
        [("PCB and Footprint".to_owned(), board_and_footprint)]
    );
    // By a field.
    d.defaults.filter = "teardrop".into();
    assert_eq!(listed(&d, &shell), [("PCB".to_owned(), owned(&["Via"]))]);
    assert_eq!(d.defaults_kind(&shell).unwrap().id, "pcb.via");
    d.defaults.filter = "nothing like it".into();
    assert!(listed(&d, &shell).is_empty());
}

#[test]
fn a_kinds_fields_are_its_records_leaves_with_their_kinds_from_the_schema() {
    let kind = |id: &str| preferences::defaults_kind(id).unwrap();
    let rows = |id: &str| -> Vec<(String, Kind, String, Option<String>)> {
        defaults_fields(kind(id))
            .into_iter()
            .map(|s| (s.key, s.kind, s.label, s.heading))
            .collect()
    };
    let length = Kind::Length {
        area: false,
        optional: false,
    };
    let line: Vec<_> = rows("pcb.line");
    assert_eq!(
        line.iter()
            .map(|(k, kind, label, heading)| (k.as_str(), kind, label.as_str(), heading.as_deref()))
            .collect::<Vec<_>>(),
        [
            (
                "defaults.pcb.line.assembly.width",
                &length,
                "Width",
                Some("Assembly")
            ),
            (
                "defaults.pcb.line.copper.width",
                &length,
                "Width",
                Some("Copper")
            ),
            (
                "defaults.pcb.line.courtyard.width",
                &length,
                "Width",
                Some("Courtyard")
            ),
            (
                "defaults.pcb.line.mechanical.width",
                &length,
                "Width",
                Some("Mechanical")
            ),
            (
                "defaults.pcb.line.silkscreen.width",
                &length,
                "Width",
                Some("Silkscreen")
            ),
        ]
    );
    let pad: Vec<(String, Kind)> = rows("footprint.pad")
        .into_iter()
        .map(|(k, kind, _, heading)| {
            assert_eq!(heading, None, "{k}");
            (
                k.trim_start_matches("defaults.footprint.pad.").to_owned(),
                kind,
            )
        })
        .collect();
    let optional = Kind::Length {
        area: false,
        optional: true,
    };
    let find = |name: &str| pad.iter().find(|(n, _)| n == name).map(|(_, k)| k.clone());
    assert_eq!(pad.len(), 9);
    assert_eq!(find("width"), Some(length.clone()));
    assert_eq!(find("hole_height"), Some(length.clone()));
    assert_eq!(find("mask_margin"), Some(optional.clone()));
    assert_eq!(find("chamfer_ratio"), Some(Kind::Percent));
    assert!(matches!(find("kind"), Some(Kind::Choice(c)) if c.len() == 3));
    assert!(matches!(find("shape"), Some(Kind::Choice(c)) if c.len() == 5));

    let zone = rows("pcb.zone");
    let area = zone
        .iter()
        .find(|(k, ..)| k == "defaults.pcb.zone.min_island_area")
        .unwrap();
    assert_eq!(
        area.1,
        Kind::Length {
            area: true,
            optional: false
        }
    );
    let clearance = zone
        .iter()
        .find(|(k, ..)| k == "defaults.pcb.zone.clearance")
        .unwrap();
    assert_eq!(clearance.1, optional);
    let restrict = rows("pcb.keepout")
        .into_iter()
        .find(|(k, ..)| k == "defaults.pcb.keepout.restrict")
        .unwrap();
    let Kind::Flags(choices) = restrict.1 else {
        panic!("{restrict:?}")
    };
    let mut labels: Vec<&str> = choices.iter().map(|c| c.label.as_str()).collect();
    labels.sort_unstable();
    assert_eq!(
        labels,
        [
            "Footprints",
            "SMD Pads",
            "THT Pads",
            "Tracks",
            "Vias",
            "Zones"
        ]
    );
    let text: Vec<_> = rows("pcb.text")
        .into_iter()
        .filter_map(|(k, _, label, heading)| {
            (heading.as_deref() == Some("Justify")).then_some((k, label))
        })
        .collect();
    assert_eq!(
        text,
        [
            (
                "defaults.pcb.text.justify.h".to_owned(),
                "Justify".to_owned()
            ),
            (
                "defaults.pcb.text.justify.v".to_owned(),
                "Vertical Justify".to_owned()
            ),
        ]
    );
}

#[test]
fn a_length_typed_on_the_page_keeps_the_unit_it_was_typed_in() {
    let home = TempDir::new("defaults-page-unit");
    let mut shell = shell(home.path());
    let mut d = PreferencesDialog::on(PageId::DEFAULTS);
    let pin = "defaults.symbol.pin.length";
    // Shown in the unit it ships in.
    assert_eq!(d.text(&shell, pin), "100 mil");
    d.commit_text(&mut shell, pin, "0.25 in");
    assert_eq!(value(&shell, pin), json!("0.25in"));
    assert_eq!(d.text(&shell, pin), "0.25 in");
    // A bare number is in the editor's default units, and keeps them.
    d.commit_text(&mut shell, pin, "3");
    assert_eq!(value(&shell, pin), json!("3mm"));
    shell
        .run(
            "app.preferences.set",
            json!({"key": "pcb.default_units", "value": "mil"}),
        )
        .unwrap();
    let width = "defaults.pcb.line.silkscreen.width";
    d.commit_text(&mut shell, width, "8");
    assert_eq!(value(&shell, width), json!("8mil"));
    assert_eq!(d.text(&shell, width), "8 mil");
    let prefs = std::fs::read_to_string(UserDirs::under(home.path()).preferences_file()).unwrap();
    assert!(prefs.contains("width = \"8mil\""), "{prefs}");
    // An area too, and blank unsets an optional length.
    let island = "defaults.pcb.zone.min_island_area";
    d.commit_text(&mut shell, island, "0.5 mm²");
    assert_eq!(value(&shell, island), json!("0.5mm2"));
    assert_eq!(d.text(&shell, island), "0.5 mm²");
    let clearance = "defaults.pcb.zone.clearance";
    d.commit_text(&mut shell, clearance, "0.3mm");
    assert_eq!(value(&shell, clearance), json!("0.3mm"));
    d.commit_text(&mut shell, clearance, "");
    assert_eq!(value(&shell, clearance), Value::Null);
    // A Pad's chamfer is typed in percent.
    let chamfer = "defaults.footprint.pad.chamfer_ratio";
    assert_eq!(d.text(&shell, chamfer), "50 %");
    d.commit_text(&mut shell, chamfer, "25 %");
    assert_eq!(value(&shell, chamfer), json!(25_000));
    // Text that is no length is refused and kept in the field.
    d.commit_text(&mut shell, width, "wide");
    assert!(d.problem(width).is_some());
    assert_eq!(value(&shell, width), json!("8mil"));
}

fn last_command(shell: &Shell) -> (String, Value) {
    let entry = shell.engine().journal().last().unwrap();
    (entry.id.clone(), entry.params.clone())
}

#[test]
fn reset_save_and_load_run_the_defaults_commands_for_a_kind_or_all() {
    let home = TempDir::new("defaults-page-file");
    let work = TempDir::new("defaults-page-file-out");
    let mut shell = shell(home.path());
    let mut d = PreferencesDialog::on(PageId::DEFAULTS);
    let pin = "defaults.symbol.pin.length";
    d.commit_text(&mut shell, pin, "200mil");

    d.choose_defaults_kind("symbol.pin");
    assert_eq!(d.defaults_kind(&shell).unwrap().id, "symbol.pin");
    let pin_row = |d: &PreferencesDialog, shell: &Shell| {
        d.defaults_groups(shell)
            .into_iter()
            .flat_map(|g| g.kinds)
            .find(|k| k.id == "symbol.pin")
            .unwrap()
            .modified
    };
    assert_eq!(pin_row(&d, &shell), 1);
    d.reset_defaults(&mut shell, Some("symbol.pin"));
    assert_eq!(
        last_command(&shell),
        ("app.defaults.reset".into(), json!({"kind": "symbol.pin"}))
    );
    assert_eq!(value(&shell, pin), json!("100mil"));
    assert_eq!(pin_row(&d, &shell), 0);
    d.reset_defaults(&mut shell, None);
    assert_eq!(
        last_command(&shell),
        ("app.defaults.reset".into(), json!({}))
    );

    d.commit_text(&mut shell, pin, "150mil");
    let file = work.path().join("pin.toml");
    assert_eq!(
        d.save_defaults_as(Some("symbol.pin")),
        FileRequest::SaveDefaults
    );
    d.picked_file(&mut shell, FileRequest::SaveDefaults, &file);
    assert_eq!(
        last_command(&shell),
        (
            "app.defaults.save".into(),
            json!({"path": file, "kind": "symbol.pin"})
        )
    );
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(saved.contains("[defaults.symbol.pin]"), "{saved}");
    assert!(!saved.contains("[defaults.pcb"), "{saved}");
    let all = work.path().join("all.toml");
    d.save_defaults_as(None);
    d.picked_file(&mut shell, FileRequest::SaveDefaults, &all);
    assert_eq!(
        last_command(&shell),
        ("app.defaults.save".into(), json!({"path": all}))
    );

    let shared = work.path().join("shared.toml");
    std::fs::write(
        &shared,
        "[defaults.symbol.pin]\nlength = \"250mil\"\ncolour = \"red\"\n\
         [defaults.symbol.wire]\nwidth = \"1mm\"\n",
    )
    .unwrap();
    d.picked_file(&mut shell, FileRequest::LoadDefaults, &shared);
    assert_eq!(
        last_command(&shell),
        ("app.defaults.load".into(), json!({"path": shared}))
    );
    assert_eq!(value(&shell, pin), json!("250mil"));
    let said = "Loaded 1 kind of Object Defaults; 2 entries skipped.";
    assert_eq!(d.defaults.message.as_deref(), Some(said));
    assert_eq!(shell.note(), Some(said));
}

/// The dialog alone in a window, keeping what it last asked for.
struct Page {
    dialog: PreferencesDialog,
    shell: Shell,
    bridge: Bridge,
    asked: Option<Outcome>,
}

fn page(home: &Path) -> Harness<'static, Page> {
    let page = Page {
        dialog: PreferencesDialog::on(PageId::DEFAULTS),
        shell: shell(home),
        bridge: Bridge::off(),
        asked: None,
    };
    let mut h = Harness::builder()
        .with_size([1100.0, 800.0])
        .build_ui_state(
            |ui, p: &mut Page| {
                let out = p.dialog.show(ui, &mut p.shell, &mut p.bridge);
                if out != Outcome::Open {
                    p.asked = Some(out);
                }
            },
            page,
        );
    h.run();
    h
}

#[test]
fn the_page_shows_a_chosen_kinds_fields_and_asks_for_files() {
    let home = TempDir::new("defaults-page-window");
    let mut h = page(home.path());
    assert!(
        h.query_by_role_and_label(Role::CheckBox, "Remember last used")
            .is_some()
    );
    h.get_by_label("Pin").scroll_to_me();
    h.run();
    h.get_by_label("Pin").click();
    h.run();
    h.run();
    let length = h.get_by_role_and_label(Role::TextInput, "Length");
    assert_eq!(length.accesskit_node().value().as_deref(), Some("100 mil"));
    h.get_by_role_and_label(Role::TextInput, "Length")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Length").click();
    h.run();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Length")
        .type_text("2.54 mm");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    let shell = &h.state().shell;
    let journal: Vec<_> = shell
        .engine()
        .journal()
        .iter()
        .map(|e| (&e.id, &e.params))
        .collect();
    assert_eq!(
        value(shell, "defaults.symbol.pin.length"),
        json!("2.54mm"),
        "{journal:?} {:?}",
        h.state().dialog.problem("defaults.symbol.pin.length")
    );
    assert!(h.query_by_label("Pin (1)").is_some());

    h.get_by_role_and_label(Role::Button, "Save As…").click();
    h.run();
    assert_eq!(
        h.state().asked,
        Some(Outcome::PickFile(FileRequest::SaveDefaults))
    );
    h.get_by_role_and_label(Role::Button, "Load…").click();
    h.run();
    assert_eq!(
        h.state().asked,
        Some(Outcome::PickFile(FileRequest::LoadDefaults))
    );
    h.get_by_role_and_label(Role::Button, "Reset").click();
    h.run();
    let shell = &h.state().shell;
    assert_eq!(value(shell, "defaults.symbol.pin.length"), json!("100mil"));
    assert_eq!(
        shell.engine().journal().last().map(|e| e.id.as_str()),
        Some("app.defaults.reset")
    );

    // The filter narrows the list.
    h.get_by_role_and_label(Role::SearchInput, "Filter kinds")
        .click();
    h.run();
    h.get_by_role_and_label(Role::SearchInput, "Filter kinds")
        .type_text("keepout");
    h.run();
    assert!(h.query_by_label("Pin").is_none());
    assert!(
        h.query_by_role_and_label(Role::CheckBox, "SMD Pads")
            .is_some()
    );
}
