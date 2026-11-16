//! The bars page: the Active Bar's buttons in every state (its Tools, More Tools, the
//! Selection Filter and the snap palette), whole bars on the floating surface, the
//! Command Palette's rows and window, and the Keys in Tool card.

use egui::{Ui, Vec2, vec2};
use reda_ui::theme::TokensExt as _;
use reda_ui::widgets::{self, Glyph, State};

use crate::kit;

/// The size the page is snapshotted at.
pub const SIZE: Vec2 = vec2(1400.0, 1180.0);
/// The width of a state column.
const COLUMN: f32 = 180.0;
/// The width of a Palette drawn whole, and of a row in the state grid.
const PALETTE: f32 = 560.0;

pub fn page(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.section, t.space.section);
    kit::section(ui, "Active Bar buttons", buttons);
    kit::section(ui, "Bars on the canvas", canvas_bars);
    kit::section(ui, "Command Palette rows", palette_rows);
    ui.horizontal_top(|ui| {
        kit::section(ui, "Command Palette", palette);
        kit::section(ui, "Keys in Tool card", keys_card);
    });
}

fn buttons(ui: &mut Ui) {
    kit::states(
        ui,
        "bar-buttons",
        COLUMN,
        &mut [
            ("Tool", &mut |ui, s| {
                ui.add(widgets::icon_button("wire").bar().forced(s));
            }),
            ("Active Tool (on)", &mut |ui, s| {
                ui.add(widgets::icon_button("select").bar().on(true).forced(s));
            }),
            ("More Tools", &mut |ui, s| {
                ui.add(widgets::glyph_button(Glyph::ChevronDown).forced(s));
            }),
            ("Selection Filter", &mut |ui, s| {
                ui.add(widgets::glyph_button(Glyph::Funnel).forced(s));
            }),
            ("Snap palette", &mut |ui, s| {
                ui.add(widgets::glyph_button(Glyph::Snap).forced(s));
            }),
        ],
    );
}

fn canvas_bars(ui: &mut Ui) {
    let t = ui.tokens();
    kit::row(ui, "Active Bar", COLUMN, |ui| {
        widgets::bar_frame(&t).show(ui, |ui| {
            widgets::bar_row(ui, |ui| {
                ui.add(widgets::icon_button("select").bar().on(true));
                ui.add(widgets::icon_button("move").bar());
                ui.add(widgets::icon_button("drag").bar());
                widgets::bar_separator(ui);
                ui.add(widgets::icon_button("wire").bar());
                ui.add(widgets::icon_button("net").bar());
                ui.add(widgets::glyph_button(Glyph::ChevronDown));
                ui.add(widgets::glyph_button(Glyph::Funnel));
                ui.add(widgets::glyph_button(Glyph::Snap));
            });
        });
    });
    kit::row(ui, "Tool's bar with a note", COLUMN, |ui| {
        ui.vertical(|ui| {
            widgets::bar_frame(&t).show(ui, |ui| {
                ui.label(t.ty.body.rich("Click to start a track").color(t.color.text.primary));
            });
            widgets::bar_frame(&t).show(ui, |ui| {
                widgets::bar_row(ui, |ui| {
                    ui.add(widgets::icon_button("track").bar().on(true));
                    ui.add(widgets::icon_button("via").bar());
                    widgets::bar_separator(ui);
                    ui.add(widgets::button("0.25 mm").flat());
                    ui.add(widgets::icon_button("undo").bar().enabled(false));
                });
            });
        });
    });
}

fn palette_rows(ui: &mut Ui) {
    let row = |text: &'static str, icon: &'static str| {
        move |ui: &mut Ui, s: State| {
            ui.set_width(COLUMN);
            ui.add(widgets::palette_row(text).icon(icon).forced(s));
        }
    };
    let (mut plain, mut noted) = (row("Sheet 1", "net"), row("Save All", "save_all"));
    kit::states(
        ui,
        "palette-rows",
        COLUMN,
        &mut [
            ("Row", &mut plain),
            ("Row with an Icon", &mut noted),
            ("Selected (live)", &mut |ui, s| {
                ui.set_width(COLUMN);
                ui.add(
                    widgets::palette_row("Undo")
                        .icon("undo")
                        .shortcut("Ctrl+Z")
                        .selected(true)
                        .forced(s),
                );
            }),
        ],
    );
}

fn palette(ui: &mut Ui) {
    let t = ui.tokens();
    let mut query = String::from(">sav");
    widgets::palette_frame(&t).show(ui, |ui| {
        ui.set_width(PALETTE);
        ui.add(widgets::text_field(&mut query).search().width(f32::INFINITY));
        ui.add_space(t.space.row_y);
        widgets::rows(ui, |ui| {
            ui.add(
                widgets::palette_row("Save All")
                    .icon("save_all")
                    .note("File › Save All")
                    .shortcut("Ctrl+S")
                    .selected(true),
            );
            ui.add(
                widgets::palette_row("Save")
                    .icon("save")
                    .note("File › Save")
                    .note("recently used"),
            );
            ui.add_enabled(
                false,
                widgets::palette_row("Redo")
                    .icon("redo")
                    .note("Edit › Redo")
                    .shortcut("Ctrl+Y")
                    .note("Nothing to redo"),
            );
            ui.add(
                widgets::palette_row("A row whose title is far too long to fit beside its notes")
                    .icon("move")
                    .note("Edit › Move")
                    .shortcut("M"),
            );
        });
    });
}

fn keys_card(ui: &mut Ui) {
    let t = ui.tokens();
    let rows: Vec<(String, String)> = [
        ("/", "Change Wire Corner"),
        ("Backspace", "Undo Last Segment"),
        ("Esc", "Cancel"),
        ("Shift+W", "Select Width"),
        ("W", "Wire"),
    ]
    .into_iter()
    .map(|(k, a)| (k.to_owned(), a.to_owned()))
    .collect();
    widgets::floating_frame(&t).show(ui, |ui| {
        let title = t.ty.body_strong.rich("Wire keys");
        ui.label(title.color(t.color.text.primary));
        widgets::key_list(ui, "gallery-keys", &rows);
    });
}
