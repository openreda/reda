// PROTOTYPE — throwaway, lives only on the `prototype/main-window-shell` branch.
//
// REDA main-window shell on egui 0.36 + egui_dock 0.21, with native OS-window tear-off built on
// egui's immediate viewports (neither docking crate supports it).
//
// Tear-off, three ways:
//   1. right-click a tab → "Move to new window";
//   2. drag a tab out of any drop zone: egui_dock makes a floating window, which this prototype
//      promotes straight into a native OS window;
//   3. inside a torn-off window, right-click → "Move to main window", or close the window
//      (its tabs return to the main window).
//
// Run: cargo run -p prototype-shell-dock

#![allow(dead_code)]

mod common;

use common::*;
use eframe::egui;
use egui_dock::{DockArea, DockState, NodeIndex, NodePath, Surface, SurfaceIndex, TabViewer};

struct Torn {
    id: egui::ViewportId,
    dock: DockState<Tab>,
    pos: Option<egui::Pos2>,
}

enum Action {
    TearOff(Tab, Option<egui::Pos2>),
    DockBack(Tab),
    CloseWindow(usize),
}

struct App {
    shell: Shell,
    main: DockState<Tab>,
    torn: Vec<Torn>,
    next_window: u64,
}

struct Viewer<'a> {
    shell: &'a mut Shell,
    actions: &'a mut Vec<Action>,
    in_torn: bool,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        self.shell.tab_title(tab).into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match *tab {
            Tab::Doc(i) => self.shell.doc_ui(ui, i),
            Tab::Panel(p) => self.shell.panel_ui(ui, p),
        }
    }

    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Tab, _path: NodePath) {
        if self.in_torn {
            if ui.button("Move to main window").clicked() {
                self.actions.push(Action::DockBack(*tab));
            }
        }
        if ui.button("Move to new window").clicked() {
            self.actions.push(Action::TearOff(*tab, None));
        }
    }

    fn on_tab_button(&mut self, tab: &mut Tab, response: &egui::Response) {
        if (response.clicked() || response.drag_started())
            && let Tab::Doc(i) = *tab
        {
            self.shell.active_doc = Some(i);
        }
    }

    fn on_close(&mut self, tab: &mut Tab) -> egui_dock::tab_viewer::OnCloseResponse {
        if let Tab::Doc(i) = *tab
            && self.shell.active_doc == Some(i)
        {
            self.shell.active_doc = None;
        }
        egui_dock::tab_viewer::OnCloseResponse::Close
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        match tab {
            Tab::Doc(_) => [false, false],
            Tab::Panel(_) => [false, true],
        }
    }
}

fn build_main(layout: Layout, docs: Vec<Tab>) -> DockState<Tab> {
    let mut ds = DockState::new(docs);
    if layout == Layout::Docked {
        let s = ds.main_surface_mut();
        let [docs_n, _] = s.split_left(NodeIndex::root(), 0.17, vec![Tab::Panel(PanelKind::Projects)]);
        let [docs_n, _] = s.split_right(docs_n, 0.78, vec![Tab::Panel(PanelKind::Properties)]);
        let _ = s.split_below(docs_n, 0.8, vec![Tab::Panel(PanelKind::Messages)]);
    }
    ds
}

fn all_tabs(ds: &DockState<Tab>) -> Vec<Tab> {
    ds.iter_all_tabs().map(|(_, t)| *t).collect()
}

/// Push a tab into the leaf that holds Documents (or the first leaf).
fn push_to_doc_leaf(ds: &mut DockState<Tab>, tab: Tab) {
    let doc_path = ds
        .iter_all_tabs()
        .find(|(p, t)| p.surface == SurfaceIndex::main() && matches!(t, Tab::Doc(_)))
        .map(|(p, _)| NodePath { surface: p.surface, node: p.node });
    match (doc_path, tab) {
        (Some(np), Tab::Doc(_)) => {
            ds.set_focused_node_and_surface(np);
            ds.push_to_focused_leaf(tab);
        }
        _ => ds.push_to_first_leaf(tab),
    }
}

impl App {
    fn new() -> Self {
        let shell = Shell::new("egui_dock 0.21 / egui 0.36");
        let main = build_main(shell.layout, vec![Tab::Doc(0), Tab::Doc(1), Tab::Doc(2)]);
        Self { shell, main, torn: vec![], next_window: 1 }
    }

    fn remove_anywhere(&mut self, tab: Tab) -> bool {
        if let Some(p) = self.main.find_tab(&tab) {
            self.main.remove_tab(p);
            return true;
        }
        for t in &mut self.torn {
            if let Some(p) = t.dock.find_tab(&tab) {
                t.dock.remove_tab(p);
                return true;
            }
        }
        false
    }

    fn tear_off(&mut self, tabs: Vec<Tab>, pos: Option<egui::Pos2>) {
        let id = egui::ViewportId::from_hash_of(("torn", self.next_window));
        self.next_window += 1;
        self.shell.log(format!("Tear-off → new OS window ({} tab(s))", tabs.len()));
        self.torn.push(Torn { id, dock: DockState::new(tabs), pos });
    }

    fn rebuild_for_layout(&mut self) {
        let docs: Vec<Tab> = all_tabs(&self.main).into_iter().filter(|t| matches!(t, Tab::Doc(_))).collect();
        self.main = build_main(self.shell.layout, docs);
        if self.shell.layout != Layout::Docked {
            for t in &mut self.torn {
                t.dock.retain_tabs(|t| matches!(t, Tab::Doc(_)));
            }
        }
    }

    /// egui_dock floating windows → native OS windows.
    fn promote_floating(ds: &mut DockState<Tab>, outer: Option<egui::Pos2>) -> Vec<(Vec<Tab>, Option<egui::Pos2>)> {
        let mut out = vec![];
        let idx: Vec<SurfaceIndex> = ds
            .iter_surfaces_indexed()
            .filter(|(_, s)| matches!(s, Surface::Window(_, ws) if !ws.dragged()))
            .map(|(i, _)| i)
            .collect();
        for si in idx {
            if let Some(Surface::Window(tree, ws)) = ds.get_surface(si) {
                let tabs: Vec<Tab> = tree.tabs().copied().collect();
                let pos = outer.map(|o| o + ws.rect().min.to_vec2());
                ds.remove_surface(si);
                out.push((tabs, pos));
            }
        }
        out
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shell.apply_theme(&ctx);
        if self.shell.layout_changed {
            self.shell.layout_changed = false;
            self.rebuild_for_layout();
        }
        for i in std::mem::take(&mut self.shell.open_requests) {
            let tab = Tab::Doc(i);
            self.shell.active_doc = Some(i);
            if let Some(p) = self.main.find_tab(&tab) {
                let _ = self.main.set_active_tab(p);
            } else if let Some(t) = self.torn.iter_mut().find(|t| t.dock.find_tab(&tab).is_some()) {
                let p = t.dock.find_tab(&tab).unwrap();
                let _ = t.dock.set_active_tab(p);
                ctx.send_viewport_cmd_to(t.id, egui::ViewportCommand::Focus);
            } else {
                push_to_doc_leaf(&mut self.main, tab);
            }
        }
        for p in std::mem::take(&mut self.shell.show_panel_requests) {
            let tab = Tab::Panel(p);
            let present = self.main.find_tab(&tab).is_some() || self.torn.iter().any(|t| t.dock.find_tab(&tab).is_some());
            if !present {
                let s = self.main.main_surface_mut();
                let _ = match p {
                    PanelKind::Projects => s.split_left(NodeIndex::root(), 0.17, vec![tab]),
                    PanelKind::Properties => s.split_right(NodeIndex::root(), 0.8, vec![tab]),
                    PanelKind::Messages => s.split_below(NodeIndex::root(), 0.8, vec![tab]),
                };
            }
        }

        self.shell.chrome_top(ui);
        self.shell.chrome_bottom(ui);
        self.shell.side_panels(ui);

        let mut actions = vec![];
        let central = egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            DockArea::new(&mut self.main)
                .id(egui::Id::new("main_dock"))
                .show_leaf_collapse_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut Viewer { shell: &mut self.shell, actions: &mut actions, in_torn: false });
        });
        let area = central.response.rect;
        self.shell.flyout_ui(&ctx, area);

        let outer = ctx.input(|i| i.viewport().inner_rect.or(i.viewport().outer_rect)).map(|r| r.min);
        for (tabs, pos) in Self::promote_floating(&mut self.main, outer) {
            self.tear_off(tabs, pos);
        }

        // Torn-off native windows, each with its own dock tree.
        let mut new_windows = vec![];
        {
            let Self { shell, torn, .. } = self;
            for (k, t) in torn.iter_mut().enumerate() {
                let title = all_tabs(&t.dock).iter().map(|x| shell.tab_title(x)).collect::<Vec<_>>().join(", ");
                let mut b = egui::ViewportBuilder::default().with_title(format!("REDA — {title}")).with_inner_size([960.0, 640.0]);
                if let Some(p) = t.pos {
                    b = b.with_position(p);
                }
                let id = t.id;
                ctx.show_viewport_immediate(id, b, |ui, _class| {
                    let kind = all_tabs(&t.dock)
                        .iter()
                        .find_map(|x| match x {
                            Tab::Doc(i) if shell.active_doc == Some(*i) => Some(shell.docs[*i].kind),
                            _ => None,
                        })
                        .or_else(|| all_tabs(&t.dock).iter().find_map(|x| if let Tab::Doc(i) = x { Some(shell.docs[*i].kind) } else { None }));
                    if shell.torn_off_window_has_menu {
                        egui::Panel::top("torn_menu").show(ui, |ui| egui::MenuBar::new().ui(ui, |ui| shell.menu_bar(ui)));
                    }
                    egui::Panel::top("torn_toolbar").show(ui, |ui| shell.toolbar(ui, kind));
                    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                        DockArea::new(&mut t.dock)
                            .id(egui::Id::new(("torn_dock", id)))
                            .show_leaf_collapse_buttons(false)
                            .show_leaf_close_all_buttons(false)
                            .show_inside(ui, &mut Viewer { shell, actions: &mut actions, in_torn: true });
                    });
                    if ui.input(|i| i.viewport().close_requested()) {
                        actions.push(Action::CloseWindow(k));
                    }
                    let o = ui.input(|i| i.viewport().inner_rect).map(|r| r.min);
                    new_windows.extend(Self::promote_floating(&mut t.dock, o));
                });
            }
        }
        for (tabs, pos) in new_windows {
            self.tear_off(tabs, pos);
        }

        if std::env::var("PROTO_TEAR").is_ok() && ctx.cumulative_frame_nr() == 5 {
            actions.push(Action::TearOff(Tab::Doc(2), None));
            actions.push(Action::TearOff(Tab::Panel(PanelKind::Properties), None));
        }
        for a in actions {
            match a {
                Action::TearOff(tab, pos) => {
                    if self.remove_anywhere(tab) {
                        self.tear_off(vec![tab], pos);
                    }
                }
                Action::DockBack(tab) => {
                    if self.remove_anywhere(tab) {
                        push_to_doc_leaf(&mut self.main, tab);
                    }
                }
                Action::CloseWindow(k) => {
                    if k < self.torn.len() {
                        for tab in all_tabs(&self.torn[k].dock) {
                            push_to_doc_leaf(&mut self.main, tab);
                        }
                        self.torn[k].dock = DockState::new(vec![]);
                        self.shell.log("Torn-off window closed; its tabs returned to the main window");
                    }
                }
            }
        }
        self.torn.retain(|t| t.dock.iter_all_tabs().next().is_some());

        self.shell.switcher(&ctx);
        if screenshot_hook(&ctx, &mut self.shell).is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if let Ok(path) = std::env::var("PROTO_SHOT")
            && let Some(img) = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            })
        {
            write_ppm(&path, &img);
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("REDA — prototype shell (egui_dock)").with_inner_size([1400.0, 860.0]),
        ..Default::default()
    };
    eframe::run_native("reda-prototype-shell-dock", options, Box::new(|_cc| Ok(Box::new(App::new()))))
}
