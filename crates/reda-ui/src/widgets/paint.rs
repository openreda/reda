//! What every recipe shares: the states of a control, the neutral state fill, text laid
//! out from a text-style token, and the focus ring
//! (docs/spec/v0.1/design-language.md#states-shared-by-every-control).

use std::sync::Arc;

use egui::{Align2, Color32, Galley, Rect, Response, Stroke, StrokeKind, Ui, pos2};

use crate::theme::{Text, Tokens, over};

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
}

pub(super) fn seen(response: &Response, state: State) -> Seen {
    match state {
        State::Live => Seen {
            hover: response.hovered(),
            press: response.is_pointer_button_down_on(),
            focus: response.has_focus(),
        },
        State::Hover => Seen {
            hover: true,
            press: false,
            focus: false,
        },
        State::Press => Seen {
            hover: true,
            press: true,
            focus: false,
        },
        State::Focus => Seen {
            hover: false,
            press: false,
            focus: true,
        },
        State::Rest | State::Disabled => Seen {
            hover: false,
            press: false,
            focus: false,
        },
    }
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
pub(super) fn state_fill(t: &Tokens, under: Color32, s: Seen) -> Color32 {
    let top = if s.press {
        t.color.fill.press
    } else if s.hover {
        t.color.fill.hover
    } else {
        return under;
    };
    if under == Color32::TRANSPARENT {
        top
    } else {
        over(top, under)
    }
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
