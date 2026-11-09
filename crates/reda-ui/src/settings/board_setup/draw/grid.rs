//! The Net Classes page: the grid (a bold Default row, then the classes in rank order,
//! dragged to re-rank) and the Net table.

use egui::{
    Button, Checkbox, Event, Key, Modifiers, Rect, RichText, Role, Sense, Stroke, TextEdit, Ui,
    WidgetInfo, vec2,
};
use reda_engine::model::Uuid;

use super::super::model::{
    BoardSetupDraft, ClassKey, GRID_FIELDS, GRID_KINDS, RuleField, class_field, kind_name,
};
use super::{LENGTH_WIDTH, no_pcb};
use crate::settings::{PageCx, field};
use crate::tr;

const CELL_WIDTH: f32 = 64.0;
const HANDLE: f32 = 18.0;

/// The Net Classes page: the grid, then the Net table.
pub fn net_classes_page(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    if no_pcb(ui, board) {
        return;
    }
    toolbar(ui, board);
    ui.add_space(4.0);
    class_grid(ui, cx, board);
    grid_keys(ui, board);
    ui.weak(tr!("board-setup-grid-note"));
    ui.add_space(12.0);
    net_table(ui, board);
}

fn toolbar(ui: &mut Ui, board: &mut BoardSetupDraft) {
    ui.horizontal_wrapped(|ui| {
        let label = tr!("board-setup-grid-search");
        ui.label(&label);
        field(ui, &label, &mut board.grid_search, 160.0, None);
        ui.checkbox(&mut board.only_overrides, tr!("board-setup-only-overrides"));
        let any = !board.selected_rows.is_empty();
        if ui
            .add_enabled(any, Button::new(tr!("board-setup-copy")))
            .clicked()
        {
            ui.ctx().copy_text(board.copy_rows());
        }
        // Pasting from the button needs the clipboard's text, which egui hands over only
        // as a paste event; the button asks for one.
        if ui
            .add_enabled(any, Button::new(tr!("board-setup-paste")))
            .clicked()
        {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
        if ui
            .add_enabled(any, Button::new(tr!("board-setup-clear")))
            .clicked()
        {
            board.clear_rows();
        }
        if ui.button(tr!("board-setup-add-class")).clicked() {
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
    let sense = if draggable {
        Sense::click_and_drag()
    } else {
        Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(HANDLE, 20.0), sense);
    let visuals = ui.style().interact_selectable(&response, selected);
    if selected {
        ui.painter()
            .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
    }
    if draggable {
        // Three grip lines, painted: the UI font lacks a grip glyph.
        let stroke = Stroke::new(1.5, visuals.fg_stroke.color);
        for dy in [-4.0, 0.0, 4.0] {
            let y = rect.center().y + dy;
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left() + 4.0, y),
                    egui::pos2(rect.right() - 4.0, y),
                ],
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
    let response = ui
        .scope(|ui| {
            if problem.is_some() {
                let error = ui.visuals().error_fg_color;
                let widgets = &mut ui.visuals_mut().widgets;
                for w in [&mut widgets.inactive, &mut widgets.hovered] {
                    w.bg_stroke = Stroke::new(1.5, error);
                }
            }
            ui.add(
                TextEdit::singleline(value)
                    .hint_text(hint)
                    .desired_width(CELL_WIDTH),
            )
        })
        .inner;
    let label = label.to_owned();
    response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
    let changed = response.changed();
    // The message is too wide for a cell: it shows on hover, and the cell is red.
    if let Some(problem) = problem {
        response.on_hover_text(&problem.message);
    }
    changed
}

/// A grid column's short heading under its kind's name: "Min", "Hole max".
fn column_name(f: RuleField) -> String {
    use RuleField as F;
    match f {
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
    let dragging: Option<ClassKey> = ui.data(|d| d.get_temp(drag_id));
    let mut row_rects: Vec<(ClassKey, Rect)> = Vec::new();
    let mut clicked: Option<(ClassKey, Modifiers)> = None;
    let mut drag_start: Option<ClassKey> = None;
    let mut drag_end = false;
    let mut delete = None;
    egui::Grid::new("board-setup-class-grid")
        .striped(true)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            // Two header rows: each kind's name over its group, then the columns.
            ui.label("");
            ui.label("");
            for kind in GRID_KINDS {
                ui.strong(kind_name(kind));
                for _ in GRID_FIELDS.iter().filter(|f| f.rule().0 == kind) {
                    ui.label("");
                }
                ui.label("");
            }
            ui.end_row();
            ui.label("");
            ui.strong(tr!("board-setup-net-class"));
            for kind in GRID_KINDS {
                ui.label(RichText::new(tr!("board-setup-on")).small());
                for f in GRID_FIELDS.iter().filter(|f| f.rule().0 == kind) {
                    ui.label(RichText::new(column_name(*f)).small());
                }
                ui.label(RichText::new(tr!("board-setup-comment")).small());
            }
            ui.label("");
            ui.end_row();
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
                if default {
                    ui.label(RichText::new(&name).strong());
                } else {
                    let label = tr!("board-setup-class-name", number = number(key));
                    let problem = cx.problem(&class_field(key, "name"));
                    if let Some(value) = board.class_name_mut(key) {
                        field(ui, &label, value, 120.0, problem);
                    }
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
                        let r = ui.add(Checkbox::without_text(&mut on));
                        r.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, &label));
                        if r.changed() {
                            board.set_enabled(key, kind, on);
                        }
                    } else {
                        ui.label("");
                    }
                    for &f in GRID_FIELDS.iter().filter(|f| f.rule().0 == kind) {
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
                        if cell(ui, &label, &mut text, &hint, problem) {
                            board.type_cell(key, f, &text);
                        }
                    }
                    if board.has_record(key, kind) {
                        let label = tr!(
                            "board-setup-comment-of",
                            class = name.as_str(),
                            rule = rule.as_str()
                        );
                        if let Some(comment) = board.comment_mut(key, kind) {
                            field(ui, &label, comment, 90.0, None);
                        }
                    } else {
                        ui.label("");
                    }
                }
                if default {
                    let b = ui.add_enabled(false, Button::new(tr!("board-setup-delete")));
                    b.on_disabled_hover_text(tr!("board-setup-default-class"));
                } else {
                    let title = tr!("board-setup-delete-class", number = number(key));
                    let b = ui.button(tr!("board-setup-delete"));
                    b.widget_info(|| WidgetInfo::labeled(Role::Button, true, &title));
                    if b.clicked() {
                        delete = Some(key);
                    }
                }
                ui.end_row();
            }
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
                ui.painter()
                    .hline(x, y, Stroke::new(2.0, ui.visuals().selection.stroke.color));
            }
            if drag_end {
                board.move_class(key, index);
            }
        }
        if drag_end {
            ui.data_mut(|d| d.remove::<ClassKey>(drag_id));
        }
    }
    if let Some(key) = delete {
        board.delete_class(key);
        board.selected_rows.remove(&key);
    }
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
    ui.strong(tr!("board-setup-nets"));
    ui.horizontal_wrapped(|ui| {
        let label = tr!("board-setup-filter");
        ui.label(&label);
        field(ui, &label, &mut board.net_filter, 200.0, None);
        let any = !board.selected_nets.is_empty();
        if ui
            .add_enabled(
                any,
                Button::new(tr!("board-setup-new-class-from-selection")),
            )
            .clicked()
        {
            board.new_class_from_selection();
        }
        if ui.button(tr!("board-setup-assign-pattern")).clicked() {
            let first = board.live_classes().next().map(|c| c.key());
            board.pattern_form = Some((String::new(), first));
        }
    });
    pattern_form(ui, board);
    if board.nets().is_empty() {
        ui.weak(tr!("board-setup-no-nets"));
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
    egui::Grid::new("board-setup-nets")
        .num_columns(2)
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for (uuid, name, member, current) in nets {
                let selected = board.selected_nets.contains(&uuid);
                if ui.selectable_label(selected, &name).clicked() {
                    clicked = Some((uuid, ui.input(|i| i.modifiers)));
                }
                let label = tr!("board-setup-class-of", net = name.as_str());
                let combo = egui::ComboBox::from_id_salt(("board-setup-net", uuid))
                    .selected_text(current)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(member.is_empty(), &default_name)
                            .clicked()
                        {
                            toggles.push((uuid, ClassKey::Default));
                        }
                        for (key, class_name) in &classes {
                            let mut on = member.contains(key);
                            if ui.checkbox(&mut on, class_name).changed() {
                                toggles.push((uuid, *key));
                            }
                        }
                    });
                combo
                    .response
                    .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
                ui.end_row();
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
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            let label = tr!("board-setup-pattern");
            ui.label(&label);
            field(ui, &label, &mut pattern, LENGTH_WIDTH * 1.5, None);
            let label = tr!("board-setup-pattern-class");
            ui.label(&label);
            let shown = class
                .and_then(|k| classes.iter().find(|(c, _)| *c == k))
                .map_or(String::new(), |(_, n)| n.clone());
            let combo = egui::ComboBox::from_id_salt("board-setup-pattern-class")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (key, name) in &classes {
                        ui.selectable_value(&mut class, Some(*key), name);
                    }
                });
            combo
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
            let ready = class.is_some() && !pattern.trim().is_empty();
            assign = ui
                .add_enabled(ready, Button::new(tr!("board-setup-assign")))
                .clicked();
            cancel = ui.button(tr!("dialog-cancel")).clicked();
        });
        ui.weak(tr!("board-setup-pattern-note"));
    });
    board.pattern_form = Some((pattern.clone(), class));
    if assign && let Some(key) = class {
        board.assign_by_pattern(&pattern, key);
        board.pattern_form = None;
    } else if cancel {
        board.pattern_form = None;
    }
}
