//! What a new PCB Document holds: the fixed v0.1 Layer set, its Stackup, the board setup
//! and the Default Net Class.

use reda_model::{
    BoardSetup, CopperEntry, DEFAULT_BOARD_THICKNESS, DEFAULT_COPPER, DielectricEntry,
    DrcSeverities, InDocument, Layer, LayerKind, LayerSet, NetClass, Object, Rules, Side, Stackup,
    StackupEntry, StackupSeeds, Uuid,
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

/// The objects of a new, empty board in PCB Document `document`: its Layers, its Stackup
/// from `stackup` (a Fab Preset's seeds, or 1.6 mm and 1 oz where it seeds nothing), its
/// board setup with `rules` (the preset's seed rules) and the default DRC severities, and
/// the Default Net Class. `new_id` supplies every UUID.
pub fn new_board(
    document: Uuid,
    rules: Rules,
    stackup: StackupSeeds,
    mut new_id: impl FnMut() -> Uuid,
) -> Vec<Object> {
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
    let stackup = seeded_stackup(&layers, stackup, &mut new_id);
    vec![
        Object::LayerSet(InDocument::new(document, layers)),
        Object::Stackup(InDocument::new(document, stackup)),
        Object::NetClass(InDocument::new(document, default)),
        Object::BoardSetup(InDocument::new(document, setup)),
    ]
}

/// A Stackup for `layers`: every copper Layer at the seeded copper, with one dielectric left
/// blank between each pair, so the dielectric takes what the copper leaves of the seeded
/// thickness.
fn seeded_stackup(
    layers: &LayerSet,
    seeds: StackupSeeds,
    new_id: &mut impl FnMut() -> Uuid,
) -> Stackup {
    let copper = seeds.copper.unwrap_or(DEFAULT_COPPER);
    let mut entries = Vec::new();
    for layer in layers.layers.iter().filter(|l| l.kind == LayerKind::Copper) {
        if !entries.is_empty() {
            entries.push(StackupEntry::Dielectric(DielectricEntry {
                uuid: new_id(),
                thickness: None,
            }));
        }
        entries.push(StackupEntry::Copper(CopperEntry {
            layer: layer.uuid,
            thickness: copper,
        }));
    }
    Stackup {
        uuid: new_id(),
        thickness: seeds.thickness.unwrap_or(DEFAULT_BOARD_THICKNESS),
        entries,
        finish: None,
        mask_color: None,
        silk_color: None,
    }
}
