//! Replace Part: the Library panel's picker of Parts, with the current Part on top and the
//! pin differences under it (`schematic.component.replace_preview`), then
//! `schematic.component.replace_part` for every Component it opened on. For a generic
//! Part (one with no MPN) the Component's value seeds the search.
//!
//! The same picker is Choose Component on page 2 of Update from Library, where it only
//! returns the Part chosen.

use egui::{Id, Role, WidgetInfo};
use reda_engine::command::CommandError;
use reda_engine::library::{LibraryIndex, SearchQuery};
use reda_engine::model::Uuid;
use serde_json::json;

use super::update::{Section, describe_change};
use crate::shell::Shell;
use crate::tr;

/// One Part the picker offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickRow {
    pub part: Uuid,
    pub name: String,
    /// Its Value, when it says more than the name.
    pub value: String,
    pub library: String,
    /// The Part the Components use now.
    pub current: bool,
}

/// Which of the Component's values a Replace keeps (`keep_fields`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeepFields {
    #[default]
    Edited,
    All,
    None,
}

impl KeepFields {
    const ALL: [KeepFields; 3] = [KeepFields::Edited, KeepFields::All, KeepFields::None];

    fn id(self) -> &'static str {
        match self {
            KeepFields::Edited => "edited",
            KeepFields::All => "all",
            KeepFields::None => "none",
        }
    }

    fn label(self) -> String {
        match self {
            KeepFields::Edited => tr!("replace-keep-edited"),
            KeepFields::All => tr!("replace-keep-all"),
            KeepFields::None => tr!("replace-keep-none"),
        }
    }
}

/// What a frame of the dialog ended with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Open,
    Closed,
    /// The picker's Part (Choose Component).
    Chosen(Uuid),
}

/// The Replace Part dialog, or Choose Component's picker.
pub struct ReplaceDialog {
    components: Vec<Uuid>,
    /// The Part the Components use now, when they share one.
    current: Option<Uuid>,
    pub search: String,
    index: LibraryIndex,
    rows: Vec<PickRow>,
    chosen: Option<Uuid>,
    pub keep_fields: KeepFields,
    /// The chosen Part's pin and Unit differences, and its problems.
    differences: Vec<String>,
    blocked: bool,
    /// Only picks a Part (Choose Component), without replacing.
    picker: bool,
    error: Option<String>,
}

impl std::fmt::Debug for ReplaceDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplaceDialog")
            .field("components", &self.components)
            .field("chosen", &self.chosen)
            .finish_non_exhaustive()
    }
}

impl ReplaceDialog {
    /// Replace… on `components` (the Properties panel's Part row).
    pub fn new(shell: &mut Shell, components: Vec<Uuid>) -> ReplaceDialog {
        let mut dialog = ReplaceDialog {
            components,
            current: None,
            search: String::new(),
            index: shell.engine().libraries(),
            rows: Vec::new(),
            chosen: None,
            keep_fields: KeepFields::default(),
            differences: Vec::new(),
            blocked: false,
            picker: false,
            error: None,
        };
        dialog.seed(shell);
        dialog.refresh();
        dialog
    }

    /// Choose Component's picker for `components`.
    pub(crate) fn picker(shell: &mut Shell, components: Vec<Uuid>) -> ReplaceDialog {
        ReplaceDialog {
            picker: true,
            ..ReplaceDialog::new(shell, components)
        }
    }

    /// The current Part, and for a generic one the Component's value as the search.
    fn seed(&mut self, shell: &Shell) {
        let Some(project) = shell.engine().project() else {
            return;
        };
        let mut parts = Vec::new();
        let mut value = None;
        for sheet in project
            .documents()
            .filter_map(|d| project.schematic_sheet(d.uuid))
        {
            for unit in sheet
                .units()
                .filter(|u| self.components.contains(&u.component.uuid))
            {
                if let Some(entry) = sheet.part(unit.part) {
                    if !parts.contains(&entry.part.uuid) {
                        parts.push(entry.part.uuid);
                    }
                    if entry.part.fields.mpn.is_empty() && !unit.fields.value.is_empty() {
                        value.get_or_insert_with(|| unit.fields.value.clone());
                    }
                }
            }
        }
        if let [one] = parts.as_slice() {
            self.current = Some(*one);
        }
        if let Some(value) = value.filter(|v| !v.contains(char::is_whitespace)) {
            self.search = format!("value:{value}");
        }
    }

    /// Lists the Parts for the search again: the current Part first, then the hits.
    pub fn refresh(&mut self) {
        let query = SearchQuery {
            text: self.search.clone(),
            ..SearchQuery::default()
        };
        let row = |library: &reda_engine::library::Library, part: &reda_engine::model::Part| {
            PickRow {
                part: part.uuid,
                name: part.name.clone(),
                value: if part.fields.value != part.name {
                    part.fields.value.clone()
                } else {
                    String::new()
                },
                library: library.info.name.clone(),
                current: Some(part.uuid) == self.current,
            }
        };
        let mut rows: Vec<PickRow> = Vec::new();
        if let Some(current) = self.current
            && let Some((library, part)) = self.index.parts().find(|(_, p)| p.uuid == current)
        {
            rows.push(row(library, part));
        }
        for hit in self.index.search(&query) {
            if hit.part.symbol.is_some() && Some(hit.part.uuid) != self.current {
                rows.push(row(hit.library, hit.part));
            }
        }
        self.rows = rows;
    }

    /// The Parts offered, the current one first.
    pub fn rows(&self) -> &[PickRow] {
        &self.rows
    }

    pub fn chosen(&self) -> Option<Uuid> {
        self.chosen
    }

    /// Chooses `part` and previews the Replace: its pin and Unit differences show.
    pub fn choose(&mut self, shell: &mut Shell, part: Uuid) {
        self.chosen = Some(part);
        self.differences.clear();
        self.blocked = false;
        self.error = None;
        match shell.run(
            "schematic.component.replace_preview",
            json!({"components": self.components, "part": part,
                   "keep_fields": self.keep_fields.id()}),
        ) {
            Ok(out) => {
                for c in out["components"].as_array().into_iter().flatten() {
                    let designator = c["designator"].as_str().unwrap_or_default();
                    for change in c["changes"].as_array().into_iter().flatten() {
                        let (section, text) = describe_change(change);
                        if matches!(section, Section::Pins | Section::Units) {
                            self.differences.push(format!("{designator}: {text}"));
                        }
                    }
                    for problem in c["problems"].as_array().into_iter().flatten() {
                        let text = super::update::describe_problem(problem);
                        self.differences.push(format!("{designator}: {text}"));
                    }
                    self.blocked |= c["blocked"].as_bool().unwrap_or(false);
                }
            }
            Err(e) => {
                self.blocked = true;
                self.error = Some(e.message);
            }
        }
    }

    /// The chosen Part's pin and Unit differences, and its problems, in words.
    pub fn differences(&self) -> &[String] {
        &self.differences
    }

    /// Replaces the Components with the chosen Part.
    pub fn apply(&mut self, shell: &mut Shell) -> Result<(), CommandError> {
        let Some(part) = self.chosen else {
            return Ok(());
        };
        shell.run_reporting(
            "schematic.component.replace_part",
            json!({"components": self.components, "part": part,
                   "keep_fields": self.keep_fields.id()}),
        )?;
        Ok(())
    }

    fn can_apply(&self) -> bool {
        self.chosen.is_some_and(|c| Some(c) != self.current) && !self.blocked
    }

    /// Draws the dialog for one frame.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        let mut clicked = None;
        let mut searched = false;
        let mut done = (false, false);
        let title = if self.picker {
            tr!("replace-choose-title")
        } else {
            tr!("replace-title")
        };
        let modal = crate::modal::Dialog::new(Id::NULL.with(("replace-part", self.picker)), title)
            .width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.label(tr!("replace-search"));
                    searched = ui
                        .text_edit_singleline(&mut self.search)
                        .labelled_by(label.id)
                        .changed();
                });
                egui::ScrollArea::vertical()
                    .id_salt("replace-parts")
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for row in &self.rows {
                            let mut text = row.name.clone();
                            if !row.value.is_empty() {
                                text += &format!(" · {}", row.value);
                            }
                            text += &format!("  ({})", row.library);
                            if row.current {
                                text += &tr!("replace-current");
                            }
                            let chosen = self.chosen == Some(row.part);
                            let response = ui.selectable_label(chosen, text);
                            let name = row.name.clone();
                            response.widget_info(|| {
                                WidgetInfo::selected(Role::ListItem, true, chosen, &name)
                            });
                            if response.clicked() {
                                clicked = Some(row.part);
                            }
                        }
                    });
                if !self.picker {
                    egui::ComboBox::from_label(tr!("replace-keep-fields"))
                        .selected_text(self.keep_fields.label())
                        .show_ui(ui, |ui| {
                            for keep in KeepFields::ALL {
                                ui.selectable_value(&mut self.keep_fields, keep, keep.label());
                            }
                        });
                }
                ui.label(egui::RichText::new(tr!("replace-differences")).strong());
                if self.chosen.is_some() && self.differences.is_empty() {
                    ui.weak(tr!("replace-no-differences"));
                }
                for text in &self.differences {
                    ui.label(text);
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                let action = if self.picker {
                    tr!("replace-choose")
                } else {
                    tr!("replace-apply")
                };
                done = super::buttons(
                    ui,
                    &action,
                    self.can_apply(),
                    &tr!("replace-pick-a-part"),
                );
            });
        if searched {
            self.refresh();
        }
        if let Some(part) = clicked {
            self.choose(shell, part);
        }
        let (run, cancel) = done;
        if run && let Some(part) = self.chosen {
            if self.picker {
                return Outcome::Chosen(part);
            }
            match self.apply(shell) {
                Ok(()) => return Outcome::Closed,
                Err(e) => self.error = Some(e.message),
            }
        }
        if cancel || modal.should_close() {
            Outcome::Closed
        } else {
            Outcome::Open
        }
    }
}
