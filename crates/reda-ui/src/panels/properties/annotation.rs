//! The Text Style, Dimension, Pad and Footprint Graphics regions of the Properties
//! panel; see [`crate::properties::annotation`] and
//! [`crate::properties::footprint_edit`].

use egui::{Checkbox, ComboBox, Grid, Role, Ui, WidgetInfo};
use reda_engine::model::geom::{Millideg, Nm};
use reda_engine::model::{
    ArrowPosition, DimensionFormat, DimensionTextPosition, DimensionUnits, FootprintLayer, PadKind,
    PadShape, Project, Shape, TextFrame, TextHJustify, TextVJustify,
};
use reda_engine::preferences::Units;

use super::{Out, parse_length, region, text_field};
use crate::pcb::view::format_length;
use crate::properties::annotation::{
    DimensionEdit, DimensionRows, KNOCKOUT_MARGIN, TextStyleEdit, TextStyleRows,
};
use crate::properties::footprint_edit::{FootprintGraphics, GraphicEdit, PadEdit, PadRows};
use crate::properties::{MIXED, Shared};
use crate::shell::Shell;
use crate::tr;

/// Draws the regions the selection has, pushing what their edits run onto `out`.
pub(super) fn show(
    ui: &mut Ui,
    shell: &Shell,
    project: &Project,
    units: Units,
    out: &mut Vec<Out>,
) {
    let selection = shell.selection();
    if let Some(rows) = TextStyleRows::of(project, selection) {
        region(
            ui,
            shell,
            "text-style",
            tr!("properties-region-text-style"),
            |ui| {
                let fonts: Vec<String> = shell
                    .engine()
                    .fonts()
                    .families()
                    .map(str::to_owned)
                    .collect();
                for edit in text_style(ui, &rows, &fonts, units) {
                    let (id, params) = rows.command(&edit);
                    out.push(Out::Run(id, params));
                }
            },
        );
    }
    if let Some(rows) = DimensionRows::of(project, selection) {
        region(
            ui,
            shell,
            "dimension",
            tr!("properties-region-dimension"),
            |ui| {
                for edit in dimension(ui, &rows) {
                    let (id, params) = rows.command(&edit);
                    out.push(Out::Run(id, params));
                }
            },
        );
    }
    if let Some(rows) = PadRows::of(project, selection) {
        region(ui, shell, "pad", tr!("properties-region-pad"), |ui| {
            for edit in pad(ui, &rows, units) {
                let (id, params) = rows.command(&edit);
                out.push(Out::Run(id, params));
            }
        });
    }
    let protect = shell.preferences().editing.protect_locked_primitives;
    if let Some(rows) = FootprintGraphics::of(project, selection, protect) {
        region(
            ui,
            shell,
            "graphics",
            tr!("properties-region-graphics"),
            |ui| {
                for edit in graphics(ui, &rows, units) {
                    if let Some((id, params)) = rows.command(&edit) {
                        out.push(Out::Run(id, params));
                    }
                }
            },
        );
    }
}

/// A labelled drop-down of `options`, with `<mixed>` when the selection differs; returns
/// the option picked.
fn combo<T: Clone + PartialEq>(
    ui: &mut Ui,
    key: &str,
    label: &str,
    value: &Shared<T>,
    options: &[(T, String)],
) -> Option<T> {
    ui.label(label);
    let shown = match value {
        Shared::Same(v) => options
            .iter()
            .find(|(o, _)| o == v)
            .map_or_else(String::new, |(_, t)| t.clone()),
        Shared::Mixed => MIXED.to_owned(),
    };
    let mut picked = None;
    let response = ComboBox::from_id_salt(("properties-combo", key))
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (option, title) in options {
                let on = value.same() == Some(option);
                if ui.selectable_label(on, title).clicked() && !on {
                    picked = Some(option.clone());
                }
            }
        })
        .response;
    let label = label.to_owned();
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
    ui.end_row();
    picked
}

/// A checkbox for a value that may be mixed; returns the new value when clicked.
fn check(ui: &mut Ui, label: &str, value: &Shared<bool>) -> Option<bool> {
    let mut on = value.same().copied().unwrap_or(false);
    let response = ui.add(Checkbox::new(&mut on, label));
    response.clicked().then_some(on)
}

/// A length field; returns the length typed.
fn length(ui: &mut Ui, key: &str, label: &str, value: Shared<Nm>, units: Units) -> Option<Nm> {
    let shown = match value {
        Shared::Same(v) => Shared::Same(format_length(v, units)),
        Shared::Mixed => Shared::Mixed,
    };
    let typed = text_field(ui, key, label, &shown, true);
    ui.end_row();
    typed.and_then(|t| parse_length(&t, units))
}

fn text_style(
    ui: &mut Ui,
    rows: &TextStyleRows,
    fonts: &[String],
    units: Units,
) -> Vec<TextStyleEdit> {
    let mut edits = Vec::new();
    Grid::new("properties-text-style")
        .num_columns(2)
        .show(ui, |ui| {
            let mut families: Vec<(String, String)> =
                fonts.iter().map(|f| (f.clone(), f.clone())).collect();
            if let Shared::Same(f) = &rows.font
                && !fonts.contains(f)
            {
                families.insert(0, (f.clone(), f.clone()));
            }
            if let Some(f) = combo(
                ui,
                "text-font",
                &tr!("properties-text-font"),
                &rows.font,
                &families,
            ) {
                edits.push(TextStyleEdit::Font(f));
            }
            let h = [
                (TextHJustify::Left, tr!("properties-justify-left")),
                (TextHJustify::Center, tr!("properties-justify-center")),
                (TextHJustify::Right, tr!("properties-justify-right")),
            ];
            if let Some(h) = combo(ui, "text-h", &tr!("properties-text-h"), &rows.h, &h) {
                edits.push(TextStyleEdit::H(h));
            }
            let v = [
                (TextVJustify::Top, tr!("properties-justify-top")),
                (TextVJustify::Center, tr!("properties-justify-center")),
                (TextVJustify::Bottom, tr!("properties-justify-bottom")),
            ];
            if let Some(v) = combo(ui, "text-v", &tr!("properties-text-v"), &rows.v, &v) {
                edits.push(TextStyleEdit::V(v));
            }
        });
    if let Some(m) = check(ui, &tr!("properties-text-mirrored"), &rows.mirrored) {
        edits.push(TextStyleEdit::Mirrored(m));
    }
    let knocked = rows
        .knockout
        .same()
        .map(Option::is_some)
        .map_or(Shared::Mixed, Shared::Same);
    if let Some(on) = check(ui, &tr!("properties-text-knockout"), &knocked) {
        edits.push(TextStyleEdit::Knockout(on.then_some(KNOCKOUT_MARGIN)));
    }
    let framed = rows
        .frame
        .same()
        .map(Option::is_some)
        .map_or(Shared::Mixed, Shared::Same);
    if let Some(on) = check(ui, &tr!("properties-text-frame"), &framed) {
        let frame = TextFrame {
            width: Nm::from_mm(10),
            height: Nm::from_mm(5),
        };
        edits.push(TextStyleEdit::Frame(on.then_some(frame)));
    }
    Grid::new("properties-text-boxes")
        .num_columns(2)
        .show(ui, |ui| {
            if let Shared::Same(Some(margin)) = rows.knockout
                && let Some(m) = length(
                    ui,
                    "text-knockout-margin",
                    &tr!("properties-text-knockout-margin"),
                    Shared::Same(margin),
                    units,
                )
            {
                edits.push(TextStyleEdit::Knockout(Some(m)));
            }
            if let Shared::Same(Some(frame)) = rows.frame {
                if let Some(w) = length(
                    ui,
                    "text-frame-width",
                    &tr!("properties-text-frame-width"),
                    Shared::Same(frame.width),
                    units,
                ) {
                    edits.push(TextStyleEdit::Frame(Some(TextFrame { width: w, ..frame })));
                }
                if let Some(h) = length(
                    ui,
                    "text-frame-height",
                    &tr!("properties-text-frame-height"),
                    Shared::Same(frame.height),
                    units,
                ) {
                    edits.push(TextStyleEdit::Frame(Some(TextFrame { height: h, ..frame })));
                }
            }
        });
    edits
}

fn dimension(ui: &mut Ui, rows: &DimensionRows) -> Vec<DimensionEdit> {
    let mut edits = Vec::new();
    if let [label] = &rows.labels[..] {
        ui.label(tr!("properties-dimension-value", value = label.as_str()));
    }
    Grid::new("properties-dimension")
        .num_columns(2)
        .show(ui, |ui| {
            let units = [
                (
                    DimensionUnits::Automatic,
                    tr!("properties-dimension-units-automatic"),
                ),
                (DimensionUnits::Mm, "mm".to_owned()),
                (DimensionUnits::Cm, "cm".to_owned()),
                (DimensionUnits::Mil, "mil".to_owned()),
                (DimensionUnits::In, "in".to_owned()),
            ];
            if let Some(u) = combo(
                ui,
                "dim-units",
                &tr!("properties-dimension-units"),
                &rows.units,
                &units,
            ) {
                edits.push(DimensionEdit::Units(u));
            }
            let precision: Vec<(u8, String)> = (0..=6).map(|p| (p, p.to_string())).collect();
            if let Some(p) = combo(
                ui,
                "dim-precision",
                &tr!("properties-dimension-precision"),
                &rows.precision,
                &precision,
            ) {
                edits.push(DimensionEdit::Precision(p));
            }
            let formats = [
                (
                    DimensionFormat::None,
                    tr!("properties-dimension-format-none"),
                ),
                (DimensionFormat::Value, "12.70".to_owned()),
                (DimensionFormat::ValueUnit, "12.70mm".to_owned()),
                (DimensionFormat::ValueBracketedUnit, "12.70 (mm)".to_owned()),
            ];
            if let Some(f) = combo(
                ui,
                "dim-format",
                &tr!("properties-dimension-format"),
                &rows.format,
                &formats,
            ) {
                edits.push(DimensionEdit::Format(f));
            }
            for (key, label, value, prefix) in [
                (
                    "dim-prefix",
                    tr!("properties-dimension-prefix"),
                    &rows.prefix,
                    true,
                ),
                (
                    "dim-suffix",
                    tr!("properties-dimension-suffix"),
                    &rows.suffix,
                    false,
                ),
            ] {
                if let Some(text) = text_field(ui, key, &label, value, true) {
                    edits.push(if prefix {
                        DimensionEdit::Prefix(text)
                    } else {
                        DimensionEdit::Suffix(text)
                    });
                }
                ui.end_row();
            }
            let mut positions = vec![
                (
                    DimensionTextPosition::Automatic,
                    tr!("properties-dimension-text-automatic"),
                ),
                (
                    DimensionTextPosition::Center,
                    tr!("properties-justify-center"),
                ),
                (DimensionTextPosition::Top, tr!("properties-justify-top")),
                (
                    DimensionTextPosition::Bottom,
                    tr!("properties-justify-bottom"),
                ),
                (DimensionTextPosition::Left, tr!("properties-justify-left")),
                (
                    DimensionTextPosition::Right,
                    tr!("properties-justify-right"),
                ),
                (
                    DimensionTextPosition::InsideLeft,
                    tr!("properties-dimension-text-inside-left"),
                ),
                (
                    DimensionTextPosition::InsideRight,
                    tr!("properties-dimension-text-inside-right"),
                ),
                (
                    DimensionTextPosition::Unidirectional,
                    tr!("properties-dimension-text-unidirectional"),
                ),
            ];
            if let Shared::Same(manual @ DimensionTextPosition::Manual(_)) = rows.text_position {
                positions.push((manual, tr!("properties-dimension-text-manual")));
            }
            if let Some(t) = combo(
                ui,
                "dim-text",
                &tr!("properties-dimension-text-position"),
                &rows.text_position,
                &positions,
            ) {
                edits.push(DimensionEdit::TextPosition(t));
            }
            let arrows = [
                (
                    ArrowPosition::Inside,
                    tr!("properties-dimension-arrows-inside"),
                ),
                (
                    ArrowPosition::Outside,
                    tr!("properties-dimension-arrows-outside"),
                ),
            ];
            if let Some(a) = combo(
                ui,
                "dim-arrows",
                &tr!("properties-dimension-arrows"),
                &rows.arrow_position,
                &arrows,
            ) {
                edits.push(DimensionEdit::ArrowPosition(a));
            }
        });
    edits
}

fn pad(ui: &mut Ui, rows: &PadRows, units: Units) -> Vec<PadEdit> {
    let mut edits = Vec::new();
    let p = &rows.pad;
    Grid::new("properties-pad").num_columns(2).show(ui, |ui| {
        ui.label(tr!("properties-pad-number"));
        ui.label(&p.number);
        ui.end_row();
        let kinds = [
            (PadKind::Smd, tr!("properties-pad-kind-smd")),
            (PadKind::PlatedThroughHole, tr!("properties-pad-kind-pth")),
            (PadKind::NonPlated, tr!("properties-pad-kind-npth")),
        ];
        if let Some(k) = combo(
            ui,
            "pad-kind",
            &tr!("properties-pad-kind"),
            &Shared::Same(p.kind),
            &kinds,
        ) {
            edits.push(PadEdit::Kind(k));
        }
        let shapes = [
            (PadShape::Circle, tr!("properties-pad-shape-circle")),
            (PadShape::Rect, tr!("properties-pad-shape-rect")),
            (
                PadShape::RoundedRect,
                tr!("properties-pad-shape-rounded-rect"),
            ),
            (PadShape::Oval, tr!("properties-pad-shape-oval")),
            (
                PadShape::ChamferedRect,
                tr!("properties-pad-shape-chamfered-rect"),
            ),
        ];
        if let Some(s) = combo(
            ui,
            "pad-shape",
            &tr!("properties-pad-shape"),
            &Shared::Same(p.shape),
            &shapes,
        ) {
            edits.push(PadEdit::Shape(s));
        }
        if let Some(w) = length(
            ui,
            "pad-width",
            &tr!("properties-width"),
            Shared::Same(p.width),
            units,
        ) {
            edits.push(PadEdit::Width(w));
        }
        if let Some(h) = length(
            ui,
            "pad-height",
            &tr!("properties-height"),
            Shared::Same(p.height),
            units,
        ) {
            edits.push(PadEdit::Height(h));
        }
        let degrees = format!("{}", p.rotation.0 as f64 / 1000.0);
        if let Some(text) = text_field(
            ui,
            "pad-rotation",
            &tr!("properties-rotation"),
            &Shared::Same(degrees),
            true,
        ) && let Ok(deg) = text
            .trim()
            .trim_end_matches('°')
            .trim_end_matches("deg")
            .parse::<f64>()
        {
            edits.push(PadEdit::Rotation(Millideg((deg * 1000.0).round() as i64)));
        }
        ui.end_row();
        if let Some(hole) = p.hole
            && let Some(w) = length(
                ui,
                "pad-hole",
                &tr!("properties-hole"),
                Shared::Same(hole.width),
                units,
            )
        {
            edits.push(PadEdit::Hole(w));
        }
    });
    edits
}

fn graphics(ui: &mut Ui, rows: &FootprintGraphics, units: Units) -> Vec<GraphicEdit> {
    let mut edits = Vec::new();
    let (label, unlocked) = if rows.unlocked {
        (tr!("properties-graphics-unlocked"), true)
    } else {
        (tr!("properties-graphics-locked"), false)
    };
    ui.horizontal(|ui| {
        let name = tr!("properties-graphics-lock");
        let button = match crate::icons::silhouette_image("lock", 14.0) {
            Some(icon) => egui::Button::image(icon),
            None => egui::Button::new(&name),
        };
        let lock = ui
            .add(button.selected(!unlocked))
            .on_hover_text(tr!("properties-graphics-lock-hint"));
        lock.widget_info(|| WidgetInfo::selected(Role::Button, true, !unlocked, &name));
        if lock.clicked() {
            edits.push(GraphicEdit::Unlock(!unlocked));
        }
        ui.label(label);
    });
    if !rows.editable() {
        return edits;
    }
    let layers = [
        (
            FootprintLayer::SilkscreenTop,
            tr!("properties-graphic-silkscreen-top"),
        ),
        (
            FootprintLayer::SilkscreenBottom,
            tr!("properties-graphic-silkscreen-bottom"),
        ),
        (
            FootprintLayer::CourtyardTop,
            tr!("properties-graphic-courtyard-top"),
        ),
        (
            FootprintLayer::CourtyardBottom,
            tr!("properties-graphic-courtyard-bottom"),
        ),
        (
            FootprintLayer::AssemblyTop,
            tr!("properties-graphic-assembly-top"),
        ),
        (
            FootprintLayer::AssemblyBottom,
            tr!("properties-graphic-assembly-bottom"),
        ),
    ];
    for (i, g) in rows.graphics.iter().enumerate() {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(tr!(
                "properties-graphic-title",
                n = i + 1,
                shape = shape_name(&g.shape)
            ));
            let delete = ui.button(tr!("properties-graphic-delete"));
            if delete.clicked() {
                edits.push(GraphicEdit::Delete(g.uuid));
            }
        });
        Grid::new(("properties-graphic", i))
            .num_columns(2)
            .show(ui, |ui| {
                let key = format!("graphic-{i}-layer");
                if let Some(l) = combo(
                    ui,
                    &key,
                    &tr!("properties-graphic-layer"),
                    &Shared::Same(g.layer),
                    &layers,
                ) {
                    edits.push(GraphicEdit::Layer(g.uuid, l));
                }
                let key = format!("graphic-{i}-width");
                if let Some(w) = length(
                    ui,
                    &key,
                    &tr!("properties-width"),
                    Shared::Same(g.width),
                    units,
                ) {
                    edits.push(GraphicEdit::Width(g.uuid, w));
                }
            });
    }
    edits
}

fn shape_name(shape: &Shape) -> String {
    match serde_json::to_value(shape) {
        Ok(v) => v["kind"].as_str().unwrap_or("shape").to_owned(),
        Err(_) => "shape".to_owned(),
    }
}
