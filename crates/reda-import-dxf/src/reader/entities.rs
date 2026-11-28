//! Each entity type's groups as [`Shape`]s in the world, seen from above (DXF Reference,
//! "ENTITIES Section": LINE, ARC, CIRCLE, ELLIPSE, LWPOLYLINE, POLYLINE and VERTEX, SPLINE,
//! TEXT, MTEXT). `map` places the entity's world coordinates, which an INSERT moves; the
//! entity's own object coordinate system comes first where it has one.

use std::f64::consts::TAU;

use reda_model::{TextHJustify, TextJustify, TextVJustify};

use crate::shape::{Piece, Shape, Vertex, Xy, bulged};
use crate::spline::Spline;
use crate::tags::{ReadError, Tag};
use crate::text::{Text, mtext_value, text_value};
use crate::transform::{Affine, V3, normal, turned};

/// What an entity reads as.
pub(super) enum Read {
    Shapes(Vec<Shape>),
    /// Not drawn, and why, as the report counts it.
    Skipped(String),
}

/// The groups that describe the entity itself: an embedded object (101) and extended data
/// (1000 and up) repeat the entity's codes with other meanings, so reading stops at them.
pub(super) fn groups<'t, 'a>(groups: &'t [Tag<'a>]) -> &'t [Tag<'a>] {
    let end = groups
        .iter()
        .position(|t| t.code == 101 || t.code >= 1000)
        .unwrap_or(groups.len());
    &groups[..end]
}

/// The last value of the real group `code`, or `default`.
pub(super) fn real_or(groups: &[Tag<'_>], code: i32, default: f64) -> Result<f64, ReadError> {
    match groups.iter().rev().find(|t| t.code == code) {
        Some(tag) => tag.real(),
        None => Ok(default),
    }
}

/// The last value of the integer group `code`, or `default`.
pub(super) fn int_or(groups: &[Tag<'_>], code: i32, default: i64) -> Result<i64, ReadError> {
    match groups.iter().rev().find(|t| t.code == code) {
        Some(tag) => tag.int(),
        None => Ok(default),
    }
}

/// The point in groups `code`, `code + 10` and `code + 20`, 0 where one is missing.
pub(super) fn point(groups: &[Tag<'_>], code: i32) -> Result<V3, ReadError> {
    Ok([
        real_or(groups, code, 0.0)?,
        real_or(groups, code + 10, 0.0)?,
        real_or(groups, code + 20, 0.0)?,
    ])
}

fn has(groups: &[Tag<'_>], code: i32) -> bool {
    groups.iter().any(|t| t.code == code)
}

/// The extrusion direction, groups 210–230; +Z when left out.
pub(super) fn extrusion(groups: &[Tag<'_>]) -> Result<V3, ReadError> {
    Ok([
        real_or(groups, 210, 0.0)?,
        real_or(groups, 220, 0.0)?,
        real_or(groups, 230, 1.0)?,
    ])
}

/// Repeated points: each `code` group starts one, and the `code + 10` and `code + 20`
/// after it are its y and z.
fn points(groups: &[Tag<'_>], code: i32) -> Result<Vec<V3>, ReadError> {
    let mut points: Vec<V3> = Vec::new();
    for tag in groups {
        let axis = match tag.code - code {
            0 => 0,
            10 => 1,
            20 => 2,
            _ => continue,
        };
        if axis == 0 {
            points.push([tag.real()?, 0.0, 0.0]);
        } else {
            let Some(point) = points.last_mut() else {
                return Err(tag.error(format!("group {} comes before a point's {code}", tag.code)));
            };
            point[axis] = tag.real()?;
        }
    }
    Ok(points)
}

fn reals(groups: &[Tag<'_>], code: i32) -> Result<Vec<f64>, ReadError> {
    groups
        .iter()
        .filter(|t| t.code == code)
        .map(|t| t.real())
        .collect()
}

pub(super) fn read(
    kind: &str,
    groups: &[Tag<'_>],
    children: &[&[Tag<'_>]],
    map: &Affine,
) -> Result<Read, ReadError> {
    let g = self::groups(groups);
    let one = |shape| Ok(Read::Shapes(vec![shape]));
    match kind {
        "LINE" => {
            let (from, to) = (point(g, 10)?, point(g, 11)?);
            one(Shape::Line {
                from: map.xy(from),
                to: map.xy(to),
            })
        }
        "ARC" | "CIRCLE" => {
            let centre = point(g, 10)?;
            let radius = real_or(g, 40, 0.0)?;
            let (start, end) = if kind == "ARC" {
                (real_or(g, 50, 0.0)?, real_or(g, 51, 0.0)?)
            } else {
                (0.0, 360.0)
            };
            let m = map.of(Affine::ocs(extrusion(g)?));
            if m == Affine::IDENTITY {
                let centre = Xy::new(centre[0], centre[1]);
                return one(if kind == "ARC" {
                    Shape::Arc {
                        centre,
                        radius,
                        start,
                        end,
                    }
                } else {
                    Shape::Circle { centre, radius }
                });
            }
            let sweep = if kind == "ARC" {
                (end - start).rem_euclid(360.0)
            } else {
                0.0
            };
            one(Shape::conic(
                m.xy(centre),
                m.dxy([radius, 0.0, 0.0]),
                m.dxy([0.0, radius, 0.0]),
                start.to_radians(),
                sweep.to_radians(),
            ))
        }
        "ELLIPSE" => {
            let n = normal(extrusion(g)?);
            let major = point(g, 11)?;
            let ratio = real_or(g, 40, 1.0)?;
            let minor = turned(n, major).map(|c| c * ratio);
            let start = real_or(g, 41, 0.0)?;
            let end = real_or(g, 42, TAU)?;
            one(Shape::conic(
                map.xy(point(g, 10)?),
                map.dxy(major),
                map.dxy(minor),
                start,
                end - start,
            ))
        }
        "LWPOLYLINE" => {
            let closed = int_or(g, 70, 0)? & 1 == 1;
            let mut vertices: Vec<Vertex> = Vec::new();
            for tag in g {
                match tag.code {
                    10 => vertices.push(Vertex {
                        at: Xy::new(tag.real()?, 0.0),
                        bulge: 0.0,
                    }),
                    20 | 42 => {
                        let Some(vertex) = vertices.last_mut() else {
                            return Err(
                                tag.error(format!("group {} comes before a vertex's 10", tag.code))
                            );
                        };
                        if tag.code == 20 {
                            vertex.at.y = tag.real()?;
                        } else {
                            vertex.bulge = tag.real()?;
                        }
                    }
                    _ => {}
                }
            }
            let m = map.of(Affine::ocs(extrusion(g)?));
            Ok(Read::Shapes(polyline(
                &m,
                real_or(g, 38, 0.0)?,
                vertices,
                closed,
            )))
        }
        "POLYLINE" => {
            let flags = int_or(g, 70, 0)?;
            if flags & (16 | 64) != 0 {
                return Ok(Read::Skipped("POLYLINE mesh".into()));
            }
            let mut vertices = Vec::new();
            let mut points3 = Vec::new();
            for vertex in children {
                let v = self::groups(vertex);
                // Spline frame control points are not on the curve.
                if int_or(v, 70, 0)? & 16 != 0 {
                    continue;
                }
                let at = point(v, 10)?;
                points3.push(at);
                vertices.push(Vertex {
                    at: Xy::new(at[0], at[1]),
                    bulge: real_or(v, 42, 0.0)?,
                });
            }
            let closed = flags & 1 == 1;
            if flags & 8 != 0 {
                // A 3D polyline: world points, straight segments.
                let vertices = points3
                    .into_iter()
                    .map(|p| Vertex {
                        at: map.xy(p),
                        bulge: 0.0,
                    })
                    .collect();
                return one(Shape::Polyline { vertices, closed });
            }
            let m = map.of(Affine::ocs(extrusion(g)?));
            let elevation = real_or(g, 30, 0.0)?;
            Ok(Read::Shapes(polyline(&m, elevation, vertices, closed)))
        }
        "SPLINE" => spline(g, map),
        "TEXT" => text(g, map),
        "MTEXT" => mtext(g, map),
        _ => unreachable!("only read types come here"),
    }
}

/// A polyline in an object coordinate system `m` at `elevation`. A map that keeps circles
/// circles keeps it one polyline, its bulges turning the other way when it mirrors; any
/// other map gives a line or elliptical arc per segment.
fn polyline(m: &Affine, elevation: f64, vertices: Vec<Vertex>, closed: bool) -> Vec<Shape> {
    if *m == Affine::IDENTITY {
        return vec![Shape::Polyline { vertices, closed }];
    }
    let place = |p: Xy| m.xy([p.x, p.y, elevation]);
    let (a, b) = (m.dxy([1.0, 0.0, 0.0]), m.dxy([0.0, 1.0, 0.0]));
    let (la, lb) = (a.x.hypot(a.y), b.x.hypot(b.y));
    let det = a.x * b.y - a.y * b.x;
    let similar =
        la > 0.0 && (la - lb).abs() <= 1e-9 * la && (a.x * b.x + a.y * b.y).abs() <= 1e-9 * la * la;
    if similar {
        let flip = if det < 0.0 { -1.0 } else { 1.0 };
        let vertices = vertices
            .into_iter()
            .map(|v| Vertex {
                at: place(v.at),
                bulge: v.bulge * flip,
            })
            .collect();
        return vec![Shape::Polyline { vertices, closed }];
    }
    let segments = if closed {
        vertices.len()
    } else {
        vertices.len().saturating_sub(1)
    };
    let mut shapes = Vec::new();
    for i in 0..segments {
        let (from, to) = (vertices[i], vertices[(i + 1) % vertices.len()]);
        if from.at == to.at {
            continue;
        }
        match bulged(from.at, to.at, from.bulge) {
            Piece::Line { from, to } => shapes.push(Shape::Line {
                from: place(from),
                to: place(to),
            }),
            Piece::Arc {
                from,
                sweep,
                centre,
                radius,
                ..
            } => {
                let start = (from.y - centre.y).atan2(from.x - centre.x);
                let sweep = sweep.to_radians();
                let start = if sweep < 0.0 { start + sweep } else { start };
                shapes.push(Shape::conic(
                    place(centre),
                    m.dxy([radius, 0.0, 0.0]),
                    m.dxy([0.0, radius, 0.0]),
                    start,
                    sweep.abs(),
                ));
            }
        }
    }
    shapes
}

/// SPLINE: its control points, knots and weights, or a cubic through its fit points when it
/// has no control points.
fn spline(g: &[Tag<'_>], map: &Affine) -> Result<Read, ReadError> {
    let flags = int_or(g, 70, 0)?;
    let degree = int_or(g, 71, 3)?;
    let controls: Vec<Xy> = points(g, 10)?.into_iter().map(|p| map.xy(p)).collect();
    let mut knots = reals(g, 40)?;
    let weights = reals(g, 41)?;
    let malformed = || Ok(Read::Skipped("malformed SPLINE".into()));
    let spline = if !controls.is_empty() {
        let Ok(degree) = usize::try_from(degree) else {
            return malformed();
        };
        if knots.is_empty() && controls.len() > degree {
            // No knots: clamped and uniform.
            let inner = controls.len() - degree;
            knots = (0..=controls.len() + degree)
                .map(|i| i.saturating_sub(degree).min(inner) as f64)
                .collect();
        }
        Spline::new(degree, knots, controls, weights)
    } else {
        let mut fit: Vec<Xy> = points(g, 11)?.into_iter().map(|p| map.xy(p)).collect();
        if flags & 1 == 1 && fit.len() > 2 && fit.first() != fit.last() {
            fit.push(fit[0]);
        }
        let tangent = |code| -> Result<Option<Xy>, ReadError> {
            Ok(has(g, code).then_some(map.dxy(point(g, code)?)))
        };
        Spline::through(&fit, tangent(12)?, tangent(13)?)
    };
    match spline {
        Some(spline) => Ok(Read::Shapes(vec![Shape::Spline(spline)])),
        None => malformed(),
    }
}

/// TEXT: its first alignment point for left and baseline text, and its second for the rest;
/// aligned and fitted text sits on its first point along the line to the second.
fn text(g: &[Tag<'_>], map: &Affine) -> Result<Read, ReadError> {
    let raw = g
        .iter()
        .find(|t| t.code == 1)
        .map_or("", |t| t.value.as_ref());
    let content = text_value(raw);
    if content.trim().is_empty() {
        return Ok(Read::Skipped("empty TEXT".into()));
    }
    let height = real_or(g, 40, 0.0)?;
    if height.is_nan() || height <= 0.0 {
        return Ok(Read::Skipped("TEXT with no height".into()));
    }
    let flags = int_or(g, 71, 0)?;
    let h = int_or(g, 72, 0)?;
    let v = int_or(g, 73, 0)?;
    let first = point(g, 10)?;
    let second = has(g, 11).then(|| point(g, 11)).transpose()?;
    let spans = matches!(h, 3 | 5);
    let at = match second {
        Some(second) if !spans && (h, v) != (0, 0) => second,
        _ => first,
    };
    let mut rotation = real_or(g, 50, 0.0)?.to_radians();
    if let (true, Some(second)) = (spans, second)
        && second != first
    {
        rotation = (second[1] - first[1]).atan2(second[0] - first[0]);
    }
    let justify = TextJustify {
        h: match h {
            1 | 4 => TextHJustify::Center,
            2 => TextHJustify::Right,
            _ => TextHJustify::Left,
        },
        v: match (h, v) {
            (4, _) | (_, 2) => TextVJustify::Center,
            (_, 3) => TextVJustify::Top,
            _ => TextVJustify::Bottom,
        },
    };
    let m = map.of(Affine::ocs(extrusion(g)?));
    let (sin, cos) = rotation.sin_cos();
    let mut along = m.dxy([cos, sin, 0.0]);
    let mut up = m.dxy([-sin * height, cos * height, 0.0]);
    // Written backward, or upside down: mirrored along the baseline, or across it.
    if flags & 2 != 0 {
        along = Xy::new(-along.x, -along.y);
    }
    if flags & 4 != 0 {
        up = Xy::new(-up.x, -up.y);
    }
    Ok(Read::Shapes(vec![Shape::Text(Text::placed(
        content,
        m.xy(at),
        along,
        up,
        justify,
    ))]))
}

/// MTEXT: world insertion point and direction, its attachment point as the justification.
fn mtext(g: &[Tag<'_>], map: &Affine) -> Result<Read, ReadError> {
    let mut raw = String::new();
    for tag in g.iter().filter(|t| t.code == 3) {
        raw.push_str(&tag.value);
    }
    if let Some(tag) = g.iter().find(|t| t.code == 1) {
        raw.push_str(&tag.value);
    }
    let content = mtext_value(&raw);
    if content.trim().is_empty() {
        return Ok(Read::Skipped("empty MTEXT".into()));
    }
    let height = real_or(g, 40, 0.0)?;
    if height.is_nan() || height <= 0.0 {
        return Ok(Read::Skipped("MTEXT with no height".into()));
    }
    let n = normal(extrusion(g)?);
    let direction = if has(g, 11) {
        point(g, 11)?
    } else {
        let (sin, cos) = real_or(g, 50, 0.0)?.sin_cos();
        Affine::ocs(n).vector([cos, sin, 0.0])
    };
    let length = direction.iter().map(|c| c * c).sum::<f64>().sqrt();
    let direction = if length > 0.0 {
        direction.map(|c| c / length)
    } else {
        Affine::ocs(n).vector([1.0, 0.0, 0.0])
    };
    let up = turned(n, direction).map(|c| c * height);
    let attachment = int_or(g, 71, 1)?.clamp(1, 9) - 1;
    let justify = TextJustify {
        h: [
            TextHJustify::Left,
            TextHJustify::Center,
            TextHJustify::Right,
        ][(attachment % 3) as usize],
        v: [
            TextVJustify::Top,
            TextVJustify::Center,
            TextVJustify::Bottom,
        ][(attachment / 3) as usize],
    };
    Ok(Read::Shapes(vec![Shape::Text(Text::placed(
        content,
        map.xy(point(g, 10)?),
        map.dxy(direction),
        map.dxy(up),
        justify,
    ))]))
}
