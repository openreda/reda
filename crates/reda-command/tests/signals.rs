//! Long-running Commands: a cancellation token and a progress reporter, which MCP and the
//! GUI map onto their own progress and Cancel.

mod support;

use std::sync::{Arc, Mutex};

use reda_command::{
    CancelToken, Command, CommandError, Context, Env, ErrorCode, Kind, Meta, Progress, Signals,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use support::Harness;

/// An edit that renames the Project once per step, reporting progress before each step
/// and checking for cancellation after it. It cancels its own token at `cancel_at`, the
/// way a caller would from another thread.
struct Count {
    cancel_at: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CountParams {
    steps: u32,
}

impl Command for Count {
    const META: Meta = Meta {
        id: "test.count",
        title: "Count",
        description: "Renames the Project once per step.",
        category: "project",
        icon: "count",
        kind: Kind::Edit,
    };
    type Params = CountParams;
    type Output = u32;

    fn run(&self, cx: &mut Context<'_>, p: CountParams) -> Result<u32, CommandError> {
        for step in 0..p.steps {
            cx.progress(f64::from(step), Some(f64::from(p.steps)), "counting");
            let tx = cx.tx()?;
            let mut props = tx.project().props().clone();
            props.name = format!("step {step}");
            tx.replace(reda_model::Object::Project(props))?;
            if self.cancel_at == Some(step) {
                cx.signals().cancel.cancel();
            }
            cx.check_cancelled()?;
        }
        Ok(p.steps)
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(f64, Option<f64>, String)>>);

impl Progress for Recorder {
    fn report(&self, progress: f64, total: Option<f64>, message: &str) {
        self.0
            .lock()
            .unwrap()
            .push((progress, total, message.to_owned()));
    }
}

fn harness(cancel_at: Option<u32>) -> Harness {
    let mut h = Harness::with_project();
    h.registry.add(Count { cancel_at });
    h
}

fn run(h: &mut Harness, signals: &Signals) -> Result<serde_json::Value, CommandError> {
    let mut env = Env {
        ids: &mut h.ids,
        clock: &h.clock,
        fonts: &reda_command::Fonts::default(),
        headless: true,
    };
    h.registry.run_with(
        &mut h.session,
        &mut env,
        "test.count",
        json!({"steps": 3}),
        signals,
    )
}

#[test]
fn progress_reaches_the_reporter() {
    let mut h = harness(None);
    let recorder = Arc::new(Recorder::default());
    let signals = Signals::new(CancelToken::new()).with_progress(recorder.clone());
    let out = run(&mut h, &signals).unwrap();
    assert_eq!(out["result"], 3);
    let reports = recorder.0.lock().unwrap().clone();
    assert_eq!(
        reports,
        [
            (0.0, Some(3.0), "counting".to_owned()),
            (1.0, Some(3.0), "counting".to_owned()),
            (2.0, Some(3.0), "counting".to_owned()),
        ]
    );
}

#[test]
fn a_cancelled_edit_rolls_back_fully() {
    let mut h = harness(Some(1));
    let before = h.name();
    let err = run(&mut h, &Signals::new(CancelToken::new())).unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(err.details, Some(json!({"cancelled": true})));
    assert!(err.is_cancelled());
    assert_eq!(h.name(), before);
    assert!(h.session.history.dirty_scopes().is_empty());
}

#[test]
fn a_token_cancelled_before_the_run_stops_at_the_first_check() {
    let mut h = harness(None);
    let cancel = CancelToken::new();
    cancel.cancel();
    let err = run(&mut h, &Signals::new(cancel.clone())).unwrap_err();
    assert!(err.is_cancelled());
    assert_eq!(h.name(), Harness::with_project().name());
}

#[test]
fn plain_run_has_no_reporter_and_is_never_cancelled() {
    let mut h = harness(None);
    let out = h.run("test.count", json!({"steps": 2})).unwrap();
    assert_eq!(out["result"], 2);
}
