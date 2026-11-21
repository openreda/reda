//! [`evaluate`]: a template at one set of values, as a [`Frame`].

use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::{self, Millideg, Nm, Point, Vertex};
use reda_model::{
    BoardTemplate, Footprint, FreePad, Hole, KeepoutShapeKind, Side, TemplateContour,
    TemplateDimensionEnd, TemplateDrawing, TemplateFollow, TemplateHole, TemplateNumber,
    TemplatePad, TemplateParameter, TemplatePoint, TemplateVertex, Uuid,
};

use super::frame::{
    Frame, FrameBevel, FrameBoard, FrameContour, FrameDimensionEnd, FrameDrawing,
    FrameDrawingGroup, FrameDrill, FrameEdgeConnector, FrameHole, FrameKeepout, FramePad,
    FramePlacement, FrameProblem, FrameVertex, ProblemObject, ResolvedValue,
};
use super::{
    BoardTemplateChoice, Combination, Namespace, TemplateError, TemplateValue, gate_holds,
};

/// The template's parameters resolved at the values given.
pub(crate) struct Resolved {
    pub(crate) values: BTreeMap<String, ResolvedValue>,
    pub(crate) combination: Combination,
    pub(crate) names: Namespace,
    pub(crate) suggested_layers: u8,
}

/// Resolves `given` against the template's parameters: defaults for what is left out or
/// hidden by a `when`, errors for unknown names, unknown option keys and lengths out of
/// range.
pub(crate) fn resolve(
    template: &BoardTemplate,
    given: &BTreeMap<String, TemplateValue>,
) -> Result<Resolved, TemplateError> {
    let pointer = |name: &str| format!("/values/{name}");
    if let Some(name) = given
        .keys()
        .find(|name| !template.parameters.iter().any(|p| p.name() == *name))
    {
        let known: Vec<&str> = template.parameters.iter().map(|p| p.name()).collect();
        return Err(TemplateError::input(
            pointer(name),
            if known.is_empty() {
                format!("unknown parameter `{name}`: this template has none")
            } else {
                format!(
                    "unknown parameter `{name}`; this template has {}",
                    known.join(", ")
                )
            },
        ));
    }

    // Choices first: a `when` names choices only.
    let mut combination = Combination::new();
    for param in &template.parameters {
        let TemplateParameter::Choice(choice) = param else {
            continue;
        };
        let key = match given.get(&choice.name) {
            None => choice.default_key().unwrap_or_default().to_owned(),
            Some(TemplateValue::Text(key)) if choice.options.iter().any(|o| &o.key == key) => {
                key.clone()
            }
            Some(value) => {
                let keys: Vec<&str> = choice.options.iter().map(|o| o.key.as_str()).collect();
                let shown = match value {
                    TemplateValue::Text(t) => format!("`{t}`"),
                    TemplateValue::Number(n) => n.to_string(),
                };
                return Err(TemplateError::input(
                    pointer(&choice.name),
                    format!(
                        "{shown} is not an option of `{}`; use one of {}",
                        choice.name,
                        keys.join(", ")
                    ),
                ));
            }
        };
        combination.insert(choice.name.clone(), key);
    }
    // A hidden choice takes its default, which may hide or show another.
    for _ in 0..=template.parameters.len() {
        let mut changed = false;
        for param in &template.parameters {
            let TemplateParameter::Choice(choice) = param else {
                continue;
            };
            let default = choice.default_key().unwrap_or_default();
            if !gate_holds(choice.when.as_ref(), &combination)
                && combination.get(&choice.name).map(String::as_str) != Some(default)
            {
                combination.insert(choice.name.clone(), default.to_owned());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let constants = Namespace::empty();
    let mut names = Namespace::empty();
    let mut values = BTreeMap::new();
    let mut suggested_layers = None;
    for param in &template.parameters {
        match param {
            TemplateParameter::Choice(choice) => {
                let key = &combination[&choice.name];
                let option = choice
                    .options
                    .iter()
                    .find(|o| &o.key == key)
                    .ok_or_else(|| {
                        TemplateError::template(format!("`{}` has no options", choice.name))
                    })?;
                for (name, value) in &option.bind {
                    let value = constants.bound(value).map_err(|e| {
                        TemplateError::template(format!(
                            "option `{key}` of `{}` binds `{name}`: {e}",
                            choice.name
                        ))
                    })?;
                    names.insert(name, value);
                }
                suggested_layers = suggested_layers.or(option.suggested_layers);
                values.insert(choice.name.clone(), ResolvedValue::Choice(key.clone()));
            }
            TemplateParameter::Length(length) => {
                let at = |field: &str, n: &TemplateNumber| {
                    constants.length(n).map_err(|e| {
                        TemplateError::template(format!("`{}` {field}: {e}", length.name))
                    })
                };
                let (default, min, max) = (
                    at("default", &length.default)?,
                    at("min", &length.min)?,
                    at("max", &length.max)?,
                );
                let hidden = !gate_holds(length.when.as_ref(), &combination);
                let value = match given.get(&length.name) {
                    Some(given) if !hidden => {
                        let value = match given {
                            TemplateValue::Number(n) => Nm(*n),
                            TemplateValue::Text(text) => constants
                                .length(&TemplateNumber::Expr(text.clone()))
                                .map_err(|e| TemplateError::input(pointer(&length.name), e))?,
                        };
                        if value < min || value > max {
                            return Err(TemplateError::input(
                                pointer(&length.name),
                                format!(
                                    "`{}` must be from {} to {} mm, not {} mm",
                                    length.name,
                                    geom::exact_decimal(min.0, 1_000_000),
                                    geom::exact_decimal(max.0, 1_000_000),
                                    geom::exact_decimal(value.0, 1_000_000),
                                ),
                            ));
                        }
                        value
                    }
                    _ => default,
                };
                names.insert(&length.name, reda_model::geom::expr::Attr::Length(value));
                values.insert(length.name.clone(), ResolvedValue::Length(value));
            }
        }
    }
    Ok(Resolved {
        values,
        combination,
        names,
        suggested_layers: suggested_layers.unwrap_or(template.suggested_layers),
    })
}

/// Evaluates `template` at `choice`'s values and exclusions: the one function preview and
/// creation share. `Err` is bad input or an expression that fails at these values; the
/// frame's own outline, cutout and Keepout problems are in [`Frame::problems`].
///
/// `choice.element` is not looked at: the caller found `template` by it.
pub fn evaluate(
    template: &BoardTemplate,
    choice: &BoardTemplateChoice,
) -> Result<Frame, TemplateError> {
    let resolved = resolve(template, &choice.values)?;
    let combo = &resolved.combination;
    let ns = &resolved.names;
    check_exclude(template, &choice.exclude)?;
    let excluded: BTreeSet<Uuid> = choice.exclude.iter().copied().collect();
    let fail = |what: String| move |e: String| TemplateError::template(format!("{what}: {e}"));

    let mut outlines = template
        .outline
        .iter()
        .filter(|r| gate_holds(r.when.as_ref(), combo));
    let outline_row = match (outlines.next(), outlines.next()) {
        (Some(row), None) => row,
        _ => {
            return Err(TemplateError::template(
                "exactly one Board Outline row must survive the gates",
            ));
        }
    };
    let (outline, outline_raw) = contour(outline_row, ns, "Board Outline")?;
    let mut raw_vertices: BTreeMap<Uuid, Vec<Point>> = BTreeMap::new();
    raw_vertices.insert(outline_row.uuid, outline_raw);
    let mut cutouts = vec![];
    for row in template
        .cutouts
        .iter()
        .filter(|r| gate_holds(r.when.as_ref(), combo))
    {
        let (cutout, raw) = contour(row, ns, "Board Cutout")?;
        raw_vertices.insert(row.uuid, raw);
        cutouts.push(cutout);
    }

    let mut placed: BTreeMap<Uuid, Point> = BTreeMap::new();
    let mut connectors = vec![];
    for row in template
        .connectors
        .iter()
        .filter(|r| gate_holds(r.when.as_ref(), combo))
    {
        let what = format!("connector {}", row.uuid);
        let at = point(&row.at, ns).map_err(fail(what.clone()))?;
        let rotation = ns.angle(&row.rotation).map_err(fail(what.clone()))?;
        let footprint = copy(template, row.footprint, &what)?;
        placed.insert(row.uuid, at);
        connectors.push(FramePlacement {
            uuid: row.uuid,
            part: row.part,
            footprint: row.footprint,
            at,
            rotation,
            side: row.side,
            locked: row.locked,
            excluded: excluded.contains(&row.uuid),
            pads: footprint_pads(footprint, at, rotation, row.side),
        });
    }
    let mut holes = vec![];
    for row in template
        .holes
        .iter()
        .filter(|r| gate_holds(r.when.as_ref(), combo))
    {
        let hole = hole(template, row, ns, excluded.contains(&row.uuid))?;
        placed.insert(row.uuid, hole.at);
        holes.push(hole);
    }

    let mut keepouts = vec![];
    for row in template
        .keepouts
        .iter()
        .filter(|r| gate_holds(r.when.as_ref(), combo))
    {
        // A Keepout whose protected object is gated out goes with it.
        if row.protects.is_some_and(|p| !placed.contains_key(&p)) {
            continue;
        }
        let what = format!("Keepout {}", row.uuid);
        let vertices = vertices(&row.vertices, ns).map_err(fail(what.clone()))?;
        raw_vertices.insert(row.uuid, vertices.iter().map(Vertex::point).collect());
        let width = row
            .width
            .as_ref()
            .map(|w| ns.length(w))
            .transpose()
            .map_err(fail(what))?;
        keepouts.push(FrameKeepout {
            uuid: row.uuid,
            layer: row.layer,
            restrict: row.restrict.clone(),
            shape: row.shape,
            vertices,
            width,
            protects: row.protects,
            locked: row.locked,
            excluded: row.protects.is_some_and(|p| excluded.contains(&p)),
        });
    }

    let gone: BTreeSet<Uuid> = excluded
        .iter()
        .copied()
        .chain(keepouts.iter().filter(|k| k.excluded).map(|k| k.uuid))
        .collect();
    let mut drawing_groups = vec![];
    for group in template
        .drawing_groups
        .iter()
        .filter(|g| gate_holds(g.when.as_ref(), combo))
    {
        let group_excluded = excluded.contains(&group.uuid);
        let mut drawings = vec![];
        for row in template
            .drawings
            .iter()
            .filter(|d| d.group == group.uuid && gate_holds(d.when.as_ref(), combo))
        {
            let ends = [row.a.as_ref(), row.b.as_ref()];
            // A Dimension whose followed object is gated out goes with it.
            let follows: Vec<TemplateFollow> =
                ends.iter().flatten().filter_map(|e| e.follow).collect();
            if follows
                .iter()
                .any(|f| !raw_vertices.contains_key(&f.object) && !placed.contains_key(&f.object))
            {
                continue;
            }
            let mut drawing = drawing(row, ns, &raw_vertices, &placed)?;
            drawing.excluded = group_excluded || follows.iter().any(|f| gone.contains(&f.object));
            drawings.push(drawing);
        }
        drawing_groups.push(FrameDrawingGroup {
            uuid: group.uuid,
            name: group.name.clone(),
            layer: group.layer,
            excluded: group_excluded,
            drawings,
        });
    }

    let board = match &template.board {
        None => None,
        Some(board) => Some(FrameBoard {
            edge_connector: match &board.edge_connector {
                None => None,
                Some(edge) => Some(FrameEdgeConnector {
                    bevel: match &edge.bevel {
                        None => None,
                        Some(bevel) => Some(FrameBevel {
                            angle: ns
                                .angle(&bevel.angle)
                                .map_err(fail("the edge connector bevel angle".into()))?,
                            depth: ns
                                .length(&bevel.depth)
                                .map_err(fail("the edge connector bevel depth".into()))?,
                        }),
                    },
                    hard_gold: edge.hard_gold,
                }),
            },
        }),
    };

    let mut frame = Frame {
        element: template.uuid,
        name: template.name.clone(),
        values: resolved.values,
        suggested_layers: resolved.suggested_layers,
        board,
        outline,
        cutouts,
        keepouts,
        connectors,
        holes,
        drawing_groups,
        problems: vec![],
    };
    frame.problems = problems(&frame);
    Ok(frame)
}

/// Refuses an `exclude` entry that is not a connector, hole or drawing-group row. A row
/// a gate removes is accepted.
fn check_exclude(template: &BoardTemplate, exclude: &[Uuid]) -> Result<(), TemplateError> {
    for (i, uuid) in exclude.iter().enumerate() {
        let ok = template.connectors.iter().any(|r| r.uuid == *uuid)
            || template.holes.iter().any(|r| r.uuid == *uuid)
            || template.drawing_groups.iter().any(|r| r.uuid == *uuid);
        if ok {
            continue;
        }
        let kind = if template.outline.iter().any(|r| r.uuid == *uuid) {
            Some("the Board Outline")
        } else if template.cutouts.iter().any(|r| r.uuid == *uuid) {
            Some("a Board Cutout")
        } else if template.keepouts.iter().any(|r| r.uuid == *uuid) {
            Some("a Keepout")
        } else if template.drawings.iter().any(|r| r.uuid == *uuid) {
            Some("a single drawing")
        } else {
            None
        };
        let message = match kind {
            Some(kind) => format!(
                "{uuid} is {kind}, which can't be excluded; exclude connectors, holes and \
                 drawing groups"
            ),
            None => format!("{uuid} is not a connector, hole or drawing group of this template"),
        };
        return Err(TemplateError::input(format!("/exclude/{i}"), message));
    }
    Ok(())
}

fn point(p: &TemplatePoint, ns: &Namespace) -> Result<Point, String> {
    Ok(Point::new(ns.length(&p.x)?, ns.length(&p.y)?))
}

fn vertices(rows: &[TemplateVertex], ns: &Namespace) -> Result<Vec<Vertex>, String> {
    rows.iter()
        .enumerate()
        .map(|(i, v)| {
            let at = |e: String| format!("vertex {i}: {e}");
            let p = Point::new(ns.length(&v.x).map_err(at)?, ns.length(&v.y).map_err(at)?);
            let sweep = match &v.sweep {
                Some(s) => ns.angle(s).map_err(at)?,
                None => Millideg::ZERO,
            };
            Ok(Vertex::arc(p, sweep))
        })
        .collect()
}

/// A contour row evaluated, with zero-length edges dropped, and its vertices' points as
/// evaluated (for Dimension ends, which count the template's vertices).
fn contour(
    row: &TemplateContour,
    ns: &Namespace,
    what: &str,
) -> Result<(FrameContour, Vec<Point>), TemplateError> {
    let evaluated = vertices(&row.vertices, ns)
        .map_err(|e| TemplateError::template(format!("{what} {}: {e}", row.uuid)))?;
    let raw = evaluated.iter().map(Vertex::point).collect();
    let mut kept: Vec<FrameVertex> = evaluated
        .iter()
        .zip(&row.vertices)
        .map(|(v, t)| FrameVertex {
            x: v.x,
            y: v.y,
            sweep: v.sweep,
            plated: t.plated,
        })
        .collect();
    // A vertex at the same point as the next starts a zero-length edge: drop it, keeping
    // the next vertex's own edge.
    loop {
        let n = kept.len();
        if n < 2 {
            break;
        }
        let Some(i) = (0..n).find(|&i| {
            let (a, b) = (kept[i], kept[(i + 1) % n]);
            (a.x, a.y) == (b.x, b.y)
        }) else {
            break;
        };
        kept.remove(i);
    }
    Ok((
        FrameContour {
            uuid: row.uuid,
            vertices: kept,
            locked: row.locked,
        },
        raw,
    ))
}

fn copy<'a>(
    template: &'a BoardTemplate,
    uuid: Uuid,
    what: &str,
) -> Result<&'a Footprint, TemplateError> {
    template
        .footprint_copies
        .iter()
        .find(|c| c.uuid == uuid)
        .map(|c| &c.footprint)
        .ok_or_else(|| {
            TemplateError::template(format!(
                "{what} names Footprint copy {uuid}, which is missing"
            ))
        })
}

/// A Footprint-frame point on the board: flipped for the bottom, turned, then moved.
fn on_board(p: Point, at: Point, rotation: Millideg, side: Side) -> Point {
    let p = match side {
        Side::Top => p,
        Side::Bottom => Point::new(-p.x, p.y),
    };
    let (x, y) = geom::rotate(p.x.0, p.y.0, rotation);
    Point::new(at.x + Nm(x), at.y + Nm(y))
}

fn footprint_pads(
    footprint: &Footprint,
    at: Point,
    rotation: Millideg,
    side: Side,
) -> Vec<FramePad> {
    let flip = |sweep: Millideg| match side {
        Side::Top => sweep,
        Side::Bottom => -sweep,
    };
    footprint
        .pads
        .iter()
        .map(|pad| {
            let outline = pad
                .outline()
                .contour()
                .into_iter()
                .map(|v| {
                    let in_footprint =
                        v.point().rotated_about(Point::default(), pad.rotation) + pad.at;
                    Vertex::arc(on_board(in_footprint, at, rotation, side), flip(v.sweep))
                })
                .collect();
            FramePad {
                number: pad.number.clone(),
                at: on_board(pad.at, at, rotation, side),
                outline,
                hole: pad.hole.map(drill),
            }
        })
        .collect()
}

fn drill(hole: Hole) -> FrameDrill {
    FrameDrill {
        width: hole.width,
        height: hole.height,
    }
}

fn hole(
    template: &BoardTemplate,
    row: &TemplateHole,
    ns: &Namespace,
    excluded: bool,
) -> Result<FrameHole, TemplateError> {
    let what = format!("hole {}", row.uuid);
    let fail = |e: String| TemplateError::template(format!("{what}: {e}"));
    let at = point(&row.at, ns).map_err(fail)?;
    let rotation = ns.angle(&row.rotation).map_err(fail)?;
    let mut out = FrameHole {
        uuid: row.uuid,
        part: row.part,
        footprint: row.footprint,
        side: row.side,
        at,
        rotation,
        free_pad: None,
        locked: row.locked,
        excluded,
        pads: vec![],
    };
    match (&row.pad, row.footprint) {
        (Some(pad), _) => {
            let free = free_pad(row.uuid, pad, at, rotation, row.locked, ns).map_err(fail)?;
            out.pads = vec![FramePad {
                number: String::new(),
                at,
                outline: free
                    .outline()
                    .contour()
                    .into_iter()
                    .map(|v| {
                        Vertex::arc(
                            v.point().rotated_about(Point::default(), rotation) + at,
                            v.sweep,
                        )
                    })
                    .collect(),
                hole: free.hole.map(drill),
            }];
            out.free_pad = Some(free);
        }
        (None, Some(footprint)) => {
            let side = row.side.unwrap_or(Side::Top);
            out.pads = footprint_pads(copy(template, footprint, &what)?, at, rotation, side);
        }
        (None, None) => {
            return Err(fail("it has neither a `pad` nor a `footprint`".into()));
        }
    }
    Ok(out)
}

fn free_pad(
    uuid: Uuid,
    pad: &TemplatePad,
    at: Point,
    rotation: Millideg,
    locked: bool,
    ns: &Namespace,
) -> Result<FreePad, String> {
    let hole = match &pad.hole {
        None => None,
        Some(h) => Some(Hole {
            width: ns.length(&h.width)?,
            height: ns.length(&h.height)?,
            rotation: match &h.rotation {
                Some(r) => ns.angle(r)?,
                None => Millideg::ZERO,
            },
            tolerance: h.tolerance,
        }),
    };
    Ok(FreePad {
        uuid,
        locked,
        teardrop: pad.teardrop,
        kind: pad.kind,
        castellated: pad.castellated,
        shape: pad.shape,
        middle: pad.middle,
        bottom: pad.bottom,
        unused_layers: pad.unused_layers,
        at,
        rotation,
        width: ns.length(&pad.width)?,
        height: ns.length(&pad.height)?,
        corner_ratio: pad.corner_ratio,
        chamfer_ratio: pad.chamfer_ratio,
        corners: pad.corners,
        offset: pad.offset.as_ref().map(|o| point(o, ns)).transpose()?,
        hole,
        side: pad.side,
        net: None,
        mask: pad.mask,
        paste: pad.paste.clone(),
        zone_connection: pad.zone_connection,
    })
}

fn drawing(
    row: &TemplateDrawing,
    ns: &Namespace,
    vertices: &BTreeMap<Uuid, Vec<Point>>,
    placed: &BTreeMap<Uuid, Point>,
) -> Result<FrameDrawing, TemplateError> {
    let fail = |e: String| TemplateError::template(format!("drawing {}: {e}", row.uuid));
    let length = |n: &Option<TemplateNumber>| n.as_ref().map(|n| ns.length(n)).transpose();
    let angle = |n: &Option<TemplateNumber>| n.as_ref().map(|n| ns.angle(n)).transpose();
    let at = |p: &Option<TemplatePoint>| p.as_ref().map(|p| point(p, ns)).transpose();
    let end = |e: &Option<TemplateDimensionEnd>| -> Result<Option<FrameDimensionEnd>, String> {
        let Some(e) = e else { return Ok(None) };
        let point = match (e.follow, &e.x, &e.y) {
            (Some(f), _, _) => follow_point(f, vertices, placed)?,
            (None, Some(x), Some(y)) => Point::new(ns.length(x)?, ns.length(y)?),
            _ => return Err("a Dimension end needs `x` and `y`, or `follow`".into()),
        };
        Ok(Some(FrameDimensionEnd {
            point,
            follow: e.follow,
        }))
    };
    Ok(FrameDrawing {
        uuid: row.uuid,
        kind: row.kind,
        width: length(&row.width).map_err(fail)?,
        from: at(&row.from).map_err(fail)?,
        to: at(&row.to).map_err(fail)?,
        sweep: angle(&row.sweep).map_err(fail)?,
        filled: row.filled,
        text: row.text.clone(),
        position: at(&row.position).map_err(fail)?,
        height: length(&row.height).map_err(fail)?,
        rotation: angle(&row.rotation).map_err(fail)?,
        a: end(&row.a).map_err(fail)?,
        b: end(&row.b).map_err(fail)?,
        offset: length(&row.offset).map_err(fail)?,
        prefix: row.prefix.clone(),
        suffix: row.suffix.clone(),
        locked: row.locked,
        excluded: false,
    })
}

fn follow_point(
    f: TemplateFollow,
    vertices: &BTreeMap<Uuid, Vec<Point>>,
    placed: &BTreeMap<Uuid, Point>,
) -> Result<Point, String> {
    if let Some(at) = placed.get(&f.object) {
        return Ok(*at);
    }
    let points = vertices
        .get(&f.object)
        .ok_or_else(|| format!("it follows {}, which is not in the frame", f.object))?;
    let i = f.vertex.unwrap_or(0);
    points.get(i).copied().ok_or_else(|| {
        format!(
            "it follows vertex {i} of {}, which has {}",
            f.object,
            points.len()
        )
    })
}

/// The Board Outline, Board Cutout and Keepout checks on the frame's included objects.
fn contour_problem(uuid: Uuid, kind: ProblemObject, vertices: &[Vertex]) -> Option<FrameProblem> {
    if let Err(e) = geom::check_contour(vertices) {
        let at = match e {
            geom::ContourError::ZeroLengthEdge(p) | geom::ContourError::BadSweep(p) => Some(p),
            _ => vertices.first().map(Vertex::point),
        };
        return Some(FrameProblem {
            object: uuid,
            kind,
            message: e.to_string(),
            at,
        });
    }
    geom::crossing(vertices).map(|at| FrameProblem {
        object: uuid,
        kind,
        message: format!("the shape crosses itself at ({} nm, {} nm)", at.x.0, at.y.0),
        at: Some(at),
    })
}

fn problems(frame: &Frame) -> Vec<FrameProblem> {
    let mut out = vec![];
    out.extend(contour_problem(
        frame.outline.uuid,
        ProblemObject::Outline,
        &frame.outline.contour(),
    ));
    for cutout in &frame.cutouts {
        out.extend(contour_problem(
            cutout.uuid,
            ProblemObject::Cutout,
            &cutout.contour(),
        ));
    }
    for keepout in frame.keepouts.iter().filter(|k| !k.excluded) {
        let problem = |message: &str| FrameProblem {
            object: keepout.uuid,
            kind: ProblemObject::Keepout,
            message: message.into(),
            at: keepout.vertices.first().map(Vertex::point),
        };
        if keepout.restrict.is_empty() {
            out.push(problem("the Keepout restricts nothing"));
        }
        match keepout.shape {
            KeepoutShapeKind::Polygon => {
                out.extend(contour_problem(
                    keepout.uuid,
                    ProblemObject::Keepout,
                    &keepout.vertices,
                ));
            }
            KeepoutShapeKind::Line => {
                if keepout.vertices.len() < 2 {
                    out.push(problem("a line Keepout needs two or more vertices"));
                }
                if keepout.width.is_none_or(|w| w <= Nm::ZERO) {
                    out.push(problem("a line Keepout needs a positive width"));
                }
            }
        }
    }
    out
}
