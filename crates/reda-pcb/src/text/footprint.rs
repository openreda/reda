//! The text a placed Footprint draws: text in its drawing, its Designator and its value,
//! as PCB text objects on board Layers. The canvas and the Gerber silkscreen both lay these
//! out with [`text_geometry`](super::text_geometry), so they agree glyph for glyph.

use reda_command::Fonts;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    FootprintInstance, FootprintLayer, FootprintText, HERSHEY_FONT, LayerKind, PcbDocument,
    PcbText, Shape, Side, Uuid,
};

use crate::placed::{self, Bounds, Frame};

/// Footprint text pens are this share of the text height.
const PEN: (i64, i64) = (3, 20);

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

/// Every text a placed Footprint draws, as Hershey PCB text whose `uuid` is the instance:
/// the text shapes of its drawing (anchored at the start of the baseline), then its
/// Designator and its value (centred on their placements) when visible and not empty.
/// Text on a Layer the board lacks is left out.
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
    let copy = pcb.footprint_copy(instance.footprint);
    let designator = placed::placed_designator(pcb, instance);
    let value = copy.map(|c| frame.text(&c.footprint.value));
    let texts = [
        (designator, instance.designator.as_str()),
        (value, instance.fields.value.as_str()),
    ];
    for (placement, text) in texts {
        let Some(placement) = placement.filter(|p| p.visible && !text.is_empty()) else {
            continue;
        };
        out.extend(centred_text(pcb, object, text, &placement, fonts));
    }
    out
}

/// Hershey text of a Footprint, pen `PEN` of its height.
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
        font: HERSHEY_FONT.to_owned(),
        position,
        height,
        thickness: Nm(height.0 * PEN.0 / PEN.1),
        rotation,
        mirrored,
    }
}

/// A Designator or value text centred on its placement, mirrored on a bottom Layer: the
/// centre of the glyphs' box (strokes and fill) lands on `at`. This is the one place the
/// canvas and the Gerber silkscreen centre Footprint text; DRC must centre it the same way.
fn centred_text(
    pcb: &PcbDocument,
    object: Uuid,
    text: &str,
    placement: &FootprintText,
    fonts: &Fonts,
) -> Option<PcbText> {
    let layer = board_layer(pcb, placement.layer)?;
    let mirrored = matches!(
        placement.layer,
        FootprintLayer::SilkscreenBottom
            | FootprintLayer::CourtyardBottom
            | FootprintLayer::AssemblyBottom
    );
    let size = placement.size;
    // Lay it out upright at the origin to find its centre, then move that onto `at`.
    let probe = pcb_text(
        object,
        layer,
        text,
        Point::default(),
        size,
        Millideg::ZERO,
        false,
    );
    let laid = super::text_geometry(&probe, fonts);
    let points = laid.strokes.iter().flat_map(|s| [s.a, s.b]);
    let points = points.chain(laid.fill.points.iter().copied());
    let centre = Bounds::of_points(points).map_or(Point::default(), |b| b.center());
    let centre = if mirrored {
        Point::new(-centre.x, centre.y)
    } else {
        centre
    };
    let d = placed::rotate(centre, Point::default(), placement.rotation);
    let position = Point::new(placement.at.x - d.x, placement.at.y - d.y);
    Some(pcb_text(
        object,
        layer,
        text,
        position,
        size,
        placement.rotation,
        mirrored,
    ))
}
