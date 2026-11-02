//! What blocks an Output Job run and what only warns (outputs.md, "What blocks a run and
//! what only warns"). Both come back as Violations, so the UI words them like any check.

use std::collections::BTreeMap;

use reda_command::Fonts;
use reda_model::{
    ComponentType, FabPreset, OutputJob, PcbDocument, Project, Severity, Uuid, Violation,
    apply_waivers,
};

/// The problems that stop a run, in a fixed order: duplicate Designators, an open Board
/// Outline, then Components without a Footprint (only when the job writes a BOM or CPL).
pub(crate) fn blockers(project: &Project, pcb: &PcbDocument, job: &OutputJob) -> Vec<Violation> {
    let mut found = duplicate_designators(project, pcb);
    let outline = pcb.outline();
    if outline.is_none_or(|o| reda_model::geom::check_contour(&o.vertices).is_err()) {
        found.push(
            Violation::new("board_outline_open", Severity::Error).objects(outline.map(|o| o.uuid)),
        );
    }
    if job.outputs.bom || job.outputs.cpl {
        found.extend(missing_footprints(project));
    }
    found
}

/// One `duplicate_designator` per Designator two Components share, over the Schematic
/// Sheets and the PCB's Mechanical Parts; objects are the Units and Footprints carrying it.
fn duplicate_designators(project: &Project, pcb: &PcbDocument) -> Vec<Violation> {
    // Designator → Component (or Mechanical Part) → the objects showing it.
    let mut by_name: BTreeMap<&str, BTreeMap<Uuid, Vec<Uuid>>> = BTreeMap::new();
    for document in project.documents() {
        if let Some(sheet) = project.schematic_sheet(document.uuid) {
            for unit in sheet.units() {
                by_name
                    .entry(&unit.designator)
                    .or_default()
                    .entry(unit.component.uuid)
                    .or_default()
                    .push(unit.uuid);
            }
        }
    }
    for footprint in pcb.footprints() {
        let owner = footprint
            .component
            .as_ref()
            .map_or(footprint.uuid, |c| c.uuid);
        by_name
            .entry(&footprint.designator)
            .or_default()
            .entry(owner)
            .or_default()
            .push(footprint.uuid);
    }
    by_name
        .into_iter()
        .filter(|(_, owners)| owners.len() > 1)
        .map(|(name, owners)| {
            Violation::new("duplicate_designator", Severity::Error)
                .arg("designator", name)
                .objects(owners.into_values().flatten())
        })
        .collect()
}

/// One `missing_footprint` per fitted Standard Component with no Footprint chosen (the BOM
/// and CPL leave Mechanical and Not Fitted Components out, so they need none).
fn missing_footprints(project: &Project) -> Vec<Violation> {
    let mut by_component: BTreeMap<Uuid, (&str, Vec<Uuid>)> = BTreeMap::new();
    for document in project.documents() {
        if let Some(sheet) = project.schematic_sheet(document.uuid) {
            for unit in sheet.units() {
                if unit.footprint.is_none()
                    && !unit.not_fitted
                    && unit.component_type == ComponentType::Standard
                {
                    by_component
                        .entry(unit.component.uuid)
                        .or_insert((&unit.designator, Vec::new()))
                        .1
                        .push(unit.uuid);
                }
            }
        }
    }
    let mut found: Vec<(&str, Vec<Uuid>)> = by_component.into_values().collect();
    found.sort_by(|a, b| reda_schematic::natural_cmp(a.0, b.0));
    found
        .into_iter()
        .map(|(designator, units)| {
            Violation::new("missing_footprint", Severity::Error)
                .arg("designator", designator)
                .objects(units)
        })
        .collect()
}

/// The warnings of a run, in a fixed order: out of sync, ERC errors, DRC errors, rules
/// looser than the preset's minimums, missing fonts.
pub(crate) fn warnings(
    project: &Project,
    pcb: &PcbDocument,
    preset: &FabPreset,
    fonts: &Fonts,
) -> Vec<Violation> {
    let mut found = Vec::new();
    let pending = crate::sync::pending_count(project);
    if pending > 0 {
        found.push(
            Violation::new("out_of_sync", Severity::Warning)
                .arg("changes", i64::try_from(pending).unwrap_or(i64::MAX)),
        );
    }
    let waivers = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.waivers());
    let erc = apply_waivers(reda_schematic::erc::check(project), waivers, false);
    let errors = erc
        .violations
        .iter()
        .filter(|v| v.severity == Severity::Error)
        .count();
    if errors > 0 {
        found.push(
            Violation::new("erc_errors", Severity::Warning)
                .arg("count", i64::try_from(errors).unwrap_or(i64::MAX)),
        );
    }
    let fills = reda_pcb::zone::current(pcb, project.zone_fills(pcb.uuid));
    let drc = apply_waivers(
        reda_pcb::drc::check(pcb, &fills, fonts),
        pcb.waivers(),
        false,
    );
    let errors = drc
        .violations
        .iter()
        .filter(|v| v.severity == Severity::Error)
        .count();
    if errors > 0 {
        found.push(
            Violation::new("drc_errors", Severity::Warning)
                .arg("count", i64::try_from(errors).unwrap_or(i64::MAX)),
        );
    }
    found.extend(loose_rules(pcb, preset));
    for font in reda_pcb::missing_fonts(pcb, fonts) {
        found.push(Violation::new("missing_font", Severity::Warning).arg("font", font));
    }
    found
}

/// A `rule_below_preset` for each board rule, and each Net Class override, that allows
/// less than the preset: `rule`, `board` (the value), `minimum` (the preset's), `preset`
/// (its name) and, for an override, `net_class`.
fn loose_rules(pcb: &PcbDocument, preset: &FabPreset) -> Vec<Violation> {
    let (board, min) = (pcb.setup().rules, preset.rules);
    let warn = |rule: &str, value: reda_model::geom::Nm, minimum: reda_model::geom::Nm| {
        (value < minimum).then(|| {
            Violation::new("rule_below_preset", Severity::Warning)
                .arg("rule", rule)
                .arg("board", value)
                .arg("minimum", minimum)
                .arg("preset", preset.name.as_str())
        })
    };
    let mut found: Vec<Violation> = [
        ("clearance", board.clearance, min.clearance),
        ("track_width", board.track_width, min.track_width),
        ("via_diameter", board.via_diameter, min.via_diameter),
        ("via_hole", board.via_hole, min.via_hole),
        ("annular_ring", board.annular_ring, min.annular_ring),
        ("hole_to_hole", board.hole_to_hole, min.hole_to_hole),
        ("edge_clearance", board.edge_clearance, min.edge_clearance),
    ]
    .into_iter()
    .filter_map(|(rule, value, minimum)| warn(rule, value, minimum))
    .collect();
    for class in pcb.net_classes() {
        let overrides = [
            ("clearance", class.clearance, min.clearance),
            ("track_width", class.track_width, min.track_width),
            ("via_diameter", class.via_diameter, min.via_diameter),
            ("via_hole", class.via_hole, min.via_hole),
        ];
        for (rule, value, minimum) in overrides {
            if let Some(v) = value.and_then(|v| warn(rule, v, minimum)) {
                found.push(
                    v.arg("net_class", class.name.as_str())
                        .objects([class.uuid]),
                );
            }
        }
    }
    found
}
