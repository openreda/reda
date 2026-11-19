//! The Track Tool's side of the Shell: the band, hovering and clicking a route, the
//! router, and the board-only checks (swap a selected pair, mirror refusal, whether the
//! board is active).

use super::*;

impl Shell {
    pub(crate) fn design_generation(&self) -> u64 {
        self.edits
    }

    pub(super) fn prepare_route_command(&mut self) {
        self.board_view_mut().defer_layer_activation = self.router.routing() && self.tool_preview().is_some();
    }

    /// A Layer Command already resolved visibility, direction and presets. Apply its
    /// destination to the route first, switching the view only for a successful jump.
    pub(super) fn apply_route_command(&mut self, id: &str, out: &mut Value) {
        if self.board_view().defer_layer_activation && id.starts_with("pcb.layer.")
            && let Some(target) = out["layer"].as_str().and_then(|s| s.parse().ok())
        {
            self.with_router(|r, shell| { r.jump(shell, target, true); });
            out["layer"] = json!(self.engine.project().and_then(|p| self.board_view().active_layer(p)));
        }
        match id {
            "pcb.route.previous_layer" | "pcb.route.add_via" => self.with_router(|r,s| r.add_via(s)),
            "pcb.route.pick_via" => {
                if let Ok(params) = serde_json::from_value(out.clone()) {
                    self.with_router(|r,s| r.pick_via(s, params));
                }
            }
            "pcb.route.cycle_via" => self.with_router(|r,s| r.cycle_via(s)),
            _ => {}
        }
    }

    /// The rubber band's corners while one is drawn.
    pub fn pcb_band(&self) -> Option<(Point, Point)> {
        self.pcb_tool.band()
    }

    /// The cursor moved to the board point `at` while routing or placing Vias.
    pub fn route_hover(&mut self, at: Point) {
        self.with_router(|r, shell| r.hover(shell, at));
    }

    /// A click at the board point `at` while routing or placing Vias.
    pub fn route_click(&mut self, at: Point) {
        self.with_router(|r, shell| r.click(shell, at));
    }

    /// The route in flight, if any.
    pub fn route_preview(&self) -> Option<pcb::route::Preview> {
        self.router.preview()
    }

    pub fn router(&self) -> &pcb::Router {
        &self.router
    }

    pub fn router_mut(&mut self) -> &mut pcb::Router {
        &mut self.router
    }

    pub(super) fn with_router(&mut self, f: impl FnOnce(&mut pcb::Router, &mut Shell)) {
        let mut router = std::mem::take(&mut self.router);
        f(&mut router, self);
        self.router = router;
    }

    /// Whether the active Document is the board, whose Tool `tool.rotate` and
    /// `tool.flip_side` turn.
    /// Swap with a pair of Footprints already selected: swaps them at once and goes back
    /// to Select. With no pair the Tool asks for two picks.
    pub(super) fn swap_selected_pair(&mut self) {
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()) else {
            return;
        };
        let pair: Vec<Uuid> = self
            .selection()
            .objects
            .iter()
            .copied()
            .filter(|o| pcb.footprint(*o).is_some())
            .collect();
        if let [a, b] = pair[..]
            && self.selection().objects.len() == 2
        {
            let _ = self.run_reporting("pcb.swap", json!({"a": a, "b": b}));
            let _ = self.run_reporting("tool.activate", json!({"tool": "select"}));
        }
    }

    /// X or Y on the board: refused when they would mirror a Footprint, carried by the
    /// move in flight or else selected. The board has only sides; L flips instead.
    pub(super) fn board_mirror_refused(&self) -> bool {
        if !self.board_is_active() {
            return false;
        }
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()) else {
            return false;
        };
        let objects = match self.pcb_tool.carried() {
            Some(carried) => carried,
            None => &self.selection().objects[..],
        };
        objects.iter().any(|o| pcb.footprint(*o).is_some())
    }

    pub(crate) fn board_is_active(&self) -> bool {
        let pcb = self.engine.project().and_then(|p| p.pcb()).map(|p| p.uuid);
        pcb.is_some() && self.active_document().map(|d| d.document) == pcb
    }
}
