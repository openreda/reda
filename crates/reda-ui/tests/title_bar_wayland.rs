//! The Title bar of a window REDA decorates itself on Linux under Wayland
//! (docs/spec/v0.1/app.md#title-bar, design-language.md#title-bar, ADR-0014): the window
//! controls at the right and the commands they send, dragging and double-clicking the
//! bar's empty space, the edge-resize strip, the round corners and rim, the maximised
//! window, and the system title bar chosen or forced.
//!
//! The harness has no compositor, so what is checked is what the window asks of it: the
//! `ViewportCommand`s it sends and the cursor it shows.

#![cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]

use egui::accesskit::Role;
use egui::epaint::{ClippedShape, RectShape};
use egui::viewport::ResizeDirection;
use egui::{CursorIcon, Event, Modifiers, PointerButton, Pos2, Shape, ViewportCommand, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::preferences_dialog::PageId;
use reda_ui::shell::Shell;
use reda_ui::theme::TokensExt as _;
use reda_ui::title_bar::Startup;

const WIDTH: f32 = 1200.0;
const HEIGHT: f32 = 720.0;
/// The harness keeps an 8 px margin around the window's `ui`; the window's own rect,
/// which the resize strip follows, is the whole screen.
const MARGIN: f32 = 8.0;

const CUSTOM: Startup = Startup {
    preference: false,
    forced: false,
};

fn window(home: &TempDir, startup: Startup, maximised: bool) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let app = App::new(Shell::new(engine)).with_title_bar(startup);
    // Frames a sixtieth of a second apart, so two clicks a frame apart are a double-click.
    let mut h = Harness::builder()
        .with_size([WIDTH, HEIGHT])
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(120)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    if maximised {
        let root = h.input_mut().viewports.entry(egui::ViewportId::ROOT);
        root.or_default().maximized = Some(true);
    }
    h.run();
    h
}

/// Runs a few frames after the queued events; returns every command the window sent.
fn commands(h: &mut Harness<'static, App>) -> Vec<ViewportCommand> {
    let mut out = Vec::new();
    for _ in 0..4 {
        h.step();
        let root = &h.output().viewport_output[&egui::ViewportId::ROOT];
        out.extend(root.commands.iter().cloned());
    }
    out
}

fn press(h: &Harness<'static, App>, at: Pos2, button: PointerButton, pressed: bool) {
    h.event(Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    });
}

/// Two clicks at `at`, a frame apart, quicker than the double-click time.
/// Half a second passes first, so an earlier click does not make it a triple-click.
fn double_click(h: &mut Harness<'static, App>, at: Pos2) {
    h.hover_at(at);
    for _ in 0..30 {
        h.step();
    }
    for pressed in [true, false, true] {
        press(h, at, PointerButton::Primary, pressed);
        h.step();
    }
    press(h, at, PointerButton::Primary, false);
}

/// A point on the bar's empty space: in the drag gap left of the window controls.
fn empty_bar(h: &Harness<'static, App>) -> Pos2 {
    let minimise = h.get_by_role_and_label(Role::Button, "Minimise").rect();
    pos2(minimise.left() - 20.0, minimise.center().y)
}

fn rects(h: &Harness<'static, App>) -> Vec<RectShape> {
    fn walk(shape: &Shape, out: &mut Vec<RectShape>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
            Shape::Rect(r) => out.push(r.clone()),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for ClippedShape { shape, .. } in &h.output().shapes {
        walk(shape, &mut out);
    }
    out
}

#[test]
fn the_window_controls_sit_at_the_right_and_act_on_the_window() {
    let home = TempDir::new("wayland-controls");
    let mut h = window(&home, CUSTOM, false);
    let button =
        |h: &Harness<'static, App>, label| h.get_by_role_and_label(Role::Button, label).rect();
    let (min, max, close) = (
        button(&h, "Minimise"),
        button(&h, "Maximise"),
        button(&h, "Close"),
    );
    assert!(min.right() <= max.left() && max.right() <= close.left());
    assert_eq!(
        close.right(),
        WIDTH - MARGIN,
        "close is at the window's right"
    );
    assert_eq!(close.width(), 46.0);
    let bar = h.ctx.tokens().title_bar_height();
    assert_eq!(close.height(), bar, "full height of the bar");
    assert!(h.query_by_label("Restore").is_none());

    for (label, sent) in [
        ("Minimise", ViewportCommand::Minimized(true)),
        ("Maximise", ViewportCommand::Maximized(true)),
        ("Close", ViewportCommand::Close),
    ] {
        h.get_by_role_and_label(Role::Button, label).click();
        assert!(commands(&mut h).contains(&sent), "{label} sends {sent:?}");
    }
}

#[test]
fn dragging_the_empty_bar_moves_the_window_and_a_double_click_maximises() {
    let home = TempDir::new("wayland-drag");
    let mut h = window(&home, CUSTOM, false);
    let at = empty_bar(&h);
    h.hover_at(at);
    press(&h, at, PointerButton::Primary, true);
    h.step();
    h.hover_at(at + egui::vec2(30.0, 10.0));
    let sent = commands(&mut h);
    assert!(sent.contains(&ViewportCommand::StartDrag), "{sent:?}");
    press(
        &h,
        at + egui::vec2(30.0, 10.0),
        PointerButton::Primary,
        false,
    );
    commands(&mut h);

    double_click(&mut h, at);
    let sent = commands(&mut h);
    assert!(sent.contains(&ViewportCommand::Maximized(true)), "{sent:?}");
    assert!(!sent.contains(&ViewportCommand::StartDrag));
}

#[test]
fn a_double_click_on_a_maximised_window_restores_it() {
    let home = TempDir::new("wayland-restore");
    let mut h = window(&home, CUSTOM, true);
    // The middle control is Restore.
    assert!(h.query_by_label("Maximise").is_none());
    h.get_by_role_and_label(Role::Button, "Restore").click();
    assert!(commands(&mut h).contains(&ViewportCommand::Maximized(false)));
    let at = empty_bar(&h);
    h.hover_at(at);
    double_click(&mut h, at);
    assert!(commands(&mut h).contains(&ViewportCommand::Maximized(false)));
}

#[test]
fn pressing_the_edge_strip_starts_a_resize_from_that_edge_or_corner() {
    let home = TempDir::new("wayland-resize");
    let mut h = window(&home, CUSTOM, false);
    for (at, direction, cursor) in [
        (
            pos2(2.0, 300.0),
            ResizeDirection::West,
            CursorIcon::ResizeWest,
        ),
        (
            pos2(WIDTH - 3.0, 300.0),
            ResizeDirection::East,
            CursorIcon::ResizeEast,
        ),
        (
            pos2(600.0, 1.0),
            ResizeDirection::North,
            CursorIcon::ResizeNorth,
        ),
        (
            pos2(600.0, HEIGHT - 2.0),
            ResizeDirection::South,
            CursorIcon::ResizeSouth,
        ),
        (
            pos2(10.0, 2.0),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNorthWest,
        ),
        // The top right corner is over the close button: the strip wins.
        (
            pos2(WIDTH - 2.0, 10.0),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNorthEast,
        ),
        (
            pos2(3.0, HEIGHT - 11.0),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeSouthWest,
        ),
        (
            pos2(WIDTH - 11.0, HEIGHT - 1.0),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeSouthEast,
        ),
    ] {
        h.hover_at(at);
        h.step();
        h.step();
        assert_eq!(h.output().platform_output.cursor_icon, cursor, "at {at:?}");
        press(&h, at, PointerButton::Primary, true);
        let sent = commands(&mut h);
        assert!(
            sent.contains(&ViewportCommand::BeginResize(direction)),
            "at {at:?}: {sent:?}"
        );
        assert!(
            !sent.contains(&ViewportCommand::Close),
            "the press does not reach the button"
        );
        press(&h, at, PointerButton::Primary, false);
        commands(&mut h);
    }
    // Seven px in from the edge is the window, not the strip.
    let inside = pos2(7.0, 300.0);
    h.hover_at(inside);
    press(&h, inside, PointerButton::Primary, true);
    let sent = commands(&mut h);
    assert!(
        !sent
            .iter()
            .any(|c| matches!(c, ViewportCommand::BeginResize(_)))
    );
}

#[test]
fn a_maximised_window_has_no_resize_strip_and_square_corners() {
    let home = TempDir::new("wayland-maximised");
    let mut h = window(&home, CUSTOM, true);
    let at = pos2(2.0, 300.0);
    h.hover_at(at);
    press(&h, at, PointerButton::Primary, true);
    let sent = commands(&mut h);
    assert!(
        !sent
            .iter()
            .any(|c| matches!(c, ViewportCommand::BeginResize(_))),
        "{sent:?}"
    );
    let radius = h.ctx.tokens().radius.window as u8;
    assert!(
        !rects(&h)
            .iter()
            .any(|r| r.corner_radius.nw == radius || r.corner_radius.sw == radius),
        "nothing is rounded to the window radius"
    );
}

#[test]
fn a_restored_window_takes_the_window_radius_with_a_rim() {
    let home = TempDir::new("wayland-corners");
    let h = window(&home, CUSTOM, false);
    let t = h.ctx.tokens();
    let radius = t.radius.window as u8;
    let rects = rects(&h);
    let base = t.color.surface.base;
    // The Title bar rounds the top corners and the status bar the bottom ones, on the base.
    assert!(rects.iter().any(|r| {
        r.fill == base && r.corner_radius.nw == radius && r.corner_radius.ne == radius
    }));
    assert!(rects.iter().any(|r| {
        r.fill == base && r.corner_radius.sw == radius && r.corner_radius.se == radius
    }));
    // A hairline rim along the whole window, at the window radius.
    let rim = rects.iter().find(|r| {
        r.stroke.color == t.color.stroke.hairline
            && r.corner_radius.nw == radius
            && r.rect.width() == WIDTH
            && r.rect.height() == HEIGHT
    });
    let rim = rim.expect("the window has a rim");
    assert_eq!(rim.stroke.width, t.stroke.hairline);
}

#[test]
fn the_system_title_bar_preference_draws_no_window_chrome() {
    let home = TempDir::new("wayland-system");
    let mut h = window(
        &home,
        Startup {
            preference: true,
            forced: false,
        },
        false,
    );
    assert!(h.query_by_label("Minimise").is_none());
    assert!(h.query_by_label("Close").is_none());
    let at = pos2(2.0, 300.0);
    h.hover_at(at);
    press(&h, at, PointerButton::Primary, true);
    let sent = commands(&mut h);
    assert!(
        !sent
            .iter()
            .any(|c| matches!(c, ViewportCommand::BeginResize(_)))
    );
    let radius = h.ctx.tokens().radius.window as u8;
    assert!(!rects(&h).iter().any(|r| r.corner_radius.nw == radius));
}

#[test]
fn a_forced_system_title_bar_draws_no_window_chrome_and_the_page_says_why() {
    let home = TempDir::new("wayland-forced");
    let mut h = window(
        &home,
        Startup {
            preference: false,
            forced: true,
        },
        false,
    );
    assert!(h.query_by_label("Minimise").is_none());
    h.state_mut().open_preferences(Some(PageId::APPEARANCE));
    h.run();
    let label = "Use the system title bar";
    h.get_by_role_and_label(Role::CheckBox, label)
        .scroll_to_me();
    h.run();
    let node = h.get_by_role_and_label(Role::CheckBox, label);
    assert_eq!(
        node.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    assert!(node.accesskit_node().is_disabled());
    h.get_by_label("Your desktop draws the title bar for every window.");
    assert!(!h.state().shell().preferences().general.system_title_bar);
}

#[test]
fn a_window_made_without_its_start_draws_the_bar_as_under_the_system_title_bar() {
    let home = TempDir::new("wayland-harness");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let mut h = Harness::builder()
        .with_size([WIDTH, HEIGHT])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(Shell::new(engine)));
    h.run();
    assert!(h.query_by_label("Minimise").is_none());
}

/// What a floating window, shown as an OS window of its own as eframe shows it, was built
/// with and drew.
#[derive(Default)]
struct Floating {
    decorations: Option<bool>,
    transparent: Option<bool>,
    /// Every AccessKit node it showed: role and label.
    nodes: Vec<(Role, String)>,
    /// Every command it sent its own window (collected by [`click`]).
    commands: Vec<ViewportCommand>,
    /// Events for its next pass.
    events: Vec<Event>,
}

const FLOATING: egui::Vec2 = egui::vec2(480.0, 320.0);

/// Shows immediate viewports as OS windows of their own, as eframe does, each with its own
/// pass, recording into `floating`.
fn native_viewports(ctx: &egui::Context, floating: std::rc::Rc<std::cell::RefCell<Floating>>) {
    ctx.set_embed_viewports(false);
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        let id = viewport.ids.this;
        let mut f = floating.borrow_mut();
        f.decorations = viewport.builder.decorations;
        f.transparent = viewport.builder.transparent;
        let input = egui::RawInput {
            viewport_id: id,
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, FLOATING)),
            events: f.events.drain(..).collect(),
            viewports: [(
                id,
                egui::ViewportInfo {
                    focused: Some(true),
                    ..Default::default()
                },
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        drop(f);
        let mut output = ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui));
        // Nothing renders this window, so its texture changes go nowhere.
        output.textures_delta.clear();
        let mut f = floating.borrow_mut();
        if let Some(update) = output.platform_output.accesskit_update {
            for (_, node) in update.nodes {
                if let Some(label) = node.label() {
                    f.nodes.push((node.role(), label.to_owned()));
                }
            }
        }
    });
}

fn floating_window(
    home: &TempDir,
    startup: Startup,
) -> (
    Harness<'static, App>,
    std::rc::Rc<std::cell::RefCell<Floating>>,
) {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "layout.tab.move_to_new_window",
            serde_json::json!({"tab": "start"}),
        )
        .unwrap();
    let app = App::new(shell).with_title_bar(startup);
    let mut h = Harness::builder()
        .with_size([WIDTH, HEIGHT])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    let floating = std::rc::Rc::default();
    native_viewports(&h.ctx, std::rc::Rc::clone(&floating));
    h.run();
    (h, floating)
}

/// A click at `at` in the floating window: the press in one pass, the release in the next.
fn click(h: &mut Harness<'static, App>, floating: &std::cell::RefCell<Floating>, at: Pos2) {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    // What a floating window asks of its OS window comes out of the main pass, under
    // the floating window's viewport.
    let step = |h: &mut Harness<'static, App>, event: Option<Event>| {
        floating.borrow_mut().events.extend(event);
        h.step();
        let out = &h.output().viewport_output;
        let sent = out.iter().filter(|(id, _)| **id != egui::ViewportId::ROOT);
        let sent: Vec<_> = sent.flat_map(|(_, o)| o.commands.iter().cloned()).collect();
        floating.borrow_mut().commands.extend(sent);
    };
    step(h, Some(Event::PointerMoved(at)));
    step(h, Some(button(true)));
    step(h, Some(button(false)));
    step(h, None);
    step(h, None);
}

#[test]
fn a_floating_window_is_undecorated_with_the_slim_bar_and_its_controls() {
    let home = TempDir::new("wayland-floating");
    let (mut h, floating) = floating_window(&home, CUSTOM);
    {
        let f = floating.borrow();
        assert_eq!(f.decorations, Some(false));
        assert_eq!(f.transparent, Some(true));
        assert!(
            f.nodes
                .iter()
                .any(|(role, label)| *role == Role::TitleBar && label.contains("Start")),
            "the slim bar is named by the window's title: {:?}",
            f.nodes
        );
        for control in ["Minimise", "Maximise", "Close"] {
            assert!(
                f.nodes.contains(&(Role::Button, control.to_owned())),
                "{control}"
            );
        }
    }
    // The close control is the last 46 px of the 32 px bar; it closes this window.
    let close = pos2(FLOATING.x - 23.0, 16.0);
    click(&mut h, &floating, close);
    assert!(floating.borrow().commands.contains(&ViewportCommand::Close));
    // The edge strip resizes this window too.
    let edge = pos2(FLOATING.x - 2.0, 200.0);
    click(&mut h, &floating, edge);
    assert!(
        floating
            .borrow()
            .commands
            .contains(&ViewportCommand::BeginResize(ResizeDirection::East))
    );
}

#[test]
fn under_the_system_title_bar_a_floating_window_has_no_reda_bar() {
    let home = TempDir::new("wayland-floating-system");
    let (_h, floating) = floating_window(
        &home,
        Startup {
            preference: true,
            forced: false,
        },
    );
    let f = floating.borrow();
    assert_eq!(f.decorations, None);
    assert_eq!(f.transparent, None);
    assert!(
        !f.nodes.iter().any(|(role, _)| *role == Role::TitleBar),
        "{:?}",
        f.nodes
    );
    assert!(!f.nodes.contains(&(Role::Button, "Minimise".to_owned())));
}
