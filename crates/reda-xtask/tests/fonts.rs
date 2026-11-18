//! `cargo xtask fonts` runs the one reda-ui test that owns the baked tabular Inter. If
//! that test is renamed, the chore would run nothing.

use std::path::Path;

use reda_xtask::fonts::{FONT, TEST, test_args};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_test_the_chore_runs_exists_beside_the_fonts() {
    let source =
        std::fs::read_to_string(root().join("crates/reda-ui/src/theme/fonts/tabular.rs")).unwrap();
    let name = TEST.rsplit("::").next().unwrap();
    assert!(source.contains(&format!("fn {name}()")), "{name}");
    assert!(source.contains("REDA_BLESS"));
    assert_eq!(test_args().last(), Some(&TEST));
}

#[test]
fn the_baked_font_is_checked_in() {
    assert!(root().join(FONT).is_file());
}
