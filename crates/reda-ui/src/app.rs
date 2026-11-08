//! The window: menu bar, the docking layout (Docks, tab groups with the Start page,
//! canvases and Panels), and the status bar with the Panels button. Drawing only; the state
//! lives in [`Shell`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use egui::{Panel, ThemePreference, Ui, UiBuilder};
use reda_engine::Engine;
use reda_engine::model::{Scope, Uuid};
use reda_engine::preferences;
use serde_json::json;

use crate::agents as bridge_ui;
use crate::bars::draw::{self, Invocation};
use crate::bars::{self, Request};
use crate::bridge::Bridge;
use crate::canvas::{CanvasResources, canvas_id};
use crate::closing::{Closing, Then};
use crate::command_dialog::{self, Call, Dialogs, ParamsDialog};
use crate::dialogs::{FileDialogs, Pick};
use crate::docking::{self, panel_title};
use crate::job_run::JobRuns;
use crate::journal_panel::JournalAction;
use crate::layout::{PanelId, SettingsWindow, TabKey};
use crate::new_project::{FormAction, NewProjectForm};
use crate::os;
use crate::palette::{self, Palette, PaletteOutcome};
use crate::preferences_dialog::{PageId, PreferencesDialog};
use crate::projects_panel::{self, ProjectsAction};
use crate::recovery_dialog::{self, CrashAction, RecoveryAction};
use crate::schematic_dialogs::{self, DialogKind, SchematicDialog};
use crate::settings::windows::{Request as SettingsRequest, SettingsWindows};
use crate::shell::{InstalledFonts, Shell};
use crate::start_page::StartAction;
use crate::user_preferences::Watcher;
use crate::{a11y, crash, keymap, tab_switcher, theme, tr};

/// Where the window remembers its status bar row's height between frames.
fn status_bar_id() -> egui::Id {
    egui::Id::NULL.with("status-bar-height")
}

/// The REDA window.
pub struct App {
    shell: Shell,
    files: FileDialogs,
    new_project: Option<NewProjectForm>,
    /// The open settings windows.
    settings: SettingsWindows,
    /// A settings window opened again, to raise once its OS window is known.
    raise: Option<SettingsWindow>,
    /// The running Output Job, or the report of the last run.
    jobs: JobRuns,
    /// The Preferences generation the window last applied (theme, UI scale).
    applied_preferences: Option<u64>,
    /// Wakes the window when `preferences.toml` changes on disk.
    preferences_watcher: Option<Watcher>,
    /// Whether to start the watcher. Off in tests, which poll each frame instead, so a
    /// wake from the thread never adds frames.
    watch_preferences: bool,
    window_title: String,
    /// The window's renderer, to free closed canvases' GPU state.
    render_state: Option<egui_wgpu::RenderState>,
    theme_installed: bool,
    /// A Crash Report from an earlier run, not yet offered.
    crash_report: Option<PathBuf>,
    /// The control port, for agents.
    bridge: Bridge,
    bridge_waker_set: bool,
    /// The open Command Palette.
    palette: Option<Palette>,
    /// The hand-written dialogs that replace generated params forms.
    dialogs: Dialogs,
    /// The dialog asking for a Command's missing params.
    params_dialog: Option<Box<dyn ParamsDialog>>,
    /// The shortcut of a Command or Preset id, for the Palette (the Keymap's lookup).
    shortcuts: palette::ShortcutLookup,
    help: crate::help::Dialogs,
    /// A notice the Shell handed over (Import part's vendor terms), shown until OK.
    notice: Option<String>,
    /// The open schematic dialog (Find, Re-annotate…), if any.
    schematic_dialog: Option<SchematicDialog>,
    /// The closing and quitting prompts.
    closing: Closing,
    /// A relink waiting for the user to confirm taking a file that holds another
    /// Document: the missing Document and the file.
    relink_confirm: Option<(Uuid, PathBuf)>,
}

impl App {
    /// The window on `shell`. REDA's theme is installed on the first frame, as the
    /// Preferences ask.
    pub fn new(shell: Shell) -> App {
        let crash_report = (shell.engine().user_dirs())
            .and_then(|dirs| crash::unseen(&crash::reports_dir(dirs)).pop());
        App {
            crash_report,
            shell,
            files: FileDialogs::default(),
            new_project: None,
            settings: SettingsWindows::default(),
            raise: None,
            jobs: JobRuns::default(),
            applied_preferences: None,
            preferences_watcher: None,
            watch_preferences: false,
            window_title: String::new(),
            render_state: None,
            theme_installed: false,
            help: Default::default(),
            palette: None,
            dialogs: {
                let mut dialogs = crate::library_dialogs::dialogs();
                schematic_dialogs::register_dialogs(&mut dialogs);
                projects_panel::register_package_dialog(&mut dialogs);
                dialogs
            },
            schematic_dialog: None,
            params_dialog: None,
            shortcuts: Box::new(|_, _| None),
            bridge: Bridge::off(),
            bridge_waker_set: false,
            notice: None,
            closing: Closing::default(),
            relink_confirm: None,
        }
    }

    /// Hosts the control port `bridge` (see [`Bridge::start`]); without one the bridge is
    /// off.
    pub fn with_bridge(mut self, bridge: Bridge) -> App {
        self.bridge = bridge;
        self
    }

    /// Wakes the window when `preferences.toml` changes on disk, so an idle window applies
    /// another instance's change at once. Without it, a change applies on the next frame.
    pub fn watching_preferences(mut self) -> App {
        self.watch_preferences = true;
        self
    }

    pub fn bridge(&self) -> &Bridge {
        &self.bridge
    }

    /// Between frames: runs the bridged Commands waiting, raising the window for a file
    /// another app handed over.
    fn pump_bridge(&mut self, ctx: &egui::Context) {
        if !self.bridge_waker_set {
            let waker = ctx.clone();
            self.bridge.set_waker(move || waker.request_repaint());
            self.bridge_waker_set = true;
        }
        self.bridge.pump(&mut self.shell);
        if self.bridge.take_focus_request() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.drop_closed_canvases();
        }
    }

    /// Lets the window free the GPU state of canvases whose tabs close.
    pub fn with_render_state(mut self, render_state: egui_wgpu::RenderState) -> App {
        self.render_state = Some(render_state);
        self
    }

    /// Shows each Command's shortcut from `lookup` (by Command or Preset id) in the
    /// Command Palette.
    pub fn with_shortcuts(
        mut self,
        lookup: impl Fn(&Shell, &str) -> Option<String> + 'static,
    ) -> App {
        self.shortcuts = Box::new(lookup);
        self
    }

    /// The hand-written dialogs that replace generated params forms; register one with
    /// [`Dialogs::register`].
    pub fn dialogs_mut(&mut self) -> &mut Dialogs {
        &mut self.dialogs
    }

    pub fn shell(&self) -> &Shell {
        &self.shell
    }

    pub fn shell_mut(&mut self) -> &mut Shell {
        &mut self.shell
    }

    /// The closing and quitting prompts' state.
    pub fn closing(&self) -> &Closing {
        &self.closing
    }

    /// Draws one frame into `ui`, which fills the window.
    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if !self.theme_installed {
            theme::install(&ctx, theme::preference(self.shell.preferences()));
            crate::icons::install_loaders(&ctx);
            self.theme_installed = true;
        }
        self.apply_preferences(&ctx);
        self.pump_bridge(&ctx);
        self.collect_picked_files();
        self.update_title(&ctx);
        if ctx.input(|i| i.viewport().close_requested())
            && !self.closing.hold_quit(&self.shell, &self.settings)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.keys(&ctx);
        tab_switcher::release(&ctx, &mut self.shell);

        let clicked = self.bars(ui);
        // The status bar's space is set aside before the docking layer's, but what is in
        // it is drawn after, so keyboard focus reaches it last, as it sits in the window.
        let status_height = ctx.data(|d| d.get_temp::<f32>(status_bar_id()));
        let status_height = status_height.unwrap_or(ui.spacing().interact_size.y);
        let status = Panel::bottom("status-bar")
            .show(ui, |ui| {
                let mut rect = ui.available_rect_before_wrap();
                rect.set_height(status_height);
                ui.advance_cursor_after_rect(rect);
                rect
            })
            .inner;
        let (windows, bridge) = (&mut self.settings, &mut self.bridge);
        let mut settings = |ui: &mut Ui, shell: &mut Shell, window: SettingsWindow| {
            windows.show(ui, shell, bridge, window);
        };
        let frame = docking::show(ui, &mut self.shell, &mut settings);
        let builder = UiBuilder::new()
            .max_rect(status)
            .id_salt("status-bar-content");
        let drawn = ui.scope_builder(builder, |ui| self.status_bar(ui));
        let height = drawn.inner.height();
        ctx.data_mut(|d| d.insert_temp(status_bar_id(), height));
        tab_switcher::show(&ctx, &mut self.shell);
        // A menu or toolbar item that lacks params asks for them.
        if let Some((id, params)) = clicked {
            self.call(&id, params);
        }
        for (id, params) in frame.commands {
            self.run_held(&id, params);
        }
        // Keys pressed in a window of its own, unless a dialog here has the keys.
        if !self.dialog_has_keys() {
            for (id, params) in frame.keys {
                self.run_held(&id, params);
            }
        }
        for (id, params) in frame.calls {
            self.call(&id, params);
        }
        self.handle_requests(&ctx);
        self.help.show(&ctx, self.shell.engine());
        for action in frame.messages {
            crate::panels::messages::apply(&mut self.shell, action);
        }
        if let Some(action) = frame.start {
            self.start_action(&ctx, action);
        }
        match frame.projects {
            Some(ProjectsAction::AddLibraryFolder) => {
                self.files.start(&ctx, Pick::LibraryFolder);
            }
            Some(ProjectsAction::ShowInFolder(folder)) => {
                if let Err(e) = os::show_in_folder(&folder) {
                    self.shell
                        .report(tr!("error-show-in-folder", error = e.to_string()));
                }
            }
            Some(ProjectsAction::ProjectSettings) => self.open_project_settings(),
            Some(ProjectsAction::BoardSetup) => self.open_board_setup(),
            Some(ProjectsAction::EditJob(job)) => self.open_output_job(job),
            Some(ProjectsAction::RunJob(job)) => self.jobs.start(&ctx, &mut self.shell, job),
            Some(ProjectsAction::Relink(document)) => {
                let entry = (self.shell.engine().project())
                    .and_then(|p| p.missing_document(document))
                    .cloned();
                if let Some(entry) = entry {
                    let pick = Pick::Relink(document, entry.kind);
                    self.files.start_save(&ctx, pick, entry.title);
                }
            }
            Some(ProjectsAction::Package) => self.call("project.package", json!({})),
            None => {}
        }
        self.settings_requests(&ctx);
        if let Some(dialog) = self.settings.dialog_mut(SettingsWindow::OutputJob) {
            self.jobs.run_from_dialog(&ctx, &mut self.shell, dialog);
        }
        self.jobs.show(&ctx, &mut self.shell);
        if let Some(JournalAction::Export) = frame.journal {
            self.files.start(&ctx, Pick::ExportJournal);
        }
        self.drop_closed_canvases();
        bridge_ui::ask(&ctx, &mut self.bridge, &mut self.shell);
        self.shell.save_view_if_due(std::time::Instant::now());
        self.crash_recovery(&ctx);
        self.show_palette(&ctx);
        self.show_params_dialog(&ctx);
        self.show_relink_confirm(&ctx);
        self.show_notice(&ctx);
        if let Some(dialog) = &mut self.schematic_dialog
            && !dialog.show(&ctx, &mut self.shell)
        {
            self.schematic_dialog = None;
        }

        if let Some(form) = &mut self.new_project {
            match form.show(&ctx) {
                FormAction::None => {}
                FormAction::Browse => self.files.start(&ctx, Pick::NewProjectLocation),
                FormAction::Cancel => self.new_project = None,
                FormAction::Create => {
                    if let Some(params) = form.params() {
                        if !self
                            .closing
                            .hold(&self.shell, &self.settings, "project.new", &params)
                        {
                            // It is created once the closing prompt is answered.
                            self.new_project = None;
                        } else if self.shell.run_reporting("project.new", params).is_ok() {
                            self.new_project = None;
                            self.drop_closed_canvases();
                        }
                    }
                }
            }
        }
        self.show_closing(&ctx);
        a11y::focus_ring(&ctx);
    }

    /// Runs a Command the GUI asked for, unless it would close a dirty Document's tab or
    /// replace a Project with unsaved changes: then the closing prompt asks first.
    fn run_held(&mut self, id: &str, params: serde_json::Value) {
        if self.closing.hold(&self.shell, &self.settings, id, &params) {
            let _ = self.shell.run_reporting(id, params);
        }
    }

    /// Opens the Project at `path` from the GUI (the Open dialog, a recent Project),
    /// asking first when the open one has unsaved changes.
    pub fn open_project(&mut self, path: &Path) {
        if (self.closing).hold_open(&self.shell, &self.settings, path.to_path_buf()) {
            let _ = self.shell.open_project(path);
            self.drop_closed_canvases();
        }
    }

    /// Asks to close the window, as Quit does: it closes at once when nothing is unsaved,
    /// else once the closing prompt lets it.
    pub fn quit(&mut self, ctx: &egui::Context) {
        if self.closing.hold_quit(&self.shell, &self.settings) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Draws the closing prompt and carries out its answer.
    fn show_closing(&mut self, ctx: &egui::Context) {
        let Some(answer) = crate::closing_dialog::show(ctx, &mut self.closing) else {
            return;
        };
        match (self.closing).answer(&mut self.shell, &mut self.settings, answer) {
            Some(Then::Run(id, params)) => {
                let _ = self.shell.run_reporting(&id, params);
            }
            Some(Then::OpenProject(path)) => {
                let _ = self.shell.open_project(&path);
            }
            Some(Then::Quit) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Some(Then::OpenOutputJob(job)) => {
                self.settings.open_job(&mut self.shell, job);
            }
            None => {}
        }
        self.drop_closed_canvases();
    }

    /// Takes due Recovery Snapshots, and shows the recovery prompt and the Crash Report
    /// offer when there is one.
    fn crash_recovery(&mut self, ctx: &egui::Context) {
        if let Some(wait) = self.shell.tick_recovery(Instant::now()) {
            ctx.request_repaint_after(wait);
        }
        // The background Zone refill: keep its fills when it ends.
        if self.shell.poll_zone_refill() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        if let Some(prompt) = self.shell.recovery_prompt() {
            match recovery_dialog::recovery(ctx, prompt) {
                Some(RecoveryAction::Restore(file)) => {
                    let _ = self.shell.restore_snapshot(&file);
                }
                Some(RecoveryAction::Discard(file)) => self.shell.discard_snapshot(&file),
                Some(RecoveryAction::RestoreAll) => self.shell.restore_all(),
                Some(RecoveryAction::DiscardAll) => self.shell.discard_all(),
                Some(RecoveryAction::ShowInFolder(dir)) => self.show_in_folder(&dir),
                None => {}
            }
        } else if let Some(report) = self.crash_report.clone() {
            let dir = report.parent().map(Path::to_path_buf).unwrap_or_default();
            match recovery_dialog::crash(ctx, &report) {
                Some(CrashAction::OpenFolder) => self.show_in_folder(&dir),
                Some(CrashAction::ReportOnGitHub) => {
                    let text = std::fs::read_to_string(&report).unwrap_or_default();
                    ctx.open_url(egui::OpenUrl::new_tab(crash::issue_url(&text)));
                }
                Some(CrashAction::Dismiss) => {
                    crash::mark_seen(&dir);
                    self.crash_report = None;
                }
                None => {}
            }
        }
    }

    fn show_in_folder(&mut self, folder: &Path) {
        if let Err(e) = os::show_in_folder(folder) {
            self.shell
                .report(tr!("error-show-in-folder", error = e.to_string()));
        }
    }

    /// Whether an open dialog takes every key. The Preferences dialog does: its Keyboard
    /// Shortcuts page records them.
    fn dialog_has_keys(&self) -> bool {
        self.closing.ask().is_some()
            || self.new_project.is_some()
            || self.settings.has_keys()
            || self.palette.is_some()
            || self.params_dialog.is_some()
            || matches!(
                self.schematic_dialog,
                Some(ref d) if !matches!(d, SchematicDialog::Find(_))
            )
    }

    /// Runs what the Keymap binds to the keys pressed this frame. Text fields and dialogs
    /// take keys first: nothing runs while a text field has the keyboard or the New Project
    /// dialog is open, and while a Project dialog is open (or a Panel or settings window has
    /// the focus) only global keys apply. A focused button keeps the plain keys (Space
    /// presses it), but chords with a modifier and the function keys (F6, Ctrl+Tab) still
    /// apply.
    fn keys(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() || self.dialog_has_keys() {
            return;
        }
        // The Projects panel's dialogs. A settings window picks its stack by its focus.
        let dialog = projects_panel::dialog_open(ctx).then_some(Scope::Project);
        // Escape and Enter go to the tab switcher while it is open.
        tab_switcher::keys(ctx, &mut self.shell);
        for (id, params) in keymap::resolve_keys(ctx, &self.shell, dialog) {
            if id == "selection.cycle" {
                ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            }
            self.run_held(&id, params);
        }
    }

    /// The open settings windows.
    pub fn settings_windows(&self) -> &SettingsWindows {
        &self.settings
    }

    /// Shows the settings window `window` once [`SettingsWindows`] opened it: a window
    /// that was open already is raised.
    fn opened(&mut self, was_open: Option<bool>, window: SettingsWindow) {
        if was_open == Some(true) {
            self.raise = Some(window);
        }
    }

    /// Opens the Project Settings window on the open Project, if there is one.
    pub fn open_project_settings(&mut self) {
        let window = SettingsWindow::ProjectSettings;
        let was_open = self.settings.open(&mut self.shell, window);
        self.opened(was_open, window);
    }

    /// Opens the Board Setup window on the open Project's PCB Document, if there is one.
    pub fn open_board_setup(&mut self) {
        let window = SettingsWindow::BoardSetup;
        let was_open = self.settings.open(&mut self.shell, window);
        self.opened(was_open, window);
    }

    /// Opens the Change List window (Update PCB from Schematic) on the open Project; with
    /// no PCB Document, the status bar says why not.
    pub fn open_change_list(&mut self) {
        match self.shell.engine().project() {
            Some(project) if project.pcb().is_some() => {
                let window = SettingsWindow::ChangeList;
                let was_open = self.settings.open(&mut self.shell, window);
                self.opened(was_open, window);
            }
            Some(_) => self.shell.report(reda_engine::sync::NO_PCB.to_owned()),
            None => {}
        }
    }

    /// Opens the Output Job window on `job`, if the open Project has it. Showing another
    /// job asks first when the window has unapplied changes.
    pub fn open_output_job(&mut self, job: reda_engine::model::Uuid) {
        let window = SettingsWindow::OutputJob;
        if self.settings.job().is_some_and(|open| open != job)
            && self.settings.has_unapplied(window)
        {
            self.closing
                .hold_settings(&self.shell, window, Then::OpenOutputJob(job));
            return;
        }
        let was_open = self.settings.open_job(&mut self.shell, job);
        self.opened(was_open, window);
    }

    /// Carries out what the settings windows asked for, closes the state of those whose
    /// tabs closed, and raises one opened again.
    fn settings_requests(&mut self, ctx: &egui::Context) {
        for request in self.settings.take_requests() {
            match request {
                SettingsRequest::Close(window) => self.shell.close_settings(window),
                SettingsRequest::PickFolder(window) => {
                    self.files.start(ctx, Pick::SettingsFolder(window));
                }
                SettingsRequest::ShowFolder(folder) => {
                    let _ = std::fs::create_dir_all(&folder);
                    self.show_in_folder(&folder);
                }
            }
        }
        let now = ctx.input(|i| i.time);
        self.settings.follow_layout(&self.shell, now);
        if let Some(window) = self.raise.take()
            && let Some(id) = self.shell.layout().window_of(&TabKey::Settings(window))
        {
            let viewport = docking::viewport_of(id);
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Focus);
        }
        // A window's place waits a moment before it is written.
        if self.settings.places_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// Opens the Preferences window on `page` (where it is, when open; its General page
    /// when it opens with `None`).
    pub fn open_preferences(&mut self, page: Option<PageId>) {
        let was_open = self.settings.open_preferences(&mut self.shell, page);
        self.opened(Some(was_open), SettingsWindow::Preferences);
    }

    pub fn preferences_dialog(&self) -> Option<&PreferencesDialog> {
        self.settings.preferences()
    }

    fn open_palette(&mut self) {
        let shell = &self.shell;
        let shortcut = |id: &str| (self.shortcuts)(shell, id);
        let entries = palette::entries(shell, &shortcut);
        self.palette = Some(Palette::new(entries));
    }

    fn show_palette(&mut self, ctx: &egui::Context) {
        let Some(open) = &mut self.palette else {
            return;
        };
        match open.show(ctx) {
            PaletteOutcome::Open => {}
            PaletteOutcome::Close => self.palette = None,
            PaletteOutcome::Run(id) => {
                self.palette = None;
                self.call(&id, json!({}));
                self.handle_requests(ctx);
            }
        }
    }

    /// Notices another instance's change to `preferences.toml`, and applies the theme and
    /// UI scale whenever the Preferences change.
    fn apply_preferences(&mut self, ctx: &egui::Context) {
        if self.watch_preferences
            && self.preferences_watcher.is_none()
            && let Some(dirs) = self.shell.engine().user_dirs()
        {
            let waker = ctx.clone();
            self.preferences_watcher = Some(Watcher::start(dirs.preferences_file(), move || {
                waker.request_repaint()
            }));
        }
        self.shell.poll_preferences();
        let generation = self.shell.preferences_generation();
        if self.applied_preferences == Some(generation) {
            return;
        }
        let prefs = self.shell.preferences();
        ctx.set_theme(theme::preference(prefs));
        let scale = prefs.general.ui_scale as f32;
        if (ctx.zoom_factor() - scale).abs() > f32::EPSILON {
            ctx.set_zoom_factor(scale);
        }
        self.applied_preferences = Some(generation);
    }

    /// Calls a Command from the GUI: it runs when its params are complete, else its
    /// params dialog opens.
    fn call(&mut self, id: &str, params: serde_json::Value) {
        if !self.closing.hold(&self.shell, &self.settings, id, &params) {
            return;
        }
        if let Call::Dialog(dialog) = self.dialogs.call(&mut self.shell, id, params) {
            self.params_dialog = Some(dialog);
        }
    }

    /// Relinks the missing `document` to the file at `path`, as picked in the file
    /// dialog. A file that holds another Document asks first.
    pub fn relink(&mut self, document: Uuid, path: &Path) {
        let params = json!({ "document": document, "path": path });
        match self.shell.run("document.relink", params) {
            Err(e) if e.details.as_ref().is_some_and(|d| d["confirm"] == true) => {
                self.relink_confirm = Some((document, path.to_path_buf()));
            }
            Err(e) => self.shell.report(tr!("error-command", error = e.message)),
            Ok(_) => self.shell.dismiss_message(),
        }
    }

    /// Asks whether to relink to a file that holds another Document.
    fn show_relink_confirm(&mut self, ctx: &egui::Context) {
        let Some((document, path)) = self.relink_confirm.clone() else {
            return;
        };
        let name = (self.shell.engine().project())
            .and_then(|p| p.missing_document(document))
            .map(|e| e.title.clone());
        let Some(name) = name else {
            self.relink_confirm = None;
            return;
        };
        let file = reda_engine::fs::display_path(&path);
        let (mut relink, mut close) = (false, false);
        let modal = crate::modal::Dialog::new(
            egui::Id::NULL.with("relink-confirm"),
            tr!("relink-confirm-title"),
        )
        .width(380.0)
        .show(ctx, |ui| {
            ui.label(tr!(
                "relink-confirm",
                file = file.as_str(),
                name = name.as_str()
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                relink = ui.button(tr!("relink-anyway")).clicked();
                close = ui.button(tr!("dialog-cancel")).clicked();
            });
        });
        if relink {
            let params = json!({ "document": document, "path": path, "confirm": true });
            let _ = self.shell.run_reporting("document.relink", params);
        }
        if relink || close || modal.should_close() {
            self.relink_confirm = None;
        }
    }

    fn show_params_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.params_dialog else {
            return;
        };
        match dialog.show(ctx, &mut self.shell) {
            command_dialog::Outcome::Open => {}
            command_dialog::Outcome::Cancel => self.params_dialog = None,
            command_dialog::Outcome::Run(params) => {
                let id = dialog.command().to_owned();
                if !self.closing.hold(&self.shell, &self.settings, &id, &params) {
                    // It runs once the closing prompt is answered.
                    self.params_dialog = None;
                    return;
                }
                match self.shell.run_reporting(&id, params) {
                    Ok(out) => {
                        if let Some((next, params)) = dialog.then(&out) {
                            // Errors reach the status bar; the first Command's work stays.
                            let _ = self.shell.run_reporting(&next, params);
                        }
                        self.params_dialog = None;
                        self.handle_requests(ctx);
                    }
                    Err(e) => dialog.failed(&e),
                }
            }
        }
    }

    /// Shows the notice the Shell has, such as the first Import part's vendor-terms
    /// notice, until OK.
    fn show_notice(&mut self, ctx: &egui::Context) {
        if self.notice.is_none() {
            self.notice = self.shell.take_notice();
        }
        let Some(notice) = &self.notice else {
            return;
        };
        let mut close = false;
        let modal =
            crate::modal::Dialog::new(egui::Id::NULL.with("notice"), tr!("import-notice-title"))
                .width(420.0)
                .show(ctx, |ui| {
                    ui.label(notice);
                    ui.add_space(8.0);
                    close = ui.button(tr!("notice-ok")).clicked();
                });
        if close || modal.should_close() {
            self.notice = None;
        }
    }

    fn open_new_project(&mut self) {
        let (presets, default) = self.shell.fab_preset_choices();
        self.new_project = Some(NewProjectForm::new(presets, default));
    }

    fn collect_picked_files(&mut self) {
        match self.files.poll() {
            Some((Pick::OpenProject, path)) => self.open_project(&path),
            Some((Pick::ExportJournal, path)) => {
                let _ = self.shell.export_journal(&path);
            }
            Some((Pick::ExportPdf, path)) => {
                let _ = schematic_dialogs::export_pdf(&mut self.shell, &path);
            }
            Some((Pick::NewProjectLocation, path)) => {
                if let Some(form) = &mut self.new_project {
                    form.location = path.display().to_string();
                }
            }
            Some((Pick::LibraryFolder, path)) => {
                if let Some(folder) = self.shell.engine().location() {
                    let relative = os::relative_path(folder, &path);
                    let path = relative.to_string_lossy().replace('\\', "/");
                    let _ = self
                        .shell
                        .run_reporting("project.library.add", json!({ "path": path }));
                }
            }
            Some((Pick::SettingsFolder(window), path)) => {
                self.settings.picked_folder(&mut self.shell, window, &path);
            }
            Some((Pick::Relink(document, _), path)) => self.relink(document, &path),
            // The Import part dialog runs its own pickers.
            Some((Pick::ImportSymbol | Pick::ImportFootprint, _)) | None => {}
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match self.shell.engine().project() {
            Some(p) => tr!("window-title-project", project = p.props().name.as_str()),
            None => tr!("window-title"),
        };
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    /// The top row for the active Document's type, drawn from the bar data files. Returns
    /// the item clicked, if any.
    fn bars(&mut self, ui: &mut Ui) -> Option<Invocation> {
        let bars = bars::for_document(bars::active_type(&self.shell));
        let mut checked =
            BTreeMap::from([("view.theme.set", json!({ "theme": theme_value(ui.ctx()) }))]);
        // The cross-probe switches show checked while on.
        let probe = self.shell.cross_probe_switches();
        if probe.selection {
            checked.insert("view.cross_select.toggle", json!({}));
        }
        if probe.nets {
            checked.insert("view.net_cross_probe.toggle", json!({}));
        }
        if let Some(document) = self.shell.active_document().map(|d| d.document) {
            let grid = self.shell.grid(document).0;
            checked.insert(
                "view.grid.set",
                json!({ "document": document.to_string(), "grid": grid }),
            );
        }
        let cx = draw::Cx {
            shell: &self.shell,
            checked: &checked,
            document: self.shell.active_document().map(|d| d.document),
        };
        Panel::top("top-row")
            .show(ui, |ui| draw::top_row(ui, &cx, bars))
            .inner
    }

    /// Carries out what the view Commands asked of the window.
    fn handle_requests(&mut self, ctx: &egui::Context) {
        for request in bars::take_requests(self.shell.engine()) {
            match request {
                Request::NewProject => self.open_new_project(),
                Request::CommandPalette => self.open_palette(),
                Request::OpenProject => self.files.start(ctx, Pick::OpenProject),
                Request::Quit => self.quit(ctx),
                Request::ZoomFit(document) => {
                    if let Some(tab) = self.shell.document_tab_mut(document) {
                        tab.camera = None;
                    }
                }
                Request::ZoomSelection(document) => {
                    self.shell.zoom_to_selection(document);
                }
                Request::JumpTo(document, jump) => self.shell.jump(document, jump),
                Request::Preferences => self.open_preferences(None),
                Request::Theme(choice) => ctx.set_theme(match choice {
                    preferences::Theme::FollowOs => ThemePreference::System,
                    preferences::Theme::Dark => ThemePreference::Dark,
                    preferences::Theme::Light => ThemePreference::Light,
                }),
                Request::About => self.help.open_about(),
                Request::Schematic(DialogKind::ExportPdf) => {
                    let name = schematic_dialogs::pdf_file_name(&self.shell);
                    self.files.start_save(ctx, Pick::ExportPdf, name);
                }
                Request::Schematic(kind) => {
                    self.schematic_dialog = SchematicDialog::open(kind, &mut self.shell);
                }
                Request::ChangeList => self.open_change_list(),
                Request::ProjectSettings => self.open_project_settings(),
                Request::BoardSetup => self.open_board_setup(),
                Request::KeyboardShortcuts => {
                    self.open_preferences(Some(PageId::KEYBOARD_SHORTCUTS))
                }
            }
        }
    }

    /// Draws the status bar; returns the rect its row took.
    fn status_bar(&mut self, ui: &mut Ui) -> egui::Rect {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.panels_button(ui);
                if bridge_ui::indicator(ui, &mut self.bridge, &mut self.shell) {
                    self.open_preferences(Some(PageId::AGENTS));
                }
                let pending = self.shell.out_of_sync();
                if pending > 0 {
                    let badge = egui::RichText::new(tr!("status-out-of-sync", count = pending))
                        .color(ui.visuals().warn_fg_color);
                    if ui
                        .small_button(badge)
                        .on_hover_text(tr!("status-out-of-sync-hint"))
                        .clicked()
                    {
                        self.open_change_list();
                    }
                }
                if self.shell.zones_need_refill() {
                    if ui.small_button(tr!("status-zones-refill")).clicked() {
                        let _ = self.shell.run_reporting("pcb.zone.fill", json!({}));
                    }
                    ui.colored_label(ui.visuals().warn_fg_color, tr!("status-zones-stale"));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // Active: <kind> | X … Y … | Grid: …, then any message or note.
                    let readout = crate::status_bar::readout(&self.shell);
                    let parts = [readout.kind, readout.cursor, readout.grid];
                    let mut first = true;
                    for part in parts.into_iter().flatten() {
                        if !first {
                            ui.separator();
                        }
                        ui.label(part);
                        first = false;
                    }
                    if !first && (self.shell.message().is_some() || self.shell.note().is_some()) {
                        ui.separator();
                    }
                    self.status_message(ui, first);
                });
            });
        })
        .response
        .rect
    }

    /// The status bar's error or note with its Dismiss button, or "Ready" when the bar
    /// shows nothing else (`idle`).
    fn status_message(&mut self, ui: &mut Ui, idle: bool) {
        match (self.shell.message(), self.shell.note()) {
            (Some(message), _) => {
                let message = message.to_owned();
                ui.colored_label(ui.visuals().error_fg_color, message);
                if ui.small_button(tr!("status-dismiss")).clicked() {
                    self.shell.dismiss_message();
                }
            }
            (None, Some(note)) => {
                ui.label(note.to_owned());
                if ui.small_button(tr!("status-dismiss")).clicked() {
                    self.shell.dismiss_note();
                }
            }
            (None, None) => {
                if idle {
                    ui.weak(tr!("status-ready"));
                }
            }
        }
    }

    /// The Panels button: shows any Panel, reopening a closed one in its usual Dock.
    fn panels_button(&mut self, ui: &mut Ui) {
        ui.menu_button(tr!("status-panels"), |ui| {
            let closed = self.shell.layout().closed_panels();
            for panel in PanelId::ALL {
                let title = panel_title(panel);
                let label = if closed.contains(&panel) {
                    tr!("panels-reopen", panel = title.as_str())
                } else {
                    title
                };
                if ui.button(label).clicked() {
                    let _ = self
                        .shell
                        .run_reporting(&format!("panel.show:{}", panel.id()), json!({}));
                    ui.close();
                }
            }
        });
    }

    fn start_action(&mut self, ctx: &egui::Context, action: StartAction) {
        match action {
            StartAction::NewProject => self.open_new_project(),
            StartAction::OpenProject => self.files.start(ctx, Pick::OpenProject),
            StartAction::OpenRecent(path) => self.open_project(&path),
            StartAction::RemoveRecent(path) => self.shell.remove_recent(&path),
        }
    }

    /// Frees the GPU state of canvases whose tabs closed.
    fn drop_closed_canvases(&self) {
        let Some(render_state) = &self.render_state else {
            return;
        };
        let keep: Vec<egui::Id> = self.shell.document_tabs().iter().map(canvas_id).collect();
        if let Some(res) = render_state
            .renderer
            .write()
            .callback_resources
            .get_mut::<CanvasResources>()
        {
            res.retain(&keep);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        App::ui(self, ui);
    }

    /// The window closes: the view state is saved, and the open Project's Recovery
    /// Snapshots go only when the close got past the closing prompt (or nothing was
    /// unsaved). A close that skipped the prompt keeps them for the next start to offer.
    fn on_exit(&mut self) {
        self.shell.save_view();
        if self.closing.may_clear_snapshots(&self.shell) {
            self.shell.close_cleanly();
        } else {
            self.shell.flush_recovery();
        }
    }
}

/// The `general.theme` value of the theme the window shows.
fn theme_value(ctx: &egui::Context) -> &'static str {
    match ctx.options(|o| o.theme_preference) {
        ThemePreference::System => "follow_os",
        ThemePreference::Dark => "dark",
        ThemePreference::Light => "light",
    }
}

/// Makes the canvas pipelines for a window drawn with `render_state`. The window (or a
/// test harness) must call this once before the first canvas paints.
pub fn install_canvas(render_state: &egui_wgpu::RenderState, sample_count: u32) {
    let resources = CanvasResources::new(
        &render_state.device,
        render_state.target_format,
        sample_count,
    );
    render_state
        .renderer
        .write()
        .callback_resources
        .insert(resources);
}

/// Opens the REDA window on `engine` and runs until it closes. `fonts` supplies installed
/// fonts on demand; `open` is a Project to open at once. Without one, the last Project
/// reopens when the Preferences ask for it.
pub fn run(
    engine: Engine,
    fonts: Arc<dyn InstalledFonts>,
    open: Option<PathBuf>,
) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(tr!("window-title"))
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([480.0, 320.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "REDA",
        options,
        Box::new(move |cc| {
            let render_state = cc
                .wgpu_render_state
                .as_ref()
                .ok_or("REDA draws with wgpu, but the window has no wgpu renderer")?;
            install_canvas(render_state, 1);
            let mut shell = Shell::new(engine).with_fonts(fonts);
            match open {
                Some(path) => {
                    let _ = shell.open_project(&path);
                }
                None => shell.reopen_last_project(),
            }
            // After the Shell, so the catalogue agents see has the app's own Commands.
            let bridge = Bridge::start(&shell);
            Ok(Box::new(
                App::new(shell)
                    .with_render_state(render_state.clone())
                    .with_bridge(bridge)
                    .watching_preferences()
                    .with_shortcuts(keymap::shortcut_of),
            ))
        }),
    )
}
