//! The Fab Preset pages' model, without a window: Rules, Stackup, Gerber, Drill, BOM, CPL
//! and Packaging for each Project copy in Project Settings (drafts committed on Apply with
//! `fab_preset.set`), each with Reset page to source.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::geom::Nm;
use reda_ui::engine::model::{FileOrigin, ProjectFabPreset, SlotCommand};
use reda_ui::settings::fab_preset::{Field, PresetEditor, PresetPage};
use reda_ui::settings::project::{self, CopyPage, FabPresetsPage};
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

fn copy(shell: &Shell) -> ProjectFabPreset {
    let project = shell.engine().project().unwrap();
    project.fab_presets().next().unwrap().clone()
}

fn last_journal_id(shell: &Shell) -> String {
    let journal = shell.engine().journal();
    journal.last().map(|e| e.id.clone()).unwrap_or_default()
}

#[test]
fn every_copy_has_the_seven_pages_under_its_name() {
    let (home, work) = (TempDir::new("fpp-home"), TempDir::new("fpp-work"));
    let shell = shell_with_project(&home, &work);
    let d = project::dialog(shell.engine().project().unwrap());
    assert_eq!(
        d.page_titles(),
        [
            "Properties",
            "ERC",
            "Schematic",
            "Libraries",
            "Fab Presets",
            "Rules",
            "Stackup",
            "Gerber",
            "Drill",
            "BOM",
            "CPL",
            "Packaging"
        ]
    );
    assert!(!d.has_changes());
}

#[test]
fn the_gerber_and_drill_switches_are_applied_as_one_batch() {
    let (home, work) = (TempDir::new("fpp-home"), TempDir::new("fpp-work"));
    let mut shell = shell_with_project(&home, &work);
    let before = copy(&shell);
    let mut d = project::dialog(shell.engine().project().unwrap());
    let gerber = d.page_where_mut::<CopyPage>(|p| p.page() == PresetPage::Gerber);
    {
        let mut editor = gerber.unwrap().editor().borrow_mut();
        editor.value_mut().gerber.file_origin = FileOrigin::Relative;
        editor.value_mut().gerber.outline_on_every_plot = true;
        editor.value_mut().gerber.slot_command = SlotCommand::G85;
        editor.value_mut().gerber.merge_drill = true;
    }
    let steps = d.steps();
    assert_eq!(steps.len(), 2, "one step per page: {steps:?}");
    assert_eq!(steps[0].id, "fab_preset.set");
    assert_eq!(
        steps[0].params["values"],
        json!({"gerber": {"file_origin": "relative", "outline_on_every_plot": true}})
    );
    assert_eq!(
        steps[1].params["values"],
        json!({"gerber": {"slot_command": "g85", "merge_drill": true}})
    );
    d.apply(&mut shell).unwrap();
    assert_eq!(last_journal_id(&shell), "batch");
    let after = copy(&shell);
    assert_eq!(after.gerber.file_origin, FileOrigin::Relative);
    assert_eq!(after.gerber.slot_command, SlotCommand::G85);
    assert!(after.gerber.merge_drill && after.gerber.outline_on_every_plot);
    assert_eq!(after.bom, before.bom);
    assert!(!d.has_changes(), "the drafts follow the applied copy");
}

#[test]
fn reset_page_to_source_puts_back_only_that_pages_keys() {
    let (home, work) = (TempDir::new("fpp-home"), TempDir::new("fpp-work"));
    let mut shell = shell_with_project(&home, &work);
    let uuid = copy(&shell).uuid;
    shell
        .run(
            "fab_preset.set",
            json!({"preset": uuid, "values": {
                "gerber": {"slot_command": "g85", "x2_attributes": false},
                "zip_assembly": true
            }}),
        )
        .unwrap();
    let mut d = project::dialog(shell.engine().project().unwrap());
    let drill = d.pages().iter().position(|p| p.title() == "Drill").unwrap();
    d.select(drill);
    assert_eq!(d.reset_label().as_deref(), Some("Reset Page to Source"));
    d.reset_page(&mut shell);
    let steps = d.steps();
    assert_eq!(steps.len(), 1, "{steps:?}");
    assert_eq!(
        steps[0].params["values"],
        json!({"gerber": {"slot_command": "route"}}),
        "the Gerber page's x2 switch and Packaging stay as the copy has them"
    );
}

#[test]
fn typed_values_are_parsed_and_bad_ones_flagged_on_their_page() {
    let generic = reda_ui::engine::output::shipped().remove(0);
    let mut editor = PresetEditor::new(generic);
    editor.set_text(Field::Clearance, "0.2");
    assert_eq!(editor.value().rules.clearance, Nm::from_um(200));
    editor.set_text(Field::SeedCopper, "2oz");
    assert_eq!(editor.value().stackup.copper, Some(Nm(70_000)));
    assert_eq!(editor.text(Field::SeedCopper), "2oz");
    editor.set_text(Field::SeedThickness, "");
    assert_eq!(
        editor.value().stackup.thickness,
        None,
        "blank seeds nothing"
    );
    editor.set_text(Field::GroupBy, "part, field:LCSC");
    assert_eq!(
        serde_json::to_value(&editor.value().bom.group_by).unwrap(),
        json!(["part", {"field": "LCSC"}])
    );

    editor.set_text(Field::TrackWidth, "wide");
    assert_eq!(editor.text(Field::TrackWidth), "wide", "the text stays");
    let rules = editor.problems(PresetPage::Rules);
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].field, Field::TrackWidth.id());
    assert!(editor.problems(PresetPage::Bom).is_empty());
}

#[test]
fn finish_and_colour_suggestions_take_any_text_and_are_never_checked() {
    let jlc = reda_ui::engine::output::shipped().remove(1);
    let mut editor = PresetEditor::new(jlc);
    assert_eq!(
        editor.text(Field::SuggestSilkColour),
        "",
        "JLCPCB lists no silkscreen colours"
    );
    editor.set_text(Field::SuggestFinish, "ENIG, Purple dip ,");
    editor.set_text(Field::SuggestMaskColour, "Matte black");
    editor.set_text(Field::SuggestSilkColour, "");
    let s = &editor.value().stackup_suggestions;
    assert_eq!(s.finish, ["ENIG", "Purple dip"]);
    assert_eq!(s.mask_color, ["Matte black"]);
    assert!(s.silk_color.is_empty());
    assert!(editor.problems(PresetPage::Stackup).is_empty());
    assert_eq!(
        editor.changes(PresetPage::Stackup),
        Some(json!({"stackup_suggestions": {
            "finish": ["ENIG", "Purple dip"],
            "mask_color": ["Matte black"]
        }}))
    );
}

#[test]
fn update_from_source_on_the_fab_presets_page_refreshes_the_copy_pages() {
    let (home, work) = (TempDir::new("fpp-home"), TempDir::new("fpp-work"));
    let mut shell = shell_with_project(&home, &work);
    let uuid = copy(&shell).uuid;
    shell
        .run(
            "fab_preset.set",
            json!({"preset": uuid, "values": {"gerber": {"slot_command": "g85"}}}),
        )
        .unwrap();
    let mut d = project::dialog(shell.engine().project().unwrap());
    let page = d.page_mut::<FabPresetsPage>().unwrap();
    page.preview_update(&mut shell, uuid);
    page.accept_preview();
    d.apply(&mut shell).unwrap();
    let drill = d
        .page_where_mut::<CopyPage>(|p| p.page() == PresetPage::Drill)
        .unwrap();
    let editor = drill.editor().borrow();
    assert_eq!(editor.value().gerber.slot_command, SlotCommand::Route);
}
