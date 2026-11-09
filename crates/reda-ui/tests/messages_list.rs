//! The Messages list's Group By, sorting, severity chips, next and previous Violation,
//! the waived and stale-Waiver groups, Run this check only, what Violation Details does,
//! and the markers the Checks Preferences shape. Without a window.

use std::path::Path;

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{Object, Severity, Uuid};
use reda_ui::engine::preferences::{OverlayStyle, OverlayZoomedOut};
use reda_ui::messages::{CheckKind, GroupBy, GroupKind, LineKey, Note, SortBy};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build()
}

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

/// A Project whose first sheet has a lone Net Label "A" and whose second has a lone Wire:
/// ERC finds the label attached to nothing and the Wire's two dangling ends.
struct Demo {
    shell: Shell,
    sheet1: Uuid,
    sheet2: Uuid,
    label: Uuid,
    _dirs: (TempDir, TempDir),
}

fn demo() -> Demo {
    let home = TempDir::new("list-home");
    let work = TempDir::new("list-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Demo").unwrap();
    let sheet1 = shell.document_tabs()[0].document;
    let out = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    let sheet2: Uuid = out["result"]["uuid"].as_str().unwrap().parse().unwrap();
    let out = shell
        .run(
            "schematic.net_label.add",
            json!({"document": sheet1, "name": "A", "at": at(500, 500)}),
        )
        .unwrap();
    let label = out["result"]["net_label"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    shell
        .run(
            "schematic.wire.add",
            json!({"document": sheet2, "points": [at(1000, 1000), at(2000, 1000)]}),
        )
        .unwrap();
    Demo {
        shell,
        sheet1,
        sheet2,
        label,
        _dirs: (home, work),
    }
}

const LABEL: &str = "A is attached to nothing";
const FIRST_END: &str = "Wire end at (25.4mm, 25.4mm) touches nothing";
const SECOND_END: &str = "Wire end at (50.8mm, 25.4mm) touches nothing";

/// Each group as its title and line texts.
fn texts(shell: &Shell) -> Vec<(String, Vec<String>)> {
    shell
        .message_groups()
        .iter()
        .map(|g| {
            (
                g.title.clone(),
                g.lines.iter().map(|l| l.text.clone()).collect(),
            )
        })
        .collect()
}

fn group(title: &str, lines: &[&str]) -> (String, Vec<String>) {
    (
        title.to_owned(),
        lines.iter().map(|l| (*l).to_owned()).collect(),
    )
}

fn key_of(shell: &Shell, text: &str) -> LineKey {
    shell
        .message_groups()
        .into_iter()
        .flat_map(|g| g.lines)
        .find(|l| l.text == text)
        .map(|l| l.key)
        .unwrap_or_else(|| panic!("no line {text:?}"))
}

fn current_text(shell: &Shell) -> Option<String> {
    let key = shell.messages().current()?;
    Some(shell.messages().entry(key)?.text.clone())
}

#[test]
fn group_by_check_puts_each_checks_violations_together_and_severity_splits_errors_out() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    assert_eq!(d.shell.messages().group_by(), GroupBy::Document);

    d.shell.set_group_by(GroupBy::Check);
    assert_eq!(
        texts(&d.shell),
        [
            group("Dangling Wire", &[FIRST_END, SECOND_END]),
            group("Unattached Label", &[LABEL]),
        ]
    );
    let kinds: Vec<GroupKind> = d
        .shell
        .message_groups()
        .into_iter()
        .map(|g| g.kind)
        .collect();
    assert_eq!(
        kinds,
        [
            GroupKind::Check("dangling_wire".into()),
            GroupKind::Check("unattached_label".into())
        ]
    );

    d.shell
        .run(
            "erc.set_severity",
            json!({"check": "dangling_wire", "severity": "error"}),
        )
        .unwrap();
    d.shell.run_check(CheckKind::Erc).unwrap();
    d.shell.set_group_by(GroupBy::Severity);
    assert_eq!(
        texts(&d.shell),
        [
            group("Errors", &[FIRST_END, SECOND_END]),
            group("Warnings", &[LABEL]),
        ]
    );
}

#[test]
fn sorting_orders_each_groups_lines_by_message_or_errors_first() {
    let mut d = demo();
    let note = |severity, text: &str| Note {
        source: "test".into(),
        document: None,
        severity,
        text: text.into(),
    };
    d.shell.set_notes(
        "test",
        vec![
            note(Severity::Warning, "b: second note"),
            note(Severity::Error, "c: third note"),
            note(Severity::Warning, "a: first note"),
        ],
    );
    let general = |shell: &Shell| texts(shell)[0].1.clone();
    assert_eq!(
        general(&d.shell),
        ["b: second note", "c: third note", "a: first note"]
    );
    d.shell.set_sort(SortBy::Message);
    assert_eq!(
        general(&d.shell),
        ["a: first note", "b: second note", "c: third note"]
    );
    d.shell.set_sort(SortBy::Severity);
    assert_eq!(
        general(&d.shell),
        ["c: third note", "b: second note", "a: first note"]
    );
}

#[test]
fn severity_chips_count_and_hide_their_lines() {
    let mut d = demo();
    d.shell
        .run(
            "erc.set_severity",
            json!({"check": "unattached_label", "severity": "error"}),
        )
        .unwrap();
    d.shell.run_check(CheckKind::Erc).unwrap();
    assert_eq!(d.shell.messages().counts(), (1, 2));
    d.shell.set_severity_shown(Severity::Warning, false);
    assert_eq!(texts(&d.shell), [group("Sheet 1", &[LABEL])]);
    // The chips don't change the counts they show.
    assert_eq!(d.shell.messages().counts(), (1, 2));
    d.shell.set_severity_shown(Severity::Error, false);
    assert!(d.shell.message_groups().is_empty());
    d.shell.set_severity_shown(Severity::Warning, true);
    assert_eq!(
        texts(&d.shell),
        [group("Sheet 2", &[FIRST_END, SECOND_END])]
    );
}

#[test]
fn next_and_previous_violation_step_through_the_list_and_wrap_around() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let step = |shell: &mut Shell, preset: &str| {
        shell
            .run(&format!("view.messages.step:{preset}"), json!({}))
            .unwrap();
        current_text(shell)
    };
    assert_eq!(step(&mut d.shell, "next").as_deref(), Some(LABEL));
    assert_eq!(d.shell.layout().active_document(), Some(d.sheet1));
    assert_eq!(step(&mut d.shell, "next").as_deref(), Some(FIRST_END));
    assert_eq!(d.shell.layout().active_document(), Some(d.sheet2));
    assert_eq!(step(&mut d.shell, "next").as_deref(), Some(SECOND_END));
    assert_eq!(step(&mut d.shell, "next").as_deref(), Some(LABEL));
    assert_eq!(step(&mut d.shell, "previous").as_deref(), Some(SECOND_END));
    let journal: Vec<String> = (d.shell.engine().journal().iter())
        .map(|e| e.id.clone())
        .collect();
    assert!(
        journal
            .iter()
            .any(|id| id.starts_with("view.messages.step")),
        "{journal:?}"
    );
}

#[test]
fn the_keymap_binds_f4_and_shift_f4_to_next_and_previous_violation() {
    let d = demo();
    let next = reda_ui::keymap::shortcut_label(&d.shell, "view.messages.step:next", false);
    let previous = reda_ui::keymap::shortcut_label(&d.shell, "view.messages.step:previous", false);
    assert_eq!(next.as_deref(), Some("F4"));
    assert_eq!(previous.as_deref(), Some("Shift+F4"));
}

#[test]
fn a_waiver_that_matches_nothing_shows_in_a_collapsed_stale_group_with_delete() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    d.shell.waive(key_of(&d.shell, LABEL)).unwrap();
    // Deleting the label leaves its Waiver matching nothing.
    d.shell
        .run("schematic.delete", json!({"objects": [d.label]}))
        .unwrap();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let groups = d.shell.message_groups();
    let stale = groups.last().unwrap();
    assert_eq!(stale.kind, GroupKind::StaleWaivers);
    assert!(stale.collapsed);
    let texts: Vec<&str> = stale.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["Waiver of “Unattached Label” matches nothing"]);

    let key = stale.lines[0].key;
    let waiver = d.shell.messages().stale_waiver(key).unwrap().waiver;
    d.shell.unwaive(key).unwrap();
    assert!(d.shell.engine().project().unwrap().object(waiver).is_none());
    assert!(
        d.shell
            .message_groups()
            .iter()
            .all(|g| g.kind != GroupKind::StaleWaivers)
    );
}

#[test]
fn run_this_check_only_replaces_just_that_checks_lines() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    // A second lone Wire and a second lone label, not checked yet.
    d.shell
        .run(
            "schematic.wire.add",
            json!({"document": d.sheet2, "points": [at(1000, 3000), at(2000, 3000)]}),
        )
        .unwrap();
    d.shell
        .run(
            "schematic.net_label.add",
            json!({"document": d.sheet1, "name": "B", "at": at(500, 3000)}),
        )
        .unwrap();
    d.shell
        .run_check_only(CheckKind::Erc, "unattached_label")
        .unwrap();
    let last = d.shell.engine().journal().last().unwrap();
    assert_eq!(last.id, "erc.run");
    assert_eq!(
        last.params["checks"],
        json!(["unattached_label"]),
        "{:?}",
        last.params
    );
    // Only the label check ran again: B is listed, the new Wire's ends are not.
    let mut listed = texts(&d.shell);
    listed[0].1.sort();
    assert_eq!(
        listed,
        [
            group("Sheet 1", &[LABEL, "B is attached to nothing"]),
            group("Sheet 2", &[FIRST_END, SECOND_END]),
        ]
    );
}

#[test]
fn violation_details_follows_its_violation_and_highlights_jumps_and_edits_the_reason() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let key = key_of(&d.shell, LABEL);
    d.shell.open_details(key);
    assert_eq!(
        d.shell.messages().details().unwrap().check,
        "unattached_label"
    );
    assert_eq!(d.shell.details_line(), Some(key));

    d.shell.highlight_violation(key).unwrap();
    assert_eq!(
        d.shell.engine().journal().last().unwrap().id,
        "view.highlight"
    );
    d.shell.jump_to_violation(key).unwrap();
    let last = d.shell.engine().journal().last().unwrap();
    assert_eq!(last.id, "view.jump_to");
    assert_eq!(d.shell.layout().active_document(), Some(d.sheet1));

    // Waiving re-runs ERC; the window still finds its Violation, now waived.
    d.shell.waive_many(&[key], "on purpose").unwrap();
    let key = d.shell.details_line().expect("still listed, waived");
    let entry = d.shell.messages().entry(key).unwrap();
    let waiver = entry.violation.waiver.expect("waived");
    d.shell.set_waiver_reason(key, "Test point").unwrap();
    let Some(Object::Waiver(w)) = d.shell.engine().project().unwrap().object(waiver) else {
        panic!("a Waiver");
    };
    assert_eq!(w.object.reason, "Test point");
    let key = d.shell.details_line().unwrap();
    d.shell.unwaive(key).unwrap();
    let key = d.shell.details_line().unwrap();
    assert!(
        d.shell
            .messages()
            .entry(key)
            .unwrap()
            .violation
            .waiver
            .is_none()
    );
    d.shell.close_details();
    assert!(d.shell.messages().details().is_none());
}

#[test]
fn an_object_link_opens_its_document_and_selects_just_that_object() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    d.shell.go_to(key_of(&d.shell, FIRST_END));
    d.shell.go_to_object(Some(d.sheet1), d.label);
    assert_eq!(d.shell.layout().active_document(), Some(d.sheet1));
    assert_eq!(d.shell.selection().objects, [d.label]);
}

#[test]
fn waive_picks_every_mod_clicked_line_and_asks_one_reason() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    d.shell.go_to(key_of(&d.shell, LABEL));
    d.shell.toggle_picked(key_of(&d.shell, FIRST_END));
    let picked = d.shell.messages().picked();
    assert_eq!(picked.len(), 2);
    d.shell.start_waive(picked);
    assert_eq!(
        d.shell.messages().waive_prompt().unwrap().violations.len(),
        2
    );
    d.shell.set_waive_reason("bring-up".into());
    d.shell.finish_waive(true).unwrap();
    assert!(d.shell.messages().waive_prompt().is_none());
    let run = d.shell.messages().run(CheckKind::Erc).unwrap();
    // The Wire's Waiver covers both its ends.
    assert!(run.entries.iter().all(|e| e.violation.waiver.is_some()));
    // Cancel waives nothing.
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    d.shell.start_waive(vec![key_of(&d.shell, LABEL)]);
    d.shell.finish_waive(false).unwrap();
    let run = d.shell.messages().run(CheckKind::Erc).unwrap();
    assert!(run.entries.iter().all(|e| e.violation.waiver.is_none()));
}

#[test]
fn the_checks_preferences_shape_each_marker() {
    let mut d = demo();
    d.shell.run_check(CheckKind::Erc).unwrap();
    let marker = |shell: &Shell| shell.markers(d.sheet1)[0].clone();
    let m = marker(&d.shell);
    assert!(m.details);
    assert_eq!(m.overlay, None, "Overlay is off for unattached_label");
    assert!(m.layers.is_empty());

    let set = |shell: &mut Shell, key: &str, value: Value| {
        shell
            .run("app.preferences.set", json!({"key": key, "value": value}))
            .unwrap();
    };
    set(
        &mut d.shell,
        "checks.display.unattached_label.overlay",
        json!(true),
    );
    let look = marker(&d.shell).overlay.expect("overlay on");
    assert_eq!(look.style, OverlayStyle::CrossPattern);
    assert_eq!(look.zoomed_out, OverlayZoomedOut::OverrideColorDominates);
    assert_eq!(marker(&d.shell).objects, [d.label]);
    set(
        &mut d.shell,
        "checks.overlay_style",
        json!("warning_pattern"),
    );
    set(
        &mut d.shell,
        "checks.overlay_zoomed_out",
        json!("pattern_scales"),
    );
    let look = marker(&d.shell).overlay.unwrap();
    assert_eq!(look.style, OverlayStyle::WarningPattern);
    assert_eq!(look.zoomed_out, OverlayZoomedOut::PatternScales);
    set(
        &mut d.shell,
        "checks.display.unattached_label.details",
        json!(false),
    );
    assert!(!marker(&d.shell).details);
    // With neither, the Violation has no marker; it is still listed.
    set(
        &mut d.shell,
        "checks.display.unattached_label.overlay",
        json!(false),
    );
    assert!(d.shell.markers(d.sheet1).is_empty());
    assert_eq!(texts(&d.shell)[0], group("Sheet 1", &[LABEL]));
}

#[test]
fn a_drc_marker_carries_its_value_closest_points_and_layers() {
    let home = TempDir::new("list-home");
    let work = TempDir::new("list-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Demo").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    let mut net = |name: &str| -> Uuid {
        let out = shell.run("test.net.add", json!({"name": name})).unwrap();
        out["result"]["uuid"].as_str().unwrap().parse().unwrap()
    };
    let (a, b) = (net("A"), net("B"));
    for (net, y) in [(a, "0mm"), (b, "0.35mm")] {
        shell
            .run(
                "pcb.track.add",
                json!({"layer": "copper_top", "net": net, "width": "0.25mm",
                       "points": [{"x": 0, "y": y}, {"x": "5mm", "y": y}]}),
            )
            .unwrap();
    }
    shell.run_check(CheckKind::Drc).unwrap();
    let clearance = shell
        .markers(pcb)
        .iter()
        .find(|m| shell.messages().entry(m.key).unwrap().violation.check == "clearance")
        .cloned()
        .expect("a clearance marker");
    let top = shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .layers()
        .find(
            reda_ui::engine::model::LayerKind::Copper,
            Some(reda_ui::engine::model::Side::Top),
        );
    assert_eq!(clearance.layers, [top.unwrap().uuid]);
    // 0.35 mm apart, 0.25 mm wide: a 0.1 mm gap.
    let value = clearance.value.as_deref().unwrap();
    assert!(value.starts_with("0.1mm under "), "{value}");
    let [p, q] = clearance.points.expect("closest points");
    assert_eq!((q.y.0 - p.y.0).abs(), 100_000);
    // Clearance has its overlay on by default.
    assert!(clearance.overlay.is_some());
}

#[test]
fn messages_opens_only_when_a_run_finds_errors() {
    use reda_ui::layout::{PanelId, TabKey};
    let mut d = demo();
    let messages = TabKey::Panel(PanelId::Messages);
    d.shell
        .run("layout.tab.close", json!({"tab": "messages"}))
        .unwrap();
    assert!(d.shell.layout().place(&messages).is_none());
    // Only Warnings: the panel stays closed.
    d.shell.run_check(CheckKind::Erc).unwrap();
    assert!(d.shell.layout().place(&messages).is_none());

    d.shell
        .run(
            "erc.set_severity",
            json!({"check": "dangling_wire", "severity": "error"}),
        )
        .unwrap();
    d.shell.run_check(CheckKind::Erc).unwrap();
    assert!(d.shell.layout().place(&messages).is_some());
}
