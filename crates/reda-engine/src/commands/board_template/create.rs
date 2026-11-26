//! Creating a board from a Board Template (`docs/spec/v0.2.5/creation.md`): the frame the
//! preview shows, written as plain board objects with fresh UUIDs, in the edit that
//! creates the PCB Document.
//!
//! `project.new` and `document.add {kind: pcb}` both call [`frame_for`] to evaluate and
//! validate, then [`write`] inside their own transaction once the PCB Document exists.

use std::collections::BTreeMap;

use reda_command::{CommandError, Context};
use reda_library::board_template::{
    BoardTemplateChoice, Frame, FrameContour, FrameDimensionEnd, FrameDrawing, FrameHole,
    FramePlacement, ResolvedValue, evaluate,
};
use reda_model::geom::{Millideg, Point};
use reda_model::{
    BoardCutout, BoardOutline, BoardTemplate, ComponentRef, Dimension, DimensionEnd, DrawingKind,
    EdgeBevel, EdgeConnector, FootprintCopy, FootprintInstance, GraphicShape, Hotspot, HotspotRef,
    InDocument, Keepout, KeepoutShape, KeepoutShapeKind, LayerKind, Object, PadNet, PartEntry,
    PcbGraphic, PcbText, PlatedEdge, PlatedEdges, Project, Side, TemplateCopperLayer, TemplatePart,
    TextJustify, TextStyle, Transaction, UnitInstance, Uuid,
};
use schemars::JsonSchema;
use serde::Serialize;

/// Where the template's objects go: the edit's Project as it stands, a source of fresh
/// UUIDs and the transaction to write into.
pub(crate) trait Writer {
    fn project(&self) -> &Project;
    fn new_id(&mut self) -> Uuid;
    fn insert(&mut self, object: Object) -> Result<(), CommandError>;
    fn replace(&mut self, object: Object) -> Result<(), CommandError>;
}

/// An edit Command writing into its own transaction.
impl Writer for Context<'_> {
    fn project(&self) -> &Project {
        Context::project(self).expect("an edit has a Project")
    }

    fn new_id(&mut self) -> Uuid {
        Context::new_id(self)
    }

    fn insert(&mut self, object: Object) -> Result<(), CommandError> {
        Ok(self.tx()?.insert(object)?)
    }

    fn replace(&mut self, object: Object) -> Result<(), CommandError> {
        Ok(self.tx()?.replace(object)?)
    }
}

/// A Project being built in a transaction of its own, with the Command's UUIDs.
pub(crate) struct Building<'t, 'p, 'c> {
    pub tx: &'t mut Transaction<'p>,
    pub cx: &'t mut Context<'c>,
}

impl Writer for Building<'_, '_, '_> {
    fn project(&self) -> &Project {
        self.tx.project()
    }

    fn new_id(&mut self) -> Uuid {
        self.cx.new_id()
    }

    fn insert(&mut self, object: Object) -> Result<(), CommandError> {
        Ok(self.tx.insert(object)?)
    }

    fn replace(&mut self, object: Object) -> Result<(), CommandError> {
        Ok(self.tx.replace(object)?)
    }
}

/// The JSON pointer of the `board_template` param.
const PARAM: &str = "/board_template";

/// `template` evaluated at `choice` through the preview's function, and validated. Every
/// failure is `invalid_params` under `/board_template`, carrying the preview's `problems`
/// (empty when the values or `exclude` themselves are bad).
pub(crate) fn frame_for(
    template: &BoardTemplate,
    choice: &BoardTemplateChoice,
) -> Result<Frame, CommandError> {
    let frame = evaluate(template, choice).map_err(|e| {
        refusal(
            &format!("{PARAM}{}", e.pointer),
            e.message,
            serde_json::Value::Array(vec![]),
        )
    })?;
    if !frame.is_valid() {
        let messages: Vec<&str> = frame.problems.iter().map(|p| p.message.as_str()).collect();
        return Err(refusal(
            PARAM,
            format!(
                "{} is invalid at these values: {}",
                frame.name,
                messages.join("; ")
            ),
            serde_json::to_value(&frame.problems).expect("problems serialise"),
        ));
    }
    Ok(frame)
}

/// An `invalid_params` at `pointer` that also carries `problems`.
fn refusal(pointer: &str, message: String, problems: serde_json::Value) -> CommandError {
    let mut err = CommandError::invalid_params(pointer, message);
    err.details = Some(serde_json::json!({"pointer": pointer, "problems": problems}));
    err
}

/// What `seeded.template` reports: which template at which values, and what each of its
/// objects became.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SeededTemplate {
    /// The template's Library element UUID.
    pub element: Uuid,
    pub name: String,
    /// Every parameter's value, defaults filled in.
    pub values: BTreeMap<String, ResolvedValue>,
    /// One entry per template object created, in creation order. Excluded and gated-out
    /// objects are absent.
    pub objects: Vec<SeededObject>,
}

/// One template object and what creating the board made of it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SeededObject {
    /// The template-local UUID.
    pub template_object: Uuid,
    /// The board object it became; absent for a drawing group, which makes none of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<Uuid>,
    /// A connector's or Mechanical Part hole's Designator.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub designator: Option<String>,
    /// A connector's Component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<Uuid>,
}

impl SeededObject {
    fn plain(template_object: Uuid, created: Uuid) -> SeededObject {
        SeededObject {
            template_object,
            created: Some(created),
            designator: None,
            component: None,
        }
    }
}

/// Writes `frame` (of `template`) onto the new PCB Document `pcb`, its connectors' Symbols
/// on the Project's first Schematic Sheet, skipping excluded objects. Every object gets a
/// fresh UUID. `pcb` must already hold its Layers and Board Setup, and no outline.
pub(crate) fn write(
    w: &mut dyn Writer,
    pcb: Uuid,
    template: &BoardTemplate,
    frame: &Frame,
) -> Result<SeededTemplate, CommandError> {
    let mut created = Created::default();
    let mut objects = vec![];

    let outline = contour(w, &frame.outline);
    created.contour(&frame.outline, outline.uuid);
    objects.push(SeededObject::plain(frame.outline.uuid, outline.uuid));
    w.insert(Object::BoardOutline(InDocument::new(pcb, outline)))?;
    for row in &frame.cutouts {
        let BoardOutline {
            uuid,
            locked,
            vertices,
            plated,
        } = contour(w, row);
        created.contour(row, uuid);
        objects.push(SeededObject::plain(row.uuid, uuid));
        let cutout = BoardCutout {
            uuid,
            locked,
            vertices,
            plated,
        };
        w.insert(Object::BoardCutout(InDocument::new(pcb, cutout)))?;
    }

    let layers = w.project().pcb_document(pcb).expect("the new PCB").layers();
    let copper = |side| layers.find(LayerKind::Copper, Some(side)).map(|l| l.uuid);
    let (top, bottom) = (copper(Side::Top), copper(Side::Bottom));
    // A drawing group goes on the first Mechanical Layer with its Purpose.
    let mechanical: Vec<_> = layers
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Mechanical)
        .map(|l| (l.purpose, l.uuid))
        .collect();
    let drawing_layer = |purpose| {
        mechanical
            .iter()
            .find(|(p, _)| *p == purpose)
            .map(|(_, uuid)| *uuid)
    };

    for row in frame.keepouts.iter().filter(|k| !k.excluded) {
        let uuid = w.new_id();
        let layer = match row.layer {
            TemplateCopperLayer::AllCopper => None,
            TemplateCopperLayer::TopCopper => top,
            TemplateCopperLayer::BottomCopper => bottom,
        };
        let vertices = row.vertices.clone();
        let shape = match row.shape {
            KeepoutShapeKind::Polygon => KeepoutShape::Polygon { vertices },
            KeepoutShapeKind::Line => KeepoutShape::Line {
                vertices,
                width: row.width.unwrap_or_default(),
            },
        };
        let keepout = Keepout {
            uuid,
            locked: row.locked,
            layer,
            shape,
            restrict: row.restrict.clone(),
        };
        created.vertices(row.uuid, uuid, row.vertices.iter().map(|v| v.point()));
        objects.push(SeededObject::plain(row.uuid, uuid));
        w.insert(Object::Keepout(InDocument::new(pcb, keepout)))?;
    }

    let connectors: Vec<&FramePlacement> =
        frame.connectors.iter().filter(|c| !c.excluded).collect();
    if !connectors.is_empty() {
        for placed in connectors::write(w, pcb, template, &connectors)? {
            created.origin(placed.template_object, placed.footprint);
            objects.push(SeededObject {
                template_object: placed.template_object,
                created: Some(placed.footprint),
                designator: Some(placed.designator),
                component: Some(placed.component),
            });
        }
    }

    for hole in frame.holes.iter().filter(|h| !h.excluded) {
        objects.push(write_hole(w, pcb, template, hole, &mut created)?);
    }

    for group in frame.drawing_groups.iter().filter(|g| !g.excluded) {
        objects.push(SeededObject {
            template_object: group.uuid,
            created: None,
            designator: None,
            component: None,
        });
        let layer = drawing_layer(group.purpose).ok_or_else(|| {
            CommandError::internal(format!(
                "the new board has no {} Layer",
                group.purpose.title()
            ))
        })?;
        for drawing in group.drawings.iter().filter(|d| !d.excluded) {
            let uuid = w.new_id();
            objects.push(SeededObject::plain(drawing.uuid, uuid));
            let object = drawn(pcb, layer, uuid, drawing, &created)?;
            w.insert(object)?;
        }
    }

    if let Some(edge) = frame.board.as_ref().and_then(|b| b.edge_connector.as_ref()) {
        let mut setup = w
            .project()
            .pcb_document(pcb)
            .expect("the new PCB")
            .setup()
            .clone();
        setup.edge_connector = Some(EdgeConnector {
            bevel: edge.bevel.map(|b| EdgeBevel {
                angle: b.angle,
                depth: b.depth,
            }),
            hard_gold: edge.hard_gold,
        });
        w.replace(Object::BoardSetup(InDocument::new(pcb, setup)))?;
    }

    Ok(SeededTemplate {
        element: frame.element,
        name: frame.name.clone(),
        values: frame.values.clone(),
        objects,
    })
}

/// A frame contour as a Board Outline (a cutout takes its fields), with its Plated Edges
/// on no Net.
fn contour(w: &mut dyn Writer, row: &FrameContour) -> BoardOutline {
    let mut outline = BoardOutline::new(w.new_id(), row.contour());
    outline.locked = row.locked;
    outline.plated = row
        .vertices
        .iter()
        .enumerate()
        .filter(|(_, v)| v.plated)
        .map(|(i, _)| (i, PlatedEdge { net: None }))
        .collect::<PlatedEdges>();
    outline
}

fn write_hole(
    w: &mut dyn Writer,
    pcb: Uuid,
    template: &BoardTemplate,
    hole: &FrameHole,
    created: &mut Created,
) -> Result<SeededObject, CommandError> {
    if let Some(pad) = &hole.free_pad {
        let mut pad = pad.clone();
        pad.uuid = w.new_id();
        pad.locked = hole.locked;
        pad.net = None;
        let uuid = pad.uuid;
        created
            .follows
            .insert(hole.uuid, Target::Fixed(uuid, Hotspot::Center));
        w.insert(Object::FreePad(InDocument::new(pcb, pad)))?;
        return Ok(SeededObject::plain(hole.uuid, uuid));
    }
    let (Some(part), Some(footprint)) = (hole.part, hole.footprint) else {
        return Err(CommandError::internal(format!(
            "hole {} is neither a Free Pad nor a Mechanical Part",
            hole.uuid
        )));
    };
    let entry = part_entry(template, part)?;
    let copy = template_copy(template, footprint)?;
    let copy = ensure_pcb_copy(w, pcb, copy)?;
    let designator =
        reda_pcb::mechanical_designator(w.project(), &entry.part, entry.symbol.as_ref());
    let uuid = w.new_id();
    let instance = FootprintInstance {
        uuid,
        component: None,
        footprint: copy.uuid,
        at: hole.at,
        rotation: hole.rotation.normalized(),
        side: hole.side.unwrap_or(Side::Top),
        locked: hole.locked,
        graphics_unlocked: false,
        designator: designator.clone(),
        fields: entry.part.fields.clone(),
        component_type: entry.part.component_type,
        in_bom: entry.part.in_bom,
        in_cpl: entry.part.in_cpl,
        not_fitted: false,
        text_overrides: Vec::new(),
        pads: blank_pads(&copy),
    };
    created.origin(hole.uuid, uuid);
    w.insert(Object::FootprintInstance(InDocument::new(pcb, instance)))?;
    Ok(SeededObject {
        template_object: hole.uuid,
        created: Some(uuid),
        designator: Some(designator),
        component: None,
    })
}

/// The template's part-table entry `uuid`.
fn part_entry(template: &BoardTemplate, uuid: Uuid) -> Result<&TemplatePart, CommandError> {
    template
        .parts
        .iter()
        .find(|p| p.uuid == uuid)
        .ok_or_else(|| CommandError::internal(format!("the template has no part entry {uuid}")))
}

/// The template's Footprint copy `uuid`.
fn template_copy(template: &BoardTemplate, uuid: Uuid) -> Result<&FootprintCopy, CommandError> {
    template
        .footprint_copies
        .iter()
        .find(|c| c.uuid == uuid)
        .ok_or_else(|| CommandError::internal(format!("the template has no Footprint copy {uuid}")))
}

/// The PCB's copy of the Footprint in `from`, inserted when the PCB has none with its
/// content hash.
fn ensure_pcb_copy(
    w: &mut dyn Writer,
    pcb: Uuid,
    from: &FootprintCopy,
) -> Result<FootprintCopy, CommandError> {
    let existing = w
        .project()
        .pcb_document(pcb)
        .and_then(|p| p.footprint_copy_for(from.footprint.uuid, &from.hash))
        .cloned();
    if let Some(copy) = existing {
        return Ok(copy);
    }
    let copy = FootprintCopy {
        uuid: w.new_id(),
        hash: from.hash.clone(),
        edited_from: None,
        footprint: from.footprint.clone(),
    };
    w.insert(Object::FootprintCopy(InDocument::new(pcb, copy.clone())))?;
    Ok(copy)
}

/// One entry per Pad of the copy, sorted by Pad UUID, all on no Net, as Sync makes them.
fn blank_pads(copy: &FootprintCopy) -> Vec<PadNet> {
    let mut pads: Vec<PadNet> = copy
        .footprint
        .pads
        .iter()
        .map(|p| PadNet {
            pad: p.uuid,
            net: None,
        })
        .collect();
    pads.sort_by_key(|p| p.pad);
    pads
}

/// The drawing as a board object on `layer`.
fn drawn(
    pcb: Uuid,
    layer: Uuid,
    uuid: Uuid,
    d: &FrameDrawing,
    created: &Created,
) -> Result<Object, CommandError> {
    let missing =
        |field: &str| CommandError::internal(format!("drawing {} has no `{field}`", d.uuid));
    let width = d.width.unwrap_or(reda_pcb::DEFAULT_GRAPHIC_WIDTH);
    let graphic = |shape| {
        Object::PcbGraphic(InDocument::new(
            pcb,
            PcbGraphic {
                uuid,
                locked: d.locked,
                layer,
                width,
                shape,
            },
        ))
    };
    let from = || d.from.ok_or_else(|| missing("from"));
    let to = || d.to.ok_or_else(|| missing("to"));
    Ok(match d.kind {
        DrawingKind::Line => graphic(GraphicShape::Line {
            from: from()?,
            to: to()?,
        }),
        DrawingKind::Arc => graphic(GraphicShape::Arc {
            from: from()?,
            to: to()?,
            sweep: d.sweep.ok_or_else(|| missing("sweep"))?,
        }),
        DrawingKind::Rectangle => graphic(GraphicShape::Rect {
            from: from()?,
            to: to()?,
            filled: d.filled,
        }),
        DrawingKind::Text => {
            let height = d.height.unwrap_or(reda_pcb::DEFAULT_TEXT_HEIGHT);
            Object::PcbText(InDocument::new(
                pcb,
                PcbText {
                    uuid,
                    locked: d.locked,
                    layer,
                    text: d.text.clone().ok_or_else(|| missing("text"))?,
                    position: d.position.ok_or_else(|| missing("position"))?,
                    style: TextStyle {
                        justify: TextJustify::default(),
                        ..TextStyle::of_height(height)
                    },
                    frame: None,
                    rotation: d.rotation.unwrap_or(Millideg::ZERO).normalized(),
                    mirrored: false,
                },
            ))
        }
        DrawingKind::Dimension => {
            let end = |e: Option<FrameDimensionEnd>, field| {
                e.map(|e| created.end(e)).ok_or_else(|| missing(field))
            };
            let mut dim: Dimension =
                reda_pcb::new_dimension(uuid, layer, end(d.a, "a")?, end(d.b, "b")?);
            dim.locked = d.locked;
            if let Some(rotation) = d.rotation {
                dim.rotation = rotation.normalized();
            }
            if let Some(offset) = d.offset {
                dim.offset = offset;
            }
            dim.prefix = d.prefix.clone().unwrap_or_default();
            dim.suffix = d.suffix.clone().unwrap_or_default();
            Object::Dimension(InDocument::new(pcb, dim))
        }
    })
}

/// What a Dimension end that follows a template object follows on the board.
enum Target {
    /// One Hotspot of the new object, whatever the end's vertex.
    Fixed(Uuid, Hotspot),
    /// A vertex of the new contour or Keepout, found by the template vertex's point:
    /// the board drops zero-length edges, so indices shift.
    Vertices(Uuid, Vec<Point>),
}

/// The board objects made so far, by template object, for Dimension ends to follow.
#[derive(Default)]
struct Created {
    follows: BTreeMap<Uuid, Target>,
}

impl Created {
    fn contour(&mut self, row: &FrameContour, uuid: Uuid) {
        let points = row.vertices.iter().map(|v| Point::new(v.x, v.y));
        self.vertices(row.uuid, uuid, points);
    }

    fn vertices(&mut self, template: Uuid, uuid: Uuid, points: impl Iterator<Item = Point>) {
        self.follows
            .insert(template, Target::Vertices(uuid, points.collect()));
    }

    fn origin(&mut self, template: Uuid, footprint: Uuid) {
        self.follows
            .insert(template, Target::Fixed(footprint, Hotspot::Origin));
    }

    /// The end on the board: following the new object when the template end follows one
    /// that was created, else free at its evaluated point.
    fn end(&self, e: FrameDimensionEnd) -> DimensionEnd {
        let reference = e.follow.and_then(|f| match self.follows.get(&f.object)? {
            Target::Fixed(object, hotspot) => Some(HotspotRef {
                object: *object,
                hotspot: *hotspot,
            }),
            Target::Vertices(object, points) => {
                let i = points.iter().position(|p| *p == e.point)?;
                Some(HotspotRef {
                    object: *object,
                    hotspot: Hotspot::Vertex(i),
                })
            }
        });
        DimensionEnd {
            point: e.point,
            reference,
        }
    }
}

mod connectors {
    //! Connectors: linked Components, the Symbol on the first sheet and the Footprint on
    //! the board, as placing, syncing and moving them by hand would leave them.

    use super::*;

    pub(super) struct Placed {
        pub template_object: Uuid,
        pub footprint: Uuid,
        pub designator: String,
        pub component: Uuid,
    }

    pub(super) fn write(
        w: &mut dyn Writer,
        pcb: Uuid,
        template: &BoardTemplate,
        connectors: &[&FramePlacement],
    ) -> Result<Vec<Placed>, CommandError> {
        let sheet = w
            .project()
            .documents()
            .find(|d| w.project().schematic_sheet(d.uuid).is_some())
            .map(|d| d.uuid)
            .ok_or_else(|| {
                CommandError::rejected(
                    "the template's connectors need a Schematic Sheet for their Symbols; \
                     add one, or exclude the connectors",
                )
            })?;
        let mut entries = vec![];
        for c in connectors {
            let entry = part_entry(template, c.part)?;
            let symbol = entry.symbol.as_ref().ok_or_else(|| {
                CommandError::internal(format!(
                    "connector {} uses Part {}, which has no Symbol",
                    c.uuid, entry.part.name
                ))
            })?;
            let unit = symbol.units.first().map(|u| u.uuid).ok_or_else(|| {
                CommandError::internal(format!("Symbol {} has no Units", symbol.name))
            })?;
            entries.push((entry, symbol, unit));
        }
        let spots = {
            let body = w.project().schematic_sheet(sheet).expect("found above");
            let units: Vec<_> = entries.iter().map(|(_, s, u)| (*s, *u)).collect();
            reda_schematic::beside_content(body, &units)
        };

        let mut placed = vec![];
        for ((c, (entry, symbol, unit)), at) in connectors.iter().zip(entries).zip(spots) {
            let part_entry = ensure_sheet_entry(w, sheet, template, entry, symbol)?;
            let chosen = template_copy(template, c.footprint)?;
            let component = ComponentRef::new(w.new_id());
            let designator =
                reda_schematic::next_free_designator(w.project(), &symbol.designator_prefix);
            let fields = entry.part.fields.clone();
            let unit = UnitInstance {
                uuid: w.new_id(),
                component: component.clone(),
                part: part_entry,
                unit,
                at,
                rotation: Millideg::ZERO,
                mirror: false,
                designator: designator.clone(),
                designator_locked: false,
                fields: fields.clone(),
                footprint: Some(chosen.footprint.uuid),
                component_type: entry.part.component_type,
                in_bom: entry.part.in_bom,
                in_cpl: entry.part.in_cpl,
                not_fitted: false,
                visible_fields: UnitInstance::default_visible_fields(),
                field_text: Default::default(),
                locked_fields: Default::default(),
            };
            w.insert(Object::Unit(InDocument::new(sheet, unit)))?;

            let copy = ensure_pcb_copy(w, pcb, chosen)?;
            let uuid = w.new_id();
            let instance = FootprintInstance {
                uuid,
                component: Some(component.clone()),
                footprint: copy.uuid,
                at: c.at,
                rotation: c.rotation.normalized(),
                side: c.side,
                locked: c.locked,
                graphics_unlocked: false,
                designator: designator.clone(),
                fields,
                component_type: entry.part.component_type,
                in_bom: entry.part.in_bom,
                in_cpl: entry.part.in_cpl,
                not_fitted: false,
                text_overrides: Vec::new(),
                pads: blank_pads(&copy),
            };
            w.insert(Object::FootprintInstance(InDocument::new(pcb, instance)))?;
            placed.push(Placed {
                template_object: c.uuid,
                footprint: uuid,
                designator,
                component: component.uuid,
            });
        }
        Ok(placed)
    }

    /// The sheet's part-table entry for the template's copy, inserted (with any Footprint
    /// copy the sheet lacks) when the sheet has none with the same Part and hash.
    fn ensure_sheet_entry(
        w: &mut dyn Writer,
        sheet: Uuid,
        template: &BoardTemplate,
        entry: &TemplatePart,
        symbol: &reda_model::Symbol,
    ) -> Result<Uuid, CommandError> {
        let body = w.project().schematic_sheet(sheet).expect("a sheet");
        if let Some(existing) = body.part_entry_for(entry.part.uuid, &entry.hash) {
            return Ok(existing.uuid);
        }
        let mut copies = vec![];
        for &uuid in &entry.footprints {
            let from = template_copy(template, uuid)?;
            let existing = w
                .project()
                .schematic_sheet(sheet)
                .expect("a sheet")
                .footprint_copy_for(from.footprint.uuid, &from.hash)
                .map(|c| c.uuid);
            let copy = match existing {
                Some(uuid) => uuid,
                None => {
                    let copy = FootprintCopy {
                        uuid: w.new_id(),
                        hash: from.hash.clone(),
                        edited_from: None,
                        footprint: from.footprint.clone(),
                    };
                    let uuid = copy.uuid;
                    w.insert(Object::FootprintCopy(InDocument::new(sheet, copy)))?;
                    uuid
                }
            };
            copies.push(copy);
        }
        let new = PartEntry {
            uuid: w.new_id(),
            hash: entry.hash.clone(),
            part: entry.part.clone(),
            symbol: symbol.clone(),
            footprints: copies,
        };
        let uuid = new.uuid;
        w.insert(Object::PartEntry(InDocument::new(sheet, new)))?;
        Ok(uuid)
    }
}
