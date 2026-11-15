//! The Title bar of the main window, checked through AccessKit and on what a frame paints
//! (docs/spec/v0.1/app.md#title-bar, docs/spec/v0.1/design-language.md#title-bar): one row
//! on the base with no line under it, its height in both Densities, the command field's
//! width and text, the narrow-window steps, and the muted look of a window in the
//! background.

use egui::accesskit::Role;
use egui::epaint::ClippedShape;
use egui::{Color32, Rect, Shape, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::preferences;
use reda_ui::shell::Shell;
use reda_ui::theme::TokensExt as _;
use serde_json::json;

const PROMPT: &str = "Search Documents, Components, Nets";

fn engine(home: &std::path::Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn harness_of(shell: Shell, width: f32) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([width, 640.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

/// A window `width` wide with no Project open.
fn empty(home: &TempDir, width: f32) -> Harness<'static, App> {
    harness_of(Shell::new(engine(home.path())), width)
}

/// A window `width` wide with the Project "Look" open on its first sheet.
fn with_project(home: &TempDir, work: &TempDir, width: f32) -> Harness<'static, App> {
    let mut shell = Shell::new(engine(home.path()));
    shell.new_project(work.path(), "Look").unwrap();
    harness_of(shell, width)
}

fn button(h: &Harness<'_, App>, label: &str) -> Rect {
    h.get_by_role_and_label(Role::Button, label).rect()
}

fn flatten(shape: &Shape, out: &mut Vec<Shape>) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
        other => out.push(other.clone()),
    }
}

fn shapes(h: &Harness<'_, App>) -> Vec<Shape> {
    let mut out = Vec::new();
    for ClippedShape { shape, .. } in &h.output().shapes {
        flatten(shape, &mut out);
    }
    out
}

/// The colour each painted text equal to `text` was drawn in.
fn text_colours(h: &Harness<'_, App>, text: &str) -> Vec<Color32> {
    shapes(h)
        .into_iter()
        .filter_map(|s| match s {
            Shape::Text(t) if t.galley.text() == text => Some(
                t.galley
                    .job
                    .sections
                    .first()
                    .map_or(t.fallback_color, |s| s.format.color),
            ),
            _ => None,
        })
        .collect()
}

/// The bar: from the window's top to the top of the first tab strip.
fn bar(h: &Harness<'_, App>) -> Rect {
    let save = button(h, "Save All");
    let tab = h.get_by_role_and_label(Role::Tab, "Projects").rect();
    // The harness keeps an 8 px margin around the window.
    Rect::from_min_max(pos2(8.0, 8.0), pos2(save.right(), tab.top()))
}

#[test]
fn one_row_holds_the_quick_strip_menus_field_and_gear_with_no_line_under_it() {
    let (home, work) = (
        TempDir::new("title-bar-home"),
        TempDir::new("title-bar-work"),
    );
    let h = with_project(&home, &work, 2000.0);
    let save = button(&h, "Save");
    let file = button(&h, "File");
    let help = button(&h, "Help");
    let field = button(&h, PROMPT);
    let gear = button(&h, "Preferences…");
    for r in [file, help, field, gear] {
        assert_eq!(r.center().y, save.center().y, "{r:?}");
    }
    assert!(save.right() < file.left());
    assert!(help.right() < field.left() && field.right() < gear.left());
    // Quick strip buttons are the control height, holding the 20 px bar Icons.
    assert_eq!((save.width(), save.height()), (30.0, 30.0));
    // The field is centred in the window.
    assert_eq!(field.center().x, 1000.0);
    // Nothing is drawn along the bar's bottom edge: no rule, no separator.
    let bottom = bar(&h).bottom();
    let t = h.ctx.tokens();
    for shape in shapes(&h) {
        match shape {
            Shape::LineSegment { points, stroke } => {
                let along =
                    (points[0].y - bottom).abs() <= 1.0 && (points[1].y - bottom).abs() <= 1.0;
                assert!(!along, "a line under the bar: {points:?} {stroke:?}");
            }
            Shape::Rect(r) if r.rect.height() <= 2.0 && r.rect.width() > 200.0 => {
                assert!((r.rect.center().y - bottom).abs() > 1.0, "a rule: {r:?}");
            }
            _ => {}
        }
    }
    // The bar is on the base colour.
    assert!(shapes(&h).iter().any(|s| matches!(s,
        Shape::Rect(r) if r.fill == t.color.surface.base
            && r.rect.top() == 8.0 && r.rect.width() == 2000.0 - 16.0
            && r.rect.height() == t.title_bar_height())));
}

#[test]
fn the_bar_is_40_px_comfortable_and_36_px_compact() {
    for (density, height) in [("comfortable", 40.0), ("compact", 36.0)] {
        let home = TempDir::new("title-bar-density");
        preferences::set(
            &UserDirs::under(home.path()),
            "general.density",
            json!(density),
        )
        .unwrap();
        let h = empty(&home, 1200.0);
        assert_eq!(bar(&h).height(), height, "{density}");
        assert_eq!(h.ctx.tokens().title_bar_height(), height);
        // The controls sit in its middle.
        let save = button(&h, "Save All");
        assert_eq!(save.center().y, 8.0 + height / 2.0, "{density}");
    }
}

#[test]
fn the_field_shows_the_project_and_active_document_at_rest() {
    let (home, work) = (
        TempDir::new("title-bar-home"),
        TempDir::new("title-bar-work"),
    );
    let mut h = with_project(&home, &work, 1200.0);
    h.run();
    let node = h.get_by_role_and_label(Role::Button, PROMPT);
    assert_eq!(
        node.accesskit_node().value().as_deref(),
        Some("Look — Sheet 1")
    );
    let width = node.rect().width();
    assert_eq!(width, 420.0, "as wide as it gets in a wide window");
    let t = h.ctx.tokens();
    assert_eq!(text_colours(&h, "Look — Sheet 1"), [t.color.text.primary]);
}

#[test]
fn with_no_project_the_field_says_search_commands_in_muted_text() {
    let home = TempDir::new("title-bar-home");
    let h = empty(&home, 1200.0);
    let node = h.get_by_role_and_label(Role::Button, PROMPT);
    assert_eq!(
        node.accesskit_node().value().as_deref(),
        Some("Search commands")
    );
    let t = h.ctx.tokens();
    assert_eq!(text_colours(&h, "Search commands"), [t.color.text.muted]);
}

#[test]
fn a_narrowing_window_shrinks_the_field_then_shows_its_icon_then_collapses_the_menus() {
    let home = TempDir::new("title-bar-home");
    let mut widths = Vec::new();
    for width in [1200.0, 900.0, 760.0, 600.0] {
        let h = empty(&home, width);
        let field = button(&h, PROMPT);
        let menus = h.query_by_label("File").is_some();
        let collapsed = h.query_by_label("Menu").is_some();
        assert_ne!(menus, collapsed, "at {width}");
        widths.push((width, field.width(), menus));
        // The gear and the quick strip stay put.
        assert_eq!(width - 8.0 - button(&h, "Preferences…").right(), 6.0);
        assert_eq!(button(&h, "Save").left(), 8.0 + 6.0);
    }
    let field = |i: usize| widths[i].1;
    assert_eq!(field(0), 420.0);
    assert!((180.0..420.0).contains(&field(1)), "{widths:?}");
    // Then an Icon button of the control height.
    assert_eq!(field(2), 30.0, "{widths:?}");
    assert!(widths[2].2, "the menus are still a row: {widths:?}");
    // Then the menus collapse, and the field comes back.
    assert!(!widths[3].2, "{widths:?}");
    assert!(field(3) >= 180.0, "{widths:?}");
}

#[test]
fn the_collapsed_menu_button_holds_every_menu() {
    let home = TempDir::new("title-bar-home");
    let mut h = empty(&home, 600.0);
    h.get_by_role_and_label(Role::Button, "Menu").click();
    h.run();
    for menu in ["File", "Edit", "View", "Help"] {
        h.get_by_role_and_label(Role::Button, menu);
    }
    h.get_by_role_and_label(Role::Button, "File").hover();
    h.run();
    assert!(
        h.query_all_by_label_contains("Open Project…")
            .next()
            .is_some()
    );
}

#[test]
fn the_field_s_icon_opens_the_palette() {
    let home = TempDir::new("title-bar-home");
    let mut h = empty(&home, 760.0);
    assert_eq!(button(&h, PROMPT).width(), 30.0);
    h.get_by_role_and_label(Role::Button, PROMPT).click();
    h.run();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Command Palette");
}

#[test]
fn text_and_glyphs_go_muted_while_the_window_is_in_the_background() {
    let (home, work) = (
        TempDir::new("title-bar-home"),
        TempDir::new("title-bar-work"),
    );
    let mut h = with_project(&home, &work, 1200.0);
    let t = h.ctx.tokens();
    assert_eq!(text_colours(&h, "File"), [t.color.text.primary]);
    h.input_mut().focused = false;
    h.run();
    h.run();
    assert_eq!(text_colours(&h, "File"), [t.color.text.muted]);
    assert_eq!(text_colours(&h, "Look — Sheet 1"), [t.color.text.muted]);
    // In a narrow window the collapsed menus' Glyph is muted too: no stroke on the bar
    // keeps the primary text colour.
    let mut h = empty(&home, 600.0);
    h.input_mut().focused = false;
    h.run();
    let bar = bar(&h);
    for shape in shapes(&h) {
        if let Shape::Path(p) = &shape
            && bar.contains(p.points[0])
        {
            assert_ne!(
                p.stroke.color,
                egui::epaint::ColorMode::Solid(t.color.text.primary)
            );
        }
        if let Shape::LineSegment { points, stroke } = &shape
            && bar.contains(points[0])
        {
            assert_ne!(stroke.color, t.color.text.primary, "{points:?}");
        }
    }
}
