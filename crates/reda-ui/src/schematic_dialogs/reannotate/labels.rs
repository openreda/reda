//! Words for the Re-annotate settings, shared by the dialog and the read-only summary on
//! Project Settings ▸ Schematic.

use reda_engine::model::Project;
use reda_engine::model::Uuid;
use reda_engine::model::annotate::{
    Location, Mode, Order, ReannotateSettings, ScopeKind, SheetStart, Sides, Source,
};
use reda_engine::model::geom::Nm;

use crate::tr;

/// Where each sheet's numbering starts, as the dialog offers it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartIndices {
    /// Every sheet from 1.
    #[default]
    None,
    /// Sheet n from n×100 + 1: R101 on sheet 1, R201 on sheet 2.
    Hundreds,
    /// Sheet n from n×1000 + 1.
    Thousands,
    /// Start indices set some other way (by a Command).
    Custom,
}

impl StartIndices {
    /// The choices the dialog offers.
    pub const OFFERED: [StartIndices; 3] = [
        StartIndices::None,
        StartIndices::Hundreds,
        StartIndices::Thousands,
    ];

    /// The `per_sheet` starts for `sheets`, in Projects panel order.
    pub fn per_sheet(self, sheets: &[Uuid]) -> Vec<SheetStart> {
        let step = match self {
            StartIndices::Hundreds => 100,
            StartIndices::Thousands => 1000,
            StartIndices::None | StartIndices::Custom => return Vec::new(),
        };
        (sheets.iter().enumerate())
            .map(|(i, sheet)| SheetStart {
                sheet: *sheet,
                start_index: (i as u64 + 1) * step + 1,
            })
            .collect()
    }

    /// Which choice `per_sheet` is, for the Project's `sheets` in order.
    pub fn of(per_sheet: &[SheetStart], sheets: &[Uuid]) -> StartIndices {
        if per_sheet.is_empty() {
            return StartIndices::None;
        }
        for choice in [StartIndices::Hundreds, StartIndices::Thousands] {
            if choice.per_sheet(sheets) == per_sheet {
                return choice;
            }
        }
        StartIndices::Custom
    }

    pub fn label(self) -> String {
        match self {
            StartIndices::None => tr!("reannotate-start-none"),
            StartIndices::Hundreds => tr!("reannotate-start-hundreds"),
            StartIndices::Thousands => tr!("reannotate-start-thousands"),
            StartIndices::Custom => tr!("reannotate-start-custom"),
        }
    }
}

/// The Project's Schematic Sheets, in Projects panel order.
pub fn sheets(project: &Project) -> Vec<Uuid> {
    (project.documents())
        .filter(|d| project.schematic_sheet(d.uuid).is_some())
        .map(|d| d.uuid)
        .collect()
}

pub fn source(source: Source) -> String {
    match source {
        Source::Schematic => tr!("reannotate-source-schematic"),
        Source::Board => tr!("reannotate-source-board"),
    }
}

pub fn mode(mode: Mode) -> String {
    match mode {
        Mode::RenumberAll => tr!("reannotate-mode-renumber-all"),
        Mode::FixDuplicatesOnly => tr!("reannotate-mode-fix-duplicates"),
    }
}

pub fn order(order: Order) -> String {
    match order {
        Order::AcrossThenDown => tr!("reannotate-across-then-down"),
        Order::AcrossThenUp => tr!("reannotate-across-then-up"),
        Order::DownThenAcross => tr!("reannotate-down-then-across"),
        Order::UpThenAcross => tr!("reannotate-up-then-across"),
        Order::AscendingXThenAscendingY => tr!("reannotate-x-up-then-y-up"),
        Order::AscendingXThenDescendingY => tr!("reannotate-x-up-then-y-down"),
        Order::AscendingYThenAscendingX => tr!("reannotate-y-up-then-x-up"),
        Order::DescendingYThenAscendingX => tr!("reannotate-y-down-then-x-up"),
    }
}

/// The orders that walk `source`, the default first.
pub fn orders(source: Source) -> [Order; 4] {
    match source {
        Source::Schematic => [
            Order::AcrossThenDown,
            Order::AcrossThenUp,
            Order::DownThenAcross,
            Order::UpThenAcross,
        ],
        Source::Board => [
            Order::DescendingYThenAscendingX,
            Order::AscendingYThenAscendingX,
            Order::AscendingXThenDescendingY,
            Order::AscendingXThenAscendingY,
        ],
    }
}

pub fn location(location: Location) -> String {
    match location {
        Location::Part => tr!("reannotate-location-part"),
        Location::Designator => tr!("reannotate-location-designator"),
    }
}

pub fn sides(sides: Sides) -> String {
    match sides {
        Sides::TopThenBottom => tr!("reannotate-sides-both"),
        Sides::Top => tr!("reannotate-sides-top"),
        Sides::Bottom => tr!("reannotate-sides-bottom"),
    }
}

/// A threshold in mil when it is a whole number of mil, else in mm.
pub fn length(length: Nm) -> String {
    let units = match length.0 % 25_400 {
        0 => reda_engine::preferences::Units::Mil,
        _ => reda_engine::preferences::Units::Mm,
    };
    crate::pcb::view::format_length(length, units)
}

/// The settings as summary lines, such as "Order: Across, then down".
pub fn summary(project: &Project, s: &ReannotateSettings) -> Vec<String> {
    let line =
        |name: String, value: String| tr!("reannotate-summary-line", name = name, value = value);
    let scope = match s.scope {
        ScopeKind::Project => tr!("reannotate-scope-project"),
        ScopeKind::Selection => tr!("reannotate-scope-selection"),
        ScopeKind::Sheets => {
            let titles: Vec<String> = (s.sheets.iter())
                .filter_map(|u| project.document(*u).map(|d| d.title.clone()))
                .collect();
            tr!("reannotate-scope-sheets-list", sheets = titles.join(", "))
        }
    };
    let mut lines = vec![
        line(tr!("reannotate-source"), source(s.source)),
        line(tr!("reannotate-mode"), mode(s.mode)),
        line(tr!("reannotate-scope"), scope),
        line(tr!("reannotate-order"), order(s.order)),
        line(tr!("reannotate-threshold"), length(s.threshold)),
    ];
    match s.source {
        Source::Schematic => {
            lines.push(line(tr!("reannotate-location"), location(s.location)));
            let starts = StartIndices::of(&s.per_sheet, &sheets(project));
            lines.push(line(tr!("reannotate-start-indices"), starts.label()));
        }
        Source::Board => {
            lines.push(line(tr!("reannotate-sides"), sides(s.sides)));
            lines.push(line(
                tr!("reannotate-start-index"),
                s.start_index.to_string(),
            ));
        }
    }
    lines
}
