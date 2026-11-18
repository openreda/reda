//! Each platform's Title bar drawn through the one seam, `title_bar::Platform`, on every
//! platform (docs/spec/v0.1/app.md#title-bar, design-language.md#title-bar, ADR-0014).
//!
//! A window built with `App::with_platform(p)` must be `p`'s whole chrome, whatever
//! platform the tests run on: the main bar's window controls or traffic-light inset, its
//! corners and what a double-click does, and a floating window's options, slim bar height
//! and controls. Nothing on the way may decide by the platform REDA was compiled for, so
//! the three platforms below run on all three CI runners and must agree everywhere.

use egui::accesskit::Role;
use egui::epaint::{ClippedShape, RectShape};
use egui::{
    Event, Modifiers, PointerButton, Pos2, Rect, Shape, ViewportBuilder, ViewportCommand,
    ViewportId, pos2,
};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::shell::Shell;
use reda_ui::theme::TokensExt as _;
use reda_ui::title_bar::{Platform, Startup, macos, wayland, windows};

const WIDTH: f32 = 1200.0;
const HEIGHT: f32 = 720.0;
/// The harness keeps an 8 px margin around the window's `ui`.
const MARGIN: f32 = 8.0;

const CUSTOM: Startup = Startup {
    preference: false,
    forced: false,
};

/// What each platform's chrome must look like, from the spec.
struct Expected {
    name: &'static str,
    platform: Platform,
    /// REDA draws minimise, maximise and close.
    controls: bool,
    /// Where the quick strip starts, from the bar's leading edge.
    strip_left: f32,
    /// REDA rounds the window to the window radius itself.
    rounds: bool,
    /// The slim bar of a floating window.
    floating_bar: f32,
    /// How a window is built: decorations, transparency, content under the title bar.
    built: (Option<bool>, Option<bool>, Option<bool>),
}

fn options(b: ViewportBuilder) -> (Option<bool>, Option<bool>, Option<bool>) {
    (b.decorations, b.transparent, b.fullsize_content_view)
}

fn platforms() -> [Expected; 3] {
    [
        Expected {
            name: "Windows",
            platform: windows::PLATFORM,
            controls: true,
            strip_left: reda_ui::widgets::CARD_GAP,
            rounds: false,
            floating_bar: 32.0,
            built: (Some(false), None, None),
        },
        Expected {
            name: "Wayland",
            platform: wayland::PLATFORM,
            controls: true,
            strip_left: reda_ui::widgets::CARD_GAP,
            rounds: true,
            floating_bar: 32.0,
            built: (Some(false), Some(true), None),
        },
        Expected {
            name: "macOS",
            platform: macos::PLATFORM,
            controls: false,
            strip_left: macos::TRAFFIC_LIGHTS_INSET,
            rounds: false,
            floating_bar: 28.0,
            built: (None, None, Some(true)),
        },
    ]
}

fn shell(home: &TempDir) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    Shell::new(engine)
}

fn window(home: &TempDir, platform: Platform) -> Harness<'static, App> {
    let app = App::new(shell(home))
        .with_platform(platform)
        .with_title_bar(CUSTOM);
    let mut h = Harness::builder()
        .with_size([WIDTH, HEIGHT])
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(120)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    h.run();
    h
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

fn press(h: &Harness<'static, App>, at: Pos2, pressed: bool) {
    h.event(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
}

/// Two clicks at `at`, a frame apart: every command the window sent.
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
    let mut out = Vec::new();
    for _ in 0..4 {
        h.step();
        out.extend(
            h.output().viewport_output[&ViewportId::ROOT]
                .commands
                .clone(),
        );
    }
    out
}

#[test]
fn each_platform_s_main_bar_is_drawn_through_the_seam() {
    for e in platforms() {
        let home = TempDir::new("platforms-main");
        let mut h = window(&home, e.platform);
        let name = e.name;
        for control in ["Minimise", "Maximise", "Close"] {
            assert_eq!(
                h.query_by_label(control).is_some(),
                e.controls,
                "{name}: {control}"
            );
        }
        let save = h.get_by_role_and_label(Role::Button, "Save").rect();
        assert_eq!(
            save.left(),
            MARGIN + e.strip_left,
            "{name}: the quick strip"
        );
        let t = h.ctx.tokens();
        let radius = t.radius.window as u8;
        let rounded = rects(&h)
            .iter()
            .any(|r| r.fill == t.color.surface.base && r.corner_radius.nw == radius);
        assert_eq!(rounded, e.rounds, "{name}: the window corners");

        // A double-click on the empty bar, between the field and the gear.
        let field = h
            .get_by_role_and_label(Role::Button, "Search Documents, Components, Nets")
            .rect();
        let gear = h.get_by_role_and_label(Role::Button, "Preferences…").rect();
        let at = pos2((field.right() + gear.left()) / 2.0, field.center().y);
        let sent = double_click(&mut h, at);
        let got: Vec<_> = (sent.into_iter())
            .filter(|c| {
                matches!(
                    c,
                    ViewportCommand::Maximized(_) | ViewportCommand::Minimized(_)
                )
            })
            .collect();
        let asked = if e.name == "macOS" {
            macos::double_click(macos::DoubleClick::system(), false)
        } else {
            Some(ViewportCommand::Maximized(true))
        };
        assert_eq!(got, asked.into_iter().collect::<Vec<_>>(), "{name}");
    }
}

/// What a floating window, shown as an OS window of its own, was built with and drew.
#[derive(Default)]
struct Floating {
    builder: Option<ViewportBuilder>,
    nodes: Vec<(Role, String, Option<egui::accesskit::Rect>)>,
}

const FLOATING: egui::Vec2 = egui::vec2(480.0, 320.0);

fn native_viewports(ctx: &egui::Context, floating: std::rc::Rc<std::cell::RefCell<Floating>>) {
    ctx.set_embed_viewports(false);
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        let id = viewport.ids.this;
        floating.borrow_mut().builder = Some(viewport.builder.clone());
        let input = egui::RawInput {
            viewport_id: id,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, FLOATING)),
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
        let mut output = ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui));
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

#[test]
fn each_platform_s_floating_window_is_built_and_drawn_through_the_seam() {
    for e in platforms() {
        let home = TempDir::new("platforms-floating");
        let mut shell = shell(&home);
        shell
            .run(
                "layout.tab.move_to_new_window",
                serde_json::json!({"tab": "start"}),
            )
            .unwrap();
        let app = App::new(shell)
            .with_platform(e.platform)
            .with_title_bar(CUSTOM);
        let mut h = Harness::builder()
            .with_size([WIDTH, HEIGHT])
            .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
        let floating = std::rc::Rc::<std::cell::RefCell<Floating>>::default();
        native_viewports(&h.ctx, std::rc::Rc::clone(&floating));
        h.run();
        let f = floating.borrow();
        let name = e.name;
        let built = options(f.builder.clone().expect("the floating window was shown"));
        assert_eq!(
            built, e.built,
            "{name}: decorations, transparency, full-size content"
        );
        let bar = f
            .nodes
            .iter()
            .find(|(role, label, _)| *role == Role::TitleBar && label.contains("Start"));
        let (_, _, bar) = bar.unwrap_or_else(|| panic!("{name}: the slim bar"));
        let bar = bar.expect("the bar has a rect");
        assert_eq!(bar.y1 - bar.y0, f64::from(e.floating_bar), "{name}: height");
        for control in ["Minimise", "Maximise", "Close"] {
            let found = f
                .nodes
                .iter()
                .find(|(role, label, _)| *role == Role::Button && label == control);
            assert_eq!(found.is_some(), e.controls, "{name}: {control}");
            if let Some((_, _, Some(rect))) = found {
                assert_eq!(rect.y1 - rect.y0, f64::from(e.floating_bar), "{name}");
            }
        }
    }
}
