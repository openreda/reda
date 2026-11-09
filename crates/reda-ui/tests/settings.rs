//! The settings dialogs' model, without a window: Project Settings' pages hold drafts of
//! the Project's settings, Apply commits every page's changes as one `batch` (one Journal
//! entry, one undo entry), and invalid values block Apply.

use std::path::Path;

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{ErcCheck, Scope, Severity, Uuid};
use reda_ui::settings::project::{self, ErcPage, FabPresetsPage, LibrariesPage, PropertiesPage};
use reda_ui::shell::Shell;
use serde_json::json;

fn shell_with_project(home: &TempDir, work: &TempDir) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    shell
}

fn dialog(shell: &Shell) -> reda_ui::settings::SettingsDialog {
    project::dialog(shell.engine().project().unwrap())
}

fn journal_ids(shell: &Shell) -> Vec<String> {
    shell
        .engine()
        .journal()
        .iter()
        .map(|e| e.id.clone())
        .collect()
}

#[test]
fn project_settings_starts_with_its_four_pages() {
    let (home, work) = (TempDir::new("set-home"), TempDir::new("set-work"));
    let shell = shell_with_project(&home, &work);
    let d = dialog(&shell);
    assert_eq!(
        d.page_titles()[..4],
        ["Properties", "ERC", "Libraries", "Fab Presets"]
    );
    assert!(!d.has_changes());
}

#[test]
fn apply_commits_every_page_as_one_batch_and_one_undo_entry() {
    let (home, work) = (TempDir::new("set-home"), TempDir::new("set-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    let props = d.page_mut::<PropertiesPage>().unwrap();
    props.title.set("Motor driver".into());
    props.company.set("Acme".into());
    d.page_mut::<ErcPage>()
        .unwrap()
        .set(ErcCheck::SinglePinNet, Severity::Off);
    d.page_mut::<LibrariesPage>().unwrap().add("parts");
    assert!(d.has_changes() && d.can_apply());

    let before = journal_ids(&shell).len();
    d.apply(&mut shell).unwrap();
    let journal = journal_ids(&shell);
    assert_eq!(journal[before..], ["batch"]);
    let p = shell.engine().project().unwrap().props().clone();
    assert_eq!(p.properties.title, "Motor driver");
    assert_eq!(p.properties.company, "Acme");
    assert_eq!(
        p.erc.severities.get(ErcCheck::SinglePinNet),
        Some(Severity::Off)
    );
    assert_eq!(p.libraries, ["parts"]);
    assert!(!d.has_changes(), "applied changes are no longer pending");

    // One undo puts everything back, and the dialog follows.
    shell.undo(false, Some(Scope::Project)).unwrap();
    let p = shell.engine().project().unwrap().props().clone();
    assert_eq!(p.properties.title, "");
    assert_eq!(
        p.erc.severities.get(ErcCheck::SinglePinNet),
        Some(Severity::Warning)
    );
    assert!(p.libraries.is_empty());
    assert!(
        shell.undo(false, Some(Scope::Project)).is_err(),
        "nothing else to undo"
    );
    d.sync(shell.engine().project().unwrap());
    assert_eq!(d.page::<PropertiesPage>().unwrap().title.value(), "");
    assert!(!d.has_changes());
}

#[test]
fn apply_with_nothing_changed_runs_nothing() {
    let (home, work) = (TempDir::new("set-home"), TempDir::new("set-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    let before = journal_ids(&shell).len();
    assert!(!d.can_apply());
    d.apply(&mut shell).unwrap();
    assert_eq!(journal_ids(&shell).len(), before);
}

#[test]
fn invalid_values_are_flagged_on_their_page_and_block_apply() {
    let (home, work) = (TempDir::new("set-home"), TempDir::new("set-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    let libraries = d.page_mut::<LibrariesPage>().unwrap();
    libraries.add("parts");
    libraries.add("parts/");
    libraries.add("/abs/libs");
    libraries.add("  ");

    let problems: Vec<(String, String)> = d
        .problems()
        .into_iter()
        .map(|(page, p)| (page, p.field))
        .collect();
    assert_eq!(
        problems,
        [
            ("Libraries".to_owned(), "path/1".to_owned()),
            ("Libraries".to_owned(), "path/2".to_owned()),
            ("Libraries".to_owned(), "path/3".to_owned()),
        ]
    );
    assert!(!d.can_apply() && !d.can_ok());
    let before = journal_ids(&shell).len();
    assert!(d.apply(&mut shell).is_err());
    assert_eq!(journal_ids(&shell).len(), before, "nothing ran");

    // Fixing the values clears the flags.
    let libraries = d.page_mut::<LibrariesPage>().unwrap();
    libraries.remove(3);
    libraries.remove(2);
    libraries.remove(1);
    assert!(d.problems().is_empty() && d.can_apply());
}

#[test]
fn syncing_keeps_pending_edits_and_refreshes_the_rest() {
    let (home, work) = (TempDir::new("set-home"), TempDir::new("set-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    d.page_mut::<PropertiesPage>()
        .unwrap()
        .title
        .set("Mine".into());
    shell
        .run(
            "project.properties.set",
            json!({"author": "Ada", "title": "Theirs"}),
        )
        .unwrap();
    d.sync(shell.engine().project().unwrap());
    let page = d.page::<PropertiesPage>().unwrap();
    assert_eq!(page.title.value(), "Mine");
    assert_eq!(page.author.value(), "Ada");
}

#[test]
fn an_undo_under_pending_library_edits_does_not_bring_back_what_it_undid() {
    let (home, work) = (TempDir::new("settings-home"), TempDir::new("settings-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    d.page_mut::<LibrariesPage>().unwrap().add("parts");
    d.apply(&mut shell).unwrap();
    // A pending edit, then the add is undone.
    d.page_mut::<LibrariesPage>().unwrap().add("more");
    shell.undo(false, Some(Scope::Project)).unwrap();
    d.sync(shell.engine().project().unwrap());
    assert_eq!(d.page::<LibrariesPage>().unwrap().rows(), ["more"]);
    let ids: Vec<_> = d.steps().into_iter().map(|s| s.params).collect();
    assert_eq!(ids, [json!({"path": "more"})]);
}

/// A Project copy of the user preset "Mine", whose source the user then edits by hand so
/// that the copy is out of date.
fn stale_user_preset(shell: &mut Shell, home: &TempDir) -> Uuid {
    let copy = shell
        .engine()
        .project()
        .unwrap()
        .fab_presets()
        .next()
        .unwrap()
        .uuid;
    shell
        .run(
            "fab_preset.save_user",
            json!({"preset": copy, "name": "Mine"}),
        )
        .unwrap();
    let out = shell
        .run("fab_preset.add", json!({"name": "Mine"}))
        .unwrap();
    let path = home.path().join("config/fab-presets/Mine.redafab");
    let text = std::fs::read_to_string(&path).unwrap();
    let edited = text.replace("\"clearance\":", "\"clearance\":1");
    assert_ne!(text, edited);
    std::fs::write(&path, edited).unwrap();
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

#[test]
fn update_from_source_is_previewed_then_committed_on_apply() {
    let (home, work) = (TempDir::new("set-home"), TempDir::new("set-work"));
    let mut shell = shell_with_project(&home, &work);
    let mine = stale_user_preset(&mut shell, &home);
    let mut d = dialog(&shell);
    let page = d.page_mut::<FabPresetsPage>().unwrap();
    assert_eq!(page.copies().len(), 2);

    page.preview_update(&mut shell, mine);
    let preview = page.preview().unwrap();
    assert_eq!(preview.preset, mine);
    let fields: Vec<&str> = preview
        .changes
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| c.field.as_str())
        .collect();
    assert_eq!(fields, ["rules.clearance"]);
    assert!(!d.has_changes(), "previewing changes nothing");

    let page = d.page_mut::<FabPresetsPage>().unwrap();
    page.accept_preview();
    assert!(page.preview().is_none());
    assert!(page.updating(mine));
    assert!(d.has_changes());
    d.apply(&mut shell).unwrap();
    assert_eq!(
        journal_ids(&shell).last().map(String::as_str),
        Some("batch")
    );
    let page = d.page_mut::<FabPresetsPage>().unwrap();
    assert!(!page.updating(mine));
    page.preview_update(&mut shell, mine);
    let preview = d.page::<FabPresetsPage>().unwrap().preview().unwrap();
    assert_eq!(preview.changes.as_ref().unwrap().len(), 0);
}

#[test]
fn save_as_user_preset_runs_at_once_and_reports_on_the_page() {
    let (home, work) = (TempDir::new("set-home"), TempDir::new("set-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    let page = d.page_mut::<FabPresetsPage>().unwrap();
    let copy = page.copies()[0].uuid;

    page.save_as_user(&mut shell, copy, "Shop");
    assert!(page.status().unwrap().ok, "{:?}", page.status());
    assert!(
        home.path()
            .join("config/fab-presets/Shop.redafab")
            .is_file()
    );
    assert!(
        !shell.engine().is_dirty(),
        "saving a user preset leaves the Project alone"
    );

    let page = d.page_mut::<FabPresetsPage>().unwrap();
    page.save_as_user(&mut shell, copy, "JLCPCB");
    let status = page.status().unwrap();
    assert!(
        !status.ok && status.message.contains("JLCPCB"),
        "{status:?}"
    );
}

#[test]
fn a_picked_folder_becomes_a_library_row() {
    let (home, work) = (TempDir::new("set-home"), TempDir::new("set-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    d.picked_folder(Path::new("../shared/libs"));
    assert_eq!(
        d.page::<LibrariesPage>().unwrap().rows(),
        ["../shared/libs"]
    );
    d.apply(&mut shell).unwrap();
    let p = shell.engine().project().unwrap().props();
    assert_eq!(p.libraries, ["../shared/libs"]);

    // Removing it again is one more batch.
    d.page_mut::<LibrariesPage>().unwrap().remove(0);
    d.apply(&mut shell).unwrap();
    assert!(
        shell
            .engine()
            .project()
            .unwrap()
            .props()
            .libraries
            .is_empty()
    );
}

#[test]
fn new_library_writes_the_library_at_once_and_registers_it_on_apply() {
    let (home, work) = (TempDir::new("set-home"), TempDir::new("set-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    let page = d.page_mut::<LibrariesPage>().unwrap();
    assert_eq!(page.create(&mut shell, "Parts"), Ok("Parts".to_owned()));
    assert!(work.path().join("Board/Parts/Parts.redalib").is_file());
    assert_eq!(d.page::<LibrariesPage>().unwrap().rows(), ["Parts"]);
    assert!(journal_ids(&shell).contains(&"library.create".to_owned()));

    d.apply(&mut shell).unwrap();
    assert_eq!(
        shell.engine().project().unwrap().props().libraries,
        ["Parts"]
    );
    // A second Library in the same folder is refused, and no row is added.
    let page = d.page_mut::<LibrariesPage>().unwrap();
    assert!(page.create(&mut shell, "Parts").is_err());
    assert_eq!(d.page::<LibrariesPage>().unwrap().rows(), ["Parts"]);
}

#[test]
fn the_new_library_dialog_registers_a_project_library_after_creating_it() {
    use reda_ui::command_dialog::ParamsDialog;
    use reda_ui::library_dialogs::{NewLibraryDialog, NewLibraryFor};

    let (home, work) = (TempDir::new("set-home"), TempDir::new("set-work"));
    let shell = shell_with_project(&home, &work);
    let mut d = NewLibraryDialog::new(&shell, &json!({}));
    assert_eq!(d.owner, NewLibraryFor::Project);
    assert_eq!(d.params(), None);
    d.name = "Mine".into();
    assert_eq!(
        d.params(),
        Some(json!({"path": "Mine", "name": "Mine", "register": "none"}))
    );
    assert_eq!(
        d.then(&json!({"project_path": "Mine"})),
        Some(("project.library.add".to_owned(), json!({"path": "Mine"})))
    );

    // For the user, a folder is needed and Preferences register it.
    d.owner = NewLibraryFor::User;
    assert_eq!(d.params(), None);
    d.folder = "/abs/Mine".into();
    assert_eq!(d.params().unwrap()["register"], "user");
    assert_eq!(d.then(&json!({"project_path": "Mine"})), None);
}
