//! Show on board: a settings window's row shows what it affects on the board. The main
//! window comes forward on the PCB Document with those objects highlighted, the canvas
//! becomes view-only (it pans and zooms, but no Tool or edit key acts), and a banner at
//! the top centre, in the Active Bar's place, gives the count and **Back to Board
//! Setup**, which raises the settings window again. The window keeps its pending edits
//! throughout.
//!
//! The Change List's **Show** works the same way, with **Back to Change List**, but a
//! row can have objects on both sides of Sync: it opens the board when the row has
//! something there and the sheet otherwise, and the banner switches between the two.

use reda_engine::model::Uuid;
use serde_json::json;

use crate::layout::SettingsWindow;
use crate::shell::Shell;

/// While the board (or a sheet) is shown for a settings window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShowOnBoard {
    /// How many objects are highlighted, while something is shown.
    pub count: Option<usize>,
    /// The main window should come forward (the app takes this each frame).
    raise_main: bool,
    /// Back was pressed: the app raises this settings window.
    back: Option<SettingsWindow>,
    /// The settings window that showed it: Board Setup (the default) or the Change List.
    from: Option<SettingsWindow>,
    /// The Document shown, view-only.
    document: Option<Uuid>,
    /// The Change List's two sides, and which one is shown.
    sides: Option<(Sides, Side)>,
}

/// What a row shows on the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// A rule record, by UUID.
    Rule(Uuid),
    /// Every record of a Net Class.
    NetClass(Uuid),
}

/// A side of Sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Board,
    Sheet,
}

/// What a Change List row has on each side: the board's objects, and the sheet that shows
/// the schematic's (with every object, on whatever sheet).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sides {
    pub board: Vec<Uuid>,
    pub sheet: Option<(Uuid, Vec<Uuid>)>,
}

impl Sides {
    fn has(&self, side: Side) -> bool {
        match side {
            Side::Board => !self.board.is_empty(),
            Side::Sheet => self.sheet.as_ref().is_some_and(|(_, o)| !o.is_empty()),
        }
    }
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

    /// The settings window that Back returns to, while something is shown.
    pub fn shown_for(&self) -> Option<SettingsWindow> {
        let state = self.engine.view().get::<ShowOnBoard>()?;
        state.count?;
        Some(state.from.unwrap_or(SettingsWindow::BoardSetup))
    }

    /// Which side of Sync the Change List's Show is on, while it shows a row.
    pub fn shown_side(&self) -> Option<Side> {
        let state = self.engine.view().get::<ShowOnBoard>()?;
        state.count?;
        state.sides.as_ref().map(|(_, side)| *side)
    }

    /// Whether the shown Change List row has objects on the other side too.
    pub fn can_switch_shown_side(&self) -> bool {
        let Some(state) = self.engine.view().get::<ShowOnBoard>() else {
            return false;
        };
        match (&state.sides, state.count) {
            (Some((sides, side)), Some(_)) => sides.has(other(*side)),
            _ => false,
        }
    }

    /// Whether `document`'s canvas is view-only: the Document shown, while it is shown.
    pub fn view_only(&self, document: Uuid) -> bool {
        self.engine
            .view()
            .get::<ShowOnBoard>()
            .is_some_and(|s| s.count.is_some() && s.document == Some(document))
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
        if self.show_document(pcb, &objects) {
            let state = self.show_state();
            state.count = Some(count);
            state.from = Some(SettingsWindow::BoardSetup);
            state.sides = None;
        }
    }

    /// The Change List's Show: the board side when the row has objects there, the sheet
    /// otherwise; nothing when it has neither.
    pub fn show_change(&mut self, sides: Sides) {
        let side = if sides.has(Side::Board) {
            Side::Board
        } else if sides.has(Side::Sheet) {
            Side::Sheet
        } else {
            return;
        };
        self.show_side(sides, side);
    }

    /// Switches the shown Change List row to its other side.
    pub fn switch_shown_side(&mut self) {
        if !self.can_switch_shown_side() {
            return;
        }
        let state = self.show_state();
        let Some((sides, side)) = state.sides.clone() else {
            return;
        };
        self.show_side(sides, other(side));
    }

    fn show_side(&mut self, sides: Sides, side: Side) {
        let (document, objects) = match side {
            Side::Board => {
                let Some(pcb) = self.engine.project().and_then(|p| p.pcb()) else {
                    return;
                };
                (pcb.uuid, sides.board.clone())
            }
            Side::Sheet => match &sides.sheet {
                Some((sheet, objects)) => (*sheet, objects.clone()),
                None => return,
            },
        };
        if self.show_document(document, &objects) {
            let state = self.show_state();
            state.count = Some(objects.len());
            state.from = Some(SettingsWindow::ChangeList);
            state.sides = Some((sides, side));
        }
    }

    /// Opens `document` with `objects` highlighted and framed, view-only, and asks for the
    /// main window to come forward. False when the Document could not open.
    fn show_document(&mut self, document: Uuid, objects: &[Uuid]) -> bool {
        if self
            .run_reporting("document.open", json!({ "document": document }))
            .is_err()
        {
            return false;
        }
        let _ = self.run("view.highlight", json!({ "objects": objects }));
        if let Some(tab) = self.document_tab_mut(document)
            && let Some(frame) = crate::reveal::frame(&tab.scene.scene, objects)
        {
            tab.zoom_to = Some(frame);
        }
        let state = self.show_state();
        state.document = Some(document);
        state.raise_main = true;
        true
    }

    /// Back to the settings window (Board Setup or the Change List): the highlight clears,
    /// the canvas takes edits again and the settings window is raised.
    pub fn back_from_show(&mut self) {
        let Some(window) = self.shown_for() else {
            return;
        };
        let _ = self.run("view.highlight", json!({}));
        let state = self.show_state();
        *state = ShowOnBoard {
            back: Some(window),
            ..ShowOnBoard::default()
        };
    }

    /// Whether the main window should come forward now; asking clears it.
    pub fn take_raise_main(&mut self) -> bool {
        std::mem::take(&mut self.show_state().raise_main)
    }

    /// The settings window to raise, when Back was pressed since this was last asked.
    pub fn take_back(&mut self) -> Option<SettingsWindow> {
        self.show_state().back.take()
    }
}

fn other(side: Side) -> Side {
    match side {
        Side::Board => Side::Sheet,
        Side::Sheet => Side::Board,
    }
}

/// What the banner was asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerAction {
    Back,
    Switch,
}

/// The banner at the top centre of the canvas `canvas`, in the Active Bar's place and
/// frame, filled with the tab bar's blue: the count, Back to the settings window and, for
/// the Change List, the switch to the other side.
pub fn banner(
    ui: &mut egui::Ui,
    id: egui::Id,
    canvas: egui::Rect,
    shell: &Shell,
) -> Option<BannerAction> {
    let count = shell.shown_on_board()?;
    let from = shell.shown_for()?;
    let side = shell.shown_side();
    let switch = shell.can_switch_shown_side();
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
                        let text = match from {
                            SettingsWindow::ChangeList => {
                                crate::tr!("show-change-count", count = count)
                            }
                            _ => crate::tr!("show-on-board-count", count = count),
                        };
                        let label = egui::RichText::new(text).color(crate::theme::tab_text(true));
                        ui.add_space(6.0);
                        ui.label(label);
                        ui.add_space(6.0);
                        let mut action = None;
                        if let Some(side) = side {
                            let text = match side {
                                Side::Board => crate::tr!("show-change-on-sheet"),
                                Side::Sheet => crate::tr!("show-change-on-board"),
                            };
                            if ui.add_enabled(switch, egui::Button::new(text)).clicked() {
                                action = Some(BannerAction::Switch);
                            }
                        }
                        let back = match from {
                            SettingsWindow::ChangeList => crate::tr!("show-change-back"),
                            _ => crate::tr!("show-on-board-back"),
                        };
                        if ui.button(back).clicked() {
                            action = Some(BannerAction::Back);
                        }
                        action
                    })
                    .inner
                })
                .inner
        })
        .inner
}
