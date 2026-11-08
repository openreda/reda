//! The test-engine builder: a headless engine with deterministic inputs.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use reda_engine::Engine;
use reda_engine::command::{CommandError, FixedClock, SeededIds, Trash, UserDirs};
use serde_json::Value;

/// A headless [`Engine`] with `SeededIds::new(0)` and `FixedClock::EPOCH`, so UUIDs run
/// `00000000-0000-4000-8000-000000000001`, `…002`, … and saved files are reproducible.
/// It has no per-user folders, so it never reads the real user's Preferences, and no
/// starter Library (see [`test_engine_with_library`]); use
/// [`test_engine_in`] for tests of per-user files.
///
/// It derefs to [`Engine`]; [`TestEngine::run_ok`] and [`TestEngine::run_err`] add the
/// panicking helpers scenario tests want.
pub fn test_engine() -> TestEngine {
    TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .version(crate::TEST_VERSION)
            .user_dirs(None)
            .starter_library(None)
            .build(),
    )
}

/// Like [`test_engine`], with the per-user folders under `home`, as `REDA_HOME=home` would
/// put them.
pub fn test_engine_in(home: &Path) -> TestEngine {
    TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .version(crate::TEST_VERSION)
            .user_dirs(Some(UserDirs::under(home)))
            .starter_library(None)
            .build(),
    )
}

/// The repo's starter Library, `library/starter`: what scenario scripts place Parts from.
pub fn starter_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

/// Like [`test_engine`], with the Library folder `library` registered as the starter
/// Library, for tests that place Parts.
pub fn test_engine_with_library(library: &Path) -> TestEngine {
    TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .version(crate::TEST_VERSION)
            .user_dirs(None)
            .starter_library(Some(library.to_owned()))
            .build(),
    )
}

/// Like [`test_engine`], with a trash that moves deleted Documents' files into the folder
/// `trash` (created as needed), for tests of what saving does with them.
pub fn test_engine_with_trash(trash: &Path) -> TestEngine {
    TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .version(crate::TEST_VERSION)
            .user_dirs(None)
            .starter_library(None)
            .trash(FolderTrash(trash.to_owned()))
            .build(),
    )
}

/// A [`Trash`] that moves files into a folder, keeping their names; a later file of the
/// same name replaces an earlier one.
#[derive(Debug)]
pub struct FolderTrash(pub PathBuf);

impl Trash for FolderTrash {
    fn trash(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.0)?;
        let name = path
            .file_name()
            .ok_or_else(|| std::io::Error::other("no file name"))?;
        std::fs::rename(path, self.0.join(name))
    }
}

/// See [`test_engine`].
pub struct TestEngine(pub Engine);

impl TestEngine {
    /// Runs a Command and returns its result, panicking with the error if it fails.
    #[track_caller]
    pub fn run_ok(&mut self, id: &str, params: Value) -> Value {
        match self.0.run(id, params.clone()) {
            Ok(value) => value,
            Err(err) => panic!("{id} {params} failed: {err}"),
        }
    }

    /// Runs a Command that must fail and returns its error.
    #[track_caller]
    pub fn run_err(&mut self, id: &str, params: Value) -> CommandError {
        match self.0.run(id, params.clone()) {
            Ok(value) => panic!("{id} {params} should have failed but returned {value}"),
            Err(err) => err,
        }
    }
}

impl Deref for TestEngine {
    type Target = Engine;
    fn deref(&self) -> &Engine {
        &self.0
    }
}

impl DerefMut for TestEngine {
    fn deref_mut(&mut self) -> &mut Engine {
        &mut self.0
    }
}
