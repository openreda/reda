//! Drawing the Change List: the toolbar, the tree grid (a header per group with a fold
//! chevron, a count and a tick-all box, then a row per change with its tick, Kind, Target,
//! From → To and Status) and the rows' context menu.

use egui::{Key, Role, Ui, WidgetInfo};
use reda_engine::sync::{GroupKind, Status};

use super::{ChangeListPage, describe, group_title};
use crate::settings::{FileRequest, PageCx, layout, strong};
use crate::theme::{Tokens, TokensExt as _};
use crate::tr;
use crate::widgets::{self, Column, Glyph, RowLook, Selection, TableBody};

/// The filter field in the toolbar.
const FILTER_WIDTH: f32 = 180.0;

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
        let t = ui.tokens();
        if self.stale {
            layout::warning(ui, tr!("change-list-stale"));
        }
        if let Some(block) = &self.list.duplicate_nets {
            let message = block.message.as_str();
            layout::error(ui, tr!("change-list-blocked", message = message));
        }
        self.toolbar(ui);
        ui.add_space(t.space.stack);
        if self.list.count == 0 {
            layout::text(ui, tr!("change-list-in-sync"));
        } else {
            layout::text(
                ui,
                tr!(
                    "change-list-summary",
                    ticked = self.ticked_count(),
                    count = self.list.count
                ),
            );
        }
        if let Some(notice) = &self.notice {
            layout::muted(ui, notice);
        }
        ui.add_space(t.space.stack);
        let mut show = None;
        let focused = ui.input(|i| i.focused);
        let modifiers = ui.input(|i| i.modifiers);
        widgets::table("change-list-grid")
            .columns([Column::auto(); 5])
            .show(ui, |table| {
                table.header(|h| {
                    h.empty();
                    h.text(tr!("change-list-column-kind"));
                    h.text(tr!("change-list-column-target"));
                    h.ui(|ui| {
                        let ink = t.color.text.muted;
                        ui.label(t.ty.small.rich(tr!("change-list-column-from")).color(ink));
                        ui.add(widgets::glyph(Glyph::Arrow).color(ink));
                        ui.label(t.ty.small.rich(tr!("change-list-column-to")).color(ink));
                    });
                    h.text(tr!("change-list-column-status"));
                });
                for group in self.groups() {
                    self.group_header(table, group);
                    if self.is_expanded(group) {
                        for id in self.rows_of(group) {
                            if let Some(asked) = self.row(table, &id, focused, modifiers, &t) {
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
                widgets::text_field(&mut self.filter)
                    .hint(label.clone())
                    .width(FILTER_WIDTH),
            );
            filter.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
            let mut only = self.only_problems;
            let only_problems = widgets::checkbox(&mut only, tr!("change-list-only-problems"));
            if ui.add(only_problems).changed() {
                self.set_only_problems(only);
            }
            let button = |ui: &mut Ui, label: String| ui.add(widgets::button(label)).clicked();
            if button(ui, tr!("change-list-expand-all")) {
                self.expand_all();
            }
            if button(ui, tr!("change-list-collapse-all")) {
                self.collapse_all();
            }
            if button(ui, tr!("change-list-include-all")) {
                self.include_all();
            }
            if button(ui, tr!("change-list-exclude-all")) {
                self.exclude_all();
            }
        });
    }

    /// A group's header row: the fold chevron and the tick-all box, then the title with
    /// its count.
    fn group_header(&mut self, table: &mut TableBody<'_>, group: GroupKind) {
        let title = group_title(group);
        let excluded = self.excluded();
        let tickable: Vec<String> = (self.rows_of(group).into_iter())
            .filter(|id| self.tickable(id))
            .collect();
        let all = tickable.iter().all(|id| !excluded.contains(id));
        let none = tickable.iter().all(|id| excluded.contains(id));
        let expanded = self.is_expanded(group);
        let count = self.group_count(group);
        let (mut fold, mut tick) = (false, None);
        table.row(|row| {
            row.ui(|ui| {
                if fold_button(ui, expanded, &title).clicked() {
                    fold = true;
                }
                let mut check = all && !tickable.is_empty();
                let response = ui.add_enabled(
                    !tickable.is_empty(),
                    widgets::checkbox(&mut check, "").indeterminate(!all && !none),
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
                    tick = Some(check);
                }
            });
            row.ui(|ui| {
                strong(
                    ui,
                    tr!("change-list-group", group = title.as_str(), count = count),
                )
            });
            row.empty();
            row.empty();
            row.empty();
        });
        if fold {
            self.set_expanded(group, !expanded);
        }
        if let Some(check) = tick {
            self.set_kind(group, check);
        }
    }

    /// One change's row. Returns its id when its menu asked to Show it.
    fn row(
        &mut self,
        table: &mut TableBody<'_>,
        id: &str,
        focused: bool,
        modifiers: egui::Modifiers,
        t: &Tokens,
    ) -> Option<String> {
        let cells = self.cells(id)?;
        let pending = self.change(id).cloned();
        let applied = pending.is_none();
        let change = pending
            .clone()
            .or_else(|| self.applied.iter().find(|c| c.id == id).cloned())?;
        let problem = change.is_problem();
        let mut ticked = !applied && self.is_ticked(id);
        let text = describe(&change, &self.pcb_nets);
        let selected = self.is_selected(id);
        let selection = match (selected, focused) {
            (false, _) => Selection::None,
            (true, true) => Selection::Live,
            (true, false) => Selection::Unfocused,
        };
        let mut tick = false;
        let response = table.row_with(RowLook::selected(selection), |row| {
            row.ui(|ui| {
                ui.add_space(widgets::TREE_INDENT);
                let response =
                    ui.add_enabled(!problem && !applied, widgets::checkbox(&mut ticked, ""));
                let label = text.clone();
                response.widget_info(|| {
                    WidgetInfo::selected(Role::CheckBox, true, ticked, label.clone())
                });
                tick = response.clicked();
            });
            let plain = |row: &mut widgets::TableRow<'_>, text: String| {
                if applied {
                    row.muted(text);
                } else {
                    row.text(text);
                }
            };
            plain(row, cells.kind.clone());
            plain(row, cells.target.clone());
            plain(row, cells.from_to.clone());
            let ink = match change.status {
                _ if applied => t.color.text.muted,
                Status::Ok => t.color.text.primary,
                Status::Note => t.color.warning.text,
                Status::Problem => t.color.error.text,
            };
            row.ui(|ui| {
                if applied {
                    ui.add(widgets::glyph(Glyph::Tick).color(ink));
                }
                ui.label(t.ty.body.rich(cells.status.clone()).color(ink));
            });
        });
        let name = tr!(
            "change-list-row",
            row = text.as_str(),
            status = cells.status.as_str()
        );
        response.widget_info(|| WidgetInfo::selected(Role::Row, true, selected, name.clone()));
        if tick {
            self.set_ticked(id, ticked);
        }
        if response.clicked() {
            self.select(id, modifiers);
        }
        if response.secondary_clicked() && !selected {
            self.select(id, egui::Modifiers::NONE);
        }
        let mut asked = None;
        widgets::context_menu(&response, |ui| {
            let can = !applied && !problem;
            let mut item = |ui: &mut Ui, on: bool, label: String, action: MenuAction| {
                if ui.add_enabled(on, widgets::menu_item(label)).clicked() {
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
            widgets::menu_separator(ui);
            item(ui, true, tr!("change-list-show"), MenuAction::Show);
        });
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
        ui.add(widgets::button(tr!("change-list-export")))
            .clicked()
            .then_some(FileRequest::ExportChangeList)
    }
}

/// A group's fold chevron, announced as "Collapse <group>" or "Expand <group>".
fn fold_button(ui: &mut Ui, expanded: bool, title: &str) -> egui::Response {
    let (glyph, label) = if expanded {
        (
            Glyph::ChevronDown,
            tr!("change-list-collapse-group", group = title),
        )
    } else {
        (
            Glyph::ChevronRight,
            tr!("change-list-expand-group", group = title),
        )
    };
    let response = ui.add(widgets::glyph_button(glyph));
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    response
}
