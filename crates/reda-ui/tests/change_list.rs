//! The Change List window's model, through the Shell with no window: the columns of each
//! row, group counts and tick-all, the toolbar (text filter, Only Problems, Expand and
//! Collapse All, Include and Exclude All), multi-select with Space and the context menu's
//! Include, Exclude, Invert and All of This Kind, Applied rows kept after Apply, Export
//! through `checks.report`, and Show on the board or the sheet.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::sync::GroupKind;
use reda_ui::settings::change_list::{self, ChangeListPage};
use reda_ui::settings::{FileRequest, SettingsDialog};
use reda_ui::shell::Shell;
use reda_ui::show_on_board::Side;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn shell(dir: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    shell
}

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// R1 at the origin and R2 10 mm to the right, their pin 2s wired and labelled SIG: five
/// changes. Returns R1's (Component, Unit) and the Wire.
fn two_wired_resistors(shell: &mut Shell) -> ((Uuid, Uuid), Uuid) {
    let mut units = Vec::new();
    for x in ["0mm", "10mm"] {
        let out = run(
            shell,
            "schematic.component.place",
            json!({"document": SHEET, "part": R_10K, "at": {"x": x, "y": 0}}),
        );
        units.push((
            uuid(&out["result"]["component"]["uuid"]),
            uuid(&out["result"]["unit"]),
        ));
    }
    let out = run(
        shell,
        "schematic.wire.add",
        json!({"document": SHEET, "points": [
            {"x": "0mm", "y": "-3.81mm"}, {"x": "10mm", "y": "-3.81mm"}]}),
    );
    let wire = uuid(&out["result"]["wires"][0]);
    run(
        shell,
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SIG", "at": {"x": "5mm", "y": "-3.81mm"}}),
    );
    (units[0], wire)
}

fn sync(shell: &mut Shell) {
    let list = run(shell, "sync.preview", json!({}));
    run(shell, "sync.apply", json!({"hash": list["hash"]}));
}

fn dialog(shell: &Shell) -> SettingsDialog {
    change_list::dialog(shell.engine().project().unwrap())
}

fn page(dialog: &mut SettingsDialog) -> &mut ChangeListPage {
    dialog.page_mut::<ChangeListPage>().unwrap()
}

/// The id of the row whose Target is `target` in group `group`.
fn row(page: &ChangeListPage, group: GroupKind, target: &str) -> String {
    page.list()
        .groups
        .iter()
        .filter(|g| g.group == group)
        .flat_map(|g| &g.changes)
        .find(|c| page.cells(&c.id).unwrap().target == target)
        .unwrap_or_else(|| panic!("no {group:?} row for {target}"))
        .id
        .clone()
}

#[test]
fn each_row_has_kind_target_from_to_and_status_columns() {
    let dir = TempDir::new("cl-cells");
    let mut shell = shell(&dir);
    two_wired_resistors(&mut shell);
    let mut d = dialog(&shell);
    let page = page(&mut d);

    let add = page
        .cells(&row(page, GroupKind::AddComponents, "R1"))
        .unwrap();
    assert_eq!(add.kind, "Add Component");
    assert_eq!(add.from_to, "(none) to R_0603");
    assert_eq!(add.status, "OK");
    let pad = page.cells(&row(page, GroupKind::PadNets, "R2.2")).unwrap();
    assert_eq!(pad.kind, "Pad Net");
    assert_eq!(pad.from_to, "(no Net) to SIG");
    let net = page.cells(&row(page, GroupKind::AddNets, "SIG")).unwrap();
    assert_eq!(net.kind, "Add Net");

    // Group counts, and the tick-all box of a group.
    assert_eq!(page.group_count(GroupKind::AddComponents), 2);
    assert_eq!(page.group_count(GroupKind::PadNets), 2);
    page.set_kind(GroupKind::AddComponents, false);
    assert_eq!(
        page.ticked_count(),
        1,
        "the Net only: Pads need their Components"
    );
    page.set_kind(GroupKind::AddComponents, true);
    assert_eq!(page.ticked_count(), 5, "their Pads follow them back");
    page.set_kind(GroupKind::PadNets, false);
    assert_eq!(page.ticked_count(), 3);
    page.set_kind(GroupKind::PadNets, true);
    assert_eq!(page.ticked_count(), 5);
}

#[test]
fn the_text_filter_and_only_problems_narrow_the_rows() {
    let dir = TempDir::new("cl-filter");
    let mut shell = shell(&dir);
    let (_, wire) = two_wired_resistors(&mut shell);
    sync(&mut shell);
    let net = run(&mut shell, "objects.query", json!({"type": "net"}));
    let sig = uuid(&net["objects"][0]["object"]["uuid"]);
    run(
        &mut shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "net": sig,
               "points": [{"x": "40mm", "y": "40mm"}, {"x": "50mm", "y": "40mm"}]}),
    );
    // Deleting the Wire (and the label with it) removes SIG: a Note, as its Track stays.
    let label = run(&mut shell, "objects.query", json!({"type": "net_label"}));
    let label = uuid(&label["objects"][0]["object"]["uuid"]);
    run(
        &mut shell,
        "schematic.delete",
        json!({"objects": [wire, label]}),
    );
    let mut d = dialog(&shell);
    let page = page(&mut d);
    let all = page.rows().len();
    assert_eq!(all, 3, "{:?}", page.list());

    page.set_filter("r2");
    let rows = page.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(page.cells(&rows[0]).unwrap().target, "R2.2");

    page.set_filter("");
    page.set_only_problems(true);
    let rows = page.rows();
    assert_eq!(rows.len(), 1, "only the Note row");
    let cells = page.cells(&rows[0]).unwrap();
    assert_eq!(cells.kind, "Remove Net");
    assert!(cells.status.starts_with("Note: "), "{}", cells.status);
    assert_eq!(page.group_count(GroupKind::PadNets), 0);

    page.set_only_problems(false);
    assert_eq!(page.rows().len(), all);
}

#[test]
fn expand_and_collapse_all_and_include_and_exclude_all() {
    let dir = TempDir::new("cl-toolbar");
    let mut shell = shell(&dir);
    two_wired_resistors(&mut shell);
    let mut d = dialog(&shell);
    let page = page(&mut d);
    let groups = [
        GroupKind::AddComponents,
        GroupKind::AddNets,
        GroupKind::PadNets,
    ];
    assert!(groups.iter().all(|g| page.is_expanded(*g)));
    page.collapse_all();
    assert!(groups.iter().all(|g| !page.is_expanded(*g)));
    page.set_expanded(GroupKind::AddNets, true);
    assert!(page.is_expanded(GroupKind::AddNets));
    page.expand_all();
    assert!(groups.iter().all(|g| page.is_expanded(*g)));

    page.exclude_all();
    assert_eq!(page.ticked_count(), 0);
    page.include_all();
    assert_eq!(page.ticked_count(), 5);
    // With a filter, they act on the rows shown.
    page.set_filter("pad");
    page.exclude_all();
    assert_eq!(page.ticked_count(), 3);
}

#[test]
fn rows_multi_select_and_space_invert_and_kind_act_on_them() {
    let dir = TempDir::new("cl-select");
    let mut shell = shell(&dir);
    two_wired_resistors(&mut shell);
    let mut d = dialog(&shell);
    let page = page(&mut d);
    let r1 = row(page, GroupKind::AddComponents, "R1");
    let r2 = row(page, GroupKind::AddComponents, "R2");
    let sig = row(page, GroupKind::AddNets, "SIG");
    let pad1 = row(page, GroupKind::PadNets, "R1.2");
    let pad2 = row(page, GroupKind::PadNets, "R2.2");

    page.select(&r1, Modifiers::NONE);
    page.select(&pad1, Modifiers::SHIFT);
    assert_eq!(
        page.selected(),
        [r1.clone(), r2.clone(), sig.clone(), pad1.clone()]
    );
    page.select(&r2, Modifiers::COMMAND);
    assert_eq!(page.selected(), [r1.clone(), sig.clone(), pad1.clone()]);

    // Space toggles the tick of the selection.
    page.toggle_selected();
    assert!(!page.is_ticked(&r1) && !page.is_ticked(&sig) && !page.is_ticked(&pad1));
    assert!(!page.is_ticked(&pad2), "R2.2 needs the Net");
    page.toggle_selected();
    assert!(page.is_ticked(&r1) && page.is_ticked(&sig) && page.is_ticked(&pad1));

    page.select(&r2, Modifiers::NONE);
    page.exclude_selected();
    assert!(!page.is_ticked(&r2) && !page.is_ticked(&pad2));
    page.include_selected();
    assert!(page.is_ticked(&r2));

    page.select(&r1, Modifiers::NONE);
    page.select(&r2, Modifiers::COMMAND);
    page.exclude_selected();
    page.select(&r2, Modifiers::NONE);
    page.invert_selected();
    assert!(page.is_ticked(&r2) && !page.is_ticked(&r1));

    // All of This Kind: the selected row's group.
    page.select(&pad1, Modifiers::NONE);
    page.set_kind_of_selected(false);
    assert!(!page.is_ticked(&pad2));
    page.set_kind_of_selected(true);
    assert!(page.is_ticked(&pad1) && page.is_ticked(&pad2) && page.is_ticked(&r1));
}

#[test]
fn applied_rows_stay_greyed_and_the_rest_keep_their_ticks() {
    let dir = TempDir::new("cl-applied");
    let mut shell = shell(&dir);
    two_wired_resistors(&mut shell);
    let mut d = dialog(&shell);
    let p = page(&mut d);
    let r2 = row(p, GroupKind::AddComponents, "R2");
    let sig = row(p, GroupKind::AddNets, "SIG");
    p.set_ticked(&sig, false);
    d.apply(&mut shell).unwrap();
    // Apply ran again: the Net is still pending, and still unticked.
    let p = page(&mut d);
    assert_eq!(p.list().count, 3);
    assert!(!p.is_ticked(&sig), "the untick is kept");
    let applied: Vec<String> = p.applied().iter().map(|c| c.id.clone()).collect();
    assert_eq!(applied.len(), 2);
    assert!(applied.contains(&r2));
    assert_eq!(p.cells(&r2).unwrap().status, "Applied");
    assert!(p.rows().contains(&r2), "Applied rows stay visible");
    assert!(!p.is_ticked(&r2) || !p.list().changes().any(|c| c.id == r2));
    // A second Apply adds to them.
    p.set_ticked(&sig, true);
    d.apply(&mut shell).unwrap();
    let p = page(&mut d);
    assert_eq!(p.list().count, 0);
    assert_eq!(p.applied().len(), 5);
}

#[test]
fn export_writes_the_change_list_through_checks_report() {
    let dir = TempDir::new("cl-export");
    let mut shell = shell(&dir);
    two_wired_resistors(&mut shell);
    let mut d = dialog(&shell);
    let html = dir.path().join("changes.html");
    d.picked_file(&mut shell, FileRequest::ExportChangeList, &html);
    let last = shell.engine().journal().last().unwrap();
    assert_eq!(last.id, "checks.report");
    let text = std::fs::read_to_string(&html).unwrap();
    assert!(text.contains("<html"), "{text}");
    let file = dir.path().join("changes.json");
    d.picked_file(&mut shell, FileRequest::ExportChangeList, &file);
    let report: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(report["kind"], "sync");
    assert_eq!(report["change_list"]["count"], 5);
    assert!(page(&mut d).notice().unwrap().contains("changes.json"));
}

#[test]
fn show_opens_the_side_that_has_the_row_and_switches_between_board_and_sheet() {
    let dir = TempDir::new("cl-show");
    let mut shell = shell(&dir);
    let ((r1, r1_unit), _) = two_wired_resistors(&mut shell);
    // Add Component has nothing on the board yet: Show opens the sheet.
    let mut d = dialog(&shell);
    let add = row(page(&mut d), GroupKind::AddComponents, "R1");
    let sheet: Uuid = SHEET.parse().unwrap();
    page(&mut d).show_row(&mut shell, &add);
    assert_eq!(shell.shown_side(), Some(Side::Sheet));
    assert_eq!(shell.highlight().objects, [r1_unit]);
    assert!(shell.view_only(sheet));
    assert!(!shell.can_switch_shown_side(), "nothing on the board");

    // A Designator change on a synced Component: the board first, and the sheet too.
    sync(&mut shell);
    run(
        &mut shell,
        "schematic.component.set",
        json!({"components": [r1], "designator": "R9"}),
    );
    let mut d = dialog(&shell);
    let id = row(page(&mut d), GroupKind::Designators, "R1");
    page(&mut d).set_ticked(&id, false);
    page(&mut d).show_row(&mut shell, &id);
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    let footprint = pcb
        .footprints()
        .find(|f| f.designator == "R1")
        .unwrap()
        .uuid;
    let pcb = pcb.uuid;
    assert_eq!(shell.shown_side(), Some(Side::Board));
    assert_eq!(shell.highlight().objects, [footprint]);
    assert!(shell.view_only(pcb) && !shell.view_only(sheet));
    assert_eq!(shell.active_document().unwrap().document, pcb);

    assert!(shell.can_switch_shown_side());
    shell.switch_shown_side();
    assert_eq!(shell.shown_side(), Some(Side::Sheet));
    assert_eq!(shell.highlight().objects, [r1_unit]);
    assert_eq!(shell.active_document().unwrap().document, sheet);
    assert!(shell.view_only(sheet) && !shell.view_only(pcb));

    shell.back_from_show();
    assert_eq!(shell.shown_side(), None);
    assert!(shell.highlight().objects.is_empty());
    assert!(
        !page(&mut d).is_ticked(&id),
        "the Change List kept its ticks"
    );
}
