//! The copper of one Footprint in its own frame, and the two checks the Library check and
//! DRC share on it: Pads with different numbers touching (`shorted_copper`) and whether a
//! Net Tie's Pads make a valid tie (`net_tie_not_joined`). DRC runs them on the placed
//! copies, so both see the same geometry.

use crate::geom::grown::Shape;
use crate::geom::rotate;
use crate::{Footprint, Pad, PadKind, Side};

/// A Pad's copper in its Footprint's frame, relative to the Footprint origin.
pub fn pad_shape(pad: &Pad) -> Shape {
    Shape::outline(&pad.outline(), |p| {
        let (x, y) = rotate(p[0], p[1], pad.rotation);
        [x + pad.at.x.0, y + pad.at.y.0]
    })
}

/// Where a Pad has copper: on one side (SMD), on every copper Layer (plated through
/// hole), or nowhere (non-plated).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Copper {
    Side(Side),
    Through,
    Nowhere,
}

fn copper(pad: &Pad) -> Copper {
    match pad.kind {
        PadKind::Smd => Copper::Side(pad.side.unwrap_or(Side::Top)),
        PadKind::PlatedThroughHole => Copper::Through,
        PadKind::NonPlated => Copper::Nowhere,
    }
}

/// Whether two Pads have copper on a common Layer. SMD Pads on different sides don't.
fn share_layer(a: &Pad, b: &Pad) -> bool {
    match (copper(a), copper(b)) {
        (Copper::Nowhere, _) | (_, Copper::Nowhere) => false,
        (Copper::Side(x), Copper::Side(y)) => x == y,
        _ => true,
    }
}

/// Every pair of Pads (by index, the lesser first) whose copper touches or overlaps on a
/// common Layer, in index order.
pub fn touching_pads(footprint: &Footprint) -> Vec<(usize, usize)> {
    let shapes: Vec<Shape> = footprint.pads.iter().map(pad_shape).collect();
    let mut out = Vec::new();
    for (i, a) in footprint.pads.iter().enumerate() {
        for (j, b) in footprint.pads.iter().enumerate().skip(i + 1) {
            if share_layer(a, b) && shapes[i].touches(&shapes[j]) {
                out.push((i, j));
            }
        }
    }
    out
}

/// `shorted_copper` on a Footprint whose Part isn't a Net Tie: the touching Pad pairs
/// whose numbers differ.
pub fn shorted_pads(footprint: &Footprint) -> Vec<(usize, usize)> {
    touching_pads(footprint)
        .into_iter()
        .filter(|&(i, j)| footprint.pads[i].number != footprint.pads[j].number)
        .collect()
}

/// Why a Net Tie's Footprint isn't a valid tie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TieProblem {
    /// Its Pads aren't all SMD on one side.
    NotSmdOneSide,
    /// Its Pads have fewer than two different numbers.
    TooFewNumbers,
    /// Its Pads don't form one connected copper group.
    NotJoined,
}

impl TieProblem {
    /// Its name in the `reason` argument of `net_tie_not_joined`.
    pub fn id(self) -> &'static str {
        match self {
            TieProblem::NotSmdOneSide => "not_smd_one_side",
            TieProblem::TooFewNumbers => "too_few_numbers",
            TieProblem::NotJoined => "not_joined",
        }
    }
}

/// `net_tie_not_joined`: what keeps a Net Tie's Footprint from being a valid tie, if
/// anything. A tie is two or more SMD Pads on one side, with at least two different
/// numbers, that touch or overlap so they form one connected copper group.
pub fn tie_problem(footprint: &Footprint) -> Option<TieProblem> {
    let pads = &footprint.pads;
    let side = pads.first().map(copper);
    if pads.is_empty()
        || pads
            .iter()
            .any(|p| p.kind != PadKind::Smd || Some(copper(p)) != side)
    {
        return Some(TieProblem::NotSmdOneSide);
    }
    if pads.iter().all(|p| p.number == pads[0].number) {
        return Some(TieProblem::TooFewNumbers);
    }
    // Union-find over the touching pairs: one group when every Pad shares a root.
    let mut root: Vec<usize> = (0..pads.len()).collect();
    fn find(root: &mut [usize], mut i: usize) -> usize {
        while root[i] != i {
            root[i] = root[root[i]];
            i = root[i];
        }
        i
    }
    for (a, b) in touching_pads(footprint) {
        let (ra, rb) = (find(&mut root, a), find(&mut root, b));
        root[ra] = rb;
    }
    let first = find(&mut root, 0);
    if (1..pads.len()).any(|i| find(&mut root, i) != first) {
        return Some(TieProblem::NotJoined);
    }
    None
}
