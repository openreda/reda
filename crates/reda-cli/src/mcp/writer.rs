//! The single writer: one thread owns the [`Engine`] and runs Commands one at a time.
//!
//! rmcp handles requests concurrently, so every tool call that touches the session sends
//! its Command here and awaits the reply. Nothing else touches the engine.

use std::thread::JoinHandle;

use reda_engine::Engine;
use reda_engine::command::{CommandError, Signals};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

type Outcome = Result<Value, CommandError>;

enum Message {
    Run {
        id: String,
        params: Value,
        signals: Signals,
        reply: oneshot::Sender<Outcome>,
    },
}

/// What the engine held when the writer stopped.
pub struct Leftover {
    /// The open Project's name, if one is open.
    pub project: Option<String>,
    /// How many files (the Project file and Documents) have unsaved changes.
    pub dirty_files: usize,
}

/// A handle to the writer thread. Clones send to the same thread.
#[derive(Clone)]
pub struct Writer {
    tx: mpsc::UnboundedSender<Message>,
}

/// Owns the writer thread; [`WriterThread::finish`] stops it.
pub struct WriterThread {
    thread: JoinHandle<Leftover>,
}

/// Builds the engine on a new thread with `make` and starts serving Commands. Returns the
/// handle, the thread, and the registry's catalogue (fixed for the engine's lifetime).
pub fn spawn(make: impl FnOnce() -> Engine + Send + 'static) -> (Writer, WriterThread, Value) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let (catalogue_tx, catalogue_rx) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let mut engine = make();
        catalogue_tx
            .send(engine.registry().catalogue())
            .expect("the spawner waits for the catalogue");
        while let Some(message) = rx.blocking_recv() {
            match message {
                Message::Run {
                    id,
                    params,
                    signals,
                    reply,
                } => {
                    let outcome = if signals.cancel.is_cancelled() {
                        Err(CommandError::cancelled())
                    } else {
                        engine.run_with(&id, params, &signals)
                    };
                    // Dropping the signals first closes the progress channel, so every
                    // progress notification goes out before the response.
                    drop(signals);
                    let _ = reply.send(outcome);
                }
            }
        }
        Leftover {
            project: engine.project().map(|p| p.props().name.clone()),
            dirty_files: engine.dirty_scopes().len(),
        }
    });
    let catalogue = catalogue_rx
        .recv()
        .expect("the writer thread builds the engine");
    (Writer { tx }, WriterThread { thread }, catalogue)
}

impl Writer {
    /// Runs one Command on the writer thread, after every Command sent before it.
    pub async fn run(&self, id: &str, params: Value, signals: Signals) -> Outcome {
        let (reply, answer) = oneshot::channel();
        let message = Message::Run {
            id: id.to_owned(),
            params,
            signals,
            reply,
        };
        if self.tx.send(message).is_err() {
            return Err(CommandError::internal("the REDA session has stopped"));
        }
        answer
            .await
            .unwrap_or_else(|_| Err(CommandError::internal("the REDA session has stopped")))
    }
}

impl WriterThread {
    /// Waits for every [`Writer`] handle to drop and the thread to finish its last Command,
    /// then reports what was left unsaved.
    pub fn finish(self) -> Leftover {
        self.thread
            .join()
            .expect("the writer thread does not panic")
    }
}
