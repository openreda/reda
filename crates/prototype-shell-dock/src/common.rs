// PROTOTYPE — throwaway, lives only on the `prototype/main-window-shell` branch.
//
// Shared shell code for the two prototype binaries. It is compiled twice: once by
// `prototype-shell-dock` (eframe/egui 0.36 + egui_dock) and once by `prototype-shell-tiles`
// (eframe/egui 0.37 + egui_tiles, via `#[path]`). It only touches APIs both egui versions share.
//
// Question: how should REDA's single-window shell look and behave, and which docking crate?
// Layout follows the professional integrated-suite pattern (grouped panels docked left/right/
// bottom, pin or auto-hide, a floating Active Bar over each Document, a Panels button).
// Three variants (switch with the floating bar or ←/→): A "Everything docks",
// B "Fixed edges", C "Pinned / auto-hide edges". Three themes: REDA dark, REDA light, stock egui.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2,
    vec2,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum DocKind {
    Schematic,
    Pcb,
    Library,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PanelKind {
    Projects,
    Properties,
    Messages,
}

pub const PANELS: [PanelKind; 3] = [PanelKind::Projects, PanelKind::Properties, PanelKind::Messages];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tab {
    Doc(usize),
    Panel(PanelKind),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    Docked,
    Edges,
    AutoHide,
}

impl Layout {
    pub const ALL: [Layout; 3] = [Layout::Docked, Layout::Edges, Layout::AutoHide];
    pub fn label(self) -> &'static str {
        match self {
            Layout::Docked => "A · Everything docks",
            Layout::Edges => "B · Fixed edges",
            Layout::AutoHide => "C · Pinned / auto-hide edges",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeChoice {
    RedaDark,
    RedaLight,
    StockEgui,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] =
        [ThemeChoice::RedaDark, ThemeChoice::RedaLight, ThemeChoice::StockEgui];
    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::RedaDark => "REDA dark",
            ThemeChoice::RedaLight => "REDA light",
            ThemeChoice::StockEgui => "stock egui",
        }
    }
}

pub struct Obj {
    pub designator: String,
    pub value: String,
    pub pos: Pos2,
    pub size: Vec2,
}

pub struct Doc {
    pub name: String,
    pub kind: DocKind,
    pub objects: Vec<Obj>,
    pub wires: Vec<(usize, usize)>,
    pub pan: Vec2,
    pub zoom: f32,
}

pub struct Shell {
    pub crate_name: &'static str,
    pub docs: Vec<Doc>,
    /// The Document whose menus and toolbar the main window shows: the last one interacted with,
    /// in any OS window.
    pub active_doc: Option<usize>,
    pub selection: Option<(usize, usize)>,
    pub messages: Vec<String>,
    pub layout: Layout,
    pub layout_changed: bool,
    pub theme: ThemeChoice,
    theme_dirty: bool,
    /// Edges layout: which side panels are shown.
    pub edge_visible: [bool; 3],
    /// Auto-hide layout: pinned panels stay docked at their edge; unpinned ones collapse to an
    /// edge tab and fly out over the Document when clicked.
    pub pinned: [bool; 3],
    pub flyout: Option<PanelKind>,
    /// Docked layout: panels the user asked to show again (from the Panels button).
    pub show_panel_requests: Vec<PanelKind>,
    /// Floating placement bar at the top of each Document canvas.
    pub active_bar: bool,
    /// Documents the user asked to open or focus (from the Projects panel); the crate places them.
    pub open_requests: Vec<usize>,
    pub cursor: Pos2,
    pub torn_off_window_has_menu: bool,
    /// The own-docking-layer prototype has its own variants, so it hides the Layout radios.
    pub layout_menu: bool,
}

impl Shell {
    pub fn new(crate_name: &'static str) -> Self {
        let r = |d: &str, v: &str, x: f32, y: f32| Obj {
            designator: d.into(),
            value: v.into(),
            pos: pos2(x, y),
            size: vec2(60.0, 24.0),
        };
        let u = |d: &str, v: &str, x: f32, y: f32| Obj {
            designator: d.into(),
            value: v.into(),
            pos: pos2(x, y),
            size: vec2(90.0, 120.0),
        };
        let fp = |d: &str, v: &str, x: f32, y: f32, w: f32, h: f32| Obj {
            designator: d.into(),
            value: v.into(),
            pos: pos2(x, y),
            size: vec2(w, h),
        };
        let docs = vec![
            Doc {
                name: "Power".into(),
                kind: DocKind::Schematic,
                objects: vec![
                    u("U1", "AP2112K-3.3", 220.0, 120.0),
                    r("C1", "1µF", 80.0, 160.0),
                    r("C2", "1µF", 400.0, 160.0),
                    r("R1", "100k", 220.0, 320.0),
                ],
                wires: vec![(0, 1), (0, 2), (0, 3)],
                pan: Vec2::ZERO,
                zoom: 1.0,
            },
            Doc {
                name: "MCU".into(),
                kind: DocKind::Schematic,
                objects: vec![
                    u("U2", "RP2040", 260.0, 140.0),
                    r("R2", "27Ω", 460.0, 140.0),
                    r("R3", "27Ω", 460.0, 200.0),
                    r("C3", "100nF", 100.0, 120.0),
                    r("C4", "100nF", 100.0, 180.0),
                    u("J1", "USB-C", 600.0, 140.0),
                ],
                wires: vec![(0, 1), (0, 2), (1, 5), (2, 5), (0, 3), (0, 4)],
                pan: Vec2::ZERO,
                zoom: 1.0,
            },
            Doc {
                name: "Board".into(),
                kind: DocKind::Pcb,
                objects: vec![
                    fp("U1", "SOT-23-5", 150.0, 120.0, 40.0, 30.0),
                    fp("U2", "QFN-56", 300.0, 160.0, 70.0, 70.0),
                    fp("C1", "0402", 110.0, 200.0, 20.0, 10.0),
                    fp("C2", "0402", 210.0, 200.0, 20.0, 10.0),
                    fp("R2", "0402", 420.0, 150.0, 20.0, 10.0),
                    fp("R3", "0402", 420.0, 180.0, 20.0, 10.0),
                    fp("J1", "USB-C-16P", 520.0, 160.0, 40.0, 60.0),
                ],
                wires: vec![(0, 1), (0, 2), (0, 3), (1, 4), (1, 5), (4, 6), (5, 6)],
                pan: Vec2::ZERO,
                zoom: 1.0,
            },
            Doc {
                name: "Starter Library".into(),
                kind: DocKind::Library,
                objects: vec![
                    u("Symbol", "Op-amp (dual)", 0.0, 0.0),
                    u("Symbol", "LDO 5-pin", 0.0, 0.0),
                    u("Footprint", "SOT-23-5", 0.0, 0.0),
                    u("Footprint", "R_0402", 0.0, 0.0),
                    u("Part", "AP2112K-3.3", 0.0, 0.0),
                ],
                wires: vec![],
                pan: Vec2::ZERO,
                zoom: 1.0,
            },
        ];
        Self {
            crate_name,
            docs,
            active_doc: Some(0),
            selection: None,
            messages: vec![format!("Prototype shell running on {crate_name}.")],
            layout: Layout::Docked,
            layout_changed: true,
            theme: ThemeChoice::RedaDark,
            theme_dirty: true,
            edge_visible: [true, true, true],
            pinned: [true, false, false],
            flyout: None,
            show_panel_requests: vec![],
            active_bar: true,
            open_requests: vec![],
            cursor: Pos2::ZERO,
            torn_off_window_has_menu: false,
            layout_menu: true,
        }
    }

    pub fn log(&mut self, s: impl Into<String>) {
        self.messages.push(s.into());
    }

    fn cmd(&mut self, id: &str) {
        self.log(format!("▶ {id}"));
    }

    pub fn tab_title(&self, tab: &Tab) -> String {
        match tab {
            Tab::Doc(i) => {
                let d = &self.docs[*i];
                let ext = match d.kind {
                    DocKind::Schematic => "redasch",
                    DocKind::Pcb => "redapcb",
                    DocKind::Library => "lib",
                };
                let icon = match d.kind {
                    DocKind::Schematic => "📄",
                    DocKind::Pcb => "🔲",
                    DocKind::Library => "📚",
                };
                format!("{icon} {}.{ext}", d.name)
            }
            Tab::Panel(p) => panel_name(*p).into(),
        }
    }

    pub fn active_kind(&self) -> Option<DocKind> {
        self.active_doc.map(|i| self.docs[i].kind)
    }

    // ---------------------------------------------------------------- theme

    pub fn apply_theme(&mut self, ctx: &egui::Context) {
        if !self.theme_dirty {
            return;
        }
        self.theme_dirty = false;
        match self.theme {
            ThemeChoice::StockEgui => {
                ctx.set_theme(egui::Theme::Dark);
                ctx.set_style_of(egui::Theme::Dark, egui::Style::default());
            }
            ThemeChoice::RedaDark => {
                ctx.set_theme(egui::Theme::Dark);
                ctx.set_style_of(egui::Theme::Dark, reda_style(true));
            }
            ThemeChoice::RedaLight => {
                ctx.set_theme(egui::Theme::Light);
                ctx.set_style_of(egui::Theme::Light, reda_style(false));
            }
        }
    }

    // ---------------------------------------------------------------- chrome

    /// Menu bar + toolbar for the main window. Both adapt to the active Document's type.
    pub fn chrome_top(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("reda_menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| self.menu_bar(ui));
        });
        let kind = self.active_kind();
        egui::Panel::top("reda_toolbar").show(ui, |ui| self.toolbar(ui, kind));
    }

    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let kind = self.active_kind();
        ui.menu_button("File", |ui| {
            for (l, c) in [
                ("New Schematic Sheet", "project.sheet.new"),
                ("New PCB Document", "project.pcb.new"),
                ("Open Project…", "file.project.open"),
                ("Save", "file.document.save"),
                ("Save All", "file.project.save_all"),
            ] {
                if ui.button(l).clicked() {
                    self.cmd(c);
                }
            }
            ui.separator();
            if kind == Some(DocKind::Schematic) && ui.button("Export PDF…").clicked() {
                self.cmd("schematic.export.pdf");
            }
            if kind == Some(DocKind::Pcb) && ui.button("Run Output Job…").clicked() {
                self.cmd("output.job.run");
            }
        });
        ui.menu_button("Edit", |ui| {
            for (l, c) in [
                ("Undo", "edit.undo"),
                ("Redo", "edit.redo"),
                ("Select Similar…", "edit.select.similar"),
                ("Delete", "edit.selection.delete"),
            ] {
                if ui.button(l).clicked() {
                    self.cmd(c);
                }
            }
        });
        ui.menu_button("View", |ui| {
            if self.layout_menu {
                ui.label("Layout");
                for l in Layout::ALL {
                    if ui.radio(self.layout == l, l.label()).clicked() {
                        self.set_layout(l);
                    }
                }
                ui.separator();
            }
            ui.label("Theme");
            for t in ThemeChoice::ALL {
                if ui.radio(self.theme == t, t.label()).clicked() {
                    self.set_theme(t);
                }
            }
            ui.separator();
            if self.layout == Layout::Edges {
                for (i, p) in PANELS.iter().enumerate() {
                    ui.checkbox(&mut self.edge_visible[i], panel_name(*p));
                }
                ui.separator();
            }
            ui.checkbox(&mut self.active_bar, "Active Bar on Documents");
            ui.checkbox(&mut self.torn_off_window_has_menu, "Torn-off windows get a menu bar");
            if kind.is_some() && ui.button("Fit Document").clicked() {
                self.cmd("view.fit");
            }
        });
        match kind {
            Some(DocKind::Schematic) => {
                self.menu(ui, "Place", &[
                    ("Part…", "schematic.component.place"),
                    ("Wire", "schematic.wire.place"),
                    ("Net Label", "schematic.net_label.place"),
                    ("Port", "schematic.port.place"),
                    ("Power Port", "schematic.power_port.place"),
                ]);
                self.menu(ui, "Design", &[
                    ("Re-annotate…", "schematic.designators.reannotate"),
                    ("Run ERC", "erc.run"),
                    ("Update PCB Document…", "sync.preview"),
                ]);
            }
            Some(DocKind::Pcb) => {
                self.menu(ui, "Place", &[
                    ("Track", "pcb.track.place"),
                    ("Via", "pcb.via.place"),
                    ("Zone", "pcb.zone.place"),
                    ("Keepout", "pcb.keepout.place"),
                    ("Free Pad", "pcb.pad.place"),
                    ("Text", "pcb.text.place"),
                ]);
                self.menu(ui, "Design", &[
                    ("Board Outline…", "pcb.board_outline.edit"),
                    ("Net Classes…", "pcb.net_class.edit"),
                    ("Rules…", "pcb.rules.edit"),
                    ("Import Changes from Schematic…", "sync.preview"),
                ]);
                self.menu(ui, "Route", &[
                    ("Interactive Route", "pcb.route.interactive"),
                    ("Refill Zones", "pcb.zone.refill"),
                ]);
                self.menu(ui, "Tools", &[("Run DRC", "drc.run"), ("Arrange in Rectangle", "pcb.footprint.arrange")]);
            }
            Some(DocKind::Library) => {
                self.menu(ui, "Library", &[
                    ("New Symbol", "library.symbol.new"),
                    ("New Footprint", "library.footprint.new"),
                    ("Footprint Generator…", "library.footprint.generate"),
                    ("New Part", "library.part.new"),
                    ("Import Part…", "library.part.import"),
                ]);
            }
            None => {}
        }
        ui.menu_button("Window", |ui| {
            for (i, d) in self.docs.iter().enumerate() {
                if ui.button(&d.name).clicked() {
                    self.open_requests.push(i);
                }
            }
        });
        ui.menu_button("Help", |ui| {
            if ui.button("Command list…").clicked() {
                self.cmd("help.commands");
            }
        });
    }

    fn menu(&mut self, ui: &mut egui::Ui, title: &str, items: &[(&str, &str)]) {
        ui.menu_button(title, |ui| {
            for (l, c) in items {
                if ui.button(*l).clicked() {
                    self.cmd(c);
                }
            }
        });
    }

    pub fn toolbar(&mut self, ui: &mut egui::Ui, kind: Option<DocKind>) {
        ui.horizontal(|ui| {
            for (l, c) in [("💾", "file.document.save"), ("⟲", "edit.undo"), ("⟳", "edit.redo")] {
                if ui.button(l).on_hover_text(c).clicked() {
                    self.cmd(c);
                }
            }
            ui.separator();
            let items: &[(&str, &str)] = match kind {
                Some(DocKind::Schematic) => &[
                    ("Part", "schematic.component.place"),
                    ("Wire", "schematic.wire.place"),
                    ("Net Label", "schematic.net_label.place"),
                    ("Port", "schematic.port.place"),
                    ("Power Port", "schematic.power_port.place"),
                    ("ERC", "erc.run"),
                ],
                Some(DocKind::Pcb) => &[
                    ("Track", "pcb.track.place"),
                    ("Via", "pcb.via.place"),
                    ("Zone", "pcb.zone.place"),
                    ("Keepout", "pcb.keepout.place"),
                    ("DRC", "drc.run"),
                    ("Sync", "sync.preview"),
                ],
                Some(DocKind::Library) => &[
                    ("New Symbol", "library.symbol.new"),
                    ("New Footprint", "library.footprint.new"),
                    ("Generate", "library.footprint.generate"),
                    ("Import Part", "library.part.import"),
                ],
                None => &[],
            };
            for (l, c) in items {
                if ui.button(*l).on_hover_text(*c).clicked() {
                    self.cmd(c);
                }
            }
            if kind == Some(DocKind::Pcb) {
                ui.separator();
                egui::ComboBox::from_id_salt("layer")
                    .selected_text("F.Cu")
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        let _ = ui.selectable_label(true, "F.Cu");
                        let _ = ui.selectable_label(false, "B.Cu");
                    });
            }
        });
    }

    pub fn chrome_bottom(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("reda_status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let k = match self.active_kind() {
                    Some(DocKind::Schematic) => "Schematic Sheet",
                    Some(DocKind::Pcb) => "PCB Document",
                    Some(DocKind::Library) => "Library",
                    None => "no Document",
                };
                ui.label(format!("Active: {k}"));
                ui.separator();
                ui.label(format!("X {:>7.2} mm  Y {:>7.2} mm", self.cursor.x / 10.0, self.cursor.y / 10.0));
                ui.separator();
                ui.label("Grid 2.54 mm");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.panels_button(ui);
                    ui.separator();
                    ui.weak(format!("prototype · {}", self.crate_name));
                });
            });
        });
    }

    /// For layouts B and C: side and bottom panels around the dock area. Call before the
    /// CentralPanel that hosts the dock.
    pub fn side_panels(&mut self, ui: &mut egui::Ui) {
        match self.layout {
            Layout::Docked => {}
            Layout::Edges => {
                if self.edge_visible[0] {
                    egui::Panel::left("edge_projects").resizable(true).default_size(200.0).show(ui, |ui| {
                        panel_header(ui, "Projects");
                        self.panel_ui(ui, PanelKind::Projects);
                    });
                }
                if self.edge_visible[1] {
                    egui::Panel::right("edge_properties").resizable(true).default_size(240.0).show(ui, |ui| {
                        panel_header(ui, "Properties");
                        self.panel_ui(ui, PanelKind::Properties);
                    });
                }
                if self.edge_visible[2] {
                    egui::Panel::bottom("edge_messages").resizable(true).default_size(120.0).show(ui, |ui| {
                        panel_header(ui, "Messages");
                        self.panel_ui(ui, PanelKind::Messages);
                    });
                }
            }
            Layout::AutoHide => {
                for (i, p) in PANELS.iter().enumerate() {
                    if self.pinned[i] {
                        let mut unpin = false;
                        let mut body = |ui: &mut egui::Ui, this: &mut Self| {
                            ui.horizontal(|ui| {
                                panel_header_inline(ui, panel_name(*p));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("📌").on_hover_text("Unpin: collapse to edge tab").clicked() {
                                        unpin = true;
                                    }
                                });
                            });
                            ui.separator();
                            this.panel_ui(ui, *p);
                        };
                        match p {
                            PanelKind::Projects => {
                                egui::Panel::left("ah_projects").resizable(true).default_size(210.0).show(ui, |ui| body(ui, self));
                            }
                            PanelKind::Properties => {
                                egui::Panel::right("ah_properties").resizable(true).default_size(240.0).show(ui, |ui| body(ui, self));
                            }
                            PanelKind::Messages => {
                                egui::Panel::bottom("ah_messages").resizable(true).default_size(120.0).show(ui, |ui| body(ui, self));
                            }
                        }
                        if unpin {
                            self.pinned[i] = false;
                        }
                    } else {
                        let (panel, vertical) = match p {
                            PanelKind::Projects => (egui::Panel::left("ah_strip_l").exact_size(22.0), true),
                            PanelKind::Properties => (egui::Panel::right("ah_strip_r").exact_size(22.0), true),
                            PanelKind::Messages => (egui::Panel::bottom("ah_strip_b").exact_size(22.0), false),
                        };
                        panel.resizable(false).show(ui, |ui| {
                            if edge_tab(ui, panel_name(*p), vertical, self.flyout == Some(*p)).clicked() {
                                self.flyout = if self.flyout == Some(*p) { None } else { Some(*p) };
                            }
                        });
                    }
                }
            }
        }
    }

    /// Auto-hide layout: the open flyout, drawn over the Document area. Call after the
    /// CentralPanel so it sits on top.
    pub fn flyout_ui(&mut self, ctx: &egui::Context, area: Rect) {
        if self.layout != Layout::AutoHide {
            return;
        }
        let Some(p) = self.flyout else { return };
        let i = PANELS.iter().position(|x| *x == p).unwrap_or(0);
        if self.pinned[i] {
            self.flyout = None;
            return;
        }
        let r = match p {
            PanelKind::Projects => Rect::from_min_size(area.min, vec2(240.0, area.height())),
            PanelKind::Properties => Rect::from_min_max(pos2(area.max.x - 260.0, area.min.y), area.max),
            PanelKind::Messages => Rect::from_min_max(pos2(area.min.x, area.max.y - 160.0), area.max),
        };
        let mut pin = false;
        let resp = egui::Area::new(egui::Id::new("flyout"))
            .fixed_pos(r.min)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::window(ui.style()).corner_radius(CornerRadius::ZERO).show(ui, |ui| {
                    ui.set_min_size(r.size() - vec2(14.0, 14.0));
                    ui.set_max_size(r.size() - vec2(14.0, 14.0));
                    ui.horizontal(|ui| {
                        panel_header_inline(ui, panel_name(p));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("📌").on_hover_text("Pin to edge").clicked() {
                                pin = true;
                            }
                        });
                    });
                    ui.separator();
                    self.panel_ui(ui, p);
                });
            });
        if pin {
            self.pinned[i] = true;
            self.flyout = None;
        } else if resp.response.clicked_elsewhere() && !ctx.is_being_dragged(egui::Id::NULL) {
            // Clicking the Document (or anywhere else) slides the panel away again.
            let strip_click = ctx.input(|inp| inp.pointer.interact_pos()).is_some_and(|pt| !area.contains(pt));
            if !strip_click {
                self.flyout = None;
            }
        }
    }

    /// Status-bar "Panels" button: show any panel again, whatever the layout.
    fn panels_button(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Panels ▴", |ui| {
            for (i, p) in PANELS.iter().enumerate() {
                if ui.button(panel_name(*p)).clicked() {
                    match self.layout {
                        Layout::Docked => self.show_panel_requests.push(*p),
                        Layout::Edges => self.edge_visible[i] = true,
                        Layout::AutoHide => {
                            if !self.pinned[i] {
                                self.flyout = Some(*p);
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn set_layout(&mut self, l: Layout) {
        if self.layout != l {
            self.layout = l;
            self.layout_changed = true;
            self.log(format!("Layout → {}", l.label()));
        }
    }

    pub fn set_theme(&mut self, t: ThemeChoice) {
        self.theme = t;
        self.theme_dirty = true;
    }

    /// Floating prototype switcher, visually distinct from the design being judged.
    pub fn switcher(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused().is_some());
        if !typing {
            let (l, r) = ctx.input(|i| (i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight)));
            let idx = Layout::ALL.iter().position(|x| *x == self.layout).unwrap_or(0);
            if l {
                self.set_layout(Layout::ALL[(idx + 2) % 3]);
            }
            if r {
                self.set_layout(Layout::ALL[(idx + 1) % 3]);
            }
        }
        egui::Area::new(egui::Id::new("proto_switcher"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -34.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(250, 204, 21))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .stroke(Stroke::new(1.0, Color32::BLACK))
                    .show(ui, |ui| {
                        ui.visuals_mut().override_text_color = Some(Color32::BLACK);
                        ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::from_rgb(253, 224, 71);
                        ui.visuals_mut().widgets.hovered.weak_bg_fill = Color32::from_rgb(254, 240, 138);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("PROTOTYPE").strong());
                            let idx = Layout::ALL.iter().position(|x| *x == self.layout).unwrap_or(0);
                            if ui.button("◀").clicked() {
                                self.set_layout(Layout::ALL[(idx + 2) % 3]);
                            }
                            ui.label(egui::RichText::new(self.layout.label()).strong());
                            if ui.button("▶").clicked() {
                                self.set_layout(Layout::ALL[(idx + 1) % 3]);
                            }
                            ui.separator();
                            for t in ThemeChoice::ALL {
                                if ui.selectable_label(self.theme == t, t.label()).clicked() {
                                    self.set_theme(t);
                                }
                            }
                            ui.separator();
                            ui.label(self.crate_name);
                        });
                    });
            });
    }

    // ---------------------------------------------------------------- panels

    pub fn panel_ui(&mut self, ui: &mut egui::Ui, p: PanelKind) {
        match p {
            PanelKind::Projects => {
                egui::CollapsingHeader::new("🗁 Demo.reda").default_open(true).show(ui, |ui| {
                    for i in 0..self.docs.len() {
                        if self.docs[i].kind == DocKind::Library {
                            continue;
                        }
                        let title = self.tab_title(&Tab::Doc(i));
                        if ui.selectable_label(self.active_doc == Some(i), title).clicked() {
                            self.open_requests.push(i);
                        }
                    }
                    ui.weak("Output Jobs");
                    ui.indent("jobs", |ui| {
                        if ui.selectable_label(false, "⚙ JLCPCB 2-layer").clicked() {
                            self.cmd("output.job.run");
                        }
                    });
                });
                egui::CollapsingHeader::new("📚 Libraries").default_open(true).show(ui, |ui| {
                    for i in 0..self.docs.len() {
                        if self.docs[i].kind == DocKind::Library {
                            let title = self.tab_title(&Tab::Doc(i));
                            if ui.selectable_label(self.active_doc == Some(i), title).clicked() {
                                self.open_requests.push(i);
                            }
                        }
                    }
                });
            }
            PanelKind::Properties => match self.selection {
                Some((d, o)) if d < self.docs.len() && o < self.docs[d].objects.len() => {
                    let kind = self.docs[d].kind;
                    let obj = &mut self.docs[d].objects[o];
                    ui.weak(match kind {
                        DocKind::Schematic => "Component (Unit A)",
                        DocKind::Pcb => "Footprint",
                        DocKind::Library => "Library element",
                    });
                    egui::Grid::new("props").num_columns(2).striped(true).show(ui, |ui| {
                        ui.label("Designator");
                        ui.text_edit_singleline(&mut obj.designator);
                        ui.end_row();
                        ui.label("Value");
                        ui.text_edit_singleline(&mut obj.value);
                        ui.end_row();
                        ui.label("X");
                        ui.add(egui::DragValue::new(&mut obj.pos.x).suffix(" px"));
                        ui.end_row();
                        ui.label("Y");
                        ui.add(egui::DragValue::new(&mut obj.pos.y).suffix(" px"));
                        ui.end_row();
                        if kind == DocKind::Pcb {
                            ui.label("Layer");
                            ui.label("Top");
                            ui.end_row();
                        }
                    });
                }
                _ => {
                    ui.weak("Nothing selected. Click an object in a Document.");
                    if let Some(k) = self.active_kind() {
                        ui.separator();
                        ui.label(match k {
                            DocKind::Schematic => "Sheet: A4, grid 2.54 mm",
                            DocKind::Pcb => "Board: 2 layers, 50 × 30 mm",
                            DocKind::Library => "Library: read-only starter",
                        });
                    }
                }
            },
            PanelKind::Messages => {
                ui.horizontal(|ui| {
                    if ui.small_button("Clear").clicked() {
                        self.messages.clear();
                    }
                    ui.weak(format!("{} messages", self.messages.len()));
                });
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                    for m in &self.messages {
                        ui.monospace(m);
                    }
                });
            }
        }
    }

    // ---------------------------------------------------------------- documents

    pub fn doc_ui(&mut self, ui: &mut egui::Ui, idx: usize) {
        let kind = self.docs[idx].kind;
        if kind == DocKind::Library {
            self.library_ui(ui, idx);
            return;
        }
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = resp.rect;
        if resp.is_pointer_button_down_on() || resp.clicked() {
            self.active_doc = Some(idx);
        }
        let dark = ui.visuals().dark_mode;
        let doc = &mut self.docs[idx];
        // Pan (middle or right drag) and zoom (scroll).
        if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) {
            doc.pan += resp.drag_delta();
        }
        if resp.hovered() {
            let s = ui.input(|i| i.smooth_scroll_delta.y);
            if s != 0.0 {
                doc.zoom = (doc.zoom * (1.0 + s * 0.002)).clamp(0.2, 8.0);
            }
        }
        let z = doc.zoom;
        let origin = rect.min + doc.pan;
        let to_screen = |p: Pos2| origin + p.to_vec2() * z;
        if let Some(h) = resp.hover_pos() {
            self.cursor = ((h - origin) / z).to_pos2();
        }

        let (bg, grid, line, body, text, sel) = match (kind, dark) {
            (DocKind::Pcb, _) => (
                Color32::from_rgb(12, 14, 16),
                Color32::from_rgb(34, 38, 44),
                Color32::from_rgb(200, 52, 52),
                Color32::from_rgb(200, 160, 60),
                Color32::from_rgb(230, 230, 230),
                Color32::from_rgb(255, 255, 255),
            ),
            (_, true) => (
                Color32::from_rgb(24, 26, 29),
                Color32::from_rgb(48, 52, 58),
                Color32::from_rgb(90, 190, 120),
                Color32::from_rgb(60, 110, 170),
                Color32::from_rgb(220, 224, 230),
                Color32::from_rgb(255, 200, 60),
            ),
            (_, false) => (
                Color32::from_rgb(252, 251, 247),
                Color32::from_rgb(220, 220, 215),
                Color32::from_rgb(20, 120, 60),
                Color32::from_rgb(150, 30, 30),
                Color32::from_rgb(30, 30, 30),
                Color32::from_rgb(220, 120, 0),
            ),
        };
        painter.rect_filled(rect, CornerRadius::ZERO, bg);
        let step = 25.4 * z;
        if step > 6.0 {
            let mut x = rect.min.x + (origin.x - rect.min.x).rem_euclid(step);
            while x < rect.max.x {
                let mut y = rect.min.y + (origin.y - rect.min.y).rem_euclid(step);
                while y < rect.max.y {
                    painter.circle_filled(pos2(x, y), 1.0, grid);
                    y += step;
                }
                x += step;
            }
        }
        if kind == DocKind::Pcb {
            let outline = Rect::from_min_size(to_screen(pos2(60.0, 60.0)), vec2(540.0, 260.0) * z);
            painter.rect_filled(outline, CornerRadius::same(4), Color32::from_rgb(18, 40, 26));
            painter.rect_stroke(outline, CornerRadius::same(4), Stroke::new(2.0, Color32::from_rgb(230, 210, 80)), StrokeKind::Outside);
        }
        for &(a, b) in &doc.wires {
            let pa = to_screen(doc.objects[a].pos + doc.objects[a].size / 2.0);
            let pb = to_screen(doc.objects[b].pos + doc.objects[b].size / 2.0);
            let mid = pos2(pb.x, pa.y);
            let w = if kind == DocKind::Pcb { 3.0 * z } else { 1.5 };
            painter.line_segment([pa, mid], Stroke::new(w, line));
            painter.line_segment([mid, pb], Stroke::new(w, line));
            if kind == DocKind::Pcb {
                painter.circle_filled(mid, 4.0 * z, Color32::from_rgb(180, 180, 180));
                painter.circle_filled(mid, 1.8 * z, bg);
            }
        }
        let mut hit = None;
        for (i, o) in doc.objects.iter().enumerate() {
            let r = Rect::from_min_size(to_screen(o.pos), o.size * z);
            let selected = self.selection == Some((idx, i));
            if kind == DocKind::Pcb {
                // two pads per side for a rough footprint look
                let pad = vec2(o.size.x * 0.3, o.size.y * 0.3) * z;
                for c in [r.left_top(), r.right_top() - vec2(pad.x, 0.0), r.left_bottom() - vec2(0.0, pad.y), r.right_bottom() - pad] {
                    painter.rect_filled(Rect::from_min_size(c, pad), CornerRadius::same(1), body);
                }
                painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, Color32::from_rgb(200, 200, 200)), StrokeKind::Middle);
            } else {
                painter.rect_filled(r, CornerRadius::same(2), body.gamma_multiply(0.25));
                painter.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.5, body), StrokeKind::Middle);
            }
            if selected {
                painter.rect_stroke(r.expand(3.0), CornerRadius::same(2), Stroke::new(1.5, sel), StrokeKind::Outside);
            }
            painter.text(r.center_top() - vec2(0.0, 2.0), Align2::CENTER_BOTTOM, &o.designator, FontId::proportional(12.0 * z.sqrt()), text);
            painter.text(r.center_bottom() + vec2(0.0, 2.0), Align2::CENTER_TOP, &o.value, FontId::proportional(10.0 * z.sqrt()), text.gamma_multiply(0.7));
            if let Some(p) = resp.interact_pointer_pos()
                && r.contains(p)
            {
                hit = Some(i);
            }
        }
        if resp.clicked() || resp.drag_started_by(egui::PointerButton::Primary) {
            self.selection = hit.map(|i| (idx, i));
        }
        if self.active_bar {
            self.active_bar_ui(ui, rect, kind);
        }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let Some((d, o)) = self.selection
            && d == idx
        {
            let doc = &mut self.docs[idx];
            doc.objects[o].pos += resp.drag_delta() / z;
        }
    }

    /// Compact floating placement bar centred at the top of a Document canvas.
    fn active_bar_ui(&mut self, ui: &mut egui::Ui, canvas: Rect, kind: DocKind) {
        let items: &[(&str, &str)] = match kind {
            DocKind::Schematic => &[
                ("Sel", "edit.select.rect"),
                ("Part", "schematic.component.place"),
                ("Wire", "schematic.wire.place"),
                ("Net", "schematic.net_label.place"),
                ("Port", "schematic.port.place"),
                ("Pwr", "schematic.power_port.place"),
                ("Text", "schematic.text.place"),
            ],
            DocKind::Pcb => &[
                ("Sel", "edit.select.rect"),
                ("Arr", "pcb.footprint.arrange"),
                ("Route", "pcb.route.interactive"),
                ("Via", "pcb.via.place"),
                ("Zone", "pcb.zone.place"),
                ("Keep", "pcb.keepout.place"),
                ("Text", "pcb.text.place"),
            ],
            DocKind::Library => &[],
        };
        let w = items.len() as f32 * 46.0 + 12.0;
        let bar = Rect::from_center_size(pos2(canvas.center().x, canvas.top() + 20.0), vec2(w, 30.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(egui::Margin::symmetric(4, 3))
                .corner_radius(CornerRadius::same(4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (l, c) in items {
                            if ui.add(egui::Button::new(*l).min_size(vec2(40.0, 20.0))).on_hover_text(*c).clicked() {
                                self.active_doc = Some(self.active_doc.unwrap_or(0));
                                self.cmd(c);
                            }
                        }
                    });
                });
        });
    }

    fn library_ui(&mut self, ui: &mut egui::Ui, idx: usize) {
        let resp = ui.interact(ui.max_rect(), ui.id().with("libbg"), Sense::click());
        if resp.is_pointer_button_down_on() {
            self.active_doc = Some(idx);
        }
        let sel = match self.selection {
            Some((d, o)) if d == idx => Some(o),
            _ => None,
        };
        let mut clicked = None;
        egui::Panel::left(egui::Id::new("liblist").with(idx)).resizable(true).default_size(200.0).show(ui, |ui| {
            for (i, o) in self.docs[idx].objects.iter().enumerate() {
                if ui.selectable_label(sel == Some(i), format!("{} · {}", o.designator, o.value)).clicked() {
                    clicked = Some(i);
                }
            }
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let (r, p) = ui.allocate_painter(ui.available_size(), Sense::click());
            if r.is_pointer_button_down_on() {
                clicked = clicked.or(sel).or(Some(0));
            }
            p.rect_filled(r.rect, CornerRadius::ZERO, ui.visuals().extreme_bg_color);
            if let Some(o) = sel {
                let o = &self.docs[idx].objects[o];
                let b = Rect::from_center_size(r.rect.center(), vec2(160.0, 200.0));
                p.rect_stroke(b, CornerRadius::same(2), Stroke::new(2.0, ui.visuals().strong_text_color()), StrokeKind::Middle);
                for k in 0..4 {
                    let y = b.top() + 30.0 + k as f32 * 40.0;
                    p.line_segment([pos2(b.left() - 30.0, y), pos2(b.left(), y)], Stroke::new(1.5, ui.visuals().text_color()));
                    p.line_segment([pos2(b.right(), y), pos2(b.right() + 30.0, y)], Stroke::new(1.5, ui.visuals().text_color()));
                }
                p.text(b.center(), Align2::CENTER_CENTER, &o.value, FontId::proportional(14.0), ui.visuals().strong_text_color());
            } else {
                p.text(r.rect.center(), Align2::CENTER_CENTER, "Select a Library element", FontId::proportional(14.0), ui.visuals().weak_text_color());
            }
        });
        if let Some(i) = clicked {
            self.active_doc = Some(idx);
            self.selection = Some((idx, i));
        }
    }
}

pub fn panel_name(p: PanelKind) -> &'static str {
    match p {
        PanelKind::Projects => "Projects",
        PanelKind::Properties => "Properties",
        PanelKind::Messages => "Messages",
    }
}

fn panel_header_inline(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title.to_uppercase()).small().strong().color(ui.visuals().weak_text_color()));
}

/// A collapsed panel's tab on the window edge; vertical on the left and right edges.
fn edge_tab(ui: &mut egui::Ui, title: &str, vertical: bool, open: bool) -> egui::Response {
    let font = FontId::proportional(11.5);
    let color = if open { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
    let galley = ui.painter().layout_no_wrap(title.to_owned(), font, color);
    let len = galley.size().x + 16.0;
    let size = if vertical { vec2(ui.available_width(), len) } else { vec2(len, ui.available_height()) };
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if open || resp.hovered() { ui.visuals().widgets.hovered.bg_fill } else { ui.visuals().widgets.inactive.bg_fill };
    ui.painter().rect_filled(rect, CornerRadius::same(2), fill);
    if vertical {
        let pos = pos2(rect.center().x + galley.size().y / 2.0, rect.top() + 8.0);
        ui.painter().add(egui::epaint::TextShape::new(pos, galley, color).with_angle(std::f32::consts::FRAC_PI_2));
    } else {
        ui.painter().galley(rect.left_top() + vec2(8.0, (rect.height() - galley.size().y) / 2.0), galley, color);
    }
    resp
}

fn panel_header(ui: &mut egui::Ui, title: &str) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new(title.to_uppercase()).small().strong().color(ui.visuals().weak_text_color()));
    ui.separator();
}

// ---------------------------------------------------------------- REDA theme

fn rgb(h: u32) -> Color32 {
    Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}

/// A denser, quieter look than stock egui: neutral greys, one blue accent, small radii,
/// no drop shadows, tighter spacing and slightly smaller text.
pub fn reda_style(dark: bool) -> egui::Style {
    let mut s = egui::Style::default();
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    // (panel, window, extreme, faint, widget, hover, active, stroke, text, strong, accent)
    let p = if dark {
        [0x1d2024, 0x23272c, 0x141619, 0x24282d, 0x2b3036, 0x343a41, 0x3d444c, 0x383e45, 0xc4cad3, 0xeef1f4, 0x3d8fd6]
    } else {
        [0xeceef1, 0xf7f8fa, 0xffffff, 0xe4e7eb, 0xdfe3e8, 0xd2d7de, 0xc5ccd5, 0xc3c9d1, 0x2a2f36, 0x0d1014, 0x1f6fb8]
    };
    let [panel, window, extreme, faint, widget, hover, active, stroke, text, strong, accent] = p.map(rgb);
    v.panel_fill = panel;
    v.window_fill = window;
    v.extreme_bg_color = extreme;
    v.faint_bg_color = faint;
    v.code_bg_color = faint;
    v.hyperlink_color = accent;
    v.selection.bg_fill = accent.gamma_multiply(if dark { 0.55 } else { 0.35 });
    v.selection.stroke = Stroke::new(1.0, strong);
    v.window_stroke = Stroke::new(1.0, stroke);
    v.window_shadow = egui::Shadow::NONE;
    v.popup_shadow = egui::Shadow::NONE;
    v.window_corner_radius = CornerRadius::same(4);
    v.menu_corner_radius = CornerRadius::same(3);
    let r = CornerRadius::same(3);
    v.widgets.noninteractive.bg_fill = panel;
    v.widgets.noninteractive.weak_bg_fill = panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, stroke);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text);
    for (w, bg, fg) in [
        (&mut v.widgets.inactive, widget, text),
        (&mut v.widgets.hovered, hover, strong),
        (&mut v.widgets.active, active, strong),
        (&mut v.widgets.open, hover, strong),
    ] {
        w.bg_fill = bg;
        w.weak_bg_fill = bg;
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke = Stroke::new(1.0, fg);
        w.corner_radius = r;
        w.expansion = 0.0;
    }
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, stroke);
    v.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT; // flat buttons until hovered
    s.visuals = v;
    s.spacing.item_spacing = vec2(6.0, 3.0);
    s.spacing.button_padding = vec2(6.0, 2.0);
    s.spacing.menu_margin = egui::Margin::same(4);
    s.spacing.window_margin = egui::Margin::same(6);
    s.spacing.interact_size.y = 20.0;
    use egui::{FontFamily::*, TextStyle};
    s.text_styles = [
        (TextStyle::Small, FontId::new(10.5, Proportional)),
        (TextStyle::Body, FontId::new(12.5, Proportional)),
        (TextStyle::Button, FontId::new(12.5, Proportional)),
        (TextStyle::Heading, FontId::new(16.0, Proportional)),
        (TextStyle::Monospace, FontId::new(11.5, Monospace)),
    ]
    .into();
    s
}

/// `PROTO_SHOT=out.ppm cargo run -p …` saves a screenshot of the main window after a few frames
/// and exits. `PROTO_LAYOUT=0|1|2` and `PROTO_THEME=0|1|2` pick the variant first. Returns the
/// path on the frame the binary should request the (egui-version-specific) screenshot.
pub fn screenshot_hook(ctx: &egui::Context, shell: &mut Shell) -> Option<String> {
    let path = std::env::var("PROTO_SHOT").ok()?;
    let frame = ctx.cumulative_frame_nr();
    if frame == 1 {
        if let Some(l) = std::env::var("PROTO_LAYOUT").ok().and_then(|v| v.parse::<usize>().ok()) {
            shell.set_layout(Layout::ALL[l % 3]);
        }
        if let Some(t) = std::env::var("PROTO_THEME").ok().and_then(|v| v.parse::<usize>().ok()) {
            shell.set_theme(ThemeChoice::ALL[t % 3]);
        }
        if let Some(d) = std::env::var("PROTO_DOC").ok().and_then(|v| v.parse::<usize>().ok()) {
            shell.open_requests.push(d);
        }
        if std::env::var("PROTO_SELECT").is_ok() {
            shell.selection = Some((0, 0));
        }
        if let Some(f) = std::env::var("PROTO_FLYOUT").ok().and_then(|v| v.parse::<usize>().ok()) {
            shell.flyout = Some(PANELS[f % 3]);
        }
    }
    ctx.request_repaint();
    (frame == 20).then_some(path)
}

pub fn write_ppm(path: &str, img: &egui::ColorImage) {
    let mut out = format!("P6\n{} {}\n255\n", img.size[0], img.size[1]).into_bytes();
    for p in &img.pixels {
        out.extend_from_slice(&[p.r(), p.g(), p.b()]);
    }
    let _ = std::fs::write(path, out);
    std::process::exit(0);
}
