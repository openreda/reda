//! The Schematic page: the Project's schematic options grouped as Nets, Cross-references
//! and Assembly (applied with `schematic.options.set`), then read-only summaries of the
//! saved Re-annotate and Update from Library settings, each with a Reset that clears them
//! on Apply (`schematic.saved_settings.reset`).

use egui::Ui;
use reda_engine::model::library_update::{
    ComponentFieldMissing, FootprintChoice, LibraryFieldMissing, LibraryUpdateSettings,
    LocallyEdited,
};
use reda_engine::model::{LocationStyle, NotFittedDisplay, Project, SchematicOptions, SheetStyle};
use serde_json::{Map, Value, json};

use crate::schematic_dialogs::reannotate_labels;
use crate::settings::{Draft, Page, PageCx, Problem, Step};
use crate::tr;

/// The Schematic page's drafts.
#[derive(Clone, Debug)]
pub struct SchematicPage {
    /// The schematic options, as chosen.
    pub options: Draft<SchematicOptions>,
    /// Whether Apply clears the saved Re-annotate settings.
    reset_reannotate: Draft<bool>,
    /// Whether Apply clears the saved Update from Library settings.
    reset_library_update: Draft<bool>,
    /// The summaries of the saved settings, as loaded.
    reannotate: Option<Vec<String>>,
    library_update: Option<Vec<String>>,
}

impl Default for SchematicPage {
    fn default() -> Self {
        SchematicPage {
            options: Draft::new(SchematicOptions::DEFAULT),
            reset_reannotate: Draft::new(false),
            reset_library_update: Draft::new(false),
            reannotate: None,
            library_update: None,
        }
    }
}

impl SchematicPage {
    /// The saved Re-annotate settings as lines, such as "Mode: Renumber all"; `None` when
    /// none are saved.
    pub fn reannotate_summary(&self) -> Option<&[String]> {
        self.reannotate.as_deref()
    }

    /// The saved Update from Library settings as lines; `None` when none are saved.
    pub fn library_update_summary(&self) -> Option<&[String]> {
        self.library_update.as_deref()
    }

    /// Clears the saved Re-annotate settings on Apply.
    pub fn reset_reannotate(&mut self) {
        if self.reannotate.is_some() {
            self.reset_reannotate.set(true);
        }
    }

    /// Clears the saved Update from Library settings on Apply.
    pub fn reset_library_update(&mut self) {
        if self.library_update.is_some() {
            self.reset_library_update.set(true);
        }
    }

    pub fn reannotate_reset_pending(&self) -> bool {
        *self.reset_reannotate.value()
    }

    pub fn library_update_reset_pending(&self) -> bool {
        *self.reset_library_update.value()
    }
}

/// The changed options as `schematic.options.set` params.
fn option_params(base: &SchematicOptions, value: &SchematicOptions) -> Map<String, Value> {
    let mut params = Map::new();
    let mut put = |name: &str, changed: bool, v: Value| {
        if changed {
            params.insert(name.to_owned(), v);
        }
    };
    put(
        "allow_ports_to_name_nets",
        base.allow_ports_to_name_nets != value.allow_ports_to_name_nets,
        json!(value.allow_ports_to_name_nets),
    );
    put(
        "power_port_names_priority",
        base.power_port_names_priority != value.power_port_names_priority,
        json!(value.power_port_names_priority),
    );
    put(
        "append_sheet_numbers",
        base.append_sheet_numbers != value.append_sheet_numbers,
        json!(value.append_sheet_numbers),
    );
    put(
        "cross_references",
        base.cross_references != value.cross_references,
        json!(value.cross_references),
    );
    put(
        "cross_reference_sheet_style",
        base.cross_reference_styles.sheet != value.cross_reference_styles.sheet,
        json!(value.cross_reference_styles.sheet),
    );
    put(
        "cross_reference_location_style",
        base.cross_reference_styles.location != value.cross_reference_styles.location,
        json!(value.cross_reference_styles.location),
    );
    put(
        "cross_references_follow_preferences",
        base.cross_references_follow_preferences != value.cross_references_follow_preferences,
        json!(value.cross_references_follow_preferences),
    );
    put(
        "not_fitted_display",
        base.not_fitted_display != value.not_fitted_display,
        json!(value.not_fitted_display),
    );
    params
}

/// The saved Update from Library settings as summary lines.
pub fn library_update_summary(s: &LibraryUpdateSettings) -> Vec<String> {
    let line =
        |name: String, value: String| tr!("reannotate-summary-line", name = name, value = value);
    let fields = match &s.fields_to_update {
        None => tr!("settings-schematic-lu-fields-all"),
        Some(fields) => fields.join(", "),
    };
    vec![
        line(
            tr!("settings-schematic-lu-library-field-missing"),
            match s.library_field_missing {
                LibraryFieldMissing::Add => tr!("settings-schematic-lu-add"),
                LibraryFieldMissing::AddIfNotBlank => tr!("settings-schematic-lu-add-if-not-blank"),
                LibraryFieldMissing::Skip => tr!("settings-schematic-lu-skip"),
            },
        ),
        line(
            tr!("settings-schematic-lu-component-field-missing"),
            match s.component_field_missing {
                ComponentFieldMissing::Keep => tr!("settings-schematic-lu-keep"),
                ComponentFieldMissing::Remove => tr!("settings-schematic-lu-remove"),
            },
        ),
        line(
            tr!("settings-schematic-lu-locally-edited"),
            match s.locally_edited {
                LocallyEdited::Keep => tr!("settings-schematic-lu-keep"),
                LocallyEdited::TakeNew => tr!("settings-schematic-lu-take-new"),
            },
        ),
        line(tr!("settings-schematic-lu-fields"), fields),
        line(
            tr!("settings-schematic-lu-new-footprints"),
            match s.add_new_footprints {
                true => tr!("settings-schematic-lu-added"),
                false => tr!("settings-schematic-lu-left-out"),
            },
        ),
        line(
            tr!("settings-schematic-lu-missing-footprints"),
            match s.remove_missing_footprints {
                true => tr!("settings-schematic-lu-removed"),
                false => tr!("settings-schematic-lu-kept"),
            },
        ),
        line(
            tr!("settings-schematic-lu-footprint-choice"),
            match s.footprint_choice {
                FootprintChoice::KeepIfPresent => tr!("settings-schematic-lu-keep-if-present"),
                FootprintChoice::Default => tr!("settings-schematic-lu-new-default"),
            },
        ),
        line(
            tr!("settings-schematic-lu-field-locations"),
            match s.preserve_field_locations {
                true => tr!("settings-schematic-lu-preserved"),
                false => tr!("settings-schematic-lu-autoposition"),
            },
        ),
        line(
            tr!("settings-schematic-lu-field-visibility"),
            match s.preserve_field_visibility {
                true => tr!("settings-schematic-lu-preserved"),
                false => tr!("settings-schematic-lu-designator-value"),
            },
        ),
    ]
}

impl Page for SchematicPage {
    fn title(&self) -> String {
        tr!("settings-page-schematic")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        let props = project.props();
        self.options.load(props.schematic, reset);
        self.reannotate =
            (props.reannotate.as_ref()).map(|s| reannotate_labels::summary(project, s));
        self.library_update = props.library_update.as_ref().map(library_update_summary);
        // A Reset only stands while there is something saved to clear.
        self.reset_reannotate
            .load(false, reset || self.reannotate.is_none());
        self.reset_library_update
            .load(false, reset || self.library_update.is_none());
    }

    fn steps(&self) -> Vec<Step> {
        let mut steps = Vec::new();
        let params = option_params(self.options.base(), self.options.value());
        if !params.is_empty() {
            steps.push(Step::new("schematic.options.set", Value::Object(params)));
        }
        let mut cleared = Vec::new();
        if *self.reset_reannotate.value() {
            cleared.push("reannotate");
        }
        if *self.reset_library_update.value() {
            cleared.push("library_update");
        }
        if !cleared.is_empty() {
            steps.push(Step::new(
                "schematic.saved_settings.reset",
                json!({ "settings": cleared }),
            ));
        }
        steps
    }

    fn has_defaults(&self) -> bool {
        true
    }

    /// The default options, and both saved settings cleared.
    fn reset_to_defaults(&mut self, _shell: &mut crate::shell::Shell) {
        self.options.set(SchematicOptions::DEFAULT);
        self.reset_reannotate();
        self.reset_library_update();
    }

    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }

    fn show(&mut self, ui: &mut Ui, _cx: &mut PageCx<'_>) {
        let options = self.options.value_mut();
        heading(ui, tr!("settings-schematic-nets"));
        ui.checkbox(
            &mut options.allow_ports_to_name_nets,
            tr!("settings-schematic-allow-ports"),
        );
        ui.checkbox(
            &mut options.power_port_names_priority,
            tr!("settings-schematic-power-priority"),
        );
        ui.checkbox(
            &mut options.append_sheet_numbers,
            tr!("settings-schematic-append-sheet-numbers"),
        );

        heading(ui, tr!("settings-schematic-cross-references"));
        ui.checkbox(
            &mut options.cross_references,
            tr!("settings-schematic-cross-references-on"),
        );
        let follow = options.cross_references_follow_preferences;
        egui::Grid::new("settings-schematic-styles")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.add_enabled_ui(!follow, |ui| {
                    ui.label(tr!("settings-schematic-sheet-style"));
                });
                ui.add_enabled_ui(!follow, |ui| {
                    let styles = &mut options.cross_reference_styles;
                    choice(
                        ui,
                        tr!("settings-schematic-sheet-style"),
                        &mut styles.sheet,
                        &[
                            (SheetStyle::None, tr!("settings-schematic-style-none")),
                            (SheetStyle::Name, tr!("settings-schematic-sheet-name")),
                            (SheetStyle::Number, tr!("settings-schematic-sheet-number")),
                        ],
                    );
                });
                ui.end_row();
                ui.add_enabled_ui(!follow, |ui| {
                    ui.label(tr!("settings-schematic-location-style"));
                });
                ui.add_enabled_ui(!follow, |ui| {
                    let styles = &mut options.cross_reference_styles;
                    choice(
                        ui,
                        tr!("settings-schematic-location-style"),
                        &mut styles.location,
                        &[
                            (LocationStyle::None, tr!("settings-schematic-style-none")),
                            (LocationStyle::Zone, tr!("settings-schematic-location-zone")),
                            (LocationStyle::Xy, tr!("settings-schematic-location-xy")),
                        ],
                    );
                });
                ui.end_row();
            });
        ui.checkbox(
            &mut options.cross_references_follow_preferences,
            tr!("settings-schematic-follow-preferences"),
        );

        heading(ui, tr!("settings-schematic-assembly"));
        ui.horizontal(|ui| {
            ui.label(tr!("settings-schematic-not-fitted"));
            choice(
                ui,
                tr!("settings-schematic-not-fitted"),
                &mut options.not_fitted_display,
                &[
                    (
                        NotFittedDisplay::Cross,
                        tr!("settings-schematic-not-fitted-cross"),
                    ),
                    (
                        NotFittedDisplay::Mesh,
                        tr!("settings-schematic-not-fitted-mesh"),
                    ),
                    (NotFittedDisplay::None, tr!("settings-schematic-style-none")),
                ],
            );
        });

        heading(ui, tr!("settings-schematic-reannotate"));
        if summary(
            ui,
            self.reannotate.as_deref(),
            *self.reset_reannotate.value(),
            tr!("settings-schematic-reset-reannotate"),
        ) {
            self.reset_reannotate();
        }
        heading(ui, tr!("settings-schematic-library-update"));
        if summary(
            ui,
            self.library_update.as_deref(),
            *self.reset_library_update.value(),
            tr!("settings-schematic-reset-library-update"),
        ) {
            self.reset_library_update();
        }
    }
}

fn heading(ui: &mut Ui, text: String) {
    ui.add_space(6.0);
    ui.strong(text);
}

/// A row of selectable buttons, one per choice. The accessible name of each is
/// "<group>: <choice>".
fn choice<T: Copy + PartialEq>(ui: &mut Ui, group: String, value: &mut T, choices: &[(T, String)]) {
    ui.horizontal(|ui| {
        for (option, label) in choices {
            let chosen = *value == *option;
            let button = ui.add(egui::Button::selectable(chosen, label));
            let name = format!("{group}: {label}");
            button.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::RadioButton, ui.is_enabled(), chosen, &name)
            });
            if button.clicked() {
                *value = *option;
            }
        }
    });
}

/// A read-only summary with its Reset button. Returns whether Reset was clicked.
fn summary(ui: &mut Ui, lines: Option<&[String]>, pending: bool, reset: String) -> bool {
    let Some(lines) = lines else {
        ui.weak(tr!("settings-schematic-none-saved"));
        return false;
    };
    ui.indent(reset.as_str(), |ui| {
        for line in lines {
            ui.label(line);
        }
    });
    if pending {
        ui.weak(tr!("settings-schematic-reset-pending"));
        return false;
    }
    ui.button(reset).clicked()
}
