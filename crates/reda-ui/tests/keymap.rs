//! The Keymap (docs/spec/v0.1/app.md#keymap): a default Keymap data file, bindings by
//! logical key written portably, resolved by the most specific context (active Tool →
//! Document type → global), clashes reported as load errors, and the user's `keymap.toml`
//! applied last with its overrides and removals.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::bars::{self, Binding, DocumentType};
use reda_ui::engine::Engine;
use reda_ui::engine::command::{ErrorCode, UserDirs};
use reda_ui::engine::model::Severity;
use reda_ui::engine::preferences::KeymapBase;
use reda_ui::keymap::{Active, Chord, Context, Keymap};
use reda_ui::shell::Shell;
use serde_json::json;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

/// Registers a user Library under `folder` and opens a new element of `kind` in it.
fn new_element(shell: &mut Shell, folder: &Path, kind: &str) {
    let lib = folder.join("lib");
    // A fresh Library each time: an earlier binding may have saved the element.
    let _ = std::fs::remove_dir_all(&lib);
    std::fs::create_dir_all(&lib).unwrap();
    let uuid = "000000aa-0000-4000-8000-000000000000";
    let info = reda_ui::engine::model::LibraryInfo {
        uuid: uuid.parse().unwrap(),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(
        lib.join("Mine.redalib"),
        reda_ui::engine::format::library::write_library_info(&info).unwrap(),
    )
    .unwrap();
    shell
        .run(
            "app.preferences.set",
            json!({"key": "libraries.paths", "value": [lib]}),
        )
        .unwrap();
    shell
        .run(
            "library.element.new",
            json!({"library": uuid, "kind": kind, "name": "Bound"}),
        )
        .unwrap();
}

fn make_project(folder: &Path, name: &str) -> PathBuf {
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    engine
        .run("project.new", json!({"path": folder, "name": name}))
        .unwrap();
    folder.join(name).join(format!("{name}.reda"))
}

fn chord(text: &str) -> Chord {
    Chord::parse(text).unwrap()
}

fn active(tool: Option<&str>, document: Option<DocumentType>) -> Active {
    Active {
        tool: tool.map(str::to_owned),
        document,
        part: false,
    }
}

/// What `key` runs in `active`, as the bound Command or Preset id.
fn bound(keymap: &Keymap, key: &str, active: &Active) -> Option<String> {
    let action = keymap.lookup(&chord(key), active)?;
    Some(match &action.binding {
        Binding::Command(id) | Binding::Preset(id) => id.clone(),
        Binding::Raw { command, .. } => command.clone(),
    })
}

#[test]
fn keys_are_written_portably_by_logical_key() {
    assert_eq!(chord("Mod+Shift+Z").portable(), "Mod+Shift+Z");
    // Order and case do not matter; the written form is canonical.
    assert_eq!(chord("shift+mod+z"), chord("Mod+Shift+Z"));
    assert_eq!(chord("Shift+S").portable(), "Shift+S");
    assert_eq!(chord("Del").portable(), "Delete");
    assert_eq!(chord("+").portable(), "+");
    assert_eq!(chord("Mod++").portable(), "Mod++");
    assert_eq!(chord("*").portable(), "*");
    assert_eq!(chord("Space").portable(), "Space");
    assert_eq!(chord("/").portable(), "/");
    assert!(Chord::parse("Mod+Hyper+Z").is_err());
    assert!(Chord::parse("Mod+").is_err());
    assert!(Chord::parse("").is_err());
    // Shown as the platform writes it.
    assert_eq!(chord("Mod+Shift+Z").label(false), "Ctrl+Shift+Z");
    assert_eq!(chord("Mod+Shift+Z").label(true), "Shift+Cmd+Z");
    // Ctrl is its own key on macOS; elsewhere it is Mod.
    let ctrl_q = |mac| Chord::parse_for("Ctrl+Q", mac).unwrap();
    assert_eq!(ctrl_q(true).label(true), "Ctrl+Q");
    assert_ne!(ctrl_q(true), Chord::parse_for("Mod+Q", true).unwrap());
    assert_eq!(ctrl_q(false), Chord::parse_for("Mod+Q", false).unwrap());
}

#[test]
fn the_default_keymap_loads_cleanly_and_follows_the_scheme() {
    let (keymap, problems) = Keymap::load(KeymapBase::Reda, None);
    assert_eq!(problems, Vec::<String>::new());
    let global = active(None, None);
    assert_eq!(
        bound(&keymap, "Mod+S", &global).as_deref(),
        Some("project.save")
    );
    assert_eq!(
        bound(&keymap, "Mod+Z", &global).as_deref(),
        Some("edit.undo")
    );
    assert_eq!(
        bound(&keymap, "Mod+Shift+Z", &global).as_deref(),
        Some("edit.redo")
    );
    assert_eq!(
        bound(&keymap, "Mod+Y", &global).as_deref(),
        Some("edit.redo")
    );
    // Zoom to fit is a canvas key: bound per Document type, not globally.
    assert_eq!(bound(&keymap, "F", &global), None);
    for ty in [DocumentType::Schematic, DocumentType::Pcb] {
        assert_eq!(
            bound(&keymap, "F", &active(None, Some(ty))).as_deref(),
            Some("view.zoom_fit")
        );
    }
}

#[test]
fn every_default_binding_names_a_command_or_preset_and_fits_its_params() {
    let home = TempDir::new("keymap-home");
    let work = TempDir::new("keymap-work");
    let file = make_project(work.path(), "Keys");
    let (keymap, _) = Keymap::load(KeymapBase::Reda, None);
    assert!(!keymap.bindings().is_empty());
    for b in keymap.bindings() {
        let mut shell = Shell::new(engine_in(home.path()));
        shell.open_project(&file).unwrap();
        // The library editors' keys act on the active element tab: open a new one.
        let context = b.context.to_string();
        if matches!(context.as_str(), "footprint" | "symbol" | "part") {
            new_element(&mut shell, work.path(), &context);
        }
        let item = bars::resolve(&shell, &b.action);
        assert!(!item.title.is_empty());
        let Ok((id, params)) = item.run else {
            continue;
        };
        // Not run: one ends the app, the other (offered on macOS only) links the CLI into
        // the real /usr/local/bin.
        if id == "app.quit" || id == "app.cli.install" {
            continue;
        }
        // A key that leaves required params out opens the Command's params dialog
        // (Mod+Shift+G asks for a custom grid).
        if reda_ui::command_dialog::lacks_required(shell.engine().registry(), &id, &params) {
            continue;
        }
        if let Err(e) = shell.run(&id, params.clone()) {
            assert!(
                !matches!(e.code, ErrorCode::InvalidParams | ErrorCode::NotFound),
                "{} {}: {id} {params} is not a valid binding: {e:?}",
                b.context,
                b.chord.portable()
            );
        }
    }
}

const LAYERED: &str = r#"
[[bind]]
key = "F"
command = "panel.show:journal"

[[bind]]
context = "schematic"
key = "F"
command = "panel.show:layers"

[[bind]]
context = "tool:wire"
key = "F"
command = "panel.show:library"
"#;

#[test]
fn the_most_specific_context_wins() {
    let (keymap, problems) = Keymap::load(KeymapBase::Reda, Some(LAYERED));
    assert_eq!(problems, Vec::<String>::new());
    let sch = Some(DocumentType::Schematic);
    assert_eq!(
        bound(&keymap, "F", &active(Some("wire"), sch)).as_deref(),
        Some("panel.show:library")
    );
    assert_eq!(
        bound(&keymap, "F", &active(None, sch)).as_deref(),
        Some("panel.show:layers")
    );
    assert_eq!(
        bound(
            &keymap,
            "F",
            &active(Some("track"), Some(DocumentType::Pcb))
        )
        .as_deref(),
        Some("view.zoom_fit"),
        "a Tool without its own F falls back to the Document type's"
    );
    assert_eq!(
        bound(&keymap, "F", &active(None, None)).as_deref(),
        Some("panel.show:journal")
    );
    // Keys a context does not bind fall through to the global ones.
    assert_eq!(
        bound(&keymap, "Mod+S", &active(Some("wire"), sch)).as_deref(),
        Some("project.save")
    );
}

#[test]
fn a_shortcut_shows_only_where_it_is_not_shadowed() {
    let (keymap, _) = Keymap::load(KeymapBase::Reda, Some(LAYERED));
    let journal = Binding::Preset("panel.show:journal".to_owned());
    let global = active(None, None);
    let sch = active(None, Some(DocumentType::Schematic));
    assert_eq!(
        keymap.shortcut(&journal, &global).map(Chord::portable),
        Some("F".to_owned())
    );
    assert_eq!(keymap.shortcut(&journal, &sch), None);
    let save = Binding::Command("project.save".to_owned());
    assert_eq!(
        keymap.shortcut(&save, &sch).map(Chord::portable),
        Some("Mod+S".to_owned())
    );
}

#[test]
fn a_clash_within_a_context_is_a_load_error_and_keeps_the_earlier_binding() {
    let user = r#"
[[bind]]
key = "Alt+J"
command = "panel.show:journal"

[[bind]]
key = "alt+j"
command = "panel.show:layers"

[[bind]]
context = "pcb"
key = "Alt+J"
command = "panel.show:library"
"#;
    let (keymap, problems) = Keymap::load(KeymapBase::Reda, Some(user));
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("Alt+J") && problems[0].contains("global"),
        "{}",
        problems[0]
    );
    assert_eq!(
        bound(&keymap, "Alt+J", &active(None, None)).as_deref(),
        Some("panel.show:journal")
    );
    // The same key in another context is not a clash.
    assert_eq!(
        bound(&keymap, "Alt+J", &active(None, Some(DocumentType::Pcb))).as_deref(),
        Some("panel.show:library")
    );
}

#[test]
fn keymap_toml_overrides_and_removes_default_bindings() {
    let user = r#"
# Save with F2, and give Mod+S to the Journal.
[[bind]]
key = "F2"
command = "project.save"

[[bind]]
key = "Mod+S"
command = "panel.show:journal"

[[unbind]]
key = "Mod+Y"
"#;
    let (keymap, problems) = Keymap::load(KeymapBase::Reda, Some(user));
    assert_eq!(problems, Vec::<String>::new());
    let global = active(None, None);
    assert_eq!(
        bound(&keymap, "F2", &global).as_deref(),
        Some("project.save")
    );
    assert_eq!(
        bound(&keymap, "Mod+S", &global).as_deref(),
        Some("panel.show:journal")
    );
    assert_eq!(bound(&keymap, "Mod+Y", &global), None);
    assert_eq!(
        bound(&keymap, "Mod+Shift+Z", &global).as_deref(),
        Some("edit.redo")
    );
}

#[test]
fn bad_entries_are_reported_and_skipped() {
    let user = r#"
[[bind]]
key = "Mod+Hyper+S"
command = "project.save"

[[bind]]
context = "kitchen"
key = "F9"
command = "project.save"

[[bind]]
key = "F7"
command = "panel.show:layers"
params = { panel = "layers" }

[[bind]]
key = "F5"
"#;
    let (keymap, problems) = Keymap::load(KeymapBase::Reda, Some(user));
    assert_eq!(problems.len(), 4, "{problems:?}");
    for key in ["F9", "F7", "F5"] {
        assert_eq!(bound(&keymap, key, &active(None, None)), None);
    }
    let (_, problems) = Keymap::load(KeymapBase::Reda, Some("[[bind]\nkey = "));
    assert_eq!(problems.len(), 1, "{problems:?}");
}

#[test]
fn context_names_round_trip() {
    for name in [
        "global",
        "schematic",
        "pcb",
        "symbol",
        "footprint",
        "tool:wire",
    ] {
        assert_eq!(Context::parse(name).unwrap().to_string(), name);
    }
    assert!(Context::parse("tool:").is_none());
    assert!(Context::parse("board").is_none());
}

#[test]
fn a_raw_binding_is_labelled_with_its_params() {
    let home = TempDir::new("keymap-home");
    let user = r#"
[[bind]]
key = "F7"
command = "panel.show"
params = { panel = "layers" }
"#;
    let dirs = UserDirs::under(home.path());
    std::fs::create_dir_all(&dirs.config).unwrap();
    std::fs::write(dirs.config.join("keymap.toml"), user).unwrap();
    let shell = Shell::new(engine_in(home.path()));
    let action = shell
        .keymap()
        .lookup(&chord("F7"), &active(None, None))
        .unwrap()
        .clone();
    let item = bars::resolve(&shell, &action);
    assert_eq!(item.title, "Show Panel (panel: layers)");
}

#[test]
fn the_shell_reports_keymap_problems_in_messages() {
    let home = TempDir::new("keymap-home");
    let dirs = UserDirs::under(home.path());
    std::fs::create_dir_all(&dirs.config).unwrap();
    let user = r#"
[[bind]]
key = "F8"
command = "no.such.command"

[[bind]]
key = "F9"
command = "panel.show:nowhere"
"#;
    std::fs::write(dirs.config.join("keymap.toml"), user).unwrap();
    let mut shell = Shell::new(engine_in(home.path()));
    let notes: Vec<_> = shell
        .messages()
        .notes()
        .iter()
        .filter(|n| n.source == "keymap")
        .collect();
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes.iter().all(|n| n.severity == Severity::Error));
    assert!(
        notes[0].text.contains("no.such.command"),
        "{}",
        notes[0].text
    );
    assert_eq!(
        shell.keymap().lookup(&chord("F8"), &active(None, None)),
        None
    );

    // Fixing the file and reloading clears them.
    std::fs::write(dirs.config.join("keymap.toml"), "").unwrap();
    shell.reload_keymap();
    assert!(
        shell
            .messages()
            .notes()
            .iter()
            .all(|n| n.source != "keymap")
    );
}
