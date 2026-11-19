//! Single-ended Track impedance at the Default Net Class width on each physical Layer.
mod formulas;

use reda_model::geom::Nm;
use reda_model::{
    CopperRole, DielectricEntry, PcbDocument, RuleField, RuleKind, StackupEntry, StackupInfo, Uuid,
};
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Model {
    SurfaceMicrostrip,
    CentredStripline,
    OffsetStripline,
    EmbeddedMicrostrip,
}

#[derive(Clone, Debug, PartialEq, JsonSchema, Serialize)]
pub struct Estimate {
    pub ohms: f64,
    pub model: Model,
    pub approximate: bool,
    /// Reference Layer UUIDs, in physical order.
    pub references: Vec<Uuid>,
    /// At least one sheet in the field region used the 4.5 fallback.
    pub missing_dk: bool,
}

#[derive(Clone, Debug, PartialEq, JsonSchema, Serialize)]
pub struct LayerImpedance {
    pub layer: Uuid,
    pub width: Nm,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<Estimate>,
}

/// A path from the signal Track to the nearest reference, or to the edge of the stack.
#[derive(Default)]
struct Gap<'a> {
    reference: Option<Uuid>,
    sheets: Vec<(&'a DielectricEntry, f64)>,
    height: f64,
}

fn gap<'a>(
    pcb: &'a PcbDocument,
    info: &StackupInfo,
    indices: impl Iterator<Item = usize>,
) -> Gap<'a> {
    let mut gap = Gap::default();
    for i in indices {
        match &pcb.stackup().entries[i] {
            StackupEntry::Copper(copper) => {
                let layer = pcb
                    .layers()
                    .layer(copper.layer)
                    .expect("Stackup names a copper Layer");
                if layer.role != CopperRole::Signal {
                    gap.reference = Some(copper.layer);
                    break;
                }
            }
            StackupEntry::Dielectric(sheet) => {
                let thickness = info.entries[i].thickness;
                gap.height += thickness;
                gap.sheets.push((sheet, thickness));
            }
        }
    }
    gap
}

fn estimate(
    width: f64,
    thickness: f64,
    upper: &Gap<'_>,
    lower: &Gap<'_>,
    outer: bool,
) -> Option<Estimate> {
    let refs: Vec<_> = [upper.reference, lower.reference]
        .into_iter()
        .flatten()
        .collect();
    if refs.is_empty()
        || width <= 0.0
        || [upper, lower]
            .iter()
            .any(|gap| gap.reference.is_some() && gap.height <= 0.0)
    {
        return None;
    }

    let mut sheets = upper.sheets.iter().chain(&lower.sheets);
    let &(first, _) = sheets.next()?;
    let mut homogeneous = true;
    let mut missing_dk = false;
    let mut weighted_dk = 0.0;
    for &(sheet, height) in upper.sheets.iter().chain(&lower.sheets) {
        // Explicit material names distinguish laminates with the same nominal Dk. With
        // unnamed sheets, compare both their electrical value and their construction kind.
        homogeneous &= sheet.dk.unwrap_or(4500) == first.dk.unwrap_or(4500)
            && sheet.material == first.material
            && (sheet.material.is_some() || sheet.kind == first.kind);
        missing_dk |= sheet.dk.is_none();
        weighted_dk += height * f64::from(sheet.dk.unwrap_or(4500)) / 1000.0;
    }
    let dk = weighted_dk / (upper.height + lower.height);
    let (model, ohms) = match (upper.reference, lower.reference) {
        (Some(_), Some(_)) => {
            let tolerance = 1e-12 * upper.height.max(lower.height);
            if homogeneous && (upper.height - lower.height).abs() <= tolerance {
                (
                    Model::CentredStripline,
                    formulas::centred(width, thickness, upper.height, dk),
                )
            } else {
                (
                    Model::OffsetStripline,
                    formulas::offset(width, thickness, upper.height, lower.height, dk),
                )
            }
        }
        _ => {
            let (plane, cover) = if upper.reference.is_some() {
                (upper, lower)
            } else {
                (lower, upper)
            };
            if outer {
                (
                    Model::SurfaceMicrostrip,
                    formulas::surface(width, thickness, plane.height, dk),
                )
            } else {
                (
                    Model::EmbeddedMicrostrip,
                    formulas::embedded(width, thickness, plane.height, cover.height, dk),
                )
            }
        }
    };
    (ohms.is_finite() && ohms > 0.0).then_some(Estimate {
        ohms,
        model,
        approximate: missing_dk
            || matches!(model, Model::OffsetStripline | Model::EmbeddedMicrostrip),
        references: refs,
        missing_dk,
    })
}

/// Resolve the Default Net Class Track width on each copper Layer and use the Stackup's
/// resolved (possibly fractional) sheet thicknesses for the electrical model.
pub fn layers(pcb: &PcbDocument, info: &StackupInfo) -> Vec<LayerImpedance> {
    let entries = &pcb.stackup().entries;
    entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            let StackupEntry::Copper(copper) = entry else {
                return None;
            };
            let layer = pcb
                .layers()
                .layer(copper.layer)
                .expect("Stackup names a copper Layer");
            let width = crate::rules::resolve_on(
                pcb,
                None,
                Some(copper.layer),
                RuleKind::TrackWidth,
                RuleField::Preferred,
            )
            .and_then(|r| r.value.length())
            .expect("board-wide preferred Track width");
            let estimate = if layer.role == CopperRole::Plane {
                None
            } else {
                let upper = gap(pcb, info, (0..i).rev());
                let lower = gap(pcb, info, i + 1..entries.len());
                estimate(
                    width.0 as f64,
                    copper.thickness.0 as f64,
                    &upper,
                    &lower,
                    i == 0 || i + 1 == entries.len(),
                )
            };
            Some(LayerImpedance {
                layer: copper.layer,
                width,
                estimate,
            })
        })
        .collect()
}
