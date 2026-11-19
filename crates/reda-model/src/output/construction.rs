//! A physical build in a Fab Preset. Copper positions travel between boards with
//! different Layer UUIDs.

use crate::{
    CopperEntry, DielectricEntry, DielectricKind, StackupEntry, UnusedLayers, Uuid, ViaDiameter,
    ViaType,
};
use reda_geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Construction {
    pub name: String,
    pub default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub thickness: Nm,
    pub entries: Vec<ConstructionEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via_types: Vec<ConstructionViaType>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum ConstructionEntry {
    Copper {
        copper: Nm,
    },
    Dielectric {
        kind: DielectricKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thickness: Option<Nm>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        material: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dk: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        df: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        test_frequency: Option<u64>,
    },
}

/// Portable Via type, with drilling direction expressed as one-based copper positions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConstructionViaType {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub from: u8,
    pub to: u8,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub micro: bool,
    pub hole: Nm,
    pub diameter: ViaDiameter,
    #[serde(default, skip_serializing_if = "UnusedLayers::is_keep")]
    pub unused_layers: UnusedLayers,
}

impl Construction {
    /// Check persisted presets and command patches with the same physical invariants as a
    /// board. The temporary identities represent copper positions in a standalone preset.
    pub fn read_list<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Self>, D::Error> {
        use serde::de::Error as _;
        let builds = Vec::<Self>::deserialize(d)?;
        let mut names = std::collections::BTreeSet::new();
        let mut defaults = std::collections::BTreeSet::new();
        for build in &builds {
            let count = build.layer_count();
            if build.name.trim().is_empty() || !names.insert(build.name.to_ascii_lowercase()) {
                return Err(D::Error::custom("a Construction name is empty or repeated"));
            }
            if !(2..=32).contains(&count) {
                return Err(D::Error::custom(
                    "a Construction needs 2 to 32 copper Layers",
                ));
            }
            if build.default && !defaults.insert(count) {
                return Err(D::Error::custom(format!(
                    "multiple default Constructions for {count} Layers"
                )));
            }
            let mut ids = (1u128..).map(Uuid::from_u128);
            let copper: Vec<_> = ids.by_ref().take(count).collect();
            let stack = build
                .board_stackup(&copper, Nm(1), Nm(2), || ids.next().unwrap())
                .map_err(D::Error::custom)?;
            if let Some(why) = stack.problem(&copper) {
                return Err(D::Error::custom(format!(
                    "Construction {:?}: {why}",
                    build.name
                )));
            }
        }
        Ok(builds)
    }

    /// Start a board with this build, supplying the preset's preferred size when the
    /// Construction carries no Via types.
    pub fn board_stackup(
        &self,
        copper: &[Uuid],
        hole: Nm,
        diameter: Nm,
        mut new_id: impl FnMut() -> Uuid,
    ) -> Result<crate::Stackup, String> {
        if !(2..=32).contains(&copper.len()) {
            return Err("choose 2 to 32 copper Layers".into());
        }
        let entries = self.board_entries(copper, &mut new_id)?;
        let uuid = new_id();
        let through = ViaType::through(
            new_id(),
            copper[0],
            copper[copper.len() - 1],
            hole,
            diameter,
        );
        let via_types = if self.via_types.is_empty() {
            vec![through]
        } else {
            self.board_via_types(copper, through.uuid, new_id)?
        };
        Ok(crate::Stackup {
            uuid,
            construction: Some(self.name.clone()),
            thickness: self.thickness,
            entries,
            via_types,
            finish: None,
            mask_color: None,
            silk_color: None,
        })
    }

    pub fn layer_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e, ConstructionEntry::Copper { .. }))
            .count()
    }

    /// Transfer the physical entries to the given board Layers, keeping their UUIDs.
    pub fn board_entries(
        &self,
        layers: &[Uuid],
        mut new_id: impl FnMut() -> Uuid,
    ) -> Result<Vec<StackupEntry>, String> {
        if self.layer_count() != layers.len() {
            return Err("the Construction has a different copper Layer count".into());
        }
        let mut copper = layers.iter();
        Ok(self
            .entries
            .iter()
            .map(|entry| match entry {
                ConstructionEntry::Copper { copper: thickness } => {
                    StackupEntry::Copper(CopperEntry {
                        layer: *copper.next().expect("count checked"),
                        thickness: *thickness,
                    })
                }
                ConstructionEntry::Dielectric {
                    kind,
                    name,
                    thickness,
                    material,
                    dk,
                    df,
                    test_frequency,
                } => StackupEntry::Dielectric(DielectricEntry {
                    uuid: new_id(),
                    kind: *kind,
                    name: name.clone(),
                    thickness: *thickness,
                    material: material.clone(),
                    dk: *dk,
                    df: *df,
                    test_frequency: *test_frequency,
                }),
            })
            .collect())
    }

    /// Transfer Via types in table order, substituting Layer UUIDs for copper positions.
    pub fn board_via_types(
        &self,
        layers: &[Uuid],
        through: Uuid,
        mut new_id: impl FnMut() -> Uuid,
    ) -> Result<Vec<ViaType>, String> {
        let layer = |n: u8| {
            n.checked_sub(1)
                .and_then(|i| layers.get(usize::from(i)))
                .copied()
                .ok_or_else(|| format!("Via type position {n} is outside the Stackup"))
        };
        self.via_types
            .iter()
            .map(|v| {
                let uuid = if v.from == 1 && usize::from(v.to) == layers.len() {
                    through
                } else {
                    new_id()
                };
                Ok(ViaType {
                    uuid,
                    name: v.name.clone(),
                    from: layer(v.from)?,
                    to: layer(v.to)?,
                    micro: v.micro,
                    hole: v.hole,
                    diameter: v.diameter,
                    unused_layers: v.unused_layers,
                })
            })
            .collect()
    }
}

impl Construction {
    /// Save the board's physical Stackup and ordered Via types without board identities.
    pub fn from_stackup(name: String, stack: &crate::Stackup) -> Self {
        let copper: Vec<_> = stack.copper().map(|c| c.layer).collect();
        let position = |id| {
            (1 + copper
                .iter()
                .position(|l| *l == id)
                .expect("Via type ends on copper")) as u8
        };
        Self {
            name,
            default: false,
            notes: None,
            thickness: stack.thickness,
            entries: stack.entries.iter().map(ConstructionEntry::from).collect(),
            via_types: stack
                .via_types
                .iter()
                .map(|v| ConstructionViaType {
                    name: v.name.clone(),
                    from: position(v.from),
                    to: position(v.to),
                    micro: v.micro,
                    hole: v.hole,
                    diameter: v.diameter,
                    unused_layers: v.unused_layers,
                })
                .collect(),
        }
    }

    /// Capture the board for a particular preset. Replacing a row retains its notes,
    /// and the first build for a copper count becomes its default.
    pub fn capture(name: String, stack: &crate::Stackup, builds: &[Self]) -> Self {
        let mut build = Self::from_stackup(name, stack);
        let count = build.layer_count();
        build.default = !builds.iter().any(|b| {
            b.default && b.layer_count() == count && !b.name.eq_ignore_ascii_case(&build.name)
        });
        if let Some(old) = builds
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(&build.name))
        {
            build.notes = old.notes.clone();
        }
        build
    }

    /// Whether a board still has this physical build. Names, roles, appearance and the
    /// Via type table are independent of the ordered Construction.
    pub fn matches_stackup(&self, stack: &crate::Stackup) -> bool {
        self.thickness == stack.thickness
            && self.entries.len() == stack.entries.len()
            && self
                .entries
                .iter()
                .zip(&stack.entries)
                .all(|(a, b)| match (a, b) {
                    (ConstructionEntry::Copper { copper }, StackupEntry::Copper(c)) => {
                        *copper == c.thickness
                    }
                    (
                        ConstructionEntry::Dielectric {
                            kind,
                            thickness,
                            material,
                            dk,
                            df,
                            ..
                        },
                        StackupEntry::Dielectric(d),
                    ) => {
                        *kind == d.kind
                            && *thickness == d.thickness
                            && *material == d.material
                            && *dk == d.dk
                            && *df == d.df
                    }
                    _ => false,
                })
    }
}

impl From<&StackupEntry> for ConstructionEntry {
    fn from(entry: &StackupEntry) -> Self {
        match entry {
            StackupEntry::Copper(c) => Self::Copper {
                copper: c.thickness,
            },
            StackupEntry::Dielectric(d) => Self::Dielectric {
                kind: d.kind,
                name: d.name.clone(),
                thickness: d.thickness,
                material: d.material.clone(),
                dk: d.dk,
                df: d.df,
                test_frequency: d.test_frequency,
            },
        }
    }
}
