//! The board's `edge_connector` as Board Setup drafts it on the Stackup page: whether the
//! card edge is bevelled, the bevel's angle and depth as typed, and hard gold.

use reda_engine::model::geom::{Millideg, Nm};
use reda_engine::model::{EdgeBevel, EdgeConnector};
use serde_json::{Value, json};

use crate::settings::{Problem, Step};
use crate::tr;

/// The problem field of the bevel angle.
pub const ANGLE_FIELD: &str = "edge_connector/angle";
/// The problem field of the bevel depth.
pub const DEPTH_FIELD: &str = "edge_connector/depth";

/// The bevel the fields start from when the board has none: the PCIe card edge's
/// 20°, 1.30 mm, the one bevel a published drawing gives.
const START_ANGLE: &str = "20";
const START_DEPTH: &str = "1.3";

/// The `edge_connector` property as drafted. The angle (degrees) and depth (mm) are kept
/// as typed, and kept while the bevel is unticked so ticking it again restores them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeDraft {
    pub bevel: bool,
    pub angle: String,
    pub depth: String,
    pub hard_gold: bool,
}

impl Default for EdgeDraft {
    fn default() -> Self {
        EdgeDraft::of(None)
    }
}

impl EdgeDraft {
    /// The draft of the board's property.
    pub fn of(edge: Option<EdgeConnector>) -> EdgeDraft {
        let bevel = edge.and_then(|e| e.bevel);
        EdgeDraft {
            bevel: bevel.is_some(),
            angle: bevel.map_or(START_ANGLE.to_owned(), |b| {
                reda_engine::model::geom::exact_decimal(b.angle.0, 1_000)
            }),
            depth: bevel.map_or(START_DEPTH.to_owned(), |b| b.depth.mm_exact()),
            hard_gold: edge.is_some_and(|e| e.hard_gold),
        }
    }

    /// The property the draft makes, or the problems in its fields.
    pub fn parse(&self) -> Result<Option<EdgeConnector>, Vec<Problem>> {
        let bevel = if self.bevel {
            let angle = parse_angle(&self.angle);
            let depth = parse_depth(&self.depth);
            let mut problems = Vec::new();
            if let Err(message) = &angle {
                problems.push(Problem::new(ANGLE_FIELD, message.clone()));
            }
            if let Err(message) = &depth {
                problems.push(Problem::new(DEPTH_FIELD, message.clone()));
            }
            match (angle, depth) {
                (Ok(angle), Ok(depth)) => Some(EdgeBevel { angle, depth }),
                _ => return Err(problems),
            }
        } else {
            None
        };
        Ok(
            (bevel.is_some() || self.hard_gold).then_some(EdgeConnector {
                bevel,
                hard_gold: self.hard_gold,
            }),
        )
    }

    /// The problems in its fields.
    pub fn problems(&self) -> Vec<Problem> {
        self.parse().err().unwrap_or_default()
    }

    /// The `pcb.edge_connector.set` step that makes the board match `self`, given what
    /// the board has (`base`); none when they mean the same or the draft is invalid.
    pub fn step(&self, base: &EdgeDraft) -> Option<Step> {
        let value = self.parse().ok()?;
        if base.parse().ok() == Some(value) {
            return None;
        }
        let param = value.map_or(Value::Null, |e| {
            let mut param = json!({"hard_gold": e.hard_gold});
            if let Some(b) = e.bevel {
                param["bevel"] = json!({"angle": b.angle.0, "depth": b.depth.0});
            }
            param
        });
        Some(Step::new(
            "pcb.edge_connector.set",
            json!({"edge_connector": param}),
        ))
    }
}

fn parse_angle(text: &str) -> Result<Millideg, String> {
    let angle = Millideg::parse(&format!("{}deg", text.trim()))
        .map_err(|_| tr!("board-setup-edge-angle-invalid"))?;
    if angle.0 <= 0 || angle.0 >= 90_000 {
        return Err(tr!("board-setup-edge-angle-range"));
    }
    Ok(angle)
}

fn parse_depth(text: &str) -> Result<Nm, String> {
    let depth = Nm::parse(&format!("{}mm", text.trim()))
        .map_err(|_| tr!("board-setup-edge-depth-invalid"))?;
    if depth <= Nm::ZERO {
        return Err(tr!("board-setup-edge-depth-positive"));
    }
    Ok(depth)
}
