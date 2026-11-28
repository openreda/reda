//! Local Grids: named areas of the board with their own grid, which replaces the everyday
//! grid inside them (docs/spec/v0.3/grids-guides.md#local-grids).
//!
//! A Local Grid is an editor aid: no output, export, 3D view or check reads it. A PCB
//! Document stores them in `grids.local`, in priority order: where two overlap, the one
//! higher in the list wins, and the everyday grid is always last.

use reda_geom::{Millideg, Nm, Point};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::{TypedLength, Uuid};

/// Which objects a Local Grid snaps while they are placed or moved. One that does not
/// apply to the object in flight is skipped as if it weren't there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppliesTo {
    /// Everything.
    #[default]
    All,
    /// Components only.
    Components,
    /// Everything but Components.
    NotComponents,
}

impl AppliesTo {
    /// Whether it applies to a Component (`component`) or to anything else.
    pub fn applies(self, component: bool) -> bool {
        match self {
            AppliesTo::All => true,
            AppliesTo::Components => component,
            AppliesTo::NotComponents => !component,
        }
    }
}

/// A Local Grid. Field order is the on-disk key order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalGrid {
    pub uuid: Uuid,
    /// Unique on the board; the status bar shows it while the cursor is inside.
    pub name: String,
    pub applies_to: AppliesTo,
    /// Drawn on the canvas. A hidden Local Grid still snaps while it is enabled.
    pub shown: bool,
    /// The cursor snaps to it.
    pub enabled: bool,
    /// Locked against moves, reshapes and deletes, as any placed object.
    pub locked: bool,
    pub shape: LocalGridShape,
    /// Its place in the priority order, lowest first. The file keeps the order as the
    /// list's, so this is not written: a load numbers the rows from 0, and an edit only
    /// compares ranks.
    #[serde(skip)]
    #[schemars(skip)]
    pub rank: u32,
}

/// The area a Local Grid covers and the grid points inside it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalGridShape {
    /// A rectangle with a rectangular grid, both turned by the same rotation.
    Cartesian(CartesianGrid),
}

/// A Cartesian Local Grid: a `width` × `height` rectangle from `origin` along its own X
/// and Y axes, turned `rotation` counter-clockwise, with grid points every `step_x` along
/// X and `step_y` along Y from `origin`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CartesianGrid {
    /// A grid point and the rectangle's first corner, in nm from design zero.
    pub origin: Point,
    /// The grid's X axis, counter-clockwise from +x, in `[0°, 360°)`.
    pub rotation: Millideg,
    #[schemars(schema_with = "step_schema")]
    pub step_x: TypedLength,
    #[schemars(schema_with = "step_schema")]
    pub step_y: TypedLength,
    /// Along the grid's X axis, in nm.
    pub width: Nm,
    /// Along the grid's Y axis, in nm.
    pub height: Nm,
}

impl LocalGrid {
    /// A shown, enabled, unlocked Local Grid that applies to everything, ranked first.
    pub fn new(uuid: Uuid, name: impl Into<String>, shape: LocalGridShape) -> LocalGrid {
        LocalGrid {
            uuid,
            name: name.into(),
            applies_to: AppliesTo::All,
            shown: true,
            enabled: true,
            locked: false,
            shape,
            rank: 0,
        }
    }

    /// Whether the point `at` (nm) lies inside it, edges included.
    pub fn contains(&self, at: (f64, f64)) -> bool {
        match &self.shape {
            LocalGridShape::Cartesian(c) => c.contains(at),
        }
    }

    /// The grid point nearest `at` (nm).
    pub fn nearest(&self, at: (f64, f64)) -> Point {
        match &self.shape {
            LocalGridShape::Cartesian(c) => c.nearest(at),
        }
    }

    /// The outline, as the corners in order.
    pub fn outline(&self) -> Vec<Point> {
        match &self.shape {
            LocalGridShape::Cartesian(c) => c.corners().to_vec(),
        }
    }

    /// Why the row is not a valid Local Grid, if it is not: a name that is not blank,
    /// steps positive and written with their unit, a positive size and a rotation in
    /// `[0°, 360°)`.
    pub fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some(format!("Local Grid {} has a blank name", self.uuid));
        }
        match &self.shape {
            LocalGridShape::Cartesian(c) => c
                .problem()
                .map(|p| format!("Local Grid {} ({}) {p}", self.uuid, self.name)),
        }
    }
}

impl CartesianGrid {
    /// The unit vectors of the grid's X and Y axes, exact on quarter turns.
    pub fn axes(&self) -> ((f64, f64), (f64, f64)) {
        let (s, c) = match self.rotation.normalized().0 {
            0 => (0.0, 1.0),
            90_000 => (1.0, 0.0),
            180_000 => (0.0, -1.0),
            270_000 => (-1.0, 0.0),
            a => (a as f64 / 1000.0).to_radians().sin_cos(),
        };
        ((c, s), (-s, c))
    }

    /// `at` in the grid's own frame: along its X and Y axes from `origin`, in nm.
    pub fn local(&self, at: (f64, f64)) -> (f64, f64) {
        let (u, v) = self.axes();
        let d = (at.0 - self.origin.x.0 as f64, at.1 - self.origin.y.0 as f64);
        (d.0 * u.0 + d.1 * u.1, d.0 * v.0 + d.1 * v.1)
    }

    /// The board point at `local` (nm along the grid's X and Y axes from `origin`).
    pub fn point(&self, local: (f64, f64)) -> Point {
        let (u, v) = self.axes();
        let x = self.origin.x.0 as f64 + local.0 * u.0 + local.1 * v.0;
        let y = self.origin.y.0 as f64 + local.0 * u.1 + local.1 * v.1;
        Point::new(Nm(x.round() as i64), Nm(y.round() as i64))
    }

    /// Whether `at` lies inside the rectangle, edges included.
    pub fn contains(&self, at: (f64, f64)) -> bool {
        let (s, t) = self.local(at);
        // Half a nanometre of slack, so a corner rounded to the nm is inside.
        let e = 0.5;
        (-e..=self.width.0 as f64 + e).contains(&s) && (-e..=self.height.0 as f64 + e).contains(&t)
    }

    /// The grid point nearest `at`.
    pub fn nearest(&self, at: (f64, f64)) -> Point {
        let (s, t) = self.local(at);
        let (sx, sy) = (
            self.step_x.nm().0.max(1) as f64,
            self.step_y.nm().0.max(1) as f64,
        );
        let (i, j) = ((s / sx).round(), (t / sy).round());
        self.point((i * sx, j * sy))
    }

    /// The four corners, counter-clockwise from `origin` in the grid's own frame.
    pub fn corners(&self) -> [Point; 4] {
        let (w, h) = (self.width.0 as f64, self.height.0 as f64);
        [
            self.origin,
            self.point((w, 0.0)),
            self.point((w, h)),
            self.point((0.0, h)),
        ]
    }

    fn problem(&self) -> Option<String> {
        for (field, step) in [("step_x", self.step_x), ("step_y", self.step_y)] {
            if step.unit().is_none() {
                return Some(format!(
                    "has a `{field}` with no unit; write it as \"0.5mm\" or \"25mil\""
                ));
            }
            if step.nm().0 <= 0 {
                return Some(format!("has a `{field}` of {step}; it must be positive"));
            }
        }
        for (field, size) in [("width", self.width), ("height", self.height)] {
            if size.0 <= 0 {
                return Some(format!(
                    "has a `{field}` of {} nm; it must be positive",
                    size.0
                ));
            }
        }
        if !(0..360_000).contains(&self.rotation.0) {
            return Some(format!(
                "has a rotation of {} millidegrees; it must be in [0, 360000)",
                self.rotation.0
            ));
        }
        None
    }
}

/// A Local Grid's step in the file: always text with its unit.
fn step_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "description": "A length with the unit it was typed in, such as \"0.5mm\" or \"25mil\" (nm, um, mm, mil or in).",
        "type": "string",
        "format": "length"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MM: i64 = 1_000_000;

    fn grid(rotation: i64) -> CartesianGrid {
        CartesianGrid {
            origin: Point::new(Nm(10 * MM), Nm(20 * MM)),
            rotation: Millideg(rotation),
            step_x: TypedLength::new(Nm(MM), crate::LengthUnit::Mm),
            step_y: TypedLength::new(Nm(2 * MM), crate::LengthUnit::Mm),
            width: Nm(5 * MM),
            height: Nm(4 * MM),
        }
    }

    fn mm(x: f64, y: f64) -> (f64, f64) {
        (x * MM as f64, y * MM as f64)
    }

    #[test]
    fn an_unturned_grid_holds_its_rectangle_and_snaps_per_axis_from_its_origin() {
        let g = grid(0);
        assert!(g.contains(mm(10.0, 20.0)));
        assert!(g.contains(mm(15.0, 24.0)));
        assert!(!g.contains(mm(15.1, 22.0)));
        assert!(!g.contains(mm(12.0, 19.9)));
        assert_eq!(
            g.nearest(mm(12.4, 22.9)),
            Point::new(Nm(12 * MM), Nm(22 * MM))
        );
        assert_eq!(
            g.nearest(mm(12.6, 23.1)),
            Point::new(Nm(13 * MM), Nm(24 * MM))
        );
    }

    #[test]
    fn a_quarter_turn_swaps_the_axes_exactly() {
        let g = grid(90_000);
        // X runs up the board from the origin, Y runs to the left.
        assert!(g.contains(mm(9.0, 24.0)));
        assert!(!g.contains(mm(11.0, 24.0)));
        assert_eq!(
            g.nearest(mm(8.1, 22.9)),
            Point::new(Nm(8 * MM), Nm(23 * MM))
        );
        assert_eq!(
            g.corners(),
            [
                Point::new(Nm(10 * MM), Nm(20 * MM)),
                Point::new(Nm(10 * MM), Nm(25 * MM)),
                Point::new(Nm(6 * MM), Nm(25 * MM)),
                Point::new(Nm(6 * MM), Nm(20 * MM)),
            ]
        );
    }

    #[test]
    fn a_turned_grid_snaps_along_its_own_axes() {
        let g = grid(45_000);
        let r = std::f64::consts::FRAC_1_SQRT_2;
        // One step along X and one along Y, from the origin.
        let p = (10.0 + r * 1.0 - r * 2.0, 20.0 + r * 1.0 + r * 2.0);
        let near = g.nearest(mm(p.0 + 0.1, p.1 - 0.05));
        assert!((near.x.0 as f64 - p.0 * MM as f64).abs() <= 1.0, "{near:?}");
        assert!((near.y.0 as f64 - p.1 * MM as f64).abs() <= 1.0, "{near:?}");
        assert!(g.contains(mm(p.0, p.1)));
    }

    #[test]
    fn applies_to_skips_the_kind_it_leaves_out() {
        assert!(AppliesTo::All.applies(true) && AppliesTo::All.applies(false));
        assert!(AppliesTo::Components.applies(true));
        assert!(!AppliesTo::Components.applies(false));
        assert!(!AppliesTo::NotComponents.applies(true));
        assert!(AppliesTo::NotComponents.applies(false));
    }

    #[test]
    fn a_row_with_a_blank_name_or_no_size_is_a_problem() {
        let shape = LocalGridShape::Cartesian(grid(0));
        let uuid = Uuid::from_u128(1);
        assert_eq!(
            LocalGrid::new(uuid, "Header", shape.clone()).problem(),
            None
        );
        assert!(LocalGrid::new(uuid, " ", shape).problem().is_some());
        let mut flat = grid(0);
        flat.height = Nm(0);
        let g = LocalGrid::new(uuid, "Flat", LocalGridShape::Cartesian(flat));
        assert!(g.problem().unwrap().contains("height"));
        let mut bare = grid(0);
        bare.step_x = TypedLength::bare(Nm(MM));
        let g = LocalGrid::new(uuid, "Bare", LocalGridShape::Cartesian(bare));
        assert!(g.problem().unwrap().contains("unit"));
    }
}
