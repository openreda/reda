//! Crash recovery in the app: Recovery Snapshots of unsaved work on a fixed schedule, the
//! Journal mirror, and the prompt that offers leftover snapshots when a Project opens
//! after an unclean exit.
//!
//! [`Recorder`] decides what to write and when; a background thread ([`worker`]) writes it
//! through `reda_engine::recovery`, from a read view taken on the app's thread. The
//! timings are fixed: a snapshot about [`QUIET`] after the last change, and never more
//! than about [`MAX_GAP`] apart while changes keep coming. Only the GUI app records:
//! `reda run` and `reda mcp` never do.

mod worker;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reda_engine::command::{CommandError, UserDirs};
use reda_engine::journal;
use reda_engine::model::Scope;
use reda_engine::recovery::{self as snapshots, Found, State, Target};
use reda_engine::{Engine, store};
use serde_json::json;

use crate::shell::Shell;
use crate::tr;
use worker::{Job, Worker};

/// How long after the last change a snapshot is written.
pub const QUIET: Duration = Duration::from_secs(5);
/// The longest a change waits for its snapshot while changes keep coming.
pub const MAX_GAP: Duration = Duration::from_secs(60);

/// What a prompt row will do by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Restore,
    Discard,
}

/// One leftover snapshot offered on open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub found: Found,
    /// The default action: Discard when the file on disk changed since the snapshot (saving
    /// it would overwrite newer content) or when it cannot be read.
    pub choice: Choice,
}

impl Row {
    fn new(found: Found) -> Row {
        let choice = match (&found.state, found.disk_changed) {
            (State::Ready, false) => Choice::Restore,
            _ => Choice::Discard,
        };
        Row { found, choice }
    }

    /// Why the snapshot cannot be read, if it cannot.
    pub fn error(&self) -> Option<&str> {
        match &self.found.state {
            State::Unparsable(why) => Some(why),
            State::Ready => None,
        }
    }

    /// Unparsable snapshots offer only Discard and "Show in folder".
    pub fn can_restore(&self) -> bool {
        self.error().is_none()
    }

    /// The row's name: the file it belongs to.
    pub fn title(&self) -> String {
        let snapshot = &self.found.snapshot;
        match &snapshot.target {
            Target::Document { document } => {
                format!("{} ({})", document.title, snapshot.path.display())
            }
            _ => snapshot.path.display().to_string(),
        }
    }
}

/// The recovery prompt: leftover snapshots not yet restored or discarded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    /// The recovery folder they are in.
    pub dir: PathBuf,
    pub rows: Vec<Row>,
}

/// Records snapshots and the Journal mirror for the open Project.
pub struct Recorder {
    /// The recovery folder of the open Project (or the per-user one with none open).
    dir: Option<PathBuf>,
    /// Each scope's history state when its snapshot was last written.
    written: BTreeMap<Scope, u64>,
    /// Each dirty scope's history state when last looked at.
    seen: BTreeMap<Scope, u64>,
    /// The first change not yet in a snapshot, and the latest change.
    first_change: Option<Instant>,
    last_change: Option<Instant>,
    /// How many Journal entries the mirror holds.
    mirrored: usize,
    /// Leftover snapshots to offer; nothing is written while there are some.
    prompt: Option<Prompt>,
    worker: Worker,
}

impl Default for Recorder {
    fn default() -> Recorder {
        Recorder::new()
    }
}

impl Recorder {
    pub fn new() -> Recorder {
        Recorder {
            dir: None,
            written: BTreeMap::new(),
            seen: BTreeMap::new(),
            first_change: None,
            last_change: None,
            mirrored: 0,
            prompt: None,
            worker: Worker::spawn(),
        }
    }

    /// A Project was just opened or created by the newest Journal entry. The previous one
    /// closed cleanly, so its recovery folder goes. Leftover snapshots in the new one mean
    /// the last session ended uncleanly: they are offered, and the old Journal mirror is
    /// kept as `journal.previous.jsonl`.
    pub fn opened(&mut self, engine: &Engine) {
        self.close_cleanly();
        let dir = recovery_dir(engine);
        if let Some(dir) = &dir {
            let rows: Vec<Row> = snapshots::scan(dir, engine.location())
                .into_iter()
                .map(Row::new)
                .collect();
            if !rows.is_empty() {
                self.prompt = Some(Prompt {
                    dir: dir.clone(),
                    rows,
                });
            }
            self.worker.send(Job::Rotate(dir.clone()));
        }
        self.dir = dir;
        self.mirrored = engine.journal().len().saturating_sub(1);
    }

    /// Looks for changes at time `now` and hands due work to the background writer.
    pub fn tick(&mut self, engine: &Engine, now: Instant) {
        let Some(dir) = self.dir.clone() else {
            return;
        };
        self.mirror(engine, &dir);
        if self.prompt.is_some() {
            return;
        }
        let history = engine.history();
        let dirty: BTreeMap<Scope, u64> = engine
            .dirty_scopes()
            .into_iter()
            .map(|s| (s, history.state(s)))
            .collect();
        if dirty != self.seen {
            if dirty.iter().any(|(s, v)| self.written.get(s) != Some(v)) {
                self.first_change.get_or_insert(now);
                self.last_change = Some(now);
            }
            self.seen = dirty.clone();
        }

        let clean: Vec<Scope> = (self.written.keys())
            .filter(|s| !dirty.contains_key(s))
            .copied()
            .collect();
        if !clean.is_empty() {
            for scope in &clean {
                self.written.remove(scope);
            }
            self.worker.send(Job::Remove(dir.clone(), clean));
        }

        if self.due(now) {
            let changed: Vec<Scope> = (dirty.iter())
                .filter(|(s, v)| self.written.get(s) != Some(v))
                .map(|(s, _)| *s)
                .collect();
            self.first_change = None;
            self.last_change = None;
            if !changed.is_empty() {
                for scope in &changed {
                    self.written.insert(*scope, dirty[scope]);
                }
                let view = snapshots::View::of(engine);
                self.worker.send(Job::Write(dir, Box::new(view), changed));
            }
        }
    }

    fn due(&self, now: Instant) -> bool {
        match (self.first_change, self.last_change) {
            (Some(first), Some(last)) => {
                now.duration_since(last) >= QUIET || now.duration_since(first) >= MAX_GAP
            }
            _ => false,
        }
    }

    /// How long until the next snapshot is due, if one is waiting: the window asks for a
    /// frame then.
    pub fn next_due(&self, now: Instant) -> Option<Duration> {
        let (first, last) = (self.first_change?, self.last_change?);
        let at = (last + QUIET).min(first + MAX_GAP);
        Some(at.saturating_duration_since(now))
    }

    fn mirror(&mut self, engine: &Engine, dir: &Path) {
        let entries = engine.journal();
        if self.mirrored >= entries.len() {
            return;
        }
        let text = journal::export(&entries[self.mirrored..]);
        self.mirrored = entries.len();
        crate::crash::remember_journal(&text);
        self.worker.send(Job::Journal(dir.to_owned(), text));
    }

    /// The Project closes cleanly: everything in its recovery folder goes, the Journal
    /// mirror included.
    pub fn close_cleanly(&mut self) {
        if let Some(dir) = self.dir.take() {
            self.worker.send(Job::Clear(dir));
        }
        self.written.clear();
        self.seen.clear();
        self.first_change = None;
        self.last_change = None;
        self.prompt = None;
        self.worker.flush();
    }

    /// Waits until the background writer has done everything handed to it.
    pub fn flush(&self) {
        self.worker.flush();
    }

    /// The last error the background writer met, once.
    pub fn take_error(&self) -> Option<String> {
        self.worker.take_error()
    }

    pub fn prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref()
    }

    /// Drops the settled rows; the prompt closes when none are left.
    fn settle(&mut self, files: &[String]) {
        if let Some(prompt) = &mut self.prompt {
            prompt
                .rows
                .retain(|r| !files.contains(&r.found.snapshot.file));
            if prompt.rows.is_empty() {
                self.prompt = None;
            }
        }
    }
}

/// Where the open Project's snapshots go: its `.reda/recovery/`, or with no Project open
/// (Library editor tabs only) the per-user data dir's `recovery/`.
fn recovery_dir(engine: &Engine) -> Option<PathBuf> {
    match engine.location() {
        Some(folder) => Some(snapshots::project_dir(folder)),
        None => engine.user_dirs().map(user_recovery_dir),
    }
}

/// The per-user recovery folder.
pub fn user_recovery_dir(dirs: &UserDirs) -> PathBuf {
    dirs.data.join("recovery")
}

/// Whether a live REDA instance already holds the Project at `path` (its `.reda` file or
/// folder): the pid of that instance.
pub fn live_instance(dirs: &UserDirs, path: &Path) -> Option<u32> {
    let wanted = reda_control::normalize(&store::resolve_project_file(path));
    reda_control::client::instances(dirs)
        .into_iter()
        .filter(|i| i.project.as_deref() == Some(wanted.as_path()))
        .find(|i| reda_control::client::Connection::connect(i, reda_engine::Source::Gui).is_ok())
        .map(|i| i.pid)
}

impl Shell {
    /// Leftover snapshots to offer, while any are unsettled.
    pub fn recovery_prompt(&self) -> Option<&Prompt> {
        self.recorder.prompt()
    }

    /// Restores one leftover snapshot as unsaved changes, then deletes it (the next
    /// snapshot of its file is taken afresh).
    pub fn restore_snapshot(&mut self, file: &str) -> Result<(), CommandError> {
        self.restore_snapshots(&[file.to_owned()])
    }

    fn restore_snapshots(&mut self, files: &[String]) -> Result<(), CommandError> {
        let Some(dir) = self.recorder.prompt().map(|p| p.dir.clone()) else {
            return Ok(());
        };
        if files.is_empty() {
            return Ok(());
        }
        self.run_reporting("recovery.restore", json!({ "snapshots": files }))?;
        self.discard_files(&dir, files);
        Ok(())
    }

    /// Deletes one leftover snapshot.
    pub fn discard_snapshot(&mut self, file: &str) {
        if let Some(dir) = self.recorder.prompt().map(|p| p.dir.clone()) {
            self.discard_files(&dir, &[file.to_owned()]);
        }
    }

    /// Restores every row whose default is Restore; rows that default to Discard (the
    /// file changed on disk, or the snapshot cannot be read) stay for the user to decide.
    pub fn restore_all(&mut self) {
        let files: Vec<String> = self
            .recovery_prompt()
            .map(|p| {
                p.rows
                    .iter()
                    .filter(|r| r.choice == Choice::Restore)
                    .map(|r| r.found.snapshot.file.clone())
                    .collect()
            })
            .unwrap_or_default();
        let _ = self.restore_snapshots(&files);
    }

    /// Deletes every leftover snapshot.
    pub fn discard_all(&mut self) {
        let Some(prompt) = self.recovery_prompt() else {
            return;
        };
        let dir = prompt.dir.clone();
        let files: Vec<String> = prompt
            .rows
            .iter()
            .map(|r| r.found.snapshot.file.clone())
            .collect();
        self.discard_files(&dir, &files);
    }

    fn discard_files(&mut self, dir: &Path, files: &[String]) {
        if let Err(e) = snapshots::discard(dir, files) {
            self.report(tr!("error-recovery", error = e.to_string()));
        }
        self.recorder.settle(files);
    }

    /// Takes snapshots and mirrors the Journal as due at `now`; the window calls it every
    /// frame. Returns how long until the next snapshot is due, if one is waiting.
    pub fn tick_recovery(&mut self, now: Instant) -> Option<Duration> {
        self.recorder.tick(&self.engine, now);
        if let Some(e) = self.recorder.take_error() {
            self.report(tr!("error-recovery", error = e));
        }
        self.recorder.next_due(now)
    }

    /// Waits for the background snapshot writer.
    pub fn flush_recovery(&self) {
        self.recorder.flush();
    }

    /// The window closes cleanly: the open Project's recovery folder is deleted.
    pub fn close_cleanly(&mut self) {
        self.recorder.tick(&self.engine, Instant::now());
        self.recorder.close_cleanly();
    }
}
