//! Update from Library settings: how the field cells of an update start and what happens to
//! Footprints and field positions. Page 1 of the dialog saves them with the Project (in
//! `<name>.reda`, key `library_update`); Command parameters override them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A field the Library Part has and the Component has no value for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LibraryFieldMissing {
    /// The cell starts as Update: the Component takes the field.
    #[default]
    Add,
    /// Update when the Library value is not blank.
    AddIfNotBlank,
    /// The cell starts as Reject.
    Skip,
}

/// A field the Component has and the Library Part does not (any more), with a value other
/// than the old default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComponentFieldMissing {
    /// The cell starts as Reject: the Component keeps its value.
    #[default]
    Keep,
    /// The cell starts as Update, which removes the field.
    Remove,
}

/// A changed default where the Component holds a value of its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocallyEdited {
    /// The cell starts as Reject: the Component keeps its value.
    #[default]
    Keep,
    /// The cell starts as Update: the Component takes the new default.
    TakeNew,
}

/// Which Footprint a Component chooses after its alternates are updated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FootprintChoice {
    /// The current Footprint while it is still an alternate, else the new default.
    #[default]
    KeepIfPresent,
    /// Always the new default.
    Default,
}

/// The Update from Library settings. A setting left out has its default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LibraryUpdateSettings {
    /// A field in the Library and not on the Component.
    pub library_field_missing: LibraryFieldMissing,
    /// A field on the Component and not in the Library.
    pub component_field_missing: ComponentFieldMissing,
    /// A changed default the Component has its own value for.
    pub locally_edited: LocallyEdited,
    /// The cells that may start as Update, by name: field names, `component_type`,
    /// `in_bom` and `in_cpl`. `null` (the default) is every one; a cell not listed starts
    /// as Reject.
    pub fields_to_update: Option<Vec<String>>,
    /// Footprints new in the Library join the alternates.
    pub add_new_footprints: bool,
    /// Footprints gone from the Library leave the alternates.
    pub remove_missing_footprints: bool,
    pub footprint_choice: FootprintChoice,
    /// Fields moved by hand stay where they are; off, they return to Autoposition.
    pub preserve_field_locations: bool,
    /// Which fields show stays as it is; off, the Designator and value show.
    pub preserve_field_visibility: bool,
}

impl Default for LibraryUpdateSettings {
    fn default() -> LibraryUpdateSettings {
        LibraryUpdateSettings {
            library_field_missing: LibraryFieldMissing::default(),
            component_field_missing: ComponentFieldMissing::default(),
            locally_edited: LocallyEdited::default(),
            fields_to_update: None,
            add_new_footprints: true,
            remove_missing_footprints: true,
            footprint_choice: FootprintChoice::default(),
            preserve_field_locations: true,
            preserve_field_visibility: true,
        }
    }
}

impl LibraryUpdateSettings {
    /// Whether the cell `name` is on the `fields_to_update` checklist.
    pub fn updates(&self, name: &str) -> bool {
        self.fields_to_update
            .as_ref()
            .is_none_or(|names| names.iter().any(|n| n == name))
    }
}
