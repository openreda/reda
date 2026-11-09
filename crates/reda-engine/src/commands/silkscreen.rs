//! Silkscreen Preparation (pcb.md#silkscreen-preparation): `pcb.silkscreen.prepare` on the
//! board and `library.footprint.silkscreen.prepare` in the Footprint editor, each one
//! undoable edit. The planning is [`reda_pcb::silkscreen`]; this applies it. Clipping a
//! placed Footprint's silkscreen gives it a Locally Edited Footprint.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::geom::{Nm, Point};
use reda_model::{
    ComponentType, DocumentEntry, DocumentKind, FabRules, Fields, Footprint, FootprintCopy,
    FootprintGraphic, FootprintInstance, GraphicShape, InDocument, Object, PcbGraphic,
    PreferredSeeds, Project, ProjectProps, Side, TextOverride, Uuid,
};
use reda_pcb::silkscreen::{
    self, MAX_TEXT_DISTANCE, MIN_REMAINING_LENGTH, Options, Plan, Unplaced,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::local_edits::{needs_pcb, pcb, place_edited};
use crate::builders::add_document;
use crate::footprint_scene::DESIGNATOR_PLACEHOLDER;

/// Each Footprint graphic's new shapes, in the Footprint's own frame.
type GraphicPieces = Vec<(Uuid, Vec<reda_model::Shape>)>;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(PrepareBoard);
    registry.add(PrepareFootprint);
}

fn yes() -> bool {
    true
}

fn default_clearance() -> Length {
    Length(Nm(100_000))
}

fn default_min_remaining_length() -> Length {
    Length(MIN_REMAINING_LENGTH)
}

fn default_max_distance() -> Length {
    Length(MAX_TEXT_DISTANCE)
}

/// The options of the Silkscreen Preparation dialog.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareOptions {
    /// Take the clearances from the `silk_to_mask` and `silk_to_edge` rules. Off, the
    /// manual `clearance` and `clip_to_exposed_copper` apply, with no edge clearance.
    #[serde(default = "yes")]
    pub use_design_rules: bool,
    /// The manual Silkscreen Clearance from mask targets, without design rules.
    #[serde(default = "default_clearance")]
    pub clearance: Length,
    /// Without design rules: measure to the exposed copper rather than the whole mask
    /// opening.
    #[serde(default = "yes")]
    pub clip_to_exposed_copper: bool,
    /// Min Remaining Length: a line or arc shorter than this, end to end, goes, whether it
    /// was clipped or not.
    #[serde(default = "default_min_remaining_length")]
    pub min_remaining_length: Length,
    /// Delete Silkscreen Outside Board Shape.
    #[serde(default = "yes")]
    pub delete_outside_board: bool,
    /// Clip Locked Components and Primitives: also clip locked graphics and locked
    /// Footprints' silkscreen, and move their text.
    #[serde(default)]
    pub clip_locked: bool,
    /// Move text and Designators that aren't clear to the nearest clear place.
    #[serde(default = "yes")]
    pub move_text: bool,
    /// Max Distance a text moves.
    #[serde(default = "default_max_distance")]
    pub max_distance: Length,
}

impl Default for PrepareOptions {
    fn default() -> PrepareOptions {
        serde_json::from_value(serde_json::json!({})).expect("every option has a default")
    }
}

impl PrepareOptions {
    fn options(&self) -> Result<Options, CommandError> {
        for (value, pointer) in [
            (self.clearance, "/clearance"),
            (self.min_remaining_length, "/min_remaining_length"),
            (self.max_distance, "/max_distance"),
        ] {
            if value.nm() < Nm::ZERO {
                return Err(CommandError::invalid_params(pointer, "cannot be negative"));
            }
        }
        Ok(Options {
            use_design_rules: self.use_design_rules,
            clearance: self.clearance.nm(),
            clip_to_exposed_copper: self.clip_to_exposed_copper,
            min_remaining_length: self.min_remaining_length.nm(),
            delete_outside_board: self.delete_outside_board,
            clip_locked: self.clip_locked,
            move_text: self.move_text,
            max_distance: self.max_distance.nm(),
        })
    }
}

/// What Silkscreen Preparation changed.
#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct PrepareResult {
    /// Silkscreen graphics clipped or deleted (board graphics, or the Footprint's own).
    pub graphics: Vec<Uuid>,
    /// Placed Footprints whose silkscreen was clipped: each now has a Locally Edited
    /// Footprint.
    pub footprints: Vec<Uuid>,
    /// Texts moved: board texts, or Footprint field text entries.
    pub moved: Vec<Uuid>,
    /// Texts that found no clear place within Max Distance and stayed.
    pub unplaced: Vec<Unplaced>,
}

pub struct PrepareBoard;

impl Command for PrepareBoard {
    const META: Meta = Meta {
        id: "pcb.silkscreen.prepare",
        title: "Silkscreen Preparation",
        description: "Clears the silkscreen of the solder-mask openings and the board edge \
                      in one undoable edit. Lines, arcs and rectangles are clipped (a clipped \
                      rectangle becomes lines); a line or arc under Min Remaining Length \
                      goes; silkscreen outside the board goes. Texts and Designators that \
                      aren't clear move to the nearest clear place within Max Distance (a \
                      moved Designator becomes manual); those that find none stay and are \
                      listed in `unplaced`. Clipping a Footprint's silkscreen gives it a \
                      Locally Edited Footprint. Locked objects are left alone unless \
                      `clip_locked`.",
        category: "pcb",
        icon: "clearance",
        kind: Kind::Edit,
    };
    type Params = PrepareOptions;
    type Output = PrepareResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: PrepareOptions) -> Result<PrepareResult, CommandError> {
        let options = p.options()?;
        let pcb = pcb(cx)?;
        let plan = silkscreen::plan(pcb, cx.fonts(), &options);
        apply_board(cx, plan)
    }
}

fn apply_board(cx: &mut Context<'_>, plan: Plan) -> Result<PrepareResult, CommandError> {
    let pcb = pcb(cx)?;
    let doc = pcb.uuid;
    let mut result = PrepareResult {
        unplaced: plan.unplaced,
        ..PrepareResult::default()
    };
    let graphics: Vec<(PcbGraphic, Vec<GraphicShape>)> = plan
        .graphics
        .into_iter()
        .filter_map(|e| Some((pcb.graphic(e.graphic)?.clone(), e.pieces)))
        .collect();
    let fields: Vec<(FootprintInstance, silkscreen::FieldMove)> = plan
        .fields
        .into_iter()
        .filter_map(|m| Some((pcb.footprint(m.instance)?.clone(), m)))
        .collect();
    let texts: Vec<_> = plan
        .texts
        .into_iter()
        .filter_map(|m| Some((pcb.text(m.text)?.clone(), m.position)))
        .collect();
    let footprints: Vec<(Uuid, Footprint, GraphicPieces)> = plan
        .footprints
        .into_iter()
        .filter_map(|e| {
            let instance = pcb.footprint(e.instance)?;
            let copy = pcb.footprint_copy(instance.footprint)?;
            Some((e.instance, copy.footprint.clone(), e.graphics))
        })
        .collect();

    for (graphic, pieces) in graphics {
        result.graphics.push(graphic.uuid);
        let mut pieces = pieces.into_iter();
        match pieces.next() {
            None => drop(cx.tx()?.remove(graphic.uuid)?),
            Some(first) => {
                for shape in pieces {
                    let more = PcbGraphic {
                        uuid: cx.new_id(),
                        shape,
                        ..graphic.clone()
                    };
                    cx.tx()?
                        .insert(Object::PcbGraphic(InDocument::new(doc, more)))?;
                }
                let first = PcbGraphic {
                    shape: first,
                    ..graphic
                };
                cx.tx()?
                    .replace(Object::PcbGraphic(InDocument::new(doc, first)))?;
            }
        }
    }
    for (mut text, position) in texts {
        result.moved.push(text.uuid);
        text.position = position;
        cx.tx()?
            .replace(Object::PcbText(InDocument::new(doc, text)))?;
    }
    // Text moves first, so an instance that also gets a Locally Edited Footprint keeps
    // them: `place_edited` rereads the instance.
    let mut moved_instances: Vec<FootprintInstance> = Vec::new();
    for (instance, m) in fields {
        result.moved.push(m.text);
        let placed = match moved_instances.iter_mut().find(|i| i.uuid == instance.uuid) {
            Some(i) => i,
            None => {
                moved_instances.push(instance);
                moved_instances.last_mut().expect("just pushed")
            }
        };
        let index = match placed.text_overrides.iter().position(|o| o.text == m.text) {
            Some(i) => i,
            None => {
                placed.text_overrides.push(TextOverride::new(m.text));
                placed.text_overrides.sort_by_key(|o| o.text);
                placed
                    .text_overrides
                    .iter()
                    .position(|o| o.text == m.text)
                    .expect("just added")
            }
        };
        let change = &mut placed.text_overrides[index];
        change.at = Some(m.at);
        if let Some(rotation) = m.rotation {
            change.rotation = Some(rotation);
        }
        // A moved Designator is placed by hand from now on.
        change.autoposition = None;
    }
    for instance in moved_instances {
        cx.tx()?
            .replace(Object::FootprintInstance(InDocument::new(doc, instance)))?;
    }
    for (instance, mut footprint, edits) in footprints {
        result.footprints.push(instance);
        for (graphic, pieces) in edits {
            result.graphics.push(graphic);
            replace_graphic(cx, &mut footprint, graphic, pieces);
        }
        place_edited(cx, instance, footprint)?;
    }
    Ok(result)
}

/// Puts `pieces` in place of `graphic` in `footprint`: the first takes its UUID, the rest
/// are new graphics like it; none removes it.
fn replace_graphic(
    cx: &mut Context<'_>,
    footprint: &mut Footprint,
    graphic: Uuid,
    pieces: Vec<reda_model::Shape>,
) {
    let Some(index) = footprint.graphics.iter().position(|g| g.uuid == graphic) else {
        return;
    };
    let original = footprint.graphics.remove(index);
    for (i, shape) in pieces.into_iter().enumerate() {
        let uuid = if i == 0 { original.uuid } else { cx.new_id() };
        footprint.graphics.push(FootprintGraphic {
            uuid,
            shape,
            ..original.clone()
        });
    }
    footprint.graphics.sort_by_key(|g| g.uuid);
}

pub struct PrepareFootprint;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareFootprintParams {
    /// The open Footprint.
    pub footprint: Uuid,
    #[serde(flatten)]
    pub options: PrepareOptions,
}

impl Command for PrepareFootprint {
    const META: Meta = Meta {
        id: "library.footprint.silkscreen.prepare",
        title: "Silkscreen Preparation",
        description: "`pcb.silkscreen.prepare` for an open Footprint, in one undoable \
                      edit: its silkscreen lines, arcs and rectangles are clipped clear of \
                      its Pads' mask openings, short leftovers go, and its silkscreen field \
                      texts move to the nearest clear place. The design rules are a new \
                      board's: 0.10 mm from exposed copper, with each Pad's own mask \
                      override.",
        category: "library",
        icon: "clearance",
        kind: Kind::Edit,
    };
    type Params = PrepareFootprintParams;
    type Output = PrepareResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: PrepareFootprintParams,
    ) -> Result<PrepareResult, CommandError> {
        let options = p.options.options()?;
        let footprint = match cx.project()?.element(p.footprint) {
            Some(reda_model::LibraryElement::Footprint(f)) => f.clone(),
            Some(_) => {
                return Err(CommandError::invalid_params(
                    "/footprint",
                    "not a Footprint",
                ));
            }
            None => {
                return Err(CommandError::not_found(format!(
                    "no open Footprint {}",
                    p.footprint
                )));
            }
        };
        let (project, document, instance) = board_holding(&footprint);
        let pcb = project.pcb_document(document).expect("just added");
        let plan = silkscreen::plan(pcb, cx.fonts(), &options);
        let mut result = PrepareResult {
            unplaced: plan
                .unplaced
                .into_iter()
                .map(|u| Unplaced {
                    object: p.footprint,
                    ..u
                })
                .collect(),
            ..PrepareResult::default()
        };
        let mut edited = footprint;
        for edit in plan
            .footprints
            .into_iter()
            .filter(|e| e.instance == instance)
        {
            for (graphic, pieces) in edit.graphics {
                result.graphics.push(graphic);
                replace_graphic(cx, &mut edited, graphic, pieces);
            }
        }
        for m in plan.fields.into_iter().filter(|m| m.instance == instance) {
            if let Some(text) = edited.texts.iter_mut().find(|t| t.uuid == m.text) {
                result.moved.push(m.text);
                text.at = m.at;
            }
        }
        if result.graphics.is_empty() && result.moved.is_empty() {
            return Ok(result);
        }
        reda_library::edit::edit_footprint(cx, p.footprint, |f| {
            *f = edited;
            Ok(())
        })?;
        Ok(result)
    }
}

/// A blank Project holding a new board (a new board's rules, no outline) with `footprint`
/// placed once at the origin on top, its Designator shown as in the Footprint editor.
/// Returns the Project, the board and the instance.
fn board_holding(footprint: &Footprint) -> (Project, Uuid, Uuid) {
    let mut next = footprint.uuid.as_u128();
    let mut new_id = move || {
        next = next.wrapping_add(1);
        Uuid::from_u128(next)
    };
    let mut project = Project::new(ProjectProps {
        uuid: new_id(),
        name: String::new(),
        documents: vec![],
        libraries: vec![],
        properties: Default::default(),
        erc: Default::default(),
        schematic: Default::default(),
        reannotate: None,
        library_update: None,
        missing: vec![],
    });
    let document = new_id();
    let rules = FabRules {
        mask_expansion: Default::default(),
        paste_expansion: Default::default(),
        clearance: Nm(0),
        track_width: Nm(0),
        via_diameter: Nm(0),
        via_hole: Nm(0),
        hole_min: Nm(0),
        hole_max: Nm(0),
        annular_ring: Nm(0),
        hole_to_hole: Nm(0),
        edge_clearance: Nm(0),
        preferred: PreferredSeeds {
            track_width: Nm(0),
            via_diameter: Nm(0),
            via_hole: Nm(0),
        },
    };
    let copy = FootprintCopy {
        uuid: new_id(),
        hash: String::new(),
        edited_from: None,
        footprint: footprint.without_generator(),
    };
    let instance = FootprintInstance {
        uuid: new_id(),
        component: None,
        footprint: copy.uuid,
        at: Point::default(),
        rotation: Default::default(),
        side: Side::Top,
        locked: false,
        designator: DESIGNATOR_PLACEHOLDER.to_owned(),
        fields: Fields {
            value: footprint.name.clone(),
            ..Fields::default()
        },
        component_type: ComponentType::Standard,
        in_bom: false,
        in_cpl: false,
        not_fitted: false,
        text_overrides: Vec::new(),
        pads: footprint
            .pads
            .iter()
            .map(|p| reda_model::PadNet {
                pad: p.uuid,
                net: None,
            })
            .collect(),
    };
    let placed = instance.uuid;
    let mut tx = project.transaction();
    let entry = DocumentEntry {
        uuid: document,
        kind: DocumentKind::Pcb,
        name: String::new(),
        title: String::new(),
    };
    add_document(&mut tx, entry).expect("a blank Project takes a board");
    for object in reda_pcb::new_board(document, &rules, Default::default(), &mut new_id) {
        tx.insert(object).expect("a new board's objects fit");
    }
    tx.insert(Object::FootprintCopy(InDocument::new(document, copy)))
        .expect("a copy fits");
    tx.insert(Object::FootprintInstance(InDocument::new(
        document, instance,
    )))
    .expect("an instance fits");
    tx.commit().expect("a scratch board commits");
    (project, document, placed)
}
