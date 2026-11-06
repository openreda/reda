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
/// ERC errors, DRC errors, rules looser than the preset's minimums, missing fonts.
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
    let erc = apply_waivers(reda_schematic::erc::check(project), waivers, false, |_| {
        false
    });
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
    found.extend(loose_rules(pcb, preset));
    for font in reda_pcb::missing_fonts(pcb, fonts) {
        found.push(Violation::new("missing_font", Severity::Warning).arg("font", font));
    }
    found
}

/// A `rule_below_preset` for each enabled rule record value, board-wide or a Net Class's,
/// that allows less than the preset's minimum: `rule` (the kind), `value` (which of its
/// values), `board` (the value), `minimum` (the preset's), `preset` (its name) and, for a
/// class record, `net_class`. Its objects are the class, if any, then the record.
fn loose_rules(pcb: &PcbDocument, preset: &FabPreset) -> Vec<Violation> {
    use reda_model::{RuleField, RuleKind};
    let min = preset.rules;
    let minimum = |kind: RuleKind, field: RuleField| match (kind, field) {
        (RuleKind::Clearance, RuleField::Value) => Some(min.clearance),
        (RuleKind::TrackWidth, RuleField::Min) => Some(min.track_width),
        (RuleKind::ViaSize, RuleField::DiameterMin) => Some(min.via_diameter),
        (RuleKind::ViaSize, RuleField::HoleMin) => Some(min.via_hole),
        (RuleKind::HoleSize, RuleField::Min) => Some(min.hole_min),
        (RuleKind::AnnularRing, RuleField::Min) => Some(min.annular_ring),
        (RuleKind::HoleToHole, RuleField::Min) => Some(min.hole_to_hole),
        (RuleKind::EdgeClearance, RuleField::Value) => Some(min.edge_clearance),
        _ => None,
    };
    let mut records: Vec<_> = pcb.rules().filter(|r| r.enabled).collect();
    records.sort_by_key(|r| {
        let rank = r
            .scope
            .net_class
            .and_then(|c| pcb.net_class(c))
            .map_or(0, |c| c.rank);
        (rank, r.kind)
    });
    let mut found = Vec::new();
    for record in records {
        for &field in record.kind.fields() {
            let (Some(value), Some(minimum)) =
                (record.values.length(field), minimum(record.kind, field))
            else {
                continue;
            };
            if value >= minimum {
                continue;
            }
            let class = record.scope.net_class.and_then(|c| pcb.net_class(c));
            let mut v = Violation::new("rule_below_preset", Severity::Warning)
                .arg("rule", record.kind.id())
                .arg("value", field.path())
                .arg("board", value)
                .arg("minimum", minimum)
                .arg("preset", preset.name.as_str());
            if let Some(class) = class {
                v = v
                    .arg("net_class", class.name.as_str())
                    .objects([class.uuid]);
            }
            found.push(v.objects([record.uuid]));
        }
    }
    found
}
