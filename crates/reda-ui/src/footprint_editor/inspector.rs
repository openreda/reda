//! The panel beside a Footprint's canvas: Save and Check, the Pad Tool's settings, the
//! drawing Layer, and the properties of the one selected Pad, graphic or text. Edits of
//! the Footprint become `library.footprint.*` Commands; the Tools' settings are view
//! state and change at once.

use egui::{ComboBox, Grid, Id, Role, Ui, WidgetInfo};
use reda_engine::model::geom::{Millideg, Nm};
use reda_engine::model::{
    DESIGNATOR_FIELD, Footprint, FootprintLayer, FootprintText, LibraryElement, PadKind,
    TextHJustify, TextVJustify, Uuid, VALUE_FIELD,
};
use serde_json::{Value, json};

use super::{PadTemplate, ToolSettings, tool_settings};
use crate::element_editor::{self, Invocation};
use crate::shell::Shell;
use crate::tr;

/// The panel's width in points.
pub const WIDTH: f32 = 240.0;

pub use crate::pad_form::draw::{KINDS, SHAPES, kind_name, shape_name};
/// The Layers a field text can go on.
const TEXT_LAYERS: [FootprintLayer; 4] = [
    FootprintLayer::SilkscreenTop,
    FootprintLayer::SilkscreenBottom,
    FootprintLayer::AssemblyTop,
    FootprintLayer::AssemblyBottom,
];

pub const LAYERS: [FootprintLayer; 6] = [
    FootprintLayer::SilkscreenTop,
    FootprintLayer::SilkscreenBottom,
    FootprintLayer::CourtyardTop,
    FootprintLayer::CourtyardBottom,
    FootprintLayer::AssemblyTop,
    FootprintLayer::AssemblyBottom,
];

pub fn layer_name(layer: FootprintLayer) -> String {
    match layer {
        FootprintLayer::SilkscreenTop => tr!("footprint-layer-silkscreen-top"),
        FootprintLayer::SilkscreenBottom => tr!("footprint-layer-silkscreen-bottom"),
        FootprintLayer::CourtyardTop => tr!("footprint-layer-courtyard-top"),
        FootprintLayer::CourtyardBottom => tr!("footprint-layer-courtyard-bottom"),
        FootprintLayer::AssemblyTop => tr!("footprint-layer-assembly-top"),
        FootprintLayer::AssemblyBottom => tr!("footprint-layer-assembly-bottom"),
    }
}

/// Draws the panel for the Footprint `element`; returns the Commands its edits ask for.
pub fn show(ui: &mut Ui, shell: &mut Shell, element: Uuid) -> Vec<Invocation> {
    let Some(LibraryElement::Footprint(footprint)) = shell.engine().element(element).cloned()
    else {
        return Vec::new();
    };
    let read_only = element_editor::read_only(shell.engine(), element);
    let mut out = Vec::new();
    let id = Id::NULL.with(("footprint-inspector", element));
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading(&footprint.name);
        if read_only {
            ui.colored_label(ui.visuals().warn_fg_color, tr!("footprint-read-only"));
        }
        if let Some(generator) = &footprint.generator {
            generated_by(ui, shell, element, &generator.command, read_only, &mut out);
        }
        ui.horizontal(|ui| {
            let save = ui.add_enabled(!read_only, egui::Button::new(tr!("footprint-save")));
            if save.clicked() {
                out.push(cmd("library.element.save", json!({ "element": element })));
            }
            if ui.button(tr!("footprint-check")).clicked() {
                // An error shows in the status bar; the lines go to the Messages panel.
                let _ = element_editor::check(shell, element);
            }
        });
        ui.separator();
        if !read_only {
            footprint_section(ui, id, &footprint, &tool_settings(shell), &mut out);
            ui.separator();
        }
        let before = tool_settings(shell);
        let mut settings = before.clone();
        tool_settings_ui(ui, id, &mut settings);
        if settings != before {
            out.push(cmd(
                "view.footprint_tool.set",
                json!({ "pad": settings.pad, "layer": settings.layer }),
            ));
        }
        ui.separator();
        if !read_only {
            let selected = element_editor::selected_on(shell, element);
            if let [one] = selected[..] {
                selection(ui, id, &footprint, one, &mut out);
            }
        }
    });
    out
}

/// The Footprint's own rows: its `height`, and its `texts` as a list to select, add to
/// and remove from.
fn footprint_section(
    ui: &mut Ui,
    id: Id,
    footprint: &Footprint,
    settings: &ToolSettings,
    out: &mut Vec<Invocation>,
) {
    let fp = footprint.uuid;
    Grid::new(id.with("footprint"))
        .num_columns(2)
        .show(ui, |ui| {
            if let Some(height) = length(
                ui,
                id.with("height"),
                &tr!("footprint-height"),
                footprint.height,
                true,
            ) {
                out.push(cmd(
                    "library.footprint.edit",
                    json!({"footprint": fp, "height": height.map(|h| h.0)}),
                ));
            }
            ui.end_row();
        });
    ui.strong(tr!("footprint-texts"));
    for text in &footprint.texts {
        ui.horizontal(|ui| {
            let label = tr!(
                "footprint-text-row",
                field = text.field.as_str(),
                layer = layer_name(text.layer)
            );
            let row = ui.selectable_label(false, &label);
            if row.clicked() {
                out.push(cmd("selection.set", json!({"objects": [text.uuid]})));
            }
            let remove = ui.small_button(tr!("footprint-text-remove"));
            let what = tr!("footprint-text-remove-of", text = label.as_str());
            remove.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
            if remove.clicked() {
                out.push(cmd(
                    "library.footprint.text.remove",
                    json!({"footprint": fp, "text": text.uuid}),
                ));
            }
        });
    }
    ui.horizontal(|ui| {
        let field_id = id.with("new-text-field");
        let mut field: String = ui
            .data(|d| d.get_temp(field_id))
            .unwrap_or_else(|| reda_engine::model::DESIGNATOR_FIELD.to_owned());
        let hint = tr!("footprint-text-new-field");
        ui.add(egui::TextEdit::singleline(&mut field).desired_width(110.0))
            .widget_info(|| WidgetInfo::labeled(Role::TextInput, true, hint.clone()));
        ui.data_mut(|d| d.insert_temp(field_id, field.clone()));
        if ui.button(tr!("footprint-text-add")).clicked() && !field.trim().is_empty() {
            out.push(cmd(
                "library.footprint.text.add",
                json!({"footprint": fp, "field": field.trim(), "at": {"x": 0, "y": 0},
                       "layer": text_layer(settings.layer)}),
            ));
        }
    });
}

/// The Layer a new text goes on: the drawing Layer when it can hold text, else the
/// Silkscreen on the same side.
pub fn text_layer(drawing: FootprintLayer) -> FootprintLayer {
    match drawing {
        FootprintLayer::CourtyardTop => FootprintLayer::SilkscreenTop,
        FootprintLayer::CourtyardBottom => FootprintLayer::SilkscreenBottom,
        other => other,
    }
}

/// Which generator made the Footprint, and Regenerate…, which opens the generator form
/// on its stored params.
fn generated_by(
    ui: &mut Ui,
    shell: &Shell,
    element: Uuid,
    command: &str,
    read_only: bool,
    out: &mut Vec<Invocation>,
) {
    let title = shell.engine().registry().get(command).map_or_else(
        || command.to_owned(),
        |m| crate::bars::command_title(command, m.title),
    );
    ui.weak(tr!("footprint-generated-by", generator = title));
    let regenerate = ui.add_enabled(!read_only, egui::Button::new(tr!("footprint-regenerate")));
    if regenerate.clicked() {
        out.push(cmd(
            "library.footprint.regenerate",
            json!({ "footprint": element }),
        ));
    }
}

fn cmd(id: &str, params: Value) -> Invocation {
    (id.to_owned(), params)
}

fn tool_settings_ui(ui: &mut Ui, id: Id, opts: &mut ToolSettings) {
    ui.strong(tr!("footprint-pad-tool"));
    let pad = &mut opts.pad;
    Grid::new(id.with("pad-tool"))
        .num_columns(2)
        .show(ui, |ui| {
            combo(
                ui,
                id.with("kind"),
                &tr!("footprint-pad-kind"),
                &mut pad.kind,
                &KINDS,
                kind_name,
            );
            ui.end_row();
            combo(
                ui,
                id.with("shape"),
                &tr!("footprint-pad-shape"),
                &mut pad.shape,
                &SHAPES,
                shape_name,
            );
            ui.end_row();
            template_lengths(ui, id, pad);
        });
    ui.add_space(4.0);
    ui.strong(tr!("footprint-drawing"));
    Grid::new(id.with("drawing")).num_columns(2).show(ui, |ui| {
        combo(
            ui,
            id.with("layer"),
            &tr!("footprint-layer"),
            &mut opts.layer,
            &LAYERS,
            layer_name,
        );
        ui.end_row();
    });
}

fn template_lengths(ui: &mut Ui, id: Id, pad: &mut PadTemplate) {
    if let Some(Some(w)) = length(
        ui,
        id.with("w"),
        &tr!("footprint-pad-width"),
        Some(pad.width),
        false,
    ) {
        pad.width = w;
    }
    ui.end_row();
    if let Some(Some(h)) = length(
        ui,
        id.with("h"),
        &tr!("footprint-pad-height"),
        Some(pad.height),
        false,
    ) {
        pad.height = h;
    }
    ui.end_row();
    if pad.kind != PadKind::Smd {
        let (hw, hh) = pad
            .hole
            .unwrap_or((super::DEFAULT_HOLE, super::DEFAULT_HOLE));
        if let Some(Some(w)) = length(
            ui,
            id.with("hw"),
            &tr!("footprint-hole-width"),
            Some(hw),
            false,
        ) {
            pad.hole = Some((w, hh));
        }
        ui.end_row();
        let (hw, hh) = pad.hole.unwrap_or((hw, hh));
        if let Some(Some(h)) = length(
            ui,
            id.with("hh"),
            &tr!("footprint-hole-height"),
            Some(hh),
            false,
        ) {
            pad.hole = Some((hw, h));
        }
        ui.end_row();
    }
    if let Some(m) = length(
        ui,
        id.with("mask"),
        &tr!("footprint-mask-margin"),
        pad.mask_margin,
        true,
    ) {
        pad.mask_margin = m;
    }
    ui.end_row();
    if let Some(m) = length(
        ui,
        id.with("paste"),
        &tr!("footprint-paste-margin"),
        pad.paste_margin,
        true,
    ) {
        pad.paste_margin = m;
    }
    ui.end_row();
}

/// The properties of the one selected part.
fn selection(
    ui: &mut Ui,
    id: Id,
    footprint: &Footprint,
    selected: Uuid,
    out: &mut Vec<Invocation>,
) {
    let fp = footprint.uuid;
    if let Some(pad) = footprint.pads.iter().find(|p| p.uuid == selected) {
        ui.strong(tr!("footprint-selected-pad"));
        let edits = crate::pad_form::draw::rows(
            ui,
            id.with(("pad", selected)),
            pad,
            true,
            true,
            reda_engine::preferences::Units::Mm,
        );
        out.extend(
            edits
                .iter()
                .filter_map(|e| crate::pad_form::footprint_command(fp, pad, e))
                .map(|(id, params)| cmd(id, params)),
        );
    } else if let Some(text) = footprint.texts.iter().find(|t| t.uuid == selected) {
        ui.strong(match text.field.as_str() {
            DESIGNATOR_FIELD => tr!("footprint-designator-text"),
            VALUE_FIELD => tr!("footprint-value-text"),
            field => tr!("footprint-field-text", field = field.to_owned()),
        });
        Grid::new(id.with(("text", selected)))
            .num_columns(2)
            .show(ui, |ui| {
                text_properties(ui, id.with(selected), fp, text, out);
            });
    } else if let Some(g) = footprint.graphics.iter().find(|g| g.uuid == selected) {
        ui.strong(tr!("footprint-selected-graphic"));
        Grid::new(id.with(("graphic", selected)))
            .num_columns(2)
            .show(ui, |ui| {
                let mut layer = g.layer;
                combo(
                    ui,
                    id.with(("gl", selected)),
                    &tr!("footprint-layer"),
                    &mut layer,
                    &LAYERS,
                    layer_name,
                );
                if layer != g.layer {
                    out.push(cmd(
                        "library.footprint.graphic.edit",
                        json!({"footprint": fp, "graphic": g.uuid, "layer": layer}),
                    ));
                }
                ui.end_row();
                if let Some(Some(w)) = length(
                    ui,
                    id.with(("gw", selected)),
                    &tr!("footprint-stroke-width"),
                    Some(g.width),
                    false,
                ) {
                    out.push(cmd(
                        "library.footprint.graphic.edit",
                        json!({"footprint": fp, "graphic": g.uuid, "width": w.0}),
                    ));
                }
                ui.end_row();
            });
    }
}

fn text_properties(ui: &mut Ui, id: Id, fp: Uuid, text: &FootprintText, out: &mut Vec<Invocation>) {
    let set = |field: &str, value: Value| {
        let mut p = json!({"footprint": fp, "text": text.uuid});
        p[field] = value;
        cmd("library.footprint.text.edit", p)
    };
    let label = tr!("footprint-text-field");
    ui.label(&label);
    if let Some(field) = text_field(ui, id.with("field"), &label, &text.field)
        && !field.is_empty()
    {
        out.push(set("field", json!(field)));
    }
    ui.end_row();
    for (key, label, value) in [
        ("x", tr!("footprint-text-x"), text.at.x),
        ("y", tr!("footprint-text-y"), text.at.y),
    ] {
        if let Some(Some(v)) = length(ui, id.with(key), &label, Some(value), false) {
            let at = if key == "x" {
                json!({"x": v.0, "y": text.at.y.0})
            } else {
                json!({"x": text.at.x.0, "y": v.0})
            };
            out.push(set("at", at));
        }
        ui.end_row();
    }
    if let Some(Some(v)) = length(
        ui,
        id.with("thickness"),
        &tr!("footprint-text-thickness"),
        Some(text.style.thickness),
        false,
    ) {
        out.push(set("thickness", json!(v.0)));
    }
    ui.end_row();
    let mut h = text.style.justify.h;
    combo(
        ui,
        id.with("justify-h"),
        &tr!("footprint-text-justify-h"),
        &mut h,
        &[
            TextHJustify::Left,
            TextHJustify::Center,
            TextHJustify::Right,
        ],
        h_name,
    );
    ui.end_row();
    let mut v = text.style.justify.v;
    combo(
        ui,
        id.with("justify-v"),
        &tr!("footprint-text-justify-v"),
        &mut v,
        &[
            TextVJustify::Top,
            TextVJustify::Center,
            TextVJustify::Bottom,
        ],
        v_name,
    );
    ui.end_row();
    if (h, v) != (text.style.justify.h, text.style.justify.v) {
        out.push(set("justify", json!({"h": h, "v": v})));
    }
    if let Some(margin) = length(
        ui,
        id.with("knockout"),
        &tr!("footprint-text-knockout"),
        text.style.knockout.map(|k| k.margin),
        true,
    ) {
        out.push(match margin {
            Some(m) => set("knockout", json!({"margin": m.0})),
            None => set("clear_knockout", json!(true)),
        });
    }
    ui.end_row();
    if let Some(Some(v)) = length(
        ui,
        id.with("size"),
        &tr!("footprint-text-size"),
        Some(text.style.height),
        false,
    ) {
        out.push(set("height", json!(v.0)));
    }
    ui.end_row();
    if let Some(r) = angle(
        ui,
        id.with("rotation"),
        &tr!("footprint-rotation"),
        text.rotation,
    ) {
        out.push(set("rotation", json!(r.0)));
    }
    ui.end_row();
    let mut layer = text.layer;
    combo(
        ui,
        id.with("layer"),
        &tr!("footprint-layer"),
        &mut layer,
        &TEXT_LAYERS,
        layer_name,
    );
    if layer != text.layer {
        out.push(set("layer", json!(layer)));
    }
    ui.end_row();
    ui.label(tr!("footprint-text-visible"));
    let mut visible = text.visible;
    let check = ui.checkbox(&mut visible, "");
    check.widget_info(|| WidgetInfo::labeled(Role::CheckBox, true, tr!("footprint-text-visible")));
    if check.changed() {
        out.push(set("visible", json!(visible)));
    }
    ui.end_row();
}

fn h_name(h: TextHJustify) -> String {
    match h {
        TextHJustify::Left => tr!("footprint-justify-left"),
        TextHJustify::Center => tr!("footprint-justify-center"),
        TextHJustify::Right => tr!("footprint-justify-right"),
    }
}

fn v_name(v: TextVJustify) -> String {
    match v {
        TextVJustify::Top => tr!("footprint-justify-top"),
        TextVJustify::Center => tr!("footprint-justify-middle"),
        TextVJustify::Bottom => tr!("footprint-justify-bottom"),
    }
}

fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: Id,
    label: &str,
    value: &mut T,
    all: &[T],
    name: fn(T) -> String,
) {
    ui.label(label);
    let shown = ComboBox::from_id_salt(id)
        .selected_text(name(*value))
        .show_ui(ui, |ui| {
            for v in all {
                ui.selectable_value(value, *v, name(*v));
            }
        });
    let label = label.to_owned();
    shown
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
}

fn text_field(ui: &mut Ui, id: Id, label: &str, current: &str) -> Option<String> {
    element_editor::text_field(ui, id, label, current, 110.0)
}

/// A length field shown in mm. Returns the committed value when it changed: `Some(None)`
/// for an emptied `optional` field. A bare number is in mm; units may be typed.
fn length(
    ui: &mut Ui,
    id: Id,
    label: &str,
    current: Option<Nm>,
    optional: bool,
) -> Option<Option<Nm>> {
    ui.label(label);
    let shown = current.map(format_mm).unwrap_or_default();
    let typed = text_field(ui, id, label, &shown)?;
    if typed.is_empty() {
        return optional.then_some(None).filter(|_| current.is_some());
    }
    let value = parse_length(&typed)?;
    (Some(value) != current).then_some(Some(value))
}

fn angle(ui: &mut Ui, id: Id, label: &str, current: Millideg) -> Option<Millideg> {
    ui.label(label);
    let shown = format_number(current.0 as f64 / 1000.0);
    let typed = text_field(ui, id, label, &shown)?;
    let value = match typed.parse::<f64>() {
        Ok(deg) => Millideg((deg * 1000.0).round() as i64),
        Err(_) => Millideg::parse(&typed).ok()?,
    };
    (value != current).then_some(value)
}

/// `text` as a length: a bare number is in mm, else a length with its unit.
pub fn parse_length(text: &str) -> Option<Nm> {
    match text.trim().parse::<f64>() {
        Ok(mm) => Some(Nm((mm * 1_000_000.0).round() as i64)),
        Err(_) => Nm::parse(text.trim()).ok(),
    }
}

/// `nm` in mm without trailing zeros: `1.5`, `0.25`.
pub fn format_mm(nm: Nm) -> String {
    format_number(nm.0 as f64 / 1_000_000.0)
}

fn format_number(v: f64) -> String {
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}
