//! The Fab Presets REDA ships: Generic, JLCPCB and PCBWay.
//!
//! The values come from the fab requirements research (openreda/reda#4), which read each
//! fab's capability and assembly pages in October 2026. Where a fab publishes two minimums
//! for one REDA rule, the preset takes the one that holds for every case the rule covers
//! (JLCPCB's hole spacing is 0.20 mm between vias but 0.45 mm between pad holes, so 0.45).

use reda_model::geom::Nm;
use reda_model::{
    Column, ColumnSource, FabPreset, FileNaming, GerberSettings, Rules, StackupSeeds,
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

/// Gerber X2 with Protel extensions, which every fab in the research reads.
const X2_PROTEL: GerberSettings = GerberSettings {
    x2_attributes: true,
    naming: FileNaming::Protel,
};

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
        rules: Rules {
            clearance: Nm::from_um(160),
            track_width: Nm::from_um(200),
            // A 0.30 mm drill with a 0.20 mm ring.
            via_diameter: Nm::from_um(700),
            via_hole: Nm::from_um(300),
            annular_ring: Nm::from_um(200),
            hole_to_hole: Nm::from_um(500),
            edge_clearance: Nm::from_um(400),
        },
        stackup: StackupSeeds::default(),
        gerber: X2_PROTEL,
        bom: columns(&[
            ("Designator", Designator),
            ("Quantity", Quantity),
            ("Value", Value),
            ("Footprint", Footprint),
            ("Manufacturer", Field("Manufacturer".into())),
            ("MPN", Field("MPN".into())),
        ]),
        cpl: columns(&[
            ("Designator", Designator),
            ("Mid X", MidX),
            ("Mid Y", MidY),
            ("Layer", Layer),
            ("Rotation", Rotation),
        ]),
    }
}

/// JLCPCB's 2-layer standard service and its assembly BOM and CPL columns.
fn jlcpcb() -> FabPreset {
    use ColumnSource::*;
    FabPreset {
        name: "JLCPCB".into(),
        rules: Rules {
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
        },
        stackup: StackupSeeds::default(),
        gerber: X2_PROTEL,
        bom: columns(&[
            ("Comment", Value),
            ("Designator", Designator),
            ("Footprint", Footprint),
            ("LCSC Part #", Field("LCSC".into())),
        ]),
        cpl: columns(&[
            ("Designator", Designator),
            ("Mid X", MidX),
            ("Mid Y", MidY),
            ("Layer", Layer),
            ("Rotation", Rotation),
        ]),
    }
}

/// PCBWay's standard service and its assembly BOM and centroid columns. PCBWay publishes
/// no hole spacing or edge clearance, so those keep the Generic values.
fn pcbway() -> FabPreset {
    use ColumnSource::*;
    FabPreset {
        name: "PCBWay".into(),
        rules: Rules {
            clearance: Nm::from_um(100),
            track_width: Nm::from_um(100),
            // A 0.15 mm drill with a 0.15 mm ring.
            via_diameter: Nm::from_um(450),
            via_hole: Nm::from_um(150),
            annular_ring: Nm::from_um(150),
            hole_to_hole: Nm::from_um(500),
            edge_clearance: Nm::from_um(400),
        },
        stackup: StackupSeeds::default(),
        gerber: X2_PROTEL,
        bom: columns(&[
            ("Line #", Line),
            ("Quantity", Quantity),
            ("Reference Designator", Designator),
            ("Part Number", Field("MPN".into())),
            ("Manufacturer", Field("Manufacturer".into())),
            ("Description", Value),
            ("Package", Footprint),
            ("Type", Mount),
        ]),
        cpl: columns(&[
            ("Designator", Designator),
            ("X", MidX),
            ("Y", MidY),
            ("Rotation", Rotation),
            ("Side", Layer),
        ]),
    }
}
