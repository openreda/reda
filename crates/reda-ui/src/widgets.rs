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
//! `section_header` and `section`, `form` with `form_row`, `status_mark`, `swatch`,
//! `badge` and `count_pill`.
//! So do the shell's: `tab`, `status_item`, `paint_card` with `mask_card_corners`, the
//! Dock `splitter`, and `drop_preview`, `drop_button` and `drag_chip` for a dragged tab.
//!
//! Every control draws rest, hover, press, keyboard focus and disabled, and selected and
//! invalid where they apply. Disabled comes from the `Ui` (`ui.add_enabled(false, ..)`).
//! [`State`] forces one state, for the gallery that snapshots them side by side
//! (`tests/widget_gallery.rs`).
//!
//! The Title bar's recipes are here too: `command_field`, `window_controls` and the slim
//! `floating_bar` of a floating window.
//!
//! Dimensions that no setting changes (the 28 × 16 switch track, the 16 px mark) are
//! named constants beside their recipe, not `Tokens` entries.

mod badge;
mod button;
mod check;
mod color;
mod command_field;
mod dialog;
mod drop;
mod dropdown;
mod field;
mod form;
pub mod glyph;
mod mark;
mod menu;
mod paint;
mod range;
mod row;
mod section;
mod segmented;
mod splitter;
mod status;
mod stepper;
mod surface;
mod tab;
mod table;
mod tooltip;
mod window_bar;

pub use badge::{BADGE_PAD_X, BADGE_PAD_Y, Badge, BadgeTone, badge, count_pill};
pub use button::{
    Button, ICON_GAP, IconButton, Link, button, glyph_button, icon_button, icon_button_image, link,
};
pub use check::{
    Checkbox, MARK, Radio, SWITCH_KNOB, SWITCH_TRACK, Switch, checkbox, radio, switch,
};
pub use color::{
    ColorButton, PICKER_FIELD, PICKER_HUE, PICKER_WIDTH, color_button, color_picker, data_color,
};
pub use command_field::{COMMAND_FIELD_MAX, COMMAND_FIELD_MIN, CommandField, command_field};
pub use dialog::{Shown, Window, button_row, dialog_frame, scrim, title_row, window, window_frame};
pub use drop::{DROP_BUTTON, DropPart, drag_chip, drop_button, drop_preview};
pub use dropdown::{
    Choice, ChoiceRow, Dropdown, FilterDropdown, Picked, choice, choice_value, dropdown,
    filter_dropdown,
};
pub use field::{InvalidMessage, TextField, invalid_message, number_field, text_field};
pub use form::{Form, FormName, form, form_name, form_row};
pub use glyph::{Glyph, GlyphMark, glyph};
pub use mark::{SWATCH, Status, StatusMark, Swatch, status_mark, swatch, swatch_none, swatch_rgb};
pub use menu::{
    Menu, MenuButton, MenuItem, context_menu, menu, menu_bar, menu_below, menu_button, menu_frame,
    menu_is_open, menu_item, menu_separator, menu_titles_width,
};
pub use paint::{State, drew_focus};
pub use range::{PROGRESS_RAIL, Progress, SLIDER_KNOB, SLIDER_RAIL, Slider, progress, slider};
pub use row::{Row, RowResponse, Selection, TREE_INDENT, list_row, rows, tree_row};
pub use section::{Section, SectionHeader, SectionResponse, section, section_header};
pub use segmented::{Segmented, segmented};
pub use splitter::splitter;
pub use status::{
    STATUS_ITEM_MIN, STATUS_ITEM_PAD, StatusItem, status_bar, status_ink, status_item,
};
pub use stepper::{STEPPER_WIDTH, StepperField, stepper_field};
pub use surface::{
    CARD_GAP, card_frame, floating_frame, flyout_frame, flyout_title, mask_card_corners, paint_card,
};
pub use tab::{
    TAB_INSET, Tab, TabKind, TabResponse, accent_line, dock_strip, join_tab, tab, tab_strip,
};
pub use table::{COLUMN_GAP, Column, RowLook, Sorted, Table, TableBody, TableRow, table};
pub use tooltip::{
    KeyChip, TOOLTIP_WIDTH, Tooltip, TooltipText, key_chip, tooltip_contents, tooltip_frame,
};
pub use window_bar::{
    FloatingBar, FloatingBarResponse, WINDOW_CONTROL, WindowAction, WindowControl, WindowControls,
    floating_bar, window_control, window_controls,
};
