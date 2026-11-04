//! A host with no window: one thread owns an [`Engine`] and runs every bridged [`Call`].
//!
//! It is the reference for embedding the control server, and what the tests bridge to
//! before the app embeds it. It never saves; neither does any bridged Command.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::JoinHandle;

use reda_engine::Engine;
use reda_engine::command::UserDirs;
use reda_engine::journal::JournalEntry;

use crate::{Call, ControlServer, project_file};

/// What the engine held when the host stopped.
pub struct Leftover {
    /// The open Project file, if any.
    pub project: Option<PathBuf>,
    /// How many files have unsaved changes.
    pub dirty_files: usize,
    /// Every Command the host ran, with its source.
    pub journal: Vec<JournalEntry>,
}

/// A running headless host. [`HeadlessHost::stop`] stops it.
pub struct HeadlessHost {
    server: ControlServer,
    thread: JoinHandle<Leftover>,
}

impl HeadlessHost {
    /// Builds the engine with `make` on the host thread (which may first open a Project),
    /// starts the control server with its catalogue and Project, and serves Calls.
    pub fn spawn(
        dirs: &UserDirs,
        make: impl FnOnce() -> Engine + Send + 'static,
    ) -> std::io::Result<HeadlessHost> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (start_tx, start_rx) = mpsc::channel::<Option<(ControlServer, mpsc::Receiver<Call>)>>();
        let thread = std::thread::spawn(move || {
            let mut engine = make();
            let _ = ready_tx.send((engine.registry().catalogue(), project_file(&engine)));
            if let Ok(Some((server, calls))) = start_rx.recv() {
                for call in calls {
                    let source = call.source;
                    let _ = call
                        .run(|id, params, signals| engine.run_from(source, id, params, signals));
                    let _ = server.set_project(project_file(&engine));
                }
            }
            Leftover {
                project: project_file(&engine),
                dirty_files: engine.dirty_scopes().len(),
                journal: engine.journal().to_vec(),
            }
        });
        let (catalogue, project) = ready_rx
            .recv()
            .map_err(|_| std::io::Error::other("the host engine failed to build"))?;
        match ControlServer::start(dirs, &catalogue, project) {
            Ok((server, calls)) => {
                let _ = start_tx.send(Some((server.clone(), calls)));
                Ok(HeadlessHost { server, thread })
            }
            Err(err) => {
                let _ = start_tx.send(None);
                let _ = thread.join();
                Err(err)
            }
        }
    }

    pub fn port(&self) -> u16 {
        self.server.port()
    }

    pub fn server(&self) -> &ControlServer {
        &self.server
    }

    /// Stops the server, lets the host finish its last Call, and reports what it held.
    pub fn stop(self) -> Leftover {
        self.server.stop();
        drop(self.server);
        self.thread.join().expect("the host thread does not panic")
    }
}
