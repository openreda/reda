//! The window: menu bar, the docking layout (Docks, tab groups with the Start page,
//! canvases and Panels), and the status bar with the Panels button. Drawing only; the state
//! lives in [`Shell`].

use std::path::PathBuf;
use std::sync::Arc;

use egui::{Key, Modifiers, Panel, ThemePreference, Ui};
use reda_engine::Engine;
use reda_engine::preferences;
use serde_json::json;

use crate::canvas::{CanvasResources, canvas_id};
use crate::dialogs::{FileDialogs, Pick};
use crate::docking::{self, panel_title};
use crate::layout::PanelId;
use crate::new_project::{FormAction, NewProjectForm};
use crate::os;
use crate::projects_panel::{self, ProjectsAction};
use crate::settings::{self, Outcome, SettingsDialog};
use crate::shell::{InstalledFonts, Shell};
use crate::start_page::StartAction;
use crate::{theme, tr};

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
}

impl App {
    /// The window on `shell`. REDA's theme is installed on the first frame, as the
    /// Preferences ask.
    pub fn new(shell: Shell) -> App {
        App {
            shell,
            files: FileDialogs::default(),
            new_project: None,
            settings: None,
            window_title: String::new(),
            render_state: None,
            theme_installed: false,
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
            self.theme_installed = true;
        }
        self.collect_picked_files();
        self.update_title(&ctx);
        self.undo_keys(&ctx);

        Panel::top("menu-bar").show(ui, |ui| self.menu_bar(ui));
        Panel::bottom("status-bar").show(ui, |ui| self.status_bar(ui));
        let frame = docking::show(ui, &mut self.shell);
        for (id, params) in frame.commands {
            let _ = self.shell.run_reporting(id, params);
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
        self.drop_closed_canvases();

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

    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tr!("menu-file"), |ui| {
                if ui.button(tr!("menu-file-new-project")).clicked() {
                    self.open_new_project();
                }
                if ui.button(tr!("menu-file-open-project")).clicked() {
                    self.files.start(ui.ctx(), Pick::OpenProject);
                }
                let open = self.shell.engine().project().is_some();
                if ui
                    .add_enabled(open, egui::Button::new(tr!("menu-file-save-all")))
                    .clicked()
                {
                    let _ = self.shell.run("project.save", json!({}));
                }
                ui.separator();
                if ui.button(tr!("menu-file-quit")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(tr!("menu-view"), |ui| {
                if ui.button(tr!("menu-view-zoom-fit")).clicked()
                    && let Some(tab) = self.shell.active_document_mut()
                {
                    tab.camera = None;
                }
                ui.menu_button(tr!("menu-view-theme"), |ui| {
                    let current = ui.ctx().options(|o| o.theme_preference);
                    for (choice, label, value) in [
                        (ThemePreference::System, tr!("theme-follow-os"), "follow_os"),
                        (ThemePreference::Dark, tr!("theme-dark"), "dark"),
                        (ThemePreference::Light, tr!("theme-light"), "light"),
                    ] {
                        if ui.radio(current == choice, label).clicked() {
                            ui.ctx().set_theme(choice);
                            // Kept as the Preference when there is a per-user folder.
                            let _ = self.shell.run(
                                "app.preferences.set",
                                json!({"key": "general.theme", "value": value}),
                            );
                        }
                    }
                });
            });
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.panels_button(ui);
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| match self.shell.message() {
                        Some(message) => {
                            let message = message.to_owned();
                            ui.colored_label(ui.visuals().error_fg_color, message);
                            if ui.small_button(tr!("status-dismiss")).clicked() {
                                self.shell.dismiss_message();
                            }
                        }
                        None => {
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
/// fonts on demand; `open` is a Project to open at once.
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
            if let Some(path) = open {
                let _ = shell.open_project(&path);
            }
            Ok(Box::new(
                App::new(shell).with_render_state(render_state.clone()),
            ))
        }),
    )
}
