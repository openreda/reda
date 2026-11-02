//! Drawing the website's screenshots (`cargo xtask showcase`).
//!
//! The scenes are driven by the real window in `crates/reda-ui/tests/showcase.rs`, which
//! has the headless wgpu harness. This chore runs that test on Mesa's llvmpipe with
//! `REDA_SHOWCASE_OUT` pointing at the output folder, then stamps the manifest it wrote
//! with the commit and, for a release, the version.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::site::Manifest;

/// Adds `version` and `commit` to a manifest the renderer wrote, and checks it lists at
/// least one scene.
pub fn stamp(text: &str, version: Option<&str>, commit: Option<&str>) -> Result<String, String> {
    if Manifest::parse(text)?.shots.is_empty() {
        return Err("the showcase drew no scenes".into());
    }
    let mut json: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let object = json
        .as_object_mut()
        .ok_or("the manifest is not an object")?;
    for (key, value) in [("version", version), ("commit", commit)] {
        if let Some(value) = value {
            object.insert(key.into(), Value::from(value));
        }
    }
    Ok(serde_json::to_string_pretty(&json).map_err(|e| e.to_string())? + "\n")
}

/// Draws the showcase into `out` and stamps its manifest; returns the number of images.
pub fn render(root: &Path, out: &Path, version: Option<&str>) -> Result<usize, String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let out = std::fs::canonicalize(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut test = Command::new(cargo);
    test.current_dir(root)
        .args(["test", "--locked", "-p", "reda-ui", "--test", "showcase"])
        .args(["--", "--exact", "every_scene_renders_in_both_themes"])
        .env("REDA_SHOWCASE_OUT", &out)
        // A machine that cannot draw it fails instead of skipping.
        .env("REDA_PNG_SNAPSHOTS", "1");
    // Mesa's software renderer through GL, as the PNG snapshots use, unless already set.
    for (key, value) in [("LIBGL_ALWAYS_SOFTWARE", "1"), ("WGPU_BACKEND", "gl")] {
        if std::env::var_os(key).is_none() {
            test.env(key, value);
        }
    }
    let status = test
        .status()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("the showcase test failed ({status})"));
    }
    let manifest = out.join("manifest.json");
    let text =
        std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let stamped = stamp(&text, version, head_commit(root).as_deref())?;
    std::fs::write(&manifest, &stamped).map_err(|e| format!("{}: {e}", manifest.display()))?;
    Ok(Manifest::parse(&stamped)?.shots.len())
}

/// The checked-out commit's short hash, if it can be read.
fn head_commit(root: &Path) -> Option<String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()?;
    let hash = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (out.status.success() && !hash.is_empty()).then_some(hash)
}
