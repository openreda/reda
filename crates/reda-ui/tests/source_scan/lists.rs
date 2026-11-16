//! The lists the source scan runs against. Paths are below `crates/reda-ui/src`.

use crate::rules::StockWidget;

/// Modules the scan does not read: a file, or a folder ending in `/`.
pub const EXEMPT: &[&str] = &[
    // Where styling lives.
    "theme.rs",
    "theme/",
    "widgets.rs",
    "widgets/",
    // On-canvas drawing: world-space geometry, Layer colours and the marks a Tool draws
    // over a Scene. A file that also draws chrome (a popup, a button) is not one of these.
    "canvas/",
    "pcb/copper_drag.rs",
    "pcb/dimension_edit.rs",
    "pcb/live_hover.rs",
    "pcb/polygon_edit.rs",
    "pcb/route.rs",
    "snap.rs",
];

/// Stock egui widgets that have a REDA widget, banned outside the exempt modules. The
/// slice that lands a REDA widget adds the stock patterns it replaces.
/// A pattern that starts with a letter matches only at the start of a word. Files on
/// [`UNCONVERTED`] are skipped, so a new pattern fails only files already converted.
pub const STOCK_WIDGETS: &[StockWidget] = &[
    // The controls (`widgets/button.rs`, `field.rs`, `dropdown.rs`, `check.rs`,
    // `range.rs`, `segmented.rs`, `stepper.rs`).
    StockWidget {
        pattern: "ui.button(",
        instead: "`widgets::button`, or `widgets::menu_item` in a menu",
    },
    StockWidget {
        pattern: "Button::new",
        instead: "`widgets::button`, or `widgets::menu_item` in a menu",
    },
    StockWidget {
        pattern: "ui.small_button(",
        instead: "`widgets::button`, or `widgets::menu_item` in a menu",
    },
    StockWidget {
        pattern: "Button::image",
        instead: "`widgets::icon_button`",
    },
    StockWidget {
        pattern: "ImageButton",
        instead: "`widgets::icon_button`",
    },
    StockWidget {
        pattern: "ui.toggle_value(",
        instead: "`widgets::button(..).on(..)`",
    },
    StockWidget {
        pattern: "ui.link(",
        instead: "`widgets::link`",
    },
    StockWidget {
        pattern: "Link::new",
        instead: "`widgets::link`",
    },
    StockWidget {
        pattern: "ui.hyperlink(",
        instead: "`widgets::link`",
    },
    StockWidget {
        pattern: "ui.hyperlink_to(",
        instead: "`widgets::link`",
    },
    StockWidget {
        pattern: "Hyperlink::",
        instead: "`widgets::link`",
    },
    StockWidget {
        pattern: "TextEdit::",
        instead: "`widgets::text_field`",
    },
    StockWidget {
        pattern: "ui.text_edit_singleline(",
        instead: "`widgets::text_field`",
    },
    StockWidget {
        pattern: "ui.text_edit_multiline(",
        instead: "`widgets::text_field`",
    },
    StockWidget {
        pattern: "ui.code_editor(",
        instead: "`widgets::text_field`",
    },
    StockWidget {
        pattern: "DragValue::",
        instead: "`widgets::number_field` or `widgets::stepper_field`",
    },
    StockWidget {
        pattern: "ui.drag_angle(",
        instead: "`widgets::number_field`",
    },
    StockWidget {
        pattern: "ComboBox::",
        instead: "`widgets::dropdown` or `widgets::filter_dropdown`",
    },
    StockWidget {
        pattern: "ui.checkbox(",
        instead: "`widgets::checkbox`, or `widgets::switch` for a setting that applies at once",
    },
    StockWidget {
        pattern: "Checkbox::",
        instead: "`widgets::checkbox`, or `widgets::switch` for a setting that applies at once",
    },
    StockWidget {
        pattern: "ui.radio(",
        instead: "`widgets::radio`",
    },
    StockWidget {
        pattern: "ui.radio_value(",
        instead: "`widgets::radio`",
    },
    StockWidget {
        pattern: "RadioButton::",
        instead: "`widgets::radio`",
    },
    StockWidget {
        pattern: "Slider::new",
        instead: "`widgets::slider`",
    },
    StockWidget {
        pattern: "ProgressBar::",
        instead: "`widgets::progress`",
    },
    // The popups and containers (`widgets/menu.rs`, `tooltip.rs`, `dialog.rs`,
    // `surface.rs`).
    StockWidget {
        pattern: "ui.menu_button(",
        instead: "`widgets::menu_button`",
    },
    StockWidget {
        pattern: "MenuBar::",
        instead: "`widgets::menu_bar`",
    },
    StockWidget {
        pattern: "MenuButton::",
        instead: "`widgets::menu_button`",
    },
    StockWidget {
        pattern: "SubMenuButton::",
        instead: "`widgets::menu_button` inside a menu",
    },
    StockWidget {
        pattern: "SubMenu::",
        instead: "`widgets::menu_button` inside a menu",
    },
    StockWidget {
        pattern: ".context_menu(",
        instead: "`widgets::context_menu`",
    },
    StockWidget {
        pattern: "Popup::menu(",
        instead: "`widgets::menu`",
    },
    StockWidget {
        pattern: "Popup::context_menu(",
        instead: "`widgets::context_menu`",
    },
    StockWidget {
        pattern: "Frame::menu(",
        instead: "`widgets::menu`, or `widgets::menu_frame` for one drawn in place",
    },
    StockWidget {
        pattern: "Frame::popup(",
        instead: "`widgets::menu`, `widgets::floating_frame` or the `widgets::Tooltip` methods",
    },
    StockWidget {
        pattern: "Frame::window(",
        instead: "`widgets::window`, or `widgets::floating_frame` for a bar on the canvas",
    },
    StockWidget {
        pattern: "Frame::group(",
        instead: "`widgets::card_frame`",
    },
    StockWidget {
        pattern: "Window::new",
        instead: "`widgets::window`, or `modal::Dialog` for a modal one",
    },
    StockWidget {
        pattern: "Modal::new",
        instead: "`modal::Dialog`",
    },
    StockWidget {
        pattern: ".on_hover_text(",
        instead: "`.tooltip(..)` of `widgets::Tooltip`",
    },
    StockWidget {
        pattern: ".on_hover_text_at_pointer(",
        instead: "`.tooltip(..)` of `widgets::Tooltip`",
    },
    StockWidget {
        pattern: ".on_hover_ui(",
        instead: "`.tooltip_ui(..)` of `widgets::Tooltip`",
    },
    StockWidget {
        pattern: ".on_hover_ui_at_pointer(",
        instead: "`.tooltip_ui(..)` of `widgets::Tooltip`",
    },
    StockWidget {
        pattern: ".on_disabled_hover_text(",
        instead: "`.disabled_tooltip(..)` of `widgets::Tooltip`",
    },
    StockWidget {
        pattern: ".on_disabled_hover_ui(",
        instead: "`.disabled_tooltip_ui(..)` of `widgets::Tooltip`",
    },
    StockWidget {
        pattern: "Tooltip::for_",
        instead: "the `widgets::Tooltip` methods",
    },
    // The data widgets (`widgets/row.rs`, `table.rs`, `section.rs`, `form.rs`).
    StockWidget {
        pattern: "selectable_label(",
        instead: "`widgets::list_row` or `widgets::tree_row`, `widgets::choice` in a dropdown, or `widgets::button(..).on(..)` for a toggle",
    },
    StockWidget {
        pattern: "selectable_value(",
        instead: "`widgets::list_row` or `widgets::tree_row`, `widgets::choice_value` in a dropdown, or `widgets::segmented`",
    },
    StockWidget {
        pattern: "Button::selectable",
        instead: "`widgets::list_row` or `widgets::tree_row`",
    },
    StockWidget {
        pattern: "CollapsingHeader",
        instead: "`widgets::section`, or `widgets::section_header` when the caller keeps the open state",
    },
    StockWidget {
        pattern: "ui.collapsing(",
        instead: "`widgets::section`",
    },
    StockWidget {
        pattern: "Grid::new",
        instead: "`widgets::form` with `widgets::form_row` for names and values, or `widgets::table`",
    },
];

/// Files not yet converted, sorted. The scan skips a listed file and fails once it is
/// clean, so the slice that converts a file deletes its line here. Never add a file: a
/// new file must be clean from the start.
pub const UNCONVERTED: &[&str] = &[
    "bars/draw.rs",
    "bars/groups.rs",
    "closing_dialog.rs",
    "footprint_editor/inspector.rs",
    "help/dialogs.rs",
    "icons.rs",
    "job_run/draw.rs",
    "keys_card.rs",
    "new_project.rs",
    "palette/draw.rs",
    "part_editor/view.rs",
    "pcb/canvas.rs",
    "pcb/layer_control.rs",
    "pcb/placement_bar.rs",
    "pcb/routing_bar.rs",
    "pcb/update_dialog.rs",
    "recovery_dialog.rs",
    "show_on_board.rs",
    "start_page.rs",
];
