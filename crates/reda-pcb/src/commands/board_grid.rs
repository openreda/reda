//! `pcb.board_grid.add / set / delete`: grid steps saved with the board
//! (docs/spec/v0.3/grids-guides.md#board-grids). Each step keeps the unit it was typed in;
//! at most one row is the Default a board opened with no view state starts on.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::geom::Nm;
use reda_model::{BoardGrid, InDocument, LengthUnit, Object, PcbDocument, TypedLength, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Created, needs_pcb, nullable, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(BoardGridAdd);
    registry.add(BoardGridSet);
    registry.add(BoardGridDelete);
}

fn find(pcb: &PcbDocument, uuid: Uuid) -> Result<&BoardGrid, CommandError> {
    pcb.board_grid(uuid)
        .ok_or_else(|| CommandError::not_found(format!("no Board Grid has UUID {uuid}")))
}

/// `length` as stored: positive, and a bare number of nanometres kept in mm.
fn step(field: &str, length: TypedLength) -> Result<TypedLength, CommandError> {
    if length.nm() <= Nm(0) {
        return Err(CommandError::invalid_params(
            &format!("/{field}"),
            "a Board Grid's steps and visible spacing must be positive",
        ));
    }
    Ok(match length.unit() {
        Some(_) => length,
        None => TypedLength::new(length.nm(), LengthUnit::Mm),
    })
}

/// Refuses a Default mark on `uuid` while another row holds it.
fn one_default(pcb: &PcbDocument, uuid: Option<Uuid>) -> Result<(), CommandError> {
    match pcb.default_board_grid() {
        Some(other) if Some(other.uuid) != uuid => Err(CommandError::invalid_params(
            "/default",
            format!(
                "the board already has a Default Board Grid ({}, {}); clear its Default \
                 mark first",
                other.step_x, other.uuid
            ),
        )),
        _ => Ok(()),
    }
}

pub struct BoardGridAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardGridAddParams {
    /// The snap step along X (and Y, unless `step_y` is given), such as `"0.5mm"` or
    /// `"25mil"`; the unit is kept. A bare integer of nanometres is kept in mm.
    pub step_x: TypedLength,
    /// Step Y, when it differs from Step X.
    #[serde(default)]
    pub step_y: Option<TypedLength>,
    /// The visible grid's fine spacing along X; along Y the same multiple of Step Y.
    /// Unset is Step X.
    #[serde(default)]
    pub visible: Option<TypedLength>,
    /// G and Shift+G step through it; on when left out.
    #[serde(default)]
    pub in_cycle: Option<bool>,
    /// The grid a board opened with no view state starts on. Refused while another
    /// Board Grid has it.
    #[serde(default)]
    pub default: bool,
}

impl Command for BoardGridAdd {
    const META: Meta = Meta {
        id: "pcb.board_grid.add",
        title: "Add Board Grid",
        description: "Saves a grid step with the board, so G and Shift+G offer it beside the \
                      user's presets. Steps keep the unit they are typed in. At most one \
                      Board Grid is the `default` a board opened with no view state starts \
                      on.",
        category: "pcb",
        icon: "grid",
        kind: Kind::Edit,
    };
    type Params = BoardGridAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: BoardGridAddParams) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        if p.default {
            one_default(pcb, None)?;
        }
        let step_x = step("step_x", p.step_x)?;
        let step_y = p.step_y.map(|y| step("step_y", y)).transpose()?;
        let visible = p.visible.map(|v| step("visible", v)).transpose()?;
        let uuid = cx.new_id();
        let grid = BoardGrid {
            uuid,
            step_x,
            step_y,
            visible,
            in_cycle: p.in_cycle.unwrap_or(true),
            default: p.default,
        };
        cx.tx()?
            .insert(Object::BoardGrid(InDocument::new(document, grid)))?;
        Ok(Created { uuid })
    }
}

pub struct BoardGridSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardGridSetParams {
    /// The Board Grid's UUID.
    pub uuid: Uuid,
    /// A new Step X; the unit is kept.
    #[serde(default)]
    pub step_x: Option<TypedLength>,
    /// A new Step Y; `null` makes the grid square.
    #[serde(default, deserialize_with = "nullable")]
    pub step_y: Option<Option<TypedLength>>,
    /// A new visible spacing; `null` makes it Step X.
    #[serde(default, deserialize_with = "nullable")]
    pub visible: Option<Option<TypedLength>>,
    #[serde(default)]
    pub in_cycle: Option<bool>,
    /// Marks or clears the Default. Marking is refused while another Board Grid has it.
    #[serde(default)]
    pub default: Option<bool>,
}

impl Command for BoardGridSet {
    const META: Meta = Meta {
        id: "pcb.board_grid.set",
        title: "Edit Board Grid",
        description: "Changes the given fields of one Board Grid and keeps the rest: its \
                      steps, visible spacing, In cycle and the Default mark.",
        category: "pcb",
        icon: "grid",
        kind: Kind::Edit,
    };
    type Params = BoardGridSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: BoardGridSetParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut g = find(pcb, p.uuid)?.clone();
        if let Some(x) = p.step_x {
            g.step_x = step("step_x", x)?;
        }
        if let Some(y) = p.step_y {
            g.step_y = y.map(|y| step("step_y", y)).transpose()?;
        }
        if let Some(v) = p.visible {
            g.visible = v.map(|v| step("visible", v)).transpose()?;
        }
        if let Some(in_cycle) = p.in_cycle {
            g.in_cycle = in_cycle;
        }
        if let Some(default) = p.default {
            if default {
                one_default(pcb, Some(g.uuid))?;
            }
            g.default = default;
        }
        cx.tx()?
            .replace(Object::BoardGrid(InDocument::new(document, g)))?;
        Ok(())
    }
}

pub struct BoardGridDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardGridDeleteParams {
    /// The Board Grid's UUID.
    pub uuid: Uuid,
}

impl Command for BoardGridDelete {
    const META: Meta = Meta {
        id: "pcb.board_grid.delete",
        title: "Delete Board Grid",
        description: "Deletes one Board Grid from the board. A view on its step keeps it \
                      until the grid is next changed.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = BoardGridDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: BoardGridDeleteParams) -> Result<(), CommandError> {
        find(pcb(cx)?, p.uuid)?;
        cx.tx()?.remove(p.uuid)?;
        Ok(())
    }
}
