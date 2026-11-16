//! The schematic's dialogs: Select Similar, Re-annotate, Re-annotate (last settings), Update from Library,
//! Convert Cross Junctions, the words of new text, and Export PDF's file picker.
//!
//! Each dialog is a model that works on a [`Shell`] (tested without a window) plus its
//! drawing. The menus open them through the view Command `schematic.dialog.open`, which
//! queues a [`Request`] the window carries out; every change they make goes through
//! Commands (`schematic.find`, `selection.set`, `schematic.reannotate`, …).

mod cross;
mod library;
mod reannotate;
mod replace;
mod text;
pub mod update;

use std::path::Path;

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Preset, Registry, Unavailable,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::bars::{Request, request};
use crate::command_dialog::Dialogs;
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Tooltip as _};

pub use cross::ConvertCrossDialog;
pub use library::LibraryDialog;
pub use reannotate::labels as reannotate_labels;
pub use reannotate::{
    ReannotateDialog, ReannotateLast, Rename, RowSort, SheetRow as ReannotateSheetRow,
};
pub use replace::{KeepFields, Outcome, PickRow, ReplaceDialog};
pub use text::TextDialog;
pub use update::{
    ComponentRow, GridCell, Page, PartRow, Replacement, Section, SheetRow, UpdateDialog,
    UpdateScope,
};

/// A schematic dialog the menus open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DialogKind {
    /// The Find Panel, with its text field focused.
    Find,
    /// Select the Units like the selected one.
    SelectSimilar,
    /// Renumber Designators, with an old → new preview.
    Reannotate,
    /// Re-annotate (last settings): a count to confirm, then the saved settings run.
    ReannotateLast,
    /// Update outdated Components, with a preview.
    UpdateFromLibrary,
    /// Pick where to write the PDF of every sheet.
    ExportPdf,
    /// Convert Cross Junctions, on this sheet or the Project, with a Miter Size.
    ConvertCrossJunctions,
}

/// Adds `schematic.dialog.open` to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(OpenDialog);
    library::register_all(registry);
    registry.add(crate::panels::find::FindStep);
}

/// Adds the hand-written dialogs that ask for what an incomplete call lacks: the words
/// of new text.
pub fn register_dialogs(dialogs: &mut Dialogs) {
    dialogs.register("schematic.text.add", |_, params| {
        Box::new(TextDialog::new(params))
    });
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenDialogParams {
    dialog: DialogKind,
}

/// What `schematic.dialog.open` returns.
#[derive(Serialize, JsonSchema)]
pub struct Opened {}

struct OpenDialog;

impl Command for OpenDialog {
    const META: Meta = Meta {
        id: "schematic.dialog.open",
        title: "Open Schematic Dialog",
        description: "Opens one of the schematic's dialogs in the window: Find (the Find \
                      Panel), Select \
                      Similar, Re-annotate, Re-annotate (last settings), Update from Library, Convert Cross Junctions, \
                      or the file picker of Export PDF.",
        category: "schematic",
        icon: "dialog",
        kind: Kind::View,
    };
    type Params = OpenDialogParams;
    type Output = Opened;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        let has_sheet = cx
            .project
            .is_some_and(|p| p.documents().any(|d| p.schematic_sheet(d.uuid).is_some()));
        if has_sheet {
            Ok(())
        } else {
            Err(Unavailable {
                reason: "no Project with a Schematic Sheet is open".into(),
            })
        }
    }

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new("find", "Find…", "find", json!({"dialog": "find"})),
            Preset::new(
                "select_similar",
                "Select Similar…",
                "select_similar",
                json!({"dialog": "select_similar"}),
            ),
            Preset::new(
                "reannotate",
                "Re-annotate…",
                "annotate",
                json!({"dialog": "reannotate"}),
            ),
            Preset::new(
                "reannotate_last",
                "Re-annotate (last settings)…",
                "annotate",
                json!({"dialog": "reannotate_last"}),
            ),
            Preset::new(
                "update_from_library",
                "Update from Library…",
                "library_update",
                json!({"dialog": "update_from_library"}),
            ),
            Preset::new(
                "convert_cross_junctions",
                "Convert Cross Junctions…",
                "wire",
                json!({"dialog": "convert_cross_junctions"}),
            ),
            Preset::new(
                "export_pdf",
                "Export PDF…",
                "export_pdf",
                json!({"dialog": "export_pdf"}),
            ),
        ]
    }

    fn run(&self, cx: &mut Context<'_>, p: OpenDialogParams) -> Result<Opened, CommandError> {
        request(cx, Request::Schematic(p.dialog))?;
        Ok(Opened {})
    }
}

/// An open schematic dialog. The window keeps one and shows it every frame.
pub enum SchematicDialog {
    Reannotate(ReannotateDialog),
    ReannotateLast(ReannotateLast),
    Update(Box<UpdateDialog>),
    ConvertCross(ConvertCrossDialog),
    Replace(Box<ReplaceDialog>),
    Board(Box<crate::pcb::update_dialog::BoardUpdateDialog>),
}

impl SchematicDialog {
    /// The dialog `kind` on the Shell's Project; `None` for Export PDF (a file picker) or
    /// when it cannot open (Select Similar with no Unit selected says why in the status
    /// bar).
    pub fn open(kind: DialogKind, shell: &mut Shell) -> Option<SchematicDialog> {
        Some(match kind {
            DialogKind::Find => {
                crate::panels::find::open(shell);
                return None;
            }
            DialogKind::SelectSimilar => {
                // The non-modal window, on the first selected object.
                let selection = shell.selection().clone();
                let params = json!({"objects": selection.objects, "pads": selection.pads});
                if shell.run("selection.similar.window", params).is_err() {
                    shell.report(tr!("similar-needs-object"));
                }
                return None;
            }
            DialogKind::Reannotate => SchematicDialog::Reannotate(ReannotateDialog::new(shell)),
            DialogKind::ReannotateLast => match ReannotateLast::new(shell) {
                Ok(dialog) => SchematicDialog::ReannotateLast(dialog),
                Err(reason) => {
                    shell.report(reason);
                    return None;
                }
            },
            DialogKind::UpdateFromLibrary => {
                SchematicDialog::Update(Box::new(UpdateDialog::new(shell, UpdateScope::default())))
            }
            DialogKind::ConvertCrossJunctions => {
                SchematicDialog::ConvertCross(ConvertCrossDialog::new(shell)?)
            }
            DialogKind::ExportPdf => return None,
        })
    }

    /// Draws the dialog for one frame. Returns whether it is still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        match self {
            SchematicDialog::Reannotate(d) => d.show(ctx, shell),
            SchematicDialog::ReannotateLast(d) => d.show(ctx, shell),
            SchematicDialog::Update(d) => d.show(ctx, shell),
            SchematicDialog::ConvertCross(d) => d.show(ctx, shell),
            SchematicDialog::Replace(d) => d.show(ctx, shell) == Outcome::Open,
            SchematicDialog::Board(d) => d.show(ctx, shell),
        }
    }
}

/// Writes every sheet to the PDF at `path` (`schematic.export_pdf`) and says so in the
/// status bar.
pub fn export_pdf(shell: &mut Shell, path: &Path) -> Result<(), CommandError> {
    let out = shell.run_reporting(
        "schematic.export_pdf",
        json!({ "path": path.to_string_lossy() }),
    )?;
    let pages = out["result"]["pages"]
        .as_u64()
        .or_else(|| out["pages"].as_u64())
        .unwrap_or(0);
    shell.set_note(tr!(
        "export-pdf-done",
        pages = pages as i64,
        path = path.display().to_string()
    ));
    Ok(())
}

/// The file name Export PDF suggests: the Project's name.
pub fn pdf_file_name(shell: &Shell) -> String {
    let name = shell
        .engine()
        .project()
        .map(|p| p.props().name.clone())
        .unwrap_or_default();
    format!("{name}.pdf")
}

/// A line of text in the error tone: why a dialog could not do what it was asked.
pub(crate) fn error(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let t = ui.tokens();
    ui.label(t.ty.body.rich(text).color(t.color.error.text))
}

/// A line of text in the warning tone: something to look at before going on.
fn warning(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let t = ui.tokens();
    ui.label(t.ty.body.rich(text).color(t.color.warning.text))
}

/// A heading inside a dialog: body text in the strong weight.
fn strong(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let t = ui.tokens();
    ui.label(t.ty.body_strong.rich(text).color(t.color.text.primary))
}

/// Secondary text: a column's name, a note, a value that is not changing.
fn muted(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let t = ui.tokens();
    ui.label(t.ty.body.rich(text).color(t.color.text.muted))
}

/// A radio button for `choice`: clicking it sets `value` to `choice`.
fn radio<T: PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    choice: T,
    text: impl Into<String>,
) -> egui::Response {
    let response = ui.add(widgets::radio(*value == choice, text));
    if response.clicked() {
        *value = choice;
    }
    response
}

/// The two buttons that end a dialog: the action (greyed with `why` as its tooltip when
/// it cannot run) and Cancel. Returns (action clicked, cancel clicked).
fn buttons(ui: &mut egui::Ui, action: &str, enabled: bool, why: &str) -> (bool, bool) {
    crate::modal::buttons(ui, |ui| {
        let run = ui
            .add_enabled(enabled, widgets::button(action).primary())
            .disabled_tooltip(why)
            .clicked();
        let cancel = ui.add(widgets::button(tr!("dialog-cancel"))).clicked();
        (run, cancel)
    })
}
