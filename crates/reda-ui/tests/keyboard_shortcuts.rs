//! The Keyboard Shortcuts page's model, driven through the Shell with no window: search
//! over every Command and Preset, recording a key into the user's `keymap.toml`, conflict
//! flags, removing a default key, reset to default, and clashes in the file.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::bars::Binding;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::keymap::{Active, Chord, Context};
use reda_ui::preferences_dialog::Shortcuts;
use reda_ui::shell::Shell;
use serde_json::json;

fn shell_in(home: &Path) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    Shell::new(engine)
}

fn keymap_file(home: &Path) -> PathBuf {
    UserDirs::under(home).config.join("keymap.toml")
}

fn user_file(home: &Path) -> String {
    std::fs::read_to_string(keymap_file(home)).unwrap_or_default()
}

fn chord(text: &str) -> Chord {
    Chord::parse(text).unwrap()
}

fn command(id: &str) -> Binding {
    Binding::Command(id.to_owned())
}

/// The Command or Preset `key` runs globally.
fn bound(shell: &Shell, key: &str) -> Option<String> {
    let action = shell.keymap().lookup(&chord(key), &Active::default())?;
    Some(match &action.binding {
        Binding::Command(id) | Binding::Preset(id) => id.clone(),
        Binding::Raw { command, .. } => command.clone(),
    })
}

fn keys(shell: &Shell, binding: &Binding) -> Vec<String> {
    Shortcuts::keys(shell, binding)
        .into_iter()
        .map(|(context, chord)| format!("{context} {}", chord.portable()))
        .collect()
}

#[test]
fn search_covers_every_command_and_preset_by_title_id_or_key() {
    let home = TempDir::new("shortcuts-search");
    let shell = shell_in(home.path());
    let mut page = Shortcuts::default();
    let ids = |rows: Vec<reda_ui::preferences_dialog::shortcuts::Row>| -> Vec<String> {
        rows.into_iter().map(|r| r.id).collect()
    };

    let all = ids(page.matching(&shell, ""));
    assert!(all.contains(&"project.save".to_owned()));
    assert!(
        all.contains(&"panel.show:layers".to_owned()),
        "Presets are listed"
    );
    assert!(!all.contains(&"object.get".to_owned()), "queries are not");

    assert!(ids(page.matching(&shell, "save all")).contains(&"project.save".to_owned()));
    assert!(ids(page.matching(&shell, "PROJECT.SAVE")).contains(&"project.save".to_owned()));
    // By key: Mod+S as the portable form.
    assert_eq!(ids(page.matching(&shell, "mod+s")), ["project.save"]);
}

#[test]
fn a_recorded_key_is_written_to_keymap_toml_and_applies_at_once() {
    let home = TempDir::new("shortcuts-record");
    let mut shell = shell_in(home.path());
    let mut page = Shortcuts::default();
    let save = command("project.save");

    page.start_recording(save.clone());
    page.record(chord("Mod+Alt+S"));
    assert_eq!(page.conflict(&shell), None);
    page.confirm(&mut shell).unwrap();

    assert_eq!(
        user_file(home.path()),
        "[[bind]]\nkey = \"Mod+Alt+S\"\ncommand = \"project.save\"\n"
    );
    assert_eq!(bound(&shell, "Mod+Alt+S").as_deref(), Some("project.save"));
    assert_eq!(keys(&shell, &save), ["global Mod+S", "global Mod+Alt+S"]);
    assert!(Shortcuts::is_changed(&shell, &save));
}

#[test]
fn recording_a_key_in_use_flags_the_conflict_and_replaces_it_when_confirmed() {
    let home = TempDir::new("shortcuts-conflict");
    let mut shell = shell_in(home.path());
    let mut page = Shortcuts::default();

    page.start_recording(command("project.save"));
    page.record(chord("Mod+N"));
    let conflict = page.conflict(&shell).expect("Mod+N opens New Project");
    assert!(conflict.contains("New Project"), "{conflict}");

    page.confirm(&mut shell).unwrap();
    assert_eq!(bound(&shell, "Mod+N").as_deref(), Some("project.save"));
    assert!(
        keys(
            &shell,
            &Binding::Preset("app.dialog.open:new_project".into())
        )
        .is_empty()
    );
}

#[test]
fn removing_a_default_key_writes_an_unbind_and_reset_brings_it_back() {
    let home = TempDir::new("shortcuts-reset");
    let mut shell = shell_in(home.path());
    let page = Shortcuts::default();
    let save = command("project.save");
    // The user's own comment survives every edit.
    std::fs::create_dir_all(keymap_file(home.path()).parent().unwrap()).unwrap();
    std::fs::write(keymap_file(home.path()), "# my keys\n").unwrap();

    page.unbind(&mut shell, &Context::Global, &chord("Mod+S"))
        .unwrap();
    assert_eq!(
        user_file(home.path()),
        "# my keys\n\n[[unbind]]\nkey = \"Mod+S\"\n"
    );
    assert_eq!(bound(&shell, "Mod+S"), None);
    assert!(Shortcuts::is_changed(&shell, &save));

    page.reset(&mut shell, &save).unwrap();
    assert_eq!(user_file(home.path()), "# my keys\n");
    assert_eq!(bound(&shell, "Mod+S").as_deref(), Some("project.save"));
    assert!(!Shortcuts::is_changed(&shell, &save));
}

#[test]
fn a_raw_binding_is_written_with_its_params() {
    let home = TempDir::new("shortcuts-raw");
    let mut shell = shell_in(home.path());
    let mut page = Shortcuts::default();
    let zoom = Binding::Raw {
        command: "view.zoom_fit".into(),
        params: json!({"document": "$active_document"}),
    };
    page.start_recording(zoom.clone());
    page.set_context(Context::parse("schematic").unwrap());
    page.record(chord("Shift+F"));
    page.confirm(&mut shell).unwrap();
    assert_eq!(
        user_file(home.path()),
        "[[bind]]\ncontext = \"schematic\"\nkey = \"Shift+F\"\ncommand = \"view.zoom_fit\"\nparams = { document = \"$active_document\" }\n"
    );
    assert!(keys(&shell, &zoom).contains(&"schematic Shift+F".to_owned()));
}

#[test]
fn clashes_in_keymap_toml_are_flagged_on_the_page() {
    let home = TempDir::new("shortcuts-clash");
    std::fs::create_dir_all(keymap_file(home.path()).parent().unwrap()).unwrap();
    std::fs::write(
        keymap_file(home.path()),
        "[[bind]]\nkey = \"F2\"\ncommand = \"project.save\"\n\n[[bind]]\nkey = \"F2\"\ncommand = \"app.quit\"\n",
    )
    .unwrap();
    let shell = shell_in(home.path());
    let problems = Shortcuts::problems(&shell);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("F2"), "{problems:?}");
}

#[test]
fn open_folder_points_at_the_config_folder() {
    let home = TempDir::new("shortcuts-folder");
    let shell = shell_in(home.path());
    assert_eq!(
        Shortcuts::folder(&shell),
        Some(UserDirs::under(home.path()).config)
    );
}
