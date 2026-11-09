//! The preview of an update or a Replace in words, sorted into the sections page 2 shows:
//! Symbol, Pins, Units, Footprints and the Part's name (with the field grid).

use serde_json::Value;

use crate::tr;

/// A section of one Component's preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    /// The Part's name, beside the field grid.
    Fields,
    Symbol,
    Pins,
    Units,
    Footprints,
}

impl Section {
    /// Every section, in the order page 2 shows them.
    pub const ALL: [Section; 5] = [
        Section::Fields,
        Section::Symbol,
        Section::Pins,
        Section::Units,
        Section::Footprints,
    ];

    /// The section's heading.
    pub fn title(self) -> String {
        match self {
            Section::Fields => tr!("library-update-section-fields"),
            Section::Symbol => tr!("library-update-section-symbol"),
            Section::Pins => tr!("library-update-section-pins"),
            Section::Units => tr!("library-update-section-units"),
            Section::Footprints => tr!("library-update-section-footprints"),
        }
    }
}

/// One `PartChange` of a preview: its section and its words.
pub fn change(change: &Value) -> (Section, String) {
    let s = |key: &str| change[key].as_str().unwrap_or_default().to_owned();
    let list = |key: &str| {
        (change[key].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    match change["kind"].as_str().unwrap_or_default() {
        "name" => (
            Section::Fields,
            tr!("library-update-name", from = s("from"), to = s("to")),
        ),
        "pin_map" => (Section::Footprints, tr!("library-update-pin-map")),
        "symbol" => (
            Section::Symbol,
            tr!("library-update-symbol", name = s("name")),
        ),
        "unit_removed" => (
            Section::Units,
            tr!("library-update-unit-removed", unit = s("unit")),
        ),
        "unit_added" => (
            Section::Units,
            tr!("library-update-unit-added", unit = s("unit")),
        ),
        "pin" => {
            let nets = if change.get("leaves").is_some() || change.get("joins").is_some() {
                tr!(
                    "library-update-pin-nets",
                    leaves = s("leaves"),
                    joins = s("joins")
                )
            } else {
                String::new()
            };
            let how = if change["added"] == true {
                tr!("library-update-pin-added")
            } else if change["removed"] == true {
                tr!("library-update-pin-removed")
            } else {
                tr!("library-update-pin-changed")
            };
            (
                Section::Pins,
                tr!(
                    "library-update-pin",
                    unit = s("unit"),
                    number = s("number"),
                    how = how,
                    nets = nets
                ),
            )
        }
        "footprint" => {
            let mut text = tr!(
                "library-update-footprint",
                name = s("name"),
                change = s("change")
            );
            for (key, label) in [
                ("pads_added", "library-update-pads-added"),
                ("pads_removed", "library-update-pads-removed"),
                ("pads_moved", "library-update-pads-moved"),
                ("pads_resized", "library-update-pads-resized"),
            ] {
                let pads = list(key);
                if !pads.is_empty() {
                    text += &crate::i18n::translate(label, &[("pads", crate::i18n::arg(pads))]);
                }
            }
            (Section::Footprints, text)
        }
        "footprint_order" => (Section::Footprints, tr!("library-update-footprint-order")),
        "footprint_choice" => (Section::Footprints, tr!("library-update-footprint-choice")),
        other => (Section::Fields, other.to_owned()),
    }
}

/// A Problem of a preview in words.
pub fn problem(problem: &Value) -> String {
    let s = |key: &str| problem[key].as_str().unwrap_or_default().to_owned();
    match problem["kind"].as_str().unwrap_or_default() {
        "pin_without_pad" => tr!(
            "library-update-pin-without-pad",
            unit = s("unit"),
            number = s("number")
        ),
        "placed_unit_removed" => tr!("library-update-placed-unit-removed", unit = s("unit")),
        "no_symbol" => tr!("library-update-no-symbol"),
        other => other.to_owned(),
    }
}

/// A cell's name as the grid shows it.
pub fn cell_name(cell: &str) -> String {
    match cell {
        "component_type" => tr!("library-update-cell-component-type"),
        "in_bom" => tr!("properties-in-bom"),
        "in_cpl" => tr!("properties-in-cpl"),
        "not_fitted" => tr!("properties-not-fitted"),
        "value" => tr!("properties-value"),
        "description" => tr!("properties-description"),
        "datasheet" => tr!("properties-datasheet"),
        "manufacturer" => tr!("properties-manufacturer"),
        "mpn" => tr!("properties-mpn"),
        "keywords" => tr!("properties-keywords"),
        other => other.to_owned(),
    }
}

/// A cell's case as the grid shows it.
pub fn case(case: &str) -> String {
    match case {
        "changed" => tr!("library-update-case-changed"),
        "same" => tr!("library-update-case-same"),
        "edited" => tr!("library-update-case-edited"),
        "added" => tr!("library-update-case-added"),
        "removed" => tr!("library-update-case-removed"),
        "component_only" => tr!("library-update-case-component-only"),
        "locked" => tr!("library-update-case-locked"),
        "cleared" => tr!("library-update-case-cleared"),
        other => other.to_owned(),
    }
}
