//! Draws the docking [`Layout`]: the Docks (expanded, or as icon strips with a flyout),
//! the centre's splits and tab groups, floating windows, and each tab's menu.
//!
//! Each floating window is an OS window of its own (an egui immediate viewport), drawn in
//! the same frame as the main window. Tabs drag between windows ([`windows`]). Where egui
//! cannot open OS windows, as in tests, floating windows float inside the main window.
//!
//! Drawing never changes the layout. Every click and every drop of a dragged tab
//! ([`drag`]) becomes a layout Command in [`Frame::commands`], which the window runs after
//! the frame, so the layout changes the same way from the mouse, the keyboard, scripts and
//! MCP.

mod drag;
pub mod windows;

use std::collections::BTreeMap;

use egui::containers::menu::menu_style;
use egui::{
    Align2, Button, Color32, Id, Key, Modifiers, Popup, PopupKind, Rect, Response, Role, Sense,
    SetOpenCommand, Stroke, Ui, UiBuilder, Vec2, ViewportBuilder, ViewportId,
};
use reda_engine::model::{DocumentKind, Uuid};
use serde_json::{Value, json};

use crate::bars;
use crate::bars::draw::{self, Invocation};
use crate::canvas::{CanvasInputs, canvas_id, canvas_input, canvas_paint};
use crate::chrome;
use crate::icons;
use crate::journal_panel::{self, JournalAction};
use crate::layout::{
    Area, Axis, DockSide, FloatingWindow, Group, Layout, Node, PanelId, SettingsWindow, SplitSide,
    TabKey, WindowId, WindowRect,
};
use crate::panels;
use crate::projects_panel::{self, ProjectsAction};
use crate::schematic_editor;
use crate::shell::{RecentProject, Shell};
use crate::start_page::{self, StartAction};
use crate::{a11y, keymap};
use crate::{theme, tr};
use windows::{WindowView, Zones};

/// What one frame of the docking layer asks for.
#[derive(Default)]
pub struct Frame {
    /// Layout Commands to run, in order.
    pub commands: Vec<(String, Value)>,
    /// A Start page button, if one was clicked.
    pub start: Option<StartAction>,
    /// What the Projects panel asks of the window, if anything.
    pub projects: Option<ProjectsAction>,
    /// A Journal panel button, if one was clicked.
    pub journal: Option<JournalAction>,
    /// Clicks in the Messages panel.
    pub messages: Vec<panels::messages::Action>,
    /// Commands to call from the GUI, which ask for params they lack in a dialog.
    pub calls: Vec<(String, Value)>,
    /// What the keys pressed in windows of their own bind, resolved: the main window's
    /// keys are the App's.
    pub keys: Vec<(String, Value)>,
}

/// The size of a collapsed Dock's icon strip.
const STRIP: f32 = 24.0;
/// The side of a button on a collapsed Dock's icon strip.
const STRIP_BUTTON: f32 = 22.0;
/// The height of a tab group's tab strip.
const TAB_STRIP: f32 = 24.0;
/// The gap between tab groups, and between the Docks and the centre.
const GAP: f32 = 3.0;
/// The margin around a Panel's contents.
const PANEL_MARGIN: i8 = 6;
/// The size of a flyout from a collapsed Dock.
const FLYOUT: Vec2 = Vec2::new(280.0, 320.0);
/// The size a floating window opens at.
const WINDOW: Vec2 = Vec2::new(640.0, 420.0);
/// The size a settings window opens at, the first time.
pub const SETTINGS_WINDOW: Vec2 = Vec2::new(1000.0, 680.0);

/// Draws a settings window's content into a tab.
pub type SettingsContent<'a> = dyn FnMut(&mut Ui, &mut Shell, SettingsWindow) + 'a;

/// The size the floating window `window` opens at when it has no place yet: a settings
/// window's when it holds only settings windows, else a plain floating window's.
pub fn opening_size(layout: &Layout, window: WindowId) -> Vec2 {
    let tabs = layout.tabs_in_window(window);
    if !tabs.is_empty() && tabs.iter().all(|t| matches!(t, TabKey::Settings(_))) {
        SETTINGS_WINDOW
    } else {
        WINDOW
    }
}

/// The OS window (viewport) that shows the floating window `window`.
pub fn viewport_of(window: WindowId) -> ViewportId {
    drag::viewport_id(window)
}

/// The title of a settings window's tab when the window has not named it.
pub fn settings_title(window: SettingsWindow) -> String {
    match window {
        SettingsWindow::Preferences => tr!("pref-title"),
        SettingsWindow::ProjectSettings => tr!("project-settings-title"),
        SettingsWindow::BoardSetup => tr!("board-setup-title"),
        SettingsWindow::OutputJob => tr!("output-job-window-title"),
        SettingsWindow::ChangeList => tr!("change-list-title"),
    }
}

/// The title of a Panel, for its tab, its Dock icon and the Panels button.
pub fn panel_title(panel: PanelId) -> String {
    match panel {
        PanelId::Projects => tr!("panel-projects"),
        PanelId::Properties => tr!("panel-properties"),
        PanelId::Messages => tr!("panel-messages"),
        PanelId::Library => tr!("panel-library"),
        PanelId::Layers => tr!("panel-layers"),
        PanelId::Journal => tr!("panel-journal"),
        PanelId::Navigator => tr!("panel-navigator"),
        PanelId::Find => tr!("panel-find"),
    }
}

/// A tab's 16 px icon: its Panel's, its Document type's, or the Start page's.
pub(crate) fn tab_icon(
    shell: &Shell,
    tab: &TabKey,
    theme: egui::Theme,
) -> Option<egui::Image<'static>> {
    icons::image(tab_icon_key(shell, tab)?, icons::TAB, theme, true)
}

/// The icon key of `tab`: its Panel's, its Document type's, or the Start page's.
fn tab_icon_key(shell: &Shell, tab: &TabKey) -> Option<&'static str> {
    Some(match tab {
        TabKey::Start => icons::START_PAGE,
        TabKey::Panel(panel) => icons::panel_icon(*panel),
        TabKey::Settings(_) => return None,
        TabKey::Document(uuid) => {
            let kind = shell
                .engine()
                .project()?
                .documents()
                .find(|d| d.uuid == *uuid)?
                .kind;
            icons::document_icon(kind)
        }
    })
}

/// The key that shows `tab`: a Panel's show Preset's, labelled for the OS `ctx` reports,
/// as the bars label theirs.
fn tab_shortcut(ctx: &egui::Context, shell: &Shell, tab: &TabKey) -> Option<String> {
    match tab {
        TabKey::Panel(panel) => keymap::shortcut_label(
            shell,
            &format!("panel.show:{}", panel.id()),
            ctx.os().is_mac(),
        ),
        _ => None,
    }
}

pub(crate) fn tab_title(shell: &Shell, tab: &TabKey) -> String {
    match tab {
        TabKey::Start => tr!("start-page-title"),
        TabKey::Panel(panel) => panel_title(*panel),
        TabKey::Document(_) => shell.document_title(tab).unwrap_or_default().to_owned(),
        TabKey::Settings(window) => match shell.settings_title(*window) {
            Some(title) => title.to_owned(),
            None => settings_title(*window),
        },
    }
}

/// Draws the whole layout into `ui`, which fills the window below the menu bar and above
/// the status bar.
/// `settings` draws the settings windows' tabs.
pub fn show(ui: &mut Ui, shell: &mut Shell, settings: &mut SettingsContent<'_>) -> Frame {
    let layout = shell.layout().clone();
    let recent = if layout.place(&TabKey::Start).is_some() {
        shell.recent_projects()
    } else {
        Vec::new()
    };
    let focus_request = shell.take_focus_request();
    let mut cx = Cx {
        shell,
        layout: &layout,
        recent: &recent,
        frame: Frame::default(),
        zones: Zones {
            area: ui.available_rect_before_wrap(),
            groups: Vec::new(),
            docks: Vec::new(),
            dock_edges: true,
        },
        focus_request,
        settings,
    };
    // What no group covers shows as the gaps between them.
    let gap = theme::gap(ui.ctx().theme());
    ui.painter()
        .rect_filled(ui.available_rect_before_wrap(), 0.0, gap);
    // Space for the Docks and the centre is set aside first, the bottom Dock first since
    // it spans the window. What is in them is drawn after, in layout order (left Dock,
    // centre, right Dock, bottom Dock), which is the order Tab moves keyboard focus in.
    let mut docks = Vec::new();
    for side in [DockSide::Bottom, DockSide::Left, DockSide::Right] {
        if let Some(rect) = cx.reserve_dock(ui, side) {
            docks.push((side, rect));
        }
    }
    let centre = egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, reserve)
        .inner;
    let dock_at = |side| docks.iter().find(|(s, _)| *s == side).map(|(_, r)| *r);
    if let Some(rect) = dock_at(DockSide::Left) {
        cx.dock(ui, DockSide::Left, rect);
    }
    cx.node_at(ui, centre, &layout.centre, Area::Centre);
    for side in [DockSide::Right, DockSide::Bottom] {
        if let Some(rect) = dock_at(side) {
            cx.dock(ui, side, rect);
        }
    }
    let ctx = ui.ctx().clone();
    // Where egui cannot open OS windows (as in tests), floating windows float inside the
    // main window and share its drop zones.
    let embedded = ctx.embed_viewports();
    if embedded {
        for window in &layout.windows {
            // A settings window opens centred, as large as it opens natively where the main
            // window has room for that.
            let screen = ctx.content_rect();
            let (size, at) = match opening_size(&layout, window.id) {
                size if size == SETTINGS_WINDOW => (
                    size.min(screen.size() - Vec2::splat(40.0)),
                    Some(screen.center()),
                ),
                _ => (Vec2::new(480.0, 320.0), None),
            };
            let mut floating = egui::Window::new(window_title(cx.shell, &layout, window.id));
            if let Some(at) = at {
                floating = floating.pivot(Align2::CENTER_CENTER).default_pos(at);
            }
            floating
                .id(Id::NULL.with(("floating-window", window.id.0)))
                .default_size(size)
                .show(&ctx, |ui| {
                    cx.node(ui, &window.root, Area::Window(window.id));
                });
        }
    }
    let view = WindowView {
        window: None,
        origin: ctx.input(|i| i.viewport().inner_rect).map(|r| r.min),
        rect: ctx.content_rect(),
        zones: std::mem::replace(&mut cx.zones, Zones::empty(Rect::NOTHING)),
        first_group: layout.groups().first().map(|(_, g)| g.id),
    };
    let shell = &*cx.shell;
    let commands = drag::show(&ctx, &layout, view, |tab| tab_title(shell, tab));
    cx.frame.commands.extend(commands);
    let mut live = vec![egui::ViewportId::ROOT];
    if !embedded {
        for window in &layout.windows {
            live.push(cx.native_window(&ctx, window));
        }
    }
    drag::retain(&ctx, &live);
    cx.frame
}

/// A floating window's title: its active tab's, after the app's name.
fn window_title(shell: &Shell, layout: &Layout, window: WindowId) -> String {
    match first_group(layout, window).and_then(|g| g.active_tab()) {
        Some(tab) => tr!("floating-window-title-tab", tab = tab_title(shell, tab)),
        None => tr!("floating-window-title"),
    }
}

/// The first group of the floating window `window`.
fn first_group(layout: &Layout, window: WindowId) -> Option<&Group> {
    layout
        .groups()
        .into_iter()
        .find(|(area, _)| *area == Area::Window(window))
        .map(|(_, g)| g)
}

struct Cx<'a, 'b> {
    shell: &'a mut Shell,
    layout: &'a Layout,
    recent: &'a [RecentProject],
    frame: Frame,
    /// Where a dragged tab can be dropped, as drawn this frame.
    zones: Zones,
    /// The tab whose button takes the keyboard focus this frame, after F6 or the tab
    /// switcher moved focus to it.
    focus_request: Option<TabKey>,
    settings: &'a mut SettingsContent<'b>,
}

/// Sets aside all the space left in `ui` for contents drawn later; returns it. A Dock
/// keeps its size (its default, or as the user dragged it) whatever its contents take.
fn reserve(ui: &mut Ui) -> Rect {
    let rect = ui.available_rect_before_wrap();
    ui.advance_cursor_after_rect(rect);
    rect
}

impl Cx<'_, '_> {
    fn run(&mut self, id: &'static str, params: Value) {
        self.frame.commands.push((id.to_owned(), params));
    }

    /// Shows the floating window `window` as an OS window of its own; returns its
    /// viewport. Closing the OS window closes the window's tabs.
    fn native_window(&mut self, ctx: &egui::Context, window: &FloatingWindow) -> ViewportId {
        let viewport = drag::viewport_id(window.id);
        let size = window
            .rect
            .map_or(opening_size(self.layout, window.id), |r| {
                egui::vec2(r.width as f32, r.height as f32)
            });
        let mut builder = ViewportBuilder::default()
            .with_title(window_title(self.shell, self.layout, window.id))
            .with_inner_size(size)
            .with_min_inner_size([240.0, 160.0]);
        let saved = window.rect.map(|r| egui::pos2(r.x as f32, r.y as f32));
        if let Some(at) = drag::placement(ctx, self.layout, window.id).or(saved) {
            builder = builder.with_position(at);
        }
        ctx.show_viewport_immediate(viewport, builder, |ui, _| {
            let ctx = ui.ctx().clone();
            let area = egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(ui.style()).inner_margin(0))
                .show(ui, |ui| {
                    let area = ui.available_rect_before_wrap();
                    self.zones = Zones::empty(area);
                    self.node(ui, &window.root, Area::Window(window.id));
                    area
                })
                .inner;
            if ctx.input(|i| i.viewport().close_requested()) {
                self.run("layout.window.close", json!({"window": window.id}));
            }
            if !ctx.text_edit_focused() {
                let keys = crate::keymap::resolve_keys(&ctx, self.shell, None);
                self.frame.keys.extend(keys);
            }
            // Keep where the user put the window, once the OS reports it.
            let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
            if let (Some(outer), Some(inner)) = (outer, inner) {
                let now = WindowRect {
                    x: outer.min.x.round() as i32,
                    y: outer.min.y.round() as i32,
                    width: inner.width().round() as u32,
                    height: inner.height().round() as u32,
                };
                if window.rect != Some(now) {
                    self.run(
                        "layout.window.place",
                        json!({"window": window.id, "x": now.x, "y": now.y,
                               "width": now.width, "height": now.height}),
                    );
                }
            }
            let mut zones = std::mem::replace(&mut self.zones, Zones::empty(Rect::NOTHING));
            zones.area = area;
            let view = WindowView {
                window: Some(window.id),
                origin: ctx.input(|i| i.viewport().inner_rect).map(|r| r.min),
                rect: ctx.content_rect(),
                zones,
                first_group: first_group(self.layout, window.id).map(|g| g.id),
            };
            let shell = &*self.shell;
            let commands = drag::show(&ctx, self.layout, view, |tab| tab_title(shell, tab));
            self.frame.commands.extend(commands);
        });
        viewport
    }

    /// Sets aside the space of the Dock on `side`, if it holds anything; returns the
    /// rect its contents go in.
    fn reserve_dock(&mut self, ui: &mut Ui, side: DockSide) -> Option<Rect> {
        let dock = self.layout.dock(side);
        if dock.groups.is_empty() {
            return None;
        }
        let id = format!("dock-{side}");
        // The gap on the Dock's inner edge is its splitter: it turns the accent while
        // hovered or dragged.
        let gap = GAP as i8;
        let none = egui::Margin::ZERO;
        let (panel, margin) = match side {
            DockSide::Left => (egui::Panel::left(id), egui::Margin { right: gap, ..none }),
            DockSide::Right => (egui::Panel::right(id), egui::Margin { left: gap, ..none }),
            DockSide::Bottom => (egui::Panel::bottom(id), egui::Margin { top: gap, ..none }),
        };
        let panel = panel
            .frame(egui::Frame::NONE.outer_margin(margin))
            .show_separator_line(false);
        let shown = if dock.collapsed {
            panel
                .resizable(false)
                .exact_size(STRIP + GAP)
                .show(ui, reserve)
        } else {
            let default = if side == DockSide::Bottom {
                160.0
            } else {
                240.0
            };
            let style = ui.style().clone();
            let splitter = Stroke::new(GAP, theme::accent(ui.ctx().theme()).gamma_multiply(0.6));
            let widgets = &mut ui.style_mut().visuals.widgets;
            widgets.hovered.fg_stroke = splitter;
            widgets.active.fg_stroke = splitter;
            let shown = panel
                .resizable(true)
                .default_size(default)
                .show(ui, reserve);
            ui.set_style(style);
            shown
        };
        self.zones.docks.push((side, shown.response.rect));
        Some(shown.inner)
    }

    /// Draws the Dock on `side` into `rect`, which [`Cx::reserve_dock`] set aside: its
    /// groups, or as an icon strip with any flyout.
    fn dock(&mut self, ui: &mut Ui, side: DockSide, rect: Rect) {
        let dock = self.layout.dock(side);
        if dock.collapsed {
            ui.painter()
                .rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
            let builder = UiBuilder::new()
                .max_rect(rect)
                .id_salt(("dock-strip", side.to_string()));
            let strip = ui.scope_builder(builder, |ui| self.icon_strip(ui, side));
            let strip = strip.response.rect.union(rect);
            if let Some(tab) = &self.layout.flyout
                && let Some(place) = self.layout.place(tab)
                && place.area == Area::Dock(side)
            {
                self.flyout(ui, side, strip, tab);
            }
            return;
        }
        // An expanded Dock has no header: it collapses from the View menu or a tab's menu.
        let groups = &dock.groups;
        for (group, part) in groups
            .iter()
            .zip(split_rect(rect, Axis::Vertical, groups.len()))
        {
            self.group_at(ui, part, group, Area::Dock(side));
        }
    }

    fn icon_strip(&mut self, ui: &mut Ui, side: DockSide) {
        let mut add = |ui: &mut Ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(2.0);
            ui.add_space(1.0);
            let size = match side {
                DockSide::Bottom => Vec2::new(18.0, STRIP_BUTTON),
                _ => Vec2::new(STRIP_BUTTON, 18.0),
            };
            let (rect, expand) = ui.allocate_exact_size(size, Sense::click());
            let colour = if expand.hovered() {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().text_color()
            };
            // The chevron points the way the Dock opens.
            let towards = match side {
                DockSide::Left => Vec2::new(1.0, 0.0),
                DockSide::Right => Vec2::new(-1.0, 0.0),
                DockSide::Bottom => Vec2::new(0.0, -1.0),
            };
            chevron(ui, rect.center(), towards, colour);
            let expand = expand.on_hover_text(tr!("dock-expand"));
            a11y::name(&expand, Role::Button, true, &tr!("dock-expand"), None, None);
            if expand.clicked() {
                self.run(
                    "layout.dock.collapse",
                    json!({"dock": side, "collapsed": false}),
                );
            }
            for group in &self.layout.dock(side).groups {
                for tab in &group.tabs {
                    let title = tab_title(self.shell, tab);
                    let open = self.layout.flyout.as_ref() == Some(tab);
                    let icon = match tab_icon(self.shell, tab, ui.ctx().theme()) {
                        Some(image) => {
                            chrome::icon_button(ui, image, icons::TAB, STRIP_BUTTON, open, true)
                        }
                        None => ui.add(Button::selectable(
                            open,
                            title.chars().take(1).collect::<String>(),
                        )),
                    };
                    if self.focus_request.as_ref() == Some(tab) {
                        icon.request_focus();
                    }
                    let icon = icon.on_hover_text(&title);
                    let shortcut = tab_shortcut(ui.ctx(), self.shell, tab);
                    a11y::name(&icon, Role::Button, true, &title, None, shortcut.as_deref());
                    if icon.clicked() {
                        if open {
                            // A second click dismisses the flyout.
                            self.run("layout.dock.collapse", json!({"dock": side}));
                        } else {
                            self.run("layout.tab.activate", json!({"tab": tab.to_string()}));
                        }
                    }
                }
            }
        };
        if side == DockSide::Bottom {
            ui.horizontal_centered(|ui| add(ui));
        } else {
            ui.vertical_centered(|ui| add(ui));
        }
    }

    /// Runs the editor of `document`'s type on this frame's canvas input. Returns the
    /// Commands to run. Keys reach it through the Keymap (`App::keys`).
    fn edit(&mut self, document: Uuid, input: &CanvasInputs) -> Vec<(String, Value)> {
        let kind = self
            .shell
            .engine()
            .project()
            .and_then(|p| p.document(document))
            .map(|d| d.kind);
        self.shell
            .selection_picking_mut()
            .set_time(input.response.ctx.input(|i| i.time));
        if self.shell.tool() == crate::tools::Tool::Select
            && input.response.double_clicked()
            && let Some(screen) = input.response.interact_pointer_pos()
        {
            crate::selection::picking::double_click(
                self.shell,
                document,
                input.document_point(screen),
                5.0 * input.camera.nm_per_point,
                screen,
            );
            return Vec::new();
        }
        for event in &input.events {
            match event {
                crate::canvas::interact::Pointer::DragStart { .. } => {
                    self.shell.selection_picking_mut().set_idle(false)
                }
                crate::canvas::interact::Pointer::DragEnd(_) => {
                    self.shell.selection_picking_mut().set_idle(true)
                }
                _ => {}
            }
        }
        // A Part from the Library panel in flight takes the pointer on any canvas.
        if self.shell.tool() == crate::tools::Tool::Place {
            crate::placement::canvas(self.shell, document, input);
            return Vec::new();
        }
        match kind {
            // A right-click in the Wire Tool is Esc: it ends the run, or leaves the Tool.
            Some(DocumentKind::Schematic)
                if self.shell.tool() == crate::tools::Tool::Wire
                    && input.response.secondary_clicked() =>
            {
                vec![("tool.activate:select".to_owned(), json!({}))]
            }
            Some(DocumentKind::Schematic) => {
                let scale = schematic_editor::Scale::at(input.camera.nm_per_point);
                schematic_editor::pointer(self.shell, document, &input.events, scale)
            }
            // The PCB Tool runs its Commands itself.
            Some(DocumentKind::Pcb) => {
                crate::pcb::canvas::pointer(self.shell, document, input);
                Vec::new()
            }
            None => crate::element_editor::edit(self.shell, document, input),
        }
    }

    /// The Active Bar over the canvas of `document`, for its Document type.
    fn active_bar(&self, ui: &mut Ui, document: Uuid, id: Id, rect: Rect) -> Option<Invocation> {
        let kind = bars::document_type(self.shell, document)?;
        let checked =
            BTreeMap::from([("tool.activate", json!({ "tool": self.shell.tool().id() }))]);
        let cx = draw::Cx {
            shell: self.shell,
            checked: &checked,
            document: Some(document),
        };
        draw::active_bar(ui, &cx, bars::for_document(Some(kind)), id, rect)
    }

    fn flyout(&mut self, ui: &mut Ui, side: DockSide, strip: Rect, tab: &TabKey) {
        let (pos, pivot) = match side {
            DockSide::Left => (strip.right_top(), Align2::LEFT_TOP),
            DockSide::Right => (strip.left_top(), Align2::RIGHT_TOP),
            DockSide::Bottom => (strip.left_top(), Align2::LEFT_BOTTOM),
        };
        egui::Area::new(Id::NULL.with(("dock-flyout", side.to_string())))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .pivot(pivot)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_size(FLYOUT);
                    ui.set_max_size(FLYOUT);
                    ui.strong(tab_title(self.shell, tab));
                    ui.separator();
                    self.content(ui, tab);
                });
            });
    }

    fn node(&mut self, ui: &mut Ui, node: &Node, area: Area) {
        let rect = ui.available_rect_before_wrap();
        let gap = theme::gap(ui.ctx().theme());
        ui.painter().rect_filled(rect, 0.0, gap);
        self.node_at(ui, rect, node, area);
    }

    fn node_at(&mut self, ui: &mut Ui, rect: Rect, node: &Node, area: Area) {
        match node {
            Node::Group(group) => self.group_at(ui, rect, group, area),
            Node::Split { axis, children } => {
                for (child, rect) in children.iter().zip(split_rect(rect, *axis, children.len())) {
                    self.node_at(ui, rect, child, area);
                }
            }
        }
    }

    /// Draws a tab group into `rect`: its tab strip, a solid blue bar across the group, then
    /// its active tab's contents on the panel colour. The bar is the full accent on the
    /// group holding the active Document, which the menus follow, and muted on the others.
    /// Groups have no border: the gaps between them separate them.
    fn group_at(&mut self, ui: &mut Ui, rect: Rect, group: &Group, area: Area) {
        let strip = Rect::from_min_size(rect.min, Vec2::new(rect.width(), TAB_STRIP));
        let body = Rect::from_min_max(egui::pos2(rect.left(), strip.bottom()), rect.max);
        let holds_active_document = self
            .layout
            .active_document()
            .is_some_and(|uuid| group.tabs.contains(&TabKey::Document(uuid)));
        let bar = theme::tab_bar(ui.ctx().theme(), holds_active_document);
        ui.painter().rect_filled(strip, 0.0, bar);
        ui.painter().rect_filled(body, 0.0, ui.visuals().panel_fill);
        let clip = ui.clip_rect();
        let builder = UiBuilder::new()
            .max_rect(strip)
            .layout(egui::Layout::left_to_right(egui::Align::Max))
            .id_salt(("tab-group", group.id.0));
        let mut tabs = Vec::new();
        ui.scope_builder(builder, |ui| {
            ui.set_clip_rect(strip.intersect(clip));
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(2.0);
            for (i, tab) in group.tabs.iter().enumerate() {
                tabs.push(self.tab(ui, tab, i == group.active, area));
            }
        });
        self.zones.groups.push(drag::GroupZone {
            group: group.id,
            rect,
            strip,
            tabs,
        });
        let builder = UiBuilder::new()
            .max_rect(body)
            .id_salt(("tab-group-body", group.id.0));
        ui.scope_builder(builder, |ui| {
            ui.set_clip_rect(body.intersect(clip));
            match group.active_tab() {
                // Panels and the Start page keep a margin; Documents fill the group.
                Some(tab @ (TabKey::Panel(_) | TabKey::Start | TabKey::Settings(_))) => {
                    egui::Frame::new()
                        .inner_margin(PANEL_MARGIN)
                        .show(ui, |ui| self.content(ui, tab));
                }
                Some(tab) => self.content(ui, tab),
                None => {
                    ui.centered_and_justified(|ui| ui.weak(tr!("document-area-empty")));
                }
            }
        });
    }

    /// Draws a tab on its group's blue bar; returns its rect. The active tab's title and
    /// icon are bright, the others' dimmer; a hovered tab lightens. The active or hovered
    /// tab shows its close button.
    fn tab(&mut self, ui: &mut Ui, tab: &TabKey, active: bool, area: Area) -> Rect {
        let title = tab_title(self.shell, tab);
        // The PCB tab carries the "out of sync" badge while Sync changes are pending.
        let pending = match tab {
            TabKey::Document(uuid) if self.shell.is_pcb(*uuid) => self.shell.out_of_sync(),
            _ => 0,
        };
        let font = egui::TextStyle::Button.resolve(ui.style());
        let colour = theme::tab_text(active);
        let galley = ui
            .painter()
            .layout_no_wrap(title.clone(), font.clone(), colour);
        // The badge is a dark-on-amber pill, legible on either blue.
        let badge_text = Color32::from_rgb(0x14, 0x16, 0x19);
        let badge_fill = Color32::from_rgb(0xe8, 0xa3, 0x3d);
        let badge = (pending > 0).then(|| {
            ui.painter().layout_no_wrap(
                tr!("tab-out-of-sync-badge", count = pending),
                egui::TextStyle::Small.resolve(ui.style()),
                badge_text,
            )
        });
        // The icon's silhouette in the title's colour: the bar is blue on both themes, and
        // the role colours would be faint on it.
        let icon = tab_icon_key(self.shell, tab)
            .and_then(|key| icons::silhouette_image(key, icons::TAB))
            .map(|i| i.tint(colour));
        // | 8 | icon 4 | title | 4 badge | 4 × 4 |
        let close_size = 14.0;
        let width = 8.0
            + icon.as_ref().map_or(0.0, |_| icons::TAB + 4.0)
            + galley.size().x
            + badge.as_ref().map_or(0.0, |b| 4.0 + b.size().x + 6.0)
            + 4.0
            + close_size
            + 4.0;
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, TAB_STRIP - 2.0), Sense::click_and_drag());
        let pointed = ui.rect_contains_pointer(rect);
        let close_rect = Rect::from_center_size(
            egui::pos2(rect.right() - 4.0 - close_size / 2.0, rect.center().y),
            Vec2::splat(close_size),
        );
        let close = (active || pointed).then(|| {
            let close = ui.interact(close_rect, response.id.with("close"), Sense::click());
            let name = tr!("tab-close", tab = title.as_str());
            let close = close.on_hover_text(&name);
            a11y::name(&close, Role::Button, true, &name, None, None);
            close
        });
        if ui.is_rect_visible(rect) {
            if active {
                // White at 15 % behind the active tab, rounded at the top, and a 2 px
                // white underline across its width.
                let top = egui::CornerRadius {
                    nw: 3,
                    ne: 3,
                    sw: 0,
                    se: 0,
                };
                ui.painter().rect_filled(rect, top, theme::ACTIVE_TAB_FILL);
                let line = Rect::from_min_max(
                    egui::pos2(rect.left(), rect.bottom() - theme::ACTIVE_TAB_UNDERLINE),
                    rect.max,
                );
                ui.painter().rect_filled(line, 0.0, Color32::WHITE);
            } else if pointed {
                let hover = Color32::from_white_alpha(20);
                ui.painter().rect_filled(rect, 3.0, hover);
            }
            let mut x = rect.left() + 8.0;
            if let Some(image) = icon {
                let at = Rect::from_min_size(
                    egui::pos2(x, rect.center().y - icons::TAB / 2.0),
                    Vec2::splat(icons::TAB),
                );
                image.paint_at(ui, at);
                x += icons::TAB + 4.0;
            }
            let painter = ui.painter();
            let text_y = rect.center().y - galley.size().y / 2.0;
            let title_width = galley.size().x;
            painter.galley(egui::pos2(x, text_y), galley, colour);
            x += title_width + 4.0;
            if let Some(badge) = badge {
                let y = rect.center().y - badge.size().y / 2.0;
                let pill = Rect::from_min_size(egui::pos2(x, y), badge.size())
                    .expand2(Vec2::new(3.0, 1.0))
                    .translate(Vec2::new(3.0, 0.0));
                painter.rect_filled(pill, 3.0, badge_fill);
                painter.galley(egui::pos2(x + 3.0, y), badge, badge_text);
            }
            if let Some(close) = &close {
                let colour = theme::tab_text(close.hovered());
                painter.text(
                    close_rect.center(),
                    Align2::CENTER_CENTER,
                    "×",
                    egui::FontId::proportional(13.0),
                    colour,
                );
            }
        }
        // A middle-click on any tab closes it.
        if close.as_ref().is_some_and(Response::clicked) || response.middle_clicked() {
            self.run("layout.tab.close", json!({"tab": tab.to_string()}));
        }
        let shortcut = tab_shortcut(&response.ctx, self.shell, tab);
        // The badge is read out with the tab's name.
        let name = if pending > 0 {
            tr!("tab-out-of-sync", tab = title.as_str(), count = pending)
        } else {
            title.clone()
        };
        a11y::name(
            &response,
            Role::Tab,
            true,
            &name,
            Some(active),
            shortcut.as_deref(),
        );
        if pending > 0 {
            response
                .clone()
                .on_hover_text(tr!("status-out-of-sync", count = pending));
        }
        if self.focus_request.as_ref() == Some(tab) {
            response.request_focus();
        }
        // A click on a tab focuses it, even when it is already shown.
        if response.clicked() && (!active || self.layout.focus.as_ref() != Some(tab)) {
            self.run("layout.tab.activate", json!({"tab": tab.to_string()}));
        }
        if response.drag_started() {
            drag::start(ui.ctx(), tab);
        }
        // The tab menu opens on a right-click, or from the keyboard with Shift+F10 or the
        // Menu key while the tab has focus.
        // A menu opened from the keyboard hangs below the tab rather than at the pointer.
        let by_key_id = Popup::default_response_id(&response).with("by-key");
        let pressed = response.has_focus()
            && ui.input_mut(|i| {
                i.consume_key(Modifiers::SHIFT, Key::F10)
                    || i.consume_key(Modifiers::NONE, Key::Menu)
            });
        let by_key = !response.secondary_clicked()
            && (pressed || ui.data(|d| d.get_temp::<bool>(by_key_id).unwrap_or(false)));
        let popup = if by_key {
            Popup::from_response(&response)
                .kind(PopupKind::Menu)
                .layout(egui::Layout::top_down_justified(egui::Align::Min))
                .style(menu_style)
                .open_memory(pressed.then_some(SetOpenCommand::Bool(true)))
        } else {
            Popup::context_menu(&response)
        };
        let shown = popup.show(|ui| self.tab_menu(ui, tab, area)).is_some();
        ui.data_mut(|d| d.insert_temp(by_key_id, by_key && shown));
        response.rect
    }

    /// The tab menu: every keyboard docking move, each a layout Command. An item the
    /// layout would refuse is greyed out, with the reason on hover.
    fn tab_menu(&mut self, ui: &mut Ui, tab: &TabKey, area: Area) {
        let param = tab.to_string();
        ui.menu_button(tr!("tab-menu-move-to"), |ui| {
            for (side, label) in [
                (DockSide::Left, tr!("tab-menu-left-dock")),
                (DockSide::Right, tr!("tab-menu-right-dock")),
                (DockSide::Bottom, tr!("tab-menu-bottom-dock")),
            ] {
                let refused = refusal(self.layout, |l| l.move_to_dock(tab, side).map(drop));
                if self.item(ui, label, refused) {
                    self.run(
                        "layout.tab.move_to_dock",
                        json!({"tab": param, "dock": side}),
                    );
                }
            }
            let refused = refusal(self.layout, |l| l.move_to_new_window(tab).map(drop));
            if self.item(ui, tr!("tab-menu-new-window"), refused) {
                self.run("layout.tab.move_to_new_window", json!({"tab": param}));
            }
            for (side, label) in [
                (SplitSide::Right, tr!("tab-menu-split-right")),
                (SplitSide::Down, tr!("tab-menu-split-down")),
            ] {
                let refused = refusal(self.layout, |l| l.split(tab, side).map(drop));
                if self.item(ui, label, refused) {
                    self.run("layout.tab.split", json!({"tab": param, "side": side}));
                }
            }
        });
        if self.item(ui, tr!("tab-menu-close"), None) {
            self.run("layout.tab.close", json!({"tab": param}));
        }
        let refused = match area {
            Area::Dock(side) => refusal(self.layout, |l| l.collapse(side, true)),
            _ => Some(tr!("tab-menu-not-in-dock")),
        };
        if self.item(ui, tr!("tab-menu-collapse-dock"), refused)
            && let Area::Dock(side) = area
        {
            self.run("layout.dock.collapse", json!({"dock": side}));
        }
    }

    /// A menu item that closes the menu when clicked; `refused` greys it out.
    fn item(&mut self, ui: &mut Ui, label: String, refused: Option<String>) -> bool {
        let response = ui.add_enabled(refused.is_none(), Button::new(label));
        let clicked = response.clicked();
        if let Some(reason) = refused {
            response.on_disabled_hover_text(reason);
        }
        if clicked {
            ui.close();
        }
        clicked
    }

    /// Queues a Command to run after the frame; one still lacking params (text's words,
    /// a Layer set's name) asks for them in a dialog.
    fn push_call(&mut self, id: String, params: serde_json::Value) {
        let registry = self.shell.engine().registry();
        if crate::command_dialog::lacks_required(registry, &id, &params) {
            self.frame.calls.push((id, params));
        } else {
            self.frame.commands.push((id, params));
        }
    }

    fn content(&mut self, ui: &mut Ui, tab: &TabKey) {
        match tab {
            TabKey::Start => {
                if let Some(action) = start_page::show(ui, self.recent) {
                    self.frame.start = Some(action);
                }
            }
            TabKey::Document(uuid) => {
                let start = self.frame.commands.len();
                if crate::element_editor::is_element(self.shell.engine(), *uuid) {
                    let rect = ui.available_rect_before_wrap();
                    let commands = crate::element_editor::panels(ui, self.shell, *uuid);
                    self.frame.commands.extend(commands);
                    // A Part's form fills its tab: there is no canvas.
                    if !crate::element_editor::has_canvas(self.shell.engine(), *uuid) {
                        self.focus_on_press(ui, rect, tab, start);
                        return;
                    }
                }
                let rect = ui.available_rect_before_wrap();
                let theme = theme::canvas_theme(ui.ctx().theme());
                let kind = bars::document_type(self.shell, *uuid);
                let kind = kind.unwrap_or(bars::DocumentType::Schematic);
                let Some(doc) = self.shell.document_tab_mut(*uuid) else {
                    return;
                };
                let input = canvas_input(ui, doc, kind);
                // Shown on the board for a settings window, the canvas only pans and zooms.
                let view_only = self.shell.view_only(*uuid);
                let edits = if view_only {
                    Vec::new()
                } else {
                    let (modifiers, pressed) =
                        ui.input(|i| (i.modifiers, i.pointer.primary_down()));
                    self.shell.hold_snap_keys(*uuid, modifiers, pressed);
                    self.edit(*uuid, &input)
                };
                // A Command still lacking params (text's words) asks for them in a dialog.
                for (id, params) in edits {
                    self.push_call(id, params);
                }
                // In the Wire Tool a right-click is Esc (see `edit`), and while routing it
                // ends the route (pcb::canvas), instead of the menu.
                let wiring = kind == bars::DocumentType::Schematic
                    && self.shell.tool() == crate::tools::Tool::Wire;
                let routing = kind == bars::DocumentType::Pcb
                    && crate::pcb::route::takes_right_click(self.shell);
                if !view_only && !wiring && !routing {
                    crate::context_menu::canvas(ui, self.shell, *uuid, kind, &input);
                    crate::canvas::markers::click(self.shell, *uuid, &input);
                }
                let Some(doc) = self.shell.document_tab(*uuid) else {
                    return;
                };
                canvas_paint(ui, &input, doc, theme);
                if !view_only
                    && self
                        .shell
                        .engine()
                        .project()
                        .is_some_and(|p| p.pcb_document(*uuid).is_some())
                {
                    for (id, params) in crate::pcb::canvas::decorate(ui, self.shell, &input) {
                        self.push_call(id, params);
                    }
                }
                let Some(doc) = self.shell.document_tab(*uuid) else {
                    return;
                };
                let id = canvas_id(doc);
                match self.shell.shown_on_board().filter(|_| view_only) {
                    Some(count) => {
                        if crate::show_on_board::banner(ui, id, rect, count) {
                            self.shell.back_to_board_setup();
                        }
                    }
                    None => {
                        if let Some(clicked) = self.active_bar(ui, *uuid, id, rect) {
                            self.frame.commands.push(clicked);
                        }
                    }
                }
                crate::selection::picking::show(ui, self.shell, *uuid);
                let options = schematic_editor::auto_wire_options(ui, self.shell, *uuid, rect);
                self.frame.commands.extend(options);
                let options = schematic_editor::break_wire_options(ui, self.shell, *uuid, rect);
                self.frame.commands.extend(options);
                if self.shell.active_document().map(|d| d.document) == Some(*uuid) {
                    crate::keys_card::show(ui, self.shell, rect);
                }
                self.focus_on_press(ui, rect, tab, start);
            }
            TabKey::Panel(PanelId::Projects) => {
                let (rect, start) = (ui.available_rect_before_wrap(), self.frame.commands.len());
                let mut commands = Vec::new();
                let action = projects_panel::show(ui, self.shell, &mut commands);
                let commands = commands.into_iter().map(|(id, p)| (id.to_owned(), p));
                self.frame.commands.extend(commands);
                if action.is_some() {
                    self.frame.projects = action;
                }
                self.focus_on_press(ui, rect, tab, start);
            }
            TabKey::Panel(PanelId::Properties) => {
                let commands = panels::properties::show(ui, self.shell);
                let commands = commands.into_iter().map(|(id, p)| (id.to_owned(), p));
                self.frame.commands.extend(commands);
            }
            TabKey::Panel(PanelId::Journal) => {
                if let Some(action) = journal_panel::show(ui, self.shell.engine()) {
                    self.frame.journal = Some(action);
                }
            }
            TabKey::Panel(PanelId::Layers) => {
                for (id, params) in panels::layers::show(ui, self.shell) {
                    self.push_call(id, params);
                }
            }
            TabKey::Panel(PanelId::Find) => panels::find::show(ui, self.shell),
            TabKey::Panel(PanelId::Navigator) => {
                let commands = panels::navigator::show(ui, self.shell);
                self.frame.commands.extend(commands);
            }
            TabKey::Panel(PanelId::Library) => {
                for action in panels::library::show(ui, self.shell) {
                    match action {
                        panels::library::Action::Run(id, params) => {
                            self.frame.commands.push((id, params));
                        }
                        panels::library::Action::Call(id, params) => {
                            self.frame.calls.push((id, params));
                        }
                    }
                }
            }
            TabKey::Panel(PanelId::Messages) => {
                let actions = panels::messages::show(ui, self.shell);
                self.frame.messages.extend(actions);
            }
            TabKey::Settings(window) => {
                let (rect, start) = (ui.available_rect_before_wrap(), self.frame.commands.len());
                (self.settings)(ui, self.shell, *window);
                self.focus_on_press(ui, rect, tab, start);
            }
        }
    }

    /// Focuses `tab` when a pointer button goes down on its content, `rect`: focus picks
    /// the undo stack and what the menus follow. It runs before the commands the content
    /// pushed from `start` on, so a Document a double-click opens keeps the focus.
    fn focus_on_press(&mut self, ui: &Ui, rect: Rect, tab: &TabKey, start: usize) {
        if self.layout.focus.as_ref() == Some(tab) {
            return;
        }
        let pressed_at = ui.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        let pressed_here = pressed_at
            .is_some_and(|p| rect.contains(p) && ui.ctx().layer_id_at(p) == Some(ui.layer_id()));
        if pressed_here {
            let activate = (
                "layout.tab.activate".to_owned(),
                json!({"tab": tab.to_string()}),
            );
            self.frame.commands.insert(start, activate);
        }
    }
}

/// Why the layout would refuse `action`, tried on a copy.
fn refusal(
    layout: &Layout,
    action: impl FnOnce(&mut Layout) -> Result<(), reda_engine::command::CommandError>,
) -> Option<String> {
    let mut copy = layout.clone();
    action(&mut copy).err().map(|e| e.message)
}

/// `rect` cut into `n` equal parts along `axis`, a [`GAP`] apart.
fn split_rect(rect: Rect, axis: Axis, n: usize) -> Vec<Rect> {
    let n = n.max(1);
    let length = match axis {
        Axis::Horizontal => rect.width(),
        Axis::Vertical => rect.height(),
    };
    let part = ((length - GAP * (n - 1) as f32) / n as f32).max(0.0);
    (0..n)
        .map(|i| {
            let a = i as f32 * (part + GAP);
            match axis {
                Axis::Horizontal => {
                    Rect::from_x_y_ranges(rect.left() + a..=rect.left() + a + part, rect.y_range())
                }
                Axis::Vertical => {
                    Rect::from_x_y_ranges(rect.x_range(), rect.top() + a..=rect.top() + a + part)
                }
            }
        })
        .collect()
}

/// A small chevron centred on `at`, pointing `towards` (a unit vector).
fn chevron(ui: &Ui, at: egui::Pos2, towards: Vec2, colour: egui::Color32) {
    let across = Vec2::new(-towards.y, towards.x);
    let (tip, back) = (at + towards * 2.0, at - towards * 2.0);
    let stroke = Stroke::new(1.5, colour);
    ui.painter()
        .line_segment([back + across * 3.5, tip], stroke);
    ui.painter()
        .line_segment([tip, back - across * 3.5], stroke);
}
