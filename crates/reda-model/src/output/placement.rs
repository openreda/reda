//! A Fab Preset's ordered CPL corrections, in the Footprint's local frame.

use reda_geom::{Millideg, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum PlacementCorrection {
    Footprint {
        footprint: String,
        #[serde(flatten)]
        adjustment: PlacementAdjustment,
    },
    Field {
        field: String,
        value: String,
        #[serde(flatten)]
        adjustment: PlacementAdjustment,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlacementAdjustment {
    pub rotation: Millideg,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<Point>,
}

impl PlacementCorrection {
    pub fn new(matcher: PlacementMatch, rotation: Millideg, offset: Option<Point>) -> Self {
        let adjustment = PlacementAdjustment { rotation, offset };
        match matcher {
            PlacementMatch::Footprint { footprint } => Self::Footprint {
                footprint,
                adjustment,
            },
            PlacementMatch::Field { field, value } => Self::Field {
                field,
                value,
                adjustment,
            },
        }
    }

    pub fn matcher(&self) -> PlacementMatch {
        match self {
            Self::Footprint { footprint, .. } => PlacementMatch::Footprint {
                footprint: footprint.clone(),
            },
            Self::Field { field, value, .. } => PlacementMatch::Field {
                field: field.clone(),
                value: value.clone(),
            },
        }
    }

    pub fn adjustment(&self) -> PlacementAdjustment {
        match self {
            Self::Footprint { adjustment, .. } | Self::Field { adjustment, .. } => *adjustment,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum PlacementMatch {
    Footprint { footprint: String },
    Field { field: String, value: String },
}

impl PlacementMatch {
    /// Matches that identify the same preset row when updating or editing it.
    pub fn same_match(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Footprint { footprint: a }, Self::Footprint { footprint: b }) => {
                a.to_lowercase() == b.to_lowercase()
            }
            (
                Self::Field {
                    field: a,
                    value: av,
                },
                Self::Field {
                    field: b,
                    value: bv,
                },
            ) => {
                a.to_lowercase() == b.to_lowercase()
                    && av.trim().to_lowercase() == bv.trim().to_lowercase()
            }
            _ => false,
        }
    }
}
