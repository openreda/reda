//! `pcb.local_grid.add / set / delete / reorder`: the board's Local Grids
//! (docs/spec/v0.3/grids-guides.md#local-grids). The list order is the priority: where two
//! overlap the first wins, and the everyday grid is always last. Moving, turning, flipping
//! and locking them go through `pcb.move`, `pcb.rotate`, `pcb.flip` and `pcb.lock`, as any
//! placed object.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry,
    Unavailable,
};
use reda_model::geom::Nm;
use reda_model::{
    AppliesTo, CartesianGrid, InDocument, LengthUnit, LocalGrid, LocalGridShape, Object,
    PcbDocument, TypedLength, Uuid,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::outline::PointParam;
use super::{Created, needs_pcb, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(LocalGridAdd);
    registry.add(LocalGridSet);
    registry.add(LocalGridDelete);
    registry.add(LocalGridReorder);
}

fn find(pcb: &PcbDocument, uuid: Uuid) -> Result<&LocalGrid, CommandError> {
    pcb.local_grid(uuid)
        .ok_or_else(|| CommandError::not_found(format!("no Local Grid has UUID {uuid}")))
}

/// `name` as stored: not blank, and no other Local Grid (than `own`) has it.
fn name(pcb: &PcbDocument, name: String, own: Option<Uuid>) -> Result<String, CommandError> {
    if name.trim().is_empty() {
        return Err(CommandError::invalid_params(
            "/name",
            "a Local Grid's name can't be blank",
        ));
    }
    CommandError::check_text("/name", &name)?;
    if let Some(other) = pcb.local_grid_named(&name).filter(|g| Some(g.uuid) != own) {
        return Err(CommandError::invalid_params(
            "/name",
            format!(
                "the board already has a Local Grid named \"{name}\" ({}); a Local Grid's \
                 name is unique on the board",
                other.uuid
            ),
        ));
    }
    Ok(name)
}

/// The first of "Local Grid 1", "Local Grid 2"… no Local Grid has.
pub fn next_name(pcb: &PcbDocument) -> String {
    (1..)
        .map(|n| format!("Local Grid {n}"))
        .find(|n| pcb.local_grid_named(n).is_none())
        .expect("a free name")
}

/// A Local Grid's shape as Commands take it.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShapeParam {
    /// A `width` × `height` rectangle from `origin` along the grid's own axes, turned
    /// `rotation` counter-clockwise, with grid points every `step_x` along X and `step_y`
    /// along Y from `origin`.
    Cartesian {
        /// A grid point and the rectangle's first corner, from design zero as every
        /// Command's coordinates (never snapped).
        origin: PointParam,
        /// The grid's X axis, counter-clockwise from +x; 0° when left out. Kept in
        /// `[0°, 360°)`.
        #[serde(default)]
        rotation: Angle,
        /// The step along the grid's X axis, such as `"0.5mm"` or `"25mil"`; the unit is
        /// kept. A bare integer of nanometres is kept in mm.
        step_x: TypedLength,
        /// The step along its Y axis; Step X when left out.
        #[serde(default)]
        step_y: Option<TypedLength>,
        /// Along the grid's X axis; positive.
        width: Length,
        /// Along the grid's Y axis; positive.
        height: Length,
    },
}

/// `length` as stored: positive, and a bare number of nanometres kept in mm.
fn step(field: &str, length: TypedLength) -> Result<TypedLength, CommandError> {
    if length.nm() <= Nm(0) {
        return Err(CommandError::invalid_params(
            &format!("/shape/{field}"),
            "a Local Grid's steps must be positive",
        ));
    }
    Ok(match length.unit() {
        Some(_) => length,
        None => TypedLength::new(length.nm(), LengthUnit::Mm),
    })
}

impl ShapeParam {
    fn shape(self) -> Result<LocalGridShape, CommandError> {
        let ShapeParam::Cartesian {
            origin,
            rotation,
            step_x,
            step_y,
            width,
            height,
        } = self;
        for (field, size) in [("width", width), ("height", height)] {
            if size.nm() <= Nm(0) {
                return Err(CommandError::invalid_params(
                    &format!("/shape/{field}"),
                    "a Local Grid's width and height must be positive",
                ));
            }
        }
        let step_x = step("step_x", step_x)?;
        let step_y = step_y.map(|y| step("step_y", y)).transpose()?;
        Ok(LocalGridShape::Cartesian(CartesianGrid {
            origin: origin.point(),
            rotation: rotation.millideg().normalized(),
            step_x,
            step_y: step_y.unwrap_or(step_x),
            width: width.nm(),
            height: height.nm(),
        }))
    }
}

pub struct LocalGridAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalGridAddParams {
    /// Unique on the board; "Local Grid N" (the first free N) when left out.
    #[serde(default)]
    pub name: Option<String>,
    /// Which objects it snaps while they are placed or moved: `all` (when left out),
    /// `components` or `not_components`.
    #[serde(default)]
    pub applies_to: AppliesTo,
    /// Drawn on the canvas; on when left out. A hidden Local Grid still snaps.
    #[serde(default)]
    pub shown: Option<bool>,
    /// The cursor snaps to it; on when left out.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Locked against moves, reshapes and deletes; off when left out.
    #[serde(default)]
    pub locked: bool,
    pub shape: ShapeParam,
}

impl Command for LocalGridAdd {
    const META: Meta = Meta {
        id: "pcb.local_grid.add",
        title: "Add Local Grid",
        description: "Places a Local Grid: a named area of the board with its own grid, \
                      which replaces the everyday grid inside it. It goes last in the \
                      priority order (`pcb.local_grid.reorder` moves it); where two \
                      overlap, the first wins. No output, export or 3D view shows one.",
        category: "pcb",
        icon: "grid",
        kind: Kind::Edit,
    };
    type Params = LocalGridAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LocalGridAddParams) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let name = match p.name {
            Some(n) => name(pcb, n, None)?,
            None => next_name(pcb),
        };
        let shape = p.shape.shape()?;
        let rank = pcb.local_grids().map(|g| g.rank + 1).max().unwrap_or(0);
        let uuid = cx.new_id();
        let grid = LocalGrid {
            uuid,
            name,
            applies_to: p.applies_to,
            shown: p.shown.unwrap_or(true),
            enabled: p.enabled.unwrap_or(true),
            locked: p.locked,
            shape,
            rank,
        };
        cx.tx()?
            .insert(Object::LocalGrid(InDocument::new(document, grid)))?;
        Ok(Created { uuid })
    }
}

pub struct LocalGridSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalGridSetParams {
    /// The Local Grid's UUID.
    pub uuid: Uuid,
    /// A new name, unique on the board.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub applies_to: Option<AppliesTo>,
    #[serde(default)]
    pub shown: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub locked: Option<bool>,
    /// A new shape, given whole.
    #[serde(default)]
    pub shape: Option<ShapeParam>,
    /// Reshapes a locked Local Grid too; without it `shape` on a locked one is refused.
    #[serde(default)]
    pub include_locked: bool,
}

impl Command for LocalGridSet {
    const META: Meta = Meta {
        id: "pcb.local_grid.set",
        title: "Edit Local Grid",
        description: "Changes the given fields of one Local Grid and keeps the rest: its \
                      name, Applies to, shape, and the shown, enabled and locked switches.",
        category: "pcb",
        icon: "grid",
        kind: Kind::Edit,
    };
    type Params = LocalGridSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LocalGridSetParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut g = find(pcb, p.uuid)?.clone();
        if let Some(n) = p.name {
            g.name = name(pcb, n, Some(g.uuid))?;
        }
        if let Some(shape) = p.shape {
            let shape = shape.shape()?;
            if shape != g.shape {
                crate::lock::refuse(pcb, [g.uuid], p.include_locked, "reshape")?;
            }
            g.shape = shape;
        }
        if let Some(applies_to) = p.applies_to {
            g.applies_to = applies_to;
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
            .replace(Object::LocalGrid(InDocument::new(document, g)))?;
        Ok(())
    }
}

pub struct LocalGridDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalGridDeleteParams {
    /// The Local Grid's UUID.
    pub uuid: Uuid,
    /// Deletes a locked Local Grid too; without it a locked one refuses the Command.
    #[serde(default)]
    pub include_locked: bool,
}

impl Command for LocalGridDelete {
    const META: Meta = Meta {
        id: "pcb.local_grid.delete",
        title: "Delete Local Grid",
        description: "Deletes one Local Grid from the board.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = LocalGridDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LocalGridDeleteParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        find(pcb, p.uuid)?;
        crate::lock::refuse(pcb, [p.uuid], p.include_locked, "delete")?;
        cx.tx()?.remove(p.uuid)?;
        Ok(())
    }
}

pub struct LocalGridReorder;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalGridReorderParams {
    /// The Local Grid to move in the priority order.
    pub local_grid: Uuid,
    /// The Local Grid it goes just before; the end of the list (just above the everyday
    /// grid) when left out.
    #[serde(default)]
    pub before: Option<Uuid>,
}

impl Command for LocalGridReorder {
    const META: Meta = Meta {
        id: "pcb.local_grid.reorder",
        title: "Reorder Local Grid",
        description: "Moves one Local Grid in the priority order, just before `before` or \
                      to the end. Where Local Grids overlap, the first in the order wins; \
                      the everyday grid is always last.",
        category: "pcb",
        icon: "grid",
        kind: Kind::Edit,
    };
    type Params = LocalGridReorderParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LocalGridReorderParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        find(pcb, p.local_grid)?;
        if let Some(before) = p.before {
            if before == p.local_grid {
                return Err(CommandError::invalid_params(
                    "/before",
                    "a Local Grid can't go before itself",
                ));
            }
            if pcb.local_grid(before).is_none() {
                return Err(CommandError::invalid_params(
                    "/before",
                    format!("no Local Grid has UUID {before}"),
                ));
            }
        }
        let mut order: Vec<&LocalGrid> = pcb.local_grids().collect();
        let at = order.iter().position(|g| g.uuid == p.local_grid);
        let moved = order.remove(at.expect("found above"));
        let to = match p.before {
            Some(before) => order.iter().position(|g| g.uuid == before),
            None => None,
        };
        order.insert(to.unwrap_or(order.len()), moved);
        let changed: Vec<LocalGrid> = (order.iter().zip(0u32..))
            .filter(|(g, rank)| g.rank != *rank)
            .map(|(g, rank)| LocalGrid {
                rank,
                ..(*g).clone()
            })
            .collect();
        let same_order = pcb
            .local_grids()
            .map(|g| g.uuid)
            .eq(order.iter().map(|g| g.uuid));
        if same_order {
            return Ok(());
        }
        let tx = cx.tx()?;
        for g in changed {
            tx.replace(Object::LocalGrid(InDocument::new(document, g)))?;
        }
        Ok(())
    }
}
