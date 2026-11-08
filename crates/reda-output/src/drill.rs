//! REDA's own Excellon writer: metric, absolute coordinates with a decimal point and
//! three decimals (1 µm), in the command form `FMAT,2`, `METRIC`, `T1C0.300`, `G90`, `G05`.
//!
//! Every hole comes from a Via, a Footprint Pad or a Free Pad. A round hole is a drill
//! hit; a slot (a hole whose width and height differ) is routed from one end of its centre
//! line to the other with a tool as wide as the slot (`G00`, `M15`, `G01`, `M16`, `G05`),
//! or written as a `G85` canned slot when the Fab Preset's `slot_command` says so. With X2
//! attributes on, the file and tool attributes are written as `; #@! TF.…` and
//! `; #@! TA.…` comments in printable ASCII, as XNC does.

use std::collections::BTreeMap;
use std::fmt::Write;

use reda_model::geom::{Nm, Point};
use reda_model::{HoleTolerance, PadKind, SlotCommand};
use reda_pcb::placed::{self, PlacedPad};

use crate::plot::{OutputFile, Plot, SkippedFile, file_attributes};

/// The drill files of a plot, and the ones left out because they would hold no holes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drills {
    pub files: Vec<OutputFile>,
    pub skipped: Vec<SkippedFile>,
}

/// Every name a drill file of the Project `name` can have, whichever the switches:
/// `<name>-PTH.drl`, `<name>-NPTH.drl` and the merged `<name>.drl`.
pub fn drill_file_names(name: &str) -> [String; 3] {
    [
        format!("{name}-PTH.drl"),
        format!("{name}-NPTH.drl"),
        format!("{name}.drl"),
    ]
}

/// The drill files of `plot`: the plated holes, then the non-plated ones, or with the Fab
/// Preset's `merge_drill` one file holding both. A file with no holes is left out and
/// listed in `skipped`.
pub fn drill_files(plot: &Plot<'_>) -> Drills {
    let mut drills = Drills {
        files: Vec::new(),
        skipped: Vec::new(),
    };
    if plot.instances.is_empty() {
        return drills;
    }
    let name = &plot.project.props().name;
    let layers = plot
        .instances
        .first()
        .map_or(0, |i| i.pcb.copper_layers().count());
    let holes: Vec<_> = plot
        .instances
        .iter()
        .flat_map(|instance| {
            let mut holes = holes(instance.pcb);
            for hole in &mut holes {
                hole.at = instance.transform.point(hole.at);
                hole.to = hole.to.map(|p| instance.transform.point(p));
            }
            holes
        })
        .collect();
    let [pth, npth, merged] = drill_file_names(name);
    let files = if plot.gerber.merge_drill {
        vec![(
            merged,
            "drill_mixed",
            format!("MixedPlating,1,{layers}"),
            holes,
        )]
    } else {
        let (plated, non_plated): (Vec<DrillHole>, Vec<DrillHole>) =
            holes.into_iter().partition(|h| h.plated);
        vec![
            (
                pth,
                "drill_plated",
                format!("Plated,1,{layers},PTH"),
                plated,
            ),
            (
                npth,
                "drill_non_plated",
                format!("NonPlated,1,{layers},NPTH"),
                non_plated,
            ),
        ]
    };
    for (name, function, file_function, holes) in files {
        if holes.is_empty() {
            drills.skipped.push(SkippedFile { name, function });
        } else {
            drills.files.push(OutputFile {
                name,
                function,
                bytes: write(plot, file_function, &holes),
            });
        }
    }
    drills
}

/// One hole on the board.
struct DrillHole {
    plated: bool,
    /// The drill or routing tool diameter, rounded to 1 µm.
    diameter: Nm,
    /// The tool's `.AperFunction`: `ViaDrill`, `ComponentDrill` or `MechanicalDrill`.
    function: &'static str,
    /// The hole's `hole.tolerance`, written as the tool's `.DrillTolerance`.
    tolerance: Option<HoleTolerance>,
    at: Point,
    /// The other end of a slot's centre line; `None` for a round hole.
    to: Option<Point>,
}

/// Every hole: Vias, then Footprint Pads, then Free Pads, each in UUID order.
fn holes(pcb: &reda_model::PcbDocument) -> Vec<DrillHole> {
    let mut holes: Vec<DrillHole> = pcb
        .vias()
        .map(|via| DrillHole {
            plated: true,
            diameter: micron(via.hole),
            function: "ViaDrill",
            tolerance: None,
            at: via.at,
            to: None,
        })
        .collect();
    for instance in pcb.footprints() {
        holes.extend(
            placed::placed_pads(pcb, instance)
                .iter()
                .filter_map(|pad| pad_hole(pad, "ComponentDrill")),
        );
    }
    holes.extend(
        pcb.free_pads()
            .filter_map(|pad| pad_hole(&placed::placed_free_pad(pad), "MechanicalDrill")),
    );
    holes
}

/// The hole of a placed Pad, if it has one. A plated hole takes `plated_function`; a
/// non-plated hole is a mechanical one.
fn pad_hole(pad: &PlacedPad, plated_function: &'static str) -> Option<DrillHole> {
    let hole = pad.hole?;
    let plated = match pad.kind {
        PadKind::Smd => return None,
        PadKind::PlatedThroughHole => true,
        PadKind::NonPlated => false,
    };
    let function = match plated {
        true => plated_function,
        false => "MechanicalDrill",
    };
    // A slot runs along its longer side, turned by its own rotation and the Pad's.
    let (from, to) = pad.hole_ends()?;
    let (at, to) = if from == to {
        (pad.at, None)
    } else {
        (from, Some(to))
    };
    Some(DrillHole {
        plated,
        diameter: micron(hole.diameter()),
        function,
        tolerance: hole.tolerance,
        at,
        to,
    })
}

/// A length rounded to the nearest micrometre, halves away from zero.
fn micron(nm: Nm) -> Nm {
    let n = nm.0;
    let rounded = (n.abs() + 500) / 1000 * 1000;
    Nm(if n < 0 { -rounded } else { rounded })
}

/// A length as millimetres with three decimals, rounded to 1 µm: `5.000`, `-0.001`.
fn mm(nm: Nm) -> String {
    let um = micron(nm).0 / 1000;
    let sign = if um < 0 { "-" } else { "" };
    let a = um.unsigned_abs();
    format!("{sign}{}.{:03}", a / 1000, a % 1000)
}

fn xy(p: Point) -> String {
    format!("X{}Y{}", mm(p.x), mm(p.y))
}

/// An attribute value in printable ASCII: `;` (which XNC comments may not hold) and every
/// character outside printable ASCII are escaped as `\uXXXX`, or `\UXXXXXXXX` beyond
/// 16 bits, the Gerber form the values already use for `%*,\`.
fn ascii(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            ' '..=':' | '<'..='~' => out.push(c),
            c if (c as u32) <= 0xFFFF => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => {
                let _ = write!(out, "\\U{:08X}", c as u32);
            }
        }
    }
    out
}

/// The `<label>` of a file's `.FileFunction`: `Drill` for round holes only, `Rout` for
/// slots only, `Mixed` for both.
fn label(holes: &[DrillHole]) -> &'static str {
    let slots = holes.iter().filter(|h| h.to.is_some()).count();
    match slots {
        0 => "Drill",
        n if n == holes.len() => "Rout",
        _ => "Mixed",
    }
}

/// A tool: its size, then plating (so a plated and a non-plated hole never share one),
/// function and tolerance. T1 is the smallest.
type Tool = (Nm, bool, &'static str, Option<(Nm, Nm)>);

/// One drill file holding `holes`, which is not empty.
fn write(plot: &Plot<'_>, function: String, holes: &[DrillHole]) -> Vec<u8> {
    let x2 = plot.gerber.x2_attributes;
    let mut tools: BTreeMap<Tool, Vec<&DrillHole>> = BTreeMap::new();
    for hole in holes {
        let tolerance = hole.tolerance.map(|t| (micron(t.plus), micron(t.minus)));
        tools
            .entry((hole.diameter, !hole.plated, hole.function, tolerance))
            .or_default()
            .push(hole);
    }

    let mut out = String::from("M48\n");
    if x2 {
        let function = format!("{function},{}", label(holes));
        let mut attributes = file_attributes(plot, function);
        attributes.push((".FilePolarity", "Positive".into()));
        for (name, value) in attributes {
            let _ = writeln!(out, "; #@! TF{name},{}", ascii(&value));
        }
    }
    out.push_str("FMAT,2\nMETRIC\n");
    for (n, (diameter, _, function, tolerance)) in tools.keys().enumerate() {
        if x2 {
            let _ = writeln!(out, "; #@! TA.AperFunction,{function}");
            if let Some((plus, minus)) = tolerance {
                let _ = writeln!(out, "; #@! TA.DrillTolerance,{},{}", mm(*plus), mm(*minus));
            }
        }
        let _ = writeln!(out, "T{}C{}", n + 1, mm(*diameter));
    }
    out.push_str("%\nG90\nG05\n");
    for (n, holes) in tools.values().enumerate() {
        let _ = writeln!(out, "T{}", n + 1);
        for hole in holes.iter().filter(|h| h.to.is_none()) {
            let _ = writeln!(out, "{}", xy(hole.at));
        }
        for hole in holes {
            if let Some(to) = hole.to {
                let _ = match plot.gerber.slot_command {
                    SlotCommand::Route => {
                        writeln!(out, "G00{}\nM15\nG01{}\nM16\nG05", xy(hole.at), xy(to))
                    }
                    SlotCommand::G85 => writeln!(out, "{}G85{}", xy(hole.at), xy(to)),
                };
            }
        }
    }
    out.push_str("M30\n");
    out.into_bytes()
}
