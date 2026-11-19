//! The Net Classes page: the grid (a bold Default row, then the classes in rank order,
//! dragged to re-rank) and the Net table.

use egui::{Event, Key, Modifiers, Rect, Role, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};
use reda_engine::model::Uuid;

use super::super::model::{
    BoardSetupDraft, ClassKey, GRID_FIELDS, GRID_KINDS, RuleField, class_field, kind_name,
};
use super::{LENGTH_WIDTH, no_pcb};

/// The pattern field of Assign by pattern.
const PATTERN_WIDTH: f32 = LENGTH_WIDTH * 1.5;
use crate::settings::{PageCx, field, layout, strong};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Column, RowLook, Selection, Tooltip as _};

/// A value cell of the grid: room for a value such as `0.125` and the field's padding.
const CELL_WIDTH: f32 = 80.0;
/// The grip at the start of a row.
const HANDLE: f32 = 18.0;
/// How far apart the grip's three lines are.
const GRIP_GAP: f32 = 4.0;
/// The search field over the grid.
const SEARCH_WIDTH: f32 = 160.0;
/// A class's name in the grid.
const NAME_WIDTH: f32 = 120.0;
/// A class's comment on one rule kind.
const COMMENT_WIDTH: f32 = 90.0;
/// The Net table's filter.
const FILTER_WIDTH: f32 = 200.0;
/// A Net's Net Classes dropdown.
const CLASSES_WIDTH: f32 = 180.0;

/// The Net Classes page: the grid, then the Net table.
pub fn net_classes_page(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    if no_pcb(ui, board) {
        return;
    }
    let t = ui.tokens();
    toolbar(ui, board);
    ui.add_space(t.space.stack);
    class_grid(ui, cx, board);
    grid_keys(ui, board);
    allowed_types(ui, cx, board);
    layout::muted(ui, tr!("board-setup-grid-note"));
    ui.add_space(t.space.section);
    net_table(ui, board);
}

fn allowed_types(ui: &mut Ui, cx: &PageCx<'_>, board: &mut BoardSetupDraft) {
    let Some(pcb) = cx.shell.engine().project().and_then(|p| p.pcb()) else {
        return;
    };
    if pcb.stackup().via_types.len() < 2 {
        return;
    }
    let selected: Vec<_> = board
        .selected_rows
        .iter()
        .copied()
        .filter(|k| *k != ClassKey::Default)
        .collect();
    for key in selected {
        ui.push_id(("via-permissions", key), |ui| {
            strong(ui, tr!("via-allowed", class = board.class_name(key)));
            ui.horizontal_wrapped(|ui| {
                for kind in &pcb.stackup().via_types {
                    let allowed = board.allowed_types(key);
                    let mut on = allowed.is_empty() || allowed.contains(&kind.uuid);
                    if ui
                        .add(widgets::checkbox(
                            &mut on,
                            tr!("via-allow", name = crate::pcb::via_type_name(pcb, kind)),
                        ))
                        .changed()
                    {
                        let mut list = if allowed.is_empty() {
                            pcb.stackup().via_types.iter().map(|v| v.uuid).collect()
                        } else {
                            allowed.to_vec()
                        };
                        if on {
                            list.push(kind.uuid);
                        } else {
                            list.retain(|id| *id != kind.uuid);
                        }
                        if list.len() == pcb.stackup().via_types.len() {
                            list.clear();
                        }
                        board.set_allowed_types(key, list);
                    }
                }
            });
        });
    }
}

fn toolbar(ui: &mut Ui, board: &mut BoardSetupDraft) {
    ui.horizontal_wrapped(|ui| {
        let label = tr!("board-setup-grid-search");
        layout::text(ui, &label);
        field(ui, &label, &mut board.grid_search, SEARCH_WIDTH, None);
        let only = tr!("board-setup-only-overrides");
        ui.add(widgets::checkbox(&mut board.only_overrides, only));
        let any = !board.selected_rows.is_empty();
        if ui
            .add_enabled(any, widgets::button(tr!("board-setup-copy")))
            .clicked()
        {
            ui.ctx().copy_text(board.copy_rows());
        }
        // Pasting from the button needs the clipboard's text, which egui hands over only
        // as a paste event; the button asks for one.
        if ui
            .add_enabled(any, widgets::button(tr!("board-setup-paste")))
            .clicked()
        {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
        if ui
            .add_enabled(any, widgets::button(tr!("board-setup-clear")))
            .clicked()
        {
            board.clear_rows();
        }
        if ui
            .add(widgets::button(tr!("board-setup-add-class")))
            .clicked()
        {
            board.add_class();
        }
    });
}

/// Copy, paste and Delete on the selected rows while no text field has the keyboard.
fn grid_keys(ui: &mut Ui, board: &mut BoardSetupDraft) {
    if board.selected_rows.is_empty() || ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let events = ui.input(|i| i.events.clone());
    for event in events {
        match event {
            Event::Copy => ui.ctx().copy_text(board.copy_rows()),
            Event::Paste(text) => board.paste_rows(&text),
            Event::Key {
                key: Key::Delete,
                pressed: true,
                ..
            } => board.clear_rows(),
            _ => {}
        }
    }
}

/// A row's grip: click selects it (Ctrl adds, Shift extends), dragging a class re-ranks
/// it.
fn handle(ui: &mut Ui, selected: bool, label: &str, draggable: bool) -> egui::Response {
    let t = ui.tokens();
    let sense = if draggable {
        Sense::click_and_drag()
    } else {
        Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(HANDLE, t.control_height()), sense);
    let grip = rect.shrink2(vec2(0.0, (rect.height() - HANDLE) / 2.0));
    if selected {
        ui.painter()
            .rect_filled(grip, t.radius.mark, t.color.accent.subtle);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(grip, t.radius.mark, t.color.fill.hover);
    }
    if draggable {
        // Three grip lines, painted: the Glyph set has no grip.
        let ink = if response.hovered() || selected {
            t.color.text.primary
        } else {
            t.color.text.muted
        };
        let stroke = Stroke::new(widgets::glyph::STROKE, ink);
        let inset = GRIP_GAP;
        for dy in [-GRIP_GAP, 0.0, GRIP_GAP] {
            let y = rect.center().y + dy;
            ui.painter().line_segment(
                [pos2(rect.left() + inset, y), pos2(rect.right() - inset, y)],
                stroke,
            );
        }
    }
    let label = label.to_owned();
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, &label));
    if draggable && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    response
}

/// A value cell: grey with what it inherits when empty, red when invalid.
fn cell(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    problem: Option<&crate::settings::Problem>,
) -> bool {
    let response = ui.add(
        widgets::text_field(value)
            .hint(hint)
            .width(CELL_WIDTH)
            .invalid(problem.is_some()),
    );
    let label = label.to_owned();
    response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
    let changed = response.changed();
    // The message is too wide for a cell: it shows on hover, and the cell is red.
    if let Some(problem) = problem {
        response.tooltip(&problem.message);
    }
    changed
}

/// A grid column's short heading under its kind's name: "Min", "Hole max".
fn column_name(f: RuleField) -> String {
    use RuleField as F;
    match f {
        F::ViaHoleClearance | F::PadHoleClearance => f.name(),
        F::TrackWidth => tr!("board-setup-col-min"),
        F::TrackPreferred => tr!("board-setup-col-preferred"),
        F::TrackMax => tr!("board-setup-col-max"),
        F::ViaDiameter => tr!("board-setup-col-diameter-min"),
        F::ViaDiameterPreferred => tr!("board-setup-col-diameter-preferred"),
        F::ViaDiameterMax => tr!("board-setup-col-diameter-max"),
        F::ViaHole => tr!("board-setup-col-hole-min"),
        F::ViaHolePreferred => tr!("board-setup-col-hole-preferred"),
        F::ViaHoleMax => tr!("board-setup-col-hole-max"),
        _ => tr!("board-setup-col-value"),
    }
}

fn class_grid(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    let rows = board.grid_rows();
    let numbers: Vec<(ClassKey, usize)> = board
        .live_classes()
        .enumerate()
        .map(|(i, c)| (c.key(), i + 1))
        .collect();
    let number = |key: ClassKey| numbers.iter().find(|(k, _)| *k == key).map_or(0, |n| n.1);
    let drag_id = ui.scope_id().with("board-setup-drag");
    let context = ui.ctx().clone();
    let dragging: Option<ClassKey> = ui.data(|d| d.get_temp(drag_id));
    let mut row_rects: Vec<(ClassKey, Rect)> = Vec::new();
    let mut clicked: Option<(ClassKey, Modifiers)> = None;
    let mut drag_start: Option<ClassKey> = None;
    let mut drag_end = false;
    let mut delete_key = None;
    let fields = |kind| {
        GRID_FIELDS
            .iter()
            .filter(move |f: &&RuleField| f.rule().0 == kind)
    };
    let count = 4 + GRID_KINDS
        .iter()
        .map(|&kind| 2 + fields(kind).count())
        .sum::<usize>();
    let columns = std::iter::once(Column::fixed(HANDLE))
        .chain(std::iter::repeat_n(Column::auto(), count - 1));
    // The grid is wider than the page: it scrolls sideways on its own, so the notes
    // under it wrap at the page's width.
    egui::ScrollArea::horizontal()
        .id_salt("board-setup-class-grid-scroll")
        .show(ui, |ui| {
            widgets::table("board-setup-class-grid")
                .columns(columns)
                .show(ui, |table| {
                    // Two header rows: each kind's name over its group, then the columns.
                    table.row(|row| {
                        row.empty();
                        row.empty();
                        for kind in GRID_KINDS {
                            row.ui(|ui| group_heading(ui, &kind_name(kind)));
                            for _ in fields(kind) {
                                row.empty();
                            }
                            row.empty();
                        }
                        row.empty();
                        row.empty();
                    });
                    table.header(|h| {
                        h.empty();
                        h.text(tr!("board-setup-net-class"));
                        for kind in GRID_KINDS {
                            h.text(tr!("board-setup-on"));
                            for f in fields(kind) {
                                h.text(column_name(*f));
                            }
                            h.text(tr!("board-setup-comment"));
                        }
                        h.empty();
                        h.empty();
                    });
                    for key in rows {
                        let default = key == ClassKey::Default;
                        let name = if default {
                            tr!("board-setup-default-class-name")
                        } else {
                            board.class_name(key).to_owned()
                        };
                        let selected = board.selected_rows.contains(&key);
                        let label = if default {
                            tr!("board-setup-select-row", class = name.as_str())
                        } else {
                            tr!("board-setup-drag-row", class = name.as_str())
                        };
                        let open_key = drag_id.with(("class-layer-rules", key));
                        let mut expanded = context
                            .data(|d| d.get_temp::<bool>(open_key))
                            .unwrap_or(false);
                        table.row(|row| {
                            row.ui(|ui| {
                                let grip = handle(ui, selected, &label, !default);
                                row_rects.push((key, grip.rect));
                                if grip.clicked() {
                                    clicked = Some((key, ui.input(|i| i.modifiers)));
                                }
                                if grip.drag_started() {
                                    drag_start = Some(key);
                                }
                                if grip.drag_stopped() {
                                    drag_end = true;
                                }
                            });
                            if default {
                                row.ui(|ui| strong(ui, &name));
                            } else {
                                let label = tr!("board-setup-class-name", number = number(key));
                                let problem = cx.problem(&class_field(key, "name"));
                                row.ui(|ui| {
                                    ui.horizontal(|ui| {
                                        if let Some(value) = board.class_name_mut(key) {
                                            field(ui, &label, value, NAME_WIDTH, problem);
                                        }
                                        if matches!(key, ClassKey::Existing(_))
                                            && ui
                                                .add(widgets::button(tr!("board-rule-layers")))
                                                .clicked()
                                        {
                                            expanded = !expanded;
                                            ui.data_mut(|d| d.insert_temp(open_key, expanded));
                                        }
                                    });
                                });
                            }
                            for kind in GRID_KINDS {
                                let rule = kind_name(kind);
                                if board.has_record(key, kind) {
                                    let mut on = board.enabled(key, kind);
                                    let label = tr!(
                                        "board-setup-enabled",
                                        class = name.as_str(),
                                        rule = rule.as_str()
                                    );
                                    let r = row.ui(|ui| ui.add(widgets::checkbox(&mut on, "")));
                                    r.widget_info(|| {
                                        WidgetInfo::selected(Role::CheckBox, true, on, &label)
                                    });
                                    if r.changed() {
                                        board.set_enabled(key, kind, on);
                                    }
                                } else {
                                    row.empty();
                                }
                                for &f in fields(kind) {
                                    if default && !f.applies_board_wide() {
                                        row.empty();
                                        continue;
                                    }
                                    let label = tr!(
                                        "board-setup-override",
                                        class = name.as_str(),
                                        rule = f.name()
                                    );
                                    let problem_field = if default {
                                        f.problem_field()
                                    } else {
                                        class_field(key, &f.id())
                                    };
                                    let problem = cx.problem(&problem_field);
                                    let hint = board.inherited(key, f);
                                    let mut text = board.cell(key, f).to_owned();
                                    if row.ui(|ui| cell(ui, &label, &mut text, &hint, problem)) {
                                        board.type_cell(key, f, &text);
                                    }
                                }
                                if board.has_record(key, kind) {
                                    let label = tr!(
                                        "board-setup-comment-of",
                                        class = name.as_str(),
                                        rule = rule.as_str()
                                    );
                                    row.ui(|ui| {
                                        if let Some(comment) = board.comment_mut(key, kind) {
                                            field(ui, &label, comment, COMMENT_WIDTH, None);
                                        }
                                    });
                                } else {
                                    row.empty();
                                }
                            }
                            row.ui(|ui| {
                                let delete = widgets::button(tr!("board-setup-delete"));
                                if default {
                                    let b = ui.add_enabled(false, delete);
                                    b.disabled_tooltip(tr!("board-setup-default-class"));
                                } else {
                                    let title =
                                        tr!("board-setup-delete-class", number = number(key));
                                    let b = ui.add(delete);
                                    b.widget_info(|| {
                                        WidgetInfo::labeled(Role::Button, true, &title)
                                    });
                                    if b.clicked() {
                                        delete_key = Some(key);
                                    }
                                }
                            });
                            let shown = board.shown_row(key);
                            row.ui(|ui| super::show_button(ui, cx, &name, shown));
                        });
                        if expanded
                            && let ClassKey::Existing(class) = key
                            && let Some(pcb) = cx.shell.engine().project().and_then(|p| p.pcb())
                        {
                            table.row(|row| {
                                row.empty();
                                row.ui(|ui| {
                                    super::scoped::class_section(ui, pcb, board, class);
                                });
                            });
                        }
                    }
                });
        });
    if let Some((key, modifiers)) = clicked {
        select_row(board, key, modifiers);
    }
    if let Some(key) = drag_start {
        ui.data_mut(|d| d.insert_temp(drag_id, key));
    }
    if let Some(key) = dragging.or(drag_start) {
        let classes: Vec<(ClassKey, Rect)> = row_rects
            .iter()
            .copied()
            .filter(|(k, _)| *k != ClassKey::Default && *k != key)
            .collect();
        if let Some(pointer) = ui.ctx().pointer_latest_pos() {
            let index = classes
                .iter()
                .filter(|(_, r)| r.center().y < pointer.y)
                .count();
            // Where it will land: a line above the row it goes before.
            let y = classes
                .get(index)
                .map(|(_, r)| r.top() - 2.0)
                .or_else(|| classes.last().map(|(_, r)| r.bottom() + 2.0));
            if let Some(y) = y {
                let x = ui.min_rect().x_range();
                let t = ui.tokens();
                let line = Stroke::new(t.stroke.accent_line, t.color.accent.text);
                ui.painter().hline(x, y, line);
            }
            if drag_end {
                board.move_class(key, index);
            }
        }
        if drag_end {
            ui.data_mut(|d| d.remove::<ClassKey>(drag_id));
        }
    }
    if let Some(key) = delete_key {
        board.delete_class(key);
        board.selected_rows.remove(&key);
    }
}

/// A rule kind's name over its group of columns. It takes no width, so the column under
/// it stays as narrow as its check box, and the name runs on over the group.
fn group_heading(ui: &mut Ui, text: &str) {
    let t = ui.tokens();
    let ink = t.color.text.primary;
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), t.ty.body_strong.font_id(), ink);
    let (rect, _) = ui.allocate_exact_size(vec2(0.0, galley.size().y), Sense::hover());
    let at = pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at.round(), galley, ink);
}

/// Click selects one row, Ctrl+click adds or removes one, Shift+click selects a range.
fn select_row(board: &mut BoardSetupDraft, key: ClassKey, modifiers: Modifiers) {
    if modifiers.command {
        if !board.selected_rows.remove(&key) {
            board.selected_rows.insert(key);
        }
    } else if modifiers.shift
        && let Some(&anchor) = board.selected_rows.iter().next()
    {
        let rows = board.grid_rows();
        let (a, b) = (
            rows.iter().position(|k| *k == anchor),
            rows.iter().position(|k| *k == key),
        );
        if let (Some(a), Some(b)) = (a, b) {
            board.selected_rows = rows[a.min(b)..=a.max(b)].iter().copied().collect();
        }
    } else {
        board.selected_rows = [key].into();
    }
}

/// The Net table: Net → Net Classes, filterable, with multi-select, **New Class** from
/// the selection and **Assign by pattern…**.
fn net_table(ui: &mut Ui, board: &mut BoardSetupDraft) {
    strong(ui, tr!("board-setup-nets"));
    ui.horizontal_wrapped(|ui| {
        let label = tr!("board-setup-filter");
        layout::text(ui, &label);
        field(ui, &label, &mut board.net_filter, FILTER_WIDTH, None);
        let any = !board.selected_nets.is_empty();
        let new_class = widgets::button(tr!("board-setup-new-class-from-selection"));
        if ui.add_enabled(any, new_class).clicked() {
            board.new_class_from_selection();
        }
        if ui
            .add(widgets::button(tr!("board-setup-assign-pattern")))
            .clicked()
        {
            let first = board.live_classes().next().map(|c| c.key());
            board.pattern_form = Some((String::new(), first));
        }
    });
    pattern_form(ui, board);
    if board.nets().is_empty() {
        layout::muted(ui, tr!("board-setup-no-nets"));
        return;
    }
    let classes: Vec<(ClassKey, String)> = board
        .live_classes()
        .map(|c| (c.key(), c.name().to_owned()))
        .collect();
    let default_name = tr!("board-setup-default-class-name");
    let nets: Vec<(Uuid, String, Vec<ClassKey>, String)> = board
        .filtered_nets()
        .map(|n| {
            let names: Vec<&str> = classes
                .iter()
                .filter(|(k, _)| n.classes().contains(k))
                .map(|(_, name)| name.as_str())
                .collect();
            let shown = if names.is_empty() {
                default_name.clone()
            } else {
                names.join(", ")
            };
            (
                n.uuid,
                n.name.clone(),
                n.classes().iter().copied().collect(),
                shown,
            )
        })
        .collect();
    let order: Vec<Uuid> = nets.iter().map(|n| n.0).collect();
    let mut toggles = Vec::new();
    let mut clicked = None;
    let focused = ui.input(|i| i.focused);
    let modifiers = ui.input(|i| i.modifiers);
    widgets::table("board-setup-nets")
        .columns([Column::auto(), Column::auto()])
        .show(ui, |table| {
            for (uuid, name, member, current) in nets {
                let selected = board.selected_nets.contains(&uuid);
                let selection = match (selected, focused) {
                    (false, _) => Selection::None,
                    (true, true) => Selection::Live,
                    (true, false) => Selection::Unfocused,
                };
                let label = tr!("board-setup-class-of", net = name.as_str());
                let row = table.row_with(RowLook::selected(selection), |row| {
                    row.text(&name);
                    row.ui(|ui| {
                        let combo = widgets::dropdown(("board-setup-net", uuid), current)
                            .width(CLASSES_WIDTH)
                            .show(ui, |ui| {
                                let none = widgets::choice(member.is_empty(), &default_name);
                                if ui.add(none).clicked() {
                                    toggles.push((uuid, ClassKey::Default));
                                }
                                for (key, class_name) in &classes {
                                    let on = member.contains(key);
                                    let item = widgets::choice(on, class_name);
                                    if ui.add(item).clicked() {
                                        toggles.push((uuid, *key));
                                    }
                                }
                            });
                        combo
                            .response
                            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
                    });
                });
                if row.clicked() {
                    clicked = Some((uuid, modifiers));
                }
            }
        });
    for (net, key) in toggles {
        board.toggle_net_class(net, key);
    }
    if let Some((net, modifiers)) = clicked {
        if modifiers.command {
            if !board.selected_nets.remove(&net) {
                board.selected_nets.insert(net);
            }
        } else if modifiers.shift
            && let Some(anchor) = order.iter().position(|n| board.selected_nets.contains(n))
            && let Some(at) = order.iter().position(|n| *n == net)
        {
            board.selected_nets = order[anchor.min(at)..=anchor.max(at)]
                .iter()
                .copied()
                .collect();
        } else {
            board.selected_nets = [net].into();
        }
    }
}

/// "Assign by pattern…": a glob and a class; Assign adds the matching Nets to it.
fn pattern_form(ui: &mut Ui, board: &mut BoardSetupDraft) {
    let Some((mut pattern, mut class)) = board.pattern_form.clone() else {
        return;
    };
    let classes: Vec<(ClassKey, String)> = board
        .live_classes()
        .map(|c| (c.key(), c.name().to_owned()))
        .collect();
    let (mut assign, mut cancel) = (false, false);
    let t = ui.tokens();
    widgets::card_frame(&t).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            let label = tr!("board-setup-pattern");
            layout::text(ui, &label);
            field(ui, &label, &mut pattern, PATTERN_WIDTH, None);
            let label = tr!("board-setup-pattern-class");
            layout::text(ui, &label);
            let shown = class
                .and_then(|k| classes.iter().find(|(c, _)| *c == k))
                .map_or(String::new(), |(_, n)| n.clone());
            let combo = widgets::dropdown("board-setup-pattern-class", shown).show(ui, |ui| {
                for (key, name) in &classes {
                    widgets::choice_value(ui, &mut class, Some(*key), name);
                }
            });
            combo
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
            let ready = class.is_some() && !pattern.trim().is_empty();
            assign = ui
                .add_enabled(ready, widgets::button(tr!("board-setup-assign")))
                .clicked();
            cancel = ui.add(widgets::button(tr!("dialog-cancel"))).clicked();
        });
        layout::muted(ui, tr!("board-setup-pattern-note"));
    });
    board.pattern_form = Some((pattern.clone(), class));
    if assign && let Some(key) = class {
        board.assign_by_pattern(&pattern, key);
        board.pattern_form = None;
    } else if cancel {
        board.pattern_form = None;
    }
}
