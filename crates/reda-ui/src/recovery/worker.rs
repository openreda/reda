//! The background thread that writes Recovery Snapshots and the Journal mirror, so the
//! window never waits on the disk.

use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use reda_engine::model::Scope;
use reda_engine::recovery::{self as snapshots, JOURNAL, View};

/// One piece of work for the writer.
pub(super) enum Job {
    /// Snapshots of these scopes from the view into the folder.
    Write(PathBuf, Box<View>, Vec<Scope>),
    /// Deletes these scopes' snapshots.
    Remove(PathBuf, Vec<Scope>),
    /// Appends JSON lines to the Journal mirror.
    Journal(PathBuf, String),
    /// Keeps a crashed session's Journal mirror as `journal.previous.jsonl`.
    Rotate(PathBuf),
    /// Deletes the whole folder.
    Clear(PathBuf),
    /// Answers once everything sent before it is done.
    Flush(Sender<()>),
}

pub(super) struct Worker {
    jobs: Sender<Job>,
    error: Arc<Mutex<Option<String>>>,
}

impl Worker {
    pub(super) fn spawn() -> Worker {
        let (jobs, rx) = mpsc::channel::<Job>();
        let error = Arc::new(Mutex::new(None));
        let failed = Arc::clone(&error);
        std::thread::Builder::new()
            .name("reda-recovery".into())
            .spawn(move || {
                for job in rx {
                    if let Err(e) = run(job) {
                        *failed.lock().unwrap_or_else(|p| p.into_inner()) = Some(e.to_string());
                    }
                }
            })
            .expect("the recovery writer thread starts");
        Worker { jobs, error }
    }

    pub(super) fn send(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    pub(super) fn flush(&self) {
        let (tx, rx) = mpsc::channel();
        if self.jobs.send(Job::Flush(tx)).is_ok() {
            let _ = rx.recv();
        }
    }

    pub(super) fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(|p| p.into_inner()).take()
    }
}

fn run(job: Job) -> std::io::Result<()> {
    match job {
        Job::Write(dir, view, scopes) => view.write(&dir, &scopes, now_ms()),
        Job::Remove(dir, scopes) => snapshots::remove(&dir, &scopes),
        Job::Journal(dir, text) => snapshots::append_journal(&dir, &text),
        Job::Rotate(dir) => {
            match std::fs::rename(dir.join(JOURNAL), dir.join("journal.previous.jsonl")) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        }
        Job::Clear(dir) => snapshots::clear(&dir),
        Job::Flush(done) => {
            let _ = done.send(());
            Ok(())
        }
    }
}

fn now_ms() -> i64 {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
}
