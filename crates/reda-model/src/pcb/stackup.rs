//! The Stackup: the board's ordered copper and dielectric build, and its finish and
//! colours.

use reda_geom::{NM_PER_OZ, Nm};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// The board's physical build, top to bottom. v0.1 boards hold one; the table leaves room
/// for more stacks later, such as rigid-flex substacks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stackup {
    pub uuid: Uuid,
    /// The finished thickness you order.
    pub thickness: Nm,
    /// The copper and dielectric entries, top to bottom: copper, dielectric, copper, and so
    /// on, with one copper entry per copper Layer in Layer order.
    pub entries: Vec<StackupEntry>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum StackupEntry {
    /// A copper Layer's thickness.
    Copper(CopperEntry),
    /// Board material between two copper Layers.
    Dielectric(DielectricEntry),
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DielectricEntry {
    pub uuid: Uuid,
    /// Its thickness; left out, it takes the remainder of the Stackup's thickness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness: Option<Nm>,
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
    /// What each dielectric left blank takes: the thickness left over, shared evenly. `None`
    /// when every dielectric is typed in.
    pub remainder: Option<Nm>,
    /// Copper and dielectric together, the blank ones at their share of the remainder.
    pub total: Nm,
}

impl Stackup {
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
        let copper = self
            .copper()
            .map(|c| c.thickness)
            .fold(Nm::ZERO, |a, b| a + b);
        let dielectric = self
            .dielectrics()
            .filter_map(|d| d.thickness)
            .fold(Nm::ZERO, |a, b| a + b);
        let blank = self.dielectrics().filter(|d| d.thickness.is_none()).count();
        let remainder = (blank > 0).then(|| self.thickness - copper - dielectric);
        let total = match remainder {
            Some(_) => self.thickness,
            None => copper + dielectric,
        };
        StackupTotals {
            copper,
            dielectric,
            remainder,
            total,
        }
    }

    /// Why this Stackup cannot be stored with these copper Layers (top to bottom), if it
    /// cannot. The message names the field at fault.
    pub fn problem(&self, copper_layers: &[Uuid]) -> Option<String> {
        if self.thickness <= Nm::ZERO {
            return Some("the Stackup thickness must be positive".into());
        }
        let mut expect_copper = true;
        for entry in &self.entries {
            match (entry, expect_copper) {
                (StackupEntry::Copper(c), true) => {
                    if c.thickness <= Nm::ZERO {
                        return Some(format!(
                            "the copper of Layer {} must have a positive thickness",
                            c.layer
                        ));
                    }
                }
                (StackupEntry::Dielectric(d), false) => {
                    if d.thickness.is_some_and(|t| t <= Nm::ZERO) {
                        return Some(format!(
                            "dielectric {} must have a positive thickness or none",
                            d.uuid
                        ));
                    }
                }
                _ => {
                    return Some(
                        "Stackup entries must alternate copper and dielectric, starting and \
                         ending with copper"
                            .into(),
                    );
                }
            }
            expect_copper = !expect_copper;
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
        let mut seen = std::collections::BTreeSet::new();
        if let Some(d) = self.dielectrics().find(|d| !seen.insert(d.uuid)) {
            return Some(format!("dielectric {} appears twice", d.uuid));
        }
        if let Some(remainder) = self.totals().remainder
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
