//! Regenerating the bundled Inter with tabular digits (`cargo xtask fonts`).
//!
//! `crates/reda-ui/fonts/inter/Inter-Tabular.ttf` is `Inter-Regular.ttf` with its `tnum`
//! digits baked into the character map. The bake sits beside the fonts
//! (`crates/reda-ui/src/theme/fonts/tabular.rs`) as a test that fails when the checked-in
//! file is stale. This chore runs that test with `REDA_BLESS=1`, which rewrites the file
//! instead. Run it after replacing `Inter-Regular.ttf`.

use std::path::Path;
use std::process::Command;

/// The test that owns the file.
pub const TEST: &str =
    "theme::fonts::tabular::tests::the_bundled_tabular_inter_is_the_bake_of_inter_regular";
/// The baked font, from the repo root.
pub const FONT: &str = "crates/reda-ui/fonts/inter/Inter-Tabular.ttf";

/// The cargo arguments that run [`TEST`] alone.
pub fn test_args() -> Vec<&'static str> {
    vec![
        "test", "--locked", "-p", "reda-ui", "--lib", "--", "--exact", TEST,
    ]
}

/// Rewrites the baked font; returns its path from the repo root.
pub fn regenerate(root: &Path) -> Result<&'static str, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(root)
        .args(test_args())
        .env("REDA_BLESS", "1")
        .output()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // A filter that matches nothing also succeeds, so check that the test ran.
    if !output.status.success() || !stdout.contains("1 passed") {
        return Err(format!(
            "the tabular font test did not run ({}):\n{stdout}{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(FONT)
}
