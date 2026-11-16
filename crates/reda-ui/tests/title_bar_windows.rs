//! The Title bar as a Windows window gives it (docs/spec/v0.1/app.md#title-bar,
//! design-language.md#title-bar, ADR-0014): REDA's window controls at the right and the
//! commands they send, dragging and double-clicking the bar's empty space, the edge-resize
//! strip and its corners, square corners REDA leaves to Windows, the maximised window, a
//! floating window's slim bar, and the system title bar.
//!
//! The chrome is `title_bar::windows::PLATFORM`, pure functions of the start and the
//! window's state, so these run on every platform. On Windows the same checks also run
//! through the native dispatch (`title_bar::Platform::NATIVE`), the code the real window
//! runs, in the CI Windows job. The harness has no OS window: what is checked is what the
//! window asks of Windows, the `ViewportCommand`s it sends, the cursor and what it paints.

use egui::accesskit::Role;
use egui::epaint::{ClippedShape, RectShape};
use egui::viewport::ResizeDirection;
use egui::{
    CursorIcon, Event, Modifiers, PointerButton, Pos2, Rect, Shape, ViewportCommand, ViewportId,
    pos2, vec2,
};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::shell::Shell;
use reda_ui::theme::TokensExt as _;
use reda_ui::title_bar::{self, Chrome, Hit, Platform, Startup, windows};
use reda_ui::widgets::{WindowAction, window_control_rects};

const WIDTH: f32 = 1200.0;
const HEIGHT: f32 = 720.0;
/// The harness keeps an 8 px margin around the window's `ui`; the window's own rect,
/// which the resize strip follows, is the whole screen.
const MARGIN: f32 = 8.0;

const CUSTOM: Startup = Startup {
    preference: false,
    forced: false,
};
const SYSTEM: Startup = Startup {
    preference: true,
    forced: false,
};

fn shell(home: &TempDir) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    Shell::new(engine)
}

/// The main window built as `startup` asks with `platform`'s chrome.
fn window_on(
    home: &TempDir,
    platform: Platform,
    startup: Startup,
    maximised: bool,
) -> Harness<'static, App> {
    let app = App::new(shell(home))
        .with_platform(platform)
        .with_title_bar(startup);
    // Frames a sixtieth of a second apart, so two clicks a frame apart are a double-click.
    let mut h = Harness::builder()
        .with_size([WIDTH, HEIGHT])
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(120)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    if maximised {
        let root = h.input_mut().viewports.entry(ViewportId::ROOT);
        root.or_default().maximized = Some(true);
    }
    h.run();
    h
}

/// The main window with Windows' chrome.
fn window(home: &TempDir, startup: Startup, maximised: bool) -> Harness<'static, App> {
    window_on(home, windows::PLATFORM, startup, maximised)
}

/// Runs a few frames after the queued events; returns every command the window sent.
fn commands(h: &mut Harness<'static, App>) -> Vec<ViewportCommand> {
    let mut out = Vec::new();
    for _ in 0..4 {
        h.step();
        let root = &h.output().viewport_output[&ViewportId::ROOT];
        out.extend(root.commands.iter().cloned());
    }
    out
}

fn press(h: &Harness<'static, App>, at: Pos2, pressed: bool) {
    h.event(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
}

/// A press at `at`, a move 40 px along, a release: what the window sent meanwhile.
fn drag(h: &mut Harness<'static, App>, at: Pos2) -> Vec<ViewportCommand> {
    let to = at + vec2(-40.0, 10.0);
    let mut out = Vec::new();
    let mut step = |h: &mut Harness<'static, App>| {
        h.step();
        let root = &h.output().viewport_output[&ViewportId::ROOT];
        out.extend(root.commands.iter().cloned());
    };
    h.hover_at(at);
    step(h);
    press(h, at, true);
    step(h);
    h.hover_at(to);
    step(h);
    step(h);
    press(h, to, false);
    step(h);
    step(h);
    out
}

/// Two clicks at `at`, a frame apart, after half a second so no earlier click counts.
fn double_click(h: &mut Harness<'static, App>, at: Pos2) -> Vec<ViewportCommand> {
    h.hover_at(at);
    for _ in 0..30 {
        h.step();
    }
    for pressed in [true, false, true] {
        press(h, at, pressed);
        h.step();
    }
    press(h, at, false);
    commands(h)
}

fn button(h: &Harness<'static, App>, label: &str) -> Rect {
    h.get_by_role_and_label(Role::Button, label).rect()
}

/// The Title bar's rect: the harness's margin in from the sides, the controls' height.
fn bar(h: &Harness<'static, App>) -> Rect {
    let close = button(h, "Close");
    Rect::from_min_max(
        pos2(MARGIN, close.top()),
        pos2(WIDTH - MARGIN, close.bottom()),
    )
}

/// A point on the bar's empty space: in the drag gap left of the window controls.
fn empty_bar(h: &Harness<'static, App>) -> Pos2 {
    let minimise = button(h, "Minimise");
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

/// The controls are where `window_control_rects` puts them in the bar, minimise,
/// maximise or restore, close, at the window's right and the bar's full height, and each
/// sends its command; the close button hovers in `close_hover`.
fn check_controls(h: &mut Harness<'static, App>, maximised: bool) {
    let bar = bar(h);
    assert_eq!(bar.height(), h.ctx.tokens().title_bar_height());
    for (action, rect) in window_control_rects(bar, maximised) {
        let drawn = button(h, &action.name());
        assert_eq!(drawn, rect, "{action:?}");
    }
    assert_eq!(button(h, "Close").right(), WIDTH - MARGIN);
    assert_eq!(button(h, "Close").width(), 46.0);
    let hidden = if maximised { "Maximise" } else { "Restore" };
    assert!(h.query_by_label(hidden).is_none());

    // Close hovers red, square at the corner: Windows rounds the window itself.
    let close = button(h, "Close");
    h.hover_at(close.center());
    h.run();
    let close_hover = h.ctx.tokens().color.close_hover;
    let hover = rects(h).into_iter().find(|r| r.fill == close_hover);
    let hover = hover.expect("the close button hovers in close_hover");
    assert_eq!(hover.rect, close);
    assert_eq!(hover.corner_radius, egui::CornerRadius::ZERO);

    let middle = if maximised {
        ("Restore", ViewportCommand::Maximized(false))
    } else {
        ("Maximise", ViewportCommand::Maximized(true))
    };
    for (label, sent) in [
        ("Minimise", ViewportCommand::Minimized(true)),
        middle,
        ("Close", ViewportCommand::Close),
    ] {
        h.get_by_role_and_label(Role::Button, label).click();
        assert!(commands(h).contains(&sent), "{label} sends {sent:?}");
    }
}

#[test]
fn the_window_controls_sit_at_the_right_and_act_on_the_window() {
    let home = TempDir::new("windows-controls");
    let mut h = window(&home, CUSTOM, false);
    check_controls(&mut h, false);
}

#[test]
fn a_maximised_window_shows_restore() {
    let home = TempDir::new("windows-controls-maximised");
    let mut h = window(&home, CUSTOM, true);
    check_controls(&mut h, true);
}

/// Presses and drags at points across the bar and its edge, and checks that what the
/// window does there is what `title_bar::hit` says the point is.
fn check_hits(h: &mut Harness<'static, App>, chrome: Chrome) {
    let window = Rect::from_min_size(Pos2::ZERO, vec2(WIDTH, HEIGHT));
    let bar = bar(h);
    let y = bar.center().y;
    let field = button(h, "Search Documents, Components, Nets");
    let gear = button(h, "Preferences…");
    let file = button(h, "File");
    let items = [field, gear, file];
    let minimise = button(h, "Minimise");
    let points = [
        // The bar's empty space: between the field and the gear, and the drag gap.
        pos2((field.right() + gear.left()) / 2.0, y),
        pos2(minimise.left() - 10.0, y),
        // Items on the bar.
        file.center(),
        gear.center(),
        field.center(),
        // The resize strip: the left and right edges beside the bar, the top edge above
        // the field, and the top right corner.
        pos2(2.0, y),
        pos2(WIDTH - 2.0, y),
        pos2(field.center().x, 2.0),
        pos2(WIDTH - 2.0, 2.0),
        // The content.
        pos2(600.0, 400.0),
    ];
    for at in points {
        let expected = title_bar::hit(window, bar, chrome, &items, at);
        let sent = drag(h, at);
        let moved = sent.contains(&ViewportCommand::StartDrag);
        let resized = sent.iter().find_map(|c| match c {
            ViewportCommand::BeginResize(d) => Some(*d),
            _ => None,
        });
        match expected {
            Hit::Drag => assert!(moved && resized.is_none(), "{at:?} drags: {sent:?}"),
            Hit::Resize(direction) => {
                assert_eq!(resized, Some(direction), "{at:?}: {sent:?}");
                assert!(!moved, "{at:?}: {sent:?}");
            }
            Hit::Item | Hit::Client | Hit::Control(_) => {
                assert!(
                    !moved && resized.is_none(),
                    "{at:?} is {expected:?}: {sent:?}"
                );
            }
        }
        // Close whatever the press opened.
        h.key_press(egui::Key::Escape);
        h.run();
    }
}

#[test]
fn the_window_moves_and_resizes_where_the_hit_test_says() {
    let home = TempDir::new("windows-hits");
    let mut h = window(&home, CUSTOM, false);
    let chrome = windows::chrome(CUSTOM, &egui::ViewportInfo::default());
    check_hits(&mut h, chrome);
}

#[test]
fn a_maximised_window_moves_where_the_hit_test_says_and_does_not_resize() {
    let home = TempDir::new("windows-hits-maximised");
    let mut h = window(&home, CUSTOM, true);
    let info = egui::ViewportInfo {
        maximized: Some(true),
        ..Default::default()
    };
    let chrome = windows::chrome(CUSTOM, &info);
    assert!(!chrome.resizes);
    check_hits(&mut h, chrome);
}

#[test]
fn dragging_the_empty_bar_moves_the_window_and_a_double_click_maximises() {
    let home = TempDir::new("windows-drag");
    let mut h = window(&home, CUSTOM, false);
    let at = empty_bar(&h);
    let sent = drag(&mut h, at);
    assert!(sent.contains(&ViewportCommand::StartDrag), "{sent:?}");
    let sent = double_click(&mut h, at);
    assert!(sent.contains(&ViewportCommand::Maximized(true)), "{sent:?}");
    assert!(!sent.contains(&ViewportCommand::StartDrag));
}

#[test]
fn a_double_click_on_a_maximised_window_restores_it() {
    let home = TempDir::new("windows-restore");
    let mut h = window(&home, CUSTOM, true);
    let at = empty_bar(&h);
    let sent = double_click(&mut h, at);
    assert!(
        sent.contains(&ViewportCommand::Maximized(false)),
        "{sent:?}"
    );
}

#[test]
fn pressing_the_edge_strip_starts_a_resize_from_that_edge_or_corner() {
    let home = TempDir::new("windows-resize");
    let mut h = window(&home, CUSTOM, false);
    use ResizeDirection::*;
    for (at, direction, cursor) in [
        (pos2(2.0, 300.0), West, CursorIcon::ResizeWest),
        (pos2(WIDTH - 3.0, 300.0), East, CursorIcon::ResizeEast),
        (pos2(600.0, 1.0), North, CursorIcon::ResizeNorth),
        (pos2(600.0, HEIGHT - 2.0), South, CursorIcon::ResizeSouth),
        (pos2(10.0, 2.0), NorthWest, CursorIcon::ResizeNorthWest),
        (
            pos2(WIDTH - 2.0, 10.0),
            NorthEast,
            CursorIcon::ResizeNorthEast,
        ),
        (
            pos2(3.0, HEIGHT - 11.0),
            SouthWest,
            CursorIcon::ResizeSouthWest,
        ),
        (
            pos2(WIDTH - 11.0, HEIGHT - 1.0),
            SouthEast,
            CursorIcon::ResizeSouthEast,
        ),
    ] {
        h.hover_at(at);
        h.step();
        h.step();
        assert_eq!(h.output().platform_output.cursor_icon, cursor, "at {at:?}");
        press(&h, at, true);
        let sent = commands(&mut h);
        assert!(
            sent.contains(&ViewportCommand::BeginResize(direction)),
            "at {at:?}: {sent:?}"
        );
        press(&h, at, false);
        commands(&mut h);
    }
    // Seven px in from the edge is the window, not the strip.
    let inside = pos2(7.0, 300.0);
    h.hover_at(inside);
    press(&h, inside, true);
    let sent = commands(&mut h);
    assert!(
        !sent
            .iter()
            .any(|c| matches!(c, ViewportCommand::BeginResize(_)))
    );
}

#[test]
fn the_corners_are_left_to_windows_square_in_reda_and_without_a_rim() {
    let home = TempDir::new("windows-corners");
    for maximised in [false, true] {
        let h = window(&home, CUSTOM, maximised);
        let t = h.ctx.tokens();
        let radius = t.radius.window as u8;
        let rects = rects(&h);
        assert!(
            !rects
                .iter()
                .any(|r| r.corner_radius.nw == radius || r.corner_radius.sw == radius),
            "REDA rounds nothing to the window radius"
        );
        assert!(
            !rects
                .iter()
                .any(|r| r.stroke.color == t.color.stroke.hairline
                    && r.rect.width() == WIDTH
                    && r.rect.height() == HEIGHT),
            "no rim: Windows draws the window's border and shadow"
        );
    }
}

#[test]
fn the_system_title_bar_draws_no_window_chrome() {
    let home = TempDir::new("windows-system");
    let mut h = window(&home, SYSTEM, false);
    for label in ["Minimise", "Maximise", "Close"] {
        assert!(h.query_by_label(label).is_none(), "{label}");
    }
    let field = button(&h, "Search Documents, Components, Nets");
    let gear = button(&h, "Preferences…");
    let at = pos2((field.right() + gear.left()) / 2.0, field.center().y);
    let sent = drag(&mut h, at);
    assert!(!sent.contains(&ViewportCommand::StartDrag), "{sent:?}");
    let edge = pos2(2.0, 300.0);
    let sent = drag(&mut h, edge);
    assert!(
        !sent
            .iter()
            .any(|c| matches!(c, ViewportCommand::BeginResize(_)))
    );
}

/// What a floating window, shown as an OS window of its own as eframe shows it, was built
/// with and drew.
#[derive(Default)]
struct Floating {
    decorations: Option<bool>,
    transparent: Option<bool>,
    /// Every AccessKit node it showed: role, label and rect.
    nodes: Vec<(Role, String, Option<egui::accesskit::Rect>)>,
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
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, FLOATING)),
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
                    f.nodes.push((node.role(), label.to_owned(), node.bounds()));
                }
            }
        }
    });
}

fn floating_window(
    home: &TempDir,
    platform: Platform,
    startup: Startup,
) -> (
    Harness<'static, App>,
    std::rc::Rc<std::cell::RefCell<Floating>>,
) {
    let mut shell = shell(home);
    shell
        .run(
            "layout.tab.move_to_new_window",
            serde_json::json!({"tab": "start"}),
        )
        .unwrap();
    let app = App::new(shell)
        .with_platform(platform)
        .with_title_bar(startup);
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
        let sent = out.iter().filter(|(id, _)| **id != ViewportId::ROOT);
        let sent: Vec<_> = sent.flat_map(|(_, o)| o.commands.iter().cloned()).collect();
        floating.borrow_mut().commands.extend(sent);
    };
    step(h, Some(Event::PointerMoved(at)));
    step(h, Some(button(true)));
    step(h, Some(button(false)));
    step(h, None);
    step(h, None);
}

fn check_floating_window(platform: Platform) {
    let home = TempDir::new("windows-floating");
    let (mut h, floating) = floating_window(&home, platform, CUSTOM);
    {
        let f = floating.borrow();
        assert_eq!(f.decorations, Some(false), "undecorated");
        assert_eq!(f.transparent, None, "opaque");
        let bar = f
            .nodes
            .iter()
            .find(|(role, label, _)| *role == Role::TitleBar && label.contains("Start"));
        let (_, _, bar) = bar.expect("the slim bar is named by the window's title");
        let bar = bar.expect("the bar has a rect");
        assert_eq!(bar.y1 - bar.y0, 32.0, "the 32 px slim bar");
        for control in ["Minimise", "Maximise", "Close"] {
            let found = f
                .nodes
                .iter()
                .find(|(role, label, _)| *role == Role::Button && label == control);
            let (_, _, rect) = found.unwrap_or_else(|| panic!("{control}"));
            let rect = rect.expect("a rect");
            assert_eq!(rect.y1 - rect.y0, 32.0, "{control} is the bar's height");
        }
    }
    // The close control is the last 46 px of the 32 px bar; it closes this window.
    click(&mut h, &floating, pos2(FLOATING.x - 23.0, 16.0));
    assert!(floating.borrow().commands.contains(&ViewportCommand::Close));
    click(&mut h, &floating, pos2(FLOATING.x - 23.0 - 46.0, 16.0));
    assert!(
        (floating.borrow().commands).contains(&WindowAction::Maximise.command()),
        "maximise acts on this window"
    );
    // The edge strip resizes this window too.
    click(&mut h, &floating, pos2(FLOATING.x - 2.0, 200.0));
    assert!(
        floating
            .borrow()
            .commands
            .contains(&ViewportCommand::BeginResize(ResizeDirection::East))
    );
}

#[test]
fn a_floating_window_gets_the_slim_bar_with_the_same_controls() {
    check_floating_window(windows::PLATFORM);
}

#[test]
fn under_the_system_title_bar_a_floating_window_has_no_reda_bar() {
    let home = TempDir::new("windows-floating-system");
    let (_h, floating) = floating_window(&home, windows::PLATFORM, SYSTEM);
    let f = floating.borrow();
    assert_eq!(f.decorations, None);
    assert!(
        !f.nodes.iter().any(|(role, _, _)| *role == Role::TitleBar),
        "{:?}",
        f.nodes
    );
}

/// The same checks through the native dispatch, which is Windows' only on Windows: the
/// code the real window runs, in CI's Windows job.
#[cfg(target_os = "windows")]
mod native {
    use super::*;

    #[test]
    fn the_system_title_bar_is_on_by_default_until_the_bar_is_tried_on_windows() {
        let prefs = reda_ui::engine::preferences::Preferences::default();
        assert!(prefs.general.system_title_bar);
        assert!(Startup::detect(&prefs).system_title_bar());
        assert!(!Startup::detect(&prefs).forced);
    }

    #[test]
    fn the_window_is_built_undecorated_and_opaque() {
        let builder = title_bar::viewport(CUSTOM, egui::ViewportBuilder::default());
        assert_eq!(builder.decorations, Some(false));
        assert_eq!(builder.transparent, None);
        let builder = title_bar::viewport(SYSTEM, egui::ViewportBuilder::default());
        assert_eq!(builder.decorations, None);
    }

    #[test]
    fn the_whole_window_draws_the_windows_bar_through_the_native_chrome() {
        let home = TempDir::new("windows-native");
        let mut h = window_on(&home, Platform::NATIVE, CUSTOM, false);
        check_controls(&mut h, false);
        let chrome = windows::chrome(CUSTOM, &egui::ViewportInfo::default());
        check_hits(&mut h, chrome);
        let mut h = window_on(&home, Platform::NATIVE, CUSTOM, true);
        check_controls(&mut h, true);
    }

    #[test]
    fn a_floating_window_gets_the_slim_bar_through_the_native_chrome() {
        check_floating_window(Platform::NATIVE);
    }
}
