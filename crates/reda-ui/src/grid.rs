//! The snap grid of each open Document: one of its Document type's presets, kept as view
//! state and changed by `view.grid.set` and `view.grid.cycle` (G). The canvas draws it and
//! the Tools snap to it.
//!
//! Sheets offer 10, 25, 50 and 100 mil and the metric 0.25, 0.5, 1 and 2.5 mm, starting at
//! 50 mil. Boards offer 0.1, 0.25, 0.5 and 1 mm and 5, 10, 25 and 50 mil, starting at the
//! middle preset of the `pcb.default_units` Preference. Cycling steps through the presets
//! of the current one's unit. On a board the grid's unit is also the unit lengths show in
//! (`view.units.toggle` switches both).

use std::collections::BTreeMap;

use reda_engine::command::{Command, CommandError, Context, Kind, Length, Meta, Registry};
use reda_engine::model::geom::Nm;
use reda_engine::model::{DocumentKind, Project, Uuid};
use reda_engine::preferences::Units;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pcb::BoardView;

const MIL: i64 = 25_400;
const MM: i64 = 1_000_000;

/// The sheet presets, imperial first, each unit in ascending order.
pub const SCHEMATIC_PRESETS: [Nm; 8] = [
    Nm(10 * MIL),
    Nm(25 * MIL),
    Nm(50 * MIL),
    Nm(100 * MIL),
    Nm(MM / 4),
    Nm(MM / 2),
    Nm(MM),
    Nm(5 * MM / 2),
];

/// A sheet's grid until one is picked: 50 mil.
pub const SCHEMATIC_DEFAULT: Nm = Nm(50 * MIL);

/// The metric board presets: 0.1, 0.25, 0.5 and 1 mm.
pub const PCB_MM: [Nm; 4] = [Nm(MM / 10), Nm(MM / 4), Nm(MM / 2), Nm(MM)];
/// The imperial board presets: 5, 10, 25 and 50 mil.
pub const PCB_MIL: [Nm; 4] = [Nm(5 * MIL), Nm(10 * MIL), Nm(25 * MIL), Nm(50 * MIL)];
/// Every board preset, metric first.
pub const PCB_PRESETS: [Nm; 8] = [
    PCB_MM[0], PCB_MM[1], PCB_MM[2], PCB_MM[3], PCB_MIL[0], PCB_MIL[1], PCB_MIL[2], PCB_MIL[3],
];

/// The board presets in `units`, finest first.
pub fn pcb_presets(units: Units) -> &'static [Nm; 4] {
    match units {
        Units::Mm => &PCB_MM,
        Units::Mil => &PCB_MIL,
    }
}

/// A board's grid until one is picked: the middle preset in `units`.
pub fn pcb_default(units: Units) -> Nm {
    pcb_presets(units)[2]
}

/// The board grid matching `grid` in `units`: the preset at the same place in that list.
pub fn pcb_in_units(grid: Nm, units: Units) -> Nm {
    let index = [PCB_MM, PCB_MIL]
        .iter()
        .find_map(|list| list.iter().position(|g| *g == grid))
        .unwrap_or(2);
    pcb_presets(units)[index]
}

/// The grid presets of a Document type; empty when it has none yet.
pub fn presets(kind: DocumentKind) -> &'static [Nm] {
    match kind {
        DocumentKind::Schematic => &SCHEMATIC_PRESETS,
        DocumentKind::Pcb => &PCB_PRESETS,
    }
}

/// The grid a Document type starts with.
pub fn default_grid(kind: DocumentKind) -> Nm {
    match kind {
        DocumentKind::Schematic => SCHEMATIC_DEFAULT,
        DocumentKind::Pcb => pcb_default(Units::Mm),
    }
}

/// Whether `pitch` is a whole number of mil (else it is metric).
pub fn is_imperial(pitch: Nm) -> bool {
    pitch.0 % MIL == 0
}

/// `pitch` as the status bar shows it: `50 mil` or `0.25 mm`.
pub fn label(pitch: Nm) -> String {
    if is_imperial(pitch) {
        format!("{} mil", pitch.0 / MIL)
    } else {
        let mm = pitch.0 as f64 / MM as f64;
        format!("{mm} mm")
    }
}

/// The grids picked so far, per Document, in the engine's view state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grids(pub BTreeMap<Uuid, Nm>);

impl Grids {
    /// `document`'s grid: the one picked, else its type's default.
    pub fn of(&self, project: Option<&Project>, document: Uuid) -> Nm {
        self.0.get(&document).copied().unwrap_or_else(|| {
            project
                .and_then(|p| p.document(document))
                .map_or(SCHEMATIC_DEFAULT, |d| default_grid(d.kind))
        })
    }
}

/// Adds `view.grid.set` and `view.grid.cycle` to `registry`. The Shell does this.
pub fn register_all(registry: &mut Registry) {
    registry.add(GridSet);
    registry.add(GridCycle);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridSetParams {
    /// The Document whose grid to set.
    document: Uuid,
    /// The grid pitch: one of the Document type's presets, such as `"25mil"` or
    /// `"0.5mm"`.
    grid: Length,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridCycleParams {
    /// The Document whose grid to step.
    document: Uuid,
}

/// The grid now in force.
#[derive(Serialize, JsonSchema)]
pub struct GridResult {
    grid: Nm,
}

/// Makes `grid` `document`'s grid; on a board, lengths then show in the grid's unit.
fn put(cx: &mut Context<'_>, document: Uuid, grid: Nm) -> Result<(), CommandError> {
    let board = cx.project()?.pcb().is_some_and(|pcb| pcb.uuid == document);
    cx.view::<Grids>()?.0.insert(document, grid);
    if board && let Ok(view) = cx.view::<BoardView>() {
        view.units = if is_imperial(grid) {
            Units::Mil
        } else {
            Units::Mm
        };
    }
    Ok(())
}

/// The Document's kind and its presets, or why it has none.
fn document_presets(
    cx: &mut Context<'_>,
    document: Uuid,
) -> Result<(Nm, &'static [Nm]), CommandError> {
    let project = cx.project()?;
    let Some(entry) = project.document(document) else {
        return Err(CommandError::not_found(format!("no Document {document}")));
    };
    let (presets, default) = (presets(entry.kind), default_grid(entry.kind));
    if presets.is_empty() {
        return Err(CommandError::rejected(
            "this Document type has no grid presets yet",
        ));
    }
    let current = cx.view::<Grids>()?.0.get(&document).copied();
    Ok((current.unwrap_or(default), presets))
}

struct GridSet;

impl Command for GridSet {
    const META: Meta = Meta {
        id: "view.grid.set",
        title: "Set Snap Grid",
        description: "Sets a Document's snap grid to one of its type's presets: 10, 25, 50 \
                      or 100 mil, or 0.25, 0.5, 1 or 2.5 mm on a sheet; 0.1, 0.25, 0.5 or 1 \
                      mm, or 5, 10, 25 or 50 mil on a board, whose units follow the grid's.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = GridSetParams;
    type Output = GridResult;

    fn run(&self, cx: &mut Context<'_>, p: GridSetParams) -> Result<GridResult, CommandError> {
        let (_, presets) = document_presets(cx, p.document)?;
        let grid = p.grid.0;
        if !presets.contains(&grid) {
            let list: Vec<String> = presets.iter().map(|g| label(*g)).collect();
            return Err(CommandError::invalid_params(
                "/grid",
                format!("not a grid preset; pick one of {}", list.join(", ")),
            ));
        }
        put(cx, p.document, grid)?;
        Ok(GridResult { grid })
    }
}

struct GridCycle;

impl Command for GridCycle {
    const META: Meta = Meta {
        id: "view.grid.cycle",
        title: "Cycle Snap Grid",
        description: "Steps a Document's snap grid to the next preset in the same unit, \
                      from the finest again after the coarsest.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = GridCycleParams;
    type Output = GridResult;

    fn run(&self, cx: &mut Context<'_>, p: GridCycleParams) -> Result<GridResult, CommandError> {
        let (current, presets) = document_presets(cx, p.document)?;
        let same_unit: Vec<Nm> = presets
            .iter()
            .copied()
            .filter(|g| is_imperial(*g) == is_imperial(current))
            .collect();
        let grid = match same_unit.iter().position(|g| *g == current) {
            Some(i) => same_unit[(i + 1) % same_unit.len()],
            None => same_unit.first().copied().unwrap_or(presets[0]),
        };
        put(cx, p.document, grid)?;
        Ok(GridResult { grid })
    }
}
