//! The Stackup: the board's ordered copper and dielectric build, and its finish and
//! colours.

use reda_geom::{NM_PER_OZ, Nm};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{LayerSpan, Uuid};

/// The board's physical build, top to bottom. v0.1 boards hold one; the table leaves room
/// for more stacks later, such as rigid-flex substacks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stackup {
    pub uuid: Uuid,
    /// The original named Construction, including while the board differs from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction: Option<String>,
    /// The finished thickness you order.
    pub thickness: Nm,
    /// The copper Layers in physical order, with one or more dielectric sheets in every gap.
    pub entries: Vec<StackupEntry>,
    /// The ordered Via types, including the through type.
    pub via_types: Vec<ViaType>,
    /// The surface finish, as typed; left out when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    /// The solder mask colour, as typed; left out when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_color: Option<String>,
    /// The silkscreen colour, as typed; left out when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silk_color: Option<String>,
}

/// One entry of a [`Stackup`]. Solder mask, silkscreen and paste are not entries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum StackupEntry {
    /// A copper Layer's thickness.
    Copper(CopperEntry),
    /// Board material between two copper Layers.
    Dielectric(DielectricEntry),
}

impl StackupEntry {
    /// The copper Layer or dielectric sheet represented by this entry.
    pub fn uuid(&self) -> Uuid {
        match self {
            Self::Copper(c) => c.layer,
            Self::Dielectric(d) => d.uuid,
        }
    }

    fn thickness(&self) -> Option<Nm> {
        match self {
            Self::Copper(c) => Some(c.thickness),
            Self::Dielectric(d) => d.thickness,
        }
    }
}

/// The copper of one copper Layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopperEntry {
    /// The copper Layer.
    pub layer: Uuid,
    /// The copper thickness; 1 oz is exactly 35 µm.
    pub thickness: Nm,
}

/// A dielectric between two copper Layers. Dielectrics are Stackup entries, not Layers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DielectricEntry {
    pub uuid: Uuid,
    pub kind: DielectricKind,
    /// The optional name shown in the Stackup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Its thickness; left out, it takes the remainder of the Stackup's thickness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness: Option<Nm>,
    /// The material or glass style as specified by the fab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Relative permittivity in milli-units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dk: Option<u32>,
    /// Loss tangent in micro-units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub df: Option<u32>,
    /// The measurement frequency for Dk and Df, in Hz.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_frequency: Option<u64>,
}

impl DielectricEntry {
    /// A blank sheet of this kind, sharing any unassigned board thickness.
    pub fn new(uuid: Uuid, kind: DielectricKind) -> Self {
        Self {
            uuid,
            kind,
            name: None,
            thickness: None,
            material: None,
            dk: None,
            df: None,
            test_frequency: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DielectricKind {
    Core,
    Prepreg,
}

/// Diameters at the start, middle and end of a Via. A missing shape inherits start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViaDiameter {
    pub start: Nm,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle: Option<Nm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<Nm>,
}

impl ViaDiameter {
    pub fn fits_hole(self, hole: Nm) -> bool {
        hole > Nm::ZERO
            && [Some(self.start), self.middle, self.end]
                .into_iter()
                .flatten()
                .all(|d| d > hole)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnusedLayers {
    #[default]
    Keep,
    Remove,
    RemoveKeepEnds,
}

impl UnusedLayers {
    pub fn is_keep(&self) -> bool {
        *self == Self::Keep
    }
}

impl From<Nm> for ViaDiameter {
    fn from(start: Nm) -> Self {
        Self {
            start,
            middle: None,
            end: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViaType {
    pub uuid: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub from: Uuid,
    pub to: Uuid,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub micro: bool,
    pub hole: Nm,
    pub diameter: ViaDiameter,
    #[serde(default, skip_serializing_if = "UnusedLayers::is_keep")]
    pub unused_layers: UnusedLayers,
}

impl ViaType {
    /// The table name, using the current positions until the user supplies one.
    pub fn display_name(&self, pcb: &super::PcbDocument) -> String {
        if let Some(name) = self.name.as_ref().filter(|n| !n.is_empty()) {
            return name.clone();
        }
        let outer = [crate::Side::Top, crate::Side::Bottom]
            .into_iter()
            .filter(|&s| self.span().reaches(pcb, s))
            .count();
        let name = match (self.micro, outer) {
            (true, _) => "Micro",
            (false, 2) => return "Through".into(),
            (false, 1) => "Blind",
            _ => "Buried",
        };
        format!(
            "{name} {}:{}",
            pcb.copper_number(self.from)
                .expect("Via type starts on copper"),
            pcb.copper_number(self.to).expect("Via type ends on copper")
        )
    }

    pub fn span(&self) -> LayerSpan {
        LayerSpan {
            from: self.from,
            to: self.to,
        }
    }

    pub fn through(stackup: Uuid, from: Uuid, to: Uuid, hole: Nm, start: Nm) -> Self {
        Self {
            uuid: stackup.child("through"),
            name: None,
            micro: false,
            from,
            to,
            hole,
            diameter: start.into(),
            unused_layers: UnusedLayers::Keep,
        }
    }
}

/// A new board's Stackup values, from its Fab Preset. A value left out uses the fallback:
/// [`DEFAULT_BOARD_THICKNESS`] and [`DEFAULT_COPPER`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StackupSeeds {
    /// The finished board thickness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness: Option<Nm>,
    /// The copper thickness of every copper Layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copper: Option<Nm>,
}

impl StackupSeeds {
    pub fn is_empty(&self) -> bool {
        *self == StackupSeeds::default()
    }
}

/// The board thickness a new board gets when its Fab Preset seeds none: 1.6 mm.
pub const DEFAULT_BOARD_THICKNESS: Nm = Nm::from_um(1_600);
/// The copper a new board gets when its Fab Preset seeds none: 1 oz (35 µm).
pub const DEFAULT_COPPER: Nm = Nm(NM_PER_OZ);

/// A Stackup's thicknesses, added up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct StackupTotals {
    /// All copper entries.
    pub copper: Nm,
    /// The dielectrics typed in.
    pub dielectric: Nm,
    /// The total thickness left for all blank sheets to share. None when all are specified.
    pub remainder: Option<Nm>,
    /// Copper and dielectric together, the blank ones at their share of the remainder.
    pub total: Nm,
}

/// The resolved physical thickness of a copper Layer or dielectric sheet.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct ResolvedEntry {
    pub uuid: Uuid,
    /// Nanometres, including the fractional share of a blank sheet.
    pub thickness: f64,
}

/// The readouts and checks for a single Stackup.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct StackupInfo {
    pub entries: Vec<ResolvedEntry>,
    pub totals: StackupTotals,
    pub symmetric: bool,
    /// Pairs of mirrored entries whose kind, material or resolved thickness differs.
    pub symmetry_mismatches: Vec<[Uuid; 2]>,
    /// The total differs from the ordered thickness by more than ten percent.
    pub thickness_warning: bool,
}

impl Stackup {
    /// Resolve blank sheets at exactly equal shares and compare mirrored entries. The
    /// thickness comparison uses integer numerators over a common denominator; only the
    /// final readout converts a fractional nanometre to floating point.
    pub fn info(&self) -> StackupInfo {
        let totals = self.totals();
        let blanks = self
            .dielectrics()
            .filter(|s| s.thickness.is_none())
            .count()
            .max(1) as i128;
        let numerator = |entry: &StackupEntry| match entry.thickness() {
            Some(nm) => i128::from(nm.0) * blanks,
            None => i128::from(totals.remainder.expect("blank sheet has a remainder").0),
        };
        let entries = self
            .entries
            .iter()
            .map(|e| ResolvedEntry {
                uuid: e.uuid(),
                thickness: numerator(e) as f64 / blanks as f64,
            })
            .collect();
        let mut symmetry_mismatches = Vec::new();
        for (top, bottom) in self
            .entries
            .iter()
            .zip(self.entries.iter().rev())
            .take(self.entries.len() / 2)
        {
            let material_matches = match (top, bottom) {
                (StackupEntry::Copper(_), StackupEntry::Copper(_)) => true,
                (StackupEntry::Dielectric(a), StackupEntry::Dielectric(b)) => {
                    a.kind == b.kind && a.material == b.material
                }
                _ => false,
            };
            if !material_matches || numerator(top) != numerator(bottom) {
                symmetry_mismatches.push([top.uuid(), bottom.uuid()]);
            }
        }
        let deviation = (i128::from(totals.total.0) - i128::from(self.thickness.0)).abs();
        StackupInfo {
            entries,
            totals,
            symmetric: self.copper().count().is_multiple_of(2) && symmetry_mismatches.is_empty(),
            symmetry_mismatches,
            thickness_warning: deviation * 10 > i128::from(self.thickness.0),
        }
    }

    pub fn type_for_span(&self, span: LayerSpan) -> Option<&ViaType> {
        self.via_types.iter().find(|v| v.span().joins(span))
    }

    pub fn through_type(&self) -> &ViaType {
        let first = self.copper().next().expect("a board has top copper").layer;
        let last = self
            .copper()
            .last()
            .expect("a board has bottom copper")
            .layer;
        self.via_types
            .iter()
            .find(|v| v.from == first && v.to == last)
            .expect("a board has a through Via type")
    }

    /// The copper entries, top to bottom.
    pub fn copper(&self) -> impl Iterator<Item = &CopperEntry> {
        self.entries.iter().filter_map(|e| match e {
            StackupEntry::Copper(c) => Some(c),
            StackupEntry::Dielectric(_) => None,
        })
    }

    /// The copper entry of Layer `layer`.
    pub fn copper_of(&self, layer: Uuid) -> Option<&CopperEntry> {
        self.copper().find(|c| c.layer == layer)
    }

    /// The dielectric entries, top to bottom.
    pub fn dielectrics(&self) -> impl Iterator<Item = &DielectricEntry> {
        self.entries.iter().filter_map(|e| match e {
            StackupEntry::Dielectric(d) => Some(d),
            StackupEntry::Copper(_) => None,
        })
    }

    /// The thicknesses added up. The remainder is negative when the typed entries already
    /// exceed `thickness`.
    pub fn totals(&self) -> StackupTotals {
        self.checked_totals()
            .expect("a validated Stackup fits in nanometres")
    }

    /// Compute totals for proposed edits and files, before validating the Stackup.
    pub fn checked_totals(&self) -> Option<StackupTotals> {
        let copper = self
            .copper()
            .try_fold(0_i64, |sum, c| sum.checked_add(c.thickness.0))?;
        let dielectric = self
            .dielectrics()
            .filter_map(|d| d.thickness)
            .try_fold(0_i64, |sum, t| sum.checked_add(t.0))?;
        let typed = copper.checked_add(dielectric)?;
        let blank = self.dielectrics().any(|d| d.thickness.is_none());
        let remainder = if blank {
            Some(Nm(self.thickness.0.checked_sub(typed)?))
        } else {
            None
        };
        Some(StackupTotals {
            copper: Nm(copper),
            dielectric: Nm(dielectric),
            remainder,
            total: if blank { self.thickness } else { Nm(typed) },
        })
    }

    /// Why this Stackup cannot be stored with these copper Layers (top to bottom), if it
    /// cannot. The message names the field at fault.
    pub fn problem(&self, copper_layers: &[Uuid]) -> Option<String> {
        if self.thickness <= Nm::ZERO {
            return Some("the Stackup thickness must be positive".into());
        }
        if !matches!(self.entries.first(), Some(StackupEntry::Copper(_)))
            || !matches!(self.entries.last(), Some(StackupEntry::Copper(_)))
        {
            return Some("the Stackup must start and end with copper".into());
        }
        if self
            .entries
            .windows(2)
            .any(|pair| matches!(pair, [StackupEntry::Copper(_), StackupEntry::Copper(_)]))
        {
            return Some("copper Layers must alternate with one or more dielectric sheets".into());
        }
        if let Some(c) = self.copper().find(|c| c.thickness <= Nm::ZERO) {
            return Some(format!(
                "the copper of Layer {} must have a positive thickness",
                c.layer
            ));
        }
        if let Some(d) = self
            .dielectrics()
            .find(|d| d.thickness.is_some_and(|t| t <= Nm::ZERO))
        {
            return Some(format!(
                "dielectric {} must have a positive thickness or none",
                d.uuid
            ));
        }
        if !self
            .copper()
            .map(|c| c.layer)
            .eq(copper_layers.iter().copied())
        {
            return Some(
                "the Stackup must have one copper entry per copper Layer, in Layer order".into(),
            );
        }
        if !self
            .via_types
            .iter()
            .any(|v| Some(&v.from) == copper_layers.first() && Some(&v.to) == copper_layers.last())
        {
            return Some(
                "the Stackup needs a through Via type joining the outer copper Layers".into(),
            );
        }
        let mut seen: std::collections::BTreeSet<Uuid> = copper_layers.iter().copied().collect();
        let mut spans = std::collections::BTreeSet::new();
        for kind in &self.via_types {
            if !seen.insert(kind.uuid) {
                return Some(format!("duplicate Via type UUID {}", kind.uuid));
            }
            let (Some(a), Some(b)) = (
                copper_layers.iter().position(|l| *l == kind.from),
                copper_layers.iter().position(|l| *l == kind.to),
            ) else {
                return Some("a Via type must join copper Layers".into());
            };
            if a == b || !spans.insert((a.min(b), a.max(b))) {
                return Some(
                    "each Via type needs a distinct span between different copper Layers".into(),
                );
            }
            if kind.micro && a.abs_diff(b) + 1 == copper_layers.len() {
                return Some("the through Via type cannot be micro".into());
            }
            if !kind.diameter.fits_hole(kind.hole) {
                return Some(
                    "a Via type must have a positive hole smaller than every diameter".into(),
                );
            }
        }
        if let Some(d) = self.dielectrics().find(|d| !seen.insert(d.uuid)) {
            return Some(format!("dielectric {} appears twice", d.uuid));
        }
        let Some(totals) = self.checked_totals() else {
            return Some("the sum of Stackup thicknesses exceeds the supported range".into());
        };
        if let Some(remainder) = totals.remainder
            && remainder < Nm::ZERO
        {
            return Some(format!(
                "the copper and dielectrics typed in exceed the Stackup thickness by {} nm, \
                 leaving nothing for the blank dielectric",
                -remainder.0
            ));
        }
        None
    }

    /// The warning for a Stackup whose dielectrics are all typed in but do not add up to
    /// `thickness`.
    pub fn mismatch(&self) -> Option<String> {
        let totals = self.totals();
        (totals.remainder.is_none() && totals.total != self.thickness).then(|| {
            format!(
                "the entries add up to {} nm, not the Stackup thickness of {} nm",
                totals.total.0, self.thickness.0
            )
        })
    }
}
