//! Drawing Board Setup's rule pages and DRC page; the Net Classes page is in
//! [`grid`].

use egui::{Button, ComboBox, Role, Ui, WidgetInfo};
use reda_engine::model::{DrcCheck, Limit, Severity, SilkMaskMode, Uuid};

use super::length::mm;
use super::model::{BoardSetupDraft, ClassKey, RuleField, class_field};
use super::{DrcPage, Rule, Section};
use crate::settings::{PageCx, field};
use crate::tr;

mod grid;

pub use grid::net_classes_page;

const SEVERITIES: [Severity; 3] = [Severity::Error, Severity::Warning, Severity::Off];
const LENGTH_WIDTH: f32 = 90.0;

fn severity_name(severity: Severity) -> String {
    match severity {
        Severity::Error => tr!("severity-error"),
        Severity::Warning => tr!("severity-warning"),
        Severity::Off => tr!("severity-off"),
    }
}

/// A DRC check's name for the user.
pub fn check_name(check: DrcCheck) -> String {
    match check {
        DrcCheck::Clearance => tr!("check-clearance"),
        DrcCheck::ShortCircuit => tr!("check-short-circuit"),
        DrcCheck::ShortedCopper => tr!("check-shorted-copper"),
        DrcCheck::NetTieNotJoined => tr!("check-net-tie-not-joined"),
        DrcCheck::TrackWidth => tr!("check-track-width"),
        DrcCheck::ViaSize => tr!("check-via-size"),
        DrcCheck::HoleSize => tr!("check-hole-size"),
        DrcCheck::AnnularRing => tr!("check-annular-ring"),
        DrcCheck::HoleToHole => tr!("check-hole-to-hole"),
        DrcCheck::EdgeClearance => tr!("check-edge-clearance"),
        DrcCheck::Keepout => tr!("check-keepout"),
        DrcCheck::FootprintOutsideBoard => tr!("check-footprint-outside-board"),
        DrcCheck::Unrouted => tr!("check-unrouted"),
        DrcCheck::CopperWithoutNet => tr!("check-copper-without-net"),
        DrcCheck::Dangling => tr!("check-dangling"),
        DrcCheck::PoorConnection => tr!("check-poor-connection"),
        DrcCheck::ViaUnderSmd => tr!("check-via-under-smd"),
        DrcCheck::CourtyardOverlap => tr!("check-courtyard-overlap"),
        DrcCheck::SilkToMask => tr!("check-silk-to-mask"),
        DrcCheck::SilkToEdge => tr!("check-silk-to-edge"),
        DrcCheck::ZeroLengthTrack => tr!("check-zero-length-track"),
        DrcCheck::MicroSegment => tr!("check-micro-segment"),
        DrcCheck::DuplicateCopper => tr!("check-duplicate-copper"),
        DrcCheck::ZeroAreaZone => tr!("check-zero-area-zone"),
        DrcCheck::DeadCopper => tr!("check-dead-copper"),
        DrcCheck::EmptyZone => tr!("check-empty-zone"),
        DrcCheck::ShelvedZone => tr!("check-shelved-zone"),
        DrcCheck::OutOfSync => tr!("check-out-of-sync"),
        DrcCheck::OpeningCollapsed => tr!("check-opening-collapsed"),
        DrcCheck::RuleInvalid => tr!("check-rule-invalid"),
        DrcCheck::RuleOutsidePreset => tr!("check-rule-outside-preset"),
    }
}

/// Error / Warning / Off for `check`, each announced as "<check>: <severity>".
fn severity_choice(ui: &mut Ui, board: &mut BoardSetupDraft, check: DrcCheck) {
    let name = check_name(check);
    ui.horizontal(|ui| {
        for severity in SEVERITIES {
            let label = severity_name(severity);
            let chosen = board.severity(check) == severity;
            let button = ui.add(Button::selectable(chosen, &label));
            let accessible = tr!(
                "board-setup-severity-choice",
                check = name.as_str(),
                severity = label
            );
            button
                .widget_info(|| WidgetInfo::selected(Role::RadioButton, true, chosen, &accessible));
            if button.clicked() {
                board.set_severity(check, severity);
            }
        }
    });
}

fn no_pcb(ui: &mut Ui, board: &BoardSetupDraft) -> bool {
    if board.has_pcb() {
        return false;
    }
    ui.weak(tr!("board-setup-no-pcb"));
    true
}

/// The strictest job preset limit on `f`, as shown beside its value: "JLCPCB: at least
/// 0.1". Empty when no job preset limits it.
pub fn limit_text(board: &BoardSetupDraft, f: RuleField) -> Option<String> {
    let (limit, value, preset) = board.strictest(f)?;
    let value = mm(value);
    Some(match limit {
        Limit::Min => tr!("board-setup-limit-min", preset = preset, value = value),
        Limit::Max => tr!("board-setup-limit-max", preset = preset, value = value),
    })
}

/// A rule's page: "Apply Fab Preset Rules…", then each check with its board-wide values
/// (and the strictest Fab Preset limit beside each), its Net Class overrides and its
/// severity.
pub fn rule_page(ui: &mut Ui, cx: &mut PageCx<'_>, rule: Rule, board: &mut BoardSetupDraft) {
    if no_pcb(ui, board) {
        return;
    }
    fab_preset(ui, board);
    for (i, section) in rule.sections().iter().enumerate() {
        ui.add_space(if i == 0 { 6.0 } else { 14.0 });
        if rule.sections().len() > 1 {
            ui.heading(check_name(section.check));
        }
        section_ui(ui, cx, section, board);
    }
    ui.add_space(8.0);
    ui.weak(tr!("board-setup-units"));
}

fn section_ui(ui: &mut Ui, cx: &mut PageCx<'_>, section: &Section, board: &mut BoardSetupDraft) {
    egui::Grid::new(("board-setup-rule", section.check))
        .num_columns(3)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            for &f in section.fields {
                let label = f.board_wide_label();
                ui.label(&label);
                let problem = cx.problem(&f.problem_field());
                field(ui, &label, board.rule_mut(f), LENGTH_WIDTH, problem);
                match limit_text(board, f) {
                    Some(text) => ui.weak(text),
                    None => ui.label(""),
                };
                ui.end_row();
            }
            if section.check == DrcCheck::SilkToMask {
                silk_mode(ui, board);
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    let overrides = section.fields;
    let classes: Vec<(ClassKey, String)> = board
        .live_classes()
        .map(|c| (c.key(), c.name().to_owned()))
        .collect();
    ui.strong(tr!("board-setup-overrides"));
    if classes.is_empty() {
        ui.weak(tr!("board-setup-no-classes"));
    } else {
        egui::Grid::new(("board-setup-overrides", section.check))
            .num_columns(1 + overrides.len())
            .striped(true)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.strong(tr!("board-setup-net-class"));
                for f in overrides {
                    ui.strong(f.name());
                }
                ui.end_row();
                for (key, name) in classes {
                    ui.label(&name);
                    for &f in overrides {
                        let label = tr!(
                            "board-setup-override",
                            class = name.as_str(),
                            rule = f.name()
                        );
                        let problem = cx.problem(&class_field(key, &f.id()));
                        if let Some(value) = board.class_override_mut(key, f) {
                            field(ui, &label, value, LENGTH_WIDTH, problem);
                        }
                    }
                    ui.end_row();
                }
            });
    }
    ui.weak(tr!("board-setup-overrides-note"));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(tr!(
            "board-setup-severity",
            check = check_name(section.check)
        ));
        severity_choice(ui, board, section.check);
    });
}

/// What `silk_to_mask` measures silkscreen against.
fn silk_mode(ui: &mut Ui, board: &mut BoardSetupDraft) {
    let label = tr!("board-setup-silk-mode");
    ui.label(&label);
    let name = |mode: SilkMaskMode| match mode {
        SilkMaskMode::ExposedCopper => tr!("board-setup-silk-mode-copper"),
        SilkMaskMode::MaskOpening => tr!("board-setup-silk-mode-opening"),
    };
    let mut mode = board.silk_mode();
    let combo = ComboBox::from_id_salt("board-setup-silk-mode")
        .selected_text(name(mode))
        .width(180.0)
        .show_ui(ui, |ui| {
            for m in [SilkMaskMode::ExposedCopper, SilkMaskMode::MaskOpening] {
                ui.selectable_value(&mut mode, m, name(m));
            }
        });
    combo
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
    if mode != board.silk_mode() {
        board.set_silk_mode(mode);
    }
}

/// "Apply Fab Preset Rules…": one button per Project preset copy, then the preview.
fn fab_preset(ui: &mut Ui, board: &mut BoardSetupDraft) {
    let presets: Vec<(Uuid, String)> = board.presets().map(|(u, n)| (u, n.to_owned())).collect();
    ui.horizontal_wrapped(|ui| {
        ui.label(tr!("board-setup-fab-preset"));
        if presets.is_empty() {
            ui.weak(tr!("settings-preset-none"));
        }
        for (uuid, name) in presets {
            let label = tr!("board-setup-apply-preset", preset = name.as_str());
            if ui.button(label).clicked() {
                board.preview_fab_preset(uuid);
            }
        }
    });
    let Some(preview) = board.fab_preview().cloned() else {
        return;
    };
    let mut accept = false;
    let mut cancel = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        if preview.changes.is_empty() {
            ui.label(tr!(
                "board-setup-preset-same",
                preset = preview.name.as_str()
            ));
        } else {
            ui.label(tr!(
                "board-setup-preset-preview",
                preset = preview.name.as_str()
            ));
            egui::Grid::new("board-setup-preset-diff")
                .num_columns(3)
                .striped(true)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.strong(tr!("board-setup-rule"));
                    ui.strong(tr!("board-setup-now"));
                    ui.strong(tr!("board-setup-preset"));
                    ui.end_row();
                    for c in &preview.changes {
                        ui.label(c.rule.name());
                        ui.monospace(&c.board);
                        ui.monospace(mm(c.preset));
                        ui.end_row();
                    }
                });
        }
        ui.horizontal(|ui| {
            accept = ui
                .add_enabled(
                    !preview.changes.is_empty(),
                    Button::new(tr!("board-setup-use-preset")),
                )
                .clicked();
            cancel = ui.button(tr!("dialog-cancel")).clicked();
        });
    });
    if accept {
        board.accept_fab_preview();
    } else if cancel {
        board.cancel_fab_preview();
    }
}

/// The DRC page: every check's severity, with the values that sit beside their checks.
pub fn drc_page(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    if no_pcb(ui, board) {
        return;
    }
    egui::Grid::new("board-setup-drc")
        .num_columns(3)
        .min_col_width(200.0)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for &check in DrcCheck::ALL {
                ui.label(check_name(check));
                severity_choice(ui, board, check);
                match DrcPage::VALUES.iter().find(|(c, _)| *c == check) {
                    Some(&(_, f)) => {
                        ui.horizontal(|ui| {
                            let label = f.board_wide_label();
                            ui.label(&label);
                            let problem = cx.problem(&f.problem_field());
                            field(ui, &label, board.rule_mut(f), LENGTH_WIDTH, problem);
                        });
                    }
                    None => {
                        ui.label("");
                    }
                }
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    ui.weak(tr!("board-setup-units"));
}
