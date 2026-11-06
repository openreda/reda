//! The text a placed Footprint draws: text in its drawing and its field texts (such as its
//! Designator and value), as PCB text objects on board Layers. The canvas, the Gerber silkscreen and DRC's
//! silkscreen check all lay these out with [`text_geometry`](super::text_geometry), so they
//! agree glyph for glyph.

use reda_command::Fonts;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    DESIGNATOR_FIELD, Footprint, FootprintInstance, FootprintLayer, FootprintText, LayerKind,
    PcbDocument, PcbText, Shape, Side, TextAutoposition, TextStyle, Uuid, VALUE_FIELD, field_text,
};

use crate::placed::{self, Bounds, Frame};

/// The pen width of text in a Footprint's drawing, as a fraction of its height (`.0 /
/// .1`). Field texts carry their own pen width, which starts at the same fraction.
pub const FOOTPRINT_TEXT_PEN: (i64, i64) = TextStyle::PEN;

/// The board Layer a Footprint drawing Layer is, if the board has it.
pub fn board_layer(pcb: &PcbDocument, layer: FootprintLayer) -> Option<Uuid> {
    use FootprintLayer::*;
    let (kind, side) = match layer {
        SilkscreenTop => (LayerKind::Silkscreen, Side::Top),
        SilkscreenBottom => (LayerKind::Silkscreen, Side::Bottom),
        CourtyardTop => (LayerKind::Courtyard, Side::Top),
        CourtyardBottom => (LayerKind::Courtyard, Side::Bottom),
        AssemblyTop => (LayerKind::Assembly, Side::Top),
        AssemblyBottom => (LayerKind::Assembly, Side::Bottom),
    };
    pcb.layers().find(kind, Some(side)).map(|l| l.uuid)
}

/// Every text a placed Footprint draws, as PCB text whose `uuid` is the instance: the
/// text shapes of its drawing (Hershey, anchored at the start of the baseline), then each
/// of its field texts (centred on its placement) when visible and the instance has that
/// field. Text on a Layer the board lacks is left out.
pub fn footprint_texts(
    pcb: &PcbDocument,
    instance: &FootprintInstance,
    fonts: &Fonts,
) -> Vec<PcbText> {
    let object = instance.uuid;
    let frame = Frame::of(instance);
    let mut out = Vec::new();
    for g in placed::placed_graphics(pcb, instance) {
        let Shape::Text {
            at,
            text,
            size,
            rotation,
        } = &g.shape
        else {
            continue;
        };
        if let Some(layer) = board_layer(pcb, g.layer) {
            out.push(pcb_text(
                object,
                layer,
                text,
                *at,
                *size,
                *rotation,
                frame.side == Side::Bottom,
            ));
        }
    }
    for entry in placed_field_texts(pcb, instance, fonts) {
        let text = field_text(&entry.field, &instance.designator, &instance.fields);
        if !entry.visible || text.is_empty() {
            continue;
        }
        out.extend(centred_text(pcb, object, text, &entry, fonts));
    }
    out
}

/// Resolved field text placements in board coordinates, including Autoposition.
pub fn placed_field_texts(
    pcb: &PcbDocument,
    instance: &FootprintInstance,
    fonts: &Fonts,
) -> Vec<FootprintText> {
    use TextAutoposition::*;
    let frame = Frame::of(instance);
    let copy = pcb
        .footprint_copy(instance.footprint)
        .expect("committed Footprint copy");
    let local_box = crate::cluster::footprint_box(&copy.footprint);
    let corners = [
        local_box.min,
        Point::new(local_box.max.x, local_box.min.y),
        local_box.max,
        Point::new(local_box.min.x, local_box.max.y),
    ];
    let bounds = Bounds::of_points(corners.map(|p| frame.point(p))).expect("four corners");
    placed::instance_texts(pcb, instance)
        .into_iter()
        .map(|entry| {
            let mut placed = frame.text(&entry);
            let mode = instance
                .text_overrides
                .iter()
                .find(|o| o.text == entry.uuid)
                .and_then(|o| o.autoposition)
                .unwrap_or_default();
            if mode == Manual {
                return placed;
            }
            // Automatic placements read upright on both sides and after any Footprint turn.
            placed.rotation = Millideg::ZERO;
            let content = field_text(&entry.field, &instance.designator, &instance.fields);
            let Some(mut probe) = centred_text(pcb, instance.uuid, content, &placed, fonts) else {
                return placed;
            };
            probe.position = Point::default();
            let geometry = super::text_geometry(&probe, fonts);
            let r = Nm(geometry.stroke_width.0 / 2);
            let ink = geometry
                .fill
                .points
                .iter()
                .map(|&p| Bounds::of_point(p))
                .chain(
                    geometry
                        .strokes
                        .iter()
                        .flat_map(|s| [s.a, s.b])
                        .map(|p| Bounds {
                            min: Point::new(p.x - r, p.y - r),
                            max: Point::new(p.x + r, p.y + r),
                        }),
                )
                .reduce(Bounds::union)
                .unwrap_or_else(|| Bounds::of_point(Point::default()));
            let mid = |a: Nm, b: Nm| Nm((a.0 + b.0) / 2);
            let x = match mode {
                LeftAbove | LeftCenter | LeftBelow => bounds.min.x - ink.max.x,
                RightAbove | RightCenter | RightBelow => bounds.max.x - ink.min.x,
                _ => mid(bounds.min.x, bounds.max.x) - mid(ink.min.x, ink.max.x),
            };
            let y = match mode {
                LeftAbove | CenterAbove | RightAbove => bounds.max.y - ink.min.y,
                LeftBelow | CenterBelow | RightBelow => bounds.min.y - ink.max.y,
                _ => mid(bounds.min.y, bounds.max.y) - mid(ink.min.y, ink.max.y),
            };
            placed.at = Point::new(x, y);
            placed
        })
        .collect()
}

/// Every text a Footprint draws as the Footprint editor shows it, unplaced (at its own
/// origin, top side), on the board Layers of `pcb`, each with the UUID of its graphic or
/// field text: its drawing's text shapes, then its field texts, laid out exactly as on a
/// board. The Designator shows as `designator`, the value as `value` and any other field
/// as its name. Each PCB text's `uuid` is the Footprint's.
pub fn element_texts(
    pcb: &PcbDocument,
    footprint: &Footprint,
    (designator, value): (&str, &str),
    fonts: &Fonts,
) -> Vec<(Uuid, PcbText)> {
    let object = footprint.uuid;
    let mut out = Vec::new();
    for g in &footprint.graphics {
        let Shape::Text {
            at,
            text,
            size,
            rotation,
        } = &g.shape
        else {
            continue;
        };
        if let Some(layer) = board_layer(pcb, g.layer) {
            let mirrored = g.layer.is_bottom();
            let t = pcb_text(object, layer, text, *at, *size, *rotation, mirrored);
            out.push((g.uuid, t));
        }
    }
    for entry in &footprint.texts {
        let text = match entry.field.as_str() {
            DESIGNATOR_FIELD => designator,
            VALUE_FIELD => value,
            field => field,
        };
        if !entry.visible || text.is_empty() {
            continue;
        }
        if let Some(t) = centred_text(pcb, object, text, entry, fonts) {
            out.push((entry.uuid, t));
        }
    }
    out
}

/// Hershey text of a Footprint's drawing, pen [`FOOTPRINT_TEXT_PEN`] of its height.
fn pcb_text(
    object: Uuid,
    layer: Uuid,
    text: &str,
    position: Point,
    height: Nm,
    rotation: Millideg,
    mirrored: bool,
) -> PcbText {
    PcbText {
        uuid: object,
        layer,
        text: text.to_owned(),
        position,
        style: TextStyle {
            justify: Default::default(),
            ..TextStyle::of_height(height)
        },
        frame: None,
        rotation,
        mirrored,
    }
}

/// A field text centred on its placement, mirrored on a bottom Layer: the centre of the
/// glyphs' box (strokes and fill) lands on `at`. This is the one place the canvas, the
/// Gerber silkscreen and DRC centre Footprint text.
fn centred_text(
    pcb: &PcbDocument,
    _object: Uuid,
    text: &str,
    placement: &FootprintText,
    _fonts: &Fonts,
) -> Option<PcbText> {
    let layer = board_layer(pcb, placement.layer)?;
    let mirrored = placement.layer.is_bottom();
    Some(PcbText {
        uuid: placement.uuid,
        layer,
        text: text.to_owned(),
        position: placement.at,
        style: placement.style.clone(),
        frame: None,
        rotation: placement.rotation,
        mirrored,
    })
}
