//! Dev-only test support: goldens, the test-engine builder, the scenario harness.
//!
//! Every crate takes this as a dev-dependency only; a workspace test enforces that.

mod engine;
mod files;
mod pcb;
mod probe;
mod scenario;

pub use engine::{
    FolderTrash, TestEngine, starter_library, test_engine, test_engine_in, test_engine_with_library,
    test_engine_with_trash,
};
pub use files::{TempDir, read_tree};
pub use probe::{probe_commands, probe_engine};
pub use scenario::{run_scenarios, scenario_failures};

use std::path::Path;

/// Asserts that the file at `path` holds exactly `actual`.
///
/// With `REDA_BLESS=1` in the environment it writes `actual` to `path` instead, creating
/// parent directories as needed, so a reviewer sees the change as a diff.
#[track_caller]
pub fn assert_golden(path: impl AsRef<Path>, actual: &[u8]) {
    let path = path.as_ref();
    if std::env::var_os("REDA_BLESS").is_some_and(|v| v == "1") {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("creating {}: {e}", parent.display()));
        }
        std::fs::write(path, actual).unwrap_or_else(|e| panic!("blessing {}: {e}", path.display()));
        return;
    }
    let expected = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => panic!(
            "golden {} unreadable ({e}); rerun with REDA_BLESS=1 to create it",
            path.display()
        ),
    };
    if expected != actual {
        panic!(
            "golden {} differs from the actual output; rerun with REDA_BLESS=1 to accept the change\n\
             --- expected\n{}\n--- actual\n{}",
            path.display(),
            String::from_utf8_lossy(&expected),
            String::from_utf8_lossy(actual),
        );
    }
}
