//! Re-annotate: renumbers Designators from the schematic or the board, after an old → new
//! preview (`schematic.reannotate.preview`). The dialog takes every setting of
//! `schematic.reannotate` (source, mode, scope with a sheet grid, order, location,
//! threshold, start indices, sides), opens with the last settings applied, and has a lock
//! switch on each preview row (`schematic.component.set`). [`ReannotateLast`] is
//! Re-annotate (last settings): a count to confirm, then `{saved: true}`.

pub mod labels;

use egui::Id;
use reda_engine::command::CommandError;
use reda_engine::model::annotate::{
    Location, Mode, Order, ReannotateSettings, ScopeKind, Sides, Source,
};
use reda_engine::model::geom::Nm;
use reda_engine::model::{Project, Uuid};
use reda_engine::schematic::natural_cmp;
use serde_json::{Value, json};

use self::labels::StartIndices;
use super::buttons;
use crate::shell::Shell;
use crate::tr;

/// One preview row: a Component in the scope, old → new.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub component: Uuid,
    pub old: String,
    pub new: String,
    /// Its Designator is locked, so it keeps it.
    pub locked: bool,
}

impl Rename {
    pub fn changed(&self) -> bool {
        self.old != self.new
    }
}

/// How the preview rows are sorted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowSort {
    /// Processing order, locked rows last.
    #[default]
    Order,
    /// Changed rows first.
    Changed,
    /// Locked rows first.
    Locked,
}

/// A row of the sheet grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetRow {
    pub sheet: Uuid,
    pub title: String,
    /// Its number in the Projects panel, from 1.
    pub number: usize,
    /// Ticked for scope Sheets.
    pub included: bool,
    /// Where its numbering starts.
    pub start: u64,
}

/// The Re-annotate dialog.
#[derive(Clone, Debug)]
pub struct ReannotateDialog {
    /// The settings as chosen. `per_sheet` follows [`ReannotateDialog::start_indices`] and
    /// `threshold` the threshold text.
    pub settings: ReannotateSettings,
    /// Show only the rows that change (and the locked ones).
    pub only_changed: bool,
    pub sort: RowSort,
    start: StartIndices,
    threshold: String,
    threshold_problem: Option<String>,
    /// Every Schematic Sheet, in Projects panel order, with its title.
    sheets: Vec<(Uuid, String)>,
    rows: Vec<Rename>,
    kept_duplicates: Vec<String>,
    /// Why the preview or Apply failed.
    error: Option<String>,
}

impl ReannotateDialog {
    /// The dialog with the last settings applied, or the defaults (the whole Project, from
    /// the schematic, across then down), and its preview.
    pub fn new(shell: &mut Shell) -> ReannotateDialog {
        let (settings, sheets) = match shell.engine().project() {
            Some(project) => (
                (project.props().reannotate.clone())
                    .unwrap_or_else(|| ReannotateSettings::new(Source::Schematic)),
                (labels::sheets(project).into_iter())
                    .map(|s| {
                        let title = project.document(s).map(|d| d.title.clone());
                        (s, title.unwrap_or_default())
                    })
                    .collect(),
            ),
            None => (ReannotateSettings::new(Source::Schematic), Vec::new()),
        };
        let ids: Vec<Uuid> = sheets.iter().map(|(s, _)| *s).collect();
        let mut dialog = ReannotateDialog {
            start: StartIndices::of(&settings.per_sheet, &ids),
            threshold: labels::length(settings.threshold),
            threshold_problem: None,
            settings,
            only_changed: true,
            sort: RowSort::default(),
            sheets,
            rows: Vec::new(),
            kept_duplicates: Vec::new(),
            error: None,
        };
        // A sheet scope from before keeps only the sheets still there.
        dialog.settings.sheets.retain(|s| ids.contains(s));
        dialog.refresh(shell);
        dialog
    }

    /// Switches the source; the order goes to that source's default.
    pub fn set_source(&mut self, source: Source) {
        if self.settings.source != source {
            self.settings.source = source;
            self.settings.order = Order::default_for(source);
        }
    }

    /// The orders the current source offers, its default first.
    pub fn orders(&self) -> [Order; 4] {
        labels::orders(self.settings.source)
    }

    pub fn start_indices(&self) -> StartIndices {
        self.start
    }

    pub fn set_start_indices(&mut self, start: StartIndices) {
        self.start = start;
        if start != StartIndices::Custom {
            let ids: Vec<Uuid> = self.sheets.iter().map(|(s, _)| *s).collect();
            self.settings.per_sheet = start.per_sheet(&ids);
        }
    }

    /// The threshold as typed.
    pub fn threshold_text(&self) -> &str {
        &self.threshold
    }

    /// Takes a typed threshold, such as `100mil` or `2.5 mm`.
    pub fn set_threshold(&mut self, text: &str) {
        self.threshold = text.to_owned();
        match Nm::parse(text.trim()) {
            Ok(nm) if nm.0 >= 0 => {
                self.settings.threshold = nm;
                self.threshold_problem = None;
            }
            _ => self.threshold_problem = Some(tr!("reannotate-threshold-invalid")),
        }
    }

    pub fn threshold_problem(&self) -> Option<&str> {
        self.threshold_problem.as_deref()
    }

    /// Ticks or unticks a sheet for scope Sheets.
    pub fn set_sheet(&mut self, sheet: Uuid, included: bool) {
        self.settings.sheets.retain(|s| *s != sheet);
        if included {
            self.settings.sheets.push(sheet);
            let order: Vec<Uuid> = self.sheets.iter().map(|(s, _)| *s).collect();
            (self.settings.sheets).sort_by_key(|s| order.iter().position(|o| o == s));
        }
    }

    /// The sheet grid: every sheet with its number, tick and start index.
    pub fn sheet_rows(&self) -> Vec<SheetRow> {
        (self.sheets.iter().enumerate())
            .map(|(i, (sheet, title))| SheetRow {
                sheet: *sheet,
                title: title.clone(),
                number: i + 1,
                included: self.settings.sheets.contains(sheet),
                start: self.settings.start_for(Some(*sheet)),
            })
            .collect()
    }

    /// The params `schematic.reannotate` takes for the dialog's choices.
    pub fn params(&self, shell: &Shell) -> Value {
        let s = &self.settings;
        let mut params = json!({
            "source": s.source,
            "mode": s.mode,
            "scope": s.scope,
            "order": s.order,
            "threshold": s.threshold.0,
        });
        match s.source {
            Source::Schematic => {
                params["location"] = json!(s.location);
                if !s.per_sheet.is_empty() {
                    params["per_sheet"] = json!(s.per_sheet);
                }
            }
            Source::Board => {
                params["sides"] = json!(s.sides);
                params["start_index"] = json!(s.start_index);
            }
        }
        match s.scope {
            ScopeKind::Project => {}
            ScopeKind::Sheets => params["sheets"] = json!(s.sheets),
            ScopeKind::Selection => params["selection"] = json!(selected(shell)),
        }
        params
    }

    /// Works the preview out again (`schematic.reannotate.preview`), with the locked
    /// Components in the scope.
    pub fn refresh(&mut self, shell: &mut Shell) {
        self.rows.clear();
        self.kept_duplicates.clear();
        self.error = None;
        if self.settings.scope == ScopeKind::Selection && selected(shell).is_empty() {
            self.error = Some(tr!("reannotate-nothing-selected"));
            return;
        }
        if self.settings.scope == ScopeKind::Sheets && self.settings.sheets.is_empty() {
            self.error = Some(tr!("reannotate-no-sheets"));
            return;
        }
        let params = self.params(shell);
        match shell.run("schematic.reannotate.preview", params) {
            Ok(out) => {
                for r in out["designators"].as_array().into_iter().flatten() {
                    let Some(component) = r["component"].as_str().and_then(|u| u.parse().ok())
                    else {
                        continue;
                    };
                    self.rows.push(Rename {
                        component,
                        old: r["old"].as_str().unwrap_or_default().to_owned(),
                        new: r["new"].as_str().unwrap_or_default().to_owned(),
                        locked: false,
                    });
                }
                for d in out["kept_duplicates"].as_array().into_iter().flatten() {
                    if let Some(designator) = d["designator"].as_str() {
                        self.kept_duplicates.push(designator.to_owned());
                    }
                }
            }
            Err(e) => self.error = Some(e.message),
        }
        let locked = self.locked_in_scope(shell);
        self.rows.extend(locked);
    }

    /// The preview rows to show: all, or only the changed and locked ones, sorted.
    pub fn rows(&self) -> Vec<&Rename> {
        let mut rows: Vec<&Rename> = (self.rows.iter())
            .filter(|r| !self.only_changed || r.changed() || r.locked)
            .collect();
        match self.sort {
            RowSort::Order => rows.sort_by_key(|r| r.locked),
            RowSort::Changed => rows.sort_by_key(|r| !r.changed()),
            RowSort::Locked => rows.sort_by_key(|r| !r.locked),
        }
        rows
    }

    /// The Components the preview renumbers, in processing order.
    pub fn changes(&self) -> impl Iterator<Item = &Rename> {
        self.rows.iter().filter(|r| r.changed())
    }

    /// The Designators still shared after the run by locked or out-of-scope Components.
    pub fn kept_duplicates(&self) -> &[String] {
        &self.kept_duplicates
    }

    /// Locks or unlocks `component`'s Designator (an undoable edit, at once) and
    /// refreshes the preview.
    pub fn set_locked(&mut self, shell: &mut Shell, component: Uuid, locked: bool) {
        let params = json!({"components": [component], "designator_locked": locked});
        if let Err(e) = shell.run_reporting("schematic.component.set", params) {
            self.error = Some(e.message);
            return;
        }
        self.refresh(shell);
    }

    /// Whether Apply would renumber anything.
    pub fn can_apply(&self) -> bool {
        self.error.is_none() && self.threshold_problem.is_none() && self.changes().next().is_some()
    }

    /// Renumbers as previewed (`schematic.reannotate`), which saves the settings.
    pub fn apply(&mut self, shell: &mut Shell) -> Result<(), CommandError> {
        let params = self.params(shell);
        shell.run_reporting("schematic.reannotate", params)?;
        Ok(())
    }

    fn locked_in_scope(&self, shell: &Shell) -> Vec<Rename> {
        let Some(project) = shell.engine().project() else {
            return Vec::new();
        };
        let selected = selected(shell);
        let mut out: Vec<Rename> = Vec::new();
        for document in project.documents() {
            let Some(sheet) = project.schematic_sheet(document.uuid) else {
                continue;
            };
            let in_scope = match self.settings.scope {
                ScopeKind::Project | ScopeKind::Selection => true,
                ScopeKind::Sheets => self.settings.sheets.contains(&document.uuid),
            };
            if !in_scope {
                continue;
            }
            for unit in sheet.units().filter(|u| u.designator_locked) {
                let picked = self.settings.scope != ScopeKind::Selection
                    || selected
                        .iter()
                        .any(|s| unit_component(project, *s) == Some(unit.component.uuid));
                if picked && !out.iter().any(|l| l.component == unit.component.uuid) {
                    out.push(Rename {
                        component: unit.component.uuid,
                        old: unit.designator.clone(),
                        new: unit.designator.clone(),
                        locked: true,
                    });
                }
            }
        }
        out.sort_by(|a, b| natural_cmp(&a.old, &b.old));
        out
    }

    /// Draws the dialog. Returns whether it is still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        let before = self.settings.clone();
        let mut lock = None;
        let mut result = (false, false);
        let modal = crate::modal::Dialog::new(
            Id::NULL.with("schematic-reannotate"),
            tr!("reannotate-title"),
        )
        .width(580.0)
        .show(ctx, |ui| {
            self.options(ui);
            ui.separator();
            self.sheet_grid(ui);
            ui.separator();
            lock = self.preview(ui);
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            result = buttons(
                ui,
                &tr!("reannotate-apply"),
                self.can_apply(),
                &tr!("reannotate-no-changes"),
            );
        });
        if self.settings != before {
            self.refresh(shell);
        }
        if let Some((component, locked)) = lock {
            self.set_locked(shell, component, locked);
        }
        let (run, cancel) = result;
        if run {
            match self.apply(shell) {
                Ok(()) => return false,
                Err(e) => self.error = Some(e.message),
            }
        }
        !(cancel || modal.should_close())
    }

    fn options(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("reannotate-options")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label(tr!("reannotate-source"));
                let mut source = self.settings.source;
                radios(
                    ui,
                    &mut source,
                    &[Source::Schematic, Source::Board],
                    labels::source,
                );
                self.set_source(source);
                ui.end_row();

                ui.label(tr!("reannotate-mode"));
                radios(
                    ui,
                    &mut self.settings.mode,
                    &[Mode::RenumberAll, Mode::FixDuplicatesOnly],
                    labels::mode,
                );
                ui.end_row();

                ui.label(tr!("reannotate-scope"));
                radios(
                    ui,
                    &mut self.settings.scope,
                    &[ScopeKind::Project, ScopeKind::Sheets, ScopeKind::Selection],
                    |s| match s {
                        ScopeKind::Project => tr!("reannotate-scope-project"),
                        ScopeKind::Sheets => tr!("reannotate-scope-sheets"),
                        ScopeKind::Selection => tr!("reannotate-scope-selection"),
                    },
                );
                ui.end_row();

                ui.label(tr!("reannotate-order"));
                let orders = self.orders();
                combo(
                    ui,
                    tr!("reannotate-order"),
                    &mut self.settings.order,
                    &orders,
                    labels::order,
                );
                ui.end_row();

                ui.label(tr!("reannotate-threshold"));
                let mut text = self.threshold.clone();
                let field = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(100.0)
                        .id_salt("reannotate-threshold"),
                );
                let name = tr!("reannotate-threshold");
                field.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, &name));
                if text != self.threshold {
                    self.set_threshold(&text);
                }
                ui.end_row();
                if let Some(problem) = &self.threshold_problem {
                    ui.label("");
                    ui.colored_label(ui.visuals().error_fg_color, problem);
                    ui.end_row();
                }

                match self.settings.source {
                    Source::Schematic => {
                        ui.label(tr!("reannotate-location"));
                        radios(
                            ui,
                            &mut self.settings.location,
                            &[Location::Part, Location::Designator],
                            labels::location,
                        );
                        ui.end_row();
                        ui.label(tr!("reannotate-start-indices"));
                        let mut start = self.start;
                        let mut offered = StartIndices::OFFERED.to_vec();
                        if start == StartIndices::Custom {
                            offered.push(StartIndices::Custom);
                        }
                        radios(ui, &mut start, &offered, StartIndices::label);
                        if start != self.start {
                            self.set_start_indices(start);
                        }
                        ui.end_row();
                    }
                    Source::Board => {
                        ui.label(tr!("reannotate-sides"));
                        radios(
                            ui,
                            &mut self.settings.sides,
                            &[Sides::TopThenBottom, Sides::Top, Sides::Bottom],
                            labels::sides,
                        );
                        ui.end_row();
                        ui.label(tr!("reannotate-start-index"));
                        let name = tr!("reannotate-start-index");
                        let drag = ui.add(
                            egui::DragValue::new(&mut self.settings.start_index).range(1..=999_999),
                        );
                        drag.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::SpinButton, true, &name)
                        });
                        ui.end_row();
                    }
                }
            });
    }

    /// The sheets with their numbers, a tick for scope Sheets, and where each starts.
    fn sheet_grid(&mut self, ui: &mut egui::Ui) {
        let sheets_scope = self.settings.scope == ScopeKind::Sheets;
        let schematic = self.settings.source == Source::Schematic;
        let mut toggled = None;
        egui::ScrollArea::vertical()
            .id_salt("reannotate-sheets")
            .max_height(120.0)
            .show(ui, |ui| {
                egui::Grid::new("reannotate-sheet-grid")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.weak(tr!("reannotate-sheet-number"));
                        ui.weak(tr!("reannotate-sheet"));
                        ui.weak(tr!("reannotate-sheet-start"));
                        ui.end_row();
                        for row in self.sheet_rows() {
                            ui.label(row.number.to_string());
                            let mut included = row.included || !sheets_scope;
                            let check = ui.add_enabled(
                                sheets_scope,
                                egui::Checkbox::new(&mut included, &row.title),
                            );
                            if check.changed() {
                                toggled = Some((row.sheet, included));
                            }
                            match schematic {
                                true => ui.label(row.start.to_string()),
                                false => ui.weak("-"),
                            };
                            ui.end_row();
                        }
                    });
            });
        if let Some((sheet, included)) = toggled {
            self.set_sheet(sheet, included);
        }
    }

    /// The old → new rows with Only changed, the sort and a lock switch on each row.
    /// Returns a lock switch flipped this frame.
    fn preview(&mut self, ui: &mut egui::Ui) -> Option<(Uuid, bool)> {
        let mut lock = None;
        ui.horizontal(|ui| {
            ui.strong(tr!("reannotate-preview"));
            ui.checkbox(&mut self.only_changed, tr!("reannotate-only-changed"));
            ui.label(tr!("reannotate-sort"));
            radios(
                ui,
                &mut self.sort,
                &[RowSort::Order, RowSort::Changed, RowSort::Locked],
                |s| match s {
                    RowSort::Order => tr!("reannotate-sort-order"),
                    RowSort::Changed => tr!("reannotate-sort-changed"),
                    RowSort::Locked => tr!("reannotate-sort-locked"),
                },
            );
        });
        if !self.kept_duplicates.is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr!(
                    "reannotate-kept-duplicates",
                    designators = self.kept_duplicates.join(", ")
                ),
            );
        }
        egui::ScrollArea::vertical()
            .id_salt("reannotate-preview")
            .max_height(220.0)
            .show(ui, |ui| {
                let rows = self.rows();
                if !rows.iter().any(|r| r.changed()) && self.error.is_none() {
                    ui.weak(tr!("reannotate-no-changes"));
                }
                egui::Grid::new("reannotate-rows")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for r in rows {
                            let text =
                                tr!("reannotate-row", old = r.old.clone(), new = r.new.clone());
                            match r.changed() {
                                true => ui.label(text),
                                false => ui.weak(text),
                            };
                            let mut locked = r.locked;
                            let name = tr!("reannotate-lock-row", designator = r.old.clone());
                            let check = ui.checkbox(&mut locked, tr!("reannotate-lock"));
                            check.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::Role::CheckBox,
                                    true,
                                    locked,
                                    &name,
                                )
                            });
                            if check.changed() {
                                lock = Some((r.component, locked));
                            }
                            ui.end_row();
                        }
                    });
            });
        lock
    }
}

/// One radio button per choice.
fn radios<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    choices: &[T],
    label: impl Fn(T) -> String,
) {
    ui.horizontal(|ui| {
        for choice in choices {
            ui.radio_value(value, *choice, label(*choice));
        }
    });
}

fn combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    name: String,
    value: &mut T,
    choices: &[T],
    label: impl Fn(T) -> String,
) {
    let combo = egui::ComboBox::from_id_salt(name.as_str())
        .selected_text(label(*value))
        .show_ui(ui, |ui| {
            for choice in choices {
                ui.selectable_value(value, *choice, label(*choice));
            }
        });
    combo
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &name));
}

/// The selected Units, for scope Selection.
fn selected(shell: &Shell) -> Vec<Uuid> {
    let Some(project) = shell.engine().project() else {
        return Vec::new();
    };
    (shell.selection().objects.iter().copied())
        .filter(|o| unit_component(project, *o).is_some())
        .collect()
}

/// The Component of placed Unit `uuid`, on any sheet.
fn unit_component(project: &Project, uuid: Uuid) -> Option<Uuid> {
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .find_map(|s| s.unit(uuid))
        .map(|u| u.component.uuid)
}

/// Re-annotate (last settings): how many Components the saved settings would renumber,
/// to confirm before it runs them (`schematic.reannotate {saved: true}`).
#[derive(Clone, Debug)]
pub struct ReannotateLast {
    count: usize,
    /// The selection, for saved settings with scope Selection.
    selection: Option<Vec<Uuid>>,
    error: Option<String>,
}

impl ReannotateLast {
    /// Previews the saved settings. Fails, with the reason, when none are saved or they
    /// can't run now.
    pub fn new(shell: &mut Shell) -> Result<ReannotateLast, String> {
        let saved = (shell.engine().project())
            .and_then(|p| p.props().reannotate.clone())
            .ok_or_else(|| tr!("reannotate-last-none"))?;
        let selection = (saved.scope == ScopeKind::Selection).then(|| selected(shell));
        if selection.as_ref().is_some_and(Vec::is_empty) {
            return Err(tr!("reannotate-nothing-selected"));
        }
        let mut last = ReannotateLast {
            count: 0,
            selection,
            error: None,
        };
        let out = shell
            .run("schematic.reannotate.preview", last.params())
            .map_err(|e| e.message)?;
        last.count = (out["designators"].as_array().into_iter().flatten())
            .filter(|r| r["old"] != r["new"])
            .count();
        Ok(last)
    }

    fn params(&self) -> Value {
        match &self.selection {
            Some(selection) => json!({"saved": true, "selection": selection}),
            None => json!({"saved": true}),
        }
    }

    /// How many Components it would renumber.
    pub fn count(&self) -> usize {
        self.count
    }

    /// The confirmation, with the count.
    pub fn message(&self) -> String {
        tr!("reannotate-last-confirm", count = self.count as i64)
    }

    /// Runs the saved settings.
    pub fn run(&self, shell: &mut Shell) -> Result<(), CommandError> {
        shell.run_reporting("schematic.reannotate", self.params())?;
        Ok(())
    }

    /// Draws the confirmation. Returns whether it is still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        let mut result = (false, false);
        let modal = crate::modal::Dialog::new(
            Id::NULL.with("schematic-reannotate-last"),
            tr!("reannotate-last-title"),
        )
        .width(360.0)
        .show(ctx, |ui| {
            ui.label(self.message());
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            result = buttons(
                ui,
                &tr!("reannotate-apply"),
                self.count > 0,
                &tr!("reannotate-no-changes"),
            );
        });
        let (run, cancel) = result;
        if run {
            match self.run(shell) {
                Ok(()) => return false,
                Err(e) => self.error = Some(e.message),
            }
        }
        !(cancel || modal.should_close())
    }
}
