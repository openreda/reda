//! Re-annotate settings: how a Re-annotate numbers Designators. The last settings applied
//! are stored with the Project (in `<name>.reda`), so a later run can repeat them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;
use crate::geom::Nm;

/// Where Re-annotate takes the positions it numbers by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Where the Units sit on their sheets.
    #[default]
    Schematic,
    /// Where the Footprints sit on the board. The new Designators write through to it.
    Board,
}

/// What Re-annotate renumbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Every unlocked Component in the scope.
    #[default]
    RenumberAll,
    /// Only Components whose Designator another Component holds too: the first in
    /// processing order keeps it, unless a locked or out-of-scope Component holds it.
    FixDuplicatesOnly,
}

/// Which Components a Re-annotate takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    /// Every Component in the Project.
    #[default]
    Project,
    /// Every Component with a Unit on one of the listed sheets.
    Sheets,
    /// The selected Components (from the board: Selected Only).
    Selection,
}

/// The order Re-annotate numbers in. The first four walk the schematic: sheets go in
/// Projects panel order, and sheet y grows upwards, so "down" starts at the top. The last
/// four walk the board, the bottom side mirrored in X (seen from below); board y grows
/// upwards too, so "descending Y" starts at the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Rows from the top, each left to right.
    AcrossThenDown,
    /// Rows from the bottom, each left to right.
    AcrossThenUp,
    /// Columns from the left, each top to bottom.
    DownThenAcross,
    /// Columns from the left, each bottom to top.
    UpThenAcross,
    /// Columns from the left, each bottom to top.
    AscendingXThenAscendingY,
    /// Columns from the left, each top to bottom.
    AscendingXThenDescendingY,
    /// Rows from the bottom, each left to right.
    AscendingYThenAscendingX,
    /// Rows from the top, each left to right.
    DescendingYThenAscendingX,
}

impl Order {
    /// The source this order walks.
    pub fn source(self) -> Source {
        match self {
            Order::AcrossThenDown
            | Order::AcrossThenUp
            | Order::DownThenAcross
            | Order::UpThenAcross => Source::Schematic,
            _ => Source::Board,
        }
    }

    /// The default order for `source`: rows from the top, each left to right.
    pub fn default_for(source: Source) -> Order {
        match source {
            Source::Schematic => Order::AcrossThenDown,
            Source::Board => Order::DescendingYThenAscendingX,
        }
    }

    /// How this order walks positions.
    pub fn walk(self) -> Walk {
        let (rows, across, along) = match self {
            Order::AcrossThenDown | Order::DescendingYThenAscendingX => (true, false, true),
            Order::AcrossThenUp | Order::AscendingYThenAscendingX => (true, true, true),
            Order::DownThenAcross | Order::AscendingXThenDescendingY => (false, true, false),
            Order::UpThenAcross | Order::AscendingXThenAscendingY => (false, true, true),
        };
        Walk {
            rows,
            across,
            along,
        }
    }
}

/// How an [`Order`] walks positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Walk {
    /// Rows (grouped by Y, walked along X), or else columns (grouped by X, along Y).
    pub rows: bool,
    /// Whether the rows (or columns) follow each other up their axis: rows from the
    /// bottom, columns from the left.
    pub across: bool,
    /// Whether the walk along each row (or column) goes up its axis.
    pub along: bool,
}

/// Which point of a Unit the schematic orders sort by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Location {
    /// The centre of its body.
    #[default]
    Part,
    /// Where its Designator text sits, or the body centre when it is hidden.
    Designator,
}

/// Which board sides the board orders number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Sides {
    /// The top side, then the bottom, then Components with no Footprint on the board.
    #[default]
    TopThenBottom,
    /// Only Components whose Footprint is on the top.
    Top,
    /// Only Components whose Footprint is on the bottom.
    Bottom,
}

/// Where one sheet's numbering starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetStart {
    pub sheet: Uuid,
    pub start_index: u64,
}

/// The settings of one Re-annotate, as stored for the next run. A `selection` scope is
/// stored without the selection itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReannotateSettings {
    pub source: Source,
    pub mode: Mode,
    pub scope: ScopeKind,
    /// The sheets, for scope `sheets`, in Projects panel order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sheets: Vec<Uuid>,
    pub order: Order,
    /// Schematic only: the point of a Unit to sort by.
    pub location: Location,
    /// A part this close to the first part of a row (or column) joins it.
    pub threshold: Nm,
    /// Schematic only: where each listed sheet's numbering starts; 1 for the others.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_sheet: Vec<SheetStart>,
    /// Board only: the sides to number.
    pub sides: Sides,
    /// Board only: where numbering starts.
    pub start_index: u64,
}

impl ReannotateSettings {
    /// The default threshold: 100 mil.
    pub const THRESHOLD: Nm = Nm::from_mil(100);

    /// The defaults for `source`.
    pub fn new(source: Source) -> ReannotateSettings {
        ReannotateSettings {
            source,
            mode: Mode::RenumberAll,
            scope: ScopeKind::Project,
            sheets: Vec::new(),
            order: Order::default_for(source),
            location: Location::Part,
            threshold: ReannotateSettings::THRESHOLD,
            per_sheet: Vec::new(),
            sides: Sides::TopThenBottom,
            start_index: 1,
        }
    }

    /// Where numbering starts for a Component whose earliest sheet is `sheet` (schematic),
    /// or for any Component (board).
    pub fn start_for(&self, sheet: Option<Uuid>) -> u64 {
        match self.source {
            Source::Board => self.start_index,
            Source::Schematic => sheet
                .and_then(|s| self.per_sheet.iter().find(|p| p.sheet == s))
                .map_or(1, |p| p.start_index),
        }
    }
}
