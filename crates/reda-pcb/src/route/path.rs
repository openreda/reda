//! Corner modes and postures: the path from the last placed vertex to the cursor.

use reda_model::geom::{Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How a route turns the corner (`/` cycles them). Arc corners `45_arc` and `90_arc` are
/// reserved for v0.4.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CornerMode {
    /// A straight leg and a 45° leg.
    #[default]
    #[serde(rename = "45")]
    Deg45,
    /// A horizontal and a vertical leg.
    #[serde(rename = "90")]
    Deg90,
    /// Straight to the cursor, with no second leg.
    #[serde(rename = "any")]
    Any,
}

impl CornerMode {
    /// The mode `/` goes to next: 45°, 90°, Any Angle.
    pub fn next(self) -> CornerMode {
        match self {
            CornerMode::Deg45 => CornerMode::Deg90,
            CornerMode::Deg90 => CornerMode::Any,
            CornerMode::Any => CornerMode::Deg45,
        }
    }
}

/// Which leg comes first (`Space` flips it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Posture {
    /// 45°: the straight leg, along the longer axis, then the 45° leg. 90°: horizontal,
    /// then vertical.
    #[default]
    Default,
    /// 45°: the 45° leg first. 90°: vertical, then horizontal.
    Flipped,
}

impl Posture {
    pub fn flipped(self) -> Posture {
        match self {
            Posture::Default => Posture::Flipped,
            Posture::Flipped => Posture::Default,
        }
    }
}

/// The path from `from` to `to`: `from`, the corner when there is one, and `to`.
pub fn corner_path(from: Point, to: Point, corner: CornerMode, posture: Posture) -> Vec<Point> {
    let (dx, dy) = (to.x.0 - from.x.0, to.y.0 - from.y.0);
    let (sx, sy) = (dx.signum(), dy.signum());
    let d = dx.abs().min(dy.abs());
    let bend = match (corner, posture) {
        (CornerMode::Deg45, Posture::Default) if dx.abs() >= dy.abs() => {
            Point::new(Nm(to.x.0 - sx * d), from.y)
        }
        (CornerMode::Deg45, Posture::Default) => Point::new(from.x, Nm(to.y.0 - sy * d)),
        (CornerMode::Deg45, Posture::Flipped) => {
            Point::new(Nm(from.x.0 + sx * d), Nm(from.y.0 + sy * d))
        }
        (CornerMode::Deg90, Posture::Default) => Point::new(to.x, from.y),
        (CornerMode::Deg90, Posture::Flipped) => Point::new(from.x, to.y),
        (CornerMode::Any, _) => to,
    };
    let mut path = vec![from];
    for p in [bend, to] {
        if *path.last().expect("from") != p {
            path.push(p);
        }
    }
    path
}
