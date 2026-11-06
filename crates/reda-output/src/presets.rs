//! The Fab Presets REDA ships: Generic, JLCPCB and PCBWay.
//!
//! The limits come from the fab requirements research (openreda/reda#4), which read each
//! fab's capability and assembly pages in October 2026. Where a fab publishes two minimums
//! for one REDA rule, the preset takes the one that holds for every case the rule covers
//! (JLCPCB's hole spacing is 0.20 mm between vias but 0.45 mm between pad holes, so 0.45).
//! The preferred Track and Via sizes and the hole range are the seeds `checks.md` gives:
//! a 0.25 mm Track everywhere, and a looser 0.60/0.30 mm Via on the per-fab presets.

use reda_model::geom::Nm;
use reda_model::{
    BomSettings, BottomRotation, Column, ColumnSource, CplSettings, FabPreset, FabRules,
    FileOrigin, GerberSettings, PreferredSeeds, StackupSeeds, StackupSuggestions,
};

/// The name of the preset a Project gets when none is asked for.
pub const GENERIC_NAME: &str = "Generic";

/// Every preset REDA ships, Generic first.
pub fn shipped() -> Vec<FabPreset> {
    vec![generic(), jlcpcb(), pcbway()]
}

/// The shipped preset with this name, compared case-insensitively.
pub fn shipped_named(name: &str) -> Option<FabPreset> {
    shipped()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

fn columns(list: &[(&str, ColumnSource)]) -> Vec<Column> {
    list.iter()
        .map(|(header, source)| Column::new(header, source.clone()))
        .collect()
}

/// The conservative common denominator: the strictest standard-service value across
/// JLCPCB, PCBWay, OSH Park and Aisler, so a board that passes it can go to any of them.
pub fn generic() -> FabPreset {
    use ColumnSource::*;
    FabPreset {
        name: GENERIC_NAME.into(),
        rules: FabRules {
            clearance: Nm::from_um(160),
            track_width: Nm::from_um(200),
            // A 0.30 mm drill with a 0.20 mm ring.
            via_diameter: Nm::from_um(700),
            via_hole: Nm::from_um(300),
            annular_ring: Nm::from_um(200),
            hole_to_hole: Nm::from_um(500),
            edge_clearance: Nm::from_um(400),
            hole_min: Nm::from_um(500),
            hole_max: Nm::from_um(6300),
            preferred: PreferredSeeds {
                track_width: Nm::from_um(250),
                via_diameter: Nm::from_um(700),
                via_hole: Nm::from_um(300),
            },
        },
        stackup: StackupSeeds::default(),
        gerber: GerberSettings::default(),
        zip_assembly: false,
        stackup_suggestions: suggestions(GENERIC_NAME),
        bom: BomSettings {
            columns: columns(&[
                ("Designator", Designator),
                ("Quantity", Quantity),
                ("Value", Value),
                ("Footprint", Footprint),
                ("Manufacturer", Field("Manufacturer".into())),
                ("MPN", Field("MPN".into())),
            ]),
            ..Default::default()
        },
        cpl: CplSettings {
            columns: columns(&[
                ("Designator", Designator),
                ("Mid X", CentreX),
                ("Mid Y", CentreY),
                ("Layer", Side),
                ("Rotation", Rotation),
            ]),
            ..Default::default()
        },
    }
}

/// JLCPCB's 2-layer standard service and its assembly BOM and CPL columns.
fn jlcpcb() -> FabPreset {
    use ColumnSource::*;
    let mut preset = FabPreset {
        name: "JLCPCB".into(),
        rules: FabRules {
            clearance: Nm::from_um(100),
            track_width: Nm::from_um(100),
            // A 0.15 mm drill with the 0.18 mm ring the rule asks of every hole. JLCPCB's
            // own Via minimum (0.25 mm, a 0.05 mm ring) would fail that rule, so the
            // default Via would fail DRC on a fresh board.
            via_diameter: Nm::from_um(510),
            via_hole: Nm::from_um(150),
            annular_ring: Nm::from_um(180),
            hole_to_hole: Nm::from_um(450),
            // Routed edges; V-cut edges need 0.40 mm.
            edge_clearance: Nm::from_um(200),
            hole_min: Nm::from_um(200),
            hole_max: Nm::from_um(6300),
            preferred: PreferredSeeds {
                track_width: Nm::from_um(250),
                via_diameter: Nm::from_um(600),
                via_hole: Nm::from_um(300),
            },
        },
        stackup: StackupSeeds::default(),
        gerber: GerberSettings::default(),
        zip_assembly: false,
        stackup_suggestions: suggestions("JLCPCB"),
        bom: BomSettings {
            columns: columns(&[
                ("Comment", Value),
                ("Designator", Designator),
                ("Footprint", Footprint),
                ("LCSC Part #", Field("LCSC".into())),
            ]),
            ..Default::default()
        },
        cpl: CplSettings {
            columns: columns(&[
                ("Designator", Designator),
                ("Mid X", CentreX),
                ("Mid Y", CentreY),
                ("Layer", Side),
                ("Rotation", Rotation),
            ]),
            ..Default::default()
        },
    };
    preset.stackup = StackupSeeds {
        thickness: Some(Nm::from_um(1600)),
        copper: Some(Nm::from_um(35)),
    };
    preset.bom.required_fields = vec!["LCSC".into()];
    preset.bom.list_unplaced = false;
    preset.cpl.bottom_rotation = BottomRotation::Negate;
    preset
}

/// PCBWay's standard service and its assembly BOM and centroid columns. PCBWay publishes
/// hole spacing and edge clearance in its Advanced table, but not its Standard table;
/// this standard-service preset therefore keeps the Generic values.
fn pcbway() -> FabPreset {
    use ColumnSource::*;
    let mut preset = FabPreset {
        name: "PCBWay".into(),
        rules: FabRules {
            clearance: Nm::from_um(100),
            track_width: Nm::from_um(100),
            // A 0.15 mm drill with a 0.15 mm ring.
            via_diameter: Nm::from_um(450),
            via_hole: Nm::from_um(150),
            annular_ring: Nm::from_um(150),
            hole_to_hole: Nm::from_um(500),
            edge_clearance: Nm::from_um(400),
            hole_min: Nm::from_um(200),
            hole_max: Nm::from_um(6300),
            preferred: PreferredSeeds {
                track_width: Nm::from_um(250),
                via_diameter: Nm::from_um(600),
                via_hole: Nm::from_um(300),
            },
        },
        stackup: StackupSeeds::default(),
        gerber: GerberSettings {
            file_origin: FileOrigin::Relative,
            ..Default::default()
        },
        zip_assembly: false,
        stackup_suggestions: suggestions("PCBWay"),
        bom: BomSettings {
            columns: columns(&[
                ("Item #", Line),
                ("Designator", Designator),
                ("Qty", Quantity),
                ("Manufacturer", Field("Manufacturer".into())),
                ("Mfg Part #", Field("MPN".into())),
                ("Description / Value", Value),
                ("Package/Footprint", Footprint),
                ("Type", Mount),
            ]),
            ..Default::default()
        },
        cpl: CplSettings {
            columns: columns(&[
                ("Designator", Designator),
                ("Footprint", Footprint),
                ("Mid X", CentreX),
                ("Mid Y", CentreY),
                ("Ref X", OriginX),
                ("Ref Y", OriginY),
                ("Pad X", Pad1X),
                ("Pad Y", Pad1Y),
                ("TB", Side),
                ("Rotation", Rotation),
                ("Comment", Value),
            ]),
            ..Default::default()
        },
    };
    preset.stackup = StackupSeeds {
        thickness: Some(Nm::from_um(1600)),
        copper: Some(Nm::from_um(35)),
    };
    preset.bom.required_fields = vec!["MPN".into()];
    preset.bom.list_unplaced = false;
    preset.bom.line_numbers.enabled = true;
    preset.bom.mount_values.smt = "SMD".into();
    preset.cpl.unit_suffix = true;
    preset.cpl.exclude_through_hole = true;
    preset
}

/// Published 2-layer options, from the 2026-11-04 fab research (#142).
fn suggestions(name: &str) -> StackupSuggestions {
    let strings = |values: &[&str]| values.iter().map(|v| (*v).to_owned()).collect();
    let thickness = if name == "PCBWay" {
        vec![
            200, 400, 600, 800, 1000, 1200, 1600, 2000, 2400, 2600, 2800, 3000, 3200,
        ]
    } else {
        vec![400, 600, 800, 1000, 1200, 1600, 2000]
    };
    let copper = match name {
        "JLCPCB" => vec![35_000, 70_000, 87_500, 122_500, 157_500],
        "PCBWay" => (1..=8).map(|oz| 35_000 * oz).collect(),
        _ => vec![35_000, 70_000],
    };
    StackupSuggestions {
        thickness: thickness.into_iter().map(Nm::from_um).collect(),
        copper: copper.into_iter().map(Nm).collect(),
        finish: strings(if name == "PCBWay" {
            &[
                "HASL with lead",
                "HASL lead free",
                "Immersion Gold",
                "OSP",
                "Hard Gold",
                "Immersion Silver",
                "Immersion Tin",
                "ENEPIG",
            ]
        } else {
            &["HASL (leaded)", "HASL (lead-free)", "ENIG", "OSP"]
        }),
        mask_color: strings(if name == "PCBWay" {
            &[
                "Green",
                "Red",
                "Yellow",
                "Blue",
                "White",
                "Black",
                "Matt green",
                "Matte black",
                "Purple",
                "none",
            ]
        } else {
            &["Green", "Purple", "Red", "Yellow", "Blue", "White", "Black"]
        }),
        silk_color: strings(if name == "PCBWay" {
            &["White", "Black", "Yellow", "None"]
        } else {
            &[]
        }),
    }
}
