//! PROTOTYPE (throwaway, #398): three variants of the New Project / Add PCB Document
//! dialog with a Board Template picker and a live 2D preview, switchable from a floating
//! bar at the bottom of the window (or ← / → when no text field has focus).
//!
//! Run it with `REDA_PROTO_BOARD_PICKER=A cargo run -p reda-app` (B or C to start on
//! another variant), then press New Project. Without the variable the real dialog shows.
//! Only "No outline" and "Custom rectangle" really create a board; the rest is read-only.
//!
//! - A · Browser: a searchable, grouped list on the left; a large preview with the
//!   parameters and the tick list under it on the right. One page.
//! - B · Gallery: two steps. Step 1 is a grid of thumbnail cards; step 2 sets the
//!   parameters beside a large preview, with the Project fields.
//! - C · Compact: today's single-column form, the board picked from a dropdown with its
//!   parameters inline, and a preview beside the form.

mod catalogue;
mod preview;

use std::collections::HashSet;

use egui::{Context, Id, Ui, vec2};

use self::catalogue::{Frame, ItemKind, Param, Template, Values};
use super::{Board, FormAction, NewProjectForm};
use crate::settings::{self, layout};
use crate::theme::TokensExt as _;
use crate::widgets;

const VARIANTS: [&str; 3] = ["A · Browser", "B · Gallery (two steps)", "C · Compact form"];

/// The prototype's state, kept in egui memory so the real form needs no new field.
#[derive(Clone, Debug)]
pub struct Proto {
    pub variant: usize,
    /// Add PCB Document rather than New Project.
    pub pcb: bool,
    pub selected: usize,
    pub values: Vec<Values>,
    pub unticked: Vec<HashSet<String>>,
    pub search: String,
    /// Variant B's step: 0 choose, 1 set up.
    pub page: u8,
}

impl Default for Proto {
    fn default() -> Self {
        let all = catalogue::all();
        let variant = match std::env::var("REDA_PROTO_BOARD_PICKER").as_deref() {
            Ok("B" | "b") => 1,
            Ok("C" | "c") => 2,
            _ => 0,
        };
        Proto {
            variant,
            pcb: false,
            selected: 1,
            values: all.iter().map(Template::defaults).collect(),
            unticked: vec![HashSet::new(); all.len()],
            search: String::new(),
            page: 0,
        }
    }
}

fn id() -> Id {
    Id::NULL.with("prototype-board-picker")
}

/// Whether the prototype replaces the real dialog.
pub fn active(ctx: &Context) -> bool {
    std::env::var_os("REDA_PROTO_BOARD_PICKER").is_some()
        || ctx.data(|d| d.get_temp::<Proto>(id()).is_some())
}

/// Sets the prototype's state, for the screenshot test: `template` by name, and
/// parameter values as `(key, value)`.
pub fn preset(
    ctx: &Context,
    variant: usize,
    pcb: bool,
    template: &str,
    values: &[(&str, &str)],
    page: u8,
) {
    let mut p = Proto::default();
    p.variant = variant;
    p.pcb = pcb;
    p.page = page;
    let all = catalogue::all();
    p.selected = all
        .iter()
        .position(|t| t.name == template)
        .expect("a template name");
    for (k, v) in values {
        p.values[p.selected].insert((*k).to_owned(), (*v).to_owned());
    }
    ctx.data_mut(|d| d.insert_temp(id(), p));
}

/// Unticks an item of the selected template, for the screenshot test.
pub fn untick(ctx: &Context, item: &str) {
    ctx.data_mut(|d| {
        let p = d.get_temp_mut_or_default::<Proto>(id());
        let s = p.selected;
        p.unticked[s].insert(item.to_owned());
    });
}

pub fn show(ctx: &Context, form: &mut NewProjectForm) -> FormAction {
    let mut p = ctx.data_mut(|d| d.get_temp_mut_or_default::<Proto>(id()).clone());
    let all = catalogue::all();
    let mut action = FormAction::None;
    let title = if p.pcb {
        "Add PCB Document"
    } else {
        "New Project"
    };
    let width = match p.variant {
        0 => 980.0,
        1 => 940.0,
        _ => 760.0,
    };
    let modal = crate::modal::Dialog::new(Id::NULL.with("new-project"), title)
        .width(width)
        .show(ctx, |ui| match p.variant {
            0 => variant_a(ui, &mut p, &all, form, &mut action),
            1 => variant_b(ui, &mut p, &all, form, &mut action),
            _ => variant_c(ui, &mut p, &all, form, &mut action),
        });
    switcher(ctx, &mut p);
    if modal.should_close() && action == FormAction::None {
        action = FormAction::Cancel;
    }
    if action == FormAction::Create {
        let v = &p.values[p.selected];
        match p.selected {
            0 => form.board = Board::DrawLater,
            1 => {
                form.board = Board::Rectangle;
                form.width = v["width"].clone();
                form.height = v["height"].clone();
                form.corner_radius = match v["corner_radius"].trim() {
                    "0" | "" => String::new(),
                    r => r.to_owned(),
                };
            }
            _ => action = FormAction::None,
        }
    }
    ctx.data_mut(|d| d.insert_temp(id(), p));
    action
}

// ---------------------------------------------------------------------------------------
// Variant A: browser.

fn variant_a(
    ui: &mut Ui,
    p: &mut Proto,
    all: &[Template],
    form: &mut NewProjectForm,
    action: &mut FormAction,
) {
    let t = ui.tokens();
    if !p.pcb {
        project_row(ui, form, action);
        ui.add_space(t.space.section);
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(250.0);
            settings::strong(ui, "Board Template");
            ui.add(
                widgets::text_field(&mut p.search)
                    .search()
                    .hint("Search templates")
                    .width(250.0),
            );
            egui::ScrollArea::vertical()
                .id_salt("proto-a-list")
                .max_height(470.0)
                .show(ui, |ui| {
                    for category in categories(all) {
                        let rows: Vec<_> = matching(all, &p.search)
                            .filter(|(_, t)| t.category == category)
                            .collect();
                        if rows.is_empty() {
                            continue;
                        }
                        ui.add_space(4.0);
                        layout::muted(ui, category.to_uppercase());
                        for (i, t) in rows {
                            if widgets::list_row(t.name)
                                .selected(i == p.selected)
                                .show(ui)
                                .response
                                .clicked()
                            {
                                p.selected = i;
                            }
                        }
                    }
                    ui.add_space(6.0);
                    layout::muted(ui, "Templates from your Libraries show here too.");
                });
        });
        ui.add_space(t.space.section);
        ui.vertical(|ui| {
            let tpl = &all[p.selected];
            settings::strong(ui, tpl.name);
            layout::muted(ui, tpl.description);
            let frame = (tpl.build)(&p.values[p.selected]);
            preview_block(ui, &frame, vec2(640.0, 330.0), &p.unticked[p.selected]);
            ui.add_space(t.space.section);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(300.0);
                    if !tpl.params.is_empty() {
                        settings::strong(ui, "Parameters");
                        params(ui, tpl, &mut p.values[p.selected], Style::Dropdown);
                    }
                    ui.add_space(t.space.section);
                    preset_and_layers(ui, form);
                });
                ui.add_space(t.space.section);
                ui.vertical(|ui| {
                    if let Ok(f) = &frame {
                        includes(ui, f, &mut p.unticked[p.selected], true);
                    }
                });
            });
        });
    });
    create_buttons(ui, p, all, form, action, false);
}

// ---------------------------------------------------------------------------------------
// Variant B: gallery, then set-up.

fn variant_b(
    ui: &mut Ui,
    p: &mut Proto,
    all: &[Template],
    form: &mut NewProjectForm,
    action: &mut FormAction,
) {
    let t = ui.tokens();
    ui.horizontal(|ui| {
        for (n, step) in ["1  Choose a board", "2  Set it up"]
            .into_iter()
            .enumerate()
        {
            let on = p.page as usize == n;
            let text = if on {
                t.ty.body_strong.rich(step).color(t.color.accent.text)
            } else {
                t.ty.body.rich(step).color(t.color.text.muted)
            };
            if ui
                .add(egui::Label::new(text).sense(egui::Sense::click()))
                .clicked()
            {
                p.page = n as u8;
            }
            ui.add_space(18.0);
        }
    });
    ui.separator();
    if p.page == 0 {
        ui.add(
            widgets::text_field(&mut p.search)
                .search()
                .hint("Search templates")
                .width(300.0),
        );
        egui::ScrollArea::vertical()
            .id_salt("proto-b-grid")
            .max_height(560.0)
            .show(ui, |ui| {
                ui.set_min_width(880.0);
                for category in categories(all) {
                    let rows: Vec<_> = matching(all, &p.search)
                        .filter(|(_, t)| t.category == category)
                        .collect();
                    if rows.is_empty() {
                        continue;
                    }
                    ui.add_space(8.0);
                    settings::strong(ui, category);
                    ui.horizontal_wrapped(|ui| {
                        for (i, tpl) in rows {
                            if card(ui, tpl, i == p.selected) {
                                if p.selected == i {
                                    p.page = 1;
                                }
                                p.selected = i;
                            }
                        }
                    });
                }
            });
        crate::modal::buttons(ui, |ui| {
            if ui.add(widgets::button("Next").primary()).clicked() {
                p.page = 1;
            }
            if ui.add(widgets::button("Cancel")).clicked() {
                *action = FormAction::Cancel;
            }
        });
        return;
    }
    let tpl = &all[p.selected];
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            settings::strong(ui, tpl.name);
            layout::muted(ui, tpl.description);
            let frame = (tpl.build)(&p.values[p.selected]);
            preview_block(ui, &frame, vec2(540.0, 420.0), &p.unticked[p.selected]);
        });
        ui.add_space(t.space.section);
        ui.vertical(|ui| {
            ui.set_width(330.0);
            if !tpl.params.is_empty() {
                settings::strong(ui, "Parameters");
                params(ui, tpl, &mut p.values[p.selected], Style::Radios);
                ui.add_space(t.space.section);
            }
            if let Ok(f) = (tpl.build)(&p.values[p.selected]) {
                includes(ui, &f, &mut p.unticked[p.selected], true);
                ui.add_space(t.space.section);
            }
            if !p.pcb {
                settings::strong(ui, "Project");
                project_column(ui, form, action);
                ui.add_space(t.space.section);
            }
            preset_and_layers(ui, form);
        });
    });
    create_buttons(ui, p, all, form, action, true);
}

/// A gallery card: a thumbnail of the template at its defaults, and its name. True when
/// clicked.
fn card(ui: &mut Ui, tpl: &Template, selected: bool) -> bool {
    let t = ui.tokens();
    let size = vec2(196.0, 160.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let stroke = if selected {
        egui::Stroke::new(2.0, t.color.accent.solid)
    } else if response.hovered() {
        egui::Stroke::new(1.0, t.color.stroke.strong)
    } else {
        egui::Stroke::new(1.0, t.color.stroke.hairline)
    };
    ui.painter().rect(
        rect,
        t.radius.control,
        t.color.surface.card,
        stroke,
        egui::StrokeKind::Inside,
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(8.0))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let frame = (tpl.build)(&tpl.defaults()).ok();
    preview::show(
        &mut child,
        frame.as_ref(),
        vec2(180.0, 96.0),
        false,
        &HashSet::new(),
    );
    let font = egui::FontId::proportional(13.0);
    let name = ui
        .painter()
        .layout(tpl.name.to_owned(), font, t.color.text.primary, 180.0);
    ui.painter().galley(
        rect.left_top() + vec2(8.0, 108.0),
        name,
        t.color.text.primary,
    );
    response.clicked()
}

// ---------------------------------------------------------------------------------------
// Variant C: compact form with the preview beside.

fn variant_c(
    ui: &mut Ui,
    p: &mut Proto,
    all: &[Template],
    form: &mut NewProjectForm,
    action: &mut FormAction,
) {
    let t = ui.tokens();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(360.0);
            if !p.pcb {
                project_column(ui, form, action);
                ui.add_space(t.space.section);
            }
            preset_and_layers(ui, form);
            ui.add_space(t.space.section);
            settings::strong(ui, "Board");
            let options: Vec<_> = all
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let label = if t.category == "Generic" {
                        t.name.to_owned()
                    } else {
                        format!("{} · {}", t.category, t.name)
                    };
                    (i, label)
                })
                .collect();
            ui.horizontal(|ui| {
                ui.add(widgets::form_name("Template"));
                settings::dropdown(
                    ui,
                    "proto-c-template",
                    "Template",
                    &mut p.selected,
                    &options,
                    270.0,
                );
            });
            let tpl = &all[p.selected];
            params(ui, tpl, &mut p.values[p.selected], Style::Segmented);
            if let Ok(f) = (tpl.build)(&p.values[p.selected]) {
                let items = f.items.len();
                if items > 0 {
                    let on = items - p.unticked[p.selected].len();
                    widgets::section("Includes")
                        .count(format!("{on} of {items}"))
                        .id_salt(("proto-c-includes", p.selected))
                        .default_open(true)
                        .show(ui, |ui| {
                            includes(ui, &f, &mut p.unticked[p.selected], false)
                        });
                }
            }
        });
        ui.add_space(t.space.section);
        ui.vertical(|ui| {
            let tpl = &all[p.selected];
            let frame = (tpl.build)(&p.values[p.selected]);
            preview_block(ui, &frame, vec2(360.0, 360.0), &p.unticked[p.selected]);
            ui.add(
                egui::Label::new(t.ty.body.rich(tpl.description).color(t.color.text.muted)).wrap(),
            );
        });
    });
    create_buttons(ui, p, all, form, action, false);
}

// ---------------------------------------------------------------------------------------
// Shared pieces.

fn categories(all: &[Template]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for t in all {
        if !out.contains(&t.category) {
            out.push(t.category);
        }
    }
    out
}

fn matching<'a>(
    all: &'a [Template],
    search: &'a str,
) -> impl Iterator<Item = (usize, &'a Template)> {
    let s = search.trim().to_lowercase();
    all.iter().enumerate().filter(move |(_, t)| {
        s.is_empty()
            || t.name.to_lowercase().contains(&s)
            || t.category.to_lowercase().contains(&s)
            || t.description.to_lowercase().contains(&s)
    })
}

fn project_row(ui: &mut Ui, form: &mut NewProjectForm, action: &mut FormAction) {
    ui.horizontal(|ui| {
        let l = ui.add(widgets::form_name("Name"));
        ui.add(widgets::text_field(&mut form.name).width(220.0))
            .labelled_by(l.id);
        ui.add_space(12.0);
        let l = ui.add(widgets::form_name("Location"));
        ui.add(widgets::text_field(&mut form.location).width(420.0))
            .labelled_by(l.id);
        if ui.add(widgets::button("Browse…")).clicked() {
            *action = FormAction::Browse;
        }
    });
}

fn project_column(ui: &mut Ui, form: &mut NewProjectForm, action: &mut FormAction) {
    widgets::form("proto-project").show(ui, |ui| {
        super::labelled_field(ui, "Name", &mut form.name, true);
        ui.end_row();
        super::labelled_field(ui, "Location", &mut form.location, true);
        ui.end_row();
        ui.label("");
        if ui.add(widgets::button("Browse…")).clicked() {
            *action = FormAction::Browse;
        }
        ui.end_row();
    });
}

fn preset_and_layers(ui: &mut Ui, form: &mut NewProjectForm) {
    let options: Vec<_> = form
        .presets
        .iter()
        .map(|p| (p.name.clone(), p.name.clone()))
        .collect();
    ui.horizontal(|ui| {
        ui.add(widgets::form_name("Fab Preset"));
        settings::dropdown(
            ui,
            "proto-preset",
            "Fab Preset",
            &mut form.preset,
            &options,
            200.0,
        );
    });
    if let Some(preset) = form.presets.iter().find(|p| p.name == form.preset).cloned() {
        form.layers.show(ui, &preset);
    }
    layout::muted(ui, "The template suggests 2 copper layers.");
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Dropdown,
    Segmented,
    Radios,
}

fn params(ui: &mut Ui, tpl: &Template, values: &mut Values, style: Style) {
    widgets::form(("proto-params", tpl.name)).show(ui, |ui| {
        for param in &tpl.params {
            if !param.shown(values) {
                continue;
            }
            match param {
                Param::Choice {
                    key,
                    label,
                    options,
                    ..
                } => {
                    let l = ui.add(widgets::form_name(*label));
                    let value = values.entry((*key).to_owned()).or_default();
                    let opts: Vec<(String, String)> = options
                        .iter()
                        .map(|(k, l)| ((*k).to_owned(), (*l).to_owned()))
                        .collect();
                    match style {
                        Style::Segmented if opts.len() <= 3 => {
                            ui.add(widgets::segmented(value, &opts)).labelled_by(l.id);
                        }
                        Style::Radios if opts.len() <= 3 => {
                            ui.vertical(|ui| {
                                for (k, text) in &opts {
                                    if ui.add(widgets::radio(value == k, text)).clicked() {
                                        *value = k.clone();
                                    }
                                }
                            });
                        }
                        _ => {
                            settings::dropdown(
                                ui,
                                ("proto-param", tpl.name, *key),
                                label,
                                value,
                                &opts,
                                180.0,
                            );
                        }
                    }
                }
                Param::Length {
                    key,
                    label,
                    min,
                    max,
                    ..
                } => {
                    let l = ui.add(widgets::form_name(*label));
                    let value = values.entry((*key).to_owned()).or_default();
                    ui.horizontal(|ui| {
                        ui.add(widgets::number_field(value).unit("mm").width(90.0))
                            .labelled_by(l.id);
                        layout::muted(
                            ui,
                            format!("{} – {}", catalogue::trim(*min), catalogue::trim(*max)),
                        );
                    });
                }
            }
            ui.end_row();
        }
    });
}

fn includes(ui: &mut Ui, frame: &Frame, unticked: &mut HashSet<String>, heading: bool) {
    if frame.items.is_empty() {
        return;
    }
    if heading {
        settings::strong(ui, "Includes");
    }
    egui::ScrollArea::vertical()
        .id_salt("proto-includes")
        .max_height(240.0)
        .show(ui, |ui| {
            for item in &frame.items {
                let mut on = !unticked.contains(&item.id);
                let keepout = frame
                    .keepouts
                    .iter()
                    .any(|k| k.protects.as_deref() == Some(item.id.as_str()));
                let text = if keepout {
                    format!("{}  + Keepout", item.label)
                } else {
                    item.label.clone()
                };
                if ui.add(widgets::checkbox(&mut on, text)).changed() {
                    if on {
                        unticked.remove(&item.id);
                    } else {
                        unticked.insert(item.id.clone());
                    }
                }
            }
        });
    let connectors = frame
        .items
        .iter()
        .filter(|i| matches!(i.kind, ItemKind::Connector { .. }))
        .count();
    if connectors > 0 {
        layout::muted(
            ui,
            "Connectors are placed as Components, with their Symbols on Sheet 1.",
        );
    }
}

fn preview_block(
    ui: &mut Ui,
    frame: &Result<Frame, String>,
    size: egui::Vec2,
    unticked: &HashSet<String>,
) {
    let t = ui.tokens();
    preview::show(ui, frame.as_ref().ok(), size, true, unticked);
    match frame {
        Ok(_) => preview::legend(ui),
        Err(e) => {
            ui.label(t.ty.body.rich(format!("⚠ {e}")).color(t.color.error.text));
        }
    }
}

fn create_buttons(
    ui: &mut Ui,
    p: &mut Proto,
    all: &[Template],
    form: &NewProjectForm,
    action: &mut FormAction,
    back: bool,
) {
    let wired = p.selected <= 1;
    let valid = (all[p.selected].build)(&p.values[p.selected]).is_ok();
    let ready = valid && (p.pcb || form.name.trim().len() > 0) && form.layers.value().is_some();
    crate::modal::buttons(ui, |ui| {
        let label = if p.pcb { "Add" } else { "Create" };
        let r = ui.add_enabled(ready && wired, widgets::button(label).primary());
        if r.clicked() {
            *action = FormAction::Create;
        }
        if ui.add(widgets::button("Cancel")).clicked() {
            *action = FormAction::Cancel;
        }
        if back && ui.add(widgets::button("Back")).clicked() {
            p.page = 0;
        }
        if !wired {
            layout::muted(ui, "Prototype: only the Generic outlines create a board.");
        }
    });
}

/// The floating variant switcher: not part of the design being judged.
fn switcher(ctx: &Context, p: &mut Proto) {
    if !ctx.egui_wants_keyboard_input() {
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            p.variant = (p.variant + 1) % VARIANTS.len();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            p.variant = (p.variant + VARIANTS.len() - 1) % VARIANTS.len();
        }
    }
    egui::Area::new(Id::NULL.with("proto-switcher"))
        .order(egui::Order::Tooltip)
        .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 4.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(250, 210, 40))
                .corner_radius(20.0)
                .inner_margin(egui::Margin::symmetric(14, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let black = egui::Color32::BLACK;
                        let txt = |s: &str| egui::RichText::new(s).color(black).strong();
                        if ui.add(egui::Button::new(txt("◀")).frame(false)).clicked() {
                            p.variant = (p.variant + VARIANTS.len() - 1) % VARIANTS.len();
                        }
                        ui.label(txt(&format!("PROTOTYPE  {}", VARIANTS[p.variant])));
                        if ui.add(egui::Button::new(txt("▶")).frame(false)).clicked() {
                            p.variant = (p.variant + 1) % VARIANTS.len();
                        }
                        ui.add_space(12.0);
                        let mode = if p.pcb {
                            "Mode: Add PCB Document"
                        } else {
                            "Mode: New Project"
                        };
                        if ui.add(egui::Button::new(txt(mode)).frame(false)).clicked() {
                            p.pcb = !p.pcb;
                        }
                    });
                });
        });
}
