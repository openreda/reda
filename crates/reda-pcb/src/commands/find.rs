//! `pcb.find`: what the Find Panel lists on a board. It searches Designators, values, Net
//! names and Pads, which it names as `U1-6` (the Footprint's Designator, a hyphen and the
//! Pad number).

use std::cmp::Ordering;

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{PadRef, PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, pcb};
use crate::natural_cmp;

/// What `pcb.find` compares the text with.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PcbFindField {
    /// A Footprint instance's Designator.
    Designator,
    /// A Footprint instance's value.
    Value,
    /// The name of a PCB Net.
    Net,
    /// A Pad of a Footprint instance, as `U1-6`. Only the whole name matches, so `U1`
    /// doesn't list every Pad of U1.
    Pad,
}

/// How closely a match fits: results rank in this order.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FindRank {
    /// The whole text, case included.
    Exact,
    /// Part of the text, case included.
    Contains,
    /// The whole text or part of it, ignoring case.
    IgnoringCase,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PcbFindParams {
    /// The text to look for.
    pub text: String,
    /// What to search; every field by default.
    #[serde(default)]
    pub fields: Option<Vec<PcbFindField>>,
    /// Match the whole text rather than any part of it.
    #[serde(default)]
    pub exact: bool,
    /// Match case. Without it, matches that differ only in case rank last.
    #[serde(default)]
    pub match_case: bool,
}

/// What `pcb.find` found.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PcbFindResult {
    /// Exact matches, then those containing the text, then those ignoring case; within
    /// each, Designators, values, Nets and Pads, each in natural order of the text.
    pub matches: Vec<PcbFindMatch>,
}

/// One found object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PcbFindMatch {
    pub field: PcbFindField,
    pub rank: FindRank,
    /// The whole matching text, such as the Designator or `U1-6`.
    pub text: String,
    /// The Footprint instance, for Designator, value and Pad matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Uuid>,
    /// The Pad, for Pad matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pad: Option<PadRef>,
    /// The PCB Net, for Net matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    /// The copper on the Net (Tracks, Vias, Zones and Free Pads), for Net matches.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<Uuid>,
    /// The Footprint Pads on the Net, for Net matches.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pads: Vec<PadRef>,
}

pub struct PcbFind;

impl Command for PcbFind {
    const META: Meta = Meta {
        id: "pcb.find",
        title: "Find on Board",
        description: "Finds Footprints by Designator or value, Nets by name and Pads by name \
                      such as `U1-6`, as the Find Panel lists them on a board: exact matches \
                      first, then those containing the text, then those ignoring case.",
        category: "pcb",
        icon: "find",
        kind: Kind::Query,
    };
    type Params = PcbFindParams;
    type Output = PcbFindResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: PcbFindParams) -> Result<PcbFindResult, CommandError> {
        if p.text.is_empty() {
            return Err(CommandError::invalid_params("/text", "the text is empty"));
        }
        Ok(PcbFindResult {
            matches: find(pcb(cx)?, &p),
        })
    }
}

/// How `text` fits the query, if it matches at all.
fn rank(p: &PcbFindParams, text: &str, whole_only: bool) -> Option<FindRank> {
    if text == p.text {
        return Some(FindRank::Exact);
    }
    let whole_only = whole_only || p.exact;
    if !whole_only && text.contains(&p.text) {
        return Some(FindRank::Contains);
    }
    if p.match_case {
        return None;
    }
    let (text, needle) = (text.to_lowercase(), p.text.to_lowercase());
    let hit = match whole_only {
        true => text == needle,
        false => text.contains(&needle),
    };
    hit.then_some(FindRank::IgnoringCase)
}

/// Every match on `pcb`, in [`PcbFindResult::matches`] order.
pub fn find(pcb: &PcbDocument, p: &PcbFindParams) -> Vec<PcbFindMatch> {
    let fields = p.fields.clone().unwrap_or_else(|| {
        vec![
            PcbFindField::Designator,
            PcbFindField::Value,
            PcbFindField::Net,
            PcbFindField::Pad,
        ]
    });
    let wanted = |f: PcbFindField| fields.contains(&f);
    let blank = |field, rank, text: &str| PcbFindMatch {
        field,
        rank,
        text: text.to_owned(),
        footprint: None,
        pad: None,
        net: None,
        objects: Vec::new(),
        pads: Vec::new(),
    };
    let mut out = Vec::new();
    for instance in pcb.footprints() {
        let texts = [
            (PcbFindField::Designator, &instance.designator),
            (PcbFindField::Value, &instance.fields.value),
        ];
        for (field, text) in texts {
            if wanted(field)
                && let Some(rank) = rank(p, text, false)
            {
                out.push(PcbFindMatch {
                    footprint: Some(instance.uuid),
                    ..blank(field, rank, text)
                });
            }
        }
        if wanted(PcbFindField::Pad)
            && let Some(copy) = pcb.footprint_copy(instance.footprint)
        {
            for pad in copy.footprint.pads.iter().filter(|p| !p.number.is_empty()) {
                let name = format!("{}-{}", instance.designator, pad.number);
                if let Some(rank) = rank(p, &name, true) {
                    out.push(PcbFindMatch {
                        footprint: Some(instance.uuid),
                        pad: Some(PadRef {
                            footprint: instance.uuid,
                            pad: pad.uuid,
                        }),
                        ..blank(PcbFindField::Pad, rank, &name)
                    });
                }
            }
        }
    }
    if wanted(PcbFindField::Net) {
        for net in pcb.nets().filter(|n| !n.name.is_empty()) {
            let Some(rank) = rank(p, &net.name, false) else {
                continue;
            };
            let on = |n: Option<Uuid>| n == Some(net.uuid);
            let mut objects: Vec<Uuid> = (pcb.tracks().filter(|t| on(t.net)).map(|t| t.uuid))
                .chain(pcb.vias().filter(|v| on(v.net)).map(|v| v.uuid))
                .chain(pcb.zones().filter(|z| on(z.net)).map(|z| z.uuid))
                .chain(pcb.free_pads().filter(|f| on(f.net)).map(|f| f.uuid))
                .collect();
            objects.sort();
            let mut pads: Vec<PadRef> = pcb
                .footprints()
                .flat_map(|f| {
                    f.pads.iter().filter(|p| on(p.net)).map(|p| PadRef {
                        footprint: f.uuid,
                        pad: p.pad,
                    })
                })
                .collect();
            pads.sort();
            out.push(PcbFindMatch {
                net: Some(net.uuid),
                objects,
                pads,
                ..blank(PcbFindField::Net, rank, &net.name)
            });
        }
    }
    out.sort_by(order);
    out
}

fn order(a: &PcbFindMatch, b: &PcbFindMatch) -> Ordering {
    a.rank
        .cmp(&b.rank)
        .then(a.field.cmp(&b.field))
        .then_with(|| natural_cmp(&a.text, &b.text))
        .then(a.footprint.cmp(&b.footprint))
        .then(a.pad.cmp(&b.pad))
        .then(a.net.cmp(&b.net))
}
