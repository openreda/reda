//! The CSV BOM: what it lists, how it groups, orders and numbers its rows, and which
//! rows miss a required field.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use reda_model::text::display;
use reda_model::{
    BomGroup, BomLayout, BomSettings, ColumnSource, ComponentType, NotFittedBom, PcbDocument,
    Project, Uuid,
};
use reda_pcb::natural_cmp;

use super::{Fitted, Table, field};
use crate::plot::OutputFile;

/// The BOM file and the rows that miss a field the preset requires.
#[derive(Clone, Debug)]
pub struct Bom {
    pub file: OutputFile,
    /// The rows the file is written from, header first: what the Output Job window
    /// previews.
    pub table: Table,
    /// One entry per row and missing field, in row order, then `required_fields` order.
    pub missing: Vec<MissingField>,
}

/// A BOM row that lacks a `required_fields` value. Mechanical Components are never named,
/// so a row of only Mechanical ones never misses a field, nor does a Not Fitted row,
/// which nobody sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingField {
    /// The field as the preset names it, such as `LCSC`.
    pub field: String,
    /// The row's Designators that need it, in natural order.
    pub designators: Vec<String>,
    /// Their Footprint instances on the PCB.
    pub footprints: Vec<Uuid>,
}

/// The BOM of `pcb` with the preset's `settings`: what it lists (In BOM and not Not
/// Fitted, plus with `list_unplaced` footprintless Mechanical Components on a sheet;
/// Not Fitted ones on their own rows with quantity 0 when `not_fitted` is `list`), rows
/// grouped by `group_by` (or one per Component when `flat`) and ordered by `sort`, then
/// first Designator. Designators are in natural order, joined by `designator_separator`.
/// The line column counts from `line_numbers.start` by `step`, and is blank when off.
pub fn bom_file(project: &Project, pcb: &PcbDocument, settings: &BomSettings) -> Bom {
    let mut rows: Vec<Vec<Fitted<'_>>> = Vec::new();
    for part in bom_parts(project, pcb, settings) {
        let joins = |row: &&mut Vec<Fitted<'_>>| {
            settings.layout == BomLayout::Grouped && same_row(&row[0], &part, &settings.group_by)
        };
        match rows.iter_mut().find(joins) {
            Some(row) => row.push(part),
            None => rows.push(vec![part]),
        }
    }
    rows.sort_by(|a, b| {
        settings
            .sort
            .iter()
            .map(|source| compare(a, b, source, settings))
            .find(|o| o.is_ne())
            .unwrap_or_else(|| natural_cmp(a[0].designator, b[0].designator))
    });
    let lines = &settings.line_numbers;
    let cells = rows.iter().enumerate().map(|(i, row)| {
        settings
            .columns
            .iter()
            .map(|c| match &c.source {
                ColumnSource::Line if lines.enabled => {
                    let n = u64::from(lines.start) + i as u64 * u64::from(lines.step);
                    n.to_string()
                }
                source => cell(row, source, settings),
            })
            .collect()
    });
    let missing = rows
        .iter()
        .flat_map(|row| missing_fields(row, &settings.required_fields))
        .collect();
    let table = Table::new(&settings.columns, cells);
    Bom {
        file: OutputFile {
            name: format!("{}-BOM.csv", project.props().name),
            function: "bom",
            bytes: table.csv(),
            attributes: None,
        },
        table,
        missing,
    }
}

/// A row's cell for `source`, the line number aside.
fn cell(row: &[Fitted<'_>], source: &ColumnSource, settings: &BomSettings) -> String {
    let first = &row[0];
    match source {
        ColumnSource::Quantity if first.not_fitted => "0".into(),
        ColumnSource::Quantity => row.len().to_string(),
        ColumnSource::Designator => row
            .iter()
            .map(|p| p.designator)
            .collect::<Vec<_>>()
            .join(&settings.designator_separator),
        ColumnSource::Line
        | ColumnSource::CentreX
        | ColumnSource::CentreY
        | ColumnSource::OriginX
        | ColumnSource::OriginY
        | ColumnSource::Pad1X
        | ColumnSource::Pad1Y
        | ColumnSource::Side
        | ColumnSource::Rotation => String::new(),
        source => first.describe(source, &settings.mount_values),
    }
}

/// Two rows in `source` order: the first Designator for the Designator column, the
/// quantity by number, any other column by its text in natural order.
fn compare(
    a: &[Fitted<'_>],
    b: &[Fitted<'_>],
    source: &ColumnSource,
    settings: &BomSettings,
) -> Ordering {
    match source {
        ColumnSource::Designator => natural_cmp(a[0].designator, b[0].designator),
        source => natural_cmp(&cell(a, source, settings), &cell(b, source, settings)),
    }
}

/// Whether `part` joins the row started by `first`: both fitted or both Not Fitted, and
/// equal in every `group_by` source.
fn same_row(first: &Fitted<'_>, part: &Fitted<'_>, group_by: &[BomGroup]) -> bool {
    first.not_fitted == part.not_fitted
        && group_by.iter().all(|g| match g {
            BomGroup::Part => first.part == part.part,
            BomGroup::Footprint => {
                first.copy.map(|c| c.footprint.uuid) == part.copy.map(|c| c.footprint.uuid)
            }
            BomGroup::Fields => first.fields == part.fields,
            BomGroup::Value => first.fields.value == part.fields.value,
            BomGroup::Field(name) => field(first.fields, name) == field(part.fields, name),
        })
}

/// The `required` fields `row` lacks, naming its non-Mechanical Designators; none for a
/// Not Fitted row.
fn missing_fields(row: &[Fitted<'_>], required: &[String]) -> Vec<MissingField> {
    let needing: Vec<&Fitted<'_>> = row
        .iter()
        .filter(|p| !p.not_fitted && p.component_type != ComponentType::Mechanical)
        .collect();
    let Some(first) = needing.first() else {
        return Vec::new();
    };
    required
        .iter()
        .filter(|name| display(field(first.fields, name)).trim().is_empty())
        .map(|name| MissingField {
            field: name.clone(),
            designators: needing.iter().map(|p| p.designator.to_owned()).collect(),
            footprints: needing.iter().filter_map(|p| p.instance).collect(),
        })
        .collect()
}

/// What the BOM lists, in natural Designator order: every Footprint instance In BOM, and
/// with `list_unplaced` every Mechanical Component on a sheet In BOM with no Footprint
/// choice; Not Fitted ones only when `not_fitted` is `list`. Free Pads are not Footprint
/// instances, so they are never listed.
fn bom_parts<'a>(
    project: &'a Project,
    pcb: &'a PcbDocument,
    settings: &BomSettings,
) -> Vec<Fitted<'a>> {
    let with_not_fitted = settings.not_fitted == NotFittedBom::List;
    let listed = |in_bom: bool, not_fitted: bool| in_bom && (with_not_fitted || !not_fitted);
    let sheets: Vec<_> = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .collect();
    // Component → its Library Part, for grouping by Part.
    let parts: BTreeMap<Uuid, Uuid> = sheets
        .iter()
        .flat_map(|sheet| {
            sheet
                .units()
                .filter_map(|u| Some((u.component.uuid, sheet.part(u.part)?.part.uuid)))
        })
        .collect();
    let on_board = pcb
        .footprints()
        .filter(|i| listed(i.in_bom, i.not_fitted))
        .filter_map(|i| {
            let mut part = Fitted::instance(pcb, i)?;
            part.part = i
                .component
                .as_ref()
                .and_then(|c| parts.get(&c.uuid).copied());
            Some(part)
        });
    let mut seen = BTreeSet::new();
    let on_sheets = sheets
        .iter()
        .flat_map(|sheet| sheet.units().map(move |u| (sheet, u)))
        .filter(|(_, u)| {
            settings.list_unplaced
                && u.component_type == ComponentType::Mechanical
                && u.footprint.is_none()
                && listed(u.in_bom, u.not_fitted)
        })
        .filter(|(_, u)| seen.insert(&u.component))
        .map(|(sheet, u)| Fitted {
            designator: &u.designator,
            fields: &u.fields,
            copy: None,
            part: sheet.part(u.part).map(|e| e.part.uuid),
            component_type: u.component_type,
            not_fitted: u.not_fitted,
            instance: None,
        });
    let mut parts: Vec<Fitted<'a>> = on_board.chain(on_sheets).collect();
    parts.sort_by(|a, b| natural_cmp(a.designator, b.designator));
    parts
}
