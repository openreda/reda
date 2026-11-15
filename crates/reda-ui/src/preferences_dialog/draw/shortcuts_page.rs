//! The Keyboard Shortcuts page of Preferences: every Command and Preset with its keys,
//! the search, the modifier of each mouse gesture role, recording a key with its context
//! and conflicts, Remove, Reset and Open folder.

use egui::{Key, Role, Ui, WidgetInfo};

use super::{Cx, Outcome, fixed_label, label_width};
use crate::bars::DocumentType;
use crate::keymap::{Chord, Context, Held};
use crate::preferences_dialog::shortcuts::{self, Row};
use crate::preferences_dialog::{PreferencesDialog, Shortcuts};
use crate::settings::layout;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Tooltip as _};

/// The width of the page's search box.
const SEARCH_WIDTH: f32 = 260.0;
/// The width of a dropdown on the page.
const DROPDOWN_WIDTH: f32 = 160.0;

impl PreferencesDialog {
    /// Every Command and Preset with its keys: search, recording, conflicts, Remove,
    /// Reset and Open folder.
    pub(super) fn keyboard_shortcuts(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        ui.horizontal(|ui| {
            let search = ui.add(
                widgets::text_field(&mut self.shortcuts.query)
                    .search()
                    .hint(tr!("shortcuts-search-hint"))
                    .width(SEARCH_WIDTH),
            );
            search.widget_info(|| {
                WidgetInfo::labeled(Role::SearchInput, true, tr!("shortcuts-search"))
            });
            if let Some(folder) = Shortcuts::folder(cx.shell)
                && ui
                    .add(widgets::link(tr!("shortcuts-open-folder")))
                    .clicked()
            {
                cx.outcome = Outcome::ShowFolder(folder);
            }
        });
        for problem in Shortcuts::problems(cx.shell) {
            layout::error(ui, problem);
        }
        if let Some(error) = self.shortcuts.error() {
            layout::error(ui, error.to_owned());
        }
        let query = self.shortcuts.query.clone();
        let rows = self.shortcuts.matching(cx.shell, &query);
        let roles = Shortcuts::modifiers(cx.shell);
        let names: Vec<String> = roles.iter().map(|(role, ..)| role_name(*role)).collect();
        let titles = rows.iter().map(|row| row.title.as_str());
        let width = label_width(ui, titles.chain(names.iter().map(String::as_str)));
        self.modifier_roles(ui, cx, width);
        self.recording(ui, cx);
        ui.add_space(ui.tokens().space.section);
        if rows.is_empty() {
            layout::muted(ui, tr!("pref-search-none"));
        }
        for row in rows {
            self.shortcut_row(ui, cx, &row, width);
        }
    }

    /// The Modifiers section: the held modifier of each mouse gesture role.
    fn modifier_roles(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, width: f32) {
        layout::group(ui, &tr!("shortcuts-modifiers"));
        let mut change = None;
        for (role, held, changed) in Shortcuts::modifiers(cx.shell) {
            ui.horizontal(|ui| {
                let name = role_name(role);
                fixed_label(ui, width, &name, changed);
                let combo = widgets::dropdown(("modifier-role", role.name()), held.name())
                    .width(DROPDOWN_WIDTH)
                    .show(ui, |ui| {
                        for option in Held::ALL {
                            let row = widgets::choice(option == held, option.name());
                            if ui.add(row).clicked() {
                                change = Some((role, option));
                            }
                        }
                    });
                combo
                    .response
                    .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, name.clone()));
            });
        }
        if let Some((role, held)) = change {
            let _ = self.shortcuts.set_modifier(cx.shell, role, held);
        }
    }

    fn shortcut_row(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, row: &Row, label: f32) {
        let mac = cfg!(target_os = "macos");
        ui.horizontal_top(|ui| {
            let changed = Shortcuts::is_changed(cx.shell, &row.binding);
            fixed_label(ui, label, &row.title, changed).tooltip(row.id.as_str());
            // A Command bound in many contexts wraps its keys rather than widening the
            // dialog.
            let width = ui.available_width();
            ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut remove = None;
                    for (context, chord) in Shortcuts::keys(cx.shell, &row.binding) {
                        let text = match &context {
                            Context::Global => chord.label(mac),
                            other => format!("{} ({other})", chord.label(mac)),
                        };
                        let chip = ui.add(widgets::button(format!("{text}  ×")));
                        let name = tr!(
                            "shortcuts-remove-named",
                            key = text.as_str(),
                            command = row.title.as_str()
                        );
                        chip.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                        if chip.clicked() {
                            remove = Some((context, chord));
                        }
                    }
                    if let Some((context, chord)) = remove {
                        self.shortcuts.remove_key(cx.shell, &context, &chord);
                    }
                    let add = ui.add(widgets::button(tr!("shortcuts-add")).flat());
                    let name = tr!("shortcuts-add-named", command = row.title.as_str());
                    add.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                    if add.clicked() {
                        self.shortcuts
                            .start_recording_in(cx.shell, row.binding.clone());
                    }
                    let reset = widgets::button(tr!("pref-reset")).flat();
                    let reset = ui.add_enabled(changed, reset);
                    let name = tr!("pref-reset-named", setting = row.title.as_str());
                    reset.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                    if reset.clicked() {
                        self.shortcuts.reset_binding(cx.shell, &row.binding);
                    }
                });
            });
        });
    }

    /// The key being recorded: waits for a key (Escape cancels), then shows it with its
    /// context, any conflict, and Assign or Cancel.
    fn recording(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        let Some(recording) = self.shortcuts.recording.clone() else {
            return;
        };
        let mac = cfg!(target_os = "macos");
        if recording.chord.is_none() {
            let pressed = ui.ctx().input_mut(|i| {
                let mut pressed = None;
                i.events.retain(|event| {
                    if pressed.is_some() {
                        return true;
                    }
                    if let egui::Event::Key {
                        key: Key::Escape,
                        pressed: true,
                        modifiers,
                        ..
                    } = event
                        && modifiers.is_none()
                    {
                        pressed = Some(None);
                        return false;
                    }
                    match Chord::from_event(event).first() {
                        Some(chord) => {
                            pressed = Some(Some(*chord));
                            false
                        }
                        None => true,
                    }
                });
                pressed
            });
            match pressed {
                Some(None) => {
                    self.shortcuts.cancel();
                    return;
                }
                Some(Some(chord)) => self.shortcuts.record(chord),
                None => {}
            }
        }
        let title = shortcuts::title(cx.shell, &recording.binding);
        let t = ui.tokens();
        widgets::card_frame(&t).show(ui, |ui| {
            match &self.shortcuts.recording.as_ref().and_then(|r| r.chord) {
                None => layout::text(ui, tr!("shortcuts-press-key", command = title.as_str())),
                Some(chord) => layout::text(
                    ui,
                    tr!(
                        "shortcuts-recorded",
                        key = chord.label(mac),
                        command = title.as_str()
                    ),
                ),
            };
            ui.horizontal(|ui| {
                layout::text(ui, tr!("shortcuts-context"));
                let current = recording.context.clone();
                let mut picked = None;
                let combo = widgets::dropdown("shortcuts-context", context_label(&current))
                    .width(DROPDOWN_WIDTH)
                    .show(ui, |ui| {
                        for context in contexts(&current) {
                            let row = widgets::choice(context == current, context_label(&context));
                            if ui.add(row).clicked() {
                                picked = Some(context);
                            }
                        }
                    })
                    .response;
                combo.widget_info(|| {
                    WidgetInfo::labeled(Role::ComboBox, true, tr!("shortcuts-context"))
                });
                if let Some(context) = picked {
                    self.shortcuts.set_context(context);
                }
            });
            if let Some(conflict) = self.shortcuts.conflict(cx.shell) {
                layout::warning(ui, conflict);
            }
            ui.horizontal(|ui| {
                let has_key = self
                    .shortcuts
                    .recording
                    .as_ref()
                    .is_some_and(|r| r.chord.is_some());
                if ui
                    .add_enabled(has_key, widgets::button(tr!("shortcuts-assign")).primary())
                    .clicked()
                {
                    let _ = self.shortcuts.confirm(cx.shell);
                }
                if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                    self.shortcuts.cancel();
                }
            });
        });
    }
}

/// The contexts a key can be recorded in: global, each Document type, and a Tool context
/// the binding already uses.
fn contexts(current: &Context) -> Vec<Context> {
    let mut out = vec![Context::Global];
    out.extend(DocumentType::ALL.iter().map(|&t| Context::Document(t)));
    out.push(Context::Part);
    if !out.contains(current) {
        out.push(current.clone());
    }
    out
}

fn context_label(context: &Context) -> String {
    match context {
        Context::Global => tr!("shortcuts-context-global"),
        other => other.to_string(),
    }
}

/// The name of a mouse gesture role, from the catalogue.
fn role_name(role: crate::keymap::Role) -> String {
    let key = format!("shortcuts-role-{}", role.name().replace('_', "-"));
    crate::i18n::translate(&key, &[])
}
