//! Update from Library: a modal dialog with two pages, as in the leading suite.
//!
//! - **Page 1** ([`UpdateDialog::sheets`], [`UpdateDialog::parts`]): the sheets with All On
//!   and All Off, the Parts with their instance counts, Selected Parts Only, the aspects
//!   and the settings. The settings are saved with the Project when the update applies.
//! - **Page 2** ([`UpdateDialog::rows`]): a row per Component that is not Up to Date, with
//!   its status and aspect ticks, Choose Component (a Replace) and Return Selected to
//!   Default, and the sections of the preview with the field grid. Update Selected and
//!   Reject Selected work on the selected cells. A Locally Edited Footprint the update
//!   would revert shows its local edits, compared with an overlay drawing.
//!
//! Nothing changes before Apply: page 2 is `schematic.component.update_preview`, and
//! Apply runs `schematic.component.update_from_library` with its hash, then
//! `schematic.component.replace_part` for each Component given another Part.

mod describe;
mod draw;

use std::collections::{BTreeMap, BTreeSet};

use reda_engine::command::CommandError;
use reda_engine::model::library_update::LibraryUpdateSettings;
use reda_engine::model::local_edit::LocalEdit;
use reda_engine::model::{Footprint, Object, Project, Uuid};
use reda_engine::schematic::{Aspect, LibraryStatus};
use serde_json::{Value, json};

use crate::shell::Shell;

pub use describe::Section;
pub(crate) use describe::{change as describe_change, problem as describe_problem};
pub(crate) use draw::{local_edits as local_edits_compare, status_text};

/// What the dialog opens on: the whole Project, or narrowed by its entry point.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateScope {
    /// The selected Components (Component ▸ Update from Library…, the Properties badge):
    /// Selected Parts Only starts ticked with these.
    pub components: Vec<Uuid>,
    /// Only these Parts start ticked (the Library panel row, a Library editor).
    pub parts: Vec<Uuid>,
}

impl UpdateScope {
    /// The scope of the selected objects: the Components of the selected Units (or of the
    /// selected Footprints synced to one).
    pub fn of_selection(project: &Project, objects: &[Uuid]) -> UpdateScope {
        let mut components = Vec::new();
        for object in objects {
            let component = match project.object(*object) {
                Some(Object::Unit(u)) => Some(u.object.component.uuid),
                Some(Object::FootprintInstance(f)) => f.object.component.as_ref().map(|c| c.uuid),
                _ => None,
            };
            if let Some(c) = component.filter(|c| !components.contains(c)) {
                components.push(c);
            }
        }
        UpdateScope {
            components,
            parts: Vec::new(),
        }
    }
}

/// Which page shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Setup,
    Preview,
}

/// A sheet on page 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetRow {
    pub sheet: Uuid,
    pub name: String,
    pub on: bool,
}

/// A Part on page 1, with how many Components use it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartRow {
    pub part: Uuid,
    pub name: String,
    /// Placed Components of this Part.
    pub count: usize,
    /// How many of them are not Up to Date.
    pub outdated: usize,
    pub on: bool,
}

/// One cell of the field grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridCell {
    /// A field's name, or `component_type`, `in_bom`, `in_cpl` or `not_fitted`.
    pub cell: String,
    /// `changed`, `same`, `edited`, `added`, `removed`, `component_only`, `locked` or
    /// `cleared`.
    pub case: String,
    pub old: String,
    pub new: String,
    /// The Component's value.
    pub component: String,
    /// How the preview starts it: `update`, `reject` or `none`.
    pub start: String,
    /// Whether Update Selected and Reject Selected can change it.
    pub overridable: bool,
}

/// Another Part chosen for a Component on page 2 (Choose Component).
#[derive(Clone, Debug, PartialEq)]
pub struct Replacement {
    pub part: Uuid,
    pub part_name: String,
    /// The Replace preview, in words, by section.
    pub changes: Vec<(Section, String)>,
    pub problems: Vec<String>,
    pub blocked: bool,
}

/// One Component of page 2.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentRow {
    pub component: Uuid,
    pub designator: String,
    pub part_name: String,
    pub status: LibraryStatus,
    /// For Source Missing: what is missing.
    pub reason: Option<String>,
    /// The aspects that differ.
    pub differs: Vec<Aspect>,
    /// The aspects taken.
    pub aspects: Vec<Aspect>,
    /// The preview in words, by section.
    pub changes: Vec<(Section, String)>,
    /// The field grid, when the Fields aspect is taken.
    pub cells: Vec<GridCell>,
    pub problems: Vec<String>,
    /// A Problem stops this Component, or it is Source Missing.
    pub blocked: bool,
    /// The local edits on the board the update removes.
    pub lost_local_edits: Vec<LocalEdit>,
    /// Whether Apply updates it.
    pub included: bool,
}

/// The Update from Library dialog.
#[derive(Debug)]
pub struct UpdateDialog {
    page: Page,
    pub sheets: Vec<SheetRow>,
    pub parts: Vec<PartRow>,
    /// The Components of the entry point's selection, for Selected Parts Only.
    selection: Vec<Uuid>,
    pub selected_only: bool,
    /// The aspects every Component takes; all three is Full Replace.
    pub aspects: BTreeSet<Aspect>,
    /// The settings, from the Project's saved ones; saved again on Apply.
    pub settings: LibraryUpdateSettings,
    /// Every cell name a preview of the Project has, for the `fields_to_update` checklist.
    pub field_names: Vec<String>,
    rows: Vec<ComponentRow>,
    preview_hash: String,
    /// Aspects chosen for single Components on page 2.
    component_aspects: BTreeMap<Uuid, Vec<Aspect>>,
    /// Cells set by Update Selected (`true`) or Reject Selected (`false`).
    overrides: BTreeMap<(Uuid, String), bool>,
    /// Another Part chosen for a Component.
    replacements: BTreeMap<Uuid, Replacement>,
    /// The Components selected on page 2, in pick order.
    selected_rows: Vec<Uuid>,
    /// The cells selected on page 2.
    selected_cells: BTreeSet<(Uuid, String)>,
    /// The Component whose preview sections show.
    shown: Option<Uuid>,
    /// The Choose Component picker, while it is open.
    picker: Option<super::ReplaceDialog>,
    error: Option<String>,
}

impl UpdateDialog {
    /// The dialog on page 1 for `scope`, with the Project's saved settings.
    pub fn new(shell: &mut Shell, scope: UpdateScope) -> UpdateDialog {
        let mut dialog = UpdateDialog {
            page: Page::Setup,
            sheets: Vec::new(),
            parts: Vec::new(),
            selected_only: !scope.components.is_empty(),
            selection: scope.components,
            aspects: Aspect::ALL.into_iter().collect(),
            settings: LibraryUpdateSettings::default(),
            field_names: Vec::new(),
            rows: Vec::new(),
            preview_hash: String::new(),
            component_aspects: BTreeMap::new(),
            overrides: BTreeMap::new(),
            replacements: BTreeMap::new(),
            selected_rows: Vec::new(),
            selected_cells: BTreeSet::new(),
            shown: None,
            picker: None,
            error: None,
        };
        if let Some(project) = shell.engine().project() {
            dialog.settings = project.props().library_update.clone().unwrap_or_default();
            dialog.sheets = project
                .documents()
                .filter(|d| project.schematic_sheet(d.uuid).is_some())
                .map(|d| SheetRow {
                    sheet: d.uuid,
                    name: if d.title.is_empty() {
                        d.name.clone()
                    } else {
                        d.title.clone()
                    },
                    on: true,
                })
                .collect();
        }
        match shell.run("schematic.component.library_status", json!({})) {
            Ok(out) => {
                for c in out["components"].as_array().into_iter().flatten() {
                    let Some(part) = uuid(&c["part"]) else {
                        continue;
                    };
                    let outdated = usize::from(c["status"] != "up_to_date");
                    match dialog.parts.iter_mut().find(|p| p.part == part) {
                        Some(row) => {
                            row.count += 1;
                            row.outdated += outdated;
                        }
                        None => dialog.parts.push(PartRow {
                            part,
                            name: c["part_name"].as_str().unwrap_or_default().to_owned(),
                            count: 1,
                            outdated,
                            on: scope.parts.is_empty() || scope.parts.contains(&part),
                        }),
                    }
                }
                dialog.parts.sort_by(|a, b| a.name.cmp(&b.name));
            }
            Err(e) => dialog.error = Some(e.message),
        }
        // Every cell the Project's update would show, for the checklist.
        if let Ok(out) = shell.run(
            "schematic.component.update_preview",
            json!({"aspects": ["fields"]}),
        ) {
            let mut names = BTreeSet::new();
            for c in out["components"].as_array().into_iter().flatten() {
                for cell in c["cells"].as_array().into_iter().flatten() {
                    if let Some(name) = cell["cell"].as_str() {
                        names.insert(name.to_owned());
                    }
                }
            }
            dialog.field_names = names.into_iter().collect();
        }
        dialog
    }

    pub fn page(&self) -> Page {
        self.page
    }

    /// Ticks or unticks every sheet (All On, All Off).
    pub fn all_sheets(&mut self, on: bool) {
        for sheet in &mut self.sheets {
            sheet.on = on;
        }
    }

    /// Whether Selected Parts Only can be ticked: the dialog opened on a selection.
    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// Ticks or unticks `aspect` for every Component.
    pub fn set_aspect(&mut self, aspect: Aspect, on: bool) {
        if on {
            self.aspects.insert(aspect);
        } else {
            self.aspects.remove(&aspect);
        }
    }

    /// Puts the cell `name` on the `fields_to_update` checklist or takes it off; with
    /// every name on it, the list is "every field" again.
    pub fn set_field_updated(&mut self, name: &str, on: bool) {
        let mut names: Vec<String> = match &self.settings.fields_to_update {
            Some(names) => names.clone(),
            None => self.field_names.clone(),
        };
        names.retain(|n| n != name);
        if on {
            names.push(name.to_owned());
        }
        names.sort();
        let every = self.field_names.iter().all(|n| names.contains(n));
        self.settings.fields_to_update = (!every).then_some(names);
    }

    /// The parameters the preview and the update share. Every setting is passed, so Apply
    /// saves them with the Project.
    pub fn params(&self) -> Value {
        let mut p = json!({
            "aspects": self.aspects,
            "settings": serde_json::to_value(&self.settings).unwrap_or_else(|_| json!({})),
        });
        if self.selected_only && !self.selection.is_empty() {
            p["components"] = json!(self.selection);
        }
        if self.sheets.iter().any(|s| !s.on) {
            let on: Vec<Uuid> = (self.sheets.iter())
                .filter(|s| s.on)
                .map(|s| s.sheet)
                .collect();
            p["sheets"] = json!(on);
        }
        if self.parts.iter().any(|s| !s.on) {
            let on: Vec<Uuid> = (self.parts.iter())
                .filter(|s| s.on)
                .map(|s| s.part)
                .collect();
            p["parts"] = json!(on);
        }
        if !self.component_aspects.is_empty() {
            p["component_aspects"] = (self.component_aspects.iter())
                .map(|(c, aspects)| json!({"component": c, "aspects": aspects}))
                .collect();
        }
        p
    }

    /// Page 2: previews the update chosen on page 1.
    pub fn next(&mut self, shell: &mut Shell) {
        self.page = Page::Preview;
        self.preview(shell);
    }

    /// Back to page 1. What was chosen on page 2 is kept for the rows that stay.
    pub fn back(&mut self) {
        self.page = Page::Setup;
    }

    /// Runs the preview again, keeping what page 2 chose.
    fn preview(&mut self, shell: &mut Shell) {
        self.error = None;
        let excluded: BTreeSet<Uuid> = (self.rows.iter())
            .filter(|r| !r.included)
            .map(|r| r.component)
            .collect();
        self.rows.clear();
        match shell.run("schematic.component.update_preview", self.params()) {
            Ok(out) => {
                self.preview_hash = out["preview_hash"].as_str().unwrap_or_default().to_owned();
                for c in out["components"].as_array().into_iter().flatten() {
                    if let Some(mut row) = component_row(c) {
                        row.included = !row.blocked && !excluded.contains(&row.component);
                        self.rows.push(row);
                    }
                }
            }
            Err(e) => self.error = Some(e.message),
        }
        let present: BTreeSet<Uuid> = self.rows.iter().map(|r| r.component).collect();
        self.selected_rows.retain(|c| present.contains(c));
        self.selected_cells.retain(|(c, _)| present.contains(c));
        self.overrides.retain(|(c, _), _| present.contains(c));
        self.replacements.retain(|c, _| present.contains(c));
        if self.shown.is_none_or(|c| !present.contains(&c)) {
            self.shown = self.rows.first().map(|r| r.component);
        }
    }

    /// The Components that are not Up to Date, by Designator.
    pub fn rows(&self) -> &[ComponentRow] {
        &self.rows
    }

    pub fn row(&self, component: Uuid) -> Option<&ComponentRow> {
        self.rows.iter().find(|r| r.component == component)
    }

    /// Ticks or unticks `component` for Apply; a blocked one can't be ticked.
    pub fn set_included(&mut self, component: Uuid, on: bool) {
        for row in &mut self.rows {
            if row.component == component && (!row.blocked || !on) {
                row.included = on;
            }
        }
    }

    /// Ticks or unticks one aspect of one Component, and previews again.
    pub fn set_component_aspect(
        &mut self,
        shell: &mut Shell,
        component: Uuid,
        aspect: Aspect,
        on: bool,
    ) {
        let Some(row) = self.row(component) else {
            return;
        };
        let mut aspects = (self.component_aspects.get(&component).cloned())
            .unwrap_or_else(|| self.aspects.iter().copied().collect());
        // An aspect that doesn't differ can't be taken, so start from what the row shows.
        if !self.component_aspects.contains_key(&component) {
            aspects.retain(|a| row.differs.contains(a));
        }
        aspects.retain(|a| *a != aspect);
        if on {
            aspects.push(aspect);
        }
        aspects.sort();
        self.component_aspects.insert(component, aspects);
        self.preview(shell);
    }

    /// Whether `component` has aspects of its own, set on page 2.
    pub fn has_own_aspects(&self, component: Uuid) -> bool {
        self.component_aspects.contains_key(&component)
    }

    /// Selects the row of `component`: alone, or added to the selection. Its sections
    /// show.
    pub fn select_row(&mut self, component: Uuid, add: bool) {
        if !add {
            self.selected_rows.clear();
        }
        if !self.selected_rows.contains(&component) {
            self.selected_rows.push(component);
        }
        self.shown = Some(component);
    }

    /// The selected rows' Components.
    pub fn selected_rows(&self) -> &[Uuid] {
        &self.selected_rows
    }

    /// The Component whose preview sections show.
    pub fn shown(&self) -> Option<&ComponentRow> {
        self.shown.and_then(|c| self.row(c))
    }

    /// Selects a cell: alone, or added to the selection.
    pub fn select_cell(&mut self, component: Uuid, cell: &str, add: bool) {
        if !add {
            self.selected_cells.clear();
        }
        self.selected_cells.insert((component, cell.to_owned()));
    }

    pub fn cell_selected(&self, component: Uuid, cell: &str) -> bool {
        self.selected_cells.contains(&(component, cell.to_owned()))
    }

    /// What a cell does now: the preview's start, or Update or Reject Selected over it.
    pub fn action(&self, component: Uuid, cell: &GridCell) -> String {
        match self.overrides.get(&(component, cell.cell.clone())) {
            Some(true) => "update".to_owned(),
            Some(false) => "reject".to_owned(),
            None => cell.start.clone(),
        }
    }

    fn set_selected_cells(&mut self, update: bool) {
        for (component, name) in &self.selected_cells {
            let overridable = (self.rows.iter())
                .find(|r| r.component == *component)
                .and_then(|r| r.cells.iter().find(|c| c.cell == *name))
                .is_some_and(|c| c.overridable);
            if overridable {
                self.overrides.insert((*component, name.clone()), update);
            }
        }
    }

    /// Update Selected: the selected cells take the new default (Locked ones stay).
    pub fn update_selected(&mut self) {
        self.set_selected_cells(true);
    }

    /// Reject Selected: the selected cells keep the Component's value.
    pub fn reject_selected(&mut self) {
        self.set_selected_cells(false);
    }

    /// The rows Choose Component and Return Selected to Default work on: the selected
    /// ones, or the shown one.
    fn chosen_rows(&self) -> Vec<Uuid> {
        if self.selected_rows.is_empty() {
            self.shown.into_iter().collect()
        } else {
            self.selected_rows.clone()
        }
    }

    /// Opens Choose Component's picker for the chosen rows.
    pub fn open_picker(&mut self, shell: &mut Shell) {
        let rows = self.chosen_rows();
        if !rows.is_empty() {
            self.picker = Some(super::ReplaceDialog::picker(shell, rows));
        }
    }

    /// Choose Component: the chosen rows take `part` instead, with the Replace preview in
    /// their sections.
    pub fn choose_component(&mut self, shell: &mut Shell, part: Uuid) {
        let components = self.chosen_rows();
        if components.is_empty() {
            return;
        }
        match shell.run(
            "schematic.component.replace_preview",
            json!({"components": components, "part": part}),
        ) {
            Ok(out) => {
                let part_name = out["part_name"].as_str().unwrap_or_default().to_owned();
                for c in out["components"].as_array().into_iter().flatten() {
                    let Some(component) = uuid(&c["component"]) else {
                        continue;
                    };
                    let (changes, problems, blocked) = preview_parts(c);
                    self.replacements.insert(
                        component,
                        Replacement {
                            part,
                            part_name: part_name.clone(),
                            changes,
                            problems,
                            blocked,
                        },
                    );
                }
            }
            Err(e) => self.error = Some(e.message),
        }
    }

    /// The Part chosen for `component` on page 2, if any.
    pub fn replacement(&self, component: Uuid) -> Option<&Replacement> {
        self.replacements.get(&component)
    }

    /// Return Selected to Default: the chosen rows lose their own aspects, chosen Part
    /// and cell overrides, and take the update as page 1 set it.
    pub fn return_selected_to_default(&mut self, shell: &mut Shell) {
        let mut again = false;
        for c in self.chosen_rows() {
            self.replacements.remove(&c);
            self.overrides.retain(|(component, _), _| *component != c);
            again |= self.component_aspects.remove(&c).is_some();
            self.set_included(c, true);
        }
        if again {
            self.preview(shell);
        }
    }

    /// Whether Apply would change anything.
    pub fn can_apply(&self) -> bool {
        self.page == Page::Preview
            && self.rows.iter().any(|r| {
                r.included
                    && match self.replacements.get(&r.component) {
                        Some(p) => !p.blocked,
                        None => !r.aspects.is_empty(),
                    }
            })
    }

    /// Applies page 2: the update as previewed, then each Replace.
    pub fn apply(&mut self, shell: &mut Shell) -> Result<(), CommandError> {
        let replaced = |r: &ComponentRow| self.replacements.contains_key(&r.component);
        let excluded: Vec<Uuid> = (self.rows.iter())
            .filter(|r| !r.included || replaced(r))
            .map(|r| r.component)
            .collect();
        let (mut updated, mut rejected) = (Vec::new(), Vec::new());
        for row in &self.rows {
            for cell in &row.cells {
                let now = self.action(row.component, cell);
                if now == cell.start {
                    continue;
                }
                let r = json!({"component": row.component, "cell": cell.cell});
                match now.as_str() {
                    "update" => updated.push(r),
                    "reject" => rejected.push(r),
                    _ => {}
                }
            }
        }
        let updating = (self.rows.iter())
            .any(|r| r.included && !replaced(r) && !r.aspects.is_empty());
        if updating {
            let mut params = self.params();
            params["preview_hash"] = json!(self.preview_hash);
            params["excluded_components"] = json!(excluded);
            params["excluded_cells"] = json!(rejected);
            params["updated_cells"] = json!(updated);
            shell.run_reporting("schematic.component.update_from_library", params)?;
        }
        let mut by_part: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
        for row in self.rows.iter().filter(|r| r.included) {
            if let Some(r) = self.replacements.get(&row.component) {
                by_part.entry(r.part).or_default().push(row.component);
            }
        }
        for (part, components) in by_part {
            shell.run_reporting(
                "schematic.component.replace_part",
                json!({"components": components, "part": part}),
            )?;
        }
        Ok(())
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// The Footprint the board holds for `component` as edited, and the copy it was edited
/// from: what the local-edit compare draws.
pub fn edited_footprint(shell: &Shell, component: Uuid) -> Option<(Footprint, Footprint)> {
    let pcb = shell.engine().project()?.pcb()?;
    let instance = pcb
        .footprints()
        .find(|f| f.component.as_ref().is_some_and(|c| c.uuid == component))?;
    edited_copy(shell, instance.uuid)
}

/// The Footprint instance `instance` as edited on the board, and the copy it was edited
/// from.
pub fn edited_copy(shell: &Shell, instance: Uuid) -> Option<(Footprint, Footprint)> {
    let pcb = shell.engine().project()?.pcb()?;
    let copy = pcb.footprint_copy(pcb.footprint(instance)?.footprint)?;
    let original = pcb.footprint_copy(copy.edited_from.as_ref()?.uuid)?;
    Some((copy.footprint.clone(), original.footprint.clone()))
}

fn uuid(v: &Value) -> Option<Uuid> {
    v.as_str().and_then(|u| u.parse().ok())
}

fn aspects(v: &Value) -> Vec<Aspect> {
    serde_json::from_value(v.clone()).unwrap_or_default()
}

/// The changes, problems and whether it is blocked, of one preview row.
fn preview_parts(c: &Value) -> (Vec<(Section, String)>, Vec<String>, bool) {
    let changes = (c["changes"].as_array().into_iter().flatten())
        .map(describe::change)
        .collect();
    let problems = (c["problems"].as_array().into_iter().flatten())
        .map(describe::problem)
        .collect();
    (changes, problems, c["blocked"].as_bool().unwrap_or(false))
}

/// One Component of `schematic.component.update_preview`.
fn component_row(c: &Value) -> Option<ComponentRow> {
    let component = uuid(&c["component"])?;
    let status = match c["status"].as_str() {
        Some("source_missing") => LibraryStatus::SourceMissing,
        Some("up_to_date") => LibraryStatus::UpToDate,
        _ => LibraryStatus::LibraryDiffers,
    };
    let (mut changes, problems, blocked) = preview_parts(c);
    if let Some(reason) = c["reason"].as_str() {
        changes.push((
            Section::Fields,
            crate::tr!("library-update-source-missing", reason = reason.to_owned()),
        ));
    }
    let cells = (c["cells"].as_array().into_iter().flatten())
        .map(|cell| {
            let s = |key: &str| cell[key].as_str().unwrap_or_default().to_owned();
            let case = s("case");
            GridCell {
                overridable: !matches!(case.as_str(), "locked" | "cleared" | "same"),
                cell: s("cell"),
                case,
                old: s("old"),
                new: s("new"),
                component: s("component"),
                start: s("action"),
            }
        })
        .collect();
    let lost_local_edits =
        serde_json::from_value(c["lost_local_edits"].clone()).unwrap_or_default();
    Some(ComponentRow {
        component,
        designator: c["designator"].as_str().unwrap_or_default().to_owned(),
        part_name: c["part_name"].as_str().unwrap_or_default().to_owned(),
        reason: c["reason"].as_str().map(str::to_owned),
        differs: aspects(&c["differs"]),
        aspects: aspects(&c["aspects"]),
        changes,
        cells,
        problems,
        blocked: blocked || status == LibraryStatus::SourceMissing,
        status,
        lost_local_edits,
        included: false,
    })
}
