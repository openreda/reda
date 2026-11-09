//! The assembly files: the CSV BOM and CPL (pick and place) of a PCB Document.
//!
//! Both read the PCB Document's Footprint instances, which carry their Component's
//! Designator, fields, In BOM, In CPL and Not Fitted: the BOM lists those In BOM, the CPL
//! those In CPL, whatever their Component Type, and neither lists a Not Fitted one. The
//! one exception is a Mechanical Component on a sheet with no Footprint (a screw), which
//! the BOM reads from the sheet. The columns are a Fab Preset's mapping; a column with
//! nothing to say for a file (a position in the BOM) is left blank.

use std::collections::BTreeSet;

use reda_model::text::display;
use reda_model::{
    Column, ColumnSource, ComponentType, Fields, FootprintCopy, FootprintInstance, MountValues,
    PadKind, PcbDocument, Project, Side,
};
use reda_pcb::natural_cmp;

use crate::BoardInstance;
use crate::gerber::{degrees, mm};
use crate::plot::OutputFile;

/// The BOM of `pcb`: one row per Part + chosen Footprint + fields, its Designators in
/// natural order, the rows in the order of their first Designator. The mount column is
/// spelled with `mount`, the preset's `mount_values`.
pub fn bom_file(
    project: &Project,
    pcb: &PcbDocument,
    columns: &[Column],
    mount: &MountValues,
) -> OutputFile {
    let mut groups: Vec<Vec<Fitted<'_>>> = Vec::new();
    for part in bom_parts(project, pcb) {
        match groups.iter_mut().find(|g| g[0].same_line(&part)) {
            Some(group) => group.push(part),
            None => groups.push(vec![part]),
        }
    }
    let rows = groups.iter().enumerate().map(|(i, group)| {
        let first = &group[0];
        columns
            .iter()
            .map(|c| match &c.source {
                ColumnSource::Line => (i + 1).to_string(),
                ColumnSource::Quantity => group.len().to_string(),
                ColumnSource::Designator => group
                    .iter()
                    .map(|p| p.designator)
                    .collect::<Vec<_>>()
                    .join(", "),
                ColumnSource::CentreX
                | ColumnSource::CentreY
                | ColumnSource::Side
                | ColumnSource::Rotation => String::new(),
                source => first.describe(source, mount),
            })
            .collect()
    });
    OutputFile {
        name: format!("{}-BOM.csv", project.props().name),
        function: "bom",
        bytes: csv(columns, rows),
        attributes: None,
    }
}

/// The CPL of `pcb`: one row per placed part in natural Designator order, positions in mm
/// from the instance transform, rotation in degrees counter-clockwise as the Footprint is turned
/// from its IPC-7351 zero orientation, and the side it sits on. The mount column is spelled
/// with `mount`, the preset's `mount_values`, as in the BOM.
pub fn cpl_file(
    project: &Project,
    instances: &[BoardInstance<'_>],
    columns: &[Column],
    mount: &MountValues,
) -> OutputFile {
    let rows = instances
        .iter()
        .flat_map(|board| {
            cpl_parts(board.pcb)
                .into_iter()
                .map(move |part| (board.transform, part))
        })
        .enumerate()
        .map(|(i, (transform, (instance, part)))| {
            let at = transform.point(instance.at);
            columns
                .iter()
                .map(|c| match &c.source {
                    ColumnSource::Line => (i + 1).to_string(),
                    ColumnSource::Quantity => "1".into(),
                    ColumnSource::Designator => instance.designator.clone(),
                    ColumnSource::CentreX => mm(at.x),
                    ColumnSource::CentreY => mm(at.y),
                    ColumnSource::Side => match instance.side {
                        Side::Top => "Top".into(),
                        Side::Bottom => "Bottom".into(),
                    },
                    ColumnSource::Rotation => degrees(transform.angle(instance.rotation)),
                    source => part.describe(source, mount),
                })
                .collect()
        });
    OutputFile {
        name: format!("{}-CPL.csv", project.props().name),
        function: "cpl",
        bytes: csv(columns, rows),
        attributes: None,
    }
}

/// A part that goes into the BOM or CPL: a Footprint instance with its Footprint copy, or
/// for the BOM a footprintless Mechanical Component on a sheet.
struct Fitted<'a> {
    designator: &'a str,
    fields: &'a Fields,
    /// Left out for a Component with no Footprint.
    copy: Option<&'a FootprintCopy>,
}

impl<'a> Fitted<'a> {
    fn instance(pcb: &'a PcbDocument, instance: &'a FootprintInstance) -> Option<Fitted<'a>> {
        Some(Fitted {
            designator: &instance.designator,
            fields: &instance.fields,
            copy: Some(pcb.footprint_copy(instance.footprint)?),
        })
    }

    /// Whether `other` shares this one's BOM line: the same Footprint (or none) and
    /// fields (which hold the Part's own).
    fn same_line(&self, other: &Fitted<'_>) -> bool {
        self.copy.map(|c| c.footprint.uuid) == other.copy.map(|c| c.footprint.uuid)
            && self.fields == other.fields
    }

    /// The value of a column the BOM and CPL fill the same way. A part with no Footprint
    /// leaves the Footprint and mount type blank; a Footprint with any plated through-hole
    /// Pad is `mount.tht`, any other `mount.smt`.
    fn describe(&self, source: &ColumnSource, mount: &MountValues) -> String {
        match (source, self.copy) {
            (ColumnSource::Value, _) => display(&self.fields.value).into_owned(),
            (ColumnSource::Footprint, Some(copy)) => copy.footprint.name.clone(),
            (ColumnSource::Mount, Some(copy)) => {
                let tht = copy
                    .footprint
                    .pads
                    .iter()
                    .any(|p| p.kind == PadKind::PlatedThroughHole);
                if tht { &mount.tht } else { &mount.smt }.clone()
            }
            (ColumnSource::Field(name), _) => display(field(self.fields, name)).into_owned(),
            _ => String::new(),
        }
    }
}

/// The field a preset column names: a built-in field by any case (`MPN` is `mpn`),
/// otherwise the free-form field of that name, exactly or else ignoring case.
fn field<'a>(fields: &'a Fields, name: &str) -> &'a str {
    let lower = name.to_ascii_lowercase();
    if Fields::BUILT_IN.contains(&lower.as_str()) {
        return fields.get(&lower);
    }
    fields
        .custom
        .iter()
        .find(|f| f.name == name)
        .or_else(|| {
            fields
                .custom
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(name))
        })
        .map_or("", |f| f.value.as_str())
}

/// What the BOM lists, in natural Designator order: every Footprint instance In BOM, and
/// every Mechanical Component on a sheet In BOM with no Footprint choice; none Not Fitted.
fn bom_parts<'a>(project: &'a Project, pcb: &'a PcbDocument) -> Vec<Fitted<'a>> {
    let on_board = pcb
        .footprints()
        .filter(|i| i.in_bom && !i.not_fitted)
        .filter_map(|i| Fitted::instance(pcb, i));
    let mut seen = BTreeSet::new();
    let on_sheets = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|sheet| sheet.units())
        .filter(|u| {
            u.component_type == ComponentType::Mechanical
                && u.footprint.is_none()
                && u.in_bom
                && !u.not_fitted
        })
        .filter(|u| seen.insert(&u.component))
        .map(|u| Fitted {
            designator: &u.designator,
            fields: &u.fields,
            copy: None,
        });
    let mut parts: Vec<Fitted<'a>> = on_board.chain(on_sheets).collect();
    parts.sort_by(|a, b| natural_cmp(a.designator, b.designator));
    parts
}

/// What the CPL lists, in natural Designator order: every Footprint instance In CPL and
/// not Not Fitted.
fn cpl_parts(pcb: &PcbDocument) -> Vec<(&FootprintInstance, Fitted<'_>)> {
    let mut parts: Vec<_> = pcb
        .footprints()
        .filter(|i| i.in_cpl && !i.not_fitted)
        .filter_map(|i| Some((i, Fitted::instance(pcb, i)?)))
        .collect();
    parts.sort_by(|a, b| {
        natural_cmp(&a.0.designator, &b.0.designator).then(a.0.uuid.cmp(&b.0.uuid))
    });
    parts
}

/// A CSV file: the header row, then `rows`, each line ending in `\n`.
fn csv(columns: &[Column], rows: impl Iterator<Item = Vec<String>>) -> Vec<u8> {
    let mut out = String::new();
    let mut line = |cells: &mut dyn Iterator<Item = &str>| {
        let cells: Vec<String> = cells.map(quote).collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    };
    line(&mut columns.iter().map(|c| c.header.as_str()));
    for row in rows {
        line(&mut row.iter().map(String::as_str));
    }
    out.into_bytes()
}

/// A CSV cell, quoted (with inner quotes doubled) when it holds a comma, a quote or a line
/// break.
fn quote(cell: &str) -> String {
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_with_commas_quotes_or_line_breaks_are_quoted() {
        assert_eq!(quote("10k"), "10k");
        assert_eq!(quote("R1, R2"), "\"R1, R2\"");
        assert_eq!(quote("5\" long"), "\"5\"\" long\"");
        assert_eq!(quote("a\nb"), "\"a\nb\"");
    }
}
