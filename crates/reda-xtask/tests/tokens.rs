//! `cargo xtask tokens` runs the one reda-ui test that owns the spec's value tables, and
//! the spec has the markers it writes between.

use std::path::Path;

use reda_xtask::tokens::{SPEC, TEST, test_args};

#[test]
fn the_chore_runs_the_table_test_alone() {
    let args = test_args();
    assert_eq!(args.last(), Some(&TEST));
    assert!(args.contains(&"--exact") && args.contains(&"--lib"));
    assert!(args.windows(2).any(|w| w == ["-p", "reda-ui"]));
}

#[test]
fn the_test_the_chore_runs_exists_beside_the_tokens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // `theme::tables::<name>` lives in src/theme/tables.rs.
    let name = TEST.rsplit("::").next().unwrap();
    let tables = std::fs::read_to_string(root.join("crates/reda-ui/src/theme/tables.rs")).unwrap();
    assert!(tables.contains(&format!("fn {name}()")), "{name}");
    assert!(TEST.starts_with("theme::tables::"));
}

#[test]
fn the_spec_has_both_markers_once_and_in_order() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let spec = std::fs::read_to_string(root.join(SPEC)).unwrap();
    let (begin, end) = ("<!-- tokens:begin -->", "<!-- tokens:end -->");
    assert_eq!(spec.matches(begin).count(), 1);
    assert_eq!(spec.matches(end).count(), 1);
    assert!(spec.find(begin) < spec.find(end));
}
