//! The settings windows: non-modal windows of their own from the docking layer, with a
//! page tree on the left and OK, Cancel and Apply. Project Settings is the first
//! ([`project`]); Board Setup, the Output Job window and the Change List are built the
//! same way, and [`windows`] holds the open ones.
//!
//! Each page keeps a draft of the settings it edits ([`Draft`]) and turns the difference
//! from the Project into Command steps. Apply runs every page's steps as one `batch`, so
//! one Journal entry and one undo entry per touched file; OK applies and closes; Cancel
//! drops the drafts. A page reports invalid values as [`Problem`]s, which are flagged
//! beside their field and disable OK and Apply.
//!
//! The model here has no drawing, so the dialogs can be tested without a window; a
//! window's tab is drawn by [`SettingsDialog::show`] (in `settings/draw.rs`).

use std::any::Any;
use std::path::Path;

use reda_engine::command::CommandError;
use reda_engine::model::{Project, Scope};
use serde_json::{Value, json};

use crate::shell::Shell;

pub mod board_setup;
pub mod change_list;
mod draw;
pub mod output_job;
pub mod project;
pub mod windows;

pub use draw::{Outcome, PageCx, field, problem_label, tree_row};

/// One Command a page wants run when the dialog applies.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub id: String,
    pub params: Value,
}

impl Step {
    pub fn new(id: &str, params: Value) -> Step {
        Step {
            id: id.to_owned(),
            params,
        }
    }
}

/// An invalid value on a page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// Which field it is on, as the page names its fields (such as `board_thickness` or
    /// `path/2` for a table's third row).
    pub field: String,
    /// What is wrong, for the user.
    pub message: String,
}

impl Problem {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Problem {
        Problem {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// One page of a settings dialog.
pub trait Page: Any {
    /// The page's name in the tree.
    fn title(&self) -> String;

    /// The heading the page sits under in the tree, if any (Board Setup's Rules pages).
    fn group(&self) -> Option<String> {
        None
    }

    /// Takes in the Project as it is now. With `reset`, every draft is replaced (after an
    /// Apply); otherwise only values the user has not changed follow the Project (after
    /// an undo, or a change made elsewhere).
    fn load(&mut self, project: &Project, reset: bool);

    /// The Commands that make the Project match the drafts, in order. Empty when nothing
    /// changed.
    fn steps(&self) -> Vec<Step>;

    /// The page's invalid values.
    fn problems(&self) -> Vec<Problem>;

    /// Draws the page's content.
    fn show(&mut self, ui: &mut egui::Ui, cx: &mut PageCx<'_>);

    /// A folder the user picked after the page asked for one (relative to the Project
    /// folder when it can be).
    fn picked_folder(&mut self, _path: &Path) {}

    /// Whether the page offers Reset to defaults.
    fn has_defaults(&self) -> bool {
        false
    }

    /// Puts the defaults in the drafts, as pending edits until Apply.
    fn reset_to_defaults(&mut self, _shell: &mut Shell) {}
}

/// The buttons along a settings window's bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Buttons {
    OkCancelApply,
    /// The Change List's.
    ApplyClose,
}

/// A value being edited: what the Project has (`base`) and what the dialog shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft<T> {
    base: T,
    value: T,
}

impl<T: Clone + PartialEq> Draft<T> {
    pub fn new(value: T) -> Draft<T> {
        Draft {
            base: value.clone(),
            value,
        }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    pub fn set(&mut self, value: T) {
        self.value = value;
    }

    /// What the Project has.
    pub fn base(&self) -> &T {
        &self.base
    }

    /// Whether the user changed it.
    pub fn changed(&self) -> bool {
        self.value != self.base
    }

    /// Follows the Project's `fresh` value unless the user changed this one; with
    /// `reset`, takes it whatever.
    pub fn load(&mut self, fresh: T, reset: bool) {
        if reset || !self.changed() {
            self.value = fresh.clone();
        }
        self.base = fresh;
    }
}

impl<T: Clone + PartialEq + Default> Default for Draft<T> {
    fn default() -> Self {
        Draft::new(T::default())
    }
}

/// A settings dialog: its pages, which one is shown, and the last Apply's error.
pub struct SettingsDialog {
    title: String,
    pages: Vec<Box<dyn Page>>,
    selected: usize,
    error: Option<String>,
    undo_scope: Scope,
    buttons: Buttons,
    /// Whether the page tree shows.
    tree: bool,
}

impl SettingsDialog {
    /// A dialog called `title` with `pages`, loaded from `project`.
    pub fn new(title: String, mut pages: Vec<Box<dyn Page>>, project: &Project) -> Self {
        for page in &mut pages {
            page.load(project, true);
        }
        SettingsDialog {
            title,
            pages,
            selected: 0,
            error: None,
            undo_scope: Scope::Project,
            buttons: Buttons::OkCancelApply,
            tree: true,
        }
    }

    /// The same dialog with Apply and Close, and no page tree: the Change List's. Its
    /// changes are the design's, not edits, so closing it never asks.
    pub fn apply_and_close(mut self) -> Self {
        self.buttons = Buttons::ApplyClose;
        self.tree = false;
        self
    }

    pub fn buttons(&self) -> Buttons {
        self.buttons
    }

    /// Whether closing the window would lose edits: something changed, and the window is
    /// one with OK and Cancel.
    pub fn unapplied(&self) -> bool {
        self.buttons == Buttons::OkCancelApply && self.has_changes()
    }

    /// Reset to defaults on the shown page.
    pub fn reset_page(&mut self, shell: &mut Shell) {
        if let Some(page) = self.pages.get_mut(self.selected) {
            page.reset_to_defaults(shell);
        }
    }

    /// The same dialog with its changes on `scope`'s undo stack, not the Project's.
    pub fn on_stack(mut self, scope: Scope) -> Self {
        self.undo_scope = scope;
        self
    }

    /// The undo stack Apply puts the changes on, which `Mod+Z` undoes while the dialog is
    /// open.
    pub fn undo_scope(&self) -> Scope {
        self.undo_scope
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn page_titles(&self) -> Vec<String> {
        self.pages.iter().map(|p| p.title()).collect()
    }

    /// The page shown, by index.
    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select(&mut self, index: usize) {
        if index < self.pages.len() {
            self.selected = index;
        }
    }

    /// The page of type `T`.
    pub fn page<T: Page>(&self) -> Option<&T> {
        self.pages
            .iter()
            .find_map(|p| (p.as_ref() as &dyn Any).downcast_ref::<T>())
    }

    pub fn page_mut<T: Page>(&mut self) -> Option<&mut T> {
        self.pages
            .iter_mut()
            .find_map(|p| (p.as_mut() as &mut dyn Any).downcast_mut::<T>())
    }

    /// Every page's Commands, in page order.
    pub fn steps(&self) -> Vec<Step> {
        self.pages.iter().flat_map(|p| p.steps()).collect()
    }

    /// Whether any page has changes to apply.
    pub fn has_changes(&self) -> bool {
        self.pages.iter().any(|p| !p.steps().is_empty())
    }

    /// Every invalid value, with its page's title.
    pub fn problems(&self) -> Vec<(String, Problem)> {
        self.pages
            .iter()
            .flat_map(|p| {
                let title = p.title();
                p.problems().into_iter().map(move |q| (title.clone(), q))
            })
            .collect()
    }

    /// OK is enabled while every value is valid.
    pub fn can_ok(&self) -> bool {
        self.pages.iter().all(|p| p.problems().is_empty())
    }

    /// Apply is enabled while every value is valid and something changed.
    pub fn can_apply(&self) -> bool {
        self.can_ok() && self.has_changes()
    }

    /// The last Apply's error, shown above the buttons.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Follows the Project where the user has not changed a value.
    pub fn sync(&mut self, project: &Project) {
        for page in &mut self.pages {
            page.load(project, false);
        }
    }

    /// Hands a picked folder to the pages.
    pub fn picked_folder(&mut self, path: &Path) {
        for page in &mut self.pages {
            page.picked_folder(path);
        }
    }

    /// Commits every page's changes as one `batch`. Refused while a value is invalid; does
    /// nothing when nothing changed. On success the drafts become the Project's values.
    pub fn apply(&mut self, shell: &mut Shell) -> Result<(), CommandError> {
        if !self.can_ok() {
            return Err(CommandError::rejected(
                "some values are invalid; fix the flagged fields first",
            ));
        }
        let steps = self.steps();
        if steps.is_empty() {
            return Ok(());
        }
        let commands: Vec<Value> = steps
            .into_iter()
            .map(|s| json!({"id": s.id, "params": s.params}))
            .collect();
        match shell.run("batch", json!({ "commands": commands })) {
            Ok(_) => {
                self.error = None;
                if let Some(project) = shell.engine().project() {
                    for page in &mut self.pages {
                        page.load(project, true);
                    }
                }
                Ok(())
            }
            Err(e) => {
                self.error = Some(e.message.clone());
                Err(e)
            }
        }
    }
}
