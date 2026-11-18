//! The Command Palette, the menus and the bars fit their text: in a 1280 × 800 window, in
//! Comfortable and Compact and in light and dark, no text is cut short with an ellipsis,
//! none runs out of the row or list that clips it, and no label breaks over lines.
//!
//! Drawn in the window on a sheet: the Active Bar and its grouped form with a group's
//! menu, every main menu, the canvas's context menu, the Palette on its Documents, its
//! Commands and its prefixes, and the Keys in Tool card.

#[path = "support/text_size.rs"]
mod text_size;

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::bars::{self, DocumentType};
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::shell::Shell;
use reda_ui::theme::{self, Density, Settings};
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

/// A laptop's window.
const WINDOW: [f32; 2] = [1280.0, 800.0];

fn test_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

/// The window on a new Project with two resistors on its open sheet.
fn window(dark: bool, density: Density, grouped: bool) -> (Harness<'static, App>, TempDir) {
    let dir = TempDir::new("bars-fit");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
        .starter_library(Some(test_library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Fit").unwrap();
    for x in ["0mm", "10mm"] {
        shell
            .run(
                "schematic.component.place",
                json!({"document": SHEET, "part": R_10K, "at": {"x": x, "y": 0}}),
            )
            .unwrap();
    }
    shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();
    shell
        .run(
            "app.preferences.set",
            json!({"key": "display.group_active_bar_tools", "value": grouped}),
        )
        .unwrap();
    let mut h = Harness::builder()
        .with_size(WINDOW)
        .with_step_dt(0.01)
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    let settings = Settings {
        density,
        text_size: text_size::get(),
        ..Settings::default()
    };
    theme::set(&h.ctx, &settings);
    h.ctx.set_theme(if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    h.run_steps(10);
    (h, dir)
}

/// What is wrong with the text drawn wholly inside `area`, one line per text.
fn misfits(h: &Harness<'_, App>, area: Rect) -> Vec<String> {
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        let mut todo = vec![&clipped.shape];
        while let Some(shape) = todo.pop() {
            let text = match shape {
                egui::Shape::Vec(inner) => {
                    todo.extend(inner);
                    continue;
                }
                egui::Shape::Text(text) => text,
                _ => continue,
            };
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            let clip = clipped.clip_rect;
            if !area.contains_rect(rect) || !clip.intersects(rect) {
                continue;
            }
            let said = text.galley.text();
            if said.trim().is_empty() {
                continue;
            }
            if text.galley.elided {
                out.push(format!("cut short: {said:?}"));
            }
            if rect.right() > clip.right() + 0.5 || rect.left() < clip.left() - 0.5 {
                out.push(format!(
                    "runs out sideways by {}: {said:?}",
                    rect.right() - clip.right()
                ));
            }
            let sentence = said.trim_end().ends_with(['.', '?']) || said.split(' ').count() > 8;
            if text.galley.rows.len() > 1 && !said.contains('\n') && !sentence {
                out.push(format!("wraps: {said:?}"));
            }
        }
    }
    out
}

/// The rects of the buttons, labels and fields AccessKit sees now.
fn nodes(h: &Harness<'_, App>) -> Vec<(Role, String, Rect)> {
    let mut out = Vec::new();
    for role in [Role::Button, Role::Label, Role::TextInput] {
        for node in h.query_all_by_role(role) {
            let label = node.accesskit_node().label().unwrap_or_default();
            out.push((role, label, node.rect()));
        }
    }
    out
}

/// What showed since `before`: the union of the nodes that are new.
fn shown_since(h: &Harness<'_, App>, before: &[(Role, String, Rect)]) -> Rect {
    let mut area = Rect::NOTHING;
    for node in nodes(h) {
        if !before.contains(&node) {
            area = area.union(node.2);
        }
    }
    area
}

fn check_in(name: &str, h: &Harness<'_, App>, area: Rect, wrong: &mut Vec<String>) {
    assert!(area.is_positive(), "{name}: nothing showed");
    wrong.extend(
        misfits(h, area)
            .into_iter()
            .map(|line| format!("{name}: {line}")),
    );
}

/// The Active Bar: its buttons and their frame.
fn active_bar(h: &Harness<'_, App>) -> Rect {
    let canvas = h.get_by_role(Role::Canvas).rect();
    let top =
        canvas.top() + bars::draw::active_bar_bottom(&reda_ui::theme::TokensExt::tokens(&h.ctx));
    Rect::from_min_max(canvas.left_top(), egui::pos2(canvas.right(), top))
}

fn menus(dark: bool, density: Density, wrong: &mut Vec<String>) {
    let (mut h, _dir) = window(dark, density, false);
    check_in("Active Bar", &h, active_bar(&h), wrong);
    let titles: Vec<String> = bars::for_document(Some(DocumentType::Schematic))
        .menus
        .iter()
        .map(|m| reda_ui::i18n::translate(&m.title, &[]))
        .collect();
    for title in titles {
        let (mut h, _dir) = window(dark, density, false);
        let before = nodes(&h);
        h.get_by_role_and_label(Role::Button, &title).click();
        h.run_steps(10);
        let area = shown_since(&h, &before);
        check_in(&format!("{title} menu"), &h, area, wrong);
    }

    // The canvas's context menu.
    let canvas = h.get_by_role(Role::Canvas).rect();
    let at = canvas.center() + egui::vec2(0.0, 120.0);
    let before = nodes(&h);
    h.hover_at(at);
    h.run();
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.event(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(10);
    let area = shown_since(&h, &before);
    check_in("Context menu", &h, area, wrong);
}

fn palette(dark: bool, density: Density, wrong: &mut Vec<String>) {
    let (mut h, _dir) = window(dark, density, false);
    for (name, query) in [("Documents", ""), ("Commands", ">"), ("Prefixes", "?")] {
        let before = nodes(&h);
        h.key_press_modifiers(Modifiers::COMMAND, Key::P);
        h.run_steps(4);
        h.get_by_role_and_label(Role::TextInput, "Command Palette")
            .type_text(query);
        h.run_steps(10);
        let area = shown_since(&h, &before);
        check_in(&format!("Palette ({name})"), &h, area, wrong);
        h.key_press(Key::Escape);
        h.run_steps(10);
    }
}

fn keys_card(dark: bool, density: Density, wrong: &mut Vec<String>) {
    let (mut h, _dir) = window(dark, density, false);
    h.state_mut()
        .shell_mut()
        .run("tool.activate:wire", json!({}))
        .unwrap();
    h.run();
    let before = nodes(&h);
    h.key_press_modifiers(Modifiers::SHIFT, Key::F1);
    h.run_steps(10);
    let area = shown_since(&h, &before);
    check_in("Keys in Tool", &h, area, wrong);
}

fn grouped(dark: bool, density: Density, wrong: &mut Vec<String>) {
    let (mut h, _dir) = window(dark, density, true);
    check_in("Grouped Active Bar", &h, active_bar(&h), wrong);
    // A group's menu, from its corner.
    let opener = h
        .get_all_by_role(Role::Button)
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.ends_with(" Tools") && l != "More Tools")
        })
        .map(|n| n.rect())
        .expect("a group's corner");
    let before = nodes(&h);
    h.hover_at(opener.center());
    h.run();
    h.drag_at(opener.center());
    h.drop_at(opener.center());
    h.run_steps(10);
    let area = shown_since(&h, &before);
    check_in("A group's menu", &h, area, wrong);
}

fn check(dark: bool, density: Density) {
    let mut wrong = Vec::new();
    menus(dark, density, &mut wrong);
    palette(dark, density, &mut wrong);
    keys_card(dark, density, &mut wrong);
    grouped(dark, density, &mut wrong);
    let theme = if dark { "dark" } else { "light" };
    assert!(
        wrong.is_empty(),
        "{theme} {density:?} {} px: text does not fit:\n{}",
        text_size::get(),
        wrong.join("\n")
    );
}

#[test]
fn the_palette_menus_and_bars_fit_their_text_in_dark_comfortable() {
    check(true, Density::Comfortable);
}

#[test]
fn the_palette_menus_and_bars_fit_their_text_in_dark_compact() {
    check(true, Density::Compact);
}

#[test]
fn the_palette_menus_and_bars_fit_their_text_in_light_comfortable() {
    check(false, Density::Comfortable);
}

#[test]
fn the_palette_menus_and_bars_fit_their_text_in_light_compact() {
    check(false, Density::Compact);
}

#[test]
fn the_palette_menus_and_bars_fit_their_text_in_light_comfortable_at_the_smallest_text_size() {
    text_size::at(text_size::SMALLEST, || check(false, Density::Comfortable));
}

#[test]
fn the_palette_menus_and_bars_fit_their_text_in_dark_compact_at_the_smallest_text_size() {
    text_size::at(text_size::SMALLEST, || check(true, Density::Compact));
}

#[test]
fn the_palette_menus_and_bars_fit_their_text_in_dark_comfortable_at_the_largest_text_size() {
    text_size::at(text_size::LARGEST, || check(true, Density::Comfortable));
}

#[test]
fn the_palette_menus_and_bars_fit_their_text_in_light_compact_at_the_largest_text_size() {
    text_size::at(text_size::LARGEST, || check(false, Density::Compact));
}
