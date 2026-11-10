//! The Pad fields of the Pad model (library.md#pad-model) as one form, shared by the
//! Footprint editor's Pad rows and Free Pad Properties: kind, shape, size, rotation,
//! corner and chamfer ratios and corners, offset, hole (size, rotation, tolerance), side,
//! mask and paste overrides and the Zone connection.
//!
//! An edit ([`PadEdit`]) becomes the fields of a `library.footprint.pad.edit` or
//! `pcb.free_pad.edit` call ([`patch`]), which the caller completes with the Pad's ids. A
//! ratio is typed as a percentage, or as a length that converts at the Pad's current
//! size ([`parse_ratio`]). The drawing is [`draw::rows`].

pub mod draw;

use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{
    ConnectionStyle, Corner, Corners, FreePad, Hole, HoleTolerance, Mask, MaskSide, Pad, PadKind,
    PadShape, Paste, Ratio, Side, Tenting, ZoneConnection,
};
use serde_json::{Map, Value, json};

/// The hole a Pad turned plated or non-plated gets: 0.8 mm round, or less on a small Pad.
pub const DEFAULT_HOLE: Nm = Nm(800_000);
/// The chamfer a Pad turned chamfered gets.
pub const DEFAULT_CHAMFER_RATIO: Ratio = Ratio::percent(50);

/// A mask side's override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskChoice {
    /// Left out: the board's rule.
    Inherit,
    Tent,
    Expansion(Nm),
}

/// The paste override of an SMD Pad.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasteChoice {
    Inherit,
    /// No paste at all.
    None,
    Expansion(Nm),
    /// The change in area, in percent: -10 has 10% less.
    Percent(i32),
}

/// One edit of the Pad rows.
#[derive(Clone, Debug, PartialEq)]
pub enum PadEdit {
    Number(String),
    Kind(PadKind),
    Shape(PadShape),
    Width(Nm),
    Height(Nm),
    Rotation(Millideg),
    CornerRatio(Ratio),
    ChamferRatio(Ratio),
    /// Turns one corner on or off in `corners`.
    Corner(Corner, bool),
    OffsetX(Nm),
    OffsetY(Nm),
    HoleWidth(Nm),
    HoleHeight(Nm),
    HoleRotation(Millideg),
    /// `None` clears the tolerance.
    HoleTolerance(Option<HoleTolerance>),
    Side(Side),
    Mask(Side, MaskChoice),
    /// `None` inherits the board's rule.
    FromHoleEdge(Option<bool>),
    Paste(PasteChoice),
    ZoneStyle(Option<ConnectionStyle>),
    ZoneSpokes(Option<u8>),
    ZoneSpokeWidth(Option<Nm>),
    ZoneGap(Option<Nm>),
    ZoneAngle(Option<Millideg>),
}

/// A Free Pad as a numberless Pad, for the form.
pub fn from_free(free: &FreePad) -> Pad {
    Pad {
        uuid: free.uuid,
        number: String::new(),
        kind: free.kind,
        shape: free.shape,
        at: free.at,
        rotation: free.rotation,
        width: free.width,
        height: free.height,
        corner_ratio: free.corner_ratio,
        chamfer_ratio: free.chamfer_ratio,
        corners: free.corners,
        offset: free.offset,
        hole: free.hole,
        side: free.side,
        mask: free.mask,
        paste: free.paste.clone(),
        zone_connection: free.zone_connection,
    }
}

/// Whether the Pad's shape has corners to round or chamfer.
pub fn cornered(shape: PadShape) -> bool {
    matches!(shape, PadShape::RoundedRect | PadShape::ChamferedRect)
}

/// `text` as a ratio of half the Pad's shorter side: `25%` or a bare `25` is a
/// percentage; a length (`0.2mm`, `8mil`) converts at the Pad's current `width` and
/// `height`, up to 100%. `None` when it reads as neither.
pub fn parse_ratio(text: &str, width: Nm, height: Nm) -> Option<Ratio> {
    let text = text.trim();
    let percent = text.strip_suffix('%').unwrap_or(text).trim();
    if let Ok(p) = percent.parse::<f64>() {
        return (p >= 0.0).then(|| Ratio(((p * 1000.0).round() as u32).min(Ratio::FULL.0)));
    }
    let length = Nm::parse(&text.replace(' ', "")).ok()?;
    let half = width.0.min(height.0) as f64 / 2.0;
    if length.0 < 0 || half <= 0.0 {
        return None;
    }
    let ratio = (length.0 as f64 / half * f64::from(Ratio::FULL.0)).round();
    Some(Ratio((ratio as u32).min(Ratio::FULL.0)))
}

/// A ratio as the form shows it: `25%`, `12.5%`.
pub fn format_ratio(ratio: Ratio) -> String {
    let p = f64::from(ratio.0) / 1000.0;
    let s = format!("{p:.3}");
    format!("{}%", s.trim_end_matches('0').trim_end_matches('.'))
}

/// The mask override of `side`.
pub fn mask_choice(pad: &Pad, side: Side) -> MaskChoice {
    let mask = pad.mask.unwrap_or_default();
    let own = match side {
        Side::Top => mask.top,
        Side::Bottom => mask.bottom,
    };
    match own {
        None => MaskChoice::Inherit,
        Some(MaskSide::Mode(Tenting::Tent)) => MaskChoice::Tent,
        Some(MaskSide::Expansion { expansion }) => MaskChoice::Expansion(expansion),
    }
}

/// The paste override; paste windows show as [`None`] (they are kept until changed).
pub fn paste_choice(pad: &Pad) -> Option<PasteChoice> {
    Some(match &pad.paste {
        None => PasteChoice::Inherit,
        Some(Paste::Mode(_)) => PasteChoice::None,
        Some(Paste::Expansion { expansion }) => PasteChoice::Expansion(*expansion),
        Some(Paste::Percent { percent }) => PasteChoice::Percent(*percent),
        Some(Paste::Windows { .. }) => return None,
    })
}

/// The mask sides a Pad can override: an SMD Pad only its own side.
pub fn mask_sides(pad: &Pad) -> Vec<Side> {
    match pad.kind {
        PadKind::Smd => vec![pad.side.unwrap_or(Side::Top)],
        _ => vec![Side::Top, Side::Bottom],
    }
}

/// The fields of the edit Command that make `edit` on `pad`, as a JSON object to complete
/// with the Pad's ids. `None` when the edit changes nothing.
pub fn patch(pad: &Pad, edit: &PadEdit) -> Option<Map<String, Value>> {
    let mut p = Map::new();
    let mut set = |key: &str, value: Value| {
        p.insert(key.to_owned(), value);
    };
    match edit {
        PadEdit::Number(n) => {
            let n = n.trim();
            if n == pad.number {
                return None;
            }
            set("number", json!(n));
        }
        PadEdit::Kind(kind) => {
            if *kind == pad.kind {
                return None;
            }
            set("kind", json!(kind));
            match (kind, pad.hole) {
                // An SMD Pad has no hole, no bottom-side override and no hole-edge mask.
                (PadKind::Smd, _) => {
                    if pad.hole.is_some() {
                        set("hole", Value::Null);
                    }
                    if let Some(mask) = pad.mask {
                        let side = pad.side.unwrap_or(Side::Top);
                        let kept = Mask {
                            top: mask.top.filter(|_| side == Side::Top),
                            bottom: mask.bottom.filter(|_| side == Side::Bottom),
                            from_hole_edge: None,
                        };
                        set("mask", mask_json(kept));
                    }
                }
                (_, None) => {
                    let side = DEFAULT_HOLE.0.min(pad.width.0).min(pad.height.0);
                    set("hole", json!({"width": side, "height": side}));
                }
                _ => {}
            }
            if *kind != PadKind::Smd {
                if pad.side.is_some() {
                    set("side", Value::Null);
                }
                if pad.paste.is_some() {
                    set("paste", Value::Null);
                }
            }
        }
        PadEdit::Shape(shape) => {
            if *shape == pad.shape {
                return None;
            }
            set("shape", json!(shape));
            if *shape == PadShape::ChamferedRect && pad.chamfer_ratio.is_none() {
                set("chamfer_ratio", json!(DEFAULT_CHAMFER_RATIO.0));
            }
        }
        PadEdit::Width(w) => changed(&mut set, "width", pad.width != *w, json!(w.0))?,
        PadEdit::Height(h) => changed(&mut set, "height", pad.height != *h, json!(h.0))?,
        PadEdit::Rotation(r) => {
            let r = r.normalized();
            changed(&mut set, "rotation", pad.rotation != r, json!(r.0))?
        }
        PadEdit::CornerRatio(r) => changed(
            &mut set,
            "corner_ratio",
            pad.corner_ratio != Some(*r),
            json!(r.0),
        )?,
        PadEdit::ChamferRatio(r) => changed(
            &mut set,
            "chamfer_ratio",
            pad.chamfer_ratio != Some(*r),
            json!(r.0),
        )?,
        PadEdit::Corner(corner, on) => {
            let now = pad.corners.unwrap_or(Corners::ALL);
            if now.contains(*corner) == *on {
                return None;
            }
            let list: Vec<Corner> = Corner::ALL
                .into_iter()
                .filter(|c| if c == corner { *on } else { now.contains(*c) })
                .collect();
            set(
                "corners",
                if list.len() == 4 {
                    Value::Null
                } else {
                    json!(list)
                },
            );
        }
        PadEdit::OffsetX(x) | PadEdit::OffsetY(x) => {
            let now = pad.offset.unwrap_or_default();
            let next = match edit {
                PadEdit::OffsetX(_) => Point::new(*x, now.y),
                _ => Point::new(now.x, *x),
            };
            if next == now {
                return None;
            }
            set(
                "offset",
                if next == Point::default() {
                    Value::Null
                } else {
                    json!({"x": next.x.0, "y": next.y.0})
                },
            );
        }
        PadEdit::HoleWidth(_)
        | PadEdit::HoleHeight(_)
        | PadEdit::HoleRotation(_)
        | PadEdit::HoleTolerance(_) => {
            let before = pad.hole?;
            let mut hole = before;
            match edit {
                PadEdit::HoleWidth(w) => hole.width = *w,
                PadEdit::HoleHeight(h) => hole.height = *h,
                PadEdit::HoleRotation(r) => hole.rotation = r.normalized(),
                PadEdit::HoleTolerance(t) => hole.tolerance = *t,
                _ => unreachable!("a hole edit"),
            }
            if hole == before {
                return None;
            }
            set("hole", hole_json(&hole));
        }
        PadEdit::Side(side) => {
            if pad.kind != PadKind::Smd || pad.side.unwrap_or(Side::Top) == *side {
                return None;
            }
            set("side", json!(side));
            // The override moves with the copper to the other side.
            if let Some(mask) = pad.mask {
                let own = mask.top.or(mask.bottom);
                let moved = match side {
                    Side::Top => Mask {
                        top: own,
                        bottom: None,
                        from_hole_edge: None,
                    },
                    Side::Bottom => Mask {
                        top: None,
                        bottom: own,
                        from_hole_edge: None,
                    },
                };
                set("mask", mask_json(moved));
            }
        }
        PadEdit::Mask(side, choice) => {
            if mask_choice(pad, *side) == *choice {
                return None;
            }
            let mut mask = pad.mask.unwrap_or_default();
            let own = match choice {
                MaskChoice::Inherit => None,
                MaskChoice::Tent => Some(MaskSide::Mode(Tenting::Tent)),
                MaskChoice::Expansion(e) => Some(MaskSide::Expansion { expansion: *e }),
            };
            match side {
                Side::Top => mask.top = own,
                Side::Bottom => mask.bottom = own,
            }
            set("mask", mask_json(mask));
        }
        PadEdit::FromHoleEdge(on) => {
            let mut mask = pad.mask.unwrap_or_default();
            if mask.from_hole_edge == *on {
                return None;
            }
            mask.from_hole_edge = *on;
            set("mask", mask_json(mask));
        }
        PadEdit::Paste(choice) => {
            if paste_choice(pad).as_ref() == Some(choice) {
                return None;
            }
            set(
                "paste",
                match choice {
                    PasteChoice::Inherit => Value::Null,
                    PasteChoice::None => json!("none"),
                    PasteChoice::Expansion(e) => json!({"expansion": e.0}),
                    PasteChoice::Percent(p) => json!({"percent": p}),
                },
            );
        }
        PadEdit::ZoneStyle(_)
        | PadEdit::ZoneSpokes(_)
        | PadEdit::ZoneSpokeWidth(_)
        | PadEdit::ZoneGap(_)
        | PadEdit::ZoneAngle(_) => {
            let before = pad.zone_connection.unwrap_or_default();
            let mut z = before;
            match edit {
                PadEdit::ZoneStyle(s) => z.style = *s,
                PadEdit::ZoneSpokes(s) => z.spokes = *s,
                PadEdit::ZoneSpokeWidth(w) => z.spoke_width = *w,
                PadEdit::ZoneGap(g) => z.gap = *g,
                PadEdit::ZoneAngle(a) => z.angle = *a,
                _ => unreachable!("a Zone connection edit"),
            }
            if z == before {
                return None;
            }
            set(
                "zone_connection",
                if z == ZoneConnection::default() {
                    Value::Null
                } else {
                    json!(z)
                },
            );
        }
    }
    Some(p)
}

fn changed(
    set: &mut impl FnMut(&str, Value),
    key: &str,
    differs: bool,
    value: Value,
) -> Option<()> {
    differs.then(|| set(key, value))
}

fn mask_json(mask: Mask) -> Value {
    if mask == Mask::default() {
        Value::Null
    } else {
        json!(mask)
    }
}

fn hole_json(hole: &Hole) -> Value {
    let mut h = json!({"width": hole.width.0, "height": hole.height.0});
    if hole.rotation != Millideg::ZERO {
        h["rotation"] = json!(hole.rotation.0);
    }
    if let Some(t) = hole.tolerance {
        h["tolerance"] = json!({"plus": t.plus.0, "minus": t.minus.0});
    }
    h
}

/// The `library.footprint.pad.edit` for `edit` on the Pad `pad` of the open Footprint
/// `footprint`.
pub fn footprint_command(
    footprint: reda_engine::model::Uuid,
    pad: &Pad,
    edit: &PadEdit,
) -> Option<(&'static str, Value)> {
    let mut p = patch(pad, edit)?;
    p.insert("footprint".into(), json!(footprint));
    p.insert("pad".into(), json!(pad.uuid));
    Some(("library.footprint.pad.edit", Value::Object(p)))
}

/// The `pcb.free_pad.edit` for `edit` on the Free Pad `free`. A Free Pad has no number.
pub fn free_pad_command(free: &FreePad, edit: &PadEdit) -> Option<(&'static str, Value)> {
    if matches!(edit, PadEdit::Number(_)) {
        return None;
    }
    let mut p = patch(&from_free(free), edit)?;
    p.insert("pad".into(), json!(free.uuid));
    Some(("pcb.free_pad.edit", Value::Object(p)))
}
