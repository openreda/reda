//! Project Settings ▸ Schematic: the Nets, Cross-references and Assembly options as drafts
//! applied with `schematic.options.set`, and read-only summaries of the saved Re-annotate
//! and Update from Library settings, each with a Reset that clears them on Apply.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{LocationStyle, NotFittedDisplay, Scope, SheetStyle};
use reda_ui::settings::project::{self, SchematicPage};
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

#[test]
fn the_schematic_page_follows_erc_and_applies_its_options_as_one_command() {
    let (home, work) = (TempDir::new("pss-home"), TempDir::new("pss-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    assert_eq!(
        d.page_titles()[..5],
        ["Properties", "ERC", "Schematic", "Libraries", "Fab Presets"]
    );
    let page = d.page_mut::<SchematicPage>().unwrap();
    // Nets.
    page.options.value_mut().append_sheet_numbers = true;
    // Cross-references.
    page.options.value_mut().cross_references = true;
    page.options.value_mut().cross_reference_styles.sheet = SheetStyle::Name;
    page.options.value_mut().cross_reference_styles.location = LocationStyle::Xy;
    // Assembly.
    page.options.value_mut().not_fitted_display = NotFittedDisplay::Mesh;
    assert!(d.has_changes());
    let steps = d.steps();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].id, "schematic.options.set");
    assert_eq!(
        steps[0].params,
        json!({"append_sheet_numbers": true, "cross_references": true,
               "cross_reference_sheet_style": "name",
               "cross_reference_location_style": "xy",
               "not_fitted_display": "mesh"})
    );
    d.apply(&mut shell).unwrap();
    let options = shell.engine().project().unwrap().props().schematic;
    assert!(options.append_sheet_numbers && options.cross_references);
    assert_eq!(options.not_fitted_display, NotFittedDisplay::Mesh);
    assert!(!d.has_changes(), "the drafts take the applied values");

    // Reset to defaults puts every option back, pending until Apply.
    d.select(2);
    d.reset_page(&mut shell);
    let steps = d.steps();
    assert_eq!(steps[0].params["append_sheet_numbers"], false);
    assert_eq!(steps[0].params["not_fitted_display"], "cross");
    assert!(
        shell
            .engine()
            .project()
            .unwrap()
            .props()
            .schematic
            .cross_references
    );
}

#[test]
fn the_saved_reannotate_and_library_settings_show_as_summaries_with_reset() {
    let (home, work) = (TempDir::new("pss-home"), TempDir::new("pss-work"));
    let mut shell = shell_with_project(&home, &work);
    let d = dialog(&shell);
    let page = d.page::<SchematicPage>().unwrap();
    assert_eq!(page.reannotate_summary(), None, "nothing saved yet");
    assert_eq!(page.library_update_summary(), None);

    shell
        .run(
            "schematic.reannotate",
            json!({"mode": "fix_duplicates_only", "order": "down_then_across",
                   "threshold": "2mm"}),
        )
        .unwrap();
    let mut d = dialog(&shell);
    let page = d.page_mut::<SchematicPage>().unwrap();
    let summary = page.reannotate_summary().unwrap();
    for line in [
        "Source: Schematic",
        "Mode: Fix duplicates only",
        "Scope: Project",
        "Order: Down, then across",
        "Threshold: 2 mm",
        "Start indices: None",
    ] {
        assert!(summary.iter().any(|l| l == line), "{line:?} in {summary:?}");
    }

    page.reset_reannotate();
    assert!(page.reannotate_reset_pending());
    let steps = d.steps();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].id, "schematic.saved_settings.reset");
    assert_eq!(steps[0].params, json!({"settings": ["reannotate"]}));
    d.apply(&mut shell).unwrap();
    assert!(
        shell
            .engine()
            .project()
            .unwrap()
            .props()
            .reannotate
            .is_none()
    );
    let page = d.page::<SchematicPage>().unwrap();
    assert_eq!(page.reannotate_summary(), None);
    assert!(!page.reannotate_reset_pending());
    // The Reset is one undo entry on the Project's stack.
    shell.undo(false, Some(Scope::Project)).unwrap();
    assert!(
        shell
            .engine()
            .project()
            .unwrap()
            .props()
            .reannotate
            .is_some()
    );
}

#[test]
fn the_library_update_summary_lists_the_saved_choices_and_resets_them() {
    let (home, work) = (TempDir::new("pss-home"), TempDir::new("pss-work"));
    let mut shell = shell_with_project(&home, &work);
    // Page 1 of Update from Library saved its settings in the `.reda` file.
    shell.run("project.save", json!({})).unwrap();
    let folder = shell.engine().location().unwrap().to_path_buf();
    let file = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "reda"))
        .unwrap();
    let mut reda: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    reda["library_update"] = json!({"locally_edited": "take_new", "add_new_footprints": false});
    std::fs::write(&file, reda.to_string()).unwrap();
    let mut shell = shell_with_project(&home, &TempDir::new("pss-other"));
    shell.open_project(&folder).unwrap();

    let mut d = dialog(&shell);
    let page = d.page_mut::<SchematicPage>().unwrap();
    let summary = page.library_update_summary().unwrap();
    for line in [
        "Locally edited values: Take the new default",
        "New Library Footprints: Left out",
        "Library field missing: Add",
    ] {
        assert!(summary.iter().any(|l| l == line), "{line:?} in {summary:?}");
    }
    page.reset_library_update();
    let steps = d.steps();
    assert_eq!(steps[0].params, json!({"settings": ["library_update"]}));
    d.apply(&mut shell).unwrap();
    assert!(
        shell
            .engine()
            .project()
            .unwrap()
            .props()
            .library_update
            .is_none()
    );
}
