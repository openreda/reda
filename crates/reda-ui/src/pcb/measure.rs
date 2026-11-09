//! Measuring distances on the board (docs/spec/v0.1/pcb.md#measuring): the measure Tool
//! (`Mod`+M) takes two clicks per measurement and repeats until Esc or a right-click.
//! Each finished measurement stays on the canvas for the session (never saved) and is
//! logged to Messages, where a double-click shows it again; Shift+C
//! (`view.highlight.clear`) clears the canvas, not the log.

use reda_engine::model::Severity;

use super::draw::Measurement;
use crate::messages::Note;
use crate::shell::Shell;
use crate::tr;

/// The source of the Messages rows measurements log.
pub const SOURCE: &str = "measure";

/// The session's measurements, in the engine's view state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Measurements {
    /// Every measurement taken, in order; Messages lists these.
    pub log: Vec<Measurement>,
    /// What the canvas shows.
    pub shown: Vec<Measurement>,
}

/// What the canvas shows now.
pub fn shown(shell: &Shell) -> Vec<Measurement> {
    shell
        .engine()
        .view()
        .get::<Measurements>()
        .map(|m| m.shown.clone())
        .unwrap_or_default()
}

fn state(shell: &mut Shell) -> Option<&mut Measurements> {
    shell.engine_mut().view_mut().get_mut::<Measurements>()
}

/// A finished measurement: shown, and logged to Messages.
pub(crate) fn record(shell: &mut Shell, m: Measurement) {
    let Some(state) = state(shell) else {
        return;
    };
    state.log.push(m);
    state.shown.push(m);
    let log = state.log.clone();
    let document = shell
        .engine()
        .project()
        .and_then(|p| p.pcb())
        .map(|p| p.uuid);
    let units = shell.board_view().units;
    let notes = log
        .iter()
        .map(|m| Note {
            source: SOURCE.to_owned(),
            document,
            severity: Severity::Warning,
            text: text(m, units),
        })
        .collect();
    shell.set_notes(SOURCE, notes);
}

/// The Messages row's words: the direct, X and Y distances.
pub fn text(m: &Measurement, units: reda_engine::preferences::Units) -> String {
    let length = |n| super::view::format_length(n, units);
    tr!(
        "message-measure",
        distance = length(m.distance()),
        dx = length(reda_engine::model::geom::Nm((m.to.x - m.from.x).0.abs())),
        dy = length(reda_engine::model::geom::Nm((m.to.y - m.from.y).0.abs()))
    )
}

/// A double-click on Messages row `note` (an index into every note): shows its
/// measurement on the canvas again. Returns false when the row is not a measurement.
pub(crate) fn show_again(shell: &mut Shell, note: usize) -> bool {
    let notes = shell.messages().notes();
    if notes.get(note).is_none_or(|n| n.source != SOURCE) {
        return false;
    }
    let nth = notes[..note].iter().filter(|n| n.source == SOURCE).count();
    if let Some(state) = state(shell)
        && let Some(m) = state.log.get(nth).copied()
        && !state.shown.contains(&m)
    {
        state.shown.push(m);
    }
    true
}

/// Shift+C: the canvas shows no measurements.
pub(crate) fn clear(state: &mut Measurements) {
    state.shown.clear();
}

/// The source of the Messages rows a selection length logs.
pub const LENGTH_SOURCE: &str = "measure_length";

/// After `pcb.measure.length` from the window: logs the total to Messages.
pub(crate) fn after_run(shell: &mut Shell, id: &str, out: &serde_json::Value) {
    if id != "pcb.measure.length" {
        return;
    }
    let result = out.get("result").unwrap_or(out);
    let (Some(length), Some(tracks)) = (result["length"].as_i64(), result["tracks"].as_u64())
    else {
        return;
    };
    let units = shell.board_view().units;
    let text = tr!(
        "message-measure-length",
        length = super::view::format_length(reda_engine::model::geom::Nm(length), units),
        count = tracks
    );
    let document = shell
        .engine()
        .project()
        .and_then(|p| p.pcb())
        .map(|p| p.uuid);
    let mut notes: Vec<Note> = shell
        .messages()
        .notes()
        .iter()
        .filter(|n| n.source == LENGTH_SOURCE)
        .cloned()
        .collect();
    notes.push(Note {
        source: LENGTH_SOURCE.to_owned(),
        document,
        severity: Severity::Warning,
        text,
    });
    shell.set_notes(LENGTH_SOURCE, notes);
}
