//! The Fab Presets REDA ships: Generic, JLCPCB, PCBWay and OSH Park.
//!
//! The limits come from the fab requirements research (openreda/reda#4), which read each
//! fab's capability and assembly pages in October 2026. Where a fab publishes two minimums
//! for one REDA rule, the preset takes the one that holds for every case the rule covers
//! (JLCPCB's hole spacing is 0.20 mm between vias but 0.45 mm between pad holes, so 0.45).
//! The preferred Track and Via sizes and the hole range are the seeds `checks.md` gives:
//! a 0.25 mm Track everywhere, and a looser Via on the per-fab presets (0.66/0.30 mm on
//! JLCPCB, so its ring meets the 0.18 mm minimum; 0.60/0.30 mm on PCBWay).

use std::sync::LazyLock;

use reda_model::geom::Nm;
use reda_model::{
    BomSettings, BottomRotation, Column, ColumnSource, Construction, CplSettings, FabPreset,
    FabRules, FileOrigin, GerberSettings, PreferredSeeds, StackupSeeds, StackupSuggestions,
};

/// The name of the preset a Project gets when none is asked for.
pub const GENERIC_NAME: &str = "Generic";

static SHIPPED: LazyLock<Vec<FabPreset>> =
    LazyLock::new(|| vec![generic(), jlcpcb(), pcbway(), osh_park()]);

/// Every preset REDA ships, Generic first.
pub fn shipped() -> Vec<FabPreset> {
    SHIPPED.clone()
}

/// The shipped preset with this name, compared case-insensitively.
pub fn shipped_named(name: &str) -> Option<FabPreset> {
    SHIPPED
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .cloned()
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
            silk_width: Some(Nm(150000)),
            silk_text_height: Some(Nm(1000000)),
            inner: None,
            microvia_size: None,
            via_aspect_ratio: None,
            clearance_hole: reda_model::HoleClearance::GENERIC,
            mask_expansion: reda_model::MaskExpansion {
                top: Nm(50_000),
                bottom: Nm(50_000),
                from_hole_edge: false,
                tent_vias_top: true,
                tent_vias_bottom: true,
            },
            paste_expansion: reda_model::PasteExpansion::Expansion {
                expansion: Nm::ZERO,
            },
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
        placement_corrections: Vec::new(),
        constructions: [2, 4, 6, 8]
            .map(|n| Construction::evenly_spaced(n, StackupSeeds::default()))
            .to_vec(),
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

/// Start with the released nominal two-layer dimensions and add the measured multilayer builds.
fn with_two_layer(catalogue: &str) -> Vec<Construction> {
    let mut builds = vec![Construction::evenly_spaced(2, StackupSeeds::default())];
    builds.extend(
        serde_json::from_str::<Vec<Construction>>(catalogue)
            .expect("shipped Construction catalogue"),
    );
    builds
}

/// JLCPCB's 2-layer standard service and its assembly BOM and CPL columns.
fn jlcpcb() -> FabPreset {
    use ColumnSource::*;
    let mut preset = FabPreset {
        name: "JLCPCB".into(),
        rules: FabRules {
            silk_width: Some(Nm(150000)),
            silk_text_height: Some(Nm(1000000)),
            inner: None,
            microvia_size: None,
            via_aspect_ratio: None,
            clearance_hole: reda_model::HoleClearance {
                via: Nm(200_000),
                pad: Nm(300_000),
            },
            mask_expansion: reda_model::MaskExpansion {
                top: Nm(0),
                bottom: Nm(0),
                from_hole_edge: false,
                tent_vias_top: true,
                tent_vias_bottom: true,
            },
            paste_expansion: reda_model::PasteExpansion::Expansion {
                expansion: Nm::ZERO,
            },
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
                // 0.66 mm, not checks.md's original 0.60, so the ring meets JLCPCB's 0.18 mm.
                via_diameter: Nm::from_um(660),
                via_hole: Nm::from_um(300),
            },
        },
        placement_corrections: Vec::new(),
        constructions: with_two_layer(include_str!("../data/constructions/jlcpcb.json")),
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
            silk_width: Some(Nm(150000)),
            silk_text_height: Some(Nm(800000)),
            inner: None,
            microvia_size: None,
            via_aspect_ratio: None,
            clearance_hole: reda_model::HoleClearance {
                via: Nm(200_000),
                pad: Nm(200_000),
            },
            mask_expansion: reda_model::MaskExpansion {
                top: Nm(50_000),
                bottom: Nm(50_000),
                from_hole_edge: false,
                tent_vias_top: true,
                tent_vias_bottom: true,
            },
            paste_expansion: reda_model::PasteExpansion::Expansion {
                expansion: Nm::ZERO,
            },
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
        placement_corrections: Vec::new(),
        constructions: with_two_layer(include_str!("../data/constructions/pcbway.json")),
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
    preset.bom.required_fields = vec!["MPN".into()];
    preset.bom.list_unplaced = false;
    preset.bom.line_numbers.enabled = true;
    preset.bom.mount_values.smt = "SMD".into();
    preset.cpl.unit_suffix = true;
    preset.cpl.exclude_through_hole = true;
    preset
}

/// The Prototype services use one flat rule table. The two-layer service sets the
/// limiting width, spacing and ring. The multilayer services set plated-hole isolation.
fn osh_park() -> FabPreset {
    let generic = generic();
    FabPreset {
        name: "OSH Park".into(),
        constructions: serde_json::from_str(include_str!("../data/constructions/osh-park.json"))
            .expect("shipped OSH Park Construction catalogue"),
        rules: FabRules {
            silk_width: Some(Nm(127000)),
            silk_text_height: None,
            microvia_size: None,
            via_aspect_ratio: None,
            track_width: Nm(152_400),
            clearance: Nm(152_400),
            annular_ring: Nm(127_000),
            via_hole: Nm(254_000),
            via_diameter: Nm(508_000),
            hole_min: Nm(254_000),
            // Above 250 mil the service mills holes with looser tolerances.
            hole_max: Nm(6_350_000),
            hole_to_hole: Nm(127_000),
            edge_clearance: Nm(381_000),
            clearance_hole: reda_model::HoleClearance {
                via: Nm(254_000),
                pad: Nm(254_000),
            },
            preferred: PreferredSeeds {
                via_diameter: Nm(600_000),
                ..generic.rules.preferred
            },
            mask_expansion: reda_model::MaskExpansion {
                top: Nm(50_800),
                bottom: Nm(50_800),
                ..generic.rules.mask_expansion
            },
            ..generic.rules
        },
        gerber: GerberSettings {
            slot_command: reda_model::SlotCommand::G85,
            ..generic.gerber
        },
        stackup_suggestions: suggestions("OSH Park"),
        ..generic
    }
}

/// Published 2-layer options, from the 2026-11-04 fab research (#142).
fn suggestions(name: &str) -> StackupSuggestions {
    let strings = |values: &[&str]| values.iter().map(|v| (*v).to_owned()).collect();
    let (thickness, copper, finish, mask_color, silk_color) = match name {
        "OSH Park" => (
            vec![1600],
            vec![35_600],
            strings(&["ENIG"]),
            strings(&["Purple"]),
            strings(&["White"]),
        ),
        "PCBWay" => (
            vec![
                200, 400, 600, 800, 1000, 1200, 1600, 2000, 2400, 2600, 2800, 3000, 3200,
            ],
            (1..=8).map(|oz| 35_000 * oz).collect(),
            strings(&[
                "HASL with lead",
                "HASL lead free",
                "Immersion Gold",
                "OSP",
                "Hard Gold",
                "Immersion Silver",
                "Immersion Tin",
                "ENEPIG",
            ]),
            strings(&[
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
            ]),
            strings(&["White", "Black", "Yellow", "None"]),
        ),
        "JLCPCB" => (
            vec![400, 600, 800, 1000, 1200, 1600, 2000],
            vec![35_000, 70_000, 87_500, 122_500, 157_500],
            strings(&["HASL (leaded)", "HASL (lead-free)", "ENIG", "OSP"]),
            strings(&["Green", "Purple", "Red", "Yellow", "Blue", "White", "Black"]),
            vec![],
        ),
        _ => (
            vec![400, 600, 800, 1000, 1200, 1600, 2000],
            vec![35_000, 70_000],
            strings(&["HASL (leaded)", "HASL (lead-free)", "ENIG", "OSP"]),
            strings(&["Green", "Purple", "Red", "Yellow", "Blue", "White", "Black"]),
            vec![],
        ),
    };
    StackupSuggestions {
        thickness: thickness.into_iter().map(Nm::from_um).collect(),
        copper: copper.into_iter().map(Nm).collect(),
        finish,
        mask_color,
        silk_color,
    }
}
