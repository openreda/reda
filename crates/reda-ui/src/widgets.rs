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
mod dialog;
mod dropdown;
mod field;
pub mod glyph;
mod menu;
mod paint;
pub mod pending;
mod range;
mod segmented;
mod stepper;
mod surface;
mod tooltip;

pub use button::{
    Button, ICON_GAP, IconButton, Link, button, glyph_button, icon_button, icon_button_image, link,
};
pub use check::{
    Checkbox, MARK, Radio, SWITCH_KNOB, SWITCH_TRACK, Switch, checkbox, radio, switch,
};
pub use dialog::{Shown, Window, button_row, dialog_frame, scrim, title_row, window};
pub use dropdown::{
    Choice, ChoiceRow, Dropdown, FilterDropdown, Picked, choice, choice_value, dropdown,
    filter_dropdown,
};
pub use field::{InvalidMessage, TextField, invalid_message, number_field, text_field};
pub use glyph::{Glyph, GlyphMark, glyph};
pub use menu::{
    Menu, MenuButton, MenuItem, context_menu, menu, menu_bar, menu_button, menu_frame,
    menu_is_open, menu_item, menu_separator,
};
pub use paint::State;
pub use range::{PROGRESS_RAIL, Progress, SLIDER_KNOB, SLIDER_RAIL, Slider, progress, slider};
pub use segmented::{Segmented, segmented};
pub use stepper::{STEPPER_WIDTH, StepperField, stepper_field};
pub use surface::{CARD_GAP, card_frame, floating_frame, flyout_frame, flyout_title};
pub use tooltip::{
    KeyChip, TOOLTIP_WIDTH, Tooltip, TooltipText, key_chip, tooltip_contents, tooltip_frame,
};
