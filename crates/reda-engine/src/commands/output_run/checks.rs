//! What blocks an Output Job run and what only warns (outputs.md, "What blocks a run and
//! what only warns"). Both come back as Violations, so the UI words them like any check.

use std::collections::BTreeMap;

use reda_command::Fonts;
use reda_model::{FabPreset, PcbDocument, Project, Severity, Uuid, Violation, apply_waivers};

/// The problems that stop a run, in a fixed order: duplicate Designators, an open Board
/// Outline, then Footprints the PCB cannot resolve. All read only the job's PCB, which is
/// all the outputs read.
pub(crate) fn blockers(pcb: &PcbDocument) -> Vec<Violation> {
    let mut found = duplicate_designators(pcb);
    let outline = pcb.outline();
    if outline.is_none_or(|o| reda_model::geom::check_contour(&o.vertices).is_err()) {
        found.push(
            Violation::new("board_outline_open", Severity::Error).objects(outline.map(|o| o.uuid)),
        );
    }
    found.extend(missing_footprints(pcb));
    found
}

/// The `coordinate_out_of_range` blocker: a Gerber file would hold a coordinate beyond
/// ±2147.483647 mm, which the format can't carry. Args `file`, and `x`, `y` (nm, from the
/// output origin).
pub(crate) fn out_of_range(out: &reda_output::CoordinateOutOfRange) -> Violation {
    Violation::new("coordinate_out_of_range", Severity::Error)
        .arg("file", out.file.as_str())
        .arg("x", out.at.x.0)
        .arg("y", out.at.y.0)
}

/// One `duplicate_designator` per Designator two Components (or Mechanical Parts) share
/// on the job's PCB, which is all the BOM and CPL read; objects are the Footprints
/// carrying it. A sheet that is out of sync with the board only warns (`out_of_sync`).
fn duplicate_designators(pcb: &PcbDocument) -> Vec<Violation> {
    // Designator → Component (or Mechanical Part) → the Footprints showing it.
    let mut by_name: BTreeMap<&str, BTreeMap<Uuid, Vec<Uuid>>> = BTreeMap::new();
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

/// One `missing_footprint` per Footprint on the PCB whose Footprint copy the PCB does not
/// hold, so no output can draw or list it; objects are the Footprint. A committed Project
/// never holds one (the PCB keeps its own copy of every Footprint it places), so this
/// guards the writers, which would otherwise leave such a Footprint out silently.
fn missing_footprints(pcb: &PcbDocument) -> Vec<Violation> {
    pcb.footprints()
        .filter(|f| pcb.footprint_copy(f.footprint).is_none())
        .map(|f| {
            Violation::new("missing_footprint", Severity::Error)
                .arg("designator", f.designator.as_str())
                .objects([f.uuid])
        })
        .collect()
}

/// One `unassigned_footprint` per fitted Standard or Net Tie Component on a sheet with no
/// Footprint chosen: it never reaches the PCB, so no output shows it; objects are its
/// Units. The fix is to assign a Footprint and Sync. Mechanical and Not Fitted Components
/// need none.
fn unassigned_footprints(project: &Project) -> Vec<Violation> {
    let mut by_component: BTreeMap<Uuid, (&str, Vec<Uuid>)> = BTreeMap::new();
    for document in project.documents() {
        if let Some(sheet) = project.schematic_sheet(document.uuid) {
            for unit in sheet.units() {
                if unit.footprint.is_none()
                    && !unit.not_fitted
                    && unit.component_type.needs_symbol_and_footprint()
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
            Violation::new("unassigned_footprint", Severity::Warning)
                .arg("designator", designator)
                .objects(units)
        })
        .collect()
}

/// The warnings of a run, in a fixed order: out of sync, Components without a Footprint,
/// ERC errors, DRC errors, rules outside the preset's limits (`rule_outside_preset`, as
/// DRC reports it for this preset), missing fonts.
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
    found.extend(unassigned_footprints(project));
    let waivers = project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.waivers());
    // Library Status needs the Libraries, which a run does not read: it is left out here.
    let no_library = |uuids: &[reda_model::Uuid]| {
        uuids
            .iter()
            .map(|u| Err(reda_command::CommandError::not_found(u.to_string())))
            .collect()
    };
    let erc = apply_waivers(
        reda_schematic::erc::check(project, &no_library),
        waivers,
        false,
        |_| false,
    );
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
        |_| false,
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
    found.extend(reda_pcb::rules::presets::outside_preset_violations(
        pcb,
        &[reda_pcb::rules::presets::PresetRules {
            name: preset.name.clone(),
            rules: preset.rules,
        }],
        Severity::Warning,
    ));
    for font in reda_pcb::missing_fonts(pcb, fonts) {
        found.push(Violation::new("missing_font", Severity::Warning).arg("font", font));
    }
    found
}

/// One `bom_missing_field` per BOM row and `required_fields` entry the row has no value
/// for, in row order (outputs.md, BOM): args `field` (as the preset names it) and
/// `designators` (the row's, joined by the preset's separator); objects are their
/// Footprints. The row stays in the BOM. Mechanical Components and Not Fitted rows never
/// warn.
pub(crate) fn bom_missing_fields(
    project: &Project,
    pcb: &PcbDocument,
    preset: &FabPreset,
) -> Vec<Violation> {
    reda_output::bom_file(project, pcb, &preset.bom)
        .missing
        .into_iter()
        .map(|m| {
            Violation::new("bom_missing_field", Severity::Warning)
                .arg("field", m.field)
                .arg(
                    "designators",
                    m.designators.join(&preset.bom.designator_separator),
                )
                .objects(m.footprints)
        })
        .collect()
}
