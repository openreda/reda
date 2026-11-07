//! Marker positions belong to the check result, so they survive later edits.
use crate::connectivity::pin_position;
use reda_model::geom::{Nm, Point};
use reda_model::{Object, Project, Scope, Violation, ViolationArg};

pub(super) fn complete(project: &Project, v: &mut Violation) {
    let document = v.document.or_else(|| {
        v.objects
            .iter()
            .find_map(|id| match project.object(*id)?.scope() {
                Scope::Document(d) => Some(d),
                _ => None,
            })
    });
    let Some(document) = document else {
        return;
    };
    let Some(sheet) = project.schematic_sheet(document) else {
        return;
    };
    v.document = Some(document);
    if v.at.is_some() {
        return;
    }
    if let (Some(ViolationArg::Int(x)), Some(ViolationArg::Int(y))) =
        (v.args.get("x"), v.args.get("y"))
    {
        v.at = Some(Point::new(Nm(*x), Nm(*y)));
        return;
    }
    for pair in v.objects.windows(2) {
        if let Some(unit) = sheet.unit(pair[0])
            && let Some(entry) = sheet.part(unit.part)
            && let Some(pin) = entry
                .symbol
                .unit_pins(unit.unit)
                .find(|p| p.uuid == pair[1])
        {
            v.at = Some(pin_position(unit, pin));
            return;
        }
    }
    v.at = v.objects.iter().find_map(|id| match project.object(*id)? {
        Object::Unit(o) => Some(o.at),
        Object::Wire(o) => o.vertices.first().copied(),
        Object::NetLabel(o) => Some(o.at),
        Object::Port(o) => Some(o.at),
        Object::PowerPort(o) => Some(o.at),
        Object::NoConnect(o) => Some(o.at),
        Object::SheetText(o) => Some(o.at),
        Object::SheetGraphic(o) => Some(o.shape.ends().0),
        _ => None,
    });
}
