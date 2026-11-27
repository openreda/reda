//! The panel beside a Footprint's canvas: Save and Check, the Pad Tool's settings, the
//! drawing Layer, and the properties of the one selected Pad, graphic or text. Edits of
//! the Footprint become `library.footprint.*` Commands; the Tools' settings are view
//! state and change at once.

use egui::{Id, Role, Ui, WidgetInfo};
use reda_engine::model::geom::{Millideg, Nm};
use reda_engine::model::{
    DESIGNATOR_FIELD, Footprint, FootprintLayer, FootprintText, LibraryElement, PadKind,
    TextHJustify, TextVJustify, Uuid, VALUE_FIELD,
};
use serde_json::{Value, json};

use super::{ToolSettings, tool_settings};
use crate::element_editor::{self, Invocation};
use crate::settings::layout;
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Tooltip as _};
use reda_engine::command::Length;
use reda_engine::preferences::PadDefaults;

/// The panel's width in points: a side Dock's, so the Pad rows (the same as Free Pad
/// Properties in a Dock) fit, plus the gap the panel keeps from the canvas. At 13 px: it
/// follows the text size as a Dock does (`size.of(WIDTH)`).
pub const WIDTH: f32 = 300.0;

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
        let t = ui.tokens();
        ui.label(t.ty.title.rich(&footprint.name).color(t.color.text.primary));
        if read_only {
            layout::warning(ui, tr!("footprint-read-only"));
        }
        if let Some(generator) = &footprint.generator {
            generated_by(ui, shell, element, &generator.command, read_only, &mut out);
        }
        ui.horizontal(|ui| {
            let save = ui.add_enabled(!read_only, widgets::button(tr!("footprint-save")));
            if save.clicked() {
                out.push(cmd("library.element.save", json!({ "element": element })));
            }
            if ui.add(widgets::button(tr!("footprint-check"))).clicked() {
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
        tool_settings_ui(ui, id, shell, &mut settings, &mut out);
        let before_layer = super::preview_layer(shell.engine(), &footprint);
        let mut layer = before_layer;
        widgets::form(id.with("copper-preview")).show(ui, |ui| {
            use super::CopperLayer::*;
            let layers: &[super::CopperLayer] = if Middle.available(&footprint) {
                &[Top, Middle, Bottom]
            } else {
                &[Top, Bottom]
            };
            combo(
                ui,
                id.with("copper-layer"),
                &tr!("footprint-copper-layer"),
                &mut layer,
                layers,
                |l| match l {
                    Top => tr!("pad-side-top"),
                    Middle => tr!("pad-copper-middle"),
                    Bottom => tr!("pad-side-bottom"),
                },
            );
            ui.end_row();
        });
        if layer != before_layer {
            out.push(cmd(
                "view.footprint_tool.set",
                json!({"preview":{"footprint":element,"layer":layer}}),
            ));
        }
        if settings.layer != before.layer {
            out.push(cmd(
                "view.footprint_tool.set",
                json!({ "layer": settings.layer }),
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
    widgets::form(id.with("footprint")).show(ui, |ui| {
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
    crate::settings::strong(ui, tr!("footprint-texts"));
    // By field, then layer: the stored order follows the texts' UUIDs.
    let mut texts: Vec<_> = footprint.texts.iter().collect();
    texts.sort_by_key(|t| (t.field.clone(), format!("{:?}", t.layer)));
    widgets::rows(ui, |ui| {
        for text in texts {
            ui.horizontal(|ui| {
                let label = tr!(
                    "footprint-text-row",
                    field = text.field.as_str(),
                    layer = layer_name(text.layer)
                );
                // The row leaves room for its Remove button: a glyph, so that a long
                // field and Layer still fit the panel.
                let t = ui.tokens();
                let room = ui.available_width() - t.control_height() - t.space.gap;
                let row = ui.add(widgets::list_row(&label).width(room));
                if row.clicked() {
                    out.push(cmd("selection.set", json!({"objects": [text.uuid]})));
                }
                let remove = ui.add(widgets::glyph_button(widgets::Glyph::Close));
                let remove = remove.tooltip(tr!("footprint-text-remove"));
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
    });
    ui.horizontal(|ui| {
        // The field Add Text and the Text Tool place.
        let hint = tr!("footprint-text-new-field");
        let field = settings.text_field.clone();
        if let Some(typed) = text_field(ui, id.with("new-text-field"), &hint, &field)
            && !typed.is_empty()
        {
            out.push(cmd(
                "view.footprint_tool.set",
                json!({ "text_field": typed }),
            ));
        }
        if ui.add(widgets::button(tr!("footprint-text-add"))).clicked() {
            out.push(cmd(
                "library.footprint.text.add",
                json!({"footprint": fp, "field": field, "at": {"x": 0, "y": 0},
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
    layout::muted(ui, tr!("footprint-generated-by", generator = title));
    let regenerate = ui.add_enabled(!read_only, widgets::button(tr!("footprint-regenerate")));
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

/// The Pad Tool's record (`defaults.footprint.pad`, as Tool-option edits) and the
/// drawing Layer.
fn tool_settings_ui(
    ui: &mut Ui,
    id: Id,
    shell: &Shell,
    opts: &mut ToolSettings,
    out: &mut Vec<Invocation>,
) {
    crate::settings::strong(ui, tr!("footprint-pad-tool"));
    let before = crate::object_defaults::current(shell).footprint.pad;
    let mut pad = before.clone();
    widgets::form(id.with("pad-tool")).show(ui, |ui| {
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
        template_lengths(ui, id, &mut pad);
    });
    let (was, now) = (json!(before), json!(pad));
    for (field, value) in now.as_object().expect("a record is an object") {
        if was[field] != *value {
            let key = format!("defaults.footprint.pad.{field}");
            let (id, params) = crate::object_defaults::edit(shell, &key, value.clone());
            out.push(cmd(id, params));
        }
    }
    ui.add_space(ui.tokens().space.stack);
    crate::settings::strong(ui, tr!("footprint-drawing"));
    widgets::form(id.with("drawing")).show(ui, |ui| {
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

fn template_lengths(ui: &mut Ui, id: Id, pad: &mut PadDefaults) {
    let row = |ui: &mut Ui, key: &str, label: String, value: &mut Length| {
        if let Some(Some(v)) = length(ui, id.with(key), &label, Some(value.nm()), false) {
            *value = Length(v);
        }
        ui.end_row();
    };
    row(ui, "w", tr!("footprint-pad-width"), &mut pad.width);
    row(ui, "h", tr!("footprint-pad-height"), &mut pad.height);
    if pad.kind != PadKind::Smd {
        row(ui, "hw", tr!("footprint-hole-width"), &mut pad.hole_width);
        row(ui, "hh", tr!("footprint-hole-height"), &mut pad.hole_height);
    }
    let optional = |ui: &mut Ui, key: &str, label: String, value: &mut Option<Length>| {
        if let Some(v) = length(ui, id.with(key), &label, value.map(Length::nm), true) {
            *value = v.map(Length);
        }
        ui.end_row();
    };
    optional(
        ui,
        "mask",
        tr!("footprint-mask-margin"),
        &mut pad.mask_margin,
    );
    optional(
        ui,
        "paste",
        tr!("footprint-paste-margin"),
        &mut pad.paste_margin,
    );
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
        crate::settings::strong(ui, tr!("footprint-selected-pad"));
        let id = id.with(("pad", selected));
        let units = reda_engine::preferences::Units::Mm;
        let edits = crate::pad_form::draw::rows(ui, id, pad, true, true, units);
        out.extend(
            edits
                .iter()
                .filter_map(|e| crate::pad_form::footprint_command(fp, pad, e))
                .map(|(id, params)| cmd(id, params)),
        );
        for (group, copper) in crate::pad_form::draw::groups::rows(ui, id, pad, units) {
            out.push(cmd(
                "library.footprint.pad.edit",
                json!({"footprint":fp, "pad":pad.uuid, (group):copper}),
            ));
        }
        if pad.kind == PadKind::PlatedThroughHole {
            use reda_engine::model::UnusedLayers;
            let mut mode = pad.unused_layers;
            widgets::form(id.with("unused-lands")).show(ui, |ui| {
                combo(
                    ui,
                    id.with("unused-layers"),
                    &tr!("unused-lands-pads"),
                    &mut mode,
                    &[UnusedLayers::Keep, UnusedLayers::Remove],
                    crate::pcb::unused_layers::mode_label,
                );
                ui.end_row();
            });
            if mode != pad.unused_layers {
                out.push(cmd(
                    "library.footprint.pad.edit",
                    json!({"footprint":fp, "pad":selected, "unused_layers":mode}),
                ));
            }
        }
    } else if let Some(text) = footprint.texts.iter().find(|t| t.uuid == selected) {
        crate::settings::strong(
            ui,
            match text.field.as_str() {
                DESIGNATOR_FIELD => tr!("footprint-designator-text"),
                VALUE_FIELD => tr!("footprint-value-text"),
                field => tr!("footprint-field-text", field = field.to_owned()),
            },
        );
        widgets::form(id.with(("text", selected))).show(ui, |ui| {
            text_properties(ui, id.with(selected), fp, text, out);
        });
    } else if let Some(g) = footprint.graphics.iter().find(|g| g.uuid == selected) {
        crate::settings::strong(ui, tr!("footprint-selected-graphic"));
        widgets::form(id.with(("graphic", selected))).show(ui, |ui| {
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
    ui.add(widgets::form_name(&label));
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
    ui.add(widgets::form_name(tr!("footprint-text-visible")));
    let mut visible = text.visible;
    let check = ui.add(widgets::checkbox(&mut visible, ""));
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
    ui.add(widgets::form_name(label));
    let shown = widgets::dropdown(id, name(*value)).show(ui, |ui| {
        for v in all {
            widgets::choice_value(ui, value, *v, name(*v));
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
    ui.add(widgets::form_name(label));
    let shown = current.map(format_mm).unwrap_or_default();
    let typed = text_field(ui, id, label, &shown)?;
    if typed.is_empty() {
        return optional.then_some(None).filter(|_| current.is_some());
    }
    let value = parse_length(&typed)?;
    (Some(value) != current).then_some(Some(value))
}

fn angle(ui: &mut Ui, id: Id, label: &str, current: Millideg) -> Option<Millideg> {
    ui.add(widgets::form_name(label));
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
    nm.mm_exact()
}

fn format_number(v: f64) -> String {
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}
