//! The Title bar as a macOS window gives it (docs/spec/v0.1/app.md#title-bar,
//! design-language.md#title-bar, ADR-0014): the quick strip after the traffic lights'
//! 78 px inset, no REDA-drawn window controls, a bar whose empty space moves the window,
//! the inset gone in full screen, the plain row under the system title bar, and a
//! floating window's slim bar at the native title-bar height.
//!
//! The chrome comes from `title_bar::macos::chrome`, a pure function of the start and the
//! window's state, so this runs on any platform. What AppKit does with the commands (the
//! move, the zoom, the traffic lights' place) is for the desktop run on a Mac.

use std::collections::BTreeMap;

use egui::accesskit::Role;
use egui::{Event, Modifiers, PointerButton, Pos2, ViewportCommand, ViewportId, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::shell::Shell;
use reda_ui::theme::TokensExt as _;
use reda_ui::title_bar::{self, Startup, macos};
use reda_ui::widgets;

const WIDTH: f32 = 2000.0;
/// The harness keeps an 8 px margin around the window's `ui`.
const MARGIN: f32 = 8.0;

const CUSTOM: Startup = Startup {
    preference: false,
    forced: false,
};
const SYSTEM: Startup = Startup {
    preference: true,
    forced: false,
};

struct Window {
    shell: Shell,
    startup: Startup,
    themed: bool,
}

/// The main window's bar as macOS gives it, drawn and driven as the app does each frame.
fn window(home: &TempDir, startup: Startup, fullscreen: bool) -> Harness<'static, Window> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let state = Window {
        shell: Shell::new(engine),
        startup,
        themed: false,
    };
    // Frames a sixtieth of a second apart, so two clicks a frame apart are a double-click.
    let mut h = Harness::builder()
        .with_size([WIDTH, 640.0])
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(120)
        .build_ui_state(
            |ui, w: &mut Window| {
                if !w.themed {
                    reda_ui::theme::install(ui.ctx(), egui::Theme::Dark.into());
                    reda_ui::icons::install_loaders(ui.ctx());
                    w.themed = true;
                }
                let chrome = ui.input(|i| macos::chrome(w.startup, i.viewport()));
                let bars = bars::for_document(bars::active_type(&w.shell));
                let checked = BTreeMap::new();
                let cx = bars::draw::Cx {
                    shell: &w.shell,
                    checked: &checked,
                    document: None,
                };
                let outcome = title_bar::show(ui, &cx, bars, chrome);
                title_bar::drive(ui.ctx(), chrome, outcome.window, &outcome.background);
            },
            state,
        );
    if fullscreen {
        let root = h.input_mut().viewports.entry(ViewportId::ROOT);
        root.or_default().fullscreen = Some(true);
    }
    h.run();
    h
}

/// Runs a few frames after the queued events; returns every command the window sent.
fn commands(h: &mut Harness<'static, Window>) -> Vec<ViewportCommand> {
    let mut out = Vec::new();
    for _ in 0..4 {
        h.step();
        let root = &h.output().viewport_output[&ViewportId::ROOT];
        out.extend(root.commands.iter().cloned());
    }
    out
}

fn press(h: &Harness<'static, Window>, at: Pos2, pressed: bool) {
    h.event(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
}

fn drag(h: &mut Harness<'static, Window>, at: Pos2) -> Vec<ViewportCommand> {
    let to = at + egui::vec2(40.0, 10.0);
    let mut out = Vec::new();
    let mut step = |h: &mut Harness<'static, Window>| {
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
    out
}

/// Two clicks at `at`, a frame apart, after half a second so no earlier click counts.
fn double_click(h: &mut Harness<'static, Window>, at: Pos2) -> Vec<ViewportCommand> {
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

fn button(h: &Harness<'static, Window>, label: &str) -> egui::Rect {
    h.get_by_role_and_label(Role::Button, label).rect()
}

/// A point on the bar's empty space: between the command field and the gear.
fn empty_bar(h: &Harness<'static, Window>) -> Pos2 {
    let field = button(h, "Search Documents, Components, Nets");
    let gear = button(h, "Preferences…");
    pos2((field.right() + gear.left()) / 2.0, field.center().y)
}

#[test]
fn the_quick_strip_starts_after_the_traffic_lights_78_px_inset() {
    let home = TempDir::new("macos-inset");
    let h = window(&home, CUSTOM, false);
    let save = button(&h, "Save");
    assert_eq!(save.left(), MARGIN + 78.0);
    // The bar keeps its Density height, which the traffic lights centre in.
    let t = h.ctx.tokens();
    assert_eq!(save.center().y, MARGIN + t.title_bar_height() / 2.0);
}

#[test]
fn macos_has_no_window_controls_drawn_by_reda() {
    let home = TempDir::new("macos-controls");
    let h = window(&home, CUSTOM, false);
    for label in ["Minimise", "Maximise", "Restore", "Close"] {
        assert!(h.query_by_label(label).is_none(), "{label}");
    }
    // With no controls and dragging, the gear keeps the drag gap from the right edge.
    let gear = button(&h, "Preferences…");
    assert!(
        gear.right() <= WIDTH - MARGIN - title_bar::DRAG_GAP,
        "{gear:?}"
    );
}

#[test]
fn dragging_the_bar_s_empty_space_moves_the_window() {
    let home = TempDir::new("macos-drag");
    let mut h = window(&home, CUSTOM, false);
    let at = empty_bar(&h);
    let sent = drag(&mut h, at);
    assert!(sent.contains(&ViewportCommand::StartDrag), "{sent:?}");
}

#[test]
fn dragging_a_menu_title_or_the_gear_does_not_move_the_window() {
    let home = TempDir::new("macos-drag-button");
    let mut h = window(&home, CUSTOM, false);
    for label in ["File", "Preferences…"] {
        let at = button(&h, label).center();
        let sent = drag(&mut h, at);
        assert!(
            !sent.contains(&ViewportCommand::StartDrag),
            "{label}: {sent:?}"
        );
        // Close whatever the press opened.
        h.key_press(egui::Key::Escape);
        h.run();
    }
}

#[test]
fn a_double_click_on_the_bar_does_what_the_system_setting_asks() {
    let home = TempDir::new("macos-double-click");
    let mut h = window(&home, CUSTOM, false);
    let at = empty_bar(&h);
    let sent = double_click(&mut h, at);
    // Off macOS the system setting is not there to read, and the bar zooms, as macOS
    // does with nothing set.
    #[cfg(not(target_os = "macos"))]
    let asked = macos::double_click(macos::DoubleClick::Zoom, false);
    #[cfg(target_os = "macos")]
    let asked = macos::double_click(macos::DoubleClick::system(), false);
    let zoom_or_minimise = |c: &ViewportCommand| {
        matches!(
            c,
            ViewportCommand::Maximized(_) | ViewportCommand::Minimized(_)
        )
    };
    let got: Vec<_> = sent.into_iter().filter(zoom_or_minimise).collect();
    assert_eq!(got, asked.into_iter().collect::<Vec<_>>());
}

#[test]
fn full_screen_hides_the_inset_and_the_bar_stops_dragging() {
    let home = TempDir::new("macos-full-screen");
    let mut h = window(&home, CUSTOM, true);
    let save = button(&h, "Save");
    assert_eq!(save.left(), MARGIN + widgets::CARD_GAP);
    let at = empty_bar(&h);
    let sent = drag(&mut h, at);
    assert!(!sent.contains(&ViewportCommand::StartDrag), "{sent:?}");
}

#[test]
fn with_the_system_title_bar_the_row_loses_the_inset_and_the_dragging() {
    let home = TempDir::new("macos-system");
    let mut h = window(&home, SYSTEM, false);
    let save = button(&h, "Save");
    assert_eq!(save.left(), MARGIN + widgets::CARD_GAP);
    for label in ["Minimise", "Maximise", "Close"] {
        assert!(h.query_by_label(label).is_none(), "{label}");
    }
    let at = empty_bar(&h);
    let sent = drag(&mut h, at);
    assert!(!sent.contains(&ViewportCommand::StartDrag), "{sent:?}");
    let sent = double_click(&mut h, at);
    assert!(
        !sent
            .iter()
            .any(|c| matches!(c, ViewportCommand::Maximized(_))),
        "{sent:?}"
    );
    // And the window is built with the system's own title bar.
    let built = macos::viewport(SYSTEM, egui::ViewportBuilder::default());
    assert_eq!(built.fullsize_content_view, None);
}

#[test]
fn a_floating_window_s_slim_bar_takes_the_native_title_bar_height() {
    let mut h = Harness::builder()
        .with_size([600.0, 200.0])
        .with_step_dt(1.0 / 60.0)
        .build_ui(|ui| {
            let chrome = ui.input(|i| macos::chrome(CUSTOM, i.viewport()));
            let bar = widgets::floating_bar("REDA — Sheet 1")
                .height(macos::NATIVE_TITLE_BAR_HEIGHT)
                .leading_inset(chrome.leading_inset)
                .controls(chrome.controls)
                .drags(chrome.drags)
                .show(ui);
            title_bar::drive(ui.ctx(), chrome, bar.action, &bar.response);
        });
    h.run();
    let bar = h
        .get_by_role_and_label(Role::TitleBar, "REDA — Sheet 1")
        .rect();
    assert_eq!(bar.height(), 28.0);
    assert!(h.query_by_label("Close").is_none());
    let at = pos2(bar.right() - 40.0, bar.center().y);
    h.hover_at(at);
    h.step();
    h.event(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    h.hover_at(at + egui::vec2(30.0, 0.0));
    h.step();
    let sent = &h.output().viewport_output[&ViewportId::ROOT].commands;
    let mut all = sent.clone();
    h.step();
    all.extend(
        h.output().viewport_output[&ViewportId::ROOT]
            .commands
            .iter()
            .cloned(),
    );
    assert!(all.contains(&ViewportCommand::StartDrag), "{all:?}");
}

#[test]
fn a_floating_window_uses_the_native_height_only_on_macos() {
    let tokens = egui::Context::default().tokens();
    let height = title_bar::floating_bar_height(&tokens);
    if cfg!(target_os = "macos") {
        assert_eq!(height, 28.0);
    } else {
        assert_eq!(height, tokens.floating_bar_height());
    }
}
