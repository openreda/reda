//! The ERC page: each check's severity (`erc.set_severity`) in a grid grouped by what the
//! checks look at, with multi-select Set severity, beside the editable Pin Matrix that sets
//! the two matrix checks (`erc.set_pin_matrix`).

use std::collections::BTreeSet;

use egui::{Button, RichText, Role, Sense, Ui, Vec2, WidgetInfo};
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
    /// The Pin Matrix, as chosen.
    matrix: Draft<PinMatrix>,
    /// The checks picked for Set severity.
    picked: BTreeSet<ErcCheck>,
}

impl Default for ErcPage {
    fn default() -> Self {
        ErcPage {
            severities: (ErcCheck::ALL.iter())
                .filter(|c| !c.is_matrix())
                .map(|&c| (c, Draft::new(Severity::Warning)))
                .collect(),
            matrix: Draft::new(PinMatrix::DEFAULT),
            picked: BTreeSet::new(),
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

    /// Picks `check` for Set severity: alone, or (`extend`, as Mod+click) toggled in the
    /// picked set. The matrix checks have no severity to pick.
    pub fn pick(&mut self, check: ErcCheck, extend: bool) {
        if check.is_matrix() {
            return;
        }
        if !extend {
            self.picked.clear();
            self.picked.insert(check);
        } else if !self.picked.remove(&check) {
            self.picked.insert(check);
        }
    }

    /// Picks every check of `group` that has a severity, and nothing else.
    pub fn pick_group(&mut self, group: ErcGroup) {
        self.picked = (group.checks().iter().copied())
            .filter(|c| !c.is_matrix())
            .collect();
    }

    /// The picked checks, in check order.
    pub fn picked(&self) -> Vec<ErcCheck> {
        let picked = &self.picked;
        (ErcCheck::ALL.iter().copied())
            .filter(|c| picked.contains(c))
            .collect()
    }

    /// Sets every picked check to `severity`.
    pub fn set_picked(&mut self, severity: Severity) {
        for check in self.picked() {
            self.set(check, severity);
        }
    }

    /// The severity chosen for a Pin Matrix cell.
    pub fn cell(&self, cell: Cell) -> Severity {
        self.matrix.value().get(cell)
    }

    /// Whether a cell differs from its default, as the dot on it shows.
    pub fn is_changed_from_default(&self, cell: Cell) -> bool {
        self.cell(cell) != cell.default_severity()
    }

    /// A click on a cell: Off, then Warning, then Error, then Off again.
    pub fn cycle(&mut self, cell: Cell) {
        let next = match self.cell(cell) {
            Severity::Off => Severity::Warning,
            Severity::Warning => Severity::Error,
            Severity::Error => Severity::Off,
        };
        self.matrix.value_mut().set(cell, Some(next));
    }

    /// The matrix menu: every cell to `severity`, or each to its default with `None`.
    pub fn set_all(&mut self, severity: Option<Severity>) {
        let matrix = self.matrix.value_mut();
        for cell in Cell::all() {
            matrix.set(cell, severity);
        }
    }

    /// A cell's hover sentence, such as "Output Pin connected to Output Pin: Error".
    pub fn sentence(&self, cell: Cell) -> String {
        let severity = severity_name(self.cell(cell));
        let (a, b) = cell.types();
        match b {
            MatrixType::Unconnected => tr!(
                "pin-matrix-cell-unconnected",
                a = cell_name(a),
                severity = severity
            ),
            _ => tr!(
                "pin-matrix-cell",
                a = cell_name(a),
                b = cell_name(b),
                severity = severity
            ),
        }
    }
}

/// A row's or column's type in a cell's sentence: "Output Pin", "Port in".
fn cell_name(t: MatrixType) -> String {
    match t {
        MatrixType::Pin(p) => tr!("pin-matrix-pin", kind = pin_type_names(p).0),
        other => matrix_type_names(other).0,
    }
}

/// What a group of ERC checks looks at, as the grid shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErcGroup {
    Pins,
    Nets,
    Ports,
    Components,
    Drawing,
}

impl ErcGroup {
    pub const ALL: [ErcGroup; 5] = [
        ErcGroup::Pins,
        ErcGroup::Nets,
        ErcGroup::Ports,
        ErcGroup::Components,
        ErcGroup::Drawing,
    ];

    /// The group `check` sits in.
    pub fn of(check: ErcCheck) -> ErcGroup {
        match check {
            ErcCheck::UnconnectedPin
            | ErcCheck::PinConflict
            | ErcCheck::UndrivenPower
            | ErcCheck::NoDrivingSource => ErcGroup::Pins,
            ErcCheck::SinglePinNet
            | ErcCheck::DanglingWire
            | ErcCheck::UnattachedLabel
            | ErcCheck::NoConnectConnected
            | ErcCheck::NetNamesOnSheet
            | ErcCheck::NetNamesAcrossSheets
            | ErcCheck::DuplicateNets
            | ErcCheck::NetTieOneNet => ErcGroup::Nets,
            ErcCheck::SingleSheetPort | ErcCheck::PortNoDriver => ErcGroup::Ports,
            ErcCheck::DuplicateDesignator
            | ErcCheck::UnplacedUnits
            | ErcCheck::UnitPlacedTwice
            | ErcCheck::NoFootprintChoice
            | ErcCheck::UnitDataMismatch
            | ErcCheck::LibraryOutOfDate => ErcGroup::Components,
            ErcCheck::OffGrid | ErcCheck::OffSheet => ErcGroup::Drawing,
        }
    }

    /// The group's checks, in check order.
    pub fn checks(self) -> &'static [ErcCheck] {
        use std::sync::OnceLock;
        static GROUPS: OnceLock<Vec<Vec<ErcCheck>>> = OnceLock::new();
        let groups = GROUPS.get_or_init(|| {
            (ErcGroup::ALL.iter())
                .map(|g| {
                    (ErcCheck::ALL.iter().copied())
                        .filter(|c| ErcGroup::of(*c) == *g)
                        .collect()
                })
                .collect()
        });
        &groups[self as usize]
    }

    pub fn title(self) -> String {
        match self {
            ErcGroup::Pins => tr!("settings-erc-group-pins"),
            ErcGroup::Nets => tr!("settings-erc-group-nets"),
            ErcGroup::Ports => tr!("settings-erc-group-ports"),
            ErcGroup::Components => tr!("settings-erc-group-components"),
            ErcGroup::Drawing => tr!("settings-erc-group-drawing"),
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
        self.matrix.load(erc.pin_matrix_overrides.clone(), reset);
    }

    fn steps(&self) -> Vec<Step> {
        let mut steps: Vec<Step> = (self.severities.iter())
            .filter(|(_, d)| d.changed())
            .map(|(check, d)| {
                Step::new(
                    "erc.set_severity",
                    json!({"check": check, "severity": d.value()}),
                )
            })
            .collect();
        let (base, value) = (self.matrix.base(), self.matrix.value());
        let cells: Vec<serde_json::Value> = Cell::all()
            .filter(|c| base.get(*c) != value.get(*c))
            .map(|c| {
                let (a, b) = c.types();
                let severity = value.get(c);
                let severity = (severity != c.default_severity()).then_some(severity);
                json!({"a": a, "b": b, "severity": severity})
            })
            .collect();
        if !cells.is_empty() {
            steps.push(Step::new("erc.set_pin_matrix", json!({ "cells": cells })));
        }
        steps
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
        self.matrix.set(PinMatrix::DEFAULT);
    }

    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }

    fn show(&mut self, ui: &mut Ui, _cx: &mut PageCx<'_>) {
        // Side by side when the window is wide enough, else the matrix first.
        if ui.available_width() >= 1100.0 {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| self.check_grid(ui));
                ui.add_space(16.0);
                ui.vertical(|ui| self.pin_matrix(ui));
            });
        } else {
            self.pin_matrix(ui);
            ui.add_space(8.0);
            self.check_grid(ui);
        }
    }
}

impl ErcPage {
    /// The checks by group, each with its severity choices; a click on a check's name
    /// picks it (Mod+click adds), a click on a group picks the group, and Set severity
    /// sets every picked check.
    fn check_grid(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(tr!("settings-erc-set-picked", count = self.picked.len() as i64));
            for severity in SEVERITIES {
                let label = severity_name(severity);
                let name = tr!("settings-erc-set-picked-to", severity = label.as_str());
                let button = ui.add_enabled(!self.picked.is_empty(), Button::new(&label));
                button.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
                if button.clicked() {
                    self.set_picked(severity);
                }
            }
        });
        ui.add_space(4.0);
        // Mod or Shift extends, held now or with the click itself.
        let extend = ui.input(|i| {
            let held = |m: &egui::Modifiers| m.command || m.shift;
            held(&i.modifiers)
                || i.events.iter().any(|e| {
                    matches!(e, egui::Event::PointerButton { pressed: false, modifiers, .. }
                        if held(modifiers))
                })
        });
        let mut pick = None;
        let mut pick_group = None;
        egui::Grid::new("settings-erc-checks")
            .num_columns(2)
            .spacing([8.0, 2.0])
            .show(ui, |ui| {
                for group in ErcGroup::ALL {
                    let heading = ui.add(
                        egui::Label::new(RichText::new(group.title()).strong())
                            .sense(Sense::click()),
                    );
                    if heading.clicked() {
                        pick_group = Some(group);
                    }
                    ui.end_row();
                    for check in group.checks() {
                        let name = check_name(*check);
                        if check.is_matrix() {
                            ui.weak(&name);
                            ui.weak(tr!("settings-erc-by-matrix"));
                            ui.end_row();
                            continue;
                        }
                        let picked = self.picked.contains(check);
                        let row = ui.add(Button::selectable(picked, &name).frame_when_inactive(false));
                        let accessible = name.clone();
                        row.widget_info(|| {
                            WidgetInfo::selected(Role::Row, true, picked, &accessible)
                        });
                        if row.clicked() {
                            pick = Some(*check);
                        }
                        let Some((_, draft)) =
                            self.severities.iter_mut().find(|(c, _)| c == check)
                        else {
                            ui.end_row();
                            continue;
                        };
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
                                    WidgetInfo::selected(
                                        Role::RadioButton,
                                        true,
                                        chosen,
                                        &accessible,
                                    )
                                });
                                if button.clicked() {
                                    draft.set(severity);
                                }
                            }
                        });
                        ui.end_row();
                    }
                }
            });
        if let Some(check) = pick {
            self.pick(check, extend);
        }
        if let Some(group) = pick_group {
            self.pick_group(group);
        }
    }

    /// The Pin Matrix as one triangle: a cell per pair, coloured by level with its letter,
    /// a dot when it is not the default, and its sentence on hover. A click cycles it; the
    /// right-click menu sets every cell.
    fn pin_matrix(&mut self, ui: &mut Ui) {
        ui.strong(tr!("settings-erc-matrix-title"));
        ui.weak(tr!("settings-erc-matrix"));
        let mut clicked = None;
        let mut all = None;
        let size = Vec2::splat(20.0);
        egui::Grid::new("settings-erc-matrix-grid")
            .spacing([2.0, 2.0])
            .show(ui, |ui| {
                ui.label("");
                for &t in MatrixType::ALL {
                    let (name, short) = matrix_type_names(t);
                    ui.label(RichText::new(short).small()).on_hover_text(name);
                }
                ui.end_row();
                for &a in (MatrixType::ALL.iter()).filter(|t| **t != MatrixType::Unconnected) {
                    ui.label(RichText::new(matrix_type_names(a).0).small());
                    for &b in MatrixType::ALL {
                        // One triangle: each pair once, plus the Unconnected column.
                        let shown = b.index() <= a.index() || b == MatrixType::Unconnected;
                        let Some(cell) = Cell::new(a, b).filter(|_| shown) else {
                            ui.allocate_exact_size(size, Sense::hover());
                            continue;
                        };
                        let response = self.matrix_cell(ui, cell, size);
                        if response.clicked() {
                            clicked = Some(cell);
                        }
                        response.context_menu(|ui| {
                            for (label, severity) in [
                                (tr!("settings-erc-matrix-all-off"), Some(Severity::Off)),
                                (tr!("settings-erc-matrix-all-warning"), Some(Severity::Warning)),
                                (tr!("settings-erc-matrix-all-error"), Some(Severity::Error)),
                                (tr!("settings-erc-matrix-default"), None),
                            ] {
                                if ui.button(label).clicked() {
                                    all = Some(severity);
                                    ui.close();
                                }
                            }
                        });
                    }
                    ui.end_row();
                }
            });
        if let Some(cell) = clicked {
            self.cycle(cell);
        }
        if let Some(severity) = all {
            self.set_all(severity);
        }
    }

    fn matrix_cell(&self, ui: &mut Ui, cell: Cell, size: Vec2) -> egui::Response {
        let severity = self.cell(cell);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let sentence = self.sentence(cell);
        response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &sentence));
        let visuals = ui.visuals();
        let (fill, letter) = match severity {
            Severity::Error => (visuals.error_fg_color, "E"),
            Severity::Warning => (visuals.warn_fg_color, "W"),
            Severity::Off => (visuals.faint_bg_color, ""),
        };
        let painter = ui.painter();
        painter.rect_filled(rect.shrink(1.0), 2.0, fill.gamma_multiply(0.35));
        if response.hovered() {
            painter.rect_stroke(
                rect.shrink(1.0),
                2.0,
                visuals.widgets.hovered.fg_stroke,
                egui::StrokeKind::Inside,
            );
        }
        if !letter.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                letter,
                egui::FontId::proportional(11.0),
                fill,
            );
        }
        if self.is_changed_from_default(cell) {
            let dot = rect.right_top() + Vec2::new(-4.0, 4.0);
            painter.circle_filled(dot, 2.0, visuals.strong_text_color());
        }
        response.on_hover_text(sentence)
    }
}
