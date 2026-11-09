//! Show on board: a settings window's row shows what it affects on the board. The main
//! window comes forward on the PCB Document with those objects highlighted, the canvas
//! becomes view-only (it pans and zooms, but no Tool or edit key acts), and a banner at
//! the top centre, in the Active Bar's place, gives the count and **Back to Board
//! Setup**, which raises the settings window again. The window keeps its pending edits
//! throughout.

use reda_engine::model::Uuid;
use serde_json::json;

use crate::shell::Shell;

/// While the board is shown for a settings window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShowOnBoard {
    /// How many objects are highlighted, while the board is shown.
    pub count: Option<usize>,
    /// The main window should come forward (the app takes this each frame).
    raise_main: bool,
    /// Back to Board Setup was pressed: the app raises the settings window.
    back: bool,
}

/// What a row shows on the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// A rule record, by UUID.
    Rule(Uuid),
    /// Every record of a Net Class.
    NetClass(Uuid),
}

impl Shell {
    fn show_state(&mut self) -> &mut ShowOnBoard {
        if self.engine.view().get::<ShowOnBoard>().is_none() {
            self.engine.view_mut().set(ShowOnBoard::default());
        }
        self.engine
            .view_mut()
            .get_mut::<ShowOnBoard>()
            .expect("just set")
    }

    /// While the board is shown for a settings window: how many objects it highlights.
    pub fn shown_on_board(&self) -> Option<usize> {
        self.engine.view().get::<ShowOnBoard>()?.count
    }

    /// Whether `document`'s canvas is view-only: the board, while it is shown.
    pub fn view_only(&self, document: Uuid) -> bool {
        self.shown_on_board().is_some()
            && self
                .engine
                .project()
                .and_then(|p| p.pcb())
                .is_some_and(|pcb| pcb.uuid == document)
    }

    /// Shows on the board what `rows` affect (`pcb.rules.affected`): opens the PCB
    /// Document, highlights the copper and the Footprints of the Pads, frames them and
    /// makes the canvas view-only.
    pub fn show_on_board(&mut self, rows: &[Shown]) {
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()).map(|p| p.uuid) else {
            return;
        };
        let mut objects: Vec<Uuid> = Vec::new();
        let mut count = 0;
        for row in rows {
            let params = match row {
                Shown::Rule(rule) => json!({ "rule": rule }),
                Shown::NetClass(class) => json!({ "net_class": class }),
            };
            let Ok(out) = self.run("pcb.rules.affected", params) else {
                continue;
            };
            // A query's output is its result.
            let result = out.get("result").unwrap_or(&out);
            let uuid = |v: &serde_json::Value| v.as_str().and_then(|s| s.parse::<Uuid>().ok());
            for object in result["objects"].as_array().into_iter().flatten() {
                if let Some(u) = uuid(object)
                    && !objects.contains(&u)
                {
                    objects.push(u);
                    count += 1;
                }
            }
            for pad in result["pads"].as_array().into_iter().flatten() {
                count += 1;
                if let Some(u) = uuid(&pad["footprint"])
                    && !objects.contains(&u)
                {
                    objects.push(u);
                }
            }
        }
        if self
            .run_reporting("document.open", json!({ "document": pcb }))
            .is_err()
        {
            return;
        }
        let _ = self.run("view.highlight", json!({ "objects": objects }));
        if let Some(tab) = self.document_tab_mut(pcb)
            && let Some(frame) = crate::reveal::frame(&tab.scene.scene, &objects)
        {
            tab.zoom_to = Some(frame);
        }
        let state = self.show_state();
        state.count = Some(count);
        state.raise_main = true;
    }

    /// Back to Board Setup: the highlight clears, the canvas takes edits again and the
    /// settings window is raised.
    pub fn back_to_board_setup(&mut self) {
        if self.shown_on_board().is_none() {
            return;
        }
        let _ = self.run("view.highlight", json!({}));
        let state = self.show_state();
        state.count = None;
        state.back = true;
    }

    /// Whether the main window should come forward now; asking clears it.
    pub fn take_raise_main(&mut self) -> bool {
        std::mem::take(&mut self.show_state().raise_main)
    }

    /// Whether Back to Board Setup was pressed since this was last asked.
    pub fn take_back_to_board_setup(&mut self) -> bool {
        std::mem::take(&mut self.show_state().back)
    }
}

/// The banner at the top centre of the canvas `canvas`, in the Active Bar's place and
/// frame, filled with the tab bar's blue: the count and Back to Board Setup. Returns
/// whether Back was pressed.
pub fn banner(ui: &mut egui::Ui, id: egui::Id, canvas: egui::Rect, count: usize) -> bool {
    let fill = crate::theme::tab_bar(ui.ctx().theme(), true);
    egui::Area::new(id.with("show-on-board"))
        .order(egui::Order::Middle)
        .fixed_pos(canvas.center_top() + egui::vec2(0.0, crate::bars::draw::ACTIVE_BAR_TOP))
        .pivot(egui::Align2::CENTER_TOP)
        .constrain_to(canvas)
        .show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style())
                .fill(fill)
                .inner_margin(egui::Margin::same(crate::bars::draw::ACTIVE_BAR_MARGIN))
                .corner_radius(egui::CornerRadius::same(5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let text = crate::tr!("show-on-board-count", count = count);
                        let label = egui::RichText::new(text).color(crate::theme::tab_text(true));
                        ui.add_space(6.0);
                        ui.label(label);
                        ui.add_space(6.0);
                        ui.button(crate::tr!("show-on-board-back")).clicked()
                    })
                    .inner
                })
                .inner
        })
        .inner
}
