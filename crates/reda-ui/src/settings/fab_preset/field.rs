//! The typed fields of the Fab Preset pages: each reads its value from a preset as text
//! and writes text back, or says why the text is not a value.

use reda_engine::model::geom::Nm;
use reda_engine::model::{BomGroup, ColumnSource, FabPreset, PasteExpansion};

use super::PresetPage;
use crate::footprint_editor::inspector::format_mm;
use crate::tr;

/// A field typed as text on a Fab Preset page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    Clearance,
    TrackWidth,
    ViaDiameter,
    ViaHole,
    HoleMin,
    HoleMax,
    AnnularRing,
    HoleToHole,
    EdgeClearance,
    PreferredTrackWidth,
    PreferredViaDiameter,
    PreferredViaHole,
    MaskTop,
    MaskBottom,
    /// The paste seed: a length per edge, or a percent of area, as the mode has it.
    Paste,
    SeedThickness,
    SeedCopper,
    SuggestThickness,
    SuggestCopper,
    SuggestFinish,
    SuggestMaskColour,
    SuggestSilkColour,
    GroupBy,
    Sort,
    LineStart,
    LineStep,
    RequiredFields,
}

impl Field {
    /// Every typed field, in page order.
    pub const ALL: [Field; 27] = [
        Field::Clearance,
        Field::TrackWidth,
        Field::ViaDiameter,
        Field::ViaHole,
        Field::HoleMin,
        Field::HoleMax,
        Field::AnnularRing,
        Field::HoleToHole,
        Field::EdgeClearance,
        Field::PreferredTrackWidth,
        Field::PreferredViaDiameter,
        Field::PreferredViaHole,
        Field::MaskTop,
        Field::MaskBottom,
        Field::Paste,
        Field::SeedThickness,
        Field::SeedCopper,
        Field::SuggestThickness,
        Field::SuggestCopper,
        Field::SuggestFinish,
        Field::SuggestMaskColour,
        Field::SuggestSilkColour,
        Field::GroupBy,
        Field::Sort,
        Field::LineStart,
        Field::LineStep,
        Field::RequiredFields,
    ];

    /// The preset key it edits, such as `rules.clearance`; a [`Problem`](crate::settings::Problem)
    /// on it is filed under this.
    pub fn id(self) -> &'static str {
        match self {
            Field::Clearance => "rules.clearance",
            Field::TrackWidth => "rules.track_width",
            Field::ViaDiameter => "rules.via_diameter",
            Field::ViaHole => "rules.via_hole",
            Field::HoleMin => "rules.hole_min",
            Field::HoleMax => "rules.hole_max",
            Field::AnnularRing => "rules.annular_ring",
            Field::HoleToHole => "rules.hole_to_hole",
            Field::EdgeClearance => "rules.edge_clearance",
            Field::PreferredTrackWidth => "rules.preferred.track_width",
            Field::PreferredViaDiameter => "rules.preferred.via_diameter",
            Field::PreferredViaHole => "rules.preferred.via_hole",
            Field::MaskTop => "rules.mask_expansion.top",
            Field::MaskBottom => "rules.mask_expansion.bottom",
            Field::Paste => "rules.paste_expansion",
            Field::SeedThickness => "stackup.thickness",
            Field::SeedCopper => "stackup.copper",
            Field::SuggestThickness => "stackup_suggestions.thickness",
            Field::SuggestCopper => "stackup_suggestions.copper",
            Field::SuggestFinish => "stackup_suggestions.finish",
            Field::SuggestMaskColour => "stackup_suggestions.mask_color",
            Field::SuggestSilkColour => "stackup_suggestions.silk_color",
            Field::GroupBy => "bom.group_by",
            Field::Sort => "bom.sort",
            Field::LineStart => "bom.line_numbers.start",
            Field::LineStep => "bom.line_numbers.step",
            Field::RequiredFields => "bom.required_fields",
        }
    }

    /// The page it is on.
    pub fn page(self) -> PresetPage {
        let id = self.id();
        PresetPage::ALL
            .into_iter()
            .find(|p| p.owns(id))
            .expect("every field is on a page")
    }

    /// Its value in `preset`, as the field shows it.
    pub fn text(self, preset: &FabPreset) -> String {
        let r = &preset.rules;
        let list = |items: &[String]| items.join(", ");
        match self {
            Field::Clearance => format_mm(r.clearance),
            Field::TrackWidth => format_mm(r.track_width),
            Field::ViaDiameter => format_mm(r.via_diameter),
            Field::ViaHole => format_mm(r.via_hole),
            Field::HoleMin => format_mm(r.hole_min),
            Field::HoleMax => format_mm(r.hole_max),
            Field::AnnularRing => format_mm(r.annular_ring),
            Field::HoleToHole => format_mm(r.hole_to_hole),
            Field::EdgeClearance => format_mm(r.edge_clearance),
            Field::PreferredTrackWidth => format_mm(r.preferred.track_width),
            Field::PreferredViaDiameter => format_mm(r.preferred.via_diameter),
            Field::PreferredViaHole => format_mm(r.preferred.via_hole),
            Field::MaskTop => format_mm(r.mask_expansion.top),
            Field::MaskBottom => format_mm(r.mask_expansion.bottom),
            Field::Paste => match r.paste_expansion {
                PasteExpansion::Expansion { expansion } => format_mm(expansion),
                PasteExpansion::Percent { percent } => percent.to_string(),
            },
            Field::SeedThickness => preset.stackup.thickness.map(format_mm).unwrap_or_default(),
            Field::SeedCopper => preset.stackup.copper.map(copper_text).unwrap_or_default(),
            Field::SuggestThickness => lengths(&preset.stackup_suggestions.thickness, format_mm),
            Field::SuggestCopper => lengths(&preset.stackup_suggestions.copper, copper_text),
            Field::SuggestFinish => list(&preset.stackup_suggestions.finish),
            Field::SuggestMaskColour => list(&preset.stackup_suggestions.mask_color),
            Field::SuggestSilkColour => list(&preset.stackup_suggestions.silk_color),
            Field::GroupBy => preset
                .bom
                .group_by
                .iter()
                .map(group_token)
                .collect::<Vec<_>>()
                .join(", "),
            Field::Sort => preset
                .bom
                .sort
                .iter()
                .map(source_token)
                .collect::<Vec<_>>()
                .join(", "),
            Field::LineStart => preset.bom.line_numbers.start.to_string(),
            Field::LineStep => preset.bom.line_numbers.step.to_string(),
            Field::RequiredFields => list(&preset.bom.required_fields),
        }
    }

    /// Writes `text` into `preset`, or says why it is not a value. Suggestion lists take
    /// any text: they are never checked.
    pub fn apply(self, preset: &mut FabPreset, text: &str) -> Result<(), String> {
        let r = &mut preset.rules;
        let names = |text: &str| -> Vec<String> {
            text.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        };
        match self {
            Field::Clearance => r.clearance = size(text)?,
            Field::TrackWidth => r.track_width = size(text)?,
            Field::ViaDiameter => r.via_diameter = size(text)?,
            Field::ViaHole => r.via_hole = size(text)?,
            Field::HoleMin => r.hole_min = size(text)?,
            Field::HoleMax => r.hole_max = size(text)?,
            Field::AnnularRing => r.annular_ring = size(text)?,
            Field::HoleToHole => r.hole_to_hole = size(text)?,
            Field::EdgeClearance => r.edge_clearance = size(text)?,
            Field::PreferredTrackWidth => r.preferred.track_width = size(text)?,
            Field::PreferredViaDiameter => r.preferred.via_diameter = size(text)?,
            Field::PreferredViaHole => r.preferred.via_hole = size(text)?,
            Field::MaskTop => r.mask_expansion.top = length(text)?,
            Field::MaskBottom => r.mask_expansion.bottom = length(text)?,
            Field::Paste => {
                r.paste_expansion = match r.paste_expansion {
                    PasteExpansion::Expansion { .. } => PasteExpansion::Expansion {
                        expansion: length(text)?,
                    },
                    PasteExpansion::Percent { .. } => PasteExpansion::Percent {
                        percent: text
                            .trim()
                            .trim_end_matches('%')
                            .trim()
                            .parse()
                            .map_err(|_| tr!("preset-not-percent"))?,
                    },
                }
            }
            Field::SeedThickness => preset.stackup.thickness = optional(text, size)?,
            Field::SeedCopper => preset.stackup.copper = optional(text, copper)?,
            Field::SuggestThickness => {
                preset.stackup_suggestions.thickness = each(text, size)?;
            }
            Field::SuggestCopper => preset.stackup_suggestions.copper = each(text, copper)?,
            Field::SuggestFinish => preset.stackup_suggestions.finish = names(text),
            Field::SuggestMaskColour => preset.stackup_suggestions.mask_color = names(text),
            Field::SuggestSilkColour => preset.stackup_suggestions.silk_color = names(text),
            Field::GroupBy => preset.bom.group_by = each(text, group)?,
            Field::Sort => preset.bom.sort = each(text, source)?,
            Field::LineStart => preset.bom.line_numbers.start = count(text, 0)?,
            Field::LineStep => preset.bom.line_numbers.step = count(text, 1)?,
            Field::RequiredFields => preset.bom.required_fields = names(text),
        }
        Ok(())
    }
}

/// A length: a bare number is mm, else a length with its unit (`10mil`).
fn length(text: &str) -> Result<Nm, String> {
    crate::footprint_editor::inspector::parse_length(text).ok_or_else(|| tr!("preset-not-length"))
}

/// A length that cannot be negative.
fn size(text: &str) -> Result<Nm, String> {
    match length(text)? {
        nm if nm.0 < 0 => Err(tr!("preset-negative")),
        nm => Ok(nm),
    }
}

/// A copper thickness: mm unless it names a unit (`35um`, `1oz`).
fn copper(text: &str) -> Result<Nm, String> {
    let text = text.trim();
    let bare = text.parse::<f64>().is_ok();
    let with_unit = if bare {
        format!("{text}mm")
    } else {
        text.to_owned()
    };
    match Nm::parse_copper(&with_unit) {
        Ok(nm) if nm.0 >= 0 => Ok(nm),
        _ => Err(tr!("preset-not-copper")),
    }
}

/// A copper thickness as ounces when it is a whole number of them, else in mm.
fn copper_text(nm: Nm) -> String {
    let oz = reda_engine::model::geom::NM_PER_OZ;
    if nm.0 > 0 && nm.0 % oz == 0 {
        format!("{}oz", nm.0 / oz)
    } else {
        format_mm(nm)
    }
}

fn lengths(items: &[Nm], text: fn(Nm) -> String) -> String {
    items
        .iter()
        .map(|n| text(*n))
        .collect::<Vec<_>>()
        .join(", ")
}

fn optional(text: &str, parse: fn(&str) -> Result<Nm, String>) -> Result<Option<Nm>, String> {
    if text.trim().is_empty() {
        Ok(None)
    } else {
        parse(text).map(Some)
    }
}

/// Each comma-separated item of `text`, blanks skipped.
fn each<T>(text: &str, parse: fn(&str) -> Result<T, String>) -> Result<Vec<T>, String> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse)
        .collect()
}

fn count(text: &str, min: u32) -> Result<u32, String> {
    match text.trim().parse::<u32>() {
        Ok(n) if n >= min => Ok(n),
        _ => Err(tr!("preset-not-count")),
    }
}

/// The token of a `group_by` entry: `part`, `footprint`, `fields`, `value` or
/// `field:<name>`.
fn group_token(group: &BomGroup) -> String {
    match group {
        BomGroup::Field(name) => format!("field:{name}"),
        other => serde_json::to_value(other)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
    }
}

fn group(token: &str) -> Result<BomGroup, String> {
    if let Some(name) = field_name(token) {
        return Ok(BomGroup::Field(name));
    }
    serde_json::from_value(serde_json::Value::String(token.to_owned()))
        .map_err(|_| tr!("preset-not-group", token = token))
}

/// The token of a column source: its snake_case name, or `field:<name>`.
pub fn source_token(source: &ColumnSource) -> String {
    match source {
        ColumnSource::Field(name) => format!("field:{name}"),
        other => serde_json::to_value(other)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
    }
}

fn source(token: &str) -> Result<ColumnSource, String> {
    if let Some(name) = field_name(token) {
        return Ok(ColumnSource::Field(name));
    }
    serde_json::from_value(serde_json::Value::String(token.to_owned()))
        .map_err(|_| tr!("preset-not-source", token = token))
}

fn field_name(token: &str) -> Option<String> {
    let name = token.strip_prefix("field:")?.trim();
    (!name.is_empty()).then(|| name.to_owned())
}
