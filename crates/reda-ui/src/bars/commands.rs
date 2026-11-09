//! The view Commands behind menu items that act on the window rather than the design:
//! opening the New Project and Open Project dialogs and the About box, Quit, Zoom to Fit and the Theme.
//!
//! The window owns dialogs, cameras and the egui theme, so these Commands only queue a
//! [`Request`] in the engine's view state; the window takes the queue each frame with
//! [`take_requests`] and carries the requests out.

use std::sync::{Arc, Mutex};

use reda_engine::Engine;
use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, NoParams, Preset,
    Registry, Unavailable,
};
use reda_engine::model::Uuid;
use reda_engine::preferences::{self, Theme};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::layout::Layout;

/// Something only the window can do, asked for by a view Command.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Show the New Project form.
    NewProject,
    /// Show the OS dialog to pick a Project to open.
    OpenProject,
    /// Show the Palette, empty.
    Palette,
    /// Show the Palette on Commands (`>` typed in).
    CommandPalette,
    /// Close the window.
    Quit,
    /// Fit this Document's canvas to the Document.
    ZoomFit(Uuid),
    /// Zoom this Document's canvas to the selection.
    ZoomSelection(Uuid),
    /// Zoom this Document's canvas to these objects (`view.zoom_to`).
    ZoomTo(Uuid, Vec<Uuid>),
    /// Move this Document's canvas (`view.jump_to`).
    JumpTo(Uuid, crate::navigation::Jump),
    /// Show the window in this theme.
    Theme(Theme),
    /// Show the About box.
    About,
    /// Show the Preferences dialog.
    Preferences,
    /// Show a schematic dialog (Find, Re-annotate…).
    Schematic(crate::schematic_dialogs::DialogKind),
    /// Show the Change List dialog (Update PCB from Schematic).
    ChangeList,
    /// Show the Project Settings dialog.
    ProjectSettings,
    /// Show the Board Setup dialog.
    BoardSetup,
    /// Show Preferences on the Keyboard Shortcuts page.
    KeyboardShortcuts,
    /// Scale the user interface by this factor, one of the UI scale steps.
    UiScale(f64),
}

/// The UI scale in force, in the engine's view state: the step `view.ui_scale.step`
/// moves from. The Shell keeps it equal to the `general.ui_scale` Preference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiScale(pub f64);

impl Default for UiScale {
    fn default() -> UiScale {
        UiScale(1.0)
    }
}

/// The queue of [`Request`]s, in the engine's view state.
#[derive(Clone, Default)]
pub struct Requests(Arc<Mutex<Vec<Request>>>);

impl Requests {
    fn push(&self, request: Request) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(request);
    }
}

/// Adds the Commands to `registry`.
pub fn register_all(registry: &mut Registry) {
    registry.add(OpenDialog);
    registry.add(Quit);
    registry.add(ZoomFit);
    registry.add(ZoomSelection);
    registry.add(ZoomTo);
    registry.add(crate::navigation::JumpTo);
    registry.add(SetTheme);
    registry.add(OpenSettings);
    registry.add(SetUiScale);
    registry.add(StepUiScale);
}

/// Adds the Commands to `engine` and gives it a request queue. The Shell does this.
pub fn install(engine: &mut Engine) {
    if engine.registry().get(Quit::META.id).is_none() {
        engine.register(register_all);
    }
    engine.view_mut().set(Requests::default());
    engine.view_mut().set(UiScale::default());
}

/// Takes the requests queued since the last call, oldest first.
pub fn take_requests(engine: &Engine) -> Vec<Request> {
    engine
        .view()
        .get::<Requests>()
        .map(|r| std::mem::take(&mut *r.0.lock().unwrap_or_else(|e| e.into_inner())))
        .unwrap_or_default()
}

pub(crate) fn request(cx: &mut Context<'_>, request: Request) -> Result<(), CommandError> {
    cx.view::<Requests>()?.push(request);
    Ok(())
}

/// The result of a Command with nothing to report.
#[derive(Serialize, JsonSchema)]
pub struct Done {}

/// A dialog the menus open.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Dialog {
    /// The New Project form.
    NewProject,
    /// The OS dialog that picks a Project to open.
    OpenProject,
    /// The Preferences dialog.
    Preferences,
    /// The Palette, empty: go to a Document, Component or Net.
    Palette,
    /// The Palette on Commands.
    CommandPalette,
    /// The About box, with the third-party licences.
    About,
    /// The Change List dialog: Update PCB from Schematic.
    ChangeList,
    /// Preferences, on the Keyboard Shortcuts page.
    KeyboardShortcuts,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenDialogParams {
    dialog: Dialog,
}

struct OpenDialog;

impl Command for OpenDialog {
    const META: Meta = Meta {
        id: "app.dialog.open",
        title: "Open Dialog",
        description: "Opens one of the window's dialogs, such as the New Project form.",
        category: "app",
        icon: "dialog",
        kind: Kind::View,
    };
    type Params = OpenDialogParams;
    type Output = Done;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new(
                "new_project",
                "New Project…",
                "project_new",
                json!({"dialog": "new_project"}),
            ),
            Preset::new(
                "open_project",
                "Open Project…",
                "project_open",
                json!({"dialog": "open_project"}),
            ),
            Preset::new(
                "preferences",
                "Preferences…",
                "preferences",
                json!({"dialog": "preferences"}),
            ),
            Preset::new("palette", "Go to…", "search", json!({"dialog": "palette"})),
            Preset::new(
                "command_palette",
                "Command Palette…",
                "find",
                json!({"dialog": "command_palette"}),
            ),
            Preset::new("about", "About REDA", "about", json!({"dialog": "about"})),
            Preset::new(
                "change_list",
                "Update PCB from Schematic…",
                "sync",
                json!({"dialog": "change_list"}),
            ),
            Preset::new(
                "keyboard_shortcuts",
                "Keyboard Shortcuts…",
                "preferences",
                json!({"dialog": "keyboard_shortcuts"}),
            ),
        ]
    }

    fn run(&self, cx: &mut Context<'_>, p: OpenDialogParams) -> Result<Done, CommandError> {
        let r = match p.dialog {
            Dialog::NewProject => Request::NewProject,
            Dialog::OpenProject => Request::OpenProject,
            Dialog::Preferences => Request::Preferences,
            Dialog::Palette => Request::Palette,
            Dialog::CommandPalette => Request::CommandPalette,
            Dialog::About => Request::About,
            Dialog::ChangeList => Request::ChangeList,
            Dialog::KeyboardShortcuts => Request::KeyboardShortcuts,
        };
        request(cx, r)?;
        Ok(Done {})
    }
}

/// A Project's settings dialog.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Settings {
    /// Project Settings.
    ProjectSettings,
    /// Board Setup, for the PCB Document.
    BoardSetup,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenSettingsParams {
    settings: Settings,
}

/// Opens Project Settings or Board Setup: greyed while no Project is open.
struct OpenSettings;

impl Command for OpenSettings {
    const META: Meta = Meta {
        id: "app.settings.open",
        title: "Open Project Settings",
        description: "Opens the open Project's Project Settings or its Board Setup.",
        category: "app",
        icon: "properties",
        kind: Kind::View,
    };
    type Params = OpenSettingsParams;
    type Output = Done;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            Some(_) => Ok(()),
            None => Err(Unavailable {
                reason: NO_PROJECT.into(),
            }),
        }
    }

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new(
                "project_settings",
                "Project Settings…",
                "properties",
                json!({"settings": "project_settings"}),
            ),
            Preset::new(
                "board_setup",
                "Board Setup…",
                "rules",
                json!({"settings": "board_setup"}),
            ),
        ]
    }

    fn run(&self, cx: &mut Context<'_>, p: OpenSettingsParams) -> Result<Done, CommandError> {
        let project = cx.project()?;
        let r = match p.settings {
            Settings::ProjectSettings => Request::ProjectSettings,
            Settings::BoardSetup if project.pcb().is_none() => {
                return Err(CommandError::unavailable(reda_engine::sync::NO_PCB));
            }
            Settings::BoardSetup => Request::BoardSetup,
        };
        request(cx, r)?;
        Ok(Done {})
    }
}

struct Quit;

impl Command for Quit {
    const META: Meta = Meta {
        id: "app.quit",
        title: "Quit",
        description: "Closes the REDA window.",
        category: "app",
        icon: "quit",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Done, CommandError> {
        request(cx, Request::Quit)?;
        Ok(Done {})
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoomFitParams {
    /// The Document whose canvas to fit; the active Document when left out.
    #[serde(default)]
    document: Option<Uuid>,
}

struct ZoomFit;

impl Command for ZoomFit {
    const META: Meta = Meta {
        id: "view.zoom_fit",
        title: "Zoom to Fit",
        description: "Zooms a Document's canvas, the active Document's by default, so the whole Document fits.",
        category: "view",
        icon: "zoom_fit",
        kind: Kind::View,
    };
    type Params = ZoomFitParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, p: ZoomFitParams) -> Result<Done, CommandError> {
        let document = match p.document {
            Some(document) => document,
            None => cx
                .view::<Layout>()?
                .active_document()
                .ok_or_else(|| CommandError::unavailable("no Document is active"))?,
        };
        request(cx, Request::ZoomFit(document))?;
        Ok(Done {})
    }
}

struct ZoomSelection;

impl Command for ZoomSelection {
    const META: Meta = Meta {
        id: "view.zoom_selection",
        title: "Zoom to Selection",
        description: "Zooms a Document's canvas, the active Document's by default, to the selected objects.",
        category: "view",
        icon: "zoom_fit",
        kind: Kind::View,
    };
    type Params = ZoomFitParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, p: ZoomFitParams) -> Result<Done, CommandError> {
        let document = match p.document {
            Some(document) => document,
            None => cx
                .view::<Layout>()?
                .active_document()
                .ok_or_else(|| CommandError::unavailable("no Document is active"))?,
        };
        if cx.view::<crate::selection::Selection>()?.is_empty() {
            return Err(CommandError::unavailable("nothing is selected"));
        }
        request(cx, Request::ZoomSelection(document))?;
        Ok(Done {})
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoomToParams {
    /// The Document whose canvas to zoom; the active Document when left out.
    #[serde(default)]
    document: Option<Uuid>,
    /// The objects to frame, by UUID: Pads and texts go by their Footprint or owner.
    objects: Vec<Uuid>,
}

struct ZoomTo;

impl Command for ZoomTo {
    const META: Meta = Meta {
        id: "view.zoom_to",
        title: "Zoom to Objects",
        description: "Zooms a Document's canvas, the active Document's by default, to the given objects without selecting them.",
        category: "view",
        icon: "zoom_fit",
        kind: Kind::View,
    };
    type Params = ZoomToParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, p: ZoomToParams) -> Result<Done, CommandError> {
        let document = match p.document {
            Some(document) => document,
            None => cx
                .view::<Layout>()?
                .active_document()
                .ok_or_else(|| CommandError::unavailable("no Document is active"))?,
        };
        if p.objects.is_empty() {
            return Err(CommandError::invalid_params(
                "/objects",
                "no objects to zoom to",
            ));
        }
        request(cx, Request::ZoomTo(document, p.objects))?;
        Ok(Done {})
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetThemeParams {
    /// Follow the OS, or always dark or light.
    theme: Theme,
}

struct SetTheme;

impl Command for SetTheme {
    const META: Meta = Meta {
        id: "view.theme.set",
        title: "Set Theme",
        description: "Shows the window in the dark or light theme, or follows the OS, and keeps the choice as the general.theme Preference.",
        category: "view",
        icon: "theme_follow_os",
        kind: Kind::View,
    };
    type Params = SetThemeParams;
    type Output = Done;

    fn presets(&self) -> Vec<Preset> {
        [
            ("follow_os", "Theme: Follow the System"),
            ("dark", "Theme: Dark"),
            ("light", "Theme: Light"),
        ]
        .into_iter()
        .map(|(name, title)| {
            Preset::new(name, title, format!("theme_{name}"), json!({"theme": name}))
        })
        .collect()
    }

    fn run(&self, cx: &mut Context<'_>, p: SetThemeParams) -> Result<Done, CommandError> {
        // Kept as the Preference when there is a per-user folder.
        if let Some(dirs) = cx.user_dirs() {
            preferences::set(dirs, "general.theme", json!(p.theme))?;
        }
        request(cx, Request::Theme(p.theme))?;
        Ok(Done {})
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetUiScaleParams {
    /// The factor: 0.75, 0.85, 1, 1.15, 1.25, 1.5 or 2.
    scale: f64,
}

/// The result of a UI scale Command: the scale now in force.
#[derive(Serialize, JsonSchema)]
pub struct UiScaleResult {
    pub scale: f64,
}

/// Puts `scale` in force: the Preference (when there is a per-user folder), the view
/// state and the window.
fn apply_ui_scale(cx: &mut Context<'_>, scale: f64) -> Result<UiScaleResult, CommandError> {
    if let Some(dirs) = cx.user_dirs() {
        preferences::set(dirs, "general.ui_scale", json!(scale))?;
    }
    cx.view::<UiScale>()?.0 = scale;
    request(cx, Request::UiScale(scale))?;
    Ok(UiScaleResult { scale })
}

struct SetUiScale;

impl Command for SetUiScale {
    const META: Meta = Meta {
        id: "view.ui_scale.set",
        title: "UI Scale",
        description: "Scales the whole user interface to one of its steps, 75 to 200 %, and \
                      keeps it as the general.ui_scale Preference.",
        category: "view",
        icon: "zoom_fit",
        kind: Kind::View,
    };
    type Params = SetUiScaleParams;
    type Output = UiScaleResult;

    fn presets(&self) -> Vec<Preset> {
        preferences::UI_SCALES
            .into_iter()
            .map(|scale| {
                let percent = (scale * 100.0).round();
                Preset::new(
                    format!("{percent}"),
                    format!("UI Scale: {percent} %"),
                    "zoom_fit",
                    json!({ "scale": scale }),
                )
            })
            .collect()
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SetUiScaleParams,
    ) -> Result<UiScaleResult, CommandError> {
        if !preferences::UI_SCALES
            .iter()
            .any(|s| (s - p.scale).abs() < 1e-9)
        {
            return Err(CommandError::invalid_params(
                "/scale",
                "the UI scale is one of 0.75, 0.85, 1, 1.15, 1.25, 1.5 and 2",
            ));
        }
        apply_ui_scale(cx, p.scale)
    }
}

/// Which way `view.ui_scale.step` goes.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepDirection {
    /// The next step up.
    Up,
    /// The next step down.
    Down,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepUiScaleParams {
    direction: StepDirection,
}

struct StepUiScale;

impl Command for StepUiScale {
    const META: Meta = Meta {
        id: "view.ui_scale.step",
        title: "Step UI Scale",
        description: "Scales the user interface one step up or down (75 to 200 %), staying at \
                      the ends, and keeps it as the general.ui_scale Preference.",
        category: "view",
        icon: "zoom_fit",
        kind: Kind::View,
    };
    type Params = StepUiScaleParams;
    type Output = UiScaleResult;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new("up", "Larger UI", "zoom_fit", json!({"direction": "up"})),
            Preset::new(
                "down",
                "Smaller UI",
                "zoom_fit",
                json!({"direction": "down"}),
            ),
        ]
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: StepUiScaleParams,
    ) -> Result<UiScaleResult, CommandError> {
        let now = cx.view::<UiScale>()?.0;
        let next = preferences::step_ui_scale(now, matches!(p.direction, StepDirection::Up));
        apply_ui_scale(cx, next)
    }
}
