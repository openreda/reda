//! Pointer input on the PCB canvas: press, drag, release and cancel, rotate, flip and
//! board-shape moves, hover snapping, and the drawing Tools, all dispatched to the active
//! PCB Tool.

use super::*;

impl Shell {
    /// A button went down on the board at `at`; `toggle` is `Mod` held; `tolerance` is
    /// how near counts as on an object.
    pub fn pcb_press(&mut self, at: Point, toggle: bool, tolerance: Nm) {
        if self.drawing() {
            let mut draw = std::mem::take(&mut self.pcb_draw);
            draw.press(self, at);
            self.pcb_draw = draw;
            return;
        }
        let modifiers = egui::Modifiers {
            command: toggle,
            ..egui::Modifiers::NONE
        };
        self.with_pcb_tool(|tool, shell| tool.press(shell, at, modifiers, tolerance));
    }

    /// The canvas passes actual modifiers: Shift toggles picks, while Mod detaches drags.
    pub fn pcb_press_with_modifiers(
        &mut self,
        at: Point,
        modifiers: egui::Modifiers,
        tolerance: Nm,
    ) {
        if self.drawing() {
            self.pcb_press(at, modifiers.command, tolerance);
            return;
        }
        self.with_pcb_tool(|tool, shell| tool.press(shell, at, modifiers, tolerance));
    }

    /// The pointer moved to `at` with the button down.
    pub fn pcb_drag(&mut self, at: Point) {
        self.drag_started();
        if self.drawing() {
            let mut draw = std::mem::take(&mut self.pcb_draw);
            draw.drag(self, at);
            self.pcb_draw = draw;
            return;
        }
        self.with_pcb_tool(|tool, shell| tool.drag(shell, at));
    }

    /// The button came up: selects, or runs the move in flight as one Command.
    pub fn pcb_release(&mut self) {
        self.pcb_dragging = false;
        if self.drawing() {
            self.with_draw(|draw, shell| draw.release(shell));
            return;
        }
        self.with_pcb_tool(|tool, shell| tool.release(shell));
    }

    /// Esc: drops the move or rubber band in flight.
    pub fn pcb_cancel(&mut self) {
        self.with_pcb_tool(|tool, shell| tool.cancel(shell));
    }

    /// Space: turns the move in flight, or the selection, by 90°.
    pub(crate) fn pcb_rotate(&mut self) {
        self.with_pcb_tool(|tool, shell| tool.rotate(shell));
    }

    /// Move Board Shape: the Board Outline, with any selected Board Cutouts, follows the
    /// pointer until a click puts it down. Anything in flight is dropped first.
    /// With `whole`, Move Board: the outline and Cutouts show the move, and the click
    /// runs `pcb.board.move`.
    pub(crate) fn pcb_move_board_shape(&mut self, whole: bool) {
        selection::picking::cancel(self);
        self.pcb_cancel();
        self.with_router(|r, shell| r.cancel(shell));
        let tool = self.tool();
        self.pcb_draw.start(tool);
        self.with_pcb_tool(|tool, shell| tool.carry_board_shape(shell, whole));
    }

    /// L: flips the move in flight, or the selection, to the other side.
    pub(crate) fn pcb_flip(&mut self) {
        self.with_pcb_tool(|tool, shell| tool.flip(shell));
    }

    /// The cursor is at `at` with no button down: where it snaps.
    pub fn pcb_hover(&mut self, at: Point) -> Snapped {
        if self.drawing() {
            let mut draw = std::mem::take(&mut self.pcb_draw);
            let snapped = draw.hover(self, at);
            self.pcb_draw = draw;
            return snapped;
        }
        self.with_pcb_tool(|tool, shell| tool.hover_mut(shell, at))
    }

    /// Runs `f` on the PCB Tool, then shares what it has in flight with the placement
    /// Commands ([`pcb::placing::PlaceFlight`]).
    pub(crate) fn with_pcb_tool<T>(
        &mut self,
        f: impl FnOnce(&mut pcb::SelectTool, &mut Shell) -> T,
    ) -> T {
        let mut tool = std::mem::take(&mut self.pcb_tool);
        let out = f(&mut tool, self);
        self.pcb_tool = tool;
        let (moving, queue) = (
            self.pcb_tool.carries_footprints(self),
            self.pcb_tool.queue(),
        );
        let dragging = self.pcb_tool.copper_drag().is_some();
        if let Some(flight) = self
            .engine
            .view_mut()
            .get_mut::<pcb::copper_drag::DragFlight>()
        {
            flight.0 = dragging;
        }
        if let Some(flight) = pcb::placing::flight_mut(self) {
            flight.moving = moving;
            flight.queue = queue;
        }
        out
    }

    /// The PCB Tool, to read what it draws (snap marker, rubber band) and set its snap
    /// radius from the zoom.
    pub fn pcb_tool(&self) -> &pcb::SelectTool {
        &self.pcb_tool
    }

    pub fn pcb_tool_mut(&mut self) -> &mut pcb::SelectTool {
        &mut self.pcb_tool
    }

    /// Where the cursor last snapped.
    pub fn pcb_snap(&self) -> Option<Snapped> {
        if self.drawing() {
            return self.pcb_draw.snap();
        }
        self.pcb_tool.snap()
    }

    /// Whether a PCB drawing Tool is active (see `pcb::draw`).
    pub(super) fn drawing(&self) -> bool {
        self.tool().draws_on_board()
    }

    pub(super) fn with_draw(&mut self, f: impl FnOnce(&mut pcb::draw::Draw, &mut Shell)) {
        let mut draw = std::mem::take(&mut self.pcb_draw);
        f(&mut draw, self);
        self.pcb_draw = draw;
    }

    /// The PCB drawing Tools' state: the preview, a measurement, text waiting for words.
    pub fn pcb_draw(&self) -> &pcb::draw::Draw {
        &self.pcb_draw
    }

    /// Places the text the text Tool was clicked for, with these words (one Command).
    pub fn pcb_draw_text(&mut self, text: &str) {
        self.with_draw(|draw, shell| draw.place_text(shell, text));
    }

    /// Drops the text the text Tool was clicked for.
    pub fn pcb_cancel_text(&mut self) {
        self.pcb_draw.cancel_text();
    }

    /// Board point `at` as the cursor readout gives it: from the Output Origin, if the
    /// board has one.
    pub fn pcb_cursor_from_origin(&self, at: Point) -> Point {
        at - self.board_grid_origin()
    }
}
