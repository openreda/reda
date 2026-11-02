//! Dragging a tab: while the drag lasts, drop-target buttons over the group under the
//! pointer and at the window edges, a live preview of where the tab would go, and the
//! reason when the layout would refuse it; on release, the one layout Command the drop
//! runs.
//!
//! The drag never changes the layout itself. The model plans each drop
//! ([`Layout::plan_drop`]), so the preview, the refusal and the Command all come from the
//! same dry run.

use egui::{
    Color32, Context, CursorIcon, Id, LayerId, Order, Pos2, Rect, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, vec2,
};
use serde_json::Value;

use crate::layout::{DockSide, DropOutcome, DropTarget, GroupId, Layout, SplitSide, TabKey};
use crate::tr;

/// The size of a drop-target button.
const BUTTON: f32 = 28.0;
/// The distance between the centres of neighbouring drop-target buttons.
const STEP: f32 = 32.0;
/// How far a Dock-edge button sits in from its window edge.
const EDGE: f32 = 22.0;
/// The size of a Dock preview where the Dock is not shown yet.
const DOCK_SIZE: f32 = 240.0;

/// A tab being dragged, kept in egui's memory between frames.
#[derive(Clone)]
struct Dragging {
    tab: TabKey,
    /// The last pointer position seen: the release frame may not report one.
    pos: Option<Pos2>,
}

fn state_id() -> Id {
    Id::NULL.with("dock-drag")
}

/// Starts dragging `tab`.
pub(super) fn start(ctx: &Context, tab: &TabKey) {
    let dragging = Dragging {
        tab: tab.clone(),
        pos: ctx.pointer_latest_pos(),
    };
    ctx.data_mut(|d| d.insert_temp(state_id(), dragging));
}

/// A tab group as drawn this frame: where drops on it land.
pub(super) struct GroupZone {
    pub group: GroupId,
    pub rect: Rect,
    /// The tab strip: dropping there puts the tab among the group's tabs.
    pub strip: Rect,
    /// Each tab's rect in the strip, in order.
    pub tabs: Vec<Rect>,
}

/// What this frame drew that a tab can be dropped on.
pub(super) struct Zones {
    /// The whole docking area, inside which the Dock-edge buttons sit.
    pub area: Rect,
    /// In drawing order, so the last one under the pointer is on top.
    pub groups: Vec<GroupZone>,
    /// Each Dock's rect, where it is shown.
    pub docks: Vec<(DockSide, Rect)>,
}

/// A drop-target button.
struct Button {
    target: DropTarget,
    rect: Rect,
    label: String,
}

impl Zones {
    /// The topmost group under `p`.
    fn group_at(&self, p: Pos2) -> Option<&GroupZone> {
        self.groups.iter().rev().find(|g| g.rect.contains(p))
    }

    /// The buttons shown with the pointer at `p`: the five of the group under the pointer,
    /// and the three Dock edges, less any edge button the group's buttons cover.
    fn buttons(&self, p: Pos2) -> Vec<Button> {
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
        for (side, centre, label) in [
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
        ] {
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
    fn target_at(&self, p: Pos2, buttons: &[Button]) -> Option<DropTarget> {
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

/// Draws the drag in progress, if any, over everything else, and on release returns the
/// layout Command the drop runs (`None` when it lands nowhere, changes nothing or would be
/// refused).
pub(super) fn show(
    ctx: &Context,
    layout: &Layout,
    zones: &Zones,
    title: impl FnOnce(&TabKey) -> String,
) -> Option<(&'static str, Value)> {
    let mut dragging = ctx.data(|d| d.get_temp::<Dragging>(state_id()))?;
    if let Some(p) = ctx.pointer_latest_pos() {
        dragging.pos = Some(p);
    }
    let p = dragging.pos?;
    let buttons = zones.buttons(p);
    let plan = zones
        .target_at(p, &buttons)
        .map(|t| (t, layout.plan_drop(&dragging.tab, t)));

    if !ctx.input(|i| i.pointer.primary_down()) {
        ctx.data_mut(|d| d.remove::<Dragging>(state_id()));
        return plan.and_then(|(_, plan)| {
            (plan.outcome == DropOutcome::Moves).then_some((plan.command, plan.params))
        });
    }
    ctx.data_mut(|d| d.insert_temp(state_id(), dragging.clone()));
    ctx.set_cursor_icon(CursorIcon::Grabbing);

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
        title(&dragging.tab),
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
    None
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
