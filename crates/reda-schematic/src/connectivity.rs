//! Derived connectivity: the Nets and Junctions that follow from one Schematic Sheet's
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
//! - Every Pin of every placed Unit is in exactly one Net.

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
    /// The winning name: a Net Label's name, else the auto-name `Net-(<Designator>-<Pin
    /// number>)` from its lowest Designator and Pin number in natural order. `null` for a
    /// Net with neither a name nor a Pin.
    pub name: Option<String>,
    /// Every distinct Net Label name on the Net, in natural order. More than one is a
    /// naming conflict for ERC to report.
    pub labels: Vec<String>,
    /// Its Pins, by Designator then Pin number in natural order.
    pub pins: Vec<NetPin>,
    /// Its Wires, sorted by UUID.
    pub wires: Vec<Uuid>,
    /// Its Net Labels, sorted by UUID.
    pub net_labels: Vec<Uuid>,
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
            pins: vec![],
            wires: vec![],
            net_labels: vec![],
            junctions: vec![],
        }
    }

    /// Every object on this Net: Wires, Net Labels, No-Connect Markers and the Units with a
    /// Pin on it.
    pub fn objects(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.wires
            .iter()
            .chain(&self.net_labels)
            .copied()
            .chain(self.pins.iter().map(|p| p.unit))
            .chain(self.pins.iter().filter_map(|p| p.no_connect))
    }
}

/// The connectivity of every Schematic Sheet, in Document order.
pub fn project_connectivity(project: &Project) -> Vec<SheetConnectivity> {
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .map(derive)
        .collect()
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

fn rotate(x: i64, y: i64, angle: Millideg) -> (i64, i64) {
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
    let pins = placed_pins(sheet);
    // Items: Wires, then Pins, then Net Labels.
    let wire_item: BTreeMap<Uuid, usize> =
        wires.iter().enumerate().map(|(i, w)| (w.uuid, i)).collect();
    let pin_base = wires.len();
    let label_base = pin_base + pins.len();
    let mut sets = Sets((0..label_base + labels.len()).collect());
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
    // Net Labels join the Wires and Pins under them, and each other by name.
    let mut by_name: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, label) in labels.iter().enumerate() {
        let item = label_base + i;
        for w in index.through(label.at) {
            sets.union(item, wire_item[&w.uuid]);
        }
        for &pin in pins_at.get(&label.at).into_iter().flatten() {
            sets.union(item, pin);
        }
        let first = *by_name.entry(label.name.as_str()).or_insert(item);
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
    for p in &junctions {
        let first_end = ends_at[p][0];
        group(&mut groups, &mut sets, first_end).junctions.push(*p);
    }

    let mut nets: Vec<SchematicNet> = groups.into_values().map(finish).collect();
    nets.sort_by(net_order);
    let mut lookup: BTreeMap<Uuid, Vec<usize>> = BTreeMap::new();
    let mut pin_index = BTreeMap::new();
    for (i, net) in nets.iter().enumerate() {
        for object in net.objects() {
            let list = lookup.entry(object).or_default();
            if list.last() != Some(&i) {
                list.push(i);
            }
        }
        for pin in &net.pins {
            pin_index.insert((pin.unit, pin.pin), i);
        }
    }
    SheetConnectivity {
        document: sheet.uuid,
        nets,
        junctions: junctions.into_iter().collect(),
        index: lookup,
        pin_index,
    }
}

/// Sorts a Net's members and picks its name.
fn finish(mut net: SchematicNet) -> SchematicNet {
    net.wires.sort();
    net.net_labels.sort();
    net.labels.sort_by(|a, b| natural_cmp(a, b));
    net.labels.dedup();
    net.pins.sort_by(|a, b| {
        natural_cmp(&a.designator, &b.designator)
            .then_with(|| natural_cmp(&a.number, &b.number))
            .then_with(|| a.unit.cmp(&b.unit))
    });
    net.junctions.sort();
    net.name = net.labels.first().cloned().or_else(|| {
        net.pins
            .first()
            .map(|p| format!("Net-({}-{})", p.designator, p.number))
    });
    net
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
