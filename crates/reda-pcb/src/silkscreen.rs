//! Silkscreen Preparation (pcb.md#silkscreen-preparation): what `pcb.silkscreen.prepare`
//! changes, worked out without touching the board.
//!
//! Silkscreen lines, arcs, rectangles, polygons and circles are clipped where their pen
//! would come closer to a mask target than the silk-to-mask clearance, or closer to the
//! board edge than the silk-to-edge clearance, and (with `delete_outside_board`) wherever
//! they leave the board. A clipped rectangle or polygon becomes lines and a clipped circle
//! arcs. A line or arc shorter than `min_remaining_length` goes, clipped or not. Text that
//! is not clear moves to the nearest clear place no further than `max_distance`, searched
//! outward on a 0.1 mm grid; text that finds none stays and is listed in `unplaced`.
//!
//! The targets are exactly those the `silk_to_mask` check measures against, and every
//! cut keeps a little more than the clearance, so a board prepared with the design rules
//! passes that check.

mod clip;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

use reda_command::Fonts;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    GraphicShape, LayerKind, PcbDocument, RuleField, RuleKind, Shape, Side, SilkMaskMode,
    TextAutoposition, Uuid,
};
use serde::Serialize;

use self::clip::{Curve, Cuts, kept, to_point};
use crate::drc::silkscreen::{targets, text_area};
use crate::placed::{Frame, placed_graphics};
use crate::rules::resolve;
use crate::text::{footprint_texts, text_geometry};
use crate::zone::poly::{self, Area, inside};

/// Min Remaining Length's default: REDA's own, as the docs give none.
pub const MIN_REMAINING_LENGTH: Nm = Nm(500_000);
/// Max Distance's default for moving text: REDA's own, as the docs give none.
pub const MAX_TEXT_DISTANCE: Nm = Nm(2_000_000);
/// How far apart the places tried for a text are.
const SEARCH_STEP: i64 = 100_000;
/// Kept beyond each clearance, so rounding and the polygons that stand in for round pen
/// ends never bring silkscreen under it.
const MARGIN: i64 = 2_000;

/// What Silkscreen Preparation does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Take the clearances from the `silk_to_mask` and `silk_to_edge` rules; else
    /// `clearance` from mask targets (no edge clearance), measured to exposed copper with
    /// `clip_to_exposed_copper` or to the mask opening without.
    pub use_design_rules: bool,
    pub clearance: Nm,
    pub clip_to_exposed_copper: bool,
    pub min_remaining_length: Nm,
    pub delete_outside_board: bool,
    /// Also clip locked graphics and the silkscreen of locked Footprints, and move their
    /// text.
    pub clip_locked: bool,
    pub move_text: bool,
    pub max_distance: Nm,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            use_design_rules: true,
            clearance: Nm(100_000),
            clip_to_exposed_copper: true,
            min_remaining_length: MIN_REMAINING_LENGTH,
            delete_outside_board: true,
            clip_locked: false,
            move_text: true,
            max_distance: MAX_TEXT_DISTANCE,
        }
    }
}

/// A board graphic's new shapes: the first takes its place, the rest are new graphics
/// like it; none deletes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphicEdit {
    pub graphic: Uuid,
    pub pieces: Vec<GraphicShape>,
}

/// A placed Footprint whose silkscreen drawing changes: each listed graphic's new shapes in
/// the Footprint's own frame (the first takes its place, none deletes it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootprintEdit {
    pub instance: Uuid,
    pub graphics: Vec<(Uuid, Vec<Shape>)>,
}

/// A board text moved to `position`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextMove {
    pub text: Uuid,
    pub position: Point,
}

/// A placed Footprint's field text moved to `at`, in the Footprint's own frame. `rotation`
/// is set when the text was placed automatically, to keep the turn it showed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldMove {
    pub instance: Uuid,
    pub text: Uuid,
    pub at: Point,
    pub rotation: Option<Millideg>,
}

/// A text that found no clear place within reach and stayed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Unplaced {
    /// The board text, or the placed Footprint.
    pub object: Uuid,
    /// The Footprint's field text entry, for Footprint text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Uuid>,
    /// What it reads.
    pub content: String,
}

/// Everything Silkscreen Preparation changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub graphics: Vec<GraphicEdit>,
    pub footprints: Vec<FootprintEdit>,
    pub texts: Vec<TextMove>,
    pub fields: Vec<FieldMove>,
    pub unplaced: Vec<Unplaced>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.graphics.is_empty()
            && self.footprints.is_empty()
            && self.texts.is_empty()
            && self.fields.is_empty()
    }
}

/// What Silkscreen Preparation would change on `pcb`.
pub fn plan(pcb: &PcbDocument, fonts: &Fonts, options: &Options) -> Plan {
    let env = Env::new(pcb, options);
    let mut plan = Plan::default();
    let side_of = |layer: Uuid| {
        pcb.layers()
            .layer(layer)
            .filter(|l| l.kind == LayerKind::Silkscreen)
            .and_then(|l| l.side)
    };
    for graphic in pcb.graphics() {
        let Some(side) = side_of(graphic.layer) else {
            continue;
        };
        if graphic.locked && !options.clip_locked {
            continue;
        }
        let drawn = match graphic.shape {
            GraphicShape::Line { from, to } => Drawn::Open(Curve::line(from, to)),
            GraphicShape::Arc { from, to, sweep } => Drawn::Open(Curve::arc(
                arc_center(from, to, sweep),
                from,
                radians(sweep),
            )),
            GraphicShape::Rect { filled: true, .. } => continue,
            GraphicShape::Rect { from, to, .. } => {
                Drawn::closed(&[from, Point::new(to.x, from.y), to, Point::new(from.x, to.y)])
            }
        };
        if let Some(pieces) = env.clip(side, graphic.width, &drawn) {
            plan.graphics.push(GraphicEdit {
                graphic: graphic.uuid,
                pieces: pieces.iter().map(Piece::board_shape).collect(),
            });
        }
    }
    for instance in pcb.footprints() {
        if instance.locked && !options.clip_locked {
            continue;
        }
        let frame = Frame::of(instance);
        let mut edited = Vec::new();
        for graphic in placed_graphics(pcb, instance) {
            let side = match graphic.layer {
                reda_model::FootprintLayer::SilkscreenTop => Side::Top,
                reda_model::FootprintLayer::SilkscreenBottom => Side::Bottom,
                _ => continue,
            };
            let Some(drawn) = Drawn::of_shape(&graphic.shape, graphic.fill) else {
                continue;
            };
            if let Some(pieces) = env.clip(side, graphic.width, &drawn) {
                let local = pieces.iter().map(|p| p.local_shape(&frame)).collect();
                edited.push((graphic.graphic, local));
            }
        }
        if !edited.is_empty() {
            plan.footprints.push(FootprintEdit {
                instance: instance.uuid,
                graphics: edited,
            });
        }
    }
    for text in pcb.texts() {
        let Some(side) = side_of(text.layer) else {
            continue;
        };
        if text.locked && !options.clip_locked {
            continue;
        }
        let area = text_area(&text_geometry(text, fonts));
        match env.place(side, &area) {
            Placement::Clear => {}
            Placement::Moved(by) => plan.texts.push(TextMove {
                text: text.uuid,
                position: text.position + by,
            }),
            Placement::Stuck => plan.unplaced.push(Unplaced {
                object: text.uuid,
                text: None,
                content: text.text.clone(),
            }),
        }
    }
    for instance in pcb.footprints() {
        if instance.locked && !options.clip_locked {
            continue;
        }
        let Some(copy) = pcb.footprint_copy(instance.footprint) else {
            continue;
        };
        let fields: BTreeSet<Uuid> = copy.footprint.texts.iter().map(|t| t.uuid).collect();
        let frame = Frame::of(instance);
        for text in footprint_texts(pcb, instance, fonts) {
            if !fields.contains(&text.uuid) {
                continue;
            }
            let Some(side) = side_of(text.layer) else {
                continue;
            };
            let area = text_area(&text_geometry(&text, fonts));
            match env.place(side, &area) {
                Placement::Clear => {}
                Placement::Moved(by) => {
                    let automatic = instance
                        .text_override(text.uuid)
                        .and_then(|o| o.autoposition)
                        .is_some_and(|a| a != TextAutoposition::Manual);
                    plan.fields.push(FieldMove {
                        instance: instance.uuid,
                        text: text.uuid,
                        at: frame.local(text.position + by),
                        rotation: automatic.then(|| frame.local_angle(text.rotation)),
                    });
                }
                Placement::Stuck => plan.unplaced.push(Unplaced {
                    object: instance.uuid,
                    text: Some(text.uuid),
                    content: text.text.clone(),
                }),
            }
        }
    }
    plan
}

/// One silkscreen drawing as centre lines.
enum Drawn {
    /// A line or arc: kept whole, cut, or dropped when too short.
    Open(Curve),
    /// A rectangle's or polygon's sides, or a circle as one turn: kept whole unless cut,
    /// when its pieces become lines or arcs.
    Closed(Vec<Curve>),
}

impl Drawn {
    fn closed(corners: &[Point]) -> Drawn {
        let n = corners.len();
        Drawn::Closed(
            (0..n)
                .map(|i| Curve::line(corners[i], corners[(i + 1) % n]))
                .collect(),
        )
    }

    /// A Footprint drawing shape on the board; `None` for filled shapes and text, which
    /// stay as they are.
    fn of_shape(shape: &Shape, fill: bool) -> Option<Drawn> {
        Some(match shape {
            Shape::Line { start, end } => Drawn::Open(Curve::line(*start, *end)),
            Shape::Arc {
                center,
                start,
                sweep,
            } => Drawn::Open(Curve::arc(*center, *start, radians(*sweep))),
            _ if fill => return None,
            Shape::Rect { start, end } => Drawn::closed(&[
                *start,
                Point::new(end.x, start.y),
                *end,
                Point::new(start.x, end.y),
            ]),
            Shape::Polygon { points } if points.len() >= 2 => Drawn::closed(points),
            Shape::Circle { center, radius } => Drawn::Closed(vec![Curve::arc(
                *center,
                Point::new(center.x + *radius, center.y),
                TAU,
            )]),
            _ => return None,
        })
    }
}

/// A piece of clipped silkscreen, on the board.
#[derive(Clone, Copy, Debug)]
enum Piece {
    Line {
        from: Point,
        to: Point,
    },
    Arc {
        center: Point,
        start: Point,
        sweep: Millideg,
    },
}

impl Piece {
    fn of(curve: &Curve, (t0, t1): (f64, f64)) -> Piece {
        match *curve {
            Curve::Line { .. } => Piece::Line {
                from: to_point(curve.at(t0)),
                to: to_point(curve.at(t1)),
            },
            Curve::Arc { center, sweep, .. } => Piece::Arc {
                center: to_point(center),
                start: to_point(curve.at(t0)),
                sweep: Millideg((sweep.to_degrees() * 1000.0 * (t1 - t0)).round() as i64),
            },
        }
    }

    fn ends(&self) -> (Point, Point) {
        match *self {
            Piece::Line { from, to } => (from, to),
            Piece::Arc {
                center,
                start,
                sweep,
            } => (start, start.rotated_about(center, sweep)),
        }
    }

    /// The distance between its ends.
    fn length(&self) -> f64 {
        let (a, b) = self.ends();
        ((b.x - a.x).0 as f64).hypot((b.y - a.y).0 as f64)
    }

    fn board_shape(&self) -> GraphicShape {
        match *self {
            Piece::Line { from, to } => GraphicShape::Line { from, to },
            Piece::Arc { sweep, .. } => {
                let (from, to) = self.ends();
                GraphicShape::Arc { from, to, sweep }
            }
        }
    }

    fn local_shape(&self, frame: &Frame) -> Shape {
        match *self {
            Piece::Line { from, to } => Shape::Line {
                start: frame.local(from),
                end: frame.local(to),
            },
            Piece::Arc {
                center,
                start,
                sweep,
            } => Shape::Arc {
                center: frame.local(center),
                start: frame.local(start),
                sweep: if frame.side == Side::Bottom {
                    -sweep
                } else {
                    sweep
                },
            },
        }
    }
}

/// Where a text ends up.
enum Placement {
    Clear,
    Moved(Point),
    Stuck,
}

/// What silkscreen keeps clear of, per side.
struct Env<'a> {
    options: &'a Options,
    /// The mask targets of each side (top, bottom).
    targets: [Vec<Area>; 2],
    mask: Option<Nm>,
    /// The Board Outline and each Board Cutout, as areas.
    edges: Vec<Area>,
    edge: Option<Nm>,
    /// The board shape, when silkscreen outside it goes.
    board: Option<Area>,
    forbidden: RefCell<BTreeMap<(usize, i64), std::rc::Rc<Cuts>>>,
    offsets: Vec<Point>,
}

fn side_index(side: Side) -> usize {
    match side {
        Side::Top => 0,
        Side::Bottom => 1,
    }
}

impl<'a> Env<'a> {
    fn new(pcb: &PcbDocument, options: &'a Options) -> Env<'a> {
        let (mask, mode) = if options.use_design_rules {
            let value = resolve(pcb, None, RuleKind::SilkToMask, RuleField::Value)
                .and_then(|r| r.value.length());
            let mode = resolve(pcb, None, RuleKind::SilkToMask, RuleField::Mode)
                .and_then(|m| m.value.mode())
                .unwrap_or_default();
            (value, mode)
        } else {
            let mode = match options.clip_to_exposed_copper {
                true => SilkMaskMode::ExposedCopper,
                false => SilkMaskMode::MaskOpening,
            };
            (Some(options.clearance), mode)
        };
        let mut sided: [Vec<Area>; 2] = [Vec::new(), Vec::new()];
        if mask.is_some() {
            for target in targets(pcb, mode) {
                sided[side_index(target.side)].push(target.area);
            }
        }
        let outline = pcb
            .outline()
            .filter(|o| o.vertices.len() >= 3)
            .map(|o| poly::polygon(poly::contour(&o.vertices)));
        let cutouts: Vec<Area> = pcb
            .cutouts()
            .filter(|c| c.vertices.len() >= 3)
            .map(|c| poly::polygon(poly::contour(&c.vertices)))
            .collect();
        let rule_edge = if options.use_design_rules {
            resolve(pcb, None, RuleKind::SilkToEdge, RuleField::Value)
                .and_then(|r| r.value.length())
        } else {
            None
        };
        let board = outline
            .as_ref()
            .filter(|_| options.delete_outside_board)
            .map(|o| {
                poly::difference(o, &poly::union(cutouts.iter().flatten().cloned().collect()))
            });
        // Without an edge rule, silkscreen that must stay on the board still keeps its pen
        // off the edge.
        let edge = rule_edge.or(board.as_ref().map(|_| Nm::ZERO));
        let edges = outline.into_iter().chain(cutouts).collect();
        Env {
            options,
            targets: sided,
            mask,
            edges,
            edge,
            board,
            forbidden: RefCell::new(BTreeMap::new()),
            offsets: offsets(options.max_distance),
        }
    }

    /// Where the centre line of a pen `half` wide may not go on `side`.
    fn forbidden(&self, side: Side, half: i64) -> std::rc::Rc<Cuts> {
        let key = (side_index(side), half);
        if let Some(cuts) = self.forbidden.borrow().get(&key) {
            return cuts.clone();
        }
        let mut shapes = Vec::new();
        if let Some(mask) = self.mask {
            for target in &self.targets[key.0] {
                shapes.extend(poly::dilated(target, mask.0 + half + MARGIN));
            }
        }
        if let Some(edge) = self.edge {
            for area in &self.edges {
                shapes.extend(poly::edge_band(area, edge.0 + half + MARGIN));
            }
        }
        let cuts = std::rc::Rc::new(Cuts::new(poly::union(shapes)));
        self.forbidden.borrow_mut().insert(key, cuts.clone());
        cuts
    }

    /// The pieces a drawing on `side` with a pen `width` wide becomes, or `None` when it
    /// stays as it is.
    fn clip(&self, side: Side, width: Nm, drawn: &Drawn) -> Option<Vec<Piece>> {
        let cuts = self.forbidden(side, width.0 / 2);
        let board = self.board.as_ref();
        let min = self.options.min_remaining_length.0 as f64;
        match drawn {
            Drawn::Open(curve) => {
                let stretches = kept(curve, &cuts, board);
                let whole = stretches == [(0.0, 1.0)];
                let pieces: Vec<Piece> = stretches
                    .into_iter()
                    .map(|s| Piece::of(curve, s))
                    .filter(|p| p.length() >= min)
                    .collect();
                (!whole || pieces.is_empty()).then_some(pieces)
            }
            Drawn::Closed(curves) => {
                let stretches: Vec<Vec<(f64, f64)>> =
                    curves.iter().map(|c| kept(c, &cuts, board)).collect();
                if stretches.iter().all(|s| s == &[(0.0, 1.0)]) {
                    return None;
                }
                let mut pieces = Vec::new();
                for (curve, mut kept) in curves.iter().zip(stretches) {
                    // A circle cut where it starts joins its last stretch to its first.
                    if curves.len() == 1 && kept.len() > 1 {
                        let (first, last) = (kept[0], kept[kept.len() - 1]);
                        if first.0 == 0.0 && last.1 == 1.0 {
                            kept.remove(0);
                            let n = kept.len();
                            kept[n - 1].1 = 1.0 + first.1;
                        }
                    }
                    pieces.extend(kept.into_iter().map(|s| Piece::of(curve, s)));
                }
                pieces.retain(|p| p.length() >= min);
                Some(pieces)
            }
        }
    }

    /// Whether text covering `area` on `side` is clear where it is.
    fn clear(&self, side: Side, area: &Area) -> bool {
        if self.forbidden(side, 0).overlaps(area) {
            return false;
        }
        let first = area.iter().flatten().flatten().next();
        match (&self.board, first) {
            (Some(board), Some(p)) => inside(board, poly::point(*p)),
            _ => true,
        }
    }

    /// Where text covering `area` on `side` goes.
    fn place(&self, side: Side, area: &Area) -> Placement {
        if area.is_empty() || self.clear(side, area) {
            return Placement::Clear;
        }
        if !self.options.move_text {
            return Placement::Stuck;
        }
        for &by in &self.offsets {
            if self.clear(side, &moved(area, by)) {
                return Placement::Moved(by);
            }
        }
        Placement::Stuck
    }
}

/// The places to try for a text, nearest first: the 0.1 mm grid points within `max` of
/// where it is, ties broken by angle.
fn offsets(max: Nm) -> Vec<Point> {
    let n = max.0 / SEARCH_STEP;
    let mut out: Vec<(i64, f64, Point)> = Vec::new();
    for i in -n..=n {
        for j in -n..=n {
            let d2 = i * i + j * j;
            if d2 == 0 || d2 > n * n {
                continue;
            }
            let angle = (j as f64).atan2(i as f64).rem_euclid(TAU);
            let by = Point::new(Nm(i * SEARCH_STEP), Nm(j * SEARCH_STEP));
            out.push((d2, angle, by));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    out.into_iter().map(|(_, _, p)| p).collect()
}

fn moved(area: &Area, by: Point) -> Area {
    area.iter()
        .map(|shape| {
            shape
                .iter()
                .map(|ring| {
                    ring.iter()
                        .map(|p| poly::ip([p.x + by.x.0, p.y + by.y.0]))
                        .collect()
                })
                .collect()
        })
        .collect()
}

fn radians(sweep: Millideg) -> f64 {
    (sweep.0 as f64 / 1000.0).to_radians()
}

/// The centre of the arc from `from` to `to` sweeping `sweep`.
fn arc_center(from: Point, to: Point, sweep: Millideg) -> Point {
    let theta = radians(sweep);
    let (fx, fy) = (from.x.0 as f64, from.y.0 as f64);
    let (tx, ty) = (to.x.0 as f64, to.y.0 as f64);
    let (cx, cy) = (tx - fx, ty - fy);
    let chord = cx.hypot(cy);
    if theta == 0.0 || chord == 0.0 {
        return from;
    }
    let offset = chord / 2.0 / (theta / 2.0).tan();
    let (mx, my) = ((fx + tx) / 2.0, (fy + ty) / 2.0);
    to_point([mx - cy / chord * offset, my + cx / chord * offset])
}
