//! Per-target Teardrop values. A scoped rule can override each shape value independently.
use super::{RuleField, RuleValue, RuleValues, TeardropTarget, ViaRuleValues};
use reda_geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TeardropKind {
    ThroughHolePad,
    SmdPad,
    Via,
    WidthChange,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TeardropField {
    Enabled,
    Length,
    MaxLength,
    Width,
    MaxWidth,
    Curved,
    SkipRatio,
}
impl TeardropKind {
    pub const ALL: [Self; 4] = [
        Self::ThroughHolePad,
        Self::SmdPad,
        Self::Via,
        Self::WidthChange,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::ThroughHolePad => "through_hole_pad",
            Self::SmdPad => "smd_pad",
            Self::Via => "via",
            Self::WidthChange => "width_change",
        }
    }
}

impl TeardropField {
    pub const ALL: [Self; 7] = [
        Self::Enabled,
        Self::Length,
        Self::MaxLength,
        Self::Width,
        Self::MaxWidth,
        Self::Curved,
        Self::SkipRatio,
    ];
    pub fn path(self, kind: TeardropKind) -> &'static str {
        const PATHS: [[&str; 7]; 4] = [
            [
                "through_hole_pad.enabled",
                "through_hole_pad.length",
                "through_hole_pad.max_length",
                "through_hole_pad.width",
                "through_hole_pad.max_width",
                "through_hole_pad.curved",
                "through_hole_pad.skip_ratio",
            ],
            [
                "smd_pad.enabled",
                "smd_pad.length",
                "smd_pad.max_length",
                "smd_pad.width",
                "smd_pad.max_width",
                "smd_pad.curved",
                "smd_pad.skip_ratio",
            ],
            [
                "via.enabled",
                "via.length",
                "via.max_length",
                "via.width",
                "via.max_width",
                "via.curved",
                "via.skip_ratio",
            ],
            [
                "width_change.enabled",
                "width_change.length",
                "width_change.max_length",
                "width_change.width",
                "width_change.max_width",
                "width_change.curved",
                "width_change.skip_ratio",
            ],
        ];
        PATHS[kind as usize][self as usize]
    }
}

pub(super) const FIELDS: [RuleField; 28] = {
    let mut fields = [RuleField::Teardrop(TeardropKind::Via, TeardropField::Enabled); 28];
    let mut i = 0;
    while i < fields.len() {
        fields[i] = RuleField::Teardrop(TeardropKind::ALL[i / 7], TeardropField::ALL[i % 7]);
        i += 1;
    }
    fields
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TeardropValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<Nm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<Nm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curved: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_ratio: Option<u32>,
}

impl From<TeardropTarget> for TeardropValues {
    fn from(target: TeardropTarget) -> Self {
        Self {
            enabled: Some(target.enabled),
            length: Some(target.length),
            max_length: Some(target.max_length),
            width: Some(target.width),
            max_width: Some(target.max_width),
            curved: Some(target.curved),
            skip_ratio: Some(target.skip_ratio),
        }
    }
}
impl TeardropValues {
    pub(super) fn get(self, field: TeardropField) -> Option<RuleValue> {
        match field {
            TeardropField::Enabled => self.enabled.map(RuleValue::Flag),
            TeardropField::Length => self.length.map(RuleValue::UnsignedPercent),
            TeardropField::MaxLength => self.max_length.map(RuleValue::Length),
            TeardropField::Width => self.width.map(RuleValue::UnsignedPercent),
            TeardropField::MaxWidth => self.max_width.map(RuleValue::Length),
            TeardropField::Curved => self.curved.map(RuleValue::Flag),
            TeardropField::SkipRatio => self.skip_ratio.map(RuleValue::UnsignedPercent),
        }
    }
    fn set(&mut self, field: TeardropField, value: Option<RuleValue>) -> Result<(), String> {
        match (field, value) {
            (TeardropField::Enabled, None) => self.enabled = None,
            (TeardropField::Enabled, Some(RuleValue::Flag(v))) => self.enabled = Some(v),
            (TeardropField::Length, None) => self.length = None,
            (TeardropField::Length, Some(RuleValue::UnsignedPercent(v))) => self.length = Some(v),
            (TeardropField::MaxLength, None) => self.max_length = None,
            (TeardropField::MaxLength, Some(RuleValue::Length(v))) => self.max_length = Some(v),
            (TeardropField::Width, None) => self.width = None,
            (TeardropField::Width, Some(RuleValue::UnsignedPercent(v))) => self.width = Some(v),
            (TeardropField::MaxWidth, None) => self.max_width = None,
            (TeardropField::MaxWidth, Some(RuleValue::Length(v))) => self.max_width = Some(v),
            (TeardropField::Curved, None) => self.curved = None,
            (TeardropField::Curved, Some(RuleValue::Flag(v))) => self.curved = Some(v),
            (TeardropField::SkipRatio, None) => self.skip_ratio = None,
            (TeardropField::SkipRatio, Some(RuleValue::UnsignedPercent(v))) => {
                self.skip_ratio = Some(v)
            }
            _ => return Err(format!("wrong value type for Teardrop field {field:?}")),
        }
        Ok(())
    }
    /// Applies the available fields over the lower-priority resolved shape.
    pub fn apply(self, target: &mut TeardropTarget) {
        if let Some(value) = self.enabled {
            target.enabled = value;
        }
        if let Some(value) = self.length {
            target.length = value;
        }
        if let Some(value) = self.max_length {
            target.max_length = value;
        }
        if let Some(value) = self.width {
            target.width = value;
        }
        if let Some(value) = self.max_width {
            target.max_width = value;
        }
        if let Some(value) = self.curved {
            target.curved = value;
        }
        if let Some(value) = self.skip_ratio {
            target.skip_ratio = value;
        }
    }
}

impl RuleValues {
    pub fn teardrop_values(&self, kind: TeardropKind) -> Option<TeardropValues> {
        match kind {
            TeardropKind::ThroughHolePad => self.through_hole_pad,
            TeardropKind::SmdPad => self.smd_pad,
            TeardropKind::WidthChange => self.width_change,
            TeardropKind::Via => match self.via {
                Some(ViaRuleValues::Teardrop(t)) => Some(t),
                _ => None,
            },
        }
    }
    pub(super) fn set_teardrop(
        &mut self,
        kind: TeardropKind,
        field: TeardropField,
        value: Option<RuleValue>,
    ) -> Result<(), String> {
        let mut entry = self.teardrop_values(kind).unwrap_or_default();
        entry.set(field, value)?;
        let entry = (entry != TeardropValues::default()).then_some(entry);
        match kind {
            TeardropKind::ThroughHolePad => self.through_hole_pad = entry,
            TeardropKind::SmdPad => self.smd_pad = entry,
            TeardropKind::WidthChange => self.width_change = entry,
            TeardropKind::Via => self.via = entry.map(ViaRuleValues::Teardrop),
        }
        Ok(())
    }
}
