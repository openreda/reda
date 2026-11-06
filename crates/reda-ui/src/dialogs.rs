//! Native file pickers, run off the UI thread.

use std::path::PathBuf;
use std::pin::pin;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::task::{Context, Poll, Wake, Waker};

use crate::layout::SettingsWindow;
use crate::tr;

/// What a pending file picker is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// A `.reda` file to open.
    OpenProject,
    /// The folder a new Project is created in.
    NewProjectLocation,
    /// A folder to add as a Project Library.
    LibraryFolder,
    /// A folder a settings window's page asked for (a user Library folder, for
    /// Preferences).
    SettingsFolder(SettingsWindow),
    /// Where to save the exported Journal.
    ExportJournal,
    /// The symbol file Import part reads.
    ImportSymbol,
    /// The footprint file Import part reads.
    ImportFootprint,
    /// Where to write the schematic PDF.
    ExportPdf,
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
        self.start_save(ctx, pick, String::new());
    }

    /// Opens a picker for `pick`, suggesting `file_name` when it saves a file.
    pub fn start_save(&mut self, ctx: &egui::Context, pick: Pick, file_name: String) {
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
            Pick::LibraryFolder | Pick::SettingsFolder(_) => {
                rfd::AsyncFileDialog::new().set_title(tr!("dialog-library-folder-title"))
            }
            Pick::ExportJournal => rfd::AsyncFileDialog::new()
                .set_title(tr!("journal-export-title"))
                .add_filter(tr!("journal-export-filter"), &["jsonl"])
                .set_file_name("journal.jsonl"),
            Pick::ImportSymbol => rfd::AsyncFileDialog::new()
                .set_title(tr!("import-symbol-title"))
                .add_filter(tr!("import-symbol-filter"), &["kicad_sym"]),
            Pick::ImportFootprint => rfd::AsyncFileDialog::new()
                .set_title(tr!("import-footprint-title"))
                .add_filter(tr!("import-footprint-filter"), &["kicad_mod"]),
            Pick::ExportPdf => rfd::AsyncFileDialog::new()
                .set_title(tr!("export-pdf-title"))
                .add_filter(tr!("export-pdf-filter"), &["pdf"])
                .set_file_name(file_name),
        };
        std::thread::spawn(move || {
            let picked = match pick {
                Pick::OpenProject | Pick::ImportSymbol | Pick::ImportFootprint => {
                    block_on(dialog.pick_file())
                }
                Pick::NewProjectLocation | Pick::LibraryFolder | Pick::SettingsFolder(_) => {
                    block_on(dialog.pick_folder())
                }
                Pick::ExportJournal | Pick::ExportPdf => block_on(dialog.save_file()),
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
