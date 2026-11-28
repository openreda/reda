//! A read drawing on a PCB Document: the [`Report`] a preview returns, and the Layers and
//! graphics an import adds ([`plan`]).
//!
//! Units, scale and position: drawing units × the unit's nanometres × `scale`, measured
//! from the DXF origin point (the Output Origin, otherwise design zero). Board Y grows
//! upwards, as DXF's does, so Y keeps its sign. With `at`, the drawing's lower-left corner
//! lands there instead.
//!
//! Graphics become lines and arcs at one line width, ellipses and splines fitted within
//! [`FIT_TOLERANCE`]; TEXT and MTEXT become free text in the default stroke font.

use std::collections::BTreeMap;

use reda_command::CommandError;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{GraphicShape, PcbDocument, PcbGraphic, PcbText, TextStyle, Uuid};
use schemars::JsonSchema;
use serde::Serialize;

use crate::Drawing;
use crate::mapping::{Rule, Target, new_mechanical, prefill, reference_layer, takes_drawing};
use crate::shape::{Piece, Shape, Xy};
use crate::text::Text;
use crate::units::Unit;

/// How far from design zero an imported drawing may reach: 1 m. Further than that is a
/// wrong unit or scale, and would overflow the board's geometry.
pub const REACH: Nm = Nm(1_000_000_000);

/// How far a fitted ellipse or spline may stray from the true curve: 0.005 mm.
pub const FIT_TOLERANCE: Nm = Nm(5_000);

/// The skipped count of text on the row that becomes the Board Outline.
const OUTLINE_TEXT: &str = "text on the Board Outline layer";

/// What the Commands' params ask for, defaults filled in.
#[derive(Clone, Debug)]
pub struct Options {
    /// The unit to read the drawing in; `$INSUNITS`'s, or mm, when left out.
    pub unit: Option<Unit>,
    pub scale: f64,
    pub close_gaps: Nm,
    pub line_width: Nm,
    /// Targets by DXF layer name; the rows left out are pre-filled.
    pub mapping: BTreeMap<String, Target>,
    /// Place with cursor: where the drawing's lower-left corner lands.
    pub at: Option<Point>,
}

/// The drawing's resulting size in mm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, JsonSchema)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// One DXF layer that holds readable entities, and where it goes.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct LayerRow {
    /// The DXF layer's name, as `mapping` keys it.
    pub name: String,
    /// Its readable entities.
    pub entities: usize,
    /// Its target as `mapping` takes it: `skip`, `board_outline`, the Layer's UUID, or
    /// `new_mechanical` or `reference` for a Layer the import adds.
    pub target: String,
    /// The name of the board Layer it lands on; left out for `skip` and `board_outline`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_name: Option<String>,
    /// The import adds that Layer.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub new_layer: bool,
    /// The pre-fill rule that chose the target; left out when `mapping` gave it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefill: Option<Rule>,
}

/// What a preview returns, and an import with it.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct Report {
    /// The unit the drawing is read in.
    pub unit: Unit,
    /// The unit `$INSUNITS` gives; left out when it is missing, unitless or not a length
    /// a board drawing uses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_unit: Option<Unit>,
    pub scale: f64,
    /// The width and height of the imported rows' entities, in mm, as they land.
    pub size: Size,
    /// Where the DXF file's origin lands: the Output Origin or design zero, or wherever
    /// `at` moves it.
    pub origin: Point,
    /// The pen width of every imported line and arc.
    pub line_width: Nm,
    /// How far apart line ends may be and still close a loop on the Board Outline row.
    pub close_gaps: Nm,
    /// The DXF layers that hold readable entities, in the order they first appear.
    pub layers: Vec<LayerRow>,
    /// The entities not imported, counted per DXF entity type.
    pub skipped: BTreeMap<String, usize>,
    pub warnings: Vec<String>,
}

/// What an import adds.
#[derive(Clone, Debug)]
pub struct Plan {
    pub report: Report,
    /// The Layer Set with the Layers the import adds, when it adds any.
    pub layers: Option<reda_model::LayerSet>,
    pub layers_added: Vec<Uuid>,
    /// The new graphics, in entity order.
    pub graphics: Vec<PcbGraphic>,
    /// The new free texts, in entity order.
    pub texts: Vec<PcbText>,
    /// Every new graphic and text, in entity order.
    pub objects: Vec<Uuid>,
    /// What the Messages panel says about the import.
    pub messages: Vec<String>,
}

/// Where a row's pieces go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Landing {
    Skip,
    BoardOutline,
    Layer(Uuid),
}

/// What importing `drawing` onto `pcb` would do, changing nothing.
pub fn report(
    pcb: &PcbDocument,
    drawing: &Drawing,
    options: &Options,
) -> Result<Report, CommandError> {
    // Made-up IDs, for the Layers the import would add.
    let mut next = 0u128;
    let mut made_up = || {
        next += 1;
        Uuid::from_u128(next)
    };
    Ok(planned(pcb, drawing, options, &mut made_up, false)?.report)
}

/// Plans putting `drawing` on `pcb`: the Layers and graphics to add.
pub fn plan(
    pcb: &PcbDocument,
    drawing: &Drawing,
    options: &Options,
    new_id: &mut dyn FnMut() -> Uuid,
) -> Result<Plan, CommandError> {
    planned(pcb, drawing, options, new_id, true)
}

fn planned(
    pcb: &PcbDocument,
    drawing: &Drawing,
    options: &Options,
    new_id: &mut dyn FnMut() -> Uuid,
    build: bool,
) -> Result<Plan, CommandError> {
    let mut warnings = drawing.warnings.clone();
    let file_unit = drawing.insunits.and_then(Unit::from_insunits);
    if let Some(code) = drawing.insunits
        && code != 0
        && file_unit.is_none()
    {
        warnings.push(format!(
            "$INSUNITS is {code}, which is not a unit for board drawings; mm is assumed"
        ));
    }
    let unit = options.unit.or(file_unit).unwrap_or(Unit::Mm);

    // Rows in first-appearance order, with their entity counts.
    let mut names: Vec<&str> = Vec::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for entity in &drawing.entities {
        let count = counts.entry(&entity.layer).or_default();
        if *count == 0 {
            names.push(&entity.layer);
        }
        *count += 1;
    }
    for name in options.mapping.keys() {
        if !counts.contains_key(name.as_str()) {
            return Err(CommandError::invalid_params(
                &pointer(name),
                format!("the drawing has no DXF layer {name:?} with entities to import"),
            ));
        }
    }

    // Targets: given ones first, then the pre-fill, with one Board Outline row at most.
    let mut outline_free = true;
    for name in &names {
        if options.mapping.get(*name) == Some(&Target::BoardOutline) {
            if !outline_free {
                return Err(CommandError::invalid_params(
                    &pointer(name),
                    "only one DXF layer can become the Board Outline",
                ));
            }
            outline_free = false;
        }
    }
    let mut layers = pcb.layers().clone();
    let mut layers_added = Vec::new();
    let mut added_reference = None;
    let mut rows = Vec::new();
    let mut landings = BTreeMap::new();
    for name in &names {
        let (target, rule) = match options.mapping.get(*name) {
            Some(target) => (*target, None),
            None => {
                let (target, rule) = prefill(pcb, name, outline_free);
                (target, Some(rule))
            }
        };
        if target == Target::BoardOutline {
            outline_free = false;
        }
        let landing = match target {
            Target::Skip => Landing::Skip,
            Target::BoardOutline => Landing::BoardOutline,
            Target::NewMechanical => {
                Landing::Layer(new_mechanical(&mut layers, name, &mut layers_added, new_id))
            }
            Target::Reference => Landing::Layer(reference_layer(
                pcb.layers(),
                &mut layers,
                &mut layers_added,
                &mut added_reference,
                new_id,
            )),
            Target::Layer(layer) => {
                let layer = layer.resolve(pcb, &pointer(name))?;
                if !takes_drawing(layer) {
                    return Err(CommandError::invalid_params(
                        &pointer(name),
                        "a DXF layer goes on the Board Outline or on a copper, silkscreen, \
                         Courtyard, Assembly or Mechanical Layer",
                    ));
                }
                Landing::Layer(layer.uuid)
            }
        };
        let (layer_name, new_layer) = match landing {
            Landing::Layer(uuid) => (
                layers.layer(uuid).map(|l| pcb.layer_title(l)),
                layers_added.contains(&uuid),
            ),
            _ => (None, false),
        };
        let target = match (landing, target.word()) {
            (Landing::Layer(uuid), _) if !new_layer => uuid.to_string(),
            (_, Some(word)) => word.to_owned(),
            (_, None) => unreachable!("a Layer target lands on an existing Layer"),
        };
        rows.push(LayerRow {
            name: (*name).to_owned(),
            entities: counts[name],
            target,
            layer_name,
            new_layer,
            prefill: rule,
        });
        landings.insert(*name, landing);
    }
    let layer_name = |layers: &reda_model::LayerSet, uuid| {
        layers
            .layer(uuid)
            .and_then(|l| l.name.clone())
            .unwrap_or_default()
    };

    // Placement.
    let k = unit.nm() * options.scale;
    // A nanometre is left for rounding to the board's grid.
    let tolerance = (FIT_TOLERANCE.0 - 1) as f64 / k;
    let mut skipped = drawing.skipped.clone();
    let mut items: Vec<(&str, Item<'_>)> = Vec::new();
    for entity in &drawing.entities {
        let landing = landings[entity.layer.as_str()];
        match (&entity.shape, landing) {
            (_, Landing::Skip) => {}
            (Shape::Text(_), Landing::BoardOutline) => {
                *skipped.entry(OUTLINE_TEXT.to_owned()).or_default() += 1
            }
            (Shape::Text(text), _) => items.push((&entity.layer, Item::Text(text))),
            (shape, _) => items.extend(
                shape
                    .pieces(tolerance)
                    .into_iter()
                    .map(|p| (entity.layer.as_str(), Item::Piece(p))),
            ),
        }
    }
    let bounds = extent(items.iter().map(|(_, item)| item));
    let dxf_origin = pcb
        .output_origin()
        .map_or(Point::new(Nm(0), Nm(0)), |o| o.at);
    let origin = match (options.at, bounds) {
        (Some(at), Some((min, _))) => Xy::new(at.x.0 as f64 - min.x * k, at.y.0 as f64 - min.y * k),
        _ => Xy::new(dxf_origin.x.0 as f64, dxf_origin.y.0 as f64),
    };
    let size = bounds.map_or(
        Size {
            width: 0.0,
            height: 0.0,
        },
        |(min, max)| Size {
            width: in_mm((max.x - min.x) * k),
            height: in_mm((max.y - min.y) * k),
        },
    );
    let place = |p: Xy| (origin.x + p.x * k, origin.y + p.y * k);

    // The Board Outline row's pieces wait on the Reference Layer, where
    // `pcb.board_outline.add_from_graphics` converts them.
    let mut messages = Vec::new();
    let mut outline_layer = None;
    for row in &rows {
        if landings[row.name.as_str()] == Landing::BoardOutline {
            let layer = reference_layer(
                pcb.layers(),
                &mut layers,
                &mut layers_added,
                &mut added_reference,
                new_id,
            );
            outline_layer = Some(layer);
            messages.push(format!(
                "DXF layer {:?} went on the Layer {:?}, not the Board Outline: select its \
                 graphics there and use pcb.board_outline.add_from_graphics to make the \
                 Board Outline.",
                row.name,
                layer_name(&layers, layer)
            ));
        }
    }

    if let Some((min, max)) = bounds {
        let far = [place(min), place(max)]
            .into_iter()
            .flat_map(|(x, y)| [x.abs(), y.abs()])
            .fold(0.0, f64::max);
        if far > REACH.0 as f64 {
            let reach = format!(
                "the drawing reaches {:.1} mm from design zero, more than 1 m, and can't be \
                 imported so; check the unit and scale",
                far / 1e6
            );
            if build {
                return Err(CommandError::rejected(reach));
            }
            warnings.push(reach);
        }
    }

    let mut graphics = Vec::new();
    let mut texts = Vec::new();
    let mut objects = Vec::new();
    let (mut dropped, mut dropped_texts) = (0, 0);
    for (dxf_layer, item) in items.iter().filter(|_| build) {
        let layer = match landings[dxf_layer] {
            Landing::Layer(uuid) => uuid,
            Landing::BoardOutline => outline_layer.expect("added above"),
            Landing::Skip => continue,
        };
        match item {
            Item::Piece(piece) => {
                let Some(shape) = graphic(*piece, &place) else {
                    dropped += 1;
                    continue;
                };
                let uuid = new_id();
                objects.push(uuid);
                graphics.push(PcbGraphic {
                    uuid,
                    locked: false,
                    layer,
                    width: options.line_width,
                    shape,
                });
            }
            Item::Text(text) => {
                let Some(text) = free_text(text, layer, k, &place, new_id) else {
                    dropped_texts += 1;
                    continue;
                };
                objects.push(text.uuid);
                texts.push(text);
            }
        }
    }
    if dropped > 0 {
        warnings.push(format!(
            "{dropped} lines or arcs shorter than a nanometre were left out"
        ));
    }
    if dropped_texts > 0 {
        warnings.push(format!(
            "{dropped_texts} texts less than a nanometre high were left out"
        ));
    }

    let report = Report {
        unit,
        file_unit,
        scale: options.scale,
        size,
        origin: Point::new(Nm(origin.x.round() as i64), Nm(origin.y.round() as i64)),
        line_width: options.line_width,
        close_gaps: options.close_gaps,
        layers: rows,
        skipped,
        warnings,
    };
    Ok(Plan {
        report,
        layers: (!layers_added.is_empty()).then_some(layers),
        layers_added,
        graphics,
        texts,
        objects,
        messages,
    })
}

/// What one entity lands as: pieces of a graphic, or a text.
enum Item<'d> {
    Piece(Piece),
    Text(&'d Text),
}

/// A text as free board text on `layer`, `k` nanometres to the drawing unit, in the default
/// stroke font; `None` when it is less than a nanometre high.
fn free_text(
    text: &Text,
    layer: Uuid,
    k: f64,
    place: &dyn Fn(Xy) -> (f64, f64),
    new_id: &mut dyn FnMut() -> Uuid,
) -> Option<PcbText> {
    let height = Nm((text.height * k).round() as i64);
    if height < Nm(1) {
        return None;
    }
    let (x, y) = place(text.at);
    let mut style = TextStyle::of_height(height);
    style.justify = text.justify;
    Some(PcbText {
        uuid: new_id(),
        locked: false,
        layer,
        text: reda_model::text::escape(&text.text).into_owned(),
        position: Point::new(Nm(x.round() as i64), Nm(y.round() as i64)),
        style,
        frame: None,
        rotation: Millideg((text.rotation * 1000.0).round() as i64).normalized(),
        mirrored: false,
    })
}

/// The JSON pointer of the `mapping` row `name`.
fn pointer(name: &str) -> String {
    format!("/mapping/{}", name.replace('~', "~0").replace('/', "~1"))
}

/// A length in drawing nanometres as mm, to the nanometre.
fn in_mm(nm: f64) -> f64 {
    nm.round() / 1e6
}

/// The smallest box around `items`, arcs by their extreme points and texts by their
/// anchors.
fn extent<'i>(items: impl Iterator<Item = &'i Item<'i>>) -> Option<(Xy, Xy)> {
    let mut points = Vec::new();
    for item in items {
        let piece = match item {
            Item::Piece(piece) => piece,
            Item::Text(text) => {
                points.push(text.at);
                continue;
            }
        };
        match *piece {
            Piece::Line { from, to } => points.extend([from, to]),
            Piece::Arc {
                from,
                to,
                sweep,
                centre,
                radius,
            } => {
                points.extend([from, to]);
                let start = (from.y - centre.y).atan2(from.x - centre.x).to_degrees();
                let (low, high) = if sweep > 0.0 {
                    (start, start + sweep)
                } else {
                    (start + sweep, start)
                };
                // Every quarter turn the arc passes.
                let first = (low / 90.0).ceil() as i64;
                let last = (high / 90.0).floor() as i64;
                for quarter in first..=last {
                    let (x, y) = match quarter.rem_euclid(4) {
                        0 => (radius, 0.0),
                        1 => (0.0, radius),
                        2 => (-radius, 0.0),
                        _ => (0.0, -radius),
                    };
                    points.push(Xy::new(centre.x + x, centre.y + y));
                }
            }
        }
    }
    // A point past what a float holds makes the drawing endless, so it is refused on its
    // size.
    if points.iter().any(|p| !(p.x.is_finite() && p.y.is_finite())) {
        let endless = f64::INFINITY;
        return Some((Xy::new(-endless, -endless), Xy::new(endless, endless)));
    }
    let mut points = points.into_iter();
    let first = points.next()?;
    Some(points.fold((first, first), |(min, max), p| {
        (
            Xy::new(min.x.min(p.x), min.y.min(p.y)),
            Xy::new(max.x.max(p.x), max.y.max(p.y)),
        )
    }))
}

/// A piece as a board graphic, through `place` (drawing units to board nanometres); `None`
/// when it rounds to nothing. The drawing's bounds are within [`REACH`] by now.
fn graphic(piece: Piece, place: &dyn Fn(Xy) -> (f64, f64)) -> Option<GraphicShape> {
    let point = |p: Xy| {
        let (x, y) = place(p);
        Point::new(Nm(x.round() as i64), Nm(y.round() as i64))
    };
    let shape = match piece {
        Piece::Line { from, to } => GraphicShape::Line {
            from: point(from),
            to: point(to),
        },
        Piece::Arc {
            from, to, sweep, ..
        } => {
            let (from, to) = (point(from), point(to));
            let sweep = Millideg((sweep * 1000.0).round() as i64);
            if sweep == Millideg::ZERO {
                GraphicShape::Line { from, to }
            } else {
                GraphicShape::Arc { from, to, sweep }
            }
        }
    };
    let (GraphicShape::Line { from, to } | GraphicShape::Arc { from, to, .. }) = shape else {
        unreachable!("only lines and arcs are made")
    };
    (from != to).then_some(shape)
}
