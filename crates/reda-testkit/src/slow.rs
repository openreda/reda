//! `test.slow`: a long-running edit for tests of progress and cancellation across a
//! bridge.

use std::time::{Duration, Instant};

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::Object;
use schemars::JsonSchema;
use serde::Deserialize;

/// Adds `test.slow {steps, wait}` to `registry`: it renames the Project to "slow", reports
/// `steps` progress steps ("working"), then with `wait` waits for cancellation, giving up
/// (and succeeding) after ten seconds. It returns `steps`.
pub fn slow_commands(registry: &mut Registry) {
    registry.add(Slow);
}

struct Slow;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SlowParams {
    steps: u32,
    wait: bool,
}

impl Command for Slow {
    const META: Meta = Meta {
        id: "test.slow",
        title: "Slow",
        description: "A long-running edit for tests.",
        category: "test",
        icon: "slow",
        kind: Kind::Edit,
    };
    type Params = SlowParams;
    type Output = u32;

    fn run(&self, cx: &mut Context<'_>, p: SlowParams) -> Result<u32, CommandError> {
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        props.name = "slow".into();
        tx.replace(Object::Project(props))?;
        for step in 1..=p.steps {
            cx.progress(f64::from(step), Some(f64::from(p.steps)), "working");
        }
        let start = Instant::now();
        while p.wait && start.elapsed() < Duration::from_secs(10) {
            cx.check_cancelled()?;
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(p.steps)
    }
}
