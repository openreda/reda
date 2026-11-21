//! The UTF-8 Fab Notes accompanying an Output Job describe the board, Stackup,
//! drilling, minimum features, hole tolerances and deliberate shorts. They list
//! the copper files and zip contents, followed by the board's own notes.

use std::collections::BTreeMap;
use std::fmt::Write;

use reda_model::geom::{NM_PER_OZ, Nm, Point, contour_bounds, exact_decimal};
use reda_model::{
    ComponentType, CopperRole, DielectricKind, Hole, PadKind, PcbDocument, Project, RuleKind,
    RuleValues, Side, StackupEntry, ViaDiameter, ViaType,
};
use reda_pcb::placed;

use crate::drill::{partial_file_name, span_positions};
use crate::plot::{OutputFile, Plot};

/// The Fab Notes' file name for the Project `name`.
pub fn fab_notes_name(name: &str) -> String {
    format!("{name}-fab-notes.txt")
}

/// Describe the board and list the files in archive order: manufacturing files,
/// these notes, then the job's remaining reports and attachments.
pub fn fab_notes_file(
    plot: &Plot<'_>,
    manufacturing: &[OutputFile],
    after: &[OutputFile],
) -> OutputFile {
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
        board(&mut out, plot.project, pcb, manufacturing);
        stackup_table(&mut out, pcb);
        via_table(&mut out, plot, pcb, manufacturing);
        minimum_features(&mut out, pcb);
        tolerances(&mut out, plot);
        net_ties(&mut out, pcb);
    }
    let copper = manufacturing
        .iter()
        .filter(|f| matches!(f.function, "copper_top" | "copper_inner" | "copper_bottom"))
        .map(|f| f.name.clone())
        .collect();
    section(&mut out, "Copper files in order", copper);
    let name = fab_notes_name(&props.name);
    let zip = manufacturing
        .iter()
        .map(|f| f.name.clone())
        .chain([name.clone()])
        .chain(after.iter().map(|f| f.name.clone()))
        .collect();
    section(&mut out, "Files in the zip", zip);
    if let Some(notes) = plot
        .instances
        .first()
        .and_then(|i| i.pcb.setup().fab_notes.as_deref())
    {
        let _ = write!(out, "\nNotes\n{}", notes.trim_end());
        out.push('\n');
    }
    OutputFile {
        name,
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

fn board(out: &mut String, project: &Project, pcb: &PcbDocument, files: &[OutputFile]) {
    let mut lines = Vec::new();
    if let Some((width, height)) = board_size(pcb) {
        lines.push(format!("Size: {} x {} mm", number(width), number(height)));
    }
    let count = pcb.copper_layers().count();
    lines.push(format!("Copper layers: {count}"));
    let stackup = pcb.stackup();
    lines.push(format!("Thickness: {} mm", number(stackup.thickness)));
    if let Some(name) = stackup
        .construction
        .as_ref()
        .filter(|_| project.construction_matches(stackup))
    {
        lines.push(format!("Construction: {name}"));
    }
    let info = stackup.info();
    let symmetry = if info.symmetric {
        "Symmetric"
    } else {
        "Not symmetric"
    };
    let thickness = if info.thickness_warning {
        "Outside"
    } else {
        "Within"
    };
    lines.push(format!("Symmetry: {symmetry}"));
    lines.push(format!("Thickness check: {thickness} 10% tolerance"));
    for (label, value) in [
        ("Finish", &stackup.finish),
        ("Solder mask colour", &stackup.mask_color),
        ("Silkscreen colour", &stackup.silk_color),
    ] {
        if let Some(value) = value {
            lines.push(format!("{label}: {value}"));
        }
    }
    lines.extend(edge_plating(pcb, files));
    section(out, "Board", lines);
}

/// The edge plating line, when any edge is a Plated Edge: where the plated edges are
/// drawn, or "whole outline" when every edge of the Board Outline is plated, and the Nets
/// the plating carries.
fn edge_plating(pcb: &PcbDocument, files: &[OutputFile]) -> Option<String> {
    let edges = pcb.plated_edges();
    if edges.is_empty() {
        return None;
    }
    let file = files
        .iter()
        .find(|f| f.function == "plated_edges")
        .map(|f| f.name.as_str());
    // Plated cutout edges need the drawing, so the line names the file then.
    let whole_outline = pcb.outline().is_some_and(|o| {
        !o.vertices.is_empty()
            && o.plated.len() == o.vertices.len()
            && edges.iter().all(|e| e.contour == o.uuid)
    });
    let mut nets: Vec<&str> = edges
        .iter()
        .filter_map(|e| pcb.net(e.net?))
        .map(|n| n.name.as_str())
        .collect();
    nets.sort_by(|a, b| natural(a, b));
    nets.dedup();
    let unnetted = edges.iter().any(|e| e.net.is_none());
    let carry = match nets.as_slice() {
        [] => "plated edges carry no net".to_owned(),
        [one] => format!("plated edges carry net {one}"),
        [rest @ .., last] => format!("plated edges carry nets {} and {last}", rest.join(", ")),
    };
    let carry = if unnetted && !nets.is_empty() {
        format!("{carry}; some carry no net")
    } else {
        carry
    };
    let place = match (whole_outline, file) {
        (true, Some(file)) => format!("the whole outline, drawn in {file}"),
        (true, None) => "the whole outline".to_owned(),
        (false, Some(file)) => format!("the edges drawn in {file}"),
        (false, None) => "the marked board edges".to_owned(),
    };
    Some(format!("Edge plating on {place}; {carry}"))
}

/// Align the text columns and keep blank cells visible between the separators.
fn table<const N: usize>(
    out: &mut String,
    heading: &str,
    header: [&str; N],
    rows: Vec<[String; N]>,
) {
    let rows: Vec<_> = std::iter::once(header.map(str::to_owned))
        .chain(rows)
        .collect();
    let widths: [usize; N] = std::array::from_fn(|i| {
        rows.iter()
            .map(|row| row[i].chars().count())
            .max()
            .unwrap_or(0)
    });
    let _ = writeln!(out, "\n{heading}");
    for row in rows {
        out.push_str("  |");
        for (cell, width) in row.iter().zip(widths) {
            let _ = write!(out, " {cell:<width$} |");
        }
        out.push('\n');
    }
}

fn stackup_table(out: &mut String, pcb: &PcbDocument) {
    let rows = pcb
        .stackup()
        .entries
        .iter()
        .map(|entry| match entry {
            StackupEntry::Copper(copper) => {
                let layer = pcb
                    .layers()
                    .layer(copper.layer)
                    .expect("Stackup copper Layer");
                let name = match layer.side {
                    Some(Side::Top) => String::from("L1 (top)"),
                    Some(Side::Bottom) => format!("L{} (bottom)", pcb.copper_layers().count()),
                    None => pcb
                        .inner_layer_label(layer.uuid)
                        .expect("inner copper Layer"),
                };
                let role = match layer.role {
                    CopperRole::Signal => "Signal",
                    CopperRole::Plane => "Plane",
                    CopperRole::Mixed => "Mixed",
                };
                [
                    name,
                    "Copper".into(),
                    role.into(),
                    copper_weight(copper.thickness),
                    String::new(),
                    String::new(),
                ]
            }
            StackupEntry::Dielectric(sheet) => {
                let kind = match sheet.kind {
                    DielectricKind::Core => "Core",
                    DielectricKind::Prepreg => "Prepreg",
                };
                [
                    sheet.name.clone().unwrap_or_default(),
                    kind.into(),
                    String::new(),
                    sheet
                        .thickness
                        .map(|t| format!("{} mm", number(t)))
                        .unwrap_or_default(),
                    sheet.material.clone().unwrap_or_default(),
                    sheet
                        .dk
                        .map(|dk| exact_decimal(i64::from(dk), 1000))
                        .unwrap_or_default(),
                ]
            }
        })
        .collect();
    table(
        out,
        "Stackup",
        ["Layer", "Kind", "Role", "Thickness", "Material", "Dk"],
        rows,
    );
}

/// Type defaults follow drilling direction. A Via may override the sizes while
/// still counting towards the type whose span joins its two Layers.
fn via_table(out: &mut String, plot: &Plot<'_>, pcb: &PcbDocument, files: &[OutputFile]) {
    let mut counts = BTreeMap::<(usize, usize), usize>::new();
    for instance in plot.instances {
        for via in instance.pcb.vias() {
            let (from, to) = span_positions(instance.pcb, via.span);
            *counts.entry((from.min(to), from.max(to))).or_default() += 1;
        }
    }
    let layers = pcb.copper_layers().count();
    let through = files
        .iter()
        .find(|f| matches!(f.function, "drill_plated" | "drill_mixed"));
    let row = |kind: Option<&ViaType>, from: usize, to: usize, count: usize| {
        let file = if count == 0 {
            None
        } else if from.min(to) == 1 && from.max(to) == layers {
            through
        } else {
            let name = partial_file_name(&plot.project.props().name, from, to);
            files
                .iter()
                .find(|f| matches!(f.function, "drill_blind" | "drill_buried") && f.name == name)
        };
        [
            kind.map_or_else(|| "Unmatched".into(), |v| v.display_name(pcb)),
            format!("L{from}-L{to}"),
            if kind.is_some_and(|v| v.micro) {
                "Yes"
            } else {
                "No"
            }
            .into(),
            kind.map(|v| number(v.hole)).unwrap_or_default(),
            kind.map(|v| pad_diameters(v.diameter)).unwrap_or_default(),
            count.to_string(),
            file.map(|f| f.name.clone()).unwrap_or_default(),
        ]
    };
    let mut rows = Vec::new();
    for kind in &pcb.stackup().via_types {
        let (from, to) = span_positions(pcb, kind.span());
        let count = counts
            .remove(&(from.min(to), from.max(to)))
            .unwrap_or_default();
        rows.push(row(Some(kind), from, to, count));
    }
    for ((from, to), count) in counts {
        rows.push(row(None, from, to, count));
    }
    table(
        out,
        "Via types",
        [
            "Name",
            "Span",
            "Micro",
            "Drill (mm)",
            "Pad (mm)",
            "Vias",
            "Drill file",
        ],
        rows,
    );
}

fn pad_diameters(diameter: ViaDiameter) -> String {
    let mut pad = number(diameter.start);
    let overrides: Vec<_> = [("middle", diameter.middle), ("end", diameter.end)]
        .into_iter()
        .filter_map(|(label, size)| Some(format!("{label} {}", number(size?))))
        .collect();
    if !overrides.is_empty() {
        let _ = write!(pad, " ({})", overrides.join(", "));
    }
    pad
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
