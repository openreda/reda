//! What the Properties panel shows for the selection, with no drawing: the values the
//! selected objects share (`<mixed>` where they differ), and the one Command each edit
//! runs, which applies to everything selected.
//!
//! - Selected Units give their Components' properties: Designator, value and the other
//!   fields, which fields show on the sheet, the Footprint (from the Part's alternates)
//!   and Not Fitted. They are edited with `schematic.component.set`.
//! - Selected Tracks, Vias, Zones, Pads and highlighted Nets give their Nets' Net Class,
//!   set with `pcb.net.set_class`.
//! - Any one selected object shows its stored properties, read-only.

use std::collections::BTreeSet;
use std::fmt;

use reda_engine::model::{Fields, Object, Project, UnitInstance, Uuid};
use serde_json::{Value, json};

use crate::selection::Selection;

/// A value the selected objects share, or `Mixed` when they differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shared<T> {
    Same(T),
    Mixed,
}

impl<T: PartialEq> Shared<T> {
    /// What `values` share; `None` when there are none.
    fn of(values: impl IntoIterator<Item = T>) -> Option<Shared<T>> {
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
    /// Chooses the Footprint among the Part's alternates.
    Footprint(Uuid),
    NotFitted(bool),
    /// Moves the selection's Nets into a Net Class.
    NetClass(Uuid),
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
    /// With one object selected, its stored properties as `(name, value)`, read-only.
    pub details: Vec<(String, String)>,
}

/// The properties the selected Components share.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentProperties {
    /// The Components, in the order their Units were picked.
    pub components: Vec<Uuid>,
    /// `designator`, `value`, the other built-in fields, then free-form fields.
    pub fields: Vec<FieldRow>,
    /// The chosen Footprint, by Library element UUID.
    pub footprint: Shared<Option<Uuid>>,
    /// The alternates to choose from; empty unless every Component is the same Part.
    pub alternates: Vec<Alternate>,
    pub not_fitted: Shared<bool>,
}

/// One Component field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldRow {
    pub name: String,
    pub value: Shared<String>,
    /// Shown on the sheet.
    pub visible: Shared<bool>,
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

/// The Net Class of the selection's Nets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetClassProperty {
    /// The PCB Nets, sorted by UUID.
    pub nets: Vec<Uuid>,
    pub class: Shared<Uuid>,
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
        for object in &objects {
            let kind = kind_name(object);
            match kinds.iter_mut().find(|(k, _)| *k == kind) {
                Some((_, n)) => *n += 1,
                None => kinds.push((kind, 1)),
            }
        }
        let details = match objects.as_slice() {
            [one] => details(one),
            _ => Vec::new(),
        };
        Properties {
            count: objects.len() + selection.pads.len() + selection.nets.len(),
            kinds,
            component: component_properties(project, &objects),
            net_class: net_class(project, &objects, selection),
            details,
        }
    }

    /// The Command that makes `edit` to everything selected, as `(id, params)`; `None`
    /// when the selection has nothing it applies to.
    pub fn command(&self, edit: &Edit) -> Option<(&'static str, Value)> {
        if let Edit::NetClass(class) = edit {
            let nets = &self.net_class.as_ref()?.nets;
            return Some(("pcb.net.set_class", json!({"nets": nets, "class": class})));
        }
        let component = self.component.as_ref()?;
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
            Edit::NotFitted(not_fitted) => {
                set.insert("not_fitted".into(), json!(not_fitted));
            }
            Edit::NetClass(_) => unreachable!("handled above"),
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
            FieldRow {
                editable: single || name != UnitInstance::DESIGNATOR,
                name,
                value,
                visible,
            }
        })
        .collect();
    Some(ComponentProperties {
        components: units.iter().map(|(_, u)| u.component.uuid).collect(),
        fields,
        footprint: Shared::of(units.iter().map(|(_, u)| u.footprint)).expect("not empty"),
        alternates: alternates(project, &units),
        not_fitted: Shared::of(units.iter().map(|(_, u)| u.not_fitted)).expect("not empty"),
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
    let class = Shared::of(nets.iter().filter_map(|n| pcb.net(*n)).map(|n| n.class))?;
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
