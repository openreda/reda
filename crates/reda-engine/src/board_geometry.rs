//! What a PCB Scene needs from `reda-pcb`: placed Pads and Footprint drawing, laid-out
//! text, the Ratsnest and Zone fill states. `reda-render` does not depend on `reda-pcb`,
//! so the engine works these out and hands them over ([`reda_render::BoardParts`]).

use reda_command::Fonts;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    FootprintInstance, FootprintLayer, FootprintText, HERSHEY_FONT, LayerKind, PcbDocument,
    PcbText, Project, Shape, Side, Uuid,
};
use reda_pcb::placed::{self, Frame, PlacedPad};
use reda_pcb::zone::{self, FillState};
use reda_pcb::{Copper, PlacedText, text_geometry};
use reda_render::{BoardGeometry, BoardGraphic, BoardPad, BoardText, RatsnestLine, ZoneFillState};
use reda_render::{Mesh, Segment};

/// Footprint text pens are this share of the text height.
const PEN: (i64, i64) = (3, 20);

/// Everything `reda_render::pcb_scene` needs from PCB behaviour for `pcb`.
pub(crate) fn board_geometry(project: &Project, pcb: &PcbDocument, fonts: &Fonts) -> BoardGeometry {
    let mut out = BoardGeometry::default();
    for instance in pcb.footprints() {
        footprint(pcb, instance, fonts, &mut out);
    }
    out.pads.extend(
        pcb.free_pads()
            .map(|p| board_pad(p.uuid, placed::placed_free_pad(p))),
    );
    out.texts.extend(
        pcb.texts()
            .map(|t| board_text(t.uuid, t, text_geometry(t, fonts))),
    );
    let fills = project.zone_fills(pcb.uuid);
    let mut copper = Copper::build(pcb);
    if fills.is_some() {
        copper.set_zone_fills(pcb, fills);
    }
    out.ratsnest = copper
        .ratsnest()
        .into_iter()
        .map(|l| RatsnestLine {
            net: l.net,
            from: l.from,
            to: l.to,
        })
        .collect();
    out.zones = zone::status(pcb, fills)
        .zones
        .into_iter()
        .map(|z| {
            let state = match z.state {
                FillState::Filled => ZoneFillState::Filled,
                FillState::Stale => ZoneFillState::Stale,
                FillState::Unfilled => ZoneFillState::Unfilled,
            };
            (z.zone, state)
        })
        .collect();
    out
}

/// One Footprint's Pads, drawing, Designator and value text.
fn footprint(
    pcb: &PcbDocument,
    instance: &FootprintInstance,
    fonts: &Fonts,
    out: &mut BoardGeometry,
) {
    let object = instance.uuid;
    out.pads.extend(
        placed::placed_pads(pcb, instance)
            .into_iter()
            .map(|p| board_pad(object, p)),
    );
    let frame = Frame::of(instance);
    for g in placed::placed_graphics(pcb, instance) {
        let Some(layer) = layer_of(pcb, g.layer) else {
            continue;
        };
        if let Shape::Text {
            at,
            text,
            size,
            rotation,
        } = &g.shape
        {
            // Footprint text is anchored at the start of its baseline, like free text.
            let t = pcb_text(
                object,
                layer,
                text,
                *at,
                *size,
                *rotation,
                frame.side == Side::Bottom,
            );
            out.texts
                .push(board_text(object, &t, text_geometry(&t, fonts)));
            continue;
        }
        out.graphics.push(BoardGraphic {
            object,
            layer,
            shape: g.shape,
            width: g.width,
            fill: g.fill,
        });
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
        if let Some(t) = centred_text(pcb, object, text, &placement, fonts) {
            out.texts.push(t);
        }
    }
}

fn board_pad(object: Uuid, p: PlacedPad) -> BoardPad {
    BoardPad {
        object,
        number: p.number,
        kind: p.kind,
        shape: p.shape,
        at: p.at,
        rotation: p.rotation,
        width: p.width,
        height: p.height,
        corner_radius: p.corner_radius,
        hole: p.hole,
        side: p.side,
        net: p.net,
    }
}

/// The board Layer a Footprint drawing Layer is.
fn layer_of(pcb: &PcbDocument, layer: FootprintLayer) -> Option<Uuid> {
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

/// A Designator or value text centred on its placement, mirrored on a bottom Layer.
fn centred_text(
    pcb: &PcbDocument,
    object: Uuid,
    text: &str,
    placement: &FootprintText,
    fonts: &Fonts,
) -> Option<BoardText> {
    let layer = layer_of(pcb, placement.layer)?;
    let mirrored = matches!(
        placement.layer,
        FootprintLayer::SilkscreenBottom
            | FootprintLayer::CourtyardBottom
            | FootprintLayer::AssemblyBottom
    );
    let size = placement.size;
    // Lay it out at the origin to find its width, then move its centre onto `at`.
    let probe = pcb_text(
        object,
        layer,
        text,
        Point::default(),
        size,
        Millideg::ZERO,
        false,
    );
    let laid = text_geometry(&probe, fonts);
    let xs = laid.strokes.iter().flat_map(|s| [s.a.x, s.b.x]);
    let xs = xs.chain(laid.fill.points.iter().map(|p| p.x));
    let (lo, hi) = xs.fold((Nm(i64::MAX), Nm(i64::MIN)), |(lo, hi), x| {
        (lo.min(x), hi.max(x))
    });
    let span = if lo <= hi { (hi + lo).0 } else { 0 };
    let lines = text.split('\n').count() as i64;
    let pitch = size.0 * reda_pcb::LINE_PITCH.0 / reda_pcb::LINE_PITCH.1;
    // The middle of the block, from the start of the first baseline, unmirrored.
    let (cx, cy) = (span / 2, size.0 / 2 - (lines - 1) * pitch / 2);
    let cx = if mirrored { -cx } else { cx };
    let (dx, dy) = rotate(cx, cy, placement.rotation);
    let position = Point::new(placement.at.x - Nm(dx), placement.at.y - Nm(dy));
    let t = pcb_text(
        object,
        layer,
        text,
        position,
        size,
        placement.rotation,
        mirrored,
    );
    Some(board_text(object, &t, text_geometry(&t, fonts)))
}

fn rotate(x: i64, y: i64, angle: Millideg) -> (i64, i64) {
    let p = placed::rotate(Point::new(Nm(x), Nm(y)), Point::default(), angle);
    (p.x.0, p.y.0)
}

fn board_text(object: Uuid, text: &PcbText, laid: PlacedText) -> BoardText {
    BoardText {
        object,
        layer: text.layer,
        text: text.text.clone(),
        at: text.position,
        height: text.height,
        rotation: text.rotation,
        strokes: laid
            .strokes
            .iter()
            .map(|s| Segment::new(s.a, s.b))
            .collect(),
        stroke_width: laid.stroke_width,
        fill: Mesh {
            points: laid.fill.points,
            indices: laid.fill.indices,
        },
    }
}
