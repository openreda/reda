//! What a new PCB Document holds: the fixed v0.1 Layer set, the board setup, the
//! board-wide rule records and the Stackup, seeded from a Fab Preset.

use reda_model::{
    BoardSetup, Bounds, CopperEntry, DEFAULT_BOARD_THICKNESS, DEFAULT_COPPER, DesignRule,
    DielectricEntry, DrcSeverities, FabRules, InDocument, Layer, LayerKind, LayerSet, Object,
    PcbDocument, PreferredSeeds, RuleField, RuleKind, RuleScope, RuleValues, Side, Stackup,
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
        (
            RuleKind::MaskExpansion,
            RuleValues {
                top: Some(rules.mask_expansion.top),
                bottom: Some(rules.mask_expansion.bottom),
                from_hole_edge: Some(rules.mask_expansion.from_hole_edge),
                tent_vias_top: Some(rules.mask_expansion.tent_vias_top),
                tent_vias_bottom: Some(rules.mask_expansion.tent_vias_bottom),
                ..RuleValues::default()
            },
        ),
        (
            RuleKind::PasteExpansion,
            match rules.paste_expansion {
                reda_model::PasteExpansion::Expansion { expansion } => RuleValues {
                    expansion: Some(expansion),
                    ..RuleValues::default()
                },
                reda_model::PasteExpansion::Percent { percent } => RuleValues {
                    percent: Some(percent),
                    ..RuleValues::default()
                },
            },
        ),
    ]
}

/// The board-wide values of the rule kinds no Fab Preset seeds, which start at fixed
/// defaults: silkscreen 0.254 mm from the edge and 0.10 mm from exposed copper, courtyards
/// and the dangling tolerance at 0, micro-segments up to 0.01 mm, dead copper above
/// 100 sq mil, and the Zone connection seeds.
pub fn default_rules() -> Vec<(RuleKind, RuleValues)> {
    use reda_model::geom::Nm;
    let value = |value| RuleValues {
        value: Some(value),
        ..RuleValues::default()
    };
    vec![
        (RuleKind::SilkToEdge, value(Nm(254_000))),
        (
            RuleKind::SilkToMask,
            RuleValues {
                value: Some(Nm(100_000)),
                mode: Some(reda_model::SilkMaskMode::ExposedCopper),
                ..RuleValues::default()
            },
        ),
        (RuleKind::CourtyardClearance, value(Nm::ZERO)),
        (
            RuleKind::Dangling,
            RuleValues {
                tolerance: Some(Nm::ZERO),
                ..RuleValues::default()
            },
        ),
        (
            RuleKind::MicroSegment,
            RuleValues {
                max_length: Some(Nm(10_000)),
                ..RuleValues::default()
            },
        ),
        (
            RuleKind::DeadCopper,
            RuleValues {
                min_area: Some(DEAD_COPPER_MIN_AREA),
                ..RuleValues::default()
            },
        ),
        (
            RuleKind::ZoneConnection,
            RuleValues {
                through_hole: Some(PAD_CONNECTION_SEED),
                smd: Some(PAD_CONNECTION_SEED),
                via: Some(VIA_CONNECTION_SEED),
                ..RuleValues::default()
            },
        ),
    ]
}

/// The seeded `dead_copper` area: 100 square mil (0.0645 mm²), in square nanometres.
pub const DEAD_COPPER_MIN_AREA: i64 = 25_400 * 25_400 * 100;

/// The seeded Zone connection of through-hole and SMD Pads: relief, 4 spokes 0.25 mm wide
/// across a 0.25 mm gap, along the Pad's axes.
pub const PAD_CONNECTION_SEED: reda_model::ZoneConnection = reda_model::ZoneConnection {
    style: Some(reda_model::ConnectionStyle::Relief),
    spokes: Some(4),
    spoke_width: Some(reda_model::geom::Nm(250_000)),
    gap: Some(reda_model::geom::Nm(250_000)),
    angle: Some(reda_model::geom::Millideg(0)),
};

/// The seeded Zone connection of Vias: solid, with the Pads' spoke values.
pub const VIA_CONNECTION_SEED: reda_model::ZoneConnection = reda_model::ZoneConnection {
    style: Some(reda_model::ConnectionStyle::Solid),
    ..PAD_CONNECTION_SEED
};

/// The Fab Preset rules that seed a board like `pcb`'s board-wide records: their limits,
/// preferred sizes and output seeds, whether enabled or not. Track and Via maximums and
/// Net Class records have no place in a preset and are left out.
pub fn board_fab_rules(pcb: &PcbDocument) -> FabRules {
    use RuleField::*;
    let length = |kind: RuleKind, field: RuleField| {
        pcb.board_rule(kind)
            .values
            .length(field)
            .expect("the board-wide record holds every required value")
    };
    let mask = pcb.board_rule(RuleKind::MaskExpansion).values;
    let paste = pcb.board_rule(RuleKind::PasteExpansion).values;
    FabRules {
        clearance: length(RuleKind::Clearance, Value),
        track_width: length(RuleKind::TrackWidth, Min),
        via_diameter: length(RuleKind::ViaSize, DiameterMin),
        via_hole: length(RuleKind::ViaSize, HoleMin),
        hole_min: length(RuleKind::HoleSize, Min),
        hole_max: length(RuleKind::HoleSize, Max),
        annular_ring: length(RuleKind::AnnularRing, Min),
        hole_to_hole: length(RuleKind::HoleToHole, Min),
        edge_clearance: length(RuleKind::EdgeClearance, Value),
        preferred: PreferredSeeds {
            track_width: length(RuleKind::TrackWidth, Preferred),
            via_diameter: length(RuleKind::ViaSize, DiameterPreferred),
            via_hole: length(RuleKind::ViaSize, HolePreferred),
        },
        mask_expansion: reda_model::MaskExpansion {
            top: mask.top.unwrap_or_default(),
            bottom: mask.bottom.unwrap_or_default(),
            from_hole_edge: mask.from_hole_edge.unwrap_or_default(),
            tent_vias_top: mask.tent_vias_top.unwrap_or_default(),
            tent_vias_bottom: mask.tent_vias_bottom.unwrap_or_default(),
        },
        paste_expansion: match paste.percent {
            Some(percent) => reda_model::PasteExpansion::Percent { percent },
            None => reda_model::PasteExpansion::Expansion {
                expansion: paste.expansion.unwrap_or_default(),
            },
        },
    }
}

/// The objects of a new, empty board in PCB Document `document`: its Layers, its board
/// setup with the default DRC severities, and one board-wide record per rule kind seeded
/// from `rules` (a Fab Preset's), and its Stackup from `stackup` (the preset's seeds, or
/// 1.6 mm and 1 oz where it seeds nothing). `new_id` supplies every UUID.
pub fn new_board(
    document: Uuid,
    rules: &FabRules,
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
    let setup = BoardSetup {
        uuid: new_id(),
        severities: DrcSeverities::DEFAULT,
    };
    let mut objects = vec![
        Object::LayerSet(InDocument::new(document, layers.clone())),
        Object::BoardSetup(InDocument::new(document, setup)),
    ];
    // The kinds no preset seeds come last, so the preset's records keep their UUIDs.
    for (kind, values) in seed_rules(rules).into_iter().chain(default_rules()) {
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
    let stackup = seeded_stackup(&layers, stackup, &mut new_id);
    objects.push(Object::Stackup(InDocument::new(document, stackup)));
    objects
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
