//! Editing Component data: `schematic.component.set`. Every Unit of a Component carries
//! the same copy of its data, so an edit rewrites each placed Unit, on every sheet.

use std::collections::BTreeMap;

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::{InDocument, Object, Project, UnitInstance, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentSetParams {
    /// The Components to edit, by UUID. The edit applies to each of them.
    pub components: Vec<Uuid>,
    /// The new Designator. Only with a single Component, and not one another Component
    /// uses.
    pub designator: Option<String>,
    /// Whether the Designator is locked against Re-annotate.
    pub designator_locked: Option<bool>,
    /// The new value.
    pub value: Option<String>,
    /// Fields to set, by name: the built-in `description`, `datasheet`, `manufacturer`,
    /// `mpn` and `keywords`, or any free-form name such as `LCSC`. An empty value or
    /// `null` clears a built-in field and removes a free-form one. Set the value with
    /// `value`.
    #[serde(default)]
    pub fields: BTreeMap<String, Option<String>>,
    /// The Footprint, by UUID: one of each Component's Part's alternates.
    pub footprint: Option<Uuid>,
    /// Whether the Components go into the BOM.
    #[serde(default)]
    pub in_bom: Option<bool>,
    /// Whether the Components go into the CPL.
    #[serde(default)]
    pub in_cpl: Option<bool>,
    /// Whether this build leaves the Components off the board and out of the BOM and
    /// CPL. A Net Tie refuses it.
    pub not_fitted: Option<bool>,
    /// Which fields to show (`true`) or hide (`false`) on the sheet, by name:
    /// `designator`, `value`, another built-in field or a free-form field's name.
    #[serde(default)]
    pub visible: BTreeMap<String, bool>,
    /// Which fields to lock (`true`) or unlock (`false`), by the same names as
    /// `visible`. Update from Library leaves a Locked Field as it is; it can still be
    /// edited by hand.
    #[serde(default)]
    pub locked: BTreeMap<String, bool>,
}

/// What an edit changed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ComponentSetResult {
    /// Every rewritten Unit, in the order of `components`, then Document order.
    pub units: Vec<Uuid>,
}

pub struct ComponentSet;

impl Command for ComponentSet {
    const META: Meta = Meta {
        id: "schematic.component.set",
        title: "Edit Component Properties",
        description: "Sets properties of one or more Components: each property given applies \
                      to every Component listed and to every placed Unit of it, on every \
                      sheet, so the Units keep one copy of the Component's data.",
        category: "schematic",
        icon: "properties",
        kind: Kind::Edit,
    };
    type Params = ComponentSetParams;
    type Output = ComponentSetResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ComponentSetParams,
    ) -> Result<ComponentSetResult, CommandError> {
        if let Some(designator) = &p.designator {
            check_designator(cx.project()?, &p.components, designator)?;
        }
        check_field_names("/fields", p.fields.keys(), true)?;
        check_field_names("/visible", p.visible.keys(), false)?;
        check_field_names("/locked", p.locked.keys(), false)?;
        let mut edited = Vec::new();
        for component in &p.components {
            let units = placed_component(cx.project()?, *component)?;
            for (sheet, mut unit) in units {
                if let Some(designator) = &p.designator {
                    unit.designator = designator.clone();
                }
                if let Some(locked) = p.designator_locked {
                    unit.designator_locked = locked;
                }
                if let Some(value) = &p.value {
                    unit.fields.value = value.clone();
                }
                for (name, value) in &p.fields {
                    unit.fields.set(name, value.as_deref().unwrap_or(""));
                }
                if let Some(footprint) = p.footprint {
                    check_alternate(cx.project()?, sheet, &unit, footprint)?;
                    unit.footprint = Some(footprint);
                }
                if let Some(in_bom) = p.in_bom {
                    unit.in_bom = in_bom;
                }
                if let Some(in_cpl) = p.in_cpl {
                    unit.in_cpl = in_cpl;
                }
                if let Some(not_fitted) = p.not_fitted {
                    if not_fitted && !unit.component_type.allows_not_fitted() {
                        return Err(CommandError::rejected(format!(
                            "{} is a Net Tie: its copper is on the board in every build, so \
                             it cannot be Not Fitted",
                            unit.designator
                        )));
                    }
                    unit.not_fitted = not_fitted;
                }
                for (name, &shown) in &p.visible {
                    if shown {
                        unit.visible_fields.insert(name.clone());
                    } else {
                        unit.visible_fields.remove(name);
                    }
                }
                for (name, &locked) in &p.locked {
                    if locked {
                        unit.locked_fields.insert(name.clone());
                    } else {
                        unit.locked_fields.remove(name);
                    }
                }
                edited.push(unit.uuid);
                cx.tx()?
                    .replace(Object::Unit(InDocument::new(sheet, unit)))?;
            }
        }
        Ok(ComponentSetResult { units: edited })
    }
}

/// Refuses a Designator for several Components, a blank one, or one another Component
/// already uses.
pub(crate) fn check_designator(
    project: &Project,
    components: &[Uuid],
    designator: &str,
) -> Result<(), CommandError> {
    let [component] = components else {
        return Err(CommandError::invalid_params(
            "/designator",
            "a Designator can be set on one Component at a time",
        ));
    };
    if designator.trim().is_empty() || designator.trim() != designator {
        return Err(CommandError::invalid_params(
            "/designator",
            format!("{designator:?} is not a Designator: it is blank or has spaces at its ends"),
        ));
    }
    let holder = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.units())
        .find(|u| u.designator == designator && u.component.uuid != *component);
    if let Some(holder) = holder {
        return Err(CommandError::conflict(format!(
            "Designator {designator} is already used by Component {}",
            holder.component.uuid
        )));
    }
    Ok(())
}

/// Refuses a Footprint that is not one of the alternates of the Unit's Part.
fn check_alternate(
    project: &Project,
    sheet: Uuid,
    unit: &UnitInstance,
    footprint: Uuid,
) -> Result<(), CommandError> {
    let entry = crate::sheet(project, sheet)?
        .part(unit.part)
        .expect("a Unit's part entry is on its sheet");
    if entry.part.footprints.contains(&footprint) {
        return Ok(());
    }
    Err(CommandError::invalid_params(
        "/footprint",
        format!(
            "Footprint {footprint} is not one of the alternates of {}'s Part {}",
            unit.designator, entry.part.name
        ),
    ))
}

/// Refuses blank field names, and `value` when `value_has_own_param`.
pub(crate) fn check_field_names<'a>(
    param: &str,
    names: impl Iterator<Item = &'a String>,
    value_has_own_param: bool,
) -> Result<(), CommandError> {
    for name in names {
        let pointer = format!("{param}/{}", name.replace('~', "~0").replace('/', "~1"));
        if value_has_own_param && name == "value" {
            return Err(CommandError::invalid_params(
                &pointer,
                "set the value with the `value` parameter",
            ));
        }
        if name.trim().is_empty() || name.trim() != name {
            return Err(CommandError::invalid_params(
                &pointer,
                format!("{name:?} is not a field name: it is blank or has spaces at its ends"),
            ));
        }
    }
    Ok(())
}

/// Copies of every placed Unit of `component` with their sheets, or `not_found` when it
/// has none.
fn placed_component(
    project: &Project,
    component: Uuid,
) -> Result<Vec<(Uuid, UnitInstance)>, CommandError> {
    let units: Vec<(Uuid, UnitInstance)> = units_of(project, component)
        .into_iter()
        .map(|(s, u)| (s, u.clone()))
        .collect();
    if units.is_empty() {
        return Err(CommandError::not_found(format!(
            "no Component has UUID {component}"
        )));
    }
    Ok(units)
}

/// Every placed Unit of `component` with the sheet it is on, in Document order.
pub(crate) fn units_of(project: &Project, component: Uuid) -> Vec<(Uuid, &UnitInstance)> {
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.units().map(move |u| (s.uuid, u)))
        .filter(|(_, u)| u.component.uuid == component && u.component.path.is_empty())
        .collect()
}
