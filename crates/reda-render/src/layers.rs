//! Which Layers a PCB view draws, in what order and at what opacity.
//!
//! Each Layer is drawn opaque into a target of its own, then blended over what is below at
//! its opacity ([`Composite`]). Layers go bottom to top in physical order, the active Layer
//! last; the view from the bottom reverses the physical order (and mirrors the view). The
//! Layers that are not physical (Board Outline, Mechanical, Documentation) follow, then the
//! drilled holes, then the Ratsnest while it is visible.

use reda_model::{Layer, LayerKind, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::scene::SceneLayer;

/// How Layers other than the active one are drawn. A key cycles through the three.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InactiveLayers {
    /// At [`NORMAL_OPACITY`].
    #[default]
    Normal,
    /// At [`DIMMED_OPACITY`].
    Dimmed,
    /// Not drawn: single-Layer mode.
    Hidden,
}

/// The opacity of inactive Layers in [`InactiveLayers::Normal`] mode.
pub const NORMAL_OPACITY: f32 = 0.7;
/// The opacity of inactive Layers in [`InactiveLayers::Dimmed`] mode.
pub const DIMMED_OPACITY: f32 = 0.25;

/// What a PCB view shows of the board's Layers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayerView {
    /// The Layers shown, by UUID; `None` shows every Layer.
    pub visible: Option<Vec<Uuid>>,
    /// The Layer edited, drawn last at full opacity. With none, every Layer is inactive.
    pub active: Option<Uuid>,
    pub inactive: InactiveLayers,
    /// Seen from the bottom: the physical order is reversed (and the view mirrored).
    pub from_bottom: bool,
    /// Whether the Ratsnest is drawn.
    pub ratsnest: bool,
}

/// One Layer to draw, opaque into its own target, then blended at `opacity`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Composite {
    pub layer: SceneLayer,
    /// 0 (invisible) to 1 (opaque).
    pub opacity: f32,
}

impl Composite {
    pub const fn opaque(layer: SceneLayer) -> Composite {
        Composite {
            layer,
            opacity: 1.0,
        }
    }
}

/// The Layers of a board with `layers` (its Layer set, top to bottom) to draw for `view`,
/// bottom first.
pub fn board_composites(layers: &[Layer], view: &LayerView) -> Vec<Composite> {
    let shown = |l: &&Layer| view.visible.as_ref().is_none_or(|v| v.contains(&l.uuid));
    let physical = |l: &&Layer| l.side.is_some() || l.kind == LayerKind::Copper;
    // Top to bottom; drawing goes the other way.
    let mut physical: Vec<&Layer> = layers.iter().filter(physical).collect();
    if !view.from_bottom {
        physical.reverse();
    }
    let others = layers.iter().filter(|l| !physical.contains(l));
    let mut order: Vec<&Layer> = physical
        .iter()
        .copied()
        .chain(others)
        .filter(shown)
        .collect();
    if let Some(i) = order.iter().position(|l| Some(l.uuid) == view.active) {
        let active = order.remove(i);
        order.push(active);
    }
    let inactive = match view.inactive {
        InactiveLayers::Normal => Some(NORMAL_OPACITY),
        InactiveLayers::Dimmed => Some(DIMMED_OPACITY),
        InactiveLayers::Hidden => None,
    };
    let mut out: Vec<Composite> = order
        .into_iter()
        .filter_map(|l| {
            let opacity = if Some(l.uuid) == view.active {
                1.0
            } else {
                inactive?
            };
            Some(Composite {
                layer: SceneLayer::Board(l.uuid),
                opacity,
            })
        })
        .collect();
    out.push(Composite::opaque(SceneLayer::Drill));
    if view.ratsnest {
        out.push(Composite::opaque(SceneLayer::Ratsnest));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use reda_model::Side;

    fn layer(n: u128, kind: LayerKind, side: Option<Side>) -> Layer {
        Layer {
            uuid: Uuid::from_u128(n),
            kind,
            side,
        }
    }

    /// Top silkscreen, top copper, bottom copper, bottom silkscreen, Board Outline.
    fn board() -> Vec<Layer> {
        vec![
            layer(1, LayerKind::Silkscreen, Some(Side::Top)),
            layer(2, LayerKind::Copper, Some(Side::Top)),
            layer(3, LayerKind::Copper, Some(Side::Bottom)),
            layer(4, LayerKind::Silkscreen, Some(Side::Bottom)),
            layer(5, LayerKind::BoardOutline, None),
        ]
    }

    fn plan(view: &LayerView) -> Vec<(String, f32)> {
        board_composites(&board(), view)
            .into_iter()
            .map(|c| {
                let name = match c.layer {
                    SceneLayer::Board(u) => (u.as_u128()).to_string(),
                    SceneLayer::Drill => "drill".into(),
                    SceneLayer::Ratsnest => "ratsnest".into(),
                    SceneLayer::Schematic => "sch".into(),
                };
                (name, c.opacity)
            })
            .collect()
    }

    fn names(view: &LayerView) -> Vec<String> {
        plan(view).into_iter().map(|(n, _)| n).collect()
    }

    #[test]
    fn layers_go_bottom_to_top_with_the_active_layer_last() {
        let view = LayerView {
            active: Some(Uuid::from_u128(3)),
            ratsnest: true,
            ..LayerView::default()
        };
        assert_eq!(names(&view), ["4", "2", "1", "5", "3", "drill", "ratsnest"]);
        let opacities: Vec<f32> = plan(&view).into_iter().map(|(_, o)| o).collect();
        assert_eq!(
            opacities,
            [
                NORMAL_OPACITY,
                NORMAL_OPACITY,
                NORMAL_OPACITY,
                NORMAL_OPACITY,
                1.0,
                1.0,
                1.0
            ]
        );
    }

    #[test]
    fn the_view_from_the_bottom_reverses_the_physical_order() {
        let view = LayerView {
            from_bottom: true,
            ..LayerView::default()
        };
        assert_eq!(names(&view), ["1", "2", "3", "4", "5", "drill"]);
    }

    #[test]
    fn inactive_layers_can_be_dimmed_or_hidden() {
        let mut view = LayerView {
            active: Some(Uuid::from_u128(2)),
            inactive: InactiveLayers::Dimmed,
            ..LayerView::default()
        };
        assert_eq!(plan(&view)[0], ("4".to_owned(), DIMMED_OPACITY));
        view.inactive = InactiveLayers::Hidden;
        assert_eq!(names(&view), ["2", "drill"]);
    }

    #[test]
    fn only_visible_layers_are_drawn() {
        let view = LayerView {
            visible: Some(vec![Uuid::from_u128(2), Uuid::from_u128(5)]),
            ..LayerView::default()
        };
        assert_eq!(names(&view), ["2", "5", "drill"]);
    }
}
