//! The Library panel: the registered Libraries' Parts, searched and filtered, with a
//! preview of the chosen one. On a sheet it lists Parts that have a Symbol, on the PCB
//! Mechanical Parts, and beside a Library editor tab that element's Library's Symbols,
//! Footprints, Parts and Board Templates (Library mode). Each Part row shows its Library Status over the
//! open Project ("3 placed, 1 differs"). Clicking or dragging a Part starts the placement
//! Tool (`tool.place`); Edit opens the chosen element in its editor; a row's menu has
//! Where Used, Duplicate, Copy to Library…, Check and more (a Board Template's: Duplicate,
//! Copy to Library… and Delete…, which asks first); and New… makes a Part, Symbol
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

use egui::{Id, Role, ScrollArea, Sense, Ui, WidgetInfo};
use reda_engine::library::{ElementKind, LibraryIndex, SearchQuery};
use reda_engine::model::{ComponentType, DocumentKind, Part, Uuid};
use serde_json::{Value, json};

pub use rows::{Entry, Mode, RowStatus};

use crate::shell::Shell;
use crate::tr;
use crate::widgets;

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
    /// Ask whether to delete the element, then run `library.element.delete`.
    Delete(Box<Entry>),
}

impl Do {
    /// Opens the element in its editor.
    fn open(element: Uuid) -> Do {
        Do::Out(Action::Run(
            "library.element.open".into(),
            json!({ "element": element }),
        ))
    }

    /// Opens the Update from Library dialog on the Components of Part `part`.
    fn update(part: Uuid) -> Do {
        Do::Out(Action::Run(
            "schematic.update_from_library.open".into(),
            json!({ "parts": [part] }),
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
    /// The row whose Delete… waits for its confirmation.
    pub deleting: Option<Entry>,
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
            Do::Delete(entry) => panel.deleting = Some(*entry),
        }
    }
    if let Some(entry) = panel.deleting.clone() {
        match confirm_delete(ui.ctx(), &entry) {
            Some(true) => {
                panel.deleting = None;
                actions.push(Action::Run(
                    "library.element.delete".into(),
                    json!({ "element": entry.uuid }),
                ));
            }
            Some(false) => panel.deleting = None,
            None => {}
        }
    }
    *shell.library_panel_mut() = panel;
    actions
}

/// The confirmation of a row's Delete…: `Some(true)` to delete, `Some(false)` to keep it,
/// `None` while it waits.
fn confirm_delete(ctx: &egui::Context, entry: &Entry) -> Option<bool> {
    let mut answer = None;
    let title = tr!("library-delete-title", name = entry.name.clone());
    let modal = crate::modal::Dialog::new(Id::NULL.with("library-delete"), title)
        .width(420.0)
        .show(ctx, |ui| {
            let kind = match entry.kind {
                ElementKind::BoardTemplate => {
                    tr!("library-kind-board-template")
                }
                ElementKind::Symbol => tr!("library-kind-symbol"),
                ElementKind::Footprint => tr!("library-kind-footprint"),
                ElementKind::Part => tr!("library-kind-part"),
            };
            crate::settings::layout::text(
                ui,
                tr!(
                    "library-delete-intro",
                    kind = kind,
                    name = entry.name.clone(),
                    library = entry.library_name.clone()
                ),
            );
            crate::modal::buttons(ui, |ui| {
                if ui
                    .add(widgets::button(tr!("library-delete-run")).primary())
                    .clicked()
                {
                    answer = Some(true);
                }
                if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                    answer = Some(false);
                }
            });
        });
    if modal.should_close() && answer.is_none() {
        answer = Some(false);
    }
    answer
}

fn draw(ui: &mut Ui, shell: &Shell, panel: &mut LibraryPanel) -> Vec<Do> {
    let mut out = Vec::new();
    let target = target(shell);
    // Whether the Panel holds keyboard focus: its chosen row is live only then.
    let focused = shell.panel_focused(crate::layout::PanelId::Library);
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
            widgets::rows(ui, |ui| {
                for entry in &entries {
                    let status = statuses.get(&entry.uuid).copied();
                    out.extend(row(ui, panel, mode, target, entry, status, focused));
                }
            });
        });
    ui.separator();
    let chosen = panel
        .selected
        .and_then(|s| entries.iter().find(|e| e.uuid == s))
        .cloned();
    // The buttons wrap where the Dock is too narrow for the three.
    ui.horizontal_wrapped(|ui| {
        // Edit opens the chosen element in its editor (a Part's links to its Symbol and
        // Footprints); New makes an element or a Library, imports a Part, generates a
        // Footprint or makes a Project Library.
        let label = match mode {
            Mode::Place(_) => tr!("library-edit"),
            Mode::Library(_) => tr!("library-edit-element"),
        };
        // A Board Template has no editor.
        let editable = chosen
            .as_ref()
            .is_some_and(|e| e.kind != ElementKind::BoardTemplate);
        let edit = ui.add_enabled(editable, widgets::button(label));
        if edit.clicked()
            && let Some(entry) = &chosen
        {
            out.push(Do::open(entry.uuid));
        }
        widgets::menu_button(tr!("library-new")).show(ui, |ui| {
            out.extend(new_menu(ui, shell, panel.library).map(Do::Out));
        });
        if ui.add(widgets::button(tr!("library-refresh"))).clicked() {
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
        if ui.add(widgets::menu_item(label)).clicked() {
            let params = with_library(json!({ "kind": kind }));
            picked = Some(Action::Call("library.element.new".into(), params));
        }
    }
    widgets::menu_separator(ui);
    if ui
        .add(widgets::menu_item(tr!("library-new-library")))
        .clicked()
    {
        picked = Some(Action::Call("library.create".into(), json!({})));
    }
    if ui
        .add(widgets::menu_item(tr!("library-make-project-library")))
        .clicked()
    {
        picked = Some(Action::Call(
            "library.extract_from_project".into(),
            with_library(json!({})),
        ));
    }
    widgets::menu_separator(ui);
    if ui
        .add(widgets::menu_item(tr!("library-import-part")))
        .clicked()
    {
        picked = Some(Action::Call(
            "library.import_part".into(),
            with_library(json!({})),
        ));
    }
    widgets::menu_button(tr!("library-generate")).show(ui, |ui| {
        for id in reda_engine::library::GENERATORS {
            let title = shell.engine().registry().get(id).map_or_else(
                || id.to_owned(),
                |m| crate::bars::command_title(id, m.title),
            );
            if ui.add(widgets::menu_item(format!("{title}…"))).clicked() {
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
        widgets::text_field(&mut panel.text)
            .id(Id::NULL.with("library-search"))
            .hint(search.clone())
            .width(f32::INFINITY),
    );
    response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, search.clone()));
    let libraries: Vec<(Uuid, String)> = panel
        .index(shell)
        .libraries()
        .map(|l| (l.info.uuid, l.info.name.clone()))
        .collect();
    ui.horizontal_wrapped(|ui| {
        let all = tr!("library-all-libraries");
        // In Library mode the editor's Library is the one listed.
        let shown = matches!(mode, Mode::Place(_));
        let chosen = panel
            .library
            .and_then(|u| libraries.iter().find(|(l, _)| *l == u))
            .map_or(all.clone(), |(_, n)| n.clone());
        let label = tr!("library-filter-library");
        if shown {
            let response = widgets::dropdown("library-filter-library", chosen)
                .show(ui, |ui| {
                    widgets::choice_value(ui, &mut panel.library, None, all);
                    for (uuid, name) in &libraries {
                        widgets::choice_value(ui, &mut panel.library, Some(*uuid), name);
                    }
                })
                .response;
            response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
        }
        let type_name =
            |t: Option<ComponentType>| t.map_or_else(|| tr!("library-all-types"), type_name);
        let label = tr!("library-filter-type");
        let response = widgets::dropdown("library-filter-type", type_name(panel.component_type))
            .show(ui, |ui| {
                for t in std::iter::once(None).chain(ComponentType::ALL.map(Some)) {
                    widgets::choice_value(ui, &mut panel.component_type, t, type_name(t));
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
    focused: bool,
) -> Vec<Do> {
    let mut out = Vec::new();
    let chosen = panel.selected == Some(entry.uuid);
    let label = entry.label(mode);
    // The Library Status sits at the row's trailing end, and the row takes the rest.
    let across = egui::Layout::right_to_left(egui::Align::Center);
    let response = ui
        .with_layout(across, |ui| {
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
                    out.push(Do::update(entry.uuid));
                }
            }
            let row = widgets::list_row(entry.text(mode))
                .selected(chosen)
                .panel_focused(focused)
                .sense(Sense::click_and_drag());
            let response = ui.add(row);
            response.widget_info(|| WidgetInfo::selected(Role::ListItem, true, chosen, &label));
            response
        })
        .inner;
    widgets::context_menu(&response, |ui| {
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
    let editable = entry.kind != ElementKind::BoardTemplate;
    if matches!(mode, Mode::Library(_)) && editable && response.double_clicked() {
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
