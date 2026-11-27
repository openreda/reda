//! Grading one model as the trial did (#442, `tools/score.py` and `tools/tiers.py`): a model
//! is at the **right volume** when every face meshed and the mesh's volume is within 1 % of
//! FreeCAD's, and **closed** when, besides, every solid's mesh closes up. A model FreeCAD
//! gave no volume cannot be at the right volume.

use crate::manifest::Entry;
use crate::run::{Measured, Outcome};

/// The first test a model fails, in the trial's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    Missing,
    Crash,
    Timeout,
    /// The reader refused the file.
    Refused,
    /// Some faces were not meshed.
    Partial,
    /// The volume is more than 1 % off, or there is no reference.
    Volume,
    /// The mesh does not close up.
    Open,
    Good,
}

impl Grade {
    pub fn word(self) -> &'static str {
        match self {
            Grade::Missing => "missing",
            Grade::Crash => "crash",
            Grade::Timeout => "timeout",
            Grade::Refused => "refused",
            Grade::Partial => "partial",
            Grade::Volume => "volume",
            Grade::Open => "open",
            Grade::Good => "good",
        }
    }
}

/// One model's grade and what it rests on.
#[derive(Clone, Debug, PartialEq)]
pub struct Graded {
    pub name: String,
    pub grade: Grade,
    pub outcome: Outcome,
    /// Every face meshed.
    pub every_face: bool,
    /// Every face meshed and the volume within 1 % of the reference.
    pub right_volume: bool,
    /// At the right volume, and closed.
    pub closed: bool,
    /// For a model whose file has colours, whether any were read.
    pub colours_read: Option<bool>,
    /// The mesh's volume over the reference.
    pub volume_ratio: Option<f64>,
    /// The largest distance, mm, between a corner of the mesh's box and the reference's.
    pub bbox_off: Option<f64>,
    /// Whether the reader took the file for the protocol its schema names.
    pub protocol_agrees: Option<bool>,
}

impl Graded {
    pub fn measured(&self) -> Option<&Measured> {
        match &self.outcome {
            Outcome::Measured(m) => Some(m),
            _ => None,
        }
    }
}

pub fn grade(entry: &Entry, outcome: Outcome) -> Graded {
    let mut g = Graded {
        name: entry.name.clone(),
        grade: match outcome {
            Outcome::Missing => Grade::Missing,
            Outcome::Crash(_) => Grade::Crash,
            Outcome::Timeout(_) => Grade::Timeout,
            Outcome::Refused(_) => Grade::Refused,
            Outcome::Measured(_) => Grade::Good,
        },
        outcome,
        every_face: false,
        right_volume: false,
        closed: false,
        // A model that was not read showed none of its colours.
        colours_read: entry.colours.then_some(false),
        volume_ratio: None,
        bbox_off: None,
        protocol_agrees: None,
    };
    let Outcome::Measured(m) = &g.outcome else {
        return g;
    };
    g.every_face = m.faces > 0 && m.faces_meshed == m.faces;
    g.volume_ratio = entry.volume.map(|v| m.volume / v);
    let volume_ok = entry
        .volume
        .is_some_and(|v| (m.volume - v).abs() <= 0.01 * v.abs());
    g.right_volume = g.every_face && volume_ok;
    g.closed = g.right_volume && m.closed;
    g.grade = if !g.every_face {
        Grade::Partial
    } else if !volume_ok {
        Grade::Volume
    } else if !m.closed {
        Grade::Open
    } else {
        Grade::Good
    };
    g.colours_read = entry.colours.then_some(m.colours > 0);
    g.bbox_off = m.bbox.map(|(lo, hi)| {
        (0..3)
            .flat_map(|i| {
                [
                    (lo[i] - entry.bbox_min[i]).abs(),
                    (hi[i] - entry.bbox_max[i]).abs(),
                ]
            })
            .fold(0.0, f64::max)
    });
    g.protocol_agrees =
        Some(format!("{:?}", reda_step::Protocol::from_schema(&entry.schema)) == m.protocol);
    g
}
