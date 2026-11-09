//! The Schematic page. Its Update from Library group is a read-only summary of the
//! settings page 1 of the Update from Library dialog saved with the Project, with Reset
//! (`schematic.component.update_settings.reset`), as in the leading suite.

use egui::{Grid, RichText, Ui};
use reda_engine::model::Project;
use reda_engine::model::library_update::{
    ComponentFieldMissing, FootprintChoice, LibraryFieldMissing, LibraryUpdateSettings,
    LocallyEdited,
};

use crate::settings::{Page, PageCx, Problem, Step};
use crate::tr;

/// The Schematic page's state: the saved Update from Library settings, and whether Reset
/// is pending until Apply.
#[derive(Clone, Debug, Default)]
pub struct SchematicPage {
    /// The settings saved with the Project; `None` when none are (the defaults apply).
    pub saved: Option<LibraryUpdateSettings>,
    /// Reset was clicked: Apply forgets the saved settings.
    pub reset: bool,
}

impl SchematicPage {
    /// The summary's rows, `(setting, value)`, of the settings in force after Apply.
    pub fn summary(&self) -> Vec<(String, String)> {
        let s = match (&self.saved, self.reset) {
            (Some(saved), false) => saved.clone(),
            _ => LibraryUpdateSettings::default(),
        };
        let on_off = |on: bool| {
            if on {
                tr!("settings-on")
            } else {
                tr!("settings-off")
            }
        };
        vec![
            (
                tr!("library-update-setting-library-field-missing"),
                match s.library_field_missing {
                    LibraryFieldMissing::Add => tr!("library-update-add"),
                    LibraryFieldMissing::AddIfNotBlank => tr!("library-update-add-if-not-blank"),
                    LibraryFieldMissing::Skip => tr!("library-update-skip"),
                },
            ),
            (
                tr!("library-update-setting-component-field-missing"),
                match s.component_field_missing {
                    ComponentFieldMissing::Keep => tr!("library-update-keep"),
                    ComponentFieldMissing::Remove => tr!("library-update-remove"),
                },
            ),
            (
                tr!("library-update-setting-locally-edited"),
                match s.locally_edited {
                    LocallyEdited::Keep => tr!("library-update-keep"),
                    LocallyEdited::TakeNew => tr!("library-update-take-new"),
                },
            ),
            (
                tr!("library-update-setting-fields-to-update"),
                match &s.fields_to_update {
                    None => tr!("settings-every-field"),
                    Some(names) => names.join(", "),
                },
            ),
            (
                tr!("library-update-setting-add-new-footprints"),
                on_off(s.add_new_footprints),
            ),
            (
                tr!("library-update-setting-remove-missing-footprints"),
                on_off(s.remove_missing_footprints),
            ),
            (
                tr!("library-update-setting-footprint-choice"),
                match s.footprint_choice {
                    FootprintChoice::KeepIfPresent => tr!("library-update-keep-if-present"),
                    FootprintChoice::Default => tr!("library-update-default"),
                },
            ),
            (
                tr!("library-update-setting-preserve-field-locations"),
                on_off(s.preserve_field_locations),
            ),
            (
                tr!("library-update-setting-preserve-field-visibility"),
                on_off(s.preserve_field_visibility),
            ),
        ]
    }
}

impl Page for SchematicPage {
    fn title(&self) -> String {
        tr!("settings-page-schematic")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        self.saved = project.props().library_update.clone();
        if reset || self.saved.is_none() {
            self.reset = false;
        }
    }

    fn steps(&self) -> Vec<Step> {
        if self.reset && self.saved.is_some() {
            vec![Step::new(
                "schematic.component.update_settings.reset",
                serde_json::json!({}),
            )]
        } else {
            Vec::new()
        }
    }

    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }

    fn show(&mut self, ui: &mut Ui, _cx: &mut PageCx<'_>) {
        ui.label(RichText::new(tr!("settings-update-from-library")).strong());
        ui.weak(tr!("settings-update-from-library-hint"));
        Grid::new("settings-update-from-library")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                for (name, value) in self.summary() {
                    ui.label(name);
                    ui.label(value);
                    ui.end_row();
                }
            });
        let can_reset = self.saved.is_some() && !self.reset;
        if ui
            .add_enabled(can_reset, egui::Button::new(tr!("settings-update-reset")))
            .clicked()
        {
            self.reset = true;
        }
    }
}
