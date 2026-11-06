//! The open settings windows: Preferences, Project Settings, Board Setup, the Output Job
//! window and the Change List.
//!
//! Each is a tab of its own ([`TabKey::Settings`]) that opens in a new OS window from the
//! docking layer, at [`SETTINGS_WINDOW`](crate::docking::SETTINGS_WINDOW) the first time
//! and where it was last put after that (kept in `state.json`). There is one of each:
//! opening one again shows it where it is, with its edits. A window's state lives here
//! while its tab is open; once the tab closes (from its window, the tab menu, a script or
//! a Project closing) the state goes with it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use reda_engine::model::{Project, Uuid};
use reda_engine::state::{self, WindowPlace};

use super::{Outcome, SettingsDialog, board_setup, change_list, output_job, project};
use crate::bridge::Bridge;
use crate::layout::{Layout, SettingsWindow, TabKey, WindowRect};
use crate::preferences_dialog::{Outcome as PreferencesOutcome, PageId, PreferencesDialog};
use crate::shell::Shell;

pub use crate::docking::{SETTINGS_WINDOW as OPENING_SIZE, opening_size};

/// How long, in seconds, a settings window must stay put before its place is written to
/// `state.json`.
pub const PLACE_SAVE_DELAY: f64 = 1.0;

/// What a settings window asks of the app after drawing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Its OK, Cancel or Close closed it.
    Close(SettingsWindow),
    /// A page wants a folder picked.
    PickFolder(SettingsWindow),
    /// Preferences wants a folder shown in the file manager.
    ShowFolder(PathBuf),
}

/// The open settings windows' state.
#[derive(Default)]
pub struct SettingsWindows {
    dialogs: BTreeMap<SettingsWindow, SettingsDialog>,
    /// The job the Output Job window shows.
    job: Option<Uuid>,
    preferences: Option<PreferencesDialog>,
    requests: Vec<Request>,
    /// Places not yet written to `state.json`, with when each was last seen to change.
    places: BTreeMap<SettingsWindow, (WindowRect, f64)>,
}

impl SettingsWindows {
    /// The dialog of `window`, if it is open (any but Preferences).
    pub fn dialog(&self, window: SettingsWindow) -> Option<&SettingsDialog> {
        self.dialogs.get(&window)
    }

    pub fn dialog_mut(&mut self, window: SettingsWindow) -> Option<&mut SettingsDialog> {
        self.dialogs.get_mut(&window)
    }

    pub fn preferences(&self) -> Option<&PreferencesDialog> {
        self.preferences.as_ref()
    }

    /// The job the Output Job window shows, while it is open.
    pub fn job(&self) -> Option<Uuid> {
        self.job
            .filter(|_| self.dialogs.contains_key(&SettingsWindow::OutputJob))
    }

    /// Whether `window` is open.
    pub fn is_open(&self, window: SettingsWindow) -> bool {
        match window {
            SettingsWindow::Preferences => self.preferences.is_some(),
            _ => self.dialogs.contains_key(&window),
        }
    }

    /// Whether closing `window` now would lose edits.
    pub fn has_unapplied(&self, window: SettingsWindow) -> bool {
        self.dialogs
            .get(&window)
            .is_some_and(SettingsDialog::unapplied)
    }

    /// The windows with unapplied changes, in [`SettingsWindow::ALL`] order.
    pub fn unapplied(&self) -> Vec<SettingsWindow> {
        SettingsWindow::ALL
            .into_iter()
            .filter(|w| self.has_unapplied(*w))
            .collect()
    }

    /// Applies `window`'s changes as one `batch`; the error says why it could not.
    pub fn apply(&mut self, shell: &mut Shell, window: SettingsWindow) -> Result<(), String> {
        match self.dialogs.get_mut(&window) {
            Some(dialog) => dialog.apply(shell).map_err(|e| e.message),
            None => Ok(()),
        }
    }

    /// Shows the settings window `window`, opening it on the Project when it is not open.
    /// Returns whether it was open already (so the app raises its OS window), or `None`
    /// when it cannot open (no Project, no PCB Document for Board Setup).
    pub fn open(&mut self, shell: &mut Shell, window: SettingsWindow) -> Option<bool> {
        let open = self.is_open(window) && shell.layout().place(&tab(window)).is_some();
        if !open {
            match window {
                SettingsWindow::Preferences => {
                    self.preferences = Some(PreferencesDialog::on(PageId::General));
                }
                SettingsWindow::OutputJob => return None,
                _ => {
                    let dialog = new_dialog(shell.engine().project()?, window)?;
                    shell.set_settings_title(window, dialog.title().to_owned());
                    self.dialogs.insert(window, dialog);
                }
            }
        }
        self.show_tab(shell, window);
        Some(open)
    }

    /// Opens Preferences on `page`; when it is open, it moves to `page`.
    pub fn open_preferences(&mut self, shell: &mut Shell, page: Option<PageId>) -> bool {
        let open = self.open(shell, SettingsWindow::Preferences) == Some(true);
        if let (Some(dialog), Some(page)) = (&mut self.preferences, page) {
            dialog.select(page);
        }
        open
    }

    /// Opens the Output Job window on `job`, or moves it there; returns whether it was
    /// open already, or `None` when the Project has no such job. The app asks first when
    /// the window has unapplied changes for another job.
    pub fn open_job(&mut self, shell: &mut Shell, job: Uuid) -> Option<bool> {
        let window = SettingsWindow::OutputJob;
        let open = self.is_open(window) && shell.layout().place(&tab(window)).is_some();
        if !open || self.job != Some(job) {
            let dialog = output_job::dialog(shell.engine().project()?, job)?;
            shell.set_settings_title(window, dialog.title().to_owned());
            self.dialogs.insert(window, dialog);
            self.job = Some(job);
        }
        self.show_tab(shell, window);
        Some(open)
    }

    fn show_tab(&mut self, shell: &mut Shell, window: SettingsWindow) {
        let saved = shell.engine().user_dirs().and_then(|dirs| {
            let place = state::read(dirs)
                .settings_windows
                .get(window.id())
                .copied()?;
            Some(WindowRect {
                x: place.x,
                y: place.y,
                width: place.width,
                height: place.height,
            })
        });
        shell.open_settings(window, saved);
    }

    /// Draws `window`'s content into its tab.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        shell: &mut Shell,
        bridge: &mut Bridge,
        window: SettingsWindow,
    ) {
        if window == SettingsWindow::Preferences {
            let Some(dialog) = &mut self.preferences else {
                return;
            };
            match dialog.show(ui, shell, bridge) {
                PreferencesOutcome::Open => {}
                PreferencesOutcome::Closed => self.requests.push(Request::Close(window)),
                PreferencesOutcome::PickFolder => self.requests.push(Request::PickFolder(window)),
                PreferencesOutcome::ShowFolder(folder) => {
                    self.requests.push(Request::ShowFolder(folder));
                }
            }
            return;
        }
        let Some(dialog) = self.dialogs.get_mut(&window) else {
            return;
        };
        match dialog.show(ui, shell) {
            Outcome::Open => {}
            Outcome::Closed => self.requests.push(Request::Close(window)),
            Outcome::PickFolder => self.requests.push(Request::PickFolder(window)),
        }
    }

    /// What the windows asked for since this was last called.
    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    /// A folder the user picked for `window`.
    pub fn picked_folder(
        &mut self,
        shell: &mut Shell,
        window: SettingsWindow,
        path: &std::path::Path,
    ) {
        match window {
            SettingsWindow::Preferences => {
                if let Some(dialog) = &mut self.preferences {
                    dialog.picked_folder(shell, path);
                }
            }
            _ => {
                if let (Some(folder), Some(dialog)) =
                    (shell.engine().location(), self.dialogs.get_mut(&window))
                {
                    dialog.picked_folder(&crate::os::relative_path(folder, path));
                }
            }
        }
    }

    /// Drops the state of every window whose tab is no longer in `layout`, after writing
    /// where it was. Call it once a frame, at `now` (seconds).
    pub fn follow_layout(&mut self, shell: &Shell, now: f64) {
        let layout = shell.layout();
        let gone = |w: &SettingsWindow| layout.place(&tab(*w)).is_none();
        self.dialogs.retain(|w, _| !gone(w));
        if self.preferences.is_some() && gone(&SettingsWindow::Preferences) {
            self.preferences = None;
        }
        self.track_places(shell, layout, now);
    }

    /// Notes where each settings window alone in its OS window is now, and writes the
    /// places that stayed put for [`PLACE_SAVE_DELAY`] (or whose window closed).
    fn track_places(&mut self, shell: &Shell, layout: &Layout, now: f64) {
        for window in SettingsWindow::ALL {
            let key = tab(window);
            let rect = layout.window_of(&key).and_then(|id| {
                let tabs = layout.tabs_in_window(id);
                let alone = tabs.len() == 1;
                let floating = layout.windows.iter().find(|w| w.id == id)?;
                floating.rect.filter(|_| alone)
            });
            if let Some(rect) = rect
                && self.places.get(&window).map(|(r, _)| *r) != Some(rect)
            {
                self.places.insert(window, (rect, now));
            }
        }
        let due: Vec<(SettingsWindow, WindowRect)> = self
            .places
            .iter()
            .filter(|(w, (_, since))| {
                now - since >= PLACE_SAVE_DELAY || layout.place(&tab(**w)).is_none()
            })
            .map(|(w, (r, _))| (*w, *r))
            .collect();
        if due.is_empty() {
            return;
        }
        if let Some(dirs) = shell.engine().user_dirs() {
            let _ = state::update(dirs, |s| {
                for (window, r) in &due {
                    let place = WindowPlace {
                        x: r.x,
                        y: r.y,
                        width: r.width,
                        height: r.height,
                    };
                    s.settings_windows.insert(window.id().to_owned(), place);
                }
            });
        }
        // Written: keep the rect, so it is not written again until it changes, with a time
        // that never comes due; forget a closed window's.
        for (window, rect) in due {
            if layout.place(&tab(window)).is_some() {
                self.places.insert(window, (rect, f64::INFINITY));
            } else {
                self.places.remove(&window);
            }
        }
    }

    /// Whether a window's place waits to be written to `state.json`.
    pub fn places_pending(&self) -> bool {
        self.places.values().any(|(_, since)| since.is_finite())
    }

    /// Whether a settings window takes every key: Preferences does while its Keyboard
    /// Shortcuts page records one.
    pub fn has_keys(&self) -> bool {
        self.preferences
            .as_ref()
            .is_some_and(PreferencesDialog::is_recording)
    }
}

fn tab(window: SettingsWindow) -> TabKey {
    TabKey::Settings(window)
}

/// A new dialog for `window` on `project`; `None` when it cannot open there.
fn new_dialog(project: &Project, window: SettingsWindow) -> Option<SettingsDialog> {
    match window {
        SettingsWindow::ProjectSettings => Some(project::dialog(project)),
        SettingsWindow::BoardSetup => project.pcb().map(|_| board_setup::dialog(project)),
        SettingsWindow::ChangeList => project.pcb().map(|_| change_list::dialog(project)),
        SettingsWindow::Preferences | SettingsWindow::OutputJob => None,
    }
}
