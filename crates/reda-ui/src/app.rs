//! The window: menu bar, the docking layout (Docks, tab groups with the Start page,
//! canvases and Panels), and the status bar with the Panels button. Drawing only; the state
//! lives in [`Shell`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use egui::{Panel, ThemePreference, Ui, UiBuilder};
use reda_engine::Engine;
use reda_engine::preferences;
use serde_json::json;

use crate::agents as bridge_ui;
use crate::bars::draw::{self, Invocation};
use crate::bars::{self, Request};
use crate::bridge::Bridge;
use crate::canvas::{CanvasResources, canvas_id};
use crate::command_dialog::{self, Call, Dialogs, ParamsDialog};
use crate::dialogs::{FileDialogs, Pick};
use crate::docking::{self, panel_title};
use crate::job_run::JobRuns;
use crate::journal_panel::JournalAction;
use crate::layout::{PanelId, TabKey};
use crate::new_project::{FormAction, NewProjectForm};
use crate::os;
use crate::palette::{self, Palette, PaletteOutcome};
use crate::preferences_dialog::{Outcome as PreferencesOutcome, PageId, PreferencesDialog};
use crate::projects_panel::{self, ProjectsAction};
use crate::recovery_dialog::{self, CrashAction, RecoveryAction};
use crate::schematic_dialogs::{self, DialogKind, SchematicDialog};
use crate::settings::{self, Outcome, SettingsDialog};
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
    /// The open settings dialog (Project Settings, Board Setup or an Output Job), if any.
    settings: Option<SettingsDialog>,
    /// The running Output Job, or the report of the last run.
    jobs: JobRuns,
    /// The open Preferences dialog, if any.
    preferences: Option<PreferencesDialog>,
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
    /// The open schematic dialog (Find, Re-annotate…), if any.
    schematic_dialog: Option<SchematicDialog>,
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
            settings: None,
            jobs: JobRuns::default(),
            preferences: None,
            applied_preferences: None,
            preferences_watcher: None,
            watch_preferences: false,
            window_title: String::new(),
            render_state: None,
            theme_installed: false,
            help: Default::default(),
            palette: None,
            dialogs: {
                let mut dialogs = Dialogs::new();
                schematic_dialogs::register_dialogs(&mut dialogs);
                dialogs
            },
            schematic_dialog: None,
            params_dialog: None,
            shortcuts: Box::new(|_, _| None),
            bridge: Bridge::off(),
            bridge_waker_set: false,
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
        let frame = docking::show(ui, &mut self.shell);
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
            let _ = self.shell.run_reporting(&id, params);
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
            None => {}
        }
        self.show_settings(&ctx);
        if let Some(dialog) = &mut self.settings {
            self.jobs.run_from_dialog(&ctx, &mut self.shell, dialog);
        }
        self.jobs.show(&ctx, &mut self.shell);
        self.show_preferences(&ctx);
        if let Some(JournalAction::Export) = frame.journal {
            self.files.start(&ctx, Pick::ExportJournal);
        }
        self.drop_closed_canvases();
        bridge_ui::ask(&ctx, &mut self.bridge, &mut self.shell);
        self.shell.save_view_if_due(std::time::Instant::now());
        self.crash_recovery(&ctx);
        self.show_palette(&ctx);
        self.show_params_dialog(&ctx);
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
                    if let Some(params) = form.params()
                        && self.shell.run_reporting("project.new", params).is_ok()
                    {
                        self.new_project = None;
                        self.drop_closed_canvases();
                    }
                }
            }
        }
        a11y::focus_ring(&ctx);
    }

    /// Takes due Recovery Snapshots, and shows the recovery prompt and the Crash Report
    /// offer when there is one.
    fn crash_recovery(&mut self, ctx: &egui::Context) {
        if let Some(wait) = self.shell.tick_recovery(Instant::now()) {
            ctx.request_repaint_after(wait);
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

    /// Runs what the Keymap binds to the keys pressed this frame. Text fields and dialogs
    /// take keys first: nothing runs while a text field has the keyboard or the New Project
    /// dialog is open, and while a Project dialog is open only global keys apply, with
    /// undo on the Project stack. A focused button keeps the plain keys (Space presses
    /// it), but chords with a modifier and the function keys (F6, Ctrl+Tab) still apply.
    fn keys(&mut self, ctx: &egui::Context) {
        // The Preferences dialog takes every key: its Keyboard Shortcuts page records them.
        if ctx.text_edit_focused()
            || self.new_project.is_some()
            || self.preferences.is_some()
            || self.palette.is_some()
            || self.params_dialog.is_some()
            || matches!(
                self.schematic_dialog,
                Some(ref d) if !matches!(d, SchematicDialog::Find(_))
            )
        {
            return;
        }
        // The Project dialogs: the Projects panel's and the settings dialog.
        let dialog = projects_panel::dialog_open(ctx) || self.settings.is_some();
        let active = if dialog {
            keymap::Active::default()
        } else {
            let mut active = keymap::active(&self.shell);
            // A focused Panel keeps the canvas's keys (Del, Mod+C, the Tools') off it.
            if matches!(self.shell.layout().focus, Some(TabKey::Panel(_))) {
                active = keymap::Active::default();
            }
            active
        };
        // Escape and Enter go to the tab switcher while it is open.
        tab_switcher::keys(ctx, &mut self.shell);
        let widget_focused = ctx.egui_wants_keyboard_input();
        let actions = ctx.input_mut(|i| {
            let wanted = |chord: &keymap::Chord| !widget_focused || chord.passes_focused_widgets();
            self.shell.keymap().take_where(i, &active, wanted)
        });
        let document = self.shell.active_document().map(|d| d.document);
        for action in actions {
            // A key whose Command cannot run now does nothing, as its greyed menu item.
            if let Ok((id, params)) = bars::resolve_in(&self.shell, &action, document, dialog).run {
                let _ = self.shell.run_reporting(&id, params);
            }
        }
    }

    /// Opens the Project Settings dialog on the open Project, if there is one.
    pub fn open_project_settings(&mut self) {
        if let Some(project) = self.shell.engine().project() {
            self.settings = Some(settings::project::dialog(project));
        }
    }

    /// Opens the Board Setup dialog on the open Project's PCB Document, if there is one.
    pub fn open_board_setup(&mut self) {
        if let Some(project) = self.shell.engine().project()
            && project.pcb().is_some()
        {
            self.settings = Some(settings::board_setup::dialog(project));
        }
    }

    /// Opens the Change List dialog (Update PCB from Schematic) on the open Project; with
    /// no PCB Document, the status bar says why not.
    pub fn open_change_list(&mut self) {
        match self.shell.engine().project() {
            Some(project) if project.pcb().is_some() => {
                self.settings = Some(settings::change_list::dialog(project));
            }
            Some(_) => self.shell.report(reda_engine::sync::NO_PCB.to_owned()),
            None => {}
        }
    }

    /// Opens the Output Job dialog on `job`, if the open Project has it.
    pub fn open_output_job(&mut self, job: reda_engine::model::Uuid) {
        if let Some(project) = self.shell.engine().project() {
            self.settings = settings::output_job::dialog(project, job);
        }
    }

    /// Draws the open settings dialog, and closes it or opens its folder picker as it asks.
    fn show_settings(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.settings else {
            return;
        };
        match dialog.show(ctx, &mut self.shell) {
            Outcome::Open => {}
            Outcome::Closed => self.settings = None,
            Outcome::PickFolder => self.files.start(ctx, Pick::SettingsFolder),
        }
    }

    /// Opens the Preferences dialog on `page` (its General page with `None`).
    pub fn open_preferences(&mut self, page: Option<PageId>) {
        self.preferences = Some(PreferencesDialog::on(page.unwrap_or(PageId::General)));
    }

    pub fn preferences_dialog(&self) -> Option<&PreferencesDialog> {
        self.preferences.as_ref()
    }

    /// Draws the open Preferences dialog and does what it asks of the window.
    fn show_preferences(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.preferences else {
            return;
        };
        match dialog.show(ctx, &mut self.shell, &mut self.bridge) {
            PreferencesOutcome::Open => {}
            PreferencesOutcome::Closed => self.preferences = None,
            PreferencesOutcome::PickFolder => self.files.start(ctx, Pick::PreferencesFolder),
            PreferencesOutcome::ShowFolder(folder) => {
                let _ = std::fs::create_dir_all(&folder);
                self.show_in_folder(&folder);
            }
        }
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
        if let Call::Dialog(dialog) = self.dialogs.call(&mut self.shell, id, params) {
            self.params_dialog = Some(dialog);
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
                match self.shell.run_reporting(&id, params) {
                    Ok(_) => {
                        self.params_dialog = None;
                        self.handle_requests(ctx);
                    }
                    Err(e) => dialog.failed(&e),
                }
            }
        }
    }

    fn open_new_project(&mut self) {
        let (presets, default) = self.shell.fab_preset_choices();
        self.new_project = Some(NewProjectForm::new(presets, default));
    }

    fn collect_picked_files(&mut self) {
        match self.files.poll() {
            Some((Pick::OpenProject, path)) => {
                let _ = self.shell.open_project(&path);
                self.drop_closed_canvases();
            }
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
            Some((Pick::PreferencesFolder, path)) => {
                if let Some(dialog) = &mut self.preferences {
                    dialog.picked_folder(&mut self.shell, &path);
                }
            }
            Some((Pick::SettingsFolder, path)) => {
                if let (Some(folder), Some(dialog)) =
                    (self.shell.engine().location(), &mut self.settings)
                {
                    dialog.picked_folder(&os::relative_path(folder, &path));
                }
            }
            None => {}
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

    /// The menu bar and toolbar for the active Document's type, drawn from the bar data
    /// files. Returns the item clicked, if any.
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
        let menu = Panel::top("menu-bar")
            .show(ui, |ui| draw::menu_bar(ui, &cx, bars))
            .inner;
        let tool = Panel::top("toolbar")
            .show(ui, |ui| draw::toolbar(ui, &cx, bars))
            .inner;
        menu.or(tool)
    }

    /// Carries out what the view Commands asked of the window.
    fn handle_requests(&mut self, ctx: &egui::Context) {
        for request in bars::take_requests(self.shell.engine()) {
            match request {
                Request::NewProject => self.open_new_project(),
                Request::CommandPalette => self.open_palette(),
                Request::OpenProject => self.files.start(ctx, Pick::OpenProject),
                Request::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Request::ZoomFit(document) => {
                    if let Some(tab) = self.shell.document_tab_mut(document) {
                        tab.camera = None;
                    }
                }
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
            }
        }
    }

    /// Draws the status bar; returns the rect its row took.
    fn status_bar(&mut self, ui: &mut Ui) -> egui::Rect {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.panels_button(ui);
                if bridge_ui::indicator(ui, &mut self.bridge, &mut self.shell) {
                    self.open_preferences(Some(PageId::Agents));
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
                if let Some(document) = self.shell.active_document().map(|d| d.document) {
                    let grid = crate::grid::label(self.shell.grid(document));
                    ui.label(tr!("status-grid", grid = grid));
                }
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| match (self.shell.message(), self.shell.note()) {
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
                            ui.weak(tr!("status-ready"));
                        }
                    },
                );
            });
        })
        .response
        .rect
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
            StartAction::OpenRecent(path) => {
                let _ = self.shell.open_project(&path);
            }
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

    /// The window closes cleanly: the view state is saved and the open Project's
    /// Recovery Snapshots go.
    fn on_exit(&mut self) {
        self.shell.save_view();
        self.shell.close_cleanly();
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
