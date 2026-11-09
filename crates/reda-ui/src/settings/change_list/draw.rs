//! Drawing the Change List: the toolbar, the tree grid (a header per group with a fold
//! arrow, a count and a tick-all box, then a row per change with its tick, Kind, Target,
//! From → To and Status) and the rows' context menu.

use egui::{
    Button, Checkbox, Color32, Key, RichText, Role, Sense, Stroke, TextEdit, Ui, WidgetInfo,
};
use reda_engine::sync::{GroupKind, Status};

use super::{ChangeListPage, describe, group_title};
use crate::settings::{FileRequest, PageCx};
use crate::tr;

/// What the context menu of a row asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuAction {
    Include,
    Exclude,
    IncludeKind,
    ExcludeKind,
    Invert,
    Show,
}

impl ChangeListPage {
    pub(super) fn draw(&mut self, ui: &mut Ui, cx: &mut PageCx<'_>) {
        if let Some(block) = &self.list.duplicate_nets {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr!("change-list-blocked", message = block.message.as_str()),
            );
        }
        self.toolbar(ui);
        ui.add_space(4.0);
        if self.list.count == 0 {
            ui.label(tr!("change-list-in-sync"));
        } else {
            ui.label(tr!(
                "change-list-summary",
                ticked = self.ticked_count(),
                count = self.list.count
            ));
        }
        if let Some(notice) = &self.notice {
            ui.weak(notice);
        }
        ui.add_space(4.0);
        let mut show = None;
        egui::Grid::new("change-list-grid")
            .num_columns(5)
            .spacing([12.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("");
                ui.strong(tr!("change-list-column-kind"));
                ui.strong(tr!("change-list-column-target"));
                ui.horizontal(|ui| {
                    ui.strong(tr!("change-list-column-from"));
                    arrow(ui);
                    ui.strong(tr!("change-list-column-to"));
                });
                ui.strong(tr!("change-list-column-status"));
                ui.end_row();
                for group in self.groups() {
                    self.group_header(ui, group);
                    if self.is_expanded(group) {
                        for id in self.rows_of(group) {
                            if let Some(asked) = self.row(ui, &id) {
                                show = Some(asked);
                            }
                        }
                    }
                }
            });
        let space = ui.input(|i| i.key_pressed(Key::Space));
        if space && !ui.ctx().text_edit_focused() && !self.selected.is_empty() {
            self.toggle_selected();
        }
        if let Some(id) = show {
            self.show_row(cx.shell, &id);
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let label = tr!("change-list-filter");
            let filter = ui.add(
                TextEdit::singleline(&mut self.filter)
                    .hint_text(label.clone())
                    .desired_width(180.0),
            );
            filter.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
            let mut only = self.only_problems;
            if ui
                .checkbox(&mut only, tr!("change-list-only-problems"))
                .changed()
            {
                self.set_only_problems(only);
            }
            if ui.button(tr!("change-list-expand-all")).clicked() {
                self.expand_all();
            }
            if ui.button(tr!("change-list-collapse-all")).clicked() {
                self.collapse_all();
            }
            if ui.button(tr!("change-list-include-all")).clicked() {
                self.include_all();
            }
            if ui.button(tr!("change-list-exclude-all")).clicked() {
                self.exclude_all();
            }
        });
    }

    /// A group's header row: the fold arrow and the tick-all box, then the title with its
    /// count.
    fn group_header(&mut self, ui: &mut Ui, group: GroupKind) {
        let title = group_title(group);
        let excluded = self.excluded();
        let tickable: Vec<String> = (self.rows_of(group).into_iter())
            .filter(|id| self.tickable(id))
            .collect();
        let all = tickable.iter().all(|id| !excluded.contains(id));
        let none = tickable.iter().all(|id| excluded.contains(id));
        ui.horizontal(|ui| {
            let expanded = self.is_expanded(group);
            if fold_arrow(ui, expanded, &title).clicked() {
                self.set_expanded(group, !expanded);
            }
            let mut check = all && !tickable.is_empty();
            let response = ui.add_enabled(
                !tickable.is_empty(),
                Checkbox::without_text(&mut check).indeterminate(!all && !none),
            );
            response.widget_info(|| {
                WidgetInfo::selected(
                    Role::CheckBox,
                    true,
                    all,
                    tr!("change-list-tick-group", group = title.as_str()),
                )
            });
            if response.clicked() {
                self.set_kind(group, check);
            }
        });
        ui.strong(tr!(
            "change-list-group",
            group = title.as_str(),
            count = self.group_count(group)
        ));
        ui.end_row();
    }

    /// One change's row. Returns its id when its menu asked to Show it.
    fn row(&mut self, ui: &mut Ui, id: &str) -> Option<String> {
        let cells = self.cells(id)?;
        let pending = self.change(id).cloned();
        let applied = pending.is_none();
        let change = pending
            .clone()
            .or_else(|| self.applied.iter().find(|c| c.id == id).cloned())?;
        let problem = change.is_problem();
        let mut ticked = !applied && self.is_ticked(id);
        let text = describe(&change, &self.pcb_nets);
        let weak = ui.visuals().weak_text_color();
        let mut asked = None;
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            let response =
                ui.add_enabled(!problem && !applied, Checkbox::without_text(&mut ticked));
            let label = text.clone();
            response
                .widget_info(|| WidgetInfo::selected(Role::CheckBox, true, ticked, label.clone()));
            if response.clicked() {
                self.set_ticked(id, ticked);
            }
        });
        // The Kind cell selects the row and carries its menu.
        let mut kind = RichText::new(cells.kind);
        if applied {
            kind = kind.color(weak);
        }
        let selected = self.is_selected(id);
        let response = ui.add(Button::selectable(selected, kind));
        let name = tr!(
            "change-list-row",
            row = text.as_str(),
            status = cells.status.as_str()
        );
        response.widget_info(|| WidgetInfo::selected(Role::Row, true, selected, name.clone()));
        if response.clicked() {
            let modifiers = ui.input(|i| i.modifiers);
            self.select(id, modifiers);
        }
        if response.secondary_clicked() && !selected {
            self.select(id, egui::Modifiers::NONE);
        }
        response.context_menu(|ui| {
            let can = !applied && !problem;
            let mut item = |ui: &mut Ui, on: bool, label: String, action: MenuAction| {
                if ui.add_enabled(on, Button::new(label)).clicked() {
                    asked = Some(action);
                    ui.close();
                }
            };
            item(
                ui,
                can,
                tr!("change-list-include-selected"),
                MenuAction::Include,
            );
            item(
                ui,
                can,
                tr!("change-list-exclude-selected"),
                MenuAction::Exclude,
            );
            item(
                ui,
                can,
                tr!("change-list-include-kind"),
                MenuAction::IncludeKind,
            );
            item(
                ui,
                can,
                tr!("change-list-exclude-kind"),
                MenuAction::ExcludeKind,
            );
            item(ui, can, tr!("change-list-invert"), MenuAction::Invert);
            ui.separator();
            item(ui, true, tr!("change-list-show"), MenuAction::Show);
        });
        let colour = |ui: &Ui| {
            if applied {
                Some(weak)
            } else {
                ui.visuals().override_text_color
            }
        };
        let plain = |ui: &mut Ui, text: String| {
            let mut text = RichText::new(text);
            if let Some(c) = colour(ui) {
                text = text.color(c);
            }
            ui.label(text);
        };
        plain(ui, cells.target);
        plain(ui, cells.from_to);
        ui.horizontal(|ui| {
            if applied {
                check_mark(ui, weak);
            }
            let colour = match change.status {
                _ if applied => weak,
                Status::Ok => ui.visuals().text_color(),
                Status::Note => ui.visuals().warn_fg_color,
                Status::Problem => ui.visuals().error_fg_color,
            };
            ui.label(RichText::new(cells.status).color(colour));
        });
        ui.end_row();
        match asked {
            Some(MenuAction::Include) => self.include_selected(),
            Some(MenuAction::Exclude) => self.exclude_selected(),
            Some(MenuAction::IncludeKind) => self.set_kind_of_selected(true),
            Some(MenuAction::ExcludeKind) => self.set_kind_of_selected(false),
            Some(MenuAction::Invert) => self.invert_selected(),
            Some(MenuAction::Show) => return Some(id.to_owned()),
            None => {}
        }
        None
    }

    /// The footer's Export button, left of Apply and Close.
    pub(super) fn draw_footer(&mut self, ui: &mut Ui) -> Option<FileRequest> {
        ui.button(tr!("change-list-export"))
            .clicked()
            .then_some(FileRequest::ExportChangeList)
    }
}

/// A group's fold arrow, painted (the UI font has no triangle glyphs).
fn fold_arrow(ui: &mut Ui, expanded: bool, title: &str) -> egui::Response {
    let size = egui::vec2(14.0, ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let label = if expanded {
        tr!("change-list-collapse-group", group = title)
    } else {
        tr!("change-list-expand-group", group = title)
    };
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    let colour = ui.style().interact(&response).fg_stroke.color;
    let c = rect.center();
    let points = if expanded {
        vec![
            c + egui::vec2(-4.0, -2.0),
            c + egui::vec2(4.0, -2.0),
            c + egui::vec2(0.0, 3.0),
        ]
    } else {
        vec![
            c + egui::vec2(-2.0, -4.0),
            c + egui::vec2(3.0, 0.0),
            c + egui::vec2(-2.0, 4.0),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(points, colour, Stroke::NONE));
    response
}

/// The From → To header's arrow, painted (the UI font has no arrow glyph).
fn arrow(ui: &mut Ui) {
    let colour = ui.visuals().strong_text_color();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 12.0), Sense::hover());
    let y = rect.center().y;
    let (l, r) = (rect.left() + 1.0, rect.right() - 1.0);
    let stroke = Stroke::new(1.2, colour);
    ui.painter()
        .line_segment([egui::pos2(l, y), egui::pos2(r, y)], stroke);
    ui.painter().line(
        vec![
            egui::pos2(r - 4.0, y - 4.0),
            egui::pos2(r, y),
            egui::pos2(r - 4.0, y + 4.0),
        ],
        stroke,
    );
}

/// The Applied tick, painted.
fn check_mark(ui: &mut Ui, colour: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
    let at = |x: f32, y: f32| rect.left_top() + egui::vec2(x, y);
    ui.painter().line(
        vec![at(2.0, 6.5), at(5.0, 9.5), at(10.5, 2.5)],
        Stroke::new(1.5, colour),
    );
}
