//! The Part editor on a Shell, without a window: an open Part is a Document tab with no
//! canvas, and its controls turn edits into the `library.part.*` Commands: the Symbol and
//! the ordered alternate Footprints, the pin-map overrides, the fields and the Component
//! Type. Import part and the Footprint generators report their Library check in Messages.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{ComponentType, Field, LibraryElement, Part, PinPads, Uuid};
use reda_ui::keymap::{self, Chord};
use reda_ui::layout::TabKey;
use reda_ui::part_editor::{self, FootprintMove, PinRow};
use reda_ui::shell::Shell;
use reda_ui::{bars, element_editor};
use serde_json::{Value, json};

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";
/// The Demo Library's OpAmp_Dual Symbol (Pins 1–8), SOIC-8 Footprint (Pads 1, 4, 4),
/// MountingHole_M3 Footprint and LM358 Part.
const OPAMP: &str = "00000010-0000-4000-8000-000000000000";
const SOIC8: &str = "00000030-0000-4000-8000-000000000000";
const LM358: &str = "00000050-0000-4000-8000-000000000000";

fn demo_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-engine/tests/golden/library/Demo")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reda-import-kicad/tests/fixtures")
        .join(name)
}

/// A Shell with no Project open, the Demo Library as its (read-only) starter Library and
/// an empty user Library "Mine" registered in the Preferences.
fn shell(home: &TempDir) -> Shell {
    let lib = home.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let info = reda_ui::engine::model::LibraryInfo {
        uuid: LIBRARY.parse().unwrap(),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(
        lib.join("Mine.redalib"),
        reda_ui::engine::format::library::write_library_info(&info).unwrap(),
    )
    .unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&home.path().join("home"))))
        .starter_library(Some(demo_library()))
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

fn new_part(shell: &mut Shell) -> Uuid {
    let out = shell
        .run(
            "library.element.new",
            json!({"library": LIBRARY, "kind": "part", "name": "Amp"}),
        )
        .unwrap();
    out["element"].as_str().unwrap().parse().unwrap()
}

fn part(shell: &Shell, uuid: Uuid) -> Part {
    match shell.engine().element(uuid) {
        Some(LibraryElement::Part(p)) => p.clone(),
        other => panic!("no open Part: {other:?}"),
    }
}

/// Runs what an editor control asked for, as the window does after a frame.
fn apply(shell: &mut Shell, invocation: Option<(String, Value)>) {
    let (id, params) = invocation.expect("the edit asks for a Command");
    shell.run(&id, params).unwrap();
}

/// A Part on the OpAmp Symbol with the SOIC-8 Footprint.
fn opamp_part(shell: &mut Shell) -> Uuid {
    let uuid = new_part(shell);
    shell
        .run(
            "library.part.symbol.set",
            json!({"part": uuid, "symbol": OPAMP}),
        )
        .unwrap();
    let p = part(shell, uuid);
    apply(
        shell,
        part_editor::add_footprint(&p, SOIC8.parse().unwrap()),
    );
    uuid
}

#[test]
fn a_part_opens_in_a_tab_with_no_canvas() {
    let home = TempDir::new("part-tab");
    let mut shell = shell(&home);
    let uuid = new_part(&mut shell);
    assert!(shell.layout().place(&TabKey::Document(uuid)).is_some());
    assert_eq!(shell.layout().active_document(), Some(uuid));
    assert_eq!(shell.document_tab(uuid).unwrap().title, "Amp");
    assert!(!element_editor::has_canvas(shell.engine(), uuid));
    // A Symbol or Footprint tab does have one.
    let out = shell
        .run(
            "library.element.new",
            json!({"library": LIBRARY, "kind": "symbol", "name": "S"}),
        )
        .unwrap();
    let symbol = out["element"].as_str().unwrap().parse().unwrap();
    assert!(element_editor::has_canvas(shell.engine(), symbol));
}

#[test]
fn mod_s_on_a_part_tab_saves_the_part() {
    let home = TempDir::new("part-save-key");
    let mut shell = shell(&home);
    let uuid = new_part(&mut shell);
    let active = keymap::active(&shell);
    let key = Chord::parse("Mod+S").unwrap();
    let action = shell.keymap().lookup(&key, &active).unwrap().clone();
    let (id, params) = bars::resolve_for(&shell, &action, Some(uuid)).run.unwrap();
    assert_eq!(id, "library.element.save");
    assert_eq!(params["element"], json!(uuid));
    shell.run(&id, params).unwrap();
}

#[test]
fn the_symbol_and_footprints_come_from_the_registered_libraries() {
    let home = TempDir::new("part-choices");
    let mut shell = shell(&home);
    let symbols = part_editor::symbol_choices(&mut shell);
    assert_eq!(
        symbols
            .iter()
            .map(|c| (c.uuid.to_string(), c.label.as_str()))
            .collect::<Vec<_>>(),
        [(OPAMP.to_owned(), "Demo / OpAmp_Dual")]
    );
    let footprints = part_editor::footprint_choices(&mut shell);
    let labels: Vec<_> = footprints.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["Demo / MountingHole_M3", "Demo / SOIC-8"]);
    // The Part editor's previews read them, open ones with their unsaved edits.
    let soic = part_editor::footprint(&mut shell, SOIC8.parse().unwrap()).unwrap();
    // The Demo SOIC-8 is a stub with Pads 1, 4 and 4.
    assert_eq!(soic.pads.len(), 3);
    assert_eq!(
        part_editor::symbol(&mut shell, OPAMP.parse().unwrap())
            .unwrap()
            .name,
        "OpAmp_Dual"
    );
}

#[test]
fn the_alternate_footprints_are_added_ordered_and_removed() {
    let home = TempDir::new("part-footprints");
    let mut shell = shell(&home);
    let uuid = opamp_part(&mut shell);
    let hole: Uuid = "00000040-0000-4000-8000-000000000000".parse().unwrap();
    let soic: Uuid = SOIC8.parse().unwrap();
    // Adding one already listed asks for nothing.
    assert!(part_editor::add_footprint(&part(&shell, uuid), soic).is_none());
    let p = part(&shell, uuid);
    apply(&mut shell, part_editor::add_footprint(&p, hole));
    assert_eq!(part(&shell, uuid).footprints, [soic, hole]);
    // Moving the second up makes it the default.
    let p = part(&shell, uuid);
    apply(
        &mut shell,
        part_editor::move_footprint(&p, 1, FootprintMove::Up),
    );
    assert_eq!(part(&shell, uuid).footprints, [hole, soic]);
    let p = part(&shell, uuid);
    assert!(part_editor::move_footprint(&p, 0, FootprintMove::Up).is_none());
    assert!(part_editor::move_footprint(&p, 1, FootprintMove::Down).is_none());
    apply(
        &mut shell,
        part_editor::move_footprint(&p, 0, FootprintMove::Remove),
    );
    assert_eq!(part(&shell, uuid).footprints, [soic]);
    // Each was one step on the Part's own undo stack.
    shell.run("edit.undo", json!({"document": uuid})).unwrap();
    assert_eq!(part(&shell, uuid).footprints, [hole, soic]);
}

#[test]
fn the_pin_map_lists_each_pin_with_its_pads_and_edits_overrides() {
    let home = TempDir::new("part-pin-map");
    let mut shell = shell(&home);
    let uuid = opamp_part(&mut shell);
    let rows = |shell: &mut Shell| {
        let p = part(shell, uuid);
        let symbol = part_editor::symbol(shell, OPAMP.parse().unwrap());
        part_editor::pin_rows(&p, symbol.as_ref())
    };
    let first = rows(&mut shell);
    assert_eq!(first.len(), 8);
    assert_eq!(
        first[0],
        PinRow {
            number: "1".into(),
            name: "OUT".into(),
            pads: vec!["1".into()],
            overridden: false,
        }
    );
    let numbers: Vec<_> = first.iter().map(|r| r.number.as_str()).collect();
    assert_eq!(numbers, ["1", "2", "3", "4", "5", "6", "7", "8"]);

    // Pin 4 reaches Pads 4 and 9, typed as a list.
    let p = part(&shell, uuid);
    apply(&mut shell, part_editor::set_pin_pads(&p, "4", " 4, 9 "));
    assert_eq!(
        part(&shell, uuid).pin_map.pins,
        [PinPads {
            pin: "4".into(),
            pads: vec!["4".into(), "9".into()]
        }]
    );
    let row = rows(&mut shell).remove(3);
    assert!(row.overridden);
    assert_eq!(row.pads, ["4", "9"]);
    // Typing its own number (or nothing) restores the implicit map.
    let p = part(&shell, uuid);
    assert!(
        part_editor::set_pin_pads(&p, "4", "4, 9").is_none(),
        "unchanged"
    );
    apply(&mut shell, part_editor::set_pin_pads(&p, "4", ""));
    assert!(part(&shell, uuid).pin_map.is_empty());

    // Pin 1 moved to Pad 2 leaves Pad 1 to no Pin, which can be marked unconnected.
    let p = part(&shell, uuid);
    apply(&mut shell, part_editor::set_pin_pads(&p, "1", "2"));
    let p = part(&shell, uuid);
    let footprints = vec![part_editor::footprint(&mut shell, SOIC8.parse().unwrap()).unwrap()];
    let symbol = part_editor::symbol(&mut shell, OPAMP.parse().unwrap());
    assert_eq!(
        part_editor::unmapped_pads(&p, symbol.as_ref(), &footprints),
        ["1"]
    );
    apply(&mut shell, part_editor::set_unconnected(&p, "1", true));
    let p = part(&shell, uuid);
    assert_eq!(p.pin_map.unconnected, ["1"]);
    // Still listed, so it can be unmarked.
    assert_eq!(
        part_editor::unmapped_pads(&p, symbol.as_ref(), &footprints),
        ["1"]
    );
    apply(&mut shell, part_editor::set_unconnected(&p, "1", false));
    assert!(part(&shell, uuid).pin_map.unconnected.is_empty());
}

#[test]
fn free_form_fields_are_added_renamed_and_removed() {
    let home = TempDir::new("part-fields");
    let mut shell = shell(&home);
    let uuid = new_part(&mut shell);
    let p = part(&shell, uuid);
    apply(&mut shell, Some(part_editor::add_field(&p)));
    assert_eq!(
        part(&shell, uuid).fields.custom,
        [Field {
            name: "Field 1".into(),
            value: String::new()
        }]
    );
    let p = part(&shell, uuid);
    apply(
        &mut shell,
        part_editor::set_field(&p, 0, Some("LCSC"), None),
    );
    let p = part(&shell, uuid);
    apply(
        &mut shell,
        part_editor::set_field(&p, 0, None, Some("C7950")),
    );
    assert_eq!(
        part(&shell, uuid).fields.custom,
        [Field {
            name: "LCSC".into(),
            value: "C7950".into()
        }]
    );
    let p = part(&shell, uuid);
    assert!(part_editor::set_field(&p, 0, Some("LCSC"), None).is_none());
    apply(&mut shell, Some(part_editor::remove_field(&p, 0)));
    assert!(part(&shell, uuid).fields.custom.is_empty());
    // The built-in fields and the Component Type are one Command each.
    let p = part(&shell, uuid);
    apply(&mut shell, part_editor::set_builtin(&p, "mpn", "LM358DR"));
    let p = part(&shell, uuid);
    apply(
        &mut shell,
        part_editor::set_component_type(&p, ComponentType::Mechanical),
    );
    let p = part(&shell, uuid);
    assert_eq!(p.fields.mpn, "LM358DR");
    assert_eq!(p.component_type, ComponentType::Mechanical);
    assert!(part_editor::set_builtin(&p, "mpn", "LM358DR").is_none());
}

#[test]
fn a_starter_part_opens_read_only_with_its_overrides() {
    let home = TempDir::new("part-starter");
    let mut shell = shell(&home);
    shell
        .run("library.element.open", json!({"element": LM358}))
        .unwrap();
    let uuid: Uuid = LM358.parse().unwrap();
    assert!(element_editor::read_only(shell.engine(), uuid));
    let p = part(&shell, uuid);
    let symbol = part_editor::symbol(&mut shell, OPAMP.parse().unwrap());
    let rows = part_editor::pin_rows(&p, symbol.as_ref());
    assert_eq!(rows[3].pads, ["4", "9"]);
    assert!(rows[3].overridden);
    // Its second Footprint is in no registered Library: the editor says so.
    assert!(part_editor::footprint(&mut shell, p.footprints[1]).is_none());
}

#[test]
fn import_part_puts_its_library_check_in_messages_and_shows_the_notice_once() {
    let home = TempDir::new("part-import");
    let mut shell = shell(&home);
    let params = json!({
        "library": LIBRARY,
        "symbol_file": fixture("divider.kicad_sym"),
        "footprint_file": fixture("divider.kicad_mod"),
    });
    shell.run("library.import_part", params).unwrap();
    let notice = shell
        .take_notice()
        .expect("the first import shows the notice");
    assert!(notice.contains("terms"), "{notice}");
    assert!(shell.take_notice().is_none());
    let checked = shell
        .messages()
        .notes()
        .iter()
        .filter(|n| n.source == element_editor::LIBRARY_CHECK)
        .count();
    let clean = shell.note().is_some_and(|n| n.contains("no problems"));
    assert!(checked > 0 || clean, "the check reports either way");

    // A second import (under another name) does not show it again.
    let again = json!({
        "library": LIBRARY,
        "symbol_file": fixture("dual_switch.kicad_sym"),
        "footprint_file": fixture("dual_switch.kicad_mod"),
    });
    shell.run("library.import_part", again).unwrap();
    assert!(shell.take_notice().is_none());
}

#[test]
fn a_generated_footprint_is_checked_into_messages() {
    let home = TempDir::new("part-generate");
    let mut shell = shell(&home);
    shell.set_notes(
        element_editor::LIBRARY_CHECK,
        vec![reda_ui::messages::Note {
            source: element_editor::LIBRARY_CHECK.into(),
            document: None,
            severity: reda_ui::engine::model::Severity::Warning,
            text: "stale".into(),
        }],
    );
    shell
        .run(
            "library.footprint.generate.chip",
            json!({"library": LIBRARY, "size": "0603"}),
        )
        .unwrap();
    // The chip has no problems: the old lines go and the status bar says so.
    assert!(
        !shell
            .messages()
            .notes()
            .iter()
            .any(|n| n.source == element_editor::LIBRARY_CHECK)
    );
    assert_eq!(
        shell.note(),
        Some("Library check: no problems in RESC1608X55N")
    );
}
