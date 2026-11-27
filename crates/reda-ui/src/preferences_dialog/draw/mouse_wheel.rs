//! The **Mouse Wheel** page (docs/spec/v0.3/input-and-defaults.md#wheel-mapping): a preset
//! dropdown, the table binding each wheel action to a modifier combination, then zoom
//! speed, reverse zoom and the touchpad mode as ordinary setting rows.
//!
//! - A combination another action holds is refused, and the row says which action holds
//!   it; nothing is written.
//! - Choosing a preset rewrites the table through `app.preferences.set`, one clash-free
//!   key at a time ([`Wheel::steps_to`]). The preset shown is read off the table, Custom
//!   when it matches none; it is never stored.

use egui::{Role, Ui, WidgetInfo};
use reda_engine::preferences::{Wheel, WheelAction, WheelBinding, WheelPreset};
use serde_json::json;

use super::{Cx, DOT_WIDTH, EDITOR_WIDTH, fixed_label, label_width};
use crate::preferences_dialog::{PageId, PreferencesDialog, Setting, schema};
use crate::settings::layout;
use crate::settings::{Problem, problem_label};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets;

/// A binding as the table shows it: `Plain wheel`, `Ctrl+Shift`, `Unbound`.
pub(crate) fn binding_label(binding: WheelBinding) -> String {
    match binding {
        WheelBinding::Unbound => tr!("pref-wheel-unbound"),
        WheelBinding::With(m) if !m.ctrl && !m.alt && !m.shift => tr!("pref-wheel-plain"),
        WheelBinding::With(m) => [(m.ctrl, "Ctrl"), (m.alt, "Alt"), (m.shift, "Shift")]
            .into_iter()
            .filter_map(|(on, name)| on.then_some(name))
            .collect::<Vec<_>>()
            .join("+"),
    }
}

/// A dropdown named `label` showing `value`, as assistive technology reads it.
fn combo_info(label: &str, value: &str) -> WidgetInfo {
    let mut info = WidgetInfo::labeled(Role::ComboBox, true, label);
    info.current_text_value = Some(value.to_owned());
    info
}

/// An action's name, as its row's label gives it.
fn action_label(action: WheelAction) -> String {
    schema::setting(action.key()).label
}

/// Why `binding` cannot go to `action` in `wheel`: the action that holds it.
fn clash(wheel: &Wheel, action: WheelAction, binding: WheelBinding) -> Option<String> {
    wheel.held_by(action, binding).map(|holder| {
        tr!(
            "pref-wheel-clash",
            combination = binding_label(binding),
            action = action_label(holder)
        )
    })
}

impl PreferencesDialog {
    /// The page: the preset, the action table and the other wheel settings.
    pub(super) fn mouse_wheel(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        let rows = self.rows(cx.shell, PageId::MOUSE_WHEEL);
        let preset = tr!("pref-wheel-preset");
        let width = label_width(
            ui,
            rows.iter()
                .map(|s| s.label.as_str())
                .chain([preset.as_str()]),
        );
        if !self.modified_only {
            self.wheel_preset(ui, cx, &preset, width);
        }
        if rows.is_empty() {
            layout::muted(ui, tr!("pref-none-modified"));
            return;
        }
        let (actions, others): (Vec<&Setting>, Vec<&Setting>) = rows
            .iter()
            .partition(|s| WheelAction::of_key(&s.key).is_some());
        if !actions.is_empty() {
            layout::group(ui, &tr!("pref-wheel-actions"));
        }
        for setting in actions {
            let action = WheelAction::of_key(&setting.key).expect("an action row");
            self.row(ui, cx, setting, width, |dialog, ui, cx| {
                dialog.wheel_binding(ui, cx, action);
            });
            if let Some(problem) = self.problem(&setting.key) {
                let problem = Problem::new(setting.key.as_str(), problem);
                ui.horizontal(|ui| {
                    ui.add_space(DOT_WIDTH + ui.tokens().space.gap);
                    problem_label(ui, &problem);
                });
            }
        }
        if !others.is_empty() {
            layout::group(ui, &tr!("pref-wheel-zoom-touchpad"));
        }
        for setting in others {
            self.setting_row(ui, cx, setting, width);
        }
    }

    /// The preset dropdown: REDA or Wheel scrolls, showing Custom for an edited table.
    fn wheel_preset(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, label: &str, width: f32) {
        let wheel = cx.shell.preferences().canvas.wheel.clone();
        let now = wheel.preset();
        let name = |p: WheelPreset| match p {
            WheelPreset::Reda => tr!("pref-wheel-preset-reda"),
            WheelPreset::WheelScrolls => tr!("pref-wheel-preset-wheel-scrolls"),
        };
        let shown = now.map_or_else(|| tr!("pref-wheel-preset-custom"), name);
        let mut picked = None;
        let t = ui.tokens();
        ui.add_space((t.space.stack - t.space.row_y).max(0.0));
        ui.horizontal(|ui| {
            // The reset dot's column: a preset is never stored, so it has none.
            ui.allocate_space(egui::vec2(DOT_WIDTH, t.control_height()));
            fixed_label(ui, width, label, false);
            let response = widgets::dropdown("pref-wheel-preset", shown.clone())
                .width(t.size.of(EDITOR_WIDTH))
                .show(ui, |ui| {
                    for preset in WheelPreset::ALL {
                        if ui
                            .add(widgets::choice(now == Some(preset), name(preset)))
                            .clicked()
                        {
                            picked = Some(preset);
                        }
                    }
                })
                .response;
            response.widget_info(|| combo_info(label, &shown));
        });
        if let Some(preset) = picked.filter(|p| Some(*p) != now) {
            for (action, binding) in wheel.steps_to(&preset.table()) {
                if self
                    .set(cx.shell, action.key(), json!(binding.to_string()))
                    .is_err()
                {
                    break;
                }
            }
        }
    }

    /// An action's combination dropdown, refusing one another action holds.
    fn wheel_binding(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, action: WheelAction) {
        let key = action.key();
        let wheel = cx.shell.preferences().canvas.wheel.clone();
        let now = wheel.binding(action);
        let label = action_label(action);
        let mut picked = None;
        let t = ui.tokens();
        let shown = binding_label(now);
        let response = widgets::dropdown(("pref", key), shown.clone())
            .width(t.size.of(EDITOR_WIDTH))
            .show(ui, |ui| {
                for binding in WheelBinding::all() {
                    if ui
                        .add(widgets::choice(binding == now, binding_label(binding)))
                        .clicked()
                    {
                        picked = Some(binding);
                    }
                }
            })
            .response;
        response.widget_info(|| combo_info(&label, &shown));
        layout::muted(
            ui,
            if action.pcb_only() {
                tr!("pref-wheel-editors-pcb")
            } else {
                tr!("pref-wheel-editors-all")
            },
        );
        let Some(binding) = picked.filter(|b| *b != now) else {
            return;
        };
        match clash(&wheel, action, binding) {
            Some(why) => {
                self.problems.insert(key.to_owned(), why);
            }
            None => {
                let _ = self.set(cx.shell, key, json!(binding.to_string()));
            }
        }
    }

    /// Whether resetting `key`, a wheel action's, is refused because another action holds
    /// its default; the row then says which.
    pub(super) fn wheel_reset_refused(&mut self, cx: &Cx<'_>, key: &str) -> bool {
        let Some(action) = WheelAction::of_key(key) else {
            return false;
        };
        let wheel = &cx.shell.preferences().canvas.wheel;
        let default = Wheel::default().binding(action);
        match clash(wheel, action, default) {
            Some(why) => {
                self.problems.insert(key.to_owned(), why);
                true
            }
            None => false,
        }
    }
}
