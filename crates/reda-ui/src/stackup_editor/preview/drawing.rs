//! Geometry shared by painting, picking and dragging the cross-section.
use egui::{Pos2, Rect, pos2};
use reda_engine::model::{PcbDocument, StackupEntry, StackupInfo, Uuid, ViaType};

use super::Scale;

pub(super) struct Band {
    pub id: Uuid,
    pub rect: Rect,
    pub copper: bool,
    pub label_y: f32,
}

/// The first endpoint is the drilling origin, including for upward-drilled micro Vias.
pub(super) struct Barrel {
    pub x: f32,
    pub from: Rect,
    pub to: Rect,
    pub hole: [Pos2; 4],
    pub pads: Vec<Rect>,
    pub bounds: Rect,
}

pub(super) fn bands(
    pcb: &PcbDocument,
    info: &StackupInfo,
    body: Rect,
    scale: Scale,
    line: f32,
) -> Vec<Band> {
    let entries = &pcb.stackup().entries;
    let weights = entries
        .iter()
        .zip(&info.entries)
        .map(|(entry, resolved)| match scale {
            Scale::Physical => resolved.thickness.max(0.0),
            Scale::Schematic => {
                if matches!(entry, StackupEntry::Copper(_)) {
                    16.0
                } else {
                    46.0
                }
            }
        })
        .collect::<Vec<_>>();
    let total: f64 = weights.iter().sum();
    let mut y = body.top();
    let mut next_label = f32::NEG_INFINITY;
    let mut bands = Vec::with_capacity(entries.len());
    for (entry, weight) in entries.iter().zip(weights) {
        let height = if total > 0.0 {
            (weight / total) as f32 * body.height()
        } else {
            body.height() / entries.len() as f32
        };
        let rect = Rect::from_x_y_ranges(body.x_range(), y..=y + height);
        let label_y = rect.center().y.max(next_label);
        next_label = label_y + line;
        bands.push(Band {
            id: entry.uuid(),
            rect,
            copper: matches!(entry, StackupEntry::Copper(_)),
            label_y,
        });
        y += height;
    }
    // Move tightly packed labels up from the bottom margin while preserving their order.
    let mut last_label = body.bottom();
    for band in bands.iter_mut().rev() {
        band.label_y = band.label_y.min(last_label);
        last_label = band.label_y - line;
    }
    bands
}

pub(super) fn barrel(bands: &[Band], kind: &ViaType, x: f32, max_radius: f32) -> Option<Barrel> {
    let first = bands.iter().position(|b| b.id == kind.from)?;
    let last = bands.iter().position(|b| b.id == kind.to)?;
    let from = bands[first].rect;
    let to = bands[last].rect;
    let (top, bottom) = (from.top().min(to.top()), from.bottom().max(to.bottom()));
    let (origin, end) = if first < last {
        (top, bottom)
    } else {
        (bottom, top)
    };
    let largest = [
        Some(kind.diameter.start),
        kind.diameter.middle,
        kind.diameter.end,
    ]
    .into_iter()
    .flatten()
    .max()?;
    let radius = |nm: i64| nm as f32 / largest.0 as f32 * max_radius;
    let hole_radius = radius(kind.hole.0);
    let end_radius = if kind.micro {
        hole_radius * 0.55
    } else {
        hole_radius
    };
    let hole = [
        pos2(x - hole_radius, origin),
        pos2(x + hole_radius, origin),
        pos2(x + end_radius, end),
        pos2(x - end_radius, end),
    ];
    let pads = bands[first.min(last)..=first.max(last)]
        .iter()
        .filter(|b| b.copper)
        .map(|band| {
            let diameter = kind.diameter.on(kind.span(), band.id);
            let r = radius(diameter.0);
            Rect::from_x_y_ranges(x - r..=x + r, band.rect.y_range())
        })
        .collect();
    let bounds = Rect::from_x_y_ranges(x - max_radius..=x + max_radius, top..=bottom);
    Some(Barrel {
        x,
        from,
        to,
        hole,
        pads,
        bounds,
    })
}

pub(super) fn nearest_copper(bands: &[Band], position: Pos2) -> Option<Uuid> {
    bands
        .iter()
        .filter(|b| b.copper)
        .min_by(|a, b| {
            let distance = |band: &Band| (band.rect.center().y - position.y).abs();
            distance(a).total_cmp(&distance(b))
        })
        .map(|b| b.id)
}
