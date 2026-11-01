//! Placing Components: `schematic.component.place` and `schematic.unit.place`.

use std::collections::BTreeSet;

use reda_command::{Angle, Command, CommandError, Context, Kind, Length, Meta, PartCopy};
use reda_model::geom::{Millideg, Point};
use reda_model::{
    ComponentRef, Footprint, FootprintCopy, InDocument, Object, Part, PartEntry, Project, Symbol,
    UnitInstance, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::sheet;

/// A point on the sheet.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct At {
    pub x: Length,
    pub y: Length,
}

impl At {
    pub(crate) fn point(self) -> Point {
        Point::new(self.x.nm(), self.y.nm())
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentPlaceParams {
    /// The Schematic Sheet to place on.
    pub document: Uuid,
    /// The Library Part to place, by UUID.
    pub part: Uuid,
    /// Where the Symbol origin goes.
    pub at: At,
    /// Counter-clockwise; 0 by default.
    pub rotation: Option<Angle>,
    /// Mirror left to right before rotating.
    #[serde(default)]
    pub mirror: bool,
    /// Which Unit to place, by name (such as `B`); the first by default.
    pub unit: Option<String>,
    /// The Footprint, by UUID: one of the Part's alternates. The first by default.
    pub footprint: Option<Uuid>,
    /// The new Unit's UUID; a fresh one by default.
    pub uuid: Option<Uuid>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitPlaceParams {
    /// The Schematic Sheet to place on.
    pub document: Uuid,
    /// The Component, by UUID.
    pub component: Uuid,
    /// Where the Symbol origin goes.
    pub at: At,
    /// Counter-clockwise; 0 by default.
    pub rotation: Option<Angle>,
    /// Mirror left to right before rotating.
    #[serde(default)]
    pub mirror: bool,
    /// Which Unit to place, by name; by default the first one not yet placed.
    pub unit: Option<String>,
    /// The new Unit's UUID; a fresh one by default.
    pub uuid: Option<Uuid>,
}

/// What placing a Unit made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PlacedResult {
    /// The placed Unit.
    pub unit: Uuid,
    pub component: ComponentRef,
    pub designator: String,
    /// The part-table entry on the sheet the Unit was placed from.
    pub part_entry: Uuid,
}

pub struct ComponentPlace;

impl Command for ComponentPlace {
    const META: Meta = Meta {
        id: "schematic.component.place",
        title: "Place Component",
        description: "Places a new Component from a Library Part on a Schematic Sheet: \
                      copies the Part with its Symbol and every alternate Footprint into the \
                      sheet's part table (reusing an identical copy), and places one Unit \
                      with the next free Designator for the Symbol's prefix and the Part's \
                      fields. Place the other Units with `schematic.unit.place`.",
        category: "schematic",
        icon: "component_place",
        kind: Kind::Edit,
    };
    type Params = ComponentPlaceParams;
    type Output = PlacedResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ComponentPlaceParams,
    ) -> Result<PlacedResult, CommandError> {
        sheet(cx.project()?, p.document)?;
        let copy = cx.library_part(p.part)?;
        let content = EntryContent::from_copy(copy)?;
        let footprint = match p.footprint {
            Some(f) if content.part.footprints.contains(&f) => Some(f),
            Some(f) => {
                return Err(CommandError::invalid_params(
                    "/footprint",
                    format!(
                        "Footprint {f} is not one of Part {}'s alternates",
                        content.part.name
                    ),
                ));
            }
            None => content.part.footprints.first().copied(),
        };
        let unit = match &p.unit {
            Some(name) => unit_named(&content.symbol, name)?,
            None => content.symbol.units[0].uuid,
        };
        let designator = next_free_designator(cx.project()?, &content.symbol.designator_prefix);
        let fields = content.part.fields.clone();
        let component_type = content.part.component_type;
        let entry = ensure_entry(cx, p.document, content)?;
        let component = ComponentRef::new(cx.new_id());
        let uuid = p.uuid.unwrap_or_else(|| cx.new_id());
        let instance = UnitInstance {
            uuid,
            component: component.clone(),
            part: entry,
            unit,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg::ZERO, Angle::millideg),
            mirror: p.mirror,
            designator: designator.clone(),
            fields,
            footprint,
            component_type,
            not_fitted: false,
        };
        cx.tx()?
            .insert(Object::Unit(InDocument::new(p.document, instance)))?;
        Ok(PlacedResult {
            unit: uuid,
            component,
            designator,
            part_entry: entry,
        })
    }
}

pub struct UnitPlace;

impl Command for UnitPlace {
    const META: Meta = Meta {
        id: "schematic.unit.place",
        title: "Place Unit",
        description: "Places a further Unit of an existing Component, on any Schematic \
                      Sheet, with the Component's Designator and data. By default it places \
                      the first Unit not yet placed. No Library is needed: the Part comes \
                      from the part table of a sheet the Component is already on.",
        category: "schematic",
        icon: "unit_place",
        kind: Kind::Edit,
    };
    type Params = UnitPlaceParams;
    type Output = PlacedResult;

    fn run(&self, cx: &mut Context<'_>, p: UnitPlaceParams) -> Result<PlacedResult, CommandError> {
        let project = cx.project()?;
        sheet(project, p.document)?;
        let placed: Vec<(Uuid, &UnitInstance)> = project
            .documents()
            .filter_map(|d| project.schematic_sheet(d.uuid))
            .flat_map(|s| s.units().map(move |u| (s.uuid, u)))
            .filter(|(_, u)| u.component.uuid == p.component && u.component.path.is_empty())
            .collect();
        let Some(&(home, template)) = placed.first() else {
            return Err(CommandError::not_found(format!(
                "no Component has UUID {}",
                p.component
            )));
        };
        let source = project
            .schematic_sheet(home)
            .expect("the Unit's sheet exists");
        let entry = source.part(template.part).expect("a Unit's entry exists");
        let content = EntryContent {
            hash: entry.hash.clone(),
            part: entry.part.clone(),
            symbol: entry.symbol.clone(),
            footprints: entry
                .footprints
                .iter()
                .map(|c| {
                    let copy = source.footprint(*c).expect("an entry's copies exist");
                    (copy.hash.clone(), copy.footprint.clone())
                })
                .collect(),
        };
        let taken: BTreeSet<Uuid> = placed.iter().map(|(_, u)| u.unit).collect();
        let designator = template.designator.clone();
        let unit = match &p.unit {
            Some(name) => {
                let unit = unit_named(&content.symbol, name)?;
                if taken.contains(&unit) {
                    return Err(CommandError::rejected(format!(
                        "Unit {name} of {designator} is already placed"
                    )));
                }
                unit
            }
            None => content
                .symbol
                .units
                .iter()
                .map(|u| u.uuid)
                .find(|u| !taken.contains(u))
                .ok_or_else(|| {
                    CommandError::rejected(format!("every Unit of {designator} is placed"))
                })?,
        };
        let template = template.clone();
        let entry = ensure_entry(cx, p.document, content)?;
        let uuid = p.uuid.unwrap_or_else(|| cx.new_id());
        let instance = UnitInstance {
            uuid,
            part: entry,
            unit,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg::ZERO, Angle::millideg),
            mirror: p.mirror,
            ..template
        };
        let component = instance.component.clone();
        cx.tx()?
            .insert(Object::Unit(InDocument::new(p.document, instance)))?;
        Ok(PlacedResult {
            unit: uuid,
            component,
            designator,
            part_entry: entry,
        })
    }
}

/// What a part-table entry holds, wherever it comes from.
struct EntryContent {
    hash: String,
    part: Part,
    symbol: Symbol,
    /// Each alternate's content hash and Footprint, in the Part's order.
    footprints: Vec<(String, Footprint)>,
}

impl EntryContent {
    fn from_copy(copy: PartCopy) -> Result<EntryContent, CommandError> {
        let Some(symbol) = copy.symbol else {
            return Err(CommandError::rejected(format!(
                "Part {} has no Symbol, so it cannot be placed on a Schematic Sheet",
                copy.part.name
            )));
        };
        Ok(EntryContent {
            hash: copy.hash,
            part: copy.part,
            symbol,
            footprints: copy
                .footprints
                .into_iter()
                .map(|f| (f.hash, f.footprint))
                .collect(),
        })
    }
}

/// The UUID of the Symbol's Unit called `name`, or `invalid_params`.
fn unit_named(symbol: &Symbol, name: &str) -> Result<Uuid, CommandError> {
    symbol
        .units
        .iter()
        .find(|u| u.name == name)
        .map(|u| u.uuid)
        .ok_or_else(|| {
            let names: Vec<&str> = symbol.units.iter().map(|u| u.name.as_str()).collect();
            CommandError::invalid_params(
                "/unit",
                format!(
                    "Symbol {} has no Unit {name:?}; it has {}",
                    symbol.name,
                    names.join(", ")
                ),
            )
        })
}

/// The sheet's part-table entry for this content, inserting it (and any Footprint copy
/// the sheet lacks) when the sheet has none with the same Part UUID and hash.
fn ensure_entry(
    cx: &mut Context<'_>,
    document: Uuid,
    content: EntryContent,
) -> Result<Uuid, CommandError> {
    let body = sheet(cx.project()?, document)?;
    if let Some(entry) = body.part_entry_for(content.part.uuid, &content.hash) {
        return Ok(entry.uuid);
    }
    let mut copies = Vec::with_capacity(content.footprints.len());
    for (hash, footprint) in content.footprints {
        let existing = sheet(cx.project()?, document)?
            .footprint_copy_for(footprint.uuid, &hash)
            .map(|c| c.uuid);
        let uuid = match existing {
            Some(uuid) => uuid,
            None => {
                let uuid = cx.new_id();
                let copy = FootprintCopy {
                    uuid,
                    hash,
                    footprint,
                };
                cx.tx()?
                    .insert(Object::FootprintCopy(InDocument::new(document, copy)))?;
                uuid
            }
        };
        copies.push(uuid);
    }
    let uuid = cx.new_id();
    let entry = PartEntry {
        uuid,
        hash: content.hash,
        part: content.part,
        symbol: content.symbol,
        footprints: copies,
    };
    cx.tx()?
        .insert(Object::PartEntry(InDocument::new(document, entry)))?;
    Ok(uuid)
}

/// `<prefix><n>` with the smallest `n` from 1 that no Unit in the Project uses.
pub fn next_free_designator(project: &Project, prefix: &str) -> String {
    let used: BTreeSet<u64> = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.units())
        .filter_map(|u| u.designator.strip_prefix(prefix))
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|n| n.parse().ok())
        .collect();
    let n = (1..).find(|n| !used.contains(n)).expect("a free number");
    format!("{prefix}{n}")
}
