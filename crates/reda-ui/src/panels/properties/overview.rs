//! Properties with nothing selected (docs/spec/v0.1/editing.md#properties-with-nothing-selected):
//! Ready for fab? on a board, the Selection Filter, Snap Options, the Board or Sheet, the
//! Grid, Other (units), Checks (the check summary, with **Fix** for health checks) and
//! links to Board Setup or Project Settings. Each region collapses.

use std::sync::Arc;
use std::time::SystemTime;

use egui::{Id, Role, Ui, WidgetInfo};
use reda_engine::model::{DocumentKind, Severity, Uuid};
use reda_engine::preferences::GridStyle;
use reda_engine::preferences::Units;
use serde_json::json;

use super::{Out, region};
use crate::messages::CheckKind;
use crate::panels::messages::Action;
use crate::pcb::view::format_length;
use crate::properties::overview::{
    BoardFigures, CheckSummary, FabAction, FabItem, FabLine, ready_for_fab,
};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Status, Tooltip as _};

pub(super) fn show(ui: &mut Ui, shell: &Shell) -> Vec<Out> {
    let Some(document) = shell.active_document().map(|d| d.document) else {
        return Vec::new();
    };
    let project = shell.engine().project();
    let kind = project
        .and_then(|p| p.documents().find(|d| d.uuid == document))
        .map(|d| d.kind);
    let board = kind == Some(DocumentKind::Pcb);
    let sheet = kind == Some(DocumentKind::Schematic);
    let figures = if board { figures(ui, shell) } else { None };
    let mut out = Vec::new();
    let run = |out: &mut Vec<Out>, commands: Vec<(&'static str, serde_json::Value)>| {
        out.extend(commands.into_iter().map(|(id, p)| Out::Run(id, p)));
    };
    egui::ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            if let Some(figures) = &figures {
                region(
                    ui,
                    shell,
                    "ready_for_fab",
                    tr!("properties-ready-for-fab"),
                    |ui| {
                        ready(ui, &ready_for_fab(shell, figures), &mut out);
                    },
                );
            }
            region(
                ui,
                shell,
                "selection_filter",
                tr!("selection-filter-title"),
                |ui| {
                    let commands = crate::selection::filter_ui::controls(ui, shell, document);
                    run(&mut out, commands);
                },
            );
            region(ui, shell, "snap", tr!("snap-palette-title"), |ui| {
                let commands = crate::snap::palette_controls(ui, shell, document);
                run(&mut out, commands);
                let units = units(shell, board, document);
                let (label, value) = if board {
                    let distance = shell.preferences().pcb.snap_distance.0;
                    (
                        tr!("properties-snap-distance"),
                        format_length(distance, units),
                    )
                } else {
                    let range = shell.electrical_range(document);
                    (
                        tr!("properties-electrical-range"),
                        format_length(range, units),
                    )
                };
                ui.horizontal(|ui| {
                    ui.add(widgets::form_name(label));
                    ui.label(value);
                });
            });
            if let Some(figures) = &figures {
                region(ui, shell, "board", tr!("properties-board"), |ui| {
                    board_figures(ui, figures, units(shell, true, document));
                });
            }
            if sheet && let Some(project) = project {
                region(ui, shell, "sheet", tr!("properties-sheet"), |ui| {
                    super::sheet_frame::show(ui, project, document, &mut out);
                });
            }
            if board || sheet {
                region(ui, shell, "grid", tr!("properties-grid"), |ui| {
                    grid(ui, shell, board, document, &mut out);
                });
                region(ui, shell, "other", tr!("properties-other"), |ui| {
                    other(ui, shell, board, document, &mut out);
                });
                let check = if board {
                    CheckKind::Drc
                } else {
                    CheckKind::Erc
                };
                region(ui, shell, "checks", tr!("properties-region-checks"), |ui| {
                    let summary = CheckSummary::of(shell, check, figures.as_deref());
                    check_summary(ui, &summary, &mut out);
                });
                region(ui, shell, "links", tr!("properties-links"), |ui| {
                    let (settings, title) = if board {
                        ("board_setup", tr!("properties-link-board-setup"))
                    } else {
                        ("project_settings", tr!("properties-link-project-settings"))
                    };
                    if ui.add(widgets::link(title)).clicked() {
                        out.push(Out::Run(
                            "app.settings.open",
                            json!({ "settings": settings }),
                        ));
                    }
                });
            }
        });
    out
}

/// The board's figures, worked out again only when the board changed.
fn figures(ui: &Ui, shell: &Shell) -> Option<Arc<BoardFigures>> {
    let project = shell.engine().project()?;
    let id = Id::NULL.with("properties-board-figures");
    if let Some(cached) = ui.data(|d| d.get_temp::<Arc<BoardFigures>>(id))
        && cached.current(project)
    {
        return Some(cached);
    }
    let figures = Arc::new(BoardFigures::of(project)?);
    ui.data_mut(|d| d.insert_temp(id, figures.clone()));
    Some(figures)
}

/// The units lengths show in: the board's, or those of the sheet's grid.
fn units(shell: &Shell, board: bool, document: Uuid) -> Units {
    if board {
        shell.board_view().units
    } else if crate::grid::is_imperial(shell.grid(document)) {
        Units::Mil
    } else {
        Units::Mm
    }
}

/// The status a severity shows as.
pub(super) fn severity_status(severity: Severity) -> Status {
    match severity {
        Severity::Error => Status::Error,
        Severity::Warning => Status::Warning,
        Severity::Off => Status::Off,
    }
}

/// The mark of a severity: its shape in its colour.
pub(super) fn severity_dot(ui: &mut Ui, severity: Severity) {
    ui.add(widgets::status_mark(severity_status(severity)));
}

/// An OK tick or a warning mark, read out as "OK" or "!".
fn status(ui: &mut Ui, ok: bool) {
    let (status, name) = if ok {
        (Status::Success, "OK")
    } else {
        (Status::Warning, "!")
    };
    let response = ui.add(widgets::status_mark(status));
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, name));
}

fn ready(ui: &mut Ui, lines: &[FabLine], out: &mut Vec<Out>) {
    for line in lines {
        ui.horizontal(|ui| {
            status(ui, line.ok);
            let count = line.count.unwrap_or(0) as i64;
            let text = match line.item {
                FabItem::Outline => tr!("properties-fab-outline"),
                FabItem::Unrouted => tr!("properties-fab-unrouted", count = count),
                FabItem::Zones => tr!("properties-fab-zones"),
                FabItem::Drc => tr!("properties-fab-drc"),
                FabItem::Sync => tr!("properties-fab-sync", count = count),
                FabItem::NotFitted => tr!("properties-fab-not-fitted", count = count),
            };
            ui.label(&text);
            let Some(action) = &line.action else {
                return;
            };
            let button = match line.item {
                FabItem::Unrouted => tr!("properties-fab-show"),
                FabItem::Zones => tr!("properties-fab-fill"),
                FabItem::Drc => tr!("properties-fab-run"),
                _ => tr!("properties-fab-sync-button"),
            };
            // Named for its line: other panels have a "Run" or a "Show" of their own.
            let name = format!("{button} ({text})");
            let response = ui.add(widgets::button(button));
            response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
            if response.clicked() {
                out.push(match action {
                    FabAction::Run(id, params) => Out::Run(id, params.clone()),
                    FabAction::RunDrc => Out::Messages(Action::Run(CheckKind::Drc)),
                });
            }
        });
    }
}

fn board_figures(ui: &mut Ui, figures: &BoardFigures, units: Units) {
    let info = &figures.info;
    widgets::form("properties-board").show(ui, |ui| {
        let mut row = |name: String, value: String| {
            widgets::form_row(ui, name, |ui| ui.label(value));
        };
        if let (Some(w), Some(h)) = (info.width, info.height) {
            row(
                tr!("properties-board-size"),
                format!("{} x {}", format_length(w, units), format_length(h, units)),
            );
        }
        row(
            tr!("properties-board-area"),
            format!("{:.2} mm2", info.area),
        );
        row(
            tr!("properties-board-component-area"),
            format!("{:.2} mm2", info.component_area),
        );
        if let Some(density) = info.density {
            row(
                tr!("properties-board-density"),
                format!("{:.1}%", density * 100.0),
            );
        }
        row(
            tr!("properties-board-routed"),
            format!("{:.1}%", info.routed_percent),
        );
        row(
            tr!("properties-board-components"),
            info.components.to_string(),
        );
        row(tr!("properties-board-layers"), info.layers.to_string());
        row(tr!("properties-board-nets"), info.nets.to_string());
        row(
            tr!("properties-board-primitives"),
            info.primitives.to_string(),
        );
    });
}

/// The current grid with the preset picker.
fn grid(ui: &mut Ui, shell: &Shell, board: bool, document: Uuid, out: &mut Vec<Out>) {
    let current = shell.grid(document);
    let rows = shell.grids().rows(shell.engine().elements(), document);
    let presets: Vec<_> = (rows.iter().map(|r| r.snap.0))
        .filter(|g| {
            !board || crate::grid::is_imperial(*g) == (shell.board_view().units == Units::Mil)
        })
        .collect();
    let mut picked = current;
    let response = widgets::dropdown("properties-grid", crate::grid::label(current))
        .show(ui, |ui| {
            for g in &presets {
                widgets::choice_value(ui, &mut picked, *g, crate::grid::label(*g));
            }
        })
        .response;
    let name = tr!("layers-grid");
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, name.clone()));
    if picked != current {
        out.push(Out::Run(
            "view.grid.set",
            json!({"document": document, "grid": picked}),
        ));
    }
    // The visible grid: on or off for this Document, and its style (a Preference).
    let visible = shell.grids().visible(shell.engine().elements(), document);
    let mut shown = visible.shown;
    let visible_box = widgets::checkbox(&mut shown, tr!("properties-grid-visible"));
    if ui.add(visible_box).changed() {
        out.push(Out::Run(
            "view.grid.show",
            json!({"document": document, "visible": shown}),
        ));
    }
    let mut style = visible.style;
    let title = |s: GridStyle| match s {
        GridStyle::Dots => tr!("properties-grid-dots"),
        GridStyle::Lines => tr!("properties-grid-lines"),
    };
    let response = widgets::dropdown("properties-grid-style", title(style))
        .show(ui, |ui| {
            for s in [GridStyle::Dots, GridStyle::Lines] {
                widgets::choice_value(ui, &mut style, s, title(s));
            }
        })
        .response;
    let name = tr!("properties-grid-style");
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, name.clone()));
    if style != visible.style {
        out.push(Out::Run(
            "app.preferences.set",
            json!({"key": "editing.grid.style", "value": style}),
        ));
    }
}

/// Units: switched on a board; on a sheet they follow the grid.
fn other(ui: &mut Ui, shell: &Shell, board: bool, document: Uuid, out: &mut Vec<Out>) {
    let current = units(shell, board, document);
    ui.horizontal(|ui| {
        ui.add(widgets::form_name(tr!("layers-units")));
        let mut picked = current;
        let options = [(Units::Mm, "mm"), (Units::Mil, "mil")];
        let switch = ui.add_enabled(board, widgets::segmented(&mut picked, &options));
        if switch.changed() {
            out.push(Out::Run("view.units.toggle", json!({})));
        }
    });
}

/// The check summary: last run, stale flag, Error, Warning and waived counts, **Run**,
/// and the health checks with **Fix**.
fn check_summary(ui: &mut Ui, summary: &CheckSummary, out: &mut Vec<Out>) {
    let name = summary.kind.id().to_uppercase();
    let t = ui.tokens();
    ui.horizontal(|ui| {
        ui.label(t.ty.body_strong.rich(name.clone()));
        let last = match summary.ran_at {
            None => tr!("properties-checks-never"),
            Some(at) => ago(at),
        };
        ui.label(last);
        if summary.stale {
            ui.colored_label(t.color.warning.text, tr!("properties-checks-stale"));
        }
    });
    // The tallies are badges: on its severity's tint while there is something to count.
    ui.horizontal_wrapped(|ui| {
        let tally = |text: String, count: usize, status: Status| {
            let badge = widgets::badge(text);
            if count > 0 {
                badge.status(status)
            } else {
                badge
            }
        };
        let errors = tr!("properties-checks-errors", count = summary.errors as i64);
        ui.add(tally(errors, summary.errors, Status::Error));
        let warnings = tr!(
            "properties-checks-warnings",
            count = summary.warnings as i64
        );
        ui.add(tally(warnings, summary.warnings, Status::Warning));
        ui.add(widgets::badge(tr!(
            "properties-checks-waived",
            count = summary.waived as i64
        )));
    });
    let run = tr!("properties-checks-run", check = name);
    if ui.add(widgets::button(run)).clicked() {
        out.push(Out::Messages(Action::Run(summary.kind)));
    }
    if summary.health.is_empty() {
        return;
    }
    ui.weak(tr!("properties-checks-health"));
    for line in &summary.health {
        ui.horizontal(|ui| {
            let title = crate::messages::check_title(line.check.id());
            ui.label(format!("{title} ({})", line.count));
            let fix = ui
                .add(widgets::button(tr!("properties-checks-fix")))
                .tooltip(tr!("properties-checks-fix-hint", check = title.clone()));
            let label = tr!("properties-checks-fix-named", check = title);
            fix.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
            if fix.clicked() {
                out.push(Out::Run(line.fix.0, line.fix.1.clone()));
            }
        });
    }
}

/// How long ago `at` was, roughly.
fn ago(at: SystemTime) -> String {
    let minutes = at.elapsed().map_or(0, |d| d.as_secs() / 60) as i64;
    match minutes {
        0 => tr!("properties-checks-just-now"),
        m if m < 60 => tr!("properties-checks-minutes-ago", count = m),
        m => tr!("properties-checks-hours-ago", count = m / 60),
    }
}
