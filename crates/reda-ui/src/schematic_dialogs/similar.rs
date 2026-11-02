//! Select Similar: selects the Units that match the selected one on the chosen fields,
//! on its sheet or across the Project (`schematic.select_similar`, then `selection.set`).

use egui::{Id, Modal};
use reda_engine::command::CommandError;
use reda_engine::model::{Project, UnitInstance, Uuid};
use serde_json::{Value, json};

use super::buttons;
use crate::shell::Shell;
use crate::tr;

/// One field the Units can be compared on.
#[derive(Clone, Debug, PartialEq)]
pub struct Criterion {
    /// What the dialog shows: the field's name and the reference Unit's value.
    pub label: String,
    /// The field as `schematic.select_similar` takes it (`"value"`, `{"field": "LCSC"}`).
    pub param: Value,
    pub checked: bool,
}

/// The Select Similar dialog, comparing with one reference Unit.
#[derive(Clone, Debug)]
pub struct SimilarDialog {
    /// The reference Unit: the first Unit selected.
    pub unit: Uuid,
    /// Its Designator, for the title.
    pub designator: String,
    criteria: Vec<Criterion>,
    /// Across every sheet rather than the reference Unit's.
    pub project: bool,
    error: Option<String>,
}

impl SimilarDialog {
    /// The dialog comparing with the first selected Unit; `None` when no Unit is selected.
    /// It starts matching on the Part.
    pub fn new(shell: &Shell) -> Option<SimilarDialog> {
        let project = shell.engine().project()?;
        let unit = shell
            .selection()
            .objects
            .iter()
            .find_map(|o| unit(project, *o))?;
        let fields = &unit.fields;
        let mut criteria = vec![
            criterion(tr!("similar-part"), json!("part"), true),
            criterion(
                tr!("similar-value", value = fields.value.clone()),
                json!("value"),
                false,
            ),
            criterion(tr!("similar-footprint"), json!("footprint"), false),
            criterion(tr!("similar-prefix"), json!("designator_prefix"), false),
            criterion(tr!("similar-unit"), json!("unit"), false),
            criterion(
                tr!("similar-component-type"),
                json!("component_type"),
                false,
            ),
            criterion(tr!("similar-not-fitted"), json!("not_fitted"), false),
        ];
        let built_in = [
            ("mpn", &fields.mpn),
            ("manufacturer", &fields.manufacturer),
            ("description", &fields.description),
        ];
        for (name, value) in built_in {
            if !value.is_empty() {
                criteria.push(field_criterion(name, value));
            }
        }
        for field in &fields.custom {
            criteria.push(field_criterion(&field.name, &field.value));
        }
        Some(SimilarDialog {
            unit: unit.uuid,
            designator: unit.designator.clone(),
            criteria,
            project: false,
            error: None,
        })
    }

    /// The fields the dialog offers, with whether each is ticked.
    pub fn criteria(&self) -> &[Criterion] {
        &self.criteria
    }

    /// Ticks or unticks the field `param` (as in [`Criterion::param`]).
    pub fn set(&mut self, param: &Value, on: bool) {
        for c in &mut self.criteria {
            if c.param == *param {
                c.checked = on;
            }
        }
    }

    /// Whether at least one field is ticked.
    pub fn can_apply(&self) -> bool {
        self.criteria.iter().any(|c| c.checked)
    }

    /// Selects the matching Units. Returns how many.
    pub fn apply(&mut self, shell: &mut Shell) -> Result<usize, CommandError> {
        let on: Vec<&Value> = self
            .criteria
            .iter()
            .filter(|c| c.checked)
            .map(|c| &c.param)
            .collect();
        let scope = if self.project { "project" } else { "sheet" };
        let out = shell.run(
            "schematic.select_similar",
            json!({"unit": self.unit, "match": on, "scope": scope}),
        )?;
        let units = out["units"].clone();
        let count = units.as_array().map_or(0, Vec::len);
        shell.run("selection.set", json!({ "objects": units }))?;
        Ok(count)
    }

    /// Draws the dialog. Returns whether it is still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        let mut result = (false, false);
        let modal = Modal::new(Id::NULL.with("schematic-select-similar")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(tr!("similar-title", designator = self.designator.clone()));
            ui.label(tr!("similar-match"));
            for c in &mut self.criteria {
                ui.checkbox(&mut c.checked, &c.label);
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.project, false, tr!("similar-scope-sheet"));
                ui.radio_value(&mut self.project, true, tr!("similar-scope-project"));
            });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            result = buttons(
                ui,
                &tr!("similar-select"),
                self.can_apply(),
                &tr!("similar-pick-a-field"),
            );
        });
        let (run, cancel) = result;
        if run {
            match self.apply(shell) {
                Ok(_) => return false,
                Err(e) => self.error = Some(e.message),
            }
        }
        !(cancel || modal.should_close())
    }
}

fn criterion(label: String, param: Value, checked: bool) -> Criterion {
    Criterion {
        label,
        param,
        checked,
    }
}

fn field_criterion(name: &str, value: &str) -> Criterion {
    criterion(
        tr!(
            "similar-field",
            name = name.to_owned(),
            value = value.to_owned()
        ),
        json!({ "field": name }),
        false,
    )
}

/// The placed Unit `uuid`, on any sheet.
fn unit(project: &Project, uuid: Uuid) -> Option<&UnitInstance> {
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .find_map(|s| s.unit(uuid))
}
