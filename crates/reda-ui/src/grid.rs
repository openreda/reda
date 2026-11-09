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

mod commands;

use std::collections::{BTreeMap, BTreeSet};

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{DocumentKind, LibraryElement, Project, Uuid};
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
    if is_imperial(pitch) {
        format!("{} mil", pitch.0 / MIL)
    } else {
        let mm = pitch.0 as f64 / MM as f64;
        format!("{mm} mm")
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
        LibraryElement::Part(_) => None,
    }
}

/// Whether a Document type's grid rows have an Electrical Range (sheets and Symbols).
pub fn has_range(ty: DocumentType) -> bool {
    matches!(ty, DocumentType::Schematic | DocumentType::Symbol)
}

/// What the canvas draws of a Document's grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visible {
    /// The fine dots' or lines' spacing.
    pub pitch: Nm,
    /// Off by `view.grid.show`.
    pub shown: bool,
    pub style: GridStyle,
    /// Every how many fine ones a coarse dot or line is drawn.
    pub coarse: u32,
}

impl Default for Visible {
    fn default() -> Visible {
        Visible {
            pitch: SCHEMATIC_DEFAULT,
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
    pub picked: BTreeMap<Uuid, Nm>,
    /// Documents on a custom grid (`view.grid.custom`), with its Electrical Range on a
    /// sheet or Symbol. Picking or cycling a row discards it.
    pub custom: BTreeMap<Uuid, Option<Nm>>,
    /// Documents whose visible grid is turned off.
    pub hidden: BTreeSet<Uuid>,
    pub schematic: Nm,
    pub pcb: Nm,
    /// The `editing.grid` Preferences.
    pub tables: GridPreferences,
}

impl Default for Grids {
    fn default() -> Grids {
        Grids {
            picked: BTreeMap::new(),
            custom: BTreeMap::new(),
            hidden: BTreeSet::new(),
            schematic: SCHEMATIC_DEFAULT,
            pcb: pcb_default(Units::Mm),
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

/// The grid a Document type starts on under `preferences`: its `default_grid` when that
/// is one of the type's rows, else the type's default (a board's in its default units).
pub fn preferred(kind: DocumentKind, preferences: &Preferences) -> Nm {
    let (default_grid, units) = match kind {
        DocumentKind::Schematic => (
            preferences.schematic.default_grid,
            preferences.schematic.default_units,
        ),
        DocumentKind::Pcb => (preferences.pcb.default_grid, preferences.pcb.default_units),
    };
    let tables = tables_in(&preferences.editing.grid, kind.into());
    match default_grid {
        Some(grid) if tables.row(grid.nm()).is_some() => grid.nm(),
        _ => match kind {
            DocumentKind::Schematic => SCHEMATIC_DEFAULT,
            DocumentKind::Pcb => {
                let table = tables.table(units == Units::Mil);
                (table.get(2).or(table.last())).map_or(pcb_default(units), |r| r.snap.0)
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
    pub fn start(&self, kind: DocumentKind) -> Nm {
        match kind {
            DocumentKind::Schematic => self.schematic,
            DocumentKind::Pcb => self.pcb,
        }
    }

    /// The grid a Document of `ty` starts on.
    pub fn start_of(&self, ty: DocumentType) -> Nm {
        match ty {
            DocumentType::Schematic => self.schematic,
            DocumentType::Pcb => self.pcb,
            DocumentType::Symbol => SCHEMATIC_DEFAULT,
            DocumentType::Footprint => pcb_default(Units::Mm),
        }
    }

    /// A Document type's preset tables.
    pub fn tables(&self, ty: DocumentType) -> &GridTables {
        tables_in(&self.tables, ty)
    }

    /// `document`'s grid: the one picked, else its type's default. `document` may also be
    /// an open Library element, whose editor's grids follow its kind.
    pub fn of(&self, project: Option<&Project>, document: Uuid) -> Nm {
        if let Some(grid) = self.picked.get(&document) {
            return *grid;
        }
        project
            .and_then(|p| document_type(p, document))
            .map_or(SCHEMATIC_DEFAULT, |ty| self.start_of(ty))
    }

    /// The row `document`'s grid is on, unless it is custom or has none.
    pub fn row(&self, project: Option<&Project>, document: Uuid) -> Option<GridRow> {
        let ty = project.and_then(|p| document_type(p, document))?;
        if self.custom.contains_key(&document) {
            return None;
        }
        self.tables(ty).row(self.of(project, document)).copied()
    }

    /// Whether `document` is on a custom grid.
    pub fn is_custom(&self, document: Uuid) -> bool {
        self.custom.contains_key(&document)
    }

    /// The Electrical Range on `document` (a sheet or Symbol): its row's, a custom
    /// grid's, else 0.8 of the step.
    pub fn range(&self, project: Option<&Project>, document: Uuid) -> Nm {
        let pitch = self.of(project, document);
        if let Some(range) = self.custom.get(&document) {
            return range.unwrap_or(default_range(pitch));
        }
        self.row(project, document)
            .and_then(|r| r.range)
            .map_or(default_range(pitch), |r| r.0)
    }

    /// What the canvas draws of `document`'s grid.
    pub fn visible(&self, project: Option<&Project>, document: Uuid) -> Visible {
        let pitch = self.of(project, document);
        Visible {
            pitch: self.row(project, document).map_or(pitch, |r| r.visible.0),
            shown: !self.hidden.contains(&document),
            style: self.tables.style,
            coarse: self.tables.coarse_multiplier,
        }
    }

    /// The rows of `document`'s grid tables, imperial first.
    pub fn rows(&self, project: Option<&Project>, document: Uuid) -> Vec<GridRow> {
        project
            .and_then(|p| document_type(p, document))
            .map(|ty| self.tables(ty).rows().copied().collect())
            .unwrap_or_default()
    }

    /// The board grid matching `grid` in `units`: the row at the same place in that table.
    pub fn pcb_in_units(&self, grid: Nm, units: Units) -> Nm {
        let tables = self.tables(DocumentType::Pcb);
        let index = [&tables.metric, &tables.imperial]
            .iter()
            .find_map(|t| t.iter().position(|r| r.snap.0 == grid))
            .unwrap_or(2);
        let table = tables.table(units == Units::Mil);
        (table.get(index).or(table.last())).map_or(pcb_default(units), |r| r.snap.0)
    }
}
