//! The test-engine builder: a headless engine with deterministic inputs.

use std::ops::{Deref, DerefMut};

use reda_engine::Engine;
use reda_engine::command::{CommandError, FixedClock, SeededIds};
use serde_json::Value;

/// A headless [`Engine`] with `SeededIds::new(0)` and `FixedClock::EPOCH`, so UUIDs run
/// `00000000-0000-4000-8000-000000000001`, `…002`, … and saved files are reproducible.
///
/// It derefs to [`Engine`]; [`TestEngine::run_ok`] and [`TestEngine::run_err`] add the
/// panicking helpers scenario tests want.
pub fn test_engine() -> TestEngine {
    TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .build(),
    )
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
