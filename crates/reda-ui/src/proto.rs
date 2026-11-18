//! PROTOTYPE (issue #330): how should the Stackup editor tab look and behave? Throwaway.
//!
//! Three variants of the Stackup editor tab, drawn in place of the PCB canvas in a PCB
//! Document's tab, so the real Title bar, Docks and status bar sit around them:
//!
//! - **A · Drawing** (`variant_a`): the cross-section is the editor. No table.
//! - **B · Ledger** (`variant_b`): one grid whose rows are the bands; Via types are
//!   columns of the same grid.
//! - **C · Workbench** (`variant_c`): tables on the left, a faithful preview in the
//!   middle, readouts and board values on the right.
//!
//! Run: `REDA_PROTO_STACKUP=1 cargo run -p reda-app`, open a Project and its PCB. F10
//! turns the prototype on and off, F9 and Shift+F9 step the variant.
//!
//! The Stackup edited is in memory (`model`), shared by the three variants, and never
//! touches the board. See `docs/prototype/stackup-editor/README.md`.

pub mod kit;
pub mod model;
mod variant_a;
mod variant_b;
mod variant_c;
mod variant_d;

use std::sync::{Mutex, MutexGuard, OnceLock};

use model::Stack;

/// The variants, in switcher order.
pub const VARIANTS: [&str; 4] = [
    "A · Drawing",
    "B · Ledger",
    "C · Workbench",
    "D · Workbench plus",
];

/// What the screens remember besides the Stackup.
#[derive(Default)]
pub struct View {
    /// Bands drawn by thickness, not by kind.
    pub to_scale: bool,
    /// The entry whose thickness is being typed in the drawing (variant A).
    pub editing: Option<u32>,
    /// The copper Layer being dragged to a new place (variant A), by copper number.
    pub dragging: Option<usize>,
}

pub struct State {
    on: Option<bool>,
    pub variant: usize,
    pub stack: Stack,
    pub view: View,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE
        .get_or_init(|| {
            Mutex::new(State {
                on: None,
                variant: 0,
                stack: Stack::preset(model::PRESETS[1]),
                view: View::default(),
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Whether PCB tabs show the prototype. Starts on with `REDA_PROTO_STACKUP` set.
pub fn on() -> bool {
    let mut s = state();
    *s.on
        .get_or_insert_with(|| std::env::var_os("REDA_PROTO_STACKUP").is_some())
}

pub fn set_on(on: bool) {
    state().on = Some(on);
}

pub fn set_variant(variant: usize) {
    state().variant = variant % VARIANTS.len();
}

/// Replaces the Stackup with a sample, for the screenshots.
pub fn load(preset: &'static str) {
    let mut s = state();
    s.stack = Stack::preset(preset);
    s.view = View::default();
}

/// Edits the prototype's state, for the screenshots.
pub fn with<R>(edit: impl FnOnce(&mut Stack, &mut View) -> R) -> R {
    let mut s = state();
    let s = &mut *s;
    edit(&mut s.stack, &mut s.view)
}

/// The keys: F10 on and off, F9 and Shift+F9 the variant. Called once a frame.
pub fn keys(ctx: &egui::Context) {
    use egui::{Key, Modifiers};
    let (toggle, next, previous) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::F10),
            i.consume_key(Modifiers::NONE, Key::F9),
            i.consume_key(Modifiers::SHIFT, Key::F9),
        )
    });
    if toggle {
        let now = on();
        set_on(!now);
    }
    let mut s = state();
    let n = VARIANTS.len();
    if next {
        s.variant = (s.variant + 1) % n;
    }
    if previous {
        s.variant = (s.variant + n - 1) % n;
    }
}

/// The Stackup editor tab's content, in the current variant.
pub fn show(ui: &mut egui::Ui) {
    let mut s = state();
    let s = &mut *s;
    match s.variant {
        0 => variant_a::show(ui, &mut s.stack, &mut s.view),
        1 => variant_b::show(ui, &mut s.stack, &mut s.view),
        2 => variant_c::show(ui, &mut s.stack, &mut s.view),
        _ => variant_d::show(ui, &mut s.stack, &mut s.view),
    }
}

/// The floating bar that steps the variant: not part of any design under review.
/// Hidden with `REDA_PROTO_SWITCHER=0`.
pub fn switcher(ctx: &egui::Context) {
    if !on() || std::env::var("REDA_PROTO_SWITCHER").as_deref() == Ok("0") {
        return;
    }
    let mut step = 0isize;
    let variant = state().variant;
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("prototype-switcher"))
        .order(egui::Order::Tooltip)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(screen.center_bottom() - egui::vec2(0.0, 36.0))
        .show(ctx, |ui| {
            // theme-exempt: the prototype's own switcher, deliberately off-theme.
            let ink = egui::Color32::from_rgb(0xff, 0xe0, 0x4a);
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x11, 0x11, 0x11))
                .stroke(egui::Stroke::new(1.0, ink))
                .corner_radius(255)
                .inner_margin(egui::Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.visuals_mut().override_text_color = Some(ink);
                    ui.horizontal(|ui| {
                        let flat = |t: &str| egui::Button::new(t).frame(false);
                        if ui.add(flat("◀")).clicked() {
                            step = -1;
                        }
                        ui.label(format!("PROTOTYPE  {}", VARIANTS[variant]));
                        if ui.add(flat("▶")).clicked() {
                            step = 1;
                        }
                        ui.label("·  F9 / Shift+F9 · F10 off");
                    });
                });
        });
    if step != 0 {
        let n = VARIANTS.len() as isize;
        set_variant((variant as isize + step).rem_euclid(n) as usize);
    }
}
