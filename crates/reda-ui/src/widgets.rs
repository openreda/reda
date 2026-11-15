//! REDA's widgets: the recipes of the chrome's controls
//! (docs/spec/v0.1/design-language.md#components, ADR-0015).
//!
//! Each widget is a thin builder on stable egui that picks its colours, paddings, radii
//! and text styles from `ui.tokens()` and nothing else, so it follows light and dark,
//! Density and the text size without any code of its own. Use one with `ui.add`:
//!
//! ```ignore
//! if ui.add(widgets::button("Save").primary()).clicked() { .. }
//! ui.add(widgets::text_field(&mut name).hint("Net name"));
//! ui.add(widgets::checkbox(&mut visible, "Visible"));
//! ```
//!
//! The data widgets work the same way: `widgets::list_row`, `tree_row`, `table`,
//! `section_header` and `section`, `form` with `form_row`, `status_mark` and `swatch`.
//!
//! Every control draws rest, hover, press, keyboard focus and disabled, and selected and
//! invalid where they apply. Disabled comes from the `Ui` (`ui.add_enabled(false, ..)`).
//! [`State`] forces one state, for the gallery that snapshots them side by side
//! (`tests/widget_gallery.rs`).
//!
//! Dimensions that no setting changes (the 28 × 16 switch track, the 16 px mark) are
//! named constants beside their recipe, not `Tokens` entries. Those of recipes that are
//! not built yet wait in [`pending`].

mod button;
mod check;
mod dropdown;
mod field;
mod form;
pub mod glyph;
mod mark;
mod menu;
mod paint;
pub mod pending;
mod range;
mod row;
mod section;
mod segmented;
mod stepper;
mod table;

pub use button::{
    Button, ICON_GAP, IconButton, Link, button, glyph_button, icon_button, icon_button_image, link,
};
pub use check::{
    Checkbox, MARK, Radio, SWITCH_KNOB, SWITCH_TRACK, Switch, checkbox, radio, switch,
};
pub use dropdown::{
    Choice, ChoiceRow, Dropdown, FilterDropdown, Picked, choice, choice_value, dropdown,
    filter_dropdown,
};
pub use field::{InvalidMessage, TextField, invalid_message, number_field, text_field};
pub use form::{Form, FormName, form, form_name, form_row};
pub use glyph::{Glyph, GlyphMark, glyph};
pub use mark::{SWATCH, Status, StatusMark, Swatch, status_mark, swatch, swatch_rgb};
pub use menu::menu_item;
pub use paint::State;
pub use range::{PROGRESS_RAIL, Progress, SLIDER_KNOB, SLIDER_RAIL, Slider, progress, slider};
pub use row::{Row, RowResponse, Selection, TREE_INDENT, list_row, rows, tree_row};
pub use section::{Section, SectionHeader, SectionResponse, section, section_header};
pub use segmented::{Segmented, segmented};
pub use stepper::{STEPPER_WIDTH, StepperField, stepper_field};
pub use table::{COLUMN_GAP, Column, RowLook, Sorted, Table, TableBody, TableRow, table};
