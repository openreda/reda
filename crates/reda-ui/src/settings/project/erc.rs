//! The ERC page: each check's severity (`erc.set_severity`), with the fixed pin-type
//! matrix shown read-only beside the Pin conflict checks.

use egui::{Button, RichText, Role, Ui, WidgetInfo};
use reda_engine::model::{ErcCheck, ErcSeverities, PinType, Project, Severity, pin_conflict};
use serde_json::json;

use crate::settings::{Draft, Page, PageCx, Problem, Step};
use crate::tr;

/// The severity of every ERC check, as chosen.
#[derive(Clone, Debug)]
pub struct ErcPage {
    severities: Vec<(ErcCheck, Draft<Severity>)>,
}

impl Default for ErcPage {
    fn default() -> Self {
        ErcPage {
            severities: ErcCheck::ALL
                .iter()
                .map(|&c| (c, Draft::new(Severity::Warning)))
                .collect(),
        }
    }
}

impl ErcPage {
    /// The severity chosen for `check`.
    pub fn get(&self, check: ErcCheck) -> Severity {
        self.severities
            .iter()
            .find(|(c, _)| *c == check)
            .map_or(Severity::Warning, |(_, d)| *d.value())
    }

    pub fn set(&mut self, check: ErcCheck, severity: Severity) {
        if let Some((_, d)) = self.severities.iter_mut().find(|(c, _)| *c == check) {
            d.set(severity);
        }
    }
}

const SEVERITIES: [Severity; 3] = [Severity::Error, Severity::Warning, Severity::Off];

fn severity_name(severity: Severity) -> String {
    match severity {
        Severity::Error => tr!("severity-error"),
        Severity::Warning => tr!("severity-warning"),
        Severity::Off => tr!("severity-off"),
    }
}

/// An ERC check's name for the user.
pub fn check_name(check: ErcCheck) -> String {
    match check {
        ErcCheck::UnconnectedInput => tr!("check-unconnected-input"),
        ErcCheck::UnconnectedPin => tr!("check-unconnected-pin"),
        ErcCheck::PinConflict => tr!("check-pin-conflict"),
        ErcCheck::PinConflictMixed => tr!("check-pin-conflict-mixed"),
        ErcCheck::UndrivenPower => tr!("check-undriven-power"),
        ErcCheck::SinglePinNet => tr!("check-single-pin-net"),
        ErcCheck::DanglingWire => tr!("check-dangling-wire"),
        ErcCheck::UnattachedLabel => tr!("check-unattached-label"),
        ErcCheck::NoConnectConnected => tr!("check-no-connect-connected"),
        ErcCheck::NetNamesOnSheet => tr!("check-net-names-on-sheet"),
        ErcCheck::NetNamesAcrossSheets => tr!("check-net-names-across-sheets"),
        ErcCheck::DuplicateNets => tr!("check-duplicate-nets"),
        ErcCheck::SingleSheetPort => tr!("check-single-sheet-port"),
        ErcCheck::DuplicateDesignator => tr!("check-duplicate-designator"),
        ErcCheck::UnplacedUnits => tr!("check-unplaced-units"),
        ErcCheck::NetTieOneNet => tr!("check-net-tie-one-net"),
        ErcCheck::UnitDataMismatch => tr!("check-unit-data-mismatch"),
    }
}

/// A Pin type's name and its short form for the matrix's column heads.
fn pin_type_names(t: PinType) -> (String, String) {
    match t {
        PinType::Input => (tr!("pin-type-input"), tr!("pin-type-input-short")),
        PinType::Output => (tr!("pin-type-output"), tr!("pin-type-output-short")),
        PinType::Bidirectional => (
            tr!("pin-type-bidirectional"),
            tr!("pin-type-bidirectional-short"),
        ),
        PinType::TriState => (tr!("pin-type-tri-state"), tr!("pin-type-tri-state-short")),
        PinType::Passive => (tr!("pin-type-passive"), tr!("pin-type-passive-short")),
        PinType::PowerIn => (tr!("pin-type-power-in"), tr!("pin-type-power-in-short")),
        PinType::PowerOut => (tr!("pin-type-power-out"), tr!("pin-type-power-out-short")),
        PinType::OpenCollector => (
            tr!("pin-type-open-collector"),
            tr!("pin-type-open-collector-short"),
        ),
        PinType::OpenEmitter => (
            tr!("pin-type-open-emitter"),
            tr!("pin-type-open-emitter-short"),
        ),
        PinType::NotConnected => (
            tr!("pin-type-not-connected"),
            tr!("pin-type-not-connected-short"),
        ),
    }
}

impl Page for ErcPage {
    fn title(&self) -> String {
        tr!("settings-page-erc")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        let erc = project.props().erc;
        for (check, draft) in &mut self.severities {
            draft.load(erc.get(*check), reset);
        }
    }

    fn steps(&self) -> Vec<Step> {
        self.severities
            .iter()
            .filter(|(_, d)| d.changed())
            .map(|(check, d)| {
                Step::new(
                    "erc.set_severity",
                    json!({"check": check, "severity": d.value()}),
                )
            })
            .collect()
    }

    fn has_defaults(&self) -> bool {
        true
    }

    fn reset_to_defaults(&mut self, _shell: &mut crate::shell::Shell) {
        for (check, draft) in &mut self.severities {
            draft.set(ErcSeverities::DEFAULT.get(*check));
        }
    }

    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }

    fn show(&mut self, ui: &mut Ui, _cx: &mut PageCx<'_>) {
        // The matrix goes right after the two Pin conflict checks it decides.
        let split = ErcCheck::ALL
            .iter()
            .position(|&c| c == ErcCheck::PinConflictMixed)
            .map_or(self.severities.len(), |i| i + 1);
        let (head, tail) = self.severities.split_at_mut(split);
        severity_grid(ui, "settings-erc-head", head);
        ui.add_space(4.0);
        ui.indent("settings-erc-matrix", pin_matrix);
        ui.add_space(4.0);
        severity_grid(ui, "settings-erc-tail", tail);
    }
}

fn severity_grid(ui: &mut Ui, id: &str, rows: &mut [(ErcCheck, Draft<Severity>)]) {
    egui::Grid::new(id)
        .num_columns(2)
        .min_col_width(220.0)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for (check, draft) in rows {
                let name = check_name(*check);
                ui.label(&name);
                ui.horizontal(|ui| {
                    for severity in SEVERITIES {
                        let label = severity_name(severity);
                        let chosen = *draft.value() == severity;
                        let button = ui.add(Button::selectable(chosen, &label));
                        let accessible = tr!(
                            "settings-erc-choice",
                            check = name.as_str(),
                            severity = label
                        );
                        button.widget_info(|| {
                            WidgetInfo::selected(Role::RadioButton, true, chosen, &accessible)
                        });
                        if button.clicked() {
                            draft.set(severity);
                        }
                    }
                });
                ui.end_row();
            }
        });
}

/// The fixed pin-type matrix: E where two Pin types on one Net are a Pin conflict, W
/// where they should not mix.
fn pin_matrix(ui: &mut Ui) {
    ui.weak(tr!("settings-erc-matrix"));
    let error = ui.visuals().error_fg_color;
    let warning = ui.visuals().warn_fg_color;
    egui::Grid::new("settings-erc-matrix-grid")
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            ui.label("");
            for &t in PinType::ALL {
                let (name, short) = pin_type_names(t);
                ui.label(RichText::new(short).small()).on_hover_text(name);
            }
            ui.end_row();
            for &a in PinType::ALL {
                ui.label(RichText::new(pin_type_names(a).0).small());
                for &b in PinType::ALL {
                    let cell = match pin_conflict(a, b) {
                        Some(ErcCheck::PinConflict) => RichText::new("E").color(error).strong(),
                        Some(_) => RichText::new("W").color(warning).strong(),
                        None => RichText::new("·").weak(),
                    };
                    ui.label(cell.small());
                }
                ui.end_row();
            }
        });
}
