// PROTOTYPE — throwaway, lives only on the `prototype/icon-style` branch.
//
// Question: how should REDA's own full-colour icon set look? (openreda/reda#32)
//
// A copy of the own-docking-layer shell (`prototype-shell-own-dock`, variant A: Documents as
// tabs) with the menus, toolbar and Active Bar drawn with 12 SVG icons in two candidate styles:
//   1 · line art where colour carries meaning: grey for what exists, the domain colour for what
//       the Tool creates (green schematic, copper PCB, blue general), pink dots for picked points;
//   2 · filled shapes with soft depth: one top-to-bottom gradient per fill, a darker edge of the
//       same hue, a thin top highlight, no shadows or perspective.
// The yellow PROTOTYPE bar (or ←/→) switches style; it also sets the toolbar and Active Bar
// sizes (16/20/24 px; menus use 20) and opens an icon sheet showing every icon in every state,
// size and theme. State is on the button: accent background active, hover background, disabled
// drawn as a faded greyscale copy. Save is disabled until something changes; Move until a
// selection exists.
//
// Run: cargo run -p prototype-icons

#![allow(dead_code, deprecated)]

mod common;
mod icons;
mod model;

use common::*;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, LayerId, Order, Pos2, Rect, Sense, Stroke, StrokeKind, UiBuilder, ViewportId, pos2, vec2};
use model::*;
use std::collections::HashMap;

const BAR: f32 = 24.0;
const GAP: f32 = 3.0;
const STRIP: f32 = 24.0;

struct Drag {
    tab: Tab,
    source: ViewportId,
    /// Last pointer position seen in the source viewport (may lie outside it).
    last: Option<Pos2>,
}

/// Wayland: a drag released outside its window, waiting for a window to see the pointer.
struct Pending {
    tab: Tab,
    source: ViewportId,
    since: f64,
}

#[derive(Clone)]
struct Target {
    hit: Rect,
    dest: Dest,
    preview: Rect,
    /// Compass buttons are drawn only over the leaf (`owner`) under the pointer.
    owner: Option<Rect>,
    button: bool,
}

struct VpInfo {
    /// Screen position of the viewport's content (None on Wayland).
    origin: Option<Pos2>,
    rect: Rect,
    targets: Vec<Target>,
    win: Option<u64>,
}

struct App {
    shell: Shell,
    variant: Variant,
    m: Model,
    drag: Option<Drag>,
    pending: Option<Pending>,
    vps: HashMap<ViewportId, VpInfo>,
    building: Vec<Target>,
    queue: Vec<Action>,
    flyout: Option<(Tab, Side, Rect)>,
    focus_view: Option<usize>,
    show_state: bool,
    history: Vec<String>,
}

impl App {
    fn new() -> Self {
        let mut shell = Shell::new("own docking layer / egui 0.37");
        shell.layout_menu = false;
        let variant = Variant::Tabs;
        if let Some(i) = std::env::var("PROTO_STYLE").ok().and_then(|v| v.parse::<usize>().ok()) {
            shell.icon_style = icons::Style::ALL[i % 2];
        }
        if let Some(px) = std::env::var("PROTO_PX").ok().and_then(|v| v.parse::<f32>().ok()) {
            shell.toolbar_px = px;
            shell.bar_px = px;
        }
        let show_state = std::env::var("PROTO_SHEET").is_ok();
        let m = Model::new(variant, &[0, 1, 2]);
        Self { shell, variant, m, drag: None, pending: None, vps: HashMap::new(), building: vec![], queue: vec![], flyout: None, focus_view: None, show_state, history: vec![] }
    }

    fn set_variant(&mut self, v: Variant) {
        self.variant = v;
        self.m = Model::new(v, &[0, 1, 2]);
        self.flyout = None;
        self.focus_view = None;
        self.shell.log(format!("Variant → {}", v.label()));
    }

    fn name(&self, t: &Tab) -> String {
        self.shell.tab_title(t)
    }

    fn describe(&self, a: &Action) -> String {
        let n = |t: &Tab| format!("{:?}", self.name(t));
        let dest = |d: &Dest| match d {
            Dest::Node(id, Zone::Centre) => format!("join(#{id})"),
            Dest::Node(id, z) => format!("split(#{id}, {z:?})"),
            Dest::NewColumn(s) => format!("new_column({s:?})"),
            Dest::EmptyCentre => "centre".into(),
            Dest::NewWindow(Some(p)) => format!("new_window(at {:.0},{:.0})", p.x, p.y),
            Dest::NewWindow(None) => "new_window".into(),
            Dest::WindowOf(k) => format!("window(#{k})"),
        };
        let args = match a {
            Action::Move { tab, to } => format!("{{ tab: {}, to: {} }}", n(tab), dest(to)),
            Action::Close(t) | Action::Open(t) | Action::Home(t) => format!("{{ tab: {} }}", n(t)),
            Action::CloseWindow(k) => format!("{{ window: {k} }}"),
            Action::Collapse { side, col, collapsed } => format!("{{ dock: {side:?}, column: {col}, icons: {collapsed} }}"),
            Action::Show { view, doc } => format!("{{ view: #{view}, doc: {} }}", n(&Tab::Doc(*doc))),
            Action::CloseView(v) => format!("{{ view: #{v} }}"),
        };
        format!("{} {args}", a.command_id())
    }

    fn run_queue(&mut self) {
        for a in std::mem::take(&mut self.queue) {
            let text = self.describe(&a);
            match self.m.apply(&a, self.variant) {
                Ok(()) => {
                    self.shell.log(format!("▶ {text}"));
                    self.history.push(format!("✓ {text}"));
                }
                Err(e) => {
                    self.shell.log(format!("✗ {text} — rejected: {e}"));
                    self.history.push(format!("✗ {text} — {e}"));
                }
            }
        }
        if let Some(v) = self.focus_view
            && !matches!(self.m.nodes.get(v), Some(Some(Node::Canvas { .. })))
        {
            self.focus_view = None;
        }
    }

    /// Requests from the shared shell UI (Projects panel, Window menu, Panels button).
    fn shell_requests(&mut self) {
        for d in std::mem::take(&mut self.shell.open_requests) {
            self.shell.active_doc = Some(d);
            let tab = Tab::Doc(d);
            if self.variant == Variant::Canvas {
                let view = self.focus_view.or_else(|| self.m.canvases().first().copied());
                if let Some(view) = view {
                    self.queue.push(Action::Show { view, doc: d });
                }
            } else if let Some(g) = self.m.group_of(tab) {
                if let Node::Tabs { tabs, active } = self.m.node_mut(g) {
                    *active = tabs.iter().position(|t| *t == tab).unwrap();
                }
            } else {
                self.queue.push(Action::Open(tab));
            }
        }
        for p in std::mem::take(&mut self.shell.show_panel_requests) {
            if !self.m.contains(Tab::Panel(p)) {
                self.queue.push(Action::Open(Tab::Panel(p)));
            }
        }
    }

    // ------------------------------------------------------------ main window layout

    fn main_area(&mut self, ui: &mut egui::Ui, area: Rect) {
        let vp = ui.ctx().viewport_id();
        let (mut x0, mut x1) = (area.left(), area.right());
        let n_left = self.m.docks[0].len();
        for k in 0..n_left {
            let c = self.m.docks[0][k].clone();
            let w = if c.collapsed { STRIP } else { c.size };
            let r = Rect::from_min_max(pos2(x0, area.top()), pos2(x0 + w, area.bottom()));
            self.column(ui, vp, Side::Left, k, r);
            if !c.collapsed {
                let h = Rect::from_min_max(pos2(r.right(), r.top()), pos2(r.right() + GAP, r.bottom()));
                let d = splitter(ui, ("lcol", k), h, true);
                self.m.docks[0][k].size = (c.size + d).clamp(120.0, 600.0);
            }
            x0 += w + GAP;
        }
        for k in (0..self.m.docks[1].len()).rev() {
            let c = self.m.docks[1][k].clone();
            let w = if c.collapsed { STRIP } else { c.size };
            let r = Rect::from_min_max(pos2(x1 - w, area.top()), pos2(x1, area.bottom()));
            self.column(ui, vp, Side::Right, k, r);
            if !c.collapsed {
                let h = Rect::from_min_max(pos2(r.left() - GAP, r.top()), pos2(r.left(), r.bottom()));
                let d = splitter(ui, ("rcol", k), h, true);
                self.m.docks[1][k].size = (c.size - d).clamp(120.0, 600.0);
            }
            x1 -= w + GAP;
        }
        let mut bottom = area.bottom();
        if !self.m.docks[2].is_empty() {
            let cols = self.m.docks[2].clone();
            let expanded = cols.iter().filter(|c| !c.collapsed).count();
            let h = if expanded > 0 { self.m.bottom_height } else { STRIP };
            let row = Rect::from_min_max(pos2(x0, area.bottom() - h), pos2(x1, area.bottom()));
            let strip_w = |m: &Model, c: &Column| (m.leaves(c.root).len() as f32 * 90.0).max(60.0);
            let fixed: f32 = cols.iter().filter(|c| c.collapsed).map(|c| strip_w(&self.m, c) + GAP).sum();
            let each = ((row.width() - fixed) / expanded.max(1) as f32 - GAP).max(40.0);
            let mut x = row.left();
            for (k, c) in cols.iter().enumerate() {
                let w = if c.collapsed { strip_w(&self.m, c) } else { each };
                let r = if c.collapsed {
                    Rect::from_min_max(pos2(x, row.bottom() - STRIP), pos2(x + w, row.bottom()))
                } else {
                    Rect::from_min_max(pos2(x, row.top()), pos2(x + w, row.bottom()))
                };
                self.column(ui, vp, Side::Bottom, k, r);
                x += w + GAP;
            }
            if expanded > 0 {
                let hd = Rect::from_min_max(pos2(row.left(), row.top() - GAP), pos2(row.right(), row.top()));
                let d = splitter(ui, "bottom", hd, false);
                self.m.bottom_height = (self.m.bottom_height - d).clamp(60.0, 500.0);
            }
            bottom = row.top() - GAP;
        }
        let centre = Rect::from_min_max(pos2(x0, area.top()), pos2(x1, bottom));
        match self.m.centre {
            Some(root) => self.tree(ui, vp, root, centre, Region::Centre),
            None => {
                ui.painter().rect_filled(centre, 0.0, ui.visuals().extreme_bg_color);
                ui.painter().text(centre.center(), Align2::CENTER_CENTER, "Empty. Drop a tab here.", FontId::proportional(13.0), ui.visuals().weak_text_color());
                self.building.push(Target { hit: centre, dest: Dest::EmptyCentre, preview: centre, owner: None, button: false });
            }
        }
        // New dock columns at the window's outer edges.
        let b = 30.0;
        for (side, c) in [
            (Side::Left, pos2(area.left() + b / 2.0 + 4.0, area.center().y)),
            (Side::Right, pos2(area.right() - b / 2.0 - 4.0, area.center().y)),
            (Side::Bottom, pos2(area.center().x, area.bottom() - b / 2.0 - 60.0)),
        ] {
            let preview = match side {
                Side::Left => Rect::from_min_size(area.min, vec2(230.0, area.height())),
                Side::Right => Rect::from_min_max(pos2(area.right() - 230.0, area.top()), area.max),
                Side::Bottom => Rect::from_min_max(pos2(area.left(), area.bottom() - 150.0), area.max),
            };
            self.building.push(Target { hit: Rect::from_center_size(c, vec2(b, b)), dest: Dest::NewColumn(side), preview, owner: None, button: true });
        }
    }

    fn column(&mut self, ui: &mut egui::Ui, vp: ViewportId, side: Side, k: usize, r: Rect) {
        let c = self.m.docks[side.i()][k].clone();
        if c.collapsed {
            self.icon_strip(ui, side, k, &c, r);
            return;
        }
        // A thin column bar with the collapse button.
        let bar = Rect::from_min_size(r.min, vec2(r.width(), 14.0));
        ui.painter().rect_filled(bar, 0.0, ui.visuals().faint_bg_color);
        let btn = Rect::from_min_size(pos2(if side == Side::Right { bar.left() } else { bar.right() - 18.0 }, bar.top()), vec2(18.0, 14.0));
        let resp = ui.interact(btn, Id::new(("collapse", side, k)), Sense::click()).on_hover_text("Collapse to icons");
        let glyph = match side {
            Side::Left => "«",
            Side::Right => "»",
            Side::Bottom => "⌄",
        };
        ui.painter().text(btn.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(11.0), if resp.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() });
        if resp.clicked() {
            self.queue.push(Action::Collapse { side, col: k, collapsed: true });
        }
        let body = Rect::from_min_max(pos2(r.left(), bar.bottom()), r.max);
        self.tree(ui, vp, c.root, body, Region::Dock(side, k));
    }

    fn icon_strip(&mut self, ui: &mut egui::Ui, side: Side, k: usize, c: &Column, r: Rect) {
        ui.painter().rect_filled(r, 0.0, ui.visuals().faint_bg_color);
        let vertical = side != Side::Bottom;
        let tabs: Vec<Tab> = self.m.leaves(c.root).into_iter().flat_map(|l| if let Node::Tabs { tabs, .. } = self.m.node(l) { tabs.clone() } else { vec![] }).collect();
        let expand = if vertical { Rect::from_min_size(r.min, vec2(r.width(), 18.0)) } else { Rect::from_min_size(r.min, vec2(18.0, r.height())) };
        let resp = ui.interact(expand, Id::new(("expand", side, k)), Sense::click()).on_hover_text("Expand column");
        ui.painter().text(expand.center(), Align2::CENTER_CENTER, if vertical { "⇔" } else { "⌃" }, FontId::proportional(11.0), ui.visuals().text_color());
        if resp.clicked() {
            self.queue.push(Action::Collapse { side, col: k, collapsed: false });
        }
        let mut at = if vertical { expand.bottom() + 2.0 } else { expand.right() + 2.0 };
        for t in tabs {
            let title = self.name(&t);
            let open = self.flyout.is_some_and(|f| f.0 == t);
            let font = FontId::proportional(11.5);
            let color = if open { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
            let galley = ui.painter().layout_no_wrap(title, font, color);
            let len = galley.size().x + 14.0;
            let rr = if vertical { Rect::from_min_size(pos2(r.left(), at), vec2(r.width(), len)) } else { Rect::from_min_size(pos2(at, r.top()), vec2(len, r.height())) };
            at += len + 2.0;
            let resp = ui.interact(rr, Id::new(("icon", t)), Sense::click());
            let fill = if open || resp.hovered() { ui.visuals().widgets.hovered.bg_fill } else { ui.visuals().widgets.inactive.bg_fill };
            ui.painter().rect_filled(rr, CornerRadius::same(2), fill);
            if vertical {
                let pos = pos2(rr.center().x + galley.size().y / 2.0, rr.top() + 7.0);
                ui.painter().add(egui::epaint::TextShape::new(pos, galley, color).with_angle(std::f32::consts::FRAC_PI_2));
            } else {
                ui.painter().galley(rr.left_top() + vec2(7.0, (rr.height() - galley.size().y) / 2.0), galley, color);
            }
            if resp.clicked() {
                self.flyout = if open { None } else { Some((t, side, r)) };
            }
        }
    }

    fn flyout_ui(&mut self, ctx: &egui::Context, area: Rect) {
        let Some((tab, side, strip)) = self.flyout else { return };
        // Closed when its column was expanded or the Panel moved away.
        let still = self.m.docks[side.i()].iter().any(|c| c.collapsed && self.m.leaves(c.root).iter().any(|&l| matches!(self.m.node(l), Node::Tabs { tabs, .. } if tabs.contains(&tab))));
        if !still {
            self.flyout = None;
            return;
        }
        let r = match side {
            Side::Left => Rect::from_min_size(pos2(strip.right() + 2.0, area.top()), vec2(280.0, area.height() * 0.7)),
            Side::Right => Rect::from_min_size(pos2(strip.left() - 282.0, area.top()), vec2(280.0, area.height() * 0.7)),
            Side::Bottom => Rect::from_min_max(pos2(area.left() + 40.0, strip.top() - 220.0), pos2(area.right() - 40.0, strip.top() - 2.0)),
        };
        let resp = egui::Area::new(Id::new("dock_flyout")).fixed_pos(r.min).order(Order::Foreground).show(ctx, |ui| {
            egui::Frame::window(ui.style()).corner_radius(CornerRadius::same(3)).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                ui.set_min_size(r.size() - vec2(12.0, 12.0));
                ui.set_max_size(r.size() - vec2(12.0, 12.0));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(self.name(&tab).to_uppercase()).small().strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("📌").on_hover_text("Expand the column").clicked()
                            && let Some(k) = self.m.docks[side.i()].iter().position(|c| self.m.leaves(c.root).iter().any(|&l| matches!(self.m.node(l), Node::Tabs { tabs, .. } if tabs.contains(&tab))))
                        {
                            self.queue.push(Action::Collapse { side, col: k, collapsed: false });
                        }
                    });
                });
                ui.separator();
                match tab {
                    Tab::Panel(p) => self.shell.panel_ui(ui, p),
                    Tab::Doc(d) => self.shell.doc_ui(ui, d),
                }
            });
        });
        if resp.response.clicked_elsewhere() && !ctx.input(|i| i.pointer.interact_pos().is_some_and(|p| strip.contains(p))) {
            self.flyout = None;
        }
    }

    // ------------------------------------------------------------ trees and leaves

    fn tree(&mut self, ui: &mut egui::Ui, vp: ViewportId, id: NodeId, rect: Rect, region: Region) {
        match self.m.node(id).clone() {
            Node::Split { across, ratio, a, b } => {
                let (ra, rb, handle, len) = if across {
                    let wa = ((rect.width() - GAP) * ratio).round();
                    (
                        Rect::from_min_max(rect.min, pos2(rect.left() + wa, rect.bottom())),
                        Rect::from_min_max(pos2(rect.left() + wa + GAP, rect.top()), rect.max),
                        Rect::from_min_max(pos2(rect.left() + wa, rect.top()), pos2(rect.left() + wa + GAP, rect.bottom())),
                        rect.width() - GAP,
                    )
                } else {
                    let ha = ((rect.height() - GAP) * ratio).round();
                    (
                        Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + ha)),
                        Rect::from_min_max(pos2(rect.left(), rect.top() + ha + GAP), rect.max),
                        Rect::from_min_max(pos2(rect.left(), rect.top() + ha), pos2(rect.right(), rect.top() + ha + GAP)),
                        rect.height() - GAP,
                    )
                };
                self.tree(ui, vp, a, ra, region);
                self.tree(ui, vp, b, rb, region);
                let d = splitter(ui, ("split", id), handle, across);
                if d != 0.0
                    && let Node::Split { ratio, .. } = self.m.node_mut(id)
                {
                    *ratio = (*ratio + d / len.max(1.0)).clamp(0.08, 0.92);
                }
            }
            Node::Tabs { tabs, active } => self.tab_group(ui, vp, id, rect, region, &tabs, active),
            Node::Canvas { doc } => self.canvas_view(ui, vp, id, rect, doc),
        }
    }

    fn compass(&mut self, id: NodeId, rect: Rect, body: Rect, centre: bool) {
        let c = body.center();
        let s = 28.0;
        let step = s + 4.0;
        let mut zones = vec![
            (Zone::Left, vec2(-step, 0.0), Rect::from_min_max(rect.min, pos2(rect.center().x, rect.bottom()))),
            (Zone::Right, vec2(step, 0.0), Rect::from_min_max(pos2(rect.center().x, rect.top()), rect.max)),
            (Zone::Top, vec2(0.0, -step), Rect::from_min_max(rect.min, pos2(rect.right(), rect.center().y))),
            (Zone::Bottom, vec2(0.0, step), Rect::from_min_max(pos2(rect.left(), rect.center().y), rect.max)),
        ];
        if centre {
            zones.push((Zone::Centre, vec2(0.0, 0.0), rect));
        }
        for (z, off, preview) in zones {
            self.building.push(Target { hit: Rect::from_center_size(c + off, vec2(s, s)), dest: Dest::Node(id, z), preview, owner: Some(rect), button: true });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tab_group(&mut self, ui: &mut egui::Ui, vp: ViewportId, id: NodeId, rect: Rect, region: Region, tabs: &[Tab], active: usize) {
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), BAR));
        let body = Rect::from_min_max(pos2(rect.left(), bar.bottom()), rect.max);
        let vis = ui.visuals().clone();
        ui.painter().rect_filled(bar, 0.0, vis.faint_bg_color);
        let mut x = bar.left() + 2.0;
        for (i, t) in tabs.iter().enumerate() {
            let title = self.name(t);
            let galley = ui.painter().layout_no_wrap(title, FontId::proportional(12.0), if i == active { vis.strong_text_color() } else { vis.text_color() });
            let w = galley.size().x + 30.0;
            let r = Rect::from_min_max(pos2(x, bar.top() + 2.0), pos2((x + w).min(bar.right()), bar.bottom()));
            x += w + 2.0;
            let resp = ui.interact(r, Id::new(("dock_tab", *t)), Sense::click_and_drag());
            let fill = if i == active {
                vis.panel_fill
            } else if resp.hovered() {
                vis.widgets.hovered.bg_fill
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(r, CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, fill);
            if i == active && matches!(t, Tab::Doc(d) if self.shell.active_doc == Some(*d)) {
                ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(2.0, vis.hyperlink_color));
            }
            ui.painter().galley(pos2(r.left() + 8.0, r.center().y - galley.size().y / 2.0), galley, vis.text_color());
            let xr = Rect::from_center_size(pos2(r.right() - 11.0, r.center().y), vec2(14.0, 14.0));
            let xresp = ui.interact(xr, Id::new(("dock_tab_x", *t)), Sense::click());
            if xresp.hovered() || resp.hovered() || i == active {
                ui.painter().text(xr.center(), Align2::CENTER_CENTER, "×", FontId::proportional(13.0), if xresp.hovered() { vis.strong_text_color() } else { vis.weak_text_color() });
            }
            if xresp.clicked() {
                self.queue.push(Action::Close(*t));
            }
            if resp.clicked() || resp.drag_started() {
                if let Node::Tabs { active, .. } = self.m.node_mut(id) {
                    *active = i;
                }
                if let Tab::Doc(d) = t {
                    self.shell.active_doc = Some(*d);
                }
            }
            if resp.drag_started() {
                self.drag = Some(Drag { tab: *t, source: vp, last: ui.input(|i| i.pointer.latest_pos()) });
            }
            let t = *t;
            resp.context_menu(|ui| {
                if ui.button("Move to new window").clicked() {
                    self.queue.push(Action::Move { tab: t, to: Dest::NewWindow(None) });
                }
                if matches!(region, Region::Win(_)) && ui.button("Move to main window").clicked() {
                    self.queue.push(Action::Home(t));
                }
                if let Region::Dock(side, col) = region
                    && ui.button("Collapse column to icons").clicked()
                {
                    self.queue.push(Action::Collapse { side, col, collapsed: true });
                }
                if ui.button("Close").clicked() {
                    self.queue.push(Action::Close(t));
                }
            });
        }
        self.building.push(Target { hit: bar, dest: Dest::Node(id, Zone::Centre), preview: rect, owner: None, button: false });
        self.compass(id, rect, body, true);

        let mut child = ui.new_child(UiBuilder::new().max_rect(body).id_salt(("dock_body", id)));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        child.painter().rect_filled(body, 0.0, vis.panel_fill);
        match tabs.get(active) {
            Some(Tab::Doc(d)) => self.shell.doc_ui(&mut child, *d),
            Some(Tab::Panel(p)) => {
                let p = *p;
                egui::Frame::new().inner_margin(egui::Margin::same(6)).show(&mut child, |ui| {
                    egui::ScrollArea::vertical().id_salt(("panel_scroll", p as u8)).auto_shrink([false, false]).show(ui, |ui| self.shell.panel_ui(ui, p));
                });
            }
            None => {
                child.painter().text(body.center(), Align2::CENTER_CENTER, "Document area (protected). Open a Document from Projects.", FontId::proportional(13.0), vis.weak_text_color());
            }
        }
    }

    fn canvas_view(&mut self, ui: &mut egui::Ui, vp: ViewportId, id: NodeId, rect: Rect, doc: Option<usize>) {
        let vis = ui.visuals().clone();
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), BAR));
        let body = Rect::from_min_max(pos2(rect.left(), bar.bottom()), rect.max);
        ui.painter().rect_filled(bar, 0.0, vis.faint_bg_color);
        if self.focus_view == Some(id) || (self.focus_view.is_none() && self.m.canvases().first() == Some(&id)) {
            ui.painter().line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(2.0, vis.hyperlink_color));
        }
        let mut header = ui.new_child(UiBuilder::new().max_rect(bar.shrink2(vec2(4.0, 1.0))).layout(egui::Layout::left_to_right(egui::Align::Center)).id_salt(("view_bar", id)));
        if let Some(d) = doc {
            // The Document chip is the drag handle: drop it on another view, a split zone or
            // outside the window.
            let chip = header.add(egui::Label::new(egui::RichText::new(format!("⠿ {}", self.name(&Tab::Doc(d)))).strong()).sense(Sense::click_and_drag())).on_hover_text("Drag to show this Document elsewhere");
            if chip.drag_started() {
                self.drag = Some(Drag { tab: Tab::Doc(d), source: vp, last: ui.input(|i| i.pointer.latest_pos()) });
            }
        }
        let open = self.m.open_docs.clone();
        egui::ComboBox::from_id_salt(("view_doc", id)).selected_text("Switch ▾").width(70.0).show_ui(&mut header, |ui| {
            for o in open {
                if ui.selectable_label(doc == Some(o), self.name(&Tab::Doc(o))).clicked() {
                    self.queue.push(Action::Show { view: id, doc: o });
                    self.shell.active_doc = Some(o);
                }
            }
        });
        header.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("×").on_hover_text("Close this view").clicked() {
                self.queue.push(Action::CloseView(id));
            }
            if let Some(d) = doc {
                if ui.small_button("⇱").on_hover_text("Show in a new window").clicked() {
                    self.queue.push(Action::Move { tab: Tab::Doc(d), to: Dest::NewWindow(None) });
                }
                if ui.small_button("◫").on_hover_text("Split: second view of this Document").clicked() {
                    self.queue.push(Action::Move { tab: Tab::Doc(d), to: Dest::Node(id, Zone::Right) });
                }
                if ui.small_button("Close Document").clicked() {
                    self.queue.push(Action::Close(Tab::Doc(d)));
                }
            }
        });
        self.building.push(Target { hit: bar, dest: Dest::Node(id, Zone::Centre), preview: rect, owner: None, button: false });
        self.compass(id, rect, body, true);
        if ui.input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|p| body.contains(p))) {
            self.focus_view = Some(id);
            if let Some(d) = doc {
                self.shell.active_doc = Some(d);
            }
        }
        let mut child = ui.new_child(UiBuilder::new().max_rect(body).id_salt(("view_body", id)));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        match doc {
            Some(d) => self.shell.doc_ui(&mut child, d),
            None => {
                child.painter().rect_filled(body, 0.0, vis.extreme_bg_color);
                child.painter().text(body.center(), Align2::CENTER_CENTER, "Canvas (protected). Open a Document from Projects.", FontId::proportional(13.0), vis.weak_text_color());
            }
        }
    }

    // ------------------------------------------------------------ drag and drop

    /// Pointer position over this viewport while a drag is on: the source viewport's own
    /// pointer, or the source's screen position mapped into this one.
    fn drag_pos_here(&self, ui: &egui::Ui, vp: ViewportId) -> Option<Pos2> {
        let d = self.drag.as_ref()?;
        if d.source == vp {
            return d.last;
        }
        let src = self.vps.get(&d.source)?;
        let here = ui.input(|i| i.viewport().inner_rect).map(|r| r.min)?;
        Some(src.origin? + d.last?.to_vec2() - here.to_vec2())
    }

    fn overlay(&mut self, ui: &egui::Ui, vp: ViewportId) {
        let Some(p) = self.drag_pos_here(ui, vp) else { return };
        let Some(drag) = self.drag.as_ref() else { return };
        let tab = drag.tab;
        let painter = ui.ctx().layer_painter(LayerId::new(Order::Foreground, Id::new("dock_overlay")));
        let vis = ui.visuals();
        let accent = vis.hyperlink_color;
        let hit = hit_test(&self.building, p).cloned();
        for t in &self.building {
            if !t.button || t.owner.is_some_and(|o| !o.contains(p)) {
                continue;
            }
            let on = hit.as_ref().is_some_and(|h| h.dest == t.dest);
            painter.rect(t.hit, 4.0, if on { accent } else { vis.window_fill }, Stroke::new(1.0, accent), StrokeKind::Inside);
            let inner = t.hit.shrink(6.0);
            let mark = match t.dest {
                Dest::Node(_, Zone::Left) | Dest::NewColumn(Side::Left) => Rect::from_min_max(inner.min, pos2(inner.center().x, inner.bottom())),
                Dest::Node(_, Zone::Right) | Dest::NewColumn(Side::Right) => Rect::from_min_max(pos2(inner.center().x, inner.top()), inner.max),
                Dest::Node(_, Zone::Top) => Rect::from_min_max(inner.min, pos2(inner.right(), inner.center().y)),
                Dest::Node(_, Zone::Bottom) | Dest::NewColumn(Side::Bottom) => Rect::from_min_max(pos2(inner.left(), inner.center().y), inner.max),
                _ => inner.shrink(3.0),
            };
            painter.rect_filled(mark, 2.0, if on { vis.window_fill } else { accent.gamma_multiply(0.6) });
        }
        if let Some(h) = hit {
            let action = Action::Move { tab, to: h.dest };
            match self.m.check(&action, self.variant) {
                Ok(()) => {
                    painter.rect(h.preview.shrink(2.0), 4.0, accent.gamma_multiply(0.22), Stroke::new(2.0, accent), StrokeKind::Inside);
                }
                Err(e) => {
                    let red = Color32::from_rgb(220, 70, 60);
                    painter.rect(h.preview.shrink(2.0), 4.0, red.gamma_multiply(0.12), Stroke::new(2.0, red), StrokeKind::Inside);
                    painter.text(p + vec2(14.0, 18.0), Align2::LEFT_TOP, format!("✗ {e}"), FontId::proportional(12.0), red);
                }
            }
        }
        if self.drag.as_ref().is_some_and(|d| d.source == vp) {
            let g = painter.layout_no_wrap(self.name(&tab), FontId::proportional(12.0), vis.strong_text_color());
            let r = Rect::from_min_size(p + vec2(12.0, -24.0), g.size() + vec2(12.0, 6.0));
            painter.rect(r, 3.0, vis.window_fill.gamma_multiply(0.9), Stroke::new(1.0, accent), StrokeKind::Inside);
            painter.galley(r.min + vec2(6.0, 3.0), g, vis.strong_text_color());
        }
    }

    /// Called at the end of each viewport's frame: track the drag, finish it on release, and
    /// let a viewport claim a pending (Wayland) drop.
    fn end_of_viewport(&mut self, ui: &egui::Ui, vp: ViewportId, win: Option<u64>) {
        let origin = ui.input(|i| i.viewport().inner_rect).map(|r| r.min);
        let rect = ui.ctx().content_rect();
        self.vps.insert(vp, VpInfo { origin, rect, targets: std::mem::take(&mut self.building), win });

        if let Some(d) = self.drag.as_mut()
            && d.source == vp
        {
            if let Some(p) = ui.input(|i| i.pointer.latest_pos()) {
                d.last = Some(p);
            }
            ui.ctx().request_repaint();
            if !ui.input(|i| i.pointer.primary_down()) {
                let d = self.drag.take().unwrap();
                self.finish_drop(d, rect, origin, ui.input(|i| i.time));
            }
        }

        if let Some(p) = self.pending.as_ref()
            && p.source != vp
            && let Some(pos) = ui.input(|i| i.events.iter().rev().find_map(|e| if let egui::Event::PointerMoved(p) = e { Some(*p) } else { None }))
        {
            let p = self.pending.take().unwrap();
            let dest = self.dest_in(vp, pos);
            self.shell.log(format!("Wayland drop: claimed by {} (first to see the pointer)", if win.is_some() { "an OS window" } else { "the main window" }));
            match dest {
                Some(to) => self.queue.push(Action::Move { tab: p.tab, to }),
                None => self.shell.log("…but no drop target there; drop cancelled"),
            }
        }
    }

    /// Where a drop at `p` lands in viewport `vp`: a target, else the window itself.
    fn dest_in(&self, vp: ViewportId, p: Pos2) -> Option<Dest> {
        let info = self.vps.get(&vp)?;
        hit_test(&info.targets, p).map(|t| t.dest).or(info.win.map(Dest::WindowOf))
    }

    fn finish_drop(&mut self, d: Drag, rect: Rect, origin: Option<Pos2>, now: f64) {
        let Some(p) = d.last else { return };
        if rect.contains(p) {
            match self.dest_in(d.source, p) {
                Some(Dest::WindowOf(_)) | None => {}
                Some(to) => self.queue.push(Action::Move { tab: d.tab, to }),
            }
            return;
        }
        match origin {
            Some(o) => {
                let screen = o + p.to_vec2();
                let other = self.vps.iter().find(|(id, i)| **id != d.source && i.origin.is_some_and(|io| i.rect.contains(screen - io.to_vec2())));
                match other {
                    Some((id, i)) => {
                        let local = screen - i.origin.unwrap().to_vec2();
                        if let Some(to) = self.dest_in(*id, local) {
                            self.queue.push(Action::Move { tab: d.tab, to });
                        }
                    }
                    None => self.queue.push(Action::Move { tab: d.tab, to: Dest::NewWindow(Some(screen - vec2(40.0, 12.0))) }),
                }
            }
            None => {
                self.shell.log("Released outside the window; no screen positions (Wayland): waiting for a window to see the pointer…");
                self.pending = Some(Pending { tab: d.tab, source: d.source, since: now });
            }
        }
    }

    // ------------------------------------------------------------ native windows

    fn window_ui(&mut self, ui: &mut egui::Ui, vp: ViewportId, key: u64, root: NodeId) {
        let kind = self.m.leaves(root).into_iter().find_map(|l| match self.m.node(l) {
            Node::Tabs { tabs, active } => match tabs.get(*active) {
                Some(Tab::Doc(d)) => Some(self.shell.docs[*d].kind),
                _ => None,
            },
            Node::Canvas { doc: Some(d) } => Some(self.shell.docs[*d].kind),
            _ => None,
        });
        if self.shell.torn_off_window_has_menu {
            egui::Panel::top("win_menu").show(ui, |ui| egui::MenuBar::new().ui(ui, |ui| self.shell.menu_bar(ui)));
        }
        if kind.is_some() {
            egui::Panel::top("win_toolbar").show(ui, |ui| self.shell.toolbar(ui, kind));
        }
        let area = egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| ui.max_rect()).inner;
        self.tree(ui, vp, root, area, Region::Win(key));
        self.overlay(ui, vp);
        if ui.input(|i| i.viewport().close_requested()) {
            self.queue.push(Action::CloseWindow(key));
        }
        self.end_of_viewport(ui, vp, Some(key));
    }

    fn switcher(&mut self, ctx: &egui::Context) {
        use icons::Style;
        let typing = ctx.memory(|m| m.focused().is_some());
        let idx = Style::ALL.iter().position(|v| *v == self.shell.icon_style).unwrap();
        if !typing && ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowRight)) {
            self.shell.icon_style = Style::ALL[(idx + 1) % 2];
        }
        egui::Area::new(Id::new("proto_switcher")).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -34.0)).order(Order::Foreground).show(ctx, |ui| {
            egui::Frame::new().fill(Color32::from_rgb(250, 204, 21)).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::symmetric(12, 6)).stroke(Stroke::new(1.0, Color32::BLACK)).show(ui, |ui| {
                ui.visuals_mut().override_text_color = Some(Color32::BLACK);
                ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::from_rgb(253, 224, 71);
                ui.visuals_mut().widgets.hovered.weak_bg_fill = Color32::from_rgb(254, 240, 138);
                ui.visuals_mut().selection.bg_fill = Color32::from_rgb(234, 179, 8);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("PROTOTYPE").strong());
                    if ui.button("◀").clicked() || ui.button("▶").clicked() {
                        self.shell.icon_style = Style::ALL[(idx + 1) % 2];
                    }
                    ui.label(egui::RichText::new(self.shell.icon_style.label()).strong());
                    ui.separator();
                    for (name, v) in [("Toolbar", 0), ("Active Bar", 1)] {
                        ui.label(name);
                        for px in [16.0, 20.0, 24.0] {
                            let cur = if v == 0 { &mut self.shell.toolbar_px } else { &mut self.shell.bar_px };
                            if ui.selectable_label(*cur == px, format!("{px}")).clicked() {
                                *cur = px;
                            }
                        }
                    }
                    ui.separator();
                    for t in [ThemeChoice::RedaDark, ThemeChoice::RedaLight] {
                        if ui.selectable_label(self.shell.theme == t, t.label()).clicked() {
                            self.shell.set_theme(t);
                        }
                    }
                    ui.separator();
                    ui.toggle_value(&mut self.show_state, "Icon sheet");
                });
            });
        });
    }

    fn state_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_state;
        let style = self.shell.icon_style;
        egui::Window::new("Icon sheet (prototype)").open(&mut open).default_pos(pos2(120.0, 90.0)).show(ctx, |ui| {
            ui.label(egui::RichText::new(style.label()).strong());
            ui.weak("Each icon at 16 / 20 / 24 px: normal, hover background, active (accent) background, disabled.");
            icons::sheet(ui, style);
        });
        self.show_state = open;
    }
}

fn hit_test(targets: &[Target], p: Pos2) -> Option<&Target> {
    targets
        .iter()
        .find(|t| t.button && t.hit.contains(p) && t.owner.is_none_or(|o| o.contains(p)))
        .or_else(|| targets.iter().find(|t| !t.button && t.hit.contains(p)))
}

/// A divider handle; returns how far it was dragged along its axis.
fn splitter(ui: &egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, rect: Rect, across: bool) -> f32 {
    let resp = ui.interact(rect.expand2(if across { vec2(2.0, 0.0) } else { vec2(0.0, 2.0) }), Id::new(("splitter", salt)), Sense::drag());
    let resp = resp.on_hover_cursor(if across { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
    if resp.hovered() || resp.dragged() {
        ui.painter().rect_filled(rect, 0.0, ui.visuals().hyperlink_color.gamma_multiply(0.6));
    }
    if across { resp.drag_delta().x } else { resp.drag_delta().y }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shell.apply_theme(&ctx);
        self.shell_requests();
        self.run_queue();

        self.shell.chrome_top(ui);
        self.shell.chrome_bottom(ui);
        let area = egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| ui.max_rect()).inner;
        self.main_area(ui, area);
        self.flyout_ui(&ctx, area);
        // Screenshot aid: PROTO_DRAG=x,y[,doc] holds a drag of Properties (or a Document) there.
        let fake = std::env::var("PROTO_DRAG").ok().and_then(|v| {
            let p: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            Some((pos2(*p.first()?, *p.get(1)?), if p.len() > 2 { Tab::Doc(p[2] as usize) } else { Tab::Panel(PanelKind::Properties) }))
        });
        if let Some((p, tab)) = fake {
            self.drag = Some(Drag { tab, source: ViewportId::ROOT, last: Some(p) });
        }
        self.overlay(ui, ViewportId::ROOT);
        if fake.is_none() {
            self.end_of_viewport(ui, ViewportId::ROOT, None);
        } else {
            self.vps.insert(ViewportId::ROOT, VpInfo { origin: None, rect: area, targets: std::mem::take(&mut self.building), win: None });
        }

        let wins: Vec<(u64, NodeId, Option<Pos2>)> = self.m.windows.iter().map(|w| (w.key, w.root, w.pos)).collect();
        for (key, root, pos) in wins {
            let title: Vec<String> = self.m.leaves(root).into_iter().flat_map(|l| match self.m.node(l) {
                Node::Tabs { tabs, .. } => tabs.iter().map(|t| self.name(t)).collect::<Vec<_>>(),
                Node::Canvas { doc } => doc.iter().map(|d| self.name(&Tab::Doc(*d))).collect(),
                _ => vec![],
            }).collect();
            let vid = ViewportId::from_hash_of(("reda_dock_win", key));
            let mut b = egui::ViewportBuilder::default().with_title(format!("REDA — {}", title.join(", "))).with_inner_size([900.0, 600.0]);
            if let Some(p) = pos {
                b = b.with_position(p);
            }
            ctx.show_viewport_immediate(vid, b, |ui, _| self.window_ui(ui, vid, key, root));
        }
        let live: Vec<ViewportId> = std::iter::once(ViewportId::ROOT).chain(self.m.windows.iter().map(|w| ViewportId::from_hash_of(("reda_dock_win", w.key)))).collect();
        self.vps.retain(|id, _| live.contains(id));

        if let Some(p) = &self.pending {
            ctx.request_repaint();
            if ctx.input(|i| i.time) - p.since > 0.7 {
                let p = self.pending.take().unwrap();
                self.shell.log("No window saw the pointer: floating in a new window");
                self.queue.push(Action::Move { tab: p.tab, to: Dest::NewWindow(None) });
            }
        }
        if self.drag.is_some() && !ctx.input(|i| i.pointer.primary_down()) && self.drag.as_ref().is_some_and(|d| !self.vps.contains_key(&d.source)) {
            self.drag = None;
        }

        if std::env::var("PROTO_TEAR").is_ok() && ctx.cumulative_frame_nr() == 5 {
            let t = if self.variant == Variant::Canvas { Tab::Doc(2) } else { Tab::Panel(PanelKind::Properties) };
            self.queue.push(Action::Move { tab: t, to: Dest::NewWindow(None) });
        }
        if std::env::var("PROTO_COLLAPSE").is_ok() && ctx.cumulative_frame_nr() == 3 && !self.m.docks[1].is_empty() {
            self.queue.push(Action::Collapse { side: Side::Right, col: 0, collapsed: true });
        }
        self.switcher(&ctx);
        self.state_window(&ctx);
        if let Some(path) = screenshot_hook(&ctx, &mut self.shell) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| write_ppm(&path, &img))));
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("REDA — prototype icons").with_inner_size([1400.0, 860.0]),
        ..Default::default()
    };
    eframe::run_native("reda-prototype-icons", options, Box::new(|cc| {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        Ok(Box::new(App::new()))
    }))
}
