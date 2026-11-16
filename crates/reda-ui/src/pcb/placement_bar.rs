//! The Placement Bar (docs/spec/v0.1/app.md#placement-bar) and what a Footprint move
//! draws over the board canvas.
//!
//! - The bar takes the Routing Bar's place and form, read from the `tool_bars.place` list
//!   of `bars/pcb.json`, while the selection is only Footprints and during any Footprint
//!   move or queue. Each button runs the Command its key runs; [`item_state`] says how each
//!   shows the mode in force: the obstacle mode by its icon (toggled in `avoid`), the
//!   alignment guides toggled, the Ratsnest mode by its icon, and the queue's "3 of 12"
//!   only during Reposition Selected.
//! - The live overlap and the alignment guides a move draws are canvas marks, in
//!   [`super::marks::move_marks`].
//! - [`dialogs`]: Choose Component and the question a locked move asks.

use std::collections::BTreeMap;

use egui::{Id, Rect, Role, Ui};
use reda_engine::pcb::place::{ObstacleMode, RatsnestMode};
use serde_json::json;

use super::placing::{self, PlaceFlight};
use crate::a11y;
use crate::bars::draw::{Cx, Invocation, ToolBarItem, tool_bar, tool_bar_anchor};
use crate::bars::{self, Action, Binding, DocumentType};
use crate::shell::Shell;
use crate::tools::Tool;
use crate::tr;
use crate::widgets;

/// Whether the Placement Bar shows: in Select or Move, with only Footprints selected, or
/// a Footprint move or queue in flight.
pub fn shown(shell: &Shell) -> bool {
    if !matches!(shell.tool(), Tool::Select | Tool::Move) {
        return false;
    }
    let flight = placing::flight(shell);
    flight.moving
        || flight.queue.is_some()
        || shell
            .engine()
            .project()
            .is_some_and(|p| placing::only_footprints(p, shell.selection()))
}

/// How the item bound to `action` shows now.
pub fn item_state(shell: &Shell, action: &Action) -> ToolBarItem {
    let id = match &action.binding {
        Binding::Command(id) | Binding::Preset(id) => id.as_str(),
        Binding::Raw { command, .. } => command.as_str(),
    };
    let prefs = &shell.preferences().pcb.place;
    let icon = |key: &str| Some(key.to_owned());
    match id {
        "pcb.place.mode:obstacle_mode" => ToolBarItem {
            icon: icon(match prefs.obstacle_mode {
                ObstacleMode::Ignore => "place_obstacle_ignore",
                ObstacleMode::Avoid => "place_obstacle_avoid",
            }),
            toggled: Some(prefs.obstacle_mode == ObstacleMode::Avoid),
            ..ToolBarItem::default()
        },
        "pcb.place.mode:alignment_guides" => ToolBarItem {
            toggled: Some(prefs.alignment_guides),
            ..ToolBarItem::default()
        },
        "pcb.place.mode:ratsnest_mode" => ToolBarItem {
            icon: icon(match prefs.ratsnest_mode {
                RatsnestMode::Breaks => "place_ratsnest_breaks",
                RatsnestMode::Hidden => "place_ratsnest_hidden",
                RatsnestMode::PadToPad => "place_ratsnest_pad_to_pad",
            }),
            ..ToolBarItem::default()
        },
        "pcb.place.skip" => match placing::flight(shell).queue {
            Some((at, of)) => ToolBarItem {
                text: Some(tr!("place-queue-count", at = at, of = of)),
                ..ToolBarItem::default()
            },
            None => ToolBarItem {
                hidden: true,
                ..ToolBarItem::default()
            },
        },
        "view.keys_card.toggle" => ToolBarItem {
            icon: icon("route_keys"),
            ..ToolBarItem::default()
        },
        _ => ToolBarItem::default(),
    }
}

/// Draws the Placement Bar over the board canvas `canvas` (`id` is the canvas's) while it
/// is [`shown`]. Returns what was clicked.
pub fn show(ui: &Ui, shell: &Shell, id: Id, canvas: Rect) -> Option<Invocation> {
    if !shown(shell) {
        return None;
    }
    let items = bars::for_document(Some(DocumentType::Pcb))
        .tool_bars
        .get("place")?;
    let checked = BTreeMap::new();
    let document = shell
        .engine()
        .project()
        .and_then(|p| p.pcb())
        .map(|p| p.uuid);
    let cx = Cx {
        shell,
        checked: &checked,
        document,
    };
    let anchor = tool_bar_anchor(canvas, super::layer_control::strip_inset(ui, shell));
    let state = |action: &Action| item_state(shell, action);
    tool_bar(
        ui,
        &cx,
        items,
        id.with("placement-bar"),
        anchor,
        None,
        &state,
    )
}

/// Choose Component (while [`PlaceFlight::choose`] is open) and the locked-move question
/// (while [`PlaceFlight::confirm`] waits). Returns the Command a button runs.
pub fn dialogs(ctx: &egui::Context, shell: &mut Shell) -> Option<Invocation> {
    let flight = placing::flight(shell);
    if let Some(pending) = &flight.confirm {
        return confirm_dialog(ctx, pending.locked, pending.unroute);
    }
    let mut filter = flight.choose?;
    let pcb = shell.engine().project()?.pcb()?;
    let rows = placing::choices(pcb, &filter);
    let picked_id = Id::NULL.with("choose-component-picked");
    let mut picked: Option<reda_engine::model::Uuid> = ctx
        .data(|d| d.get_temp::<Option<reda_engine::model::Uuid>>(picked_id))
        .flatten();
    if picked.is_none_or(|p| !rows.iter().any(|(_, f)| *f == p)) {
        picked = rows.first().map(|(_, f)| *f);
    }
    let mut out = None;
    let mut close = false;
    let response =
        crate::modal::Dialog::new(Id::NULL.with("choose-component"), tr!("place-choose-title"))
            .width(320.0)
            .show(ctx, |ui| {
                let hint = tr!("place-choose-filter");
                let field = ui.add(
                    widgets::text_field(&mut filter)
                        .search()
                        .hint(&hint)
                        .width(f32::INFINITY),
                );
                a11y::name(&field, Role::TextInput, true, &hint, None, None);
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        widgets::rows(ui, |ui| {
                            for (designator, footprint) in &rows {
                                let row = widgets::list_row(designator.as_str())
                                    .selected(picked == Some(*footprint));
                                let response = ui.add(row);
                                a11y::name(&response, Role::ListItem, true, designator, None, None);
                                if response.clicked() {
                                    picked = Some(*footprint);
                                }
                            }
                        });
                    });
                crate::modal::buttons(ui, |ui| {
                    let enabled = picked.is_some();
                    let jump = widgets::button(tr!("place-choose-jump")).primary();
                    let jump = ui.add_enabled(enabled, jump).clicked();
                    if ui
                        .add(widgets::button(tr!("place-choose-cancel")))
                        .clicked()
                    {
                        close = true;
                    }
                    let to_cursor = widgets::button(tr!("place-choose-to-cursor"));
                    let to_cursor = ui.add_enabled(enabled, to_cursor).clicked();
                    if (jump || to_cursor)
                        && let Some(f) = picked
                    {
                        out = Some((
                            "pcb.place.pick_up".to_owned(),
                            json!({"footprint": f, "jump": jump}),
                        ));
                    }
                });
            });
    if response.should_close() {
        close = true;
    }
    ctx.data_mut(|d| d.insert_temp(picked_id, picked));
    if let Some(f) = placing::flight_mut(shell) {
        f.choose = (!close).then_some(filter);
    }
    out
}

fn confirm_dialog(ctx: &egui::Context, locked: usize, unroute: bool) -> Option<Invocation> {
    let mut out = None;
    let (title, body, proceed, cancel) = if unroute {
        (
            tr!("unroute-locked-title"),
            tr!("unroute-locked-body", count = locked),
            tr!("unroute-locked-proceed"),
            tr!("unroute-locked-cancel"),
        )
    } else {
        (
            tr!("place-locked-title"),
            tr!("place-locked-body", count = locked),
            tr!("place-locked-proceed"),
            tr!("place-locked-cancel"),
        )
    };
    let response = crate::modal::Dialog::new(Id::NULL.with("confirm-locked-move"), title)
        .width(320.0)
        .show(ctx, |ui| {
            ui.label(body);
            crate::modal::buttons(ui, |ui| {
                // Unrouting removes copper, so it is the destructive confirm.
                let confirm = widgets::button(proceed);
                let confirm = if unroute {
                    confirm.danger()
                } else {
                    confirm.primary()
                };
                if ui.add(confirm).clicked() {
                    out = Some(("pcb.place.confirm".to_owned(), json!({"proceed": true})));
                }
                let cancel = ui.add(widgets::button(cancel));
                if unroute {
                    crate::modal::focus_on_open(&cancel);
                }
                if cancel.clicked() {
                    out = Some(("pcb.place.confirm".to_owned(), json!({"proceed": false})));
                }
            });
        });
    if out.is_none() && response.should_close() {
        out = Some(("pcb.place.confirm".to_owned(), json!({"proceed": false})));
    }
    out
}

/// The view state, for tests and the window.
pub fn flight(shell: &Shell) -> PlaceFlight {
    placing::flight(shell)
}
