//! The drill report: `<project>-drill-report.txt`, plain text listing each drill file's
//! tools (the same T numbers the file uses) with size, plating, kind, hole count, slot
//! count and tolerance, and the totals. Tool travel is left out: REDA doesn't optimise
//! the drill path, and the fab sets its own.

use std::fmt::Write;

use crate::drill::{drill_sets, mm, tools};
use crate::plot::{OutputFile, Plot};

/// The drill report's file name for the Project `name`.
pub fn drill_report_name(name: &str) -> String {
    format!("{name}-drill-report.txt")
}

/// The drill report of `plot`'s drill files, as `drill_files` writes them.
pub fn drill_report_file(plot: &Plot<'_>) -> OutputFile {
    let name = &plot.project.props().name;
    let mut out = String::new();
    let _ = writeln!(out, "Drill report for {name}");
    out.push_str("Sizes in mm. Tool travel is left out: the fab plans its own drill path.\n");
    let (mut all_holes, mut all_slots) = (0, 0);
    for set in drill_sets(plot) {
        let _ = writeln!(out, "\n{}", set.name);
        if set.holes.is_empty() {
            out.push_str("  No holes: the file is not written.\n");
            continue;
        }
        let _ = writeln!(
            out,
            "  {:<5} {:>7}  {:<10} {:<10} {:>5} {:>5}  Tolerance",
            "Tool", "Size", "Plating", "Kind", "Holes", "Slots"
        );
        let (mut holes, mut slots) = (0, 0);
        for (n, (tool, members)) in tools(&set.holes).iter().enumerate() {
            let slot_count = members.iter().filter(|h| h.to.is_some()).count();
            let hole_count = members.len() - slot_count;
            holes += hole_count;
            slots += slot_count;
            let tolerance = tool
                .tolerance
                .map_or_else(|| "-".to_owned(), |(p, m)| format!("+{}/-{}", mm(p), mm(m)));
            let kind = match tool.function {
                "ViaDrill" => "via",
                "ComponentDrill" => "component",
                _ => "mechanical",
            };
            let plating = if tool.plated() {
                "plated"
            } else {
                "non-plated"
            };
            let _ = writeln!(
                out,
                "  {:<5} {:>7}  {:<10} {:<10} {:>5} {:>5}  {tolerance}",
                format!("T{}", n + 1),
                mm(tool.diameter),
                plating,
                kind,
                hole_count,
                slot_count
            );
        }
        let _ = writeln!(
            out,
            "  {:<5} {:>7}  {:<10} {:<10} {:>5} {:>5}",
            "Total", "", "", "", holes, slots
        );
        all_holes += holes;
        all_slots += slots;
    }
    let _ = writeln!(out, "\nAll files: {all_holes} holes, {all_slots} slots");
    OutputFile {
        name: drill_report_name(name),
        function: "drill_report",
        bytes: out.into_bytes(),
        attributes: None,
    }
}
