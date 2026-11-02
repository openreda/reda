//! The Preferences as the window last read them, and noticing when `preferences.toml`
//! changes on disk, so a change made by another running instance (or by hand) applies here
//! too (docs/spec/v0.1/app.md#preferences).
//!
//! [`Watched`] keeps the values with a generation that moves on every change, so the
//! window can apply what depends on them (theme, UI scale) once per change. Noticing is a
//! `stat` of the file: the Shell compares it each frame, and a [`Watcher`] thread wakes an
//! idle window when it changes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use reda_engine::command::UserDirs;
use reda_engine::preferences::{self, Preferences};

/// What a file looked like: its modification time and length, or `None` while it is
/// missing.
type Stamp = Option<(Option<SystemTime>, u64)>;

fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok(), meta.len()))
}

/// The Preferences last read, and the file's stamp then.
#[derive(Clone, Debug, Default)]
pub struct Watched {
    values: Preferences,
    generation: u64,
    stamp: Stamp,
}

/// What a reload found.
#[derive(Clone, Debug, PartialEq)]
pub struct Reloaded {
    /// The values before the reload.
    pub before: Preferences,
    /// Keys in the file that were ignored, for Messages.
    pub warnings: Vec<String>,
}

impl Watched {
    /// The Preferences in `dirs` now (the defaults without a per-user folder), with their
    /// warnings.
    pub fn load(dirs: Option<&UserDirs>) -> (Watched, Vec<String>) {
        let mut watched = Watched::default();
        let warnings = match dirs {
            Some(dirs) => {
                watched.stamp = stamp(&dirs.preferences_file());
                let loaded = preferences::load(dirs);
                watched.values = loaded.values;
                loaded.warnings
            }
            None => Vec::new(),
        };
        (watched, warnings)
    }

    pub fn values(&self) -> &Preferences {
        &self.values
    }

    /// Moves on every time the values change.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the file looks different from when it was last read.
    pub fn changed_on_disk(&self, dirs: &UserDirs) -> bool {
        stamp(&dirs.preferences_file()) != self.stamp
    }

    /// Reads the file again. Returns what changed, or `None` when the values are the same.
    pub fn reload(&mut self, dirs: &UserDirs) -> Option<Reloaded> {
        let (fresh, warnings) = Watched::load(Some(dirs));
        self.stamp = fresh.stamp;
        if fresh.values == self.values {
            return None;
        }
        let before = std::mem::replace(&mut self.values, fresh.values);
        self.generation += 1;
        Some(Reloaded { before, warnings })
    }
}

/// A thread that checks `preferences.toml` twice a second and calls its waker when the
/// file changes, so an idle window notices another instance's edit. It stops when dropped.
pub struct Watcher {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// How often the [`Watcher`] looks.
const INTERVAL: Duration = Duration::from_millis(500);

impl Watcher {
    pub fn start(path: PathBuf, wake: impl Fn() + Send + 'static) -> Watcher {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::Builder::new()
            .name("preferences-watch".into())
            .spawn(move || {
                let mut seen = stamp(&path);
                while !stopping.load(Ordering::Relaxed) {
                    std::thread::sleep(INTERVAL);
                    let now = stamp(&path);
                    if now != seen {
                        seen = now;
                        wake();
                    }
                }
            })
            .ok();
        Watcher { stop, thread }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
