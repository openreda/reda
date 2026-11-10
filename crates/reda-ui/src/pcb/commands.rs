//! The PCB view Commands: view Commands (no undo, never dirty) that change the
//! [`BoardView`] in the engine's view state. The Layers panel and the view keys run them,
//! so scripts and agents change the view the same way.

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NoParams, Preset, Registry,
    Unavailable,
};
use reda_engine::model::{LayerKind, LayerSet, ObjectType, Side, Uuid};
use reda_engine::preferences::{InactiveLayerMode, Units};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::view::{BoardView, VIEW_TYPES};
use crate::grid::Grids;

/// Adds the PCB view Commands to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(LayerShow);
    registry.add(LayerActivate);
    registry.add(LayerStep);
    registry.add(LayerSwapSide);
    registry.add(InactiveLayersSet);
    registry.add(FromBottom);
    registry.add(ObjectTypeShow);
    registry.add(RatsnestShow);
    registry.add(UnitsToggle);
}

fn needs_pcb(cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
    match cx.project {
        Some(p) if p.pcb().is_some() => Ok(()),
        Some(_) => Err(Unavailable {
            reason: "the Project has no PCB Document".into(),
        }),
        None => Err(Unavailable {
            reason: "no Project is open".into(),
        }),
    }
}

/// The board's Layer set.
fn layer_set(cx: &Context<'_>) -> Result<LayerSet, CommandError> {
    let project = cx.project()?;
    let pcb = project
        .pcb()
        .ok_or_else(|| CommandError::unavailable("the Project has no PCB Document"))?;
    Ok(pcb.layers().clone())
}

fn board_view<'c>(cx: &'c mut Context<'_>) -> Result<&'c mut BoardView, CommandError> {
    cx.view::<BoardView>()
}

fn check_layer(set: &LayerSet, layer: Uuid) -> Result<(), CommandError> {
    match set.layer(layer) {
        Some(_) => Ok(()),
        None => Err(CommandError::invalid_params(
            "/layer",
            format!("{layer} is not a Layer of the board"),
        )),
    }
}

/// The Layer that is active now (the chosen one, else the top copper).
fn active(set: &LayerSet, view: &BoardView) -> Option<Uuid> {
    view.layers
        .active
        .filter(|l| set.layer(*l).is_some())
        .or_else(|| set.find(LayerKind::Copper, Some(Side::Top)).map(|l| l.uuid))
}

/// The Layer now active.
#[derive(Serialize, JsonSchema)]
pub struct ActiveLayer {
    pub layer: Option<Uuid>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerShowParams {
    /// The Layer, by UUID.
    layer: Uuid,
    /// Show or hide it; left out, it toggles.
    #[serde(default)]
    visible: Option<bool>,
}

/// Whether the thing is now shown.
#[derive(Serialize, JsonSchema)]
pub struct Shown {
    pub visible: bool,
}

struct LayerShow;

impl Command for LayerShow {
    const META: Meta = Meta {
        id: "pcb.layer.show",
        title: "Show Layer",
        description: "Shows or hides one board Layer on the PCB canvas; with `visible` left \
                      out it toggles.",
        category: "pcb_view",
        icon: "layer_visible",
        kind: Kind::View,
    };
    type Params = LayerShowParams;
    type Output = Shown;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LayerShowParams) -> Result<Shown, CommandError> {
        check_layer(&layer_set(cx)?, p.layer)?;
        let hidden = &mut board_view(cx)?.layers.hidden;
        let visible = p.visible.unwrap_or(hidden.contains(&p.layer));
        if visible {
            hidden.remove(&p.layer);
        } else {
            hidden.insert(p.layer);
        }
        Ok(Shown { visible })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerActivateParams {
    /// The Layer to edit on, by UUID.
    layer: Uuid,
}

struct LayerActivate;

impl Command for LayerActivate {
    const META: Meta = Meta {
        id: "pcb.layer.activate",
        title: "Set Active Layer",
        description: "Makes a Layer the active one: drawn last at full opacity, and where new \
                      Tracks and text go. A hidden Layer is shown.",
        category: "pcb_view",
        icon: "layer_active",
        kind: Kind::View,
    };
    type Params = LayerActivateParams;
    type Output = ActiveLayer;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: LayerActivateParams,
    ) -> Result<ActiveLayer, CommandError> {
        check_layer(&layer_set(cx)?, p.layer)?;
        let view = board_view(cx)?;
        view.layers.active = Some(p.layer);
        view.layers.hidden.remove(&p.layer);
        Ok(ActiveLayer {
            layer: Some(p.layer),
        })
    }
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// The copper Layer below, wrapping to the top.
    Next,
    /// The copper Layer above, wrapping to the bottom.
    Previous,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerStepParams {
    direction: Direction,
}

struct LayerStep;

impl Command for LayerStep {
    const META: Meta = Meta {
        id: "pcb.layer.step",
        title: "Step Copper Layer",
        description: "Makes the next or previous copper Layer active, wrapping around. From a \
                      Layer that is not copper, it starts at the top copper.",
        category: "pcb_view",
        icon: "layer_active",
        kind: Kind::View,
    };
    type Params = LayerStepParams;
    type Output = ActiveLayer;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new(
                "next",
                "Next Copper Layer",
                "layer_next",
                json!({"direction": "next"}),
            ),
            Preset::new(
                "previous",
                "Previous Copper Layer",
                "layer_previous",
                json!({"direction": "previous"}),
            ),
        ]
    }

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LayerStepParams) -> Result<ActiveLayer, CommandError> {
        let set = layer_set(cx)?;
        let copper: Vec<Uuid> = set
            .layers
            .iter()
            .filter(|l| l.kind == LayerKind::Copper)
            .map(|l| l.uuid)
            .collect();
        let view = board_view(cx)?;
        let current = active(&set, view).and_then(|a| copper.iter().position(|c| *c == a));
        let n = copper.len();
        let next = match (current, p.direction) {
            (_, _) if n == 0 => return Ok(ActiveLayer { layer: None }),
            (None, _) => 0,
            (Some(i), Direction::Next) => (i + 1) % n,
            (Some(i), Direction::Previous) => (i + n - 1) % n,
        };
        view.layers.active = Some(copper[next]);
        view.layers.hidden.remove(&copper[next]);
        Ok(ActiveLayer {
            layer: Some(copper[next]),
        })
    }
}

struct LayerSwapSide;

impl Command for LayerSwapSide {
    const META: Meta = Meta {
        id: "pcb.layer.swap_side",
        title: "Swap Active Layer Side",
        description: "Makes the twin of the active Layer on the other face active: top copper \
                      and bottom copper, top and bottom silkscreen, and so on. A Layer with no \
                      side stays.",
        category: "pcb_view",
        icon: "layer_swap",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = ActiveLayer;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<ActiveLayer, CommandError> {
        let set = layer_set(cx)?;
        let view = board_view(cx)?;
        let Some(current) = active(&set, view).and_then(|a| set.layer(a)) else {
            return Ok(ActiveLayer { layer: None });
        };
        let twin = match current.side {
            Some(Side::Top) => set.find(current.kind, Some(Side::Bottom)),
            Some(Side::Bottom) => set.find(current.kind, Some(Side::Top)),
            None => Some(current),
        }
        .map(|l| l.uuid);
        if let Some(twin) = twin {
            view.layers.active = Some(twin);
            view.layers.hidden.remove(&twin);
        }
        Ok(ActiveLayer { layer: twin })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InactiveLayersParams {
    /// The mode; left out, it cycles Normal, Dimmed, Hidden.
    #[serde(default)]
    mode: Option<InactiveLayerMode>,
}

/// The Inactive-Layer mode now in force.
#[derive(Serialize, JsonSchema)]
pub struct Mode {
    pub mode: InactiveLayerMode,
}

struct InactiveLayersSet;

impl Command for InactiveLayersSet {
    const META: Meta = Meta {
        id: "pcb.view.inactive_layers",
        title: "Inactive Layers",
        description: "Sets how Layers other than the active one are drawn: normal, dimmed, or \
                      hidden (single-Layer mode). With `mode` left out it cycles through the \
                      three.",
        category: "pcb_view",
        icon: "layers",
        kind: Kind::View,
    };
    type Params = InactiveLayersParams;
    type Output = Mode;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: InactiveLayersParams) -> Result<Mode, CommandError> {
        let default = cx
            .user_dirs()
            .map(|d| {
                reda_engine::preferences::current(Some(d))
                    .display
                    .inactive_layer_mode
            })
            .unwrap_or_default();
        let view = board_view(cx)?;
        let mode = p
            .mode
            .unwrap_or_else(|| match view.layers.inactive_mode.unwrap_or(default) {
                InactiveLayerMode::Normal => InactiveLayerMode::Dimmed,
                InactiveLayerMode::Dimmed => InactiveLayerMode::Hidden,
                InactiveLayerMode::Hidden => InactiveLayerMode::Normal,
            });
        view.layers.inactive_mode = Some(mode);
        Ok(Mode { mode })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FromBottomParams {
    /// See the board from below; left out, it toggles.
    #[serde(default)]
    from_bottom: Option<bool>,
}

/// Whether the board is now seen from below.
#[derive(Serialize, JsonSchema)]
pub struct Flipped {
    pub from_bottom: bool,
}

struct FromBottom;

impl Command for FromBottom {
    const META: Meta = Meta {
        id: "pcb.view.from_bottom",
        title: "View from Bottom",
        description: "Shows the board as seen from below: mirrored left-right, with the \
                      physical Layers drawn in reverse order. With `from_bottom` left out it \
                      toggles.",
        category: "pcb_view",
        icon: "view_flip",
        kind: Kind::View,
    };
    type Params = FromBottomParams;
    type Output = Flipped;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: FromBottomParams) -> Result<Flipped, CommandError> {
        let layers = &mut board_view(cx)?.layers;
        layers.from_bottom = p.from_bottom.unwrap_or(!layers.from_bottom);
        Ok(Flipped {
            from_bottom: layers.from_bottom,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectTypeShowParams {
    /// The kind of object: footprint_instance, track, via, zone, keepout, zone_cutout,
    /// free_pad, pcb_text or pcb_graphic.
    #[serde(rename = "type")]
    kind: ObjectType,
    /// Show or hide them; left out, it toggles.
    #[serde(default)]
    visible: Option<bool>,
}

struct ObjectTypeShow;

impl Command for ObjectTypeShow {
    const META: Meta = Meta {
        id: "pcb.view.object_type.show",
        title: "Show Object Type",
        description: "Shows or hides every object of one kind on the PCB canvas, such as all \
                      Tracks; with `visible` left out it toggles.",
        category: "pcb_view",
        icon: "layer_visible",
        kind: Kind::View,
    };
    type Params = ObjectTypeShowParams;
    type Output = Shown;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ObjectTypeShowParams) -> Result<Shown, CommandError> {
        if !VIEW_TYPES.contains(&p.kind) {
            return Err(CommandError::invalid_params(
                "/type",
                "only board objects can be hidden by kind",
            ));
        }
        let hidden = &mut board_view(cx)?.layers.hidden_types;
        let visible = p.visible.unwrap_or(hidden.contains(&p.kind));
        if visible {
            hidden.remove(&p.kind);
        } else {
            hidden.insert(p.kind);
        }
        Ok(Shown { visible })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RatsnestShowParams {
    /// One Net's lines, by UUID; left out (with no `footprints`), the whole Ratsnest.
    #[serde(default)]
    net: Option<Uuid>,
    /// These Footprints' lines (every line ending on one of their Pads), by UUID.
    #[serde(default)]
    footprints: Vec<Uuid>,
    /// Show or hide; left out, it toggles.
    #[serde(default)]
    visible: Option<bool>,
}

struct RatsnestShow;

impl Command for RatsnestShow {
    const META: Meta = Meta {
        id: "pcb.ratsnest.show",
        title: "Show Ratsnest",
        description: "Shows or hides the Ratsnest, one Net's lines in it, or the lines of \
                      some Footprints (kept with the view); with `visible` left out it \
                      toggles.",
        category: "pcb_view",
        icon: "ratsnest",
        kind: Kind::View,
    };
    type Params = RatsnestShowParams;
    type Output = Shown;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: RatsnestShowParams) -> Result<Shown, CommandError> {
        if let Some(net) = p.net {
            let project = cx.project()?;
            if project.pcb().and_then(|pcb| pcb.net(net)).is_none() {
                return Err(CommandError::invalid_params(
                    "/net",
                    format!("{net} is not a Net of the board"),
                ));
            }
        }
        if !p.footprints.is_empty() {
            let project = cx.project()?;
            let pcb = project.pcb().expect("available");
            for (i, f) in p.footprints.iter().enumerate() {
                if pcb.footprint(*f).is_none() {
                    return Err(CommandError::invalid_params(
                        &format!("/footprints/{i}"),
                        format!("{f} is not a Footprint on the board"),
                    ));
                }
            }
            let hidden = &mut board_view(cx)?.layers.ratsnest.hidden_footprints;
            let visible = p
                .visible
                .unwrap_or_else(|| p.footprints.iter().any(|f| hidden.contains(f)));
            for f in p.footprints {
                if visible {
                    hidden.remove(&f);
                } else {
                    hidden.insert(f);
                }
            }
            return Ok(Shown { visible });
        }
        let ratsnest = &mut board_view(cx)?.layers.ratsnest;
        let visible = match p.net {
            Some(net) => {
                let visible = p.visible.unwrap_or(ratsnest.hidden_nets.contains(&net));
                ratsnest.set_net(net, visible);
                visible
            }
            None => {
                ratsnest.hidden = !p.visible.unwrap_or(ratsnest.hidden);
                !ratsnest.hidden
            }
        };
        Ok(Shown { visible })
    }
}

/// The units now in force.
#[derive(Serialize, JsonSchema)]
pub struct UnitsNow {
    pub units: Units,
    pub grid: reda_engine::model::geom::Nm,
}

struct UnitsToggle;

impl Command for UnitsToggle {
    const META: Meta = Meta {
        id: "view.units.toggle",
        title: "Toggle mm/mil",
        description: "Switches the units lengths show in on the board between mm and mil. \
                      The board's snap grid moves to the matching preset in the other units.",
        category: "pcb_view",
        icon: "units",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = UnitsNow;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<UnitsNow, CommandError> {
        let pcb = cx
            .project()?
            .pcb()
            .map(|pcb| pcb.uuid)
            .ok_or_else(|| CommandError::rejected("the Project has no PCB Document"))?;
        let view = board_view(cx)?;
        let units = match view.units {
            Units::Mm => Units::Mil,
            Units::Mil => Units::Mm,
        };
        view.units = units;
        let grids = cx.view::<Grids>()?;
        let current = grids.picked.get(&pcb).copied();
        let grid = grids.pcb_in_units(current.unwrap_or(grids.pcb), units);
        grids.custom.remove(&pcb);
        grids.picked.insert(pcb, grid);
        Ok(UnitsNow { units, grid })
    }
}
