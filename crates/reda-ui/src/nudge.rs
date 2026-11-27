//! Nudging the selection (docs/spec/v0.3/input-and-defaults.md#nudging): `Mod`+arrows
//! move it one grid step and Shift+`Mod`+arrows ten, in the PCB, Footprint and Symbol
//! editors as in the Schematic editor (`schematic.nudge`, in
//! [`crate::schematic_editor`]).
//!
//! - `pcb.nudge`, `library.footprint.nudge` and `library.symbol.nudge {dx, dy}` are view
//!   Commands on the active canvas's selection. Each works out the move and returns it
//!   ([`Nudged`]); the Shell then runs it as the one edit Command a drag would run
//!   ([`after_run`]), so a press is one undo step.
//! - **The step** follows the Edit Cursor's ([`crate::edit_cursor`]), measured at the
//!   selection's first object: from off the grid the first press puts that object on the
//!   next grid line in that direction, and the rest of the selection keeps its distance.
//!   `dx` is screen-right, so on a board seen from below it goes towards smaller x.
//! - **As in a drag:** on a board a lone Via or Free Pad is dragged with its Tracks
//!   (`pcb.copper.drag`) and a lone Track segment slides (`pcb.track.drag`), under the
//!   drag Preferences. Anything else moves with the Command that moves its kind:
//!   Footprints, Free Pads, Dimensions, the Output Origin, the Board Outline and Board
//!   Cutouts with `pcb.move` (their Tracks staying put, as in a mouse move), free text
//!   with `pcb.text.set`, Zones, Zone Cutouts and polygon Keepouts with
//!   `pcb.polygon.set_vertices` and line Keepouts with `pcb.keepout.set`, in one `batch`
//!   when it takes more than one. Board graphics stay, as no Command moves them; nor does
//!   a Zone that follows the Board Outline. A Footprint's or Symbol's parts move with
//!   `library.*.move`.
//! - **Locked objects stay put** (docs/spec/v0.1/pcb.md#locks): they are left behind
//!   whatever Protect Locked Objects says, since a key press asks nothing.

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::geom::{Millideg, Point, Vertex};
use reda_engine::model::{
    Footprint, KeepoutShape, LibraryElement, PcbDocument, Shape, Symbol, Uuid,
};
use reda_engine::pcb::lock::is_locked;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::bars::DocumentType;
use crate::selection::Selection;
use crate::shell::Shell;

/// Adds the Commands to `registry`.
pub fn register_all(registry: &mut Registry) {
    registry.add(PcbNudge);
    registry.add(FootprintNudge);
    registry.add(SymbolNudge);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NudgeParams {
    /// Grid steps to the right as the canvas shows it; negative goes left. On a board
    /// seen from below, right is towards smaller x.
    #[schemars(range(min = -1000, max = 1000))]
    dx: i64,
    /// Grid steps up; negative goes down.
    #[schemars(range(min = -1000, max = 1000))]
    dy: i64,
}

/// The move a nudge makes, which the Shell runs as `edit`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Nudged {
    /// The board, or open Footprint or Symbol, whose selection moves.
    pub document: Uuid,
    /// The objects that move, in selection order; locked ones are left out.
    pub objects: Vec<Uuid>,
    /// How far they move (nm).
    pub by: Point,
    /// The edit Command that moves them: `pcb.move`, `pcb.text.set`,
    /// `pcb.polygon.set_vertices`, `pcb.keepout.set` or a `batch` of them on a board,
    /// `pcb.copper.drag`, `pcb.track.drag`, `library.footprint.move` or
    /// `library.symbol.move`.
    pub edit: String,
}

/// What a board nudge moves, and how.
#[derive(Clone, Debug, PartialEq, Eq)]
enum BoardMove {
    /// A lone Via or Free Pad, dragged with its Tracks.
    Copper(Uuid),
    /// A lone Track segment, slid across itself.
    Slide(Uuid),
    /// Everything else a Command can move: see [`board_steps`].
    Place(Vec<Uuid>),
}

impl BoardMove {
    /// How the unlocked objects of `selected` move on `pcb`.
    fn of(pcb: &PcbDocument, selected: &[Uuid]) -> BoardMove {
        let unlocked: Vec<Uuid> = (selected.iter().copied())
            .filter(|o| !is_locked(pcb, *o))
            .collect();
        match unlocked[..] {
            [o] if pcb.via(o).is_some() || pcb.free_pad(o).is_some() => BoardMove::Copper(o),
            [o] if pcb.track(o).is_some() => BoardMove::Slide(o),
            _ => BoardMove::Place(
                unlocked
                    .into_iter()
                    .filter(|o| {
                        board_anchor(pcb, *o).is_some()
                            && (placeable(pcb, *o)
                                || pcb.text(*o).is_some()
                                || outline_of(pcb, *o).is_some())
                    })
                    .collect(),
            ),
        }
    }

    fn objects(&self) -> Vec<Uuid> {
        match self {
            BoardMove::Copper(o) | BoardMove::Slide(o) => vec![*o],
            BoardMove::Place(objects) => objects.clone(),
        }
    }

    fn edit(&self, pcb: &PcbDocument) -> &'static str {
        match self {
            BoardMove::Copper(_) => "pcb.copper.drag",
            BoardMove::Slide(_) => "pcb.track.drag",
            BoardMove::Place(objects) => match &board_steps(pcb, objects, Point::default())[..] {
                [(id, _)] => id,
                _ => "batch",
            },
        }
    }
}

/// The vertices a nudge moves of a Zone (unless it follows the Board Outline), Zone
/// Cutout or Keepout, and whether that is a line Keepout's.
fn outline_of(pcb: &PcbDocument, object: Uuid) -> Option<(&[Vertex], bool)> {
    if let Some(z) = pcb.zone(object) {
        return (!z.follows_outline).then_some((&z.vertices[..], false));
    }
    if let Some(c) = pcb.zone_cutout(object) {
        return Some((&c.vertices, false));
    }
    let keepout = pcb.keepout(object)?;
    let line = matches!(keepout.shape, KeepoutShape::Line { .. });
    Some((keepout.vertices(), line))
}

/// `vertices` moved `by`, as Commands take them.
fn moved_vertices(vertices: &[Vertex], by: Point) -> Vec<Value> {
    (vertices.iter())
        .map(|v| {
            let mut p = json!({"x": (v.x + by.x).0, "y": (v.y + by.y).0});
            if v.sweep != Millideg::ZERO {
                p["sweep"] = json!(v.sweep.0);
            }
            p
        })
        .collect()
}

/// The edit Commands that move `objects` (none of them a lone Via, Free Pad or Track)
/// `by` on `pcb`: one `pcb.move` for the kinds it takes, then one for each text, Zone,
/// Zone Cutout and Keepout.
fn board_steps(pcb: &PcbDocument, objects: &[Uuid], by: Point) -> Vec<(&'static str, Value)> {
    let by_param = json!({"x": by.x.0, "y": by.y.0});
    let placed: Vec<Uuid> = (objects.iter().copied())
        .filter(|o| placeable(pcb, *o))
        .collect();
    let mut steps = Vec::new();
    if !placed.is_empty() {
        steps.push(("pcb.move", json!({"objects": placed, "by": by_param})));
    }
    for &o in objects {
        if let Some(t) = pcb.text(o) {
            let at = t.position + by;
            steps.push(("pcb.text.set", json!({"uuid": o, "x": at.x.0, "y": at.y.0})));
        } else if let Some((vertices, line)) = outline_of(pcb, o) {
            let vertices = moved_vertices(vertices, by);
            steps.push(if line {
                (
                    "pcb.keepout.set",
                    json!({"keepout": o, "vertices": vertices}),
                )
            } else {
                (
                    "pcb.polygon.set_vertices",
                    json!({"object": o, "vertices": vertices}),
                )
            });
        }
    }
    steps
}

/// Whether `pcb.move` moves `object`.
fn placeable(pcb: &PcbDocument, object: Uuid) -> bool {
    pcb.footprint(object).is_some()
        || pcb.free_pad(object).is_some()
        || pcb.dimension(object).is_some()
        || pcb.cutout(object).is_some()
        || pcb.outline().is_some_and(|o| o.uuid == object)
        || pcb.output_origin().is_some_and(|o| o.uuid == object)
}

fn first_vertex(vertices: &[Vertex]) -> Option<Point> {
    vertices.first().map(Vertex::point)
}

/// The point of `object` on `pcb` that a nudge puts on the grid.
fn board_anchor(pcb: &PcbDocument, object: Uuid) -> Option<Point> {
    (pcb.footprint(object).map(|f| f.at))
        .or_else(|| pcb.free_pad(object).map(|p| p.at))
        .or_else(|| pcb.via(object).map(|v| v.at))
        .or_else(|| pcb.track(object).map(|t| t.geometry.ends().0))
        .or_else(|| pcb.dimension(object).map(|d| d.a.point))
        .or_else(|| pcb.text(object).map(|t| t.position))
        .or_else(|| outline_of(pcb, object).and_then(|(v, _)| first_vertex(v)))
        .or_else(|| pcb.cutout(object).and_then(|c| first_vertex(&c.vertices)))
        .or_else(|| {
            (pcb.outline())
                .filter(|o| o.uuid == object)
                .and_then(|o| first_vertex(&o.vertices))
        })
        .or_else(|| {
            (pcb.output_origin())
                .filter(|o| o.uuid == object)
                .map(|o| o.at)
        })
}

/// The point of a library shape that a nudge puts on the grid.
fn shape_anchor(shape: &Shape) -> Point {
    match shape {
        Shape::Line { start, .. } | Shape::Rect { start, .. } => *start,
        Shape::Arc { center, .. } | Shape::Circle { center, .. } => *center,
        Shape::Polygon { points } => points.first().copied().unwrap_or_default(),
        Shape::Text { at, .. } => *at,
    }
}

fn footprint_anchor(footprint: &Footprint, object: Uuid) -> Option<Point> {
    (footprint
        .pads
        .iter()
        .find(|p| p.uuid == object)
        .map(|p| p.at))
    .or_else(|| {
        (footprint.graphics.iter())
            .find(|g| g.uuid == object)
            .map(|g| shape_anchor(&g.shape))
    })
    .or_else(|| {
        footprint
            .texts
            .iter()
            .find(|t| t.uuid == object)
            .map(|t| t.at)
    })
}

fn symbol_anchor(symbol: &Symbol, object: Uuid) -> Option<Point> {
    (symbol.pins.iter().find(|p| p.uuid == object).map(|p| p.at)).or_else(|| {
        (symbol.graphics.iter())
            .find(|g| g.uuid == object)
            .map(|g| shape_anchor(&g.shape))
    })
}

/// The objects `selected` that `kind`'s nudge moves on `document`, the edit that moves
/// them and the point that steps on the grid (the first one's).
fn plan(
    cx: &mut Context<'_>,
    kind: DocumentType,
    document: Uuid,
    selected: &[Uuid],
) -> Result<(Vec<Uuid>, &'static str, Option<Point>), CommandError> {
    let project = cx.project()?;
    let not_here = || {
        CommandError::unavailable(match kind {
            DocumentType::Pcb => "the active canvas is not the board",
            DocumentType::Footprint => "the active canvas is not an open Footprint",
            _ => "the active canvas is not an open Symbol",
        })
    };
    Ok(match kind {
        DocumentType::Pcb => {
            let pcb = project.pcb_document(document).ok_or_else(not_here)?;
            let moved = BoardMove::of(pcb, selected);
            let objects = moved.objects();
            let anchor = objects.first().and_then(|o| board_anchor(pcb, *o));
            (objects, moved.edit(pcb), anchor)
        }
        DocumentType::Footprint => {
            let Some(LibraryElement::Footprint(f)) = project.element(document) else {
                return Err(not_here());
            };
            let objects: Vec<Uuid> = (selected.iter().copied())
                .filter(|o| footprint_anchor(f, *o).is_some())
                .collect();
            let anchor = objects.first().and_then(|o| footprint_anchor(f, *o));
            (objects, "library.footprint.move", anchor)
        }
        _ => {
            let Some(LibraryElement::Symbol(s)) = project.element(document) else {
                return Err(not_here());
            };
            let objects: Vec<Uuid> = (selected.iter().copied())
                .filter(|o| symbol_anchor(s, *o).is_some())
                .collect();
            let anchor = objects.first().and_then(|o| symbol_anchor(s, *o));
            (objects, "library.symbol.move", anchor)
        }
    })
}

/// Runs `kind`'s nudge on the active canvas.
fn nudge(cx: &mut Context<'_>, kind: DocumentType, p: NudgeParams) -> Result<Nudged, CommandError> {
    if p.dx == 0 && p.dy == 0 {
        return Err(CommandError::invalid_params(
            "/dx",
            "a nudge moves at least one grid step",
        ));
    }
    let document = crate::navigation::canvas_of(cx, None)?;
    let selected = cx.view::<Selection>()?.objects.clone();
    let (objects, edit, anchor) = plan(cx, kind, document, &selected)?;
    let Some(anchor) = anchor else {
        return Err(CommandError::rejected(
            "nothing is selected that can be nudged",
        ));
    };
    let (grid, _, mirrored) = crate::edit_cursor::step_grid(cx, document)?;
    let dx = if mirrored { -p.dx } else { p.dx };
    let by = grid.step(anchor, dx, p.dy, false) - anchor;
    Ok(Nudged {
        document,
        objects,
        by,
        edit: edit.to_owned(),
    })
}

/// The description the three nudges share, after `$what`.
macro_rules! described {
    ($what:literal) => {
        concat!(
            $what,
            " `dx` grid steps right and `dy` up, rubber-banding as a drag does: one edit, so \
             one undo step per press. The step is measured at the first selected object, \
             which lands on the next grid line when it is off the grid. Locked objects stay \
             put. Mod+arrows nudge by 1 step, Shift+Mod+arrows by 10."
        )
    };
}

/// `pcb.nudge`.
struct PcbNudge;

impl Command for PcbNudge {
    const META: Meta = Meta {
        id: "pcb.nudge",
        title: "Nudge",
        description: concat!(
            described!("Moves the board's selection"),
            " A lone Via or Free Pad keeps its Tracks attached and a lone Track segment \
             slides. Otherwise Footprints, Free Pads, Dimensions, the Output Origin, the \
             Board Outline and Board Cutouts move as a mouse move moves them, and free \
             text, Zones, Zone Cutouts and Keepouts move with them; board graphics stay."
        ),
        category: "pcb",
        icon: "drag",
        kind: Kind::View,
    };
    type Params = NudgeParams;
    type Output = Nudged;

    fn run(&self, cx: &mut Context<'_>, p: NudgeParams) -> Result<Nudged, CommandError> {
        nudge(cx, DocumentType::Pcb, p)
    }
}

/// `library.footprint.nudge`.
struct FootprintNudge;

impl Command for FootprintNudge {
    const META: Meta = Meta {
        id: "library.footprint.nudge",
        title: "Nudge",
        description: described!(
            "Moves the selected Pads, graphics and texts of the active open Footprint"
        ),
        category: "library",
        icon: "drag",
        kind: Kind::View,
    };
    type Params = NudgeParams;
    type Output = Nudged;

    fn run(&self, cx: &mut Context<'_>, p: NudgeParams) -> Result<Nudged, CommandError> {
        nudge(cx, DocumentType::Footprint, p)
    }
}

/// `library.symbol.nudge`.
struct SymbolNudge;

impl Command for SymbolNudge {
    const META: Meta = Meta {
        id: "library.symbol.nudge",
        title: "Nudge",
        description: described!("Moves the selected Pins and graphics of the active open Symbol"),
        category: "library",
        icon: "drag",
        kind: Kind::View,
    };
    type Params = NudgeParams;
    type Output = Nudged;

    fn run(&self, cx: &mut Context<'_>, p: NudgeParams) -> Result<Nudged, CommandError> {
        nudge(cx, DocumentType::Symbol, p)
    }
}

/// The edit Command that carries out the nudge `n`, with its params.
fn edit_of(shell: &Shell, n: &Nudged) -> (String, Value) {
    let by = json!({"x": n.by.x, "y": n.by.y});
    let prefs = shell.preferences();
    let one = n.objects.first().copied();
    let params = match n.edit.as_str() {
        "pcb.copper.drag" => json!({"object": one, "by": by,
            "preserve_angle": prefs.pcb.drag.preserve_angle}),
        "pcb.track.drag" => json!({"track": one, "by": by,
            "preserve_angle": prefs.pcb.drag.preserve_angle,
            "obstacle_mode": prefs.pcb.route.obstacle_mode}),
        "library.footprint.move" => {
            json!({"footprint": n.document, "objects": n.objects, "by": by})
        }
        "library.symbol.move" => json!({"symbol": n.document, "objects": n.objects, "by": by}),
        _ => {
            let project = shell.engine().project();
            let steps = (project.and_then(|p| p.pcb_document(n.document)))
                .map(|pcb| board_steps(pcb, &n.objects, n.by))
                .unwrap_or_default();
            return match &steps[..] {
                [(id, params)] => ((*id).to_owned(), params.clone()),
                _ => {
                    let commands: Vec<Value> = (steps.iter())
                        .map(|(id, params)| json!({"id": id, "params": params}))
                        .collect();
                    ("batch".to_owned(), json!({"commands": commands}))
                }
            };
        }
    };
    (n.edit.clone(), params)
}

/// Runs the move a nudge Command returned (`out`), as one edit Command.
pub(crate) fn after_run(shell: &mut Shell, out: &Value) {
    let Ok(n) = serde_json::from_value::<Nudged>(out.clone()) else {
        return;
    };
    if n.by == Point::default() || n.objects.is_empty() {
        return;
    }
    let (id, params) = edit_of(shell, &n);
    let _ = shell.run_reporting(&id, params);
}
