//! The data page: list and tree rows in every state, a list, a tree, section headers with
//! form-field rows, tables, status marks, swatches, and badges and counts.

use egui::{Color32, Ui, Vec2, vec2};
use reda_ui::theme::TokensExt as _;
use reda_ui::widgets::{self, BadgeTone, Column, RowLook, Selection, Sorted, State, Status};

use crate::kit;

/// The size the page is snapshotted at.
pub const SIZE: Vec2 = vec2(1280.0, 1380.0);
/// The width of a state column, and of a row in one.
const COLUMN: f32 = 168.0;
/// The width of the cards that hold one list, tree or form.
const NARROW: f32 = 280.0;
/// The width of the card that holds the badges, as wide as the marks' card above it.
const BADGES: f32 = 376.0;
/// The width of the cards that hold a table.
const WIDE: f32 = 560.0;

/// Data colours: Layers' and Nets'. They are the user's data, so they are the same in
/// both themes.
const RED: Color32 = Color32::from_rgb(0xc8, 0x34, 0x34);
const YELLOW: Color32 = Color32::from_rgb(0xd4, 0xc0, 0x3c);
const GREEN: Color32 = Color32::from_rgb(0x2f, 0xb3, 0x6b);
const BLUE: Color32 = Color32::from_rgb(0x3d, 0x8f, 0xd6);
const WHITE: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf2);
const BLACK: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);

pub fn page(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.section, t.space.section);
    kit::section(ui, "Rows", rows);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            kit::section(ui, "List (Layers)", list);
            kit::section(ui, "Status marks and swatches", marks);
            kit::section(ui, "Badges and counts", badges);
        });
        ui.vertical(|ui| {
            kit::section(ui, "Tree (Projects)", tree);
            kit::section(ui, "Colour picker", picker);
        });
        ui.vertical(|ui| {
            kit::section(ui, "Section headers and form-field rows", properties);
        });
    });
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            kit::section(ui, "Table (Messages), its Panel focused", |ui| {
                messages(ui, "messages-live", Selection::Live);
            });
        });
        ui.vertical(|ui| {
            kit::section(ui, "Table with number columns (Net lengths)", lengths);
        });
    });
}

/// Every kind of row in the five states, and each way a row is marked.
pub fn rows(ui: &mut Ui) {
    let row = |text: &str, s: State| widgets::list_row(text).width(COLUMN).forced(s);
    kit::states(
        ui,
        "rows",
        COLUMN,
        &mut [
            ("List row", &mut |ui, s| {
                ui.add(row("Top Copper", s).swatch(RED));
            }),
            ("Selected", &mut |ui, s| {
                ui.add(row("Top Copper", s).selection(Selection::Live));
            }),
            ("Selected, focus elsewhere", &mut |ui, s| {
                ui.add(row("Top Copper", s).selection(Selection::Unfocused));
            }),
            ("Drop target", &mut |ui, s| {
                ui.add(row("Libraries", s).drop_target(true));
            }),
            ("Muted, with a count", &mut |ui, s| {
                ui.add(row("Inner 2", s).muted(true).trailing("12"));
            }),
            ("Tree row", &mut |ui, s| {
                let row = widgets::tree_row("Schematic", 0).open(Some(true));
                ui.add(row.icon("document_schematic").width(COLUMN).forced(s));
            }),
            ("Section header", &mut |ui, s| {
                let header = widgets::section_header("Track", true).count("3");
                ui.add(header.width(COLUMN).forced(s));
            }),
            ("Swatch as a button", &mut |ui, s| {
                ui.add(widgets::swatch(BLUE).button("Colour").forced(s));
            }),
        ],
    );
}

/// The Layers list: a swatch and a name, the active Layer selected, a hidden one muted.
pub fn list(ui: &mut Ui) {
    ui.set_width(NARROW);
    let layers = [
        ("Top Copper", RED, true),
        ("Inner 1", YELLOW, true),
        ("Inner 2", GREEN, false),
        ("Bottom Copper", BLUE, true),
        ("Top Silkscreen", WHITE, true),
        ("Bottom Silkscreen", BLACK, true),
    ];
    widgets::rows(ui, |ui| {
        for (i, (name, colour, visible)) in layers.into_iter().enumerate() {
            let selection = if i == 0 {
                Selection::Live
            } else {
                Selection::None
            };
            let state = if i == 1 { State::Hover } else { State::Rest };
            let row = widgets::list_row(name).swatch(colour).muted(!visible);
            let row = if visible { row } else { row.trailing("hidden") };
            ui.add(row.selection(selection).forced(state));
        }
    });
}

/// A tree: 16 px a level, a chevron on what opens, an Icon and the name.
pub fn tree(ui: &mut Ui) {
    ui.set_width(NARROW);
    type Node = (
        usize,
        Option<bool>,
        &'static str,
        &'static str,
        State,
        Selection,
    );
    let nodes: [Node; 9] = [
        (
            0,
            Some(true),
            "dock",
            "usb_c_mcu",
            State::Rest,
            Selection::None,
        ),
        (
            1,
            Some(true),
            "document_schematic",
            "Schematic",
            State::Rest,
            Selection::None,
        ),
        (
            2,
            None,
            "document_schematic",
            "Sheet 1",
            State::Rest,
            Selection::None,
        ),
        (
            2,
            None,
            "document_schematic",
            "Power",
            State::Rest,
            Selection::Live,
        ),
        (
            2,
            None,
            "document_schematic",
            "USB",
            State::Hover,
            Selection::None,
        ),
        (
            1,
            None,
            "document_pcb",
            "usb_c_mcu.pcb",
            State::Rest,
            Selection::Unfocused,
        ),
        (
            1,
            Some(false),
            "render",
            "Output Jobs",
            State::Focus,
            Selection::None,
        ),
        (
            0,
            Some(false),
            "panel_library",
            "Libraries",
            State::Disabled,
            Selection::None,
        ),
        (
            0,
            Some(false),
            "panel_projects",
            "Another Project",
            State::Rest,
            Selection::None,
        ),
    ];
    widgets::rows(ui, |ui| {
        for (depth, open, icon, name, state, selection) in nodes {
            let row = widgets::tree_row(name, depth).open(open).icon(icon);
            let row = row.selection(selection);
            ui.add(row.drop_target(name == "Another Project").forced(state));
        }
    });
}

/// Status marks, and swatches of data colours on the card.
pub fn marks(ui: &mut Ui) {
    let t = ui.tokens();
    ui.set_width(NARROW);
    ui.horizontal(|ui| {
        for (status, name) in Status::ALL {
            ui.add(widgets::status_mark(status));
            ui.label(t.ty.body.rich(name).color(t.color.text.primary));
        }
    });
    ui.horizontal(|ui| {
        for colour in [RED, YELLOW, GREEN, BLUE, WHITE, BLACK] {
            ui.add(widgets::swatch(colour));
        }
        let card = t.color.surface.card;
        ui.add(widgets::swatch(card));
        kit::caption(ui, "the last is the card's own colour");
    });
}

/// The colour picker a swatch button opens, drawn in place: the field of saturation and
/// brightness, the hues, and the colour as text with its copy button.
pub fn picker(ui: &mut Ui) {
    ui.set_width(NARROW);
    let mut colour = BLUE;
    widgets::color_picker(ui, egui::Id::NULL.with("gallery-picker"), &mut colour);
}

/// Badges and counts: a pill of small text on each tone's subtle tint, tallies with
/// their status mark, the filter toggles of a list's header on and off and in their
/// states, and count pills.
pub fn badges(ui: &mut Ui) {
    ui.set_width(BADGES);
    ui.horizontal_wrapped(|ui| {
        for (tone, name) in BadgeTone::ALL {
            ui.add(widgets::badge(name).tone(tone));
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.add(widgets::badge("3 Errors").status(Status::Error));
        ui.add(widgets::badge("12 Warnings").status(Status::Warning));
        ui.add(widgets::badge("Clean").status(Status::Success));
        ui.add(widgets::badge("2 waived").status(Status::Off));
    });
    ui.horizontal_wrapped(|ui| {
        let filter = |on: bool, state: State| {
            let badge = widgets::badge("3 Errors").status(Status::Error);
            badge.filter(on).forced(state)
        };
        ui.add(filter(true, State::Rest));
        ui.add(filter(true, State::Hover));
        ui.add(filter(true, State::Focus));
        ui.add(filter(false, State::Rest));
        ui.add(filter(false, State::Disabled));
    });
    ui.horizontal(|ui| {
        for count in [3, 12, 128, 1024] {
            ui.add(widgets::count_pill(count));
        }
        kit::caption(ui, "counts");
    });
}

/// A Properties Panel: section headers with a count, and rows of a muted name and a
/// value, the values in one column.
pub fn properties(ui: &mut Ui) {
    let t = ui.tokens();
    ui.set_width(NARROW);
    ui.spacing_mut().item_spacing.y = t.space.stack;
    let named = |response: egui::Response, name: &str| {
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, name));
    };
    ui.add(widgets::section_header("Track", true).count("3 selected"));
    widgets::form("gallery-track").show(ui, |ui| {
        widgets::form_row(ui, "Net", |ui| {
            let net = widgets::dropdown("gallery-net", "GND")
                .width(150.0)
                .show(ui, |_| ());
            named(net.response, "Net");
        });
        widgets::form_row(ui, "Width", |ui| {
            let mut width = "0.250".to_owned();
            let field = widgets::number_field(&mut width).unit("mm").width(150.0);
            named(ui.add(field), "Width");
        });
        widgets::form_row(ui, "Length", |ui| {
            ui.label(t.ty.numeric.rich("12.446 mm").color(t.color.text.primary));
        });
        widgets::form_row(ui, "Locked", |ui| {
            let mut locked = false;
            ui.add(widgets::checkbox(&mut locked, "").indeterminate(true))
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::CheckBox, true, "Locked"));
        });
    });
    ui.add(widgets::section_header("Position", true));
    widgets::form("gallery-position").show(ui, |ui| {
        widgets::form_row(ui, "Start", |ui| {
            let mut x = "121.920".to_owned();
            named(ui.add(widgets::number_field(&mut x).width(150.0)), "Start");
        });
    });
    ui.add(
        widgets::section_header("Net Class", false)
            .count("Power")
            .forced(State::Hover),
    );
    ui.add(widgets::section_header("Rules that apply", false).count("4"));
}

/// The Messages table: a severity mark, the message, where it is, and two numbers.
pub fn messages(ui: &mut Ui, id: &str, selected: Selection) {
    ui.set_width(WIDE);
    type Line = (
        Status,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        RowLook,
    );
    let rest = RowLook::default();
    let lines: [Line; 6] = [
        (
            Status::Error,
            "Clearance between Track and Pad",
            "U1 pin 4 · Top Copper",
            "0.127",
            "0.150",
            RowLook::selected(selected),
        ),
        (
            Status::Error,
            "Track narrower than the Net Class allows",
            "GND · Top Copper",
            "0.150",
            "0.250",
            rest.forced(State::Hover),
        ),
        (
            Status::Warning,
            "Silkscreen over a Pad",
            "C3 · Top Silkscreen",
            "",
            "",
            RowLook::selected(Selection::Unfocused),
        ),
        (
            Status::Off,
            "Clearance between Zone and Via (waived)",
            "GND · Inner 1",
            "0.140",
            "0.150",
            rest,
        ),
        (
            Status::Warning,
            "Unconnected pin",
            "J2 pin 7 · Sheet 1",
            "",
            "",
            rest.forced(State::Disabled),
        ),
        (
            Status::Success,
            "A row something is dragged over",
            "Sheet 2",
            "111.110",
            "111.111",
            rest.drop_target(true),
        ),
    ];
    widgets::table(id)
        .columns([
            Column::auto(),
            Column::fill(),
            Column::auto(),
            Column::auto().number(),
            Column::auto().number(),
        ])
        .show(ui, |table| {
            table.header(|h| {
                h.empty();
                let by_message = Sorted {
                    ascending: true,
                    rank: Some(1),
                };
                h.sort("Message", Some(by_message));
                let by_place = Sorted {
                    ascending: false,
                    rank: Some(2),
                };
                h.sort("Where", Some(by_place));
                h.sort("Actual", None);
                h.text("Required");
            });
            for (status, message, place, actual, required, look) in lines {
                let waived = status == Status::Off;
                table.row_with(look, |row| {
                    row.status(status);
                    if waived {
                        row.muted(message);
                    } else {
                        row.text(message);
                    }
                    row.muted(place);
                    if waived {
                        row.muted(actual);
                        row.muted(required);
                    } else {
                        row.text(actual);
                        row.text(required);
                    }
                });
            }
        });
}

/// A table of numbers: right-aligned in tabular digits, a swatch leading each Net.
pub fn lengths(ui: &mut Ui) {
    let t = ui.tokens();
    ui.set_width(WIDE);
    let nets = [
        (BLUE, "USB_D+", "41.118", "2", "+0.012", false),
        (BLUE, "USB_D−", "41.106", "2", "0.000", false),
        (RED, "VBUS", "111.110", "11", "", false),
        (GREEN, "SWDIO", "8.890", "0", "−1.270", true),
        (YELLOW, "SWCLK", "17.071", "1", "−0.111", false),
    ];
    widgets::table("gallery-lengths")
        .columns([
            Column::auto(),
            Column::fill(),
            Column::fixed(90.0).number(),
            Column::fixed(60.0).number(),
            Column::fixed(90.0).number(),
        ])
        .show(ui, |table| {
            table.header(|h| {
                h.empty();
                h.text("Net");
                h.text("Length");
                h.text("Vias");
                h.text("Δ to target");
            });
            for (colour, net, length, vias, delta, bad) in nets {
                table.row(|row| {
                    row.ui(|ui| ui.add(widgets::swatch(colour)));
                    row.text(net);
                    row.text(length);
                    row.text(vias);
                    if bad {
                        row.colored(delta, t.color.error.text);
                    } else {
                        row.text(delta);
                    }
                });
            }
        });
}
