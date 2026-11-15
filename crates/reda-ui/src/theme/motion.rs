//! The motion engine: plays the `motion` tokens
//! (docs/spec/v0.1/design-language.md#motion).
//!
//! egui's `animate_bool` takes one time both ways, so REDA keeps a small engine of its
//! own: per id, a start value, a target, a start time and an easing, in egui's memory.
//! Time is the input's clock (`ctx.input(|i| i.time)`), so a test drives it by hand.
//!
//! - [`toggle`]: how far something that is on or off has got (a check box's fill, a
//!   hover fill fading out, a Dock opening). Enters with `enter`, leaves with `leave`.
//! - [`follow`]: a value that catches up with its target (a progress bar).
//! - [`arrive`]: how far something that has just appeared has got.
//! - [`layer`]: fades, moves and scales a whole layer as it arrives (a menu, a dialog,
//!   the Command Palette), and keeps painting its last look while it leaves, after its
//!   owner has stopped drawing it. The layer is painted once at its final place and its
//!   shapes are moved, so nothing is laid out twice and input is never moved.
//!
//! With Animate off the tokens' transitions are all zero, so every call returns its
//! final value at once and nothing repaints.

pub mod system;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::emath::TSTransform;
use egui::epaint::ClippedShape;
use egui::layers::ShapeIdx;
use egui::{Color32, Context, Id, LayerId, Pos2, Shape, Vec2, ViewportId};

use super::{Easing, Transition};

/// The eased share of a transition `x` of the way through (0 to 1).
pub fn eased(easing: Easing, x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => x,
        Easing::Out => 1.0 - (1.0 - x).powi(3),
    }
}

/// One transition in flight: from a value to a target, starting at a time.
#[derive(Clone, Copy, Debug)]
struct Run {
    from: f32,
    to: f32,
    start: f64,
    duration: f32,
    easing: Easing,
    /// The pass it was last asked about.
    pass: u64,
}

impl Run {
    fn settled(value: f32, now: f64, pass: u64) -> Run {
        Run {
            from: value,
            to: value,
            start: now,
            duration: 0.0,
            easing: Easing::Linear,
            pass,
        }
    }

    fn share(&self, now: f64) -> f32 {
        if self.duration <= 0.0 {
            1.0
        } else {
            ((now - self.start) as f32 / self.duration).clamp(0.0, 1.0)
        }
    }

    fn at(&self, now: f64) -> f32 {
        let x = self.share(now);
        if x >= 1.0 {
            self.to
        } else {
            self.from + (self.to - self.from) * eased(self.easing, x)
        }
    }

    fn moving(&self, now: f64) -> bool {
        self.share(now) < 1.0
    }
}

fn clock(ctx: &Context) -> (f64, u64) {
    (ctx.input(|i| i.time), ctx.cumulative_pass_nr())
}

/// Moves the run under `id` towards `target`: a new target starts a transition from
/// where the old one has got, taking the time `pick` gives for it. A run seen for the
/// first time starts at `first`.
fn drive(
    ctx: &Context,
    id: Id,
    target: f32,
    first: f32,
    pick: impl FnOnce(f32) -> (f32, Easing),
) -> f32 {
    let (now, pass) = clock(ctx);
    let mut run = ctx
        .data(|d| d.get_temp::<Run>(id))
        .unwrap_or_else(|| Run::settled(first, now, pass));
    if run.to != target {
        let from = run.at(now);
        let (duration, easing) = pick(from);
        run = Run {
            from,
            to: target,
            start: now,
            duration,
            easing,
            pass,
        };
    }
    run.pass = pass;
    ctx.data_mut(|d| d.insert_temp(id, run));
    if run.moving(now) {
        ctx.request_repaint();
    }
    run.at(now)
}

/// How far something that is `on` or off has got: 1 when on, 0 when off, between while
/// it changes. Turning on takes `tr.enter`, turning off `tr.leave`. What is seen for the
/// first time is where it is, with no transition (a check box drawn ticked).
pub fn toggle(ctx: &Context, id: Id, on: bool, tr: Transition) -> f32 {
    let target = if on { 1.0 } else { 0.0 };
    drive(ctx, id, target, target, |_| {
        let duration = if on {
            tr.enter
        } else {
            tr.leave.unwrap_or(0.0)
        };
        (duration, tr.easing)
    })
}

/// A value that catches up with `target` in `tr.enter` whenever the target moves (a
/// progress bar). It starts at its target.
pub fn follow(ctx: &Context, id: Id, target: f32, tr: Transition) -> f32 {
    drive(ctx, id, target, target, |_| (tr.enter, tr.easing))
}

/// How far something that appeared has got, eased: 0 on the pass it appears (it was not
/// asked about on the pass before), 1 once `tr.enter` has passed.
pub fn arrive(ctx: &Context, id: Id, tr: Transition) -> f32 {
    let (now, pass) = clock(ctx);
    let run = ctx.data(|d| d.get_temp::<Run>(id));
    let mut run = match run {
        Some(run) if run.pass + 1 >= pass => run,
        _ => Run {
            from: 0.0,
            to: 1.0,
            start: now,
            duration: tr.enter,
            easing: tr.easing,
            pass,
        },
    };
    run.pass = pass;
    ctx.data_mut(|d| d.insert_temp(id, run));
    if run.moving(now) {
        ctx.request_repaint();
    }
    run.at(now)
}

/// How a layer arrives and leaves: from where, at which scale, and whether it fades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Where it starts, from its place: the travel.
    pub offset: Vec2,
    /// The scale it starts at, around `origin`.
    pub scale: f32,
    pub origin: Pos2,
    /// Whether its opacity goes with it. A popup's fade-in is egui's own (the `menu`
    /// token is the style's animation time), so a menu only travels.
    pub fade: bool,
}

impl Look {
    /// A fade and a travel from `offset`.
    pub fn travel(offset: Vec2) -> Look {
        Look {
            offset,
            scale: 1.0,
            origin: Pos2::ZERO,
            fade: true,
        }
    }

    /// A fade, growing from `scale` around `origin`.
    pub fn grow(scale: f32, origin: Pos2) -> Look {
        Look {
            offset: Vec2::ZERO,
            scale,
            origin,
            fade: true,
        }
    }

    /// Travel only: something egui already fades.
    pub fn without_fade(mut self) -> Look {
        self.fade = false;
        self
    }

    /// Where the shapes are when the layer is `k` of the way there (0 to 1).
    fn transform(&self, k: f32) -> TSTransform {
        let s = egui::lerp(self.scale..=1.0, k);
        let offset = self.offset * (1.0 - k);
        TSTransform::new(self.origin.to_vec2() * (1.0 - s) + offset, s)
    }
}

/// A layer the engine moves: how it looks, what it last painted, and when.
#[derive(Clone)]
struct Moving {
    look: Look,
    tr: Transition,
    viewport: ViewportId,
    /// The pass it was last shown on.
    shown: u64,
    arrived: f64,
    /// Its shapes as last shown, for leaving; kept only when it takes time to leave.
    shapes: Arc<Vec<ClippedShape>>,
    leaving: Option<f64>,
}

#[derive(Clone, Default)]
struct Layers(Vec<(LayerId, Moving)>);

fn layers_id() -> Id {
    Id::NULL.with("reda-motion-layers")
}

/// Fades, moves and scales `layer` as it arrives, as `look` says, in `tr.enter`; call it
/// on each pass the layer is shown, after drawing it. When the layer stops being shown,
/// its last look is painted fading out (and travelling back) for `tr.leave`. The effect
/// is applied at the end of the pass, once the layer is complete, by the plugin
/// [`super::set`] installs.
pub fn layer(ctx: &Context, layer: LayerId, look: Look, tr: Transition) {
    let (now, pass) = clock(ctx);
    let viewport = ctx.viewport_id();
    ctx.data_mut(|d| {
        let layers = &mut d.get_temp_mut_or_default::<Layers>(layers_id()).0;
        match layers.iter_mut().find(|(l, _)| *l == layer) {
            Some((_, moving)) => {
                if moving.shown + 1 < pass || moving.leaving.is_some() {
                    moving.arrived = now;
                    moving.leaving = None;
                }
                moving.shown = pass;
                moving.look = look;
                moving.tr = tr;
                moving.viewport = viewport;
            }
            None => layers.push((
                layer,
                Moving {
                    look,
                    tr,
                    viewport,
                    shown: pass,
                    arrived: now,
                    shapes: Arc::default(),
                    leaving: None,
                },
            )),
        }
    });
}

/// Whether `layer` is painted leaving: its owner no longer draws it, and its last look
/// is fading out.
pub fn is_leaving(ctx: &Context, layer: LayerId) -> bool {
    ctx.data(|d| {
        d.get_temp::<Layers>(layers_id()).is_some_and(|layers| {
            layers
                .0
                .iter()
                .any(|(l, m)| *l == layer && m.leaving.is_some())
        })
    })
}

/// The layer a leaving layer's last look is painted on.
pub fn leaving_layer(layer: LayerId) -> LayerId {
    LayerId::new(layer.order, layer.id.with("reda-leaving"))
}

/// Multiplies the opacity of `shape` by `opacity`, as egui's painter does.
fn fade(shape: &mut Shape, opacity: f32) {
    egui::epaint::shape_transform::adjust_colors(shape, move |color| {
        if *color != Color32::PLACEHOLDER {
            *color = color.gamma_multiply(opacity);
        }
    });
}

/// Applies the arrivals of the layers shown this pass and paints those leaving.
fn end_pass(ctx: &Context) {
    let (now, pass) = clock(ctx);
    let viewport = ctx.viewport_id();
    let Some(mut layers) = ctx.data_mut(|d| d.remove_temp::<Layers>(layers_id())) else {
        return;
    };
    let mut repaint = false;
    layers.0.retain_mut(|(layer, moving)| {
        if moving.viewport != viewport {
            return true;
        }
        let layer = *layer;
        let leave = moving.tr.leave.unwrap_or(0.0);
        if moving.shown == pass {
            if leave > 0.0 {
                let shapes = ctx.graphics(|g| {
                    g.get(layer)
                        .map(|list| list.all_entries().cloned().collect::<Vec<_>>())
                        .unwrap_or_default()
                });
                moving.shapes = Arc::new(shapes);
            }
            let x = if moving.tr.enter <= 0.0 {
                1.0
            } else {
                ((now - moving.arrived) as f32 / moving.tr.enter).clamp(0.0, 1.0)
            };
            if x < 1.0 {
                let k = eased(moving.tr.easing, x);
                if moving.look.fade {
                    ctx.graphics_mut(|g| {
                        if let Some(list) = g.get_mut(layer) {
                            for i in 0..list.next_idx().0 {
                                list.mutate_shape(ShapeIdx(i), |s| fade(&mut s.shape, k));
                            }
                        }
                    });
                }
                ctx.transform_layer_shapes_after_rounding(layer, moving.look.transform(k));
                repaint = true;
            }
            return true;
        }
        // Not shown on this pass: it leaves, painting its last look.
        let started = *moving.leaving.get_or_insert(now);
        let x = if leave > 0.0 {
            (now - started) as f32 / leave
        } else {
            1.0
        };
        if x >= 1.0 || moving.shapes.is_empty() {
            return false;
        }
        let k = 1.0 - eased(moving.tr.easing, x);
        let ghost = leaving_layer(layer);
        let painter = ctx.layer_painter(ghost);
        for clipped in moving.shapes.iter() {
            if matches!(clipped.shape, Shape::Callback(_)) {
                continue;
            }
            let mut shape = clipped.shape.clone();
            if moving.look.fade {
                fade(&mut shape, k);
            }
            painter.with_clip_rect(clipped.clip_rect).add(shape);
        }
        ctx.transform_layer_shapes_after_rounding(ghost, moving.look.transform(k));
        repaint = true;
        true
    });
    ctx.data_mut(|d| d.insert_temp(layers_id(), layers));
    if repaint {
        ctx.request_repaint();
    }
}

/// The plugin that runs [`end_pass`] at the end of every pass.
pub(super) struct Plugin;

impl egui::Plugin for Plugin {
    fn debug_name(&self) -> &'static str {
        "reda-motion"
    }

    fn on_end_pass(&mut self, ui: &mut egui::Ui) {
        end_pass(ui.ctx());
    }
}
