//! The Command Palette: a fuzzy search over every Command and Preset in the current
//! context, showing each one's shortcut and why any unavailable one cannot run.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::keymap::{self, Chord};
use reda_ui::palette::{self, Entry, Row, Scope, Sources, Target};
use reda_ui::shell::Shell;
use serde_json::json;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
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

fn no_shortcuts(_: &str) -> Option<String> {
    None
}

fn entry<'e>(entries: &'e [Entry], id: &str) -> &'e Entry {
    entries
        .iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("{id} is not in the Palette"))
}

fn ids<'e>(found: &[&'e Entry]) -> Vec<&'e str> {
    found.iter().map(|e| e.id.as_str()).collect()
}

#[test]
fn every_command_and_preset_is_listed_with_its_title() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let entries = palette::entries(&shell, &no_shortcuts);
    let registry = shell.engine().registry();
    for meta in registry.commands() {
        let e = entry(&entries, meta.id);
        assert!(!e.title.is_empty(), "{}", meta.id);
    }
    assert_eq!(entry(&entries, "project.save").title, "Save All");
    // Presets, the engine's and the app's, with their own titles and icons.
    let messages = entry(&entries, "panel.show:messages");
    assert_eq!(messages.title, "Show Messages");
    assert_eq!(messages.icon, "panel_messages");
    // A Preset keeps its full title here, where no menu around it says what it is.
    assert_eq!(entry(&entries, "view.theme.set:dark").title, "Theme: Dark");
}

#[test]
fn unavailable_entries_carry_the_reason_and_available_ones_none() {
    let (home, work) = (TempDir::new("palette-home"), TempDir::new("palette-work"));
    let mut shell = Shell::new(engine_in(home.path()));
    let entries = palette::entries(&shell, &no_shortcuts);
    assert_eq!(
        entry(&entries, "project.save").unavailable.as_deref(),
        Some("no Project is open")
    );
    assert_eq!(entry(&entries, "app.quit").unavailable, None);

    shell
        .open_project(&make_project(work.path(), "Demo"))
        .unwrap();
    let entries = palette::entries(&shell, &no_shortcuts);
    assert_eq!(entry(&entries, "project.save").unavailable, None);
}

#[test]
fn entries_show_the_shortcut_the_keymap_gives() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let keymap = |id: &str| (id == "project.save").then(|| "Ctrl+S".to_owned());
    let entries = palette::entries(&shell, &keymap);
    assert_eq!(
        entry(&entries, "project.save").shortcut.as_deref(),
        Some("Ctrl+S")
    );
    assert_eq!(entry(&entries, "app.quit").shortcut, None);
}

#[test]
fn the_window_s_lookup_reads_the_keymap() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let lookup = |id: &str| keymap::shortcut_of(&shell, id);
    let entries = palette::entries(&shell, &lookup);
    // Labelled as on this OS.
    let shown = |key: &str| Chord::parse(key).unwrap().label(cfg!(target_os = "macos"));
    assert_eq!(
        entry(&entries, "project.save").shortcut,
        Some(shown("Mod+S"))
    );
    assert_eq!(
        entry(&entries, "app.dialog.open:command_palette").shortcut,
        Some(shown("Mod+Shift+P"))
    );
    assert_eq!(entry(&entries, "app.quit").shortcut, None);
}

#[test]
fn the_search_is_fuzzy_on_titles_and_ids_and_ranks_the_closest_first() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let entries = palette::entries(&shell, &no_shortcuts);

    // Initials and skipped letters match; the tightest match ranks first.
    let found = palette::search(&entries, "sav al");
    assert_eq!(found[0].id, "project.save");
    let found = palette::search(&entries, "zf");
    assert_eq!(found[0].id, "view.zoom_fit", "{:?}", ids(&found));
    let found = palette::search(&entries, "Show Mess");
    assert_eq!(found[0].id, "panel.show:messages");
    // Ids match too, and case does not matter.
    let found = palette::search(&entries, "PROJECT.SAVE");
    assert_eq!(found[0].id, "project.save");
    // Letters out of order do not match.
    assert!(palette::search(&entries, "xqzj").is_empty());
    // Every hit has every query letter in order in its title or id.
    for e in palette::search(&entries, "drc") {
        assert!(
            in_order("drc", &e.title.to_lowercase()) || in_order("drc", &e.id),
            "{} matched",
            e.id
        );
    }
}

fn in_order(query: &str, text: &str) -> bool {
    let mut chars = text.chars();
    query.chars().all(|q| chars.any(|c| c == q))
}

#[test]
fn an_empty_search_lists_available_entries_before_unavailable_ones() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let entries = palette::entries(&shell, &no_shortcuts);
    let found = palette::search(&entries, "");
    assert_eq!(found.len(), entries.len());
    let first_unavailable = found.iter().position(|e| e.unavailable.is_some()).unwrap();
    assert!(
        found[first_unavailable..]
            .iter()
            .all(|e| e.unavailable.is_some())
    );
    // Among equals, by title.
    let titles: Vec<&str> = found[..first_unavailable]
        .iter()
        .map(|e| e.title.as_str())
        .collect();
    let mut sorted = titles.clone();
    sorted.sort_by_key(|t| t.to_lowercase());
    assert_eq!(titles, sorted);
}

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

/// A Project with R1 and R2 on Sheet 1, joined by the Net SIG, and a PCB.
fn demo_shell(work: &Path) -> Shell {
    let library =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test");
    let engine = Engine::builder()
        .ids(reda_ui::engine::command::SeededIds::new(0))
        .clock(reda_ui::engine::command::FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, "Demo").unwrap();
    for x in ["0mm", "10mm"] {
        shell
            .run(
                "schematic.component.place",
                json!({"document": SHEET, "part": R_10K, "at": {"x": x, "y": 0}}),
            )
            .unwrap();
    }
    shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": [
                {"x": "0mm", "y": "-3.81mm"}, {"x": "10mm", "y": "-3.81mm"}]}),
        )
        .unwrap();
    shell
        .run(
            "schematic.net_label.add",
            json!({"document": SHEET, "name": "SIG", "at": {"x": "5mm", "y": "-3.81mm"}}),
        )
        .unwrap();
    shell
}

fn titles(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|r| r.title.as_str()).collect()
}

fn uuid(s: &str) -> reda_ui::engine::model::Uuid {
    s.parse().unwrap()
}

#[test]
fn the_first_character_picks_the_scope() {
    assert_eq!(Scope::of(">save"), (Scope::Commands, "save"));
    assert_eq!(Scope::of("@R12"), (Scope::Components, "R12"));
    assert_eq!(Scope::of("#GND"), (Scope::Nets, "GND"));
    assert_eq!(Scope::of(":Sheet"), (Scope::Documents, "Sheet"));
    assert_eq!(Scope::of("?"), (Scope::Help, ""));
    assert_eq!(Scope::of("R12"), (Scope::GoTo, "R12"));
    assert_eq!(Scope::of(""), (Scope::GoTo, ""));
}

#[test]
fn each_scope_searches_its_own_kind_of_row() {
    let work = TempDir::new("palette-work");
    let shell = demo_shell(work.path());
    let sources = Sources::of(&shell, &no_shortcuts);

    let rows = sources.rows(">save al", &[]);
    assert_eq!(rows[0].target, Target::Run("project.save".into()));
    assert!(rows.iter().all(|r| matches!(r.target, Target::Run(_))));

    let rows = sources.rows("@R2", &[]);
    assert_eq!(titles(&rows), ["R2"]);
    assert!(matches!(rows[0].target, Target::Component(_)));
    assert_eq!(rows[0].detail.as_deref(), Some("10k"));
    // Empty, a scope lists all its rows, Components in natural order.
    assert_eq!(titles(&sources.rows("@", &[])), ["R1", "R2"]);

    let rows = sources.rows("#SIG", &[]);
    assert_eq!(titles(&rows), ["SIG"]);
    assert_eq!(rows[0].target, Target::Net("SIG".into()));

    let rows = sources.rows(":", &[]);
    assert_eq!(titles(&rows), ["Sheet 1", "Demo"]);
    assert_eq!(rows[0].target, Target::Document(uuid(SHEET)));
    assert_eq!(rows[1].target, Target::Document(uuid(PCB)));

    // No prefix: Documents, Components and Nets, but no Commands.
    let rows = sources.rows("s", &[]);
    assert!(titles(&rows).contains(&"Sheet 1"));
    assert!(titles(&rows).contains(&"SIG"));
    assert!(rows.iter().all(|r| !matches!(r.target, Target::Run(_))));
    let rows = sources.rows("r1", &[]);
    assert_eq!(titles(&rows)[0], "R1");

    // `?` lists the prefixes; choosing one fills it in.
    let rows = sources.rows("?", &[]);
    let prefixes: Vec<&Target> = rows.iter().map(|r| &r.target).collect();
    assert_eq!(
        prefixes,
        [
            &Target::Prefix(Scope::Commands),
            &Target::Prefix(Scope::Components),
            &Target::Prefix(Scope::Nets),
            &Target::Prefix(Scope::Documents),
            &Target::Prefix(Scope::Help),
        ]
    );
    assert_eq!(titles(&rows)[0], ">");
    assert_eq!(rows[0].detail.as_deref(), Some("Commands"));
}

#[test]
fn empty_it_shows_recent_items_then_the_documents() {
    let work = TempDir::new("palette-work");
    let shell = demo_shell(work.path());
    let sources = Sources::of(&shell, &no_shortcuts);
    assert_eq!(titles(&sources.rows("", &[])), ["Sheet 1", "Demo"]);

    let r2 = sources.rows("@R2", &[])[0].target.clone();
    let recent = [
        r2,
        Target::Run("project.save".into()),
        Target::Document(uuid(PCB)),
        // Gone since: left out.
        Target::Net("NOWHERE".into()),
    ];
    let rows = sources.rows("", &recent);
    assert_eq!(titles(&rows), ["R2", "Save All", "Demo", "Sheet 1"]);
    assert!(rows[..3].iter().all(|r| r.recent));
    assert!(!rows[3].recent);
}

#[test]
fn command_rows_show_their_menu_path_in_the_current_context() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let entries = palette::entries(&shell, &no_shortcuts);
    let path = |id: &str| entry(&entries, id).menu_path.clone();
    assert_eq!(path("app.quit").as_deref(), Some("File › Quit"));
    // The first menu that has it.
    assert_eq!(path("project.save").as_deref(), Some("File › Save All"));
    assert_eq!(
        path("view.theme.set:dark").as_deref(),
        Some("View › Theme › Dark")
    );
    // A raw binding names its Command when it is the only menu item for it.
    assert_eq!(path("edit.undo").as_deref(), Some("Edit › Undo"));
    // In no menu: nothing.
    assert_eq!(path("app.dialog.open:palette"), None);
    // Schematic menus only once a sheet is active.
    assert_eq!(path("schematic.dialog.open:select_similar"), None);

    let work = TempDir::new("palette-work");
    let mut shell = demo_shell(work.path());
    shell
        .run("document.open", json!({ "document": SHEET }))
        .unwrap();
    let entries = palette::entries(&shell, &no_shortcuts);
    let path = |id: &str| entry(&entries, id).menu_path.clone();
    // Several menu items run it with different params: no one path.
    assert_eq!(
        path("schematic.dialog.open:select_similar").as_deref(),
        Some("Edit › Select Similar…")
    );
    assert_eq!(path("schematic.align"), None);
    // The row carries it.
    let rows = Sources::of(&shell, &no_shortcuts).rows(">select similar", &[]);
    let row = rows
        .iter()
        .find(|r| r.target == Target::Run("schematic.dialog.open:select_similar".into()))
        .unwrap();
    assert_eq!(row.menu_path.as_deref(), Some("Edit › Select Similar…"));
}
