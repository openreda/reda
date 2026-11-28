//! Board Grids: grid steps saved with the board (docs/spec/v0.3/grids-guides.md#board-grids).
//!
//! A board carries its own grid steps, so a design that needs an unusual pitch takes it
//! along in git. G and Shift+G cycle the In-cycle ones together with the user's presets,
//! and a board opened with no view state starts on its Default one. A PCB Document stores
//! them in `grids.board`; the grid in use stays working state.

use reda_geom::Nm;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::{TypedLength, Uuid};

/// One Board Grid. Field order is the on-disk key order. Each length keeps the unit it
/// was typed in (`"0.5mm"`, `"25mil"`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardGrid {
    pub uuid: Uuid,
    /// The snap step along X, and along Y too unless `step_y` is set.
    #[schemars(schema_with = "step_schema")]
    pub step_x: TypedLength,
    /// Step Y, when it differs from Step X.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "optional_step_schema")]
    pub step_y: Option<TypedLength>,
    /// The visible grid's fine spacing along X, as in a preset row; along Y it is the
    /// same multiple of Step Y. Left out, it is Step X.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "optional_step_schema")]
    pub visible: Option<TypedLength>,
    /// G and Shift+G step through it.
    pub in_cycle: bool,
    /// The grid a board opened with no view state starts on. At most one row has it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub default: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl BoardGrid {
    /// A square, In-cycle Board Grid of `step`, not the Default.
    pub fn new(uuid: Uuid, step: TypedLength) -> BoardGrid {
        BoardGrid {
            uuid,
            step_x: step,
            step_y: None,
            visible: None,
            in_cycle: true,
            default: false,
        }
    }

    /// Step X in nanometres.
    pub fn x(&self) -> Nm {
        self.step_x.nm()
    }

    /// Step Y in nanometres: `step_y`, else Step X.
    pub fn y(&self) -> Nm {
        self.step_y.unwrap_or(self.step_x).nm()
    }

    /// The visible grid's fine spacing along X and Y: `visible` (else Step X), and along
    /// Y the same multiple of Step Y.
    pub fn visible_xy(&self) -> (Nm, Nm) {
        let x = self.visible.unwrap_or(self.step_x).nm();
        let (sx, sy) = (self.x(), self.y());
        if sx == sy || sx.0 <= 0 {
            return (x, x);
        }
        let y = i128::from(x.0) * i128::from(sy.0) / i128::from(sx.0);
        (x, Nm(i64::try_from(y).unwrap_or(i64::MAX)))
    }

    /// Why the row is not a valid Board Grid, if it is not: each length is positive and
    /// written with its unit.
    pub fn problem(&self) -> Option<String> {
        let lengths = [
            ("step_x", Some(self.step_x)),
            ("step_y", self.step_y),
            ("visible", self.visible),
        ];
        for (field, length) in lengths {
            let Some(length) = length else { continue };
            if length.unit().is_none() {
                return Some(format!(
                    "Board Grid {} has a `{field}` with no unit; write it as \"0.5mm\" or \
                     \"25mil\"",
                    self.uuid
                ));
            }
            if length.nm().0 <= 0 {
                return Some(format!(
                    "Board Grid {} has a `{field}` of {length}; it must be positive",
                    self.uuid
                ));
            }
        }
        None
    }
}

/// A Board Grid's length in the file: always text with its unit.
fn step_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "description": "A length with the unit it was typed in, such as \"0.5mm\" or \"25mil\" (nm, um, mm, mil or in).",
        "type": "string",
        "format": "length"
    })
}

fn optional_step_schema(generator: &mut SchemaGenerator) -> Schema {
    step_schema(generator)
}
