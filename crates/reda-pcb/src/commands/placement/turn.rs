//! `pcb.move`, `pcb.rotate`, `pcb.flip` and `pcb.lock`.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::geom::Millideg;
use reda_model::{InDocument, Object, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Op, Placed, default_center, place, resolve};
use crate::commands::outline::PointParam;
use crate::commands::{needs_pcb, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(Move);
    registry.add(Rotate);
    registry.add(Flip);
    registry.add(Lock);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveParams {
    /// The Footprints, Free Pads and Placement Origin to move, by UUID.
    pub objects: Vec<Uuid>,
    /// How far to move them.
    pub by: PointParam,
}

pub struct Move;

impl Command for Move {
    const META: Meta = Meta {
        id: "pcb.move",
        title: "Move",
        description: "Moves Footprints, Free Pads and the Placement Origin by an offset. \
                      Refused when any of them is a locked Footprint.",
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
        place(cx, items, std::iter::repeat_n(Op::Translate(by), n))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotateParams {
    /// The Footprints, Free Pads and Placement Origin to turn, by UUID.
    pub objects: Vec<Uuid>,
    /// Counter-clockwise; 90° when left out. Any angle is allowed.
    #[serde(default)]
    pub angle: Option<Angle>,
    /// The point to turn about: by default a lone object's own position, else the middle
    /// of the selection's extent.
    #[serde(default)]
    pub center: Option<PointParam>,
}

pub struct Rotate;

impl Command for Rotate {
    const META: Meta = Meta {
        id: "pcb.rotate",
        title: "Rotate",
        description: "Turns Footprints, Free Pads and the Placement Origin counter-clockwise \
                      (90° by default) about a point: a lone object turns in place, a \
                      selection about the middle of its extent. Refused when any of them is \
                      a locked Footprint.",
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
        place(cx, items, std::iter::repeat_n(Op::Rotate(about, angle), n))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlipParams {
    /// The Footprints, Free Pads and Placement Origin to flip, by UUID.
    pub objects: Vec<Uuid>,
    /// A point on the vertical line to mirror about: by default a lone object's own
    /// position, else the middle of the selection's extent. Only its x counts.
    #[serde(default)]
    pub center: Option<PointParam>,
}

pub struct Flip;

impl Command for Flip {
    const META: Meta = Meta {
        id: "pcb.flip",
        title: "Flip Side",
        description: "Moves Footprints and SMD Free Pads to the other side of the board, \
                      mirroring them left-right: their Pads and silkscreen, courtyard and \
                      assembly drawing go to the other face, mirrored. A lone object flips \
                      in place, a selection about the middle of its extent. Refused when any \
                      of them is a locked Footprint.",
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
        place(cx, items, std::iter::repeat_n(Op::Flip(x), n))
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockParams {
    /// The Footprints to lock or unlock, by UUID.
    pub footprints: Vec<Uuid>,
    /// True locks them against moving, turning and flipping; false unlocks them.
    pub locked: bool,
}

pub struct Lock;

impl Command for Lock {
    const META: Meta = Meta {
        id: "pcb.lock",
        title: "Lock",
        description: "Locks Footprints against moving, turning and flipping, or unlocks \
                      them. Their Designator text and data stay editable.",
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
        let document = pcb.uuid;
        let mut changed = Vec::new();
        for (i, &uuid) in p.footprints.iter().enumerate() {
            let Some(f) = pcb.footprint(uuid) else {
                return Err(CommandError::invalid_params(
                    &format!("/footprints/{i}"),
                    format!("{uuid} is not a Footprint on the board"),
                ));
            };
            if f.locked != p.locked {
                changed.push(f.clone());
            }
        }
        let objects = p.footprints;
        let tx = cx.tx()?;
        for mut f in changed {
            f.locked = p.locked;
            tx.replace(Object::FootprintInstance(InDocument::new(document, f)))?;
        }
        Ok(Placed { objects })
    }
}
