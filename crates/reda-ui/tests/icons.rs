//! Icons: every semantic icon key the UI can show has art, the vendored set matches its
//! table and README, and the dark-theme SVGs are recoloured on load for the light theme
//! and for disabled buttons.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use egui::Theme;
use reda_ui::engine::Engine;
use reda_ui::engine::model::DocumentKind;
use reda_ui::icons;
use reda_ui::layout::PanelId;
use reda_ui::shell::Shell;

fn icons_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("icons")
}

/// Every icon key a Command or Preset of the app's registry names.
fn command_icon_keys() -> BTreeSet<String> {
    let engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .headless(false)
        .build();
    let shell = Shell::new(engine);
    let catalogue = shell.engine().registry().catalogue();
    let mut keys = BTreeSet::new();
    for command in catalogue["commands"].as_array().unwrap() {
        keys.insert(command["icon"].as_str().unwrap().to_owned());
        for preset in command["presets"].as_array().unwrap() {
            keys.insert(preset["icon"].as_str().unwrap().to_owned());
        }
    }
    keys
}

#[test]
fn every_command_preset_panel_and_document_icon_key_has_art() {
    let mut keys = command_icon_keys();
    assert!(keys.len() > 50, "the scan found only {keys:?}");
    for panel in PanelId::ALL {
        keys.insert(icons::panel_icon(panel).to_owned());
    }
    for kind in [DocumentKind::Schematic, DocumentKind::Pcb] {
        keys.insert(icons::document_icon(kind).to_owned());
    }
    keys.insert(icons::START_PAGE.to_owned());
    let missing: Vec<&String> = keys.iter().filter(|k| !icons::has(k)).collect();
    assert!(
        missing.is_empty(),
        "icon keys without art (add them to icons/sources.txt and run `cargo xtask icons`): {missing:?}"
    );
}

#[test]
fn a_key_without_art_is_reported_missing() {
    assert!(icons::has("save"));
    assert!(!icons::has("no_such_icon"));
    assert!(icons::svg("no_such_icon", Theme::Dark, true).is_none());
}

#[test]
fn the_vendored_set_matches_its_table_and_readme() {
    let readme = std::fs::read_to_string(icons_dir().join("README.md")).unwrap();
    let mut files = BTreeSet::new();
    for entry in std::fs::read_dir(icons_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "svg") {
            let key = path.file_stem().unwrap().to_str().unwrap().to_owned();
            let svg = std::fs::read_to_string(&path).unwrap();
            assert!(
                !svg.contains("currentColor"),
                "{key}.svg still uses currentColor"
            );
            assert!(
                readme.contains(&format!("| `{key}` |")),
                "README.md has no provenance row for {key}"
            );
            files.insert(key);
        }
    }
    let table: BTreeSet<String> = icons::keys().map(str::to_owned).collect();
    assert_eq!(
        files, table,
        "run `cargo xtask icons` to regenerate the table"
    );
}

#[test]
fn the_light_theme_swaps_the_role_colours_and_the_placeholder_white() {
    let dark = r##"<svg stroke="#fff"><path stroke="#8a93a0"/><path stroke="#2fb36b"/><path stroke="#e08a2e"/><path stroke="#3d8fd6"/><circle fill="#EC4F7A"/><path stroke="#ffffff"/></svg>"##;
    assert_eq!(icons::recolour(dark, Theme::Dark, true), dark);
    assert_eq!(
        icons::recolour(dark, Theme::Light, true),
        r##"<svg stroke="#1f2329"><path stroke="#727984"/><path stroke="#228a51"/><path stroke="#ac6921"/><path stroke="#347dbc"/><circle fill="#d2456c"/><path stroke="#1f2329"/></svg>"##
    );
}

#[test]
fn disabled_icons_are_a_greyscale_copy_that_the_disabled_button_fades_once() {
    // Each colour becomes its luminance pulled 40 % toward mid grey: #3d8fd6 has luminance
    // 130.7, which pulls to 129.6; white pulls to 204.2. The copy is not faded itself: the
    // disabled button fades it, once, to about 47 % (the theme's disabled_alpha).
    let svg = r##"<svg stroke="#3d8fd6"><path fill="#fff"/></svg>"##;
    assert_eq!(
        icons::recolour(svg, Theme::Dark, false),
        r##"<svg stroke="#828282"><path fill="#cccccc"/></svg>"##
    );
    let light = icons::recolour(svg, Theme::Light, false);
    assert!(!light.contains("opacity"), "{light}");
    // The light theme's swap comes first: #347dbc has luminance 114.0, pulled to 119.6.
    assert!(light.contains(r##"stroke="#787878""##), "{light}");
}

#[test]
fn icons_load_for_both_themes_and_states() {
    // `about` is still a placeholder, drawn in white; `save` is REDA's own art.
    let dark = String::from_utf8(icons::svg("about", Theme::Dark, true).unwrap().to_vec()).unwrap();
    let light =
        String::from_utf8(icons::svg("about", Theme::Light, true).unwrap().to_vec()).unwrap();
    assert!(dark.contains("#fff"));
    assert!(light.contains("#1f2329") && !light.contains("#fff"));
    let save = String::from_utf8(icons::svg("save", Theme::Light, true).unwrap().to_vec()).unwrap();
    assert!(
        save.contains("#347dbc") && !save.contains("#3d8fd6"),
        "{save}"
    );
    let off = String::from_utf8(icons::svg("save", Theme::Light, false).unwrap().to_vec()).unwrap();
    assert!(!off.contains("#347dbc"), "{off}");
}

#[test]
fn reda_own_icons_use_only_the_role_colours_and_plain_shapes() {
    let roles: Vec<String> = icons::role_colours()
        .map(|(dark, _)| format!("#{:02x}{:02x}{:02x}", dark.r(), dark.g(), dark.b()))
        .collect();
    let list = std::fs::read_to_string(icons_dir().join("reda.txt")).unwrap();
    let keys = list
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split(" | ").next().unwrap());
    for key in keys {
        let svg = std::fs::read_to_string(icons_dir().join(format!("{key}.svg"))).unwrap();
        assert!(
            svg.contains(r#"viewBox="0 0 24 24""#),
            "{key}.svg is not on the 24 px grid"
        );
        for banned in [
            "currentColor",
            "<text",
            "<filter",
            "<mask",
            "Gradient",
            "<image",
        ] {
            assert!(!svg.contains(banned), "{key}.svg uses {banned}");
        }
        for (at, _) in svg.match_indices('#') {
            let hex = svg[at..].get(..7).unwrap_or_default().to_ascii_lowercase();
            assert!(
                roles.contains(&hex),
                "{key}.svg uses {hex}, not a role colour"
            );
        }
    }
}
