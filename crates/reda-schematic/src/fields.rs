//! Field positions: `schematic.field.move` takes a field off Autoposition by storing it in
//! its Unit's sparse `field_text` table, and `schematic.field.reset_position` puts it back.

use reda_command::{Command, CommandError, Context, Kind, Length, Meta};
use reda_model::geom::{Millideg, Nm, Point, rotate};
use reda_model::{FieldText, Fields, InDocument, Object, Project, UnitInstance, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::component::At;

/// The placed Unit `unit` with its sheet, or `not_found`.
pub(crate) fn placed_unit(
    project: &Project,
    unit: Uuid,
) -> Result<(Uuid, UnitInstance), CommandError> {
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .find_map(|s| s.unit(unit).map(|u| (s.uuid, u.clone())))
        .ok_or_else(|| CommandError::not_found(format!("no placed Unit has UUID {unit}")))
}

/// Refuses a field name the Unit's Component does not have: `designator`, a built-in
/// field or one of its free-form fields.
fn check_field(unit: &UnitInstance, field: &str) -> Result<(), CommandError> {
    let known = field == UnitInstance::DESIGNATOR
        || Fields::BUILT_IN.contains(&field)
        || unit.fields.custom.iter().any(|f| f.name == field);
    if known {
        return Ok(());
    }
    Err(CommandError::invalid_params(
        "/field",
        format!("{} has no field called {field:?}", unit.designator),
    ))
}

/// `p` on the sheet as an offset from `unit`'s Symbol origin in the Symbol's own frame:
/// the placement (mirror, then rotate, then move) undone.
fn offset_of(unit: &UnitInstance, p: Point) -> Point {
    let (x, y) = rotate(
        (p.x - unit.at.x).0,
        (p.y - unit.at.y).0,
        Millideg(-unit.rotation.0),
    );
    Point::new(Nm(if unit.mirror { -x } else { x }), Nm(y))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldMoveParams {
    /// The placed Unit, by UUID.
    pub unit: Uuid,
    /// The field, by name: `designator`, `value`, another built-in field or a free-form
    /// field's name.
    pub field: String,
    /// Where the start of the field's text goes on the sheet (its left end, centred
    /// vertically).
    pub at: At,
    /// The text height; unchanged by default (the default height for a field not moved
    /// before).
    pub size: Option<Length>,
}

/// The Unit whose field was moved or reset.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FieldResult {
    pub unit: Uuid,
    /// The sheet it is on.
    pub document: Uuid,
}

pub struct FieldMove;

impl Command for FieldMove {
    const META: Meta = Meta {
        id: "schematic.field.move",
        title: "Move Field",
        description: "Moves one field of a placed Unit by hand, taking it off Autoposition: \
                      the Unit's `field_text` stores it by name, as an offset from the \
                      Symbol origin (so it follows the Unit as it moves, rotates and \
                      mirrors) with an optional text height. The text stays right-reading. \
                      `schematic.field.reset_position` puts it back on Autoposition.",
        category: "schematic",
        icon: "move",
        kind: Kind::Edit,
    };
    type Params = FieldMoveParams;
    type Output = FieldResult;

    fn run(&self, cx: &mut Context<'_>, p: FieldMoveParams) -> Result<FieldResult, CommandError> {
        let (document, mut unit) = placed_unit(cx.project()?, p.unit)?;
        check_field(&unit, &p.field)?;
        if let Some(size) = p.size
            && size.nm() <= Nm(0)
        {
            return Err(CommandError::invalid_params(
                "/size",
                "a text height must be more than zero",
            ));
        }
        let offset = offset_of(&unit, p.at.point());
        let size = match p.size {
            Some(size) => Some(size.nm()),
            None => unit.field_text(&p.field).and_then(|t| t.size),
        };
        unit.set_field_text(FieldText {
            field: p.field,
            offset,
            size,
        });
        cx.tx()?
            .replace(Object::Unit(InDocument::new(document, unit)))?;
        Ok(FieldResult {
            unit: p.unit,
            document,
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldResetParams {
    /// The placed Unit, by UUID.
    pub unit: Uuid,
    /// The field, by name, as for `schematic.field.move`.
    pub field: String,
}

pub struct FieldResetPosition;

impl Command for FieldResetPosition {
    const META: Meta = Meta {
        id: "schematic.field.reset_position",
        title: "Reset Field Position",
        description: "Puts a field moved by hand back on Autoposition, removing its entry \
                      (position and text height) from the Unit's `field_text`. A field \
                      already autopositioned is left as it is.",
        category: "schematic",
        icon: "home",
        kind: Kind::Edit,
    };
    type Params = FieldResetParams;
    type Output = FieldResult;

    fn run(&self, cx: &mut Context<'_>, p: FieldResetParams) -> Result<FieldResult, CommandError> {
        let (document, mut unit) = placed_unit(cx.project()?, p.unit)?;
        check_field(&unit, &p.field)?;
        if unit.remove_field_text(&p.field) {
            cx.tx()?
                .replace(Object::Unit(InDocument::new(document, unit)))?;
        }
        Ok(FieldResult {
            unit: p.unit,
            document,
        })
    }
}
