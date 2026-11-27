//! The corpus report: a Markdown table with one row per model, and the trial's tiers
//! counted against OpenCascade's result.

use std::fmt::Write as _;

use crate::grade::Graded;
use crate::run::Outcome;

/// OpenCascade's result in the trial: models at the right volume, of 58. Reported, never a
/// gate (docs/spec/v0.3/testing.md#model-corpus).
pub const TARGET: (usize, usize) = (54, 58);

/// The trial's table, counted over a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    /// Models in the manifest, and those found in the corpus folder.
    pub models: usize,
    pub present: usize,
    /// Read and meshed without a crash, a timeout or a refusal.
    pub read: usize,
    pub every_face: usize,
    /// Every face meshed and the volume within 1 %.
    pub right_volume: usize,
    /// At the right volume and closed.
    pub closed: usize,
    /// Models present whose files have colours, and those of which colours were read.
    pub with_colours: usize,
    pub colours_read: usize,
    pub crashes: usize,
    pub timeouts: usize,
}

impl Summary {
    pub fn of(graded: &[Graded]) -> Summary {
        let count = |f: &dyn Fn(&Graded) -> bool| graded.iter().filter(|g| f(g)).count();
        let present = |g: &Graded| g.outcome != Outcome::Missing;
        Summary {
            models: graded.len(),
            present: count(&present),
            read: count(&|g| g.measured().is_some()),
            every_face: count(&|g| g.every_face),
            right_volume: count(&|g| g.right_volume),
            closed: count(&|g| g.closed),
            with_colours: count(&|g| present(g) && g.colours_read.is_some()),
            colours_read: count(&|g| g.colours_read == Some(true)),
            crashes: count(&|g| matches!(g.outcome, Outcome::Crash(_))),
            timeouts: count(&|g| matches!(g.outcome, Outcome::Timeout(_))),
        }
    }
}

/// The report, in Markdown.
pub fn render(graded: &[Graded]) -> String {
    let mut out = String::from("## STEP model corpus\n\n");
    out += "| Model | Grade | Faces meshed | Volume / FreeCAD's | Closed | Colours | Box off (mm) \
            | Triangles | Time (s) | Note |\n";
    out += "|---|---|---|---|---|---|---|---|---|---|\n";
    for g in graded {
        let m = g.measured();
        let faces = m.map_or("".into(), |m| format!("{} of {}", m.faces_meshed, m.faces));
        let ratio = g.volume_ratio.map_or("".into(), |r| format!("{r:.4}"));
        let closed = m.map_or("", |m| if m.closed { "yes" } else { "no" });
        let colours = match (m, g.colours_read) {
            (Some(m), _) if m.colours > 0 => m.colours.to_string(),
            (Some(_), Some(false)) => "none read".into(),
            (Some(_), _) => "none in the file".into(),
            _ => "".into(),
        };
        let bbox = g.bbox_off.map_or("".into(), |b| format!("{b:.3}"));
        let triangles = m.map_or("".into(), |m| m.triangles.to_string());
        let secs = m.map_or("".into(), |m| format!("{:.3}", m.secs));
        let note = match &g.outcome {
            Outcome::Refused(e) | Outcome::Crash(e) => e.clone(),
            Outcome::Timeout(t) => format!("stopped after {} s", t.as_secs_f64()),
            Outcome::Missing => "not in the corpus folder".into(),
            Outcome::Measured(m) => {
                let mut note = String::new();
                if g.protocol_agrees == Some(false) {
                    note = format!("read as {}; ", m.protocol);
                }
                match m.warnings {
                    0 => {}
                    1 => note += &m.first_warning,
                    n => note += &format!("{} (and {} more warnings)", m.first_warning, n - 1),
                }
                note
            }
        };
        let cells = [
            g.name.as_str(),
            g.grade.word(),
            &faces,
            &ratio,
            closed,
            &colours,
            &bbox,
            &triangles,
            &secs,
            &note,
        ];
        let cells: Vec<String> = cells.iter().map(|c| cell(c)).collect();
        writeln!(out, "| {} |", cells.join(" | ")).unwrap();
    }
    let s = Summary::of(graded);
    let (target, of) = TARGET;
    writeln!(
        out,
        "\n**Right volume: {} of {}** (every face meshed, volume within 1 % of FreeCAD's). \
         Target: OpenCascade's {target} of {of}; reported, not a gate.\n",
        s.right_volume, s.models
    )
    .unwrap();
    writeln!(
        out,
        "| Present | Read | Every face meshed | + right volume | + closed | Colours read | \
         Crashes | Timeouts |\n|---|---|---|---|---|---|---|---|\n| {} of {} | {} | {} | {} | \
         {} | {} of {} | {} | {} |",
        s.present,
        s.models,
        s.read,
        s.every_face,
        s.right_volume,
        s.closed,
        s.colours_read,
        s.with_colours,
        s.crashes,
        s.timeouts
    )
    .unwrap();
    out
}

/// A table cell: one line, no column breaks, at most 200 characters.
fn cell(s: &str) -> String {
    let s = s.replace('|', "\\|").replace(['\n', '\r'], " ");
    match s.char_indices().nth(200) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s,
    }
}
