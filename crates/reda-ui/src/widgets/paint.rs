//! What every recipe shares: the states of a control, the neutral state fill, text laid
//! out from a text-style token, and the focus ring
//! (docs/spec/v0.1/design-language.md#states-shared-by-every-control).

use std::sync::Arc;

use egui::emath::TSTransform;
use egui::layers::ShapeIdx;
use egui::{Align2, Color32, Galley, Pos2, Rect, Response, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::theme::{Text, Tokens, TokensExt as _, motion, over};

/// The state a widget draws. `Live` follows the pointer, the keyboard and whether the
/// `Ui` is enabled; the others force one state, for the gallery.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum State {
    #[default]
    Live,
    Rest,
    Hover,
    Press,
    Focus,
    Disabled,
}

/// The white of a switch's or a slider's knob, in both themes
/// (docs/spec/v0.1/design-language.md#check-box-radio-and-switch).
pub(super) const KNOB: Color32 = Color32::WHITE;

/// What a widget shows of its state.
#[derive(Clone, Copy)]
pub(super) struct Seen {
    pub hover: bool,
    pub press: bool,
    pub focus: bool,
    /// How much of the hover fill is shown: 1 while hovered, then fading out
    /// (`motion.hover`). Hover arriving is immediate.
    pub hover_fill: f32,
    /// How much of the press fill is shown: 1 while pressed, then fading out
    /// (`motion.press`).
    pub press_fill: f32,
}

impl Seen {
    /// A state shown as it is, with nothing fading.
    pub fn still(hover: bool, press: bool, focus: bool) -> Seen {
        Seen {
            hover,
            press,
            focus,
            hover_fill: f32::from(u8::from(hover)),
            press_fill: f32::from(u8::from(press)),
        }
    }
}

pub(super) fn seen(response: &Response, state: State) -> Seen {
    match state {
        State::Live => {
            let focus = response.has_focus();
            if focus {
                draws_focus(&response.ctx, response.id);
            }
            Seen {
                focus,
                ..live(
                    response,
                    response.hovered(),
                    response.is_pointer_button_down_on(),
                )
            }
        }
        State::Hover => Seen::still(true, false, false),
        State::Press => Seen::still(true, true, false),
        State::Focus => Seen::still(false, false, true),
        State::Rest | State::Disabled => Seen::still(false, false, false),
    }
}

/// `hover` and `press` as the widget of `response` shows them: on at once, fading out
/// as the motion tokens say. The focus ring is never animated.
pub(super) fn live(response: &Response, hover: bool, press: bool) -> Seen {
    let ctx = &response.ctx;
    let m = ctx.tokens().motion;
    Seen {
        hover,
        press,
        focus: false,
        hover_fill: motion::toggle(ctx, response.id.with("hover-fill"), hover, m.hover),
        press_fill: motion::toggle(ctx, response.id.with("press-fill"), press, m.press),
    }
}

/// Notes that the widget `id`, which holds keyboard focus, draws its own focus mark this
/// frame, so the window's fallback ring ([`crate::a11y::focus_ring`]) leaves it alone.
pub(super) fn draws_focus(ctx: &egui::Context, id: egui::Id) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| d.insert_temp(own_focus_id(), (frame, id)));
}

/// Whether the widget `id` drew its own focus mark this frame.
pub fn drew_focus(ctx: &egui::Context, id: egui::Id) -> bool {
    let frame = ctx.cumulative_frame_nr();
    ctx.data(|d| d.get_temp::<(u64, egui::Id)>(own_focus_id())) == Some((frame, id))
}

fn own_focus_id() -> egui::Id {
    egui::Id::NULL.with("reda-widget-draws-focus")
}

/// Runs `add` in a disabled `Ui` when the state is forced to disabled. A disabled `Ui`
/// fades everything painted in it once, by `opacity.disabled`.
pub(super) fn stated<R>(ui: &mut Ui, state: State, add: impl FnOnce(&mut Ui) -> R) -> R {
    if state == State::Disabled {
        ui.add_enabled_ui(false, add).inner
    } else {
        add(ui)
    }
}

/// The neutral state fill for a control whose own fill is `under`: hover lays
/// `fill.hover` over what is there, and press lays `fill.press` in its place. Pass
/// `Color32::TRANSPARENT` for a flat control.
///
/// While a hover or a press fades out, the fill is that much of it: the press fill gives
/// way to the hover fill, and the hover fill to nothing.
pub(super) fn state_fill(t: &Tokens, under: Color32, s: Seen) -> Color32 {
    let hover = if s.hover { 1.0 } else { s.hover_fill };
    let press = if s.press { 1.0 } else { s.press_fill };
    if hover <= 0.0 && press <= 0.0 {
        return under;
    }
    let top = mix(t.color.fill.hover.gamma_multiply(hover), t.color.fill.press, press);
    if under == Color32::TRANSPARENT {
        top
    } else {
        over(top, under)
    }
}

/// `a` turned `k` of the way into `b` (0 to 1), in premultiplied colour: how one fill
/// fades into another.
pub(super) fn mix(a: Color32, b: Color32, k: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    if k <= 0.0 {
        return a;
    }
    if k >= 1.0 {
        return b;
    }
    let m = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * k).round() as u8;
    Color32::from_rgba_premultiplied(
        m(a.r(), b.r()),
        m(a.g(), b.g()),
        m(a.b(), b.b()),
        m(a.a(), b.a()),
    )
}

pub(super) fn galley(ui: &Ui, style: &Text, text: &str, color: Color32) -> Arc<Galley> {
    ui.painter().layout_job(style.job(text, color))
}

pub(super) fn width(ui: &Ui, style: &Text, text: &str) -> f32 {
    galley(ui, style, text, Color32::PLACEHOLDER).size().x
}

/// Paints `text` in `rect`, on whole pixels; returns where it was drawn. The galley is as
/// tall as the style's line height, so centring it centres the line.
pub(super) fn text_in(
    ui: &Ui,
    rect: Rect,
    align: Align2,
    style: &Text,
    text: &str,
    color: Color32,
) -> Rect {
    let g = galley(ui, style, text, color);
    let at = align.align_size_within_rect(g.size(), rect).min;
    let at = pos2(at.x.round(), at.y.round());
    let drawn = Rect::from_min_size(at, g.size());
    ui.painter().galley(at, g, color);
    drawn
}

/// One line of `text`, cut with an ellipsis to fit `rect` and centred on its height.
pub(super) fn text_clipped(ui: &Ui, rect: Rect, style: &Text, text: &str, color: Color32) {
    let mut job = style.job(text, color);
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let g = ui.painter().layout_job(job);
    let at = pos2(rect.left(), (rect.center().y - g.size().y / 2.0).round());
    ui.painter().galley(at, g, color);
}

/// Keyboard focus: a ring in `stroke.focus_ring`, 1 px outside the control, following its
/// radius.
pub(super) fn focus_ring(ui: &Ui, t: &Tokens, rect: Rect, radius: f32) {
    ui.painter().rect_stroke(
        rect.expand(RING_GAP),
        radius + RING_GAP,
        Stroke::new(t.stroke.focus_ring, t.color.stroke.focus_ring),
        StrokeKind::Outside,
    );
}

/// How far outside a control its focus ring sits.
pub(super) const RING_GAP: f32 = 1.0;

/// Paints with `paint` on `ui`'s layer, then scales what it painted by `k` around
/// `center`: a mark growing as it turns on. At 1 nothing is moved.
pub(super) fn scaled(ui: &Ui, center: Pos2, k: f32, paint: impl FnOnce()) {
    let (start, layer) = painted_from(ui);
    paint();
    if k >= 1.0 {
        return;
    }
    let k = k.max(0.0);
    let grow = TSTransform::new(center.to_vec2() * (1.0 - k), k);
    ui.ctx().graphics_mut(|g| {
        let list = g.entry(layer);
        let end = list.next_idx();
        list.transform_range(start, end, grow);
    });
}

/// Paints with `paint` on `ui`'s layer, then turns what it painted by `angle` (radians,
/// clockwise on screen) around `center`: a chevron turning as its section opens. Circles
/// and paths are turned; at zero nothing is moved.
pub(super) fn turned(ui: &Ui, center: Pos2, angle: f32, paint: impl FnOnce()) {
    let (start, layer) = painted_from(ui);
    paint();
    if angle == 0.0 {
        return;
    }
    let (sin, cos) = angle.sin_cos();
    let turn = move |p: &mut Pos2| {
        let d = *p - center;
        *p = center + vec2(d.x * cos - d.y * sin, d.x * sin + d.y * cos);
    };
    fn shape(s: &mut Shape, turn: &dyn Fn(&mut Pos2)) {
        match s {
            Shape::Vec(shapes) => shapes.iter_mut().for_each(|s| shape(s, turn)),
            Shape::Circle(c) => turn(&mut c.center),
            Shape::LineSegment { points, .. } => points.iter_mut().for_each(turn),
            Shape::Path(path) => path.points.iter_mut().for_each(turn),
            _ => {}
        }
    }
    ui.ctx().graphics_mut(|g| {
        let list = g.entry(layer);
        for i in start.0..list.next_idx().0 {
            list.mutate_shape(ShapeIdx(i), |c| shape(&mut c.shape, &turn));
        }
    });
}

/// Where the next shape painted on `ui`'s layer goes.
fn painted_from(ui: &Ui) -> (ShapeIdx, egui::LayerId) {
    let layer = ui.layer_id();
    let start = ui
        .ctx()
        .graphics(|g| g.get(layer).map_or(ShapeIdx(0), |l| l.next_idx()));
    (start, layer)
}
