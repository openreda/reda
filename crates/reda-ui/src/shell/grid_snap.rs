//! The Shell's grids and snapping: each Document's snap grid, its origin and electrical
//! range, snapping a point, the held snap keys, the Selection Filter, and following the
//! grid Preferences.

use super::*;

impl Shell {
    /// `document`'s snap grid; it changes through `view.grid.set` and `view.grid.cycle`.
    pub fn grid(&self, document: Uuid) -> Nm {
        self.engine
            .view()
            .get::<Grids>()
            .map(|g| g.of(self.engine.elements(), document))
            .unwrap_or(grid::SCHEMATIC_DEFAULT)
    }

    /// The grids in force, their preset tables and custom values.
    pub fn grids(&self) -> Grids {
        self.engine
            .view()
            .get::<Grids>()
            .cloned()
            .unwrap_or_default()
    }

    /// The Electrical Range on a sheet or Symbol: its grid row's, else 0.8 of the step.
    pub fn electrical_range(&self, document: Uuid) -> Nm {
        self.engine
            .view()
            .get::<Grids>()
            .map(|g| g.range(self.engine.elements(), document))
            .unwrap_or_else(|| grid::default_range(self.grid(document)))
    }

    /// The origin the Document's grid runs through (the Output Origin on a board).
    pub fn grid_origin(&self, document: Uuid) -> Point {
        grid::origin(self.engine.elements(), document)
    }

    /// The open board's grid origin: its Output Origin, or zero while no board is open
    /// or none is placed. The board tools snap, and the cursor readout measures, from it.
    pub fn board_grid_origin(&self) -> Point {
        self.engine
            .project()
            .and_then(|p| p.pcb())
            .map_or_else(Point::default, |pcb| self.grid_origin(pcb.uuid))
    }

    /// A Document point on its grid level, including the board's Output Origin; left
    /// where it is when Gridless, or with the grid suspended.
    pub fn snap_to_grid(&self, document: Uuid, at: (f64, f64)) -> Point {
        crate::snap::to_grid(self, document, at)
    }

    /// Feeds this frame's keys over `document`'s canvas to the snap suspend keys
    /// ([`crate::snap::Suspend`]): `Mod` and Shift held, and the primary button down.
    pub fn hold_snap_keys(&mut self, document: Uuid, modifiers: egui::Modifiers, pressed: bool) {
        let keys = crate::snap::held(self, document, modifiers, pressed);
        self.engine.view_mut().set(keys);
        let alt = self.modifier_roles().constrain.held(&modifiers);
        (self.engine.view_mut()).set(crate::schematic_editor::DragKeys { alt });
    }

    /// `document`'s Selection Filter; it changes through `selection.filter.set`.
    pub fn selection_filter(&self, document: Uuid) -> SelectionFilter {
        self.engine
            .view()
            .get::<WorkingState>()
            .map(|w| w.filter(document))
            .unwrap_or_default()
    }

    /// Starts new views on the snap grids the Preferences now pick.
    pub(super) fn follow_grid_preferences(&mut self) {
        let prefs = preferences::current(self.engine.user_dirs());
        if let Some(grids) = self.engine.view_mut().get_mut::<Grids>() {
            grids.follow(&prefs);
        }
        if let Some(working) = self.engine.view_mut().get_mut::<WorkingState>() {
            working.follow(&prefs);
        }
    }
}
