//! The `read_dxf` fuzz target still runs: its check (shared with the fuzz target through
//! `include!`) passes on every seed, and on cut-short and corrupted copies of each, which
//! must be refused or read but never panic. Fuzzing itself needs a nightly toolchain and
//! runs weekly; this keeps the target's code compiling and its seeds reading on every build.

use std::path::PathBuf;

include!("../../../fuzz/fuzz_targets/read_dxf_check.rs");

/// The seeds: every DXF fixture, and the DWG one, which is refused.
fn seeds() -> Vec<(PathBuf, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut seeds: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "dxf" || e == "dwg"))
        .map(|p| {
            let bytes = std::fs::read(&p).unwrap();
            (p, bytes)
        })
        .collect();
    seeds.sort();
    seeds
}

#[test]
fn every_dxf_seed_reads() {
    let seeds = seeds();
    assert!(seeds.len() >= 20, "{seeds:?}");
    for (path, bytes) in &seeds {
        let dxf = path.extension().is_some_and(|e| e == "dxf");
        assert_eq!(check(bytes).is_some(), dxf, "{}", path.display());
    }
}

#[test]
fn cut_short_seeds_are_refused_or_read_without_a_panic() {
    for (_, bytes) in seeds() {
        for end in (0..bytes.len()).step_by(13) {
            check(&bytes[..end]);
        }
    }
}

#[test]
fn corrupted_seeds_are_refused_or_read_without_a_panic() {
    // Overwrite bytes with ones that matter to DXF, at a spread of places.
    let specials = b"0\n-9.e\\%^{};P\x00\xff";
    for (_, bytes) in seeds() {
        for (k, at) in (0..bytes.len()).step_by(7).enumerate() {
            let mut b = bytes.clone();
            b[at] = specials[k % specials.len()];
            check(&b);
        }
    }
}
