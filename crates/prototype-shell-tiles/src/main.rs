// PROTOTYPE — throwaway, lives only on the `prototype/main-window-shell` branch.
//
// REDA main-window shell on egui 0.37 + egui_tiles 0.18, with native OS-window tear-off built on
// egui's immediate viewports (neither docking crate supports it). Shares all shell UI with the
// egui_dock prototype through `common.rs`.
//
// Tear-off, three ways:
//   1. right-click a tab → "Move to new window";
//   2. drag a tab and release it outside the window (egui_tiles has no floating windows, so the
//      prototype watches the drag and tears off when the pointer is released off-window);
//   3. inside a torn-off window, right-click → "Move to main window", or close the window.
//
// Run: cargo run -p prototype-shell-tiles

#![allow(dead_code, deprecated)]

#[path = "../../prototype-shell-dock/src/common.rs"]
mod common;

use common::*;
use eframe::egui;
use egui_tiles::{Container, SimplificationOptions, Tile, TileId, Tiles, Tree, UiResponse};

struct Torn {
    id: egui::ViewportId,
    tree: Tree<Tab>,
    pos: Option<egui::Pos2>,
}

enum Action {
    TearOff(Tab, Option<egui::Pos2>),
    DockBack(Tab),
    CloseWindow(usize),
}

/// A drag in progress in one tree: which pane, and the last pointer position seen.
#[derive(Clone, Copy)]
struct DragWatch {
    tab: Tab,
    last: Option<egui::Pos2>,
    screen: egui::Rect,
    outer: Option<egui::Pos2>,
}

struct App {
    shell: Shell,
    main: Tree<Tab>,
    torn: Vec<Torn>,
    next_window: u64,
    main_drag: Option<DragWatch>,
}

struct Behavior<'a> {
    shell: &'a mut Shell,
    actions: &'a mut Vec<Action>,
    in_torn: bool,
}

impl egui_tiles::Behavior<Tab> for Behavior<'_> {
    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile_id: TileId, pane: &mut Tab) -> UiResponse {
        match *pane {
            Tab::Doc(i) => self.shell.doc_ui(ui, i),
            Tab::Panel(p) => {
                egui::Frame::new().inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.shell.panel_ui(ui, p));
                });
            }
        }
        UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &Tab) -> egui::WidgetText {
        self.shell.tab_title(pane).into()
    }

    fn is_tab_closable(&self, _tiles: &Tiles<Tab>, _tile_id: TileId) -> bool {
        true
    }

    fn on_tab_close(&mut self, tiles: &mut Tiles<Tab>, tile_id: TileId) -> bool {
        if let Some(Tab::Doc(i)) = tiles.get_pane(&tile_id)
            && self.shell.active_doc == Some(*i)
        {
            self.shell.active_doc = None;
        }
        true
    }

    fn on_tab_button(&mut self, tiles: &mut Tiles<Tab>, tile_id: TileId, response: egui::Response) -> egui::Response {
        let Some(tab) = tiles.get_pane(&tile_id).copied() else { return response };
        if (response.clicked() || response.drag_started())
            && let Tab::Doc(i) = tab
        {
            self.shell.active_doc = Some(i);
        }
        let in_torn = self.in_torn;
        let actions = &mut *self.actions;
        response.context_menu(|ui| {
            if in_torn && ui.button("Move to main window").clicked() {
                actions.push(Action::DockBack(tab));
            }
            if ui.button("Move to new window").clicked() {
                actions.push(Action::TearOff(tab, None));
            }
        });
        response
    }

    fn simplification_options(&self) -> SimplificationOptions {
        SimplificationOptions { all_panes_must_have_tabs: true, ..Default::default() }
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        2.0
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        26.0
    }
}

fn build_main(layout: Layout, docs: Vec<Tab>) -> Tree<Tab> {
    let mut tiles = Tiles::default();
    let doc_panes: Vec<TileId> = docs.into_iter().map(|d| tiles.insert_pane(d)).collect();
    let docs_tabs = tiles.insert_tab_tile(doc_panes);
    let root = if layout == Layout::Docked {
        let pj = tiles.insert_pane(Tab::Panel(PanelKind::Projects));
        let pr = tiles.insert_pane(Tab::Panel(PanelKind::Properties));
        let ms = tiles.insert_pane(Tab::Panel(PanelKind::Messages));
        let pj = tiles.insert_tab_tile(vec![pj]);
        let pr = tiles.insert_tab_tile(vec![pr]);
        let ms = tiles.insert_tab_tile(vec![ms]);
        let mid = tiles.insert_vertical_tile(vec![docs_tabs, ms]);
        if let Some(Tile::Container(Container::Linear(l))) = tiles.get_mut(mid) {
            l.shares.set_share(docs_tabs, 4.0);
            l.shares.set_share(ms, 1.0);
        }
        let root = tiles.insert_horizontal_tile(vec![pj, mid, pr]);
        if let Some(Tile::Container(Container::Linear(l))) = tiles.get_mut(root) {
            l.shares.set_share(pj, 1.0);
            l.shares.set_share(mid, 4.5);
            l.shares.set_share(pr, 1.2);
        }
        root
    } else {
        docs_tabs
    };
    Tree::new(egui::Id::new("main_tree"), root, tiles)
}

fn all_tabs(tree: &Tree<Tab>) -> Vec<Tab> {
    tree.tiles.tiles().filter_map(|t| if let Tile::Pane(p) = t { Some(*p) } else { None }).collect()
}

fn remove_tab(tree: &mut Tree<Tab>, tab: Tab) -> bool {
    match tree.tiles.find_pane(&tab) {
        Some(id) => {
            tree.remove_recursively(id);
            true
        }
        None => false,
    }
}

/// Push a tab into the Tabs container that holds Documents (or the root).
fn push_tab(tree: &mut Tree<Tab>, tab: Tab) {
    let pane = tree.tiles.insert_pane(tab);
    let doc_parent = if matches!(tab, Tab::Doc(_)) {
        let docs: Vec<TileId> =
            tree.tiles.iter().filter(|(_, t)| matches!(t, Tile::Pane(Tab::Doc(_)))).map(|(id, _)| *id).collect();
        docs.into_iter().filter_map(|id| tree.tiles.parent_of(id)).find(|&p| matches!(tree.tiles.get(p), Some(Tile::Container(Container::Tabs(_)))))
    } else {
        None
    };
    match doc_parent.or(tree.root) {
        Some(c) => match tree.tiles.get_mut(c) {
            Some(Tile::Container(Container::Tabs(t))) => {
                t.add_child(pane);
                t.set_active(pane);
            }
            Some(Tile::Container(c)) => {
                let tabs = Container::new_tabs(vec![pane]);
                let _ = c;
                let id = tree.tiles.insert_container(tabs);
                if let Some(Tile::Container(c)) = tree.tiles.get_mut(doc_parent.or(tree.root).unwrap()) {
                    c.add_child(id);
                }
            }
            _ => {
                let root = tree.tiles.insert_tab_tile(vec![pane]);
                tree.root = Some(root);
            }
        },
        None => {
            let root = tree.tiles.insert_tab_tile(vec![pane]);
            tree.root = Some(root);
        }
    }
}

/// Watches a tree's drag; returns a tab to tear off when the drag ends with the pointer outside
/// the window.
fn watch_drag(ctx: &egui::Context, tree: &Tree<Tab>, watch: &mut Option<DragWatch>) -> Option<(Tab, Option<egui::Pos2>)> {
    let dragged = tree.dragged_id(ctx).and_then(|id| tree.tiles.get_pane(&id).copied());
    let pos = ctx.input(|i| i.pointer.latest_pos());
    let screen = ctx.content_rect();
    let outer = ctx.input(|i| i.viewport().inner_rect).map(|r| r.min);
    match (dragged, watch.as_mut()) {
        (Some(tab), Some(w)) if w.tab == tab => {
            if pos.is_some() {
                w.last = pos;
            }
            None
        }
        (Some(tab), _) => {
            *watch = Some(DragWatch { tab, last: pos, screen, outer });
            None
        }
        (None, Some(_)) => {
            let w = watch.take().unwrap();
            let released_outside = w.last.is_some_and(|p| !w.screen.shrink(4.0).contains(p));
            released_outside.then(|| (w.tab, w.outer.zip(w.last).map(|(o, p)| o + p.to_vec2())))
        }
        (None, None) => None,
    }
}

impl App {
    fn new() -> Self {
        let shell = Shell::new("egui_tiles 0.18 / egui 0.37");
        let main = build_main(shell.layout, vec![Tab::Doc(0), Tab::Doc(1), Tab::Doc(2)]);
        Self { shell, main, torn: vec![], next_window: 1, main_drag: None }
    }

    fn remove_anywhere(&mut self, tab: Tab) -> bool {
        remove_tab(&mut self.main, tab) || self.torn.iter_mut().any(|t| remove_tab(&mut t.tree, tab))
    }

    fn tear_off(&mut self, tab: Tab, pos: Option<egui::Pos2>) {
        let n = self.next_window;
        self.next_window += 1;
        let id = egui::ViewportId::from_hash_of(("torn", n));
        self.shell.log(format!("Tear-off → new OS window ({})", self.shell.tab_title(&tab)));
        // Wayland ignores window positions; X11/Windows/macOS use it.
        self.torn.push(Torn { id, tree: Tree::new_tabs(egui::Id::new(("torn_tree", n)), vec![tab]), pos });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shell.apply_theme(&ctx);
        if self.shell.layout_changed {
            self.shell.layout_changed = false;
            let docs: Vec<Tab> = all_tabs(&self.main).into_iter().filter(|t| matches!(t, Tab::Doc(_))).collect();
            self.main = build_main(self.shell.layout, docs);
            if self.shell.layout != Layout::Docked {
                for t in &mut self.torn {
                    for p in PANELS {
                        remove_tab(&mut t.tree, Tab::Panel(p));
                    }
                }
            }
        }
        for i in std::mem::take(&mut self.shell.open_requests) {
            let tab = Tab::Doc(i);
            self.shell.active_doc = Some(i);
            if self.main.tiles.find_pane(&tab).is_some() {
                self.main.make_active(|_, t| matches!(t, Tile::Pane(p) if *p == tab));
            } else if let Some(t) = self.torn.iter_mut().find(|t| t.tree.tiles.find_pane(&tab).is_some()) {
                t.tree.make_active(|_, x| matches!(x, Tile::Pane(p) if *p == tab));
                ctx.send_viewport_cmd_to(t.id, egui::ViewportCommand::Focus);
            } else {
                push_tab(&mut self.main, tab);
            }
        }
        for p in std::mem::take(&mut self.shell.show_panel_requests) {
            let tab = Tab::Panel(p);
            let present = self.main.tiles.find_pane(&tab).is_some() || self.torn.iter().any(|t| t.tree.tiles.find_pane(&tab).is_some());
            if !present {
                push_tab(&mut self.main, tab);
            }
        }

        self.shell.chrome_top(ui);
        self.shell.chrome_bottom(ui);
        self.shell.side_panels(ui);

        let mut actions = vec![];
        let central = egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            self.main.ui(&mut Behavior { shell: &mut self.shell, actions: &mut actions, in_torn: false }, ui);
        });
        let area = central.response.rect;
        self.shell.flyout_ui(&ctx, area);
        if let Some((tab, pos)) = watch_drag(&ctx, &self.main, &mut self.main_drag) {
            actions.push(Action::TearOff(tab, pos));
        }

        {
            let Self { shell, torn, .. } = self;
            for (k, t) in torn.iter_mut().enumerate() {
                let title = all_tabs(&t.tree).iter().map(|x| shell.tab_title(x)).collect::<Vec<_>>().join(", ");
                let mut b = egui::ViewportBuilder::default().with_title(format!("REDA — {title}")).with_inner_size([960.0, 640.0]);
                if let Some(p) = t.pos {
                    b = b.with_position(p);
                }
                let id = t.id;
                ctx.show_viewport_immediate(id, b, |ui, _class| {
                    let tabs = all_tabs(&t.tree);
                    let kind = tabs
                        .iter()
                        .find_map(|x| match x {
                            Tab::Doc(i) if shell.active_doc == Some(*i) => Some(shell.docs[*i].kind),
                            _ => None,
                        })
                        .or_else(|| tabs.iter().find_map(|x| if let Tab::Doc(i) = x { Some(shell.docs[*i].kind) } else { None }));
                    if shell.torn_off_window_has_menu {
                        egui::Panel::top("torn_menu").show(ui, |ui| egui::MenuBar::new().ui(ui, |ui| shell.menu_bar(ui)));
                    }
                    egui::Panel::top("torn_toolbar").show(ui, |ui| shell.toolbar(ui, kind));
                    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                        t.tree.ui(&mut Behavior { shell, actions: &mut actions, in_torn: true }, ui);
                    });
                    if ui.input(|i| i.viewport().close_requested()) {
                        actions.push(Action::CloseWindow(k));
                    }
                    let key = egui::Id::new(("torn_drag", id));
                    let mut watch = ui.ctx().data(|d| d.get_temp::<DragState>(key)).and_then(|d| d.0);
                    if let Some((tab, pos)) = watch_drag(ui.ctx(), &t.tree, &mut watch) {
                        actions.push(Action::TearOff(tab, pos));
                    }
                    ui.ctx().data_mut(|d| d.insert_temp(key, DragState(watch)));
                });
            }
        }

        if std::env::var("PROTO_TEAR").is_ok() && ctx.cumulative_frame_nr() == 5 {
            actions.push(Action::TearOff(Tab::Doc(2), None));
            actions.push(Action::TearOff(Tab::Panel(PanelKind::Properties), None));
        }
        for a in actions {
            match a {
                Action::TearOff(tab, pos) => {
                    if self.remove_anywhere(tab) {
                        self.tear_off(tab, pos);
                    }
                }
                Action::DockBack(tab) => {
                    if self.remove_anywhere(tab) {
                        push_tab(&mut self.main, tab);
                    }
                }
                Action::CloseWindow(k) => {
                    if k < self.torn.len() {
                        for tab in all_tabs(&self.torn[k].tree) {
                            push_tab(&mut self.main, tab);
                        }
                        self.torn[k].tree = Tree::empty(egui::Id::new("dead"));
                        self.shell.log("Torn-off window closed; its tabs returned to the main window");
                    }
                }
            }
        }
        self.torn.retain(|t| !all_tabs(&t.tree).is_empty());

        self.shell.switcher(&ctx);
        if let Some(path) = screenshot_hook(&ctx, &mut self.shell) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| write_ppm(&path, &img))));
        }
    }
}

#[derive(Clone, Copy)]
struct DragState(Option<DragWatch>);

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("REDA — prototype shell (egui_tiles)").with_inner_size([1400.0, 860.0]),
        ..Default::default()
    };
    eframe::run_native("reda-prototype-shell-tiles", options, Box::new(|_cc| Ok(Box::new(App::new()))))
}
