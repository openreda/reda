//! Drawing Board Setup's rule pages and DRC page; the Net Classes page is in
//! [`grid`].

use egui::{Role, Ui, WidgetInfo};
use reda_engine::model::{DrcCheck, Limit, Severity, SilkMaskMode, ZoneNameScheme};

use super::length::mm;
use super::model::{
    ANGLES, BoardSetupDraft, ClassKey, PasteMode, RuleField, SPOKES, STYLES, ValueKind,
    class_field, connection_value_name, kind_name, style_name, target_name,
};
use super::{DrcPage, Rule, Section};
use crate::settings::layout;
use crate::settings::{PageCx, Problem, choice_toggle, field, strong};
use crate::show_on_board::Shown;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Column};

mod actions;
mod grid;
mod scoped;
mod stackup;
mod teardrops;

pub use grid::net_classes_page;
pub use stackup::stackup_page;

const SEVERITIES: [Severity; 3] = [Severity::Error, Severity::Warning, Severity::Off];
/// A length field: room for a value such as `-0.125` in the body text.
const LENGTH_WIDTH: f32 = 96.0;
/// The Zone naming scheme's dropdown: the longest scheme fits.
const ZONE_SCHEME_WIDTH: f32 = 220.0;
/// The silkscreen-to-mask mode's dropdown.
const SILK_MODE_WIDTH: f32 = 180.0;

pub(super) fn severity_name(severity: Severity) -> String {
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
        DrcCheck::ViaSpan => tr!("check-via-span"),
        DrcCheck::MicroviaSize => tr!("check-microvia-size"),
        DrcCheck::AspectRatio => tr!("check-aspect-ratio"),
        DrcCheck::MaxStack => tr!("check-max-stack"),
        DrcCheck::SilkWidth => tr!("check-silk-width"),
        DrcCheck::SilkTextHeight => tr!("check-silk-text-height"),
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
            let accessible = tr!(
                "board-setup-severity-choice",
                check = name.as_str(),
                severity = label.as_str()
            );
            if choice_toggle(ui, chosen, &label, &accessible).clicked() {
                board.set_severity(check, severity);
            }
        }
    });
}

fn no_pcb(ui: &mut Ui, board: &BoardSetupDraft) -> bool {
    if board.has_pcb() {
        return false;
    }
    layout::muted(ui, tr!("board-setup-no-pcb"));
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
    let t = ui.tokens();
    actions::rule_actions(ui, cx, board, Some(rule));
    if rule == Rule::Teardrops {
        teardrops::outcomes(ui, cx);
    }
    if rule == Rule::ZoneConnections {
        ui.add_space(t.space.stack);
        zone_scheme(ui, board);
    }
    for (i, section) in rule.sections().iter().enumerate() {
        match section_heading(rule, i, section) {
            Some(heading) => layout::group(ui, &heading),
            None => ui.add_space(t.space.section),
        }
        ui.add_space(t.space.stack);
        section_ui(ui, cx, section, board);
        let kind = section.fields[0].rule().0;
        if kind.accepts_layers()
            && let Some(project) = cx.shell.engine().project()
            && project.pcb().is_some()
        {
            ui.push_id(("layer-rules", i), |ui| {
                scoped::rule_section(ui, project, board, kind, section.fields)
            });
        }
    }
    ui.add_space(t.space.section);
    layout::muted(ui, tr!("board-setup-units"));
}

/// A section's heading: its check's name on a page with several, else what it is about.
fn section_heading(rule: Rule, i: usize, section: &Section) -> Option<String> {
    if let Some(RuleField::Teardrop(kind, _)) = section.fields.first() {
        return Some(super::model::tear_target_name(*kind));
    }
    if let Some(target) = section.target {
        return Some(target_name(target));
    }
    match section.check {
        Some(DrcCheck::ViaSize) => Some(tr!("board-setup-drilled-vias")),
        Some(DrcCheck::MicroviaSize) => Some(tr!("board-setup-micro-vias")),
        Some(check) if rule.sections().len() > 1 => Some(check_name(check)),
        Some(_) => None,
        None if rule == Rule::MaskPaste && i == 0 => Some(tr!("board-setup-solder-mask")),
        None => Some(tr!("board-setup-paste-heading")),
    }
}

fn section_ui(ui: &mut Ui, cx: &mut PageCx<'_>, section: &Section, board: &mut BoardSetupDraft) {
    let t = ui.tokens();
    let salt = section.fields[0];
    widgets::form(("board-setup-rule", salt)).show(ui, |ui| {
        for f in section.board_fields() {
            if f == RuleField::PastePercent {
                continue;
            }
            if f == RuleField::PasteExpansion {
                paste(ui, cx, board);
                ui.end_row();
                continue;
            }
            let label = f.board_wide_label();
            ui.add(widgets::form_name(&label));
            let problem = cx.problem(&f.problem_field());
            let mut text = board.rule(f).to_owned();
            ui.horizontal(|ui| {
                value_field(ui, f, &label, &mut text, false, problem);
                if f == RuleField::MaskTop {
                    mask_link(ui, board);
                }
                if let Some(limit) = limit_text(board, f) {
                    layout::muted(ui, limit);
                }
            });
            if text != board.rule(f) {
                board.set_rule(f, text);
            }
            ui.end_row();
        }
        if section.check == Some(DrcCheck::SilkToMask) {
            silk_mode(ui, board);
            ui.end_row();
        }
    });
    let kind = section.fields[0].rule().0;
    let what = tr!("board-setup-board-wide-kind", kind = kind_name(kind));
    show_button(
        ui,
        cx,
        &what,
        board.shown(kind, ClassKey::Default).into_iter().collect(),
    );
    ui.add_space(t.space.section);
    if kind.accepts_net_class() {
        class_overrides(ui, cx, section, board);
    }
    if let Some(check) = section.check {
        ui.add_space(t.space.stack);
        ui.horizontal(|ui| {
            layout::text(ui, tr!("board-setup-severity", check = check_name(check)));
            severity_choice(ui, board, check);
        });
    }
}

fn class_overrides(
    ui: &mut Ui,
    cx: &mut PageCx<'_>,
    section: &Section,
    board: &mut BoardSetupDraft,
) {
    let salt = section.fields[0];
    let kind = salt.rule().0;
    let overrides = section.fields;
    let classes: Vec<(ClassKey, String)> = board
        .live_classes()
        .map(|c| (c.key(), c.name().to_owned()))
        .collect();
    strong(ui, tr!("board-setup-overrides"));
    if classes.is_empty() {
        layout::muted(ui, tr!("board-setup-no-classes"));
    } else {
        let columns = std::iter::repeat_n(Column::auto(), 2 + overrides.len());
        // A table of many values is wider than the page: it scrolls sideways on its own.
        egui::ScrollArea::horizontal()
            .id_salt(("board-setup-overrides-scroll", salt))
            .show(ui, |ui| {
                widgets::table(("board-setup-overrides", salt))
                    .columns(columns)
                    .show(ui, |table| {
                        table.header(|h| {
                            h.text(tr!("board-setup-net-class"));
                            h.empty();
                            for f in overrides {
                                h.text(column_name(*f));
                            }
                        });
                        for (key, name) in classes {
                            table.row(|row| {
                                row.text(&name);
                                let what = tr!(
                                    "board-setup-override",
                                    class = name.as_str(),
                                    rule = kind_name(kind)
                                );
                                let shown = board.shown(kind, key).into_iter().collect();
                                row.ui(|ui| show_button(ui, cx, &what, shown));
                                for &f in overrides {
                                    let label = tr!(
                                        "board-setup-override",
                                        class = name.as_str(),
                                        rule = f.name()
                                    );
                                    let problem = cx.problem(&class_field(key, &f.id()));
                                    row.ui(|ui| {
                                        if let Some(value) = board.class_override_mut(key, f) {
                                            value_field(ui, f, &label, value, true, problem);
                                        }
                                    });
                                }
                            });
                        }
                    });
            });
    }
    let note = match section.check {
        Some(DrcCheck::ViaSize) => tr!("board-setup-via-overrides-note"),
        Some(DrcCheck::MicroviaSize) => tr!("board-setup-microvia-overrides-note"),
        _ => tr!("board-setup-overrides-note"),
    };
    layout::muted(ui, note);
}

/// Show on board for one row, announced as "Show <what> on board"; disabled for a
/// class not applied yet.
pub fn show_button(ui: &mut Ui, cx: &mut PageCx<'_>, what: &str, shown: Vec<Shown>) {
    let label = tr!("board-setup-show-of", what = what);
    let on = !shown.is_empty();
    let button = ui.add_enabled(on, widgets::button(tr!("board-setup-show")));
    button.widget_info(|| WidgetInfo::labeled(Role::Button, on, &label));
    if button.clicked() {
        cx.shell.show_on_board(&shown);
    }
}

/// A value's column in an override table: a Zone connection's value without its
/// target, which the section heading names.
fn column_name(f: RuleField) -> String {
    match f {
        RuleField::Connection(_, v) => connection_value_name(v),
        f => f.name(),
    }
}

/// The widget for one value, announced as `label`: a text field for lengths,
/// percentages, ratios and counts, a checkbox for a board-wide flag, else a drop-down. With `inherit` (a
/// Net Class override) every drop-down also offers "Inherited", which leaves it empty.
pub fn value_field(
    ui: &mut Ui,
    f: RuleField,
    label: &str,
    value: &mut String,
    inherit: bool,
    problem: Option<&Problem>,
) {
    let options: &[&str] = match f.kind() {
        ValueKind::Length
        | ValueKind::Signed
        | ValueKind::Percent
        | ValueKind::SizePercent
        | ValueKind::Ratio
        | ValueKind::Count => {
            field(ui, label, value, LENGTH_WIDTH, problem);
            return;
        }
        ValueKind::Flag if !inherit => {
            let mut on = value == "true";
            let response = ui.add(widgets::checkbox(&mut on, ""));
            response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, label));
            if response.changed() {
                *value = on.to_string();
            }
            return;
        }
        ValueKind::Flag => &["true", "false"],
        ValueKind::Style => &STYLES,
        ValueKind::Spokes => &SPOKES,
        ValueKind::Angle => &ANGLES,
    };
    let shown = |option: &str| match option {
        "" => tr!("board-setup-inherited"),
        "true" => tr!("board-setup-yes"),
        "false" => tr!("board-setup-no"),
        other if f.kind() == ValueKind::Style => style_name(other),
        other => other.to_owned(),
    };
    let mut chosen = value.clone();
    let combo = widgets::dropdown(("board-setup-value", label), shown(value))
        .width(LENGTH_WIDTH)
        .show(ui, |ui| {
            if inherit {
                widgets::choice_value(ui, &mut chosen, String::new(), shown(""));
            }
            for option in options {
                widgets::choice_value(ui, &mut chosen, (*option).to_owned(), shown(option));
            }
        });
    combo
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label));
    if let Some(problem) = problem {
        crate::settings::problem_label(ui, problem);
    }
    if chosen != *value {
        *value = chosen;
    }
}

/// The toggle that links the top and bottom mask expansions.
fn mask_link(ui: &mut Ui, board: &mut BoardSetupDraft) {
    let mut linked = board.mask_linked();
    let label = tr!("board-setup-mask-link");
    if ui.add(widgets::checkbox(&mut linked, label)).changed() {
        board.set_mask_linked(linked);
    }
}

/// The paste expansion: absolute or a percentage, then the value.
fn paste(ui: &mut Ui, cx: &PageCx<'_>, board: &mut BoardSetupDraft) {
    ui.add(widgets::form_name(tr!("board-setup-paste-mode")));
    ui.horizontal(|ui| {
        let mut mode = board.paste_mode();
        for (m, name) in [
            (PasteMode::Absolute, tr!("board-setup-paste-absolute")),
            (PasteMode::Percent, tr!("board-setup-paste-percent-mode")),
        ] {
            if choice_toggle(ui, mode == m, &name, &name).clicked() {
                mode = m;
            }
        }
        if mode != board.paste_mode() {
            board.set_paste_mode(mode);
        }
        let f = match mode {
            PasteMode::Absolute => RuleField::PasteExpansion,
            PasteMode::Percent => RuleField::PastePercent,
        };
        let label = f.board_wide_label();
        let problem = cx.problem(&f.problem_field());
        value_field(ui, f, &label, board.rule_mut(f), false, problem);
    });
}

/// The board's Zone naming scheme.
fn zone_scheme(ui: &mut Ui, board: &mut BoardSetupDraft) {
    let label = tr!("board-setup-zone-scheme");
    ui.horizontal(|ui| {
        layout::text(ui, &label);
        let mut scheme = board.zone_name_scheme();
        let name = |s: ZoneNameScheme| {
            serde_json::to_value(s)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default()
        };
        let combo = widgets::dropdown("board-setup-zone-scheme", name(scheme))
            .width(ZONE_SCHEME_WIDTH)
            .show(ui, |ui| {
                for s in [
                    ZoneNameScheme::NetLxxPxxx,
                    ZoneNameScheme::LxxNetPxxx,
                    ZoneNameScheme::NetLayernamePxxx,
                    ZoneNameScheme::LayernameNetPxxx,
                ] {
                    widgets::choice_value(ui, &mut scheme, s, name(s));
                }
            });
        combo
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
        if scheme != board.zone_name_scheme() {
            board.set_zone_name_scheme(scheme);
        }
    });
    layout::muted(ui, tr!("board-setup-zone-scheme-note"));
}

/// What `silk_to_mask` measures silkscreen against.
fn silk_mode(ui: &mut Ui, board: &mut BoardSetupDraft) {
    let label = tr!("board-setup-silk-mode");
    ui.add(widgets::form_name(&label));
    let name = |mode: SilkMaskMode| match mode {
        SilkMaskMode::ExposedCopper => tr!("board-setup-silk-mode-copper"),
        SilkMaskMode::MaskOpening => tr!("board-setup-silk-mode-opening"),
    };
    let mut mode = board.silk_mode();
    let combo = widgets::dropdown("board-setup-silk-mode", name(mode))
        .width(SILK_MODE_WIDTH)
        .show(ui, |ui| {
            for m in [SilkMaskMode::ExposedCopper, SilkMaskMode::MaskOpening] {
                widgets::choice_value(ui, &mut mode, m, name(m));
            }
        });
    combo
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
    if mode != board.silk_mode() {
        board.set_silk_mode(mode);
    }
}

/// The DRC page: every check's severity, with the values that sit beside their checks.
pub fn drc_page(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    if no_pcb(ui, board) {
        return;
    }
    let t = ui.tokens();
    widgets::table("board-setup-drc")
        .columns([Column::auto(), Column::auto(), Column::auto()])
        .show(ui, |table| {
            for &check in DrcCheck::ALL {
                table.row(|row| {
                    row.text(check_name(check));
                    row.ui(|ui| severity_choice(ui, board, check));
                    match DrcPage::VALUES.iter().find(|(c, _)| *c == check) {
                        Some(&(_, f)) => row.ui(|ui| {
                            let label = f.board_wide_label();
                            layout::text(ui, &label);
                            let problem = cx.problem(&f.problem_field());
                            field(ui, &label, board.rule_mut(f), LENGTH_WIDTH, problem);
                        }),
                        None => row.empty(),
                    }
                });
            }
        });
    ui.add_space(t.space.section);
    layout::muted(ui, tr!("board-setup-units"));
}
