//! Grids, the HUD and the status bar in the window, driven through AccessKit with
//! `egui_kittest`: the status bar's segments in the spec's order, its grid menu and snap
//! button, X running right to left on a board seen from below, the HUD over every canvas,
//! `Mod`+Shift+G's custom grid form, the Preferences grid tables, and the Properties Grid
//! region's visible grid switch and style.

use std::path::Path;

use egui::accesskit::Role;
use egui::{Key, Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::Nm;
use reda_ui::shell::Shell;
use serde_json::json;

const MIL: i64 = 25_400;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn harness(home: &Path, work: &Path) -> Harness<'static, App> {
    sized(home, work, [1400.0, 900.0])
}

fn sized(home: &Path, work: &Path, size: [f32; 2]) -> Harness<'static, App> {
    let mut shell = Shell::new(engine_in(home));
    shell.new_project(work, "Grid").unwrap();
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn sheet(h: &Harness<'static, App>) -> Uuid {
    h.state().shell().active_document().unwrap().document
}

fn open_board(h: &mut Harness<'static, App>) -> Uuid {
    let shell = h.state_mut().shell_mut();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    h.run();
    pcb
}

/// The canvas's centre, offset by `dx` points.
fn over_canvas(h: &mut Harness<'static, App>, dx: f32) {
    let rect = h.get_by_role(Role::Canvas).rect();
    let at = Pos2::new(rect.center().x + dx, rect.center().y);
    h.event(egui::Event::PointerMoved(at));
    h.run();
}

/// The status bar's cursor segment.
fn cursor_text(h: &Harness<'static, App>) -> String {
    let node = h.get_by_label_contains("Cursor: ");
    node.accesskit_node()
        .label()
        .unwrap()
        .trim_start_matches("Cursor: ")
        .to_owned()
}

fn x_of(text: &str) -> f64 {
    text.split_whitespace().nth(1).unwrap().parse().unwrap()
}

/// The left edge of the status bar node labelled `label` (the lowest, when the canvas
/// has one of the same name).
fn left(h: &Harness<'static, App>, label: &str) -> f32 {
    h.get_all_by_label(label)
        .max_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y))
        .unwrap()
        .rect()
        .min
        .x
}

#[test]
fn the_status_bar_segments_run_in_the_specs_order() {
    let home = TempDir::new("gh-order-home");
    let work = TempDir::new("gh-order-work");
    let mut h = harness(home.path(), work.path());
    let pcb = open_board(&mut h);
    over_canvas(&mut h, 0.0);
    let cursor = format!("Cursor: {}", cursor_text(&h));
    let layer = h
        .get_all_by_label_contains("Status bar Layer: ")
        .max_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y))
        .unwrap()
        .accesskit_node()
        .label()
        .unwrap();
    h.state_mut()
        .shell_mut()
        .run("tool.activate:route", json!({}))
        .unwrap();
    h.run();
    let prompt = h
        .get_all_by_label_contains("Tool: ")
        .next()
        .unwrap()
        .accesskit_node()
        .label()
        .unwrap();
    let order = [
        "Active: PCB Document",
        cursor.as_str(),
        "Grid: 0.5 mm",
        "Object snap: Active Layer",
        layer.as_str(),
        prompt.as_str(),
        "Panels",
    ];
    let xs: Vec<f32> = order.iter().map(|l| left(&h, l)).collect();
    assert!(xs.windows(2).all(|w| w[0] < w[1]), "{order:?} at {xs:?}");

    // The snap segment cycles when clicked; Off leaves it blank.
    h.get_by_label("Object snap: Active Layer").click();
    h.run();
    assert!(h.query_by_label("Object snap: All Layers").is_some());
    h.get_by_label("Object snap: All Layers").click();
    h.run();
    let off = h.get_by_label("Object snap: Off");
    assert_eq!(off.accesskit_node().value().unwrap_or_default(), "");
    let _ = pcb;
}

#[test]
fn the_grid_segment_opens_the_grid_menu() {
    let home = TempDir::new("gh-menu-home");
    let work = TempDir::new("gh-menu-work");
    let mut h = harness(home.path(), work.path());
    let sheet = sheet(&h);
    h.get_by_label("Grid: 50 mil").click();
    h.run();
    h.get_all_by_label("100 mil").last().unwrap().click();
    h.run();
    assert_eq!(h.state().shell().grid(sheet), Nm(100 * MIL));
    h.get_by_label("Grid: 100 mil").click();
    h.run();
    h.get_all_by_label("Show visible grid")
        .last()
        .unwrap()
        .click();
    h.run();
    let tab = h.state().shell().document_tab(sheet).unwrap();
    assert!(!tab.grid_visible.shown);
    // Custom… opens the custom grid form.
    h.get_by_label("Grid: 100 mil").click();
    h.run();
    h.get_all_by_label("Custom…").last().unwrap().click();
    h.run();
    assert!(h.query_by_label_contains("Custom Snap Grid").is_some());
}

#[test]
fn mod_shift_g_asks_for_a_custom_grid_with_units() {
    let home = TempDir::new("gh-custom-home");
    let work = TempDir::new("gh-custom-work");
    let mut h = harness(home.path(), work.path());
    let sheet = sheet(&h);
    over_canvas(&mut h, 0.0);
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::G);
    h.run();
    assert!(h.query_by_label_contains("Custom Snap Grid").is_some());
    h.get_by_role_and_label(Role::TextInput, "Grid").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Grid")
        .type_text("20mil");
    h.run();
    h.get_all_by_label("Run").last().unwrap().click();
    h.run();
    assert_eq!(h.state().shell().grid(sheet), Nm(20 * MIL));
    assert_eq!(h.state().shell().electrical_range(sheet), Nm(16 * MIL));
}

#[test]
fn x_runs_right_to_left_on_a_board_seen_from_below() {
    let home = TempDir::new("gh-flip-home");
    let work = TempDir::new("gh-flip-work");
    let mut h = harness(home.path(), work.path());
    open_board(&mut h);
    over_canvas(&mut h, -100.0);
    let left_x = x_of(&cursor_text(&h));
    over_canvas(&mut h, 100.0);
    let right_x = x_of(&cursor_text(&h));
    assert!(right_x > left_x, "from above X runs left to right");
    h.state_mut()
        .shell_mut()
        .run("pcb.view.from_bottom", json!({"from_bottom": true}))
        .unwrap();
    h.run();
    over_canvas(&mut h, -100.0);
    let left_x = x_of(&cursor_text(&h));
    over_canvas(&mut h, 100.0);
    let right_x = x_of(&cursor_text(&h));
    assert!(
        right_x < left_x,
        "flipped, X runs right to left: {left_x} then {right_x}"
    );
    // Three decimals for mm.
    let text = cursor_text(&h);
    assert_eq!(
        text.split_whitespace()
            .nth(1)
            .unwrap()
            .split('.')
            .nth(1)
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn the_hud_shows_over_the_sheet_and_the_board_and_shift_h_hides_it() {
    let home = TempDir::new("gh-hud-home");
    let work = TempDir::new("gh-hud-work");
    let mut h = harness(home.path(), work.path());
    over_canvas(&mut h, 0.0);
    assert!(
        h.query_by_label("HUD").is_some(),
        "on by default on a sheet"
    );
    open_board(&mut h);
    over_canvas(&mut h, 0.0);
    assert!(h.query_by_label("HUD").is_some(), "and on the board");
    h.key_press_modifiers(Modifiers::SHIFT, Key::H);
    h.run();
    assert!(h.query_by_label("HUD").is_none());
    // Parked rather than following the cursor, it still shows on the active canvas.
    h.key_press_modifiers(Modifiers::SHIFT, Key::H);
    h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "editing.hud.follow_cursor", "value": false}),
        )
        .unwrap();
    h.run();
    let canvas = h.get_by_role(Role::Canvas).rect();
    let hud = h.get_by_label("HUD").rect();
    assert!(hud.min.x < canvas.min.x + 40.0 && hud.max.y > canvas.max.y - 120.0);
}

/// The box of the tooltip showing now: a tooltip node has no bounds of its own, so its
/// contents' bounds make it.
fn tooltip(h: &Harness<'static, App>) -> Option<egui::Rect> {
    let rect = |n: egui_kittest::Node<'_>| {
        let b = n.accesskit_node().bounding_box()?;
        Some(egui::Rect::from_min_max(
            Pos2::new(b.x0 as f32, b.y0 as f32),
            Pos2::new(b.x1 as f32, b.y1 as f32),
        ))
    };
    h.query_all_by_role(Role::Tooltip).find_map(|t| {
        t.query_all(egui_kittest::kittest::by())
            .filter_map(rect)
            .reduce(|a, b| a.union(b))
    })
}

/// Crosses the canvas up to the Active Bar's `label` button and rests there until its
/// tooltip opens.
fn rest_on_bar_button(h: &mut Harness<'static, App>, label: &str) {
    let canvas = h.get_by_role(Role::Canvas).rect();
    let button = h
        .get_all_by_role_and_label(Role::Button, label)
        .map(|n| n.rect())
        .find(|r| canvas.contains_rect(*r))
        .unwrap();
    for dy in [150.0, 30.0, 0.0] {
        h.event(egui::Event::PointerMoved(
            button.center() + egui::vec2(0.0, dy),
        ));
        h.run_steps(5);
    }
    // Past egui's half-second tooltip delay.
    h.run_steps(40);
}

#[test]
fn the_hud_keeps_off_the_active_bar_so_its_tooltips_show_whole() {
    let home = TempDir::new("gh-hud-bar-home");
    let work = TempDir::new("gh-hud-bar-work");
    let mut h = harness(home.path(), work.path());
    for (document, label) in [("sheet", "Wire"), ("board", "Route Tracks")] {
        if document == "board" {
            open_board(&mut h);
        }
        rest_on_bar_button(&mut h, label);
        let tip = tooltip(&h).unwrap_or_else(|| panic!("{document}: no tooltip on {label}"));
        if let Some(hud) = h.query_by_label("HUD") {
            let hud = hud.rect();
            assert!(
                !hud.intersects(tip),
                "{document}: the HUD {hud:?} covers {label}'s tooltip {tip:?}"
            );
        }
        // Back on the canvas, the HUD follows the pointer again.
        over_canvas(&mut h, 0.0);
        assert!(h.query_by_label("HUD").is_some(), "{document}: no HUD");
    }
}

#[test]
fn the_properties_grid_region_switches_the_visible_grid_and_its_style() {
    let home = TempDir::new("gh-props-home");
    let work = TempDir::new("gh-props-work");
    // The overview with nothing selected is long.
    let mut h = sized(home.path(), work.path(), [1400.0, 2600.0]);
    let sheet = sheet(&h);
    h.get_all_by_label("Visible grid").last().unwrap().click();
    h.run();
    assert!(
        !h.state()
            .shell()
            .document_tab(sheet)
            .unwrap()
            .grid_visible
            .shown
    );
    h.get_by_role_and_label(Role::ComboBox, "Grid style")
        .click();
    h.run();
    h.get_all_by_label("Lines").last().unwrap().click();
    h.run();
    assert_eq!(
        h.state().shell().preferences().editing.grid.style,
        reda_ui::engine::preferences::GridStyle::Lines
    );
}

#[test]
fn the_preferences_grid_page_edits_the_tables_and_restores_the_shipped_presets() {
    let home = TempDir::new("gh-prefs-home");
    let work = TempDir::new("gh-prefs-work");
    let mut h = sized(home.path(), work.path(), [1400.0, 2600.0]);
    h.get_by_label("File").click();
    h.run();
    h.get_all_by_label_contains("Preferences…")
        .last()
        .unwrap()
        .click();
    h.run();
    // The dialog's page tree draws after the Properties panel's Grid region.
    h.get_all_by_label("Grid")
        .rfind(|n| n.accesskit_node().role() == Role::TreeItem)
        .unwrap()
        .click();
    h.run();
    // The first cell reading 8mil is the sheet's imperial 10 mil row's Electrical Range.
    h.get_all_by_role(Role::TextInput)
        .find(|n| n.accesskit_node().value().as_deref() == Some("8mil"))
        .unwrap()
        .click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_all_by_role(Role::TextInput)
        .find(|n| n.accesskit_node().value().as_deref() == Some("8mil"))
        .unwrap()
        .type_text("82mil");
    h.run();
    // Commit on leaving the field.
    h.key_press(Key::Tab);
    h.run();
    let rows = |h: &Harness<'static, App>| {
        h.state()
            .shell()
            .preferences()
            .editing
            .grid
            .schematic
            .imperial
            .clone()
    };
    assert_eq!(rows(&h)[0].range.unwrap().0, Nm(82 * MIL), "typed as 82mil");
    // At or above the step it is kept, with a warning.
    assert!(
        h.query_by_label_contains("Range at or above the step")
            .is_some()
    );
    h.get_by_role_and_label(Role::CheckBox, "Schematic Sheet Imperial row 2: In cycle")
        .click();
    h.run();
    assert!(!rows(&h)[1].in_cycle);
    h.get_by_label("Add a row to Schematic Sheet Imperial")
        .click();
    h.run();
    assert_eq!(rows(&h).len(), 5);
    let restore = h.get_by_label("Restore shipped presets");
    restore.click();
    h.run();
    assert_eq!(
        rows(&h),
        reda_ui::engine::preferences::GridTables::sheet().imperial
    );
}
