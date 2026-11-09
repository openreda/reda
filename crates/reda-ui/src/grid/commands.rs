//! The grid's view Commands: `view.grid.set`, `view.grid.cycle`, `view.grid.custom` and
//! `view.grid.show`.

use reda_engine::command::{Command, CommandError, Context, Kind, Length, Meta, Registry};
use reda_engine::model::Uuid;
use reda_engine::model::geom::Nm;
use reda_engine::preferences::{GridTables, Units};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Grids, document_type, has_range, label};
use crate::bars::DocumentType;
use crate::pcb::BoardView;

/// Adds the grid Commands to `registry`. The Shell does this.
pub fn register_all(registry: &mut Registry) {
    registry.add(GridSet);
    registry.add(GridCycle);
    registry.add(GridCustom);
    registry.add(GridShow);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridSetParams {
    /// The Document whose grid to set.
    document: Uuid,
    /// The grid step: one of the Document type's preset rows, such as `"25mil"` or
    /// `"0.5mm"`.
    grid: Length,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridCycleParams {
    /// The Document whose grid to step.
    document: Uuid,
    /// Step back (Shift+G) instead of forward (G).
    #[serde(default)]
    back: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridCustomParams {
    /// The Document whose grid to set.
    document: Uuid,
    /// The grid step, with units, such as `"20mil"` or `"0.2mm"`.
    grid: Length,
    /// The Electrical Range on a sheet or Symbol; unset is 0.8 of the step. At or above
    /// the step it is kept, with a warning.
    range: Option<Length>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridShowParams {
    /// The Document whose visible grid to show or hide.
    document: Uuid,
    /// Show it or not; unset toggles.
    visible: Option<bool>,
}

/// The grid now in force.
#[derive(Serialize, JsonSchema)]
pub struct GridResult {
    grid: Nm,
    /// The Electrical Range now, on a sheet or Symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    range: Option<Nm>,
    /// Advice the grid was still set despite, such as a range at or above the step.
    #[serde(skip_serializing_if = "Option::is_none")]
    warning: Option<String>,
}

/// Whether the visible grid shows now.
#[derive(Serialize, JsonSchema)]
pub struct GridShown {
    visible: bool,
}

/// Makes `grid` `document`'s grid (custom with `range` when `custom`); on a board,
/// lengths then show in the grid's table's unit.
fn put(
    cx: &mut Context<'_>,
    document: Uuid,
    grid: Nm,
    custom: Option<Option<Nm>>,
) -> Result<GridResult, CommandError> {
    let board = cx.project()?.pcb().is_some_and(|pcb| pcb.uuid == document);
    let ty = document_type(cx.project()?, document);
    let grids = cx.view::<Grids>()?;
    grids.picked.insert(document, grid);
    match custom {
        Some(range) => grids.custom.insert(document, range),
        None => grids.custom.remove(&document),
    };
    let imperial = grids.tables(DocumentType::Pcb).is_imperial(grid);
    let range = ty.filter(|ty| has_range(*ty)).map(|ty| match custom {
        Some(range) => range.unwrap_or(super::default_range(grid)),
        None => (grids.tables(ty).row(grid))
            .and_then(|r| r.range)
            .map_or(super::default_range(grid), |r| r.0),
    });
    if board && let Ok(view) = cx.view::<BoardView>() {
        view.units = if imperial { Units::Mil } else { Units::Mm };
    }
    let warning = range.filter(|r| *r >= grid).map(|r| range_warning(r, grid));
    Ok(GridResult {
        grid,
        range,
        warning,
    })
}

/// The advice for a range at or above the step.
fn range_warning(range: Nm, grid: Nm) -> String {
    format!(
        "the Electrical Range {} is at or above the grid step {}; the cursor may snap to a \
         Hotspot a whole grid step away",
        label(range),
        label(grid)
    )
}

/// The Document's type, preset tables and grid now, or why it has none.
fn document_tables(
    cx: &mut Context<'_>,
    document: Uuid,
) -> Result<(DocumentType, GridTables, Nm), CommandError> {
    let ty = document_type(cx.project()?, document)
        .ok_or_else(|| CommandError::not_found(format!("no Document {document} with a grid")))?;
    let grids = cx.view::<Grids>()?;
    let current = grids
        .picked
        .get(&document)
        .copied()
        .unwrap_or_else(|| grids.start_of(ty));
    Ok((ty, grids.tables(ty).clone(), current))
}

struct GridSet;

impl Command for GridSet {
    const META: Meta = Meta {
        id: "view.grid.set",
        title: "Set Snap Grid",
        description: "Sets a Document's snap grid to one of its type's preset rows (the \
                      `editing.grid` Preferences): by default 10, 25, 50 or 100 mil, or \
                      0.25, 0.5, 1 or 2.5 mm on a sheet or Symbol; 0.1, 0.25, 0.5 or 1 mm, \
                      or 5, 10, 25 or 50 mil on a board or Footprint, whose units follow \
                      the grid's. Discards a custom grid.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = GridSetParams;
    type Output = GridResult;

    fn run(&self, cx: &mut Context<'_>, p: GridSetParams) -> Result<GridResult, CommandError> {
        let (_, tables, _) = document_tables(cx, p.document)?;
        let grid = p.grid.0;
        if tables.row(grid).is_none() {
            let list: Vec<String> = tables.rows().map(|r| label(r.snap.0)).collect();
            return Err(CommandError::invalid_params(
                "/grid",
                format!("not a grid preset; pick one of {}", list.join(", ")),
            ));
        }
        put(cx, p.document, grid, None)
    }
}

struct GridCycle;

impl Command for GridCycle {
    const META: Meta = Meta {
        id: "view.grid.cycle",
        title: "Cycle Snap Grid",
        description: "Steps a Document's snap grid to the next (G) or, with `back`, the \
                      previous (Shift+G) In cycle row of its preset table, wrapping round. \
                      A custom grid is discarded for the nearest row in that direction.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = GridCycleParams;
    type Output = GridResult;

    fn run(&self, cx: &mut Context<'_>, p: GridCycleParams) -> Result<GridResult, CommandError> {
        let (_, tables, current) = document_tables(cx, p.document)?;
        let custom = cx.view::<Grids>()?.is_custom(p.document);
        let imperial = tables.is_imperial(current);
        let mut cycle: Vec<Nm> = (tables.table(imperial).iter())
            .filter(|r| r.in_cycle)
            .map(|r| r.snap.0)
            .collect();
        cycle.sort();
        cycle.dedup();
        if cycle.is_empty() {
            return Err(CommandError::rejected(
                "no row of this grid table is In cycle",
            ));
        }
        let at = cycle.iter().position(|g| *g == current).filter(|_| !custom);
        let n = cycle.len();
        let grid = match (at, p.back) {
            (Some(i), false) => cycle[(i + 1) % n],
            (Some(i), true) => cycle[(i + n - 1) % n],
            // Off the cycle: the nearest row in the direction of travel.
            (None, false) => *cycle.iter().find(|g| **g > current).unwrap_or(&cycle[0]),
            (None, true) => *cycle
                .iter()
                .rev()
                .find(|g| **g < current)
                .unwrap_or(&cycle[n - 1]),
        };
        put(cx, p.document, grid, None)
    }
}

struct GridCustom;

impl Command for GridCustom {
    const META: Meta = Meta {
        id: "view.grid.custom",
        title: "Custom Snap Grid",
        description: "Sets a Document's snap grid to any step, with units. On a sheet or \
                      Symbol its Electrical Range starts at 0.8 of the step; a range at or \
                      above the step is kept and warned about. Cycling discards it.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = GridCustomParams;
    type Output = GridResult;

    fn run(&self, cx: &mut Context<'_>, p: GridCustomParams) -> Result<GridResult, CommandError> {
        let (ty, _, _) = document_tables(cx, p.document)?;
        let grid = p.grid.0;
        if grid <= Nm(0) {
            return Err(CommandError::invalid_params(
                "/grid",
                "the grid step must be positive",
            ));
        }
        let range = match p.range {
            Some(_) if !has_range(ty) => {
                return Err(CommandError::invalid_params(
                    "/range",
                    "only sheets and Symbols have an Electrical Range",
                ));
            }
            Some(r) if r.0 <= Nm(0) => {
                return Err(CommandError::invalid_params(
                    "/range",
                    "the Electrical Range must be positive",
                ));
            }
            Some(r) => Some(r.0),
            None => has_range(ty).then(|| super::default_range(grid)),
        };
        put(cx, p.document, grid, Some(range))
    }
}

struct GridShow;

impl Command for GridShow {
    const META: Meta = Meta {
        id: "view.grid.show",
        title: "Show Visible Grid",
        description: "Shows or hides a Document's visible grid; unset `visible` toggles it. \
                      Snapping is unchanged.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = GridShowParams;
    type Output = GridShown;

    fn run(&self, cx: &mut Context<'_>, p: GridShowParams) -> Result<GridShown, CommandError> {
        document_tables(cx, p.document)?;
        let grids = cx.view::<Grids>()?;
        let visible = p
            .visible
            .unwrap_or_else(|| grids.hidden.contains(&p.document));
        if visible {
            grids.hidden.remove(&p.document);
        } else {
            grids.hidden.insert(p.document);
        }
        Ok(GridShown { visible })
    }
}
