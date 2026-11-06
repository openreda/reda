//! The panel beside a Footprint's canvas: Save and Check, the Pad Tool's settings, the
//! drawing Layer, and the properties of the one selected Pad, graphic or text. Edits of
//! the Footprint become `library.footprint.*` Commands; the Tools' settings are view
//! state and change at once.

use egui::{ComboBox, Grid, Id, Role, Ui, WidgetInfo};
use reda_engine::model::geom::{Millideg, Nm};
use reda_engine::model::{
    DESIGNATOR_FIELD, Footprint, FootprintLayer, FootprintText, LibraryElement, Pad, PadKind,
    PadShape, Uuid, VALUE_FIELD,
};
use serde_json::{Value, json};

use super::{PadTemplate, ToolSettings, tool_settings};
use crate::element_editor::{self, Invocation};
use crate::shell::Shell;
use crate::tr;

/// The panel's width in points.
pub const WIDTH: f32 = 240.0;

pub const KINDS: [PadKind; 3] = [PadKind::Smd, PadKind::PlatedThroughHole, PadKind::NonPlated];
pub const SHAPES: [PadShape; 5] = [
    PadShape::Circle,
    PadShape::Rect,
    PadShape::RoundedRect,
    PadShape::ChamferedRect,
    PadShape::Oval,
];
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

pub fn kind_name(kind: PadKind) -> String {
    match kind {
        PadKind::Smd => tr!("footprint-pad-smd"),
        PadKind::PlatedThroughHole => tr!("footprint-pad-pth"),
        PadKind::NonPlated => tr!("footprint-pad-npth"),
    }
}

pub fn shape_name(shape: PadShape) -> String {
    match shape {
        PadShape::Circle => tr!("footprint-shape-circle"),
        PadShape::Rect => tr!("footprint-shape-rect"),
        PadShape::RoundedRect => tr!("footprint-shape-rounded-rect"),
        PadShape::ChamferedRect => tr!("footprint-shape-chamfered-rect"),
        PadShape::Oval => tr!("footprint-shape-oval"),
    }
}

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
        Grid::new(id.with(("pad", selected)))
            .num_columns(2)
            .show(ui, |ui| {
                pad_properties(ui, id.with(selected), fp, pad, out);
            });
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

fn pad_properties(ui: &mut Ui, id: Id, fp: Uuid, pad: &Pad, out: &mut Vec<Invocation>) {
    let edit = |field: &str, value: Value| {
        let mut p = json!({"footprint": fp, "pad": pad.uuid});
        p[field] = value;
        cmd("library.footprint.pad.edit", p)
    };
    ui.label(tr!("footprint-pad-number"));
    if let Some(number) = text_field(
        ui,
        id.with("number"),
        &tr!("footprint-pad-number"),
        &pad.number,
    ) {
        out.push(edit("number", json!(number)));
    }
    ui.end_row();
    let mut kind = pad.kind;
    combo(
        ui,
        id.with("kind"),
        &tr!("footprint-pad-kind"),
        &mut kind,
        &KINDS,
        kind_name,
    );
    if kind != pad.kind {
        // Changing to or from SMD changes whether the Pad has a hole.
        let mut p = json!({"footprint": fp, "pad": pad.uuid, "kind": kind});
        match (kind, pad.hole) {
            (PadKind::Smd, Some(_)) => p["hole"] = Value::Null,
            (k, None) if k != PadKind::Smd => {
                let side = super::DEFAULT_HOLE.0.min(pad.width.0).min(pad.height.0);
                p["hole"] = json!({"width": side, "height": side});
            }
            _ => {}
        }
        out.push(cmd("library.footprint.pad.edit", p));
    }
    ui.end_row();
    let mut shape = pad.shape;
    combo(
        ui,
        id.with("shape"),
        &tr!("footprint-pad-shape"),
        &mut shape,
        &SHAPES,
        shape_name,
    );
    if shape != pad.shape {
        let mut p = json!({"footprint": fp, "pad": pad.uuid, "shape": shape});
        if shape == PadShape::ChamferedRect && pad.chamfer_ratio.is_none() {
            p["chamfer_ratio"] = json!(super::DEFAULT_CHAMFER_RATIO.0);
        }
        out.push(cmd("library.footprint.pad.edit", p));
    }
    ui.end_row();
    for (key, label, value) in [
        ("width", tr!("footprint-pad-width"), pad.width),
        ("height", tr!("footprint-pad-height"), pad.height),
    ] {
        if let Some(Some(v)) = length(ui, id.with(key), &label, Some(value), false) {
            out.push(edit(key, json!(v.0)));
        }
        ui.end_row();
    }
    if let Some(r) = angle(
        ui,
        id.with("rotation"),
        &tr!("footprint-rotation"),
        pad.rotation,
    ) {
        out.push(edit("rotation", json!(r.0)));
    }
    ui.end_row();
    if let Some(hole) = pad.hole {
        for (key, label, value) in [
            ("hw", tr!("footprint-hole-width"), hole.width),
            ("hh", tr!("footprint-hole-height"), hole.height),
        ] {
            if let Some(Some(v)) = length(ui, id.with(key), &label, Some(value), false) {
                let (w, h) = if key == "hw" {
                    (v, hole.height)
                } else {
                    (hole.width, v)
                };
                out.push(edit("hole", json!({"width": w.0, "height": h.0})));
            }
            ui.end_row();
        }
    }
    for (key, label, value) in [
        ("mask_margin", tr!("footprint-mask-margin"), pad.mask_margin),
        (
            "paste_margin",
            tr!("footprint-paste-margin"),
            pad.paste_margin,
        ),
    ] {
        if let Some(v) = length(ui, id.with(key), &label, value, true) {
            out.push(edit(key, v.map_or(Value::Null, |v| json!(v.0))));
        }
        ui.end_row();
    }
}

fn text_properties(ui: &mut Ui, id: Id, fp: Uuid, text: &FootprintText, out: &mut Vec<Invocation>) {
    let set = |field: &str, value: Value| {
        let mut p = json!({"footprint": fp, "text": text.uuid});
        p[field] = value;
        cmd("library.footprint.text.edit", p)
    };
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
