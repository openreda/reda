//! Regenerating the value tables of the design-language spec (`cargo xtask tokens`).
//!
//! The tables between `<!-- tokens:begin -->` and `<!-- tokens:end -->` in
//! `docs/spec/v0.1/design-language.md` are the values of reda-ui's `Tokens`. The generator
//! sits beside the tokens (`crates/reda-ui/src/theme/tables.rs`), where it can read the
//! private palette, as a test that fails when the region is stale. This chore runs that
//! test with `REDA_BLESS=1`, which rewrites the region instead.

use std::path::Path;
use std::process::Command;

/// The test that owns the tables.
pub const TEST: &str = "theme::tables::the_specs_value_tables_are_the_tokens";
/// The spec file, from the repo root.
pub const SPEC: &str = "docs/spec/v0.1/design-language.md";

/// The cargo arguments that run [`TEST`] alone.
pub fn test_args() -> Vec<&'static str> {
    vec![
        "test", "--locked", "-p", "reda-ui", "--lib", "--", "--exact", TEST,
    ]
}

/// Rewrites the region from `Tokens`; returns the spec's path from the repo root.
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
            "the tokens table test did not run ({}):\n{stdout}{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(SPEC)
}
