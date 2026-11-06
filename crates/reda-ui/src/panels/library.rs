//! The Library panel: the registered Libraries' Parts, searched and filtered, with a
//! preview of the chosen one. On a sheet it lists Parts that have a Symbol, on the PCB
//! Mechanical Parts. Clicking or dragging a Part starts the placement Tool
//! (`tool.place`); Edit opens the chosen Part in the Part editor, and New… makes a Part,
//! Symbol or Footprint, imports a Part or runs a Footprint generator.
//!
//! The panel reads the Libraries through [`Engine::libraries`](reda_engine::Engine),
//! scanning again only when a Command may have changed them (the Shell's library
//! generation) or on Refresh, and changes nothing itself.

pub(crate) mod preview;

use egui::{ComboBox, Id, Role, ScrollArea, Sense, TextEdit, Ui, WidgetInfo};
use reda_engine::library::{LibraryIndex, SearchHit, SearchQuery};
use reda_engine::model::{ComponentType, DocumentKind, Part, Uuid};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tr;

/// The drag-and-drop payload of a Part dragged from the panel onto a canvas.
#[derive(Clone, Copy, Debug)]
pub struct Dragged;

/// What the panel asks of the window after the frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Run this Command.
    Run(String, Value),
    /// Call this Command, asking for the params it lacks in a dialog.
    Call(String, Value),
}

/// The panel's state between frames: the filters, the chosen Part and the scanned
/// Libraries.
#[derive(Default)]
pub struct LibraryPanel {
    pub text: String,
    pub library: Option<Uuid>,
    pub component_type: Option<ComponentType>,
    /// The chosen Part.
    pub selected: Option<Uuid>,
    /// The Unit tab shown, by index.
    pub unit: usize,
    /// The alternate Footprint shown, by index.
    pub footprint: usize,
    /// The scan and the library generation it was made at.
    index: Option<(u64, LibraryIndex)>,
}

/// Where Parts from the panel would go now: the active Document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placing {
    Sheet(Uuid),
    Board(Uuid),
    /// No sheet or board is active: Parts are listed but not placed.
    Nowhere,
}

/// Where the active Document takes Parts.
pub fn target(shell: &Shell) -> Placing {
    let Some(document) = shell.active_document().map(|d| d.document) else {
        return Placing::Nowhere;
    };
    let kind = shell
        .engine()
        .project()
        .and_then(|p| p.document(document))
        .map(|d| d.kind);
    match kind {
        Some(DocumentKind::Schematic) => Placing::Sheet(document),
        Some(DocumentKind::Pcb) => Placing::Board(document),
        _ => Placing::Nowhere,
    }
}

impl LibraryPanel {
    /// The scanned Libraries, scanning when the Shell says they may have changed.
    fn index(&mut self, shell: &Shell) -> &LibraryIndex {
        let generation = shell.library_generation();
        if self.index.as_ref().is_none_or(|(g, _)| *g != generation) {
            self.index = Some((generation, shell.engine().libraries()));
        }
        &self.index.as_ref().expect("just scanned").1
    }

    /// Scans the Libraries again on the next frame.
    pub fn rescan(&mut self) {
        self.index = None;
    }

    /// The Parts the panel lists for `target`: the search's hits, only those with a
    /// Symbol on a sheet and only Mechanical Parts on the board.
    pub fn parts(&mut self, shell: &Shell) -> Vec<(String, Part)> {
        let query = SearchQuery {
            text: self.text.clone(),
            library: self.library,
            component_type: self.component_type,
        };
        let target = target(shell);
        self.index(shell)
            .search(&query)
            .into_iter()
            .filter(|hit| fits(hit, target))
            .map(|hit| (hit.library.info.name.clone(), hit.part.clone()))
            .collect()
    }
}

fn fits(hit: &SearchHit<'_>, target: Placing) -> bool {
    match target {
        Placing::Sheet(_) => hit.part.symbol.is_some(),
        Placing::Board(_) => hit.part.component_type == ComponentType::Mechanical,
        Placing::Nowhere => true,
    }
}

/// The `tool.place` that picks `part` up for `target`, with the Unit and Footprint the
/// preview shows.
fn place(
    target: Placing,
    part: &Part,
    unit: Option<String>,
    footprint: Option<Uuid>,
) -> Option<Action> {
    let document = match target {
        Placing::Sheet(d) => d,
        Placing::Board(d) => d,
        Placing::Nowhere => return None,
    };
    let mut params = json!({"document": document, "part": part.uuid});
    if let Some(unit) = unit.filter(|_| matches!(target, Placing::Sheet(_))) {
        params["unit"] = json!(unit);
    }
    if let Some(f) = footprint {
        params["footprint"] = json!(f);
    }
    Some(Action::Run("tool.place".into(), params))
}

/// Draws the panel; returns what was clicked.
pub fn show(ui: &mut Ui, shell: &mut Shell) -> Vec<Action> {
    let mut panel = std::mem::take(shell.library_panel_mut());
    let actions = draw(ui, shell, &mut panel);
    *shell.library_panel_mut() = panel;
    actions
}

fn draw(ui: &mut Ui, shell: &Shell, panel: &mut LibraryPanel) -> Vec<Action> {
    let mut actions = Vec::new();
    let target = target(shell);
    filters(ui, shell, panel);
    let parts = panel.parts(shell);
    if panel
        .selected
        .is_some_and(|s| !parts.iter().any(|(_, p)| p.uuid == s))
    {
        panel.selected = None;
    }
    match target {
        Placing::Sheet(_) => ui.weak(tr!("library-sheet-parts")),
        Placing::Board(_) => ui.weak(tr!("library-board-parts")),
        Placing::Nowhere => ui.weak(tr!("library-no-document")),
    };
    let list_height = (ui.available_height() * 0.45).max(80.0);
    ScrollArea::vertical()
        .id_salt("library-parts")
        .max_height(list_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if parts.is_empty() {
                ui.weak(tr!("library-no-parts"));
            }
            for (_, part) in &parts {
                if let Some(action) = row(ui, panel, target, part) {
                    actions.push(action);
                }
            }
        });
    ui.separator();
    let chosen = panel
        .selected
        .and_then(|s| parts.iter().find(|(_, p)| p.uuid == s))
        .cloned();
    ui.horizontal(|ui| {
        // Edit opens the chosen Part in the Part editor, which links to its Symbol and
        // Footprints; New makes an element, imports a Part or generates a Footprint.
        let part = chosen.as_ref().map(|(_, p)| p.uuid);
        let edit = ui.add_enabled(part.is_some(), egui::Button::new(tr!("library-edit")));
        if edit.clicked()
            && let Some(part) = part
        {
            actions.push(Action::Run(
                "library.element.open".into(),
                json!({ "element": part }),
            ));
        }
        ui.menu_button(tr!("library-new"), |ui| {
            actions.extend(new_menu(ui, shell, panel.library));
        });
        if ui.button(tr!("library-refresh")).clicked() {
            panel.rescan();
        }
    });
    // The chosen Part's previews scroll under the buttons, which stay in view.
    if let Some((library, part)) = &chosen {
        ui.separator();
        ScrollArea::vertical()
            .id_salt("library-details")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                actions.extend(details(ui, panel, target, library, part));
            });
    }
    actions
}

/// The New menu: a new Part, Symbol or Footprint opened in its editor, Import part, and a
/// Footprint generator per family. Each asks for what it lacks in a dialog; `library` (the
/// Library filter) is the one they start in.
fn new_menu(ui: &mut Ui, shell: &Shell, library: Option<Uuid>) -> Option<Action> {
    let with_library = |mut params: Value| {
        if let Some(library) = library {
            params["library"] = json!(library);
        }
        params
    };
    let mut picked = None;
    for (kind, label) in [
        ("part", tr!("library-new-part")),
        ("symbol", tr!("library-new-symbol")),
        ("footprint", tr!("library-new-footprint")),
    ] {
        if ui.button(label).clicked() {
            let params = with_library(json!({ "kind": kind }));
            picked = Some(Action::Call("library.element.new".into(), params));
        }
    }
    ui.separator();
    if ui.button(tr!("library-new-library")).clicked() {
        picked = Some(Action::Call("library.create".into(), json!({})));
    }
    ui.separator();
    if ui.button(tr!("library-import-part")).clicked() {
        picked = Some(Action::Call(
            "library.import_part".into(),
            with_library(json!({})),
        ));
    }
    ui.menu_button(tr!("library-generate"), |ui| {
        for id in reda_engine::library::GENERATORS {
            let title = shell.engine().registry().get(id).map_or_else(
                || id.to_owned(),
                |m| crate::bars::command_title(id, m.title),
            );
            if ui.button(format!("{title}…")).clicked() {
                picked = Some(Action::Call(id.into(), with_library(json!({}))));
                ui.close();
            }
        }
    });
    if picked.is_some() {
        ui.close();
    }
    picked
}

/// The search box and the Library and Component Type filters.
fn filters(ui: &mut Ui, shell: &Shell, panel: &mut LibraryPanel) {
    let search = tr!("library-search");
    let response = ui.add(
        TextEdit::singleline(&mut panel.text)
            .id(Id::NULL.with("library-search"))
            .hint_text(search.clone())
            .desired_width(f32::INFINITY),
    );
    response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, search.clone()));
    let libraries: Vec<(Uuid, String)> = panel
        .index(shell)
        .libraries()
        .map(|l| (l.info.uuid, l.info.name.clone()))
        .collect();
    ui.horizontal(|ui| {
        let all = tr!("library-all-libraries");
        let chosen = panel
            .library
            .and_then(|u| libraries.iter().find(|(l, _)| *l == u))
            .map_or(all.clone(), |(_, n)| n.clone());
        let label = tr!("library-filter-library");
        let response = ComboBox::from_id_salt("library-filter-library")
            .selected_text(chosen)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut panel.library, None, all);
                for (uuid, name) in &libraries {
                    ui.selectable_value(&mut panel.library, Some(*uuid), name);
                }
            })
            .response;
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
        let type_name =
            |t: Option<ComponentType>| t.map_or_else(|| tr!("library-all-types"), type_name);
        let label = tr!("library-filter-type");
        let response = ComboBox::from_id_salt("library-filter-type")
            .selected_text(type_name(panel.component_type))
            .show_ui(ui, |ui| {
                for t in std::iter::once(None).chain(ComponentType::ALL.map(Some)) {
                    ui.selectable_value(&mut panel.component_type, t, type_name(t));
                }
            })
            .response;
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
    });
}

/// One Part's row: a click chooses it and picks it up to place, and so does a drag,
/// which carries it onto a canvas.
fn row(ui: &mut Ui, panel: &mut LibraryPanel, target: Placing, part: &Part) -> Option<Action> {
    let chosen = panel.selected == Some(part.uuid);
    let text = if part.fields.value.is_empty() || part.fields.value == part.name {
        part.name.clone()
    } else {
        format!("{} · {}", part.name, part.fields.value)
    };
    let response = ui
        .selectable_label(chosen, text)
        .interact(Sense::click_and_drag());
    let name = part.name.clone();
    response.widget_info(|| WidgetInfo::selected(Role::ListItem, true, chosen, name.clone()));
    if response.drag_started() {
        response.dnd_set_drag_payload(Dragged);
    }
    if response.clicked() || response.drag_started() {
        if !chosen {
            panel.selected = Some(part.uuid);
            panel.unit = 0;
            panel.footprint = 0;
        }
        return place(
            target,
            part,
            None,
            part.footprints.get(panel.footprint).copied(),
        );
    }
    None
}

/// The chosen Part's previews: the Symbol with a tab per Unit, the Footprint with a
/// picker for the alternates. Choosing a Unit or Footprint while it is being placed
/// places that one.
fn details(
    ui: &mut Ui,
    panel: &mut LibraryPanel,
    target: Placing,
    library: &str,
    part: &Part,
) -> Vec<Action> {
    let mut actions = Vec::new();
    let Some((_, index)) = &panel.index else {
        return actions;
    };
    ui.strong(&part.name);
    ui.weak(tr!("library-in", library = library.to_owned()));
    if !part.fields.description.is_empty() {
        ui.weak(&part.fields.description);
    }
    let symbol = part.symbol.and_then(|s| index.symbol(s)).cloned();
    let footprints: Vec<_> = part
        .footprints
        .iter()
        .filter_map(|f| index.footprint(*f))
        .cloned()
        .collect();
    let mut changed = false;
    match &symbol {
        Some(symbol) => {
            if symbol.units.len() > 1 {
                ui.horizontal(|ui| {
                    for (i, unit) in symbol.units.iter().enumerate() {
                        let label = tr!("library-unit", unit = unit.name.clone());
                        let tab = ui.selectable_label(panel.unit == i, &unit.name);
                        tab.widget_info(|| {
                            WidgetInfo::selected(Role::Tab, true, panel.unit == i, label.clone())
                        });
                        if tab.clicked() && panel.unit != i {
                            panel.unit = i;
                            changed = true;
                        }
                    }
                });
            }
            let unit = symbol.units.get(panel.unit).map(|u| u.uuid);
            preview::symbol(ui, symbol, unit);
        }
        None => {
            ui.weak(tr!("library-no-symbol"));
        }
    }
    if !footprints.is_empty() {
        panel.footprint = panel.footprint.min(footprints.len() - 1);
        let label = tr!("library-footprint");
        let response = ComboBox::from_id_salt("library-footprint")
            .selected_text(&footprints[panel.footprint].name)
            .show_ui(ui, |ui| {
                for (i, f) in footprints.iter().enumerate() {
                    if ui.selectable_label(panel.footprint == i, &f.name).clicked()
                        && panel.footprint != i
                    {
                        panel.footprint = i;
                        changed = true;
                    }
                }
            })
            .response;
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
        preview::footprint(ui, &footprints[panel.footprint]);
    }
    if changed {
        let unit = symbol
            .as_ref()
            .and_then(|s| s.units.get(panel.unit))
            .map(|u| u.name.clone());
        let footprint = part.footprints.get(panel.footprint).copied();
        actions.extend(place(target, part, unit, footprint));
    }
    actions
}

/// A Component Type as the Library panel and the Part editor show it.
pub(crate) fn type_name(t: ComponentType) -> String {
    match t {
        ComponentType::Standard => tr!("library-type-standard"),
        ComponentType::Mechanical => tr!("library-type-mechanical"),
        ComponentType::NetTie => tr!("library-type-net-tie"),
    }
}
