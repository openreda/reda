//! Refreshing what the tabs show after a run: the Scenes, the Selection and board view,
//! the edit follow-ups ([`Shell::after_edit`]) and sheet turns.

use super::*;

impl Shell {
    /// Rebuilds every tab's Scene and closes tabs whose Document is gone. With no
    /// Project open, the centre shows the Start page again.
    pub(crate) fn refresh(&mut self) {
        let elements: Vec<(Uuid, String)> = crate::element_editor::open_elements(&self.engine)
            .into_iter()
            .filter_map(|e| Some((e, crate::element_editor::title(&self.engine, e)?)))
            .collect();
        let project = self.engine.project();
        if project.is_none() && elements.is_empty() {
            self.documents.clear();
            self.replace_layout(Layout::default());
            self.change_list = None;
            self.messages.clear_runs();
            self.messages.set_notes(FONTS, Vec::new());
            self.messages.set_notes(MISSING_DOCUMENTS, Vec::new());
            return;
        }
        let no_project = project.is_none();
        let mut open: Vec<(Uuid, String)> = project
            .iter()
            .flat_map(|p| p.documents())
            .map(|d| (d.uuid, d.title.clone()))
            .collect();
        open.extend(elements);
        self.layout_mut().retain_tabs(|t| match t {
            // The Start page stays beside Library element tabs while no Project is open.
            TabKey::Start => no_project,
            TabKey::Document(uuid) => open.iter().any(|(u, _)| u == uuid),
            TabKey::Panel(_) => true,
            // The Project's settings windows close with it.
            TabKey::Settings(window) => !(window.of_project() && no_project),
        });
        self.forget_closed_documents();
        let mut documents = std::mem::take(&mut self.documents);
        for d in &mut documents {
            d.scene = self.scene_of(d.document);
            if let Some((_, title)) = open.iter().find(|(u, _)| *u == d.document) {
                d.title = title.clone();
            }
        }
        self.documents = documents;
        self.load_fonts();
        self.zones_stale = self
            .engine
            .project()
            .and_then(|p| p.pcb())
            .is_some_and(|pcb| {
                let fills = self.engine.project().and_then(|p| p.zone_fills(pcb.uuid));
                reda_engine::pcb::zone::status(pcb, fills).needs_refill
            });
        self.change_list = self.engine.project().and_then(reda_engine::sync::plan);
        self.edits += 1;
        self.mark_stale_runs();
        self.update_markers();
    }

    /// Keeps the clipboard from a copy or cut, cancels what is in flight when the Tool
    /// changes, selects what a paste or duplicate made and drops what a delete or cut
    /// removed from the selection.
    pub(super) fn after_edit(&mut self, id: &str, out: &Value) {
        match id {
            "schematic.copy" | "pcb.copy" => self.clipboard = Some(out.clone()),
            "schematic.cut" => self.clipboard = Some(out["result"].clone()),
            // Escape (and any Tool change) puts back what the canvases have in flight.
            "tool.activate" if out["run"] == "ended" => {
                if let Some(sheet) = self.active_sheet() {
                    for (id, params) in crate::schematic_editor::end_run(self, sheet) {
                        let _ = self.run_reporting(&id, params);
                    }
                }
            }
            "tool.activate" | "tool.rotate_about" => {
                if out["run"] == "suspended"
                    && let Some(sheet) = self.active_sheet()
                {
                    crate::schematic_editor::suspend(self, sheet);
                }
                selection::picking::cancel(self);
                let documents: Vec<Uuid> = self.documents.iter().map(|t| t.document).collect();
                for document in documents {
                    crate::schematic_editor::cancel(self, document);
                    crate::element_editor::cancel(self, document);
                }
                self.pcb_cancel();
                self.with_router(|r, shell| r.cancel(shell));
                crate::placement::refresh(self);
                let tool = self.tool();
                self.pcb_draw.start(tool);
                if tool == Tool::Swap {
                    self.swap_selected_pair();
                }
                if out["run"] == "resumed"
                    && let Some(sheet) = self.active_sheet()
                {
                    crate::schematic_editor::resume(self, sheet);
                }
            }
            // The Symbol editor's Pin Tool turns the next Pin, and its polygon takes back
            // a corner or closes.
            "tool.rotate" | "tool.undo_segment" | "tool.finish"
                if crate::element_editor::active_symbol(self).is_some() =>
            {
                let symbol = crate::element_editor::active_symbol(self).expect("checked");
                match id {
                    "tool.rotate" => crate::symbol_editor::rotate(self, symbol),
                    "tool.undo_segment" => crate::symbol_editor::undo_segment(self, symbol),
                    _ => {
                        if let Some((id, params)) = crate::symbol_editor::finish(self, symbol) {
                            let _ = self.run_reporting(&id, params);
                        }
                    }
                }
            }
            // The Footprint editor's polygon closes on Enter.
            "tool.finish" if crate::element_editor::active_footprint(self).is_some() => {
                let footprint = crate::element_editor::active_footprint(self).expect("checked");
                if let Some((id, params)) = crate::footprint_editor::finish(self, footprint) {
                    let _ = self.run_reporting(&id, params);
                }
            }
            "tool.cycle_corner" if self.tool() == Tool::Route => {
                self.with_router(|r, shell| r.cycle_corner(shell))
            }
            "tool.cycle_corner"
            | "tool.undo_segment"
            | "tool.cycle_wire_mode"
            | "tool.insert_vertex"
            | "tool.finish"
                if self.tool() == Tool::Wire && self.active_sheet().is_some() =>
            {
                let sheet = self.active_sheet().expect("checked above");
                let commands = match id {
                    "tool.cycle_corner" => {
                        crate::schematic_editor::corner(self, sheet);
                        Vec::new()
                    }
                    "tool.undo_segment" => {
                        crate::schematic_editor::remove_last(self, sheet);
                        Vec::new()
                    }
                    "tool.cycle_wire_mode" => {
                        crate::schematic_editor::restate_wiring(self, sheet);
                        Vec::new()
                    }
                    "tool.insert_vertex" => crate::schematic_editor::insert_vertex(self, sheet),
                    _ => crate::schematic_editor::fix_corner(self, sheet),
                };
                for (id, params) in commands {
                    let _ = self.run_reporting(&id, params);
                }
            }
            "tool.cycle_cutting_length" => {
                let (id, params) = crate::schematic_editor::cycle_cutting_length(self);
                let _ = self.run_reporting(&id, params);
            }
            // With no segment held, Insert resets the HUD's delta, as it does off a sheet.
            "tool.insert_vertex" if self.tool() == Tool::Select => {
                let held = self
                    .active_sheet()
                    .is_some_and(|sheet| crate::schematic_editor::insert_held_vertex(self, sheet));
                if !held && let Some(sheet) = self.active_sheet() {
                    crate::hud::reset(self, sheet);
                }
            }
            "view.hud.reset_delta" => crate::hud::apply_resets(self),
            // A custom grid's range at or above its step is kept, with this advice.
            "view.grid.custom" => {
                if let Some(warning) = out["warning"].as_str() {
                    self.set_note(warning.to_owned());
                }
            }
            "tool.edit_properties"
                if matches!(self.tool(), Tool::NetLabel | Tool::Port | Tool::PowerPort) =>
            {
                if let Some(sheet) = self.active_sheet() {
                    crate::schematic_editor::pause_placing(self, sheet);
                }
            }
            "schematic.edit_in_place" => {
                if let Some(sheet) = self.active_sheet() {
                    crate::schematic_editor::edit_selected(self, sheet);
                }
            }
            "tool.edit_properties" if self.tool() == Tool::Place => {
                let _ = self.run_reporting("panel.show:properties", json!({}));
            }
            "tool.undo_segment" | "tool.finish" if self.sheet_drawing().is_some() => {
                let sheet = self.sheet_drawing().expect("checked above");
                if id == "tool.finish" {
                    for (id, params) in crate::schematic_editor::finish(self, sheet) {
                        let _ = self.run_reporting(&id, params);
                    }
                } else {
                    crate::schematic_editor::undo_point(self, sheet);
                }
            }
            "tool.undo_segment" if self.tool() == Tool::Route => {
                self.with_router(|r, shell| r.undo_segment(shell))
            }
            "pcb.route.add_via" => self.with_router(|r, shell| r.add_via(shell)),
            "pcb.route.via_end" => self.with_router(|r, shell| r.via_end(shell)),
            "pcb.route.end" => self.with_router(|r, shell| r.end(shell)),
            "pcb.route.mode" => {
                if let Ok(mode) = serde_json::from_value(out["mode"].clone()) {
                    self.with_router(|r, shell| r.change_mode(shell, mode));
                }
            }
            "pcb.route.chooser" => {
                if let Ok(kind) = serde_json::from_value(out["kind"].clone()) {
                    self.with_router(|r, shell| r.open_chooser(shell, kind));
                }
            }
            "pcb.route.via_size" => {
                if let (Some(d), Some(h)) = (out["diameter"].as_i64(), out["hole"].as_i64()) {
                    self.with_router(|r, shell| r.set_via_size(shell, Nm(d), Nm(h)));
                }
            }
            "pcb.route.finish" => self.with_router(|r, shell| r.finish(shell)),
            "pcb.drag.mode" => {
                if let Ok(mode) = serde_json::from_value(out["mode"].clone()) {
                    self.with_pcb_tool(|t, shell| t.drag_mode(shell, mode));
                }
            }
            // Unwinding hands the selection on to the neighbour, if there is one.
            "pcb.copper.unwind" => {
                let objects: Vec<Value> = out["result"]["next"]
                    .as_str()
                    .map(|n| vec![json!(n)])
                    .unwrap_or_default();
                let _ = self
                    .engine
                    .run("selection.set", json!({ "objects": objects }));
            }
            "pcb.route.width" => {
                let width = out["width"].as_i64().map(Nm);
                let step = match out["step"].as_str() {
                    Some("next") => Some(1),
                    Some("previous") => Some(-1),
                    _ => None,
                };
                self.with_router(|r, shell| r.set_width(shell, width, step));
            }
            "tool.undo_segment" if self.drawing() => self.pcb_draw.undo_segment(),
            "tool.finish" if self.drawing() => self.with_draw(|draw, shell| draw.finish(shell)),
            // What the placement Tool carries turned in the Command; draw it anew.
            "tool.place" | "tool.place.set" => crate::placement::refresh(self),
            // A Part being placed takes Space, X, Y and L; the selection stays as it is.
            "tool.rotate" | "tool.mirror_x" | "tool.mirror_y" | "tool.flip_side"
                if crate::placement::placing(self).is_some() =>
            {
                crate::placement::refresh(self);
            }
            "tool.rotate" if self.board_is_active() => self.pcb_rotate(),
            "tool.rotate" | "tool.mirror_x" | "tool.mirror_y" => {
                if let Some(sheet) = self.active_sheet() {
                    match id {
                        "tool.mirror_x" => self.sheet_turn(sheet, Turn::MirrorX),
                        "tool.mirror_y" => self.sheet_turn(sheet, Turn::MirrorY),
                        _ => self.sheet_turn(sheet, Turn::Rotate),
                    }
                }
            }
            "tool.flip_side" if self.board_is_active() => self.pcb_flip(),
            "tool.move_board_shape" => self.pcb_move_board_shape(false),
            "tool.move_board" => self.pcb_move_board_shape(true),
            id if id.starts_with("pcb.place.") => pcb::placing::after_run(self, id, out),
            "pcb.delete.confirm" => pcb::delete::after_confirm(self, out),
            "schematic.port.go_to_partner" => crate::port_partner::after_go(self, out),
            "tool.cycle_corner" => {
                if let Some(sheet) = self.active_sheet() {
                    crate::schematic_editor::flip_corners(self, sheet);
                }
            }
            "schematic.nudge" => {
                let objects: Vec<Uuid> =
                    serde_json::from_value(out["objects"].clone()).unwrap_or_default();
                let direction = serde_json::from_value(out["direction"].clone());
                if let (Some(sheet), Ok(direction)) = (self.active_sheet(), direction) {
                    let steps = out["steps"].as_i64().unwrap_or(1);
                    crate::schematic_editor::nudge(self, sheet, objects, direction, steps);
                }
            }
            "tool.copy" | "tool.duplicate" => {
                let objects: Vec<Uuid> =
                    serde_json::from_value(out["objects"].clone()).unwrap_or_default();
                if id == "tool.copy" {
                    crate::schematic_editor::copy(self, objects, out["cut"] == true);
                } else {
                    crate::schematic_editor::duplicate(self, objects);
                }
            }
            "tool.paste" => {
                if let Some(document) = out["document"].as_str().and_then(|s| s.parse().ok()) {
                    let clipboard = out["clipboard"].clone();
                    crate::schematic_editor::paste(self, document, clipboard);
                }
            }
            "schematic.copy_as_text" => {
                self.copied_text = out["text"].as_str().map(str::to_owned);
            }
            "schematic.find.step" => {
                crate::panels::find::step(self, out["direction"] == "next");
            }
            "schematic.paste" | "schematic.duplicate" | "pcb.paste" => {
                let objects = out["result"]["objects"].clone();
                let _ = self
                    .engine
                    .run("selection.set", json!({ "objects": objects }));
            }
            _ => {}
        }
        let Some(project) = self.engine.elements() else {
            return;
        };
        let gone: Vec<Uuid> = self
            .selection()
            .objects
            .iter()
            .copied()
            .filter(|o| {
                project.object(*o).is_none() && !crate::element_editor::has_part(&self.engine, *o)
            })
            .collect();
        // A deleted segment, or one its Wire no longer has, is no longer selected.
        let segments: Vec<_> = (self.selection().segments.iter().copied())
            .filter(|s| {
                id != "schematic.delete"
                    && project.object(s.wire).is_some_and(|o| match o {
                        reda_engine::model::Object::Wire(w) => {
                            s.index + 1 < w.object.vertices.len()
                        }
                        _ => false,
                    })
            })
            .collect();
        if !gone.is_empty()
            && let Some(selection) = self.engine.view_mut().get_mut::<Selection>()
        {
            selection.objects.retain(|o| !gone.contains(o));
        }
        if segments != self.selection().segments
            && let Some(selection) = self.engine.view_mut().get_mut::<Selection>()
        {
            selection.segments = segments;
        }
        let unlit: Vec<Uuid> = match self.engine.project() {
            Some(project) => self
                .highlight()
                .objects
                .iter()
                .copied()
                .filter(|o| project.object(*o).is_none())
                .collect(),
            None => Vec::new(),
        };
        if !unlit.is_empty()
            && let Some(highlight) = self.engine.view_mut().get_mut::<Highlight>()
        {
            highlight.objects.retain(|o| !unlit.contains(o));
            if highlight.objects.is_empty() {
                highlight.net = None;
            }
        }
    }

    /// Drops the moves and Wire runs put down on every canvas, once their Command has run.
    pub(super) fn end_committed_moves(&mut self) {
        for tab in &mut self.documents {
            crate::schematic_editor::end_commit(tab);
        }
    }

    /// The active sheet while its Line or Rect Tool is active.
    pub(super) fn sheet_drawing(&self) -> Option<Uuid> {
        matches!(self.tool(), Tool::Line | Tool::Rect)
            .then(|| self.active_sheet())
            .flatten()
    }

    /// The active Document when it is a Schematic Sheet.
    pub fn active_sheet(&self) -> Option<Uuid> {
        let document = self.active_document()?.document;
        self.engine.project()?.schematic_sheet(document)?;
        Some(document)
    }

    /// Space, X or Y on a sheet: turns what its canvas carries, else the selection.
    pub(super) fn sheet_turn(&mut self, document: Uuid, how: Turn) {
        if crate::schematic_editor::turn(self, document, how) {
            return;
        }
        let objects = self.selection().objects.clone();
        if objects.is_empty() {
            return;
        }
        let _ = match how {
            Turn::Rotate => self.run_reporting("schematic.rotate", json!({ "objects": objects })),
            Turn::MirrorX | Turn::MirrorY => {
                let axis = if how == Turn::MirrorX { "x" } else { "y" };
                self.run_reporting(
                    "schematic.mirror",
                    json!({ "objects": objects, "axis": axis }),
                )
            }
        };
    }

    /// Hands every canvas the [`Selection`]'s objects and the highlighted ones (see
    /// [`cross_probe::marks`]) to mark, with a new generation when they changed.
    pub(super) fn sync_selection(&mut self) {
        self.mirror_selection();
        let switches = self.cross_probe_switches();
        let dim = self.preferences().cross_select.dim;
        let (dim_strength, mask_strength) = self.highlight_strengths();
        let strengths = (dim_strength as f32, mask_strength as f32);
        let key = (
            self.selection().clone(),
            self.highlight().clone(),
            self.edits,
            switches,
            dim,
        );
        if self.probed.as_ref() != Some(&key) {
            let highlighted = match self.engine.project() {
                Some(project) => cross_probe::marks(project, &key.0, &key.1, switches, dim),
                None => key.1.objects.clone(),
            };
            // Cross-selected objects draw selected too.
            let mut objects = key.0.canvas_ids();
            objects.extend(key.0.mirrored.iter().copied());
            if self.marked.objects[..] != objects[..]
                || self.marked.highlighted[..] != highlighted[..]
                || self.marked.mask != key.1.mask
            {
                self.generation += 1;
                self.marked = SelectionHandle {
                    generation: self.generation,
                    objects: Arc::from(objects),
                    highlighted: Arc::from(highlighted),
                    mask: key.1.mask,
                    strengths,
                };
            }
            self.probed = Some(key);
        }
        if self.marked.strengths != strengths {
            self.generation += 1;
            self.marked.generation = self.generation;
            self.marked.strengths = strengths;
        }
        let grids = self
            .engine
            .view()
            .get::<Grids>()
            .cloned()
            .unwrap_or_default();
        let project = self.engine.elements();
        for tab in &mut self.documents {
            if tab.selection.generation != self.marked.generation {
                tab.selection = self.marked.clone();
            }
            tab.grid = grids.of(project, tab.document);
            tab.grid_visible = grids.visible(project, tab.document);
            tab.grid_origin = grid::origin(project, tab.document);
            if tab.palette.generation != self.palette.generation {
                tab.palette = self.palette.clone();
            }
        }
    }

    /// Hands the PCB tab the board's Layer state after a Command changed it, and redraws
    /// the board with it.
    pub(super) fn sync_board_view(&mut self) {
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()).map(|p| p.uuid) else {
            return;
        };
        let layers = self.board_view().layers.clone();
        if self.document_tab(pcb).is_some_and(|t| t.layers != layers) {
            let scene = self.scene_of(pcb);
            let tab = self.document_tab_mut(pcb).expect("checked above");
            tab.layers = layers;
            tab.scene = scene;
        }
    }

    pub(super) fn scene_of(&mut self, document: Uuid) -> SceneHandle {
        self.generation += 1;
        if let Some((scene, layers)) = crate::element_editor::scene(&self.engine, document) {
            return SceneHandle {
                generation: self.generation,
                scene: Arc::new(scene),
                layers,
            };
        }
        let handle = match self.engine.project() {
            Some(project) => self.build_scene(project, document),
            None => SceneHandle {
                generation: 0,
                scene: Arc::default(),
                layers: None,
            },
        };
        SceneHandle {
            generation: self.generation,
            ..handle
        }
    }

    /// `document`'s Scene drawn from `project` (not necessarily the open one).
    pub(super) fn scene_from(&mut self, project: &Project, document: Uuid) -> SceneHandle {
        self.generation += 1;
        SceneHandle {
            generation: self.generation,
            ..self.build_scene(project, document)
        }
    }

    /// The Scene of `document` in `project`, filtered and composited as the board view
    /// asks for a PCB. Its generation is left for the caller to set.
    pub(super) fn build_scene(&self, project: &Project, document: Uuid) -> SceneHandle {
        let style = reda_engine::schematic_style::schematic_style(
            project,
            self.preferences.values(),
            reda_engine::schematic_style::Drawn::Canvas,
        );
        let board = self.board_view();
        let mut scene = if project.pcb_document(document).is_some() {
            let style = board.style(self.preferences.values());
            reda_engine::board_scene(project, document, self.engine.fonts(), &style)
        } else {
            reda_engine::document_scene(project, document, self.engine.fonts(), &style)
        }
        .unwrap_or_default();
        let layers = project.pcb_document(document).map(|pcb| {
            board.filter_scene(project, &mut scene);
            board
                .composites(pcb, self.preferences.values().display.inactive_layer_mode)
                .into()
        });
        SceneHandle {
            generation: 0,
            scene: Arc::new(scene),
            layers,
        }
    }

    /// Draws `document` from `project` instead of the open Project: a preview of an edit
    /// in flight. The next refresh (or [`Shell::redraw`]) draws the real one again.
    pub fn show_preview(&mut self, document: Uuid, project: &Project) {
        let scene = self.scene_from(project, document);
        if let Some(tab) = self.document_tab_mut(document) {
            tab.scene = scene;
        }
    }

    /// Draws `document` from the open Project again, as after a dropped preview.
    pub fn redraw(&mut self, document: Uuid) {
        let scene = self.scene_of(document);
        if let Some(tab) = self.document_tab_mut(document) {
            tab.scene = scene;
        }
    }
}
