//! What a new PCB Document holds: the fixed v0.1 Layer set, the board setup and the
//! Default Net Class.

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

/// The name of the Net Class every new board starts with.
pub const DEFAULT_NET_CLASS: &str = "Default";

/// The objects of a new, empty board in PCB Document `document`: its Layers, its board
/// setup with `rules` (a Fab Preset's seed rules) and the default DRC severities, and the Default Net Class. `new_id`
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
