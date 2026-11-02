//! Draws the docking [`Layout`]: the Docks (expanded, or as icon strips with a flyout),
//! the centre's splits and tab groups, floating windows, and each tab's menu.
//!
//! Drawing never changes the layout. Every click becomes a layout Command in
//! [`Frame::commands`], which the window runs after the frame, so the layout changes the
//! same way from the mouse, the keyboard, scripts and MCP.

use egui::containers::menu::menu_style;
use egui::{
    Align2, Button, Id, Key, Modifiers, Popup, PopupKind, Rect, Role, Sense, SetOpenCommand, Ui,
    UiBuilder, Vec2, WidgetInfo,
};
use serde_json::{Value, json};

use crate::canvas::canvas;
use crate::layout::{Area, Axis, DockSide, Group, Layout, Node, PanelId, SplitSide, TabKey};
use crate::projects_panel::{self, ProjectsAction};
use crate::shell::{RecentProject, Shell};
use crate::start_page::{self, StartAction};
use crate::{theme, tr};

/// What one frame of the docking layer asks for.
#[derive(Default)]
pub struct Frame {
    /// Layout Commands to run, in order.
    pub commands: Vec<(&'static str, Value)>,
    /// A Start page button, if one was clicked.
    pub start: Option<StartAction>,
    /// What the Projects panel asks of the window, if anything.
    pub projects: Option<ProjectsAction>,
}

/// The size of a collapsed Dock's icon strip.
const STRIP: f32 = 30.0;
/// The size of a flyout from a collapsed Dock.
const FLYOUT: Vec2 = Vec2::new(280.0, 320.0);

/// The title of a Panel, for its tab, its Dock icon and the Panels button.
pub fn panel_title(panel: PanelId) -> String {
    match panel {
        PanelId::Projects => tr!("panel-projects"),
        PanelId::Properties => tr!("panel-properties"),
        PanelId::Messages => tr!("panel-messages"),
        PanelId::Library => tr!("panel-library"),
        PanelId::Layers => tr!("panel-layers"),
        PanelId::Journal => tr!("panel-journal"),
    }
}

fn tab_title(shell: &Shell, tab: &TabKey) -> String {
    match tab {
        TabKey::Start => tr!("start-page-title"),
        TabKey::Panel(panel) => panel_title(*panel),
        TabKey::Document(_) => shell.document_title(tab).unwrap_or_default().to_owned(),
    }
}

/// Draws the whole layout into `ui`, which fills the window below the menu bar and above
/// the status bar.
pub fn show(ui: &mut Ui, shell: &mut Shell) -> Frame {
    let layout = shell.layout().clone();
    let recent = if layout.place(&TabKey::Start).is_some() {
        shell.recent_projects()
    } else {
        Vec::new()
    };
    let mut cx = Cx {
        shell,
        layout: &layout,
        recent: &recent,
        frame: Frame::default(),
    };
    // The bottom Dock spans the window, so it goes first.
    for side in [DockSide::Bottom, DockSide::Left, DockSide::Right] {
        cx.dock(ui, side);
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::central_panel(ui.style()).inner_margin(0))
        .show(ui, |ui| cx.node(ui, &layout.centre, Area::Centre));
    for window in &layout.windows {
        // Until floating windows become native OS windows (Docking III), they float
        // inside the main window.
        egui::Window::new(tr!("floating-window-title"))
            .id(Id::NULL.with(("floating-window", window.id.0)))
            .default_size([480.0, 320.0])
            .show(ui.ctx(), |ui| {
                cx.node(ui, &window.root, Area::Window(window.id));
            });
    }
    cx.frame
}

struct Cx<'a> {
    shell: &'a mut Shell,
    layout: &'a Layout,
    recent: &'a [RecentProject],
    frame: Frame,
}

impl Cx<'_> {
    fn run(&mut self, id: &'static str, params: Value) {
        self.frame.commands.push((id, params));
    }

    fn dock(&mut self, ui: &mut Ui, side: DockSide) {
        let dock = self.layout.dock(side);
        if dock.groups.is_empty() {
            return;
        }
        let id = format!("dock-{side}");
        let panel = match side {
            DockSide::Left => egui::Panel::left(id),
            DockSide::Right => egui::Panel::right(id),
            DockSide::Bottom => egui::Panel::bottom(id),
        };
        if dock.collapsed {
            let strip = panel
                .resizable(false)
                .exact_size(STRIP)
                .show(ui, |ui| self.icon_strip(ui, side));
            if let Some(tab) = &self.layout.flyout
                && let Some(place) = self.layout.place(tab)
                && place.area == Area::Dock(side)
            {
                self.flyout(ui, side, strip.response.rect, tab);
            }
            return;
        }
        let default = if side == DockSide::Bottom {
            160.0
        } else {
            240.0
        };
        panel
            .resizable(true)
            .default_size(default)
            .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(0))
            .show(ui, |ui| {
                let groups = &dock.groups;
                let rects = split_rect(
                    ui.available_rect_before_wrap(),
                    Axis::Vertical,
                    groups.len(),
                );
                for (group, rect) in groups.iter().zip(rects) {
                    self.group_at(ui, rect, group, Area::Dock(side));
                }
            });
    }

    fn icon_strip(&mut self, ui: &mut Ui, side: DockSide) {
        let mut add = |ui: &mut Ui| {
            let expand = ui.add(Button::new("»").small().frame(false));
            expand.widget_info(|| WidgetInfo::labeled(Role::Button, true, tr!("dock-expand")));
            if expand.on_hover_text(tr!("dock-expand")).clicked() {
                self.run(
                    "layout.dock.collapse",
                    json!({"dock": side, "collapsed": false}),
                );
            }
            for group in &self.layout.dock(side).groups {
                for tab in &group.tabs {
                    let title = tab_title(self.shell, tab);
                    let open = self.layout.flyout.as_ref() == Some(tab);
                    let initial: String = title.chars().take(1).collect();
                    let icon = ui.add(Button::selectable(open, initial));
                    let name = title.clone();
                    icon.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                    if icon.on_hover_text(title).clicked() {
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

    fn group_at(&mut self, ui: &mut Ui, rect: Rect, group: &Group, area: Area) {
        let builder = UiBuilder::new()
            .max_rect(rect.shrink(1.0))
            .id_salt(("tab-group", group.id.0));
        ui.scope_builder(builder, |ui| {
            ui.set_clip_rect(rect);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                for (i, tab) in group.tabs.iter().enumerate() {
                    self.tab(ui, tab, i == group.active, area);
                }
            });
            ui.separator();
            match group.active_tab() {
                Some(tab) => self.content(ui, tab),
                None => {
                    ui.centered_and_justified(|ui| ui.weak(tr!("document-area-empty")));
                }
            }
        });
        ui.painter().rect_stroke(
            rect,
            0.0,
            ui.visuals().widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );
    }

    fn tab(&mut self, ui: &mut Ui, tab: &TabKey, active: bool, area: Area) {
        let title = tab_title(self.shell, tab);
        let response = ui.add(Button::selectable(active, title).sense(Sense::click()));
        if response.clicked() && !active {
            self.run("layout.tab.activate", json!({"tab": tab.to_string()}));
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

    fn content(&mut self, ui: &mut Ui, tab: &TabKey) {
        match tab {
            TabKey::Start => {
                if let Some(action) = start_page::show(ui, self.recent) {
                    self.frame.start = Some(action);
                }
            }
            TabKey::Document(uuid) => {
                let (rect, start) = (ui.available_rect_before_wrap(), self.frame.commands.len());
                let theme = theme::canvas_theme(ui.ctx().theme());
                if let Some(doc) = self.shell.document_tab_mut(*uuid) {
                    canvas(ui, doc, theme);
                }
                self.focus_on_press(ui, rect, tab, start);
            }
            TabKey::Panel(PanelId::Projects) => {
                let (rect, start) = (ui.available_rect_before_wrap(), self.frame.commands.len());
                let action = projects_panel::show(ui, self.shell, &mut self.frame.commands);
                if action.is_some() {
                    self.frame.projects = action;
                }
                self.focus_on_press(ui, rect, tab, start);
            }
            TabKey::Panel(_) => {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| ui.weak(tr!("panel-empty")));
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
            let activate = ("layout.tab.activate", json!({"tab": tab.to_string()}));
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

/// `rect` cut into `n` equal parts along `axis`.
fn split_rect(rect: Rect, axis: Axis, n: usize) -> Vec<Rect> {
    let n = n.max(1);
    (0..n)
        .map(|i| {
            let (a, b) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
            match axis {
                Axis::Horizontal => Rect::from_x_y_ranges(
                    rect.left() + rect.width() * a..=rect.left() + rect.width() * b,
                    rect.y_range(),
                ),
                Axis::Vertical => Rect::from_x_y_ranges(
                    rect.x_range(),
                    rect.top() + rect.height() * a..=rect.top() + rect.height() * b,
                ),
            }
        })
        .collect()
}
