//! The window: menu bar, the docking layout (Docks, tab groups with the Start page,
//! canvases and Panels), and the status bar with the Panels button. Drawing only; the state
//! lives in [`Shell`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use egui::{Key, Modifiers, Panel, ThemePreference, Ui};
use reda_engine::Engine;
use reda_engine::preferences;
use serde_json::json;

use crate::agents as bridge_ui;
use crate::bars::draw::{self, Invocation};
use crate::bars::{self, Request};
use crate::bridge::Bridge;
use crate::canvas::{CanvasResources, canvas_id};
use crate::dialogs::{FileDialogs, Pick};
use crate::docking::{self, panel_title};
use crate::journal_panel::JournalAction;
use crate::layout::PanelId;
use crate::new_project::{FormAction, NewProjectForm};
use crate::os;
use crate::projects_panel::{self, ProjectsAction};
use crate::recovery_dialog::{self, CrashAction, RecoveryAction};
use crate::settings::{self, Outcome, SettingsDialog};
use crate::shell::{InstalledFonts, Shell};
use crate::start_page::StartAction;
use crate::{crash, theme, tr};

/// The REDA window.
pub struct App {
    shell: Shell,
    files: FileDialogs,
    new_project: Option<NewProjectForm>,
    /// The open settings dialog (Project Settings), if any.
    settings: Option<SettingsDialog>,
    window_title: String,
    /// The window's renderer, to free closed canvases' GPU state.
    render_state: Option<egui_wgpu::RenderState>,
    theme_installed: bool,
    /// A Crash Report from an earlier run, not yet offered.
    crash_report: Option<PathBuf>,
    /// The control port, for agents.
    bridge: Bridge,
    bridge_waker_set: bool,
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
            window_title: String::new(),
            render_state: None,
            theme_installed: false,
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
            let prefs = preferences::current(self.shell.engine().user_dirs());
            theme::install(&ctx, theme::preference(&prefs));
            crate::icons::install_loaders(&ctx);
            self.theme_installed = true;
        }
        self.pump_bridge(&ctx);
        self.collect_picked_files();
        self.update_title(&ctx);
        self.undo_keys(&ctx);

        let clicked = self.bars(ui);
        Panel::bottom("status-bar").show(ui, |ui| self.status_bar(ui));
        let frame = docking::show(ui, &mut self.shell);
        for (id, params) in clicked.into_iter().chain(frame.commands) {
            let _ = self.shell.run_reporting(&id, params);
        }
        self.handle_requests(&ctx);
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
            // The Output Job dialog is still to come.
            Some(ProjectsAction::EditJob(_)) | None => {}
        }
        self.show_settings(&ctx);
        if let Some(JournalAction::Export) = frame.journal {
            self.files.start(&ctx, Pick::ExportJournal);
        }
        self.drop_closed_canvases();
        bridge_ui::ask(&ctx, &mut self.bridge, &mut self.shell);
        self.shell.save_view_if_due(std::time::Instant::now());
        self.crash_recovery(&ctx);

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

    /// Mod+Z undoes and Mod+Shift+Z (or Mod+Y) redoes, on the stack focus picks, unless a
    /// text field has the keyboard.
    fn undo_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.new_project.is_some() {
            return;
        }
        let (redo, undo) = ctx.input_mut(|i| {
            let redo = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                || i.consume_key(Modifiers::COMMAND, Key::Y);
            (redo, i.consume_key(Modifiers::COMMAND, Key::Z))
        });
        if undo || redo {
            let dialog = projects_panel::dialog_open(ctx) || self.settings.is_some();
            let _ = self.shell.undo(redo, dialog);
        }
    }

    /// Opens the Project Settings dialog on the open Project, if there is one.
    pub fn open_project_settings(&mut self) {
        if let Some(project) = self.shell.engine().project() {
            self.settings = Some(settings::project::dialog(project));
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
        let checked =
            BTreeMap::from([("view.theme.set", json!({ "theme": theme_value(ui.ctx()) }))]);
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
                Request::OpenProject => self.files.start(ctx, Pick::OpenProject),
                Request::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Request::ZoomFit(document) => {
                    if let Some(tab) = self.shell.document_tab_mut(document) {
                        tab.camera = None;
                    }
                }
                Request::Theme(choice) => ctx.set_theme(match choice {
                    preferences::Theme::FollowOs => ThemePreference::System,
                    preferences::Theme::Dark => ThemePreference::Dark,
                    preferences::Theme::Light => ThemePreference::Light,
                }),
            }
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.panels_button(ui);
                bridge_ui::indicator(ui, &mut self.bridge, &mut self.shell);
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
        });
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
                    .with_bridge(bridge),
            ))
        }),
    )
}
