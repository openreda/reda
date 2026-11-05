//! The board's view state: the Layer state (kept per Document in `view.json`) and the units
//! lengths show in (the snap grid is per Document, in [`crate::grid`]). The PCB view Commands
//! change it in the engine's view state; the Shell hands it to the PCB tab.

use std::collections::HashMap;
use std::sync::Arc;

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{LayerKind, ObjectType, PcbDocument, Project, Side, Uuid};
use reda_engine::preferences::{InactiveLayerMode, Preferences, Units};
use reda_engine::render::{
    self, BoardColor, ColorKey, Composite, InactiveLayers, Rgba, Scene, SceneLayer,
    board_composites,
};

use crate::canvas::PaletteHandle;
use crate::view_state::LayerView;

/// The kinds of board object the Layers panel can hide, in its order.
pub const VIEW_TYPES: [ObjectType; 8] = [
    ObjectType::FootprintInstance,
    ObjectType::Track,
    ObjectType::Via,
    ObjectType::Zone,
    ObjectType::Keepout,
    ObjectType::FreePad,
    ObjectType::PcbText,
    ObjectType::PcbGraphic,
];

/// The PCB view: the Layer state and units.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardView {
    pub layers: LayerView,
    /// The units lengths show in.
    pub units: Units,
    /// The user origin the cursor position reads from (`pcb.view.origin.set`); `None`
    /// is the board's own origin.
    pub origin: Option<Point>,
}

impl Default for BoardView {
    fn default() -> BoardView {
        BoardView::new(Units::Mm)
    }
}

impl BoardView {
    /// A fresh view in `units`.
    pub fn new(units: Units) -> BoardView {
        BoardView {
            layers: LayerView::default(),
            units,
            origin: None,
        }
    }

    /// The Layer edited: the chosen one while the board has it, else the top copper.
    pub fn active_layer(&self, project: &Project) -> Option<Uuid> {
        let set = project.pcb()?.layers();
        self.layers
            .active
            .filter(|l| set.layer(*l).is_some())
            .or_else(|| set.find(LayerKind::Copper, Some(Side::Top)).map(|l| l.uuid))
    }

    /// The Layers to composite for `pcb`, bottom first; `default_mode` is the
    /// Preference used while the view has no Inactive-Layer mode of its own.
    pub fn composites(&self, pcb: &PcbDocument, default_mode: InactiveLayerMode) -> Vec<Composite> {
        let set = pcb.layers();
        let active = self
            .layers
            .active
            .filter(|l| set.layer(*l).is_some())
            .or_else(|| set.find(LayerKind::Copper, Some(Side::Top)).map(|l| l.uuid));
        let view = render::LayerView {
            visible: Some(
                set.layers
                    .iter()
                    .map(|l| l.uuid)
                    .filter(|l| !self.layers.hidden.contains(l))
                    .collect(),
            ),
            active,
            inactive: inactive_layers(self.layers.inactive_mode.unwrap_or(default_mode)),
            from_bottom: self.layers.from_bottom,
            ratsnest: !self.layers.ratsnest.hidden,
        };
        board_composites(&set.layers, &view)
    }

    /// Drops from `scene` what this view hides: objects of hidden kinds and the Ratsnest
    /// lines of hidden Nets.
    pub fn filter_scene(&self, project: &Project, scene: &mut Scene) {
        let types = &self.layers.hidden_types;
        let nets = &self.layers.ratsnest.hidden_nets;
        // PROTOTYPE: the Ratsnest is painted as a fixed-width overlay (`route::draw`).
        let _ = nets;
        scene.items.retain(|item| item.layer != SceneLayer::Ratsnest);
        if types.is_empty() {
            return;
        }
        let mut kinds: HashMap<Uuid, Option<ObjectType>> = HashMap::new();
        scene.items.retain(|item| {
            let kind = *kinds
                .entry(item.object)
                .or_insert_with(|| project.object(item.object).map(|o| o.object_type()));
            kind.is_none_or(|k| !types.contains(&k))
        });
    }
}

/// `length` in `units` with up to 4 decimals (mm) or 2 (mil), trailing zeros dropped,
/// and the unit: `12.5 mm`, `492.13 mil`.
pub fn format_length(length: Nm, units: Units) -> String {
    let (value, decimals, unit) = match units {
        Units::Mm => (length.0 as f64 / 1e6, 4, "mm"),
        Units::Mil => (length.0 as f64 / 25_400.0, 2, "mil"),
    };
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    format!("{text} {unit}")
}

/// The `display.layer_colors.<theme>.<key>` key of a Layer's colour: its kind and side,
/// such as `copper_top` or `board_outline`.
pub fn color_key(kind: LayerKind, side: Option<Side>) -> String {
    let kind = serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    match side {
        Some(Side::Top) => format!("{kind}_top"),
        Some(Side::Bottom) => format!("{kind}_bottom"),
        None => kind,
    }
}

/// `#rrggbb` as a colour.
pub fn parse_hex(text: &str) -> Option<Rgba> {
    if !reda_engine::preferences::is_hex_color(text) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&text[i..i + 2], 16).ok();
    Some(Rgba::rgb(byte(1)?, byte(3)?, byte(5)?))
}

/// A colour as `#rrggbb`.
pub fn to_hex(color: Rgba) -> String {
    let [r, g, b, _] = color.0;
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The Layer colour overrides in `prefs`, for the canvas.
pub fn palette_handle(prefs: &Preferences, generation: u64) -> PaletteHandle {
    let overrides = |colors: &reda_engine::preferences::LayerColors| -> Arc<[(ColorKey, Rgba)]> {
        let mut out = Vec::new();
        for (key, text) in colors.overrides() {
            let Some(color) = parse_hex(text) else {
                continue;
            };
            for kind in LAYER_KINDS {
                for side in [Some(Side::Top), Some(Side::Bottom), None] {
                    if color_key(kind, side) == key {
                        out.push((ColorKey::Board(BoardColor::layer(kind, side)), color));
                    }
                }
            }
        }
        out.into()
    };
    let colors = &prefs.display.layer_colors;
    PaletteHandle {
        generation,
        dark: overrides(&colors.dark),
        light: overrides(&colors.light),
    }
}

/// Every Layer kind.
const LAYER_KINDS: [LayerKind; 9] = [
    LayerKind::Copper,
    LayerKind::Silkscreen,
    LayerKind::SolderMask,
    LayerKind::Paste,
    LayerKind::Courtyard,
    LayerKind::Assembly,
    LayerKind::BoardOutline,
    LayerKind::Mechanical,
    LayerKind::Documentation,
];

/// The renderer's Inactive-Layer mode for a Preference value.
pub fn inactive_layers(mode: InactiveLayerMode) -> InactiveLayers {
    match mode {
        InactiveLayerMode::Normal => InactiveLayers::Normal,
        InactiveLayerMode::Dimmed => InactiveLayers::Dimmed,
        InactiveLayerMode::Hidden => InactiveLayers::Hidden,
    }
}
