//! The shell page: tabs in every state, a tab group as a card with a masked canvas, the
//! status bar's items, and what a dragged tab shows. All of it sits on the window's base,
//! so the page has no section cards.

use egui::{Rect, Sense, Ui, Vec2, vec2};
use reda_ui::theme::TokensExt as _;
use reda_ui::widgets::{self, DropPart, State, Status, TabKind};

use crate::kit;

/// The size the page is snapshotted at.
pub const SIZE: Vec2 = vec2(1280.0, 850.0);
/// The width of a state column.
const COLUMN: f32 = 190.0;
/// The size of the tab groups drawn whole.
const GROUP: Vec2 = vec2(420.0, 150.0);
/// The size of a drop preview.
const PREVIEW: Vec2 = vec2(200.0, 110.0);

pub fn page(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.section, t.space.stack);
    heading(ui, "Tabs");
    tabs(ui);
    ui.add_space(t.space.section);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            heading(
                ui,
                "Tab group: a card, its front tab joined, a canvas masked to its corners",
            );
            group(ui, true);
        });
        ui.vertical(|ui| {
            heading(
                ui,
                "REDA's window in the background: the accent line goes neutral",
            );
            group(ui, false);
        });
    });
    ui.add_space(t.space.section);
    heading(ui, "Status bar");
    status(ui);
    ui.add_space(t.space.section);
    heading(
        ui,
        "A dragged tab: drop previews, drop-target buttons and the tab's chip",
    );
    drops(ui);
}

fn heading(ui: &mut Ui, text: &str) {
    let t = ui.tokens();
    ui.label(t.ty.title.rich(text).color(t.color.text.primary));
}

/// Each kind of tab in the five states.
pub fn tabs(ui: &mut Ui) {
    let tab =
        |ui: &mut Ui, s: State, kind: TabKind, icon: &str, title: &str, marks: (bool, bool)| {
            widgets::tab(title)
                .icon(Some(icon))
                .kind(kind)
                .modified(marks.0)
                .out_of_sync(marks.1)
                .window_focused(true)
                .forced(s)
                .show(ui);
        };
    kit::states(
        ui,
        "tabs",
        COLUMN,
        &mut [
            ("Active Document", &mut |ui, s| {
                tab(
                    ui,
                    s,
                    TabKind::Active,
                    "document_pcb",
                    "usb_c_mcu.pcb",
                    (false, false),
                );
            }),
            ("Active, modified", &mut |ui, s| {
                tab(
                    ui,
                    s,
                    TabKind::Active,
                    "document_schematic",
                    "Sheet 1",
                    (true, false),
                );
            }),
            ("Front (a Panel)", &mut |ui, s| {
                tab(
                    ui,
                    s,
                    TabKind::Front,
                    "panel_projects",
                    "Projects",
                    (false, false),
                );
            }),
            ("Front, out of sync", &mut |ui, s| {
                tab(
                    ui,
                    s,
                    TabKind::Front,
                    "document_pcb",
                    "Board",
                    (false, true),
                );
            }),
            ("Back", &mut |ui, s| {
                tab(
                    ui,
                    s,
                    TabKind::Back,
                    "panel_navigator",
                    "Navigator",
                    (false, false),
                );
            }),
            ("Back, modified", &mut |ui, s| {
                tab(
                    ui,
                    s,
                    TabKind::Back,
                    "document_schematic",
                    "Power",
                    (true, false),
                );
            }),
        ],
    );
}

/// A tab group drawn as the docking layer draws one: the strip on the base, the card,
/// a canvas that fills it, then the corner masks and the join under the front tab.
pub fn group(ui: &mut Ui, window_focused: bool) {
    let t = ui.tokens();
    let (rect, _) = ui.allocate_exact_size(GROUP, Sense::hover());
    let strip = rect.with_max_y(rect.top() + t.tab_height());
    let body = rect.with_min_y(strip.bottom());
    widgets::paint_card(ui, &t, body);
    let mut front = Rect::NOTHING;
    let builder = egui::UiBuilder::new()
        .max_rect(strip)
        .layout(egui::Layout::left_to_right(egui::Align::Max));
    ui.scope_builder(builder, |ui| {
        widgets::tab_strip(ui, |ui| {
            let show = |ui: &mut Ui, kind: TabKind, icon: &str, title: &str| {
                widgets::tab(title)
                    .icon(Some(icon))
                    .kind(kind)
                    .window_focused(window_focused)
                    .forced(State::Rest)
                    .show(ui)
                    .response
                    .rect
            };
            show(ui, TabKind::Back, "document_schematic", "Sheet 1");
            front = show(ui, TabKind::Active, "document_pcb", "usb_c_mcu.pcb");
            show(ui, TabKind::Back, "document_schematic", "Power");
        });
    });
    // What a canvas paints: a square fill over the whole card.
    ui.painter().rect_filled(body, 0.0, t.color.surface.well);
    widgets::mask_card_corners(ui, &t, body);
    widgets::join_tab(ui, &t, front);
}

/// The status bar's items on the base.
pub fn status(ui: &mut Ui) {
    let t = ui.tokens();
    let size = vec2(ui.available_width().min(1000.0), t.status_bar_height());
    ui.allocate_ui(size, |ui| {
        widgets::status_bar(ui, |ui| {
            ui.add(widgets::status_item("Active: PCB Document"));
            ui.add(widgets::status_item("X 121.920 mm  Y 48.260 mm").numeric());
            ui.add(widgets::status_item("ΔX 2.540 mm  ΔY 0.000 mm").numeric());
            ui.add(
                widgets::status_item("Grid: 0.100 mm")
                    .numeric()
                    .forced(State::Hover),
            );
            ui.add(widgets::status_item("Active Layer").forced(State::Press));
            ui.add(widgets::status_item("Top Copper").swatch_rgb(0xc8, 0x34, 0x34));
            ui.add(widgets::status_item("Tool: Route Track").forced(State::Focus));
            ui.add(widgets::status_item("out of sync: 3 changes").tone(Status::Warning));
            ui.add(widgets::status_item("Agents: off").forced(State::Disabled));
            ui.add(widgets::status_item("Panels"));
        });
    });
}

/// The drop previews, the five drop-target buttons of a group, and the dragged tab.
pub fn drops(ui: &mut Ui) {
    let t = ui.tokens();
    ui.horizontal(|ui| {
        for (refused, name) in [(false, "Would land here"), (true, "Refused")] {
            ui.vertical(|ui| {
                kit::caption(ui, name);
                let (rect, _) = ui.allocate_exact_size(PREVIEW, Sense::hover());
                widgets::drop_preview(ui.painter(), &t, rect, refused);
            });
        }
        ui.vertical(|ui| {
            kit::caption(ui, "Buttons; the pointer is on the middle one");
            let side = widgets::DROP_BUTTON;
            let step = side + t.space.gap / 2.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(3.0 * step), Sense::hover());
            let c = rect.center();
            for (part, offset, on) in [
                (DropPart::Whole, vec2(0.0, 0.0), true),
                (DropPart::Left, vec2(-step, 0.0), false),
                (DropPart::Right, vec2(step, 0.0), false),
                (DropPart::Up, vec2(0.0, -step), false),
                (DropPart::Down, vec2(0.0, step), false),
            ] {
                let at = Rect::from_center_size(c + offset, Vec2::splat(side));
                widgets::drop_button(ui.painter(), &t, at, part, on);
            }
        });
        ui.vertical(|ui| {
            kit::caption(ui, "The tab's title follows the pointer");
            let (rect, _) = ui.allocate_exact_size(vec2(160.0, 48.0), Sense::hover());
            widgets::drag_chip(ui, &t, rect.min + vec2(8.0, 8.0), "usb_c_mcu.pcb");
        });
    });
}
