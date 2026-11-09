//! The Routing Bar (docs/spec/v0.1/app.md#routing-bar): while the Track Tool is active, a
//! bar at the bottom centre of the board canvas in the Active Bar's style, read from the
//! `tool_bars.route` list of `bars/pcb.json`. Each button runs the Command its key runs;
//! this module only says how each shows the mode in force: the width as text (only with a
//! route in flight), the corner and obstacle modes by their icon, the toggles toggled. The
//! Tool's note shows just above it, and the width or Via size chooser above that.

use std::collections::BTreeMap;

use egui::{Align2, Id, Order, Rect, Role, Ui, Vec2};
use reda_engine::model::geom::Nm;
use reda_engine::pcb::route::{CornerMode, ObstacleMode, WidthSource};
use reda_engine::preferences::ClearanceDisplay;
use serde_json::json;

use super::route::Chooser;
use crate::a11y;
use crate::bars::draw::{Cx, Invocation, ToolBarItem, tool_bar, tool_bar_anchor};
use crate::bars::{self, Action, Binding, DocumentType};
use crate::shell::Shell;
use crate::tools::Tool;
use crate::tr;

/// How the item bound to `action` shows now.
pub fn item_state(shell: &Shell, action: &Action) -> ToolBarItem {
    let id = match &action.binding {
        Binding::Command(id) | Binding::Preset(id) => id.as_str(),
        Binding::Raw { command, .. } => command.as_str(),
    };
    let router = shell.router();
    let prefs = &shell.preferences().pcb.route;
    let icon = |key: &str| Some(key.to_owned());
    match id {
        "pcb.route.chooser:width" => match shell.route_preview() {
            Some(p) => ToolBarItem {
                text: Some(super::view::format_length(p.width, shell.board_view().units)),
                ..ToolBarItem::default()
            },
            None => ToolBarItem {
                hidden: true,
                ..ToolBarItem::default()
            },
        },
        "tool.cycle_corner" => ToolBarItem {
            icon: icon(match router.corner() {
                CornerMode::Deg45 => "route_corner_45",
                CornerMode::Deg90 => "route_corner_90",
                CornerMode::Any => "route_corner_any",
            }),
            ..ToolBarItem::default()
        },
        "pcb.route.mode:lookahead" => toggle(router.lookahead()),
        "pcb.route.mode:obstacle_mode" => ToolBarItem {
            icon: icon(match prefs.obstacle_mode {
                ObstacleMode::Stop => "route_obstacle_stop",
                ObstacleMode::Ignore => "route_obstacle_ignore",
                ObstacleMode::Highlight => "route_obstacle_highlight",
            }),
            toggled: Some(prefs.obstacle_mode == ObstacleMode::Stop),
            ..ToolBarItem::default()
        },
        "pcb.route.mode:auto_terminate" => toggle(prefs.auto_terminate),
        "pcb.route.mode:loop_removal" => toggle(prefs.loop_removal),
        "pcb.route.mode:clearance_display" => ToolBarItem {
            icon: icon(match prefs.clearance_display {
                ClearanceDisplay::All => "route_clearance_all",
                _ => "route_clearance_near",
            }),
            toggled: Some(prefs.clearance_display != ClearanceDisplay::Off),
            ..ToolBarItem::default()
        },
        "view.keys_card.toggle" => ToolBarItem {
            icon: icon("route_keys"),
            ..ToolBarItem::default()
        },
        _ => ToolBarItem::default(),
    }
}

fn toggle(on: bool) -> ToolBarItem {
    ToolBarItem {
        toggled: Some(on),
        ..ToolBarItem::default()
    }
}

/// Draws the Routing Bar, its note and an open chooser over the board canvas `canvas`
/// (`id` is the canvas's), while the Track Tool is active. Returns what was clicked.
pub fn show(ui: &Ui, shell: &Shell, id: Id, canvas: Rect) -> Vec<Invocation> {
    let mut out = Vec::new();
    if shell.tool() != Tool::Route {
        return out;
    }
    let Some(items) = bars::for_document(Some(DocumentType::Pcb))
        .tool_bars
        .get("route")
    else {
        return out;
    };
    let checked = BTreeMap::new();
    let document = shell.engine().project().and_then(|p| p.pcb()).map(|p| p.uuid);
    let cx = Cx {
        shell,
        checked: &checked,
        document,
    };
    let anchor = tool_bar_anchor(canvas, super::layer_control::strip_shown(shell));
    let state = |action: &Action| item_state(shell, action);
    let note = shell.router().note();
    out.extend(tool_bar(ui, &cx, items, id.with("routing-bar"), anchor, note, &state));
    if let Some(chooser) = shell.router().chooser() {
        // Above the bar and its note.
        let above = anchor - Vec2::new(0.0, 2.0 * crate::chrome::BAR_BUTTON + 24.0);
        out.extend(chooser_ui(ui, shell, chooser, id.with("route-chooser"), above));
    }
    out
}

/// The width or Via size chooser: a list and a typed field. A row or Enter in the field
/// runs `pcb.route.width` or `pcb.route.via_size`.
fn chooser_ui(
    ui: &Ui,
    shell: &Shell,
    chooser: &Chooser,
    id: Id,
    at: egui::Pos2,
) -> Option<Invocation> {
    let units = shell.board_view().units;
    let length = |n: Nm| super::view::format_length(n, units);
    let source = |s: WidthSource| match s {
        WidthSource::Min => tr!("route-chooser-min"),
        WidthSource::Preferred => tr!("route-chooser-preferred"),
        WidthSource::Max => tr!("route-chooser-max"),
        WidthSource::Favourite => tr!("route-chooser-favourite"),
        WidthSource::Board => tr!("route-chooser-board"),
    };
    let field_id = id.with("typed");
    egui::Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(at)
        .pivot(Align2::CENTER_BOTTOM)
        .show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    let mut clicked = None;
                    let title = match chooser {
                        Chooser::Width(_) => tr!("route-chooser-width"),
                        Chooser::Via(_) => tr!("route-chooser-via"),
                    };
                    ui.strong(&title);
                    match chooser {
                        Chooser::Width(rows) => {
                            for row in rows {
                                let label = format!("{}  {}", length(row.width), source(row.source));
                                let response = ui.selectable_label(false, &label);
                                a11y::name(&response, Role::ListItem, true, &label, None, None);
                                if response.clicked() {
                                    clicked = Some((
                                        "pcb.route.width".to_owned(),
                                        json!({ "width": row.width.0 }),
                                    ));
                                }
                            }
                        }
                        Chooser::Via(rows) => {
                            for row in rows {
                                let label = format!(
                                    "{} / {}  {}",
                                    length(row.size.diameter),
                                    length(row.size.hole),
                                    source(row.source)
                                );
                                let response = ui.selectable_label(false, &label);
                                a11y::name(&response, Role::ListItem, true, &label, None, None);
                                if response.clicked() {
                                    clicked = Some((
                                        "pcb.route.via_size".to_owned(),
                                        json!({ "diameter": row.size.diameter.0,
                                                "hole": row.size.hole.0 }),
                                    ));
                                }
                            }
                        }
                    }
                    let mut typed: String = ui.data(|d| d.get_temp(field_id)).unwrap_or_default();
                    let hint = match chooser {
                        Chooser::Width(_) => tr!("route-chooser-type-width"),
                        Chooser::Via(_) => tr!("route-chooser-type-via"),
                    };
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut typed)
                            .hint_text(&hint)
                            .desired_width(160.0),
                    );
                    a11y::name(&response, Role::TextInput, true, &hint, None, None);
                    let entered =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if entered && !typed.trim().is_empty() {
                        clicked = typed_invocation(chooser, typed.trim());
                        typed.clear();
                    }
                    ui.data_mut(|d| d.insert_temp(field_id, typed));
                    clicked
                })
                .inner
        })
        .inner
}

/// What a typed value runs: a width (`0.3mm`), or a Via size as diameter and hole
/// (`0.6mm 0.3mm`).
fn typed_invocation(chooser: &Chooser, typed: &str) -> Option<Invocation> {
    match chooser {
        Chooser::Width(_) => Some(("pcb.route.width".to_owned(), json!({ "width": typed }))),
        Chooser::Via(_) => {
            let mut parts = typed.split(|c: char| c == '/' || c.is_whitespace());
            let mut next = || parts.find(|p| !p.is_empty()).map(str::to_owned);
            let (diameter, hole) = (next()?, next()?);
            Some((
                "pcb.route.via_size".to_owned(),
                json!({ "diameter": diameter, "hole": hole }),
            ))
        }
    }
}
