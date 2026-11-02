//! The view Commands behind menu items that act on the window rather than the design:
//! opening the New Project and Open Project dialogs, Quit, Zoom to Fit and the Theme.
//!
//! The window owns dialogs, cameras and the egui theme, so these Commands only queue a
//! [`Request`] in the engine's view state; the window takes the queue each frame with
//! [`take_requests`] and carries the requests out.

use std::sync::{Arc, Mutex};

use reda_engine::Engine;
use reda_engine::command::{
    Command, CommandError, Context, Kind, Meta, NoParams, Preset, Registry,
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
    /// Show the Command Palette.
    CommandPalette,
    /// Close the window.
    Quit,
    /// Fit this Document's canvas to the Document.
    ZoomFit(Uuid),
    /// Show the window in this theme.
    Theme(Theme),
    /// Show the Preferences dialog.
    Preferences,
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
    registry.add(SetTheme);
}

/// Adds the Commands to `engine` and gives it a request queue. The Shell does this.
pub fn install(engine: &mut Engine) {
    if engine.registry().get(Quit::META.id).is_none() {
        engine.register(register_all);
    }
    engine.view_mut().set(Requests::default());
}

/// Takes the requests queued since the last call, oldest first.
pub fn take_requests(engine: &Engine) -> Vec<Request> {
    engine
        .view()
        .get::<Requests>()
        .map(|r| std::mem::take(&mut *r.0.lock().unwrap_or_else(|e| e.into_inner())))
        .unwrap_or_default()
}

fn request(cx: &mut Context<'_>, request: Request) -> Result<(), CommandError> {
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
    /// The Command Palette.
    CommandPalette,
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
            Preset::new(
                "command_palette",
                "Command Palette…",
                "find",
                json!({"dialog": "command_palette"}),
            ),
        ]
    }

    fn run(&self, cx: &mut Context<'_>, p: OpenDialogParams) -> Result<Done, CommandError> {
        let r = match p.dialog {
            Dialog::NewProject => Request::NewProject,
            Dialog::OpenProject => Request::OpenProject,
            Dialog::Preferences => Request::Preferences,
            Dialog::CommandPalette => Request::CommandPalette,
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
