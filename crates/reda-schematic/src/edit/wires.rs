//! `schematic.wire.break` (cutting a gap out of a Wire) and
//! `schematic.junctions.convert_cross` (turning 4-ways into two 3-ways).

use std::collections::BTreeSet;

use reda_command::{Command, CommandError, Context, Kind, Length, Meta};
use reda_model::geom::{Nm, Point};
use reda_model::{DocumentKind, InDocument, Object, Uuid, Wire};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::selection::resolve;
use crate::component::At;
use crate::normalise::{Seeds, Wiring, Work, apply, normalise};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WireBreakParams {
    /// The Wire to cut, by UUID.
    pub wire: Uuid,
    /// Where to cut: the gap is centred on the point of the Wire nearest to it.
    pub at: At,
    /// How long a gap to cut. Left out, the whole segment goes (Snap To Segment). A gap
    /// as long as the segment or longer takes the whole segment; a shorter one near an
    /// end of the segment slides along it to fit.
    pub length: Option<Length>,
    /// How the Wires this edit touches are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

/// What `schematic.wire.break` left of the Wire.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireBreakResult {
    /// The pieces left, in order along the Wire: the first keeps the Wire's UUID. Empty
    /// when the gap took the whole Wire.
    pub wires: Vec<Uuid>,
}

pub struct WireBreak;

impl Command for WireBreak {
    const META: Meta = Meta {
        id: "schematic.wire.break",
        title: "Break Wire",
        description: "Cuts a gap out of one segment of a Wire, centred where it is nearest \
                      to `at`: the whole segment by default, or `length` of it. The piece \
                      before the gap keeps the Wire's UUID and the piece after gets a new \
                      one.",
        category: "schematic",
        icon: "split",
        kind: Kind::Edit,
    };
    type Params = WireBreakParams;
    type Output = WireBreakResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: WireBreakParams,
    ) -> Result<WireBreakResult, CommandError> {
        let project = cx.project()?;
        let [object] = &resolve(project, &[p.wire], "/wire")?[..] else {
            unreachable!("one UUID resolves to one object")
        };
        let Object::Wire(wire) = object else {
            return Err(CommandError::invalid_params(
                "/wire",
                format!("{} is not a Wire", p.wire),
            ));
        };
        if p.length.is_some_and(|l| l.nm() <= Nm::ZERO) {
            return Err(CommandError::invalid_params(
                "/length",
                "the gap must have some length",
            ));
        }
        let document = wire.document;
        let before = crate::sheet(project, document)?.clone();
        let (first, second) = cut(&wire.object, p.at.point(), p.length.map(Length::nm));
        let mut pieces = [first, second]
            .into_iter()
            .filter_map(|v| Wire::through(p.wire, v))
            .map(|w| w.vertices);
        let tx = cx.tx()?;
        let mut made = vec![p.wire];
        match pieces.next() {
            None => {
                tx.remove(p.wire)?;
                made.clear();
            }
            Some(vertices) => {
                tx.replace(Object::Wire(InDocument::new(
                    document,
                    Wire {
                        uuid: p.wire,
                        vertices,
                    },
                )))?;
                if let Some(vertices) = pieces.next() {
                    let uuid = cx.new_id();
                    made.push(uuid);
                    cx.tx()?.insert(Object::Wire(InDocument::new(
                        document,
                        Wire { uuid, vertices },
                    )))?;
                }
            }
        }
        let lineage = normalise(cx, document, &before, &p.wiring.unwrap_or_default())?;
        Ok(WireBreakResult {
            wires: lineage.trace_all(&made),
        })
    }
}

/// `wire` with a gap of `length` (the whole segment when `None`) cut out of the segment
/// nearest to `at`, centred on its point nearest to `at`: the vertices before the gap
/// and after it. Either may be left with no length.
fn cut(wire: &Wire, at: Point, length: Option<Nm>) -> (Vec<Point>, Vec<Point>) {
    let v = &wire.vertices;
    let distance = |a: Point, b: Point| {
        let (t, length) = along(a, b, at);
        let p = point_at(a, b, t, length);
        let (dx, dy) = ((p.x - at.x).0 as f64, (p.y - at.y).0 as f64);
        dx.hypot(dy)
    };
    let i = (0..v.len() - 1)
        .min_by(|&i, &j| distance(v[i], v[i + 1]).total_cmp(&distance(v[j], v[j + 1])))
        .expect("a Wire has a segment");
    let (a, b) = (v[i], v[i + 1]);
    let (t, segment) = along(a, b, at);
    let (from, to) = match length {
        None => (0.0, segment),
        Some(gap) => {
            let gap = (gap.0 as f64).min(segment);
            let start = (t - gap / 2.0).clamp(0.0, segment - gap);
            (start, start + gap)
        }
    };
    let first = v[..=i]
        .iter()
        .copied()
        .chain([point_at(a, b, from, segment)])
        .collect();
    let second = [point_at(a, b, to, segment)]
        .into_iter()
        .chain(v[i + 1..].iter().copied())
        .collect();
    (first, second)
}

/// How far along the segment from `a` to `b` the point nearest to `p` is, and the
/// segment's length.
fn along(a: Point, b: Point, p: Point) -> (f64, f64) {
    let (dx, dy) = ((b.x - a.x).0 as f64, (b.y - a.y).0 as f64);
    let length = dx.hypot(dy);
    if length == 0.0 {
        return (0.0, 0.0);
    }
    let t = ((p.x - a.x).0 as f64 * dx + (p.y - a.y).0 as f64 * dy) / length;
    (t.clamp(0.0, length), length)
}

/// The point `t` along the segment from `a` to `b` of `length`, to the nearest nanometre;
/// exactly `a` or `b` at the ends.
fn point_at(a: Point, b: Point, t: f64, length: f64) -> Point {
    if t <= 0.0 || length == 0.0 {
        return a;
    }
    if t >= length {
        return b;
    }
    let k = t / length;
    let x = a.x.0 as f64 + (b.x - a.x).0 as f64 * k;
    let y = a.y.0 as f64 + (b.y - a.y).0 as f64 * k;
    Point::new(Nm(x.round() as i64), Nm(y.round() as i64))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConvertCrossParams {
    /// The sheet to convert on; every Schematic Sheet of the Project when left out.
    pub document: Option<Uuid>,
    /// Selected Objects: only the 4-ways whose every Wire is among these. Every 4-way
    /// when left out (All).
    pub objects: Option<Vec<Uuid>>,
    /// The Miter Size: how far along the crossing Wire the moved end goes, and how far
    /// back from the 4-way its corner turns. One snap grid (`wiring.snap_grid`, 50 mil)
    /// by default.
    pub miter: Option<Length>,
    /// How the Wires this edit touches are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

/// One 4-way `schematic.junctions.convert_cross` converted.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConvertedJunction {
    /// The sheet it is on.
    pub document: Uuid,
    /// Where the 4-way was.
    pub at: Point,
}

/// What `schematic.junctions.convert_cross` converted.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConvertCrossResult {
    /// The 4-ways converted, by sheet in Project order, then by point.
    pub converted: Vec<ConvertedJunction>,
}

pub struct ConvertCross;

impl Command for ConvertCross {
    const META: Meta = Meta {
        id: "schematic.junctions.convert_cross",
        title: "Convert Cross Junctions",
        description: "Turns each 4-way (four or more Wire branches meeting where a Wire \
                      ends) into two adjacent 3-ways: one Wire ending there moves along a \
                      crossing horizontal or vertical Wire by the Miter Size, with a mitred \
                      corner. On one sheet or the whole Project, for every 4-way or only \
                      those whose Wires are all selected.",
        category: "schematic",
        icon: "wire",
        kind: Kind::Edit,
    };
    type Params = ConvertCrossParams;
    type Output = ConvertCrossResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ConvertCrossParams,
    ) -> Result<ConvertCrossResult, CommandError> {
        let wiring = p.wiring.unwrap_or_default();
        let miter = p.miter.map_or(wiring.snap_grid.nm(), Length::nm);
        if miter <= Nm::ZERO {
            return Err(CommandError::invalid_params(
                "/miter",
                "the Miter Size must be positive",
            ));
        }
        let project = cx.project()?;
        let sheets: Vec<Uuid> = match p.document {
            Some(document) => {
                crate::sheet(project, document)?;
                vec![document]
            }
            None => (project.documents())
                .filter(|d| d.kind == DocumentKind::Schematic)
                .map(|d| d.uuid)
                .collect(),
        };
        if let Some(objects) = &p.objects {
            resolve(project, objects, "/objects")?;
        }
        let selected: Option<BTreeSet<Uuid>> = p.objects.map(|o| o.into_iter().collect());
        let mut converted = vec![];
        for document in sheets {
            let before = crate::sheet(cx.project()?, document)?.clone();
            let all = Seeds::all(&before).wires;
            let mut work = Work::new(&before, all, BTreeSet::new(), BTreeSet::new());
            let points: BTreeSet<Point> = (work.four_ways().into_iter())
                .filter(|&at| {
                    selected
                        .as_ref()
                        .is_none_or(|s| work.wires_at(at).iter().all(|w| s.contains(w)))
                })
                .collect();
            let done = work.convert_cross(&points, miter);
            if done.is_empty() {
                continue;
            }
            apply(cx, document, &before, &work)?;
            let rest = Wiring {
                convert_cross_junctions: false,
                ..wiring
            };
            normalise(cx, document, &before, &rest)?;
            converted.extend(
                done.into_iter()
                    .map(|at| ConvertedJunction { document, at }),
            );
        }
        Ok(ConvertCrossResult { converted })
    }
}
