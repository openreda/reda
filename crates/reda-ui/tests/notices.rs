//! The third-party notices built into the binary (docs/spec/v0.1/app.md#icons): every
//! directory of vendored third-party data in the repo has an entry, with its licence text.

use std::path::{Path, PathBuf};

use reda_ui::help::NOTICES;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Whether `name` is a licence text or a font file: what marks a folder as vendored data.
fn marks_vendored_data(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["license", "licence", "copying", "ofl", "ufl"]
        .iter()
        .any(|p| lower.starts_with(p))
        || [".ttf", ".otf", ".woff", ".woff2", ".jhf"]
            .iter()
            .any(|ext| lower.ends_with(ext))
}

/// Every folder under the repo (leaving out the root, build output and hidden folders)
/// that holds a licence text or a font file, as a `/`-separated path from the root.
fn vendored_dirs() -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut marked = false;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if path.is_dir() {
                if !name.starts_with('.') && name != "target" {
                    walk(root, &path, out);
                }
            } else if marks_vendored_data(&name) {
                marked = true;
            }
        }
        if marked && dir != root {
            let relative = dir.strip_prefix(root).unwrap();
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    let root = repo_root().canonicalize().unwrap();
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    out.sort();
    out
}

/// The `Directory:` lines of the notices' vendored-data entries.
fn listed_dirs() -> Vec<&'static str> {
    NOTICES
        .lines()
        .filter_map(|line| line.strip_prefix("Directory: "))
        .collect()
}

#[test]
fn every_vendored_asset_dir_has_an_entry() {
    let dirs = vendored_dirs();
    for expected in [
        "crates/reda-text/fonts/hershey",
        "crates/reda-text/fonts/noto-sans",
        "crates/reda-ui/icons",
    ] {
        assert!(
            dirs.iter().any(|d| d == expected),
            "the scan should find {expected}, found {dirs:?}"
        );
    }
    let listed = listed_dirs();
    let missing: Vec<&String> = dirs
        .iter()
        .filter(|d| !listed.contains(&d.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "add these vendored folders to packaging/notices/vendored.txt: {missing:?}"
    );
}

#[test]
fn every_entry_names_a_folder_that_exists() {
    let root = repo_root();
    for dir in listed_dirs() {
        assert!(
            root.join(dir).is_dir(),
            "packaging/notices/vendored.txt lists {dir}, which does not exist"
        );
    }
}

#[test]
fn the_notices_carry_the_required_acknowledgements_and_licence_texts() {
    for needle in [
        // ADR-0002: the Hershey acknowledgements ship with the font data.
        "originally created by Dr.",
        "A. V. Hershey",
        "James Hurt",
        // Noto Sans and eframe's default fonts.
        "SIL OPEN FONT LICENSE Version 1.1",
        "UBUNTU FONT LICENCE Version 1.0",
        // The placeholder icon sets.
        "Lucide Icons and Contributors",
        "Paweł Kuna",
    ] {
        assert!(NOTICES.contains(needle), "the notices lack {needle:?}");
    }
}

#[test]
fn a_local_build_says_where_the_crate_licences_come_from() {
    // Tests are never release builds; the release workflow adds cargo-about's list.
    assert!(NOTICES.contains("Rust crates"));
    assert!(NOTICES.contains("cargo-about"));
}
