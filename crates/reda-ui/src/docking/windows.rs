//! Dragging tabs between native OS windows, with no drawing: where a release lands when it
//! may be in another window, and the Wayland blind drop.
//!
//! On X11, Windows and macOS every window knows where it is on screen, so a release is
//! hit-tested by screen position against every window ([`release`]). Wayland tells no
//! window where it is, so a release outside the source window waits as a [`BlindDrop`]
//! for the first REDA window that sees the pointer; after [`BLIND_TIMEOUT`] the tab floats
//! in a new window instead.

use egui::{Pos2, Rect};
use serde_json::{Value, json};

pub use super::drag::{GroupZone, Zones};
use crate::layout::{Area, DropOutcome, DropTarget, GroupId, Layout, TabKey, WindowId};

/// How long, in seconds, a blind drop waits for a window to see the pointer.
pub const BLIND_TIMEOUT: f64 = 0.7;

/// One of REDA's windows as last drawn.
#[derive(Clone, Debug)]
pub struct WindowView {
    /// The floating window it shows, or `None` for the main window.
    pub window: Option<WindowId>,
    /// Where its content's top-left corner is on screen, in points; `None` where the
    /// windowing system does not say (Wayland).
    pub origin: Option<Pos2>,
    /// Its content, in its own coordinates.
    pub rect: Rect,
    /// Where a tab can be dropped in it.
    pub zones: Zones,
    /// The group a drop anywhere else in it lands in.
    pub first_group: Option<GroupId>,
}

impl WindowView {
    /// `p`, a point in `other`'s coordinates, in this window's coordinates, where both
    /// know where they are on screen.
    fn local(&self, other: &WindowView, p: Pos2) -> Option<Pos2> {
        Some(p + (other.origin? - self.origin?))
    }
}

/// A drop that runs a layout Command.
#[derive(Clone, Debug, PartialEq)]
pub struct Drop {
    pub command: &'static str,
    pub params: Value,
    /// Where on screen a new window for the tab should open, if it gets one and the
    /// windowing system lets REDA place it.
    pub new_window_at: Option<Pos2>,
}

/// What releasing a dragged tab does.
#[derive(Clone, Debug, PartialEq)]
pub enum Release {
    /// Runs a layout Command.
    Run(Drop),
    /// The tab is all its window holds, so the window itself moves to this screen
    /// position.
    MoveWindow { window: WindowId, to: Pos2 },
    /// Released outside the source window with no screen positions: wait for a window to
    /// see the pointer ([`BlindDrop`]).
    Blind,
    /// Nothing changes.
    Nothing,
}

/// Where releasing `tab`, dragged from the window `source`, at `at` (in the source's
/// coordinates) lands. `windows` holds every window as last drawn, the source among them.
pub fn release(
    layout: &Layout,
    tab: &TabKey,
    source: &WindowView,
    at: Pos2,
    windows: &[WindowView],
) -> Release {
    if source.rect.contains(at) {
        return drop_in(layout, tab, source, at);
    }
    let Some(origin) = source.origin else {
        return Release::Blind;
    };
    let other = windows
        .iter()
        .filter(|w| w.window != source.window)
        .find_map(|w| {
            w.local(source, at)
                .filter(|p| w.rect.contains(*p))
                .map(|p| (w, p))
        });
    if let Some((window, p)) = other {
        return drop_in(layout, tab, window, p);
    }
    let to = origin + at.to_vec2();
    if let Some(window) = alone_in_window(layout, tab) {
        return Release::MoveWindow { window, to };
    }
    Release::Run(Drop {
        command: "layout.tab.move_to_new_window",
        params: json!({"tab": tab.to_string()}),
        new_window_at: Some(to),
    })
}

/// A drop at `at` in `window`: on the drop-target button or tab strip there, if any.
fn drop_in(layout: &Layout, tab: &TabKey, window: &WindowView, at: Pos2) -> Release {
    let buttons = window.zones.buttons(at);
    let target = window.zones.target_at(at, &buttons);
    run_plan(layout, tab, target, None)
}

fn run_plan(
    layout: &Layout,
    tab: &TabKey,
    target: Option<DropTarget>,
    at: Option<Pos2>,
) -> Release {
    let Some(target) = target else {
        return Release::Nothing;
    };
    let plan = layout.plan_drop(tab, target);
    match plan.outcome {
        DropOutcome::Moves => Release::Run(Drop {
            command: plan.command,
            params: plan.params,
            new_window_at: at,
        }),
        DropOutcome::Unchanged | DropOutcome::Refused(_) => Release::Nothing,
    }
}

/// The floating window `tab` is alone in, if it is.
fn alone_in_window(layout: &Layout, tab: &TabKey) -> Option<WindowId> {
    let place = layout.place(tab)?;
    let Area::Window(window) = place.area else {
        return None;
    };
    let tabs = layout
        .groups()
        .into_iter()
        .filter(|(area, _)| *area == place.area)
        .map(|(_, g)| g.tabs.len())
        .sum::<usize>();
    (tabs == 1).then_some(window)
}

/// A tab released outside its window on Wayland, waiting for a window to see the pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct BlindDrop {
    pub tab: TabKey,
    /// When it was released, in egui's input time (seconds).
    pub since: f64,
}

impl BlindDrop {
    /// `window` saw the pointer at `at` (its own coordinates). `None` if `at` is outside
    /// it, so the drop waits on; else the window takes the tab: at the drop target under
    /// the pointer (a tab strip, or a group), else in its first group.
    pub fn claim(&self, layout: &Layout, window: &WindowView, at: Pos2) -> Option<Release> {
        if !window.rect.contains(at) {
            return None;
        }
        let under = window.zones.group_at(at).map(|g| DropTarget::Group {
            group: g.group,
            slot: g
                .strip
                .contains(at)
                .then(|| g.tabs.iter().filter(|t| t.center().x < at.x).count()),
        });
        let target = under.or(window
            .first_group
            .map(|group| DropTarget::Group { group, slot: None }));
        Some(run_plan(layout, &self.tab, target, None))
    }

    /// Whether no window saw the pointer in time, at `now`.
    pub fn expired(&self, now: f64) -> bool {
        // A hair of slack, so 0.7 s after a release counts in spite of rounding.
        now - self.since >= BLIND_TIMEOUT - 1e-6
    }

    /// What the drop does when no window saw the pointer: the tab floats in a new window,
    /// unless it is all its window holds already.
    pub fn expire(&self, layout: &Layout) -> Release {
        if alone_in_window(layout, &self.tab).is_some() || layout.place(&self.tab).is_none() {
            return Release::Nothing;
        }
        Release::Run(Drop {
            command: "layout.tab.move_to_new_window",
            params: json!({"tab": self.tab.to_string()}),
            new_window_at: None,
        })
    }
}

/// Where on screen new windows should open, remembered from the drop that made them
/// until the window first shows.
#[derive(Clone, Debug, Default)]
pub struct Placements(Vec<(TabKey, Pos2)>);

impl Placements {
    /// The next window to hold `tab` opens at `at`.
    pub fn insert(&mut self, tab: TabKey, at: Pos2) {
        self.0.retain(|(t, _)| *t != tab);
        self.0.push((tab, at));
    }

    /// The position for the new window `window`, if a drop asked for one for a tab it
    /// holds; it is forgotten once taken.
    pub fn take(&mut self, layout: &Layout, window: WindowId) -> Option<Pos2> {
        let i = self
            .0
            .iter()
            .position(|(tab, _)| layout.place(tab).map(|p| p.area) == Some(Area::Window(window)))?;
        Some(self.0.remove(i).1)
    }
}
