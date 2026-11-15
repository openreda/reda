//! Drawing a settings window into its tab: the page tree, the shown page and the buttons,
//! plus the field helpers pages use to flag invalid values inline.

use egui::{Button, Role, Ui, WidgetInfo};

use std::path::PathBuf;

use reda_engine::model::Uuid;

use super::fab_preset::PresetPage;
use super::layout::{self, Content, TreeRow};
use super::{Buttons, FileRequest, Problem, SettingsDialog};
use crate::shell::Shell;
use crate::tr;
use crate::widgets;

/// What a frame of the dialog asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It stays open.
    Open,
    /// It closed (OK after a successful Apply, Cancel or Close).
    Closed,
    /// A page wants a folder picked; the answer goes to [`SettingsDialog::picked_folder`].
    PickFolder,
    /// A page wants a file picked; the answer goes to [`SettingsDialog::picked_file`].
    PickFile(FileRequest),
    /// A page wants a folder shown in the OS file manager (the Output Job window's Last
    /// run).
    ShowFolder(PathBuf),
    /// A page wants a page of a Project Fab Preset copy shown in Project Settings
    /// (**Edit preset copy…**).
    EditPreset(Uuid, PresetPage),
}

/// What a page may use while it draws.
pub struct PageCx<'a> {
    /// The shell, for queries and for Commands that run at once rather than on Apply
    /// (such as saving a user preset, which is not a Project edit).
    pub shell: &'a mut Shell,
    problems: Vec<Problem>,
    pick_folder: bool,
    pick_file: Option<FileRequest>,
    asked: Option<Outcome>,
}

impl PageCx<'_> {
    /// The invalid value on `field`, if there is one.
    pub fn problem(&self, field: &str) -> Option<&Problem> {
        self.problems.iter().find(|p| p.field == field)
    }

    /// Asks the window for a folder picker.
    pub fn pick_folder(&mut self) {
        self.pick_folder = true;
    }

    /// Asks the window for a file picker.
    pub fn pick_file(&mut self, request: FileRequest) {
        self.pick_file = Some(request);
    }

    /// Asks the app to show `folder` in the OS file manager.
    pub fn show_folder(&mut self, folder: PathBuf) {
        self.asked = Some(Outcome::ShowFolder(folder));
    }

    /// Asks the app to show `page` of the Project's Fab Preset copy `preset`.
    pub fn edit_preset(&mut self, preset: Uuid, page: PresetPage) {
        self.asked = Some(Outcome::EditPreset(preset, page));
    }
}

/// A one-line text field announced as `label`, drawn invalid with `problem`'s message
/// under it when its value is invalid.
pub fn field(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    width: f32,
    problem: Option<&Problem>,
) -> egui::Response {
    ui.vertical(|ui| {
        let field = widgets::text_field(value)
            .width(width)
            .invalid(problem.is_some());
        let response = ui.add(field);
        let label = label.to_owned();
        response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
        if let Some(problem) = problem {
            problem_label(ui, problem);
        }
        response
    })
    .inner
}

/// A problem's message, in the error colour.
pub fn problem_label(ui: &mut Ui, problem: &Problem) {
    ui.add(widgets::invalid_message(problem.message.as_str()));
}

/// One frame of a settings dialog in the shared layout: what the layout draws, and what
/// the frame's buttons asked for.
struct Shown<'a> {
    dialog: &'a mut SettingsDialog,
    shell: &'a mut Shell,
    outcome: Outcome,
    apply: bool,
    ok: bool,
    reset: bool,
}

impl Content for Shown<'_> {
    /// Every page, under its group heading when it has one.
    fn tree(&mut self) -> Vec<TreeRow> {
        if !self.dialog.tree {
            return Vec::new();
        }
        let mut group: Option<String> = None;
        let mut rows = Vec::new();
        for (i, page) in self.dialog.pages.iter().enumerate() {
            let title = page.title();
            let heading = (page.group() != group).then(|| page.group()).flatten();
            group = page.group();
            let invalid = !page.problems().is_empty();
            rows.push(TreeRow {
                name: invalid.then(|| tr!("settings-page-invalid", page = title.as_str())),
                title,
                heading,
                depth: usize::from(group.is_some()),
                selected: i == self.dialog.selected,
                invalid,
                ..TreeRow::default()
            });
        }
        rows
    }

    fn choose(&mut self, index: usize) {
        self.dialog.selected = index;
    }

    fn page(&mut self, ui: &mut Ui) {
        let (shown, pressed) = self.dialog.page_content(ui, self.shell);
        if shown != Outcome::Open {
            self.outcome = shown;
        }
        self.reset = pressed;
    }

    fn notice(&mut self, ui: &mut Ui) {
        if let Some(error) = self.dialog.error() {
            layout::error(ui, tr!("error-command", error = error));
        }
    }

    fn footer(&mut self, ui: &mut Ui) {
        let dialog = &mut *self.dialog;
        if let Some(request) = dialog.pages[dialog.selected].footer(ui) {
            self.outcome = Outcome::PickFile(request);
        }
        if !dialog.can_ok() {
            layout::error(ui, tr!("settings-invalid"));
        }
    }

    /// Right to left: read left to right, the buttons are OK, Cancel, Apply (or Apply,
    /// Close).
    fn buttons(&mut self, ui: &mut Ui) {
        let dialog = &mut *self.dialog;
        let reason = (!dialog.can_ok()).then(|| tr!("settings-invalid"));
        let enabled = |ui: &mut Ui, on: bool, label: String| {
            let button = ui.add_enabled(on, Button::new(label));
            if let Some(reason) = &reason {
                button.clone().on_disabled_hover_text(reason);
            }
            button.clicked()
        };
        match dialog.buttons {
            Buttons::OkCancelApply => {
                self.apply = enabled(ui, dialog.can_apply(), tr!("settings-apply"));
                if ui.button(tr!("dialog-cancel")).clicked() {
                    self.outcome = Outcome::Closed;
                }
                self.ok = enabled(ui, dialog.can_ok(), tr!("settings-ok"));
                if dialog.run.is_some() {
                    ui.add_space(12.0);
                    if enabled(ui, dialog.can_ok(), tr!("output-job-run")) {
                        dialog.run = Some(true);
                    }
                }
            }
            Buttons::ApplyClose => {
                if ui.button(tr!("settings-close")).clicked() {
                    self.outcome = Outcome::Closed;
                }
                self.apply = enabled(ui, dialog.can_apply(), tr!("settings-apply"));
            }
        }
    }
}

impl SettingsDialog {
    /// Draws the window's content into `ui` (its tab) for one frame, in the layout every
    /// settings window shares: the page tree in the sidebar, the page, and the buttons
    /// along the bottom. Changes made elsewhere (an undo) show in the fields the user has
    /// not touched.
    pub fn show(&mut self, ui: &mut Ui, shell: &mut Shell) -> Outcome {
        let Some(project) = shell.engine().project() else {
            return Outcome::Closed;
        };
        self.sync(project);
        let mut shown = Shown {
            dialog: self,
            shell,
            outcome: Outcome::Open,
            apply: false,
            ok: false,
            reset: false,
        };
        layout::show(ui, &mut shown);
        let Shown {
            mut outcome,
            apply,
            ok,
            reset,
            ..
        } = shown;
        if reset {
            self.reset_page(shell);
        }
        // Typing into one field is one step back, once the field lets go of the keyboard.
        if !ui.ctx().text_edit_focused() {
            self.checkpoint();
        }
        if apply || ok {
            let applied = self.apply(shell).is_ok();
            if ok && applied {
                outcome = Outcome::Closed;
            }
        }
        outcome
    }

    /// Draws the shown page; returns what it asks, and whether Reset to defaults was
    /// pressed.
    fn page_content(&mut self, ui: &mut Ui, shell: &mut Shell) -> (Outcome, bool) {
        let page = &mut self.pages[self.selected];
        let mut cx = PageCx {
            shell,
            problems: page.problems(),
            pick_folder: false,
            pick_file: None,
            asked: None,
        };
        let mut reset = false;
        layout::page_header(ui, &page.title(), |ui| {
            if page.has_defaults() {
                reset = ui.add(widgets::button(page.reset_label())).clicked();
            }
        });
        page.show(ui, &mut cx);
        let outcome = match (cx.pick_folder, cx.pick_file, cx.asked) {
            (true, _, _) => Outcome::PickFolder,
            (false, Some(request), _) => Outcome::PickFile(request),
            (false, None, Some(asked)) => asked,
            (false, None, None) => Outcome::Open,
        };
        (outcome, reset)
    }
}
