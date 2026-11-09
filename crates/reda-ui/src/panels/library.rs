//! The Library panel: the registered Libraries' Parts, searched and filtered, with a
//! preview of the chosen one. On a sheet it lists Parts that have a Symbol, on the PCB
//! Mechanical Parts, and beside a Library editor tab that element's Library's Symbols,
//! Footprints and Parts (Library mode). Each Part row shows its Library Status over the
//! open Project ("3 placed, 1 differs"). Clicking or dragging a Part starts the placement
//! Tool (`tool.place`); Edit opens the chosen element in its editor; a row's menu has
//! Where Used, Duplicate, Copy to Library…, Check and more; and New… makes a Part, Symbol
//! or Footprint or a Library, imports a Part, runs a Footprint generator or makes a
//! Project Library from the Project's part tables.
//!
//! The panel reads the Libraries through [`Engine::libraries`](reda_engine::Engine),
//! scanning again only when a Command may have changed them (the Shell's library
//! generation) or on Refresh, and changes nothing itself.

mod details;
mod menu;
pub(crate) mod preview;
pub mod rows;

use std::collections::BTreeMap;

use egui::{ComboBox, Id, Role, ScrollArea, Sense, TextEdit, Ui, WidgetInfo};
use reda_engine::library::{LibraryIndex, SearchQuery};
use reda_engine::model::{ComponentType, DocumentKind, Part, Uuid};
use serde_json::{Value, json};

pub use rows::{Entry, Mode, RowStatus};

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

/// What a click in the panel does: what the window runs after the frame, or what the
/// panel does itself with the Shell once drawn.
enum Do {
    Out(Action),
    /// Run Where Used on the element and show what it found in the details.
    WhereUsed(Uuid),
    /// Run the Library check on the element into the Messages panel.
    Check(Uuid),
}

impl Do {
    /// Opens the element in its editor.
    fn open(element: Uuid) -> Do {
        Do::Out(Action::Run(
            "library.element.open".into(),
            json!({ "element": element }),
        ))
    }

    /// Opens the Update from Library dialog.
    fn update() -> Do {
        Do::Out(Action::Run(
            "schematic.dialog.open".into(),
            json!({ "dialog": "update_from_library" }),
        ))
    }
}

/// The panel's state between frames: the filters, the chosen row and the scanned
/// Libraries.
#[derive(Default)]
pub struct LibraryPanel {
    pub text: String,
    pub library: Option<Uuid>,
    pub component_type: Option<ComponentType>,
    /// The chosen row's element.
    pub selected: Option<Uuid>,
    /// The Unit tab shown, by index.
    pub unit: usize,
    /// The alternate Footprint shown, by index.
    pub footprint: usize,
    /// The scan and the library generation it was made at.
    index: Option<(u64, LibraryIndex)>,
    /// How many scans the panel has made, so the statuses know when theirs is stale.
    scans: u64,
    /// Each Part's Library Status, and the scan and Journal length it was worked out at.
    statuses: Option<((u64, usize), BTreeMap<Uuid, RowStatus>)>,
    /// The last Where Used result, and the element it was asked about.
    where_used: Option<(Uuid, Value)>,
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
            self.scans += 1;
        }
        &self.index.as_ref().expect("just scanned").1
    }

    /// Scans the Libraries again on the next frame.
    pub fn rescan(&mut self) {
        self.index = None;
    }

    /// What the panel lists now: Library mode beside a Library editor tab whose
    /// element a registered Library holds, else Parts to place in the active Document.
    pub fn mode(&mut self, shell: &Shell) -> Mode {
        let element = shell
            .active_document()
            .map(|d| d.document)
            .filter(|e| shell.engine().element(*e).is_some());
        if let Some(element) = element {
            let library = (shell.engine().element_source(element).map(|s| s.library))
                .or_else(|| self.index(shell).library_of(element).map(|l| l.info.uuid));
            if let Some(library) = library {
                return Mode::Library(library);
            }
        }
        Mode::Place(target(shell))
    }

    /// The rows the panel lists in `mode` for its search and filters.
    pub fn entries(&mut self, shell: &Shell, mode: Mode) -> Vec<Entry> {
        let query = SearchQuery {
            text: self.text.clone(),
            library: self.library,
            component_type: self.component_type,
        };
        rows::entries(self.index(shell), mode, &query)
    }

    /// Each Part's Library Status over the open Project, worked out again only after a
    /// Command ran or the Libraries were scanned again.
    pub fn statuses(&mut self, shell: &Shell) -> &BTreeMap<Uuid, RowStatus> {
        self.index(shell);
        let key = (self.scans, shell.engine().journal().len());
        if self.statuses.as_ref().is_none_or(|(k, _)| *k != key) {
            let index = &self.index.as_ref().expect("just scanned").1;
            let statuses = rows::statuses(shell.engine().project(), index);
            self.statuses = Some((key, statuses));
        }
        &self.statuses.as_ref().expect("just worked out").1
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
    let done = draw(ui, shell, &mut panel);
    let mut actions = Vec::new();
    for d in done {
        match d {
            Do::Out(action) => actions.push(action),
            Do::WhereUsed(element) => {
                // From a row's menu: its details show what was found.
                if panel.selected != Some(element) {
                    panel.selected = Some(element);
                    panel.unit = 0;
                    panel.footprint = 0;
                }
                if let Ok(found) =
                    shell.run_reporting("library.where_used", json!({ "element": element }))
                {
                    panel.where_used = Some((element, found));
                }
            }
            Do::Check(element) => {
                let _ = crate::element_editor::check(shell, element);
            }
        }
    }
    *shell.library_panel_mut() = panel;
    actions
}

fn draw(ui: &mut Ui, shell: &Shell, panel: &mut LibraryPanel) -> Vec<Do> {
    let mut out = Vec::new();
    let target = target(shell);
    let mode = panel.mode(shell);
    filters(ui, shell, panel, mode);
    let entries = panel.entries(shell, mode);
    let statuses = panel.statuses(shell).clone();
    if panel
        .selected
        .is_some_and(|s| !entries.iter().any(|e| e.uuid == s))
    {
        panel.selected = None;
    }
    match mode {
        Mode::Library(_) => ui.weak(tr!("library-library-mode")),
        Mode::Place(Placing::Sheet(_)) => ui.weak(tr!("library-sheet-parts")),
        Mode::Place(Placing::Board(_)) => ui.weak(tr!("library-board-parts")),
        Mode::Place(Placing::Nowhere) => ui.weak(tr!("library-no-document")),
    };
    let list_height = (ui.available_height() * 0.45).max(80.0);
    ScrollArea::vertical()
        .id_salt("library-parts")
        .max_height(list_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if entries.is_empty() {
                ui.weak(tr!("library-no-parts"));
            }
            for entry in &entries {
                let status = statuses.get(&entry.uuid).copied();
                out.extend(row(ui, panel, mode, target, entry, status));
            }
        });
    ui.separator();
    let chosen = panel
        .selected
        .and_then(|s| entries.iter().find(|e| e.uuid == s))
        .cloned();
    ui.horizontal(|ui| {
        // Edit opens the chosen element in its editor (a Part's links to its Symbol and
        // Footprints); New makes an element or a Library, imports a Part, generates a
        // Footprint or makes a Project Library.
        let label = match mode {
            Mode::Place(_) => tr!("library-edit"),
            Mode::Library(_) => tr!("library-edit-element"),
        };
        let edit = ui.add_enabled(chosen.is_some(), egui::Button::new(label));
        if edit.clicked()
            && let Some(entry) = &chosen
        {
            out.push(Do::open(entry.uuid));
        }
        ui.menu_button(tr!("library-new"), |ui| {
            out.extend(new_menu(ui, shell, panel.library).map(Do::Out));
        });
        if ui.button(tr!("library-refresh")).clicked() {
            panel.rescan();
        }
    });
    // The chosen row's details scroll under the buttons, which stay in view.
    if let Some(entry) = &chosen {
        ui.separator();
        ScrollArea::vertical()
            .id_salt("library-details")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let project = shell.engine().project();
                out.extend(details::show(ui, panel, project, target, entry));
            });
    }
    out
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
    if ui.button(tr!("library-make-project-library")).clicked() {
        picked = Some(Action::Call(
            "library.extract_from_project".into(),
            with_library(json!({})),
        ));
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
fn filters(ui: &mut Ui, shell: &Shell, panel: &mut LibraryPanel, mode: Mode) {
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
        // In Library mode the editor's Library is the one listed.
        let shown = matches!(mode, Mode::Place(_));
        let chosen = panel
            .library
            .and_then(|u| libraries.iter().find(|(l, _)| *l == u))
            .map_or(all.clone(), |(_, n)| n.clone());
        let label = tr!("library-filter-library");
        if shown {
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
        }
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

/// One row. A Part's click chooses it and picks it up to place, and so does a drag,
/// which carries it onto a canvas; in Library mode a click only chooses it and a
/// double-click opens it. A right-click opens its menu; a Part placed in the Project
/// shows its Library Status, which opens Update from Library.
fn row(
    ui: &mut Ui,
    panel: &mut LibraryPanel,
    mode: Mode,
    target: Placing,
    entry: &Entry,
    status: Option<RowStatus>,
) -> Vec<Do> {
    let mut out = Vec::new();
    let chosen = panel.selected == Some(entry.uuid);
    let label = entry.label(mode);
    let response = ui
        .horizontal(|ui| {
            let response = ui
                .selectable_label(chosen, entry.text(mode))
                .interact(Sense::click_and_drag());
            response.widget_info(|| WidgetInfo::selected(Role::ListItem, true, chosen, &label));
            if let Some(status) = status.filter(|s| s.placed > 0) {
                let text = status.text();
                let shown = ui
                    .add(egui::Label::new(egui::RichText::new(&text).weak()).sense(Sense::click()));
                let name = tr!(
                    "library-status-of",
                    name = entry.name.clone(),
                    status = text
                );
                shown.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
                if shown.clicked() {
                    out.push(Do::update());
                }
            }
            response
        })
        .inner;
    response.context_menu(|ui| {
        out.extend(menu::show(ui, entry, target, status));
    });
    let place_here = entry
        .part
        .as_ref()
        .filter(|_| matches!(mode, Mode::Place(_)));
    if place_here.is_some() && response.drag_started() {
        response.dnd_set_drag_payload(Dragged);
    }
    let picked = response.clicked() || (place_here.is_some() && response.drag_started());
    if picked && !chosen {
        panel.selected = Some(entry.uuid);
        panel.unit = 0;
        panel.footprint = 0;
    }
    if let Some(part) = place_here
        && picked
    {
        out.extend(
            place(
                target,
                part,
                None,
                part.footprints.get(panel.footprint).copied(),
            )
            .map(Do::Out),
        );
    }
    if matches!(mode, Mode::Library(_)) && response.double_clicked() {
        out.push(Do::open(entry.uuid));
    }
    out
}

/// A Component Type as the Library panel and the Part editor show it.
pub(crate) fn type_name(t: ComponentType) -> String {
    match t {
        ComponentType::Standard => tr!("library-type-standard"),
        ComponentType::Mechanical => tr!("library-type-mechanical"),
        ComponentType::NetTie => tr!("library-type-net-tie"),
    }
}
