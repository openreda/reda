//! Re-annotate: renumbers Designators in a processing order over the Project, a sheet or
//! the selection, after an old → new preview (`schematic.reannotate.preview`). The lock
//! of each Component in the scope can be switched here (`schematic.component.set`).

use egui::Id;
use reda_engine::command::CommandError;
use reda_engine::model::{Project, Uuid};
use reda_engine::schematic::natural_cmp;
use serde_json::{Value, json};

use super::buttons;
use crate::shell::Shell;
use crate::tr;

/// The order Components are numbered in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReannotateOrder {
    /// Rows from the top, each left to right.
    #[default]
    AcrossThenDown,
    /// Rows from the bottom, each left to right.
    AcrossThenUp,
    /// Columns from the left, each top to bottom.
    DownThenAcross,
    /// Columns from the left, each bottom to top.
    UpThenAcross,
    /// By where the Footprints sit on the board.
    BoardPosition,
}

impl ReannotateOrder {
    pub const ALL: [ReannotateOrder; 5] = [
        ReannotateOrder::AcrossThenDown,
        ReannotateOrder::AcrossThenUp,
        ReannotateOrder::DownThenAcross,
        ReannotateOrder::UpThenAcross,
        ReannotateOrder::BoardPosition,
    ];

    fn id(self) -> &'static str {
        match self {
            ReannotateOrder::AcrossThenDown => "across_then_down",
            ReannotateOrder::AcrossThenUp => "across_then_up",
            ReannotateOrder::DownThenAcross => "down_then_across",
            ReannotateOrder::UpThenAcross => "up_then_across",
            ReannotateOrder::BoardPosition => "board_position",
        }
    }

    fn label(self) -> String {
        match self {
            ReannotateOrder::AcrossThenDown => tr!("reannotate-across-then-down"),
            ReannotateOrder::AcrossThenUp => tr!("reannotate-across-then-up"),
            ReannotateOrder::DownThenAcross => tr!("reannotate-down-then-across"),
            ReannotateOrder::UpThenAcross => tr!("reannotate-up-then-across"),
            ReannotateOrder::BoardPosition => tr!("reannotate-board-position"),
        }
    }
}

/// Which Components to renumber.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReannotateScope {
    #[default]
    Project,
    /// The sheet active when the dialog opened.
    Sheet,
    /// The selected Units' Components.
    Selection,
}

/// One Component's Designator, old and new.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub component: Uuid,
    pub old: String,
    pub new: String,
}

/// A Component in the scope whose Designator is locked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locked {
    pub component: Uuid,
    pub designator: String,
}

/// The Re-annotate dialog.
#[derive(Clone, Debug)]
pub struct ReannotateDialog {
    pub order: ReannotateOrder,
    pub scope: ReannotateScope,
    /// The sheet for scope Sheet.
    sheet: Option<Uuid>,
    preview: Vec<Rename>,
    locked: Vec<Locked>,
    /// Why the preview or Apply failed.
    error: Option<String>,
}

impl ReannotateDialog {
    /// The dialog over the whole Project, numbering across then down.
    pub fn new(shell: &Shell) -> ReannotateDialog {
        let sheet = shell.active_document().map(|t| t.document).filter(|d| {
            (shell.engine().project()).is_some_and(|p| p.schematic_sheet(*d).is_some())
        });
        ReannotateDialog {
            order: ReannotateOrder::default(),
            scope: ReannotateScope::default(),
            sheet,
            preview: Vec::new(),
            locked: Vec::new(),
            error: None,
        }
    }

    /// The params `schematic.reannotate` takes for the dialog's choices.
    fn params(&self, shell: &Shell) -> Value {
        let mut params = json!({"order": self.order.id()});
        match self.scope {
            ReannotateScope::Project => params["scope"] = json!("project"),
            ReannotateScope::Sheet => {
                params["scope"] = json!("sheet");
                params["document"] = json!(self.sheet);
            }
            ReannotateScope::Selection => {
                params["scope"] = json!("selection");
                params["selection"] = json!(self.selected_units(shell));
            }
        }
        params
    }

    fn selected_units(&self, shell: &Shell) -> Vec<Uuid> {
        let Some(project) = shell.engine().project() else {
            return Vec::new();
        };
        shell
            .selection()
            .objects
            .iter()
            .copied()
            .filter(|o| unit_component(project, *o).is_some())
            .collect()
    }

    /// Works the preview out again (`schematic.reannotate.preview`), and the locked
    /// Components in the scope.
    pub fn refresh(&mut self, shell: &mut Shell) {
        self.preview.clear();
        self.error = None;
        self.locked = self.locked_in_scope(shell);
        if self.scope == ReannotateScope::Selection && self.selected_units(shell).is_empty() {
            self.error = Some(tr!("reannotate-nothing-selected"));
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
                    self.preview.push(Rename {
                        component,
                        old: r["old"].as_str().unwrap_or_default().to_owned(),
                        new: r["new"].as_str().unwrap_or_default().to_owned(),
                    });
                }
            }
            Err(e) => self.error = Some(e.message),
        }
    }

    /// The Components the preview renumbers, in processing order.
    pub fn changes(&self) -> impl Iterator<Item = &Rename> {
        self.preview.iter().filter(|r| r.old != r.new)
    }

    /// The locked Components in the scope, by Designator.
    pub fn locked(&self) -> &[Locked] {
        &self.locked
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
        self.error.is_none() && self.changes().next().is_some()
    }

    /// Renumbers as previewed (`schematic.reannotate`).
    pub fn apply(&mut self, shell: &mut Shell) -> Result<(), CommandError> {
        let params = self.params(shell);
        shell.run_reporting("schematic.reannotate", params)?;
        Ok(())
    }

    fn locked_in_scope(&self, shell: &Shell) -> Vec<Locked> {
        let Some(project) = shell.engine().project() else {
            return Vec::new();
        };
        let selected = self.selected_units(shell);
        let mut out: Vec<Locked> = Vec::new();
        for document in project.documents() {
            let Some(sheet) = project.schematic_sheet(document.uuid) else {
                continue;
            };
            let in_scope = match self.scope {
                ReannotateScope::Project | ReannotateScope::Selection => true,
                ReannotateScope::Sheet => self.sheet == Some(document.uuid),
            };
            if !in_scope {
                continue;
            }
            for unit in sheet.units().filter(|u| u.designator_locked) {
                let picked = self.scope != ReannotateScope::Selection
                    || selected
                        .iter()
                        .any(|s| unit_component(project, *s) == Some(unit.component.uuid));
                if picked && !out.iter().any(|l| l.component == unit.component.uuid) {
                    out.push(Locked {
                        component: unit.component.uuid,
                        designator: unit.designator.clone(),
                    });
                }
            }
        }
        out.sort_by(|a, b| natural_cmp(&a.designator, &b.designator));
        out
    }

    /// Draws the dialog. Returns whether it is still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        let (before_order, before_scope) = (self.order, self.scope);
        let mut lock = None;
        let mut result = (false, false);
        let modal = crate::modal::Dialog::new(
            Id::NULL.with("schematic-reannotate"),
            tr!("reannotate-title"),
        )
        .width(460.0)
        .show(ctx, |ui| {
            egui::Grid::new("reannotate-options")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(tr!("reannotate-order"));
                    let combo = egui::ComboBox::from_id_salt("reannotate-order")
                        .selected_text(self.order.label())
                        .show_ui(ui, |ui| {
                            for order in ReannotateOrder::ALL {
                                ui.selectable_value(&mut self.order, order, order.label());
                            }
                        });
                    combo.response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::ComboBox,
                            true,
                            tr!("reannotate-order"),
                        )
                    });
                    ui.end_row();
                    ui.label(tr!("reannotate-scope"));
                    ui.horizontal(|ui| {
                        ui.radio_value(
                            &mut self.scope,
                            ReannotateScope::Project,
                            tr!("reannotate-scope-project"),
                        );
                        if self.sheet.is_some() {
                            ui.radio_value(
                                &mut self.scope,
                                ReannotateScope::Sheet,
                                tr!("reannotate-scope-sheet"),
                            );
                        }
                        ui.radio_value(
                            &mut self.scope,
                            ReannotateScope::Selection,
                            tr!("reannotate-scope-selection"),
                        );
                    });
                    ui.end_row();
                });
            ui.separator();
            ui.strong(tr!("reannotate-preview"));
            egui::ScrollArea::vertical()
                .id_salt("reannotate-preview")
                .max_height(240.0)
                .show(ui, |ui| {
                    let mut any = false;
                    for r in self.changes() {
                        any = true;
                        ui.horizontal(|ui| {
                            ui.label(tr!(
                                "reannotate-row",
                                old = r.old.clone(),
                                new = r.new.clone()
                            ));
                            if ui.small_button(tr!("reannotate-lock")).clicked() {
                                lock = Some((r.component, true));
                            }
                        });
                    }
                    if !any && self.error.is_none() {
                        ui.weak(tr!("reannotate-no-changes"));
                    }
                });
            if !self.locked.is_empty() {
                ui.separator();
                ui.strong(tr!("reannotate-locked"));
                for l in &self.locked {
                    ui.horizontal(|ui| {
                        ui.label(&l.designator);
                        if ui.small_button(tr!("reannotate-unlock")).clicked() {
                            lock = Some((l.component, false));
                        }
                    });
                }
            }
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
        if (self.order, self.scope) != (before_order, before_scope) {
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
}

/// The Component of placed Unit `uuid`, on any sheet.
fn unit_component(project: &Project, uuid: Uuid) -> Option<Uuid> {
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .find_map(|s| s.unit(uuid))
        .map(|u| u.component.uuid)
}
