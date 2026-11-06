//! Derived connectivity: the Nets and Junctions that follow from the Schematic Sheets'
//! drawing. Nothing here is stored; derive again after every transaction.
//!
//! The rules, from the spec:
//! - A Wire connects where one of its ends touches a Pin's connection point, another
//!   Wire's end, or anywhere along another Wire. A Pin in the middle of a Wire, or two
//!   Wires crossing mid-segment, do not connect.
//! - Pins whose connection points coincide connect.
//! - A Net Label joins the Wires through its point (and any Pin there) to every Net Label
//!   with the same name on the sheet. Names compare ignoring case, so `abc` and `ABC` join.
//! - A Junction sits where three or more Wire ends, Wire passes and Pins meet, at least
//!   one of them a Wire end.
//! - A Port joins the Wires through its point (and any Pin there) to every Port with the
//!   same name on any sheet; a Power Port likewise to every Power Port of its name in the
//!   Project. Net Labels stay on their sheet.
//! - Every Pin of every placed Unit is in exactly one Net.
//! - A Net's name is chosen by priority, then the lowest by code point, over the whole
//!   Project-wide Net: Net Label > Power Port > Port > the auto-name
//!   `Net-(<Designator>-<Pin number>)`. The Project's [`SchematicOptions`] can put Power
//!   Ports first or stop Ports naming Nets. [`project_connectivity`] names each sheet's
//!   Nets that way; [`derive`] alone sees only its sheet and uses the default options.
//! - A Net's board name is its name, with `_<sheet number>` appended to a Local Net named
//!   by a Net Label when the Project's Append Sheet Numbers option is on.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::{Nm, Point};
use reda_model::net_name::{name_key, same_name};
use reda_model::{
    ComponentRef, Pin, Project, SchematicOptions, SchematicSheet, UnitInstance, Uuid, Wire,
};
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
    /// The winning name by priority (by default a Net Label's name, else a Power Port's,
    /// else a Port's, the lowest by code point within a kind), else the auto-name
    /// `Net-(<Designator>-<Pin number>)` from its lowest Designator and Pin number in
    /// natural order; chosen over the whole Project-wide Net when derived by
    /// [`project_connectivity`]. `null` for a Net with neither a name nor a Pin.
    pub name: Option<String>,
    /// Every distinct Net Label name on the Net, by code point. Names differing in more
    /// than case are a naming conflict for ERC to report.
    pub labels: Vec<String>,
    /// Every distinct Port name on the Net, by code point.
    pub port_names: Vec<String>,
    /// Every distinct Power Port name on the Net, by code point.
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

    /// The Nets with this name, ignoring case. On one sheet that is at most one.
    pub fn net_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a SchematicNet> {
        self.nets
            .iter()
            .filter(move |n| n.name.as_deref().is_some_and(|n| same_name(n, name)))
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
    /// The name the board gets: the winning name, with `_<sheet number>` appended for a
    /// Local Net named by a Net Label when Append Sheet Numbers to Local Nets is on.
    pub board_name: Option<String>,
    /// What gave it its name: `net_label`, `power_port`, `port` or `auto_name`; `null`
    /// for a Net with neither a name nor a Pin.
    pub name_source: Option<NameSource>,
    /// Every distinct name that can name it, ignoring case, by priority and then code
    /// point: the first is the winner. More than one is a naming conflict. Port names are
    /// left out when Ports may not name Nets.
    pub names: Vec<String>,
    /// The sheets it is drawn on, in Document order.
    pub sheets: Vec<Uuid>,
    /// Its Pins on every sheet, by Designator then Pin number in natural order.
    pub pins: Vec<SheetPin>,
    /// Every object on it, on every sheet (see [`SchematicNet::objects`]), sorted by UUID.
    #[serde(skip)]
    #[schemars(skip)]
    pub objects: Vec<Uuid>,
}

/// The kind of identifier that names a Net.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NameSource {
    NetLabel,
    PowerPort,
    Port,
    /// `Net-(<Designator>-<Pin number>)`, from the lowest Pin.
    AutoName,
}

impl NameSource {
    /// Its id, as Violation arguments carry it.
    pub fn id(self) -> &'static str {
        match self {
            NameSource::NetLabel => "net_label",
            NameSource::PowerPort => "power_port",
            NameSource::Port => "port",
            NameSource::AutoName => "auto_name",
        }
    }

    /// The names of this kind on a sheet Net.
    fn names(self, net: &SchematicNet) -> &[String] {
        match self {
            NameSource::NetLabel => &net.labels,
            NameSource::PowerPort => &net.power_port_names,
            NameSource::Port => &net.port_names,
            NameSource::AutoName => &[],
        }
    }

    /// The objects of this kind on a sheet Net.
    pub(crate) fn objects(self, net: &SchematicNet) -> &[Uuid] {
        match self {
            NameSource::NetLabel => &net.net_labels,
            NameSource::PowerPort => &net.power_ports,
            NameSource::Port => &net.ports,
            NameSource::AutoName => &[],
        }
    }
}

/// The kinds that can name a Net under `options`, highest priority first.
pub fn name_priority(options: &SchematicOptions) -> Vec<NameSource> {
    let mut kinds = if options.power_port_names_priority {
        vec![NameSource::PowerPort, NameSource::NetLabel]
    } else {
        vec![NameSource::NetLabel, NameSource::PowerPort]
    };
    if options.allow_ports_to_name_nets {
        kinds.push(NameSource::Port);
    }
    kinds
}

/// How a Net is named: the winner, what gave it, and every name that competed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Naming {
    /// The winner; `None` for a Net with neither a name nor a Pin.
    pub(crate) name: Option<String>,
    pub(crate) source: Option<NameSource>,
    /// Every distinct name ignoring case, by priority then code point; each spelled as
    /// its lowest by code point.
    pub(crate) names: Vec<String>,
    /// Whether the winner beat another name of its own kind, by code point, rather than
    /// only names of lower kinds.
    pub(crate) by_code_point: bool,
}

/// Names a Net made of `parts` by the priority of `options`.
pub(crate) fn naming<'a>(
    parts: impl Iterator<Item = &'a SchematicNet> + Clone,
    options: &SchematicOptions,
) -> Naming {
    let mut names: Vec<String> = vec![];
    let mut keys: BTreeSet<String> = BTreeSet::new();
    let mut source = None;
    let mut by_code_point = false;
    for kind in name_priority(options) {
        let mut of_kind: Vec<&String> = parts.clone().flat_map(|n| kind.names(n)).collect();
        of_kind.sort();
        let mut kind_keys = BTreeSet::new();
        for name in of_kind {
            let key = name_key(name);
            kind_keys.insert(key.clone());
            if keys.insert(key) {
                names.push(name.clone());
            }
        }
        if source.is_none() && !kind_keys.is_empty() {
            source = Some(kind);
            by_code_point = kind_keys.len() > 1;
        }
    }
    let (name, source) = match names.first() {
        Some(name) => (Some(name.clone()), source),
        None => match parts.flat_map(|n| &n.pins).min_by(|a, b| pin_order(a, b)) {
            Some(pin) => (Some(auto_name(pin)), Some(NameSource::AutoName)),
            None => (None, None),
        },
    };
    Naming {
        name,
        source,
        names,
        by_code_point,
    }
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
    let options = project.props().schematic;
    let (sheets, groups) = joined(project);
    let mut nets: Vec<ProjectNet> = groups
        .into_iter()
        .map(|members| {
            let parts = || members.iter().map(|&(s, n)| &sheets[s].nets[n]);
            let naming = naming(parts(), &options);
            let mut net = ProjectNet {
                board_name: naming.name.clone(),
                name: naming.name,
                name_source: naming.source,
                names: naming.names,
                sheets: vec![],
                pins: vec![],
                objects: vec![],
            };
            let mut objects = BTreeSet::new();
            for &(s, n) in &members {
                let conn = &sheets[s];
                let part = &conn.nets[n];
                if net.sheets.last() != Some(&conn.document) {
                    net.sheets.push(conn.document);
                }
                objects.extend(part.objects());
                net.pins.extend(part.pins.iter().map(|pin| SheetPin {
                    document: conn.document,
                    pin: pin.clone(),
                }));
            }
            let local = parts().all(|p| p.ports.is_empty() && p.power_ports.is_empty());
            if options.append_sheet_numbers
                && local
                && net.name_source == Some(NameSource::NetLabel)
                && let (Some(name), Some(&(s, _))) = (&net.name, members.first())
            {
                net.board_name = Some(format!("{name}_{}", s + 1));
            }
            net.objects = objects.into_iter().collect();
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

/// A Project-wide Net picked for highlighting: its name and every object on it, on every
/// sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetObjects {
    /// The Net's winning name; `None` for a Net with neither a name nor a Pin.
    pub name: Option<String>,
    /// Its Wires, Net Labels, Ports, Power Ports, No-Connect Markers and the Units with a
    /// Pin on it, on every sheet, sorted by UUID.
    pub objects: Vec<Uuid>,
}

/// The Project-wide Net under `object` on `sheet`: the Net of a Wire, Net Label, Port,
/// Power Port or No-Connect Marker, or for a Unit the Net of its Pin nearest `at`, with
/// what Ports and Power Ports join to it on other sheets. `None` when `object` carries
/// no Net (text, graphics, an unknown UUID).
pub fn net_at(project: &Project, sheet: Uuid, object: Uuid, at: Point) -> Option<NetObjects> {
    let (sheets, groups) = joined(project);
    let s = sheets.iter().position(|c| c.document == sheet)?;
    let conn = &sheets[s];
    let distance = |p: Point| {
        let (dx, dy) = ((p.x.0 - at.x.0) as f64, (p.y.0 - at.y.0) as f64);
        dx.hypot(dy)
    };
    let n = (0..conn.nets.len())
        .filter_map(|n| {
            let net = &conn.nets[n];
            let pin = net.pins.iter().filter(|p| p.unit == object);
            match pin.map(|p| distance(p.at)).min_by(f64::total_cmp) {
                Some(d) => Some((n, d)),
                None => net.objects().any(|o| o == object).then_some((n, 0.0)),
            }
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))?
        .0;
    let members = groups.iter().find(|m| m.contains(&(s, n)))?;
    let mut objects: BTreeSet<Uuid> = BTreeSet::new();
    for &(s, n) in members {
        objects.extend(sheets[s].nets[n].objects());
    }
    Some(NetObjects {
        name: conn.nets[n].name.clone(),
        objects: objects.into_iter().collect(),
    })
}

/// Every object on the Nets named `name` (ignoring case), on every sheet, sorted by UUID.
/// Nets that only share a name (Net Labels on different sheets) are all included.
pub fn nets_named(project: &Project, name: &str) -> Vec<Uuid> {
    let objects: BTreeSet<Uuid> = project_connectivity(project)
        .iter()
        .flat_map(|c| c.objects_named(name))
        .collect();
    objects.into_iter().collect()
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
    let mut by_name: BTreeMap<(bool, String), usize> = BTreeMap::new();
    for (i, &(s, n)) in items.iter().enumerate() {
        let net = &sheets[s].nets[n];
        let keys = (net.power_port_names.iter())
            .map(|name| (true, name_key(name)))
            .chain(net.port_names.iter().map(|name| (false, name_key(name))));
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
    let options = project.props().schematic;
    let mut renames: Vec<(usize, usize, Option<String>)> = vec![];
    for members in &groups {
        let parts = members.iter().map(|&(s, n)| &sheets[s].nets[n]);
        let name = naming(parts, &options).name;
        renames.extend(members.iter().map(|&(s, n)| (s, n, name.clone())));
    }
    renames.retain(|(s, n, name)| sheets[*s].nets[*n].name != *name);
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

pub(crate) use reda_model::geom::rotate;

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
    let mut by_name: BTreeMap<(u8, String), usize> = BTreeMap::new();
    for (item, kind, name, at) in named {
        for w in index.through(at) {
            sets.union(item, wire_item[&w.uuid]);
        }
        for &pin in pins_at.get(&at).into_iter().flatten() {
            sets.union(item, pin);
        }
        let first = *by_name.entry((kind, name_key(name))).or_insert(item);
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
        names.sort();
        names.dedup();
    }
    net.pins.sort_by(pin_order);
    net.junctions.sort();
    net.name = naming(std::iter::once(&net), &SchematicOptions::DEFAULT).name;
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
