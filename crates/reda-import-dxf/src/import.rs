//! A read drawing on a PCB Document: the [`Report`] a preview returns, and the Layers and
//! graphics an import adds ([`plan`]).
//!
//! Units, scale and position: drawing units × the unit's nanometres × `scale`, measured
//! from the DXF origin point (the Output Origin, otherwise design zero). Board Y grows
//! upwards, as DXF's does, so Y keeps its sign. With `at`, the drawing's lower-left corner
//! lands there instead.

use std::collections::BTreeMap;

use reda_command::CommandError;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{GraphicShape, PcbDocument, PcbGraphic, Uuid};
use schemars::JsonSchema;
use serde::Serialize;

use crate::mapping::{Rule, Target, new_mechanical, prefill, reference_layer, takes_drawing};
use crate::shape::{Piece, Xy};
use crate::units::Unit;
use crate::{Drawing, Entity};

/// How far from design zero an imported drawing may reach: 1 m. Further than that is a
/// wrong unit or scale, and would overflow the board's geometry.
pub const REACH: Nm = Nm(1_000_000_000);

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
    let mut warnings = Vec::new();
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
            Target::Reference => {
                Landing::Layer(reference_layer(&mut layers, &mut layers_added, new_id))
            }
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
    let imported: Vec<&Entity> = drawing
        .entities
        .iter()
        .filter(|e| landings[e.layer.as_str()] != Landing::Skip)
        .collect();
    let pieces: Vec<(&str, Piece)> = imported
        .iter()
        .flat_map(|e| e.shape.pieces().into_iter().map(|p| (e.layer.as_str(), p)))
        .collect();
    let bounds = extent(pieces.iter().map(|(_, p)| p));
    let k = unit.nm() * options.scale;
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
            let layer = reference_layer(&mut layers, &mut layers_added, new_id);
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
            warnings.push(format!(
                "the drawing reaches {:.1} mm from design zero, more than 1 m, and can't be \
                 imported so; check the unit and scale",
                far / 1e6
            ));
        }
    }

    let mut graphics = Vec::new();
    let mut dropped = 0;
    for (dxf_layer, piece) in pieces.iter().filter(|_| build) {
        let layer = match landings[dxf_layer] {
            Landing::Layer(uuid) => uuid,
            Landing::BoardOutline => outline_layer.expect("added above"),
            Landing::Skip => continue,
        };
        let Some(shape) = graphic(*piece, &place)? else {
            dropped += 1;
            continue;
        };
        graphics.push(PcbGraphic {
            uuid: new_id(),
            locked: false,
            layer,
            width: options.line_width,
            shape,
        });
    }
    if dropped > 0 {
        warnings.push(format!(
            "{dropped} lines or arcs shorter than a nanometre were left out"
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
        skipped: drawing.skipped.clone(),
        warnings,
    };
    Ok(Plan {
        report,
        layers: (!layers_added.is_empty()).then_some(layers),
        layers_added,
        graphics,
        messages,
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

/// The smallest box around `pieces`, arcs by their extreme points.
fn extent<'p>(pieces: impl Iterator<Item = &'p Piece>) -> Option<(Xy, Xy)> {
    let mut points = Vec::new();
    for piece in pieces {
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
/// when it rounds to nothing.
fn graphic(
    piece: Piece,
    place: &dyn Fn(Xy) -> (f64, f64),
) -> Result<Option<GraphicShape>, CommandError> {
    let point = |p: Xy| -> Result<Point, CommandError> {
        let (x, y) = place(p);
        if x.abs() > REACH.0 as f64 || y.abs() > REACH.0 as f64 {
            return Err(CommandError::rejected(format!(
                "the drawing reaches ({:.1} mm, {:.1} mm), more than 1 m from design zero; \
                 check the unit and scale",
                x / 1e6,
                y / 1e6
            )));
        }
        Ok(Point::new(Nm(x.round() as i64), Nm(y.round() as i64)))
    };
    let shape = match piece {
        Piece::Line { from, to } => GraphicShape::Line {
            from: point(from)?,
            to: point(to)?,
        },
        Piece::Arc {
            from, to, sweep, ..
        } => {
            let (from, to) = (point(from)?, point(to)?);
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
    Ok((from != to).then_some(shape))
}
