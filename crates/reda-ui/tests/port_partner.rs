//! Jump to a Port's partner: `schematic.port.go_to_partner` (from the canvas menu or
//! Mod+double-click) opens the partner's sheet and highlights its Net, and with several
//! partners the window asks which.

use egui::accesskit::Role;
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::bars::{self, DocumentType, Item};
use reda_ui::context_menu::{self, Shown, Target};
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::highlight::Highlight;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

struct Project {
    shell: Shell,
    sheets: [Uuid; 3],
    /// CLK on sheets 1 and 2.
    clk: [Uuid; 2],
    /// DATA on sheets 1, 2 and 3.
    data: [Uuid; 3],
    /// ALONE on sheet 1 only.
    alone: Uuid,
    _home: TempDir,
    _work: TempDir,
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

fn port(shell: &mut Shell, sheet: Uuid, name: &str, x: i64) -> Uuid {
    let out = shell
        .run(
            "schematic.port.add",
            json!({"document": sheet, "name": name,
                   "at": {"x": format!("{x}mm"), "y": "50mm"}}),
        )
        .unwrap();
    uuid(&out["result"]["port"])
}

fn project() -> Project {
    let (home, work) = (TempDir::new("pp-home"), TempDir::new("pp-work"));
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    let mut sheets = [SHEET.parse().unwrap(); 3];
    for sheet in &mut sheets[1..] {
        let out = shell
            .run("document.add", json!({"kind": "schematic"}))
            .unwrap();
        *sheet = uuid(&out["result"]["uuid"]);
    }
    let clk = [
        port(&mut shell, sheets[0], "CLK", 50),
        port(&mut shell, sheets[1], "CLK", 50),
    ];
    let data = [
        port(&mut shell, sheets[0], "DATA", 100),
        port(&mut shell, sheets[1], "DATA", 100),
        port(&mut shell, sheets[2], "DATA", 100),
    ];
    let alone = port(&mut shell, sheets[0], "ALONE", 150);
    shell
        .run("document.open", json!({"document": sheets[0]}))
        .unwrap();
    Project {
        shell,
        sheets,
        clk,
        data,
        alone,
        _home: home,
        _work: work,
    }
}

fn active(shell: &Shell) -> Uuid {
    shell.layout().active_document().unwrap()
}

fn highlighted(shell: &Shell) -> Highlight {
    shell.engine().view().get::<Highlight>().unwrap().clone()
}

#[test]
fn one_partner_opens_its_sheet_and_highlights_the_net() {
    let mut p = project();
    p.shell
        .run(
            "schematic.port.go_to_partner",
            json!({"objects": [p.clk[0]]}),
        )
        .unwrap();
    assert_eq!(active(&p.shell), p.sheets[1]);
    let highlight = highlighted(&p.shell);
    assert_eq!(highlight.net.as_deref(), Some("CLK"));
    assert!(highlight.objects.contains(&p.clk[1]));
}

#[test]
fn several_partners_wait_for_a_pick_and_the_pick_goes_there() {
    let mut p = project();
    let out = p
        .shell
        .run(
            "schematic.port.go_to_partner",
            json!({"objects": [p.data[0]]}),
        )
        .unwrap();
    assert!(out["chosen"].is_null(), "{out}");
    assert_eq!(active(&p.shell), p.sheets[0], "nothing moved yet");
    let menu = reda_ui::port_partner::waiting(&p.shell).unwrap();
    let entries: Vec<String> = menu
        .partners
        .iter()
        .map(reda_ui::port_partner::entry)
        .collect();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].starts_with("Sheet 2 "), "{entries:?}");
    assert!(entries[1].starts_with("Sheet 3 "), "{entries:?}");

    p.shell
        .run(
            "schematic.port.go_to_partner",
            json!({"objects": [p.data[0]], "partner": p.data[2]}),
        )
        .unwrap();
    assert_eq!(active(&p.shell), p.sheets[2]);
    assert!(reda_ui::port_partner::waiting(&p.shell).is_none());
    assert!(highlighted(&p.shell).objects.contains(&p.data[2]));
}

#[test]
fn a_port_without_partners_or_a_non_port_is_refused() {
    let mut p = project();
    let err = p
        .shell
        .run(
            "schematic.port.go_to_partner",
            json!({"objects": [p.alone]}),
        )
        .unwrap_err();
    assert!(err.message.contains("no partner"), "{}", err.message);
    let err = p
        .shell
        .run(
            "schematic.port.go_to_partner",
            json!({"objects": [p.sheets[1]]}),
        )
        .unwrap_err();
    assert!(err.message.contains("not a Port") || err.message.contains("is a Port"));
    let err = p
        .shell
        .run(
            "schematic.port.go_to_partner",
            json!({"objects": [p.clk[0]], "partner": p.data[1]}),
        )
        .unwrap_err();
    assert_eq!(err.details.unwrap()["pointer"], "/partner");
}

#[test]
fn a_port_s_canvas_menu_offers_go_to_partner() {
    let mut p = project();
    p.shell
        .run("selection.set", json!({"objects": [p.clk[0]]}))
        .unwrap();
    let target = Target {
        violation: None,
        object: Some(p.clk[0]),
    };
    let shown = context_menu::shown(&p.shell, p.sheets[0], DocumentType::Schematic, &target);
    let mut found = None;
    for s in &shown {
        let items = match s {
            Shown::Submenu(_, items) | Shown::Items(items) => items,
        };
        for item in items {
            if let Item::Action(a) = item {
                let resolved = bars::resolve_for(&p.shell, a, Some(p.sheets[0]));
                if resolved.title == "Go to Partner Port" {
                    found = Some(resolved);
                }
            }
        }
    }
    let found = found.expect("Go to Partner Port in the Port's menu");
    let (id, params) = found.run.expect("runnable");
    p.shell.run(&id, params).unwrap();
    assert_eq!(active(&p.shell), p.sheets[1]);
}

/// The Project in a window; returns the guards that keep its folders.
fn window(p: Project) -> (Harness<'static, App>, [Uuid; 3], (TempDir, TempDir)) {
    let Project {
        shell,
        sheets,
        _home,
        _work,
        ..
    } = p;
    let mut h = Harness::builder()
        .with_step_dt(0.01)
        .with_max_steps(100)
        .with_size([1200.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    (h, sheets, (_home, _work))
}

/// Where the sheet point (mm) is on screen.
fn screen(h: &Harness<'_, App>, sheet: Uuid, x_mm: f64, y_mm: f64) -> Pos2 {
    let rect = h.get_by_role(Role::Canvas).rect();
    let camera = h
        .state()
        .shell()
        .document_tab(sheet)
        .unwrap()
        .camera
        .unwrap();
    camera.screen_point(rect, (x_mm * 1e6, y_mm * 1e6))
}

fn mod_double_click(h: &mut Harness<'_, App>, at: Pos2) {
    h.event(Event::PointerMoved(at));
    h.run();
    h.event(Event::ModifiersChanged(Modifiers::COMMAND));
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::COMMAND,
            });
            h.step();
        }
    }
    h.run();
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run();
}

#[test]
fn mod_double_click_on_a_port_jumps_and_several_partners_ask_in_a_menu() {
    let (mut h, sheets, _guards) = window(project());
    let at = screen(&h, sheets[0], 50.0, 50.0);
    mod_double_click(&mut h, at);
    assert_eq!(active(h.state().shell()), sheets[1]);

    // Back on sheet 1, DATA has two partners: the window asks which.
    h.state_mut()
        .shell_mut()
        .run("document.open", json!({"document": sheets[0]}))
        .unwrap();
    h.run();
    let at = screen(&h, sheets[0], 100.0, 50.0);
    mod_double_click(&mut h, at);
    h.get_by_label("Go to Partner Port");
    let entry = h
        .query_all_by_role(Role::Button)
        .find(|n| (n.accesskit_node().label()).is_some_and(|l| l.starts_with("Sheet 3 ")))
        .expect("a menu entry for sheet 3");
    entry.click();
    h.run();
    assert_eq!(active(h.state().shell()), sheets[2]);
    assert!(h.query_by_label("Go to Partner Port").is_none());
}
