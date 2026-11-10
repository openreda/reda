//! The Fab Notes: `<project>-fab-notes.txt`, UTF-8 plain text for a person at the fab, in
//! a fixed order: the board, its Stackup values, the minimum features of the board-wide
//! rules, holes with a `hole.tolerance`, Net Ties as deliberate shorts, the files in the
//! zip and the board's own `fab_notes` text. A section with nothing to say is left out.

use std::fmt::Write;

use reda_model::geom::{NM_PER_OZ, Nm, Point, contour_bounds, exact_decimal};
use reda_model::{ComponentType, Hole, PadKind, PcbDocument, RuleKind, RuleValues};
use reda_pcb::placed;

use crate::plot::{OutputFile, Plot};

/// The Fab Notes' file name for the Project `name`.
pub fn fab_notes_name(name: &str) -> String {
    format!("{name}-fab-notes.txt")
}

/// The Fab Notes of `plot`'s board, listing `zip` as the files in the zip (the Fab Notes
/// among them).
pub fn fab_notes_file(plot: &Plot<'_>, zip: &[String]) -> OutputFile {
    let props = plot.project.props();
    let mut out = String::new();
    let _ = writeln!(out, "Fab Notes for {}", props.name);
    let revision = props.properties.revision.trim();
    if !revision.is_empty() {
        let _ = writeln!(out, "Revision: {revision}");
    }
    let _ = writeln!(out, "Written by REDA {}", plot.version);
    if let Some(instance) = plot.instances.first() {
        let pcb = instance.pcb;
        board(&mut out, pcb);
        minimum_features(&mut out, pcb);
        tolerances(&mut out, plot);
        net_ties(&mut out, pcb);
    }
    section(&mut out, "Files in the zip", zip.to_vec());
    if let Some(notes) = plot
        .instances
        .first()
        .and_then(|i| i.pcb.setup().fab_notes.as_deref())
    {
        let _ = write!(out, "\nNotes\n{}", notes.trim_end());
        out.push('\n');
    }
    OutputFile {
        name: fab_notes_name(&props.name),
        function: "fab_notes",
        bytes: out.into_bytes(),
        attributes: None,
    }
}

/// A heading and its indented lines, after a blank line; nothing when there are no lines.
fn section(out: &mut String, heading: &str, lines: Vec<String>) {
    if lines.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n{heading}");
    for line in lines {
        let _ = writeln!(out, "  {line}");
    }
}

/// The Board Outline's width and height, arcs included; `None` without an outline.
pub(crate) fn board_size(pcb: &PcbDocument) -> Option<(Nm, Nm)> {
    let bounds = contour_bounds(&pcb.outline()?.vertices)?;
    Some((bounds.max.x - bounds.min.x, bounds.max.y - bounds.min.y))
}

/// The name of copper Layer `i` (from 0) of `count`: `L1 (top)`, `L2`, `L4 (bottom)`.
fn copper_name(i: usize, count: usize) -> String {
    match i {
        0 => "L1 (top)".into(),
        i if i + 1 == count => format!("L{} (bottom)", i + 1),
        i => format!("L{}", i + 1),
    }
}

fn board(out: &mut String, pcb: &PcbDocument) {
    let mut lines = Vec::new();
    if let Some((width, height)) = board_size(pcb) {
        lines.push(format!("Size: {} x {} mm", number(width), number(height)));
    }
    let count = pcb.copper_layers().count();
    lines.push(format!("Copper layers: {count}"));
    let stackup = pcb.stackup();
    lines.push(format!("Thickness: {} mm", number(stackup.thickness)));
    for (i, layer) in pcb.copper_layers().enumerate() {
        if let Some(copper) = stackup.copper_of(layer.uuid) {
            lines.push(format!(
                "Copper {}: {}",
                copper_name(i, count),
                copper_weight(copper.thickness)
            ));
        }
    }
    for (label, value) in [
        ("Finish", &stackup.finish),
        ("Solder mask colour", &stackup.mask_color),
        ("Silkscreen colour", &stackup.silk_color),
    ] {
        if let Some(value) = value {
            lines.push(format!("{label}: {value}"));
        }
    }
    section(out, "Board", lines);
}

/// A copper thickness: `1 oz (35 µm)` for a standard weight (a whole number of half
/// ounces), µm alone otherwise.
fn copper_weight(thickness: Nm) -> String {
    let um = micrometres(thickness);
    let half_oz = NM_PER_OZ / 2;
    if thickness.0 > 0 && thickness.0 % half_oz == 0 {
        let halves = thickness.0 / half_oz;
        let oz = if halves % 2 == 0 {
            format!("{}", halves / 2)
        } else {
            format!("{}.5", halves / 2)
        };
        format!("{oz} oz ({um} µm)")
    } else {
        format!("{um} µm")
    }
}

/// The minimum features of the board-wide rule records that count. A disabled record is
/// no limit, so it is left out.
fn minimum_features(out: &mut String, pcb: &PcbDocument) {
    let values = |kind: RuleKind| -> Option<&RuleValues> {
        let rule = pcb.board_rule(kind);
        rule.enabled.then_some(&rule.values)
    };
    let rows: [(&str, Option<Nm>); 8] = [
        (
            "Clearance",
            values(RuleKind::Clearance).and_then(|v| v.value),
        ),
        (
            "Track width",
            values(RuleKind::TrackWidth).and_then(|v| v.min),
        ),
        (
            "Via diameter",
            values(RuleKind::ViaSize).and_then(|v| v.diameter.and_then(|b| b.min)),
        ),
        (
            "Via hole",
            values(RuleKind::ViaSize).and_then(|v| v.hole.and_then(|b| b.min)),
        ),
        ("Hole size", values(RuleKind::HoleSize).and_then(|v| v.min)),
        (
            "Annular ring",
            values(RuleKind::AnnularRing).and_then(|v| v.min),
        ),
        (
            "Hole to hole",
            values(RuleKind::HoleToHole).and_then(|v| v.min),
        ),
        (
            "Copper to board edge",
            values(RuleKind::EdgeClearance).and_then(|v| v.value),
        ),
    ];
    let lines = rows
        .into_iter()
        .filter_map(|(label, value)| Some(format!("{label}: {} mm", number(value?))))
        .collect();
    section(out, "Minimum features", lines);
}

/// Every Footprint Pad and Free Pad hole with a `hole.tolerance`: Footprint Pads by
/// Designator and Pad number, Free Pads by where they sit in the files' coordinates.
fn tolerances(out: &mut String, plot: &Plot<'_>) {
    let mut lines = Vec::new();
    for instance in plot.instances {
        let pcb = instance.pcb;
        let mut footprints: Vec<_> = pcb.footprints().collect();
        footprints.sort_by(|a, b| natural(&a.designator, &b.designator));
        for footprint in footprints {
            let mut pads = placed::placed_pads(pcb, footprint);
            pads.sort_by(|a, b| natural(&a.number, &b.number));
            for pad in pads {
                if let Some(line) = tolerance(pad.hole, pad.kind) {
                    lines.push(format!(
                        "{} pad {}: {line}",
                        footprint.designator, pad.number
                    ));
                }
            }
        }
        for pad in pcb.free_pads() {
            if let Some(line) = tolerance(pad.hole, pad.kind) {
                let at: Point = instance.transform.point(pad.at);
                lines.push(format!(
                    "Free Pad at ({}, {}) mm: {line}",
                    number(at.x),
                    number(at.y)
                ));
            }
        }
    }
    section(out, "Hole tolerances", lines);
}

/// `1 mm hole, plated, +0.05/-0.05 mm`, or `2.2 x 0.8 mm slot, …`; `None` for a hole
/// with no tolerance.
fn tolerance(hole: Option<Hole>, kind: PadKind) -> Option<String> {
    let hole = hole?;
    let t = hole.tolerance?;
    let size = if hole.width == hole.height {
        format!("{} mm hole", number(hole.width))
    } else {
        format!("{} x {} mm slot", number(hole.width), number(hole.height))
    };
    let plating = match kind {
        PadKind::NonPlated => "non-plated",
        _ => "plated",
    };
    Some(format!(
        "{size}, {plating}, +{}/-{} mm",
        number(t.plus),
        number(t.minus)
    ))
}

/// Each Net Tie and the Nets its Pads join, which touch by design.
fn net_ties(out: &mut String, pcb: &PcbDocument) {
    let mut ties: Vec<_> = pcb
        .footprints()
        .filter(|f| f.component_type == ComponentType::NetTie)
        .collect();
    ties.sort_by(|a, b| natural(&a.designator, &b.designator));
    let lines = ties
        .into_iter()
        .map(|tie| {
            let mut nets: Vec<&str> = tie
                .pads
                .iter()
                .filter_map(|p| pcb.net(p.net?))
                .map(|n| n.name.as_str())
                .collect();
            nets.sort_by(|a, b| natural(a, b));
            nets.dedup();
            let nets = match nets.as_slice() {
                [] => "no Nets".to_owned(),
                [one] => (*one).to_owned(),
                [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
            };
            format!(
                "{}: joins {nets} on purpose; this short is deliberate",
                tie.designator
            )
        })
        .collect();
    section(out, "Deliberate shorts (Net Ties)", lines);
}

/// Text order with runs of digits compared as numbers, so `R2` comes before `R10`.
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    fn key(s: &str) -> Vec<(String, u64)> {
        let mut out = Vec::new();
        let mut text = String::new();
        let mut digits = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() {
                digits.push(c);
            } else {
                if !digits.is_empty() {
                    out.push((
                        std::mem::take(&mut text),
                        digits.parse().unwrap_or(u64::MAX),
                    ));
                    digits.clear();
                }
                text.push(c);
            }
        }
        out.push((text, digits.parse().unwrap_or(0)));
        out
    }
    key(a).cmp(&key(b)).then_with(|| a.cmp(b))
}

/// A length in millimetres, exact, without trailing zeros: `0.2`, `30`, `-1.25`.
pub(crate) fn number(nm: Nm) -> String {
    nm.mm_exact()
}

/// A length in micrometres, exact, without trailing zeros: `35`, `87.5`.
fn micrometres(nm: Nm) -> String {
    exact_decimal(nm.0, 1_000)
}

