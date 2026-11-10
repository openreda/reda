//! Solder-mask and solder-paste overrides, shared by Library Pads, Free Pads and Vias.

use reda_geom::{CornerCut, CornerRect, Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{PadKind, Ratio, Side};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "L: Deserialize<'de>"))]
pub struct Mask<L = Nm> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<MaskSide<L>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<MaskSide<L>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_hole_edge: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum MaskSide<L = Nm> {
    Expansion { expansion: L },
    Mode(Tenting),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tenting {
    Tent,
}

impl<L> Mask<L> {
    pub fn map<T>(self, f: impl Fn(L) -> T) -> Mask<T> {
        let side = |s: MaskSide<L>| match s {
            MaskSide::Expansion { expansion } => MaskSide::Expansion {
                expansion: f(expansion),
            },
            MaskSide::Mode(m) => MaskSide::Mode(m),
        };
        Mask {
            top: self.top.map(side),
            bottom: self.bottom.map(side),
            from_hole_edge: self.from_hole_edge,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum Paste<L = Nm, P = Point, R = Ratio> {
    Expansion { expansion: L },
    Percent { percent: i32 },
    Mode(PasteMode),
    Windows { windows: Vec<PasteWindow<L, P, R>> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PasteMode {
    None,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "L: Deserialize<'de>, P: Deserialize<'de>, R: Deserialize<'de>"))]
pub struct PasteWindow<L = Nm, P = Point, R = Ratio> {
    pub at: P,
    pub width: L,
    pub height: L,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_ratio: Option<R>,
}

impl<L, P, R> Paste<L, P, R> {
    pub fn map<T, Q, S>(
        self,
        length: impl Fn(L) -> T,
        point: impl Fn(P) -> Q,
        ratio: impl Fn(R) -> S,
    ) -> Paste<T, Q, S> {
        match self {
            Paste::Expansion { expansion } => Paste::Expansion {
                expansion: length(expansion),
            },
            Paste::Percent { percent } => Paste::Percent { percent },
            Paste::Mode(mode) => Paste::Mode(mode),
            Paste::Windows { windows } => Paste::Windows {
                windows: windows
                    .into_iter()
                    .map(|w| PasteWindow {
                        at: point(w.at),
                        width: length(w.width),
                        height: length(w.height),
                        corner_ratio: w.corner_ratio.map(&ratio),
                    })
                    .collect(),
            },
        }
    }
}

impl PasteWindow {
    /// Whether the complete window is inside the convex copper outline. Curve samples
    /// round to integer nanometres, so allow one nanometre at a coincident boundary.
    pub fn fits(&self, copper: &CornerRect) -> bool {
        self.outline()
            .polygon(Nm(1))
            .into_iter()
            .all(|p| copper.edge_distance(p) >= -1.0)
    }

    pub fn outline(&self) -> CornerRect {
        let cut = match self
            .corner_ratio
            .unwrap_or_default()
            .of(self.width, self.height)
        {
            Nm::ZERO => CornerCut::Square,
            radius => CornerCut::Round(radius),
        };
        CornerRect::uniform(self.width, self.height, self.at, cut)
    }
}

/// Structural errors of opening overrides; geometry outside copper is a Library check.
pub fn opening_problem(
    kind: PadKind,
    side: Option<Side>,
    mask: Option<&Mask>,
    paste: Option<&Paste>,
) -> Option<(String, String)> {
    let problem = |p: &str, m: &str| Some((p.into(), m.into()));
    if kind == PadKind::Smd
        && let Some(mask) = mask
    {
        if mask.from_hole_edge.is_some() {
            return problem(
                "/mask/from_hole_edge",
                "an SMD Pad cannot measure mask from a hole",
            );
        }
        match side.unwrap_or(Side::Top) {
            Side::Top if mask.bottom.is_some() => {
                return problem("/mask/bottom", "this SMD Pad is on the top");
            }
            Side::Bottom if mask.top.is_some() => {
                return problem("/mask/top", "this SMD Pad is on the bottom");
            }
            _ => {}
        }
    }
    if let Some(paste) = paste {
        if kind != PadKind::Smd {
            return problem("/paste", "only an SMD Pad has paste");
        }
        match paste {
            Paste::Percent { percent } if *percent <= -100 => {
                return problem("/paste/percent", "paste percent must be above -100");
            }
            Paste::Windows { windows } => {
                for (i, w) in windows.iter().enumerate() {
                    if w.width <= Nm::ZERO || w.height <= Nm::ZERO {
                        return Some((
                            format!("/paste/windows/{i}"),
                            "a paste window must have positive size".into(),
                        ));
                    }
                    if w.corner_ratio.is_some_and(|r| r > Ratio::FULL) {
                        return Some((
                            format!("/paste/windows/{i}/corner_ratio"),
                            "a ratio is at most 100%".into(),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    None
}
