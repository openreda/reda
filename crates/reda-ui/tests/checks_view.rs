//! Check results as view state: `view.checks.clear`, and the `checks.max_markers` cap on
//! the markers drawn.

use std::path::Path;

use egui_kittest::Harness;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::messages::{CheckKind, LineKey};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

/// A Project whose first sheet has three lone Net Labels, so ERC finds three Violations
/// there. Returns the shell and that sheet.
fn three_labels(home: &Path, work: &Path) -> (Shell, Uuid) {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, "Demo").unwrap();
    let sheet = shell.document_tabs()[0].document;
    for (i, name) in ["A", "B", "C"].iter().enumerate() {
        shell
            .run(
                "schematic.net_label.add",
                json!({"document": sheet, "name": name, "at": at(500, 500 + 500 * i as i64)}),
            )
            .unwrap();
    }
    (shell, sheet)
}

fn violation_lines(shell: &Shell) -> usize {
    shell
        .message_groups()
        .iter()
        .flat_map(|g| &g.lines)
        .filter(|l| matches!(l.key, LineKey::Violation(..)))
        .count()
}

#[test]
fn clearing_checks_empties_the_list_and_markers_and_the_next_run_lists_them_again() {
    let home = TempDir::new("checks-clear-home");
    let work = TempDir::new("checks-clear-work");
    let (mut shell, sheet) = three_labels(home.path(), work.path());
    shell.run_check(CheckKind::Erc).unwrap();
    let listed = violation_lines(&shell);
    assert!(listed >= 3, "{listed}");
    assert!(!shell.markers(sheet).is_empty());

    let dirty = shell.engine().dirty_scopes();
    shell.run("view.checks.clear", json!({})).unwrap();
    assert_eq!(violation_lines(&shell), 0);
    assert!(shell.messages().run(CheckKind::Erc).is_none());
    assert!(shell.markers(sheet).is_empty());
    // View state: it edits nothing.
    assert_eq!(shell.engine().dirty_scopes(), dirty);

    shell.run_check(CheckKind::Erc).unwrap();
    assert_eq!(violation_lines(&shell), listed);
    assert!(!shell.markers(sheet).is_empty());
}

#[test]
fn max_markers_defaults_to_500() {
    let home = TempDir::new("checks-cap-default");
    let work = TempDir::new("checks-cap-default-work");
    let (mut shell, _) = three_labels(home.path(), work.path());
    let pref = shell
        .run("app.preferences.get", json!({"key": "checks.max_markers"}))
        .unwrap();
    assert_eq!(pref["value"], 500);
    assert_eq!(pref["default"], 500);
    assert_eq!(
        reda_ui::engine::preferences::pages::page_of("checks.max_markers"),
        "checks"
    );
}

#[test]
fn markers_beyond_max_markers_are_not_drawn_but_counted_and_still_listed() {
    let home = TempDir::new("checks-cap-home");
    let work = TempDir::new("checks-cap-work");
    let (mut shell, sheet) = three_labels(home.path(), work.path());
    shell.run_check(CheckKind::Erc).unwrap();
    let all = shell.markers(sheet).len();
    assert!(all >= 3);
    assert_eq!(shell.markers_not_shown(sheet), 0);

    shell
        .run(
            "app.preferences.set",
            json!({"key": "checks.max_markers", "value": 2}),
        )
        .unwrap();
    assert_eq!(shell.markers(sheet).len(), 2);
    assert_eq!(shell.markers_not_shown(sheet), all - 2);
    // The list is never capped.
    assert_eq!(violation_lines(&shell), all);

    // The Violation last gone to always keeps its marker.
    let last = shell
        .message_groups()
        .iter()
        .flat_map(|g| g.lines.clone())
        .rfind(|l| matches!(l.key, LineKey::Violation(..)))
        .unwrap()
        .key;
    shell.go_to(last);
    assert_eq!(shell.markers(sheet).len(), 2);
    assert!(shell.markers(sheet).iter().any(|m| m.current));
}

/// Every text the last frame painted.
fn painted_texts(h: &Harness<'_, App>) -> Vec<String> {
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
                _ => {}
            }
        }
    }
    out
}

#[test]
fn the_canvas_says_how_many_markers_are_not_shown() {
    let home = TempDir::new("checks-cap-canvas");
    let work = TempDir::new("checks-cap-canvas-work");
    let (mut shell, sheet) = three_labels(home.path(), work.path());
    shell
        .run(
            "app.preferences.set",
            json!({"key": "checks.max_markers", "value": 1}),
        )
        .unwrap();
    shell.run_check(CheckKind::Erc).unwrap();
    let hidden = shell.markers_not_shown(sheet);
    assert!(hidden >= 2);
    let mut h = Harness::builder()
        .with_size([1200.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    let label = format!("{hidden} not shown");
    assert!(painted_texts(&h).contains(&label), "no {label:?} drawn");
}

/// How many line segments the last frame painted.
fn painted_segments(h: &Harness<'_, App>) -> usize {
    let mut n = 0;
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::LineSegment { .. } => n += 1,
                _ => {}
            }
        }
    }
    n
}

/// Two Tracks of different Nets 0.1 mm apart on the top copper: a clearance Violation.
fn clearance_board(home: &Path, work: &Path) -> (Harness<'static, App>, Uuid, Uuid) {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, "Demo").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    for (name, y) in [("A", "0mm"), ("B", "0.35mm")] {
        let out = shell.run("test.net.add", json!({"name": name})).unwrap();
        let net = out["result"]["uuid"].clone();
        shell
            .run(
                "pcb.track.add",
                json!({"layer": "copper_top", "net": net, "width": "0.25mm",
                       "points": [{"x": 0, "y": y}, {"x": "5mm", "y": y}]}),
            )
            .unwrap();
    }
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    shell.run_check(CheckKind::Drc).unwrap();
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
        )
        .unwrap()
        .uuid;
    let mut h = Harness::builder()
        .with_size([1200.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    (h, pcb, top)
}

/// Centres the canvas on the Violation's marker at `nm_per_point`.
fn look_at(h: &mut Harness<'static, App>, pcb: Uuid, nm_per_point: f64) {
    let at = h.state().shell().markers(pcb)[0].at;
    h.state_mut()
        .shell_mut()
        .document_tab_mut(pcb)
        .unwrap()
        .camera = Some(reda_ui::canvas::Camera {
        center: (at.x.0 as f64, at.y.0 as f64),
        nm_per_point,
    });
    h.run();
}

#[test]
fn zoomed_in_a_marker_adds_its_value_and_only_its_layers_show_it() {
    let home = TempDir::new("checks-detail");
    let work = TempDir::new("checks-detail-work");
    let (mut h, pcb, top) = clearance_board(home.path(), work.path());
    let has_value = |h: &Harness<'_, App>| {
        painted_texts(h)
            .iter()
            .any(|t| t.starts_with("0.1mm under "))
    };
    look_at(&mut h, pcb, 5_000.0);
    assert!(
        has_value(&h),
        "zoomed in, the value shows: {:?}",
        painted_texts(&h)
    );
    look_at(&mut h, pcb, 100_000.0);
    assert!(!has_value(&h), "zoomed out, only the ring");

    // Hiding the Violation's Layer hides its marker.
    look_at(&mut h, pcb, 5_000.0);
    h.state_mut()
        .shell_mut()
        .document_tab_mut(pcb)
        .unwrap()
        .layers
        .hidden
        .insert(top);
    h.run();
    assert!(!has_value(&h));
}

#[test]
fn the_overlay_colours_the_offending_objects_when_its_check_has_overlay_on() {
    let home = TempDir::new("checks-overlay");
    let work = TempDir::new("checks-overlay-work");
    let (mut h, pcb, _) = clearance_board(home.path(), work.path());
    look_at(&mut h, pcb, 5_000.0);
    // Clearance has Overlay on by default: the cross pattern hatches both Tracks.
    let with = painted_segments(&h);
    h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "checks.display.clearance.overlay", "value": false}),
        )
        .unwrap();
    h.run();
    let without = painted_segments(&h);
    assert!(with > without + 20, "{with} vs {without}");
}
