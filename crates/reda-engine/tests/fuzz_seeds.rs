//! The `import_kicad` fuzz target still runs: its check (shared with the fuzz target
//! through `include!`) passes on every seed, and on cut-short copies of each, which must be
//! refused or imported but never panic. Fuzzing itself needs a nightly toolchain and runs
//! weekly; this keeps the target's code compiling and its seeds importing on every build.

use std::path::PathBuf;

use reda_engine::model::{FootprintLayer, InsideEdge, OutsideEdge, Shape};

include!("../../../fuzz/fuzz_targets/import_kicad_check.rs");

fn seeds() -> Vec<(PathBuf, Vec<u8>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/seeds/import_kicad");
    let mut seeds: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    seeds.sort();
    seeds
}

#[test]
fn every_import_kicad_seed_imports_and_round_trips_or_is_refused() {
    let seeds = seeds();
    assert!(seeds.len() >= 4, "{seeds:?}");
    for (path, bytes) in &seeds {
        // `two_symbols` holds several symbols, so an import without a pick is refused.
        let refused = path.ends_with("two_symbols");
        assert_eq!(check(bytes).is_none(), refused, "{}", path.display());
    }
}

#[test]
fn cut_short_seeds_are_refused_or_imported_without_a_panic() {
    for (_, bytes) in seeds() {
        for end in (0..bytes.len()).step_by(29) {
            check(&bytes[..end]);
        }
    }
}

/// The `mapping` seed covers the mapping table, so fuzzing starts from those paths.
#[test]
fn the_mapping_seed_reaches_the_mapping_table() {
    let (_, bytes) = seeds()
        .into_iter()
        .find(|(p, _)| p.ends_with("mapping"))
        .expect("the mapping seed");
    let out = check(&bytes).unwrap();
    assert!(!out.part.in_bom && !out.part.in_cpl);
    assert!(
        out.symbol
            .pins
            .iter()
            .all(|p| p.name_visible && p.number_visible)
    );
    assert!(
        out.symbol
            .pins
            .iter()
            .any(|p| p.inside_edge == Some(InsideEdge::Clock))
    );
    assert!(
        out.symbol
            .pins
            .iter()
            .any(|p| p.outside_edge == Some(OutsideEdge::ActiveLowOutput))
    );
    let courtyard: Vec<_> = out
        .footprint
        .graphics
        .iter()
        .filter(|g| g.layer == FootprintLayer::CourtyardTop)
        .collect();
    assert!(
        matches!(courtyard[..], [g] if matches!(g.shape, Shape::Polygon { .. })),
        "{courtyard:?}"
    );
    assert!(
        out.footprint
            .texts
            .iter()
            .any(|t| t.field == "Designator" && t.layer == FootprintLayer::AssemblyTop)
    );
    for warning in ["hidden", "alternate", "non_logic"] {
        assert!(
            out.warnings.iter().any(|w| w.contains(warning)),
            "{:?}",
            out.warnings
        );
    }
}
