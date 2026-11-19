//! Physical builds generated for Generic and for unspecified copper counts.
use reda_model::{
    Construction, ConstructionEntry, DEFAULT_BOARD_THICKNESS, DEFAULT_COPPER, DielectricKind,
    StackupSeeds, geom::Nm,
};

/// Preserve the automatically sized Core of a format-1 Stackup seed.
pub fn two_layer(seeds: StackupSeeds) -> Construction {
    with_core_gaps(2, seeds, |_| None)
}

/// Plain copper and Core gaps. At its four supported counts Generic specifies
/// the gaps in whole nanometres. Other counts use the Stackup's equal-share rule.
pub fn evenly_spaced(layers: u8, seeds: StackupSeeds) -> Construction {
    let fixed_gaps = matches!(layers, 2 | 4 | 6 | 8) && seeds.is_empty();
    let gaps = i64::from(layers) - 1;
    let insulation = DEFAULT_BOARD_THICKNESS.0 - i64::from(layers) * DEFAULT_COPPER.0;
    // Round successive boundaries to preserve the exact total and mirrored gaps.
    let boundary = |n: u8| (insulation * i64::from(n) + gaps / 2) / gaps;
    with_core_gaps(layers, seeds, |n| {
        fixed_gaps.then(|| Nm(boundary(n) - boundary(n - 1)))
    })
}

fn with_core_gaps(layers: u8, seeds: StackupSeeds, gap: impl Fn(u8) -> Option<Nm>) -> Construction {
    let copper = seeds.copper.unwrap_or(DEFAULT_COPPER);
    let mut entries = Vec::new();
    for n in 0..layers {
        if n > 0 {
            entries.push(ConstructionEntry::Dielectric {
                kind: DielectricKind::Core,
                name: None,
                thickness: gap(n),
                material: None,
                dk: None,
                df: None,
                test_frequency: None,
            });
        }
        entries.push(ConstructionEntry::Copper { copper });
    }
    Construction {
        name: format!("{layers} Layer"),
        default: true,
        notes: None,
        thickness: seeds.thickness.unwrap_or(DEFAULT_BOARD_THICKNESS),
        entries,
        via_types: vec![],
    }
}
