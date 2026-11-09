//! Update from Library, Replace Part and the PCB's update as dialog models on a Shell,
//! without a window: page 1's sheets, Parts, Selected Parts Only, aspects and settings;
//! page 2's rows, aspect ticks, field grid with Update and Reject Selected, Choose
//! Component and Return Selected to Default; the entry points' scopes; the local edits an
//! update removes; and the Project Settings summary with Reset.

use std::path::{Path, PathBuf};

use reda_testkit::{TempDir, starter_library};
use reda_ui::bars::{Request, take_requests};
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::schematic::{Aspect, LibraryStatus};
use reda_ui::pcb::update_dialog::BoardUpdateDialog;
use reda_ui::schematic_dialogs::update::edited_footprint;
use reda_ui::schematic_dialogs::{
    LibraryDialog, Page, ReplaceDialog, SchematicDialog, UpdateDialog, UpdateScope,
};
use reda_ui::settings::Page as _;
use reda_ui::settings::project::SchematicPage;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
const R_SYMBOL: &str = "a0000000-0000-4000-8000-000000000010";

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A Project on a writable copy of `source` (the hand-written test Library by default).
struct Fixture {
    library: PathBuf,
    shell: Shell,
    _dirs: (TempDir, TempDir),
}

fn fixture_from(label: &str, source: &Path) -> Fixture {
    let lib_dir = TempDir::new(&format!("{label}-lib"));
    let library = lib_dir.path().join("Lib");
    copy_dir(source, &library);
    let dir = TempDir::new(label);
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library.clone()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .new_project(&dir.path().join("project"), "Demo")
        .unwrap();
    Fixture {
        library,
        shell,
        _dirs: (lib_dir, dir),
    }
}

fn fixture(label: &str) -> Fixture {
    fixture_from(
        label,
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test"),
    )
}

impl Fixture {
    /// Places `part`; returns (Unit, Component).
    fn place(&mut self, part: &str, x: i64) -> (Uuid, Uuid) {
        let out = self
            .shell
            .run(
                "schematic.component.place",
                json!({"document": SHEET, "part": part,
                       "at": {"x": format!("{x}mil"), "y": "0mil"}}),
            )
            .unwrap();
        (
            uuid(&out["result"]["unit"]),
            uuid(&out["result"]["component"]["uuid"]),
        )
    }

    fn edit_library(&self, file: &str, from: &str, to: &str) {
        let path = self.library.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} has no {from:?}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
    }

    /// R_10k's Value becomes 22k in the Library.
    fn change_value(&self) {
        self.edit_library(
            "parts/R_10k.redapart",
            "\"value\":\"10k\"",
            "\"value\":\"22k\"",
        );
    }

    fn unit(&self, unit: Uuid) -> Value {
        let project = self.shell.engine().project().unwrap();
        let sheet = project.schematic_sheet(uuid(&json!(SHEET))).unwrap();
        serde_json::to_value(sheet.unit(unit).unwrap()).unwrap()
    }

    fn value(&self, unit: Uuid) -> String {
        self.unit(unit)["fields"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    fn dialog(&mut self, scope: UpdateScope) -> UpdateDialog {
        UpdateDialog::new(&mut self.shell, scope)
    }
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

fn u(s: &str) -> Uuid {
    s.parse().unwrap()
}

#[test]
fn page_one_lists_sheets_and_parts_and_page_two_previews_before_anything_changes() {
    let mut f = fixture("update-pages");
    let (r1, _) = f.place(R_10K, 0);
    f.place(R_10K, 1000);
    f.place(C_100N, 2000);
    f.change_value();

    let mut dialog = f.dialog(UpdateScope::default());
    assert_eq!(dialog.page(), Page::Setup);
    assert_eq!(dialog.sheets.len(), 1);
    assert!(dialog.sheets[0].on);
    let parts: Vec<(&str, usize, usize, bool)> = (dialog.parts.iter())
        .map(|p| (p.name.as_str(), p.count, p.outdated, p.on))
        .collect();
    assert_eq!(parts, [("C_100n", 1, 0, true), ("R_10k", 2, 2, true)]);
    // No selection: Selected Parts Only can't be ticked. Full Replace is the default.
    assert!(!dialog.has_selection() && !dialog.selected_only);
    assert_eq!(dialog.aspects.len(), 3);
    // All Off leaves nothing to preview.
    dialog.all_sheets(false);
    assert!(!dialog.sheets[0].on);
    dialog.all_sheets(true);

    dialog.next(&mut f.shell);
    assert_eq!(dialog.page(), Page::Preview);
    let rows: Vec<&str> = dialog.rows().iter().map(|r| r.designator.as_str()).collect();
    assert_eq!(rows, ["R1", "R2"]);
    let r = &dialog.rows()[0];
    assert_eq!(r.status, LibraryStatus::LibraryDiffers);
    assert_eq!(r.differs, [Aspect::Fields]);
    assert!(r.included);
    let value = r.cells.iter().find(|c| c.cell == "value").unwrap();
    assert_eq!(
        (value.old.as_str(), value.new.as_str(), value.case.as_str()),
        ("10k", "22k", "changed")
    );
    assert_eq!(dialog.action(r.component, value), "update");
    // Nothing changes before Apply.
    assert_eq!(f.value(r1), "10k");
    assert!(dialog.can_apply());
    dialog.apply(&mut f.shell).unwrap();
    assert_eq!(f.value(r1), "22k");
}

#[test]
fn unticking_a_part_or_a_component_leaves_it_out() {
    let mut f = fixture("update-untick");
    let (r1, _) = f.place(R_10K, 0);
    let (r2, c2) = f.place(R_10K, 1000);
    f.change_value();

    let mut dialog = f.dialog(UpdateScope::default());
    dialog.parts.iter_mut().for_each(|p| p.on = false);
    dialog.next(&mut f.shell);
    assert!(dialog.rows().is_empty());
    assert!(!dialog.can_apply());

    dialog.back();
    dialog.parts.iter_mut().for_each(|p| p.on = true);
    dialog.next(&mut f.shell);
    dialog.set_included(c2, false);
    dialog.apply(&mut f.shell).unwrap();
    assert_eq!((f.value(r1), f.value(r2)), ("22k".into(), "10k".into()));
}

#[test]
fn selected_parts_only_follows_the_selection_it_opened_on() {
    let mut f = fixture("update-selected-only");
    let (r1, c1) = f.place(R_10K, 0);
    let (r2, _) = f.place(R_10K, 1000);
    f.change_value();
    let project = f.shell.engine().project().unwrap();
    let scope = UpdateScope::of_selection(project, &[r1]);
    assert_eq!(scope.components, [c1]);

    let mut dialog = f.dialog(scope);
    assert!(dialog.has_selection() && dialog.selected_only);
    dialog.next(&mut f.shell);
    let rows: Vec<&str> = dialog.rows().iter().map(|r| r.designator.as_str()).collect();
    assert_eq!(rows, ["R1"]);
    dialog.apply(&mut f.shell).unwrap();
    assert_eq!((f.value(r1), f.value(r2)), ("22k".into(), "10k".into()));
}

#[test]
fn update_and_reject_selected_override_cells_but_not_locked_ones() {
    let mut f = fixture("update-cells");
    let (r1, c1) = f.place(R_10K, 0);
    let (r2, c2) = f.place(R_10K, 1000);
    // R1 holds a value of its own; R2's Value is locked.
    f.shell
        .run(
            "schematic.component.set",
            json!({"components": [c1], "value": "4k7"}),
        )
        .unwrap();
    f.shell
        .run(
            "schematic.component.set",
            json!({"components": [c2], "locked": {"value": true}}),
        )
        .unwrap();
    f.change_value();

    let mut dialog = f.dialog(UpdateScope::default());
    dialog.next(&mut f.shell);
    let cell = |d: &UpdateDialog, c: Uuid| {
        d.row(c)
            .unwrap()
            .cells
            .iter()
            .find(|x| x.cell == "value")
            .unwrap()
            .clone()
    };
    let edited = cell(&dialog, c1);
    assert_eq!(edited.case, "edited");
    assert_eq!(dialog.action(c1, &edited), "reject");
    let locked = cell(&dialog, c2);
    assert_eq!(locked.case, "locked");
    assert!(!locked.overridable);

    dialog.select_cell(c1, "value", false);
    dialog.select_cell(c2, "value", true);
    assert!(dialog.cell_selected(c1, "value") && dialog.cell_selected(c2, "value"));
    dialog.update_selected();
    assert_eq!(dialog.action(c1, &edited), "update");
    assert_eq!(dialog.action(c2, &locked), "none");
    dialog.reject_selected();
    assert_eq!(dialog.action(c1, &edited), "reject");
    dialog.update_selected();

    dialog.apply(&mut f.shell).unwrap();
    assert_eq!((f.value(r1), f.value(r2)), ("22k".into(), "10k".into()));
}

#[test]
fn a_component_takes_its_own_aspects_until_returned_to_default() {
    let mut f = fixture("update-aspects");
    let (r1, c1) = f.place(R_10K, 0);
    f.change_value();
    let mut dialog = f.dialog(UpdateScope::default());
    dialog.next(&mut f.shell);
    assert_eq!(dialog.row(c1).unwrap().aspects, [Aspect::Fields]);

    dialog.set_component_aspect(&mut f.shell, c1, Aspect::Fields, false);
    let row = dialog.row(c1).unwrap();
    assert!(row.aspects.is_empty() && row.cells.is_empty());
    assert!(dialog.has_own_aspects(c1));
    assert!(!dialog.can_apply());

    dialog.select_row(c1, false);
    assert_eq!(dialog.selected_rows(), [c1]);
    dialog.return_selected_to_default(&mut f.shell);
    assert!(!dialog.has_own_aspects(c1));
    assert_eq!(dialog.row(c1).unwrap().aspects, [Aspect::Fields]);
    dialog.apply(&mut f.shell).unwrap();
    assert_eq!(f.value(r1), "22k");
}

#[test]
fn page_one_settings_shape_the_preview_and_are_saved_on_apply() {
    let mut f = fixture("update-settings");
    let (r1, c1) = f.place(R_10K, 0);
    f.shell
        .run(
            "schematic.component.set",
            json!({"components": [c1], "value": "4k7"}),
        )
        .unwrap();
    f.change_value();
    let mut dialog = f.dialog(UpdateScope::default());
    assert!(dialog.field_names.contains(&"value".to_owned()));
    dialog.settings.locally_edited =
        reda_ui::engine::model::library_update::LocallyEdited::TakeNew;
    dialog.next(&mut f.shell);
    let row = dialog.row(c1).unwrap();
    let value = row.cells.iter().find(|c| c.cell == "value").unwrap();
    assert_eq!(dialog.action(c1, value), "update");

    // Off the checklist, the cell starts as Reject.
    dialog.back();
    dialog.set_field_updated("value", false);
    assert!(!dialog.settings.updates("value"));
    dialog.next(&mut f.shell);
    let row = dialog.row(c1).unwrap();
    let value = row.cells.iter().find(|c| c.cell == "value").unwrap();
    assert_eq!(dialog.action(c1, value), "reject");
    dialog.back();
    dialog.set_field_updated("value", true);
    assert_eq!(dialog.settings.fields_to_update, None);
    dialog.next(&mut f.shell);
    dialog.apply(&mut f.shell).unwrap();
    assert_eq!(f.value(r1), "22k");

    // Saved with the Project: the next dialog starts from them, and the Project Settings
    // summary shows them until Reset.
    let saved = f.shell.engine().project().unwrap().props().library_update.clone();
    assert_eq!(
        saved.as_ref().map(|s| s.locally_edited),
        Some(reda_ui::engine::model::library_update::LocallyEdited::TakeNew)
    );
    let dialog = f.dialog(UpdateScope::default());
    assert_eq!(Some(&dialog.settings), saved.as_ref());

    let mut page = SchematicPage::default();
    page.load(f.shell.engine().project().unwrap(), true);
    let summary = page.summary();
    assert!(
        summary
            .iter()
            .any(|(k, v)| k == "Locally edited values" && v == "Take the new default"),
        "{summary:?}"
    );
    assert!(page.steps().is_empty());
    page.reset = true;
    assert!(
        page.summary()
            .iter()
            .any(|(k, v)| k == "Locally edited values" && v == "Keep")
    );
    for step in page.steps() {
        f.shell.run(&step.id, step.params).unwrap();
    }
    assert_eq!(
        f.shell.engine().project().unwrap().props().library_update,
        None
    );
}

#[test]
fn choose_component_replaces_the_part_and_return_to_default_takes_it_back() {
    let mut f = fixture("update-choose");
    let (r1, c1) = f.place(R_10K, 0);
    let (_, c2) = f.place(R_10K, 1000);
    f.change_value();
    let mut dialog = f.dialog(UpdateScope::default());
    dialog.next(&mut f.shell);
    dialog.select_row(c1, false);
    dialog.choose_component(&mut f.shell, u(C_100N));
    let replacement = dialog.replacement(c1).unwrap();
    assert_eq!(replacement.part_name, "C_100n");
    assert!(dialog.replacement(c2).is_none());

    dialog.return_selected_to_default(&mut f.shell);
    assert!(dialog.replacement(c1).is_none());
    dialog.choose_component(&mut f.shell, u(C_100N));
    dialog.apply(&mut f.shell).unwrap();

    let unit = f.unit(r1);
    let project = f.shell.engine().project().unwrap();
    let sheet = project.schematic_sheet(u(SHEET)).unwrap();
    let entry = sheet.part(uuid(&unit["part"])).unwrap();
    assert_eq!(entry.part.name, "C_100n");
    // R2 took the update as usual.
    let r2 = sheet.units().find(|x| x.component.uuid == c2).unwrap();
    assert_eq!(r2.fields.value, "22k");
}

#[test]
fn replace_part_lists_the_current_part_first_and_the_pin_differences() {
    let mut f = fixture("replace");
    let (r1, c1) = f.place(R_10K, 0);
    let mut dialog = ReplaceDialog::new(&mut f.shell, vec![c1]);
    let first = &dialog.rows()[0];
    assert_eq!(first.name, "R_10k");
    assert!(first.current);
    assert!(dialog.rows().iter().any(|r| r.name == "LM358"));
    // Parts with no Symbol are not offered.
    assert!(dialog.rows().iter().all(|r| r.name != "Hole"));

    dialog.search = "LM3".into();
    dialog.refresh();
    let names: Vec<&str> = dialog.rows().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["R_10k", "LM358"]);
    dialog.choose(&mut f.shell, u(LM358));
    assert!(
        dialog.differences().iter().any(|d| d.contains("Unit")),
        "{:?}",
        dialog.differences()
    );
    dialog.apply(&mut f.shell).unwrap();
    let unit = f.unit(r1);
    let project = f.shell.engine().project().unwrap();
    let sheet = project.schematic_sheet(u(SHEET)).unwrap();
    assert_eq!(
        sheet.part(uuid(&unit["part"])).unwrap().part.name,
        "LM358"
    );
}

#[test]
fn a_generic_part_seeds_the_search_with_its_value() {
    let mut f = fixture("replace-generic");
    // C_100n has no MPN: a generic Part.
    let (_, c1) = f.place(C_100N, 0);
    let dialog = ReplaceDialog::new(&mut f.shell, vec![c1]);
    assert_eq!(dialog.search, "value:100n");
}

#[test]
fn the_entry_points_open_the_dialog_on_their_scope() {
    let mut f = fixture("update-entry");
    let (r1, c1) = f.place(R_10K, 0);
    f.place(C_100N, 1000);
    f.change_value();
    let open = |f: &mut Fixture, id: &str, params: Value| -> SchematicDialog {
        f.shell.run(id, params).unwrap();
        let requests = take_requests(f.shell.engine());
        let Some(Request::Library(dialog)) = requests.into_iter().last() else {
            panic!("no Library request");
        };
        dialog.open(&mut f.shell).unwrap()
    };
    // A selection: Selected Parts Only.
    let SchematicDialog::Update(d) = open(
        &mut f,
        "schematic.update_from_library.open",
        json!({"objects": [r1]}),
    ) else {
        panic!()
    };
    assert!(d.selected_only && d.has_selection());
    // A Library panel row: only that Part.
    let SchematicDialog::Update(d) = open(
        &mut f,
        "schematic.update_from_library.open",
        json!({"parts": [R_10K]}),
    ) else {
        panic!()
    };
    let on: Vec<&str> = (d.parts.iter())
        .filter(|p| p.on)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(on, ["R_10k"]);
    // A Library editor's Symbol: the Parts that use it.
    let SchematicDialog::Update(d) = open(
        &mut f,
        "schematic.update_from_library.open",
        json!({"element": R_SYMBOL}),
    ) else {
        panic!()
    };
    let on: Vec<&str> = (d.parts.iter())
        .filter(|p| p.on)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(on, ["R_10k"]);
    // Replace… on the selection.
    let SchematicDialog::Replace(_) = open(
        &mut f,
        "schematic.component.replace.open",
        json!({"objects": [r1]}),
    ) else {
        panic!()
    };
    // The Design menu's whole Project.
    assert!(matches!(
        LibraryDialog::Update {
            objects: vec![],
            parts: vec![],
            element: None
        }
        .open(&mut f.shell),
        Some(SchematicDialog::Update(d)) if !d.selected_only
    ));
    let _ = c1;
}

/// The Starter Library's Resistor and its first Footprint's file.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const PCB: &str = "00000000-0000-4000-8000-000000000003";

fn sync(shell: &mut Shell) {
    let list = shell.run("sync.preview", json!({})).unwrap();
    shell
        .run("sync.apply", json!({"hash": list["hash"]}))
        .unwrap();
}

/// The board's Footprint instance of Designator `designator`, and its first Pad.
fn instance_and_pad(shell: &Shell, designator: &str) -> (Uuid, Uuid) {
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    let instance = pcb
        .footprints()
        .find(|f| f.designator == designator)
        .unwrap();
    let copy = pcb.footprint_copy(instance.footprint).unwrap();
    let pad = copy
        .footprint
        .pads
        .iter()
        .find(|p| p.number == "1")
        .unwrap();
    (instance.uuid, pad.uuid)
}

#[test]
fn page_two_lists_the_local_edits_an_update_removes_with_the_compare() {
    let mut f = fixture_from("update-local-edits", &starter_library());
    f.place(RESISTOR, 0);
    sync(&mut f.shell);
    let (instance, pad) = instance_and_pad(&f.shell, "R1");
    f.shell
        .run(
            "pcb.pad.edit",
            json!({"instance": instance, "pad": pad, "width": "2mm"}),
        )
        .unwrap();
    f.edit_library(
        "footprints/RESC1005X40N.redafp",
        "Chip resistor, 0402",
        "Chip resistor 0402",
    );
    let mut dialog = f.dialog(UpdateScope::default());
    dialog.next(&mut f.shell);
    let row = &dialog.rows()[0];
    assert_eq!(row.lost_local_edits.len(), 1);
    assert_eq!(row.lost_local_edits[0].property, "width");
    let (edited, original) = edited_footprint(&f.shell, row.component).unwrap();
    assert_ne!(edited.pads, original.pads);
    let _ = PCB;
}

/// The Starter Library's plated M3 mounting hole, and its Footprint's file.
const HOLE_PAD: &str = "1a21d727-dfca-41ae-9639-309d8abf4714";
const HOLE_FILE: &str = "footprints/MountingHole_M3_Pad.redafp";

#[test]
fn the_board_update_previews_mechanical_parts_and_updates_the_ticked_ones() {
    let mut f = fixture_from("board-update", &starter_library());
    let place = |f: &mut Fixture, x: &str| {
        uuid(
            &f.shell
                .run(
                    "pcb.mechanical.place",
                    json!({"part": HOLE_PAD, "at": {"x": x, "y": "10mm"}}),
                )
                .unwrap()["result"]["footprint"],
        )
    };
    let h1 = place(&mut f, "20mm");
    let h2 = place(&mut f, "40mm");
    assert!(BoardUpdateDialog::new(&mut f.shell).rows().is_empty());
    let pad = {
        let pcb = f.shell.engine().project().unwrap().pcb().unwrap();
        let copy = pcb.footprint_copy(pcb.footprint(h1).unwrap().footprint).unwrap();
        copy.footprint.pads[0].uuid
    };
    f.shell
        .run(
            "pcb.pad.edit",
            json!({"instance": h1, "pad": pad, "width": "5mm", "height": "5mm"}),
        )
        .unwrap();
    f.edit_library(
        HOLE_FILE,
        r#""width":5500000,"height":5500000"#,
        r#""width":6000000,"height":6000000"#,
    );

    let mut dialog = BoardUpdateDialog::new(&mut f.shell);
    let rows = dialog.rows();
    assert_eq!(rows.len(), 2);
    let r1 = rows.iter().find(|r| r.instance == h1).unwrap();
    assert!(r1.included && !r1.lost_local_edits.is_empty());
    assert!(
        r1.changes.iter().any(|c| c.contains("resized")),
        "{:?}",
        r1.changes
    );
    assert!(reda_ui::schematic_dialogs::update::edited_copy(&f.shell, h1).is_some());
    dialog.set_included(h2, false);
    let before = |f: &Fixture, i: Uuid| {
        let pcb = f.shell.engine().project().unwrap().pcb().unwrap();
        pcb.footprint(i).unwrap().footprint
    };
    let h2_copy = before(&f, h2);
    dialog.apply(&mut f.shell).unwrap();
    let after = BoardUpdateDialog::new(&mut f.shell);
    let left: Vec<Uuid> = after.rows().iter().map(|r| r.instance).collect();
    assert_eq!(left, [h2]);
    assert_eq!(before(&f, h2), h2_copy);
}
