//! Follow the narrow Track through bends. A taper is a strip of decreasing
//! width along that centreline, with the two turns of each corner joined.
use std::collections::BTreeSet;
use reda_model::{PcbDocument, TeardropFill, Track, TrackGeometry};
use reda_model::geom::{Nm, Point};
use crate::copper::{Copper,Item};
use crate::zone::{ARC_TOLERANCE, fill, poly};
use super::{Junction, obstacles::Obstacles, percent};

fn distance(a: Point, b: Point) -> f64 {
    ((a.x.0-b.x.0) as f64).hypot((a.y.0-b.y.0) as f64)
}

fn along(a: Point, b: Point, ratio: f64) -> Point {
    Point::new(Nm(a.x.0+((b.x.0-a.x.0) as f64*ratio).round() as i64),
               Nm(a.y.0+((b.y.0-a.y.0) as f64*ratio).round() as i64))
}

/// The centreline leaving the target, cut at the requested length, a branch,
/// a new width or a land. A Track whose end lies across the path of another
/// Track forms a junction even if the other Track has no vertex there.
pub(super) fn follow(pcb: &PcbDocument,copper: &Copper,first: &Track,start: Point,length: Nm) -> Vec<Point> {
    let mut path = vec![start];
    let mut used = BTreeSet::new();
    let mut track = first;
    let mut at = start;
    let mut remaining = length.0 as f64;
    while remaining > 0.0 && used.insert(track.uuid) {
        let TrackGeometry::Line { from, to } = track.geometry else { break };
        let end = if at == from { to } else { from };
        let d = distance(at,end);
        if d == 0.0 { break; }
        if remaining <= d {
            path.push(along(at,end,remaining/d));
            break;
        }
        path.push(end);
        remaining -= d;
        let others: Vec<_> = copper.items_at(track.layer,end).into_iter().filter(|i| {
            if *i == (Item::Track{uuid:track.uuid}) || i.is_artwork() { return false; }
            if let Item::Track{uuid} = i {
                let t = pcb.track(*uuid).unwrap();
                let (a,b) = t.geometry.ends();
                // A nearby round cap can cover this vertex without forming a branch.
                return reda_model::geom::grown::Shape::chain(&[a,b],0).contains(end);
            }
            true
        }).collect();
        let [Item::Track{uuid}] = others[..] else {break};
        let next = pcb.track(uuid).unwrap();
        let (a,b) = next.geometry.ends();
        if (a != end && b != end) || next.net != first.net || next.width != first.width || !next.teardrop {
            break;
        }
        at = end;
        track = next;
    }
    path.dedup();
    path
}

fn taper(path: &[Point], length: f64, base: f64, tip: f64, curved: bool) -> poly::Area {
    if length < 1. || base <= tip { return Vec::new(); }
    let radius = |s: f64| {
        let t = (1.0 - s / length).clamp(0.,1.);
        (tip + (base-tip)*if curved {t*t} else {t}) / 2.
    };
    // For a quadratic, the maximum chord error is its second difference
    // divided by four. Both offset edges use this bound in nanometres.
    let steps = if curved { ((base-tip)/(8.*ARC_TOLERANCE.0 as f64)).sqrt().ceil().max(1.) } else {1.};
    let mut strips = Vec::new();
    let mut s = 0.;
    let mut prior = None;
    for pair in path.windows(2) {
        let (a,b) = (pair[0],pair[1]);
        let d = distance(a,b);
        let span = d.min(length-s);
        if span <= 0. || d == 0. {break;}
        let normal = (-(b.y.0-a.y.0) as f64 / d, (b.x.0-a.x.0) as f64 / d);
        let section = |travel:f64| {
            let p = along(a,b,travel/d);
            let r = radius(s+travel);
            [1.,-1.].map(|side| poly::ip([p.x.0 + (r*normal.0*side).round() as i64,
                                            p.y.0 + (r*normal.1*side).round() as i64]))
        };
        let mut last = section(0.);
        if let Some([l,r]) = prior {
            let c = poly::ip([a.x.0,a.y.0]);
            strips.extend(poly::polygon(vec![l,c,last[0]]));
            strips.extend(poly::polygon(vec![r,c,last[1]]));
        }
        let n = (steps*span/length).ceil().max(1.) as usize;
        for i in 1..=n {
            let next = section(span*i as f64/n as f64);
            strips.extend(poly::polygon(vec![last[0], last[1], next[1], next[0]]));
            last = next;
        }
        prior = Some(last);
        s += span;
        if s >= length {break;}
    }
    poly::union(strips)
}

pub(super) fn fit(j: &Junction<'_>, obstacles: &Obstacles, inputs: String) -> TeardropFill {
    let requested = percent(j.size,j.rule.length,j.rule.max_length).0 as f64;
    let length = requested.min(j.path.windows(2).map(|p| distance(p[0],p[1])).sum());
    let width = percent(j.size,j.rule.width,j.rule.max_width).0 as f64;
    let tip = j.track.width.0 as f64;
    let shape = |scale: f64| taper(&j.path,length*scale,tip+(width-tip)*scale,tip,j.rule.curved);
    let mut area = shape(1.);
    let mut shortened = length + 1. < requested;
    if !obstacles.clears(&area) {
        let mut lo = 0.;
        let mut hi = 1.;
        area.clear();
        // Stop at board-grid precision. Only an independently cleared trial
        // can become the result, including when a corner changes its shape.
        while (hi-lo) * length.max(width) > 1. {
            let trial = (lo+hi)/2.;
            let candidate = shape(trial);
            if obstacles.clears(&candidate) {
                lo = trial;
                area = candidate;
            } else {
                hi = trial;
            }
        }
        if (width-tip)*lo < 1. || length*lo < 1. { area.clear(); }
        shortened = !area.is_empty();
    }
    TeardropFill {location:j.location,inputs,shortened,shapes:fill::to_shapes(&area)}
}
