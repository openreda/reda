//! Drawing Board Setup's pages.

use egui::{Button, ComboBox, Role, Ui, WidgetInfo};
use reda_engine::model::{DrcCheck, Severity, Uuid};

use super::Rule;
use super::length::mm;
use super::model::{BoardSetupDraft, ClassKey, class_field};
use crate::settings::{PageCx, field};
use crate::tr;

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
        DrcCheck::TrackWidth => tr!("check-track-width"),
        DrcCheck::HoleSize => tr!("check-hole-size"),
        DrcCheck::AnnularRing => tr!("check-annular-ring"),
        DrcCheck::HoleToHole => tr!("check-hole-to-hole"),
        DrcCheck::EdgeClearance => tr!("check-edge-clearance"),
        DrcCheck::Keepout => tr!("check-keepout"),
        DrcCheck::FootprintOutsideBoard => tr!("check-footprint-outside-board"),
        DrcCheck::Unrouted => tr!("check-unrouted"),
        DrcCheck::CopperWithoutNet => tr!("check-copper-without-net"),
        DrcCheck::Dangling => tr!("check-dangling"),
        DrcCheck::CourtyardOverlap => tr!("check-courtyard-overlap"),
        DrcCheck::SilkscreenOverPads => tr!("check-silkscreen-over-pads"),
        DrcCheck::OutOfSync => tr!("check-out-of-sync"),
        DrcCheck::OpeningCollapsed => tr!("check-opening-collapsed"),
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

/// A rule's page: "Apply Fab Preset Rules…", the board-wide values, the Net Class
/// overrides and the check's severity.
pub fn rule_page(ui: &mut Ui, cx: &mut PageCx<'_>, rule: Rule, board: &mut BoardSetupDraft) {
    if no_pcb(ui, board) {
        return;
    }
    fab_preset(ui, board);
    ui.add_space(6.0);
    egui::Grid::new("board-setup-rule")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            for &f in rule.fields() {
                let label = f.board_wide_label();
                ui.label(&label);
                let problem = cx.problem(&f.problem_field());
                field(ui, &label, board.rule_mut(f), LENGTH_WIDTH, problem);
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    let overrides = rule.overrides();
    if overrides.is_empty() {
        ui.weak(tr!("board-setup-no-overrides"));
    } else {
        ui.strong(tr!("board-setup-overrides"));
        egui::Grid::new("board-setup-overrides")
            .num_columns(1 + overrides.len())
            .striped(true)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.strong(tr!("board-setup-net-class"));
                for f in &overrides {
                    ui.strong(f.name());
                }
                ui.end_row();
                let classes: Vec<(ClassKey, String)> = board
                    .live_classes()
                    .map(|c| (c.key(), c.name().to_owned()))
                    .collect();
                for (key, name) in classes {
                    ui.label(&name);
                    for &f in &overrides {
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
        ui.weak(tr!("board-setup-overrides-note"));
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(tr!(
            "board-setup-severity",
            check = check_name(rule.check())
        ));
        severity_choice(ui, board, rule.check());
    });
    ui.weak(tr!("board-setup-units"));
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

/// The Net Classes page: the classes, then the Net → Net Class table.
pub fn net_classes_page(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    if no_pcb(ui, board) {
        return;
    }
    let classes: Vec<(ClassKey, String)> = board
        .live_classes()
        .map(|c| (c.key(), c.name().to_owned()))
        .collect();
    let mut delete = None;
    egui::Grid::new("board-setup-classes")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for (number, (key, _)) in classes.iter().enumerate() {
                let label = tr!("board-setup-class-name", number = number + 1);
                let problem = cx.problem(&class_field(*key, "name"));
                if let Some(name) = board.class_name_mut(*key) {
                    field(ui, &label, name, 200.0, problem);
                }
                let default = board.is_default(*key);
                let title = tr!("board-setup-delete-class", number = number + 1);
                let button = ui.add_enabled(!default, Button::new(tr!("board-setup-delete")));
                button.widget_info(|| WidgetInfo::labeled(Role::Button, !default, &title));
                if default {
                    button.on_disabled_hover_text(tr!("board-setup-default-class"));
                } else if button.clicked() {
                    delete = Some(*key);
                }
                ui.end_row();
            }
        });
    if let Some(key) = delete {
        board.delete_class(key);
    }
    if ui.button(tr!("board-setup-add-class")).clicked() {
        board.add_class();
    }
    ui.add_space(10.0);
    ui.strong(tr!("board-setup-nets"));
    ui.horizontal(|ui| {
        let label = tr!("board-setup-filter");
        ui.label(&label);
        field(ui, &label, &mut board.net_filter, 200.0, None);
    });
    if board.nets().is_empty() {
        ui.weak(tr!("board-setup-no-nets"));
        return;
    }
    let classes: Vec<(ClassKey, String)> =
        std::iter::once((ClassKey::Default, tr!("board-setup-default-class-name")))
            .chain(board.live_classes().map(|c| (c.key(), c.name().to_owned())))
            .collect();
    let nets: Vec<(Uuid, String, ClassKey, String)> = board
        .filtered_nets()
        .map(|n| {
            let names: Vec<&str> = classes
                .iter()
                .filter(|(k, _)| n.classes().contains(k))
                .map(|(_, name)| name.as_str())
                .collect();
            let shown = if names.is_empty() {
                classes[0].1.clone()
            } else {
                names.join(", ")
            };
            (n.uuid, n.name.clone(), n.class(), shown)
        })
        .collect();
    let mut moves = Vec::new();
    egui::Grid::new("board-setup-nets")
        .num_columns(2)
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for (uuid, name, class, current) in nets {
                ui.label(&name);
                let label = tr!("board-setup-class-of", net = name.as_str());
                let combo = ComboBox::from_id_salt(("board-setup-net", uuid))
                    .selected_text(current)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (key, class_name) in &classes {
                            if ui.selectable_label(*key == class, class_name).clicked() {
                                moves.push((uuid, *key));
                            }
                        }
                    });
                combo
                    .response
                    .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
                ui.end_row();
            }
        });
    for (net, key) in moves {
        board.set_net_class(net, key);
    }
}

/// The DRC page: every check's severity.
pub fn drc_page(ui: &mut Ui, board: &mut BoardSetupDraft) {
    if no_pcb(ui, board) {
        return;
    }
    egui::Grid::new("board-setup-drc")
        .num_columns(2)
        .min_col_width(200.0)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for &check in DrcCheck::ALL {
                ui.label(check_name(check));
                severity_choice(ui, board, check);
                ui.end_row();
            }
        });
}
