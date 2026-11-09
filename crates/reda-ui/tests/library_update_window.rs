//! Update from Library, Replace Part and the PCB's update in the window, driven through
//! AccessKit with `egui_kittest`: the Properties panel's Library Status badge, Part row
//! and field locks; the two pages with the field grid; the local-edit compare drawing;
//! and the PCB's Design menu.

use std::path::{Path, PathBuf};

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::{TempDir, starter_library};
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

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

struct Fixture {
    h: Harness<'static, App>,
    library: PathBuf,
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
    shell.new_project(dir.path(), "Demo").unwrap();
    let h = Harness::builder()
        .with_size([1400.0, 1000.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    Fixture {
        h,
        library,
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
    fn run(&mut self, id: &str, params: Value) -> Value {
        self.h.state_mut().shell_mut().run(id, params).unwrap()
    }

    /// Places `part`; returns `{unit, component, …}`.
    fn place(&mut self, part: &str, x: i64) -> Value {
        self.run(
            "schematic.component.place",
            json!({"document": SHEET, "part": part, "at": {"x": format!("{x}mil"), "y": 0}}),
        )["result"]
            .clone()
    }

    fn edit_library(&self, file: &str, from: &str, to: &str) {
        let path = self.library.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} has no {from:?}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
    }

    fn unit(&self, unit: &Value) -> Value {
        let uuid = unit.as_str().unwrap().parse().unwrap();
        let project = self.h.state().shell().engine().project().unwrap();
        let sheet = project.schematic_sheet(SHEET.parse().unwrap()).unwrap();
        serde_json::to_value(sheet.unit(uuid).unwrap()).unwrap()
    }

    fn click(&mut self, label: &str) {
        self.h.get_all_by_label(label).last().unwrap().click();
        // A menu or a new tab's scroll area can take a few frames to settle.
        self.h.run_steps(8);
    }

    fn click_role(&mut self, role: Role, label: &str) {
        self.h.get_by_role_and_label(role, label).click();
        self.h.run();
    }

    fn has(&self, label: &str) -> bool {
        self.h.query_all_by_label(label).next().is_some()
    }
}

#[test]
fn the_properties_panel_shows_the_library_status_with_update_and_the_part_row() {
    let mut f = fixture("update-window-properties");
    let r1 = f.place(R_10K, 0);
    f.run("selection.set", json!({"objects": [r1["unit"]]}));
    f.h.run();
    assert!(f.has("Library Status: Up to Date"));
    assert!(f.has("R_10k"));

    // A lock per field: Update from Library leaves a Locked Field alone.
    f.click_role(Role::CheckBox, "Lock Value");
    assert_eq!(f.unit(&r1["unit"])["locked_fields"], json!(["value"]));

    f.edit_library(
        "parts/R_10k.redapart",
        "\"mpn\":\"RC0603FR-0710KL\"",
        "\"mpn\":\"RC0603FR-0722KL\"",
    );
    // Any Library Command makes the panel look again.
    f.run("library.where_used", json!({"element": R_10K}));
    f.h.run();
    assert!(f.has("Library Status: Library Differs: Fields"));

    // Update… opens the dialog on the selection.
    f.click("Update…");
    f.h.get_by_label("Update from Library");
    let selected_only =
        f.h.get_by_role_and_label(Role::CheckBox, "Selected Parts Only");
    assert_eq!(
        selected_only.accesskit_node().toggled(),
        Some(Toggled::True)
    );
    f.click("Next");
    f.h.get_by_role_and_label(Role::ListItem, "R1");
    f.click("Cancel");
    assert!(!f.has("Next"));

    // Replace… opens the picker with the current Part on top.
    f.click("Replace…");
    f.h.get_by_label("Replace Part");
    let first = f.h.get_all_by_role(Role::ListItem).next().unwrap();
    assert_eq!(first.accesskit_node().label().as_deref(), Some("R_10k"));
    f.click("Cancel");

    // Edit in Library opens the Part in its editor.
    f.click("Edit in Library");
    let entry =
        f.h.state()
            .shell()
            .engine()
            .journal()
            .iter()
            .rfind(|e| e.outcome.is_ok() && e.id == "library.element.edit_in_library")
            .map(|e| e.params.clone());
    assert!(entry.is_some());
}

#[test]
fn page_two_shows_the_field_grid_and_reject_selected_keeps_the_value() {
    let mut f = fixture("update-window-grid");
    let r1 = f.place(R_10K, 0);
    f.edit_library(
        "parts/R_10k.redapart",
        "\"value\":\"10k\"",
        "\"value\":\"22k\"",
    );
    f.run(
        "schematic.dialog.open",
        json!({"dialog": "update_from_library"}),
    );
    f.h.run();
    f.h.get_by_role_and_label(Role::CheckBox, "R_10k");
    f.h.get_by_role_and_label(Role::CheckBox, "Full Replace");
    f.click("Next");
    f.h.get_by_label("Library Differs: Fields");
    f.h.get_by_role_and_label(Role::CheckBox, "R1: Fields");
    f.click_role(Role::ListItem, "R1 Value: Update");
    f.click("Reject Selected");
    f.h.get_by_role_and_label(Role::ListItem, "R1 Value: Reject");
    f.click("Update Selected");
    f.click("Reject Selected");
    // Apply: the Fields aspect moves the defaults, the rejected cell keeps the value.
    f.click("Update");
    assert!(!f.has("Reject Selected"));
    assert_eq!(f.unit(&r1["unit"])["fields"]["value"], "10k");
}

/// The Starter Library's Resistor.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const HOLE_PAD: &str = "1a21d727-dfca-41ae-9639-309d8abf4714";

#[test]
fn page_two_compares_the_local_edits_it_removes_over_the_original() {
    let mut f = fixture_from("update-window-local", &starter_library());
    f.place(RESISTOR, 0);
    let list = f.run("sync.preview", json!({}));
    f.run("sync.apply", json!({"hash": list["hash"]}));
    let (instance, pad) = {
        let pcb =
            f.h.state()
                .shell()
                .engine()
                .project()
                .unwrap()
                .pcb()
                .unwrap();
        let i = pcb.footprints().next().unwrap();
        let copy = pcb.footprint_copy(i.footprint).unwrap();
        (i.uuid, copy.footprint.pads[0].uuid)
    };
    f.run(
        "pcb.pad.edit",
        json!({"instance": instance, "pad": pad, "width": "2mm"}),
    );
    f.edit_library(
        "footprints/RESC1005X40N.redafp",
        "Chip resistor, 0402",
        "Chip resistor 0402",
    );
    f.run(
        "schematic.dialog.open",
        json!({"dialog": "update_from_library"}),
    );
    f.h.run();
    f.click("Next");
    f.h.get_by_label("Local Pad edits this removes");
    f.h.get_by_role_and_label(Role::Image, "Edited Pads over the original");
}

#[test]
fn the_pcb_design_menu_opens_its_update_from_library() {
    let mut f = fixture_from("update-window-board", &starter_library());
    f.run(
        "pcb.mechanical.place",
        json!({"part": HOLE_PAD, "at": {"x": "20mm", "y": "10mm"}}),
    );
    f.edit_library(
        "footprints/MountingHole_M3_Pad.redafp",
        r#""width":5500000,"height":5500000"#,
        r#""width":6000000,"height":6000000"#,
    );
    f.run("document.open", json!({"document": PCB}));
    f.h.run();
    f.click("Design");
    f.click("Update from Library…");
    f.h.get_by_label("Update Footprints from Library");
    f.h.get_by_role_and_label(Role::CheckBox, "Update H1");
    f.click("Update");
    assert!(!f.has("Update Footprints from Library"));
    let rows = f.run("pcb.footprint.update_preview", json!({}));
    assert_eq!(rows["footprints"], json!([]));
}

#[test]
fn project_settings_summarise_the_saved_settings_and_reset_them() {
    let mut f = fixture("update-window-settings");
    f.place(R_10K, 0);
    f.edit_library(
        "parts/R_10k.redapart",
        "\"value\":\"10k\"",
        "\"value\":\"22k\"",
    );
    // An update with page 1's settings saves them.
    let preview = f.run(
        "schematic.component.update_preview",
        json!({"settings": {"locally_edited": "take_new"}}),
    );
    f.run(
        "schematic.component.update_from_library",
        json!({"preview_hash": preview["preview_hash"],
               "settings": {"locally_edited": "take_new"}}),
    );
    f.h.state_mut().open_project_settings();
    f.h.run();
    f.h.get_by_role_and_label(Role::TreeItem, "Schematic")
        .click();
    f.h.run();
    f.h.get_by_label("Update from Library");
    f.h.get_by_label("Locally edited values: Take the new default");
    f.h.get_by_role_and_label(Role::Button, "Reset Update from Library Settings")
        .click();
    f.h.run();
    f.h.get_by_label("Cleared on Apply.");
    f.h.get_by_role_and_label(Role::Button, "Apply").click();
    f.h.run();
    let saved = (f.h.state().shell().engine().project())
        .unwrap()
        .props()
        .library_update
        .clone();
    assert_eq!(saved, None);
}

#[test]
fn a_library_editors_update_in_project_opens_the_dialog_on_its_parts() {
    let mut f = fixture("update-window-editors");
    f.place(R_10K, 0);
    f.place("a0000000-0000-4000-8000-000000000110", 1000);
    let ticked = |f: &Fixture| -> Vec<String> {
        ["R_10k", "C_100n"]
            .into_iter()
            .filter(|p| {
                f.h.get_by_role_and_label(Role::CheckBox, p)
                    .accesskit_node()
                    .toggled()
                    == Some(Toggled::True)
            })
            .map(str::to_owned)
            .collect()
    };
    // The Symbol editor's Tools menu.
    f.run(
        "library.element.open",
        json!({"element": "a0000000-0000-4000-8000-000000000010"}),
    );
    f.h.run();
    f.click("Tools");
    f.click("Update in Project…");
    f.h.get_by_label("Update from Library");
    assert_eq!(ticked(&f), ["R_10k"]);
    f.click("Cancel");
    // The Part editor's button.
    f.run("library.element.open", json!({"element": R_10K}));
    f.h.run();
    f.click("Update in Project…");
    f.h.get_by_label("Update from Library");
    assert_eq!(ticked(&f), ["R_10k"]);
}
