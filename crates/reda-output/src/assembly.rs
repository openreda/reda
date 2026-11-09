//! The assembly files: the CSV BOM and CPL (pick and place) of a PCB Document.
//!
//! Both read the PCB Document's Footprint instances, which carry their Component's
//! Designator, fields, In BOM, In CPL and Not Fitted: the BOM lists those In BOM, the CPL
//! those In CPL, whatever their Component Type. The CPL never lists a Not Fitted one, the
//! BOM only on its own rows when the preset's `not_fitted` is `list`. The one exception is a Mechanical Component on a sheet with no Footprint (a screw), which
//! the BOM reads from the sheet. The columns are a Fab Preset's mapping; a column with
//! nothing to say for a file (a position in the BOM) is left blank.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::text::display;
use reda_model::{
    BottomPosition, BottomRotation, Column, ColumnSource, ComponentType, CplSettings, CplUnits,
    Fields, FootprintCopy, FootprintInstance, MountValues, PadKind, PcbDocument, Project, Side,
    Uuid,
};
use reda_pcb::natural_cmp;
use reda_pcb::placed::Frame;

mod bom;

pub use bom::{Bom, MissingField, bom_file};

use crate::BoardInstance;
use crate::gerber::degrees;
use crate::plot::OutputFile;

/// The CPL of the board `instances`: one row per placed part in natural Designator order,
/// with `cpl`'s columns, units and bottom-side conventions. Positions come from the instance
/// transform; rotation is in degrees counter-clockwise as the Footprint is turned from its
/// IPC-7351 zero orientation. The mount column is spelled with `mount`, the preset's
/// `mount_values`, as in the BOM.
pub fn cpl_file(
    project: &Project,
    instances: &[BoardInstance<'_>],
    cpl: &CplSettings,
    mount: &MountValues,
) -> OutputFile {
    let rows = instances
        .iter()
        .flat_map(|board| {
            cpl_parts(board.pcb, cpl.exclude_through_hole)
                .into_iter()
                .map(move |(instance, part)| (board, instance, part))
        })
        .enumerate()
        .map(|(i, (board, instance, part))| {
            let at = Positions::of(board, instance);
            let bottom = instance.side == Side::Bottom;
            let position = |p: Point, x: bool| {
                let flip = bottom
                    && match cpl.bottom_position {
                        BottomPosition::TopView => false,
                        BottomPosition::NegateX => x,
                        BottomPosition::NegateY => !x,
                    };
                let v = if x { p.x } else { p.y };
                length(if flip { Nm(-v.0) } else { v }, cpl.units, cpl.unit_suffix)
            };
            cpl.columns
                .iter()
                .map(|c| match &c.source {
                    ColumnSource::Line => (i + 1).to_string(),
                    ColumnSource::Quantity => "1".into(),
                    ColumnSource::Designator => instance.designator.clone(),
                    ColumnSource::CentreX => position(at.centre, true),
                    ColumnSource::CentreY => position(at.centre, false),
                    ColumnSource::OriginX => position(at.origin, true),
                    ColumnSource::OriginY => position(at.origin, false),
                    ColumnSource::Pad1X => position(at.pad1, true),
                    ColumnSource::Pad1Y => position(at.pad1, false),
                    ColumnSource::Side => match instance.side {
                        Side::Top => cpl.side_values.top.clone(),
                        Side::Bottom => cpl.side_values.bottom.clone(),
                    },
                    ColumnSource::Rotation => {
                        let stored = board.transform.angle(instance.rotation);
                        degrees(match (bottom, cpl.bottom_rotation) {
                            (false, _) | (true, BottomRotation::Stored) => stored,
                            (true, BottomRotation::Plus180) => stored + Millideg(180_000),
                            (true, BottomRotation::Negate) => Millideg(-stored.0),
                        })
                    }
                    source => part.describe(source, mount),
                })
                .collect()
        });
    OutputFile {
        name: format!("{}-CPL.csv", project.props().name),
        function: "cpl",
        bytes: csv(&cpl.columns, rows),
        attributes: None,
    }
}

/// A placed part's three CPL positions, in output coordinates.
struct Positions {
    /// The centre of the box around its copper Pads (non-plated holes left out), boxed
    /// in the Footprint's frame; its origin when it has no copper Pads.
    centre: Point,
    /// The Footprint's origin.
    origin: Point,
    /// Pad 1's anchor; the origin when it has no Pad 1.
    pad1: Point,
}

impl Positions {
    fn of(board: &BoardInstance<'_>, instance: &FootprintInstance) -> Positions {
        let frame = Frame::of(instance);
        let pads = board
            .pcb
            .footprint_copy(instance.footprint)
            .map_or(&[][..], |copy| &copy.footprint.pads[..]);
        let copper = pads
            .iter()
            .filter(|p| p.kind != PadKind::NonPlated)
            .map(|p| p.bounds())
            .reduce(|a, b| a.union(b));
        let local_origin = Point::default();
        let centre = copper.map_or(local_origin, |b| b.center());
        let pad1 = pads
            .iter()
            .find(|p| p.number == "1")
            .map_or(local_origin, |p| p.at);
        let out = |local: Point| board.transform.point(frame.point(local));
        Positions {
            centre: out(centre),
            origin: out(local_origin),
            pad1: out(pad1),
        }
    }
}

/// A CPL position: 6 decimals in `units`, rounded half away from zero, then the unit
/// when `suffix` is on.
fn length(nm: Nm, units: CplUnits, suffix: bool) -> String {
    // Millionths of the unit = nm × num / den.
    let (num, den, unit) = match units {
        CplUnits::Mm => (1, 1, "mm"),
        CplUnits::Mil => (10_000, 254, "mil"),
        CplUnits::In => (10, 254, "in"),
    };
    let scaled = i128::from(nm.0) * num;
    let micro = (scaled.abs() * 2 + den) / (den * 2);
    let sign = if scaled < 0 && micro != 0 { "-" } else { "" };
    let suffix = if suffix { unit } else { "" };
    format!(
        "{sign}{}.{:06}{suffix}",
        micro / 1_000_000,
        micro % 1_000_000
    )
}

/// A part that goes into the BOM or CPL: a Footprint instance with its Footprint copy, or
/// for the BOM a footprintless Mechanical Component on a sheet.
struct Fitted<'a> {
    designator: &'a str,
    fields: &'a Fields,
    /// Left out for a Component with no Footprint.
    copy: Option<&'a FootprintCopy>,
    /// The Library Part, when its Component is on a sheet.
    part: Option<Uuid>,
    component_type: ComponentType,
    not_fitted: bool,
    /// The Footprint instance it is, if any.
    instance: Option<Uuid>,
}

impl<'a> Fitted<'a> {
    fn instance(pcb: &'a PcbDocument, instance: &'a FootprintInstance) -> Option<Fitted<'a>> {
        Some(Fitted {
            designator: &instance.designator,
            fields: &instance.fields,
            copy: Some(pcb.footprint_copy(instance.footprint)?),
            part: None,
            component_type: instance.component_type,
            not_fitted: instance.not_fitted,
            instance: Some(instance.uuid),
        })
    }

    /// Whether its Footprint has a plated through-hole Pad.
    fn through_hole(&self) -> bool {
        self.copy.is_some_and(|copy| {
            copy.footprint
                .pads
                .iter()
                .any(|p| p.kind == PadKind::PlatedThroughHole)
        })
    }

    /// The value of a column the BOM and CPL fill the same way. A part with no Footprint
    /// leaves the Footprint and mount type blank; a Footprint with any plated through-hole
    /// Pad is `mount.tht`, any other `mount.smt`. The fitted column says "Fitted" or
    /// "Not Fitted".
    fn describe(&self, source: &ColumnSource, mount: &MountValues) -> String {
        match (source, self.copy) {
            (ColumnSource::Value, _) => display(&self.fields.value).into_owned(),
            (ColumnSource::Footprint, Some(copy)) => copy.footprint.name.clone(),
            (ColumnSource::Mount, Some(_)) => if self.through_hole() {
                &mount.tht
            } else {
                &mount.smt
            }
            .clone(),
            (ColumnSource::Fitted, _) => if self.not_fitted {
                "Not Fitted"
            } else {
                "Fitted"
            }
            .into(),
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

/// What the CPL lists, in natural Designator order: every Footprint instance In CPL and
/// not Not Fitted, leaving out those with a plated through-hole Pad when
/// `exclude_through_hole` is on.
fn cpl_parts(
    pcb: &PcbDocument,
    exclude_through_hole: bool,
) -> Vec<(&FootprintInstance, Fitted<'_>)> {
    let mut parts: Vec<_> = pcb
        .footprints()
        .filter(|i| i.in_cpl && !i.not_fitted)
        .filter_map(|i| Some((i, Fitted::instance(pcb, i)?)))
        .filter(|(_, part)| !(exclude_through_hole && part.through_hole()))
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
