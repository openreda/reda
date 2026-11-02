//! The UI string catalogue: `tr!` looks keys up in the English Fluent catalogue, and every
//! key the UI uses must be in it.

use std::path::{Path, PathBuf};

use reda_ui::{i18n, tr};

#[test]
fn tr_looks_a_key_up_in_the_english_catalogue() {
    assert_eq!(tr!("start-page-title"), "Start");
}

#[test]
fn tr_fills_in_arguments() {
    assert_eq!(
        tr!("canvas-description", document = "Sheet 1"),
        "Schematic canvas: Sheet 1"
    );
}

#[test]
fn a_key_missing_from_the_catalogue_shows_as_the_key() {
    assert!(!i18n::has_key("no-such-key"));
    assert_eq!(tr!("no-such-key"), "no-such-key");
}

#[test]
fn every_key_the_ui_uses_is_in_the_catalogue() {
    let mut missing = Vec::new();
    let mut used = 0;
    for file in rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src")) {
        let text = std::fs::read_to_string(&file).unwrap();
        for key in tr_keys(&text) {
            used += 1;
            if !i18n::has_key(&key) {
                missing.push(format!("{}: {key}", file.display()));
            }
        }
    }
    assert!(
        used > 10,
        "found only {used} tr! calls; is the scan broken?"
    );
    assert!(
        missing.is_empty(),
        "keys missing from i18n/en-US.ftl:\n{}",
        missing.join("\n")
    );
}

#[test]
fn the_key_scan_finds_keys_in_tr_calls() {
    let keys =
        tr_keys(r#"let a = tr!("one"); ui.label(tr!( "two", n = 3)); // tr!(x) include_str!("f")"#);
    assert_eq!(keys, ["one", "two"]);
}

/// The string-literal first argument of every `tr!(` call in `text`.
fn tr_keys(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for (i, _) in text.match_indices("tr!(") {
        // Not the tail of another macro, such as `include_str!(`.
        let before = text[..i].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = text[i + 4..].trim_start();
        if let Some(rest) = rest.strip_prefix('"')
            && let Some(end) = rest.find('"')
        {
            keys.push(rest[..end].to_owned());
        }
    }
    keys
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}
