//! Derived copper at a Track junction. Each junction is rebuilt from its resolved
//! rule, the available path along the Track, and the obstacles around that path.
use std::collections::BTreeMap;

use crate::copper::{Copper, Item};
use crate::rules::teardrop;
use reda_command::{Fingerprint, Fonts};
use reda_model::geom::{Nm, Point};
use reda_model::{
    PcbDocument, RuleKind, TeardropFill, TeardropKind, TeardropLocation, TeardropTarget, Track,
    Uuid, ZoneFills,
};
use schemars::JsonSchema;
use serde::Serialize;

mod obstacles;
mod path;

#[derive(Clone, Debug, Default, Serialize, JsonSchema)]
pub struct TeardropStatus {
    pub generated: usize,
    pub shortened: usize,
    pub left_out: usize,
    pub shortened_items: Vec<TeardropLocation>,
    pub left_out_items: Vec<TeardropLocation>,
}

pub fn status(fills: Option<&ZoneFills>) -> TeardropStatus {
    let mut out = TeardropStatus::default();
    for drop in fills.into_iter().flat_map(|f| &f.teardrops) {
        if drop.shapes.is_empty() {
            out.left_out += 1;
            out.left_out_items.push(drop.location);
        } else if drop.shortened {
            out.shortened += 1;
            out.shortened_items.push(drop.location);
        } else {
            out.generated += 1;
        }
    }
    out
}

struct Junction<'a> {
    track: &'a Track,
    target: Item,
    location: TeardropLocation,
    rule: TeardropTarget,
    size: Nm,
    path: Vec<Point>,
    path_tracks: Vec<Uuid>,
}

fn key(location: TeardropLocation) -> (Uuid, Uuid, Uuid, i64, i64) {
    (
        location.track,
        location.target,
        location.layer,
        location.at.x.0,
        location.at.y.0,
    )
}

fn candidates<'a>(pcb: &'a PcbDocument, copper: &Copper) -> Vec<Junction<'a>> {
    let mut out = Vec::new();
    for track in pcb.tracks().filter(|t| t.teardrop) {
        let rule = teardrop(pcb, track.net, track.layer, TeardropKind::WidthChange);
        if !rule.enabled {
            continue;
        }
        let (from, to) = track.geometry.ends();
        for at in [from, to] {
            let others: Vec<_> = copper
                .items_at(track.layer, at)
                .into_iter()
                .filter(|i| {
                    !i.is_artwork()
                        && *i != (Item::Track { uuid: track.uuid })
                        && copper.net(*i) == track.net
                })
                .filter(|i| match *i {
                    Item::Track { uuid } => {
                        let (a, b) = pcb.track(uuid).expect("indexed Track").geometry.ends();
                        at.on_segment(a, b)
                    }
                    _ => true,
                })
                .collect();
            let [target @ Item::Track { uuid }] = others[..] else {
                continue;
            };
            let wide = pcb.track(uuid).expect("indexed Track");
            let (a, b) = wide.geometry.ends();
            if wide.net != track.net
                || !wide.teardrop
                || (at != a && at != b)
                || wide.width <= track.width
                || i128::from(track.width.0) * 100
                    >= i128::from(wide.width.0) * i128::from(rule.skip_ratio)
            {
                continue;
            }
            let length = percent(wide.width, rule.length, rule.max_length);
            let (path, path_tracks) = path::follow(pcb, copper, track, target, at, length);
            if path.len() >= 2 {
                out.push(Junction {
                    track,
                    target,
                    location: TeardropLocation {
                        track: track.uuid,
                        target: uuid,
                        layer: track.layer,
                        at,
                    },
                    rule,
                    size: wide.width,
                    path,
                    path_tracks,
                });
            }
        }
    }
    out.sort_by_key(|j| key(j.location));
    out
}

fn percent(size: Nm, percent: u32, cap: Nm) -> Nm {
    Nm((i128::from(size.0) * i128::from(percent) / 100).min(i128::from(cap.0)) as i64)
}

/// Staleness is checked from local inputs alone. Previously generated neighbours
/// are considered in the same order used to resolve competing Teardrops.
pub(crate) fn update(
    pcb: &PcbDocument,
    stored: Option<&ZoneFills>,
    fonts: &Fonts,
    regenerate: bool,
    step: &mut dyn FnMut(usize, usize) -> bool,
) -> Option<(Vec<TeardropFill>, usize)> {
    let saved: BTreeMap<_, _> = stored
        .into_iter()
        .flat_map(|f| &f.teardrops)
        .map(|d| (key(d.location), d))
        .collect();
    if !pcb.rules().any(|r| {
        r.kind == RuleKind::Teardrop
            && r.enabled
            && r.values
                .width_change
                .is_some_and(|v| v.enabled == Some(true))
    }) {
        return Some((Vec::new(), saved.len()));
    }
    let copper = Copper::build_with_fonts(pcb, fonts);
    let holes = crate::drill::holes(pcb);
    let mut drops = Vec::new();
    let mut stale = 0;
    let mut saved = saved;
    let junctions = candidates(pcb, &copper);
    for (index, j) in junctions.iter().enumerate() {
        if !step(index, junctions.len()) {
            return None;
        }
        let obstacles = obstacles::near(pcb, &copper, &holes, j, &drops);
        let mut hash = Fingerprint::new();
        hash.add(&1u32)
            .add(&j.location)
            .add(&j.target)
            .add(&j.track.geometry)
            .add(&j.track.net)
            .add(&j.track.width)
            .add(&j.rule)
            .add(&j.size)
            .add(&j.path);
        if let Item::Track { uuid } = j.target {
            hash.add(&pcb.track(uuid).expect("indexed Track").geometry);
        }
        obstacles.fingerprint(&mut hash);
        let inputs = hash.finish();
        let previous = saved.remove(&key(j.location));
        if let Some(d) = previous.filter(|d| d.inputs == inputs) {
            drops.push(d.clone());
        } else {
            stale += 1;
            if regenerate {
                drops.push(path::fit(j, &obstacles, inputs));
            } else if let Some(d) = previous {
                drops.push(d.clone());
            }
        }
    }
    Some((drops, stale + saved.len()))
}
