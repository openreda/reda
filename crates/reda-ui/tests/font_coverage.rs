//! Every character the GUI can show is in the bundled UI faces (Inter, in each of its
//! three weights). egui would fall back to its own fonts for a missing one, or draw a
//! box, and a test harness panics on some (notes on the colour picker's 📋).
//!
//! What the GUI can show, as far as text goes: the message catalogue, each Command's
//! title, description and parameter texts (menus, the Command Palette, generated forms,
//! tooltips), the Preferences' descriptions (the Preferences window), and the string
//! literals of the crates whose messages reach the window (errors, notices, labels).

#[path = "source_scan/lex.rs"]
#[allow(dead_code)]
mod lex;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reda_ui::engine::Engine;
use serde_json::Value;
use skrifa::raw::FontRef;
use skrifa::{GlyphId, MetadataProvider as _};

/// The bundled UI faces. Running text, titles and numbers are all drawn in one of them.
const FACES: [(&str, &[u8]); 3] = [
    (
        "Inter Regular",
        include_bytes!("../fonts/inter/Inter-Regular.ttf"),
    ),
    (
        "Inter Medium",
        include_bytes!("../fonts/inter/Inter-Medium.ttf"),
    ),
    (
        "Inter SemiBold",
        include_bytes!("../fonts/inter/Inter-SemiBold.ttf"),
    ),
];

/// Crates whose string literals can reach the window. The CLI, the release and build
/// tools print to a terminal, and the test kit is not shipped.
const SHOWN_CRATES: &[&str] = &[
    "reda-command",
    "reda-control",
    "reda-engine",
    "reda-geom",
    "reda-import-kicad",
    "reda-library",
    "reda-model",
    "reda-output",
    "reda-pcb",
    "reda-render",
    "reda-schematic",
    "reda-text",
    "reda-ui",
];

/// The faces that have no glyph for `ch`.
fn missing_from(ch: char) -> Vec<&'static str> {
    FACES
        .iter()
        .filter(|(_, bytes)| {
            let font = FontRef::new(bytes).expect("a bundled face parses");
            font.charmap()
                .map(ch)
                .is_none_or(|glyph| glyph == GlyphId::NOTDEF)
        })
        .map(|(name, _)| *name)
        .collect()
}

/// Which characters of `texts` the bundled faces lack, each with where it was first seen.
fn uncovered<'a>(texts: impl IntoIterator<Item = (String, &'a str)>) -> BTreeMap<char, String> {
    let mut found = BTreeMap::new();
    for (place, text) in texts {
        for ch in text.chars() {
            if ch.is_control() || found.contains_key(&ch) || ch.is_ascii() {
                continue;
            }
            if !missing_from(ch).is_empty() {
                found.insert(ch, format!("{place}: {text:?}"));
            }
        }
    }
    found
}

fn report(found: &BTreeMap<char, String>) -> String {
    found
        .iter()
        .map(|(ch, place)| {
            format!(
                "U+{:04X} {ch:?} (missing from {}) in {place}",
                *ch as u32,
                missing_from(*ch).join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every string inside `value`, with its JSON path under `at`.
fn strings_in(value: &Value, at: String, out: &mut Vec<(String, String)>) {
    match value {
        Value::String(s) => out.push((at, s.clone())),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                strings_in(item, format!("{at}[{i}]"), out);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                strings_in(item, format!("{at}.{key}"), out);
            }
        }
        _ => {}
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `.rs` file under `dir`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_checker_sees_a_character_inter_lacks() {
    // Inter has the single guillemet and the ellipsis, and has no small black triangle.
    assert!(missing_from('›').is_empty());
    assert!(missing_from('…').is_empty());
    assert_eq!(missing_from('▸').len(), FACES.len());
    let found = uncovered([("here".to_owned(), "Design ▸ Re-annotate")]);
    assert_eq!(found.keys().copied().collect::<Vec<_>>(), ['▸']);
}

#[test]
fn every_message_in_the_catalogue_is_in_the_bundled_faces() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("i18n/en-US.ftl");
    let text = std::fs::read_to_string(&path).expect("read the catalogue");
    let lines = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with('#'))
        .map(|(n, line)| (format!("i18n/en-US.ftl:{}", n + 1), line));
    let found = uncovered(lines);
    assert!(found.is_empty(), "{}", report(&found));
}

#[test]
fn every_command_text_and_preference_description_is_in_the_bundled_faces() {
    let engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    let mut texts = Vec::new();
    strings_in(
        &engine.registry().catalogue(),
        "commands".to_owned(),
        &mut texts,
    );
    strings_in(
        reda_ui::engine::preferences::schema(),
        "preferences".to_owned(),
        &mut texts,
    );
    assert!(texts.len() > 1000, "only {} texts were found", texts.len());
    let found = uncovered(texts.iter().map(|(at, s)| (at.clone(), s.as_str())));
    assert!(found.is_empty(), "{}", report(&found));
}

#[test]
fn every_string_literal_that_can_reach_the_window_is_in_the_bundled_faces() {
    let mut files = Vec::new();
    for name in SHOWN_CRATES {
        rust_files(
            &repo_root().join("crates").join(name).join("src"),
            &mut files,
        );
    }
    assert!(files.len() > 300, "only {} files were found", files.len());
    let mut texts = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("read a source file");
        let shown = file
            .strip_prefix(repo_root())
            .unwrap_or(file)
            .display()
            .to_string();
        for (line, string) in lex::lex(&text).strings {
            texts.push((format!("{shown}:{line}"), string));
        }
    }
    let found = uncovered(texts.iter().map(|(at, s)| (at.clone(), s.as_str())));
    assert!(found.is_empty(), "{}", report(&found));
}
