//! The chrome's transitions, with the clock driven by hand: each frame is drawn at a time
//! the test sets, and what was painted is read back from the frame's shapes
//! (docs/spec/v0.1/design-language.md#motion-values).

use egui::accesskit::Role;
use egui::emath::TSTransform;
use egui::{Color32, Id, Key, Rect, Shape, Theme, Ui, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use reda_ui::modal;
use reda_ui::theme::{self, Settings, TokensExt as _, over};
use reda_ui::widgets::{self, Tooltip as _};

#[derive(Default)]
struct Form {
    wifi: bool,
    dialog: bool,
    /// Where the dialog was drawn.
    dialog_rect: Option<Rect>,
}

fn form(ui: &mut Ui, f: &mut Form) {
    ui.add(widgets::switch(&mut f.wifi, "Wi-Fi"));
    ui.add(widgets::button("Save"));
    widgets::menu_button("Edit").show(ui, |ui| {
        ui.add(widgets::menu_item("Undo"));
        ui.add(widgets::menu_item("Redo"));
    });
    ui.horizontal(|ui| {
        ui.add(widgets::button("Route")).tooltip("Route Track");
        ui.add(widgets::button("Place")).tooltip("Place Part");
    });
    if f.dialog {
        let shown = modal::Dialog::new(Id::NULL.with("motion-dialog"), "Confirm")
            .show(ui.ctx(), |ui| ui.label("Sure?"));
        f.dialog_rect = ui.ctx().memory(|m| m.area_rect(Id::NULL.with("motion-dialog")));
        let _ = shown;
    }
}

/// A form drawn with Animate as `animate` says. The clock stands still between the
/// frames a test asks for, so a click's frames all happen at the time it is given.
fn harness(animate: bool) -> Harness<'static, Form> {
    let mut installed = false;
    let mut h = Harness::builder()
        .with_size([800.0, 600.0])
        .with_step_dt(0.0)
        .build_ui_state(
            move |ui, f: &mut Form| {
                if !installed {
                    theme::install(ui.ctx(), Theme::Light.into());
                    let settings = Settings {
                        animate: Some(animate),
                        ..Settings::default()
                    };
                    theme::set(ui.ctx(), &settings);
                    installed = true;
                }
                form(ui, f);
            },
            Form::default(),
        );
    at(&mut h, 0.0);
    at(&mut h, 0.0);
    h
}

/// Draws the frames waiting (one per queued event) at `time` seconds.
fn at(h: &mut Harness<'_, Form>, time: f64) {
    h.input_mut().time = Some(time);
    h.step();
}

/// Each shape painted in the last frame, with the transform applied after it.
fn shapes(h: &Harness<'_, Form>) -> Vec<(Shape, TSTransform)> {
    fn flat(shape: &Shape, t: TSTransform, out: &mut Vec<(Shape, TSTransform)>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| flat(s, t, out)),
            other => out.push((other.clone(), t)),
        }
    }
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        flat(&clipped.shape, clipped.transform_after_tessellation, &mut out);
    }
    out
}

/// The x of the switch's white knob.
fn knob_x(h: &Harness<'_, Form>) -> f32 {
    shapes(h)
        .into_iter()
        .find_map(|(s, _)| match s {
            Shape::Circle(c) if c.fill == Color32::WHITE && c.radius == widgets::SWITCH_KNOB / 2.0 => {
                Some(c.center.x)
            }
            _ => None,
        })
        .expect("the knob is painted")
}

/// The fill of the rect painted exactly over `rect`.
fn fill_over(h: &Harness<'_, Form>, rect: Rect) -> Option<Color32> {
    shapes(h).into_iter().find_map(|(s, _)| match s {
        Shape::Rect(r) if r.rect == rect && r.fill != Color32::TRANSPARENT => Some(r.fill),
        _ => None,
    })
}

/// The `surface.floating` frame painted over `rect` (or its inside), with its
/// transform: a menu's or a dialog's.
fn floating_frames(h: &Harness<'_, Form>) -> Vec<(Rect, Color32, TSTransform)> {
    let tokens = h.ctx.tokens();
    let floating = tokens.color.surface.floating;
    // A frame has at least the control radius (a tooltip's); a row's is smaller.
    let radius = tokens.radius.control as u8;
    shapes(h)
        .into_iter()
        .filter_map(|(s, t)| match s {
            Shape::Rect(r) if r.corner_radius.nw >= radius && same_hue(r.fill, floating) => {
                Some((r.rect, r.fill, t))
            }
            _ => None,
        })
        .collect()
}

/// Whether `c` is `of` at some opacity.
fn same_hue(c: Color32, of: Color32) -> bool {
    let k = f32::from(c.a()) / f32::from(of.a());
    let near = |a: u8, b: u8| (f32::from(a) - f32::from(b) * k).abs() <= 2.0;
    near(c.r(), of.r()) && near(c.g(), of.g()) && near(c.b(), of.b())
}

fn near(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}

fn close(a: Color32, b: Color32) -> bool {
    a.to_array()
        .iter()
        .zip(b.to_array())
        .all(|(x, y)| x.abs_diff(y) <= 1)
}

#[test]
fn a_switch_slides_its_knob_in_140_ms_eased_out_and_back() {
    let mut h = harness(true);
    let row = h.get_by_label("Wi-Fi").rect();
    let (off, on) = (row.left() + 8.0, row.left() + widgets::SWITCH_TRACK.x - 8.0);
    assert_eq!(knob_x(&h), off);
    h.get_by_label("Wi-Fi").click();
    at(&mut h, 1.0);
    at(&mut h, 1.0);
    assert!(h.state().wifi);
    assert_eq!(knob_x(&h), off, "it starts where it was");
    at(&mut h, 1.07);
    let x = knob_x(&h);
    assert!(near(x, off + 0.875 * (on - off), 0.05), "half way in time, 87.5 % there: {x}");
    at(&mut h, 1.1401);
    assert_eq!(knob_x(&h), on);
    // Back, in the same time.
    h.get_by_label("Wi-Fi").click();
    at(&mut h, 2.0);
    at(&mut h, 2.0);
    at(&mut h, 2.07);
    assert!(near(knob_x(&h), on - 0.875 * (on - off), 0.05));
    at(&mut h, 2.1401);
    assert_eq!(knob_x(&h), off);
}

#[test]
fn a_hover_fill_comes_at_once_and_fades_out_linearly_in_90_ms() {
    let mut h = harness(true);
    let save = h.get_by_role_and_label(Role::Button, "Save").rect();
    let t = h.ctx.tokens();
    let rest = t.color.fill.control;
    assert_eq!(fill_over(&h, save), Some(rest));
    h.get_by_role_and_label(Role::Button, "Save").hover();
    at(&mut h, 1.0);
    assert_eq!(fill_over(&h, save), Some(over(t.color.fill.hover, rest)), "at once");
    h.hover_at(pos2(700.0, 500.0));
    at(&mut h, 2.0);
    at(&mut h, 2.045);
    let half = over(t.color.fill.hover.gamma_multiply(0.5), rest);
    let fill = fill_over(&h, save).unwrap();
    assert!(close(fill, half), "{fill:?} is not half the hover fill {half:?}");
    at(&mut h, 2.0901);
    assert_eq!(fill_over(&h, save), Some(rest));
}

#[test]
fn a_menu_arrives_4_px_down_from_its_button_in_90_ms_and_leaves_at_once() {
    let mut h = harness(true);
    h.get_by_role_and_label(Role::Button, "Edit").click();
    at(&mut h, 1.0);
    at(&mut h, 1.0);
    let menu = |h: &Harness<'_, Form>| {
        let undo = h.get_by_role_and_label(Role::Button, "Undo").rect();
        floating_frames(h)
            .into_iter()
            .find(|(r, _, _)| r.contains_rect(undo))
            .expect("the menu's frame")
    };
    let (_, _, t) = menu(&h);
    assert_eq!(t.translation.y, -4.0, "it starts 4 px nearer its button");
    at(&mut h, 1.045);
    let (_, _, t) = menu(&h);
    assert!(near(t.translation.y, -0.5, 0.01), "{t:?}");
    at(&mut h, 1.0901);
    assert_eq!(menu(&h).2, TSTransform::IDENTITY);
    // Escape closes it, and nothing of it is painted on the next frame.
    h.key_press(Key::Escape);
    at(&mut h, 2.0);
    at(&mut h, 2.0);
    assert!(h.query_by_label("Undo").is_none());
    let frames = floating_frames(&h);
    assert!(frames.is_empty(), "{frames:?}");
}

#[test]
fn a_tooltip_waits_500_ms_then_travels_in_and_the_next_one_comes_at_once() {
    let mut h = harness(true);
    h.get_by_role_and_label(Role::Button, "Route").hover();
    at(&mut h, 1.0);
    at(&mut h, 1.45);
    assert!(h.query_by_label("Route Track").is_none(), "still waiting");
    at(&mut h, 1.51);
    at(&mut h, 1.51);
    h.get_by_label("Route Track");
    let tip = |h: &Harness<'_, Form>, label: &str| {
        let text = h.get_by_label(label).rect();
        floating_frames(h)
            .into_iter()
            .find(|(r, _, _)| r.contains_rect(text))
            .map(|(_, fill, t)| (fill.a(), t))
    };
    // egui fades it in from nothing (by the `menu` time); REDA moves it 4 px.
    let (alpha, t) = tip(&h, "Route Track").unwrap();
    assert_eq!((alpha, t.translation.y.abs()), (0, 4.0));
    at(&mut h, 1.6);
    assert_eq!(tip(&h, "Route Track"), Some((255, TSTransform::IDENTITY)));
    // Straight on to the next button: its tooltip shows with no wait, no fade and no
    // travel.
    h.get_by_role_and_label(Role::Button, "Place").hover();
    at(&mut h, 1.7);
    at(&mut h, 1.7);
    // egui hands the tooltip over between two widgets in a frame: no time passes.
    at(&mut h, 1.7);
    h.get_by_label("Place Part");
    assert_eq!(tip(&h, "Place Part").map(|(_, t)| t), Some(TSTransform::IDENTITY));
    // egui draws any area at age 0 transparent; a millisecond on it is whole, where the
    // first tooltip's fade had barely begun.
    at(&mut h, 1.701);
    assert_eq!(tip(&h, "Place Part"), Some((255, TSTransform::IDENTITY)));
}

#[test]
fn a_dialog_grows_from_97_percent_in_140_ms_and_fades_out_in_90_after_it_closes() {
    let mut h = harness(true);
    h.state_mut().dialog = true;
    at(&mut h, 1.0);
    at(&mut h, 1.0);
    // Its frame and the parts of it filled like it (the close ×'s ground): what holds
    // the text.
    let text = h.get_by_label("Sure?").rect();
    let dialog = |h: &Harness<'_, Form>| {
        floating_frames(h)
            .into_iter()
            .filter(|(r, _, _)| r.contains_rect(text))
            .collect::<Vec<_>>()
    };
    let opacity = |h: &Harness<'_, Form>| dialog(h).iter().map(|(_, f, _)| f.a()).max();
    let first = dialog(&h);
    assert!(!first.is_empty());
    assert!(first.iter().all(|(_, _, t)| near(t.scaling, 0.97, 1e-4)), "{first:?}");
    assert_eq!(opacity(&h), Some(0), "and transparent");
    at(&mut h, 1.07);
    let (_, _, t) = dialog(&h)[0];
    assert!(near(t.scaling, 0.97 + 0.03 * 0.875, 1e-4), "{t:?}");
    at(&mut h, 1.1401);
    assert!(dialog(&h).iter().all(|(_, _, t)| *t == TSTransform::IDENTITY));
    assert_eq!(opacity(&h), Some(255));
    // Closed: no longer drawn by its owner, it fades out where it was.
    h.state_mut().dialog = false;
    at(&mut h, 2.0);
    assert!(h.query_by_label("Sure?").is_none(), "it takes no input while it leaves");
    assert_eq!(opacity(&h), Some(255));
    at(&mut h, 2.045);
    let alpha = opacity(&h).unwrap();
    assert!(alpha.abs_diff(32) <= 1, "{alpha}");
    at(&mut h, 2.0901);
    assert_eq!(opacity(&h), None);
}

#[test]
fn with_animate_off_nothing_moves() {
    let mut h = harness(false);
    // The switch is there in the frame after the click.
    let row = h.get_by_label("Wi-Fi").rect();
    h.get_by_label("Wi-Fi").click();
    at(&mut h, 1.0);
    at(&mut h, 1.0);
    assert_eq!(knob_x(&h), row.left() + widgets::SWITCH_TRACK.x - 8.0);
    // A hover fill goes at once.
    let save = h.get_by_role_and_label(Role::Button, "Save").rect();
    h.get_by_role_and_label(Role::Button, "Save").hover();
    at(&mut h, 2.0);
    h.hover_at(pos2(700.0, 500.0));
    at(&mut h, 3.0);
    assert_eq!(fill_over(&h, save), Some(h.ctx.tokens().color.fill.control));
    // A menu opens where it stays.
    h.get_by_role_and_label(Role::Button, "Edit").click();
    at(&mut h, 4.0);
    at(&mut h, 4.0);
    assert!(floating_frames(&h).iter().all(|(_, _, t)| *t == TSTransform::IDENTITY));
    h.key_press(Key::Escape);
    at(&mut h, 4.5);
    // A dialog is there at once, whole, and gone at once.
    h.state_mut().dialog = true;
    at(&mut h, 5.0);
    at(&mut h, 5.0);
    let frames = floating_frames(&h);
    assert!(frames.iter().all(|(_, _, t)| *t == TSTransform::IDENTITY));
    let filled: Vec<_> = frames.iter().filter(|(_, fill, _)| fill.a() > 0).collect();
    assert!(!filled.is_empty());
    assert!(filled.iter().all(|(_, fill, _)| fill.a() == 255));
    h.state_mut().dialog = false;
    at(&mut h, 6.0);
    assert!(floating_frames(&h).is_empty());
}

#[test]
fn the_focus_ring_is_drawn_whole_on_the_frame_focus_arrives() {
    let mut h = harness(true);
    h.key_press(Key::Tab);
    at(&mut h, 1.0);
    let ring = h.ctx.tokens().color.stroke.focus_ring;
    let drawn = shapes(&h).into_iter().any(|(s, t)| match s {
        Shape::Rect(r) => r.stroke.color == ring && t == TSTransform::IDENTITY,
        _ => false,
    });
    assert!(drawn, "the focus ring is drawn at once, at full strength");
}

/// The modules whose drawing never animates: the focus ring, a tab's contents, a
/// dragged tab, and the canvas (whose zoom animation is its own Preference). None of
/// them calls the motion engine.
#[test]
fn the_focus_ring_tab_contents_drags_and_the_canvas_never_call_the_motion_engine() {
    let src = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut files = vec![
        format!("{src}/a11y.rs"),
        format!("{src}/canvas.rs"),
        format!("{src}/docking/drag.rs"),
        format!("{src}/widgets/drop.rs"),
    ];
    let mut dirs = vec![std::path::PathBuf::from(format!("{src}/canvas"))];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(path.display().to_string());
            }
        }
    }
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        assert!(!text.contains("motion::"), "{file} animates");
    }
    // In the docking, only the Dock's strip, its collapse and the flyout move: a tab's
    // contents are drawn by `content`, which has no transition.
    let docking = std::fs::read_to_string(format!("{src}/docking.rs")).unwrap();
    let content = docking
        .split("    fn content(")
        .nth(1)
        .and_then(|rest| rest.split("\n    fn ").next())
        .expect("docking draws a tab's contents in `content`");
    assert!(!content.contains("motion"));
    // The focus ring of a REDA widget is painted from its state, which is not faded.
    let paint = std::fs::read_to_string(format!("{src}/widgets/paint.rs")).unwrap();
    let ring = paint
        .split("pub(super) fn focus_ring(")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap();
    assert!(!ring.contains("motion"));
}
