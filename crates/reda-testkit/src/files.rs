//! Scratch folders and folder snapshots for tests that touch disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// An empty folder under the system temp dir, removed when dropped.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// A fresh, empty folder whose name contains `label`, the process id and a counter.
    pub fn new(label: &str) -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let n = COUNT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("reda-test-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .unwrap_or_else(|e| panic!("creating {}: {e}", path.display()));
        TempDir { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Every file under `dir`, keyed by its path relative to `dir` with `/` separators. A
/// missing folder reads as empty.
pub fn read_tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    walk(dir, "", &mut out);
    out
}

fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let entry = entry.expect("reading a folder entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = format!("{prefix}{name}");
        let path = entry.path();
        if path.is_dir() {
            walk(&path, &format!("{rel}/"), out);
        } else {
            let bytes =
                std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
            out.insert(rel, bytes);
        }
    }
}
