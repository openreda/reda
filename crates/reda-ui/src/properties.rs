//! What the Properties panel shows for the selection, with no drawing: the values the
//! selected objects share (`<mixed>` where they differ), and the one Command each edit
//! runs, which applies to everything selected.
//!
//! - Selected Units give their Components' properties: Designator, value and the other
//!   fields, which fields show on the sheet, the Footprint (from the Part's alternates)
//!   and Not Fitted. They are edited with `schematic.component.set`.
//! - Selected Footprints (with no Units selected) give the same, edited from the board
//!   with `pcb.footprint.set`: a synced Footprint writes through to every Unit of its
//!   Component, a Mechanical Part changes only on the board. Which fields show on the
//!   sheet is not set from here.
//! - Selected Tracks, Vias, Zones, Pads and highlighted Nets give their Nets' Net Classes;
//!   picking a class adds them all to it with `pcb.net.add_to_class`, or, when they are
//!   all in it already, takes them out with `pcb.net.remove_from_class`.
//! - Selected texts and Zones give their numeric properties ([`Numbers`]), which take
//!   numeric expressions run per object and committed as one batch.
//! - Selected Tracks, Vias and Footprints give their sizes and Location the same way.
//! - One or two selected copper objects give the design rules that apply ([`Rules`]).
//! - A selected Pad or Footprint whose Footprint was edited on the board gives its local
//!   edits ([`LocalEdits`]), with the compare and the Revert Command.
//! - One selected Wire, or segments of only one Wire, give its vertex list (Index, X and
//!   Y, plus Add and Remove), edited with `schematic.wire.set_vertices`.
//! - Any one selected object shows its stored properties, read-only.

pub mod commands;
mod numbers;
pub mod overview;
pub mod rules;

use std::collections::BTreeSet;
use std::fmt;

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{
    DocumentKind, Fields, FootprintInstance, Object, Project, UnitInstance, Uuid,
};
use serde_json::{Value, json};

use crate::selection::Selection;

pub use numbers::{Kind, NumberRow, Numbers, Region};
pub use rules::Rules;

/// A value the selected objects share, or `Mixed` when they differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shared<T> {
    Same(T),
    Mixed,
}

impl<T: PartialEq> Shared<T> {
    /// What `values` share; `None` when there are none.
    pub(crate) fn of(values: impl IntoIterator<Item = T>) -> Option<Shared<T>> {
        let mut values = values.into_iter();
        let first = values.next()?;
        Some(if values.all(|v| v == first) {
            Shared::Same(first)
        } else {
            Shared::Mixed
        })
    }

    /// The shared value, if there is one.
    pub fn same(&self) -> Option<&T> {
        match self {
            Shared::Same(v) => Some(v),
            Shared::Mixed => None,
        }
    }
}

impl<T: fmt::Display> fmt::Display for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Shared::Same(v) => v.fmt(f),
            Shared::Mixed => f.write_str(MIXED),
        }
    }
}

/// How a value the selection does not share is shown.
pub const MIXED: &str = "<mixed>";

/// One edit made in the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Sets a Component field by name: `designator`, `value`, another built-in field or a
    /// free-form one. An empty value clears it.
    Field(String, String),
    /// Shows (`true`) or hides a field on the sheet.
    Visible(String, bool),
    /// Locks (`true`) or unlocks a field: Update from Library leaves a Locked Field alone.
    Locked(String, bool),
    /// Chooses the Footprint among the Part's alternates.
    Footprint(Uuid),
    InBom(bool),
    InCpl(bool),
    NotFitted(bool),
    /// Adds the selection's Nets to a Net Class, or takes them out when all are in it.
    NetClass(Uuid),
    /// Moves the listed Wire's vertex at this index to the point.
    Vertex(usize, Point),
    /// Adds a vertex to the listed Wire: a new end one snap grid (this long) on from its
    /// last vertex, square to the last segment.
    AddVertex(Nm),
    /// Removes the listed Wire's vertex at this index.
    RemoveVertex(usize),
}

/// The vertex list of the one Wire selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireVertices {
    pub wire: Uuid,
    /// In order, from the first; the list's Index is the position here.
    pub vertices: Vec<Point>,
}

impl WireVertices {
    /// The vertices after `edit`, or `None` when it doesn't apply or would leave fewer
    /// than two.
    fn edited(&self, edit: &Edit) -> Option<Vec<Point>> {
        let mut v = self.vertices.clone();
        match *edit {
            Edit::Vertex(i, to) => *v.get_mut(i)? = to,
            Edit::RemoveVertex(i) if i < v.len() && v.len() > 2 => {
                v.remove(i);
            }
            Edit::AddVertex(grid) => {
                let (a, b) = (v[v.len() - 2], v[v.len() - 1]);
                let (dx, dy) = ((b.x - a.x).0, (b.y - a.y).0);
                // Square to the last segment: across a horizontal-ish one, up; else right.
                let step = if dx.abs() >= dy.abs() {
                    Point::new(Nm(0), grid)
                } else {
                    Point::new(grid, Nm(0))
                };
                v.push(b + step);
            }
            _ => return None,
        }
        Some(v)
    }
}

/// The Properties panel's content for one selection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Properties {
    /// How many things are selected (objects, Pads and highlighted Nets).
    pub count: usize,
    /// The selected objects' kinds (`unit`, `track`…) with a count each, in first-picked
    /// order.
    pub kinds: Vec<(String, usize)>,
    /// The selected Components' shared properties, when Units are selected.
    pub component: Option<ComponentProperties>,
    /// The Net Class of the selection's Nets, when it touches any PCB Net.
    pub net_class: Option<NetClassProperty>,
    /// The numeric properties every selected object has.
    pub numbers: Numbers,
    /// With one object selected, its stored properties as `(name, value)`, read-only.
    pub details: Vec<(String, String)>,
    /// The design rules that apply, for one or two copper objects.
    pub rules: Option<Rules>,
    /// The local edits of the selected Pads' or Footprints' Footprints, when any of them
    /// was edited on the board.
    pub local_edits: Option<LocalEdits>,
    /// The selected objects of each kind, in the order of `kinds`.
    by_kind: Vec<Vec<Uuid>>,
    /// With one Wire selected (whole or by segments), its vertex list.
    pub vertices: Option<WireVertices>,
}

/// The selected Footprint instances that were edited on the board.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalEdits {
    /// The locally edited instances, by UUID.
    pub instances: Vec<Uuid>,
    /// With one instance: what changed, Pad or graphic by property, edited against
    /// original.
    pub edits: Vec<reda_engine::model::local_edit::LocalEdit>,
    /// With one instance: the Footprint as edited and the copy it came from, for the
    /// compare drawing.
    pub compare: Option<(reda_engine::model::Footprint, reda_engine::model::Footprint)>,
}

/// The properties the selected Components share.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentProperties {
    /// The Components, in the order their Units (or Footprints) were picked. A Mechanical
    /// Part has none.
    pub components: Vec<Uuid>,
    /// The Footprint instances, when the edit comes from the board (`pcb.footprint.set`);
    /// empty when Units are selected.
    pub footprints: Vec<Uuid>,
    /// `designator`, `value`, the other built-in fields, then free-form fields.
    pub fields: Vec<FieldRow>,
    /// The chosen Footprint, by Library element UUID.
    pub footprint: Shared<Option<Uuid>>,
    /// The alternates to choose from; empty unless every Component is the same Part.
    pub alternates: Vec<Alternate>,
    pub in_bom: Shared<bool>,
    pub in_cpl: Shared<bool>,
    pub not_fitted: Shared<bool>,
    /// The Library Part the Components were placed from, by UUID with its name; `None`
    /// when the edit comes from the board.
    pub part: Option<Shared<(Uuid, String)>>,
}

/// One Component field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldRow {
    pub name: String,
    pub value: Shared<String>,
    /// Shown on the sheet.
    pub visible: Shared<bool>,
    /// On the Components' Locked Fields; always unlocked from the board.
    pub locked: Shared<bool>,
    /// The Designator can be set on one Component at a time only.
    pub editable: bool,
}

/// One Footprint a Part offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alternate {
    /// The Library Footprint's UUID.
    pub footprint: Uuid,
    pub name: String,
}

/// The Net Classes of the selection's Nets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetClassProperty {
    /// The PCB Nets, sorted by UUID.
    pub nets: Vec<Uuid>,
    /// The classes each Net is in besides the Default Net Class, sorted by UUID; empty
    /// for the Default Net Class only.
    pub class: Shared<Vec<Uuid>>,
    /// Every Net Class on the board, `(uuid, name)`, in name order.
    pub classes: Vec<(Uuid, String)>,
}

impl Properties {
    /// What the panel shows for `selection`. Objects no longer in `project` (after an
    /// undo, say) are skipped.
    pub fn of(project: &Project, selection: &Selection) -> Properties {
        let objects: Vec<Object> = selection
            .objects
            .iter()
            .filter_map(|uuid| project.object(*uuid))
            .collect();
        let mut kinds: Vec<(String, usize)> = Vec::new();
        let mut by_kind: Vec<Vec<Uuid>> = Vec::new();
        for object in &objects {
            let kind = kind_name(object);
            match kinds.iter().position(|(k, _)| *k == kind) {
                Some(i) => {
                    kinds[i].1 += 1;
                    by_kind[i].push(object.uuid());
                }
                None => {
                    kinds.push((kind, 1));
                    by_kind.push(vec![object.uuid()]);
                }
            }
        }
        if !selection.segments.is_empty() {
            kinds.push(("wire_segment".to_owned(), selection.segments.len()));
            by_kind.push(Vec::new());
        }
        let details = match objects.as_slice() {
            [one] => details(one),
            _ => Vec::new(),
        };
        let mut wires: Vec<Uuid> = objects.iter().map(Object::uuid).collect();
        wires.extend(selection.segments.iter().map(|s| s.wire));
        wires.sort();
        wires.dedup();
        let vertices = match (
            wires.as_slice(),
            wires.first().and_then(|w| project.object(*w)),
        ) {
            ([wire], Some(Object::Wire(w))) => Some(WireVertices {
                wire: *wire,
                vertices: w.object.vertices.clone(),
            }),
            _ => None,
        };
        Properties {
            count: objects.len()
                + selection.segments.len()
                + selection.pads.len()
                + selection.nets.len(),
            kinds,
            component: component_properties(project, &objects)
                .or_else(|| footprint_properties(project, &objects)),
            net_class: net_class(project, &objects, selection),
            numbers: Numbers::of(&objects),
            details,
            rules: Rules::of(project, selection),
            local_edits: local_edits(project, &objects, selection),
            by_kind,
            vertices,
        }
    }

    /// The `selection.set` that narrows the selection to the objects of kind `kind` (a
    /// kind chip's click); `None` for a kind not selected.
    pub fn narrow(&self, kind: &str) -> Option<(&'static str, Value)> {
        let i = self.kinds.iter().position(|(k, _)| k == kind)?;
        let objects = self.by_kind.get(i).filter(|o| !o.is_empty())?;
        Some(("selection.set", json!({ "objects": objects })))
    }

    /// The Command that returns the locally edited Footprints to the copies they were
    /// edited from (Revert).
    pub fn revert(&self) -> Option<(&'static str, Value)> {
        let edits = self.local_edits.as_ref()?;
        Some((
            "pcb.footprint.revert_local_edits",
            json!({"instances": edits.instances}),
        ))
    }

    /// The Command that makes `edit` to everything selected, as `(id, params)`; `None`
    /// when the selection has nothing it applies to.
    pub fn command(&self, edit: &Edit) -> Option<(&'static str, Value)> {
        if matches!(
            edit,
            Edit::Vertex(..) | Edit::AddVertex(_) | Edit::RemoveVertex(_)
        ) {
            let list = self.vertices.as_ref()?;
            let vertices: Vec<Value> = (list.edited(edit)?.iter())
                .map(|p| json!({"x": p.x.0, "y": p.y.0}))
                .collect();
            return Some((
                "schematic.wire.set_vertices",
                json!({"wire": list.wire, "vertices": vertices}),
            ));
        }
        if let Edit::NetClass(class) = edit {
            let property = self.net_class.as_ref()?;
            let all_in = property.class.same().is_some_and(|c| c.contains(class));
            let id = if all_in {
                "pcb.net.remove_from_class"
            } else {
                "pcb.net.add_to_class"
            };
            return Some((id, json!({"nets": property.nets, "class": class})));
        }
        let component = self.component.as_ref()?;
        if !component.footprints.is_empty() {
            return footprint_command(component, edit);
        }
        let mut params = json!({"components": component.components});
        let set = params.as_object_mut().expect("an object");
        match edit {
            Edit::Field(name, value) if name == UnitInstance::DESIGNATOR => {
                if component.components.len() != 1 {
                    return None;
                }
                set.insert("designator".into(), json!(value));
            }
            Edit::Field(name, value) if name == "value" => {
                set.insert("value".into(), json!(value));
            }
            Edit::Field(name, value) => {
                set.insert("fields".into(), json!({ name: value }));
            }
            Edit::Visible(name, shown) => {
                set.insert("visible".into(), json!({ name: shown }));
            }
            Edit::Locked(name, locked) => {
                set.insert("locked".into(), json!({ name: locked }));
            }
            Edit::Footprint(footprint) => {
                if !component
                    .alternates
                    .iter()
                    .any(|a| a.footprint == *footprint)
                {
                    return None;
                }
                set.insert("footprint".into(), json!(footprint));
            }
            Edit::InBom(in_bom) => {
                set.insert("in_bom".into(), json!(in_bom));
            }
            Edit::InCpl(in_cpl) => {
                set.insert("in_cpl".into(), json!(in_cpl));
            }
            Edit::NotFitted(not_fitted) => {
                set.insert("not_fitted".into(), json!(not_fitted));
            }
            Edit::NetClass(_) | Edit::Vertex(..) | Edit::AddVertex(_) | Edit::RemoveVertex(_) => {
                unreachable!("handled above")
            }
        }
        Some(("schematic.component.set", params))
    }
}

/// An object kind's name, as Commands and files write it (`unit`, `track`…).
fn kind_name(object: &Object) -> String {
    match serde_json::to_value(object.object_type()) {
        Ok(Value::String(name)) => name,
        _ => format!("{:?}", object.object_type()),
    }
}

/// An object's stored fields, scalars as text and anything larger as compact JSON.
fn details(object: &Object) -> Vec<(String, String)> {
    let Ok(Value::Object(fields)) = serde_json::to_value(object) else {
        return Vec::new();
    };
    fields
        .into_iter()
        .filter(|(name, _)| name != "type")
        .map(|(name, value)| {
            let text = match value {
                Value::String(s) => s,
                other => other.to_string(),
            };
            (name, text)
        })
        .collect()
}

fn component_properties(project: &Project, objects: &[Object]) -> Option<ComponentProperties> {
    // One Unit per Component: every Unit carries the same copy of its Component's data.
    let mut units: Vec<(Uuid, &UnitInstance)> = Vec::new();
    for object in objects {
        if let Object::Unit(unit) = object
            && !units
                .iter()
                .any(|(_, u)| u.component.uuid == unit.component.uuid)
        {
            units.push((unit.document, &unit.object));
        }
    }
    if units.is_empty() {
        return None;
    }
    let mut names: Vec<String> = Fields::BUILT_IN.iter().map(|n| (*n).to_owned()).collect();
    names.insert(0, UnitInstance::DESIGNATOR.to_owned());
    for (_, unit) in &units {
        for field in &unit.fields.custom {
            if !names.contains(&field.name) {
                names.push(field.name.clone());
            }
        }
    }
    let single = units.len() == 1;
    let fields = names
        .into_iter()
        .map(|name| {
            let value = Shared::of(units.iter().map(|(_, u)| field_value(u, &name)))
                .expect("units is not empty");
            let visible = Shared::of(units.iter().map(|(_, u)| u.visible_fields.contains(&name)))
                .expect("units is not empty");
            let locked = Shared::of(units.iter().map(|(_, u)| u.locked_fields.contains(&name)))
                .expect("units is not empty");
            FieldRow {
                editable: single || name != UnitInstance::DESIGNATOR,
                name,
                value,
                visible,
                locked,
            }
        })
        .collect();
    Some(ComponentProperties {
        components: units.iter().map(|(_, u)| u.component.uuid).collect(),
        footprints: Vec::new(),
        fields,
        footprint: Shared::of(units.iter().map(|(_, u)| u.footprint)).expect("not empty"),
        alternates: alternates(project, &units),
        in_bom: Shared::of(units.iter().map(|(_, u)| u.in_bom)).expect("not empty"),
        in_cpl: Shared::of(units.iter().map(|(_, u)| u.in_cpl)).expect("not empty"),
        not_fitted: Shared::of(units.iter().map(|(_, u)| u.not_fitted)).expect("not empty"),
        part: Shared::of(units.iter().filter_map(|(sheet, u)| {
            let entry = project.schematic_sheet(*sheet)?.part(u.part)?;
            Some((entry.part.uuid, entry.part.name.clone()))
        })),
    })
}

/// The board side of [`Properties::command`]: one `pcb.footprint.set` on the selected
/// Footprints.
fn footprint_command(
    component: &ComponentProperties,
    edit: &Edit,
) -> Option<(&'static str, Value)> {
    let mut params = json!({"footprints": component.footprints});
    let set = params.as_object_mut().expect("an object");
    match edit {
        Edit::Field(name, value) if name == UnitInstance::DESIGNATOR => {
            if component.footprints.len() != 1 {
                return None;
            }
            set.insert("designator".into(), json!(value));
        }
        Edit::Field(name, value) if name == "value" => {
            set.insert("value".into(), json!(value));
        }
        Edit::Field(name, value) => {
            set.insert("fields".into(), json!({ name: value }));
        }
        Edit::Footprint(footprint) => {
            if !component
                .alternates
                .iter()
                .any(|a| a.footprint == *footprint)
            {
                return None;
            }
            set.insert("footprint".into(), json!(footprint));
        }
        Edit::InBom(in_bom) => {
            set.insert("in_bom".into(), json!(in_bom));
        }
        Edit::InCpl(in_cpl) => {
            set.insert("in_cpl".into(), json!(in_cpl));
        }
        Edit::NotFitted(not_fitted) => {
            set.insert("not_fitted".into(), json!(not_fitted));
        }
        Edit::Visible(..)
        | Edit::Locked(..)
        | Edit::NetClass(_)
        | Edit::Vertex(..)
        | Edit::AddVertex(_)
        | Edit::RemoveVertex(_) => return None,
    }
    Some(("pcb.footprint.set", params))
}

/// The selected Footprints' Component data, as the board holds it.
fn footprint_properties(project: &Project, objects: &[Object]) -> Option<ComponentProperties> {
    let pcb = project.pcb()?;
    let mut footprints: Vec<&FootprintInstance> = Vec::new();
    for object in objects {
        if let Object::FootprintInstance(f) = object
            && let Some(f) = pcb.footprint(f.object.uuid)
            && !footprints.iter().any(|o| o.uuid == f.uuid)
        {
            footprints.push(f);
        }
    }
    if footprints.is_empty() {
        return None;
    }
    let mut names: Vec<String> = Fields::BUILT_IN.iter().map(|n| (*n).to_owned()).collect();
    names.insert(0, UnitInstance::DESIGNATOR.to_owned());
    for f in &footprints {
        for field in &f.fields.custom {
            if !names.contains(&field.name) {
                names.push(field.name.clone());
            }
        }
    }
    let single = footprints.len() == 1;
    let fields = names
        .into_iter()
        .map(|name| {
            let value = Shared::of(footprints.iter().map(|f| {
                if name == UnitInstance::DESIGNATOR {
                    f.designator.clone()
                } else {
                    f.fields.get(&name).to_owned()
                }
            }))
            .expect("footprints is not empty");
            FieldRow {
                editable: single || name != UnitInstance::DESIGNATOR,
                name,
                value,
                visible: Shared::Same(false),
                locked: Shared::Same(false),
            }
        })
        .collect();
    // A synced Footprint's choice is its Component's, on the sheets.
    let units: Vec<Option<(Uuid, &UnitInstance)>> = footprints
        .iter()
        .map(|f| {
            f.component
                .as_ref()
                .and_then(|c| first_unit(project, c.uuid))
        })
        .collect();
    let chosen = footprints.iter().zip(&units).map(|(f, unit)| match unit {
        Some((_, u)) => u.footprint,
        None => pcb.footprint_copy(f.footprint).map(|c| c.footprint.uuid),
    });
    let synced: Option<Vec<(Uuid, &UnitInstance)>> = units.iter().copied().collect();
    Some(ComponentProperties {
        components: footprints
            .iter()
            .filter_map(|f| f.component.as_ref().map(|c| c.uuid))
            .collect(),
        footprints: footprints.iter().map(|f| f.uuid).collect(),
        fields,
        footprint: Shared::of(chosen).expect("not empty"),
        alternates: synced.map_or_else(Vec::new, |units| alternates(project, &units)),
        in_bom: Shared::of(footprints.iter().map(|f| f.in_bom)).expect("not empty"),
        in_cpl: Shared::of(footprints.iter().map(|f| f.in_cpl)).expect("not empty"),
        not_fitted: Shared::of(footprints.iter().map(|f| f.not_fitted)).expect("not empty"),
        part: None,
    })
}

/// A Unit of Component `component`, with its sheet.
fn first_unit(project: &Project, component: Uuid) -> Option<(Uuid, &UnitInstance)> {
    project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .find_map(|sheet| {
            sheet
                .units()
                .find(|u| u.component.uuid == component)
                .map(|u| (sheet.uuid, u))
        })
}

fn field_value(unit: &UnitInstance, name: &str) -> String {
    if name == UnitInstance::DESIGNATOR {
        unit.designator.clone()
    } else {
        unit.fields.get(name).to_owned()
    }
}

/// The Footprints the Components' Part offers, when they are all the same Part.
fn alternates(project: &Project, units: &[(Uuid, &UnitInstance)]) -> Vec<Alternate> {
    let entries: Vec<_> = units
        .iter()
        .filter_map(|(sheet, unit)| {
            let sheet = project.schematic_sheet(*sheet)?;
            Some((sheet, sheet.part(unit.part)?))
        })
        .collect();
    let Some((sheet, first)) = entries.first() else {
        return Vec::new();
    };
    if entries.len() != units.len()
        || entries.iter().any(|(_, e)| {
            e.part.uuid != first.part.uuid || e.part.footprints != first.part.footprints
        })
    {
        return Vec::new();
    }
    first
        .part
        .footprints
        .iter()
        .zip(&first.footprints)
        .map(|(footprint, copy)| Alternate {
            footprint: *footprint,
            name: sheet
                .footprint(*copy)
                .map(|c| c.footprint.name.clone())
                .unwrap_or_else(|| footprint.to_string()),
        })
        .collect()
}

fn net_class(
    project: &Project,
    objects: &[Object],
    selection: &Selection,
) -> Option<NetClassProperty> {
    let pcb = project.pcb()?;
    let mut nets: BTreeSet<Uuid> = selection.nets.iter().copied().collect();
    for object in objects {
        let net = match object {
            Object::Track(t) => t.net,
            Object::Via(v) => v.net,
            Object::Zone(z) => z.net,
            Object::FreePad(p) => p.net,
            _ => None,
        };
        nets.extend(net);
    }
    for pad in &selection.pads {
        nets.extend(
            pcb.footprint(pad.footprint)
                .and_then(|f| f.pad_net(pad.pad)),
        );
    }
    let nets: Vec<Uuid> = nets.into_iter().filter(|n| pcb.net(*n).is_some()).collect();
    let class = Shared::of(nets.iter().filter_map(|n| pcb.net(*n)).map(|n| {
        let mut classes = n.classes.clone();
        classes.sort();
        classes
    }))?;
    let mut classes: Vec<(Uuid, String)> = pcb
        .net_classes()
        .map(|c| (c.uuid, c.name.clone()))
        .collect();
    classes.sort_by(|a, b| a.1.cmp(&b.1));
    Some(NetClassProperty {
        nets,
        class,
        classes,
    })
}

/// The local edits of the Footprints of the selected Pads and Footprints.
fn local_edits(project: &Project, objects: &[Object], selection: &Selection) -> Option<LocalEdits> {
    let pcb = project.pcb()?;
    let mut instances: Vec<Uuid> = Vec::new();
    let footprints = objects
        .iter()
        .filter_map(|o| match o {
            Object::FootprintInstance(f) => Some(f.object.uuid),
            _ => None,
        })
        .chain(selection.pads.iter().map(|p| p.footprint));
    for uuid in footprints {
        let edited = pcb
            .footprint(uuid)
            .and_then(|f| pcb.footprint_copy(f.footprint))
            .is_some_and(|c| c.is_locally_edited());
        if edited && !instances.contains(&uuid) {
            instances.push(uuid);
        }
    }
    if instances.is_empty() {
        return None;
    }
    let compare = match instances.as_slice() {
        [one] => pcb.footprint(*one).and_then(|f| {
            let copy = pcb.footprint_copy(f.footprint)?;
            let original = pcb.footprint_copy(copy.edited_from.as_ref()?.uuid)?;
            Some((copy.footprint.clone(), original.footprint.clone()))
        }),
        _ => None,
    };
    Some(LocalEdits {
        edits: compare
            .as_ref()
            .map_or_else(Vec::new, |(edited, original)| {
                reda_engine::model::local_edit::local_edits(original, edited)
            }),
        compare,
        instances,
    })
}
