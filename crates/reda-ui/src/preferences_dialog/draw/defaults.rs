//! Drawing the **Defaults** page (docs/spec/v0.3/input-and-defaults.md#the-defaults-page):
//! Remember last used, the filter with Reset all, Save all As… and Load…, then the kinds
//! by editor beside the chosen kind's fields (each with its reset dot) and its own Reset
//! and Save As…. The model is `preferences_dialog/defaults_page.rs`.

use egui::{Role, Ui, WidgetInfo};

use super::{Cx, Outcome, label_width};
use crate::preferences_dialog::{PreferencesDialog, defaults_fields, schema};
use crate::settings::layout;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets;

/// The width of the kinds list.
const LIST_WIDTH: f32 = 190.0;
/// The width of the filter box.
const FILTER_WIDTH: f32 = 220.0;

/// The one Preference of the page that is not a record.
const REMEMBER: &str = "defaults.remember_last_used";

impl PreferencesDialog {
    pub(super) fn defaults_page(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        let remember = schema::setting(REMEMBER);
        if !self.modified_only || self.can_reset(cx.shell, REMEMBER) {
            let width = label_width(ui, [remember.label.as_str()].into_iter());
            self.setting_row(ui, cx, &remember, width);
        }
        let t = ui.tokens();
        ui.add_space(t.space.stack);
        ui.horizontal(|ui| {
            let filter = ui.add(
                widgets::text_field(&mut self.defaults.filter)
                    .search()
                    .hint(tr!("defaults-filter-hint"))
                    .width(FILTER_WIDTH),
            );
            filter.widget_info(|| {
                WidgetInfo::labeled(Role::SearchInput, true, tr!("defaults-filter"))
            });
            if ui.add(widgets::button(tr!("defaults-reset-all"))).clicked() {
                self.reset_defaults(cx.shell, None);
            }
            if ui.add(widgets::button(tr!("defaults-save-all"))).clicked() {
                cx.outcome = Outcome::PickFile(self.save_defaults_as(None));
            }
            if ui.add(widgets::button(tr!("defaults-load"))).clicked() {
                cx.outcome = Outcome::PickFile(crate::settings::FileRequest::LoadDefaults);
            }
        });
        if let Some(message) = &self.defaults.message {
            layout::muted(ui, message.clone());
        }
        ui.add_space(t.space.stack);
        let groups = self.defaults_groups(cx.shell);
        if groups.is_empty() {
            layout::muted(ui, tr!("defaults-none-found"));
            return;
        }
        let shown = self.defaults_kind(cx.shell);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(LIST_WIDTH);
                for group in &groups {
                    layout::group(ui, &group.title);
                    for kind in &group.kinds {
                        let selected = shown.is_some_and(|s| s.id == kind.id);
                        let title = if kind.modified > 0 {
                            tr!(
                                "defaults-kind-modified",
                                kind = kind.title.as_str(),
                                count = kind.modified
                            )
                        } else {
                            kind.title.clone()
                        };
                        if ui.add(widgets::choice(selected, title)).clicked() {
                            self.choose_defaults_kind(kind.id);
                        }
                    }
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                let Some(kind) = shown else {
                    return;
                };
                let title = super::super::defaults_page::kind_title(kind.id);
                layout::page_header(ui, &title, |ui| {
                    if ui.add(widgets::button(tr!("defaults-save-kind"))).clicked() {
                        cx.outcome = Outcome::PickFile(self.save_defaults_as(Some(kind.id)));
                    }
                    if ui
                        .add(widgets::button(tr!("defaults-reset-kind")))
                        .clicked()
                    {
                        self.reset_defaults(cx.shell, Some(kind.id));
                    }
                });
                let rows: Vec<_> = defaults_fields(kind)
                    .into_iter()
                    .filter(|s| !self.modified_only || self.can_reset(cx.shell, &s.key))
                    .collect();
                if rows.is_empty() {
                    layout::muted(ui, tr!("pref-none-modified"));
                }
                self.setting_rows(ui, cx, &rows);
            });
        });
    }
}
