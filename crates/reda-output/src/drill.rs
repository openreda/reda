//! REDA's own Excellon writer: XNC-style, metric, absolute coordinates with a decimal
//! point, plated and non-plated holes in separate files.
//!
//! Every hole comes from a Via, a Footprint Pad or a Free Pad. A round hole is a drill
//! hit; a slot (a hole whose width and height differ) is routed from one end of its centre
//! line to the other with a tool as wide as the slot. With X2 attributes on, the file and
//! tool attributes are written as `; #@! TF.…` and `; #@! TA.…` comments, as XNC does.

use std::collections::BTreeMap;
use std::fmt::Write;

use reda_model::geom::{Nm, Point};
use reda_model::{Hole, PadKind};
use reda_pcb::placed::{self, PlacedPad, rotate};

use crate::plot::{OutputFile, Plot, file_attributes};

/// The drill files of `plot`: the plated holes, then the non-plated ones. Both files are
/// always written, even when one has no holes, so a run's file list does not depend on
/// the board.
pub fn drill_files(plot: &Plot<'_>) -> Vec<OutputFile> {
    let name = &plot.project.props().name;
    let layers = plot.pcb.copper_layers().count();
    let holes = holes(plot);
    let (plated, non_plated): (Vec<DrillHole>, Vec<DrillHole>) =
        holes.into_iter().partition(|h| h.plated);
    vec![
        OutputFile {
            name: format!("{name}-PTH.drl"),
            function: "drill_plated",
            bytes: write(plot, format!("Plated,1,{layers},PTH"), &plated),
        },
        OutputFile {
            name: format!("{name}-NPTH.drl"),
            function: "drill_non_plated",
            bytes: write(plot, format!("NonPlated,1,{layers},NPTH"), &non_plated),
        },
    ]
}

/// One hole on the board.
struct DrillHole {
    plated: bool,
    /// The drill or routing tool diameter.
    diameter: Nm,
    /// The tool's `.AperFunction`, such as `Plated,PTH,ViaDrill`.
    function: &'static str,
    at: Point,
    /// The other end of a slot's centre line; `None` for a round hole.
    to: Option<Point>,
}

/// Every hole: Vias, then Footprint Pads, then Free Pads, each in UUID order.
fn holes(plot: &Plot<'_>) -> Vec<DrillHole> {
    let pcb = plot.pcb;
    let mut holes: Vec<DrillHole> = pcb
        .vias()
        .map(|via| DrillHole {
            plated: true,
            diameter: via.hole,
            function: "Plated,PTH,ViaDrill",
            at: via.at,
            to: None,
        })
        .collect();
    for instance in pcb.footprints() {
        holes.extend(
            placed::placed_pads(pcb, instance)
                .iter()
                .filter_map(|pad| pad_hole(pad, "Plated,PTH,ComponentDrill")),
        );
    }
    holes.extend(
        pcb.free_pads().filter_map(|pad| {
            pad_hole(&placed::placed_free_pad(pad), "Plated,PTH,MechanicalDrill")
        }),
    );
    holes
}

/// The hole of a placed Pad, if it has one. A plated hole takes `plated_function`; a
/// non-plated hole is a mechanical one.
fn pad_hole(pad: &PlacedPad, plated_function: &'static str) -> Option<DrillHole> {
    let Hole { width, height } = pad.hole?;
    let plated = match pad.kind {
        PadKind::Smd => return None,
        PadKind::PlatedThroughHole => true,
        PadKind::NonPlated => false,
    };
    let function = match plated {
        true => plated_function,
        false => "NonPlated,NPTH,MechanicalDrill",
    };
    let diameter = width.min(height);
    // A slot runs along its longer side, in the Pad's frame, turned with the Pad.
    let half = Nm((width.0 - height.0).abs() / 2);
    let (at, to) = if half == Nm::ZERO {
        (pad.at, None)
    } else {
        let along = match width > height {
            true => Point::new(half, Nm::ZERO),
            false => Point::new(Nm::ZERO, half),
        };
        let d = rotate(along, Point::new(Nm::ZERO, Nm::ZERO), pad.rotation);
        (
            Point::new(pad.at.x - d.x, pad.at.y - d.y),
            Some(Point::new(pad.at.x + d.x, pad.at.y + d.y)),
        )
    };
    Some(DrillHole {
        plated,
        diameter,
        function,
        at,
        to,
    })
}

/// A length as millimetres with a decimal point, exact to the nanometre, with trailing
/// zeros trimmed but one decimal kept: `5.0`, `8.73`, `-0.000001`.
fn mm(nm: Nm) -> String {
    let sign = if nm.0 < 0 { "-" } else { "" };
    let a = nm.0.unsigned_abs();
    let frac = format!("{:06}", a % 1_000_000);
    let frac = frac.trim_end_matches('0');
    let frac = if frac.is_empty() { "0" } else { frac };
    format!("{sign}{}.{frac}", a / 1_000_000)
}

/// A tool diameter in millimetres with at least three decimals: `0.300`, `0.8125`.
fn tool_mm(nm: Nm) -> String {
    let text = mm(nm);
    let decimals = text.len() - text.find('.').expect("a point") - 1;
    format!("{text}{}", "0".repeat(3usize.saturating_sub(decimals)))
}

fn xy(p: Point) -> String {
    format!("X{}Y{}", mm(p.x), mm(p.y))
}

/// One drill file holding `holes`.
fn write(plot: &Plot<'_>, function: String, holes: &[DrillHole]) -> Vec<u8> {
    let x2 = plot.gerber.x2_attributes;
    // Tools by size, then function: T1 is the smallest.
    let mut tools: BTreeMap<(Nm, &'static str), Vec<&DrillHole>> = BTreeMap::new();
    for hole in holes {
        tools
            .entry((hole.diameter, hole.function))
            .or_default()
            .push(hole);
    }

    let mut out = String::from("M48\n");
    if x2 {
        for (name, value) in file_attributes(plot, function) {
            if name != ".FilePolarity" {
                let _ = writeln!(out, "; #@! TF{name},{value}");
            }
        }
    }
    out.push_str("FMAT,2\nMETRIC\n");
    for (n, (diameter, function)) in tools.keys().enumerate() {
        if x2 {
            let _ = writeln!(out, "; #@! TA.AperFunction,{function}");
        }
        let _ = writeln!(out, "T{}C{}", n + 1, tool_mm(*diameter));
    }
    out.push_str("%\nG90\nG05\n");
    for (n, holes) in tools.values().enumerate() {
        let _ = writeln!(out, "T{}", n + 1);
        for hole in holes.iter().filter(|h| h.to.is_none()) {
            let _ = writeln!(out, "{}", xy(hole.at));
        }
        for hole in holes {
            if let Some(to) = hole.to {
                let _ = writeln!(out, "G00{}\nM15\nG01{}\nM16\nG05", xy(hole.at), xy(to));
            }
        }
    }
    out.push_str("M30\n");
    out.into_bytes()
}
