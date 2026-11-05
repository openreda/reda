//! Drawing a settings dialog: the modal, the page tree, the shown page and the buttons,
//! plus the field helpers pages use to flag invalid values inline.

use egui::{Button, Id, Key, Modal, RichText, Role, Stroke, TextEdit, Ui, WidgetInfo};

use super::{Problem, SettingsDialog};
use crate::shell::Shell;
use crate::tr;

/// The dialog's size; pages scroll inside it.
const SIZE: egui::Vec2 = egui::vec2(760.0, 460.0);
/// The width of the page tree.
const TREE_WIDTH: f32 = 150.0;

/// What a frame of the dialog asks of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It stays open.
    Open,
    /// It closed (OK after a successful Apply, Cancel or Escape).
    Closed,
    /// A page wants a folder picked; the answer goes to [`SettingsDialog::picked_folder`].
    PickFolder,
}

/// What a page may use while it draws.
pub struct PageCx<'a> {
    /// The shell, for queries and for Commands that run at once rather than on Apply
    /// (such as saving a user preset, which is not a Project edit).
    pub shell: &'a mut Shell,
    problems: Vec<Problem>,
    pick_folder: bool,
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
    /// Draws the dialog as a modal for one frame. Changes made elsewhere (an undo) show in
    /// the fields the user has not touched.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        let mut outcome = Outcome::Open;
        Modal::new(Id::NULL.with("settings-dialog")).show(ctx, |ui| {
            ui.set_width(SIZE.x);
            outcome = self.body(ui, shell, SIZE.y);
        });
        if ctx.input(|i| i.key_pressed(Key::Escape)) && !ctx.egui_wants_keyboard_input() {
            outcome = Outcome::Closed;
        }
        outcome
    }

    /// PROTOTYPE, for the screenshots.
    pub fn select_page(&mut self, title: &str) {
        if let Some(i) = self.pages.iter().position(|p| p.title() == title) {
            self.selected = i;
        }
    }

    /// PROTOTYPE: the dialog's contents, on a modal or in a tab.
    pub fn body(&mut self, ui: &mut Ui, shell: &mut Shell, height: f32) -> Outcome {
        let Some(project) = shell.engine().project() else {
            return Outcome::Closed;
        };
        self.sync(project);
        let mut outcome = Outcome::Open;
        let mut apply = false;
        let mut ok = false;
        ui.heading(self.title.clone());
        ui.separator();
        ui.horizontal_top(|ui| {
            ui.set_height(height);
            self.tree(ui);
            ui.separator();
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("settings-page")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        outcome = self.page_content(ui, shell);
                    });
            });
        });
        ui.separator();
        if let Some(error) = self.error() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr!("error-command", error = error),
            );
        }
        ui.horizontal(|ui| {
            if self.title == tr!("board-setup-title") {
                crate::proto_shell::show_on_board_button(ui);
            }
            if !self.can_ok() {
                ui.colored_label(ui.visuals().error_fg_color, tr!("settings-invalid"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Right to left: Apply, Cancel, OK read left to right as OK, Cancel, Apply.
                let reason = (!self.can_ok()).then(|| tr!("settings-invalid"));
                let apply_button =
                    ui.add_enabled(self.can_apply(), Button::new(tr!("settings-apply")));
                if let Some(reason) = &reason {
                    apply_button.clone().on_disabled_hover_text(reason);
                }
                apply = apply_button.clicked();
                if ui.button(tr!("dialog-cancel")).clicked() {
                    outcome = Outcome::Closed;
                }
                let ok_button = ui.add_enabled(self.can_ok(), Button::new(tr!("settings-ok")));
                if let Some(reason) = &reason {
                    ok_button.clone().on_disabled_hover_text(reason);
                }
                ok = ok_button.clicked();
            });
        });
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
                    crate::proto_shell::tree_row(ui, i == self.selected, text)
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

    fn page_content(&mut self, ui: &mut Ui, shell: &mut Shell) -> Outcome {
        let page = &mut self.pages[self.selected];
        let mut cx = PageCx {
            shell,
            problems: page.problems(),
            pick_folder: false,
        };
        ui.strong(page.title());
        ui.add_space(6.0);
        page.show(ui, &mut cx);
        if cx.pick_folder {
            Outcome::PickFolder
        } else {
            Outcome::Open
        }
    }
}
