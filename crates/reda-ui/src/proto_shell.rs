//! PROTOTYPE, never merged. Answers "How is the app shell arranged?" by trying the
//! arrangements in the real window on a real Project. Each axis switches on its own from the
//! floating bar, and three presets set them all. State lives in memory; nothing here is
//! production code.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use egui::{Align2, Button, Color32, CornerRadius, Id, Order, Rect, RichText, Sense, Ui, Vec2};
use reda_engine::model::{LayerKind, Side, Uuid};
use serde_json::{Value, json};

use crate::bars::draw::{self, Invocation};
use crate::bars::{self, Action, Bars, DocumentType, Item, data, resolve_for};
use crate::chrome;
use crate::icons;
use crate::keymap::Context;
use crate::panels::layers::layer_name;
use crate::settings::draw::PageCx;
use crate::settings::{Page, Problem, SettingsDialog, Step};
use crate::shell::Shell;
use crate::theme;
use crate::tools::Tool;

pub mod prefs;

// ---- The axes ----------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Top {
    QuickStrip,
    Toolbar,
    PaletteField,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayerUi {
    Strip,
    Swatch,
    Rail,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nav {
    OnePanel,
    ThreePanels,
    InLayers,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    Window,
    Tab,
    Modal,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActiveBar {
    Grouped,
    Flat,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavMode {
    Layers,
    Nets,
    Footprints,
    Zones,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Axes {
    pub top: Top,
    pub layers: LayerUi,
    pub nav: Nav,
    pub prefs: Surface,
    pub setup: Surface,
    pub bar: ActiveBar,
}

pub const PRESETS: [(&str, &str, Axes); 3] = [
    (
        "A",
        "Follow the leading suite",
        Axes {
            top: Top::QuickStrip,
            layers: LayerUi::Strip,
            nav: Nav::OnePanel,
            prefs: Surface::Window,
            setup: Surface::Modal,
            bar: ActiveBar::Grouped,
        },
    ),
    (
        "B",
        "Everything a tab",
        Axes {
            top: Top::Toolbar,
            layers: LayerUi::Swatch,
            nav: Nav::ThreePanels,
            prefs: Surface::Tab,
            setup: Surface::Tab,
            bar: ActiveBar::Flat,
        },
    ),
    (
        "C",
        "Compact",
        Axes {
            top: Top::PaletteField,
            layers: LayerUi::Rail,
            nav: Nav::InLayers,
            prefs: Surface::Modal,
            setup: Surface::Modal,
            bar: ActiveBar::Grouped,
        },
    ),
];

pub struct State {
    pub axes: Axes,
    applied: Option<Axes>,
    expanded: bool,
    pub nav_mode: NavMode,
    nav_query: String,
    nav_dim: usize,
    nav_select: bool,
    nav_zoom: bool,
    faces: HashMap<String, usize>,
    pub recent: Vec<String>,
    pub key_card: bool,
    pub prefs_modal: bool,
    pub prefs: prefs::PrefsState,
    /// Board Setup held open in a tab.
    pub setup: Option<SettingsDialog>,
    /// Show on board: the dialog is hidden and the banner shows.
    pub on_board: bool,
    context_menu: Option<egui::Pos2>,
    stackup: Vec<(String, String, u32)>,
    pending: Vec<(String, Value)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::new());
}

/// Runs `f` on the state. Never nest it.
pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

pub fn axes() -> Axes {
    with(|s| s.axes)
}

impl State {
    fn new() -> State {
        let preset = match std::env::var("REDA_SHELL").as_deref() {
            Ok("B") => 1,
            Ok("C") => 2,
            _ => 0,
        };
        State {
            axes: PRESETS[preset].2,
            applied: None,
            expanded: false,
            nav_mode: NavMode::Nets,
            nav_query: String::new(),
            nav_dim: 1,
            nav_select: true,
            nav_zoom: true,
            faces: HashMap::new(),
            recent: Vec::new(),
            key_card: false,
            prefs_modal: false,
            prefs: prefs::PrefsState::default(),
            setup: None,
            on_board: false,
            context_menu: None,
            stackup: vec![
                ("Top Overlay".into(), "Silkscreen".into(), 10),
                ("Top Solder".into(), "Solder Mask".into(), 20),
                ("Top Copper".into(), "Copper".into(), 35),
                ("Core".into(), "Dielectric FR-4".into(), 1510),
                ("Bottom Copper".into(), "Copper".into(), 35),
                ("Bottom Solder".into(), "Solder Mask".into(), 20),
                ("Bottom Overlay".into(), "Silkscreen".into(), 10),
            ],
            pending: Vec::new(),
        }
    }
}

// ---- Applying the axes to the layout -----------------------------------------------------

fn run(shell: &mut Shell, id: &str, params: Value) {
    let _ = shell.run(id, params);
}

fn apply_layout(shell: &mut Shell, axes: Axes) {
    for tab in ["board", "nets", "footprints", "zones"] {
        run(shell, "layout.tab.close", json!({ "tab": tab }));
    }
    match axes.nav {
        Nav::OnePanel => run(shell, "panel.show:board", json!({})),
        Nav::ThreePanels => {
            for panel in ["nets", "footprints", "zones"] {
                run(shell, &format!("panel.show:{panel}"), json!({}));
            }
        }
        Nav::InLayers => run(shell, "panel.show:layers", json!({})),
    }
    run(shell, "panel.show:projects", json!({}));
    run(shell, "panel.show:properties", json!({}));
    // A surface that changed kind closes rather than moving.
    run(shell, "layout.tab.close", json!({ "tab": "preferences" }));
    run(shell, "layout.tab.close", json!({ "tab": "board_setup" }));
    with(|s| {
        s.prefs_modal = false;
        s.setup = None;
        s.on_board = false;
    });
}

/// Places a surface's Panel as a tab beside the Documents.
fn as_document_tab(shell: &mut Shell, tab: &str) {
    run(shell, &format!("panel.show:{tab}"), json!({}));
    let group = shell.layout().groups().first().map(|(_, g)| g.id);
    if let Some(group) = group {
        run(
            shell,
            "layout.tab.move_to_group",
            json!({"tab": tab, "group": group}),
        );
    }
    run(shell, "layout.tab.activate", json!({ "tab": tab }));
}

/// `App::open_preferences`, as the axis says.
pub fn open_preferences(shell: &mut Shell) {
    match axes().prefs {
        Surface::Modal => with(|s| s.prefs_modal = true),
        Surface::Tab => as_document_tab(shell, "preferences"),
        Surface::Window => {
            let open = shell
                .layout()
                .place(&crate::layout::TabKey::Panel(
                    crate::layout::PanelId::Preferences,
                ))
                .is_some();
            run(shell, "panel.show:preferences", json!({}));
            if !open {
                run(
                    shell,
                    "layout.tab.move_to_new_window",
                    json!({ "tab": "preferences" }),
                );
            }
        }
    }
}

/// `App::open_board_setup` when the axis makes it a tab. False leaves it to the modal.
pub fn open_board_setup_tab(shell: &mut Shell) -> bool {
    if axes().setup != Surface::Tab {
        return false;
    }
    let Some(project) = shell.engine().project() else {
        return true;
    };
    if project.pcb().is_none() {
        return true;
    }
    let dialog = crate::settings::board_setup::dialog(project);
    with(|s| {
        if s.setup.is_none() {
            s.setup = Some(dialog);
        }
    });
    as_document_tab(shell, "board_setup");
    true
}

// ---- The switcher ------------------------------------------------------------------------

fn choice<T: Copy + PartialEq>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    for (option, label) in options {
        let selected = *value == *option;
        let text = RichText::new(*label).color(if selected {
            Color32::BLACK
        } else {
            Color32::WHITE
        });
        let button = Button::new(text).fill(if selected {
            Color32::from_rgb(255, 196, 0)
        } else {
            Color32::from_gray(40)
        });
        if ui.add(button).clicked() {
            *value = *option;
        }
    }
}

/// The floating bar. Obviously not part of the design being judged.
fn switcher(ctx: &egui::Context) {
    let mut state = with(|s| (s.axes, s.expanded));
    egui::Area::new(Id::new("proto-switcher"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_BOTTOM, [0.0, -30.0])
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(20, 20, 20))
                .stroke(egui::Stroke::new(2.0, Color32::from_rgb(255, 196, 0)))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                    ui.horizontal(|ui| {
                        ui.strong("PROTOTYPE");
                        for (key, name, axes) in PRESETS {
                            let on = state.0 == axes;
                            let text = RichText::new(format!("{key} {name}")).color(if on {
                                Color32::BLACK
                            } else {
                                Color32::WHITE
                            });
                            let fill = if on {
                                Color32::from_rgb(255, 196, 0)
                            } else {
                                Color32::from_gray(40)
                            };
                            if ui.add(Button::new(text).fill(fill)).clicked() {
                                state.0 = axes;
                            }
                        }
                        let label = if state.1 { "Fewer v" } else { "Axes ^" };
                        if ui.button(label).clicked() {
                            state.1 = !state.1;
                        }
                    });
                    if state.1 {
                        egui::Grid::new("proto-axes").show(ui, |ui| {
                            let a = &mut state.0;
                            ui.label("Top row");
                            ui.horizontal(|ui| {
                                choice(
                                    ui,
                                    &mut a.top,
                                    &[
                                        (Top::QuickStrip, "Quick strip, no toolbar"),
                                        (Top::Toolbar, "Menus + toolbar (today)"),
                                        (Top::PaletteField, "Menus + Palette field"),
                                    ],
                                )
                            });
                            ui.end_row();
                            ui.label("Layers");
                            ui.horizontal(|ui| {
                                choice(
                                    ui,
                                    &mut a.layers,
                                    &[
                                        (LayerUi::Strip, "Strip, canvas bottom"),
                                        (LayerUi::Swatch, "Swatch on Active Bar"),
                                        (LayerUi::Rail, "Rail, canvas left"),
                                    ],
                                )
                            });
                            ui.end_row();
                            ui.label("Navigator");
                            ui.horizontal(|ui| {
                                choice(
                                    ui,
                                    &mut a.nav,
                                    &[
                                        (Nav::OnePanel, "One Panel, modes"),
                                        (Nav::ThreePanels, "Three Panels"),
                                        (Nav::InLayers, "Inside Layers"),
                                    ],
                                )
                            });
                            ui.end_row();
                            ui.label("Preferences");
                            ui.horizontal(|ui| {
                                choice(
                                    ui,
                                    &mut a.prefs,
                                    &[
                                        (Surface::Window, "Own window"),
                                        (Surface::Tab, "Tab"),
                                        (Surface::Modal, "Modal"),
                                    ],
                                )
                            });
                            ui.end_row();
                            ui.label("Board Setup");
                            ui.horizontal(|ui| {
                                choice(
                                    ui,
                                    &mut a.setup,
                                    &[(Surface::Modal, "Modal"), (Surface::Tab, "Tab")],
                                )
                            });
                            ui.end_row();
                            ui.label("Active Bar");
                            ui.horizontal(|ui| {
                                choice(
                                    ui,
                                    &mut a.bar,
                                    &[
                                        (ActiveBar::Grouped, "Groups, last-used face"),
                                        (ActiveBar::Flat, "Flat, six Tools (spec)"),
                                    ],
                                )
                            });
                            ui.end_row();
                        });
                    }
                });
        });
    with(|s| {
        s.axes = state.0;
        s.expanded = state.1;
    });
}

// ---- Bars --------------------------------------------------------------------------------

const MENU_ORDER: [&str; 10] = [
    "file", "edit", "view", "project", "place", "design", "route", "tools", "window", "help",
];

const EXTRA_MENUS: &str = r#"{ "menus": [
  { "id": "project", "title": "menu-project", "items": [
    { "id": "project.settings", "command": "app.dialog.open:project_settings" },
    { "id": "project.board_setup", "command": "app.dialog.open:board_setup" },
    "separator",
    { "id": "project.sync", "command": "app.dialog.open:change_list" },
    { "id": "project.save", "command": "project.save" }
  ]},
  { "id": "tools", "title": "menu-tools", "items": [
    { "id": "tools.measure", "command": "tool.activate:measure" },
    { "id": "tools.keymap", "command": "app.dialog.open:preferences" }
  ]},
  { "id": "window", "title": "menu-window", "items": [
    { "id": "window.panel.projects", "command": "panel.show:projects" },
    { "id": "window.panel.properties", "command": "panel.show:properties" },
    { "id": "window.panel.messages", "command": "panel.show:messages" },
    { "id": "window.panel.layers", "command": "panel.show:layers" },
    "separator",
    { "id": "window.collapse.left", "command": "layout.dock.collapse", "params": { "dock": "left" } },
    { "id": "window.collapse.right", "command": "layout.dock.collapse", "params": { "dock": "right" } },
    { "id": "window.collapse.bottom", "command": "layout.dock.collapse", "params": { "dock": "bottom" } }
  ]}
]}"#;

const ROUTE_MENU: &str = r#"{ "menus": [
  { "id": "route", "title": "menu-route", "items": [
    { "id": "route.track", "command": "tool.activate:route" },
    { "id": "route.via", "command": "tool.activate:via" },
    "separator",
    { "id": "route.zone_fill", "command": "pcb.zone.fill" },
    { "id": "route.ratsnest", "command": "pcb.ratsnest.show" }
  ]}
]}"#;

/// The menu skeleton: the same order everywhere, Route on the PCB.
pub fn bars_for(document: Option<DocumentType>) -> &'static Bars {
    static ALL: OnceLock<Vec<Bars>> = OnceLock::new();
    let all = ALL.get_or_init(|| {
        let build = |ty: Option<DocumentType>| {
            let mut own = bars::for_document(ty).clone();
            let extra = data::parse(EXTRA_MENUS).expect("proto menus");
            own.menus.extend(extra.menus);
            if ty == Some(DocumentType::Pcb) {
                own.menus
                    .extend(data::parse(ROUTE_MENU).expect("proto route").menus);
            }
            own.menus.sort_by_key(|m| {
                MENU_ORDER
                    .iter()
                    .position(|id| *id == m.id)
                    .unwrap_or(MENU_ORDER.len())
            });
            own
        };
        let mut all = vec![build(None)];
        for ty in DocumentType::ALL {
            all.push(build(Some(ty)));
        }
        all
    });
    &all[document.map_or(0, |ty| 1 + ty as usize)]
}

const QUICK: &str = r#"{ "toolbar": [
  { "id": "quick.save", "command": "project.save" },
  { "id": "quick.undo", "command": "edit.undo", "params": { "document": "$undo_stack" } },
  { "id": "quick.redo", "command": "edit.redo", "params": { "document": "$undo_stack" } }
]}"#;

fn quick_bars() -> &'static Bars {
    static QUICK_BARS: OnceLock<Bars> = OnceLock::new();
    QUICK_BARS.get_or_init(|| data::parse(QUICK).expect("proto quick"))
}

/// Small flat buttons, in a row, for the menu row.
fn quick_row(ui: &mut Ui, cx: &draw::Cx<'_>) -> Option<Invocation> {
    let theme = ui.ctx().theme();
    let mut clicked = None;
    for item in &quick_bars().toolbar {
        let Item::Action(action) = item else { continue };
        let item = resolve_for(cx.shell, action, cx.document);
        let enabled = item.run.is_ok();
        let Some(image) = icons::image(&item.icon, icons::MENU, theme, enabled) else {
            continue;
        };
        let response = chrome::icon_button(ui, image, 18.0, 22.0, false, enabled);
        let response = response.on_hover_text(&item.title);
        if response.clicked()
            && let Ok(run) = item.run
        {
            clicked = Some(run);
        }
    }
    clicked
}

fn gear(ui: &mut Ui) -> Option<Invocation> {
    ui.add(Button::new("Prefs").frame_when_inactive(false))
        .on_hover_text("Preferences")
        .clicked()
        .then(|| ("app.dialog.open:preferences".to_owned(), json!({})))
}

/// The top of the window, as the axis says.
pub fn top_rows(ui: &mut Ui, cx: &draw::Cx<'_>) -> Option<Invocation> {
    let document = bars::active_type(cx.shell);
    let own = bars_for(document);
    match axes().top {
        Top::Toolbar => {
            let menu = egui::Panel::top("menu-bar")
                .show(ui, |ui| draw::menu_bar(ui, cx, own))
                .inner;
            let tool = egui::Panel::top("toolbar")
                .show(ui, |ui| draw::toolbar(ui, cx, own))
                .inner;
            menu.or(tool)
        }
        Top::QuickStrip => {
            egui::Panel::top("menu-bar")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let quick = quick_row(ui, cx);
                        chrome::bar_separator(ui, 18.0);
                        let menu = draw::menu_bar(ui, cx, own);
                        let right = ui
                            .with_layout(egui::Layout::right_to_left(egui::Align::Center), gear)
                            .inner;
                        quick.or(menu).or(right)
                    })
                    .inner
                })
                .inner
        }
        Top::PaletteField => {
            egui::Panel::top("menu-bar")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let menu = draw::menu_bar(ui, cx, own);
                        let row = ui.max_rect();
                        let centre = egui::pos2(row.center().x, ui.min_rect().center().y);
                        let field = ui.put(
                            Rect::from_center_size(centre, Vec2::new(380.0, 20.0)),
                            Button::new(
                                RichText::new("Search Commands, @Component, #Net, :Document")
                                    .weak(),
                            )
                            .fill(ui.visuals().extreme_bg_color)
                            .right_text(RichText::new("Ctrl+Shift+P").weak()),
                        );
                        let palette = field
                            .clicked()
                            .then(|| ("app.dialog.open:command_palette".to_owned(), json!({})));
                        let right = ui
                            .scope_builder(
                                egui::UiBuilder::new()
                                    .max_rect(Rect::from_min_max(
                                        egui::pos2(row.right() - 120.0, row.top()),
                                        row.right_bottom(),
                                    ))
                                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
                                |ui| {
                                    let gear = gear(ui);
                                    gear.or(quick_row(ui, cx))
                                },
                            )
                            .inner;
                        menu.or(palette).or(right)
                    })
                    .inner
                })
                .inner
        }
    }
}

// ---- The Active Bar ----------------------------------------------------------------------

const GROUPS_PCB: &str = r#"{ "active_bar": [
  { "id": "g.select", "title": "active-group", "items": [
    { "id": "s", "command": "tool.activate:select" } ]},
  { "id": "g.move", "title": "active-group", "items": [
    { "id": "m", "command": "tool.activate:move" },
    { "id": "d", "command": "tool.activate:drag" } ]},
  "separator",
  { "id": "g.track", "title": "active-group", "items": [
    { "id": "t", "command": "tool.activate:route" } ]},
  { "id": "g.via", "title": "active-group", "items": [
    { "id": "v", "command": "tool.activate:via" } ]},
  { "id": "g.zone", "title": "active-group", "items": [
    { "id": "z", "command": "tool.activate:zone" },
    { "id": "k", "command": "tool.activate:keepout" } ]},
  { "id": "g.outline", "title": "active-group", "items": [
    { "id": "o", "command": "tool.activate:board_outline" },
    { "id": "r", "command": "tool.activate:board_outline_rect" },
    { "id": "c", "command": "tool.activate:board_cutout" } ]},
  { "id": "g.pad", "title": "active-group", "items": [
    { "id": "p", "command": "tool.activate:free_pad" },
    { "id": "po", "command": "tool.activate:placement_origin" } ]},
  "separator",
  { "id": "g.text", "title": "active-group", "items": [
    { "id": "x", "command": "tool.activate:text" } ]},
  { "id": "g.graphics", "title": "active-group", "items": [
    { "id": "l", "command": "tool.activate:line" },
    { "id": "a", "command": "tool.activate:arc" },
    { "id": "e", "command": "tool.activate:rect" } ]},
  { "id": "g.measure", "title": "active-group", "items": [
    { "id": "me", "command": "tool.activate:measure" },
    { "id": "uo", "command": "tool.activate:user_origin" } ]}
]}"#;

const GROUPS_SCH: &str = r#"{ "active_bar": [
  { "id": "g.select", "title": "active-group", "items": [
    { "id": "s", "command": "tool.activate:select" } ]},
  { "id": "g.move", "title": "active-group", "items": [
    { "id": "m", "command": "tool.activate:move" },
    { "id": "d", "command": "tool.activate:drag" } ]},
  "separator",
  { "id": "g.wire", "title": "active-group", "items": [
    { "id": "w", "command": "tool.activate:wire" } ]},
  { "id": "g.label", "title": "active-group", "items": [
    { "id": "n", "command": "tool.activate:net_label" },
    { "id": "p", "command": "tool.activate:port" } ]},
  { "id": "g.power", "title": "active-group", "items": [
    { "id": "b", "command": "tool.activate:power_port" } ]},
  { "id": "g.nc", "title": "active-group", "items": [
    { "id": "q", "command": "tool.activate:no_connect" } ]},
  "separator",
  { "id": "g.text", "title": "active-group", "items": [
    { "id": "x", "command": "tool.activate:text" } ]},
  { "id": "g.graphics", "title": "active-group", "items": [
    { "id": "l", "command": "tool.activate:line" },
    { "id": "e", "command": "tool.activate:rect" },
    { "id": "a", "command": "tool.activate:arc" } ]}
]}"#;

fn groups(kind: DocumentType) -> Option<&'static [Item]> {
    static PCB: OnceLock<Bars> = OnceLock::new();
    static SCH: OnceLock<Bars> = OnceLock::new();
    match kind {
        DocumentType::Pcb => Some(
            &PCB.get_or_init(|| data::parse(GROUPS_PCB).expect("proto groups"))
                .active_bar,
        ),
        DocumentType::Schematic => Some(
            &SCH.get_or_init(|| data::parse(GROUPS_SCH).expect("proto groups"))
                .active_bar,
        ),
        _ => None,
    }
}

fn grouped_bar(
    ui: &mut Ui,
    shell: &Shell,
    document: Uuid,
    items: &[Item],
    id: Id,
    canvas: Rect,
    tool: &str,
) -> Option<Invocation> {
    let theme = ui.ctx().theme();
    let mut clicked = None;
    let layer_ui = axes().layers;
    egui::Area::new(id.with("active-bar"))
        .order(Order::Middle)
        .fixed_pos(canvas.center_top() + Vec2::new(0.0, 12.0))
        .pivot(Align2::CENTER_TOP)
        .constrain_to(canvas.shrink2(Vec2::new(8.0, 0.0)))
        .show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(3)
                .corner_radius(CornerRadius::same(5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = chrome::BAR_SPACING;
                        for item in items {
                            match item {
                                Item::Separator => chrome::bar_separator(ui, chrome::BAR_BUTTON),
                                Item::Submenu(menu) => {
                                    let actions: Vec<&Action> = menu
                                        .items
                                        .iter()
                                        .filter_map(|i| match i {
                                            Item::Action(a) => Some(a),
                                            _ => None,
                                        })
                                        .collect();
                                    let face = with(|s| {
                                        s.faces.get(&menu.id).copied().unwrap_or_default()
                                    })
                                    .min(actions.len() - 1);
                                    let resolved =
                                        resolve_for(shell, actions[face], Some(document));
                                    let on = actions.iter().any(|a| {
                                        resolve_for(shell, a, Some(document)).params
                                            == json!({ "tool": tool })
                                    });
                                    let enabled = resolved.run.is_ok();
                                    let Some(image) = icons::image(
                                        &resolved.icon,
                                        icons::TOOLBAR,
                                        theme,
                                        enabled,
                                    ) else {
                                        continue;
                                    };
                                    let response = chrome::icon_button(
                                        ui,
                                        image,
                                        icons::TOOLBAR,
                                        chrome::BAR_BUTTON,
                                        on,
                                        enabled,
                                    );
                                    if actions.len() > 1 {
                                        let r = response.rect;
                                        ui.painter().add(egui::Shape::convex_polygon(
                                            vec![
                                                r.right_bottom() + Vec2::new(-2.0, -2.0),
                                                r.right_bottom() + Vec2::new(-7.0, -2.0),
                                                r.right_bottom() + Vec2::new(-2.0, -7.0),
                                            ],
                                            ui.visuals().text_color(),
                                            egui::Stroke::NONE,
                                        ));
                                    }
                                    let held = response.is_pointer_button_down_on()
                                        && ui.input(|i| {
                                            i.pointer
                                                .press_start_time()
                                                .is_some_and(|t| i.time - t > 0.5)
                                        });
                                    let response = response.on_hover_ui(|ui| {
                                        ui.strong(&resolved.title);
                                        ui.label(&resolved.description);
                                        if actions.len() > 1 {
                                            ui.weak("Hold or right-click for the group");
                                        }
                                    });
                                    if response.clicked()
                                        && let Ok(run) = resolved.run.clone()
                                    {
                                        clicked = Some(run);
                                    }
                                    if actions.len() > 1 {
                                        let popup_id = response.id.with("group");
                                        let mut open = response.secondary_clicked() || held;
                                        egui::Popup::from_response(&response)
                                            .id(popup_id)
                                            .open_memory(
                                                open.then_some(egui::SetOpenCommand::Bool(true)),
                                            )
                                            .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
                                            .show(|ui| {
                                                for (i, action) in actions.iter().enumerate() {
                                                    let item =
                                                        resolve_for(shell, action, Some(document));
                                                    let image = icons::image(
                                                        &item.icon,
                                                        icons::MENU,
                                                        theme,
                                                        item.run.is_ok(),
                                                    );
                                                    let button = match image {
                                                        Some(image) => {
                                                            Button::new((image, item.title.clone()))
                                                        }
                                                        None => Button::new(item.title.clone()),
                                                    };
                                                    if ui.add(button).clicked()
                                                        && let Ok(run) = item.run
                                                    {
                                                        with(|s| {
                                                            s.faces.insert(menu.id.clone(), i)
                                                        });
                                                        clicked = Some(run);
                                                        open = false;
                                                    }
                                                }
                                                let _ = open;
                                            });
                                    }
                                }
                                Item::Action(_) => {}
                            }
                        }
                        if layer_ui == LayerUi::Swatch && shell.is_pcb(document) {
                            chrome::bar_separator(ui, chrome::BAR_BUTTON);
                            layer_swatch(ui, shell, &mut clicked);
                        }
                    });
                });
        });
    clicked
}

fn active_layer(shell: &Shell) -> Option<Uuid> {
    let project = shell.engine().project()?;
    shell.board_view().active_layer(project)
}

fn layer_colour(ui: &Ui, shell: &Shell, kind: LayerKind, side: Option<Side>) -> Color32 {
    use reda_engine::render::{BoardColor, ColorKey};
    let canvas_theme = theme::canvas_theme(ui.ctx().theme());
    let c = shell
        .palette()
        .palette(canvas_theme)
        .color(ColorKey::Board(BoardColor::layer(kind, side)));
    Color32::from_rgb(c.0[0], c.0[1], c.0[2])
}

/// The Active Bar's painted swatch, with the Layer list under it.
fn layer_swatch(ui: &mut Ui, shell: &Shell, clicked: &mut Option<Invocation>) {
    let Some(project) = shell.engine().project() else {
        return;
    };
    let Some(pcb) = project.pcb() else { return };
    let active = active_layer(shell);
    let current = active.and_then(|id| pcb.layers().layer(id));
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(64.0, chrome::BAR_BUTTON), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(3),
            ui.visuals().widgets.hovered.weak_bg_fill,
        );
    }
    if let Some(layer) = current {
        let colour = layer_colour(ui, shell, layer.kind, layer.side);
        ui.painter().rect_filled(
            Rect::from_center_size(rect.left_center() + Vec2::new(11.0, 0.0), Vec2::splat(14.0)),
            CornerRadius::same(2),
            colour,
        );
        ui.painter().text(
            rect.left_center() + Vec2::new(22.0, 0.0),
            Align2::LEFT_CENTER,
            short_layer(layer.kind, layer.side),
            egui::TextStyle::Small.resolve(ui.style()),
            ui.visuals().text_color(),
        );
    }
    egui::Popup::menu(&response).show(|ui| {
        for layer in &pcb.layers().layers {
            let colour = layer_colour(ui, shell, layer.kind, layer.side);
            let name = layer_name(layer);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(2), colour);
                if ui
                    .selectable_label(active == Some(layer.uuid), name)
                    .clicked()
                {
                    *clicked = Some((
                        "pcb.layer.activate".to_owned(),
                        json!({"layer": layer.uuid}),
                    ));
                }
            });
        }
    });
}

fn short_layer(kind: LayerKind, side: Option<Side>) -> String {
    let s = match side {
        Some(Side::Top) => "T.",
        Some(Side::Bottom) => "B.",
        _ => "",
    };
    let k = match kind {
        LayerKind::Copper => "Cu",
        LayerKind::Silkscreen => "Silk",
        LayerKind::SolderMask => "Mask",
        LayerKind::Paste => "Paste",
        LayerKind::Courtyard => "CrtYd",
        LayerKind::Assembly => "Assy",
        LayerKind::BoardOutline => "Outline",
        LayerKind::Mechanical => "Mech",
        LayerKind::Documentation => "Doc",
    };
    format!("{s}{k}")
}

const LAYER_SETS: [&str; 7] = [
    "All",
    "Signal",
    "NonSignal",
    "Mechanical",
    "Top",
    "Bottom",
    "Fab",
];

fn in_set(set: &str, kind: LayerKind, side: Option<Side>) -> bool {
    match set {
        "Signal" => kind == LayerKind::Copper,
        "NonSignal" => kind != LayerKind::Copper,
        "Mechanical" => matches!(
            kind,
            LayerKind::Mechanical
                | LayerKind::Documentation
                | LayerKind::Assembly
                | LayerKind::Courtyard
                | LayerKind::BoardOutline
        ),
        "Top" => side != Some(Side::Bottom),
        "Bottom" => side != Some(Side::Top),
        "Fab" => matches!(
            kind,
            LayerKind::Copper
                | LayerKind::Silkscreen
                | LayerKind::SolderMask
                | LayerKind::Paste
                | LayerKind::BoardOutline
        ),
        _ => true,
    }
}

/// One Layer chip: the swatch and short name. Click activates; Mod+click would highlight.
fn layer_chip(
    ui: &mut Ui,
    shell: &Shell,
    layer: &reda_engine::model::Layer,
    active: bool,
    width: f32,
    out: &mut Vec<(String, Value)>,
) {
    let colour = layer_colour(ui, shell, layer.kind, layer.side);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::click());
    let v = ui.visuals();
    let fill = if active {
        v.selection.bg_fill
    } else if response.hovered() {
        v.widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
    ui.painter().rect_filled(
        Rect::from_center_size(rect.left_center() + Vec2::new(9.0, 0.0), Vec2::splat(10.0)),
        CornerRadius::same(2),
        colour,
    );
    let text_colour = if active {
        v.strong_text_color()
    } else {
        v.text_color()
    };
    ui.painter().text(
        rect.left_center() + Vec2::new(17.0, 0.0),
        Align2::LEFT_CENTER,
        short_layer(layer.kind, layer.side),
        egui::TextStyle::Small.resolve(ui.style()),
        text_colour,
    );
    let response = response.on_hover_text(format!(
        "{}\nClick: active. Mod+click: highlight its content. Double-click: Layers panel.",
        layer_name(layer)
    ));
    if response.double_clicked() {
        out.push(("panel.show:layers".to_owned(), json!({})));
    } else if response.clicked() {
        if ui.input(|i| i.modifiers.command) {
            out.push(("selection.set".to_owned(), json!({ "objects": [] })));
        } else {
            out.push((
                "pcb.layer.activate".to_owned(),
                json!({"layer": layer.uuid}),
            ));
        }
    }
}

/// The height the bottom Layer strip takes from the canvas.
pub const STRIP: f32 = 24.0;

/// The Layer control over the canvas, as the axis says.
fn layer_control(ui: &mut Ui, shell: &Shell, id: Id, canvas: Rect, out: &mut Vec<(String, Value)>) {
    let Some(project) = shell.engine().project() else {
        return;
    };
    let Some(pcb) = project.pcb() else { return };
    let active = active_layer(shell);
    let hidden = &shell.board_view().layers.hidden;
    let shown: Vec<_> = pcb
        .layers()
        .layers
        .iter()
        .filter(|l| !hidden.contains(&l.uuid))
        .collect();
    let mut shown = shown;
    let rank = |k: LayerKind| match k {
        LayerKind::Copper => 0,
        LayerKind::Silkscreen => 1,
        LayerKind::SolderMask => 2,
        LayerKind::Paste => 3,
        LayerKind::BoardOutline => 4,
        LayerKind::Courtyard => 5,
        LayerKind::Assembly => 6,
        LayerKind::Mechanical => 7,
        LayerKind::Documentation => 8,
    };
    shown.sort_by_key(|l| (rank(l.kind), l.side == Some(Side::Bottom)));
    match axes().layers {
        LayerUi::Swatch => {}
        LayerUi::Strip => {
            let rect = Rect::from_min_max(
                egui::pos2(canvas.left(), canvas.bottom() - STRIP),
                canvas.right_bottom(),
            );
            egui::Area::new(id.with("layer-strip"))
                .order(Order::Middle)
                .fixed_pos(rect.min)
                .show(ui.ctx(), |ui| {
                    ui.set_clip_rect(rect);
                    ui.painter().rect_filled(rect, 0.0, ui.visuals().panel_fill);
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(4.0, 3.0))),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                ui.menu_button(RichText::new("LS v").small(), |ui| {
                                    for set in LAYER_SETS {
                                        if ui.button(set).clicked() {
                                            for l in &pcb.layers().layers {
                                                out.push((
                                                    "pcb.layer.show".to_owned(),
                                                    json!({"layer": l.uuid,
                                                    "visible": in_set(set, l.kind, l.side)}),
                                                ));
                                            }
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    let _ = ui.button("Save current as...");
                                });
                                for layer in &shown {
                                    layer_chip(
                                        ui,
                                        shell,
                                        layer,
                                        active == Some(layer.uuid),
                                        62.0,
                                        out,
                                    );
                                }
                            });
                        },
                    );
                });
        }
        LayerUi::Rail => {
            let height = shown.len() as f32 * 20.0 + 30.0;
            let at = canvas.left_center() + Vec2::new(8.0, -height / 2.0);
            egui::Area::new(id.with("layer-rail"))
                .order(Order::Middle)
                .fixed_pos(at)
                .constrain_to(canvas)
                .show(ui.ctx(), |ui| {
                    egui::Frame::window(ui.style())
                        .inner_margin(3)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.menu_button(RichText::new("Set v").small(), |ui| {
                                for set in LAYER_SETS {
                                    if ui.button(set).clicked() {
                                        for l in &pcb.layers().layers {
                                            out.push((
                                                "pcb.layer.show".to_owned(),
                                                json!({"layer": l.uuid,
                                                "visible": in_set(set, l.kind, l.side)}),
                                            ));
                                        }
                                        ui.close();
                                    }
                                }
                            });
                            for layer in &shown {
                                layer_chip(ui, shell, layer, active == Some(layer.uuid), 66.0, out);
                            }
                        });
                });
        }
    }
}

/// The Routing Bar or Placement Bar, mocked in the Active Bar's form, so room for them can
/// be checked against the Layer control.
fn tool_bar(ui: &mut Ui, shell: &Shell, id: Id, canvas: Rect) {
    let footprints_selected = shell
        .engine()
        .project()
        .and_then(|p| p.pcb())
        .is_some_and(|pcb| {
            let selection = shell.selection();
            !selection.objects.is_empty()
                && selection
                    .objects
                    .iter()
                    .all(|o| pcb.footprints().any(|f| f.uuid == *o))
        });
    let buttons: &[&str] = if shell.tool() == Tool::Route {
        &[
            "0.25 mm", "45 deg", "Look", "Stop", "Auto", "Loop", "Clear", "Keys",
        ]
    } else if footprints_selected {
        &["Stop", "Guides", "Rats", "Rotate", "Flip", "Keys"]
    } else {
        return;
    };
    let lift = if axes().layers == LayerUi::Strip {
        STRIP + 28.0
    } else {
        28.0
    };
    egui::Area::new(id.with("tool-bar"))
        .order(Order::Middle)
        .fixed_pos(canvas.center_bottom() - Vec2::new(0.0, lift))
        .pivot(Align2::CENTER_BOTTOM)
        .show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(3)
                .corner_radius(CornerRadius::same(5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = chrome::BAR_SPACING;
                        for label in buttons {
                            let clicked = ui
                                .add(
                                    Button::new(RichText::new(*label).small())
                                        .frame_when_inactive(false)
                                        .min_size(Vec2::splat(chrome::BAR_BUTTON)),
                                )
                                .clicked();
                            if clicked && *label == "Keys" {
                                with(|s| s.key_card = !s.key_card);
                            }
                        }
                    });
                });
        });
}

const ROUTE_KEYS: &[(&str, &str)] = &[
    ("Space", "Flip the posture"),
    ("/", "Corner mode"),
    ("1", "Look-ahead"),
    ("2", "Via on this Layer"),
    ("3", "Width mode"),
    ("4", "Via size mode"),
    ("7", "Next connection"),
    ("9", "Other end"),
    ("L", "Start Layer"),
    ("V", "Via and Layer"),
    ("E", "Via and end"),
    ("Shift+R", "Obstacle mode"),
    ("Shift+A", "Auto-terminate"),
    ("Shift+L", "Loop removal"),
    ("C", "Clearance display"),
    ("Shift+W", "Width chooser"),
    ("Shift+V", "Via size chooser"),
    ("Enter", "End the route"),
    ("Backspace", "Remove the last segment"),
    ("Esc", "End the route, then Select"),
];

/// The key-hint card, generated from the Keymap's Tool context.
fn key_card(ui: &mut Ui, shell: &Shell, id: Id, canvas: Rect) {
    let tool = shell.tool();
    let generated: Vec<(String, String)>;
    let rows: Vec<(String, String)> = if tool == Tool::Route {
        ROUTE_KEYS
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    } else {
        let mac = ui.ctx().os().is_mac();
        let context = Context::Tool(tool.id().to_owned());
        let doc = bars::active_type(shell).map(Context::Document);
        generated = shell
            .keymap()
            .bindings()
            .iter()
            .filter(|b| b.context == context || Some(&b.context) == doc.as_ref())
            .take(26)
            .map(|b| {
                let title = resolve_for(shell, &b.action, None).title;
                (b.chord.label(mac), title)
            })
            .collect();
        generated
    };
    egui::Area::new(id.with("key-card"))
        .order(Order::Foreground)
        .fixed_pos(canvas.right_top() + Vec2::new(-10.0, 58.0))
        .pivot(Align2::RIGHT_TOP)
        .show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style()).show(ui, |ui| {
                ui.strong(format!("Keys: {} (Shift+F1 closes)", tool.id()));
                ui.separator();
                egui::Grid::new("key-card-grid")
                    .spacing([10.0, 1.0])
                    .show(ui, |ui| {
                        let half = rows.len().div_ceil(2);
                        for i in 0..half {
                            for j in [i, i + half] {
                                if let Some((key, title)) = rows.get(j) {
                                    ui.monospace(key);
                                    ui.label(title);
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        });
}

/// The banner for Show on board. It takes the Active Bar's place, as the canvas is
/// view-only while it shows.
fn banner(ui: &mut Ui, id: Id, canvas: Rect) {
    egui::Area::new(id.with("show-on-board"))
        .order(Order::Foreground)
        .fixed_pos(canvas.center_top() + Vec2::new(0.0, 12.0))
        .pivot(Align2::CENTER_TOP)
        .show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style())
                .fill(Color32::from_rgb(0x1f, 0x5f, 0xbf))
                .inner_margin(6)
                .corner_radius(CornerRadius::same(5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(
                                "Board Setup: 12 objects use Net Class Power. View only.",
                            )
                            .color(Color32::WHITE),
                        );
                        if ui.button("Back to Board Setup").clicked() {
                            with(|s| s.on_board = false);
                            with(|s| {
                                if s.setup.is_some() {
                                    s.pending.push((
                                        "layout.tab.activate".to_owned(),
                                        json!({"tab": "board_setup"}),
                                    ));
                                }
                            });
                        }
                    });
                });
        });
}

/// Canvas right-click, in the proposed order.
fn context_menu(ui: &mut Ui, shell: &Shell, id: Id, canvas: Rect, out: &mut Vec<(String, Value)>) {
    let ctx = ui.ctx().clone();
    let pressed = ctx.input(|i| i.pointer.secondary_clicked());
    let at = ctx.input(|i| i.pointer.interact_pos());
    if pressed
        && let Some(at) = at
        && canvas.contains(at)
        && ctx.layer_id_at(at) == Some(ui.layer_id())
    {
        with(|s| s.context_menu = Some(at));
    }
    let Some(at) = with(|s| s.context_menu) else {
        return;
    };
    if !canvas.contains(at) {
        return;
    }
    let has_selection = !shell.selection().is_empty();
    let mut close = false;
    let area = egui::Area::new(id.with("context-menu"))
        .order(Order::Foreground)
        .fixed_pos(at)
        .show(&ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(210.0);
                let mut item = |ui: &mut Ui, label: &str, key: &str| {
                    let clicked = ui
                        .add(
                            Button::new(label)
                                .shortcut_text(key)
                                .frame_when_inactive(false),
                        )
                        .clicked();
                    close |= clicked;
                    clicked
                };
                if has_selection {
                    ui.menu_button("Track > (object's own)", |ui| {
                        let _ = ui.button("Unroute Net");
                        let _ = ui.button("Select connected copper");
                    });
                    ui.separator();
                    item(ui, "Cut", "Ctrl+X");
                    item(ui, "Copy", "Ctrl+C");
                    item(ui, "Duplicate", "");
                    item(ui, "Delete", "Del");
                } else if item(ui, "Document properties", "") {
                    out.push(("panel.show:properties".to_owned(), json!({})));
                }
                item(ui, "Paste", "Ctrl+V");
                ui.menu_button("Select", |ui| {
                    let _ = ui.button("Similar...  Shift+F");
                    let _ = ui.button("Connected  U");
                    let _ = ui.button("All on Layer");
                });
                ui.separator();
                item(ui, "Highlight Net", "");
                if item(ui, "Clear highlight", "Shift+C") {
                    out.push(("view.highlight".to_owned(), json!({})));
                }
                item(ui, "Cross-probe", "Ctrl+Shift+C");
                item(ui, "Applicable rules...", "");
                ui.separator();
                ui.menu_button("Place", |ui| {
                    let _ = ui.button("Track  T");
                    let _ = ui.button("Via  V");
                    let _ = ui.button("Zone  Z");
                });
                ui.separator();
                if item(ui, "Properties", "F11") {
                    out.push(("panel.show:properties".to_owned(), json!({})));
                }
            });
        });
    let elsewhere = ctx.input(|i| i.pointer.any_pressed())
        && at != ctx.input(|i| i.pointer.interact_pos()).unwrap_or(at)
        && !area.response.hovered()
        && !ctx.input(|i| i.pointer.secondary_pressed());
    if close || elsewhere || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        with(|s| s.context_menu = None);
    }
}

/// All that floats on a canvas: the Active Bar, Layer control, Tool bar, key card, banner
/// and context menu.
pub fn canvas_overlays(
    ui: &mut Ui,
    shell: &Shell,
    document: Uuid,
    id: Id,
    rect: Rect,
    out: &mut Vec<(String, Value)>,
) -> bool {
    let is_pcb = shell.is_pcb(document);
    if is_pcb {
        layer_control(ui, shell, id, rect, out);
        tool_bar(ui, shell, id, rect);
    }
    context_menu(ui, shell, id, rect, out);
    if with(|s| s.key_card) {
        key_card(ui, shell, id, rect);
    }
    let on_board = with(|s| s.on_board) && is_pcb;
    if on_board {
        banner(ui, id, rect);
        return true;
    }
    let Some(kind) = bars::document_type(shell, document) else {
        return false;
    };
    if axes().bar == ActiveBar::Grouped
        && let Some(items) = groups(kind)
    {
        let tool = shell.tool().id();
        if let Some(clicked) = grouped_bar(ui, shell, document, items, id, rect, tool) {
            out.push(clicked);
        }
        return true;
    }
    if axes().layers == LayerUi::Swatch && is_pcb {
        // A swatch beside the flat bar, since the flat bar has nowhere to hold it.
        egui::Area::new(id.with("swatch"))
            .order(Order::Middle)
            .fixed_pos(rect.right_top() + Vec2::new(-8.0, 12.0))
            .pivot(Align2::RIGHT_TOP)
            .show(ui.ctx(), |ui| {
                egui::Frame::window(ui.style())
                    .inner_margin(3)
                    .show(ui, |ui| {
                        let mut clicked = None;
                        layer_swatch(ui, shell, &mut clicked);
                        out.extend(clicked);
                    });
            });
    }
    false
}

// ---- Board navigator ---------------------------------------------------------------------

fn nav_header(ui: &mut Ui) {
    let (mut query, mut dim, mut select, mut zoom) =
        with(|s| (s.nav_query.clone(), s.nav_dim, s.nav_select, s.nav_zoom));
    ui.add(
        egui::TextEdit::singleline(&mut query)
            .hint_text("Filter")
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        for (i, label) in ["Normal", "Dim", "Mask"].into_iter().enumerate() {
            if ui.selectable_label(dim == i, label).clicked() {
                dim = i;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut select, "Select");
        ui.checkbox(&mut zoom, "Zoom");
        if ui.small_button("Clear").clicked() {
            with(|s| s.pending.push(("view.highlight".to_owned(), json!({}))));
        }
    });
    with(|s| {
        s.nav_query = query;
        s.nav_dim = dim;
        s.nav_select = select;
        s.nav_zoom = zoom;
    });
}

fn nm_to_mm(nm: i64) -> f64 {
    nm as f64 / 1e6
}

fn nav_list(ui: &mut Ui, shell: &Shell, mode: NavMode, out: &mut Vec<(String, Value)>) {
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        ui.weak("No PCB Document in this Project");
        return;
    };
    let query = with(|s| s.nav_query.to_lowercase());
    let visible = shell.board_view();
    let rows_id = ui.id().with("nav-table");
    egui::ScrollArea::vertical()
        .id_salt(rows_id)
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Grid::new(rows_id.with("g"))
                .striped(true)
                .spacing([8.0, 1.0])
                .show(ui, |ui| match mode {
                    NavMode::Layers => {}
                    NavMode::Nets => {
                        ui.weak("");
                        ui.weak("Net");
                        ui.weak("Tracks");
                        ui.weak("Length");
                        ui.end_row();
                        let mut nets: Vec<_> = pcb.nets().collect();
                        nets.sort_by_key(|n| n.name.to_lowercase());
                        for net in nets {
                            if !net.name.to_lowercase().contains(&query) {
                                continue;
                            }
                            let mut shown =
                                !visible.layers.ratsnest.hidden_nets.contains(&net.uuid);
                            if ui
                                .checkbox(&mut shown, "")
                                .on_hover_text("Ratsnest")
                                .changed()
                            {
                                out.push((
                                    "pcb.ratsnest.show".to_owned(),
                                    json!({"net": net.uuid, "visible": shown}),
                                ));
                            }
                            if ui
                                .add(Button::new(&net.name).frame_when_inactive(false))
                                .clicked()
                            {
                                out.push(("selection.set".to_owned(), json!({"nets": [net.uuid]})));
                            }
                            let tracks: Vec<_> =
                                pcb.tracks().filter(|t| t.net == Some(net.uuid)).collect();
                            ui.label(tracks.len().to_string());
                            let length: f64 =
                                tracks.iter().map(|t| track_length(&t.geometry)).sum();
                            ui.label(format!("{:.1} mm", nm_to_mm(length as i64)));
                            ui.end_row();
                        }
                    }
                    NavMode::Footprints => {
                        ui.weak("Designator");
                        ui.weak("Side");
                        ui.weak("Flags");
                        ui.end_row();
                        let mut all: Vec<_> = pcb.footprints().collect();
                        all.sort_by_key(|f| f.designator.clone());
                        for f in all {
                            if !f.designator.to_lowercase().contains(&query) {
                                continue;
                            }
                            if ui
                                .add(Button::new(&f.designator).frame_when_inactive(false))
                                .clicked()
                            {
                                out.push(("proto.reveal".to_owned(), json!({"object": f.uuid})));
                            }
                            ui.label(match f.side {
                                Side::Top => "Top",
                                Side::Bottom => "Bottom",
                            });
                            ui.weak(format!(
                                "{}{}",
                                if f.locked { "locked " } else { "" },
                                if f.not_fitted { "not fitted" } else { "" }
                            ));
                            ui.end_row();
                        }
                    }
                    NavMode::Zones => {
                        ui.weak("#");
                        ui.weak("Zone");
                        ui.weak("Layer");
                        ui.weak("Net");
                        ui.end_row();
                        let mut all: Vec<_> = pcb.zones().collect();
                        all.sort_by_key(|z| z.priority);
                        for (i, z) in all.into_iter().enumerate() {
                            let net = z
                                .net
                                .and_then(|n| pcb.nets().find(|x| x.uuid == n))
                                .map_or("(no Net)".to_owned(), |n| n.name.clone());
                            let layer = pcb
                                .layers()
                                .layer(z.layer)
                                .map_or(String::new(), |l| short_layer(l.kind, l.side));
                            ui.label(z.priority.to_string());
                            if ui
                                .add(
                                    Button::new(format!("Zone {}", i + 1))
                                        .frame_when_inactive(false),
                                )
                                .clicked()
                            {
                                out.push(("proto.reveal".to_owned(), json!({"object": z.uuid})));
                            }
                            ui.label(layer);
                            ui.label(net);
                            ui.end_row();
                        }
                    }
                });
        });
}

fn track_length(geometry: &reda_engine::model::TrackGeometry) -> f64 {
    let value = serde_json::to_value(geometry).unwrap_or_default();
    let line = value.get("line").unwrap_or(&value);
    let point = |v: &Value| {
        Some(match v.as_array() {
            Some(pair) => (pair.first()?.as_f64()?, pair.get(1)?.as_f64()?),
            None => (v["x"].as_f64()?, v["y"].as_f64()?),
        })
    };
    let ends = (
        point(
            line.get("start")
                .or(line.get("from"))
                .unwrap_or(&Value::Null),
        ),
        point(line.get("end").or(line.get("to")).unwrap_or(&Value::Null)),
    );
    match ends {
        (Some(a), Some(b)) => ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt(),
        _ => 0.0,
    }
}

/// The mode row for a navigator with modes.
fn mode_row(ui: &mut Ui, with_layers: bool) -> NavMode {
    let mut mode = with(|s| s.nav_mode);
    ui.horizontal(|ui| {
        if with_layers
            && ui
                .selectable_label(mode == NavMode::Layers, "Layers")
                .clicked()
        {
            mode = NavMode::Layers;
        }
        for (m, label) in [
            (NavMode::Nets, "Nets"),
            (NavMode::Footprints, "Footprints"),
            (NavMode::Zones, "Zones"),
        ] {
            if ui.selectable_label(mode == m, label).clicked() {
                mode = m;
            }
        }
    });
    with(|s| s.nav_mode = mode);
    mode
}

/// The one Board panel.
pub fn board_panel(ui: &mut Ui, shell: &Shell) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let mut mode = mode_row(ui, false);
    if mode == NavMode::Layers {
        mode = NavMode::Nets;
        with(|s| s.nav_mode = mode);
    }
    nav_header(ui);
    ui.separator();
    nav_list(ui, shell, mode, &mut out);
    out
}

/// One of three separate Panels.
pub fn single_panel(ui: &mut Ui, shell: &Shell, mode: NavMode) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    nav_header(ui);
    ui.separator();
    nav_list(ui, shell, mode, &mut out);
    out
}

/// The Layers panel, with the navigator inside it when the axis says so.
pub fn layers_panel(ui: &mut Ui, shell: &mut Shell) -> Vec<(String, Value)> {
    if axes().nav != Nav::InLayers {
        return crate::panels::layers::show(ui, shell);
    }
    let mode = mode_row(ui, true);
    ui.separator();
    if mode == NavMode::Layers {
        return crate::panels::layers::show(ui, shell);
    }
    let mut out = Vec::new();
    nav_header(ui);
    ui.separator();
    nav_list(ui, shell, mode, &mut out);
    out
}

/// "Ready for fab?", at the end of Properties with nothing selected.
pub fn ready_for_fab(ui: &mut Ui, shell: &Shell, out: &mut Vec<(String, Value)>) {
    if !shell.selection().is_empty() {
        return;
    }
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        return;
    };
    ui.separator();
    egui::CollapsingHeader::new(RichText::new("Ready for fab?").strong())
        .default_open(true)
        .show(ui, |ui| {
            let not_fitted = pcb.footprints().filter(|f| f.not_fitted).count();
            let rows: [(bool, String, Option<(&str, &str)>); 6] = [
                (
                    pcb.outline().is_some(),
                    "Board Outline closed".to_owned(),
                    None,
                ),
                (
                    false,
                    "4 connections unrouted".to_owned(),
                    Some(("Show", "pcb.ratsnest.show")),
                ),
                (
                    false,
                    "Zones need refilling".to_owned(),
                    Some(("Fill", "pcb.zone.fill")),
                ),
                (
                    false,
                    "DRC not run since the last edit".to_owned(),
                    Some(("Run", "panel.show:messages")),
                ),
                (true, "In sync with the schematic".to_owned(), None),
                (true, format!("{not_fitted} Not Fitted"), None),
            ];
            for (ok, text, action) in rows {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        if ok {
                            Color32::from_rgb(70, 170, 90)
                        } else {
                            Color32::from_rgb(220, 150, 40)
                        },
                        if ok { "OK" } else { "!" },
                    );
                    ui.label(text);
                    if let Some((label, command)) = action
                        && ui.small_button(label).clicked()
                    {
                        out.push((command.to_owned(), json!({})));
                    }
                });
            }
        });
}

// ---- Tabs for the settings surfaces ------------------------------------------------------

/// Board Setup in a tab.
pub fn board_setup_tab(ui: &mut Ui, shell: &mut Shell) {
    let Some(mut dialog) = with(|s| s.setup.take()) else {
        ui.weak("Board Setup is closed. Open it from the Project menu.");
        return;
    };
    let height = ui.available_height() - 64.0;
    let _ = dialog.body(ui, shell, height);
    with(|s| s.setup = Some(dialog));
}

/// The button that hides the dialog or shows the board, on either surface.
pub fn show_on_board_button(ui: &mut Ui) {
    if ui
        .button("Show on board")
        .on_hover_text("On each rule and grid row in the build; one button here for the prototype.")
        .clicked()
    {
        with(|s| {
            s.on_board = true;
            if s.setup.is_some() {
                s.pending.push(("proto.focus_pcb".to_owned(), json!({})));
            }
        });
    }
}

// ---- The Stackup page --------------------------------------------------------------------

/// Board Setup's Stackup page, mocked: an editable table beside a live cross-section.
#[derive(Default)]
pub struct StackupPage;

impl Page for StackupPage {
    fn title(&self) -> String {
        "Stackup".to_owned()
    }
    fn load(&mut self, _project: &reda_engine::model::Project, _reset: bool) {}
    fn steps(&self) -> Vec<Step> {
        Vec::new()
    }
    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }
    fn show(&mut self, ui: &mut Ui, _cx: &mut PageCx<'_>) {
        let mut rows = with(|s| std::mem::take(&mut s.stackup));
        ui.columns(2, |cols| {
            egui::Grid::new("stackup-table")
                .striped(true)
                .show(&mut cols[0], |ui| {
                    ui.weak("Name");
                    ui.weak("Kind");
                    ui.weak("um");
                    ui.end_row();
                    for (name, kind, thickness) in rows.iter_mut() {
                        ui.label(name.as_str());
                        ui.weak(kind.as_str());
                        ui.add(egui::DragValue::new(thickness).range(1..=5000));
                        ui.end_row();
                    }
                });
            let total: u32 = rows.iter().map(|r| r.2).sum();
            cols[0].add_space(6.0);
            cols[0].label(format!(
                "Total {:.2} mm (target 1.60 mm)",
                total as f32 / 1000.0
            ));
            let ui = &mut cols[1];
            ui.weak("Cross-section (not to scale)");
            let width = ui.available_width();
            for (name, kind, thickness) in &rows {
                let h = (*thickness as f32).sqrt().clamp(5.0, 44.0);
                let colour = match kind.as_str() {
                    "Copper" => Color32::from_rgb(196, 120, 50),
                    "Solder Mask" => Color32::from_rgb(40, 110, 70),
                    "Silkscreen" => Color32::from_rgb(225, 225, 225),
                    _ => Color32::from_rgb(170, 160, 110),
                };
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
                let bar = Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.left() + width * 0.5, rect.bottom()),
                );
                ui.painter().rect_filled(bar, 0.0, colour);
                ui.painter().text(
                    egui::pos2(bar.right() + 6.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    format!("{name} - {thickness} um"),
                    egui::TextStyle::Small.resolve(ui.style()),
                    ui.visuals().text_color(),
                );
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Finish");
            let _ = ui.button("ENIG v");
            ui.label("Mask");
            let _ = ui.button("Green v");
            ui.label("Silk");
            let _ = ui.button("White v");
        });
        ui.label("Fab Notes");
        let mut note = String::from("Lead-free. Impedance not controlled.");
        ui.add(egui::TextEdit::multiline(&mut note).desired_rows(2));
        with(|s| s.stackup = rows);
    }
}

// ---- Palette -----------------------------------------------------------------------------

/// Each Command's menu path, read from the bars.
pub fn menu_paths(document: Option<DocumentType>) -> BTreeMap<String, String> {
    fn walk(items: &[Item], path: &str, out: &mut BTreeMap<String, String>) {
        for item in items {
            match item {
                Item::Action(action) => {
                    let id = match &action.binding {
                        bars::Binding::Command(id) | bars::Binding::Preset(id) => id.clone(),
                        bars::Binding::Raw { command, .. } => command.clone(),
                    };
                    out.entry(id).or_insert_with(|| path.to_owned());
                }
                Item::Submenu(menu) => {
                    let name = crate::i18n::translate(&menu.title, &[]);
                    walk(&menu.items, &format!("{path} > {name}"), out);
                }
                Item::Separator => {}
            }
        }
    }
    let mut out = BTreeMap::new();
    for menu in &bars_for(document).menus {
        let name = crate::i18n::translate(&menu.title, &[]);
        walk(&menu.items, &name, &mut out);
    }
    out
}

/// The go-to rows: Components, Nets and Documents.
pub fn go_to_entries(shell: &Shell) -> Vec<crate::palette::Entry> {
    let mut out = Vec::new();
    let entry = |id: String, title: String, icon: &str, what: &str| crate::palette::Entry {
        id,
        title,
        icon: icon.to_owned(),
        description: what.to_owned(),
        shortcut: None,
        unavailable: None,
        path: Some(what.to_owned()),
    };
    let Some(project) = shell.engine().project() else {
        return out;
    };
    if let Some(pcb) = project.pcb() {
        for f in pcb.footprints() {
            out.push(entry(
                format!("goto:@:{}", f.uuid),
                f.designator.clone(),
                "footprint",
                "Go to Component",
            ));
        }
        for n in pcb.nets() {
            out.push(entry(
                format!("goto:#:{}", n.uuid),
                n.name.clone(),
                "net",
                "Highlight Net",
            ));
        }
    }
    for d in project.documents() {
        out.push(entry(
            format!("goto::::{}", d.uuid),
            shell
                .document_title(&crate::layout::TabKey::Document(d.uuid))
                .unwrap_or("Document")
                .to_owned(),
            icons::document_icon(d.kind),
            "Open Document",
        ));
    }
    out
}

/// Runs a go-to row. False when `id` isn't one.
pub fn go_to(shell: &mut Shell, id: &str) -> bool {
    let Some(rest) = id.strip_prefix("goto:") else {
        return false;
    };
    let Some((kind, uuid)) = rest.rsplit_once(':') else {
        return true;
    };
    let Ok(uuid) = uuid.parse::<Uuid>() else {
        return true;
    };
    match kind {
        "@" => reveal(shell, uuid),
        "#" => {
            let _ = shell.run_reporting("selection.set", json!({ "nets": [uuid] }));
        }
        _ => {
            let _ = shell.run_reporting("document.open", json!({ "document": uuid }));
        }
    }
    true
}

fn reveal(shell: &mut Shell, object: Uuid) {
    let pcb = shell
        .engine()
        .project()
        .and_then(|p| p.pcb())
        .map(|p| p.uuid);
    if let Some(pcb) = pcb {
        shell.reveal(pcb, &[object]);
    }
}

// ---- The frame ---------------------------------------------------------------------------

/// Handles the Commands only the prototype understands. True when `id` was one.
pub fn run_own(shell: &mut Shell, id: &str, params: &Value) -> bool {
    match id {
        "proto.reveal" => {
            if let Some(uuid) = params["object"].as_str().and_then(|s| s.parse().ok()) {
                reveal(shell, uuid);
            }
            true
        }
        "proto.focus_pcb" => {
            let pcb = shell
                .engine()
                .project()
                .and_then(|p| p.pcb())
                .map(|p| p.uuid);
            if let Some(pcb) = pcb {
                let _ = shell.run("document.open", json!({ "document": pcb }));
            }
            true
        }
        _ => false,
    }
}

/// Early in each frame: applies a changed axis to the layout, and the key-card key.
pub fn begin_frame(ctx: &egui::Context, shell: &mut Shell) {
    let (axes, applied) = with(|s| (s.axes, s.applied));
    if applied != Some(axes) {
        apply_layout(shell, axes);
        with(|s| s.applied = Some(axes));
    }
    let f1 = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F1));
    if f1 {
        with(|s| s.key_card = !s.key_card);
    }
    for (id, params) in with(|s| std::mem::take(&mut s.pending)) {
        if !run_own(shell, &id, &params) {
            let _ = shell.run_reporting(&id, params);
        }
    }
}

/// Late in each frame: the switcher, and Preferences as a modal.
pub fn end_frame(ctx: &egui::Context, shell: &mut Shell) {
    if with(|s| s.prefs_modal) {
        let mut close = false;
        egui::Modal::new(Id::new("proto-prefs-modal")).show(ctx, |ui| {
            ui.set_width(860.0);
            ui.set_height(520.0);
            prefs::show(ui, shell);
            ui.separator();
            ui.horizontal(|ui| {
                ui.weak("Changes apply at once.");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui.button("Close").clicked();
                });
            });
        });
        if close {
            with(|s| s.prefs_modal = false);
        }
    }
    switcher(ctx);
}

/// A left-aligned row in a page tree.
pub fn tree_row(ui: &mut Ui, selected: bool, text: RichText) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 19.0), Sense::click());
    let v = ui.visuals();
    let fill = if selected {
        v.selection.bg_fill
    } else if response.hovered() {
        v.widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
    let colour = if selected {
        v.strong_text_color()
    } else {
        v.text_color()
    };
    let text = text.color(colour);
    let inner = rect.shrink2(Vec2::new(4.0, 0.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.add(egui::Label::new(text).selectable(false).truncate());
        },
    );
    response
}
