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

use crate::plot::{FileAttributes, OutputFile, Plot, SkippedFile, file_attributes};

/// The drill files of a plot, and the ones left out because they would hold no holes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drills {
    pub files: Vec<OutputFile>,
    pub skipped: Vec<SkippedFile>,
    /// Through rows, including empty files, then the occupied partial spans in physical order.
    pub spans: Vec<DrillSpan>,
}

/// An Output Job Drill table row, in the writer's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrillSpan {
    pub name: String,
    pub span: String,
    pub function: &'static str,
    pub plotted: bool,
}

/// The possible through-drill file names of the Project `name`:
/// `<name>-PTH.drl`, `<name>-NPTH.drl` and the merged `<name>.drl`.
pub fn drill_file_names(name: &str) -> [String; 3] {
    [
        format!("{name}-PTH.drl"),
        format!("{name}-NPTH.drl"),
        format!("{name}.drl"),
    ]
}

/// Drill files in physical order. Through holes come first, split by plating or
/// combined with `merge_drill`. Each occupied blind or buried span follows in its
/// own file. Empty through files are listed in `skipped`.
pub fn drill_files(plot: &Plot<'_>) -> Drills {
    let mut drills = Drills {
        files: Vec::new(),
        skipped: Vec::new(),
        spans: Vec::new(),
    };
    for set in drill_sets(plot) {
        drills.spans.push(DrillSpan {
            name: set.name.clone(),
            span: set.span_description,
            function: set.function,
            plotted: !set.holes.is_empty(),
        });
        if set.holes.is_empty() {
            drills.skipped.push(SkippedFile {
                name: set.name,
                function: set.function,
            });
        } else {
            let file_function = set.file_function;
            drills.files.push(OutputFile {
                bytes: write(plot, &file_function, &set.holes),
                name: set.name,
                function: set.function,
                attributes: Some(FileAttributes {
                    function: file_function,
                    polarity: "Positive",
                }),
            });
        }
    }
    drills
}

/// The holes of one drill file, written or (with no holes) skipped.
pub(crate) struct DrillSet {
    pub name: String,
    /// Human-readable span and drilling method for the drill report.
    pub span_description: String,
    /// Its output kind, including `drill_blind` and `drill_buried` for partial spans.
    pub function: &'static str,
    /// The complete file function, shared by Excellon and its Gerber job entry.
    file_function: String,
    pub holes: Vec<DrillHole>,
}

pub(crate) fn drill_sets(plot: &Plot<'_>) -> Vec<DrillSet> {
    let Some(first) = plot.instances.first() else {
        return Vec::new();
    };
    let name = &plot.project.props().name;
    let layers = first.pcb.copper_layers().count();
    let mut through = Vec::new();
    let mut partial = BTreeMap::<(usize, usize), Vec<DrillHole>>::new();
    for instance in plot.instances {
        for mut hole in holes(instance.pcb) {
            hole.at = instance.transform.point(hole.at);
            hole.to = hole.to.map(|p| instance.transform.point(p));
            if let Some(span) = hole.span {
                partial.entry(span).or_default().push(hole);
            } else {
                through.push(hole);
            }
        }
    }

    let [pth, npth, merged] = drill_file_names(name);
    let through_span = format!("L1-L{layers}, through");
    let mut sets = if plot.gerber.merge_drill {
        vec![DrillSet {
            name: merged,
            span_description: through_span,
            function: "drill_mixed",
            file_function: format!("MixedPlating,1,{layers},{}", label(&through)),
            holes: through,
        }]
    } else {
        let (plated, non_plated): (Vec<_>, Vec<_>) =
            through.into_iter().partition(|hole| hole.plated);
        let mut non_plated_function = format!("NonPlated,1,{layers},NPTH");
        // Version 0.1's two-Layer non-plated files retain their drill/route qualifier.
        if layers == 2 {
            let _ = write!(non_plated_function, ",{}", label(&non_plated));
        }
        vec![
            DrillSet {
                name: pth,
                span_description: through_span.clone(),
                function: "drill_plated",
                file_function: format!("Plated,1,{layers},PTH"),
                holes: plated,
            },
            DrillSet {
                name: npth,
                span_description: through_span,
                function: "drill_non_plated",
                file_function: non_plated_function,
                holes: non_plated,
            },
        ]
    };
    for ((from, to), holes) in partial {
        let (function, kind) = if from == 1 || to == layers {
            ("drill_blind", "Blind")
        } else {
            ("drill_buried", "Buried")
        };
        let micro = if holes.iter().any(|hole| hole.micro) {
            ", micro"
        } else {
            ""
        };
        sets.push(DrillSet {
            name: format!("{name}-L{from}-L{to}.drl"),
            span_description: format!("L{from}-L{to}, {}{micro}", kind.to_ascii_lowercase()),
            function,
            file_function: format!("Plated,{from},{to},{kind}"),
            holes,
        });
    }
    sets
}

/// One hole on the board.
pub(crate) struct DrillHole {
    /// Physical first and last positions for a partial Via. All other holes go through.
    span: Option<(usize, usize)>,
    micro: bool,
    pub plated: bool,
    /// The drill or routing tool diameter, rounded to 1 µm.
    diameter: Nm,
    /// The tool's `.AperFunction`: `ViaDrill`, `ComponentDrill` or `MechanicalDrill`.
    function: &'static str,
    /// The hole's `hole.tolerance`, written as the tool's `.DrillTolerance`.
    tolerance: Option<HoleTolerance>,
    at: Point,
    /// The other end of a slot's centre line; `None` for a round hole.
    pub to: Option<Point>,
}

/// Every hole: Vias, then Footprint Pads, then Free Pads, each in UUID order.
fn holes(pcb: &reda_model::PcbDocument) -> Vec<DrillHole> {
    let layers = pcb.copper_layers().count();
    let mut holes: Vec<DrillHole> = pcb
        .vias()
        .map(|via| {
            let from = pcb
                .copper_number(via.span.from)
                .expect("validated Via starts on copper");
            let to = pcb
                .copper_number(via.span.to)
                .expect("validated Via ends on copper");
            let span = (from.min(to), from.max(to));
            DrillHole {
                span: (span != (1, layers)).then_some(span),
                micro: pcb
                    .stackup()
                    .type_for_span(via.span)
                    .is_some_and(|t| t.micro),
                plated: true,
                diameter: micron(via.hole),
                function: "ViaDrill",
                tolerance: None,
                at: via.at,
                to: None,
            }
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
        span: None,
        micro: false,
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
pub(crate) fn mm(nm: Nm) -> String {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Tool {
    /// The diameter, rounded to 1 µm.
    pub diameter: Nm,
    /// `false` for plated, so plated tools come first.
    non_plated: bool,
    /// The `.AperFunction`: `ViaDrill`, `ComponentDrill` or `MechanicalDrill`.
    pub function: &'static str,
    /// `plus` and `minus`, rounded to 1 µm.
    pub tolerance: Option<(Nm, Nm)>,
}

impl Tool {
    pub fn plated(&self) -> bool {
        !self.non_plated
    }
}

/// The tools of a drill file and their holes, T1 first.
pub(crate) fn tools(holes: &[DrillHole]) -> BTreeMap<Tool, Vec<&DrillHole>> {
    let mut tools: BTreeMap<Tool, Vec<&DrillHole>> = BTreeMap::new();
    for hole in holes {
        let tool = Tool {
            diameter: hole.diameter,
            non_plated: !hole.plated,
            function: hole.function,
            tolerance: hole.tolerance.map(|t| (micron(t.plus), micron(t.minus))),
        };
        tools.entry(tool).or_default().push(hole);
    }
    tools
}

/// One drill file holding `holes`, which is not empty. Its attributes are always written:
/// they are `; #@! ` comments already, so the Gerber `x2_attributes` switch leaves them in.
fn write(plot: &Plot<'_>, function: &str, holes: &[DrillHole]) -> Vec<u8> {
    let tools = tools(holes);

    let mut out = String::from("M48\n");
    let mut attributes = file_attributes(plot, function.to_owned());
    attributes.push((".FilePolarity", "Positive".into()));
    for (name, value) in attributes {
        let _ = writeln!(out, "; #@! TF{name},{}", ascii(&value));
    }
    out.push_str("FMAT,2\nMETRIC\n");
    for (n, tool) in tools.keys().enumerate() {
        let _ = writeln!(out, "; #@! TA.AperFunction,{}", tool.function);
        if let Some((plus, minus)) = tool.tolerance {
            let _ = writeln!(out, "; #@! TA.DrillTolerance,{},{}", mm(plus), mm(minus));
        }
        let _ = writeln!(out, "T{}C{}", n + 1, mm(tool.diameter));
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
