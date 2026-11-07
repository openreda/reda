//! Field cells: one per field, Component Type, In BOM and In CPL, each with the old default
//! (the part-table copy), the new default (the Library) and the Component's value. The
//! three-way compare only sets how a cell starts; any cell but a locked one can be
//! overridden.

use reda_model::library_update::{
    ComponentFieldMissing, LibraryFieldMissing, LibraryUpdateSettings, LocallyEdited,
};
use reda_model::{ComponentType, Fields, Part, UnitInstance};
use schemars::JsonSchema;
use serde::Serialize;

/// The cell names that are not fields.
pub const COMPONENT_TYPE: &str = "component_type";
pub const IN_BOM: &str = "in_bom";
pub const IN_CPL: &str = "in_cpl";
pub const NOT_FITTED: &str = "not_fitted";

/// How the three values of a cell compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CellCase {
    /// The default changed and the Component holds the old one.
    Changed,
    /// The default changed and the Component already holds the new one.
    Same,
    /// The default changed (or is gone) and the Component holds a value of its own.
    Edited,
    /// The field is new in the Library and the Component has no value.
    Added,
    /// The field is gone from the Library and the Component holds the old default.
    Removed,
    /// Only the Component has the field.
    ComponentOnly,
    /// The field is one of the Component's Locked Fields: no change, and no override
    /// until it is unlocked.
    Locked,
    /// Not Fitted, cleared because the Component becomes a Net Tie, which refuses it.
    Cleared,
}

/// What a cell does to the Component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CellAction {
    /// The Component takes the new default (an empty one removes the field).
    Update,
    /// The Component keeps its value.
    Reject,
    /// Nothing to do: the value is the new default already, or the cell is locked.
    None,
}

/// One cell of the field grid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Cell {
    /// A field's name, or `component_type`, `in_bom`, `in_cpl` or `not_fitted`.
    pub cell: String,
    pub case: CellCase,
    /// The old default, from the part-table copy (`""` = not set).
    pub old: String,
    /// The new default, from the Library (`""` = not set).
    pub new: String,
    /// The Component's value.
    pub component: String,
    /// How the cell starts.
    pub action: CellAction,
}

impl Cell {
    /// Whether the action can be overridden.
    pub fn overridable(&self) -> bool {
        !matches!(
            self.case,
            CellCase::Locked | CellCase::Cleared | CellCase::Same
        )
    }
}

/// The cells of a Component whose Units carry `unit`'s data, updating from `old` to `new`.
/// Not Fitted's cell is added by [`not_fitted_cell`] once the actions are final.
pub fn cells(
    old: &Part,
    new: &Part,
    unit: &UnitInstance,
    settings: &LibraryUpdateSettings,
) -> Vec<Cell> {
    let mut out = vec![];
    for name in field_names(&old.fields, &new.fields, &unit.fields) {
        let values = (
            old.fields.get(&name),
            new.fields.get(&name),
            unit.fields.get(&name),
        );
        if let Some(cell) = cell(&name, values, true, unit, settings) {
            out.push(cell);
        }
    }
    let type_id = |t: ComponentType| type_id(t).to_owned();
    let flags = [
        (
            COMPONENT_TYPE,
            type_id(old.component_type),
            type_id(new.component_type),
            type_id(unit.component_type),
        ),
        (
            IN_BOM,
            old.in_bom.to_string(),
            new.in_bom.to_string(),
            unit.in_bom.to_string(),
        ),
        (
            IN_CPL,
            old.in_cpl.to_string(),
            new.in_cpl.to_string(),
            unit.in_cpl.to_string(),
        ),
    ];
    for (name, o, n, c) in &flags {
        if let Some(cell) = cell(name, (o, n, c), false, unit, settings) {
            out.push(cell);
        }
    }
    out
}

/// A Component Type as cells write it, as stored.
fn type_id(t: ComponentType) -> &'static str {
    match t {
        ComponentType::Standard => "standard",
        ComponentType::Mechanical => "mechanical",
        ComponentType::NetTie => "net_tie",
    }
}

/// The cell for one name, or `None` when nothing differs.
fn cell(
    name: &str,
    (old, new, component): (&str, &str, &str),
    field: bool,
    unit: &UnitInstance,
    settings: &LibraryUpdateSettings,
) -> Option<Cell> {
    let listed = settings.updates(name);
    let start = |update: bool| {
        if update && listed {
            CellAction::Update
        } else {
            CellAction::Reject
        }
    };
    let (case, action) = if old == new {
        if !(field && old.is_empty() && !component.is_empty()) {
            return None;
        }
        let remove = settings.component_field_missing == ComponentFieldMissing::Remove;
        (CellCase::ComponentOnly, start(remove))
    } else if component == new {
        (CellCase::Same, CellAction::None)
    } else if field && old.is_empty() && component.is_empty() {
        let add = match settings.library_field_missing {
            LibraryFieldMissing::Add => true,
            LibraryFieldMissing::AddIfNotBlank => !new.trim().is_empty(),
            LibraryFieldMissing::Skip => false,
        };
        (CellCase::Added, start(add))
    } else if component == old {
        if new.is_empty() {
            (CellCase::Removed, start(true))
        } else {
            (CellCase::Changed, start(true))
        }
    } else if field && new.is_empty() {
        let remove = settings.component_field_missing == ComponentFieldMissing::Remove;
        (CellCase::Edited, start(remove))
    } else {
        let take = settings.locally_edited == LocallyEdited::TakeNew;
        (CellCase::Edited, start(take))
    };
    let (case, action) = if unit.locked_fields.contains(name) && case != CellCase::Same {
        (CellCase::Locked, CellAction::None)
    } else {
        (case, action)
    };
    Some(Cell {
        cell: name.to_owned(),
        case,
        old: old.to_owned(),
        new: new.to_owned(),
        component: component.to_owned(),
        action,
    })
}

/// The cell clearing Not Fitted, when the Component ends up a Net Tie that is Not Fitted.
pub fn not_fitted_cell(unit: &UnitInstance, component_type: ComponentType) -> Option<Cell> {
    (unit.not_fitted && !component_type.allows_not_fitted()).then(|| Cell {
        cell: NOT_FITTED.to_owned(),
        case: CellCase::Cleared,
        old: String::new(),
        new: "false".to_owned(),
        component: "true".to_owned(),
        action: CellAction::Update,
    })
}

/// The built-in field names, then the free-form ones: the old Part's in its order, then
/// the new Part's new ones, then the Component's own.
fn field_names(old: &Fields, new: &Fields, component: &Fields) -> Vec<String> {
    let mut names: Vec<String> = Fields::BUILT_IN.iter().map(|&n| n.to_owned()).collect();
    for field in old
        .custom
        .iter()
        .chain(&new.custom)
        .chain(&component.custom)
    {
        if !names.contains(&field.name) {
            names.push(field.name.clone());
        }
    }
    names
}

/// The Unit with the cells whose action is Update applied: fields set (an empty value
/// removes one, and a removed field stops showing), Component Type, In BOM, In CPL, and
/// Not Fitted cleared on a Net Tie.
pub fn apply(unit: &mut UnitInstance, cells: &[Cell]) {
    for cell in cells.iter().filter(|c| c.action == CellAction::Update) {
        match cell.cell.as_str() {
            COMPONENT_TYPE => {
                if let Some(t) = ComponentType::ALL
                    .into_iter()
                    .find(|t| type_id(*t) == cell.new)
                {
                    unit.component_type = t;
                }
            }
            IN_BOM => unit.in_bom = cell.new == "true",
            IN_CPL => unit.in_cpl = cell.new == "true",
            NOT_FITTED => unit.not_fitted = false,
            name => {
                unit.fields.set(name, &cell.new);
                if cell.new.is_empty() {
                    unit.visible_fields.remove(name);
                    unit.remove_field_text(name);
                }
            }
        }
    }
}
