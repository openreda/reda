//! Applies Tool Commands to the active canvas after the registry has run them.

use serde::Deserialize;

use super::*;
use crate::schematic_editor;
use crate::tools::RunChange;

impl Shell {
    pub(super) fn after_tool(&mut self, command: ToolCommand, out: &Value) {
        use ToolCommand::*;

        let mut edits = Vec::new();
        match command {
            Activate | RotateAbout => {
                let run = out.get("run").and_then(|v| RunChange::deserialize(v).ok());
                self.after_tool_change(run);
            }
            Rotate => self.turn_tool(Turn::Rotate),
            MirrorX => self.turn_tool(Turn::MirrorX),
            MirrorY => self.turn_tool(Turn::MirrorY),
            FlipSide => {
                if crate::placement::placing(self).is_some() {
                    crate::placement::refresh(self);
                } else if self.board_is_active() {
                    self.pcb_flip();
                }
            }
            MoveBoardShape => self.pcb_move_board_shape(false),
            MoveBoard => self.pcb_move_board_shape(true),
            CycleCorner => {
                if self.tool() == Tool::Route {
                    self.with_router(|r, shell| r.cycle_corner(shell));
                } else if let Some(sheet) = self.active_sheet() {
                    if self.tool() == Tool::Wire {
                        schematic_editor::corner(self, sheet);
                    } else {
                        schematic_editor::flip_corners(self, sheet);
                    }
                }
            }
            UndoSegment => {
                if let Some(symbol) = crate::element_editor::active_symbol(self) {
                    crate::symbol_editor::undo_segment(self, symbol);
                } else if let Some(sheet) = self.wire_sheet() {
                    schematic_editor::remove_last(self, sheet);
                } else if let Some(sheet) = self.sheet_drawing() {
                    schematic_editor::undo_point(self, sheet);
                } else if self.tool() == Tool::Route {
                    self.with_router(|r, shell| r.undo_segment(shell));
                } else if self.drawing() {
                    self.pcb_draw.undo_segment();
                }
            }
            Finish => {
                if let Some(symbol) = crate::element_editor::active_symbol(self) {
                    edits.extend(crate::symbol_editor::finish(self, symbol));
                } else if let Some(footprint) = crate::element_editor::active_footprint(self) {
                    edits.extend(crate::footprint_editor::finish(self, footprint));
                } else if let Some(sheet) = self.wire_sheet() {
                    edits = schematic_editor::fix_corner(self, sheet);
                } else if let Some(sheet) = self.sheet_drawing() {
                    edits = schematic_editor::finish(self, sheet);
                } else if self.drawing() {
                    self.with_draw(|draw, shell| draw.finish(shell));
                }
            }
            CycleWireMode => {
                if let Some(sheet) = self.wire_sheet() {
                    schematic_editor::restate_wiring(self, sheet);
                }
            }
            InsertVertex => {
                if let Some(sheet) = self.wire_sheet() {
                    edits = schematic_editor::insert_vertex(self, sheet);
                } else if self.tool() == Tool::Select
                    && let Some(sheet) = self.active_sheet()
                    && !schematic_editor::insert_held_vertex(self, sheet)
                {
                    crate::delta::reset(self, sheet);
                }
            }
            CycleCuttingLength => edits.push(schematic_editor::cycle_cutting_length(self)),
            EditProperties => match self.tool() {
                Tool::NetLabel | Tool::Port | Tool::PowerPort => {
                    if let Some(sheet) = self.active_sheet() {
                        schematic_editor::pause_placing(self, sheet);
                    }
                }
                Tool::Place => {
                    let _ = self.run_reporting("panel.show:properties", json!({}));
                }
                tool if crate::object_defaults::OPTION_TOOLS.contains(&tool) => {
                    let _ = self.run_reporting("panel.show:properties", json!({}));
                }
                _ => {}
            },
            Place | SetPlacement => crate::placement::refresh(self),
            Copy | Duplicate => {
                let objects = serde_json::from_value(out["objects"].clone()).unwrap_or_default();
                if command == Copy {
                    schematic_editor::copy(self, objects, out["cut"] == true);
                } else {
                    schematic_editor::duplicate(self, objects);
                }
            }
            Paste => {
                if let Some(document) = out["document"].as_str().and_then(|s| s.parse().ok()) {
                    schematic_editor::paste(self, document, out["clipboard"].clone());
                }
            }
            // These Commands carry out their entire action in the registry.
            SetAutoWire | SetOption | PlaceUnit => {}
        }
        for (id, params) in edits {
            let _ = self.run_reporting(&id, params);
        }
    }

    fn wire_sheet(&self) -> Option<Uuid> {
        (self.tool() == Tool::Wire)
            .then(|| self.active_sheet())
            .flatten()
    }

    /// Escape ends the Wire run, or changes Tools and restores a suspended run.
    fn after_tool_change(&mut self, run: Option<RunChange>) {
        if run == Some(RunChange::Ended) {
            if let Some(sheet) = self.active_sheet() {
                for (id, params) in schematic_editor::end_run(self, sheet) {
                    let _ = self.run_reporting(&id, params);
                }
            }
            return;
        }
        if run == Some(RunChange::Suspended)
            && let Some(sheet) = self.active_sheet()
        {
            schematic_editor::suspend(self, sheet);
        }
        selection::picking::cancel(self);
        let documents: Vec<Uuid> = self.documents.iter().map(|t| t.document).collect();
        for document in documents {
            schematic_editor::cancel(self, document);
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
        if tool == Tool::Dimension && self.board_is_active() {
            self.activate_dimensions_layer();
        }
        if run == Some(RunChange::Resumed)
            && let Some(sheet) = self.active_sheet()
        {
            schematic_editor::resume(self, sheet);
        }
    }

    /// The Dimension Tool starts on the first Dimensions Layer in Layer order, when the
    /// board has one; the user can then switch Layers as with any Tool.
    fn activate_dimensions_layer(&mut self) {
        let Some(layer) = self
            .engine()
            .project()
            .and_then(|p| p.pcb())
            .and_then(|pcb| {
                pcb.layers()
                    .first_with_purpose(reda_engine::model::Purpose::Dimensions)
            })
            .map(|l| l.uuid)
        else {
            return;
        };
        let _ = self.run_reporting("pcb.layer.activate", json!({ "layer": layer }));
    }

    fn turn_tool(&mut self, how: Turn) {
        if how == Turn::Rotate
            && let Some(symbol) = crate::element_editor::active_symbol(self)
        {
            crate::symbol_editor::rotate(self, symbol);
        } else if how == Turn::Rotate && self.tool() == Tool::Guide && self.board_is_active() {
            self.pcb_draw.cycle_guide_angle();
        } else if crate::placement::placing(self).is_some() {
            // The Command has already turned the Part being placed.
            crate::placement::refresh(self);
        } else if how == Turn::Rotate && self.board_is_active() {
            self.pcb_rotate();
        } else if let Some(sheet) = self.active_sheet() {
            self.sheet_turn(sheet, how);
        }
    }
}
