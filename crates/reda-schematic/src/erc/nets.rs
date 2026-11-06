//! The ERC checks of the drawing and its Nets: unconnected Pins, pin-type conflicts,
//! undriven power, single-Pin Nets, dangling Wire ends, unattached Net Labels and Ports,
//! No-Connect Markers on connected Pins, Nets with several names on a sheet or across
//! sheets, different Nets with one board name and Ports on one sheet.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::Point;
use reda_model::net_name::{name_key, same_name};
use reda_model::{ErcCheck, PinType, Project, SchematicSheet, Uuid, Violation};

use super::Report;
use super::matrix::{conflict, type_name};
use crate::connectivity::{
    NameSource, Naming, NetPin, SchematicNet, SheetConnectivity, joined, name_priority, naming,
    on_segment, placed_pins, project_nets,
};
use crate::natural::natural_cmp;

pub(super) fn check(project: &Project, report: &mut Report) {
    let (sheets, groups) = joined(project);
    for conn in &sheets {
        let sheet = project
            .schematic_sheet(conn.document)
            .expect("connectivity is derived from the Project's sheets");
        sheet_nets(sheet, conn, report);
        drawing(sheet, report);
    }
    for members in &groups {
        let parts: Vec<(&SchematicSheet, &SchematicNet)> = members
            .iter()
            .map(|&(s, n)| {
                let conn = &sheets[s];
                let sheet = project.schematic_sheet(conn.document).expect("a sheet");
                (sheet, &conn.nets[n])
            })
            .collect();
        project_net(&parts, report);
        net_names(project, &parts, report);
    }
    duplicate_nets(project, report);
    single_sheet_ports(project, report);
}

/// `R1-2`: a Pin as the user names it.
fn pin_name(pin: &NetPin) -> String {
    format!("{}-{}", pin.designator, pin.number)
}

fn pin_type(sheet: &SchematicSheet, pin: &NetPin) -> PinType {
    sheet
        .unit(pin.unit)
        .and_then(|u| sheet.part(u.part))
        .and_then(|e| e.symbol.pins.iter().find(|p| p.uuid == pin.pin))
        .map_or(PinType::Passive, |p| p.electrical_type)
}

/// Whether a sheet Net has nothing but its Pins: no Wire, Net Label, Port or Power Port.
fn bare(net: &SchematicNet) -> bool {
    net.wires.is_empty()
        && net.net_labels.is_empty()
        && net.ports.is_empty()
        && net.power_ports.is_empty()
}

/// Unconnected Pins and No-Connect Markers on connected Pins, one sheet Net at a time.
fn sheet_nets(sheet: &SchematicSheet, conn: &SheetConnectivity, report: &mut Report) {
    for net in &conn.nets {
        if let [pin] = net.pins.as_slice()
            && bare(net)
            && pin.no_connect.is_none()
        {
            let check = match pin_type(sheet, pin) {
                PinType::NotConnected => None,
                PinType::Input | PinType::PowerIn => Some(ErcCheck::UnconnectedInput),
                _ => Some(ErcCheck::UnconnectedPin),
            };
            if let Some(check) = check {
                report.add(check, |v| {
                    v.arg("pin", pin_name(pin)).objects([pin.unit, pin.pin])
                });
            }
        }
        let connected = net.pins.len() > 1 || !bare(net);
        for pin in net.pins.iter().filter(|_| connected) {
            if let Some(marker) = pin.no_connect {
                report.add(ErcCheck::NoConnectConnected, |v| {
                    v.arg("pin", pin_name(pin))
                        .objects([marker, pin.unit, pin.pin])
                });
            }
        }
    }
}

/// Dangling Wire ends and Net Labels or Ports attached to nothing.
fn drawing(sheet: &SchematicSheet, report: &mut Report) {
    let pins: BTreeSet<Point> = placed_pins(sheet).into_iter().map(|(_, _, p)| p).collect();
    let on_wire = |p: Point, except: Option<Uuid>| {
        sheet
            .wires()
            .any(|w| Some(w.uuid) != except && on_segment(w, p))
    };
    if !report.is_off(ErcCheck::DanglingWire) {
        let attached: BTreeSet<Point> = (sheet.net_labels().map(|l| l.at))
            .chain(sheet.ports().map(|p| p.at))
            .chain(sheet.power_ports().map(|p| p.at))
            .collect();
        for wire in sheet.wires() {
            for end in [wire.from, wire.to] {
                if !pins.contains(&end)
                    && !attached.contains(&end)
                    && !on_wire(end, Some(wire.uuid))
                {
                    report.add(ErcCheck::DanglingWire, |v| {
                        v.arg("x", end.x).arg("y", end.y).object(wire.uuid)
                    });
                }
            }
        }
    }
    let loose = |at: Point| !pins.contains(&at) && !on_wire(at, None);
    for label in sheet.net_labels().filter(|l| loose(l.at)) {
        report.add(ErcCheck::UnattachedLabel, |v| {
            v.arg("name", label.name.as_str()).object(label.uuid)
        });
    }
    for port in sheet.ports().filter(|p| loose(p.at)) {
        report.add(ErcCheck::UnattachedLabel, |v| {
            v.arg("name", port.name.as_str()).object(port.uuid)
        });
    }
}

/// The checks of one Project-wide Net, given as its parts on each sheet.
fn project_net(parts: &[(&SchematicSheet, &SchematicNet)], report: &mut Report) {
    let net_name = parts
        .first()
        .and_then(|(_, n)| n.name.clone())
        .unwrap_or_default();
    let mut pins: Vec<(&NetPin, PinType)> = parts
        .iter()
        .flat_map(|&(sheet, net)| net.pins.iter().map(move |p| (p, pin_type(sheet, p))))
        .collect();
    pins.sort_by(|a, b| {
        natural_cmp(&a.0.designator, &b.0.designator)
            .then_with(|| natural_cmp(&a.0.number, &b.0.number))
            .then_with(|| a.0.unit.cmp(&b.0.unit))
    });

    // Single-Pin Net: wired, labelled or ported, with one Pin in the whole Project.
    if let [(pin, kind)] = pins.as_slice()
        && !parts.iter().all(|(_, n)| bare(n))
        && pin.no_connect.is_none()
        && *kind != PinType::NotConnected
    {
        report.add(ErcCheck::SinglePinNet, |v| {
            v.arg("net", net_name.as_str())
                .arg("pin", pin_name(pin))
                .objects([pin.unit, pin.pin])
        });
    }

    // Pin-type conflicts, one per pair of Pins.
    for (i, &(a, ta)) in pins.iter().enumerate() {
        for &(b, tb) in &pins[i + 1..] {
            if let Some(check) = conflict(ta, tb) {
                report.add(check, |v| {
                    v.arg("net", net_name.as_str())
                        .arg("pin_a", pin_name(a))
                        .arg("type_a", type_name(ta))
                        .arg("pin_b", pin_name(b))
                        .arg("type_b", type_name(tb))
                        .objects([a.unit, a.pin, b.unit, b.pin])
                });
            }
        }
    }

    // Undriven power: Power In Pins with no Power Out Pin or Power Port to feed them.
    let driven = pins.iter().any(|(_, t)| *t == PinType::PowerOut)
        || parts.iter().any(|(_, n)| !n.power_ports.is_empty());
    let power_in: Vec<&NetPin> = (pins.iter())
        .filter(|(_, t)| *t == PinType::PowerIn)
        .map(|(p, _)| *p)
        .collect();
    if !driven && !power_in.is_empty() {
        report.add(ErcCheck::UndrivenPower, |v| {
            v.arg("net", net_name.as_str())
                .objects(power_in.iter().flat_map(|p| [p.unit, p.pin]))
        });
    }
}

/// A Project-wide Net with different names: `net_names_on_sheet` for each sheet whose part
/// of it carries several, `net_names_across_sheets` when its sheets name it differently.
/// Names differing only in case are one name.
fn net_names(project: &Project, parts: &[(&SchematicSheet, &SchematicNet)], report: &mut Report) {
    let options = project.props().schematic;
    let whole = naming(parts.iter().map(|(_, n)| *n), &options);
    if whole.names.len() < 2 {
        return;
    }
    let reported = |v: Violation, names: &[String], objects: Vec<Uuid>| {
        v.arg("net", whole.name.clone().unwrap_or_default())
            .arg("names", names.join(", "))
            .arg("kind", whole.source.map_or("", NameSource::id))
            .arg(
                "reason",
                if whole.by_code_point {
                    "code_point"
                } else {
                    "priority"
                },
            )
            .objects(objects)
    };
    let kinds = name_priority(&options);
    let naming_objects = |net: &SchematicNet| -> Vec<Uuid> {
        kinds.iter().flat_map(|k| k.objects(net)).copied().collect()
    };
    let mut sheet_names: Vec<BTreeSet<String>> = vec![];
    for &(_, net) in parts {
        let on_sheet: Naming = naming(std::iter::once(net), &options);
        if on_sheet.names.is_empty() {
            continue;
        }
        sheet_names.push(on_sheet.names.iter().map(|n| name_key(n)).collect());
        if on_sheet.names.len() > 1 {
            report.add(ErcCheck::NetNamesOnSheet, |v| {
                reported(v, &on_sheet.names, naming_objects(net))
            });
        }
    }
    if sheet_names.windows(2).any(|w| w[0] != w[1]) {
        report.add(ErcCheck::NetNamesAcrossSheets, |v| {
            let objects = parts.iter().flat_map(|(_, n)| naming_objects(n)).collect();
            reported(v, &whole.names, objects)
        });
    }
}

/// Different Nets with the same board name, ignoring case: one Violation per name.
fn duplicate_nets(project: &Project, report: &mut Report) {
    if report.is_off(ErcCheck::DuplicateNets) {
        return;
    }
    let mut by_name: BTreeMap<String, Vec<_>> = BTreeMap::new();
    let nets = project_nets(project);
    for net in &nets {
        if let Some(board) = &net.board_name {
            by_name.entry(name_key(board)).or_default().push(net);
        }
    }
    let mut found: Vec<(&str, Vec<Uuid>, usize)> = vec![];
    for same in by_name.values().filter(|n| n.len() > 1) {
        let mut objects = vec![];
        for net in same {
            objects.extend(winning_objects(project, net));
        }
        let name = same[0].board_name.as_deref().unwrap_or_default();
        found.push((name, objects, same.len()));
    }
    found.sort_by(|a, b| natural_cmp(a.0, b.0));
    for (name, objects, count) in found {
        report.add(ErcCheck::DuplicateNets, |v| {
            v.arg("name", name)
                .arg("count", count as i64)
                .objects(objects)
        });
    }
}

/// The objects that give a Project-wide Net its name: its Net Labels, Ports or Power Ports
/// carrying the winning name, or for an auto-named Net its lowest Pin.
fn winning_objects(project: &Project, net: &crate::connectivity::ProjectNet) -> Vec<Uuid> {
    let Some(name) = net.name.as_deref() else {
        return vec![];
    };
    let mut out = vec![];
    for &document in &net.sheets {
        let Some(sheet) = project.schematic_sheet(document) else {
            continue;
        };
        let carries =
            |uuid: &Uuid, of: &str| net.objects.binary_search(uuid).is_ok() && same_name(of, name);
        match net.name_source {
            Some(NameSource::NetLabel) => out.extend(
                sheet
                    .net_labels()
                    .filter(|l| carries(&l.uuid, &l.name))
                    .map(|l| l.uuid),
            ),
            Some(NameSource::PowerPort) => out.extend(
                sheet
                    .power_ports()
                    .filter(|p| carries(&p.uuid, &p.name))
                    .map(|p| p.uuid),
            ),
            Some(NameSource::Port) => out.extend(
                sheet
                    .ports()
                    .filter(|p| carries(&p.uuid, &p.name))
                    .map(|p| p.uuid),
            ),
            _ => {}
        }
    }
    if out.is_empty()
        && let Some(pin) = net.pins.first()
    {
        out.extend([pin.pin.unit, pin.pin.pin]);
    }
    out
}

/// Ports whose name appears on only one sheet.
fn single_sheet_ports(project: &Project, report: &mut Report) {
    let mut by_name: BTreeMap<&str, BTreeMap<Uuid, Vec<Uuid>>> = BTreeMap::new();
    let mut order: Vec<&str> = vec![];
    for document in project.documents() {
        let Some(sheet) = project.schematic_sheet(document.uuid) else {
            continue;
        };
        for port in sheet.ports() {
            if !by_name.contains_key(port.name.as_str()) {
                order.push(&port.name);
            }
            (by_name.entry(&port.name).or_default())
                .entry(sheet.uuid)
                .or_default()
                .push(port.uuid);
        }
    }
    order.sort_by(|a, b| natural_cmp(a, b));
    for name in order {
        let sheets = &by_name[name];
        if let Some((_, ports)) = sheets.iter().next().filter(|_| sheets.len() == 1) {
            report.add(ErcCheck::SingleSheetPort, |v| {
                v.arg("name", name).objects(ports.iter().copied())
            });
        }
    }
}
