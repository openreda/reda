//! Stamps the release version into the workspace before CI builds the packages.
//!
//! The release tag is the source of truth: the committed workspace version stays `0.0.0`,
//! and CI rewrites it in its checkout only, so nothing is committed back.

use toml_edit::{DocumentMut, Item, value};

use crate::Version;

/// Sets `[workspace.package] version` in the root `Cargo.toml`, keeping everything else.
pub fn stamp_manifest(manifest: &str, version: Version) -> Result<String, String> {
    let mut doc = parse(manifest, "Cargo.toml")?;
    let slot = doc
        .get_mut("workspace")
        .and_then(|w| w.get_mut("package"))
        .and_then(|p| p.get_mut("version"))
        .filter(|v| v.is_str())
        .ok_or("Cargo.toml has no [workspace.package] version to stamp")?;
    set_keeping_decor(slot, version);
    Ok(doc.to_string())
}

/// Sets the version of every workspace crate in `Cargo.lock`, so `--locked` builds still
/// pass. Workspace crates are the entries without a `source`; registry and git crates keep
/// their versions.
pub fn stamp_lock(lock: &str, version: Version) -> Result<String, String> {
    let mut doc = parse(lock, "Cargo.lock")?;
    let packages = doc
        .get_mut("package")
        .and_then(Item::as_array_of_tables_mut)
        .ok_or("Cargo.lock has no [[package]] entries")?;
    for package in packages.iter_mut() {
        if package.contains_key("source") {
            continue;
        }
        if let Some(slot) = package.get_mut("version") {
            set_keeping_decor(slot, version);
        }
    }
    Ok(doc.to_string())
}

fn parse(text: &str, name: &str) -> Result<DocumentMut, String> {
    text.parse()
        .map_err(|e| format!("{name} is not valid TOML: {e}"))
}

/// Replaces a string value, keeping its surrounding whitespace and trailing comment.
fn set_keeping_decor(slot: &mut Item, version: Version) {
    let decor = slot.as_value().map(|v| v.decor().clone());
    *slot = value(version.to_string());
    if let (Some(decor), Some(v)) = (decor, slot.as_value_mut()) {
        *v.decor_mut() = decor;
    }
}
