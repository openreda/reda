//! What a long-running Command uses to talk to its caller while it runs: a cancellation
//! token and a progress reporter.
//!
//! Commands stay synchronous; a caller that wants to cancel holds a clone of the
//! [`CancelToken`] and cancels it from another thread. The Command polls
//! [`crate::Context::check_cancelled`] between steps; the `Err` it returns rolls an edit back
//! like any other failure. MCP maps these onto its progress notifications and
//! `notifications/cancelled`, the GUI onto a progress bar with Cancel.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A shared flag a caller sets to ask a running Command to stop. Clones share the flag.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> CancelToken {
        CancelToken::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

impl fmt::Debug for CancelToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CancelToken({})", self.is_cancelled())
    }
}

/// Receives progress from a running Command. `progress` should increase; `total` is
/// optional. Reports are advisory and may be dropped.
pub trait Progress: Send + Sync {
    fn report(&self, progress: f64, total: Option<f64>, message: &str);
}

/// The cancellation token and the optional progress reporter of one run.
#[derive(Default)]
pub struct Signals {
    pub cancel: CancelToken,
    progress: Option<Arc<dyn Progress>>,
}

impl Signals {
    pub fn new(cancel: CancelToken) -> Signals {
        Signals {
            cancel,
            progress: None,
        }
    }

    pub fn with_progress(mut self, progress: Arc<dyn Progress>) -> Signals {
        self.progress = Some(progress);
        self
    }

    /// Passes a progress report to the reporter, if there is one. Commands report through
    /// [`crate::Context::progress`]; a bridge relays reports from the far side here.
    pub fn report(&self, progress: f64, total: Option<f64>, message: &str) {
        if let Some(reporter) = &self.progress {
            reporter.report(progress, total, message);
        }
    }
}

impl fmt::Debug for Signals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signals")
            .field("cancel", &self.cancel)
            .field("progress", &self.progress.is_some())
            .finish()
    }
}
