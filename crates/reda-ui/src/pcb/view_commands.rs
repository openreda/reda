//! More PCB view Commands: Layer sets, pinned Layers, per-kind display, `F5` Net colours
//! and the Net label toggles. Like the others in [`super::commands`], they are view
//! Commands (no undo, never dirty) changing the [`BoardView`]; only saving and deleting
//! a Layer set writes, to the Preferences, because saved sets travel with them.

use std::collections::BTreeSet;

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Preset, Registry, Unavailable,
};
use reda_engine::model::{Layer, LayerKind, LayerSet, ObjectType, Side, Uuid};
use reda_engine::preferences::{self, SavedLayerSet};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::view::{BoardView, VIEW_TYPES, color_key};
use crate::view_state::{KindDisplay, LabelView};

/// The Layer sets every board has, in the order the Layer control lists them.
pub const LAYER_SETS: [&str; 7] = [
    "all",
    "signal",
    "non_signal",
    "mechanical",
    "top",
    "bottom",
    "fab",
];

/// Adds these Commands to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(LayerSetApply);
    registry.add(LayerSetSave);
    registry.add(LayerSetDelete);
    registry.add(LayerPin);
    registry.add(ObjectTypeSet);
    registry.add(NetColors);
    registry.add(Labels);
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

/// Whether built-in set `name` holds `layer`; `None` for a name that is not built in.
pub fn built_in(name: &str, layer: &Layer) -> Option<bool> {
    use LayerKind as K;
    let outline = layer.kind == K::BoardOutline;
    Some(match name {
        "all" => true,
        "signal" => layer.kind == K::Copper,
        "non_signal" => matches!(layer.kind, K::Silkscreen | K::SolderMask | K::Paste),
        "mechanical" => matches!(
            layer.kind,
            K::BoardOutline | K::Mechanical | K::Documentation | K::Courtyard | K::Assembly
        ),
        "top" => layer.side == Some(Side::Top) || outline,
        "bottom" => layer.side == Some(Side::Bottom) || outline,
        "fab" => {
            matches!(
                layer.kind,
                K::Copper | K::SolderMask | K::Silkscreen | K::Paste | K::BoardOutline
            )
        }
        _ => return None,
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerSetApplyParams {
    /// A built-in set (all, signal, non_signal, mechanical, top, bottom, fab) or the
    /// name of one saved in the Preferences.
    set: String,
}

/// The Layers now shown.
#[derive(Serialize, JsonSchema)]
pub struct LayersShown {
    pub layers: Vec<Uuid>,
}

struct LayerSetApply;

impl Command for LayerSetApply {
    const META: Meta = Meta {
        id: "pcb.view.layer_set.apply",
        title: "Apply Layer Set",
        description: "Shows exactly the Layers of a Layer set and hides the rest: a \
                      built-in set (all, signal, non_signal, mechanical, top, bottom, fab) \
                      or one saved in the Preferences. The active Layer stays shown.",
        category: "pcb_view",
        icon: "layers",
        kind: Kind::View,
    };
    type Params = LayerSetApplyParams;
    type Output = LayersShown;

    fn presets(&self) -> Vec<Preset> {
        LAYER_SETS
            .iter()
            .map(|set| {
                Preset::new(
                    *set,
                    format!("Layer Set: {}", title(set)),
                    "layers",
                    json!({ "set": set }),
                )
            })
            .collect()
    }

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: LayerSetApplyParams,
    ) -> Result<LayersShown, CommandError> {
        let set = layer_set(cx)?;
        let saved = saved_sets(cx);
        let holds: Box<dyn Fn(&Layer) -> bool> = if LAYER_SETS.contains(&p.set.as_str()) {
            let name = p.set.clone();
            Box::new(move |l: &Layer| built_in(&name, l).unwrap_or(false))
        } else if let Some(saved) = saved.into_iter().find(|s| s.name == p.set) {
            let keys: BTreeSet<String> = saved.layers.into_iter().collect();
            Box::new(move |l: &Layer| keys.contains(&color_key(l.kind, l.side)))
        } else {
            return Err(CommandError::invalid_params(
                "/set",
                format!("no Layer set is called {}", p.set),
            ));
        };
        let shown: Vec<Uuid> = set
            .layers
            .iter()
            .filter(|l| holds(l))
            .map(|l| l.uuid)
            .collect();
        let view = board_view(cx)?;
        view.layers.hidden = set
            .layers
            .iter()
            .map(|l| l.uuid)
            .filter(|l| !shown.contains(l))
            .collect();
        // A set without the Layer edited makes its first Layer active.
        let active = (view.layers.active)
            .filter(|l| set.layer(*l).is_some())
            .or_else(|| set.find(LayerKind::Copper, Some(Side::Top)).map(|l| l.uuid));
        if active.is_some_and(|a| !shown.contains(&a)) {
            view.layers.active = shown.first().copied();
        }
        Ok(LayersShown { layers: shown })
    }
}

/// A built-in set's name in the menus.
fn title(set: &str) -> &'static str {
    match set {
        "all" => "All",
        "signal" => "Signal",
        "non_signal" => "NonSignal",
        "mechanical" => "Mechanical",
        "top" => "Top",
        "bottom" => "Bottom",
        _ => "Fab",
    }
}

/// The Layer sets saved in the Preferences.
fn saved_sets(cx: &Context<'_>) -> Vec<SavedLayerSet> {
    cx.user_dirs()
        .map(|d| preferences::current(Some(d)).pcb.view.layer_sets)
        .unwrap_or_default()
}

fn write_sets(cx: &Context<'_>, sets: &[SavedLayerSet]) -> Result<(), CommandError> {
    let dirs = cx
        .user_dirs()
        .ok_or_else(|| CommandError::unavailable("there is no Preferences file to save to"))?;
    let value = serde_json::to_value(sets).expect("Layer sets serialise");
    preferences::set(dirs, "pcb.view.layer_sets", value)?;
    Ok(())
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerSetNameParams {
    /// The set's name.
    name: String,
}

/// A saved Layer set.
#[derive(Serialize, JsonSchema)]
pub struct SavedSet {
    pub name: String,
    /// Its Layers by kind and side, such as `copper_top`.
    pub layers: Vec<String>,
}

struct LayerSetSave;

impl Command for LayerSetSave {
    const META: Meta = Meta {
        id: "pcb.view.layer_set.save",
        title: "Save Layer Set",
        description: "Saves the Layers now shown as a named Layer set in the Preferences, by \
                      Layer kind and side, so it applies to any board. A set of the same \
                      name is replaced; the built-in names are refused.",
        category: "pcb_view",
        icon: "layers",
        kind: Kind::View,
    };
    type Params = LayerSetNameParams;
    type Output = SavedSet;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LayerSetNameParams) -> Result<SavedSet, CommandError> {
        let name = p.name.trim().to_owned();
        if name.is_empty() {
            return Err(CommandError::invalid_params(
                "/name",
                "a Layer set needs a name",
            ));
        }
        if LAYER_SETS.contains(&name.as_str()) {
            return Err(CommandError::invalid_params(
                "/name",
                format!("{name} is a built-in Layer set"),
            ));
        }
        let set = layer_set(cx)?;
        let hidden = board_view(cx)?.layers.hidden.clone();
        let layers: Vec<String> = set
            .layers
            .iter()
            .filter(|l| !hidden.contains(&l.uuid))
            .map(|l| color_key(l.kind, l.side))
            .collect();
        let mut sets = saved_sets(cx);
        sets.retain(|s| s.name != name);
        sets.push(SavedLayerSet {
            name: name.clone(),
            layers: layers.clone(),
        });
        write_sets(cx, &sets)?;
        Ok(SavedSet { name, layers })
    }
}

/// The name of the set deleted.
#[derive(Serialize, JsonSchema)]
pub struct Deleted {
    pub name: String,
}

struct LayerSetDelete;

impl Command for LayerSetDelete {
    const META: Meta = Meta {
        id: "pcb.view.layer_set.delete",
        title: "Delete Layer Set",
        description: "Deletes a Layer set saved in the Preferences. The built-in sets can't be \
                      deleted.",
        category: "pcb_view",
        icon: "delete",
        kind: Kind::View,
    };
    type Params = LayerSetNameParams;
    type Output = Deleted;

    fn run(&self, cx: &mut Context<'_>, p: LayerSetNameParams) -> Result<Deleted, CommandError> {
        if LAYER_SETS.contains(&p.name.as_str()) {
            return Err(CommandError::invalid_params(
                "/name",
                format!("{} is a built-in Layer set", p.name),
            ));
        }
        let mut sets = saved_sets(cx);
        let before = sets.len();
        sets.retain(|s| s.name != p.name);
        if sets.len() == before {
            return Err(CommandError::not_found(format!(
                "no saved Layer set is called {}",
                p.name
            )));
        }
        write_sets(cx, &sets)?;
        Ok(Deleted { name: p.name })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerPinParams {
    /// The Layer, by UUID.
    layer: Uuid,
    /// Pin or unpin; left out, it toggles.
    #[serde(default)]
    pinned: Option<bool>,
}

/// Whether the Layer is now pinned.
#[derive(Serialize, JsonSchema)]
pub struct Pinned {
    pub pinned: bool,
}

struct LayerPin;

impl Command for LayerPin {
    const META: Meta = Meta {
        id: "pcb.layer.pin",
        title: "Pin Layer",
        description: "Pins a Layer so it stays shown in single-Layer mode, such as the Board \
                      Outline (`Mod`+click on the Layer's eye). With `pinned` left out it \
                      toggles. Kept with the view.",
        category: "pcb_view",
        icon: "layer_visible",
        kind: Kind::View,
    };
    type Params = LayerPinParams;
    type Output = Pinned;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LayerPinParams) -> Result<Pinned, CommandError> {
        if layer_set(cx)?.layer(p.layer).is_none() {
            return Err(CommandError::invalid_params(
                "/layer",
                format!("{} is not a Layer of the board", p.layer),
            ));
        }
        let layers = &mut board_view(cx)?.layers;
        let pinned = p.pinned.unwrap_or(!layers.pinned.contains(&p.layer));
        if pinned {
            layers.pinned.insert(p.layer);
            layers.hidden.remove(&p.layer);
        } else {
            layers.pinned.remove(&p.layer);
        }
        Ok(Pinned { pinned })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectTypeSetParams {
    /// The kind of object: footprint_instance, track, via, zone, keepout, zone_cutout,
    /// free_pad, pcb_text or pcb_graphic.
    #[serde(rename = "type")]
    kind: ObjectType,
    /// 0 (opaque) to 100 percent; left out, unchanged.
    #[serde(default)]
    #[schemars(range(min = 0, max = 100))]
    transparency: Option<u8>,
    /// Outline only; left out, unchanged.
    #[serde(default)]
    draft: Option<bool>,
}

/// How the kind is now drawn.
#[derive(Serialize, JsonSchema)]
pub struct KindNow {
    pub transparency: u8,
    pub draft: bool,
}

struct ObjectTypeSet;

impl Command for ObjectTypeSet {
    const META: Meta = Meta {
        id: "pcb.view.object_type.set",
        title: "Object Type Display",
        description: "Sets how one kind of board object is drawn: its transparency, 0 to \
                      100 %, and Draft (outline only). Kept with the view.",
        category: "pcb_view",
        icon: "layers",
        kind: Kind::View,
    };
    type Params = ObjectTypeSetParams;
    type Output = KindNow;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ObjectTypeSetParams) -> Result<KindNow, CommandError> {
        if !VIEW_TYPES.contains(&p.kind) {
            return Err(CommandError::invalid_params(
                "/type",
                "only board objects have a per-kind display",
            ));
        }
        if p.transparency.is_some_and(|t| t > 100) {
            return Err(CommandError::invalid_params(
                "/transparency",
                "transparency is 0 to 100 percent",
            ));
        }
        let kinds = &mut board_view(cx)?.layers.kinds;
        let mut display = kinds.get(&p.kind).copied().unwrap_or_default();
        if let Some(t) = p.transparency {
            display.transparency = t;
        }
        if let Some(d) = p.draft {
            display.draft = d;
        }
        if display == KindDisplay::default() {
            kinds.remove(&p.kind);
        } else {
            kinds.insert(p.kind, display);
        }
        Ok(KindNow {
            transparency: display.transparency,
            draft: display.draft,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetColorsParams {
    /// Net colours on copper; left out, it toggles.
    #[serde(default)]
    on: Option<bool>,
}

/// Whether Net colours now show on copper.
#[derive(Serialize, JsonSchema)]
pub struct NetColorsNow {
    pub on: bool,
}

struct NetColors;

impl Command for NetColors {
    const META: Meta = Meta {
        id: "pcb.view.net_colors",
        title: "Net Colours on Copper",
        description: "Shows each coloured Net's colour on its copper too (Tracks, Pads, Vias \
                      and Zone fills), in the pattern and zoom-out behaviour the Preferences \
                      set. The Ratsnest always shows Net colours. With `on` left out it \
                      toggles.",
        category: "pcb_view",
        icon: "net",
        kind: Kind::View,
    };
    type Params = NetColorsParams;
    type Output = NetColorsNow;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: NetColorsParams) -> Result<NetColorsNow, CommandError> {
        let layers = &mut board_view(cx)?.layers;
        layers.net_colors = p.on.unwrap_or(!layers.net_colors);
        Ok(NetColorsNow {
            on: layers.net_colors,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LabelsParams {
    /// Net names on Pads; left out, unchanged.
    #[serde(default)]
    pad_nets: Option<bool>,
    /// Net names on Vias; left out, unchanged.
    #[serde(default)]
    via_nets: Option<bool>,
    /// Net names repeated along Tracks rather than once; left out, unchanged.
    #[serde(default)]
    repeated_net_names: Option<bool>,
}

/// The label toggles now.
#[derive(Serialize, JsonSchema)]
pub struct LabelsNow {
    pub pad_nets: bool,
    pub via_nets: bool,
    pub repeated_net_names: bool,
}

struct Labels;

impl Command for Labels {
    const META: Meta = Meta {
        id: "pcb.view.labels",
        title: "Net Labels",
        description: "Turns the view-only Net labels on or off: Pad Nets, Via Nets and \
                      Repeated Net Names on Tracks. Each label still hides below its minimum \
                      on-screen size. Kept with the view.",
        category: "pcb_view",
        icon: "net",
        kind: Kind::View,
    };
    type Params = LabelsParams;
    type Output = LabelsNow;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LabelsParams) -> Result<LabelsNow, CommandError> {
        let labels: &mut LabelView = &mut board_view(cx)?.layers.labels;
        if let Some(v) = p.pad_nets {
            labels.pad_nets = v;
        }
        if let Some(v) = p.via_nets {
            labels.via_nets = v;
        }
        if let Some(v) = p.repeated_net_names {
            labels.repeated_net_names = v;
        }
        Ok(LabelsNow {
            pad_nets: labels.pad_nets,
            via_nets: labels.via_nets,
            repeated_net_names: labels.repeated_net_names,
        })
    }
}
