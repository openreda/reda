//! Drawing a settings window into its tab: the page tree, the shown page and the buttons,
//! plus the field helpers pages use to flag invalid values inline.

use egui::{Button, RichText, Role, Stroke, TextEdit, Ui, WidgetInfo};

use super::{Buttons, FileRequest, Problem, SettingsDialog};
use crate::shell::Shell;
use crate::tr;

/// The width of the page tree.
const TREE_WIDTH: f32 = 150.0;

/// What a frame of the dialog asks of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It stays open.
    Open,
    /// It closed (OK after a successful Apply, Cancel or Close).
    Closed,
    /// A page wants a folder picked; the answer goes to [`SettingsDialog::picked_folder`].
    PickFolder,
    /// A page wants a file picked; the answer goes to [`SettingsDialog::picked_file`].
    PickFile(FileRequest),
}

/// What a page may use while it draws.
pub struct PageCx<'a> {
    /// The shell, for queries and for Commands that run at once rather than on Apply
    /// (such as saving a user preset, which is not a Project edit).
    pub shell: &'a mut Shell,
    problems: Vec<Problem>,
    pick_folder: bool,
    pick_file: Option<FileRequest>,
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
}

/// A one-line text field announced as `label`, outlined in red with `problem`'s message
/// under it when its value is invalid.
pub fn field(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    width: f32,
    problem: Option<&Problem>,
) -> egui::Response {
    ui.vertical(|ui| {
        let response = ui
            .scope(|ui| {
                if problem.is_some() {
                    let error = ui.visuals().error_fg_color;
                    let widgets = &mut ui.visuals_mut().widgets;
                    for w in [&mut widgets.inactive, &mut widgets.hovered] {
                        w.bg_stroke = Stroke::new(1.5, error);
                    }
                    ui.visuals_mut().selection.stroke = Stroke::new(1.5, error);
                }
                ui.add(TextEdit::singleline(value).desired_width(width))
            })
            .inner;
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
    ui.label(
        RichText::new(&problem.message)
            .color(ui.visuals().error_fg_color)
            .small(),
    );
}

impl SettingsDialog {
    /// Draws the window's content into `ui` (its tab) for one frame: the page tree on the
    /// left, the page, and the buttons along the bottom. Changes made elsewhere (an undo)
    /// show in the fields the user has not touched.
    pub fn show(&mut self, ui: &mut Ui, shell: &mut Shell) -> Outcome {
        let Some(project) = shell.engine().project() else {
            return Outcome::Closed;
        };
        self.sync(project);
        let mut outcome = Outcome::Open;
        let (mut apply, mut ok, mut reset) = (false, false, false);
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                if !self.can_ok() {
                    ui.colored_label(ui.visuals().error_fg_color, tr!("settings-invalid"));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Right to left: read left to right, the buttons are OK, Cancel, Apply
                    // (or Apply, Close).
                    let reason = (!self.can_ok()).then(|| tr!("settings-invalid"));
                    let enabled = |ui: &mut Ui, on: bool, label: String| {
                        let button = ui.add_enabled(on, Button::new(label));
                        if let Some(reason) = &reason {
                            button.clone().on_disabled_hover_text(reason);
                        }
                        button.clicked()
                    };
                    match self.buttons {
                        Buttons::OkCancelApply => {
                            apply = enabled(ui, self.can_apply(), tr!("settings-apply"));
                            if ui.button(tr!("dialog-cancel")).clicked() {
                                outcome = Outcome::Closed;
                            }
                            ok = enabled(ui, self.can_ok(), tr!("settings-ok"));
                        }
                        Buttons::ApplyClose => {
                            if ui.button(tr!("settings-close")).clicked() {
                                outcome = Outcome::Closed;
                            }
                            apply = enabled(ui, self.can_apply(), tr!("settings-apply"));
                        }
                    }
                });
            });
            if let Some(error) = self.error() {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    tr!("error-command", error = error),
                );
            }
            ui.separator();
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.horizontal_top(|ui| {
                    if self.tree {
                        egui::ScrollArea::vertical()
                            .id_salt("settings-tree")
                            .max_width(TREE_WIDTH)
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.tree(ui));
                        ui.separator();
                    }
                    ui.vertical(|ui| {
                        egui::ScrollArea::both()
                            .id_salt("settings-page")
                            .auto_shrink(false)
                            .show(ui, |ui| {
                                let (shown, pressed) = self.page_content(ui, shell);
                                if shown != Outcome::Open {
                                    outcome = shown;
                                }
                                reset = pressed;
                            });
                    });
                });
            });
        });
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

    /// The page tree: every page, under its group heading when it has one.
    fn tree(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.set_width(TREE_WIDTH);
            let mut group: Option<String> = None;
            for i in 0..self.pages.len() {
                let page = &self.pages[i];
                let title = page.title();
                if page.group() != group {
                    group = page.group();
                    if let Some(heading) = &group {
                        ui.weak(heading);
                    }
                }
                let invalid = !page.problems().is_empty();
                let mut text = RichText::new(title.clone());
                if invalid {
                    text = text.color(ui.visuals().error_fg_color);
                }
                let row = ui.horizontal(|ui| {
                    if group.is_some() {
                        ui.add_space(12.0);
                    }
                    ui.add(tree_row(ui, i == self.selected, text))
                });
                let row = row.inner;
                let label = if invalid {
                    tr!("settings-page-invalid", page = title.as_str())
                } else {
                    title
                };
                row.widget_info(|| WidgetInfo::labeled(Role::TreeItem, true, label.clone()));
                if row.clicked() {
                    self.selected = i;
                }
            }
        });
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
        };
        let mut reset = false;
        ui.horizontal(|ui| {
            ui.strong(page.title());
            if page.has_defaults() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    reset = ui.button(tr!("settings-reset-defaults")).clicked();
                });
            }
        });
        ui.add_space(6.0);
        page.show(ui, &mut cx);
        let outcome = match (cx.pick_folder, cx.pick_file) {
            (true, _) => Outcome::PickFolder,
            (false, Some(request)) => Outcome::PickFile(request),
            (false, None) => Outcome::Open,
        };
        (outcome, reset)
    }
}

/// A row of a page tree: selectable, as wide as the tree, its text on the left.
pub fn tree_row(ui: &Ui, selected: bool, text: RichText) -> Button<'static> {
    let height = ui.spacing().interact_size.y;
    Button::selectable(selected, text)
        .right_text("")
        .min_size(egui::vec2(ui.available_width(), height))
}
