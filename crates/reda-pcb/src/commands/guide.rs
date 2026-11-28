//! `pcb.guide.add / set / delete`: the board's Guides (docs/spec/v0.3/grids-guides.md#guides).
//! Moving, turning, flipping and locking them go through `pcb.move`, `pcb.rotate`,
//! `pcb.flip` and `pcb.lock`, as any placed object.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::{Guide, GuideKind, InDocument, Object, PcbDocument, Uuid, line_angle};
use schemars::JsonSchema;
use serde::Deserialize;

use super::outline::PointParam;
use super::{Created, needs_pcb, nullable, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(GuideAdd);
    registry.add(GuideSet);
    registry.add(GuideDelete);
}

fn find(pcb: &PcbDocument, uuid: Uuid) -> Result<&Guide, CommandError> {
    pcb.guide(uuid)
        .ok_or_else(|| CommandError::not_found(format!("no Guide has UUID {uuid}")))
}

/// `name` as stored: refused when blank.
fn name(name: Option<String>) -> Result<Option<String>, CommandError> {
    match name {
        Some(n) if n.trim().is_empty() => Err(CommandError::invalid_params(
            "/name",
            "a Guide's name can't be blank; leave it out (or null) for none",
        )),
        Some(n) => {
            CommandError::check_text("/name", &n)?;
            Ok(Some(n))
        }
        None => Ok(None),
    }
}

pub struct GuideAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GuideAddParams {
    /// `line`, an infinite line through `at` at `angle`, or `point`.
    pub kind: GuideKind,
    /// The point a line runs through, or the point itself, from design zero as every
    /// Command's coordinates (never snapped).
    pub at: PointParam,
    /// A line's direction, counter-clockwise from +x: required on a line, refused on a
    /// point. Kept in `[0°, 180°)`, since a line at θ + 180° is the same line.
    #[serde(default)]
    pub angle: Option<Angle>,
    /// An optional name for the Grids & Guides list.
    #[serde(default)]
    pub name: Option<String>,
    /// Drawn on the canvas; on when left out. A hidden Guide still snaps.
    #[serde(default)]
    pub shown: Option<bool>,
    /// The cursor snaps to it; on when left out.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Locked against moves and deletes; off when left out.
    #[serde(default)]
    pub locked: bool,
}

impl Command for GuideAdd {
    const META: Meta = Meta {
        id: "pcb.guide.add",
        title: "Add Guide",
        description: "Places a Guide on the board: a `line` through `at` at any `angle`, or a \
                      `point`. The cursor snaps onto Guides, their crossings and point \
                      Guides; no output, export or 3D view shows one.",
        category: "pcb",
        icon: "place_guides",
        kind: Kind::Edit,
    };
    type Params = GuideAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: GuideAddParams) -> Result<Created, CommandError> {
        let document = pcb(cx)?.uuid;
        let angle = match (p.kind, p.angle) {
            (GuideKind::Line, Some(a)) => Some(line_angle(a.millideg())),
            (GuideKind::Line, None) => {
                return Err(CommandError::invalid_params(
                    "/angle",
                    "a line Guide needs an angle",
                ));
            }
            (GuideKind::Point, Some(_)) => {
                return Err(CommandError::invalid_params(
                    "/angle",
                    "a point Guide has no angle",
                ));
            }
            (GuideKind::Point, None) => None,
        };
        let name = name(p.name)?;
        let uuid = cx.new_id();
        let guide = Guide {
            uuid,
            kind: p.kind,
            at: p.at.point(),
            angle,
            name,
            shown: p.shown.unwrap_or(true),
            enabled: p.enabled.unwrap_or(true),
            locked: p.locked,
        };
        cx.tx()?
            .insert(Object::Guide(InDocument::new(document, guide)))?;
        Ok(Created { uuid })
    }
}

pub struct GuideSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GuideSetParams {
    /// The Guide's UUID.
    pub uuid: Uuid,
    /// A new position, from design zero.
    #[serde(default)]
    pub at: Option<PointParam>,
    /// A new direction for a line, kept in `[0°, 180°)`; refused on a point.
    #[serde(default)]
    pub angle: Option<Angle>,
    /// A new name; `null` clears it.
    #[serde(default, deserialize_with = "nullable")]
    pub name: Option<Option<String>>,
    #[serde(default)]
    pub shown: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub locked: Option<bool>,
    /// Moves a locked Guide too; without it `at` or `angle` on a locked one is refused.
    #[serde(default)]
    pub include_locked: bool,
}

impl Command for GuideSet {
    const META: Meta = Meta {
        id: "pcb.guide.set",
        title: "Edit Guide",
        description: "Changes the given fields of one Guide and keeps the rest: its \
                      position, a line's angle, its name, and the shown, enabled and \
                      locked switches.",
        category: "pcb",
        icon: "place_guides",
        kind: Kind::Edit,
    };
    type Params = GuideSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: GuideSetParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut g = find(pcb, p.uuid)?.clone();
        if p.at.is_some() || p.angle.is_some() {
            crate::lock::refuse(pcb, [g.uuid], p.include_locked, "move")?;
        }
        if let Some(angle) = p.angle {
            if g.kind == GuideKind::Point {
                return Err(CommandError::invalid_params(
                    "/angle",
                    "a point Guide has no angle",
                ));
            }
            g.angle = Some(line_angle(angle.millideg()));
        }
        if let Some(at) = p.at {
            g.at = at.point();
        }
        if let Some(n) = p.name {
            g.name = name(n)?;
        }
        if let Some(shown) = p.shown {
            g.shown = shown;
        }
        if let Some(enabled) = p.enabled {
            g.enabled = enabled;
        }
        if let Some(locked) = p.locked {
            g.locked = locked;
        }
        cx.tx()?
            .replace(Object::Guide(InDocument::new(document, g)))?;
        Ok(())
    }
}

pub struct GuideDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GuideDeleteParams {
    /// The Guide's UUID.
    pub uuid: Uuid,
    /// Deletes a locked Guide too; without it a locked one refuses the Command.
    #[serde(default)]
    pub include_locked: bool,
}

impl Command for GuideDelete {
    const META: Meta = Meta {
        id: "pcb.guide.delete",
        title: "Delete Guide",
        description: "Deletes one Guide from the board.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = GuideDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: GuideDeleteParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        find(pcb, p.uuid)?;
        crate::lock::refuse(pcb, [p.uuid], p.include_locked, "delete")?;
        cx.tx()?.remove(p.uuid)?;
        Ok(())
    }
}
