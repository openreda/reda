//! The ERC page: each check's severity (`erc.set_severity`), with the Project's Pin Matrix
//! shown read-only beside the two checks it sets.

use egui::{Button, RichText, Role, Ui, WidgetInfo};
use reda_engine::model::{
    Cell, ErcCheck, ErcSeverities, MatrixType, PinMatrix, PinType, PortDirection, Project, Severity,
};
use serde_json::json;

use crate::settings::{Draft, Page, PageCx, Problem, Step};
use crate::tr;

/// The severity of every ERC check, as chosen.
#[derive(Clone, Debug)]
pub struct ErcPage {
    /// The checks with a severity of their own: not the Pin Matrix checks.
    severities: Vec<(ErcCheck, Draft<Severity>)>,
    /// The Project's Pin Matrix, as loaded.
    matrix: PinMatrix,
}

impl Default for ErcPage {
    fn default() -> Self {
        ErcPage {
            severities: (ErcCheck::ALL.iter())
                .filter(|c| !c.is_matrix())
                .map(|&c| (c, Draft::new(Severity::Warning)))
                .collect(),
            matrix: PinMatrix::DEFAULT,
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
        ErcCheck::UnconnectedPin => tr!("check-unconnected-pin"),
        ErcCheck::PinConflict => tr!("check-pin-conflict"),
        ErcCheck::UndrivenPower => tr!("check-undriven-power"),
        ErcCheck::NoDrivingSource => tr!("check-no-driving-source"),
        ErcCheck::SinglePinNet => tr!("check-single-pin-net"),
        ErcCheck::DanglingWire => tr!("check-dangling-wire"),
        ErcCheck::UnattachedLabel => tr!("check-unattached-label"),
        ErcCheck::NoConnectConnected => tr!("check-no-connect-connected"),
        ErcCheck::NetNamesOnSheet => tr!("check-net-names-on-sheet"),
        ErcCheck::NetNamesAcrossSheets => tr!("check-net-names-across-sheets"),
        ErcCheck::DuplicateNets => tr!("check-duplicate-nets"),
        ErcCheck::SingleSheetPort => tr!("check-single-sheet-port"),
        ErcCheck::PortNoDriver => tr!("check-port-no-driver"),
        ErcCheck::DuplicateDesignator => tr!("check-duplicate-designator"),
        ErcCheck::UnplacedUnits => tr!("check-unplaced-units"),
        ErcCheck::UnitPlacedTwice => tr!("check-unit-placed-twice"),
        ErcCheck::NoFootprintChoice => tr!("check-no-footprint-choice"),
        ErcCheck::NetTieOneNet => tr!("check-net-tie-one-net"),
        ErcCheck::UnitDataMismatch => tr!("check-unit-data-mismatch"),
        ErcCheck::OffGrid => tr!("check-off-grid"),
        ErcCheck::OffSheet => tr!("check-off-sheet"),
        ErcCheck::LibraryOutOfDate => tr!("check-library-out-of-date"),
    }
}

/// A Pin Matrix row's or column's name and its short form for the column heads.
fn matrix_type_names(t: MatrixType) -> (String, String) {
    match t {
        MatrixType::Pin(t) => pin_type_names(t),
        MatrixType::Port(PortDirection::In) => {
            (tr!("pin-matrix-port-in"), tr!("pin-matrix-port-in-short"))
        }
        MatrixType::Port(PortDirection::Out) => {
            (tr!("pin-matrix-port-out"), tr!("pin-matrix-port-out-short"))
        }
        MatrixType::Port(PortDirection::Bidirectional) => (
            tr!("pin-matrix-port-bidirectional"),
            tr!("pin-matrix-port-bidirectional-short"),
        ),
        MatrixType::Unconnected => (
            tr!("pin-matrix-unconnected"),
            tr!("pin-matrix-unconnected-short"),
        ),
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
        let erc = &project.props().erc;
        for (check, draft) in &mut self.severities {
            if let Some(severity) = erc.severities.get(*check) {
                draft.load(severity, reset);
            }
        }
        self.matrix = erc.pin_matrix_overrides.clone();
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
            if let Some(severity) = ErcSeverities::DEFAULT.get(*check) {
                draft.set(severity);
            }
        }
    }

    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }

    fn show(&mut self, ui: &mut Ui, _cx: &mut PageCx<'_>) {
        // The matrix comes first, standing for the two checks it sets.
        for check in ErcCheck::ALL.iter().filter(|c| c.is_matrix()) {
            ui.label(tr!(
                "settings-erc-set-by-matrix",
                check = check_name(*check)
            ));
        }
        ui.add_space(4.0);
        let matrix = &self.matrix;
        ui.indent("settings-erc-matrix", |ui| pin_matrix(ui, matrix));
        ui.add_space(4.0);
        severity_grid(ui, "settings-erc-checks", &mut self.severities);
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

/// The Project's Pin Matrix, read-only: E, W or a dot per cell, coloured by level.
fn pin_matrix(ui: &mut Ui, matrix: &PinMatrix) {
    ui.weak(tr!("settings-erc-matrix"));
    let error = ui.visuals().error_fg_color;
    let warning = ui.visuals().warn_fg_color;
    egui::Grid::new("settings-erc-matrix-grid")
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            ui.label("");
            for &t in MatrixType::ALL {
                let (name, short) = matrix_type_names(t);
                ui.label(RichText::new(short).small()).on_hover_text(name);
            }
            ui.end_row();
            for &a in MatrixType::ALL
                .iter()
                .filter(|t| **t != MatrixType::Unconnected)
            {
                ui.label(RichText::new(matrix_type_names(a).0).small());
                for &b in MatrixType::ALL {
                    let cell = match Cell::new(a, b).map(|c| matrix.get(c)) {
                        Some(Severity::Error) => RichText::new("E").color(error).strong(),
                        Some(Severity::Warning) => RichText::new("W").color(warning).strong(),
                        Some(Severity::Off) => RichText::new("·").weak(),
                        None => RichText::new(""),
                    };
                    ui.label(cell.small());
                }
                ui.end_row();
            }
        });
}
