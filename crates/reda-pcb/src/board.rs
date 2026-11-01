//! What a new PCB Document holds: the fixed v0.1 Layer set, the board setup and the
//! Default Net Class.

use reda_model::geom::Nm;
use reda_model::{
    BoardSetup, DrcSeverities, InDocument, Layer, LayerKind, LayerSet, NetClass, Object, Rules,
    Side, Uuid,
};

/// The v0.1 Layers, top to bottom.
pub const V01_LAYERS: [(LayerKind, Option<Side>); 15] = {
    use LayerKind::*;
    const TOP: Option<Side> = Some(Side::Top);
    const BOTTOM: Option<Side> = Some(Side::Bottom);
    [
        (Courtyard, TOP),
        (Assembly, TOP),
        (Silkscreen, TOP),
        (Paste, TOP),
        (SolderMask, TOP),
        (Copper, TOP),
        (Copper, BOTTOM),
        (SolderMask, BOTTOM),
        (Paste, BOTTOM),
        (Silkscreen, BOTTOM),
        (Assembly, BOTTOM),
        (Courtyard, BOTTOM),
        (BoardOutline, None),
        (Mechanical, None),
        (Documentation, None),
    ]
};

/// The rules of the Generic Fab Preset: conservative 2-layer values every fab in the fab
/// research accepts. Fab Presets replace this seed when they arrive.
pub const GENERIC_RULES: Rules = Rules {
    clearance: Nm::from_um(160),
    track_width: Nm::from_um(200),
    via_diameter: Nm::from_um(700),
    via_hole: Nm::from_um(300),
    annular_ring: Nm::from_um(200),
    hole_to_hole: Nm::from_um(500),
    edge_clearance: Nm::from_um(400),
};

/// The name of the Net Class every new board starts with.
pub const DEFAULT_NET_CLASS: &str = "Default";

/// The objects of a new, empty board in PCB Document `document`: its Layers, its board
/// setup with `rules` and the default DRC severities, and the Default Net Class. `new_id`
/// supplies every UUID.
pub fn new_board(document: Uuid, rules: Rules, mut new_id: impl FnMut() -> Uuid) -> Vec<Object> {
    let layers = LayerSet {
        uuid: new_id(),
        layers: V01_LAYERS
            .iter()
            .map(|&(kind, side)| Layer {
                uuid: new_id(),
                kind,
                side,
            })
            .collect(),
    };
    let default = NetClass {
        uuid: new_id(),
        name: DEFAULT_NET_CLASS.into(),
        clearance: None,
        track_width: None,
        via_diameter: None,
        via_hole: None,
    };
    let setup = BoardSetup {
        uuid: new_id(),
        default_net_class: default.uuid,
        rules,
        severities: DrcSeverities::DEFAULT,
    };
    vec![
        Object::LayerSet(InDocument::new(document, layers)),
        Object::NetClass(InDocument::new(document, default)),
        Object::BoardSetup(InDocument::new(document, setup)),
    ]
}
