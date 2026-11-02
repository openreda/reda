//! Dialogs: native file pickers (off the UI thread) and the New Project form.

use std::path::PathBuf;
use std::pin::pin;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::task::{Context, Poll, Wake, Waker};

use egui::{Id, Modal, Ui};

use crate::tr;

/// What a pending file picker is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// A `.reda` file to open.
    OpenProject,
    /// The folder a new Project is created in.
    NewProjectLocation,
    /// Where to save the exported Journal.
    ExportJournal,
}

/// Native file pickers. Each runs on its own thread, so the window keeps drawing; the
/// answer is collected with [`FileDialogs::poll`].
#[derive(Default)]
pub struct FileDialogs {
    pending: Option<(Pick, Receiver<Option<PathBuf>>)>,
}

impl FileDialogs {
    /// Whether a picker is open.
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// Opens a picker for `pick`, unless one is already open.
    pub fn start(&mut self, ctx: &egui::Context, pick: Pick) {
        if self.busy() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let dialog = match pick {
            Pick::OpenProject => rfd::AsyncFileDialog::new()
                .set_title(tr!("dialog-open-project-title"))
                .add_filter(tr!("dialog-project-filter"), &["reda"]),
            Pick::NewProjectLocation => {
                rfd::AsyncFileDialog::new().set_title(tr!("dialog-location-title"))
            }
            Pick::ExportJournal => rfd::AsyncFileDialog::new()
                .set_title(tr!("journal-export-title"))
                .add_filter(tr!("journal-export-filter"), &["jsonl"])
                .set_file_name("journal.jsonl"),
        };
        std::thread::spawn(move || {
            let picked = match pick {
                Pick::OpenProject => block_on(dialog.pick_file()),
                Pick::NewProjectLocation => block_on(dialog.pick_folder()),
                Pick::ExportJournal => block_on(dialog.save_file()),
            };
            let _ = tx.send(picked.map(|h| h.path().to_owned()));
            ctx.request_repaint();
        });
        self.pending = Some((pick, rx));
    }

    /// The picked path, once the picker closed with one.
    pub fn poll(&mut self) -> Option<(Pick, PathBuf)> {
        let (pick, rx) = self.pending.as_ref()?;
        let pick = *pick;
        match rx.try_recv() {
            Ok(path) => {
                self.pending = None;
                path.map(|p| (pick, p))
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                None
            }
        }
    }
}

/// Runs a future to completion on this thread (rfd's futures are woken by its own threads).
fn block_on<F: Future>(future: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(out) => return out,
            Poll::Pending => std::thread::park(),
        }
    }
}

/// The New Project form: a name and the folder to create it in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewProjectForm {
    pub name: String,
    pub location: String,
}

/// What the user did with the New Project form this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormAction {
    None,
    Browse,
    Create,
    Cancel,
}

impl NewProjectForm {
    /// A form with a suggested name, in the user's home folder.
    pub fn new() -> NewProjectForm {
        NewProjectForm {
            name: tr!("new-project-default-name"),
            location: std::env::home_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        }
    }

    /// Shows the form as a modal dialog.
    pub fn show(&mut self, ctx: &egui::Context) -> FormAction {
        let mut action = FormAction::None;
        let modal = Modal::new(Id::NULL.with("new-project")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.heading(tr!("new-project-title"));
            ui.add_space(6.0);
            egui::Grid::new("new-project-fields")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    labelled_field(ui, &tr!("new-project-name"), &mut self.name);
                    ui.end_row();
                    labelled_field(ui, &tr!("new-project-location"), &mut self.location);
                    ui.end_row();
                    ui.label("");
                    if ui.button(tr!("new-project-browse")).clicked() {
                        action = FormAction::Browse;
                    }
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ready = !self.name.trim().is_empty() && !self.location.trim().is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new(tr!("new-project-create")))
                    .clicked()
                {
                    action = FormAction::Create;
                }
                if ui.button(tr!("dialog-cancel")).clicked() {
                    action = FormAction::Cancel;
                }
            });
        });
        if modal.should_close() && action == FormAction::None {
            action = FormAction::Cancel;
        }
        action
    }
}

fn labelled_field(ui: &mut Ui, label: &str, value: &mut String) -> egui::Response {
    let label = ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(260.0))
        .labelled_by(label.id)
}
