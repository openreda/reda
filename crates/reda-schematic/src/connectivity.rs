//! Derived connectivity: the Nets and Junctions that follow from the Schematic Sheets'
//! drawing. Nothing here is stored; derive again after every transaction.
//!
//! The rules, from the spec:
//! - A Wire connects where one of its ends touches a Pin's connection point, another
//!   Wire's end, or anywhere along another Wire. A Pin in the middle of a Wire, or two
//!   Wires crossing mid-segment, do not connect.
//! - Pins whose connection points coincide connect.
//! - A Net Label joins the Wires through its point (and any Pin there) to every Net Label
//!   with the same name on the sheet.
//! - A Junction sits where three or more Wire ends, Wire passes and Pins meet, at least
//!   one of them a Wire end.
//! - A Port joins the Wires through its point (and any Pin there) to every Port with the
//!   same name on any sheet; a Power Port likewise to every Power Port of its name in the
//!   Project. Net Labels stay on their sheet.
//! - Every Pin of every placed Unit is in exactly one Net.
//! - A Net's name is chosen by priority: Power Port > Port > Net Label > the auto-name
//!   `Net-(<Designator>-<Pin number>)`, each in natural order, over the whole Project-wide
//!   Net. [`project_connectivity`] names each sheet's Nets that way; [`derive`] alone
//!   sees only its sheet.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{ComponentRef, Pin, Project, SchematicSheet, UnitInstance, Uuid, Wire};
use schemars::JsonSchema;
use serde::Serialize;

use crate::natural::natural_cmp;

/// The derived connectivity of one Schematic Sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SheetConnectivity {
    /// The sheet.
    pub document: Uuid,
    /// Named Nets first, in natural order of name; then unnamed Nets by lowest Wire UUID.
    pub nets: Vec<SchematicNet>,
    /// Every Junction on the sheet, sorted by x then y.
    pub junctions: Vec<Point>,
    #[serde(skip)]
    #[schemars(skip)]
    index: BTreeMap<Uuid, Vec<usize>>,
    #[serde(skip)]
    #[schemars(skip)]
    pin_index: BTreeMap<(Uuid, Uuid), usize>,
}

/// One Net on a Schematic Sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SchematicNet {
    /// The winning name by priority: a Power Port's name, else a Port's, else a Net
    /// Label's, else the auto-name `Net-(<Designator>-<Pin number>)` from its lowest
    /// Designator and Pin number in natural order; chosen over the whole Project-wide Net
    /// when derived by [`project_connectivity`]. `null` for a Net with neither a name nor
    /// a Pin.
    pub name: Option<String>,
    /// Every distinct Net Label name on the Net, in natural order. More than one name of
    /// any kind is a naming conflict for ERC to report.
    pub labels: Vec<String>,
    /// Every distinct Port name on the Net, in natural order.
    pub port_names: Vec<String>,
    /// Every distinct Power Port name on the Net, in natural order.
    pub power_port_names: Vec<String>,
    /// Its Pins, by Designator then Pin number in natural order.
    pub pins: Vec<NetPin>,
    /// Its Wires, sorted by UUID.
    pub wires: Vec<Uuid>,
    /// Its Net Labels, sorted by UUID.
    pub net_labels: Vec<Uuid>,
    /// Its Ports, sorted by UUID.
    pub ports: Vec<Uuid>,
    /// Its Power Ports, sorted by UUID.
    pub power_ports: Vec<Uuid>,
    /// The Junctions on it, sorted by x then y.
    pub junctions: Vec<Point>,
}

/// A Pin of a placed Unit, as a member of a Net.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct NetPin {
    /// The placed Unit.
    pub unit: Uuid,
    pub component: ComponentRef,
    pub designator: String,
    /// The Symbol Pin.
    pub pin: Uuid,
    pub number: String,
    /// Its connection point on the sheet.
    pub at: Point,
    /// The No-Connect Marker on it, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_connect: Option<Uuid>,
}

impl SheetConnectivity {
    /// The Nets an object is part of: one for a Wire, Net Label or No-Connect Marker, one
    /// per Pin's Net for a Unit. Empty for anything else.
    pub fn nets_of(&self, object: Uuid) -> impl Iterator<Item = &SchematicNet> {
        self.index
            .get(&object)
            .into_iter()
            .flatten()
            .map(|&i| &self.nets[i])
    }

    /// The Net holding Pin `pin` of placed Unit `unit`.
    pub fn pin_net(&self, unit: Uuid, pin: Uuid) -> Option<&SchematicNet> {
        self.pin_index.get(&(unit, pin)).map(|&i| &self.nets[i])
    }

    /// The Nets with this name. On one sheet that is at most one.
    pub fn net_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a SchematicNet> {
        self.nets
            .iter()
            .filter(move |n| n.name.as_deref() == Some(name))
    }

    /// Every object in the Nets named `name`: Wires, Net Labels, No-Connect Markers and the
    /// Units with a Pin on them.
    pub fn objects_named(&self, name: &str) -> BTreeSet<Uuid> {
        self.net_named(name)
            .flat_map(SchematicNet::objects)
            .collect()
    }
}

impl SchematicNet {
    fn empty() -> SchematicNet {
        SchematicNet {
            name: None,
            labels: vec![],
            port_names: vec![],
            power_port_names: vec![],
            pins: vec![],
            wires: vec![],
            net_labels: vec![],
            ports: vec![],
            power_ports: vec![],
            junctions: vec![],
        }
    }

    /// Every distinct name on this Net by priority: Power Port names, then Port names, then
    /// Net Label names, each in natural order. The first is the winner.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.power_port_names
            .iter()
            .chain(&self.port_names)
            .chain(&self.labels)
    }

    /// Every object on this Net: Wires, Net Labels, Ports, Power Ports, No-Connect Markers
    /// and the Units with a Pin on it.
    pub fn objects(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.wires
            .iter()
            .chain(&self.net_labels)
            .chain(&self.ports)
            .chain(&self.power_ports)
            .copied()
            .chain(self.pins.iter().map(|p| p.unit))
            .chain(self.pins.iter().filter_map(|p| p.no_connect))
    }
}

/// The connectivity of every Schematic Sheet, in Document order, with each Net named by
/// priority over the Project-wide Net it is part of.
pub fn project_connectivity(project: &Project) -> Vec<SheetConnectivity> {
    joined(project).0
}

/// The connectivity of one Schematic Sheet, its Nets named as [`project_connectivity`]
/// names them. `None` if the Project has no such sheet.
pub fn sheet_connectivity(project: &Project, sheet: Uuid) -> Option<SheetConnectivity> {
    project_connectivity(project)
        .into_iter()
        .find(|c| c.document == sheet)
}

/// A Net across the whole Project: the sheet Nets that Ports and Power Ports join.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ProjectNet {
    /// The winning name, chosen as for a sheet Net over every part of this Net.
    pub name: Option<String>,
    /// Every distinct name on it by priority: Power Port names, then Port names, then Net
    /// Label names, each in natural order. More than one is a naming conflict.
    pub names: Vec<String>,
    /// The sheets it is drawn on, in Document order.
    pub sheets: Vec<Uuid>,
    /// Its Pins on every sheet, by Designator then Pin number in natural order.
    pub pins: Vec<SheetPin>,
}

/// A Pin of a Project-wide Net, with the sheet it is on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SheetPin {
    /// The sheet.
    pub document: Uuid,
    #[serde(flatten)]
    pub pin: NetPin,
}

/// Every Net of the Project, joined across sheets by Ports and Power Ports. Named Nets come
/// first in natural order of name, then unnamed ones in sheet order.
pub fn project_nets(project: &Project) -> Vec<ProjectNet> {
    let (sheets, groups) = joined(project);
    let mut nets: Vec<ProjectNet> = groups
        .into_iter()
        .map(|members| {
            let mut net = ProjectNet {
                name: None,
                names: vec![],
                sheets: vec![],
                pins: vec![],
            };
            let mut names = (vec![], vec![], vec![]);
            for (s, n) in members {
                let conn = &sheets[s];
                let part = &conn.nets[n];
                if net.sheets.last() != Some(&conn.document) {
                    net.sheets.push(conn.document);
                }
                net.name = part.name.clone();
                names.0.extend(part.power_port_names.iter().cloned());
                names.1.extend(part.port_names.iter().cloned());
                names.2.extend(part.labels.iter().cloned());
                net.pins.extend(part.pins.iter().map(|pin| SheetPin {
                    document: conn.document,
                    pin: pin.clone(),
                }));
            }
            for list in [&mut names.0, &mut names.1, &mut names.2] {
                list.sort_by(|a, b| natural_cmp(a, b));
                list.dedup();
            }
            net.names = names.0.into_iter().chain(names.1).chain(names.2).collect();
            net.pins.sort_by(|a, b| pin_order(&a.pin, &b.pin));
            net
        })
        .collect();
    nets.sort_by(|a, b| match (&a.name, &b.name) {
        (Some(x), Some(y)) => natural_cmp(x, y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
    nets
}

/// Every sheet's connectivity, Project-named, and the Project-wide Nets as lists of
/// `(sheet index, Net index)` in sheet order, in order of their first member.
pub(crate) fn joined(project: &Project) -> (Vec<SheetConnectivity>, Vec<Vec<(usize, usize)>>) {
    let mut sheets: Vec<SheetConnectivity> = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .map(derive)
        .collect();
    let mut items = vec![];
    for (s, conn) in sheets.iter().enumerate() {
        items.extend((0..conn.nets.len()).map(|n| (s, n)));
    }
    let mut sets = Sets((0..items.len()).collect());
    let mut by_name: BTreeMap<(bool, &str), usize> = BTreeMap::new();
    for (i, &(s, n)) in items.iter().enumerate() {
        let net = &sheets[s].nets[n];
        let keys = (net
            .power_port_names
            .iter()
            .map(|name| (true, name.as_str())))
        .chain(net.port_names.iter().map(|name| (false, name.as_str())));
        for key in keys {
            let first = *by_name.entry(key).or_insert(i);
            sets.union(first, i);
        }
    }
    let mut groups: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    for (i, &item) in items.iter().enumerate() {
        groups.entry(sets.find(i)).or_default().push(item);
    }
    let groups: Vec<Vec<(usize, usize)>> = groups.into_values().collect();

    // Name every part of a Project-wide Net by the winner over all of it.
    let mut renames: Vec<(usize, usize, Option<String>)> = vec![];
    for members in groups.iter().filter(|m| m.len() > 1) {
        let parts = || members.iter().map(|&(s, n)| &sheets[s].nets[n]);
        let best = |pick: fn(&SchematicNet) -> &Vec<String>| {
            parts()
                .flat_map(pick)
                .min_by(|a, b| natural_cmp(a, b))
                .cloned()
        };
        let name = best(|n| &n.power_port_names)
            .or_else(|| best(|n| &n.port_names))
            .or_else(|| best(|n| &n.labels))
            .or_else(|| {
                parts()
                    .flat_map(|n| &n.pins)
                    .min_by(|a, b| pin_order(a, b))
                    .map(auto_name)
            });
        renames.extend(members.iter().map(|&(s, n)| (s, n, name.clone())));
    }
    if renames.is_empty() {
        return (sheets, groups);
    }
    // Renaming re-sorts a sheet's Nets, so map the old indices to the new ones.
    let mut moved: Vec<Vec<usize>> = sheets.iter().map(|c| (0..c.nets.len()).collect()).collect();
    for (s, n, name) in renames {
        sheets[s].nets[n].name = name;
    }
    for (s, conn) in sheets.iter_mut().enumerate() {
        let order = conn.reindex();
        for (new, old) in order.into_iter().enumerate() {
            moved[s][old] = new;
        }
    }
    let groups = groups
        .into_iter()
        .map(|members| members.into_iter().map(|(s, n)| (s, moved[s][n])).collect())
        .collect();
    (sheets, groups)
}

/// Where Pin `pin` of placed Unit `unit` connects on the sheet: the Pin's point, mirrored
/// left to right when the Unit is, rotated counter-clockwise by its rotation and moved to
/// its origin. Rounded to the nearest nanometre for angles off the quarter turns.
pub fn pin_position(unit: &UnitInstance, pin: &Pin) -> Point {
    let x = if unit.mirror { -pin.at.x.0 } else { pin.at.x.0 };
    let y = pin.at.y.0;
    let (x, y) = rotate(x, y, unit.rotation);
    Point::new(unit.at.x + Nm(x), unit.at.y + Nm(y))
}

pub(crate) fn rotate(x: i64, y: i64, angle: Millideg) -> (i64, i64) {
    match angle.normalized().0 {
        0 => (x, y),
        90_000 => (-y, x),
        180_000 => (-x, -y),
        270_000 => (y, -x),
        a => {
            let rad = (a as f64 / 1000.0).to_radians();
            let (sin, cos) = rad.sin_cos();
            let (fx, fy) = (x as f64, y as f64);
            (
                (fx * cos - fy * sin).round() as i64,
                (fx * sin + fy * cos).round() as i64,
            )
        }
    }
}

/// Whether `p` lies on the Wire strictly between its ends.
pub fn in_interior(wire: &Wire, p: Point) -> bool {
    on_segment(wire, p) && p != wire.from && p != wire.to
}

/// Whether `p` lies on the Wire, ends included.
pub fn on_segment(wire: &Wire, p: Point) -> bool {
    let (a, b) = (wire.from, wire.to);
    if a.x == b.x {
        p.x == a.x && between(p.y, a.y, b.y)
    } else if a.y == b.y {
        p.y == a.y && between(p.x, a.x, b.x)
    } else {
        false
    }
}

fn between(v: Nm, a: Nm, b: Nm) -> bool {
    a.min(b) <= v && v <= a.max(b)
}

/// Every Pin of every placed Unit on the sheet, with its connection point.
pub fn placed_pins(sheet: &SchematicSheet) -> Vec<(&UnitInstance, &Pin, Point)> {
    sheet
        .units()
        .flat_map(|unit| {
            let entry = sheet.part(unit.part).expect("a Unit's part entry exists");
            entry
                .symbol
                .unit_pins(unit.unit)
                .map(move |pin| (unit, pin, pin_position(unit, pin)))
        })
        .collect()
}

/// Wires indexed by the line they run along, for point lookups.
struct WireIndex<'a> {
    horizontal: BTreeMap<Nm, Vec<&'a Wire>>,
    vertical: BTreeMap<Nm, Vec<&'a Wire>>,
}

impl<'a> WireIndex<'a> {
    fn new(wires: impl Iterator<Item = &'a Wire>) -> WireIndex<'a> {
        let mut index = WireIndex {
            horizontal: BTreeMap::new(),
            vertical: BTreeMap::new(),
        };
        for w in wires {
            if w.from.y == w.to.y {
                index.horizontal.entry(w.from.y).or_default().push(w);
            } else {
                index.vertical.entry(w.from.x).or_default().push(w);
            }
        }
        index
    }

    /// The Wires passing through `p`, ends included.
    fn through(&self, p: Point) -> impl Iterator<Item = &'a Wire> + '_ {
        let h = self.horizontal.get(&p.y).into_iter().flatten();
        let v = self.vertical.get(&p.x).into_iter().flatten();
        h.chain(v).copied().filter(move |w| on_segment(w, p))
    }
}

/// A disjoint-set forest over item indices.
struct Sets(Vec<usize>);

impl Sets {
    fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while self.0[root] != root {
            root = self.0[root];
        }
        let mut i = i;
        while self.0[i] != root {
            let next = self.0[i];
            self.0[i] = root;
            i = next;
        }
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

/// Derives the Nets and Junctions of one sheet from its drawing.
pub fn derive(sheet: &SchematicSheet) -> SheetConnectivity {
    let wires: Vec<&Wire> = sheet.wires().collect();
    let labels: Vec<_> = sheet.net_labels().collect();
    let ports: Vec<_> = sheet.ports().collect();
    let powers: Vec<_> = sheet.power_ports().collect();
    let pins = placed_pins(sheet);
    // Items: Wires, then Pins, then Net Labels, then Ports, then Power Ports.
    let wire_item: BTreeMap<Uuid, usize> =
        wires.iter().enumerate().map(|(i, w)| (w.uuid, i)).collect();
    let pin_base = wires.len();
    let label_base = pin_base + pins.len();
    let port_base = label_base + labels.len();
    let power_base = port_base + ports.len();
    let mut sets = Sets((0..power_base + powers.len()).collect());
    let index = WireIndex::new(wires.iter().copied());

    let mut pins_at: BTreeMap<Point, Vec<usize>> = BTreeMap::new();
    for (i, (_, _, at)) in pins.iter().enumerate() {
        pins_at.entry(*at).or_default().push(pin_base + i);
    }
    let mut ends_at: BTreeMap<Point, Vec<usize>> = BTreeMap::new();
    for (i, w) in wires.iter().enumerate() {
        ends_at.entry(w.from).or_default().push(i);
        ends_at.entry(w.to).or_default().push(i);
    }

    // Pins on the same point.
    for items in pins_at.values() {
        for pair in items.windows(2) {
            sets.union(pair[0], pair[1]);
        }
    }
    // Wire ends join everything through their point.
    let mut junctions = BTreeSet::new();
    for (&p, ends) in &ends_at {
        let mut passes = 0;
        for other in index.through(p) {
            let o = wire_item[&other.uuid];
            sets.union(ends[0], o);
            if in_interior(other, p) {
                passes += 1;
            }
        }
        let pins_here = pins_at.get(&p).map_or(&[][..], Vec::as_slice);
        for &pin in pins_here {
            sets.union(ends[0], pin);
        }
        if ends.len() + 2 * passes + pins_here.len() >= 3 {
            junctions.insert(p);
        }
    }
    // Net Labels, Ports and Power Ports join the Wires and Pins under them, and each
    // other by kind and name.
    let named = (labels.iter().enumerate())
        .map(|(i, l)| (label_base + i, 0, l.name.as_str(), l.at))
        .chain((ports.iter().enumerate()).map(|(i, p)| (port_base + i, 1, p.name.as_str(), p.at)))
        .chain(
            (powers.iter().enumerate()).map(|(i, p)| (power_base + i, 2, p.name.as_str(), p.at)),
        );
    let mut by_name: BTreeMap<(u8, &str), usize> = BTreeMap::new();
    for (item, kind, name, at) in named {
        for w in index.through(at) {
            sets.union(item, wire_item[&w.uuid]);
        }
        for &pin in pins_at.get(&at).into_iter().flatten() {
            sets.union(item, pin);
        }
        let first = *by_name.entry((kind, name)).or_insert(item);
        sets.union(first, item);
    }

    // Gather the sets into Nets.
    let mut groups: BTreeMap<usize, SchematicNet> = BTreeMap::new();
    fn group<'a>(
        groups: &'a mut BTreeMap<usize, SchematicNet>,
        sets: &mut Sets,
        item: usize,
    ) -> &'a mut SchematicNet {
        groups
            .entry(sets.find(item))
            .or_insert_with(SchematicNet::empty)
    }
    let mut no_connects: BTreeMap<Point, Uuid> = BTreeMap::new();
    for marker in sheet.no_connects() {
        no_connects.entry(marker.at).or_insert(marker.uuid);
    }
    for (i, w) in wires.iter().enumerate() {
        group(&mut groups, &mut sets, i).wires.push(w.uuid);
    }
    for (i, (unit, pin, at)) in pins.iter().enumerate() {
        group(&mut groups, &mut sets, pin_base + i)
            .pins
            .push(NetPin {
                unit: unit.uuid,
                component: unit.component.clone(),
                designator: unit.designator.clone(),
                pin: pin.uuid,
                number: pin.number.clone(),
                at: *at,
                no_connect: no_connects.get(at).copied(),
            });
    }
    for (i, label) in labels.iter().enumerate() {
        let net = group(&mut groups, &mut sets, label_base + i);
        net.net_labels.push(label.uuid);
        net.labels.push(label.name.clone());
    }
    for (i, port) in ports.iter().enumerate() {
        let net = group(&mut groups, &mut sets, port_base + i);
        net.ports.push(port.uuid);
        net.port_names.push(port.name.clone());
    }
    for (i, power) in powers.iter().enumerate() {
        let net = group(&mut groups, &mut sets, power_base + i);
        net.power_ports.push(power.uuid);
        net.power_port_names.push(power.name.clone());
    }
    for p in &junctions {
        let first_end = ends_at[p][0];
        group(&mut groups, &mut sets, first_end).junctions.push(*p);
    }

    let mut conn = SheetConnectivity {
        document: sheet.uuid,
        nets: groups.into_values().map(finish).collect(),
        junctions: junctions.into_iter().collect(),
        index: BTreeMap::new(),
        pin_index: BTreeMap::new(),
    };
    conn.reindex();
    conn
}

impl SheetConnectivity {
    /// Sorts the Nets and rebuilds the lookups. Returns the old index of each Net in its
    /// new place.
    fn reindex(&mut self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.nets.len()).collect();
        order.sort_by(|&a, &b| net_order(&self.nets[a], &self.nets[b]));
        let mut nets: Vec<Option<SchematicNet>> = self.nets.drain(..).map(Some).collect();
        self.nets = order
            .iter()
            .map(|&i| nets[i].take().expect("once"))
            .collect();
        self.index.clear();
        self.pin_index.clear();
        for (i, net) in self.nets.iter().enumerate() {
            for object in net.objects() {
                let list = self.index.entry(object).or_default();
                if list.last() != Some(&i) {
                    list.push(i);
                }
            }
            for pin in &net.pins {
                self.pin_index.insert((pin.unit, pin.pin), i);
            }
        }
        order
    }
}

/// Sorts a Net's members and picks its name.
fn finish(mut net: SchematicNet) -> SchematicNet {
    net.wires.sort();
    net.net_labels.sort();
    net.ports.sort();
    net.power_ports.sort();
    for names in [
        &mut net.labels,
        &mut net.port_names,
        &mut net.power_port_names,
    ] {
        names.sort_by(|a, b| natural_cmp(a, b));
        names.dedup();
    }
    net.pins.sort_by(pin_order);
    net.junctions.sort();
    let name = net.names().next().cloned();
    net.name = name.or_else(|| net.pins.first().map(auto_name));
    net
}

fn pin_order(a: &NetPin, b: &NetPin) -> Ordering {
    natural_cmp(&a.designator, &b.designator)
        .then_with(|| natural_cmp(&a.number, &b.number))
        .then_with(|| a.unit.cmp(&b.unit))
}

/// The name of a Net with no Power Port, Port or Net Label, from its lowest Pin.
fn auto_name(pin: &NetPin) -> String {
    format!("Net-({}-{})", pin.designator, pin.number)
}

fn net_order(a: &SchematicNet, b: &SchematicNet) -> Ordering {
    match (&a.name, &b.name) {
        (Some(x), Some(y)) => natural_cmp(x, y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
    .then_with(|| first_member(a).cmp(&first_member(b)))
}

fn first_member(net: &SchematicNet) -> Option<Uuid> {
    net.objects().min()
}
