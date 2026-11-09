//! The routing width: where it starts, the width modes (`3`), the chooser's list
//! (`Shift+W`) and clipping to the Net's limits.

use std::collections::BTreeSet;

use reda_model::geom::{Nm, Point};
use reda_model::{Limit, PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::copper::{Copper, Item};
use crate::rules::net_rules;

/// Which of the Net's widths a route takes. Kept in `pcb.route.width_mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WidthMode {
    Min,
    #[default]
    Preferred,
    Max,
    /// The last value chosen in the chooser.
    User,
}

const MODES: [WidthMode; 4] = [
    WidthMode::Min,
    WidthMode::Preferred,
    WidthMode::Max,
    WidthMode::User,
];

/// The width `mode` gives on `net`: `None` for an absent limit or an empty `user`.
pub fn mode_width(
    pcb: &PcbDocument,
    net: Option<Uuid>,
    mode: WidthMode,
    user: Option<Nm>,
) -> Option<Nm> {
    let range = net_rules(pcb, net).track_width;
    match mode {
        WidthMode::Min => range.min,
        WidthMode::Preferred => Some(range.preferred),
        WidthMode::Max => range.max,
        WidthMode::User => user.map(|w| range.clip(w)),
    }
}

/// The mode `3` goes to after `mode`, skipping an absent limit and an empty `user`.
pub fn next_mode(
    pcb: &PcbDocument,
    net: Option<Uuid>,
    mode: WidthMode,
    user: Option<Nm>,
) -> WidthMode {
    let at = MODES.iter().position(|m| *m == mode).expect("a mode");
    (1..=MODES.len())
        .map(|i| MODES[(at + i) % MODES.len()])
        .find(|m| mode_width(pcb, net, *m, user).is_some())
        .unwrap_or(WidthMode::Preferred)
}

/// The width a route starting at `at` on `layer` takes: on a Track, that Track's width
/// (unless `pickup` is off); otherwise the width of `mode` for the Net, falling back to
/// `preferred`. A route from a bare Pad never looks at the Net's other copper. Clipped to
/// the Net's limits.
pub fn start_width(
    pcb: &PcbDocument,
    copper: &Copper,
    net: Option<Uuid>,
    start: (Uuid, Point),
    mode: WidthMode,
    user: Option<Nm>,
    pickup: bool,
) -> Nm {
    let (layer, at) = start;
    let picked = pickup
        .then(|| {
            copper
                .items_at(layer, at)
                .into_iter()
                .find_map(|i| match i {
                    Item::Track { uuid } => pcb.track(uuid).map(|t| t.width),
                    _ => None,
                })
        })
        .flatten();
    let range = net_rules(pcb, net).track_width;
    let width = picked
        .or_else(|| mode_width(pcb, net, mode, user))
        .unwrap_or(range.preferred);
    range.clip(width)
}

/// A typed or chosen width brought inside the Net's limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Clipped {
    pub width: Nm,
    /// The limit it was clipped to, for the status note; left out when it fitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<Limit>,
}

/// `width` clipped to `net`'s `min` and `max`, unless the limit is absent.
pub fn clip_width(pcb: &PcbDocument, net: Option<Uuid>, width: Nm) -> Clipped {
    let range = net_rules(pcb, net).track_width;
    match range.breach(width) {
        Some((limit, value)) => Clipped {
            width: value,
            limit: Some(limit),
        },
        None => Clipped { width, limit: None },
    }
}

/// Where a width in the chooser comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WidthSource {
    Min,
    Preferred,
    Max,
    /// `pcb.route.favourite_widths`.
    Favourite,
    /// A width already on the board.
    Board,
}

/// One row of the width chooser.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct WidthChoice {
    pub width: Nm,
    pub source: WidthSource,
}

/// The width chooser's list: `min`, `preferred` and `max` of the Net (absent limits left
/// out), then the favourites, then the widths already on the board, each width once
/// under its first source; favourites and board widths sorted.
pub fn width_choices(pcb: &PcbDocument, net: Option<Uuid>, favourites: &[Nm]) -> Vec<WidthChoice> {
    let range = net_rules(pcb, net).track_width;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut push = |width: Nm, source| {
        if seen.insert(width) {
            out.push(WidthChoice { width, source });
        }
    };
    if let Some(min) = range.min {
        push(min, WidthSource::Min);
    }
    push(range.preferred, WidthSource::Preferred);
    if let Some(max) = range.max {
        push(max, WidthSource::Max);
    }
    let favourites: BTreeSet<Nm> = favourites.iter().copied().collect();
    for w in favourites {
        push(w, WidthSource::Favourite);
    }
    let board: BTreeSet<Nm> = pcb.tracks().map(|t| t.width).collect();
    for w in board {
        push(w, WidthSource::Board);
    }
    out
}
