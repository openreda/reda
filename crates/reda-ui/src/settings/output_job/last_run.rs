//! The Last run page's model: the Output Manifest (`manifest.json`) in the job's
//! destination, read back. It reads the file, not anything the session remembers, so it
//! shows the last run after reopening the Project too, or on another machine with the
//! committed outputs. Each file it lists is checked against its size and SHA-256.

use std::path::{Path, PathBuf};

use reda_engine::output::sha256_hex;
use serde_json::Value;

/// The manifest's name in the destination, as `output.job.run` writes it.
pub const MANIFEST: &str = "manifest.json";

/// What the destination says about the last run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LastRun {
    /// No manifest: the job has not run into this destination.
    NotRun,
    /// A manifest that does not read as one, with why.
    Unreadable(String),
    /// The manifest, read.
    Read(Manifest),
}

/// A run, as its manifest records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// The destination folder it was read from.
    pub folder: PathBuf,
    /// When it ran, `YYYY-MM-DDTHH:MM:SSZ`.
    pub date: String,
    /// The REDA version that ran it.
    pub reda: String,
    /// The Project revision it ran on.
    pub revision: String,
    /// The Fab Preset copy it used.
    pub preset: String,
    /// The zip's path in the destination.
    pub zip: String,
    /// Every file it wrote, in run order.
    pub files: Vec<ManifestFile>,
    /// Its warnings, as `(check, message)`.
    pub warnings: Vec<(String, String)>,
}

impl Manifest {
    /// The files that changed or went missing since the run.
    pub fn changed(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.check != FileCheck::Same)
            .count()
    }
}

/// One file the manifest lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestFile {
    /// Its path in the destination, `/`-separated.
    pub path: String,
    pub size: u64,
    pub sha256: String,
    /// What it holds, such as `copper_top`.
    pub kind: String,
    /// How the file on disk compares now.
    pub check: FileCheck,
}

/// A listed file on disk now, against the manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileCheck {
    /// The same size and SHA-256.
    Same,
    /// It is there, but not as the run wrote it.
    Changed,
    /// It is gone.
    Missing,
}

/// Reads the manifest in `folder`.
pub fn read(folder: &Path) -> LastRun {
    let bytes = match std::fs::read(folder.join(MANIFEST)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LastRun::NotRun,
        Err(e) => return LastRun::Unreadable(e.to_string()),
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(e) => return LastRun::Unreadable(e.to_string()),
    };
    let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    let files = value["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let path = text(&f["path"]);
            let size = f["size"].as_u64().unwrap_or_default();
            let sha256 = text(&f["sha256"]);
            let check = check(folder, &path, size, &sha256);
            ManifestFile {
                path,
                size,
                sha256,
                kind: text(&f["kind"]),
                check,
            }
        })
        .collect();
    let warnings = value["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| (text(&w["check"]), text(&w["message"])))
        .collect();
    LastRun::Read(Manifest {
        folder: folder.to_path_buf(),
        date: text(&value["date"]),
        reda: text(&value["reda"]),
        revision: text(&value["revision"]),
        preset: text(&value["fab_preset"]["name"]),
        zip: text(&value["zip"]["path"]),
        files,
        warnings,
    })
}

/// How the file at `path` in `folder` compares with its manifest entry. Only a plain
/// relative path is looked up, so a hand-edited manifest can't reach outside the folder.
fn check(folder: &Path, path: &str, size: u64, sha256: &str) -> FileCheck {
    let plain = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', ':'])
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..");
    if !plain {
        return FileCheck::Missing;
    }
    let at = path.split('/').fold(folder.to_path_buf(), |p, s| p.join(s));
    match std::fs::read(at) {
        Ok(bytes) if bytes.len() as u64 == size && sha256_hex(&bytes) == sha256 => FileCheck::Same,
        Ok(_) => FileCheck::Changed,
        Err(_) => FileCheck::Missing,
    }
}
