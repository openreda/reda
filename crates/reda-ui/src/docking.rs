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

use egui::{
    Align2, Id, Key, Modifiers, Popup, Rect, Response, Role, Ui, UiBuilder, Vec2, ViewportBuilder,
    ViewportId,
};
use reda_engine::model::{DocumentKind, Scope, Uuid};
use serde_json::{Value, json};

use self::windows::{WindowView, Zones};
use crate::bars;
use crate::bars::draw::{self, Invocation};
use crate::canvas::{CanvasInputs, canvas_id, canvas_input, canvas_paint};
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
use crate::theme::{self, TokensExt as _, motion};
use crate::tr;
use crate::widgets::{self, CARD_GAP, Glyph, TabKind, Tooltip as _};
use crate::{a11y, keymap};

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

/// The size a side Dock and the bottom Dock open at, until the user drags them.
const SIDE_DOCK: f32 = 256.0;
const BOTTOM_DOCK: f32 = 220.0;
/// Where egui cannot open OS windows: the size a floating window opens at inside the
/// main window, and how much of the main window a settings window leaves free.
const EMBEDDED_WINDOW: Vec2 = Vec2::new(480.0, 320.0);
const EMBEDDED_MARGIN: f32 = 40.0;
/// The smallest a floating window can be made.
const WINDOW_MIN: Vec2 = Vec2::new(240.0, 160.0);
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
        PanelId::Pins => tr!("panel-pins"),
    }
}

/// The icon key of `tab`: its Panel's, its Document type's, or the Start page's.
pub(crate) fn tab_icon_key(shell: &Shell, tab: &TabKey) -> Option<&'static str> {
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
    let dirty = shell.engine().dirty_scopes();
    // Cards keep the card gap from the window's side edges. Above them is the top row
    // and below them the status bar, both on the base.
    let window = ui.available_rect_before_wrap();
    let area = window.shrink2(Vec2::new(CARD_GAP, 0.0));
    let mut cx = Cx {
        shell,
        layout: &layout,
        recent: &recent,
        frame: Frame::default(),
        dirty,
        zones: Zones {
            area,
            groups: Vec::new(),
            docks: Vec::new(),
            dock_edges: true,
        },
        focus_request,
        settings,
    };
    // What no group covers shows as the gaps between them: the window's base.
    let base = ui.tokens().color.surface.base;
    ui.painter().rect_filled(window, 0.0, base);
    ui.advance_cursor_after_rect(window);
    let builder = UiBuilder::new().max_rect(area).id_salt("docking-area");
    ui.scope_builder(builder, |ui| cx.main_window(ui));
    let ctx = ui.ctx().clone();
    // Where egui cannot open OS windows (as in tests), floating windows float inside the
    // main window and share its drop zones.
    let embedded = ctx.embed_viewports();
    if embedded {
        for window in &layout.windows {
            // A settings window opens centred, as large as it opens natively where the main
            // window has room for that.
            let screen = ctx.content_rect();
            let settings = opening_size(&layout, window.id) == SETTINGS_WINDOW;
            let size = if settings {
                SETTINGS_WINDOW.min(screen.size() - Vec2::splat(EMBEDDED_MARGIN))
            } else {
                EMBEDDED_WINDOW
            };
            let id = Id::NULL.with(("floating-window", window.id.0));
            let mut floating = widgets::window(id, window_title(cx.shell, &layout, window.id))
                .frame(widgets::window_frame(&ctx.tokens()))
                .size(size)
                .resizable(true)
                .still();
            if settings {
                floating = floating.centred();
            }
            floating.show(&ctx, |ui| {
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

impl Cx<'_, '_> {
    /// Draws the main window's Docks and centre into `ui`.
    fn main_window(&mut self, ui: &mut Ui) {
        let cx = self;
        let layout = cx.layout;
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
    }
}

/// The slim Title bar at the top of a floating window REDA decorates itself: the window's
/// title, drag space and, where REDA draws them, the window controls, which act on that
/// window. None under the system title bar.
fn floating_bar(ui: &mut Ui, title: &str, chrome: crate::title_bar::Chrome) {
    if !chrome.controls && !chrome.drags {
        return;
    }
    let ctx = ui.ctx().clone();
    let height = crate::title_bar::floating_bar_height(&ui.tokens());
    egui::Panel::top("floating-bar")
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .resizable(false)
        .exact_size(height)
        .show(ui, |ui| {
            let bar = widgets::floating_bar(title)
                .height(height)
                .leading_inset(chrome.leading_inset)
                .controls(chrome.controls)
                .drags(chrome.drags)
                .maximised(chrome.maximised)
                .corner(chrome.corner)
                .show(ui);
            // Only the main window's system menu is in reach (title_bar::show_window_menu).
            let _ = crate::title_bar::drive(&ctx, chrome, bar.action, &bar.response);
        });
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
    /// What has changes not yet saved: those tabs show the modified dot.
    dirty: Vec<Scope>,
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
        let title = window_title(self.shell, self.layout, window.id);
        let mut builder = ViewportBuilder::default()
            .with_title(title.clone())
            .with_inner_size(size)
            .with_min_inner_size(WINDOW_MIN);
        // Built like the main window, with REDA's slim bar, unless the system title bar
        // is in force.
        let startup = crate::title_bar::Startup::decorating(ctx);
        if let Some(startup) = startup {
            builder = crate::title_bar::viewport(startup, builder);
        }
        let saved = window.rect.map(|r| egui::pos2(r.x as f32, r.y as f32));
        if let Some(at) = drag::placement(ctx, self.layout, window.id).or(saved) {
            builder = builder.with_position(at);
        }
        ctx.show_viewport_immediate(viewport, builder, |ui, _| {
            let ctx = ui.ctx().clone();
            let chrome = startup.map_or(crate::title_bar::Chrome::SYSTEM, |startup| {
                crate::title_bar::chrome_for(&ctx, startup)
            });
            floating_bar(ui, &title, chrome);
            // Cards keep the card gap from the window's edges; the tab strip is at its top.
            let gap = CARD_GAP as i8;
            let margin = egui::Margin {
                left: gap,
                right: gap,
                top: 0,
                bottom: gap,
            };
            let base = ui.tokens().color.surface.base;
            let corner = chrome.corner.round() as u8;
            let bottom = egui::CornerRadius {
                sw: corner,
                se: corner,
                ..egui::CornerRadius::ZERO
            };
            let area = egui::CentralPanel::default()
                .frame((egui::Frame::new().fill(base).corner_radius(bottom)).inner_margin(margin))
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
            crate::title_bar::edge(&ctx, chrome);
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
        let strip = ui.tokens().control_height() + CARD_GAP;
        let open = dock_open(ui, side, dock.collapsed);
        // The strip, the expanded Dock and the Dock on its way between them are three
        // panels, so the expanded one keeps its size while the Dock is collapsed.
        let id = match open {
            0.0 => format!("dock-{side}-strip"),
            1.0 => format!("dock-{side}"),
            _ => format!("dock-{side}-moving"),
        };
        // The gap on the Dock's inner edge is its splitter.
        let gap = CARD_GAP as i8;
        let none = egui::Margin::ZERO;
        let (panel, margin) = match side {
            DockSide::Left => (egui::Panel::left(id), egui::Margin { right: gap, ..none }),
            DockSide::Right => (egui::Panel::right(id), egui::Margin { left: gap, ..none }),
            DockSide::Bottom => (egui::Panel::bottom(id), egui::Margin { top: gap, ..none }),
        };
        let panel = panel
            .frame(egui::Frame::NONE.outer_margin(margin))
            .show_separator_line(false);
        let size_id = Id::NULL.with(("dock-expanded-size", side.to_string()));
        let shown = if open == 0.0 {
            panel.resizable(false).exact_size(strip).show(ui, reserve)
        } else if open < 1.0 {
            // Collapsing or expanding: its width (or height) moves between the strip's
            // and the expanded Dock's.
            let full = ui
                .data(|d| d.get_temp::<f32>(size_id))
                .unwrap_or_else(|| default_dock_size(side));
            let size = egui::lerp(strip..=full.max(strip), open);
            panel.resizable(false).exact_size(size).show(ui, reserve)
        } else {
            // It shows the accent while dragged.
            let panel = panel.resizable(true).default_size(default_dock_size(side));
            let shown = widgets::splitter(ui, CARD_GAP, |ui| panel.show(ui, reserve));
            let rect = shown.response.rect;
            let size = match side {
                DockSide::Bottom => rect.height(),
                DockSide::Left | DockSide::Right => rect.width(),
            };
            ui.data_mut(|d| d.insert_temp(size_id, size));
            shown
        };
        self.zones.docks.push((side, shown.response.rect));
        Some(shown.inner)
    }

    /// Draws the Dock on `side` into `rect`, which [`Cx::reserve_dock`] set aside: its
    /// groups, or as an icon strip with any flyout.
    fn dock(&mut self, ui: &mut Ui, side: DockSide, rect: Rect) {
        let dock = self.layout.dock(side);
        let open = dock_open(ui, side, dock.collapsed);
        if open > 0.0 && open < 1.0 {
            self.dock_moving(ui, side, rect, open);
            return;
        }
        if dock.collapsed {
            // The strip is on the base, with no frame.
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

    /// A Dock on its way between its strip and its groups, `open` of the way to expanded:
    /// both are drawn clipped to `rect`, the groups at the expanded size from the
    /// window's edge, and they cross-fade.
    fn dock_moving(&mut self, ui: &mut Ui, side: DockSide, rect: Rect, open: f32) {
        let size_id = Id::NULL.with(("dock-expanded-size", side.to_string()));
        let full = ui
            .data(|d| d.get_temp::<f32>(size_id))
            .unwrap_or_else(|| default_dock_size(side))
            - CARD_GAP;
        let expanded = match side {
            DockSide::Left => rect.with_max_x(rect.left() + full),
            DockSide::Right => rect.with_min_x(rect.right() - full),
            DockSide::Bottom => rect.with_min_y(rect.bottom() - full),
        };
        let clip = rect.intersect(ui.clip_rect());
        let builder = UiBuilder::new()
            .max_rect(rect)
            .id_salt(("dock-strip", side.to_string()));
        ui.scope_builder(builder, |ui| {
            ui.set_clip_rect(clip);
            ui.multiply_opacity(1.0 - open);
            self.icon_strip(ui, side);
        });
        let builder = UiBuilder::new()
            .max_rect(expanded)
            .id_salt(("dock-groups", side.to_string()));
        ui.scope_builder(builder, |ui| {
            ui.set_clip_rect(clip);
            ui.multiply_opacity(open);
            let groups = &self.layout.dock(side).groups;
            for (group, part) in
                groups
                    .iter()
                    .zip(split_rect(expanded, Axis::Vertical, groups.len()))
            {
                self.group_at(ui, part, group, Area::Dock(side));
            }
        });
    }

    /// A collapsed Dock's strip: the button that expands the Dock, then a 20 px Icon
    /// button per tab. The Icon of the open flyout is on.
    fn icon_strip(&mut self, ui: &mut Ui, side: DockSide) {
        widgets::dock_strip(ui, side == DockSide::Bottom, |ui| {
            // The chevron points the way the Dock opens.
            let chevron = match side {
                DockSide::Left => Glyph::ChevronRight,
                DockSide::Right => Glyph::ChevronLeft,
                DockSide::Bottom => Glyph::ChevronUp,
            };
            let expand = ui
                .add(widgets::glyph_button(chevron))
                .tooltip(tr!("dock-expand"));
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
                    let icon = match tab_icon_key(self.shell, tab) {
                        Some(key) => ui.add(widgets::icon_button(key).bar().on(open)),
                        None => {
                            let initial = title.chars().take(1).collect::<String>();
                            ui.add(widgets::button(initial).flat().on(open))
                        }
                    };
                    if self.focus_request.as_ref() == Some(tab) {
                        icon.request_focus();
                    }
                    let icon = icon.tooltip(&title);
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
        });
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
        // Mod+double-click on a Port goes to its partner on another sheet.
        if self.shell.tool() == crate::tools::Tool::Select
            && kind == Some(DocumentKind::Schematic)
            && input.response.double_clicked()
            && input.response.ctx.input(|i| i.modifiers.command)
            && let Some(screen) = input.response.interact_pointer_pos()
            && let Some(port) = crate::port_partner::port_at(
                self.shell,
                document,
                input.document_point(screen),
                5.0 * input.camera.nm_per_point,
            )
        {
            let _ = self.shell.run_reporting(
                "schematic.port.go_to_partner",
                serde_json::json!({ "objects": [port] }),
            );
            return Vec::new();
        }
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

    /// The open flyout of the collapsed Dock on `side`: the Dock's card, floating beside
    /// its strip and as long as it, a card gap away.
    fn flyout(&mut self, ui: &mut Ui, side: DockSide, strip: Rect, tab: &TabKey) {
        let t = ui.tokens();
        let gap = widgets::CARD_GAP;
        let along = Vec2::new(FLYOUT.x, strip.height());
        let (pos, pivot, outer) = match side {
            DockSide::Left => (
                strip.right_top() + Vec2::new(gap, 0.0),
                Align2::LEFT_TOP,
                along,
            ),
            DockSide::Right => (
                strip.left_top() - Vec2::new(gap, 0.0),
                Align2::RIGHT_TOP,
                along,
            ),
            DockSide::Bottom => (
                strip.left_top() - Vec2::new(0.0, gap),
                Align2::LEFT_BOTTOM,
                Vec2::new(strip.width(), FLYOUT.y),
            ),
        };
        let frame = widgets::flyout_frame(&t);
        let inner = outer - frame.total_margin().sum();
        // It fades in from the strip, travelling away from it, and fades back out.
        let d = t.motion.flyout_travel;
        let from = match side {
            DockSide::Left => Vec2::new(-d, 0.0),
            DockSide::Right => Vec2::new(d, 0.0),
            DockSide::Bottom => Vec2::new(0.0, d),
        };
        let shown = egui::Area::new(Id::NULL.with(("dock-flyout", side.to_string())))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .pivot(pivot)
            .fade_in(false)
            .show(ui.ctx(), |ui| {
                frame.show(ui, |ui| {
                    ui.set_min_size(inner);
                    ui.set_max_size(inner);
                    // A Panel too long for the card is cut at its padding.
                    ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                    let title = tab_title(self.shell, tab);
                    widgets::flyout_title(ui, tab_icon_key(self.shell, tab), &title);
                    self.content(ui, tab);
                });
            });
        let look = motion::Look::travel(from);
        motion::layer(ui.ctx(), shown.response.layer_id, look, t.motion.flyout);
    }

    fn node(&mut self, ui: &mut Ui, node: &Node, area: Area) {
        let rect = ui.available_rect_before_wrap();
        let base = ui.tokens().color.surface.base;
        ui.painter().rect_filled(rect, 0.0, base);
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

    /// Draws a tab group into `rect`: its tab strip on the window's base, then its card,
    /// which the front tab is joined to, with the front tab's contents. The gaps between
    /// groups separate them.
    fn group_at(&mut self, ui: &mut Ui, rect: Rect, group: &Group, area: Area) {
        let t = ui.tokens();
        let strip = Rect::from_min_size(rect.min, Vec2::new(rect.width(), t.tab_height()));
        let body = rect.with_min_y(strip.bottom());
        widgets::paint_card(ui, &t, body);
        let clip = ui.clip_rect();
        let builder = UiBuilder::new()
            .max_rect(strip)
            .layout(egui::Layout::left_to_right(egui::Align::Max))
            .id_salt(("tab-group", group.id.0));
        let mut tabs = Vec::new();
        ui.scope_builder(builder, |ui| {
            // The front tab reaches one hairline into the card, over its rim.
            let reach = strip.with_max_y(strip.bottom() + t.stroke.hairline);
            ui.set_clip_rect(reach.intersect(clip));
            widgets::tab_strip(ui, |ui| {
                for (i, tab) in group.tabs.iter().enumerate() {
                    tabs.push(self.tab(ui, tab, i == group.active, area));
                }
            });
        });
        let front = tabs.get(group.active).copied();
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
                // Panels and the Start page keep the card's padding.
                Some(tab @ (TabKey::Panel(_) | TabKey::Start | TabKey::Settings(_))) => {
                    egui::Frame::new()
                        .inner_margin(t.space.panel)
                        .show(ui, |ui| self.content(ui, tab));
                }
                // Documents fill the card. A canvas cannot be clipped round, so the
                // card's corners are masked over it and its rim drawn again.
                Some(tab) => {
                    self.content(ui, tab);
                    widgets::mask_card_corners(ui, &t, body);
                    if let Some(front) = front {
                        widgets::join_tab(ui, &t, front);
                    }
                }
                None => {
                    ui.centered_and_justified(|ui| ui.weak(tr!("document-area-empty")));
                }
            }
        });
    }

    /// Draws a tab on its group's strip; returns its rect. The front tab is a card joined
    /// to the group's body, and the active Document's tab adds the accent line along its
    /// top edge: the menus and Commands act on that Document. Back tabs sit on the base in
    /// muted text, and a hovered one takes the hover fill. The front or hovered tab shows
    /// its close button.
    fn tab(&mut self, ui: &mut Ui, tab: &TabKey, active: bool, area: Area) -> Rect {
        let title = tab_title(self.shell, tab);
        // The PCB tab carries the "out of sync" badge while Sync changes are pending.
        let pending = match tab {
            TabKey::Document(uuid) if self.shell.is_pcb(*uuid) => self.shell.out_of_sync(),
            _ => 0,
        };
        let active_document = active
            && matches!(tab, TabKey::Document(uuid) if self.layout.active_document() == Some(*uuid));
        let kind = match (active_document, active) {
            (true, _) => TabKind::Active,
            (false, true) => TabKind::Front,
            (false, false) => TabKind::Back,
        };
        let modified = matches!(tab, TabKey::Document(uuid)
            if self.dirty.contains(&Scope::Document(*uuid))
                || self.dirty.contains(&Scope::Element(*uuid)));
        let shown = widgets::tab(&title)
            .icon(tab_icon_key(self.shell, tab))
            .kind(kind)
            .modified(modified)
            .out_of_sync(pending > 0)
            .show(ui);
        let response = shown.response;
        let close = shown.close.map(|close| {
            let name = tr!("tab-close", tab = title.as_str());
            let close = close.tooltip(&name);
            a11y::name(&close, Role::Button, true, &name, None, None);
            close
        });
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
        if pending > 0 {
            response
                .clone()
                .tooltip(tr!("status-out-of-sync", count = pending));
        }
        a11y::name(
            &response,
            Role::Tab,
            true,
            &name,
            Some(active),
            shortcut.as_deref(),
        );
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
        let shown = if by_key {
            let mut menu = widgets::menu_below(&response);
            if pressed {
                menu = menu.open();
            }
            menu.show(|ui| self.tab_menu(ui, tab, area)).is_some()
        } else {
            widgets::context_menu(&response, |ui| self.tab_menu(ui, tab, area)).is_some()
        };
        ui.data_mut(|d| d.insert_temp(by_key_id, by_key && shown));
        response.rect
    }

    /// The tab menu: every keyboard docking move, each a layout Command. An item the
    /// layout would refuse is greyed out, with the reason on hover.
    fn tab_menu(&mut self, ui: &mut Ui, tab: &TabKey, area: Area) {
        let param = tab.to_string();
        widgets::menu_button(tr!("tab-menu-move-to")).show(ui, |ui| {
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
        // A sheet's tab selects its Components' Footprints on the board.
        if let TabKey::Document(sheet) = tab
            && (self.shell.engine().project()).is_some_and(|p| p.schematic_sheet(*sheet).is_some())
            && self.item(ui, tr!("tab-menu-select-footprints"), None)
        {
            self.run("selection.footprints_of_sheet", json!({"document": sheet}));
        }
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
        let response = ui.add_enabled(refused.is_none(), widgets::menu_item(label));
        let clicked = response.clicked();
        if let Some(reason) = refused {
            response.disabled_tooltip(reason);
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
                // Auto-pan while an operation is in flight: a drag, a move, a Tool's run.
                let busy = ui
                    .input(|i| i.pointer.primary_down() && i.pointer.is_decidedly_dragging())
                    || self.shell.pcb_tool().moving()
                    || self.shell.router().routing();
                let auto_pan = self.shell.preferences().editing.auto_pan.clone();
                let Some(doc) = self.shell.document_tab_mut(*uuid) else {
                    return;
                };
                let busy = busy || doc.interaction != crate::schematic_editor::Interaction::Idle;
                let input = canvas_input(ui, doc, kind, busy.then_some(&auto_pan));
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
                if element_editor_symbol(self.shell, *uuid) {
                    crate::symbol_editor::hover_tip(self.shell, *uuid, &input.response);
                }
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
                    Some(_) => match crate::show_on_board::banner(ui, id, rect, self.shell) {
                        Some(crate::show_on_board::BannerAction::Back) => {
                            self.shell.back_from_show();
                        }
                        Some(crate::show_on_board::BannerAction::Switch) => {
                            self.shell.switch_shown_side();
                        }
                        None => {}
                    },
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
                let edits = schematic_editor::decorate(ui, self.shell, *uuid, rect);
                self.frame.commands.extend(edits);
                let active = self.shell.active_document().map(|d| d.document) == Some(*uuid);
                if active {
                    crate::keys_card::show(ui, self.shell, rect);
                }
                crate::delta::track_clicks(ui, self.shell, *uuid, rect);
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
                let (rect, start) = (ui.available_rect_before_wrap(), self.frame.commands.len());
                for out in panels::properties::show(ui, self.shell) {
                    match out {
                        panels::properties::Out::Run(id, p) => {
                            self.frame.commands.push((id.to_owned(), p));
                        }
                        panels::properties::Out::Messages(action) => {
                            self.frame.messages.push(action);
                        }
                        panels::properties::Out::Units(units) => {
                            self.shell.set_properties_units(Some(units));
                        }
                    }
                }
                self.focus_on_press(ui, rect, tab, start);
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
            TabKey::Panel(PanelId::Pins) => {
                let commands = panels::pins::show(ui, self.shell);
                self.frame.commands.extend(commands);
            }
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

/// `rect` cut into `n` equal parts along `axis`, a card gap apart, on whole points.
/// How far the Dock on `side` is from its strip (0) to expanded (1): it moves between
/// them in `motion.dock_collapse`.
fn dock_open(ui: &Ui, side: DockSide, collapsed: bool) -> f32 {
    let id = Id::NULL.with(("dock-open", side.to_string()));
    let tr = ui.tokens().motion.dock_collapse;
    motion::toggle(ui.ctx(), id, !collapsed, tr)
}

/// The size a Dock opens at before the user has dragged it.
fn default_dock_size(side: DockSide) -> f32 {
    match side {
        DockSide::Bottom => BOTTOM_DOCK,
        DockSide::Left | DockSide::Right => SIDE_DOCK,
    }
}

fn split_rect(rect: Rect, axis: Axis, n: usize) -> Vec<Rect> {
    let n = n.max(1);
    let length = match axis {
        Axis::Horizontal => rect.width(),
        Axis::Vertical => rect.height(),
    };
    let part = ((length - CARD_GAP * (n - 1) as f32) / n as f32)
        .floor()
        .max(0.0);
    (0..n)
        .map(|i| {
            let a = i as f32 * (part + CARD_GAP);
            // The last part takes what the rounding left.
            let part = if i + 1 == n {
                (length - a).max(0.0)
            } else {
                part
            };
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

/// Whether the tab `uuid` is an open Symbol's.
fn element_editor_symbol(shell: &Shell, uuid: Uuid) -> bool {
    crate::symbol_editor::symbol_of(shell.engine(), uuid).is_some()
}
