//! Grid presets, the visible grid and display precision
//! (docs/spec/v0.1/editing.md#grid and #properties-with-a-selection): the `editing.grid`
//! and `editing.display_precision` Preferences.

use reda_command::Length;
use reda_model::geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const MIL: i64 = 25_400;
const MM: i64 = 1_000_000;

/// One preset row of a grid table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridRow {
    /// The snap step: Step X, and Step Y too unless `snap_y` is set.
    pub snap: Length,
    /// Step Y on board and Footprint rows, when it differs from Step X; unset is the
    /// same as `snap`. Sheets and Symbols keep a single step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap_y: Option<Length>,
    /// The Electrical Range (sheets and Symbols): how near, on both axes, a Hotspot
    /// catches the cursor. Unset on board and Footprint rows. At or above the step it
    /// is allowed, with a warning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<Length>,
    /// The spacing of the visible grid's fine dots or lines along X. Along Y it follows
    /// the pair: the same multiple of Step Y.
    pub visible: Length,
    /// G and Shift+G step through this row.
    #[serde(default = "yes")]
    pub in_cycle: bool,
}

fn yes() -> bool {
    true
}

impl GridRow {
    /// A row whose visible grid equals its snap step, in the cycle.
    pub const fn new(snap: Nm, range: Option<Nm>) -> GridRow {
        GridRow {
            snap: Length(snap),
            range: match range {
                Some(r) => Some(Length(r)),
                None => None,
            },
            snap_y: None,
            visible: Length(snap),
            in_cycle: true,
        }
    }

    /// Step X.
    pub fn step_x(&self) -> Nm {
        self.snap.0
    }

    /// Step Y: `snap_y`, else Step X.
    pub fn step_y(&self) -> Nm {
        self.snap_y.map_or(self.snap.0, |y| y.0)
    }

    /// The visible grid's spacing along X and Y: `visible`, and along Y the same multiple
    /// of Step Y as `visible` is of Step X.
    pub fn visible_xy(&self) -> (Nm, Nm) {
        let x = self.visible.0;
        let (sx, sy) = (self.step_x(), self.step_y());
        if sx == sy || sx.0 <= 0 {
            return (x, x);
        }
        let y = i128::from(x.0) * i128::from(sy.0) / i128::from(sx.0);
        (x, Nm(i64::try_from(y).unwrap_or(i64::MAX)))
    }

    /// Whether the row's range is at or above its step (allowed, but warned about).
    pub fn range_warning(&self) -> bool {
        self.range.is_some_and(|r| r.0 >= self.snap.0)
    }
}

/// A Document type's presets: one imperial and one metric table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridTables {
    /// The imperial rows: each a `snap` step (and `snap_y` on boards and Footprints), a
    /// Hotspot capture `range` (sheets and Symbols),
    /// the `visible` grid's spacing and whether G cycles through it (`in_cycle`).
    pub imperial: Vec<GridRow>,
    /// The metric rows: each a `snap` step (and `snap_y` on boards and Footprints), a
    /// Hotspot capture `range` (sheets and Symbols),
    /// the `visible` grid's spacing and whether G cycles through it (`in_cycle`).
    pub metric: Vec<GridRow>,
}

impl GridTables {
    /// The shipped sheet and Symbol tables: 10, 25, 50 and 100 mil with ranges 8, 20, 40
    /// and 80 mil, and 0.25, 0.5, 1 and 2.5 mm with ranges of 0.8 of the step.
    pub fn sheet() -> GridTables {
        let row = |snap: i64, range: i64| GridRow::new(Nm(snap), Some(Nm(range)));
        GridTables {
            imperial: vec![
                row(10 * MIL, 8 * MIL),
                row(25 * MIL, 20 * MIL),
                row(50 * MIL, 40 * MIL),
                row(100 * MIL, 80 * MIL),
            ],
            metric: vec![
                row(MM / 4, MM / 5),
                row(MM / 2, 2 * MM / 5),
                row(MM, 4 * MM / 5),
                row(5 * MM / 2, 2 * MM),
            ],
        }
    }

    /// The shipped board and Footprint tables: 5, 10, 25 and 50 mil, and 0.1, 0.25, 0.5
    /// and 1.0 mm.
    pub fn board() -> GridTables {
        let row = |snap: i64| GridRow::new(Nm(snap), None);
        GridTables {
            imperial: vec![row(5 * MIL), row(10 * MIL), row(25 * MIL), row(50 * MIL)],
            metric: vec![row(MM / 10), row(MM / 4), row(MM / 2), row(MM)],
        }
    }

    /// Every row, imperial first.
    pub fn rows(&self) -> impl Iterator<Item = &GridRow> {
        self.imperial.iter().chain(&self.metric)
    }

    /// The row whose snap step is `snap`.
    pub fn row(&self, snap: Nm) -> Option<&GridRow> {
        self.rows().find(|r| r.snap.0 == snap)
    }

    /// The table `snap` belongs to: the one holding it, else the imperial one for a whole
    /// number of mil. `true` is imperial.
    pub fn is_imperial(&self, snap: Nm) -> bool {
        if self.imperial.iter().any(|r| r.snap.0 == snap) {
            true
        } else if self.metric.iter().any(|r| r.snap.0 == snap) {
            false
        } else {
            snap.0 % MIL == 0
        }
    }

    /// The table in imperial or metric units.
    pub fn table(&self, imperial: bool) -> &[GridRow] {
        if imperial {
            &self.imperial
        } else {
            &self.metric
        }
    }
}

/// How the visible grid draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GridStyle {
    #[default]
    Dots,
    Lines,
}

/// Grid presets and the visible grid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct GridPreferences {
    /// Schematic Sheet presets: Snap, Electrical Range, Visible and In cycle.
    pub schematic: GridTables,
    /// Symbol editor presets: Snap, Electrical Range, Visible and In cycle.
    pub symbol: GridTables,
    /// PCB Document presets: Snap (Step X), an optional Step Y, Visible and In cycle.
    pub pcb: GridTables,
    /// Footprint editor presets: Snap (Step X), an optional Step Y, Visible and In cycle.
    pub footprint: GridTables,
    /// The visible grid: `dots` or `lines`.
    pub style: GridStyle,
    /// Every how many fine dots or lines a coarse one is drawn: 2, 5 or 10.
    pub coarse_multiplier: u32,
}

impl GridPreferences {
    /// The coarse multipliers offered.
    pub const COARSE_MULTIPLIERS: [u32; 3] = [2, 5, 10];
}

impl Default for GridPreferences {
    fn default() -> GridPreferences {
        GridPreferences {
            schematic: GridTables::sheet(),
            symbol: GridTables::sheet(),
            pcb: GridTables::board(),
            footprint: GridTables::board(),
            style: GridStyle::Dots,
            coarse_multiplier: 5,
        }
    }
}

/// How many decimals lengths show with. Rounding is for display only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DisplayPrecision {
    /// Decimals for millimetres.
    pub mm: u32,
    /// Decimals for mil.
    pub mil: u32,
}

impl DisplayPrecision {
    /// The most decimals either unit takes.
    pub const MAX: u32 = 6;
}

impl Default for DisplayPrecision {
    fn default() -> DisplayPrecision {
        DisplayPrecision { mm: 3, mil: 1 }
    }
}

/// Range rules for these Preferences: the offending key and why.
pub(super) fn check(
    grid: &GridPreferences,
    precision: &DisplayPrecision,
) -> Result<(), (&'static str, String)> {
    if !GridPreferences::COARSE_MULTIPLIERS.contains(&grid.coarse_multiplier) {
        return Err((
            "editing.grid.coarse_multiplier",
            format!("{} is not 2, 5 or 10", grid.coarse_multiplier),
        ));
    }
    let tables = [
        ("editing.grid.schematic", &grid.schematic),
        ("editing.grid.symbol", &grid.symbol),
        ("editing.grid.pcb", &grid.pcb),
        ("editing.grid.footprint", &grid.footprint),
    ];
    for (key, tables) in tables {
        for (imperial, rows) in [(true, &tables.imperial), (false, &tables.metric)] {
            let key = match (key, imperial) {
                ("editing.grid.schematic", true) => "editing.grid.schematic.imperial",
                ("editing.grid.schematic", false) => "editing.grid.schematic.metric",
                ("editing.grid.symbol", true) => "editing.grid.symbol.imperial",
                ("editing.grid.symbol", false) => "editing.grid.symbol.metric",
                ("editing.grid.pcb", true) => "editing.grid.pcb.imperial",
                ("editing.grid.pcb", false) => "editing.grid.pcb.metric",
                (_, true) => "editing.grid.footprint.imperial",
                (_, false) => "editing.grid.footprint.metric",
            };
            if rows.is_empty() {
                return Err((key, "a grid table needs at least one row".to_owned()));
            }
            let single = key.starts_with("editing.grid.schematic.")
                || key.starts_with("editing.grid.symbol.");
            for row in rows.iter() {
                if single && row.snap_y.is_some() {
                    return Err((
                        key,
                        "sheets and Symbols keep a single grid step: leave out snap_y".to_owned(),
                    ));
                }
                let lengths = [Some(row.snap), row.snap_y, Some(row.visible), row.range];
                if lengths.into_iter().flatten().any(|l| l.0 <= Nm(0)) {
                    return Err((key, "grid steps and ranges must be positive".to_owned()));
                }
            }
        }
    }
    for (key, decimals) in [
        ("editing.display_precision.mm", precision.mm),
        ("editing.display_precision.mil", precision.mil),
    ] {
        if decimals > DisplayPrecision::MAX {
            return Err((key, format!("{decimals} is more than 6 decimals")));
        }
    }
    Ok(())
}
