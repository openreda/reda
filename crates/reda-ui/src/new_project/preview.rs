//! The creation dialog's live preview: a static 2D top view of a Board Template's frame
//! (`library.board_template.preview`), drawn with egui's painter rather than the canvas
//! (docs/spec/v0.2.5/creation-dialog.md#preview).
//!
//! It is drawn in two steps. [`draw_list`] turns a frame into a list of [`Mark`]s in
//! screen points, each naming the [`Ink`] it is drawn in; [`paint`] draws them with the
//! colours of a [`Look`]. The list is plain data, so tests snapshot it as text
//! ([`to_text`]).

use std::fmt::Write as _;

use egui::epaint::{Mesh, PathShape, PathStroke, TextShape};
use egui::{
    Align2, Color32, Pos2, Rect, Role, Sense, Shape, Stroke, Ui, Vec2, WidgetInfo, pos2, vec2,
};
use reda_engine::library::board_template::{Frame, FrameDrawing, FramePad};
use reda_engine::model::geom::{Bounds, Millideg, Nm, Point, Vertex, arc_centre, contour_bounds};
use reda_engine::model::{DrawingKind, KeepoutShapeKind, Side};
use reda_engine::render::{BoardColor, ColorKey, Palette};

use crate::theme::TokensExt as _;
use crate::tr;

/// The room kept round the board for the computed dimensions, in points: this share of
/// the preview's height, within these bounds.
const MARGIN_SHARE: f32 = 0.16;
const MARGIN: std::ops::RangeInclusive<f32> = 26.0..=46.0;
/// How far a computed dimension line sits from the board edge, as a share of the margin.
const DIMENSION_GAP: f32 = 0.4;
/// The length of a dimension's arrow head.
const ARROW: f32 = 6.0;
/// The widths lines are drawn at, in points.
const OUTLINE_WIDTH: f32 = 1.5;
const PLATED_WIDTH: f32 = 4.0;
const THIN: f32 = 1.0;
/// The radius of the ring that marks a problem's location.
const PROBLEM_RING: f32 = 7.0;
/// How far a connector body reaches past its pads, in mm.
const BODY_MARGIN: f64 = 0.6;
/// Arcs are drawn in steps of at most this many degrees.
const ARC_STEP: f64 = 10.0;

/// What a mark is drawn in. [`Look`] gives each its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    /// The board's fill.
    Board,
    /// The Board Outline's line.
    Outline,
    /// A Board Cutout: filled with the background, drawn with the outline's line.
    Cutout,
    /// A drilled hole.
    Hole,
    Keepout,
    /// Copper on the top side.
    Top,
    /// Copper on the bottom side.
    Bottom,
    /// A Plated Edge, in the top copper's colour.
    Plated,
    /// The template's own drawings, muted.
    Drawing,
    /// The preview's computed dimensions.
    Dimension,
    /// Where a problem of an invalid frame is.
    Problem,
}

impl Ink {
    fn name(self) -> &'static str {
        match self {
            Ink::Board => "board",
            Ink::Outline => "outline",
            Ink::Cutout => "cutout",
            Ink::Hole => "hole",
            Ink::Keepout => "keepout",
            Ink::Top => "top",
            Ink::Bottom => "bottom",
            Ink::Plated => "plated",
            Ink::Drawing => "drawing",
            Ink::Dimension => "dimension",
            Ink::Problem => "problem",
        }
    }
}

/// One thing the preview draws, in screen points.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// A filled simple polygon.
    Fill {
        ink: Ink,
        points: Vec<Pos2>,
        faint: bool,
    },
    /// A polyline; `closed` joins its ends, `dashed` draws it dashed.
    Line {
        ink: Ink,
        points: Vec<Pos2>,
        width: f32,
        closed: bool,
        dashed: bool,
        faint: bool,
    },
    Circle {
        ink: Ink,
        center: Pos2,
        radius: f32,
        filled: bool,
        faint: bool,
    },
    /// A label; `vertical` reads bottom to top.
    Text {
        ink: Ink,
        at: Pos2,
        text: String,
        align: Align2,
        vertical: bool,
    },
}

/// A board point in mm, y up.
type Mm = (f64, f64);

/// Maps board millimetres (y up) into the preview's rect.
struct Fit {
    center: Pos2,
    mid: (f64, f64),
    scale: f64,
}

impl Fit {
    fn new(rect: Rect, bounds: Bounds) -> Fit {
        let inner = rect.shrink(margin(rect));
        let w = mm(bounds.width()).max(1.0);
        let h = mm(bounds.height()).max(1.0);
        let scale =
            (f64::from(inner.width().max(1.0)) / w).min(f64::from(inner.height().max(1.0)) / h);
        let c = bounds.center();
        Fit {
            center: rect.center(),
            mid: (mm(c.x), mm(c.y)),
            scale,
        }
    }

    fn at(&self, (x, y): (f64, f64)) -> Pos2 {
        pos2(
            self.center.x + ((x - self.mid.0) * self.scale) as f32,
            self.center.y - ((y - self.mid.1) * self.scale) as f32,
        )
    }

    fn point(&self, p: Point) -> Pos2 {
        self.at((mm(p.x), mm(p.y)))
    }

    fn len(&self, n: Nm) -> f32 {
        (mm(n) * self.scale) as f32
    }
}

/// The room kept round the board in `rect`.
fn margin(rect: Rect) -> f32 {
    (rect.height() * MARGIN_SHARE).clamp(*MARGIN.start(), *MARGIN.end())
}

fn mm(n: Nm) -> f64 {
    n.0 as f64 / 1e6
}

/// The points along a closed contour, arcs split into short steps.
fn contour_points(vertices: &[Vertex]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for (i, v) in vertices.iter().enumerate() {
        let to = vertices[(i + 1) % vertices.len()].point();
        out.extend(edge_points(v.point(), to, v.sweep));
        out.pop();
    }
    out
}

/// The points of one edge from `from` to `to`, both ends included.
fn edge_points(from: Point, to: Point, sweep: Millideg) -> Vec<(f64, f64)> {
    let a = (mm(from.x), mm(from.y));
    let b = (mm(to.x), mm(to.y));
    if sweep == Millideg::ZERO || from == to {
        return vec![a, b];
    }
    let c = arc_centre(from, to, sweep);
    let c = (mm(c.x), mm(c.y));
    let r = (a.0 - c.0).hypot(a.1 - c.1);
    let start = (a.1 - c.1).atan2(a.0 - c.0);
    let degrees = sweep.0 as f64 / 1000.0;
    let steps = (degrees.abs() / ARC_STEP).ceil().max(2.0) as usize;
    let mut out: Vec<_> = (0..steps)
        .map(|i| {
            let angle = start + (degrees * i as f64 / steps as f64).to_radians();
            (c.0 + r * angle.cos(), c.1 + r * angle.sin())
        })
        .collect();
    out.push(b);
    out
}

fn pad_points(pad: &FramePad) -> Vec<(f64, f64)> {
    contour_points(&pad.outline)
}

/// The box everything the preview draws fits in.
fn frame_bounds(frame: &Frame) -> Option<Bounds> {
    let mut b = contour_bounds(&frame.outline.contour())?;
    let mut add = |p: Point| b = b.union(Bounds::of_point(p));
    let pads = (frame.connectors.iter().flat_map(|c| &c.pads))
        .chain(frame.holes.iter().flat_map(|h| &h.pads));
    for pad in pads {
        pad.outline.iter().for_each(|v| add(v.point()));
    }
    for d in visible_drawings(frame) {
        for p in [d.from, d.to, d.position].into_iter().flatten() {
            add(p);
        }
        for end in [d.a, d.b].into_iter().flatten() {
            add(end.point);
        }
    }
    Some(b)
}

fn visible_drawings(frame: &Frame) -> impl Iterator<Item = &FrameDrawing> {
    (frame.drawing_groups.iter())
        .filter(|g| !g.excluded)
        .flat_map(|g| &g.drawings)
        .filter(|d| !d.excluded)
}

/// The marks that draw `frame` fitted into `rect`: the board, its cutouts, Plated Edges
/// and Keepouts, holes and connectors (excluded ones faint), the template's drawings
/// muted, the computed width, height and key datum, and a ring on each located problem.
pub fn draw_list(frame: &Frame, rect: Rect) -> Vec<Mark> {
    let mut marks = Vec::new();
    let Some(bounds) = frame_bounds(frame) else {
        return marks;
    };
    let fit = Fit::new(rect, bounds);
    let pts = |v: &[(f64, f64)]| v.iter().map(|p| fit.at(*p)).collect::<Vec<_>>();

    // The board, its cutouts and its outline.
    let outline = pts(&contour_points(&frame.outline.contour()));
    marks.push(Mark::Fill {
        ink: Ink::Board,
        points: outline.clone(),
        faint: false,
    });
    for cutout in &frame.cutouts {
        let points = pts(&contour_points(&cutout.contour()));
        marks.push(Mark::Fill {
            ink: Ink::Cutout,
            points: points.clone(),
            faint: false,
        });
        marks.push(line(Ink::Outline, points, OUTLINE_WIDTH, true));
    }
    marks.push(line(Ink::Outline, outline, OUTLINE_WIDTH, true));
    for contour in std::iter::once(&frame.outline).chain(&frame.cutouts) {
        let n = contour.vertices.len();
        for (i, v) in contour
            .vertices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.plated)
        {
            let to = contour.vertices[(i + 1) % n];
            let edge = edge_points(v.vertex().point(), to.vertex().point(), v.sweep);
            marks.push(line(Ink::Plated, pts(&edge), PLATED_WIDTH, false));
        }
    }

    // Keepouts that go with an excluded object are gone with it.
    for keepout in frame.keepouts.iter().filter(|k| !k.excluded) {
        match keepout.shape {
            KeepoutShapeKind::Polygon => {
                let points = pts(&contour_points(&keepout.vertices));
                marks.push(Mark::Fill {
                    ink: Ink::Keepout,
                    points: points.clone(),
                    faint: false,
                });
                marks.push(Mark::Line {
                    ink: Ink::Keepout,
                    points,
                    width: THIN,
                    closed: true,
                    dashed: true,
                    faint: false,
                });
            }
            KeepoutShapeKind::Line => {
                let points: Vec<_> = keepout
                    .vertices
                    .iter()
                    .map(|v| fit.point(v.point()))
                    .collect();
                let width = keepout.width.map_or(THIN, |w| fit.len(w).max(THIN));
                marks.push(line(Ink::Keepout, points, width, false));
            }
        }
    }

    // Holes: their pads in copper and the drill over them.
    for hole in &frame.holes {
        let faint = hole.excluded;
        let ink = side_ink(hole.side);
        for pad in &hole.pads {
            marks.push(Mark::Fill {
                ink,
                points: pts(&pad_points(pad)),
                faint,
            });
            if let Some(drill) = pad.hole {
                marks.push(Mark::Circle {
                    ink: Ink::Hole,
                    center: fit.point(pad.at),
                    radius: fit.len(drill.width.min(drill.height)) / 2.0,
                    filled: true,
                    faint,
                });
            }
        }
    }

    // Connectors: pads and a body round them, solid on top and dashed on the bottom.
    for (n, connector) in frame.connectors.iter().enumerate() {
        let faint = connector.excluded;
        let ink = side_ink(Some(connector.side));
        let mut lo = (f64::MAX, f64::MAX);
        let mut hi = (f64::MIN, f64::MIN);
        for pad in &connector.pads {
            let points = pad_points(pad);
            for p in &points {
                lo = (lo.0.min(p.0), lo.1.min(p.1));
                hi = (hi.0.max(p.0), hi.1.max(p.1));
            }
            marks.push(Mark::Fill {
                ink,
                points: pts(&points),
                faint,
            });
            if let Some(drill) = pad.hole {
                marks.push(Mark::Circle {
                    ink: Ink::Hole,
                    center: fit.point(pad.at),
                    radius: fit.len(drill.width.min(drill.height)) / 2.0,
                    filled: true,
                    faint,
                });
            }
        }
        if connector.pads.is_empty() {
            continue;
        }
        let (lo, hi) = (
            (lo.0 - BODY_MARGIN, lo.1 - BODY_MARGIN),
            (hi.0 + BODY_MARGIN, hi.1 + BODY_MARGIN),
        );
        let body = pts(&[lo, (hi.0, lo.1), hi, (lo.0, hi.1)]);
        let top = body.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        let mid = body.iter().map(|p| p.x).sum::<f32>() / 4.0;
        marks.push(Mark::Line {
            ink,
            points: body,
            width: THIN,
            closed: true,
            dashed: connector.side == Side::Bottom,
            faint,
        });
        if !faint {
            marks.push(Mark::Text {
                ink,
                at: pos2(mid, top - 2.0),
                text: format!("J{}", n + 1),
                align: Align2::CENTER_BOTTOM,
                vertical: false,
            });
        }
    }

    for d in visible_drawings(frame) {
        drawing(&mut marks, &fit, d);
    }

    // The preview's own dimensions, over the template's.
    if let Some(b) = contour_bounds(&frame.outline.contour()) {
        let (x0, y0, x1, y1) = (mm(b.min.x), mm(b.min.y), mm(b.max.x), mm(b.max.y));
        let gap = margin(rect) * DIMENSION_GAP;
        let below = Toward::Below(gap);
        dimension(
            &mut marks,
            fit.at((x0, y0)),
            fit.at((x1, y0)),
            x1 - x0,
            below,
        );
        let left = Toward::Left(gap);
        dimension(
            &mut marks,
            fit.at((x0, y0)),
            fit.at((x0, y1)),
            y1 - y0,
            left,
        );
        if let Some((a, b, value)) = key_datum(frame) {
            let (pa, pb) = (fit.at(a), fit.at(b));
            if (a.0 - b.0).abs() >= (a.1 - b.1).abs() {
                let y = fit.at((0.0, y1)).y;
                dimension(
                    &mut marks,
                    pos2(pa.x, y),
                    pos2(pb.x, y),
                    value,
                    Toward::Above(gap),
                );
            } else {
                let x = fit.at((x1, 0.0)).x;
                dimension(
                    &mut marks,
                    pos2(x, pa.y),
                    pos2(x, pb.y),
                    value,
                    Toward::Right(gap),
                );
            }
        }
    }

    for problem in &frame.problems {
        if let Some(at) = problem.at {
            marks.push(Mark::Circle {
                ink: Ink::Problem,
                center: fit.point(at),
                radius: PROBLEM_RING,
                filled: false,
                faint: false,
            });
        }
    }
    marks
}

fn line(ink: Ink, points: Vec<Pos2>, width: f32, closed: bool) -> Mark {
    Mark::Line {
        ink,
        points,
        width,
        closed,
        dashed: false,
        faint: false,
    }
}

fn side_ink(side: Option<Side>) -> Ink {
    match side {
        Some(Side::Bottom) => Ink::Bottom,
        _ => Ink::Top,
    }
}

/// The template's key datum: the first Dimension that follows an included connector,
/// such as PCIe's key from the bracket edge or the Pi's GPIO header from a hole. Its two
/// ends in mm and the distance it measures along its main direction.
fn key_datum(frame: &Frame) -> Option<(Mm, Mm, f64)> {
    let connector = |uuid| {
        frame
            .connectors
            .iter()
            .any(|c| c.uuid == uuid && !c.excluded)
    };
    visible_drawings(frame)
        .filter(|d| d.kind == DrawingKind::Dimension)
        .find_map(|d| {
            let (a, b) = (d.a?, d.b?);
            let follows = [a, b]
                .iter()
                .any(|end| end.follow.is_some_and(|f| connector(f.object)));
            follows.then(|| {
                let (a, b) = (
                    (mm(a.point.x), mm(a.point.y)),
                    (mm(b.point.x), mm(b.point.y)),
                );
                let value = (a.0 - b.0).abs().max((a.1 - b.1).abs());
                (a, b, value)
            })
        })
}

/// Which side of the measured points a dimension line is drawn on.
#[derive(Clone, Copy)]
enum Toward {
    Below(f32),
    Left(f32),
    Above(f32),
    Right(f32),
}

/// A computed dimension from `a` to `b` (screen points along one axis): extension lines,
/// the line with an arrow at each end, and `value` in mm.
fn dimension(marks: &mut Vec<Mark>, a: Pos2, b: Pos2, value: f64, toward: Toward) {
    let n = match toward {
        Toward::Below(gap) => vec2(0.0, gap),
        Toward::Above(gap) => vec2(0.0, -gap),
        Toward::Left(gap) => vec2(-gap, 0.0),
        Toward::Right(gap) => vec2(gap, 0.0),
    };
    let (a2, b2) = (a + n, b + n);
    for (from, to) in [(a, a2), (b, b2)] {
        marks.push(line(
            Ink::Dimension,
            vec![from + n * 0.2, to + n * 0.25],
            THIN,
            false,
        ));
    }
    marks.push(line(Ink::Dimension, vec![a2, b2], THIN, false));
    if (b2 - a2).length() > 2.0 * ARROW {
        for (tip, from) in [(a2, b2), (b2, a2)] {
            let d = (from - tip).normalized() * ARROW;
            let side = vec2(-d.y, d.x) * 0.5;
            marks.push(line(
                Ink::Dimension,
                vec![tip + d + side, tip, tip + d - side],
                THIN,
                false,
            ));
        }
    }
    let mid = a2 + (b2 - a2) / 2.0;
    let text = format!("{} mm", trim(value));
    let (at, align, vertical) = match toward {
        Toward::Below(_) => (mid + vec2(0.0, 2.0), Align2::CENTER_TOP, false),
        Toward::Above(_) => (mid - vec2(0.0, 2.0), Align2::CENTER_BOTTOM, false),
        Toward::Left(_) => (mid - vec2(2.0, 0.0), Align2::RIGHT_CENTER, true),
        Toward::Right(_) => (mid + vec2(2.0, 0.0), Align2::LEFT_CENTER, true),
    };
    marks.push(Mark::Text {
        ink: Ink::Dimension,
        at,
        text,
        align,
        vertical,
    });
}

/// One of the template's drawings, muted.
fn drawing(marks: &mut Vec<Mark>, fit: &Fit, d: &FrameDrawing) {
    let width = |w: Option<Nm>| w.map_or(THIN, |w| fit.len(w).max(THIN));
    match d.kind {
        DrawingKind::Line | DrawingKind::Arc => {
            let (Some(from), Some(to)) = (d.from, d.to) else {
                return;
            };
            let sweep = d.sweep.unwrap_or(Millideg::ZERO);
            let points = edge_points(from, to, sweep)
                .into_iter()
                .map(|p| fit.at(p))
                .collect();
            marks.push(line(Ink::Drawing, points, width(d.width), false));
        }
        DrawingKind::Rectangle => {
            let (Some(a), Some(b)) = (d.from, d.to) else {
                return;
            };
            let (a, b) = (fit.point(a), fit.point(b));
            let points = vec![a, pos2(b.x, a.y), b, pos2(a.x, b.y)];
            if d.filled {
                marks.push(Mark::Fill {
                    ink: Ink::Drawing,
                    points,
                    faint: false,
                });
            } else {
                marks.push(line(Ink::Drawing, points, width(d.width), true));
            }
        }
        DrawingKind::Text => {
            let (Some(at), Some(text)) = (d.position, &d.text) else {
                return;
            };
            marks.push(Mark::Text {
                ink: Ink::Drawing,
                at: fit.point(at),
                text: text.clone(),
                align: Align2::CENTER_CENTER,
                vertical: d
                    .rotation
                    .is_some_and(|r| r.0.rem_euclid(180_000) == 90_000),
            });
        }
        DrawingKind::Dimension => {
            let (Some(a), Some(b)) = (d.a, d.b) else {
                return;
            };
            let (pa, pb) = (fit.point(a.point), fit.point(b.point));
            marks.push(Mark::Line {
                ink: Ink::Drawing,
                points: vec![pa, pb],
                width: THIN,
                closed: false,
                dashed: true,
                faint: false,
            });
        }
    }
}

/// `value` with at most three decimals and no trailing zeros.
pub fn trim(value: f64) -> String {
    let s = format!("{value:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// The marks as text, one per line, coordinates to a tenth of a point: what the
/// draw-list snapshots hold.
pub fn to_text(marks: &[Mark]) -> String {
    let p = |p: &Pos2| format!("({:.1},{:.1})", p.x, p.y);
    let list = |v: &[Pos2]| v.iter().map(p).collect::<Vec<_>>().join(" ");
    let faint = |f: bool| if f { " faint" } else { "" };
    let mut out = String::new();
    for mark in marks {
        let _ = match mark {
            Mark::Fill {
                ink,
                points,
                faint: f,
            } => {
                writeln!(out, "fill {}{} {}", ink.name(), faint(*f), list(points))
            }
            Mark::Line {
                ink,
                points,
                width,
                closed,
                dashed,
                faint: f,
            } => writeln!(
                out,
                "line {}{}{}{} w={width:.1} {}",
                ink.name(),
                if *closed { " closed" } else { "" },
                if *dashed { " dashed" } else { "" },
                faint(*f),
                list(points)
            ),
            Mark::Circle {
                ink,
                center,
                radius,
                filled,
                faint: f,
            } => writeln!(
                out,
                "circle {}{}{} {} r={radius:.1}",
                ink.name(),
                if *filled { " filled" } else { "" },
                faint(*f),
                p(center)
            ),
            Mark::Text {
                ink,
                at,
                text,
                align,
                vertical,
            } => writeln!(
                out,
                "text {}{} {} {:?} {text:?}",
                ink.name(),
                if *vertical { " vertical" } else { "" },
                p(at),
                align
            ),
        };
    }
    out
}

/// The colours the preview draws in: Layer colours from the canvas palette, chrome from
/// the theme's tokens.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub background: Color32,
    pub board: Color32,
    pub outline: Color32,
    pub hole: Color32,
    pub keepout: Color32,
    pub top: Color32,
    pub bottom: Color32,
    pub drawing: Color32,
    pub dimension: Color32,
    pub problem: Color32,
}

impl Look {
    pub fn new(ui: &Ui, palette: &Palette) -> Look {
        let t = ui.tokens();
        let board = |c: BoardColor| {
            let [r, g, b, _] = palette.color(ColorKey::Board(c)).0;
            crate::widgets::data_color(r, g, b)
        };
        let outline = board(BoardColor::BoardOutline);
        Look {
            background: t.color.surface.well,
            board: outline.gamma_multiply(0.16),
            outline,
            hole: board(BoardColor::Hole),
            keepout: t.color.warning.solid,
            top: board(BoardColor::CopperTop),
            bottom: board(BoardColor::CopperBottom),
            drawing: t.color.text.muted,
            dimension: t.color.text.primary,
            problem: t.color.error.solid,
        }
    }

    fn color(&self, ink: Ink) -> Color32 {
        match ink {
            Ink::Board => self.board,
            Ink::Outline => self.outline,
            Ink::Cutout => self.background,
            Ink::Hole => self.hole,
            Ink::Keepout => self.keepout,
            Ink::Top | Ink::Plated => self.top,
            Ink::Bottom => self.bottom,
            Ink::Drawing => self.drawing,
            Ink::Dimension => self.dimension,
            Ink::Problem => self.problem,
        }
    }

    fn ink(&self, ink: Ink, faint: bool, fill: bool) -> Color32 {
        let c = self.color(ink);
        let c = if ink == Ink::Keepout && fill {
            c.gamma_multiply(0.25)
        } else {
            c
        };
        if faint { c.gamma_multiply(0.2) } else { c }
    }
}

/// The preview of `frame` in a box `size` big, or its empty state with `empty` in it.
pub fn show(ui: &mut Ui, frame: Option<&Frame>, size: Vec2, look: &Look, empty: &str) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let label = frame.map_or_else(
        || empty.to_owned(),
        |f| tr!("new-project-preview", name = f.name.as_str()),
    );
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, label.clone()));
    let t = ui.tokens();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, t.radius.control, look.background);
    match frame {
        Some(frame) => paint(&painter, &draw_list(frame, rect), look, ui),
        None => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                empty,
                t.ty.body.font_id(),
                t.color.text.muted,
            );
        }
    }
}

/// Draws `marks` with `look`'s colours.
pub fn paint(painter: &egui::Painter, marks: &[Mark], look: &Look, ui: &Ui) {
    let t = ui.tokens();
    for mark in marks {
        match mark {
            Mark::Fill { ink, points, faint } => {
                painter.add(fill(points, look.ink(*ink, *faint, true)));
            }
            Mark::Line {
                ink,
                points,
                width,
                closed,
                dashed,
                faint,
            } => {
                let stroke = Stroke::new(*width, look.ink(*ink, *faint, false));
                if *dashed {
                    let mut path = points.clone();
                    if *closed && let Some(first) = points.first() {
                        path.push(*first);
                    }
                    painter.add(Shape::dashed_line(&path, stroke, 4.0, 3.0));
                } else if *closed {
                    painter.add(PathShape::closed_line(points.clone(), stroke));
                } else {
                    painter.add(PathShape::line(points.clone(), PathStroke::from(stroke)));
                }
            }
            Mark::Circle {
                ink,
                center,
                radius,
                filled,
                faint,
            } => {
                let c = look.ink(*ink, *faint, *filled);
                if *filled {
                    painter.circle_filled(*center, *radius, c);
                } else {
                    painter.circle_stroke(*center, *radius, Stroke::new(t.stroke.accent_line, c));
                }
            }
            Mark::Text {
                ink,
                at,
                text,
                align,
                vertical,
            } => {
                let color = look.ink(*ink, false, false);
                let galley = painter.layout_no_wrap(text.clone(), t.ty.small.font_id(), color);
                let size = galley.size();
                if *vertical {
                    // Turned a quarter left: the galley's width runs up the screen.
                    let turned = vec2(size.y, size.x);
                    let r = align.anchor_size(*at, turned);
                    let pos = pos2(r.left(), r.bottom());
                    painter.add(
                        TextShape::new(pos, galley, color).with_angle(-std::f32::consts::FRAC_PI_2),
                    );
                } else {
                    let r = align.anchor_size(*at, size);
                    painter.galley(r.min, galley, color);
                }
            }
        }
    }
}

/// A filled simple polygon, convex or not, by ear clipping.
fn fill(points: &[Pos2], color: Color32) -> Shape {
    let mut mesh = Mesh::default();
    for p in points {
        mesh.colored_vertex(*p, color);
    }
    let area: f32 = (points.iter())
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    let ccw = area > 0.0;
    let mut idx: Vec<usize> = (0..points.len()).collect();
    while idx.len() > 3 {
        let n = idx.len();
        let ear = (0..n).find(|&i| {
            let (ia, ib, ic) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
            let (a, b, c) = (points[ia], points[ib], points[ic]);
            let cross = (b - a).x * (c - b).y - (b - a).y * (c - b).x;
            if (cross > 0.0) != ccw || cross.abs() < 1e-6 {
                return false;
            }
            !(idx.iter()).any(|&j| j != ia && j != ib && j != ic && in_triangle(points[j], a, b, c))
        });
        let Some(i) = ear else {
            // Collinear or degenerate: drop the middle point and go on.
            idx.remove(0);
            continue;
        };
        let (ia, ib, ic) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
        mesh.add_triangle(ia as u32, ib as u32, ic as u32);
        idx.remove(i);
    }
    if idx.len() == 3 {
        mesh.add_triangle(idx[0] as u32, idx[1] as u32, idx[2] as u32);
    }
    Shape::mesh(mesh)
}

fn in_triangle(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let s = |p1: Pos2, p2: Pos2, p3: Pos2| {
        (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
    };
    let (d1, d2, d3) = (s(p, a, b), s(p, b, c), s(p, c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// The legend under the preview: a swatch and a name for each colour.
pub fn legend(ui: &mut Ui, look: &Look) {
    let t = ui.tokens();
    ui.horizontal_wrapped(|ui| {
        for (color, text) in [
            (
                look.board.gamma_multiply(3.0),
                tr!("new-project-legend-board"),
            ),
            (look.outline, tr!("new-project-legend-cutout")),
            (look.hole, tr!("new-project-legend-hole")),
            (look.keepout, tr!("new-project-legend-keepout")),
            (look.top, tr!("new-project-legend-top")),
            (look.bottom, tr!("new-project-legend-bottom")),
            (look.drawing, tr!("new-project-legend-drawings")),
        ] {
            ui.add(crate::widgets::swatch(color));
            crate::settings::layout::muted(ui, text);
            ui.add_space(t.space.gap);
        }
    });
}
