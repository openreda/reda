//! The closing and quitting prompts, so unsaved work is not lost.
//!
//! - **Closing a Document tab:** when the Document (or Library element) is dirty, the tab
//!   closes only after Save, Don't Save or Cancel ([`Ask::Tab`]). Documents stay loaded
//!   after their tab closes, so Don't Save reverts the Document to its file
//!   (`document.revert`); for a Library element it closes the element, which discards its
//!   changes.
//! - **Quitting, closing the Project or opening another:** with anything dirty, one list
//!   asks first ([`Ask::List`]): every dirty Document, the Project file and each open
//!   Library element with unsaved changes, with a tick per row, then Save Selected, Don't
//!   Save and Cancel.
//! - **Closing a settings window with unapplied changes** (its tab, its OS window, or the
//!   Output Job window moving to another job): Apply, Discard or Cancel
//!   ([`Ask::Settings`]). The list prompt lists these windows too, first; a ticked one is
//!   applied before the saves run, and what its Apply dirtied is saved with the rest.
//! - A Save that fails keeps the prompt open with the error, so the window stays open and
//!   the Recovery Snapshots stay in place. The window deletes snapshots on exit only once
//!   a close got past this prompt ([`Closing::may_clear_snapshots`]).
//!
//! [`Closing`] decides when to ask and carries out the answers on the [`Shell`]; drawing
//! is in [`crate::closing_dialog`]. The window routes every GUI path that closes a tab or
//! replaces the Project through [`Closing::hold`] or [`Closing::hold_open`]. Bridged
//! Commands do not ask: an agent's `layout.tab.close` loses nothing, since the Document
//! stays loaded and dirty.

use std::path::PathBuf;

use reda_engine::model::{Scope, Uuid};
use reda_engine::store;
use serde_json::{Value, json};

use crate::layout::{SettingsWindow, TabKey, WindowId};
use crate::settings::windows::SettingsWindows;
use crate::shell::Shell;
use crate::tr;

/// What a prompt row stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// The Project file, `.reda`.
    Project,
    Document(Uuid),
    /// An open Library element (a Library editor tab).
    Element(Uuid),
    /// A settings window with unapplied changes.
    Settings(SettingsWindow),
}

impl Item {
    fn of(scope: Scope) -> Item {
        match scope {
            Scope::Project => Item::Project,
            Scope::Document(uuid) => Item::Document(uuid),
            Scope::Element(uuid) => Item::Element(uuid),
        }
    }
}

/// One row of the list prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub item: Item,
    pub title: String,
    /// Save Selected saves the ticked rows; every row starts ticked.
    pub ticked: bool,
}

/// What runs once the prompt is answered with Save or Don't Save.
#[derive(Clone, Debug, PartialEq)]
pub enum Then {
    /// A Command held back (a tab close, `project.open`, `project.new`…).
    Run(String, Value),
    /// [`Shell::open_project`] on this path.
    OpenProject(PathBuf),
    /// Close the window.
    Quit,
    /// Move the Output Job window to this job.
    OpenOutputJob(Uuid),
}

/// The open prompt.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// Save, Don't Save or Cancel for one Document's last tab.
    Tab {
        item: Item,
        title: String,
        then: Then,
    },
    /// The list of everything unsaved, before quitting or replacing the Project.
    List { rows: Vec<Row>, then: Then },
    /// Apply, Discard or Cancel for a settings window with unapplied changes.
    Settings {
        window: SettingsWindow,
        title: String,
        then: Then,
    },
}

/// How the user answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Save (the tab prompt) or Save Selected (the list).
    Save,
    DontSave,
    Cancel,
}

/// The prompts' state: the open prompt, the last failed Save, and whether the window may
/// close.
#[derive(Debug, Default)]
pub struct Closing {
    ask: Option<Ask>,
    error: Option<String>,
    quit_approved: bool,
}

/// The Commands that replace or close the open Project. `project.close` is listed for
/// when it exists: its GUI path then asks like the others.
const REPLACES_PROJECT: &[&str] = &["project.open", "project.new", "project.close"];

impl Closing {
    pub fn ask(&self) -> Option<&Ask> {
        self.ask.as_ref()
    }

    pub fn ask_mut(&mut self) -> Option<&mut Ask> {
        self.ask.as_mut()
    }

    /// Why the last Save failed, while its prompt stays open.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Whether the user let the window close: quitting got past the prompt.
    pub fn quit_approved(&self) -> bool {
        self.quit_approved
    }

    /// Whether the GUI may run Command `id` now. When it would close a dirty Document's
    /// tab or a settings window with unapplied changes, or replace a Project with unsaved
    /// changes, a prompt opens instead and the Command runs once it is answered. Nothing
    /// runs while a prompt is open.
    pub fn hold(
        &mut self,
        shell: &Shell,
        settings: &SettingsWindows,
        id: &str,
        params: &Value,
    ) -> bool {
        if self.ask.is_some() {
            return false;
        }
        let base = id.split(':').next().unwrap_or(id);
        let then = || Then::Run(id.to_owned(), params.clone());
        let uuid = |key: &str| params[key].as_str().and_then(|s| s.parse::<Uuid>().ok());
        let closing_settings = match base {
            "layout.tab.close" => match params["tab"].as_str().map(str::parse) {
                Some(Ok(TabKey::Settings(window))) => vec![window],
                _ => Vec::new(),
            },
            "layout.window.close" => {
                let window = serde_json::from_value::<WindowId>(params["window"].clone());
                let tabs = window.map(|w| shell.layout().tabs_in_window(w));
                (tabs.unwrap_or_default().into_iter())
                    .filter_map(|t| match t {
                        TabKey::Settings(window) => Some(*window),
                        _ => None,
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        if let Some(window) = closing_settings
            .into_iter()
            .find(|w| settings.has_unapplied(*w))
        {
            return self.hold_settings(shell, window, then());
        }
        let target = match base {
            "layout.tab.close" => match params["tab"].as_str().map(str::parse) {
                Some(Ok(TabKey::Document(uuid))) => Some(uuid),
                _ => None,
            },
            "library.element.close" => uuid("element"),
            _ if REPLACES_PROJECT.contains(&base) => return self.list(shell, settings, then()),
            _ => None,
        };
        match target.and_then(|uuid| dirty_item(shell, uuid)) {
            Some(item) => {
                self.open(Ask::Tab {
                    item,
                    title: title(shell, item),
                    then: then(),
                });
                false
            }
            None => true,
        }
    }

    /// Whether `then` may go ahead now, closing the settings window `window` or moving it
    /// elsewhere; with unapplied changes, Apply, Discard or Cancel asks first.
    pub fn hold_settings(&mut self, shell: &Shell, window: SettingsWindow, then: Then) -> bool {
        if self.ask.is_some() {
            return false;
        }
        self.open(Ask::Settings {
            window,
            title: title(shell, Item::Settings(window)),
            then,
        });
        false
    }

    /// Whether the GUI may open the Project at `path` now, replacing the open one; when
    /// it has unsaved changes the list prompt opens instead.
    pub fn hold_open(&mut self, shell: &Shell, settings: &SettingsWindows, path: PathBuf) -> bool {
        self.ask.is_none() && self.list(shell, settings, Then::OpenProject(path))
    }

    /// Whether the window may close now. With unsaved changes the list prompt opens (once)
    /// and the close waits for it.
    pub fn hold_quit(&mut self, shell: &Shell, settings: &SettingsWindows) -> bool {
        if self.quit_approved {
            return true;
        }
        if self.ask.is_some() {
            return false;
        }
        let ok = self.list(shell, settings, Then::Quit);
        self.quit_approved = ok;
        ok
    }

    /// Whether the window may delete the Recovery Snapshots as it exits: a close got
    /// past the prompt, or there is nothing unsaved to lose.
    pub fn may_clear_snapshots(&self, shell: &Shell) -> bool {
        self.quit_approved || (self.ask.is_none() && rows_of(shell).is_empty())
    }

    /// Opens the list prompt for `then` when anything is unsaved; returns whether `then`
    /// may go ahead now.
    fn list(&mut self, shell: &Shell, settings: &SettingsWindows, then: Then) -> bool {
        let rows = rows(shell, settings);
        if rows.is_empty() {
            return true;
        }
        self.open(Ask::List { rows, then });
        false
    }

    fn open(&mut self, ask: Ask) {
        self.ask = Some(ask);
        self.error = None;
    }

    /// Carries out `answer` on `shell` (Save is Apply for a settings window). Returns what
    /// to run next once the prompt is settled; a failed Save or Apply keeps the prompt
    /// open with the error and returns nothing.
    pub fn answer(
        &mut self,
        shell: &mut Shell,
        settings: &mut SettingsWindows,
        answer: Answer,
    ) -> Option<Then> {
        let ask = self.ask.take()?;
        self.error = None;
        let then = match (ask, answer) {
            (_, Answer::Cancel) => return None,
            (Ask::Tab { item, title, then }, Answer::Save) => {
                if let Err(e) = save(shell, item) {
                    self.ask = Some(Ask::Tab { item, title, then });
                    self.error = Some(e);
                    return None;
                }
                then
            }
            (Ask::Tab { item, then, .. }, Answer::DontSave) => match item {
                // A Document never saved has no file to go back to: it stays loaded and
                // unsaved, and the quit prompt lists it.
                Item::Document(uuid) => {
                    let _ = shell.run("document.revert", json!({ "document": uuid }));
                    then
                }
                Item::Element(uuid) => {
                    Then::Run("library.element.close".into(), json!({ "element": uuid }))
                }
                Item::Project | Item::Settings(_) => then,
            },
            (
                Ask::Settings {
                    window,
                    title,
                    then,
                },
                Answer::Save,
            ) => {
                if let Err(e) = settings.apply(shell, window) {
                    self.ask = Some(Ask::Settings {
                        window,
                        title,
                        then,
                    });
                    self.error = Some(e);
                    return None;
                }
                then
            }
            (Ask::Settings { then, .. }, Answer::DontSave) => then,
            (Ask::List { mut rows, then }, Answer::Save) => {
                let mut failed = Vec::new();
                // Ticked settings windows apply first; what that dirties is saved too.
                let before = rows_of(shell);
                for row in rows.iter().filter(|r| r.ticked) {
                    if let Item::Settings(window) = row.item
                        && let Err(e) = settings.apply(shell, window)
                    {
                        failed.push(e);
                    }
                }
                let dirtied: Vec<Item> = (rows_of(shell).into_iter())
                    .filter(|i| !before.contains(i))
                    .collect();
                let to_save = (rows.iter())
                    .filter(|r| r.ticked && !matches!(r.item, Item::Settings(_)))
                    .map(|r| r.item)
                    .chain(dirtied);
                for item in to_save.collect::<Vec<_>>() {
                    if let Err(e) = save(shell, item) {
                        failed.push(e);
                    }
                }
                if !failed.is_empty() {
                    // What saved or applied leaves the list; what failed stays, with the
                    // reason.
                    let mut unsaved = rows_of(shell);
                    unsaved.extend(settings.unapplied().into_iter().map(Item::Settings));
                    rows.retain(|r| !r.ticked || unsaved.contains(&r.item));
                    self.ask = Some(Ask::List { rows, then });
                    self.error = Some(failed.join("\n"));
                    return None;
                }
                then
            }
            (Ask::List { then, .. }, Answer::DontSave) => then,
        };
        if then == Then::Quit {
            self.quit_approved = true;
        }
        Some(then)
    }
}

/// What the list prompt lists: the settings windows with unapplied changes, then the
/// Project file, then every dirty Document and open Library element, as the engine orders
/// them.
pub fn rows(shell: &Shell, settings: &SettingsWindows) -> Vec<Row> {
    (settings.unapplied().into_iter().map(Item::Settings))
        .chain(rows_of(shell))
        .map(|item| Row {
            item,
            title: title(shell, item),
            ticked: true,
        })
        .collect()
}

fn rows_of(shell: &Shell) -> Vec<Item> {
    let engine = shell.engine();
    engine.dirty_scopes().into_iter().map(Item::of).collect()
}

/// The dirty item behind a tab keyed by `uuid`, if it is dirty.
fn dirty_item(shell: &Shell, uuid: Uuid) -> Option<Item> {
    let dirty = shell.engine().dirty_scopes();
    [Scope::Document(uuid), Scope::Element(uuid)]
        .into_iter()
        .find(|s| dirty.contains(s))
        .map(Item::of)
}

/// The row's name: the Project file's name, or the tab title.
fn title(shell: &Shell, item: Item) -> String {
    let engine = shell.engine();
    match item {
        Item::Project => match engine.project() {
            Some(p) => format!("{}.{}", p.props().name, store::PROJECT_EXTENSION),
            None => String::new(),
        },
        Item::Document(uuid) => (engine.project().and_then(|p| p.document(uuid)))
            .map(|d| d.title.clone())
            .unwrap_or_default(),
        Item::Element(uuid) => crate::element_editor::title(engine, uuid).unwrap_or_default(),
        Item::Settings(window) => match shell.settings_title(window) {
            Some(title) => title.to_owned(),
            None => crate::docking::settings_title(window),
        },
    }
}

/// Saves one item's file; the error says which file and why.
fn save(shell: &mut Shell, item: Item) -> Result<(), String> {
    let out = match item {
        Item::Project => shell.run("project.save", json!({ "document": "project" })),
        Item::Document(uuid) => shell.run("project.save", json!({ "document": uuid })),
        Item::Element(uuid) => shell.run("library.element.save", json!({ "element": uuid })),
        Item::Settings(_) => return Ok(()),
    };
    out.map(|_| ()).map_err(|e| {
        tr!(
            "closing-save-failed",
            file = title(shell, item),
            error = e.message
        )
    })
}
