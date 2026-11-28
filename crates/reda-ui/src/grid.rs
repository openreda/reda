//! The snap grid of each open Document (docs/spec/v0.1/editing.md#grid): one row of its
//! Document type's preset tables (the `editing.grid` Preferences), or a custom value, kept
//! as view state and changed by `view.grid.set`, `view.grid.cycle` (G and Shift+G) and
//! `view.grid.custom` (`Mod`+Shift+G). The canvas draws the row's visible grid and the
//! Tools snap to its step.
//!
//! Sheets and Symbols ship 10, 25, 50 and 100 mil and the metric 0.25, 0.5, 1 and 2.5 mm,
//! starting at 50 mil. Boards and Footprints ship 0.1, 0.25, 0.5 and 1 mm and 5, 10, 25 and
//! 50 mil, a board starting at the middle row of the `pcb.default_units` Preference. The
//! `schematic.default_grid` and `pcb.default_grid` Preferences pick another starting row.
//! Cycling steps through the In cycle rows of the current one's table. On a board the
//! grid's table is also the unit lengths show in (`view.units.toggle` switches both).
//!
//! A board or Footprint grid may have a Step Y other than its Step X ([`Step`],
//! docs/spec/v0.3/grids-guides.md#grid-x--y): a preset row's optional `snap_y`, or the
//! custom value `0.5, 1.27`. Snapping, the arrow steps and the visible grid each follow
//! their own axis, and the status bar shows `X × Y`. Sheets and Symbols keep one step.
//!
//! A board also offers its own Board Grids (docs/spec/v0.3/grids-guides.md#board-grids):
//! G and Shift+G cycle its In-cycle ones together with the In-cycle presets of the units
//! the board shows, and a board with no grid of its own in the view state starts on its
//! Default Board Grid. Picking or cycling to a Board Grid leaves the units as they are.

mod commands;
pub mod custom_dialog;

use std::collections::{BTreeMap, BTreeSet};

use reda_engine::command::Length;
use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{BoardGrid, DocumentKind, LibraryElement, Project, Uuid};
use reda_engine::preferences::{
    GridPreferences, GridRow, GridStyle, GridTables, Preferences, Units,
};

use crate::bars::DocumentType;

pub use commands::register_all;

/// The board's Output Origin phases its grid; sheets and Library canvases use zero.
pub fn origin(project: Option<&Project>, document: Uuid) -> Point {
    project
        .and_then(|p| p.pcb_document(document))
        .and_then(|pcb| pcb.output_origin())
        .map(|origin| origin.at)
        .unwrap_or_default()
}

const MIL: i64 = 25_400;
const MM: i64 = 1_000_000;

/// A sheet's grid until one is picked: 50 mil.
pub const SCHEMATIC_DEFAULT: Nm = Nm(50 * MIL);

/// A board's grid until one is picked: the middle shipped row in `units`.
pub fn pcb_default(units: Units) -> Nm {
    GridTables::board().table(units == Units::Mil)[2].snap.0
}

/// The Electrical Range of a grid with no row: 0.8 of the step.
pub fn default_range(pitch: Nm) -> Nm {
    Nm(pitch.0 * 4 / 5)
}

/// Whether `pitch` is a whole number of mil (else it is metric).
pub fn is_imperial(pitch: Nm) -> bool {
    pitch.0 % MIL == 0
}

/// `pitch` as the status bar shows it: `50 mil` or `0.25 mm`.
pub fn label(pitch: Nm) -> String {
    format!("{} {}", number(pitch), unit(pitch))
}

/// `pitch` in its unit, without the unit: `50` or `0.25`.
fn number(pitch: Nm) -> String {
    if is_imperial(pitch) {
        format!("{}", pitch.0 / MIL)
    } else {
        format!("{}", pitch.0 as f64 / MM as f64)
    }
}

fn unit(pitch: Nm) -> &'static str {
    if is_imperial(pitch) { "mil" } else { "mm" }
}

/// A grid's step along X and along Y. They differ only on a board or Footprint grid with
/// a Step Y (docs/spec/v0.3/grids-guides.md#grid-x--y); sheets and Symbols keep one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Step {
    pub x: Nm,
    pub y: Nm,
}

impl Step {
    /// The same step on both axes.
    pub const fn square(pitch: Nm) -> Step {
        Step { x: pitch, y: pitch }
    }

    /// A preset row's Step X and Step Y.
    pub fn of_row(row: &GridRow) -> Step {
        Step {
            x: row.step_x(),
            y: row.step_y(),
        }
    }

    pub fn is_square(self) -> bool {
        self.x == self.y
    }

    /// Step Y when it differs from Step X, as Commands and `view.json` write it.
    pub fn y_if_not_square(self) -> Option<Nm> {
        (!self.is_square()).then_some(self.y)
    }

    /// The step from Step X and an optional Step Y (unset is Step X).
    pub fn new(x: Nm, y: Option<Nm>) -> Step {
        Step {
            x,
            y: y.unwrap_or(x),
        }
    }
}

/// The params of `view.grid.set` that put `document` on `step`'s row: `grid_y` only when
/// it differs from Step X.
pub fn set_params(document: Uuid, step: Step) -> serde_json::Value {
    let mut params = serde_json::json!({"document": document, "grid": step.x});
    if let Some(y) = step.y_if_not_square() {
        params["grid_y"] = serde_json::json!(y);
    }
    params
}

/// `step` as the status bar shows it: one value for a square grid (`0.5 mm`), else
/// `X × Y` in Step X's unit (`0.5 × 1.27 mm`, though 1.27 mm is 50 mil), or with a unit
/// each when Step Y is not a whole number of mil on a mil grid (`10 mil × 0.5 mm`).
pub fn step_label(step: Step) -> String {
    if step.is_square() {
        return label(step.x);
    }
    let y_in_mm = |y: Nm| format!("{}", y.0 as f64 / MM as f64);
    match (is_imperial(step.x), is_imperial(step.y)) {
        (true, true) => format!("{} × {} mil", number(step.x), number(step.y)),
        (true, false) => format!("{} × {}", label(step.x), label(step.y)),
        (false, _) => format!("{} × {} mm", number(step.x), y_in_mm(step.y)),
    }
}

/// The type of an open Document or Library element; `None` for a Part or nothing.
pub fn document_type(project: &Project, document: Uuid) -> Option<DocumentType> {
    if let Some(entry) = project.document(document) {
        return Some(entry.kind.into());
    }
    match project.element(document)? {
        LibraryElement::Symbol(_) => Some(DocumentType::Symbol),
        LibraryElement::Footprint(_) => Some(DocumentType::Footprint),
        LibraryElement::Part(_) | LibraryElement::BoardTemplate(_) => None,
    }
}

/// Whether a Document type's grid rows have an Electrical Range (sheets and Symbols).
pub fn has_range(ty: DocumentType) -> bool {
    matches!(ty, DocumentType::Schematic | DocumentType::Symbol)
}

/// What the canvas draws of a Document's grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visible {
    /// The fine dots' or lines' spacing along X and Y.
    pub pitch: Step,
    /// Off by `view.grid.show`.
    pub shown: bool,
    pub style: GridStyle,
    /// Every how many fine ones a coarse dot or line is drawn.
    pub coarse: u32,
}

impl Default for Visible {
    fn default() -> Visible {
        Visible {
            pitch: Step::square(SCHEMATIC_DEFAULT),
            shown: true,
            style: GridStyle::Dots,
            coarse: 5,
        }
    }
}

/// The grids picked so far, per Document, in the engine's view state; the grids a sheet
/// and a board start on; and the preset tables (both from the Preferences).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grids {
    pub picked: BTreeMap<Uuid, Step>,
    /// Documents on a custom grid (`view.grid.custom`), with its Electrical Range on a
    /// sheet or Symbol. Picking or cycling a row discards it.
    pub custom: BTreeMap<Uuid, Option<Nm>>,
    /// Documents whose visible grid is turned off.
    pub hidden: BTreeSet<Uuid>,
    /// Show Grids & Guides is off (`view.grids_guides.show`): no Guide is drawn or picked,
    /// though each still snaps.
    pub aids_hidden: bool,
    pub schematic: Step,
    pub pcb: Step,
    /// The `editing.grid` Preferences.
    pub tables: GridPreferences,
}

impl Default for Grids {
    fn default() -> Grids {
        Grids {
            picked: BTreeMap::new(),
            custom: BTreeMap::new(),
            hidden: BTreeSet::new(),
            aids_hidden: false,
            schematic: Step::square(SCHEMATIC_DEFAULT),
            pcb: Step::square(pcb_default(Units::Mm)),
            tables: GridPreferences::default(),
        }
    }
}

/// A Document type's preset tables in `prefs`.
pub fn tables_in(prefs: &GridPreferences, ty: DocumentType) -> &GridTables {
    match ty {
        DocumentType::Schematic => &prefs.schematic,
        DocumentType::Symbol => &prefs.symbol,
        DocumentType::Pcb => &prefs.pcb,
        DocumentType::Footprint => &prefs.footprint,
    }
}

/// The row of `tables` whose Step X and Step Y are `step`.
pub fn row_of(tables: &GridTables, step: Step) -> Option<&GridRow> {
    tables.rows().find(|r| Step::of_row(r) == step)
}

/// A Board Grid as a preset row: its steps, Visible and In cycle.
pub fn board_row(grid: &BoardGrid) -> GridRow {
    GridRow {
        snap: Length(grid.x()),
        snap_y: grid.step_y.map(|y| Length(y.nm())),
        range: None,
        visible: Length(grid.visible_xy().0),
        in_cycle: grid.in_cycle,
    }
}

/// `document`'s Board Grids as preset rows, when it is a board; none otherwise.
pub fn board_rows(project: Option<&Project>, document: Uuid) -> Vec<GridRow> {
    project
        .and_then(|p| p.pcb_document(document))
        .map(|pcb| pcb.board_grids().map(board_row).collect())
        .unwrap_or_default()
}

/// The step of `document`'s Default Board Grid, when it is a board with one.
pub fn board_default(project: Option<&Project>, document: Uuid) -> Option<Step> {
    let grid = project?.pcb_document(document)?.default_board_grid()?;
    Some(Step::new(grid.x(), Some(grid.y())))
}

/// Whether `step` is one of `document`'s Board Grids.
pub fn is_board_grid(project: Option<&Project>, document: Uuid, step: Step) -> bool {
    (board_rows(project, document).iter()).any(|r| Step::of_row(r) == step)
}

/// The steps G and Shift+G go through on a Document whose preset table is `imperial`:
/// the In-cycle rows of that table and, on a board, its In-cycle Board Grids, sorted by
/// size (Step X, then Step Y) with equal steps once.
pub fn cycle_of(tables: &GridTables, imperial: bool, board: &[GridRow]) -> Vec<Step> {
    let mut cycle: Vec<Step> = (tables.table(imperial).iter())
        .chain(board)
        .filter(|r| r.in_cycle)
        .map(Step::of_row)
        .collect();
    cycle.sort();
    cycle.dedup();
    cycle
}

/// The grid a Document type starts on under `preferences`: its `default_grid` when that
/// is one of the type's rows (the square one, when a row with a Step Y shares its Step X),
/// else the type's default (a board's in its default units).
pub fn preferred(kind: DocumentKind, preferences: &Preferences) -> Step {
    let (default_grid, units) = match kind {
        DocumentKind::Schematic => (
            preferences.schematic.default_grid,
            preferences.schematic.default_units,
        ),
        DocumentKind::Pcb => (preferences.pcb.default_grid, preferences.pcb.default_units),
    };
    let tables = tables_in(&preferences.editing.grid, kind.into());
    let picked = default_grid.and_then(|grid| {
        let square = Step::square(grid.nm());
        row_of(tables, square)
            .or_else(|| tables.row(grid.nm()))
            .map(Step::of_row)
    });
    match picked {
        Some(step) => step,
        None => match kind {
            DocumentKind::Schematic => Step::square(SCHEMATIC_DEFAULT),
            DocumentKind::Pcb => {
                let table = tables.table(units == Units::Mil);
                (table.get(2).or(table.last()))
                    .map_or(Step::square(pcb_default(units)), Step::of_row)
            }
        },
    }
}

impl Grids {
    /// Starts new views on the grids `preferences` picks, with its preset tables.
    pub fn follow(&mut self, preferences: &Preferences) {
        self.tables = preferences.editing.grid.clone();
        self.schematic = preferred(DocumentKind::Schematic, preferences);
        self.pcb = preferred(DocumentKind::Pcb, preferences);
    }

    /// The grid a Document of `kind` starts on.
    pub fn start(&self, kind: DocumentKind) -> Step {
        match kind {
            DocumentKind::Schematic => self.schematic,
            DocumentKind::Pcb => self.pcb,
        }
    }

    /// The grid a Document of `ty` starts on.
    pub fn start_of(&self, ty: DocumentType) -> Step {
        match ty {
            DocumentType::Schematic => self.schematic,
            DocumentType::Pcb => self.pcb,
            DocumentType::Symbol => Step::square(SCHEMATIC_DEFAULT),
            DocumentType::Footprint => Step::square(pcb_default(Units::Mm)),
        }
    }

    /// A Document type's preset tables.
    pub fn tables(&self, ty: DocumentType) -> &GridTables {
        tables_in(&self.tables, ty)
    }

    /// `document`'s grid: the one picked, else its type's default. `document` may also be
    /// an open Library element, whose editor's grids follow its kind.
    pub fn of(&self, project: Option<&Project>, document: Uuid) -> Step {
        if let Some(grid) = self.picked.get(&document) {
            return *grid;
        }
        if let Some(step) = board_default(project, document) {
            return step;
        }
        project
            .and_then(|p| document_type(p, document))
            .map_or(Step::square(SCHEMATIC_DEFAULT), |ty| self.start_of(ty))
    }

    /// The row `document`'s grid is on, unless it is custom or has none: a board's own
    /// Board Grid before a preset of the same step.
    pub fn row(&self, project: Option<&Project>, document: Uuid) -> Option<GridRow> {
        let ty = project.and_then(|p| document_type(p, document))?;
        if self.custom.contains_key(&document) {
            return None;
        }
        let step = self.of(project, document);
        let board = board_rows(project, document);
        (board.iter().find(|r| Step::of_row(r) == step))
            .or_else(|| row_of(self.tables(ty), step))
            .copied()
    }

    /// Whether `document` is on a custom grid.
    pub fn is_custom(&self, document: Uuid) -> bool {
        self.custom.contains_key(&document)
    }

    /// The Electrical Range on `document` (a sheet or Symbol): its row's, a custom
    /// grid's, else 0.8 of the step.
    pub fn range(&self, project: Option<&Project>, document: Uuid) -> Nm {
        let pitch = self.of(project, document).x;
        if let Some(range) = self.custom.get(&document) {
            return range.unwrap_or(default_range(pitch));
        }
        self.row(project, document)
            .and_then(|r| r.range)
            .map_or(default_range(pitch), |r| r.0)
    }

    /// What the canvas draws of `document`'s grid: its row's Visible, which follows the
    /// row's Step X and Step Y; a custom grid draws its own step.
    pub fn visible(&self, project: Option<&Project>, document: Uuid) -> Visible {
        let step = self.of(project, document);
        let pitch = self.row(project, document).map_or(step, |r| {
            let (x, y) = r.visible_xy();
            Step { x, y }
        });
        Visible {
            pitch,
            shown: !self.hidden.contains(&document),
            style: self.tables.style,
            coarse: self.tables.coarse_multiplier,
        }
    }

    /// The steps a grid picker offers on `document`: its preset rows, only those of the
    /// `imperial` or metric table when given, then a board's Board Grids in either units.
    pub fn offered(
        &self,
        project: Option<&Project>,
        document: Uuid,
        imperial: Option<bool>,
    ) -> Vec<Step> {
        let Some(ty) = project.and_then(|p| document_type(p, document)) else {
            return Vec::new();
        };
        let tables = self.tables(ty);
        let mut steps: Vec<Step> = (tables.rows())
            .filter(|r| imperial.is_none_or(|i| tables.is_imperial(r.snap.0) == i))
            .map(Step::of_row)
            .collect();
        for row in board_rows(project, document) {
            let step = Step::of_row(&row);
            if !steps.contains(&step) {
                steps.push(step);
            }
        }
        steps
    }

    /// The rows of `document`'s grid tables, imperial first.
    pub fn rows(&self, project: Option<&Project>, document: Uuid) -> Vec<GridRow> {
        project
            .and_then(|p| document_type(p, document))
            .map(|ty| self.tables(ty).rows().copied().collect())
            .unwrap_or_default()
    }

    /// The board grid matching `grid` in `units`: the row at the same place in that table.
    pub fn pcb_in_units(&self, grid: Step, units: Units) -> Step {
        let tables = self.tables(DocumentType::Pcb);
        let index = [&tables.metric, &tables.imperial]
            .iter()
            .find_map(|t| t.iter().position(|r| Step::of_row(r) == grid))
            .unwrap_or(2);
        let table = tables.table(units == Units::Mil);
        (table.get(index).or(table.last())).map_or(Step::square(pcb_default(units)), Step::of_row)
    }
}
