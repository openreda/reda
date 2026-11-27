//! Design ▸ Save Board as Template…: the name, description, category and target Library,
//! and the board objects to capture with their ticks (`library.board_template.save`).
//!
//! The rows are [`candidates`](reda_engine::commands::board_template::candidates) with
//! their defaults: the outline always, Locked Components, Mechanical Parts and Free Pads,
//! every cutout and Keepout but inner-layer ones (listed, unselectable, with the reason),
//! and both drawing Layers. Save sends exactly the ticked objects.

use std::any::Any;

use egui::{Id, Role, Ui, WidgetInfo};
use reda_engine::command::CommandError;
use reda_engine::commands::board_template::{Candidate, CandidateKind, candidates};
use reda_engine::model::{Side, TemplateCopperLayer, Uuid};
use serde_json::{Value, json};

use crate::command_dialog::{Outcome, ParamsDialog};
use crate::settings::layout;
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets;

pub const COMMAND: &str = "library.board_template.save";

/// One row of the capture list.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureRow {
    pub candidate: Candidate,
    /// What the row says, such as "J1 Socket_2x20 (bottom)".
    pub label: String,
    pub ticked: bool,
}

impl CaptureRow {
    /// Whether its tick can change: not the outline, nor an object that can't be saved.
    pub fn enabled(&self) -> bool {
        self.candidate.kind != CandidateKind::Outline && self.candidate.unavailable.is_none()
    }
}

/// The Save Board as Template dialog.
pub struct SaveTemplateDialog {
    pub name: String,
    pub description: String,
    pub category: String,
    pub library: Option<Uuid>,
    rows: Vec<CaptureRow>,
    error: Option<String>,
    started: bool,
}

impl SaveTemplateDialog {
    /// The dialog on the Shell's board, with what the call gave.
    pub fn new(shell: &Shell, given: &Value) -> SaveTemplateDialog {
        let text = |key: &str| given[key].as_str().unwrap_or_default().to_owned();
        let listed = shell.engine().project().map(candidates).unwrap_or_default();
        let mut counts = std::collections::BTreeMap::new();
        let rows = listed
            .into_iter()
            .map(|candidate| {
                let n = counts.entry(candidate.kind as u8).or_insert(0);
                *n += 1;
                let label = label(&candidate, *n);
                CaptureRow {
                    ticked: candidate.default,
                    candidate,
                    label,
                }
            })
            .collect();
        SaveTemplateDialog {
            name: text("name"),
            description: text("description"),
            category: text("category"),
            library: given["library"].as_str().and_then(|s| s.parse().ok()),
            rows,
            error: None,
            started: false,
        }
    }

    /// The capture list, grouped as it shows.
    pub fn rows(&self) -> &[CaptureRow] {
        &self.rows
    }

    /// Ticks or unticks the row of `object`, when its tick can change.
    pub fn set_ticked(&mut self, object: Uuid, on: bool) {
        for row in &mut self.rows {
            if row.candidate.object == object && row.enabled() {
                row.ticked = on;
            }
        }
    }

    /// The warnings of the ticked rows, such as local Pad edits that won't be saved.
    pub fn warnings(&self) -> Vec<&str> {
        self.rows
            .iter()
            .filter(|r| r.ticked)
            .filter_map(|r| r.candidate.warning.as_deref())
            .collect()
    }

    /// The params Save runs with, once there is a name, a Library and an outline.
    pub fn params(&self) -> Option<Value> {
        let name = self.name.trim();
        let has_outline = (self.rows.iter()).any(|r| r.candidate.kind == CandidateKind::Outline);
        if name.is_empty() || !has_outline {
            return None;
        }
        let objects: Vec<Uuid> = (self.rows.iter())
            .filter(|r| r.ticked && r.candidate.kind != CandidateKind::Outline)
            .map(|r| r.candidate.object)
            .collect();
        let mut params = json!({"library": self.library?, "name": name, "objects": objects});
        if !self.description.trim().is_empty() {
            params["description"] = json!(self.description.trim());
        }
        if !self.category.trim().is_empty() {
            params["category"] = json!(self.category.trim());
        }
        Some(params)
    }

    fn text_row(ui: &mut Ui, label: String, text: &mut String) {
        ui.add(widgets::form_name(&label));
        let response = ui.add(widgets::text_field(text).width(super::FIELD_WIDTH));
        response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
        ui.end_row();
    }

    fn capture_list(&mut self, ui: &mut Ui) {
        let mut group = None;
        for row in &mut self.rows {
            let heading = heading(row.candidate.kind);
            if group != Some(heading.clone()) {
                ui.add_space(ui.tokens().space.stack);
                layout::text(ui, &heading);
                group = Some(heading);
            }
            ui.horizontal(|ui| {
                let enabled = row.enabled();
                let mut on = row.ticked;
                let response = ui.add_enabled(enabled, widgets::checkbox(&mut on, &row.label));
                let label = row.label.clone();
                response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, on, &label));
                if response.clicked() && enabled {
                    row.ticked = on;
                }
                if let Some(why) = &row.candidate.unavailable {
                    let t = ui.tokens();
                    ui.label(t.ty.body.rich(why).color(t.color.text.muted));
                }
            });
            if let (true, Some(warning)) = (row.ticked, &row.candidate.warning) {
                layout::warning(ui, warning);
            }
        }
    }
}

/// A row's group heading.
fn heading(kind: CandidateKind) -> String {
    match kind {
        CandidateKind::Outline => tr!("save-template-group-outline"),
        CandidateKind::Cutout => tr!("save-template-group-cutouts"),
        CandidateKind::Keepout => tr!("save-template-group-keepouts"),
        CandidateKind::Component => tr!("save-template-group-components"),
        CandidateKind::MechanicalPart => tr!("save-template-group-mechanical"),
        CandidateKind::FreePad => tr!("save-template-group-free-pads"),
        CandidateKind::Drawings => tr!("save-template-group-drawings"),
    }
}

/// What the row of `c`, the `n`th of its kind, says.
fn label(c: &Candidate, n: usize) -> String {
    let n = n as i64;
    match c.kind {
        CandidateKind::Outline => tr!("save-template-outline"),
        CandidateKind::Cutout => tr!("save-template-cutout", n = n),
        CandidateKind::Keepout => {
            let layer = match c.layer {
                Some(TemplateCopperLayer::AllCopper) => tr!("save-template-layer-all"),
                Some(TemplateCopperLayer::TopCopper) => tr!("save-template-layer-top"),
                Some(TemplateCopperLayer::BottomCopper) => tr!("save-template-layer-bottom"),
                None => tr!("save-template-layer-inner"),
            };
            tr!("save-template-keepout", n = n, layer = layer)
        }
        CandidateKind::Component | CandidateKind::MechanicalPart => {
            let side = match c.side {
                Some(Side::Bottom) => tr!("save-template-side-bottom"),
                _ => tr!("save-template-side-top"),
            };
            tr!(
                "save-template-placed",
                designator = c.designator.clone().unwrap_or_default(),
                footprint = c.footprint.clone().unwrap_or_default(),
                side = side
            )
        }
        CandidateKind::FreePad => tr!("save-template-free-pad", n = n),
        CandidateKind::Drawings => tr!(
            "save-template-drawings",
            layer = c.layer_name.clone().unwrap_or_default()
        ),
    }
}

impl ParamsDialog for SaveTemplateDialog {
    fn command(&self) -> &str {
        COMMAND
    }

    fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        if !self.started {
            self.started = true;
            let given = self.library.map_or(Value::Null, |l| json!(l));
            self.library = super::initial_library(shell, &given);
        }
        let mut outcome = Outcome::Open;
        let id = Id::NULL.with("save-board-template");
        // The list scrolls, leaving room for the fields above and the buttons below.
        let height = (ctx.content_rect().height() * 0.45).clamp(160.0, 420.0);
        let modal = crate::modal::Dialog::new(id, tr!("save-template-title"))
            .width(520.0)
            .show(ctx, |ui| {
                widgets::form("save-template-fields").show(ui, |ui| {
                    Self::text_row(ui, tr!("save-template-name"), &mut self.name);
                    Self::text_row(ui, tr!("save-template-description"), &mut self.description);
                    Self::text_row(ui, tr!("save-template-category"), &mut self.category);
                    super::library_picker(ui, shell, id.with("library"), &mut self.library);
                    ui.end_row();
                });
                if super::writable_libraries(shell).is_empty() {
                    layout::warning(ui, tr!("library-target-missing"));
                }
                egui::ScrollArea::vertical()
                    .id_salt("save-template-rows")
                    .max_height(height)
                    .show(ui, |ui| {
                        if self.rows.is_empty() {
                            layout::warning(ui, tr!("save-template-no-board"));
                        }
                        self.capture_list(ui);
                    });
                let no_outline = !self.rows.is_empty()
                    && !(self.rows.iter()).any(|r| r.candidate.kind == CandidateKind::Outline);
                if no_outline {
                    layout::error(ui, tr!("save-template-no-outline"));
                }
                if let Some(error) = &self.error {
                    layout::error(ui, error);
                }
                crate::modal::buttons(ui, |ui| {
                    let params = self.params();
                    let save = ui.add_enabled(
                        params.is_some(),
                        widgets::button(tr!("save-template-run")).primary(),
                    );
                    if let Some(params) = params
                        && save.clicked()
                    {
                        outcome = Outcome::Run(params);
                    }
                    if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                        outcome = Outcome::Cancel;
                    }
                });
            });
        if modal.should_close() && outcome == Outcome::Open {
            outcome = Outcome::Cancel;
        }
        outcome
    }

    fn failed(&mut self, error: &CommandError) {
        self.error = Some(error.message.clone());
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
