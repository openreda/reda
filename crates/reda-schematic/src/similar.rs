//! Select Similar: `schematic.select_similar` lists the Units that match a reference Unit
//! on the chosen fields, on its sheet or across the Project.

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::{PartEntry, Project, SchematicSheet, UnitInstance, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::natural::natural_cmp;

/// A field two Units are compared on.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SimilarField {
    /// The Library Part the Component was placed from.
    Part,
    Value,
    /// The chosen Footprint.
    Footprint,
    /// The Symbol's Designator prefix, such as `R`.
    DesignatorPrefix,
    /// The name of the Symbol Unit shown, such as `A`.
    Unit,
    ComponentType,
    NotFitted,
    /// Any field by name: a built-in one (`mpn`…) or a free-form one (`LCSC`…). Units
    /// that both lack it match.
    Field(String),
}

/// Where to look for similar Units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SimilarScope {
    /// The reference Unit's sheet.
    Sheet,
    /// Every Schematic Sheet.
    Project,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectSimilarParams {
    /// The reference Unit, by UUID.
    pub unit: Uuid,
    /// The fields that must match; at least one.
    #[serde(rename = "match")]
    pub on: Vec<SimilarField>,
    pub scope: SimilarScope,
}

/// The matching Units.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SelectSimilarResult {
    /// The reference Unit included, sheet by sheet in Document order, then in natural
    /// order of Designator.
    pub units: Vec<Uuid>,
}

pub struct SelectSimilar;

impl Command for SelectSimilar {
    const META: Meta = Meta {
        id: "schematic.select_similar",
        title: "Select Similar",
        description: "Lists the Units that match a reference Unit on every chosen field \
                      (Part, value, Footprint, Designator prefix, Symbol Unit, Component \
                      type, Not Fitted or any field by name), on its sheet or across the \
                      Project.",
        category: "schematic",
        icon: "select_similar",
        kind: Kind::Query,
    };
    type Params = SelectSimilarParams;
    type Output = SelectSimilarResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SelectSimilarParams,
    ) -> Result<SelectSimilarResult, CommandError> {
        if p.on.is_empty() {
            return Err(CommandError::invalid_params(
                "/match",
                "choose at least one field to match on",
            ));
        }
        Ok(SelectSimilarResult {
            units: select_similar(cx.project()?, p.unit, &p.on, p.scope)?,
        })
    }
}

/// The Units similar to `unit` on every field of `on`, in [`SelectSimilarResult::units`]
/// order.
pub fn select_similar(
    project: &Project,
    unit: Uuid,
    on: &[SimilarField],
    scope: SimilarScope,
) -> Result<Vec<Uuid>, CommandError> {
    let sheets: Vec<&SchematicSheet> = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .collect();
    let Some((home, reference)) = sheets
        .iter()
        .find_map(|s| s.unit(unit).map(|u| (*s, Placed::new(s, u))))
    else {
        return Err(CommandError::not_found(format!("no Unit has UUID {unit}")));
    };
    let mut out = Vec::new();
    for sheet in sheets {
        if scope == SimilarScope::Sheet && sheet.uuid != home.uuid {
            continue;
        }
        let mut found: Vec<&UnitInstance> = sheet
            .units()
            .filter(|u| {
                let other = Placed::new(sheet, u);
                on.iter().all(|f| reference.key(f) == other.key(f))
            })
            .collect();
        found.sort_by(|a, b| natural_cmp(&a.designator, &b.designator).then(a.uuid.cmp(&b.uuid)));
        out.extend(found.into_iter().map(|u| u.uuid));
    }
    Ok(out)
}

/// A Unit with its part-table entry.
struct Placed<'a> {
    unit: &'a UnitInstance,
    entry: &'a PartEntry,
}

impl<'a> Placed<'a> {
    fn new(sheet: &'a SchematicSheet, unit: &'a UnitInstance) -> Placed<'a> {
        let entry = sheet
            .part(unit.part)
            .expect("a Unit's part entry is on its sheet");
        Placed { unit, entry }
    }

    /// What two Units must share to match on `field`.
    fn key(&self, field: &SimilarField) -> String {
        let unit = self.unit;
        match field {
            SimilarField::Part => self.entry.part.uuid.to_string(),
            SimilarField::Value => unit.fields.value.clone(),
            SimilarField::Footprint => unit.footprint.map(|f| f.to_string()).unwrap_or_default(),
            SimilarField::DesignatorPrefix => self.entry.symbol.designator_prefix.clone(),
            SimilarField::Unit => self
                .entry
                .symbol
                .units
                .iter()
                .find(|u| u.uuid == unit.unit)
                .map(|u| u.name.clone())
                .unwrap_or_default(),
            SimilarField::ComponentType => format!("{:?}", unit.component_type),
            SimilarField::NotFitted => unit.not_fitted.to_string(),
            SimilarField::Field(name) => unit.fields.get(name).to_owned(),
        }
    }
}
