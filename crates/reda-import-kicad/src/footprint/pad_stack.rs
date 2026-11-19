//! Copper shapes in a KiCad Pad Stack. The enclosing Pad carries the front shape.
//! Each retained Layer uses the same shape, size and corner conversion as the front.

use super::node::{self, Read, fail};
use super::{Sexp, flip};
use reda_model::geom::Point;
use reda_model::{Corner, Corners, PadCopperShape, PadKind, PadShape, Ratio};

pub(super) struct Stack {
    pub top: PadCopperShape,
    pub middle: Option<PadCopperShape>,
    pub bottom: Option<PadCopperShape>,
    pub offset: Option<Point>,
}

fn offset(layer: &Sexp) -> Read<Option<Point>> {
    Ok(layer
        .child("offset")
        .map(node::point)
        .transpose()?
        .map(flip)
        .filter(|p| *p != Point::default()))
}

fn layer_shape(layer: &Sexp, number: &str, warnings: &mut Vec<String>) -> Read<PadCopperShape> {
    shape(
        layer,
        node::text(node::need(layer, "shape")?, 0)?,
        number,
        warnings,
    )
}

pub(super) fn read(
    pad: &Sexp,
    kind: PadKind,
    number: &str,
    front_offset: Option<Point>,
    warnings: &mut Vec<String>,
) -> Read<Stack> {
    let mut copper = Stack {
        top: shape(pad, node::text(pad, 2)?, number, warnings)?,
        middle: None,
        bottom: None,
        offset: front_offset,
    };
    let Some(stack) = pad.child("padstack") else {
        return Ok(copper);
    };
    if kind != PadKind::PlatedThroughHole {
        warnings.push(format!(
            "pad {number:?}: ignored a Pad Stack on a non-plated or surface-mount Pad"
        ));
        return Ok(copper);
    }
    let layer = |name| stack.children("layer").find(|l| l.arg(0) == Some(name));
    if let Some(front) = layer("F.Cu") {
        copper.top = layer_shape(front, number, warnings)?;
        copper.offset = offset(front)?;
    }
    let middle = match node::text(node::need(stack, "mode")?, 0)? {
        "front_inner_back" => layer("Inner"),
        "custom" => {
            let first = stack
                .children("layer")
                .filter_map(|l| {
                    let index = l
                        .arg(0)?
                        .strip_prefix("In")?
                        .strip_suffix(".Cu")?
                        .parse::<u8>()
                        .ok()?;
                    (1..=30).contains(&index).then_some((index, l))
                })
                .min_by_key(|&(index, _)| index)
                .map(|(_, l)| l);
            let source = first.and_then(|l| l.arg(0)).unwrap_or("the front shape");
            warnings.push(format!("pad {number:?}: imported a custom Pad Stack using the front, {source} for all inner Layers, and the back"));
            first
        }
        other => return fail(stack, format!("unknown padstack mode {other:?}")),
    };
    for (source, group) in [
        (middle, &mut copper.middle),
        (layer("B.Cu"), &mut copper.bottom),
    ] {
        if let Some(source) = source {
            *group = Some(layer_shape(source, number, warnings)?);
            if offset(source)? != copper.offset {
                let name = node::text(source, 0)?;
                warnings.push(format!("pad {number:?}: used the front copper offset on {name}; REDA shares the offset across its Pad Stack"));
            }
        }
    }
    Ok(copper)
}

fn shape(
    pad: &Sexp,
    shape_word: &str,
    number: &str,
    warnings: &mut Vec<String>,
) -> Read<PadCopperShape> {
    let shape = match shape_word {
        "circle" => PadShape::Circle,
        "rect" => PadShape::Rect,
        "oval" => PadShape::Oval,
        "roundrect" => PadShape::RoundedRect,
        "trapezoid" => {
            warnings.push(format!(
                "pad {number:?}: a trapezoid was imported as a rectangle"
            ));
            PadShape::Rect
        }
        "custom" => {
            let anchor = pad
                .child("options")
                .and_then(|o| o.child("anchor"))
                .and_then(|a| a.arg(0));
            warnings.push(format!(
                "pad {number:?}: a custom shape was imported as its anchor pad"
            ));
            match anchor {
                Some("circle") => PadShape::Circle,
                _ => PadShape::Rect,
            }
        }
        other => return fail(pad, format!("unknown pad shape {other:?}")),
    };
    let size = node::need(pad, "size")?;
    let width = node::mm(size, 0)?;
    let height = if shape == PadShape::Circle {
        width
    } else {
        node::mm(size, 1)?
    };
    if width.0 <= 0 || height.0 <= 0 {
        return fail(size, format!("pad {number:?} has no size"));
    }
    // KiCad's ratios are shares of the shorter side, at most 0.5; REDA's are shares of
    // half of it.
    let ratio = |name: &str| -> Read<Option<Ratio>> {
        let Some(r) = pad.child(name) else {
            return Ok(None);
        };
        let value =
            crate::number::fixed(node::text(r, 0)?, 6).filter(|r| (0..=500_000).contains(r));
        match value {
            Some(v) => Ok(Some(Ratio((v / 5) as u32))),
            None => fail(r, format!("the {name} must be between 0 and 0.5")),
        }
    };
    let rounding = ratio("roundrect_rratio")?;
    let chamfers = chamfer_corners(pad)?;
    let chamfer = ratio("chamfer_ratio")?.filter(|r| r.0 > 0);
    let (shape, corner_ratio, chamfer_ratio, corners) = match (shape, chamfers, chamfer) {
        (PadShape::RoundedRect | PadShape::Rect, Some(corners), Some(chamfer)) => {
            let rounding = if shape == PadShape::RoundedRect {
                rounding.filter(|r| r.0 > 0)
            } else {
                None
            };
            let corners = (corners != Corners::ALL).then_some(corners);
            (PadShape::ChamferedRect, rounding, Some(chamfer), corners)
        }
        (PadShape::RoundedRect, _, _) => (
            shape,
            Some(rounding.unwrap_or(Ratio::percent(50))),
            None,
            None,
        ),
        _ => (shape, None, None, None),
    };
    Ok(PadCopperShape {
        shape,
        width,
        height,
        corner_ratio,
        chamfer_ratio,
        corners,
    })
}

/// The corners a `(chamfer …)` list names, or `None` without one.
///
/// KiCad's pad frame has y down, and the import negates y, which mirrors the frame
/// top-bottom. A corner keeps the place it is seen in: KiCad's `top_left` is its corner
/// of negative y, which the flip moves to positive y, REDA's top. So the flip of the flags
/// undoes KiCad's own y-down naming, and each name maps to the REDA corner of that name.
fn chamfer_corners(pad: &Sexp) -> Read<Option<Corners>> {
    let Some(list) = pad.child("chamfer") else {
        return Ok(None);
    };
    let mut corners = Corners::NONE;
    for word in list.args().iter().filter_map(Sexp::text) {
        // The corner in KiCad's y-down frame, by the signs of its coordinates.
        let kicad = match word {
            "top_left" => Corner::BottomLeft,
            "top_right" => Corner::BottomRight,
            "bottom_left" => Corner::TopLeft,
            "bottom_right" => Corner::TopRight,
            other => return fail(list, format!("unknown chamfer corner {other:?}")),
        };
        // Negating y mirrors it top-bottom.
        corners = corners.with(kicad.flipped());
    }
    Ok((corners != Corners::NONE).then_some(corners))
}
