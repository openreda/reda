//! Dragging a tab: while the drag lasts, drop-target buttons over the group under the
//! pointer and at the window edges, a live preview of where the tab would go, and the
//! reason when the layout would refuse it; on release, the one layout Command the drop
//! runs.
//!
//! The drag never changes the layout itself. The model plans each drop
//! ([`Layout::plan_drop`]), so the preview, the refusal and the Command all come from the
//! same dry run.

use std::collections::HashMap;

use egui::{
    Color32, Context, CursorIcon, Id, LayerId, Order, Pos2, Rect, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, ViewportCommand, ViewportId, WidgetInfo, vec2,
};
use serde_json::Value;

use super::windows::{self, BlindDrop, Placements, Release, WindowView};
use crate::layout::{
    DockSide, DropOutcome, DropTarget, GroupId, Layout, SplitSide, TabKey, WindowId,
};
use crate::tr;

/// The size of a drop-target button.
const BUTTON: f32 = 28.0;
/// The distance between the centres of neighbouring drop-target buttons.
const STEP: f32 = 32.0;
/// How far a Dock-edge button sits in from its window edge.
const EDGE: f32 = 22.0;
/// The size of a Dock preview where the Dock is not shown yet.
const DOCK_SIZE: f32 = 240.0;

/// Where a new window's top-left corner goes from the pointer that dropped its tab, so
/// the pointer lands on its tab strip.
const GRAB: Vec2 = vec2(40.0, 12.0);

/// A tab being dragged.
#[derive(Clone)]
struct Dragging {
    tab: TabKey,
    /// The viewport (OS window) the drag started in.
    source: ViewportId,
    /// The last pointer position seen there, in its coordinates and possibly outside it:
    /// the release frame may not report one.
    pos: Option<Pos2>,
}

/// The drag state of every window, kept between frames in egui's memory, which all
/// viewports share.
#[derive(Clone, Default)]
struct State {
    dragging: Option<Dragging>,
    blind: Option<BlindDrop>,
    /// Each window as last drawn.
    views: HashMap<ViewportId, WindowView>,
    placements: Placements,
}

fn state_id() -> Id {
    Id::NULL.with("dock-drag")
}

/// Starts dragging `tab` in the current viewport.
pub(super) fn start(ctx: &Context, tab: &TabKey) {
    let dragging = Dragging {
        tab: tab.clone(),
        source: ctx.viewport_id(),
        pos: ctx.pointer_latest_pos(),
    };
    ctx.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<State>(state_id());
        state.dragging = Some(dragging);
        state.blind = None;
    });
}

/// A tab group as drawn this frame: where drops on it land.
#[derive(Clone, Debug)]
pub struct GroupZone {
    pub group: GroupId,
    pub rect: Rect,
    /// The tab strip: dropping there puts the tab among the group's tabs.
    pub strip: Rect,
    /// Each tab's rect in the strip, in order.
    pub tabs: Vec<Rect>,
}

/// What this frame drew that a tab can be dropped on.
#[derive(Clone, Debug)]
pub struct Zones {
    /// The whole docking area, inside which the Dock-edge buttons sit.
    pub area: Rect,
    /// In drawing order, so the last one under the pointer is on top.
    pub groups: Vec<GroupZone>,
    /// Each Dock's rect, where it is shown.
    pub docks: Vec<(DockSide, Rect)>,
    /// Whether this window has Dock edges to drop on: only the main window does.
    pub dock_edges: bool,
}

/// A drop-target button.
pub(super) struct Button {
    target: DropTarget,
    rect: Rect,
    label: String,
}

impl Zones {
    /// No zones yet, in a floating window whose docking area is `area`.
    pub fn empty(area: Rect) -> Zones {
        Zones {
            area,
            groups: Vec::new(),
            docks: Vec::new(),
            dock_edges: false,
        }
    }

    /// The topmost group under `p`.
    pub(super) fn group_at(&self, p: Pos2) -> Option<&GroupZone> {
        self.groups.iter().rev().find(|g| g.rect.contains(p))
    }

    /// The buttons shown with the pointer at `p`: the five of the group under the pointer,
    /// and the three Dock edges, less any edge button the group's buttons cover.
    pub(super) fn buttons(&self, p: Pos2) -> Vec<Button> {
        let mut out = Vec::new();
        if let Some(g) = self.group_at(p) {
            let body = Rect::from_min_max(Pos2::new(g.rect.left(), g.strip.bottom()), g.rect.max);
            let c = body.center();
            out.push(Button {
                target: DropTarget::Group {
                    group: g.group,
                    slot: None,
                },
                rect: Rect::from_center_size(c, Vec2::splat(BUTTON)),
                label: tr!("drop-into-group"),
            });
            for (side, offset, label) in [
                (SplitSide::Left, vec2(-STEP, 0.0), tr!("drop-split-left")),
                (SplitSide::Right, vec2(STEP, 0.0), tr!("drop-split-right")),
                (SplitSide::Up, vec2(0.0, -STEP), tr!("drop-split-up")),
                (SplitSide::Down, vec2(0.0, STEP), tr!("drop-split-down")),
            ] {
                out.push(Button {
                    target: DropTarget::Split {
                        group: g.group,
                        side,
                    },
                    rect: Rect::from_center_size(c + offset, Vec2::splat(BUTTON)),
                    label,
                });
            }
        }
        let a = self.area;
        let edges = if self.dock_edges {
            [
                (
                    DockSide::Left,
                    Pos2::new(a.left() + EDGE, a.center().y),
                    tr!("drop-dock-left"),
                ),
                (
                    DockSide::Right,
                    Pos2::new(a.right() - EDGE, a.center().y),
                    tr!("drop-dock-right"),
                ),
                (
                    DockSide::Bottom,
                    Pos2::new(a.center().x, a.bottom() - EDGE),
                    tr!("drop-dock-bottom"),
                ),
            ]
            .to_vec()
        } else {
            Vec::new()
        };
        for (side, centre, label) in edges {
            let rect = Rect::from_center_size(centre, Vec2::splat(BUTTON));
            if !out.iter().any(|b| b.rect.intersects(rect)) {
                out.push(Button {
                    target: DropTarget::Dock(side),
                    rect,
                    label,
                });
            }
        }
        out
    }

    /// Where a drop at `p` lands: a button under it, else the tab strip under it.
    pub(super) fn target_at(&self, p: Pos2, buttons: &[Button]) -> Option<DropTarget> {
        if let Some(b) = buttons.iter().find(|b| b.rect.contains(p)) {
            return Some(b.target);
        }
        let g = self.group_at(p)?;
        g.strip.contains(p).then(|| DropTarget::Group {
            group: g.group,
            slot: Some(g.tabs.iter().filter(|t| t.center().x < p.x).count()),
        })
    }

    /// The part of the window a drop on `target` would fill.
    fn preview(&self, target: DropTarget) -> Option<Rect> {
        let a = self.area;
        match target {
            DropTarget::Group { group, .. } => self.zone(group).map(|g| g.rect),
            DropTarget::Split { group, side } => self.zone(group).map(|g| {
                let r = g.rect;
                match side {
                    SplitSide::Left => r.with_max_x(r.center().x),
                    SplitSide::Right => r.with_min_x(r.center().x),
                    SplitSide::Up => r.with_max_y(r.center().y),
                    SplitSide::Down => r.with_min_y(r.center().y),
                }
            }),
            DropTarget::Dock(side) => Some(
                self.docks
                    .iter()
                    .find(|(s, _)| *s == side)
                    .map(|(_, r)| *r)
                    .unwrap_or(match side {
                        DockSide::Left => a.with_max_x(a.left() + DOCK_SIZE),
                        DockSide::Right => a.with_min_x(a.right() - DOCK_SIZE),
                        DockSide::Bottom => a.with_min_y(a.bottom() - DOCK_SIZE * 2.0 / 3.0),
                    }),
            ),
        }
    }

    fn zone(&self, group: GroupId) -> Option<&GroupZone> {
        self.groups.iter().find(|g| g.group == group)
    }
}

/// Draws the drag in progress in the window just drawn, `view`, over everything else, and
/// returns the layout Commands to run: the drop's, on release in this window, or a blind
/// drop's that this window takes or that times out.
///
/// The window the drag started in tracks the pointer and handles the release. Every other
/// window works out from the screen positions whether the pointer is over it, and then
/// shows the drop buttons and preview; without screen positions (Wayland) none can.
pub(super) fn show(
    ctx: &Context,
    layout: &Layout,
    view: WindowView,
    title: impl Fn(&TabKey) -> String,
) -> Vec<(String, Value)> {
    let here = ctx.viewport_id();
    let mut state = ctx.data(|d| d.get_temp::<State>(state_id()).unwrap_or_default());
    state.views.insert(here, view.clone());
    let mut commands = Vec::new();
    let now = ctx.input(|i| i.time);

    if let Some(blind) = state.blind.clone() {
        // Keep checking until a window sees the pointer or the wait is over.
        ctx.request_repaint_of(ViewportId::ROOT);
        let seen = ctx.input(|i| {
            i.events.iter().rev().find_map(|e| match e {
                egui::Event::PointerMoved(p) => Some(*p),
                _ => None,
            })
        });
        let release = match seen.and_then(|p| blind.claim(layout, &view, p)) {
            Some(release) => Some(release),
            None if blind.expired(now) => Some(blind.expire(layout)),
            None => None,
        };
        if let Some(release) = release {
            state.blind = None;
            act(ctx, &mut state, &blind.tab, release, &mut commands);
        }
    }

    if let Some(mut dragging) = state.dragging.clone() {
        // Other OS windows draw only when the main one does, and must follow the pointer.
        if !ctx.embed_viewports() {
            ctx.request_repaint_of(ViewportId::ROOT);
        }
        if dragging.source == here {
            if let Some(p) = ctx.pointer_latest_pos() {
                dragging.pos = Some(p);
            }
            state.dragging = Some(dragging.clone());
            if !ctx.input(|i| i.pointer.primary_down()) {
                state.dragging = None;
                if let Some(p) = dragging.pos {
                    let views: Vec<WindowView> = state.views.values().cloned().collect();
                    match windows::release(layout, &dragging.tab, &view, p, &views) {
                        Release::Blind => {
                            state.blind = Some(BlindDrop {
                                tab: dragging.tab.clone(),
                                since: now,
                            });
                        }
                        release => act(ctx, &mut state, &dragging.tab, release, &mut commands),
                    }
                }
            } else if let Some(p) = dragging.pos
                && (view.rect.contains(p) || !shown_elsewhere(&state, here, &view, p))
            {
                overlay(ctx, layout, &view.zones, &dragging.tab, p, &title);
            }
        } else if let Some(source) = state.views.get(&dragging.source)
            && let (Some(from), Some(to), Some(p)) = (source.origin, view.origin, dragging.pos)
        {
            let p = p + (from - to);
            if view.rect.contains(p) {
                overlay(ctx, layout, &view.zones, &dragging.tab, p, &title);
            }
        }
    }
    ctx.data_mut(|d| d.insert_temp(state_id(), state));
    commands
}

/// Whether another window shows the drag, with the pointer at `p` in `view`'s
/// coordinates.
fn shown_elsewhere(state: &State, here: ViewportId, view: &WindowView, p: Pos2) -> bool {
    let Some(origin) = view.origin else {
        return false;
    };
    state
        .views
        .iter()
        .any(|(id, w)| *id != here && w.origin.is_some_and(|o| w.rect.contains(p + (origin - o))))
}

/// Forgets the windows not drawn this frame, and a drag whose window has closed.
pub(super) fn retain(ctx: &Context, live: &[ViewportId]) {
    ctx.data_mut(|d| {
        let state = d.get_temp_mut_or_default::<State>(state_id());
        state.views.retain(|id, _| live.contains(id));
        if state
            .dragging
            .as_ref()
            .is_some_and(|g| !live.contains(&g.source))
        {
            state.dragging = None;
        }
    });
}

/// Where the new window `window` should open, if the drop that made it asked for a place.
pub(super) fn placement(ctx: &Context, layout: &Layout, window: WindowId) -> Option<Pos2> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<State>(state_id())
            .placements
            .take(layout, window)
    })
}

/// The viewport (OS window) of the floating window `window`.
pub(super) fn viewport_id(window: WindowId) -> ViewportId {
    ViewportId::from_hash_of(("reda-window", window.0))
}

/// Carries out a release: queues its Command, or moves the window.
fn act(
    ctx: &Context,
    state: &mut State,
    tab: &TabKey,
    release: Release,
    commands: &mut Vec<(String, Value)>,
) {
    match release {
        Release::Run(drop) => {
            if let Some(at) = drop.new_window_at {
                state.placements.insert(tab.clone(), at - GRAB);
            }
            commands.push((drop.command.to_owned(), drop.params));
        }
        Release::MoveWindow { window, to } => ctx.send_viewport_cmd_to(
            viewport_id(window),
            ViewportCommand::OuterPosition(to - GRAB),
        ),
        Release::Blind | Release::Nothing => {}
    }
}

/// Draws, with the pointer at `p`, the drop buttons, a preview of the drop there (red with
/// the reason when the layout would refuse it) and the dragged tab's title.
fn overlay(
    ctx: &Context,
    layout: &Layout,
    zones: &Zones,
    tab: &TabKey,
    p: Pos2,
    title: &impl Fn(&TabKey) -> String,
) {
    ctx.set_cursor_icon(CursorIcon::Grabbing);
    let buttons = zones.buttons(p);
    let plan = zones
        .target_at(p, &buttons)
        .map(|t| (t, layout.plan_drop(tab, t)));

    let layer = LayerId::new(Order::Foreground, Id::NULL.with("dock-drop-overlay"));
    let mut ui = Ui::new(
        ctx.clone(),
        Id::NULL.with("dock-drop-overlay"),
        UiBuilder::new().layer_id(layer).max_rect(zones.area),
    );
    let accent = ui.visuals().selection.bg_fill;
    let refused = ui.visuals().error_fg_color;

    // The preview first, so the buttons stay readable over it.
    let hovered = plan.as_ref().map(|(t, _)| *t);
    if let Some((target, plan)) = &plan
        && let Some(rect) = zones.preview(*target)
    {
        let colour = match plan.outcome {
            DropOutcome::Refused(_) => refused,
            _ => accent,
        };
        if plan.outcome != DropOutcome::Unchanged {
            ui.painter().rect(
                rect.shrink(2.0),
                4.0,
                colour.gamma_multiply(0.2),
                Stroke::new(2.0, colour),
                StrokeKind::Inside,
            );
        }
    }
    for (i, b) in buttons.iter().enumerate() {
        draw_button(&mut ui, i, b, hovered == Some(b.target), accent);
    }
    if let Some((_, plan)) = &plan
        && let DropOutcome::Refused(reason) = &plan.outcome
    {
        let text = tr!("drop-refused", reason = reason.as_str());
        let at = Rect::from_min_size(p + vec2(14.0, 18.0), vec2(320.0, 80.0));
        ui.scope_builder(UiBuilder::new().max_rect(at), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(text).color(refused));
            });
        });
    }
    // The tab's title follows the pointer.
    let galley = ui.painter().layout_no_wrap(
        title(tab),
        egui::TextStyle::Button.resolve(ui.style()),
        ui.visuals().strong_text_color(),
    );
    let chip = Rect::from_min_size(p + vec2(12.0, -24.0), galley.size() + vec2(12.0, 6.0));
    ui.painter().rect(
        chip,
        3.0,
        ui.visuals().window_fill,
        Stroke::new(1.0, accent),
        StrokeKind::Inside,
    );
    ui.painter()
        .galley(chip.min + vec2(6.0, 3.0), galley, Color32::PLACEHOLDER);
}

/// A drop-target button: a square with a mark for where the tab would go. It has an
/// accessible name, so tests and screen readers can find it.
fn draw_button(ui: &mut Ui, i: usize, b: &Button, on: bool, accent: Color32) {
    let response = ui.interact(
        b.rect,
        Id::NULL.with(("dock-drop-target", i)),
        Sense::hover(),
    );
    let label = b.label.clone();
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    let fill = ui.visuals().window_fill;
    let painter = ui.painter();
    painter.rect(
        b.rect,
        4.0,
        if on { accent } else { fill },
        Stroke::new(1.0, accent),
        StrokeKind::Inside,
    );
    let inner = b.rect.shrink(6.0);
    let mark = match b.target {
        DropTarget::Split {
            side: SplitSide::Left,
            ..
        }
        | DropTarget::Dock(DockSide::Left) => inner.with_max_x(inner.center().x),
        DropTarget::Split {
            side: SplitSide::Right,
            ..
        }
        | DropTarget::Dock(DockSide::Right) => inner.with_min_x(inner.center().x),
        DropTarget::Split {
            side: SplitSide::Up,
            ..
        } => inner.with_max_y(inner.center().y),
        DropTarget::Split {
            side: SplitSide::Down,
            ..
        }
        | DropTarget::Dock(DockSide::Bottom) => inner.with_min_y(inner.center().y),
        DropTarget::Group { .. } => inner.shrink(3.0),
    };
    painter.rect_filled(
        mark,
        2.0,
        if on { fill } else { accent.gamma_multiply(0.6) },
    );
}
