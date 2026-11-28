//! `pcb.import_dxf.preview` and `pcb.import_dxf`: a DXF drawing onto board Layers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, NO_PROJECT, Registry,
    Unavailable,
};
use reda_model::geom::{Nm, Point};
use reda_model::{InDocument, Object, PcbDocument, Uuid};
use reda_pcb::DEFAULT_GRAPHIC_WIDTH;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::import::{Options, Report, plan, report};
use crate::mapping::Target;
use crate::units::Unit;
use crate::{Drawing, ReadError, read};

/// Adds the DXF import Commands.
pub fn register_all(registry: &mut Registry) {
    registry.add(Preview);
    registry.add(Import);
}

/// How far apart line ends may be and still close a loop when left out: 0.001 mm, the
/// loop builder's own.
const CLOSE_GAPS: Nm = Nm(1_000);

const NO_PCB: &str = "the Project has no PCB Document; add one with `document.add {kind: pcb}`";

fn needs_pcb(cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
    let reason = match cx.project {
        None => NO_PROJECT,
        Some(project) if project.pcb().is_none() => NO_PCB,
        Some(_) => return Ok(()),
    };
    Err(Unavailable {
        reason: reason.into(),
    })
}

fn pcb<'c>(cx: &'c Context<'_>) -> Result<&'c PcbDocument, CommandError> {
    cx.project()?
        .pcb()
        .ok_or_else(|| CommandError::unavailable(NO_PCB))
}

/// A point param.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PointParam {
    pub x: Length,
    pub y: Length,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewParams {
    /// The DXF file, as an absolute path.
    pub path: PathBuf,
    /// The drawing's unit; `$INSUNITS`'s when left out, or mm when the file has none.
    #[serde(default)]
    pub unit: Option<Unit>,
    /// A factor the drawing is scaled by; 1 when left out.
    #[serde(default)]
    pub scale: Option<f64>,
    /// How far apart line ends may be and still close a loop on the Board Outline row;
    /// 0.001 mm when left out.
    #[serde(default)]
    pub close_gaps: Option<Length>,
    /// Targets by DXF layer name; the rows left out are pre-filled.
    #[serde(default)]
    pub mapping: BTreeMap<String, Target>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportParams {
    /// The DXF file, as an absolute path.
    pub path: PathBuf,
    /// The drawing's unit; `$INSUNITS`'s when left out, or mm when the file has none.
    #[serde(default)]
    pub unit: Option<Unit>,
    /// A factor the drawing is scaled by; 1 when left out.
    #[serde(default)]
    pub scale: Option<f64>,
    /// How far apart line ends may be and still close a loop on the Board Outline row;
    /// 0.001 mm when left out.
    #[serde(default)]
    pub close_gaps: Option<Length>,
    /// The pen width of every line and arc; 0.15 mm when left out.
    #[serde(default)]
    pub line_width: Option<Length>,
    /// Targets by DXF layer name; the rows left out are pre-filled.
    #[serde(default)]
    pub mapping: BTreeMap<String, Target>,
    /// Place with cursor: the point the drawing's lower-left corner lands on, instead of
    /// its file coordinates on the DXF origin point.
    #[serde(default)]
    pub at: Option<PointParam>,
}

/// What an import did.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ImportResult {
    #[serde(flatten)]
    pub report: Report,
    /// The new graphics and texts, in file order, then the new Board Outline and Board
    /// Cutouts, ready to select.
    pub objects: Vec<Uuid>,
    /// The Board Outline made from the Board Outline row's largest loop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board_outline: Option<Uuid>,
    /// The Board Cutouts made from the loops inside it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub board_cutouts: Vec<Uuid>,
    /// The Mechanical Layers the import added.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers_added: Vec<Uuid>,
    /// What the Messages panel says about the import.
    pub messages: Vec<String>,
}

fn read_file(path: &Path) -> Result<Drawing, CommandError> {
    if path.is_relative() {
        return Err(CommandError::invalid_params(
            "/path",
            "give the DXF file as an absolute path",
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            CommandError::not_found(format!("{} does not exist", path.display()))
        }
        _ => CommandError::rejected(format!("could not read {}: {e}", path.display())),
    })?;
    read(&bytes).map_err(|e| match e {
        ReadError::Dwg => CommandError::rejected(format!(
            "{} is a DWG drawing, which DXF import does not read; save it as DXF (ASCII or \
             binary, any version) from the CAD tool that made it, and import that",
            path.display()
        )),
        e => CommandError::rejected(format!(
            "{} is not a readable DXF file: {e}",
            path.display()
        )),
    })
}

fn options(
    unit: Option<Unit>,
    scale: Option<f64>,
    close_gaps: Option<Length>,
    line_width: Option<Length>,
    mapping: BTreeMap<String, Target>,
    at: Option<PointParam>,
) -> Result<Options, CommandError> {
    let scale = scale.unwrap_or(1.0);
    if !(scale.is_finite() && scale > 0.0) {
        return Err(CommandError::invalid_params(
            "/scale",
            "the scale must be a positive number",
        ));
    }
    let close_gaps = close_gaps.map_or(CLOSE_GAPS, Length::nm);
    if close_gaps < Nm::ZERO {
        return Err(CommandError::invalid_params(
            "/close_gaps",
            "close gaps up to cannot be negative",
        ));
    }
    let line_width = line_width.map_or(DEFAULT_GRAPHIC_WIDTH, Length::nm);
    if line_width <= Nm::ZERO {
        return Err(CommandError::invalid_params(
            "/line_width",
            "the line width must be positive",
        ));
    }
    Ok(Options {
        unit,
        scale,
        close_gaps,
        line_width,
        mapping,
        at: at.map(|p| Point::new(p.x.nm(), p.y.nm())),
    })
}

pub struct Preview;

impl Command for Preview {
    const META: Meta = Meta {
        id: "pcb.import_dxf.preview",
        title: "Preview DXF Import",
        description: "Reads a DXF file and returns what `pcb.import_dxf` would do, changing \
                      nothing: each DXF layer with its entity count and pre-filled target, the \
                      unit and the drawing's size in mm, the entities it skips, counted per \
                      type, and what the Board Outline row makes: whether it builds and \
                      replaces the Board Outline, its Board Cutouts, and each gap (with its \
                      size) and crossing, ends meeting within `close_gaps`.",
        category: "pcb",
        icon: "library_import",
        kind: Kind::Query,
    };
    type Params = PreviewParams;
    type Output = Report;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: PreviewParams) -> Result<Report, CommandError> {
        let options = options(p.unit, p.scale, p.close_gaps, None, p.mapping, None)?;
        let drawing = read_file(&p.path)?;
        report(pcb(cx)?, &drawing, &options)
    }
}

pub struct Import;

impl Command for Import {
    const META: Meta = Meta {
        id: "pcb.import_dxf",
        title: "Import DXF",
        description: "Imports a DXF drawing, ASCII or binary, as one undo step: lines, \
                      arcs, circles and polylines as board graphics, bulges as exact arcs, \
                      ellipses and splines fitted with arcs within 0.005 mm, TEXT and MTEXT \
                      as free text in the stroke font, and each INSERT exploded in place. \
                      DWG is refused. It lands at its file coordinates on the Output Origin \
                      (design zero without one), or with its lower-left corner at `at`. Each \
                      DXF layer goes to its `mapping` target or the pre-filled one: a board \
                      Layer of the same name, the Board Outline for an outline-like name, a \
                      Mechanical Layer whose Purpose the name suggests, or else the Reference \
                      Layer, added if the board has none. The Board Outline row's largest \
                      loop replaces the Board Outline, and the loops wholly inside it, \
                      circles included, replace the Board Cutouts; open and crossing shapes \
                      and other loops land on the Reference Layer, with Messages saying why. \
                      Returns the preview's report and the new objects.",
        category: "pcb",
        icon: "library_import",
        kind: Kind::Edit,
    };
    type Params = ImportParams;
    type Output = ImportResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ImportParams) -> Result<ImportResult, CommandError> {
        let options = options(p.unit, p.scale, p.close_gaps, p.line_width, p.mapping, p.at)?;
        let drawing = read_file(&p.path)?;
        let (project, ids) = cx.project_and_ids()?;
        let pcb = project
            .pcb()
            .ok_or_else(|| CommandError::unavailable(NO_PCB))?;
        let document = pcb.uuid;
        let plan = plan(pcb, &drawing, &options, &mut || ids.next_id())?;
        let tx = cx.tx()?;
        if let Some(layers) = plan.layers {
            tx.replace(Object::LayerSet(InDocument::new(document, layers)))?;
        }
        for graphic in plan.graphics {
            tx.insert(Object::PcbGraphic(InDocument::new(document, graphic)))?;
        }
        for text in plan.texts {
            tx.insert(Object::PcbText(InDocument::new(document, text)))?;
        }
        for old in plan.replaced {
            tx.remove(old)?;
        }
        let board_outline = plan.outline.as_ref().map(|o| o.uuid);
        if let Some(outline) = plan.outline {
            tx.insert(Object::BoardOutline(InDocument::new(document, outline)))?;
        }
        let board_cutouts = plan.cutouts.iter().map(|c| c.uuid).collect();
        for cutout in plan.cutouts {
            tx.insert(Object::BoardCutout(InDocument::new(document, cutout)))?;
        }
        Ok(ImportResult {
            report: plan.report,
            objects: plan.objects,
            board_outline,
            board_cutouts,
            layers_added: plan.layers_added,
            messages: plan.messages,
        })
    }
}
