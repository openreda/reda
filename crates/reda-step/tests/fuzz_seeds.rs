//! The `read_step` fuzz target still runs: its check (shared with the fuzz target through
//! `include!`) passes on every seed, and on cut-short and corrupted copies of each, which must
//! be refused or read but never panic. Fuzzing itself needs a nightly toolchain and runs
//! weekly; this keeps the target's code compiling and its seeds reading on every build.

use std::path::PathBuf;

include!("../../../fuzz/fuzz_targets/read_step_check.rs");

/// The seeds: every STEP fixture.
fn seeds() -> Vec<(PathBuf, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut seeds: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "step" || e == "stpz"))
        .map(|p| {
            let bytes = std::fs::read(&p).unwrap();
            (p, bytes)
        })
        .collect();
    seeds.sort();
    seeds
}

#[test]
fn every_seed_reads_and_round_trips() {
    let seeds = seeds();
    assert!(seeds.len() >= 6, "{seeds:?}");
    for (path, bytes) in &seeds {
        assert!(check(bytes).is_some(), "{}", path.display());
    }
}

#[test]
fn cut_short_seeds_are_refused_or_read_without_a_panic() {
    for (_, bytes) in seeds() {
        for end in (0..bytes.len()).step_by(97) {
            check(&bytes[..end]);
        }
    }
}

#[test]
fn corrupted_seeds_are_refused_or_read_without_a_panic() {
    // Overwrite bytes with characters that matter to Part 21, at a spread of places.
    let specials = b"#=();,'$*.\\0E-";
    for (_, bytes) in seeds() {
        if bytes.starts_with(&[0x1F, 0x8B]) {
            continue;
        }
        for (k, at) in (0..bytes.len()).step_by(53).enumerate() {
            let mut b = bytes.clone();
            b[at] = specials[k % specials.len()];
            check(&b);
        }
    }
}
