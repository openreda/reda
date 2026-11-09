//! `pcb.move`, `pcb.rotate`, `pcb.flip` and `pcb.lock`.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::geom::Millideg;
use reda_model::Uuid;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Op, Placed, default_center, place, resolve};
use crate::commands::outline::PointParam;
use crate::commands::{needs_pcb, pcb};
use crate::lock::{lock_of, with_lock};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(Move);
    registry.add(Rotate);
    registry.add(Flip);
    registry.add(Lock);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveParams {
    /// The Footprints, Free Pads, Dimensions, Output Origin, Board Outline and Board Cutouts
    /// to move, by UUID.
    pub objects: Vec<Uuid>,
    /// How far to move them.
    pub by: PointParam,
    /// Moves locked objects too; without it a locked one refuses the whole Command.
    #[serde(default)]
    pub include_locked: bool,
}

pub struct Move;

impl Command for Move {
    const META: Meta = Meta {
        id: "pcb.move",
        title: "Move",
        description: "Moves Footprints, Free Pads, Dimensions, the Output Origin, the Board \
                      Outline and Board Cutouts by an offset. Moving the Board Outline \
                      alone is Move Board Shape; `pcb.board.move` moves the whole board. \
                      Refused when any of them is locked (unless `include_locked`), or a \
                      Dimension without the objects it references.",
        category: "pcb",
        icon: "move",
        kind: Kind::Edit,
    };
    type Params = MoveParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: MoveParams) -> Result<Placed, CommandError> {
        let items = resolve(pcb(cx)?, &p.objects)?;
        let by = p.by.point();
        let n = items.len();
        place(cx, items, std::iter::repeat_n(Op::Translate(by), n), p.include_locked)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotateParams {
    /// The Footprints, Free Pads, Dimensions, Output Origin, Board Outline and Board Cutouts
    /// to turn, by UUID.
    pub objects: Vec<Uuid>,
    /// Counter-clockwise; 90° when left out. Any angle is allowed.
    #[serde(default)]
    pub angle: Option<Angle>,
    /// The point to turn about: by default a lone object's own position, else the middle
    /// of the selection's extent.
    #[serde(default)]
    pub center: Option<PointParam>,
    /// Moves locked objects too; without it a locked one refuses the whole Command.
    #[serde(default)]
    pub include_locked: bool,
}

pub struct Rotate;

impl Command for Rotate {
    const META: Meta = Meta {
        id: "pcb.rotate",
        title: "Rotate",
        description: "Turns Footprints, Free Pads, Dimensions, the Output Origin, the Board \
                      Outline and Board Cutouts counter-clockwise (90° by default) about a point: a lone object turns \
                      in place, a \
                      selection about the middle of its extent. Refused when any of them is \
                      locked (unless `include_locked`).",
        category: "pcb",
        icon: "rotate",
        kind: Kind::Edit,
    };
    type Params = RotateParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: RotateParams) -> Result<Placed, CommandError> {
        let pcb = pcb(cx)?;
        let items = resolve(pcb, &p.objects)?;
        let angle = p.angle.map_or(Millideg::from_deg(90), Angle::millideg);
        let about = p
            .center
            .map_or_else(|| default_center(pcb, &items), PointParam::point);
        let n = items.len();
        place(
            cx,
            items,
            std::iter::repeat_n(Op::Rotate(about, angle), n),
            p.include_locked,
        )
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlipParams {
    /// The Footprints, Free Pads, Dimensions, Output Origin, Board Outline and Board Cutouts
    /// to flip, by UUID.
    pub objects: Vec<Uuid>,
    /// A point on the vertical line to mirror about: by default a lone object's own
    /// position, else the middle of the selection's extent. Only its x counts.
    #[serde(default)]
    pub center: Option<PointParam>,
    /// Moves locked objects too; without it a locked one refuses the whole Command.
    #[serde(default)]
    pub include_locked: bool,
}

pub struct Flip;

impl Command for Flip {
    const META: Meta = Meta {
        id: "pcb.flip",
        title: "Flip Side",
        description: "Moves Footprints and SMD Free Pads to the other side of the board, \
                      mirroring them left-right: their Pads and silkscreen, courtyard and \
                      assembly drawing go to the other face, mirrored. Dimensions mirror in \
                      place on their Layer, and the Board Outline and Board Cutouts mirror \
                      left-right. A lone object flips \
                      in place, a selection about the middle of its extent. Refused when any \
                      of them is locked (unless `include_locked`).",
        category: "pcb",
        icon: "flip",
        kind: Kind::Edit,
    };
    type Params = FlipParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: FlipParams) -> Result<Placed, CommandError> {
        let pcb = pcb(cx)?;
        let items = resolve(pcb, &p.objects)?;
        let x = p
            .center
            .map_or_else(|| default_center(pcb, &items), PointParam::point)
            .x;
        let n = items.len();
        place(cx, items, std::iter::repeat_n(Op::Flip(x), n), p.include_locked)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockParams {
    /// The objects to lock or unlock, by UUID: Footprints, Tracks, Vias, Zones, Free Pads,
    /// text, graphics, Dimensions, Keepouts, the Board Outline and Board Cutouts.
    pub objects: Vec<Uuid>,
    /// True locks their geometry; false unlocks it.
    pub locked: bool,
}

pub struct Lock;

impl Command for Lock {
    const META: Meta = Meta {
        id: "pcb.lock",
        title: "Lock",
        description: "Locks placed objects (Footprints, Tracks, Vias, Zones, Free Pads, text, \
                      graphics, Dimensions, Keepouts, the Board Outline and Board Cutouts) \
                      against geometry changes, or unlocks them. A lock refuses moving, \
                      turning, flipping, deleting and reshaping unless a Command is given \
                      `include_locked`; other values stay editable, and Sync, Update from \
                      Library and `pcb.board.move` still apply.",
        category: "pcb",
        icon: "lock",
        kind: Kind::Edit,
    };
    type Params = LockParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LockParams) -> Result<Placed, CommandError> {
        let pcb = pcb(cx)?;
        let mut changed = Vec::new();
        for (i, &uuid) in p.objects.iter().enumerate() {
            match lock_of(pcb, uuid) {
                None => {
                    return Err(CommandError::invalid_params(
                        &format!("/objects/{i}"),
                        format!("{uuid} is not an object on the board that can be locked"),
                    ));
                }
                Some(was) if was != p.locked => {
                    changed.extend(with_lock(pcb, uuid, p.locked));
                }
                Some(_) => {}
            }
        }
        let objects = p.objects;
        let tx = cx.tx()?;
        for object in changed {
            tx.replace(object)?;
        }
        Ok(Placed { objects })
    }
}
