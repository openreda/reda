//! The ERC checks of where things sit: Pins and Wire vertices off the connection grid, and
//! objects outside the sheet.

use reda_model::geom::{Nm, Point};
use reda_model::{ErcCheck, Object, Project, SchematicSheet};

use super::Report;
use crate::connectivity::placed_pins;
use crate::edit::bounds::bounds;

/// The connection grid Pins and Wire vertices must sit on, from the sheet origin: 50 mil,
/// the grid Library Symbols put their Pins on.
pub const CONNECTION_GRID: Nm = Nm::from_mil(50);

pub(super) fn check(project: &Project, report: &mut Report<'_>) {
    for document in project.documents() {
        let Some(sheet) = project.schematic_sheet(document.uuid) else {
            continue;
        };
        off_grid(sheet, report);
        off_sheet(sheet, report);
    }
}

fn on_grid(p: Point) -> bool {
    p.x.0 % CONNECTION_GRID.0 == 0 && p.y.0 % CONNECTION_GRID.0 == 0
}

/// One Violation per Pin off the grid, then one per Wire vertex off it.
fn off_grid(sheet: &SchematicSheet, report: &mut Report<'_>) {
    if report.is_off(ErcCheck::OffGrid) {
        return;
    }
    for (unit, pin, at) in placed_pins(sheet) {
        if !on_grid(at) {
            report.add(ErcCheck::OffGrid, |v| {
                v.arg("x", at.x)
                    .arg("y", at.y)
                    .objects([unit.uuid, pin.uuid])
            });
        }
    }
    for wire in sheet.wires() {
        for &at in wire.vertices.iter().filter(|p| !on_grid(**p)) {
            report.add(ErcCheck::OffGrid, |v| {
                v.arg("x", at.x).arg("y", at.y).object(wire.uuid)
            });
        }
    }
}

/// Every drawn object not wholly on the paper, in the sheet's object order.
fn off_sheet(sheet: &SchematicSheet, report: &mut Report<'_>) {
    if report.is_off(ErcCheck::OffSheet) {
        return;
    }
    let (w, h) = sheet.dimensions();
    for object in sheet.objects() {
        let drawn = matches!(
            object,
            Object::Unit(_)
                | Object::Wire(_)
                | Object::NetLabel(_)
                | Object::NoConnect(_)
                | Object::Port(_)
                | Object::PowerPort(_)
                | Object::SheetText(_)
                | Object::SheetGraphic(_)
        );
        if !drawn {
            continue;
        }
        let b = bounds(sheet, &object);
        if b.min.x < Nm(0) || b.min.y < Nm(0) || b.max.x > w || b.max.y > h {
            let at = b.center();
            report.add(ErcCheck::OffSheet, |v| {
                v.arg("x", at.x).arg("y", at.y).object(object.uuid())
            });
        }
    }
}
