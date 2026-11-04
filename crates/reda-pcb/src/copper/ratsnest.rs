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

impl Copper {
    /// The Ratsnest of every Net, Nets in UUID order, each Net's lines in the order the
    /// spanning tree takes them.
    pub fn ratsnest(&self) -> Vec<RatsnestLine> {
        self.nets()
            .into_iter()
            .flat_map(|net| self.net_ratsnest(net))
            .collect()
    }

    /// The Ratsnest of one Net: one line fewer than it has islands.
    pub fn net_ratsnest(&self, net: Uuid) -> Vec<RatsnestLine> {
        let islands: Vec<Vec<(Point, Item)>> = self
            .islands(net)
            .into_iter()
            .map(|island| {
                island
                    .into_iter()
                    .flat_map(|item| self.info[&item].anchors.iter().map(move |p| (*p, item)))
                    .collect()
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
