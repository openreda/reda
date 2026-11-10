//! The Active Bar over each canvas: flat Tools, or groups by Preference, then the
//! Selection Filter and snap palette pop-ups, with a More Tools menu on a narrow canvas
//! (docs/spec/v0.1/app.md#window-shell).

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::bars::{self, DocumentType};
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::shell::Shell;
use serde_json::json;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn make_project(folder: &Path, name: &str) -> PathBuf {
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    engine
        .run("project.new", json!({"path": folder, "name": name}))
        .unwrap();
    folder.join(name).join(format!("{name}.reda"))
}

/// The window, `width` points wide, on a new Project's sheet.
fn window(home: &Path, work: &Path, width: f32) -> Harness<'static, App> {
    let file = make_project(work, "Bar");
    let app = App::new(Shell::new(engine_in(home)));
    let mut h = Harness::builder()
        .with_size([width, 640.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    h.run();
    h.state_mut().shell_mut().open_project(&file).unwrap();
    h.run();
    h
}

fn last_command(h: &Harness<'_, App>) -> String {
    h.state()
        .shell()
        .engine()
        .journal()
        .iter()
        .last()
        .unwrap()
        .id
        .clone()
}

/// The labels of the Active Bar's buttons that show on the canvas, from the left.
const BAR_LABELS: [&str; 9] = [
    "Select",
    "Move",
    "Drag",
    "Wire",
    "Net Label",
    "Power Port",
    "More Tools",
    "Selection Filter",
    "Snap Options",
];

#[test]
fn a_narrow_canvas_clips_nothing_on_the_active_bar() {
    for width in [560.0, 600.0, 640.0, 680.0, 720.0, 760.0, 1000.0] {
        let home = TempDir::new("active-bar-home");
        let work = TempDir::new("active-bar-work");
        let h = window(home.path(), work.path(), width);
        let canvas = h.get_by_role(Role::Canvas).rect();
        let mut shown = 0;
        for label in BAR_LABELS {
            let Some(button) = h.query_by_role_and_label(Role::Button, label) else {
                continue;
            };
            shown += 1;
            let rect = button.rect();
            assert!(
                canvas.contains_rect(rect),
                "{width}: {label} at {rect:?} sticks out of the canvas {canvas:?}"
            );
        }
        // More Tools and the two pop-ups, 32 px each 2 px apart, in the 8 px wider frame,
        // 8 px in from each canvas edge, need 124 px; a narrower canvas hides the bar.
        if canvas.width() < 124.0 {
            assert_eq!(shown, 0, "{width}: a bar on a {canvas:?} canvas");
            continue;
        }
        for popup in ["Selection Filter", "Snap Options"] {
            assert!(h.query_by_role_and_label(Role::Button, popup).is_some());
        }
        assert!(shown >= 3, "{width}: only {shown} buttons");
    }
}

#[test]
fn the_pop_ups_keep_their_room_when_the_tools_overflow() {
    // Select, Move, Drag | Wire, Net Label, Power Port: 32 px buttons 2 px apart and a 7 px
    // separator make a 211 px row; the two pop-ups add 68, the frame 8 and the insets 16.
    let sheet = &bars::for_document(Some(DocumentType::Schematic)).active_bar;
    assert_eq!(bars::draw::active_bar_layout(sheet, 303.0, 2), Some(7));
    // One point less: Net Label and Power Port go to More Tools.
    assert_eq!(bars::draw::active_bar_layout(sheet, 302.0, 2), Some(5));
    // More Tools and the pop-ups alone take 124 px; below that the bar hides.
    assert_eq!(bars::draw::active_bar_layout(sheet, 124.0, 2), Some(0));
    assert_eq!(bars::draw::active_bar_layout(sheet, 123.0, 2), None);
    // Without a Document there are no pop-ups.
    assert_eq!(bars::draw::active_bar_layout(sheet, 235.0, 0), Some(7));
}

/// The Tool each Active Bar item activates (a group: its items'), in order.
fn tools(items: &[bars::Item]) -> Vec<Vec<String>> {
    items
        .iter()
        .filter_map(|item| match item {
            bars::Item::Action(a) => Some(vec![preset(a)]),
            bars::Item::Submenu(menu) => Some(
                menu.items
                    .iter()
                    .filter_map(|i| match i {
                        bars::Item::Action(a) => Some(preset(a)),
                        _ => None,
                    })
                    .collect(),
            ),
            bars::Item::Separator => None,
        })
        .collect()
}

fn preset(action: &bars::Action) -> String {
    match &action.binding {
        bars::Binding::Preset(p) => p.trim_start_matches("tool.activate:").to_owned(),
        other => panic!("{}: {other:?} is not a Tool Preset", action.id),
    }
}

#[test]
fn the_flat_active_bar_holds_six_tools_on_a_sheet_and_a_board() {
    let sheet = tools(&bars::for_document(Some(DocumentType::Schematic)).active_bar);
    let first = |groups: Vec<Vec<String>>| -> Vec<String> {
        groups.into_iter().map(|g| g[0].clone()).collect()
    };
    assert_eq!(
        first(sheet),
        ["select", "move", "drag", "wire", "net_label", "power_port"]
    );
    let board = tools(&bars::for_document(Some(DocumentType::Pcb)).active_bar);
    assert_eq!(
        first(board),
        ["select", "move", "route", "via", "zone", "keepout"]
    );
}

#[test]
fn the_grouped_active_bar_lists_the_leading_suites_groups() {
    let board = tools(&bars::for_document(Some(DocumentType::Pcb)).active_bar_groups);
    assert_eq!(
        board,
        [
            vec!["select"],
            vec!["move", "move_by", "swap"],
            vec!["route"],
            vec!["via"],
            vec!["zone", "zone_cutout", "keepout"],
            vec!["board_outline", "board_outline_rect", "board_cutout"],
            vec!["free_pad"],
            vec!["text"],
            vec!["line", "arc", "rect"],
            vec!["measure", "dimension"],
        ]
    );
    let sheet = tools(&bars::for_document(Some(DocumentType::Schematic)).active_bar_groups);
    assert_eq!(
        sheet,
        [
            vec!["select"],
            vec!["move", "drag"],
            vec!["wire", "break_wire"],
            vec!["net_label", "port"],
            vec![
                "power_port",
                "power_port_gnd",
                "power_port_3v3",
                "power_port_5v",
                "power_port_vcc"
            ],
            vec!["no_connect"],
            vec!["text"],
            vec!["line", "rect"],
        ]
    );
    // The Library editors have no groups: their bar stays flat.
    for ty in [DocumentType::Symbol, DocumentType::Footprint] {
        assert!(bars::for_document(Some(ty)).active_bar_groups.is_empty());
    }
}

/// Opens `doc` in its tab and makes it active.
fn activate(h: &mut Harness<'static, App>, doc: reda_ui::engine::model::Uuid) {
    let shell = h.state_mut().shell_mut();
    shell
        .run("document.open", json!({"document": doc}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": doc.to_string()}))
        .unwrap();
    h.run();
}

fn has_button(h: &Harness<'_, App>, label: &str) -> bool {
    h.query_by_role_and_label(Role::Button, label).is_some()
}

#[test]
fn group_tools_turns_the_active_bar_into_groups_faced_by_the_last_used_tool() {
    let home = TempDir::new("active-bar-home");
    let work = TempDir::new("active-bar-work");
    let mut h = window(home.path(), work.path(), 1600.0);
    let sheet = h.state().shell().active_document().unwrap().document;
    let board = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .uuid;
    activate(&mut h, board);
    // Off by default: the flat bar, with no Board Outline.
    assert!(
        !h.state()
            .shell()
            .preferences()
            .display
            .group_active_bar_tools
    );
    assert!(has_button(&h, "Keepout"));
    assert!(!has_button(&h, "Board Outline"));
    h.get_by_role_and_label(Role::Button, "Keepout").click();
    h.run();
    assert_eq!(last_command(&h), "tool.activate:keepout", "flat click");
    h.get_by_role_and_label(Role::Button, "Select").click();
    h.run();
    h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "display.group_active_bar_tools", "value": true}),
        )
        .unwrap();
    h.run();
    // Each group shows its first Tool until another is used.
    for face in [
        "Select",
        "Move",
        "Zone",
        "Board Outline",
        "Free Pad",
        "Line",
        "Measure",
    ] {
        assert!(has_button(&h, face), "{face}");
    }
    assert!(!has_button(&h, "Keepout"));
    h.get_by_role_and_label(Role::Button, "Measure").click();
    h.run();
    assert_eq!(last_command(&h), "tool.activate:measure", "face click");
    // The group's corner lists its Tools; choosing one runs it and makes it the face.
    h.get_by_role_and_label(Role::Button, "Graphics Tools")
        .click();
    h.run();
    h.get_all_by_label("Arc").last().unwrap().click();
    h.run();
    assert_eq!(last_command(&h), "tool.activate:arc");
    assert!(has_button(&h, "Arc"));
    assert!(!has_button(&h, "Line"));
    // The face stays after another Tool, and a click on it runs it again.
    h.get_by_role_and_label(Role::Button, "Select").click();
    h.run();
    assert_eq!(last_command(&h), "tool.activate:select");
    h.get_by_role_and_label(Role::Button, "Arc").click();
    h.run();
    assert_eq!(last_command(&h), "tool.activate:arc");
    // A right-click on a face opens its group's menu too.
    h.get_by_role_and_label(Role::Button, "Board Outline")
        .click_secondary();
    h.run();
    h.get_all_by_label("Board Cutout").last().unwrap().click();
    h.run();
    assert_eq!(last_command(&h), "tool.activate:board_cutout");
    assert!(has_button(&h, "Board Cutout"));
    // A Tool chosen any other way (its key, a menu) shows on its group too.
    h.state_mut()
        .shell_mut()
        .run("tool.activate:keepout", json!({}))
        .unwrap();
    h.run();
    assert!(has_button(&h, "Keepout"));
    assert!(!has_button(&h, "Zone"));
    // Per Document type: the sheet's Graphics group still shows Line.
    h.state_mut()
        .shell_mut()
        .run("tool.activate:select", json!({}))
        .unwrap();
    activate(&mut h, sheet);
    for face in [
        "Select",
        "Move",
        "Wire",
        "Net Label",
        "Power Port",
        "No-Connect Marker",
        "Line",
    ] {
        assert!(has_button(&h, face), "{face}");
    }
    assert!(!has_button(&h, "Arc"));
    activate(&mut h, board);
    assert!(has_button(&h, "Arc"));
    assert!(has_button(&h, "Keepout"));
}
