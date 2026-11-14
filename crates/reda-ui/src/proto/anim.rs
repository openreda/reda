//! PROTOTYPE (issue #282): motion profiles and the small engine that plays them. Throwaway.
//!
//! One question: which transitions does the chrome animate, and with which timing? Three
//! profiles answer it differently (F8 cycles): A · Still, B · Crisp, C · Fluid. Every
//! widget and every demo on the Motion page reads its timing from the current profile, so
//! a profile is the whole answer. A speed factor slows everything for inspection, and
//! Animate off makes every transition happen at once.

use std::sync::atomic::{AtomicU8, Ordering};

use egui::{Context, Id};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ease {
    Linear,
    /// Fast start, soft landing (cubic).
    Out,
    /// Soft start and landing (cubic): for things that travel.
    InOut,
    /// Overshoots by about a tenth and settles. Entering only; leaving uses `Out`.
    Spring,
}

impl Ease {
    pub fn name(self) -> &'static str {
        match self {
            Ease::Linear => "linear",
            Ease::Out => "ease-out",
            Ease::InOut => "ease-in-out",
            Ease::Spring => "spring",
        }
    }

    fn at(self, x: f32) -> f32 {
        match self {
            Ease::Linear => x,
            Ease::Out => 1.0 - (1.0 - x).powi(3),
            Ease::InOut => {
                if x < 0.5 {
                    4.0 * x * x * x
                } else {
                    1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
                }
            }
            Ease::Spring => 1.0 - (-6.5 * x).exp() * (10.0 * x).cos(),
        }
    }
}

/// One transition: seconds to enter (appear, turn on, travel) and to leave.
#[derive(Clone, Copy)]
pub struct Tr {
    pub enter: f32,
    pub exit: f32,
    pub ease: Ease,
}

impl Tr {
    pub const NONE: Tr = tr(0, 0, Ease::Linear);
}

const fn tr(enter_ms: u16, exit_ms: u16, ease: Ease) -> Tr {
    Tr {
        enter: enter_ms as f32 / 1000.0,
        exit: exit_ms as f32 / 1000.0,
        ease,
    }
}

#[derive(Clone, Copy)]
pub struct Profile {
    pub name: &'static str,
    pub about: &'static str,
    /// Hover fills on anything, and the scrollbar widening.
    pub hover: Tr,
    /// The press fill: on at once when pressed, this is how it lets go.
    pub press: Tr,
    /// Check box, radio, and the on state of a toggle button.
    pub check: Tr,
    pub switch: Tr,
    /// The accent line of the active Document's tab.
    pub tab: Tr,
    /// Menus, context menus, combo lists.
    pub menu: Tr,
    pub tooltip: Tr,
    /// A collapsed Dock's flyout.
    pub flyout: Tr,
    /// A Dock collapsing to its icon strip and back.
    pub dock: Tr,
    /// A Panel section folding.
    pub section: Tr,
    /// Dialogs, and the scrim behind a modal.
    pub dialog: Tr,
    pub palette: Tr,
    /// The selected row of a list, tree or table.
    pub select: Tr,
    /// On the canvas side: the Tool bar that appears under a Tool, the on state gliding.
    pub bar: Tr,
    /// A progress bar catching up with its value.
    pub progress: Tr,
    /// Seconds the pointer rests before a tooltip shows.
    pub tooltip_delay: f32,
    /// Pixels a popup, tooltip, dialog or bar travels while it fades in.
    pub travel: f32,
    /// The scale a menu and a dialog start from.
    pub menu_scale: f32,
    pub dialog_scale: f32,
    /// The accent line, the selected row and a bar's on state glide from the old item to
    /// the new one, instead of fading out there and in here.
    pub glide: bool,
    /// The Command Palette grows out of the command field, and a flyout slides out from
    /// under its icon strip, instead of fading in near them.
    pub morph: bool,
}

pub const PROFILES: [Profile; 3] = [
    Profile {
        name: "A · Still",
        about: "As the component recipes left it: hover fills fade and the switch slides. Everything else happens at once.",
        hover: tr(90, 90, Ease::Linear),
        press: Tr::NONE,
        check: Tr::NONE,
        switch: tr(140, 140, Ease::Out),
        tab: Tr::NONE,
        menu: Tr::NONE,
        tooltip: Tr::NONE,
        flyout: Tr::NONE,
        dock: Tr::NONE,
        section: Tr::NONE,
        dialog: Tr::NONE,
        palette: Tr::NONE,
        select: Tr::NONE,
        bar: Tr::NONE,
        progress: Tr::NONE,
        tooltip_delay: 0.5,
        travel: 0.0,
        menu_scale: 1.0,
        dialog_scale: 1.0,
        glide: false,
        morph: false,
    },
    Profile {
        name: "B · Crisp",
        about: "Short and eased, within the three durations. Things arrive with a fade and a few pixels of travel and leave faster than they came; selection and hover-in stay immediate.",
        hover: tr(0, 90, Ease::Linear),
        press: tr(0, 90, Ease::Out),
        check: tr(90, 90, Ease::Out),
        switch: tr(140, 140, Ease::Out),
        tab: tr(90, 0, Ease::Out),
        menu: tr(90, 0, Ease::Out),
        tooltip: tr(90, 0, Ease::Out),
        flyout: tr(140, 90, Ease::Out),
        dock: tr(140, 140, Ease::Out),
        section: tr(140, 90, Ease::Out),
        dialog: tr(140, 90, Ease::Out),
        palette: tr(140, 90, Ease::Out),
        select: Tr::NONE,
        bar: tr(90, 90, Ease::Out),
        progress: tr(140, 140, Ease::Out),
        tooltip_delay: 0.5,
        travel: 4.0,
        menu_scale: 1.0,
        dialog_scale: 0.97,
        glide: false,
        morph: false,
    },
    Profile {
        name: "C · Fluid",
        about: "Longer and spatial. Things grow from where they came from, with a slight spring; the accent line, the selected row and the on state glide between items.",
        hover: tr(60, 140, Ease::Linear),
        press: tr(0, 140, Ease::Out),
        check: tr(200, 90, Ease::Spring),
        switch: tr(260, 260, Ease::Spring),
        tab: tr(200, 200, Ease::InOut),
        menu: tr(200, 90, Ease::Spring),
        tooltip: tr(140, 90, Ease::Out),
        flyout: tr(200, 140, Ease::Out),
        dock: tr(260, 260, Ease::InOut),
        section: tr(200, 140, Ease::InOut),
        dialog: tr(320, 140, Ease::Spring),
        palette: tr(200, 140, Ease::Out),
        select: tr(140, 140, Ease::Out),
        bar: tr(260, 140, Ease::Spring),
        progress: tr(320, 320, Ease::InOut),
        tooltip_delay: 0.35,
        travel: 8.0,
        menu_scale: 0.94,
        dialog_scale: 0.9,
        glide: true,
        morph: true,
    },
];

static PROFILE: AtomicU8 = AtomicU8::new(1);
static ANIMATE: AtomicU8 = AtomicU8::new(1);
static SLOW: AtomicU8 = AtomicU8::new(0);
/// 0 nothing pending, 1 play to the "after" state, 2 play back.
static PLAY: AtomicU8 = AtomicU8::new(0);
static PLAYED: AtomicU8 = AtomicU8::new(0);

pub const SPEEDS: [(&str, f32); 3] = [("1×", 1.0), ("1/4×", 4.0), ("1/10×", 10.0)];

pub fn profile_index() -> usize {
    PROFILE.load(Ordering::Relaxed) as usize % PROFILES.len()
}

pub fn set_profile(i: usize) {
    PROFILE.store((i % PROFILES.len()) as u8, Ordering::Relaxed);
}

pub fn animate() -> bool {
    ANIMATE.load(Ordering::Relaxed) == 1
}

pub fn set_animate(on: bool) {
    ANIMATE.store(u8::from(on), Ordering::Relaxed);
}

pub fn speed_index() -> usize {
    SLOW.load(Ordering::Relaxed) as usize % SPEEDS.len()
}

pub fn set_speed(i: usize) {
    SLOW.store((i % SPEEDS.len()) as u8, Ordering::Relaxed);
}

/// Flips every demo on the Motion page at once.
pub fn play() {
    let on = PLAYED.load(Ordering::Relaxed) == 0;
    PLAYED.store(u8::from(on), Ordering::Relaxed);
    PLAY.store(if on { 1 } else { 2 }, Ordering::Relaxed);
}

pub fn take_play() -> Option<bool> {
    match PLAY.swap(0, Ordering::Relaxed) {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// The current profile. With Animate off every transition is `Tr::NONE`; the tooltip
/// delay stays, because it is a wait, not a movement.
pub fn profile() -> Profile {
    let p = PROFILES[profile_index()];
    if animate() {
        return p;
    }
    Profile {
        hover: Tr::NONE,
        press: Tr::NONE,
        check: Tr::NONE,
        switch: Tr::NONE,
        tab: Tr::NONE,
        menu: Tr::NONE,
        tooltip: Tr::NONE,
        flyout: Tr::NONE,
        dock: Tr::NONE,
        section: Tr::NONE,
        dialog: Tr::NONE,
        palette: Tr::NONE,
        select: Tr::NONE,
        bar: Tr::NONE,
        progress: Tr::NONE,
        travel: 0.0,
        menu_scale: 1.0,
        dialog_scale: 1.0,
        ..p
    }
}

#[derive(Clone, Copy)]
struct Run {
    from: f32,
    to: f32,
    start: f64,
    dur: f32,
    ease: Ease,
}

impl Run {
    fn at(&self, now: f64) -> f32 {
        let x = if self.dur <= 0.0 {
            1.0
        } else {
            ((now - self.start) as f32 / self.dur).clamp(0.0, 1.0)
        };
        if x >= 1.0 {
            self.to
        } else {
            self.from + (self.to - self.from) * self.ease.at(x)
        }
    }
}

fn run(ctx: &Context, id: Id, target: f32, pick: impl FnOnce(f32) -> (f32, Ease)) -> f32 {
    let now = ctx.input(|i| i.time);
    let mut run = ctx.data(|d| d.get_temp::<Run>(id)).unwrap_or(Run {
        from: target,
        to: target,
        start: now,
        dur: 0.0,
        ease: Ease::Linear,
    });
    if run.to != target {
        let from = run.at(now);
        let (dur, ease) = pick(from);
        run = Run {
            from,
            to: target,
            start: now,
            dur: dur * SPEEDS[speed_index()].1,
            ease,
        };
    }
    ctx.data_mut(|d| d.insert_temp(id, run));
    if run.dur > 0.0 && ((now - run.start) as f32) < run.dur {
        ctx.request_repaint();
    }
    run.at(now)
}

/// The presence of something that is `on` or not: 0 when gone, 1 when there (a spring
/// passes 1 on the way). Enters with `tr.enter`, leaves with `tr.exit`.
pub fn value(ctx: &Context, id: Id, on: bool, tr: Tr) -> f32 {
    let target = f32::from(u8::from(on));
    run(ctx, id, target, |_| {
        if on {
            (tr.enter, tr.ease)
        } else if tr.ease == Ease::Spring {
            (tr.exit, Ease::Out)
        } else {
            (tr.exit, tr.ease)
        }
    })
}

/// A position or amount that travels to `target` with `tr.enter`.
pub fn glide(ctx: &Context, id: Id, target: f32, tr: Tr) -> f32 {
    run(ctx, id, target, |_| (tr.enter, tr.ease))
}
