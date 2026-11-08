//! Closed loops from graphics: what every `add_from_graphics` Command shares (the Board
//! Outline, Board Cutouts, Zones, Zone Cutouts and Keepouts). Free lines, arcs and
//! outlined rectangles become one closed contour, along their centrelines (ends meeting
//! within [`GAP_TOLERANCE`]) or along the outer edge of their strokes.

use reda_command::{CommandError, Context, ErrorCode};
use reda_model::geom::{self, ContourError, Nm, Point, Segment, Vertex};
use reda_model::{GraphicShape, PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::copper::shape::{Core, Prim, Shape};
use crate::zone::poly;

/// How far apart line and arc ends may be and still meet: 0.001 mm.
pub const GAP_TOLERANCE: Nm = Nm(1_000);

/// Which edge of the graphics the contour follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LoopMode {
    /// Along the lines' centres; their ends must meet within 0.001 mm.
    #[default]
    Centerline,
    /// Along the outside of the strokes, their widths included, so ends that overlap
    /// by their width close the loop too.
    OuterEdge,
}

/// The params every `add_from_graphics` Command takes.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FromGraphics {
    /// Free lines, arcs and outlined rectangles forming one closed loop, such as the
    /// selection.
    pub graphics: Vec<Uuid>,
    /// `centerline` (default): along the lines' centres, ends meeting within 0.001 mm.
    /// `outer_edge`: along the outside of the strokes.
    #[serde(default)]
    pub mode: LoopMode,
    /// Keep the graphics as well; by default they are removed.
    #[serde(default)]
    pub keep: bool,
}

impl FromGraphics {
    /// The closed contour the graphics make. A gap is refused (`rejected`) with its
    /// location and size in `details.gap`, and `details.retry` suggesting `outer_edge`.
    pub fn contour(&self, pcb: &PcbDocument) -> Result<Vec<Vertex>, CommandError> {
        let strokes = strokes(pcb, &self.graphics)?;
        match self.mode {
            LoopMode::Centerline => centerline(&strokes),
            LoopMode::OuterEdge => outer_edge(&strokes),
        }
    }

    /// Removes the graphics unless `keep` is set.
    pub fn consume(&self, cx: &mut Context<'_>) -> Result<(), CommandError> {
        if !self.keep {
            let tx = cx.tx()?;
            for uuid in &self.graphics {
                tx.remove(*uuid)?;
            }
        }
        Ok(())
    }
}

/// One graphic's centreline segment and its pen width.
struct Stroke {
    segment: Segment,
    width: Nm,
}

fn strokes(pcb: &PcbDocument, graphics: &[Uuid]) -> Result<Vec<Stroke>, CommandError> {
    if graphics.is_empty() {
        return Err(CommandError::invalid_params(
            "/graphics",
            "name the lines, arcs or rectangles that form the loop",
        ));
    }
    let mut out = Vec::new();
    for (i, uuid) in graphics.iter().enumerate() {
        let refuse = || {
            CommandError::invalid_params(
                &format!("/graphics/{i}"),
                format!("{uuid} is not a free line, arc or rectangle outline"),
            )
        };
        let graphic = pcb.graphic(*uuid).ok_or_else(refuse)?;
        let width = graphic.width;
        let mut push = |segment| out.push(Stroke { segment, width });
        match graphic.shape {
            GraphicShape::Line { from, to } => push(Segment::line(from, to)),
            GraphicShape::Arc { from, to, sweep } => push(Segment::arc(from, to, sweep)),
            GraphicShape::Rect {
                from,
                to,
                filled: false,
            } => {
                let (b, d) = (Point::new(to.x, from.y), Point::new(from.x, to.y));
                for (a, z) in [(from, b), (b, to), (to, d), (d, from)] {
                    push(Segment::line(a, z));
                }
            }
            GraphicShape::Rect { .. } => return Err(refuse()),
        }
    }
    Ok(out)
}

fn centerline(strokes: &[Stroke]) -> Result<Vec<Vertex>, CommandError> {
    let segments: Vec<Segment> = strokes.iter().map(|s| s.segment).collect();
    geom::chain_within(&segments, GAP_TOLERANCE).map_err(|e| match e {
        ContourError::Gap { at, size } => CommandError {
            details: Some(json!({
                "gap": {"at": {"x": at.x.0, "y": at.y.0}, "size": size.0},
                "retry": {"mode": "outer_edge"},
            })),
            ..CommandError::new(
                ErrorCode::Rejected,
                format!(
                    "the graphics do not close: a gap of {} mm at ({} mm, {} mm); \
                     try mode outer_edge",
                    mm(size.0),
                    mm(at.x.0),
                    mm(at.y.0)
                ),
            )
        },
        e => CommandError::invalid_params("/graphics", e.to_string()),
    })
}

fn mm(nm: i64) -> String {
    let text = format!("{:.6}", nm as f64 / 1e6);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The outside of the strokes' union, which must enclose an area (have a hole).
fn outer_edge(strokes: &[Stroke]) -> Result<Vec<Vertex>, CommandError> {
    let mut shapes = Vec::new();
    for s in strokes {
        let radius = s.width.0 / 2;
        let points = if s.segment.sweep == geom::Millideg::ZERO {
            vec![s.segment.from, s.segment.to]
        } else {
            geom::grown::arc_points(s.segment.from, s.segment.to, s.segment.sweep)
        };
        let prims = points
            .windows(2)
            .map(|w| Prim {
                core: Core::Seg([w[0].x.0, w[0].y.0], [w[1].x.0, w[1].y.0]),
                radius,
            })
            .collect();
        shapes.extend(poly::grown(&Shape { prims }, 0));
    }
    let area = poly::union(shapes);
    let refuse = |why: &str| CommandError::rejected(format!("the graphics {why}"));
    let [shape] = area.as_slice() else {
        return Err(refuse("are not one connected loop"));
    };
    if shape.len() < 2 {
        return Err(refuse("enclose no area"));
    }
    Ok(shape[0]
        .iter()
        .map(|p| Vertex::at(Point::new(Nm(p.x), Nm(p.y))))
        .collect())
}
