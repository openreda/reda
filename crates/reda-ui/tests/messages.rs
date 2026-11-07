//! The Messages panel's state, without a window: check results grouped by Document,
//! going to a Violation, Waivers and severities from the right-click menu, waived and
//! stale results, and load errors.

use std::path::Path;
use std::sync::Arc;

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FontFace, UserDirs};
use reda_ui::engine::model::Severity;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::layout::TabKey;
use reda_ui::messages::{CheckKind, LineKey, MessageGroup};
use reda_ui::shell::{InstalledFonts, Shell};
use serde_json::{Value, json};

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn mil(n: i64) -> Nm {
    Nm(n * 25_400)
}

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

/// A Project with two sheets: an unattached Net Label "A" on Sheet 1, and a lone Wire on
/// Sheet 2 whose two ends dangle. Returns the shell, the sheets and the Wire.
struct Demo {
    shell: Shell,
    sheet1: String,
    sheet2: String,
    label: String,
    wire: String,
    _home: TempDir,
    _work: TempDir,
}

fn demo() -> Demo {
    let home = TempDir::new("messages-home");
    let work = TempDir::new("messages-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Demo").unwrap();
    let sheet1 = shell.document_tabs()[0].document.to_string();
    let out = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    let sheet2 = out["result"]["uuid"].as_str().unwrap().to_owned();
    let out = shell
        .run(
            "schematic.net_label.add",
            json!({"document": sheet1, "name": "A", "at": at(500, 500)}),
        )
        .unwrap();
    let label = out["result"]["net_label"].as_str().unwrap().to_owned();
    let out = shell
        .run(
            "schematic.wire.add",
            json!({"document": sheet2, "points": [at(1000, 1000), at(2000, 1000)]}),
        )
        .unwrap();
    let wire = out["result"]["wires"][0].as_str().unwrap().to_owned();
    Demo {
        shell,
        sheet1,
        sheet2,
        label,
        wire,
        _home: home,
        _work: work,
    }
}

/// Each group as its title and line texts.
fn texts(groups: &[MessageGroup]) -> Vec<(String, Vec<String>)> {
    groups
        .iter()
        .map(|g| {
            (
                g.title.clone(),
                g.lines.iter().map(|l| l.text.clone()).collect(),
            )
        })
        .collect()
}

fn violation_lines(shell: &Shell) -> Vec<(LineKey, String)> {
    shell
        .message_groups()
        .into_iter()
        .flat_map(|g| g.lines)
        .filter(|l| matches!(l.key, LineKey::Violation(..)))
        .map(|l| (l.key, l.text))
        .collect()
}

#[test]
fn an_erc_run_lists_its_violations_grouped_by_document_in_document_order() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let groups = d.shell.message_groups();
    assert_eq!(
        texts(&groups),
        [
            (
                "Sheet 1".to_owned(),
                vec!["A is attached to nothing".to_owned()]
            ),
            (
                "Sheet 2".to_owned(),
                vec![
                    "Wire end at (25.4mm, 25.4mm) touches nothing".to_owned(),
                    "Wire end at (50.8mm, 25.4mm) touches nothing".to_owned(),
                ]
            ),
        ]
    );
    assert_eq!(groups[0].document.unwrap().to_string(), d.sheet1);
    assert_eq!(groups[1].document.unwrap().to_string(), d.sheet2);
    assert!(
        groups
            .iter()
            .flat_map(|g| &g.lines)
            .all(|l| l.severity == Severity::Warning && !l.stale && !l.waived)
    );
}

#[test]
fn going_to_a_violation_opens_its_document_zooms_to_it_and_selects_its_objects() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let (key, _) = violation_lines(&d.shell)[2].clone();
    d.shell.go_to(key);

    let sheet2 = d.sheet2.parse().unwrap();
    assert_eq!(d.shell.layout().active_document(), Some(sheet2));
    assert!(d.shell.layout().place(&TabKey::Document(sheet2)).is_some());
    let ids: Vec<String> = d
        .shell
        .selection()
        .objects
        .iter()
        .map(|u| u.to_string())
        .collect();
    assert_eq!(ids, [d.wire.clone()]);
    // Through the same Commands as any other open and select, so the Journal has them.
    let journal: Vec<&str> = d
        .shell
        .engine()
        .journal()
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert!(journal.ends_with(&["document.open", "selection.set"]));
    // The canvas marks the one selection.
    let marked = &d.shell.document_tab(sheet2).unwrap().selection.objects;
    assert_eq!(marked[..], d.shell.selection().objects[..]);
    let (lo, hi) = d.shell.document_tab(sheet2).unwrap().zoom_to.unwrap();
    let end = Point::new(mil(2000), mil(1000));
    assert!(lo.x < end.x && end.x < hi.x && lo.y < end.y && end.y < hi.y);
    // Zoomed in on the end, not the whole sheet.
    assert!(hi.x - lo.x < mil(2000), "{lo:?}..{hi:?}");
}

#[test]
fn the_canvas_has_a_marker_at_each_violation_of_its_document() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let sheet1 = d.sheet1.parse().unwrap();
    let markers = d.shell.markers(sheet1);
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].severity, Severity::Warning);
    // The label sits near its anchor.
    let anchor = Point::new(mil(500), mil(500));
    let dx = (markers[0].at.x - anchor.x).0.abs();
    let dy = (markers[0].at.y - anchor.y).0.abs();
    assert!(dx < mil(300).0 && dy < mil(300).0, "{:?}", markers[0].at);

    d.shell.go_to(violation_lines(&d.shell)[1].0);
    let sheet2 = d.sheet2.parse().unwrap();
    let markers = d.shell.markers(sheet2);
    assert_eq!(
        markers.iter().map(|m| m.at).collect::<Vec<_>>(),
        [
            Point::new(mil(1000), mil(1000)),
            Point::new(mil(2000), mil(1000))
        ]
    );
    assert_eq!(
        markers.iter().map(|m| m.current).collect::<Vec<_>>(),
        [true, false]
    );
}

#[test]
fn a_waived_violation_is_hidden_until_waived_ones_are_shown() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let (label_line, _) = violation_lines(&d.shell)[0].clone();
    d.shell.waive(label_line).unwrap();

    let lines = violation_lines(&d.shell);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(!d.shell.messages().run(CheckKind::Erc).unwrap().stale);

    d.shell.set_show_waived(true).unwrap();
    let last = d.shell.engine().journal().last().unwrap().id.clone();
    assert_eq!(last, "view.messages.show_waived");
    let groups = d.shell.message_groups();
    let waived: Vec<&str> = groups
        .iter()
        .flat_map(|g| &g.lines)
        .filter(|l| l.waived)
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(waived, ["A is attached to nothing"]);

    let key = groups[0].lines[0].key;
    d.shell.unwaive(key).unwrap();
    d.shell.set_show_waived(false).unwrap();
    assert_eq!(violation_lines(&d.shell).len(), 3);
    let _ = d.label;
}

#[test]
fn setting_a_check_off_from_a_violation_drops_its_violations() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let (wire_line, _) = violation_lines(&d.shell)[1].clone();
    d.shell.set_severity(wire_line, Severity::Off).unwrap();
    let lines: Vec<String> = violation_lines(&d.shell)
        .into_iter()
        .map(|(_, t)| t)
        .collect();
    assert_eq!(lines, ["A is attached to nothing"]);

    let (label_line, _) = violation_lines(&d.shell)[0].clone();
    d.shell.set_severity(label_line, Severity::Error).unwrap();
    let groups = d.shell.message_groups();
    assert_eq!(groups[0].lines[0].severity, Severity::Error);
}

#[test]
fn results_are_marked_stale_after_an_edit_and_fresh_again_after_undoing_it() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    d.shell
        .run(
            "schematic.net_label.add",
            json!({"document": d.sheet1, "name": "B", "at": at(900, 900)}),
        )
        .unwrap();
    let groups = d.shell.message_groups();
    assert!(groups.iter().flat_map(|g| &g.lines).all(|l| l.stale));
    assert!(d.shell.messages().run(CheckKind::Erc).unwrap().stale);

    d.shell
        .run("edit.undo", json!({"document": d.sheet1}))
        .unwrap();
    assert!(!d.shell.messages().run(CheckKind::Erc).unwrap().stale);
}

#[test]
fn preference_warnings_appear_as_load_messages() {
    let home = TempDir::new("messages-home");
    let dirs = UserDirs::under(home.path());
    std::fs::create_dir_all(&dirs.config).unwrap();
    std::fs::write(dirs.preferences_file(), "[general]\nno_such_key = 1\n").unwrap();
    let shell = Shell::new(engine_in(home.path()));
    let groups = shell.message_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].title, "General");
    assert_eq!(groups[0].document, None);
    let line = &groups[0].lines[0];
    assert_eq!(line.severity, Severity::Warning);
    assert!(
        line.text.starts_with("Preferences: ") && line.text.contains("no_such_key"),
        "{}",
        line.text
    );
}

#[test]
fn load_messages_from_other_sources_can_be_reported_and_replaced() {
    let home = TempDir::new("messages-home");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.set_load_messages(
        "keymap",
        vec![(Severity::Error, "Ctrl+S is bound twice".to_owned())],
    );
    assert_eq!(
        texts(&shell.message_groups()),
        [(
            "General".to_owned(),
            vec!["Ctrl+S is bound twice".to_owned()]
        )]
    );
    shell.set_load_messages("keymap", Vec::new());
    assert!(shell.message_groups().is_empty());
}

struct NoInstalledFonts;

impl InstalledFonts for NoInstalledFonts {
    fn load(&self, _family: &str) -> Option<FontFace> {
        None
    }
}

#[test]
fn pcb_text_in_a_missing_font_warns_under_the_pcb_document() {
    let home = TempDir::new("messages-home");
    let work = TempDir::new("messages-work");
    let mut shell = Shell::new(engine_in(home.path())).with_fonts(Arc::new(NoInstalledFonts));
    shell.new_project(work.path(), "Demo").unwrap();
    // A new Project comes with its PCB Document.
    let pcb = shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .uuid
        .to_string();
    shell
        .run(
            "pcb.text.add",
            json!({"layer": "silkscreen_top", "text": "Hi", "font": "Comic Sans MS",
                   "x": 0, "y": 0}),
        )
        .unwrap();
    let groups = shell.message_groups();
    let pcb_group = groups
        .iter()
        .find(|g| g.document.map(|u| u.to_string()) == Some(pcb.clone()))
        .expect("a group for the PCB Document");
    assert_eq!(
        pcb_group
            .lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>(),
        ["Font “Comic Sans MS” is not installed; its text is drawn in Noto Sans"]
    );
}

#[test]
fn a_pin_violation_is_marked_on_its_pin() {
    let home = TempDir::new("messages-home");
    let work = TempDir::new("messages-work");
    let library =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/erc_library/Erc");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    let sheet = shell.document_tabs()[0].document;
    shell
        .run(
            "schematic.component.place",
            json!({"document": sheet, "part": "a0000000-0000-4000-8000-000000000100",
                   "at": at(1000, 1000)}),
        )
        .unwrap();
    shell.run_check(CheckKind::Erc).unwrap();

    // The resistor's two Pins are unconnected: one marker on each Pin's end, then the
    // test Part's missing Footprint choice on the Unit's origin.
    let project = shell.engine().project().unwrap();
    let pins: Vec<Point> = reda_ui::engine::schematic::connectivity::placed_pins(
        project.schematic_sheet(sheet).unwrap(),
    )
    .into_iter()
    .map(|(_, _, p)| p)
    .collect();
    let markers: Vec<Point> = shell.markers(sheet).iter().map(|m| m.at).collect();
    assert_eq!(markers[..2], pins);
    assert_eq!(markers[2..], [Point::new(Nm(25_400_000), Nm(25_400_000))]);
    assert_ne!(pins[0], pins[1]);
}

#[test]
fn a_drc_run_lists_its_violations_under_the_pcb_and_goes_stale_after_an_edit() {
    let home = TempDir::new("messages-home");
    let work = TempDir::new("messages-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Demo").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    let out = shell
        .run(
            "pcb.track.add",
            json!({"layer": "copper_top", "width": "0.25mm",
                   "points": [{"x": "10mm", "y": "10mm"}, {"x": "15mm", "y": "10mm"}]}),
        )
        .unwrap();
    let track = out["result"]["tracks"][0].as_str().unwrap().to_owned();
    shell.run_check(CheckKind::Drc).unwrap();

    let groups = shell.message_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].document, Some(pcb));
    let texts: Vec<&str> = groups[0].lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Copper with no Net",
            "Unconnected end at (10mm, 10mm)",
            "Unconnected end at (15mm, 10mm)",
        ]
    );

    shell.go_to(groups[0].lines[0].key);
    assert_eq!(shell.layout().active_document(), Some(pcb));
    let ids: Vec<String> = shell
        .selection()
        .objects
        .iter()
        .map(|u| u.to_string())
        .collect();
    assert_eq!(ids, std::slice::from_ref(&track));

    shell
        .run("pcb.track.delete", json!({"tracks": [track]}))
        .unwrap();
    assert!(shell.messages().run(CheckKind::Drc).unwrap().stale);
}

#[test]
fn a_deleted_objects_stale_marker_keeps_the_location_returned_by_the_check() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let sheet = d.sheet1.parse().unwrap();
    let before = d.shell.markers(sheet)[0].at;
    d.shell
        .run("schematic.delete", json!({"objects":[d.label]}))
        .unwrap();
    assert_eq!(d.shell.markers(sheet).len(), 1);
    assert_eq!(d.shell.markers(sheet)[0].at, before);
    assert_eq!(before, Point::new(mil(500), mil(500)));
}

#[test]
fn selected_violations_share_one_reason_and_each_sheet_gets_one_undo_entry() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let keys: Vec<LineKey> = violation_lines(&d.shell).iter().map(|(k, _)| *k).collect();
    d.shell
        .waive_many(&keys, "intentional test wiring")
        .unwrap();
    let run = d.shell.messages().run(CheckKind::Erc).unwrap();
    assert_eq!(
        run.entries
            .iter()
            .filter(|e| e.violation.waiver.is_some())
            .count(),
        3
    );
    let waivers: std::collections::BTreeSet<_> = run
        .entries
        .iter()
        .filter_map(|e| e.violation.waiver)
        .collect();
    assert_eq!(waivers.len(), 2);
    for w in waivers {
        let object = d.shell.run("object.get", json!({"uuid":w})).unwrap()["object"].clone();
        assert_eq!(object["reason"], "intentional test wiring");
    }
    d.shell
        .run("edit.undo", json!({"document":d.sheet2}))
        .unwrap();
    d.shell.run_check(CheckKind::Erc).unwrap();
    assert_eq!(violation_lines(&d.shell).len(), 2);
}

#[test]
fn collapsed_openings_translate_both_sides_and_kinds_without_changing_check_arguments() {
    let home = TempDir::new("messages-opening-home");
    let work = TempDir::new("messages-opening-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Openings").unwrap();
    for side in ["top", "bottom"] {
        shell
            .run(
                "pcb.free_pad.add",
                json!({"kind":"smd","side":side,
            "at":{"x":if side == "top" { "1mm" } else { "5mm" },"y":"1mm"},
            "width":"1mm","mask":{side:{"expansion":"-0.5mm"}},
            "paste":{"expansion":"-0.6mm"}}),
            )
            .unwrap();
    }
    shell.run_check(CheckKind::Drc).unwrap();
    let text: Vec<_> = violation_lines(&shell)
        .into_iter()
        .map(|(_, t)| t)
        .collect();
    for expected in [
        "The top mask opening has collapsed",
        "The bottom mask opening has collapsed",
        "The top paste opening has collapsed",
        "The bottom paste opening has collapsed",
    ] {
        assert!(
            text.iter().any(|t| t == expected),
            "missing {expected}: {text:?}"
        );
    }
    let report = shell.run("drc.run", json!({})).unwrap();
    let arguments: Vec<_> = report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == "opening_collapsed")
        .map(|v| {
            (
                v["args"]["side"].as_str().unwrap(),
                v["args"]["kind"].as_str().unwrap(),
            )
        })
        .collect();
    for expected in [
        ("top", "mask"),
        ("bottom", "mask"),
        ("top", "paste"),
        ("bottom", "paste"),
    ] {
        assert!(arguments.contains(&expected));
    }
}

#[test]
fn opening_a_board_runs_the_health_checks_without_opening_messages_or_dirtying_it() {
    let home = TempDir::new("messages-health-home");
    let work = TempDir::new("messages-health-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Demo").unwrap();
    shell
        .run(
            "pcb.track.add",
            json!({"layer": "copper_top", "width": "0.25mm",
                   "points": [{"x": "10mm", "y": "10mm"}, {"x": "15mm", "y": "10mm"}]}),
        )
        .unwrap();
    shell.run("project.save", json!({})).unwrap();
    let folder = work.path().join("Demo");

    // Opened as saved: nothing to report, and the layout it opens with.
    let mut clean = Shell::new(engine_in(home.path()));
    clean.open_project(&folder).unwrap();
    assert!(clean.message_groups().is_empty());
    let layout = clean.layout().clone();
    drop(clean);

    // A second, zero-length Track, as a hand edit or merge can leave.
    let path = folder.join("Demo.redapcb");
    let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut zero = file["tracks"][0].clone();
    zero["uuid"] = json!("00000000-0000-4000-8000-0000000fff01");
    zero["geometry"]["to"] = zero["geometry"]["from"].clone();
    file["tracks"].as_array_mut().unwrap().push(zero);
    std::fs::write(&path, serde_json::to_vec_pretty(&file).unwrap()).unwrap();

    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&folder).unwrap();
    let groups = shell.message_groups();
    assert_eq!(groups.len(), 1, "{groups:?}");
    let texts: Vec<&str> = groups[0].lines.iter().map(|l| l.text.as_str()).collect();
    // Only the health checks ran: the other Track's dangling ends are not listed.
    assert_eq!(texts, ["Track with no length"]);
    assert_eq!(shell.layout(), &layout, "Messages is not opened");
    assert!(!shell.engine().is_dirty());
    // It runs no Command, so the Journal holds only the user's.
    assert!(shell.engine().journal().iter().all(|e| e.id != "drc.run"));
}
