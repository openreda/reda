//! The controls page: buttons, fields, the dropdown, check box, radio and switch, slider
//! and progress, the segmented control, the stepper field, the filterable dropdown, and
//! the Glyph set.

use egui::{Frame, Stroke, Ui, Vec2, vec2};
use reda_ui::theme::TokensExt as _;
use reda_ui::widgets::{self, Glyph, State};

use crate::kit;

/// The size the page is snapshotted at.
pub const SIZE: Vec2 = vec2(1660.0, 1600.0);
/// The width of a state column, and of a field in one.
const COLUMN: f32 = 112.0;
const FIELD: f32 = 112.0;
/// The width of a state column that holds a segmented control.
const WIDE: f32 = 236.0;

pub fn page(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.section, t.space.section);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            kit::section(ui, "Buttons", buttons);
            kit::section(ui, "Check box, radio and switch", checks);
        });
        ui.vertical(|ui| {
            kit::section(ui, "Fields and the dropdown", fields);
            kit::section(ui, "Slider and progress", ranges);
            kit::section(ui, "Glyphs", glyphs);
        });
    });
    kit::section(
        ui,
        "Segmented control, stepper field and filterable dropdown",
        new_controls,
    );
}

/// Names a control that the gallery shows without a label of its own.
fn named(response: &egui::Response, role: egui::Role, name: &str) {
    response.widget_info(|| egui::WidgetInfo::labeled(role, true, name));
}

pub fn buttons(ui: &mut Ui) {
    kit::states(
        ui,
        "buttons",
        COLUMN,
        &mut [
            ("Primary", &mut |ui, s| {
                ui.add(widgets::button("Run DRC").primary().forced(s));
            }),
            ("Filled", &mut |ui, s| {
                ui.add(widgets::button("Cancel").forced(s));
            }),
            ("Flat", &mut |ui, s| {
                ui.add(widgets::button("Reset").flat().forced(s));
            }),
            ("Destructive", &mut |ui, s| {
                ui.add(widgets::button("Delete").danger().forced(s));
            }),
            ("Toggle, on", &mut |ui, s| {
                ui.add(widgets::button("Snap").on(true).forced(s));
            }),
            ("With icon", &mut |ui, s| {
                ui.add(widgets::button("Fit").icon("zoom_fit").forced(s));
            }),
            ("Icon button, off and on", &mut |ui, s| {
                ui.add(widgets::icon_button("wire").forced(s));
                ui.add(widgets::icon_button("track").on(true).forced(s));
            }),
            ("Icon button, bar size", &mut |ui, s| {
                ui.add(widgets::icon_button("wire").bar().forced(s));
                ui.add(widgets::icon_button("track").bar().on(true).forced(s));
            }),
            ("Link", &mut |ui, s| {
                ui.add(widgets::link("Open the guide").forced(s));
            }),
        ],
    );
}

pub fn fields(ui: &mut Ui) {
    let field = |text: &str| text.to_owned();
    kit::states(
        ui,
        "fields",
        COLUMN,
        &mut [
            ("Text field", &mut |ui, s| {
                let mut text = field("GND");
                ui.add(widgets::text_field(&mut text).width(FIELD).forced(s));
            }),
            ("Empty, hint", &mut |ui, s| {
                let mut text = field("");
                let hint = widgets::text_field(&mut text).hint("Net name");
                ui.add(hint.width(FIELD).forced(s));
            }),
            ("Number field", &mut |ui, s| {
                let mut text = field("0.250");
                let number = widgets::number_field(&mut text).unit("mm");
                ui.add(number.width(FIELD).forced(s));
            }),
            ("Invalid", &mut |ui, s| {
                let mut text = field("0.05");
                let number = widgets::number_field(&mut text).unit("mm").invalid(true);
                ui.add(number.width(FIELD).forced(s));
            }),
            ("Search", &mut |ui, s| {
                let mut text = field("");
                let search = widgets::text_field(&mut text).search().hint("Filter");
                ui.add(search.width(FIELD).forced(s));
            }),
            ("Dropdown", &mut |ui, s| {
                let dropdown = widgets::dropdown(("gallery-dropdown", s as u8), "Top Copper")
                    .forced(s)
                    .show(ui, |_| {});
                named(&dropdown.response, egui::Role::ComboBox, "Layer");
            }),
        ],
    );
    let t = ui.tokens();
    ui.add_space(t.space.stack);
    kit::row(ui, "Invalid, message", COLUMN, |ui| {
        let mut text = field("0.05");
        let number = widgets::number_field(&mut text).unit("mm").invalid(true);
        ui.add(number.width(FIELD).forced(State::Rest));
        ui.add(widgets::invalid_message(
            "Below the 0.1 mm minimum of the Fab Preset",
        ));
    });
    kit::row(ui, "Dropdown list", COLUMN, |ui| {
        list(ui, 180.0, |ui| {
            ui.add(widgets::choice(true, "Top Copper").forced(State::Rest));
            ui.add(widgets::choice(false, "Inner 1").forced(State::Hover));
            ui.add(widgets::choice(false, "Inner 2").forced(State::Press));
            ui.add(widgets::choice(false, "Bottom Copper").forced(State::Focus));
            ui.add(widgets::choice(false, "Board Edge").forced(State::Disabled));
        });
        kit::caption(ui, "chosen, hover, press, focus, disabled");
    });
}

/// Rows in the frame a dropdown's list is shown in, drawn in place: a popup cannot be
/// held open in a still image beside other things.
pub fn list(ui: &mut Ui, width: f32, rows: impl FnOnce(&mut Ui)) {
    let t = ui.tokens();
    Frame::new()
        .fill(t.color.surface.floating)
        .stroke(Stroke::new(t.stroke.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.popup)
        .shadow(t.elevation.popup)
        .inner_margin(t.space.menu)
        .show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.vertical(rows);
        });
}

pub fn checks(ui: &mut Ui) {
    kit::states(
        ui,
        "checks",
        COLUMN,
        &mut [
            ("Check box, off", &mut |ui, s| {
                ui.add(widgets::checkbox(&mut false, "Visible").forced(s));
            }),
            ("Check box, on", &mut |ui, s| {
                ui.add(widgets::checkbox(&mut true, "Visible").forced(s));
            }),
            ("Check box, mixed", &mut |ui, s| {
                let mut on = false;
                let mixed = widgets::checkbox(&mut on, "Visible").indeterminate(true);
                ui.add(mixed.forced(s));
            }),
            ("Radio, off", &mut |ui, s| {
                ui.add(widgets::radio(false, "Metric").forced(s));
            }),
            ("Radio, on", &mut |ui, s| {
                ui.add(widgets::radio(true, "Metric").forced(s));
            }),
            ("Switch, off", &mut |ui, s| {
                ui.add(widgets::switch(&mut false, "Animate").forced(s));
            }),
            ("Switch, on", &mut |ui, s| {
                ui.add(widgets::switch(&mut true, "Animate").forced(s));
            }),
        ],
    );
}

pub fn ranges(ui: &mut Ui) {
    kit::states(
        ui,
        "ranges",
        COLUMN,
        &mut [("Slider", &mut |ui, s| {
            let mut value = 0.6;
            let slider = widgets::slider(&mut value, 0.0..=1.0).show_value(false);
            let slider = ui.add(slider.width(FIELD).forced(s));
            slider.widget_info(|| egui::WidgetInfo::slider(true, 0.6, "Opacity"));
        })],
    );
    let t = ui.tokens();
    ui.add_space(t.space.stack);
    kit::row(ui, "Slider, value and label", COLUMN, |ui| {
        let mut seconds = 1.5;
        let slider = widgets::slider(&mut seconds, 0.1..=60.0).logarithmic(true);
        ui.add(
            slider
                .suffix(" s")
                .text("Time out after")
                .forced(State::Rest),
        );
    });
    kit::row(ui, "Progress", COLUMN, |ui| {
        ui.add(widgets::progress(0.4).width(150.0));
        ui.add(widgets::progress(1.0).width(150.0).success());
        ui.add(widgets::progress(0.7).width(150.0).error());
        kit::caption(ui, "running, finished, failed");
    });
}

/// The three choices of the Appearance page's Theme setting.
pub const THEMES: [(u8, &str); 3] = [(0, "System"), (1, "Light"), (2, "Dark")];
pub const DENSITIES: [(u8, &str); 2] = [(0, "Comfortable"), (1, "Compact")];

/// The entries of a font list: the default, a rule, then the installed families.
pub fn fonts() -> Vec<widgets::Choice<'static>> {
    let mut fonts = vec![widgets::Choice::new("Default (Inter)").rule_below()];
    fonts.extend(
        [
            "Atkinson Hyperlegible",
            "DejaVu Sans",
            "IBM Plex Sans",
            "Noto Sans",
            "Source Sans 3",
        ]
        .map(widgets::Choice::new),
    );
    fonts.push(widgets::Choice::new("Fira Sans").note("not installed"));
    fonts
}

pub fn new_controls(ui: &mut Ui) {
    kit::states(
        ui,
        "new-controls",
        // The segmented control is wider than the other controls.
        WIDE,
        &mut [
            ("Segmented, three", &mut |ui, s| {
                ui.add(widgets::segmented(&mut 1, &THEMES).forced(s));
            }),
            ("Segmented, two", &mut |ui, s| {
                ui.add(widgets::segmented(&mut 0, &DENSITIES).forced(s));
            }),
            ("Stepper field", &mut |ui, s| {
                let mut size = 13;
                ui.add(
                    widgets::stepper_field(&mut size, 11..=16)
                        .suffix("px")
                        .forced(s),
                );
            }),
            ("Stepper, at its top", &mut |ui, s| {
                let mut size = 16;
                ui.add(
                    widgets::stepper_field(&mut size, 11..=16)
                        .suffix("px")
                        .forced(s),
                );
            }),
            ("Filterable dropdown", &mut |ui, s| {
                let font = widgets::filter_dropdown(("gallery-filter", s as u8), "Default (Inter)")
                    .width(180.0)
                    .forced(s)
                    .show(ui, &fonts(), Some(0));
                named(&font.response, egui::Role::ComboBox, "UI font");
            }),
        ],
    );
    let t = ui.tokens();
    ui.add_space(t.space.stack);
    kit::row(ui, "Its list, open", WIDE, |ui| {
        filter_list(ui, "", State::Rest);
        filter_list(ui, "sans", State::Focus);
        filter_list(ui, "zz", State::Focus);
        kit::caption(ui, "as opened, typing, no match");
    });
}

/// The open list of a filterable dropdown, drawn in place with `typed` in its search
/// field.
pub fn filter_list(ui: &mut Ui, typed: &str, field: State) {
    let fonts = fonts();
    let t = ui.tokens();
    list(ui, 200.0, |ui| {
        let mut text = typed.to_owned();
        let search = widgets::text_field(&mut text).search().hint("Filter");
        ui.add(search.width(f32::INFINITY).forced(field));
        ui.add_space(t.space.menu);
        let needle = typed.to_lowercase();
        let matches: Vec<_> = fonts
            .iter()
            .filter(|f| f.text.to_lowercase().contains(&needle))
            .collect();
        if matches.is_empty() {
            ui.add_space(t.space.row_y);
            ui.horizontal(|ui| {
                ui.add_space(t.space.row_x);
                ui.label(t.ty.body.rich("No fonts match").color(t.color.text.muted));
            });
            ui.add_space(t.space.row_y);
        }
        for (i, font) in matches.iter().enumerate() {
            let state = if i == 0 && !typed.is_empty() {
                State::Hover
            } else {
                State::Rest
            };
            let chosen = font.text.starts_with("Default");
            ui.add(
                widgets::choice(chosen, font.text)
                    .note(font.note)
                    .forced(state),
            );
            if font.rule_below && typed.is_empty() {
                let (rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 9.0), egui::Sense::hover());
                ui.painter().hline(
                    rect.shrink2(vec2(t.space.row_x, 0.0)).x_range(),
                    rect.center().y,
                    Stroke::new(t.stroke.hairline, t.color.stroke.hairline),
                );
            }
        }
    });
}

pub fn glyphs(ui: &mut Ui) {
    let t = ui.tokens();
    let c = &t.color;
    let strip = |ui: &mut Ui, name: &str, ink: egui::Color32, ground: Option<egui::Color32>| {
        kit::row(ui, name, COLUMN, |ui| {
            Frame::new()
                .fill(ground.unwrap_or(egui::Color32::TRANSPARENT))
                .corner_radius(t.radius.control)
                .inner_margin(t.space.control_y)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = t.space.gap;
                    for (glyph, name) in Glyph::ALL {
                        ui.add(widgets::glyph(glyph).color(ink)).on_hover_text(name);
                    }
                });
        });
    };
    strip(ui, "Primary text", c.text.primary, None);
    strip(ui, "Muted text", c.text.muted, None);
    strip(ui, "On the accent", c.accent.on_solid, Some(c.accent.solid));
    kit::row(ui, "Semantic colours", COLUMN, |ui| {
        for (glyph, ink, text) in [
            (Glyph::Cross, c.error.text, "Error"),
            (Glyph::Warning, c.warning.text, "Warning"),
            (Glyph::Tick, c.success.text, "Clean"),
            (Glyph::Minus, c.text.muted, "Waived"),
        ] {
            ui.add(widgets::glyph(glyph).color(ink));
            ui.label(t.ty.body.rich(text).color(c.text.primary));
        }
    });
}
