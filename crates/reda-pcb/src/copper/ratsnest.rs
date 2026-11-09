//! The Ratsnest: per Net, a minimum spanning tree over the islands its copper forms.
//!
//! Copper already joined by copper is one island, so a connection drops out of the
//! Ratsnest as soon as copper makes it. Each line joins the closest pair of anchors (Pad
//! centres, Via centres, Track ends) of two islands.

use std::collections::BTreeSet;

use reda_model::Uuid;
use reda_model::geom::Point;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Copper, Item};

/// One connection the Net still needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RatsnestLine {
    pub net: Uuid,
    pub from: Point,
    pub to: Point,
    /// The copper at `from`.
    pub from_item: Item,
    /// The copper at `to`.
    pub to_item: Item,
}

/// Where a Ratsnest line may start on a partly routed island.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RatsnestEnds {
    /// Smart Track Ends (the default): at any copper, Track ends included, so a partly
    /// routed connection's line starts where its Track stops.
    #[default]
    SmartTrackEnds,
    /// At the island's Pads and Vias; an island of bare Tracks still uses their ends.
    Pads,
}

impl Copper {
    /// The Ratsnest of every Net under Smart Track Ends, Nets in UUID order, each Net's
    /// lines in the order the spanning tree takes them.
    pub fn ratsnest(&self) -> Vec<RatsnestLine> {
        self.ratsnest_with(RatsnestEnds::SmartTrackEnds)
    }

    /// The Ratsnest of every Net, starting lines as `ends` says.
    pub fn ratsnest_with(&self, ends: RatsnestEnds) -> Vec<RatsnestLine> {
        self.nets()
            .into_iter()
            .flat_map(|net| self.net_ratsnest_with(net, ends))
            .collect()
    }

    /// The Ratsnest of one Net under Smart Track Ends: one line fewer than it has islands.
    pub fn net_ratsnest(&self, net: Uuid) -> Vec<RatsnestLine> {
        self.net_ratsnest_with(net, RatsnestEnds::SmartTrackEnds)
    }

    /// The Ratsnest of one Net, starting lines as `ends` says.
    pub fn net_ratsnest_with(&self, net: Uuid, ends: RatsnestEnds) -> Vec<RatsnestLine> {
        let islands: Vec<Vec<(Point, Item)>> = self
            .islands(net)
            .into_iter()
            .map(|island| {
                let mut anchors: Vec<(Point, Item)> = island
                    .into_iter()
                    .flat_map(|item| self.info[&item].anchors.iter().map(move |p| (*p, item)))
                    .collect();
                let bare = |(_, item): &(Point, Item)| matches!(item, Item::Track { .. });
                if ends == RatsnestEnds::Pads && anchors.iter().any(|a| !bare(a)) {
                    anchors.retain(|a| !bare(a));
                }
                anchors
            })
            .collect();
        let n = islands.len();
        if n < 2 {
            return Vec::new();
        }
        // Prim's algorithm over the complete graph of islands.
        let mut best: Vec<Option<Link>> = vec![None; n];
        let mut joined = vec![false; n];
        joined[0] = true;
        for j in 1..n {
            best[j] = Some(closest(&islands[0], &islands[j]));
        }
        let mut lines = Vec::with_capacity(n - 1);
        for _ in 1..n {
            let (next, link) = (0..n)
                .filter(|j| !joined[*j])
                .filter_map(|j| best[j].map(|l| (j, l)))
                .min_by_key(|(j, l)| (l.distance, *j))
                .expect("an island left to join");
            joined[next] = true;
            lines.push(RatsnestLine {
                net,
                from: link.from.0,
                to: link.to.0,
                from_item: link.from.1,
                to_item: link.to.1,
            });
            for j in 0..n {
                if !joined[j] {
                    let link = closest(&islands[next], &islands[j]);
                    if best[j].is_none_or(|b| link.distance < b.distance) {
                        best[j] = Some(link);
                    }
                }
            }
        }
        lines
    }
}

/// How many of a Net's connections copper makes: a Net with `n` Pads (Footprint Pads and
/// Free Pads) needs `n - 1`, and each island of its copper holding a Pad beyond the first
/// is one not made yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Connections {
    /// The connections the Net needs.
    pub total: usize,
    /// Those copper makes.
    pub made: usize,
}

impl Connections {
    /// The share made, in percent: 100 when there is nothing to make.
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            self.made as f64 * 100.0 / self.total as f64
        }
    }
}

impl std::ops::Add for Connections {
    type Output = Connections;
    fn add(self, other: Connections) -> Connections {
        Connections {
            total: self.total + other.total,
            made: self.made + other.made,
        }
    }
}

impl Copper {
    /// How many of `net`'s connections copper makes.
    pub fn connections(&self, net: Uuid) -> Connections {
        let is_pad = |item: &Item| matches!(item, Item::Pad { .. } | Item::FreePad { .. });
        let mut pads = 0;
        let mut islands = 0;
        for island in self.islands(net) {
            let n = island.iter().filter(|i| is_pad(i)).count();
            pads += n;
            islands += usize::from(n > 0);
        }
        let total = pads.saturating_sub(1);
        Connections {
            total,
            made: total - islands.saturating_sub(1),
        }
    }

    /// How many connections copper makes over every Net.
    pub fn all_connections(&self) -> Connections {
        self.nets()
            .into_iter()
            .map(|net| self.connections(net))
            .fold(Connections::default(), |a, b| a + b)
    }
}

#[derive(Clone, Copy, Debug)]
struct Link {
    distance: i128,
    from: (Point, Item),
    to: (Point, Item),
}

fn closest(a: &[(Point, Item)], b: &[(Point, Item)]) -> Link {
    let mut best: Option<Link> = None;
    for pa in a {
        for pb in b {
            let dx = (pa.0.x.0 - pb.0.x.0) as i128;
            let dy = (pa.0.y.0 - pb.0.y.0) as i128;
            let distance = dx * dx + dy * dy;
            if best.is_none_or(|l| distance < l.distance) {
                best = Some(Link {
                    distance,
                    from: *pa,
                    to: *pb,
                });
            }
        }
    }
    best.expect("islands have anchors")
}

/// Which Ratsnest lines the canvas shows: view state, never stored in the Document.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RatsnestVisibility {
    /// Hides the whole Ratsnest.
    pub hidden: bool,
    /// Nets whose lines are hidden.
    pub hidden_nets: BTreeSet<Uuid>,
    /// Footprints whose lines are hidden: every line ending on one of their Pads.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden_footprints: BTreeSet<Uuid>,
}

impl RatsnestVisibility {
    /// Whether the lines of `net` show.
    pub fn shows(&self, net: Uuid) -> bool {
        !self.hidden && !self.hidden_nets.contains(&net)
    }

    /// Shows or hides one Net's lines.
    pub fn set_net(&mut self, net: Uuid, visible: bool) {
        if visible {
            self.hidden_nets.remove(&net);
        } else {
            self.hidden_nets.insert(net);
        }
    }

    /// The lines that show.
    pub fn visible<'a>(
        &'a self,
        lines: &'a [RatsnestLine],
    ) -> impl Iterator<Item = &'a RatsnestLine> + 'a {
        lines.iter().filter(|l| self.shows(l.net))
    }
}
