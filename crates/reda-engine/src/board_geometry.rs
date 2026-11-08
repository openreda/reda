//! What a PCB Scene needs from `reda-pcb`: placed Pads and Footprint drawing, laid-out
//! text, Dimensions as lines and text, the Ratsnest and Zone fill states. `reda-render`
//! does not depend on `reda-pcb`, so the engine works these out and hands them over ([`reda_render::BoardParts`]).

use reda_command::Fonts;
use reda_model::{FootprintInstance, PcbDocument, PcbText, Project, Shape, Uuid};
use reda_pcb::placed::{self, PlacedPad};
use reda_pcb::zone::{self, FillState};
use reda_pcb::{Copper, PlacedText, board_layer, footprint_texts, text_geometry};
use reda_render::{BoardGeometry, BoardGraphic, BoardPad, BoardText, RatsnestLine, ZoneFillState};
use reda_render::{Mesh, Segment};

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
    for dimension in pcb.dimensions() {
        let drawn = reda_pcb::dimension::drawing(dimension, fonts);
        out.graphics
            .extend(drawn.lines.iter().map(|line| BoardGraphic {
                object: dimension.uuid,
                layer: dimension.layer,
                shape: Shape::Line {
                    start: line.from,
                    end: line.to,
                },
                width: line.width,
                fill: false,
            }));
        out.texts.extend(
            drawn
                .text
                .map(|t| board_text(dimension.uuid, &t, text_geometry(&t, fonts))),
        );
    }
    let fills = project.zone_fills(pcb.uuid);
    let copper = Copper::of(project).expect("the Project's PCB Document");
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
    for g in placed::placed_graphics(pcb, instance) {
        if matches!(g.shape, Shape::Text { .. }) {
            continue;
        }
        let Some(layer) = board_layer(pcb, g.layer) else {
            continue;
        };
        out.graphics.push(BoardGraphic {
            object,
            layer,
            shape: g.shape,
            width: g.width,
            fill: g.fill,
        });
    }
    out.texts
        .extend(footprint_texts(pcb, instance, fonts).iter().map(|t| {
            let mut text = board_text(object, t, text_geometry(t, fonts));
            if pcb.footprint_copy(instance.footprint).is_some_and(|copy| {
                copy.footprint
                    .texts
                    .iter()
                    .any(|entry| entry.uuid == t.uuid)
            }) {
                text.child = Some(reda_render::Child::FootprintText {
                    footprint: object,
                    text: t.uuid,
                });
            }
            text
        }));
}

fn board_pad(object: Uuid, p: PlacedPad) -> BoardPad {
    BoardPad {
        child: (object != p.pad).then_some(reda_render::Child::Pad {
            footprint: object,
            pad: p.pad,
        }),
        object,
        number: p.number,
        kind: p.kind,
        at: p.at,
        rotation: p.rotation,
        outline: p.outline,
        hole: p.hole,
        side: p.side,
        net: p.net,
    }
}

fn board_text(object: Uuid, text: &PcbText, laid: PlacedText) -> BoardText {
    BoardText {
        align: text.justify.into(),
        child: None,
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
