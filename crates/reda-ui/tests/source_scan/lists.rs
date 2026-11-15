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

/// Stock egui widgets that have a REDA widget, banned outside the exempt modules. It
/// starts empty: the slice that lands a REDA widget adds the stock patterns it replaces,
/// for example
/// `StockWidget { pattern: "ui.button(", instead: "`widgets::button`" }` and
/// `StockWidget { pattern: "Button::new", instead: "`widgets::button`" }`.
/// A pattern that starts with a letter matches only at the start of a word. Files on
/// [`UNCONVERTED`] are skipped, so a new pattern fails only files already converted.
pub const STOCK_WIDGETS: &[StockWidget] = &[];

/// Files not yet converted, sorted. The scan skips a listed file and fails once it is
/// clean, so the slice that converts a file deletes its line here. Never add a file: a
/// new file must be clean from the start.
pub const UNCONVERTED: &[&str] = &[
    "agents.rs",
    "app.rs",
    "bars/draw.rs",
    "bars/groups.rs",
    "chrome.rs",
    "closing_dialog.rs",
    "command_dialog/draw.rs",
    "docking.rs",
    "docking/drag.rs",
    "footprint_editor/inspector.rs",
    "help/dialogs.rs",
    "icons.rs",
    "job_run/draw.rs",
    "journal_panel.rs",
    "keys_card.rs",
    "library_dialogs/generate/callouts.rs",
    "library_dialogs/generate/draw.rs",
    "library_dialogs/import.rs",
    "library_dialogs/new_library.rs",
    "library_dialogs/paste.rs",
    "library_dialogs/paste_array.rs",
    "library_dialogs/to_library.rs",
    "library_dialogs/wizard.rs",
    "modal.rs",
    "new_project.rs",
    "palette/draw.rs",
    "panels/find.rs",
    "panels/layers.rs",
    "panels/library/preview.rs",
    "panels/messages.rs",
    "panels/navigator.rs",
    "panels/properties.rs",
    "panels/properties/library.rs",
    "panels/properties/overview.rs",
    "panels/properties/rules.rs",
    "panels/properties/track.rs",
    "part_editor/view.rs",
    "pcb/canvas.rs",
    "pcb/layer_control.rs",
    "pcb/placement_bar.rs",
    "pcb/routing_bar.rs",
    "pcb/update_dialog.rs",
    "preferences_dialog/check_grid.rs",
    "preferences_dialog/draw.rs",
    "preferences_dialog/fab_preset_pages.rs",
    "preferences_dialog/grid_tables.rs",
    "projects_panel.rs",
    "recovery_dialog.rs",
    "schematic_dialogs/cross.rs",
    "schematic_dialogs/reannotate.rs",
    "schematic_dialogs/replace.rs",
    "schematic_dialogs/text.rs",
    "schematic_dialogs/update/draw.rs",
    "schematic_editor/decorate.rs",
    "selection/filter_ui.rs",
    "selection/picking/pick_list.rs",
    "settings/board_setup/draw.rs",
    "settings/board_setup/draw/actions.rs",
    "settings/board_setup/draw/grid.rs",
    "settings/board_setup/draw/stackup.rs",
    "settings/change_list/draw.rs",
    "settings/draw.rs",
    "settings/fab_preset/draw.rs",
    "settings/output_job/draw.rs",
    "settings/project/erc.rs",
    "settings/project/fab_presets.rs",
    "settings/project/libraries.rs",
    "settings/project/preset_pages.rs",
    "settings/project/properties.rs",
    "settings/project/schematic.rs",
    "shared_element.rs",
    "show_on_board.rs",
    "similar.rs",
    "snap/palette.rs",
    "start_page.rs",
    "status_bar.rs",
    "tab_switcher.rs",
];
