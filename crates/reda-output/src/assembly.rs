//! The assembly files: the CSV BOM and CPL (pick and place) of a PCB Document.
//!
//! Both read only the PCB Document's Footprint instances, which carry their Component's
//! Designator, fields and Not Fitted. Not Fitted Footprints and Mechanical Parts are left
//! out. The columns are a Fab Preset's mapping; a column with nothing to say for a file
//! (a position in the BOM) is left blank.

use std::collections::BTreeSet;

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    Column, ColumnSource, ComponentRef, ComponentType, Fields, FootprintCopy, FootprintInstance,
    PadKind, PcbDocument, Project, Side,
};
use reda_pcb::natural_cmp;

use crate::gerber::{arc_centre, degrees, mm};
use crate::plot::OutputFile;

/// The BOM of `pcb`: one row per Part + chosen Footprint + fields, its Designators in
/// natural order, the rows in the order of their first Designator.
pub fn bom_file(project: &Project, pcb: &PcbDocument, columns: &[Column]) -> OutputFile {
    let mut groups: Vec<Vec<Fitted<'_>>> = Vec::new();
    for part in fitted(project, pcb) {
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
                    .map(|p| p.instance.designator.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                ColumnSource::MidX
                | ColumnSource::MidY
                | ColumnSource::Layer
                | ColumnSource::Rotation => String::new(),
                source => first.describe(source),
            })
            .collect()
    });
    OutputFile {
        name: format!("{}-BOM.csv", project.props().name),
        function: "bom",
        bytes: csv(columns, rows),
    }
}

/// The CPL of `pcb`: one row per placed part in natural Designator order, positions in mm
/// from [`cpl_origin`], rotation in degrees counter-clockwise as the Footprint is turned
/// from its IPC-7351 zero orientation, and the side it sits on.
pub fn cpl_file(project: &Project, pcb: &PcbDocument, columns: &[Column]) -> OutputFile {
    let origin = cpl_origin(pcb);
    let rows = fitted(project, pcb).into_iter().enumerate().map(|(i, part)| {
        let at = part.instance.at;
        columns
            .iter()
            .map(|c| match &c.source {
                ColumnSource::Line => (i + 1).to_string(),
                ColumnSource::Quantity => "1".into(),
                ColumnSource::Designator => part.instance.designator.clone(),
                ColumnSource::MidX => mm(at.x - origin.x),
                ColumnSource::MidY => mm(at.y - origin.y),
                ColumnSource::Layer => match part.instance.side {
                    Side::Top => "Top".into(),
                    Side::Bottom => "Bottom".into(),
                },
                ColumnSource::Rotation => degrees(part.instance.rotation),
                source => part.describe(source),
            })
            .collect()
    });
    OutputFile {
        name: format!("{}-CPL.csv", project.props().name),
        function: "cpl",
        bytes: csv(columns, rows),
    }
}

/// Where CPL positions are measured from: the Placement Origin when the board has one,
/// otherwise the lower-left corner of the Board Outline's bounding box (arcs included),
/// otherwise the board's own zero.
pub fn cpl_origin(pcb: &PcbDocument) -> Point {
    if let Some(origin) = pcb.placement_origin() {
        return origin.at;
    }
    let Some(outline) = pcb.outline() else {
        return Point::new(Nm::ZERO, Nm::ZERO);
    };
    let vertices = &outline.vertices;
    let mut min = Point::new(Nm(i64::MAX), Nm(i64::MAX));
    let mut take = |p: Point| {
        min = Point::new(min.x.min(p.x), min.y.min(p.y));
    };
    for (i, v) in vertices.iter().enumerate() {
        take(v.point());
        if v.sweep != Millideg::ZERO {
            let to = vertices[(i + 1) % vertices.len()].point();
            for p in arc_extremes(v.point(), to, v.sweep) {
                take(p);
            }
        }
    }
    min
}

/// The points where the arc from `from` to `to` sweeping `sweep` reaches furthest left,
/// down, right or up, when it passes them.
fn arc_extremes(from: Point, to: Point, sweep: Millideg) -> Vec<Point> {
    let c = arc_centre(from, to, sweep);
    let (dx, dy) = ((from.x.0 - c.x.0) as f64, (from.y.0 - c.y.0) as f64);
    let r = Nm(dx.hypot(dy).round() as i64);
    let start = dy.atan2(dx).to_degrees();
    let s = sweep.0 as f64 / 1000.0;
    [
        (0.0, Point::new(c.x + r, c.y)),
        (90.0, Point::new(c.x, c.y + r)),
        (180.0, Point::new(c.x - r, c.y)),
        (270.0, Point::new(c.x, c.y - r)),
    ]
    .into_iter()
    .filter(|(axis, _)| {
        if s > 0.0 {
            (axis - start).rem_euclid(360.0) <= s
        } else {
            (start - axis).rem_euclid(360.0) <= -s
        }
    })
    .map(|(_, p)| p)
    .collect()
}

/// A Footprint instance that goes into the BOM and CPL, with its Footprint copy.
struct Fitted<'a> {
    instance: &'a FootprintInstance,
    copy: &'a FootprintCopy,
}

impl Fitted<'_> {
    /// Whether `other` shares this one's BOM line: the same Footprint and fields (which
    /// hold the Part's own).
    fn same_line(&self, other: &Fitted<'_>) -> bool {
        self.copy.footprint.uuid == other.copy.footprint.uuid
            && self.instance.fields == other.instance.fields
    }

    /// The value of a column the BOM and CPL fill the same way.
    fn describe(&self, source: &ColumnSource) -> String {
        match source {
            ColumnSource::Value => self.instance.fields.value.clone(),
            ColumnSource::Footprint => self.copy.footprint.name.clone(),
            ColumnSource::Mount => {
                let tht = self
                    .copy
                    .footprint
                    .pads
                    .iter()
                    .any(|p| p.kind == PadKind::PlatedThroughHole);
                if tht { "THT" } else { "SMT" }.into()
            }
            ColumnSource::Field(name) => field(&self.instance.fields, name).into(),
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
        .or_else(|| fields.custom.iter().find(|f| f.name.eq_ignore_ascii_case(name)))
        .map_or("", |f| f.value.as_str())
}

/// The Footprint instances to assemble, in natural Designator order: not Not Fitted, not
/// a Mechanical Part (placed on the PCB alone, or a Mechanical Component on a sheet).
fn fitted<'a>(project: &Project, pcb: &'a PcbDocument) -> Vec<Fitted<'a>> {
    let mechanical = mechanical_components(project);
    let mut parts: Vec<Fitted<'a>> = pcb
        .footprints()
        .filter(|i| !i.not_fitted)
        .filter(|i| i.component.as_ref().is_some_and(|c| !mechanical.contains(c)))
        .filter_map(|instance| {
            let copy = pcb.footprint_copy(instance.footprint)?;
            Some(Fitted { instance, copy })
        })
        .collect();
    parts.sort_by(|a, b| {
        natural_cmp(&a.instance.designator, &b.instance.designator)
            .then(a.instance.uuid.cmp(&b.instance.uuid))
    });
    parts
}

/// The Components placed on a sheet from a Mechanical Part. The Footprint instance does
/// not carry the Component Type, so it is read from the Component's Units.
fn mechanical_components(project: &Project) -> BTreeSet<ComponentRef> {
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|sheet| sheet.units())
        .filter(|u| u.component_type == ComponentType::Mechanical)
        .map(|u| u.component.clone())
        .collect()
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
    fn arcs_reach_their_extremes_only_when_they_pass_them() {
        let p = |x: i64, y: i64| Point::new(Nm(x), Nm(y));
        // Half circle from the top round the left to the bottom.
        assert_eq!(
            arc_extremes(p(0, 10), p(0, -10), Millideg(180_000)),
            [p(0, 10), p(-10, 0), p(0, -10)]
        );
        // The same ends clockwise go round the right.
        assert_eq!(
            arc_extremes(p(0, 10), p(0, -10), Millideg(-180_000)),
            [p(10, 0), p(0, 10), p(0, -10)]
        );
        // A 60° arc from 0° passes only its start.
        assert_eq!(
            arc_extremes(p(1_000_000, 0), p(500_000, 866_025), Millideg(60_000)),
            [p(1_000_000, 0)]
        );
    }

    #[test]
    fn cells_with_commas_quotes_or_line_breaks_are_quoted() {
        assert_eq!(quote("10k"), "10k");
        assert_eq!(quote("R1, R2"), "\"R1, R2\"");
        assert_eq!(quote("5\" long"), "\"5\"\" long\"");
        assert_eq!(quote("a\nb"), "\"a\nb\"");
    }
}
