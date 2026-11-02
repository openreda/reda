//! The board's view state: the Layer state (kept per Document in `view.json`), the units
//! lengths show in and the snap grid. The PCB view Commands change it in the engine's
//! view state; the Shell hands it to the PCB tab.

use std::collections::HashMap;

use reda_engine::model::geom::Nm;
use reda_engine::model::{LayerKind, ObjectType, PcbDocument, Project, Side, Uuid};
use reda_engine::preferences::{InactiveLayerMode, Units};
use reda_engine::render::{self, Composite, InactiveLayers, Scene, SceneLayer, board_composites};

use crate::view_state::LayerView;

/// The metric snap-grid presets: 0.1, 0.25, 0.5 and 1 mm.
pub const MM_GRIDS: [Nm; 4] = [
    Nm::from_um(100),
    Nm::from_um(250),
    Nm::from_um(500),
    Nm::from_mm(1),
];
/// The imperial snap-grid presets: 5, 10, 25 and 50 mil.
pub const MIL_GRIDS: [Nm; 4] = [
    Nm::from_mil(5),
    Nm::from_mil(10),
    Nm::from_mil(25),
    Nm::from_mil(50),
];

/// The snap-grid presets of `units`, finest first.
pub fn grids(units: Units) -> &'static [Nm; 4] {
    match units {
        Units::Mm => &MM_GRIDS,
        Units::Mil => &MIL_GRIDS,
    }
}

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

/// The PCB view: the Layer state, units and snap grid.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardView {
    pub layers: LayerView,
    /// The units lengths show in.
    pub units: Units,
    /// The snap-grid pitch: one of [`grids`] for `units`.
    pub grid: Nm,
}

impl Default for BoardView {
    fn default() -> BoardView {
        BoardView::new(Units::Mm)
    }
}

impl BoardView {
    /// A fresh view in `units`, with the middle grid preset.
    pub fn new(units: Units) -> BoardView {
        BoardView {
            layers: LayerView::default(),
            units,
            grid: grids(units)[2],
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

    /// Switches units, keeping the grid at the same place in the other list.
    pub fn set_units(&mut self, units: Units) {
        let index = grids(self.units)
            .iter()
            .position(|g| *g == self.grid)
            .unwrap_or(2);
        self.units = units;
        self.grid = grids(units)[index];
    }

    /// Steps to the next grid preset of the current units, wrapping around.
    pub fn cycle_grid(&mut self) {
        let list = grids(self.units);
        let next = list
            .iter()
            .position(|g| *g == self.grid)
            .map_or(0, |i| (i + 1) % list.len());
        self.grid = list[next];
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
        if types.is_empty() && nets.is_empty() {
            return;
        }
        let mut kinds: HashMap<Uuid, Option<ObjectType>> = HashMap::new();
        scene.items.retain(|item| {
            if item.layer == SceneLayer::Ratsnest {
                return !nets.contains(&item.object);
            }
            if types.is_empty() {
                return true;
            }
            let kind = *kinds
                .entry(item.object)
                .or_insert_with(|| project.object(item.object).map(|o| o.object_type()));
            kind.is_none_or(|k| !types.contains(&k))
        });
    }
}

/// The renderer's Inactive-Layer mode for a Preference value.
pub fn inactive_layers(mode: InactiveLayerMode) -> InactiveLayers {
    match mode {
        InactiveLayerMode::Normal => InactiveLayers::Normal,
        InactiveLayerMode::Dimmed => InactiveLayers::Dimmed,
        InactiveLayerMode::Hidden => InactiveLayers::Hidden,
    }
}
