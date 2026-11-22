//! `library.board_template.save`: Save Board as Template (`docs/spec/v0.2.5/save-as-template.md`).
//!
//! The active board's outline and the chosen objects become a new `.redaboard` in a
//! writable Library: shifted so the outline's bounding box starts at (0,0), every number a
//! literal, each row's `locked` as the object has it, and fresh UUIDs. [`candidates`] is
//! what the dialog lists and what an omitted `objects` takes the defaults of; [`capture`]
//! builds the template.

use std::collections::BTreeMap;
use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_library::board_template::{BOARD_TEMPLATE_EXTENSION, BOARD_TEMPLATE_FOLDER, check};
use reda_library::{Library, LibraryIndex, check_element_name};
use reda_model::geom::{Millideg, Nm, Point, Vertex, contour_bounds};
use reda_model::local_edit::local_edits;
use reda_model::{
    BoardTemplate, ComponentType, Dimension, DimensionEnd, DrawingKind, DrawingLayer,
    FootprintCopy, FootprintInstance, FreePad, GraphicShape, Hotspot, Keepout, KeepoutShape,
    KeepoutShapeKind, LayerKind, PartEntry, PcbDocument, PlatedEdges, Project, SchematicSheet,
    Side, TemplateBevel, TemplateBoard, TemplateContour, TemplateCopperLayer, TemplateDimensionEnd,
    TemplateDrawing, TemplateDrawingGroup, TemplateDrill, TemplateEdgeConnector, TemplateFollow,
    TemplateHole, TemplateKeepout, TemplateNumber, TemplatePad, TemplatePart, TemplatePlacement,
    TemplatePoint, TemplateVertex, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::fs::write_atomic;
use crate::library::{copy_part, scan_registered, writable_library};

/// Why an inner-layer Keepout can't be ticked.
pub const INNER_KEEPOUT: &str = "inner-layer Keepouts can't go in a template yet";

pub struct BoardTemplateSave;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveParams {
    /// The writable registered Library to write into; not the Starter Library.
    pub library: Uuid,
    /// The template's name, unique within the Library; also its file stem.
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Groups the picker; left out, the template is listed under "Other".
    #[serde(default)]
    pub category: Option<String>,
    /// The board objects to capture beyond the outline, which always comes: Board
    /// Cutouts, Keepouts, Components, Mechanical Parts and Free Pads by UUID, and a
    /// Mechanical or Documentation Layer's UUID for every graphic, text and Dimension on
    /// it. Left out, the dialog's defaults: everything but inner-layer Keepouts, with
    /// Components, Mechanical Parts and Free Pads only when Locked.
    #[serde(default)]
    pub objects: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SaveResult {
    /// The new template's Library element UUID.
    pub element: Uuid,
    /// The file written.
    pub path: PathBuf,
    /// What the template leaves behind, such as a Footprint's local Pad edits.
    pub warnings: Vec<String>,
}

impl Command for BoardTemplateSave {
    const META: Meta = Meta {
        id: "library.board_template.save",
        title: "Save Board as Template",
        description: "Writes the active board's outline and the chosen `objects` (Board \
                      Cutouts, Keepouts, Components, Mechanical Parts, Free Pads, and the \
                      Mechanical and Documentation Layers' drawings) as a new Board \
                      Template `board-templates/<name>.redaboard` in a writable Library: \
                      shifted so the outline's bounding box starts at (0,0), as literal \
                      numbers with each object's Locked state and fresh UUIDs. Parts and \
                      Footprints are the unedited copies; Plated Edges keep their mark \
                      without a Net; Keepout Layers become `all_copper`, `top_copper` or \
                      `bottom_copper`. Refuses a name the Library already has, the Starter \
                      or a read-only Library, a board with no outline and an inner-layer \
                      Keepout. Returns the element, its path and warnings.",
        category: "library",
        icon: "board_outline_rect",
        kind: Kind::File,
    };
    type Params = SaveParams;
    type Output = SaveResult;

    fn run(&self, cx: &mut Context<'_>, p: SaveParams) -> Result<SaveResult, CommandError> {
        check_element_name(&p.name).map_err(|why| CommandError::invalid_params("/name", why))?;
        let element = cx.new_id();
        let session = cx.session()?;
        let project = session
            .project
            .clone()
            .ok_or_else(|| CommandError::unavailable("no Project is open"))?;
        let index = scan_registered(session);
        let library = writable_library(&index, p.library)?;
        let path = template_file(library, &p.name);
        if library.board_templates.values().any(|t| t.name == p.name) || path.exists() {
            return Err(CommandError::rejected(format!(
                "Library {} already has a Board Template named {:?}; choose another name, \
                 or delete the old template in the Library panel first",
                library.info.name, p.name
            )));
        }
        if read_only(library) {
            return Err(CommandError::rejected(format!(
                "Library {} is read-only; choose a Library you can write to",
                library.info.name
            )));
        }
        let mut ids = || cx.new_id();
        let captured = capture(&project, &index, &p, element, &mut ids)?;
        let problems = check(&captured.template);
        if !problems.is_empty() {
            return Err(CommandError::internal(format!(
                "the captured template fails its load checks: {}",
                problems.join("; ")
            )));
        }
        let bytes = crate::format::write_board_template(&captured.template)
            .map_err(|e| CommandError::internal(e.to_string()))?;
        let io =
            |e: std::io::Error| CommandError::internal(format!("writing {}: {e}", path.display()));
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        write_atomic(&path, &bytes).map_err(io)?;
        Ok(SaveResult {
            element,
            path,
            warnings: captured.warnings,
        })
    }
}

/// `<library>/board-templates/<name>.redaboard`.
fn template_file(library: &Library, name: &str) -> PathBuf {
    library
        .root
        .join(BOARD_TEMPLATE_FOLDER)
        .join(format!("{name}.{BOARD_TEMPLATE_EXTENSION}"))
}

/// Whether the Library's folder (or its `board-templates/`) can't be written.
fn read_only(library: &Library) -> bool {
    let folder = library.root.join(BOARD_TEMPLATE_FOLDER);
    let dir = if folder.is_dir() {
        folder
    } else {
        library.root.clone()
    };
    std::fs::metadata(dir).is_ok_and(|m| m.permissions().readonly())
}

/// What a candidate object is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Outline,
    Cutout,
    Keepout,
    Component,
    MechanicalPart,
    FreePad,
    /// Every graphic, text and Dimension on one Mechanical or Documentation Layer.
    Drawings,
}

/// One object the dialog lists, with its tick's default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Candidate {
    /// The board object's UUID, or the Layer's for drawings: what `objects` names.
    pub object: Uuid,
    pub kind: CandidateKind,
    /// A Component's or Mechanical Part's Designator.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub designator: Option<String>,
    /// A Component's or Mechanical Part's Footprint name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    /// A Keepout's Layer, as the template names it; `None` for an inner Layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<TemplateCopperLayer>,
    /// The drawings' Layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drawing_layer: Option<DrawingLayer>,
    pub locked: bool,
    /// Ticked by default.
    pub default: bool,
    /// Why it can't be ticked at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
    /// What saving it leaves behind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl Candidate {
    fn new(object: Uuid, kind: CandidateKind, locked: bool, default: bool) -> Candidate {
        Candidate {
            object,
            kind,
            designator: None,
            footprint: None,
            side: None,
            layer: None,
            drawing_layer: None,
            locked,
            default,
            unavailable: None,
            warning: None,
        }
    }
}

/// What Save Board as Template lists for the Project's board, grouped as the dialog
/// shows them: the outline (always captured), cutouts, Keepouts, Components, Mechanical
/// Parts, Free Pads, then the Mechanical and Documentation drawings, each kind in board
/// order. Empty with no board.
pub fn candidates(project: &Project) -> Vec<Candidate> {
    let Some(pcb) = project.pcb() else {
        return vec![];
    };
    let mut out = vec![];
    if let Some(outline) = pcb.outline() {
        out.push(Candidate::new(
            outline.uuid,
            CandidateKind::Outline,
            outline.locked,
            true,
        ));
    }
    for cutout in pcb.cutouts() {
        out.push(Candidate::new(
            cutout.uuid,
            CandidateKind::Cutout,
            cutout.locked,
            true,
        ));
    }
    for keepout in pcb.keepouts() {
        let layer = keepout_layer(pcb, keepout);
        let mut c = Candidate::new(
            keepout.uuid,
            CandidateKind::Keepout,
            keepout.locked,
            layer.is_some(),
        );
        c.layer = layer;
        if layer.is_none() {
            c.unavailable = Some(INNER_KEEPOUT.into());
        }
        out.push(c);
    }
    let mut placed: Vec<&FootprintInstance> = pcb.footprints().collect();
    placed.sort_by(|a, b| {
        (a.component.is_none(), &a.designator, a.uuid).cmp(&(
            b.component.is_none(),
            &b.designator,
            b.uuid,
        ))
    });
    for instance in placed {
        let kind = if instance.component.is_some() {
            CandidateKind::Component
        } else {
            CandidateKind::MechanicalPart
        };
        let mut c = Candidate::new(instance.uuid, kind, instance.locked, instance.locked);
        c.designator = Some(instance.designator.clone());
        c.footprint = pcb
            .footprint_copy(instance.footprint)
            .map(|f| f.footprint.name.clone());
        c.side = Some(instance.side);
        c.warning = local_edit_warning(pcb, instance);
        if kind == CandidateKind::Component && component_entry(project, instance).is_none() {
            c.default = false;
            c.unavailable = Some(format!(
                "{} has no Unit on a Schematic Sheet",
                instance.designator
            ));
        }
        out.push(c);
    }
    for pad in pcb.free_pads() {
        out.push(Candidate::new(
            pad.uuid,
            CandidateKind::FreePad,
            pad.locked,
            pad.locked,
        ));
    }
    for (layer, kind) in drawing_layers(pcb) {
        if drawings_on(pcb, layer).next().is_some() {
            let mut c = Candidate::new(layer, CandidateKind::Drawings, false, true);
            c.drawing_layer = Some(kind);
            out.push(c);
        }
    }
    out
}

/// The board's Mechanical and Documentation Layers, top to bottom.
fn drawing_layers(pcb: &PcbDocument) -> Vec<(Uuid, DrawingLayer)> {
    pcb.layers()
        .layers
        .iter()
        .filter_map(|l| match l.kind {
            LayerKind::Mechanical => Some((l.uuid, DrawingLayer::Mechanical)),
            LayerKind::Documentation => Some((l.uuid, DrawingLayer::Documentation)),
            _ => None,
        })
        .collect()
}

/// One graphic, text or Dimension.
enum Drawn<'a> {
    Graphic(&'a reda_model::PcbGraphic),
    Text(&'a reda_model::PcbText),
    Dimension(&'a Dimension),
}

/// The drawings on `layer`, graphics then texts then Dimensions, each by UUID.
fn drawings_on(pcb: &PcbDocument, layer: Uuid) -> impl Iterator<Item = Drawn<'_>> {
    let graphics = pcb
        .graphics()
        .filter(move |g| g.layer == layer)
        .map(Drawn::Graphic);
    let texts = pcb
        .texts()
        .filter(move |t| t.layer == layer)
        .map(Drawn::Text);
    let dimensions = pcb
        .dimensions()
        .filter(move |d| d.layer == layer)
        .map(Drawn::Dimension);
    graphics.chain(texts).chain(dimensions)
}

/// A Keepout's Layer as a template names it; `None` for an inner copper Layer.
fn keepout_layer(pcb: &PcbDocument, keepout: &Keepout) -> Option<TemplateCopperLayer> {
    let Some(layer) = keepout.layer else {
        return Some(TemplateCopperLayer::AllCopper);
    };
    let layers = pcb.layers();
    let copper = |side| layers.find(LayerKind::Copper, Some(side)).map(|l| l.uuid);
    if copper(Side::Top) == Some(layer) {
        Some(TemplateCopperLayer::TopCopper)
    } else if copper(Side::Bottom) == Some(layer) {
        Some(TemplateCopperLayer::BottomCopper)
    } else {
        None
    }
}

/// "J1 has local Pad edits that won't be saved", for a Locally Edited Footprint.
fn local_edit_warning(pcb: &PcbDocument, instance: &FootprintInstance) -> Option<String> {
    let edited = pcb.footprint_copy(instance.footprint)?;
    let from = edited.edited_from.as_ref()?;
    let original = pcb.footprint_copy(from.uuid)?;
    let edits = local_edits(&original.footprint, &edited.footprint);
    let what = if edits.iter().any(|e| e.pad.is_some()) {
        "Pad"
    } else {
        "Footprint"
    };
    Some(format!(
        "{} has local {what} edits that won't be saved",
        instance.designator
    ))
}

/// The part-table entry a Component's Units were placed from, and its sheet.
fn component_entry<'p>(
    project: &'p Project,
    instance: &FootprintInstance,
) -> Option<(&'p SchematicSheet, &'p PartEntry)> {
    let component = instance.component.as_ref()?;
    project.documents().find_map(|d| {
        let sheet = project.schematic_sheet(d.uuid)?;
        let unit = sheet.units().find(|u| &u.component == component)?;
        Some((sheet, sheet.part(unit.part)?))
    })
}

/// A captured template and what it leaves behind.
pub(crate) struct Captured {
    pub template: BoardTemplate,
    pub warnings: Vec<String>,
}

/// The Project's board as a Board Template named by `p`, with element UUID `element` and
/// row UUIDs from `ids`.
pub(crate) fn capture(
    project: &Project,
    index: &LibraryIndex,
    p: &SaveParams,
    element: Uuid,
    ids: &mut dyn FnMut() -> Uuid,
) -> Result<Captured, CommandError> {
    let pcb = project
        .pcb()
        .ok_or_else(|| CommandError::unavailable("the Project has no PCB Document"))?;
    let outline = pcb.outline().ok_or_else(|| {
        CommandError::rejected("the board has no outline; draw the Board Outline first")
    })?;
    let listed = candidates(project);
    let chosen: Vec<&Candidate> = match &p.objects {
        None => listed
            .iter()
            .filter(|c| c.default && c.kind != CandidateKind::Outline)
            .collect(),
        Some(objects) => {
            let mut chosen = vec![];
            for (i, object) in objects.iter().enumerate() {
                let pointer = format!("/objects/{i}");
                let c = listed.iter().find(|c| c.object == *object).ok_or_else(|| {
                    CommandError::invalid_params(
                        &pointer,
                        format!("{object} is not an object Save Board as Template can capture"),
                    )
                })?;
                if let Some(why) = &c.unavailable {
                    return Err(CommandError::invalid_params(&pointer, why.clone()));
                }
                if c.kind != CandidateKind::Outline && !chosen.contains(&c) {
                    chosen.push(c);
                }
            }
            chosen
        }
    };
    let origin = contour_bounds(&outline.vertices)
        .map(|b| b.min)
        .unwrap_or(Point::new(Nm(0), Nm(0)));
    let mut c = Capture {
        origin,
        ids,
        template: BoardTemplate::new(element, p.name.clone()),
        follows: BTreeMap::new(),
        parts: BTreeMap::new(),
        copies: BTreeMap::new(),
        warnings: vec![],
    };
    c.template.description = p.description.clone().unwrap_or_default();
    c.template.category = p.category.clone().filter(|s| !s.trim().is_empty());
    c.template.suggested_layers = u8::try_from(pcb.copper_layers().count())
        .unwrap_or(32)
        .clamp(2, 32);
    c.template.board = pcb.setup().edge_connector.as_ref().map(|e| TemplateBoard {
        edge_connector: Some(TemplateEdgeConnector {
            bevel: e.bevel.map(|b| TemplateBevel {
                angle: TemplateNumber::Int(b.angle.0),
                depth: TemplateNumber::Int(b.depth.0),
            }),
            hard_gold: e.hard_gold,
        }),
    });

    let row = c.contour(
        outline.uuid,
        &outline.vertices,
        &outline.plated,
        outline.locked,
    );
    c.template.outline.push(row);
    let of_kind = |kind| chosen.iter().filter(move |x| x.kind == kind);
    for cand in of_kind(CandidateKind::Cutout) {
        let cutout = pcb.cutout(cand.object).expect("a listed cutout");
        let row = c.contour(cutout.uuid, &cutout.vertices, &cutout.plated, cutout.locked);
        c.template.cutouts.push(row);
    }
    for cand in of_kind(CandidateKind::Keepout) {
        let keepout = pcb.keepout(cand.object).expect("a listed Keepout");
        let row = c.keepout(keepout, cand.layer.expect("a ticked Keepout has a layer"));
        c.template.keepouts.push(row);
    }
    for cand in of_kind(CandidateKind::Component) {
        let instance = pcb.footprint(cand.object).expect("a listed Component");
        let (sheet, entry) =
            component_entry(project, instance).expect("a ticked Component has one");
        let placement = c.connector(pcb, sheet, instance, entry)?;
        c.template.connectors.push(placement);
        c.warnings.extend(cand.warning.clone());
    }
    for cand in of_kind(CandidateKind::MechanicalPart) {
        let instance = pcb
            .footprint(cand.object)
            .expect("a listed Mechanical Part");
        let hole = c.mechanical(pcb, index, instance)?;
        c.template.holes.push(hole);
        c.warnings.extend(cand.warning.clone());
    }
    for cand in of_kind(CandidateKind::FreePad) {
        let pad = pcb.free_pad(cand.object).expect("a listed Free Pad");
        let hole = c.free_pad(pad);
        c.template.holes.push(hole);
    }
    for cand in of_kind(CandidateKind::Drawings) {
        let layer = cand
            .drawing_layer
            .expect("a drawings candidate has a Layer");
        let group = (c.ids)();
        c.template.drawing_groups.push(TemplateDrawingGroup {
            uuid: group,
            name: match layer {
                DrawingLayer::Mechanical => "Mechanical drawings".into(),
                DrawingLayer::Documentation => "Documentation drawings".into(),
            },
            layer,
            when: None,
        });
        for drawn in drawings_on(pcb, cand.object) {
            let drawing = c.drawing(group, drawn);
            c.template.drawings.push(drawing);
        }
    }
    Ok(Captured {
        template: c.template,
        warnings: c.warnings,
    })
}

/// The template being built.
struct Capture<'i> {
    /// The board point that becomes (0,0).
    origin: Point,
    ids: &'i mut dyn FnMut() -> Uuid,
    template: BoardTemplate,
    /// Captured board objects by UUID: their template row UUID, for Dimension ends.
    follows: BTreeMap<Uuid, Uuid>,
    /// Part-table entries by (Part UUID, hash).
    parts: BTreeMap<(Uuid, String), Uuid>,
    /// Footprint copies by (Footprint UUID, hash).
    copies: BTreeMap<(Uuid, String), Uuid>,
    warnings: Vec<String>,
}

fn int(n: Nm) -> TemplateNumber {
    TemplateNumber::Int(n.0)
}

fn angle(a: Millideg) -> TemplateNumber {
    TemplateNumber::Int(a.0)
}

impl Capture<'_> {
    fn new_row(&mut self, board: Uuid) -> Uuid {
        let uuid = (self.ids)();
        self.follows.insert(board, uuid);
        uuid
    }

    fn point(&self, p: Point) -> TemplatePoint {
        TemplatePoint {
            x: int(p.x - self.origin.x),
            y: int(p.y - self.origin.y),
        }
    }

    fn vertices(&self, vertices: &[Vertex], plated: Option<&PlatedEdges>) -> Vec<TemplateVertex> {
        vertices
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let at = self.point(v.point());
                TemplateVertex {
                    x: at.x,
                    y: at.y,
                    sweep: (v.sweep != Millideg::ZERO).then(|| angle(v.sweep)),
                    plated: plated.is_some_and(|p| p.contains_key(&i)),
                }
            })
            .collect()
    }

    fn contour(
        &mut self,
        board: Uuid,
        vertices: &[Vertex],
        plated: &PlatedEdges,
        locked: bool,
    ) -> TemplateContour {
        TemplateContour {
            uuid: self.new_row(board),
            vertices: self.vertices(vertices, Some(plated)),
            locked,
            when: None,
        }
    }

    fn keepout(&mut self, keepout: &Keepout, layer: TemplateCopperLayer) -> TemplateKeepout {
        let (shape, width) = match &keepout.shape {
            KeepoutShape::Polygon { .. } => (KeepoutShapeKind::Polygon, None),
            KeepoutShape::Line { width, .. } => (KeepoutShapeKind::Line, Some(int(*width))),
        };
        TemplateKeepout {
            uuid: self.new_row(keepout.uuid),
            layer,
            restrict: keepout.restrict.clone(),
            shape,
            vertices: self.vertices(keepout.vertices(), None),
            width,
            protects: None,
            locked: keepout.locked,
            when: None,
        }
    }

    /// The template's copy of `copy`, added once per Footprint and hash.
    fn copy(&mut self, copy: &FootprintCopy) -> Uuid {
        let key = (copy.footprint.uuid, copy.hash.clone());
        if let Some(uuid) = self.copies.get(&key) {
            return *uuid;
        }
        let uuid = (self.ids)();
        self.template.footprint_copies.push(FootprintCopy {
            uuid,
            hash: copy.hash.clone(),
            edited_from: None,
            footprint: copy.footprint.clone(),
        });
        self.copies.insert(key, uuid);
        uuid
    }

    /// The template's part entry for this Part and hash, made with `copies` (one per
    /// alternate) the first time.
    fn part(
        &mut self,
        part: &reda_model::Part,
        hash: &str,
        symbol: Option<&reda_model::Symbol>,
        copies: &[FootprintCopy],
    ) -> Uuid {
        let key = (part.uuid, hash.to_owned());
        if let Some(uuid) = self.parts.get(&key) {
            return *uuid;
        }
        let footprints = copies.iter().map(|c| self.copy(c)).collect();
        let uuid = (self.ids)();
        self.template.parts.push(TemplatePart {
            uuid,
            hash: hash.to_owned(),
            part: part.clone(),
            symbol: symbol.cloned(),
            footprints,
        });
        self.parts.insert(key, uuid);
        uuid
    }

    /// The PCB copy `instance` is placed from, before any local edit.
    fn unedited<'p>(pcb: &'p PcbDocument, instance: &FootprintInstance) -> &'p FootprintCopy {
        let copy = pcb
            .footprint_copy(instance.footprint)
            .expect("a placed Footprint has its copy");
        copy.edited_from
            .as_ref()
            .and_then(|from| pcb.footprint_copy(from.uuid))
            .unwrap_or(copy)
    }

    fn connector(
        &mut self,
        pcb: &PcbDocument,
        sheet: &SchematicSheet,
        instance: &FootprintInstance,
        entry: &PartEntry,
    ) -> Result<TemplatePlacement, CommandError> {
        let sheet_copies: Vec<FootprintCopy> = entry
            .footprints
            .iter()
            .filter_map(|uuid| sheet.footprint(*uuid).cloned())
            .collect();
        if sheet_copies.len() != entry.part.footprints.len() {
            return Err(CommandError::rejected(format!(
                "{}'s part table entry is missing a Footprint copy",
                instance.designator
            )));
        }
        let used = Self::unedited(pcb, instance);
        let chosen = sheet_copies
            .iter()
            .position(|c| c.footprint.uuid == used.footprint.uuid)
            .ok_or_else(|| {
                CommandError::rejected(format!(
                    "{}'s Footprint {} is not one of its Part's; run Sync first",
                    instance.designator, used.footprint.name
                ))
            })?;
        let part = self.part(&entry.part, &entry.hash, Some(&entry.symbol), &sheet_copies);
        let footprint = self
            .template
            .parts
            .iter()
            .find(|e| e.uuid == part)
            .expect("just added")
            .footprints[chosen];
        Ok(TemplatePlacement {
            uuid: self.new_row(instance.uuid),
            part,
            footprint,
            at: self.point(instance.at),
            rotation: angle(instance.rotation),
            side: instance.side,
            locked: instance.locked,
            when: None,
        })
    }

    fn mechanical(
        &mut self,
        pcb: &PcbDocument,
        index: &LibraryIndex,
        instance: &FootprintInstance,
    ) -> Result<TemplateHole, CommandError> {
        let used = Self::unedited(pcb, instance).clone();
        let mut found: Vec<&reda_model::Part> = index
            .parts()
            .map(|(_, p)| p)
            .filter(|p| {
                p.component_type == ComponentType::Mechanical
                    && p.footprints.contains(&used.footprint.uuid)
            })
            .collect();
        found.sort_by(|a, b| {
            let named = |p: &reda_model::Part| p.fields.value != instance.fields.value;
            (named(a), &a.name, a.uuid).cmp(&(named(b), &b.name, b.uuid))
        });
        let part = found.first().ok_or_else(|| {
            CommandError::rejected(format!(
                "{} is a Mechanical Part no registered Library holds, so the template \
                 can't name its Part; leave it out",
                instance.designator
            ))
        })?;
        let library = copy_part(index, part.uuid)?;
        let copies: Vec<FootprintCopy> = library
            .footprints
            .iter()
            .map(|f| {
                if f.footprint.uuid == used.footprint.uuid {
                    used.clone()
                } else {
                    FootprintCopy {
                        uuid: used.uuid,
                        hash: f.hash.clone(),
                        edited_from: None,
                        footprint: f.footprint.clone(),
                    }
                }
            })
            .collect();
        let chosen = library
            .part
            .footprints
            .iter()
            .position(|f| *f == used.footprint.uuid)
            .expect("found by this Footprint");
        let entry = self.part(
            &library.part,
            &library.hash,
            library.symbol.as_ref(),
            &copies,
        );
        let footprint = self
            .template
            .parts
            .iter()
            .find(|e| e.uuid == entry)
            .expect("just added")
            .footprints[chosen];
        Ok(TemplateHole {
            uuid: self.new_row(instance.uuid),
            part: Some(entry),
            footprint: Some(footprint),
            side: Some(instance.side),
            pad: None,
            at: self.point(instance.at),
            rotation: angle(instance.rotation),
            locked: instance.locked,
            when: None,
        })
    }

    fn free_pad(&mut self, pad: &FreePad) -> TemplateHole {
        let template_pad = TemplatePad {
            kind: pad.kind,
            castellated: pad.castellated,
            shape: pad.shape,
            width: int(pad.width),
            height: int(pad.height),
            corner_ratio: pad.corner_ratio,
            chamfer_ratio: pad.chamfer_ratio,
            corners: pad.corners,
            offset: pad.offset.map(|o| TemplatePoint {
                x: int(o.x),
                y: int(o.y),
            }),
            hole: pad.hole.as_ref().map(|h| TemplateDrill {
                width: int(h.width),
                height: int(h.height),
                rotation: (h.rotation != Millideg::ZERO).then(|| angle(h.rotation)),
                tolerance: h.tolerance,
            }),
            side: pad.side,
            middle: pad.middle,
            bottom: pad.bottom,
            unused_layers: pad.unused_layers,
            mask: pad.mask,
            paste: pad.paste.clone(),
            zone_connection: pad.zone_connection,
            teardrop: pad.teardrop,
        };
        TemplateHole {
            uuid: self.new_row(pad.uuid),
            part: None,
            footprint: None,
            side: None,
            pad: Some(template_pad),
            at: self.point(pad.at),
            rotation: angle(pad.rotation),
            locked: pad.locked,
            when: None,
        }
    }

    fn drawing(&mut self, group: Uuid, drawn: Drawn<'_>) -> TemplateDrawing {
        let mut d = TemplateDrawing {
            uuid: (self.ids)(),
            group,
            kind: DrawingKind::Line,
            width: None,
            from: None,
            to: None,
            sweep: None,
            filled: false,
            text: None,
            position: None,
            height: None,
            rotation: None,
            a: None,
            b: None,
            offset: None,
            prefix: None,
            suffix: None,
            locked: false,
            when: None,
        };
        match drawn {
            Drawn::Graphic(g) => {
                d.locked = g.locked;
                d.width = Some(int(g.width));
                match g.shape {
                    GraphicShape::Line { from, to } => {
                        d.from = Some(self.point(from));
                        d.to = Some(self.point(to));
                    }
                    GraphicShape::Arc { from, to, sweep } => {
                        d.kind = DrawingKind::Arc;
                        d.from = Some(self.point(from));
                        d.to = Some(self.point(to));
                        d.sweep = Some(angle(sweep));
                    }
                    GraphicShape::Rect { from, to, filled } => {
                        d.kind = DrawingKind::Rectangle;
                        d.from = Some(self.point(from));
                        d.to = Some(self.point(to));
                        d.filled = filled;
                    }
                }
            }
            Drawn::Text(t) => {
                d.kind = DrawingKind::Text;
                d.locked = t.locked;
                d.text = Some(t.text.clone());
                d.position = Some(self.point(t.position));
                d.height = Some(int(t.style.height));
                d.rotation = (t.rotation != Millideg::ZERO).then(|| angle(t.rotation));
            }
            Drawn::Dimension(dim) => {
                d.kind = DrawingKind::Dimension;
                d.locked = dim.locked;
                d.a = Some(self.end(&dim.a));
                d.b = Some(self.end(&dim.b));
                d.rotation = Some(angle(dim.rotation));
                d.offset = Some(int(dim.offset));
                d.prefix = Some(dim.prefix.clone()).filter(|s| !s.is_empty());
                d.suffix = Some(dim.suffix.clone()).filter(|s| !s.is_empty());
            }
        }
        d
    }

    /// A Dimension end: following the captured object it follows, else free.
    fn end(&self, end: &DimensionEnd) -> TemplateDimensionEnd {
        let follow = end.reference.and_then(|r| {
            let object = *self.follows.get(&r.object)?;
            match r.hotspot {
                Hotspot::Vertex(i) => Some(TemplateFollow {
                    object,
                    vertex: Some(i),
                }),
                Hotspot::Origin | Hotspot::Center => Some(TemplateFollow {
                    object,
                    vertex: None,
                }),
                _ => None,
            }
        });
        match follow {
            Some(follow) => TemplateDimensionEnd {
                x: None,
                y: None,
                follow: Some(follow),
            },
            None => {
                let at = self.point(end.point);
                TemplateDimensionEnd {
                    x: Some(at.x),
                    y: Some(at.y),
                    follow: None,
                }
            }
        }
    }
}
