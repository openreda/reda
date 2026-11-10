//! The hand-written params dialogs of the library workflow, in the Command dialog
//! framework ([`crate::command_dialog`]): Import part (file pickers and a target Library)
//! and the Footprint generators (their generated form, a Library picker and a live
//! preview). The Library check they run, and the one-time vendor-terms notice, reach the
//! Messages panel and the window through the Shell after the Command runs.

mod generate;
mod import;
mod new_library;
mod paste;
pub mod paste_array;
mod to_library;
mod wizard;

use reda_engine::library::LibraryOrigin;
use reda_engine::model::Uuid;

use crate::command_dialog::Dialogs;
use crate::shell::Shell;

pub use generate::{
    Axis, Callout, GenerateDialog, Generated, GeneratorForm, Preview, Row, Section, Shown, Side,
    callouts,
};
pub use import::ImportDialog;
pub use new_library::{For as NewLibraryFor, NewLibraryDialog};
pub use paste::{PasteMapping, PasteOutcome};
pub use paste_array::{ArrayOptions, PasteArrayDialog};
pub use to_library::ToLibraryDialog;
pub use wizard::{SymbolWizardDialog, Target as WizardTarget};

/// The window's hand-written dialogs: New Library's, Import part's, every generator's, and
/// Copy to Library's and Extract's (Make Project Library).
pub fn dialogs() -> Dialogs {
    let mut dialogs = Dialogs::new();
    dialogs.register("library.create", |shell, params| {
        Box::new(NewLibraryDialog::new(shell, &params))
    });
    dialogs.register_always(wizard::COMMAND, |shell, params| {
        Box::new(SymbolWizardDialog::new(shell, &params))
    });
    dialogs.register_always(paste_array::COMMAND, |shell, params| {
        Box::new(PasteArrayDialog::new(shell, &params))
    });
    dialogs.register("library.import_part", |_, params| {
        Box::new(ImportDialog::new(&params))
    });
    for id in [to_library::COPY, to_library::EXTRACT] {
        dialogs.register(id, move |shell, params| {
            Box::new(ToLibraryDialog::new(shell, id, &params))
        });
    }
    // Regenerate… asks first even with the Footprint given: the form shows its stored
    // params and what regenerating loses.
    dialogs.register_always(generate::REGENERATE, |shell, params| {
        let footprint = params["footprint"].as_str().and_then(|f| f.parse().ok());
        match footprint.and_then(|f| GenerateDialog::regenerate(shell, f)) {
            Some(dialog) => Box::new(dialog),
            None => Box::new(crate::command_dialog::ParamForm::new(
                shell.engine().registry(),
                generate::REGENERATE,
                &params,
            )),
        }
    });
    for id in reda_engine::library::GENERATORS {
        dialogs.register(id, move |shell, params| {
            Box::new(GenerateDialog::new(shell, id, &params))
        });
    }
    dialogs
}

/// The registered Libraries that can be written to (not the starter Library), by name.
pub fn writable_libraries(shell: &mut Shell) -> Vec<(Uuid, String)> {
    let index = shell.library_index();
    let mut libraries: Vec<(Uuid, String)> = index
        .libraries()
        .filter(|l| l.origin != LibraryOrigin::Starter)
        .map(|l| (l.info.uuid, l.info.name.clone()))
        .collect();
    libraries.sort_by(|a, b| a.1.cmp(&b.1));
    libraries
}

/// The Library a dialog starts with: the one the call gave (`given`), else the first
/// writable one.
fn initial_library(shell: &mut Shell, given: &serde_json::Value) -> Option<Uuid> {
    let libraries = writable_libraries(shell);
    given
        .as_str()
        .and_then(|s| s.parse().ok())
        .filter(|l| libraries.iter().any(|(u, _)| u == l))
        .or_else(|| libraries.first().map(|(u, _)| *u))
}

/// The target Library picker shared by the dialogs.
fn library_picker(ui: &mut egui::Ui, shell: &mut Shell, id: egui::Id, library: &mut Option<Uuid>) {
    let libraries = writable_libraries(shell);
    let label = crate::tr!("library-target");
    ui.label(&label);
    let shown = library
        .and_then(|l| libraries.iter().find(|(u, _)| *u == l))
        .map_or_else(|| crate::tr!("library-target-none"), |(_, n)| n.clone());
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(260.0)
        .show_ui(ui, |ui| {
            for (uuid, name) in &libraries {
                ui.selectable_value(library, Some(*uuid), name);
            }
        })
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
}
