//! What a new PCB Document holds: its copper and fabrication Layers, the board setup, the
//! board-wide rule records and the Stackup, seeded from a Fab Preset.

use reda_model::{
    BoardSetup, Bounds, Construction, DesignRule, DrcSeverities, FabPreset, FabRules, InDocument,
    Layer, LayerKind, LayerSet, Object, PcbDocument, PreferredSeeds, RuleField, RuleKind,
    RuleScope, RuleValues, Side, Stackup, StackupSeeds, Uuid, ZoneNameScheme,
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
    let bounds = |min| Bounds {
        min: Some(min),
        preferred: None,
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
                diameter: Some(bounds(rules.via_diameter)),
                hole: Some(bounds(rules.via_hole)),
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

/// Silk rules have deterministic format-2 identities. Unspecified fabrication
/// limits produce the zero minimum used when migrating an existing board.
pub fn silk_rules(rules: &FabRules) -> [(RuleKind, RuleValues); 2] {
    [
        (RuleKind::SilkWidth, rules.silk_width),
        (RuleKind::SilkTextHeight, rules.silk_text_height),
    ]
    .map(|(kind, limit)| {
        (
            kind,
            RuleValues {
                min: Some(limit.unwrap_or_default()),
                ..Default::default()
            },
        )
    })
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
                via: Some(reda_model::ViaRuleValues::Connection(VIA_CONNECTION_SEED)),
                ..RuleValues::default()
            },
        ),
    ]
}

/// The additional board-wide records introduced by format 2. Existing boards had no
/// silk minimum. A zero minimum preserves their behaviour until a fab limit is applied.
pub fn multilayer_rules(document: Uuid) -> Vec<DesignRule> {
    let target = reda_model::TeardropTarget::default();
    let silk = RuleValues {
        min: Some(reda_model::geom::Nm::ZERO),
        ..Default::default()
    };
    [
        (RuleKind::ViaSpan, RuleValues::default()),
        (RuleKind::MicroviaSize, RuleValues::default()),
        (
            RuleKind::Teardrop,
            RuleValues {
                through_hole_pad: Some(target),
                smd_pad: Some(target),
                via: Some(reda_model::ViaRuleValues::Teardrop(target)),
                width_change: Some(target),
                ..Default::default()
            },
        ),
        (RuleKind::SilkWidth, silk.clone()),
        (RuleKind::SilkTextHeight, silk),
    ]
    .into_iter()
    .map(|(kind, values)| DesignRule {
        uuid: document.child(kind.id()),
        kind,
        scope: RuleScope::BOARD,
        values,
        enabled: true,
        comment: None,
    })
    .collect()
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
    let mask = &pcb.board_rule(RuleKind::MaskExpansion).values;
    let paste = &pcb.board_rule(RuleKind::PasteExpansion).values;
    let inner_limit = |kind, field| {
        pcb.scoped_rule(kind, &RuleScope::INNER)
            .and_then(|r| r.values.length(field))
    };
    let inner = reda_model::InnerFabRules {
        track_width_min: inner_limit(RuleKind::TrackWidth, Min),
        clearance: inner_limit(RuleKind::Clearance, Value),
        annular_ring_min: inner_limit(RuleKind::AnnularRing, Min),
    };
    FabRules {
        inner: (inner != Default::default()).then_some(inner),
        clearance_hole: reda_model::HoleClearance::GENERIC,
        silk_width: pcb
            .board_rule(RuleKind::SilkWidth)
            .values
            .min
            .filter(|v| v.0 > 0),
        silk_text_height: pcb
            .board_rule(RuleKind::SilkTextHeight)
            .values
            .min
            .filter(|v| v.0 > 0),
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
            via_diameter: pcb.stackup().through_type().diameter.start,
            via_hole: pcb.stackup().through_type().hole,
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
    new_id: impl FnMut() -> Uuid,
) -> Vec<Object> {
    new_board_with_layers(document, rules, stackup, 2, new_id).expect("two copper Layers")
}

/// Build a board with the requested copper count. Until a matching Construction is
/// selected, multilayer boards use Generic copper and finished thickness.
pub fn new_board_with_layers(
    document: Uuid,
    rules: &FabRules,
    stackup: StackupSeeds,
    layers: u8,
    new_id: impl FnMut() -> Uuid,
) -> Result<Vec<Object>, reda_command::CommandError> {
    build_board(document, rules, stackup, layers, None, new_id)
}

/// Seed the rules and select the physical build for a new PCB Document.
pub fn new_board_from_preset(
    document: Uuid,
    preset: &FabPreset,
    layers: u8,
    name: Option<&str>,
    new_id: impl FnMut() -> Uuid,
) -> Result<Vec<Object>, reda_command::CommandError> {
    let construction = if let Some(name) = name {
        let build = preset
            .construction(name)
            .filter(|c| c.layer_count() == usize::from(layers))
            .ok_or_else(|| {
                reda_command::CommandError::invalid_params(
                    "/construction",
                    format!(
                        "{} has no {layers}-Layer Construction named {name:?}",
                        preset.name
                    ),
                )
            })?;
        Some(build)
    } else {
        preset.default_construction(usize::from(layers))
    };
    build_board(
        document,
        &preset.rules,
        StackupSeeds::default(),
        layers,
        construction,
        new_id,
    )
}

fn build_board(
    document: Uuid,
    rules: &FabRules,
    stackup: StackupSeeds,
    layers: u8,
    construction: Option<&Construction>,
    mut new_id: impl FnMut() -> Uuid,
) -> Result<Vec<Object>, reda_command::CommandError> {
    if !(2..=32).contains(&layers) {
        return Err(reda_command::CommandError::invalid_params(
            "/layers",
            "choose 2 to 32 copper Layers",
        ));
    }
    let stackup = if layers == 2 {
        stackup
    } else {
        StackupSeeds::default()
    };
    let mut kinds = V01_LAYERS.to_vec();
    let middle = kinds
        .iter()
        .position(|l| *l == (LayerKind::Copper, Some(Side::Bottom)))
        .unwrap();
    kinds.splice(
        middle..middle,
        std::iter::repeat_n((LayerKind::Copper, None), usize::from(layers - 2)),
    );
    let layers = LayerSet {
        uuid: new_id(),
        layers: kinds
            .iter()
            .map(|&(kind, side)| Layer {
                uuid: new_id(),
                kind,
                side,
                name: None,
                role: Default::default(),
            })
            .collect(),
    };
    let setup = BoardSetup {
        uuid: new_id(),
        severities: DrcSeverities::DEFAULT,
        zone_name_scheme: ZoneNameScheme::default(),
        fab_notes: None,
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
    objects.extend(multilayer_rules(document).into_iter().map(|mut rule| {
        if let Some((_, values)) = silk_rules(rules)
            .into_iter()
            .find(|(kind, _)| *kind == rule.kind)
        {
            rule.values = values;
        }
        Object::DesignRule(InDocument::new(document, rule))
    }));
    for (kind, field, min) in rules.inner.unwrap_or_default().limits() {
        if let Some(value) = min {
            let mut values = RuleValues::default();
            values
                .set(field, Some(reda_model::RuleValue::Length(value)))
                .expect("inner limit is a length");
            let rule = DesignRule {
                uuid: new_id(),
                kind,
                scope: RuleScope::INNER,
                values,
                enabled: true,
                comment: None,
            };
            objects.push(Object::DesignRule(InDocument::new(document, rule)));
        }
    }
    let stackup = seeded_stackup(&layers, stackup, rules.preferred, construction, &mut new_id)
        .map_err(|why| reda_command::CommandError::invalid_params("/construction", why))?;
    objects.push(Object::Stackup(InDocument::new(document, stackup)));
    Ok(objects)
}

/// A Stackup for `layers`: every copper Layer at the seeded copper, with one dielectric left
/// blank between each pair, so the dielectric takes what the copper leaves of the seeded
/// thickness.
fn seeded_stackup(
    layers: &LayerSet,
    seeds: StackupSeeds,
    preferred: PreferredSeeds,
    construction: Option<&Construction>,
    new_id: &mut impl FnMut() -> Uuid,
) -> Result<Stackup, String> {
    let copper: Vec<_> = layers
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Copper)
        .map(|l| l.uuid)
        .collect();
    let fallback;
    let build = match construction {
        Some(build) => build,
        None => {
            fallback = Construction::evenly_spaced(copper.len() as u8, seeds);
            &fallback
        }
    };
    let mut stack =
        build.board_stackup(&copper, preferred.via_hole, preferred.via_diameter, new_id)?;
    stack.construction = construction.map(|c| c.name.clone());
    Ok(stack)
}
