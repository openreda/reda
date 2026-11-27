//! **Arrow keys move the mouse pointer** (`canvas.arrows_move_pointer`,
//! docs/spec/v0.3/input-and-defaults.md#the-edit-cursor): where the desktop does not let
//! REDA move the pointer it is shown ticked and disabled with a caption, as the system
//! title bar Preference is, and the stored value is left alone.

use egui::{Role, Ui, WidgetInfo};

use super::Cx;
use crate::preferences_dialog::{PreferencesDialog, Setting};
use crate::tr;
use crate::widgets;

const ARROWS_MOVE_POINTER: &str = "canvas.arrows_move_pointer";

/// Whether the desktop refuses what `setting` asks.
fn refused(ui: &Ui, setting: &Setting) -> bool {
    setting.key == ARROWS_MOVE_POINTER && !crate::edit_cursor::moves_pointer(ui.ctx())
}

impl PreferencesDialog {
    /// Draws the disabled, ticked check box of a setting the desktop refuses, and says
    /// whether it did.
    pub(super) fn refused_control(
        &mut self,
        ui: &mut Ui,
        _cx: &mut Cx<'_>,
        setting: &Setting,
    ) -> bool {
        if !refused(ui, setting) {
            return false;
        }
        let label = setting.label.clone();
        ui.add_enabled_ui(false, |ui| {
            let response = ui.add(widgets::checkbox(&mut true, ""));
            response
                .widget_info(|| WidgetInfo::selected(Role::CheckBox, false, true, label.clone()));
        });
        true
    }

    /// The caption under a setting the desktop refuses.
    pub(super) fn refused_caption(&self, ui: &Ui, setting: &Setting) -> Option<String> {
        refused(ui, setting).then(|| tr!("pref-canvas-arrows-move-pointer-refused"))
    }
}
