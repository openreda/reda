//! What a new PCB Document holds: the fixed v0.1 Layer set, the board setup and the
//! board-wide rule records, seeded from a Fab Preset.

use reda_model::{
    BoardSetup, Bounds, DesignRule, DrcSeverities, FabRules, InDocument, Layer, LayerKind,
    LayerSet, Object, RuleKind, RuleScope, RuleValues, Side, Uuid,
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

/// The name of the Default Net Class, which every Net is in and which is not stored.
pub const DEFAULT_NET_CLASS: &str = "Default";

/// The board-wide values of every rule kind, seeded from a Fab Preset's rules: its limits
/// as the minimums (and the hole maximum), its preferred seeds, Track and Via maximums
/// empty.
pub fn seed_rules(rules: &FabRules) -> Vec<(RuleKind, RuleValues)> {
    let min = |min: reda_model::geom::Nm| RuleValues {
        min: Some(min),
        ..RuleValues::default()
    };
    let value = |value| RuleValues {
        value: Some(value),
        ..RuleValues::default()
    };
    let bounds = |min, preferred| Bounds {
        min: Some(min),
        preferred: Some(preferred),
        max: None,
    };
    let p = rules.preferred;
    vec![
        (RuleKind::Clearance, value(rules.clearance)),
        (
            RuleKind::TrackWidth,
            RuleValues {
                min: Some(rules.track_width),
                preferred: Some(p.track_width),
                ..RuleValues::default()
            },
        ),
        (
            RuleKind::ViaSize,
            RuleValues {
                diameter: Some(bounds(rules.via_diameter, p.via_diameter)),
                hole: Some(bounds(rules.via_hole, p.via_hole)),
                ..RuleValues::default()
            },
        ),
        (
            RuleKind::HoleSize,
            RuleValues {
                min: Some(rules.hole_min),
                max: Some(rules.hole_max),
                ..RuleValues::default()
            },
        ),
        (RuleKind::AnnularRing, min(rules.annular_ring)),
        (RuleKind::HoleToHole, min(rules.hole_to_hole)),
        (RuleKind::EdgeClearance, value(rules.edge_clearance)),
    ]
}

/// The objects of a new, empty board in PCB Document `document`: its Layers, its board
/// setup with the default DRC severities, and one board-wide record per rule kind seeded
/// from `rules` (a Fab Preset's). `new_id` supplies every UUID.
pub fn new_board(
    document: Uuid,
    rules: &FabRules,
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
    let setup = BoardSetup {
        uuid: new_id(),
        severities: DrcSeverities::DEFAULT,
    };
    let mut objects = vec![
        Object::LayerSet(InDocument::new(document, layers)),
        Object::BoardSetup(InDocument::new(document, setup)),
    ];
    for (kind, values) in seed_rules(rules) {
        let rule = DesignRule {
            uuid: new_id(),
            kind,
            scope: RuleScope::BOARD,
            values,
            enabled: true,
            comment: None,
        };
        objects.push(Object::DesignRule(InDocument::new(document, rule)));
    }
    objects
}
