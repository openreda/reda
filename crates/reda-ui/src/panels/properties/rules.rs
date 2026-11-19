//! The Rules region (which design rule records apply, docs/spec/v0.1/checks.md#explaining-a-rule)
//! and a locally edited Footprint's badge with Revert and the compare
//! (docs/spec/v0.1/pcb.md#placed-pad-edits).

use egui::{Id, Pos2, Role, Sense, Shape, Stroke, Ui, Vec2, WidgetInfo};
use reda_engine::model::{Footprint, PadShape, RuleValue, Severity};
use reda_engine::preferences::Units;
use serde_json::Value;

use super::{Out, overview::severity_dot, region, set_open};
use crate::layout::SettingsWindow;
use crate::panels::messages::Action;
use crate::pcb::view::format_length;
use crate::properties::LocalEdits;
use crate::properties::commands::RevealRules;
use crate::properties::rules::{self as model, Rules, Standing};
use crate::shell::Shell;
use crate::theme::{self, TokensExt as _};
use crate::tr;
use crate::widgets::{self, BadgeTone, Column, Status, Tooltip as _};

/// The size of the drawing that compares edited Pads with the copy they came from.
const COMPARE_SIZE: Vec2 = Vec2::new(220.0, 120.0);

/// The Rules region: per rule kind, a tick on the records that won (with the value), a
/// cross on those that lost and strike-through on disabled ones. A row opens its Board
/// Setup page. Right-click **Applicable Rules** (`view.properties.rules`) opens the
/// region and scrolls to it.
pub(super) fn show(ui: &mut Ui, shell: &Shell, rules: &Rules, units: Units, out: &mut Vec<Out>) {
    let asked = shell
        .engine()
        .view()
        .get::<RevealRules>()
        .map_or(0, |r| r.0);
    let seen_id = Id::NULL.with("properties-rules-revealed");
    let seen: u64 = ui.data(|d| d.get_temp(seen_id)).unwrap_or(0);
    let reveal = asked != seen;
    if reveal {
        ui.data_mut(|d| d.insert_temp(seen_id, asked));
        set_open(ui, shell, "rules", true);
    }
    let response = region(ui, shell, "rules", tr!("properties-region-rules"), |ui| {
        let t = ui.tokens();
        let focused = shell.panel_focused(crate::layout::PanelId::Properties);
        if rules.kinds.is_empty() {
            ui.weak(tr!("properties-rules-none"));
        }
        for kind in &rules.kinds {
            let page = model::page(kind.kind);
            ui.horizontal(|ui| {
                ui.label(t.ty.body_strong.rich(kind.kind.id()));
                if !kind.layer_numbers.is_empty() {
                    ui.label(
                        kind.layer_numbers
                            .iter()
                            .map(|n| format!("L{n}"))
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                }
                if let Some(severity) = kind.severity {
                    severity_dot(ui, severity);
                    ui.weak(severity_label(severity));
                }
            });
            for row in &kind.rows {
                let source = row
                    .class
                    .clone()
                    .unwrap_or_else(|| tr!("properties-rules-board"));
                let source = row
                    .layers
                    .as_ref()
                    .map_or_else(|| source.clone(), |l| format!("{source} · {l}"));
                let mut text = match (&row.value, row.standing) {
                    (Some(value), _) => format!(
                        "{} = {}  ({source})",
                        row.fields.join(", "),
                        value_text(value, units)
                    ),
                    (None, Standing::Disabled) => {
                        format!("{source}: {}", tr!("properties-rules-disabled"))
                    }
                    (None, _) => format!("{source}: {}", row.fields.join(", ")),
                };
                if rules.objects == 2 {
                    text = format!("{}  {text}", row.object + 1);
                }
                // A row: its standing as a mark (a disabled record is muted), and the
                // record. The tooltip has the whole line, should the Panel cut it short.
                let (standing, status) = match row.standing {
                    Standing::Won => (tr!("properties-rules-won"), Status::Success),
                    Standing::Lost => (tr!("properties-rules-lost"), Status::Error),
                    Standing::Disabled => (tr!("properties-rules-disabled"), Status::Off),
                };
                let line = widgets::list_row(text.as_str())
                    .status(status)
                    .muted(row.standing == Standing::Disabled)
                    .panel_focused(focused);
                let response = ui.add(line);
                let name = format!("{standing}: {text}");
                response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                let open = tr!("properties-rules-open", page = page.clone());
                let clicked = response.tooltip_titled(&text, open).clicked();
                if clicked {
                    out.push(Out::Messages(Action::ShowPage(
                        SettingsWindow::BoardSetup,
                        page.clone(),
                    )));
                }
            }
            let limits = kind
                .between
                .iter()
                .map(|(f, v, o)| (f, v, o, &kind.pair))
                .chain(kind.between_holes.iter().map(|(f, v, o, p)| (f, v, o, p)));
            for (field, value, object, pair) in limits {
                if let Some(pair) = pair {
                    ui.weak(format!("{pair} · {field} = {}", value_text(value, units)));
                } else if let Some(object) = object {
                    ui.weak(tr!(
                        "properties-rules-between",
                        field = field.clone(),
                        value = value_text(value, units),
                        object = (object + 1) as i64
                    ));
                }
            }
            ui.add_space(t.space.gap / 2.0);
        }
    });
    if reveal {
        response.header.scroll_to_me(Some(egui::Align::TOP));
    }
}

fn severity_label(severity: Severity) -> String {
    match severity {
        Severity::Error => tr!("properties-severity-error"),
        Severity::Warning => tr!("properties-severity-warning"),
        Severity::Off => tr!("properties-severity-off"),
    }
}

/// A rule value as people read it.
fn value_text(value: &RuleValue, units: Units) -> String {
    match value {
        RuleValue::Length(nm) => format_length(*nm, units),
        RuleValue::Flag(on) => on.to_string(),
        RuleValue::Percent(p) => format!("{p}%"),
        RuleValue::Area(nm2) => format!("{} mm2", *nm2 as f64 / 1e12),
        RuleValue::Count(n) => n.to_string(),
        RuleValue::Angle(md) => format!("{} deg", md.0 as f64 / 1000.0),
        other => match serde_json::to_value(other) {
            Ok(Value::String(s)) => s,
            Ok(v) => v.to_string(),
            Err(_) => format!("{other:?}"),
        },
    }
}

/// The locally edited badge with Revert and, for one instance, the compare: what changed
/// (Pad or graphic, property, edited against original) and the edited Pads drawn over
/// the copy they came from. Returns Revert's Command when it was clicked.
pub(super) fn local_edits(ui: &mut Ui, local: &LocalEdits) -> Option<(&'static str, Value)> {
    let mut revert = false;
    ui.horizontal(|ui| {
        let badge = widgets::badge(tr!("properties-locally-edited")).tone(BadgeTone::Warning);
        ui.add(badge).tooltip(tr!("properties-locally-edited-hint"));
        revert = ui
            .add(widgets::button(tr!("properties-revert-local-edits")))
            .tooltip(tr!("properties-revert-local-edits-hint"))
            .clicked();
    });
    if !local.edits.is_empty() {
        widgets::section(tr!("properties-compare"))
            .id_salt("properties-compare")
            .show(ui, |ui| {
                // What changed, the edited value, and the original in muted text.
                let columns = [Column::auto(), Column::auto(), Column::auto()];
                let table = widgets::table("properties-compare-rows").columns(columns);
                table.show(ui, |table| {
                    for edit in &local.edits {
                        let what = match (&edit.number, edit.graphic) {
                            (Some(number), _) => {
                                tr!("properties-compare-pad", number = number.clone())
                            }
                            (None, Some(_)) => tr!("properties-compare-graphic"),
                            (None, None) => String::new(),
                        };
                        table.row(|row| {
                            row.text(format!("{what} {}", edit.property));
                            row.text(compare_value(edit.edited.as_ref()));
                            row.muted(compare_value(edit.original.as_ref()));
                        });
                    }
                });
                if let Some((edited, original)) = &local.compare {
                    compare_drawing(ui, edited, original);
                }
            });
    }
    revert.then(|| {
        (
            "pcb.footprint.revert_local_edits",
            serde_json::json!({"instances": local.instances}),
        )
    })
}

fn compare_value(value: Option<&Value>) -> String {
    match value {
        None => "-".to_owned(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}

/// The original copy's Pads in outline, the edited Pads filled over them.
pub(crate) fn compare_drawing(ui: &mut Ui, edited: &Footprint, original: &Footprint) {
    let t = ui.tokens();
    let (rect, response) = ui.allocate_exact_size(COMPARE_SIZE, Sense::hover());
    let label = tr!("properties-compare-drawing");
    response.widget_info(|| WidgetInfo::labeled(egui::Role::Image, true, label.clone()));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, t.radius.mark, t.color.surface.well);
    let outlines: Vec<Vec<(f64, f64)>> = original.pads.iter().map(pad_outline).collect();
    let fills: Vec<Vec<(f64, f64)>> = edited.pads.iter().map(pad_outline).collect();
    let all = outlines.iter().chain(&fills).flatten();
    let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for &(x, y) in all {
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
    }
    if min.0 > max.0 {
        return;
    }
    let inner = rect.shrink(t.space.gap);
    let scale = (inner.width() as f64 / (max.0 - min.0).max(1.0))
        .min(inner.height() as f64 / (max.1 - min.1).max(1.0));
    let mid = ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0);
    let to_screen = |(x, y): (f64, f64)| {
        Pos2::new(
            inner.center().x + ((x - mid.0) * scale) as f32,
            inner.center().y - ((y - mid.1) * scale) as f32,
        )
    };
    // The edited Pads are the warning tint, laid twice so they read on the well.
    let tint = t.color.warning.subtle;
    let edited_color = theme::over(tint, theme::over(tint, t.color.surface.well));
    for points in &fills {
        let points: Vec<Pos2> = points.iter().map(|p| to_screen(*p)).collect();
        painter.add(Shape::convex_polygon(points, edited_color, Stroke::NONE));
    }
    let original_stroke = Stroke::new(widgets::glyph::STROKE, t.color.text.primary);
    for points in &outlines {
        let points: Vec<Pos2> = points.iter().map(|p| to_screen(*p)).collect();
        painter.add(Shape::closed_line(points, original_stroke));
    }
}

/// A Pad's outline in nanometres: its rotated box, or a polygon for a circle.
fn pad_outline(pad: &reda_engine::model::Pad) -> Vec<(f64, f64)> {
    let (w, h) = (pad.width.0 as f64 / 2.0, pad.height.0 as f64 / 2.0);
    let corners: Vec<(f64, f64)> = if pad.shape == PadShape::Circle {
        (0..16)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / 16.0;
                (w * a.cos(), w * a.sin())
            })
            .collect()
    } else {
        vec![(-w, -h), (w, -h), (w, h), (-w, h)]
    };
    let angle = (pad.rotation.0 as f64 / 1000.0).to_radians();
    let (sin, cos) = angle.sin_cos();
    let at = (pad.at.x.0 as f64, pad.at.y.0 as f64);
    corners
        .into_iter()
        .map(|(x, y)| (at.0 + x * cos - y * sin, at.1 + x * sin + y * cos))
        .collect()
}
