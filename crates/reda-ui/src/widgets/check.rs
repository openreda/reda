//! Check box, radio and switch
//! (docs/spec/v0.1/design-language.md#check-box-radio-and-switch).
//!
//! A switch is for a setting that applies the moment it is flipped. A check box is for a
//! choice confirmed with a button, and for lists and tables.

use egui::{
    Align2, Id, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, Vec2, Widget, WidgetInfo,
    pos2, vec2,
};

use super::glyph::Glyph;
use super::paint::{self, Seen, State};
use crate::theme::{Tokens, TokensExt as _, motion};

/// The side of a check box's or a radio's mark.
pub const MARK: f32 = 16.0;
/// A switch's track.
pub const SWITCH_TRACK: Vec2 = Vec2::new(28.0, 16.0);
/// The diameter of a switch's knob.
pub const SWITCH_KNOB: f32 = 12.0;
/// Between a mark and its label.
const LABEL_GAP: f32 = 8.0;
/// The diameter of the dot in a radio that is on.
const RADIO_DOT: f32 = 6.0;

/// The row every one of the three shares: a mark, then the label, a control tall so that
/// a column of them lines up with a column of fields. `radius` is the mark's, for its
/// focus ring. `paint_mark` is given the control's id, under which its own transition is
/// kept.
#[expect(clippy::too_many_arguments)]
fn mark_row(
    ui: &mut Ui,
    t: &Tokens,
    state: State,
    text: &str,
    mark: Vec2,
    radius: f32,
    info: impl Fn(bool) -> WidgetInfo,
    paint_mark: impl FnOnce(&Ui, Rect, Seen, Id),
) -> Response {
    paint::stated(ui, state, |ui| {
        let label = if text.is_empty() {
            0.0
        } else {
            LABEL_GAP + paint::width(ui, &t.ty.body, text)
        };
        let size = vec2(mark.x + label, t.control_height());
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let enabled = ui.is_enabled();
        response.widget_info(|| info(enabled));
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let s = paint::seen(&response, state);
        let at = Rect::from_center_size(pos2(rect.left() + mark.x / 2.0, rect.center().y), mark);
        paint_mark(ui, at, s, response.id);
        let label = rect.with_min_x(at.right() + LABEL_GAP);
        let ink = t.color.text.primary;
        paint::text_in(ui, label, Align2::LEFT_CENTER, &t.ty.body, text, ink);
        if s.focus {
            paint::focus_ring(ui, t, at, radius);
        }
        response
    })
}

/// A check box. Clicking it flips `checked`.
pub fn checkbox(checked: &mut bool, text: impl Into<String>) -> Checkbox<'_> {
    Checkbox {
        checked,
        text: text.into(),
        mixed: false,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct Checkbox<'a> {
    checked: &'a mut bool,
    text: String,
    mixed: bool,
    state: State,
}

impl Checkbox<'_> {
    /// Shows a minus instead: the things it stands for disagree.
    pub fn indeterminate(mut self, mixed: bool) -> Self {
        self.mixed = mixed;
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for Checkbox<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        let (on, mixed) = (*self.checked, self.mixed);
        let mut response = mark_row(
            ui,
            &t,
            self.state,
            &self.text,
            Vec2::splat(MARK),
            t.radius.mark,
            |enabled| WidgetInfo::selected(Role::CheckBox, enabled, on, &self.text),
            |ui, mark, s, id| {
                let c = &t.color;
                // The fill fades in over the empty box, and the mark grows.
                let k = motion::toggle(ui.ctx(), id.with("on"), on || mixed, t.motion.toggle);
                if k < 1.0 {
                    ui.painter()
                        .rect_filled(mark, t.radius.mark, c.surface.well);
                    ui.painter().rect_stroke(
                        mark,
                        t.radius.mark,
                        Stroke::new(t.stroke.hairline, outline(&t, s)),
                        StrokeKind::Inside,
                    );
                }
                if k > 0.0 {
                    let fill = paint::state_fill(&t, c.accent.solid, s);
                    ui.painter()
                        .rect_filled(mark, t.radius.mark, fill.gamma_multiply(k));
                    let glyph = if mixed { Glyph::Minus } else { Glyph::Tick };
                    paint::scaled(ui, mark.center(), k, || {
                        glyph.paint(ui.painter(), mark.center(), c.accent.on_solid);
                    });
                }
            },
        );
        if response.clicked() {
            *self.checked = !*self.checked;
            response.mark_changed();
        }
        response
    }
}

/// The outline of a mark that is off: `stroke.strong`, darker when hovered.
fn outline(t: &Tokens, s: Seen) -> egui::Color32 {
    if s.hover {
        t.color.text.muted
    } else {
        t.color.stroke.strong
    }
}

/// A radio button. It only shows `on`; the caller sets the choice when it is clicked.
pub fn radio(on: bool, text: impl Into<String>) -> Radio {
    Radio {
        on,
        text: text.into(),
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct Radio {
    on: bool,
    text: String,
    state: State,
}

impl Radio {
    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for Radio {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        let on = self.on;
        mark_row(
            ui,
            &t,
            self.state,
            &self.text,
            Vec2::splat(MARK),
            MARK / 2.0,
            |enabled| WidgetInfo::selected(Role::RadioButton, enabled, on, &self.text),
            |ui, mark, s, id| {
                let c = &t.color;
                let p = ui.painter();
                let round = MARK / 2.0;
                // The fill fades in over the empty ring, and the dot grows.
                let k = motion::toggle(ui.ctx(), id.with("on"), on, t.motion.toggle);
                if k < 1.0 {
                    p.rect_filled(mark, round, c.surface.well);
                    p.rect_stroke(
                        mark,
                        round,
                        Stroke::new(t.stroke.hairline, outline(&t, s)),
                        StrokeKind::Inside,
                    );
                }
                if k > 0.0 {
                    let fill = paint::state_fill(&t, c.accent.solid, s);
                    p.rect_filled(mark, round, fill.gamma_multiply(k));
                    p.circle_filled(mark.center(), k * RADIO_DOT / 2.0, c.accent.on_solid);
                }
            },
        )
    }
}

/// A switch. Clicking it flips `on`.
pub fn switch(on: &mut bool, text: impl Into<String>) -> Switch<'_> {
    Switch {
        on,
        text: text.into(),
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct Switch<'a> {
    on: &'a mut bool,
    text: String,
    state: State,
}

impl Switch<'_> {
    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for Switch<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        let on = *self.on;
        let mut response = mark_row(
            ui,
            &t,
            self.state,
            &self.text,
            SWITCH_TRACK,
            SWITCH_TRACK.y / 2.0,
            |enabled| WidgetInfo::selected(Role::Switch, enabled, on, &self.text),
            |ui, track, s, id| {
                let c = &t.color;
                // The knob slides and the track's colour mixes from off to on.
                let k = motion::toggle(ui.ctx(), id.with("on"), on, t.motion.switch);
                let under = paint::mix(c.stroke.strong, c.accent.solid, k);
                let round = SWITCH_TRACK.y / 2.0;
                ui.painter()
                    .rect_filled(track, round, paint::state_fill(&t, under, s));
                let x = egui::lerp((track.left() + round)..=(track.right() - round), k);
                let knob = pos2(x, track.center().y);
                ui.painter()
                    .circle_filled(knob, SWITCH_KNOB / 2.0, paint::KNOB);
            },
        );
        if response.clicked() {
            *self.on = !*self.on;
            response.mark_changed();
        }
        response
    }
}
