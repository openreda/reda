//! Fixed dimensions of recipes that are not built yet
//! (docs/spec/v0.1/design-language.md#fixed-values).
//!
//! A dimension no setting changes is a named constant beside its recipe. These wait here
//! for theirs: the slice that builds a recipe moves its constants into the recipe's file
//! and deletes them from this one, and the spec's table generator
//! (`theme/tables.rs`) follows the move.

/// The side of a data colour's swatch.
pub const SWATCH: f32 = 14.0;
/// A tree's indent per level.
pub const TREE_INDENT: f32 = 16.0;
/// The width of one window control in the Title bar.
pub const WINDOW_CONTROL: f32 = 46.0;
/// The drag gap kept beside the window controls.
pub const DRAG_GAP: f32 = 48.0;
/// The command field's least and greatest width.
pub const COMMAND_FIELD_MIN: f32 = 180.0;
pub const COMMAND_FIELD_MAX: f32 = 420.0;
