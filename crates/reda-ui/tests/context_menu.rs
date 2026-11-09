//! The canvas context menu (docs/spec/v0.1/app.md#window-shell): a data file per Document
//! type, in the spec's order, read like the bars; what it shows for an empty spot, an
//! object, a multiple selection and a Violation marker; and a right-click on the board in
//! the window.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::bars::{self, Binding, DocumentType, Item};
use reda_ui::canvas::Camera;
use reda_ui::context_menu::{self, SECTION_ORDER, Shown, Target};
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::messages::CheckKind;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// A board with a GND Zone, a SIG Track and Footprints R1 and R2.
struct Board {
    shell: Shell,
    pcb: Uuid,
    zone: Uuid,
    track: Uuid,
    r1: Uuid,
    r2: Uuid,
    gnd: Uuid,
    _home: TempDir,
    _work: TempDir,
}

fn board() -> Board {
    let home = TempDir::new("context-home");
    let work = TempDir::new("context-work");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let mut run = |id: &str, params: Value| shell.run(id, params).unwrap();
    run(
        "app.preferences.set",
        json!({"key": "pcb.zone.auto_refill", "value": "off"}),
    );
    let gnd = uuid(&run("test.net.add", json!({"name": "GND"}))["result"]["uuid"]);
    let sig = uuid(&run("test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    let zone = uuid(
        &run(
            "pcb.zone.add",
            json!({"layer": "copper_top", "net": gnd, "name": "GND", "vertices": [
                {"x": "20mm", "y": "20mm"}, {"x": "30mm", "y": "20mm"},
                {"x": "30mm", "y": "30mm"}, {"x": "20mm", "y": "30mm"}]}),
        )["result"]["uuid"],
    );
    let track = uuid(
        &run(
            "pcb.track.add",
            json!({"layer": "copper_top", "net": sig, "width": "0.5mm",
                   "points": [{"x": "-5mm", "y": 0}, {"x": "5mm", "y": 0}]}),
        )["result"]["tracks"][0],
    );
    let mut footprint = |designator: &str, x: &str| {
        uuid(
            &run(
                "test.footprint.add",
                json!({"at": {"x": x, "y": "-20mm"}, "designator": designator,
                       "pads": [{"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}]}),
            )["result"]["footprint"],
        )
    };
    let r1 = footprint("R1", "-10mm");
    let r2 = footprint("R2", "10mm");
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    Board {
        shell,
        pcb,
        zone,
        track,
        r1,
        r2,
        gnd,
        _home: home,
        _work: work,
    }
}

/// Each shown entry in short: a submenu's title, or the first item's title of a row.
fn outline(shell: &Shell, document: Uuid, shown: &[Shown]) -> Vec<String> {
    shown
        .iter()
        .map(|s| match s {
            Shown::Submenu(title, _) => format!("{title} >"),
            Shown::Items(items) => titles(shell, document, items).join(", "),
        })
        .collect()
}

fn titles(shell: &Shell, document: Uuid, items: &[Item]) -> Vec<String> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::Action(a) => Some(bars::resolve_for(shell, a, Some(document)).title),
            Item::Submenu(m) => Some(format!("{} >", reda_ui::i18n::translate(&m.title, &[]))),
            Item::Separator => None,
        })
        .collect()
}

#[test]
fn every_document_type_has_a_data_file_in_the_spec_order() {
    for ty in DocumentType::ALL {
        let menu = context_menu::for_type(ty);
        let ids: Vec<&str> = menu.sections.iter().map(|s| s.id.as_str()).collect();
        let mut ranks = ids
            .iter()
            .map(|id| SECTION_ORDER.iter().position(|s| s == id).unwrap());
        let mut last = ranks.next().unwrap();
        for rank in ranks {
            assert!(rank > last, "{ty:?}: {ids:?}");
            last = rank;
        }
        for needed in [
            "object",
            "document",
            "edit",
            "select",
            "place",
            "properties",
        ] {
            assert!(ids.contains(&needed), "{ty:?} has no {needed} section");
        }
    }
    // The sheet and the board have every slot of the spec's menu.
    for ty in [DocumentType::Schematic, DocumentType::Pcb] {
        let ids: Vec<&str> = context_menu::for_type(ty)
            .sections
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(ids, SECTION_ORDER, "{ty:?}");
    }
}

#[test]
fn every_item_binds_to_a_registered_command() {
    let b = board();
    let registry = b.shell.engine().registry();
    fn walk(items: &[Item], out: &mut Vec<String>) {
        for item in items {
            match item {
                Item::Action(a) => out.push(match &a.binding {
                    Binding::Command(id) | Binding::Raw { command: id, .. } => id.clone(),
                    Binding::Preset(id) => id.split(':').next().unwrap().to_owned(),
                }),
                Item::Submenu(m) => walk(&m.items, out),
                Item::Separator => {}
            }
        }
    }
    for ty in DocumentType::ALL {
        let mut commands = Vec::new();
        for section in &context_menu::for_type(ty).sections {
            walk(&section.items, &mut commands);
            for items in section.kinds.values() {
                walk(items, &mut commands);
            }
        }
        for command in commands {
            assert!(
                registry.get(&command).is_some(),
                "{ty:?}: {command} is not a Command"
            );
        }
    }
}

#[test]
fn the_parser_refuses_sections_out_of_order_and_callbacks() {
    let item = r#"{ "id": "x", "command": "panel.show:properties" }"#;
    let ok = format!(
        r#"{{ "sections": [ {{ "id": "edit", "items": [{item}] }}, {{ "id": "properties", "items": [{item}] }} ] }}"#
    );
    assert!(context_menu::parse(&ok).is_ok());
    let swapped = format!(
        r#"{{ "sections": [ {{ "id": "properties", "items": [{item}] }}, {{ "id": "edit", "items": [{item}] }} ] }}"#
    );
    let err = context_menu::parse(&swapped).unwrap_err();
    assert!(err.contains("out of order"), "{err}");
    let callback = r#"{ "sections": [ { "id": "edit", "items": [
        { "id": "x", "command": "panel.show:properties", "on_click": "run_me" } ] } ] }"#;
    assert!(context_menu::parse(callback).is_err());
    let unknown = format!(r#"{{ "sections": [ {{ "id": "extras", "items": [{item}] }} ] }}"#);
    assert!(context_menu::parse(&unknown).is_err());
}

#[test]
fn an_empty_spot_starts_with_document_properties_in_the_spec_order() {
    let b = board();
    let shown = context_menu::shown(&b.shell, b.pcb, DocumentType::Pcb, &Target::default());
    assert_eq!(
        outline(&b.shell, b.pcb, &shown),
        [
            "Document properties",
            "Copy, Paste, Delete",
            "Select >",
            "Highlight Net, Clear Highlight, Cross-probe, Applicable rules",
            "Place >",
            "Properties",
        ]
    );
    let Shown::Submenu(_, select) = &shown[2] else {
        panic!()
    };
    assert_eq!(
        titles(&b.shell, b.pcb, select),
        ["Similar", "Connected", "All on Layer"]
    );
}

#[test]
fn an_objects_submenu_is_named_for_it_and_align_needs_two_selected() {
    let mut b = board();
    let on = |b: &Board, object: Uuid| {
        let target = Target {
            violation: None,
            object: Some(object),
        };
        outline(
            &b.shell,
            b.pcb,
            &context_menu::shown(&b.shell, b.pcb, DocumentType::Pcb, &target),
        )
    };
    assert_eq!(on(&b, b.track)[0], "Track >");
    assert_eq!(on(&b, b.zone)[0], "Zone GND >");
    assert_eq!(on(&b, b.r1)[0], "R1 >");
    assert!(!on(&b, b.r1).contains(&"Align >".to_owned()));
    b.shell
        .run("selection.set", json!({"objects": [b.r1, b.r2]}))
        .unwrap();
    let outline = on(&b, b.r1);
    assert_eq!(outline[2..4], ["Select >".to_owned(), "Align >".to_owned()]);
}

#[test]
fn a_violation_marker_starts_with_waive_and_set_severity() {
    let mut b = board();
    // A GND Track across the SIG one shorts them: the DRC finds a Violation.
    b.shell
        .run(
            "pcb.track.add",
            json!({"layer": "copper_top", "net": b.gnd, "width": "0.5mm",
                   "points": [{"x": 0, "y": "-5mm"}, {"x": 0, "y": "5mm"}]}),
        )
        .unwrap();
    b.shell.run_check(CheckKind::Drc).unwrap();
    let marker = b.shell.markers(b.pcb)[0];
    let target = Target {
        violation: Some(marker.key),
        object: None,
    };
    let shown = context_menu::shown(&b.shell, b.pcb, DocumentType::Pcb, &target);
    let Shown::Items(first) = &shown[0] else {
        panic!("{shown:?}")
    };
    assert_eq!(titles(&b.shell, b.pcb, first), ["Waive", "Set severity >"]);
    // Waive runs drc.waive on exactly that Violation.
    let Item::Action(waive) = &first[0] else {
        panic!()
    };
    let (id, params) = bars::resolve_for(&b.shell, waive, Some(b.pcb)).run.unwrap();
    let entry = b.shell.messages().entry(marker.key).unwrap();
    assert_eq!(id, "drc.waive");
    assert_eq!(
        params,
        json!({"violations": [{"check": entry.violation.check, "objects": entry.violation.objects}]})
    );
}

/// The window on the board, its canvas framed on the origin at 50 µm a point.
struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    track: Uuid,
    _dirs: (TempDir, TempDir),
}

fn window(b: Board) -> Window {
    let (pcb, track) = (b.pcb, b.track);
    let mut shell = b.shell;
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .with_max_steps(100)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h.state_mut()
        .shell_mut()
        .document_tab_mut(pcb)
        .unwrap()
        .camera = Some(Camera {
        center: (0.0, 0.0),
        nm_per_point: 50_000.0,
    });
    h.run();
    Window {
        h,
        pcb,
        track,
        _dirs: (b._home, b._work),
    }
}

fn right_click(h: &mut Harness<'_, App>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run();
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run();
    }
}

/// The menu entry `name` (its label is the name, maybe followed by its shortcut) in the
/// menu opened at `at`, below and right of it.
fn entry<'h>(h: &'h Harness<'_, App>, at: Pos2, name: &str) -> Option<egui::Rect> {
    let prefix = format!("{name} ");
    h.query_all_by(|n: &_| {
        let n: &egui_kittest::kittest::AccessKitNode<'_> = n;
        n.label()
            .is_some_and(|l| l == name || l.starts_with(&prefix))
    })
    .map(|n| n.rect())
    .find(|r| r.left() >= at.x - 1.0 && r.left() < at.x + 40.0 && r.top() >= at.y - 5.0)
}

fn canvas_point(h: &Harness<'_, App>, pcb: Uuid, p: (f64, f64)) -> Pos2 {
    let rect = h.get_by_role(Role::Canvas).rect();
    let camera = h.state().shell().document_tab(pcb).unwrap().camera.unwrap();
    camera.screen_point(rect, p)
}

#[test]
fn a_right_click_on_a_track_selects_it_and_opens_its_menu() {
    let Window {
        mut h,
        pcb,
        track,
        _dirs,
    } = window(board());
    let at = canvas_point(&h, pcb, (2_000_000.0, 0.0));
    right_click(&mut h, at);
    assert_eq!(h.state().shell().selection().objects, [track]);
    // The spec's order, top to bottom.
    let y = |h: &Harness<'_, App>, label: &str| {
        entry(h, at, label)
            .unwrap_or_else(|| panic!("{label} is in the menu"))
            .center()
            .y
    };
    let order = [
        "Track",
        "Copy",
        "Select",
        "Highlight Net",
        "Place",
        "Properties",
    ];
    for pair in order.windows(2) {
        assert!(y(&h, pair[0]) < y(&h, pair[1]), "{pair:?}");
    }
    let item = entry(&h, at, "Highlight Net").unwrap().center();
    h.event(Event::PointerMoved(item));
    h.run();
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: item,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run();
    }
    let journal = h.state().shell().engine().journal();
    let last = journal.iter().rev().find(|e| e.outcome.is_ok()).unwrap();
    assert_eq!(last.id, "view.highlight.net_of");
    assert!(entry(&h, at, "Highlight Net").is_none(), "the menu closed");
}

#[test]
fn a_right_click_on_an_empty_spot_offers_document_properties() {
    let Window {
        mut h, pcb, _dirs, ..
    } = window(board());
    let at = canvas_point(&h, pcb, (0.0, 8_000_000.0));
    right_click(&mut h, at);
    assert!(entry(&h, at, "Document properties").is_some());
    assert!(entry(&h, at, "Copy").is_some());
    assert!(entry(&h, at, "Track").is_none());
}
