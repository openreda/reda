//! The Projects panel: the open Project as a tree of its sheets, PCB Document, Output
//! Jobs and Library folders, each item with a dirty marker and a menu of actions. Each
//! Document shows its number in Project order and its file's path on hover; a missing
//! Document shows greyed, with Relink… and Remove from Project.
//!
//! The panel only reads the Project. Every change is a Command it pushes for the window
//! to run after the frame; what needs the window (a folder picker, the file manager, a
//! dialog) comes back as a [`ProjectsAction`]. Renaming, the dragged sheet and a pending
//! delete confirmation are the panel's own UI state, kept in egui's memory.

use std::path::{Path, PathBuf};

use egui::{Button, CollapsingHeader, Id, Role, Sense, TextEdit, Ui, WidgetInfo};
use reda_engine::model::{DocumentEntry, DocumentKind, ListedDocument, Project, Scope, Uuid};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tr;

/// What the panel asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectsAction {
    /// Pick a folder to add as a Project Library.
    AddLibraryFolder,
    /// Show a folder in the OS file manager.
    ShowInFolder(PathBuf),
    /// Open an Output Job's dialog.
    EditJob(Uuid),
    /// Run an Output Job.
    RunJob(Uuid),
    /// Run every Output Job (`output.job.run_all`).
    RunAllJobs,
    /// Open the Project Settings dialog.
    ProjectSettings,
    /// Open the Board Setup dialog.
    BoardSetup,
    /// Pick a file to relink a missing Document to.
    Relink(Uuid),
    /// Open the Package Project dialog.
    Package,
}

/// The panel's own UI state.
#[derive(Clone, Debug, Default)]
struct PanelState {
    /// The Document being renamed and the name typed so far.
    renaming: Option<(Uuid, String)>,
    /// Focus the rename field on its first frame.
    focus_rename: bool,
    /// A sheet waiting for the user to confirm its delete.
    confirm_delete: Option<Uuid>,
}

/// The payload of a dragged sheet.
struct DraggedSheet(Uuid);

fn state_id() -> Id {
    Id::NULL.with("projects-panel")
}

/// Whether one of the panel's dialogs is open. Mod+Z then uses the Project stack.
pub fn dialog_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<PanelState>(state_id()))
        .is_some_and(|s| s.confirm_delete.is_some())
}

/// Commands the panel wants run, in order.
type Commands = Vec<(&'static str, Value)>;

/// Draws the panel. Commands go into `commands`; anything else comes back.
pub fn show(ui: &mut Ui, shell: &Shell, commands: &mut Commands) -> Option<ProjectsAction> {
    let Some(project) = shell.engine().project() else {
        ui.weak(tr!("projects-none"));
        return None;
    };
    let mut state = ui
        .data(|d| d.get_temp::<PanelState>(state_id()))
        .unwrap_or_default();
    let mut tree = Tree {
        shell,
        project,
        dirty: shell.engine().dirty_scopes(),
        state: &mut state,
        commands,
        action: None,
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| tree.root(ui));
    tree.confirm_delete(ui.ctx());
    let action = tree.action.take();
    ui.data_mut(|d| d.insert_temp(state_id(), state));
    action
}

struct Tree<'a> {
    shell: &'a Shell,
    project: &'a Project,
    dirty: Vec<Scope>,
    state: &'a mut PanelState,
    commands: &'a mut Commands,
    action: Option<ProjectsAction>,
}

impl Tree<'_> {
    fn run(&mut self, id: &'static str, params: Value) {
        self.commands.push((id, params));
    }

    /// `name`, marked when its file has unsaved changes.
    fn label(&self, name: &str, scope: Scope) -> String {
        if self.dirty.contains(&scope) {
            tr!("projects-dirty", name = name)
        } else {
            name.to_owned()
        }
    }

    fn folder(&self) -> Option<PathBuf> {
        self.shell.engine().location().map(Path::to_path_buf)
    }

    fn root(&mut self, ui: &mut Ui) {
        let name = self.project.props().name.clone();
        let header = CollapsingHeader::new(self.label(&name, Scope::Project))
            .id_salt("projects-root")
            .default_open(true)
            .show(ui, |ui| {
                self.sheets(ui);
                self.pcb(ui);
                self.jobs(ui);
                self.libraries(ui);
            });
        header.header_response.context_menu(|ui| {
            if item(ui, tr!("projects-settings"), None) {
                self.action = Some(ProjectsAction::ProjectSettings);
            }
            if item(ui, tr!("projects-save-all"), None) {
                self.run("project.save", json!({}));
            }
            if item(ui, tr!("projects-package"), None) {
                self.action = Some(ProjectsAction::Package);
            }
            if item(ui, tr!("projects-show-in-folder"), None)
                && let Some(folder) = self.folder()
            {
                self.action = Some(ProjectsAction::ShowInFolder(folder));
            }
        });
    }

    fn sheets(&mut self, ui: &mut Ui) {
        let sheets: Vec<ListedDocument> = (self.project.document_list().into_iter())
            .filter(|d| d.entry.kind == DocumentKind::Schematic)
            .collect();
        let header = CollapsingHeader::new(tr!("projects-schematic"))
            .id_salt("projects-schematic")
            .default_open(true)
            .show(ui, |ui| {
                for (i, sheet) in sheets.iter().enumerate() {
                    if sheet.missing {
                        self.missing_row(ui, &sheet.entry);
                        continue;
                    }
                    let before = i.checked_sub(1).map(|j| &sheets[j].entry);
                    let after = sheets.get(i + 1).map(|d| &d.entry);
                    self.sheet(ui, &sheet.entry, before, after);
                }
            });
        header.header_response.context_menu(|ui| {
            if item(ui, tr!("projects-add-sheet"), None) {
                self.run("document.add", json!({"kind": "schematic"}));
            }
        });
    }

    /// The position of `document` in the Project's Document list, missing Documents
    /// included: what `document.reorder` takes, and the Document's number less one.
    fn index(&self, document: Uuid) -> usize {
        (self.project.props().documents.iter())
            .position(|d| *d == document)
            .unwrap_or_default()
    }

    /// The full path of `document`'s file, as its row's tooltip shows it.
    fn file_path(&self, document: &DocumentEntry) -> String {
        let file = document.file_name();
        match self.folder() {
            Some(folder) => reda_engine::fs::display_path(&folder.join(file)),
            None => file,
        }
    }

    /// The Document's number in Project order, before its row.
    fn number(&self, ui: &mut Ui, document: &DocumentEntry) {
        let number = (self.index(document.uuid) + 1).to_string();
        let label = egui::Label::new(egui::RichText::new(number).weak());
        ui.add_sized([18.0, 18.0], label);
    }

    /// A sheet's number among the sheets, "n/m" after its name: what its title block and
    /// Port cross-references call it.
    fn sheet_number(&self, ui: &mut Ui, document: Uuid) {
        let sheets: Vec<Uuid> = (self.project.documents())
            .filter(|d| self.project.schematic_sheet(d.uuid).is_some())
            .map(|d| d.uuid)
            .collect();
        let Some(index) = sheets.iter().position(|s| *s == document) else {
            return;
        };
        let (number, count) = (index as i64 + 1, sheets.len() as i64);
        let text = egui::RichText::new(format!("{number}/{count}"))
            .weak()
            .small();
        let label = ui.add(egui::Label::new(text).selectable(false));
        let name = tr!("projects-sheet-number", number = number, count = count);
        label.widget_info(|| WidgetInfo::labeled(Role::Label, true, name.clone()));
    }

    /// A missing Document: greyed, with its file's path on hover and a menu to relink it
    /// or remove it from the Project.
    fn missing_row(&mut self, ui: &mut Ui, document: &DocumentEntry) {
        let path = self.file_path(document);
        let label = tr!("projects-missing", name = document.title.as_str());
        let row = ui
            .horizontal(|ui| {
                self.number(ui, document);
                let text = egui::RichText::new(&label).color(ui.visuals().weak_text_color());
                let row = ui.add(Button::selectable(false, text).sense(Sense::click()));
                let name = label.clone();
                row.widget_info(|| WidgetInfo::labeled(Role::TreeItem, true, name.clone()));
                row
            })
            .inner;
        let row = row.on_hover_text(tr!("projects-missing-hint", path = path.as_str()));
        row.context_menu(|ui| {
            if item(ui, tr!("projects-relink"), None) {
                self.action = Some(ProjectsAction::Relink(document.uuid));
            }
            if item(ui, tr!("projects-remove-from-project"), None) {
                self.run("document.delete", json!({"document": document.uuid}));
            }
        });
    }

    fn sheet(
        &mut self,
        ui: &mut Ui,
        sheet: &DocumentEntry,
        before: Option<&DocumentEntry>,
        after: Option<&DocumentEntry>,
    ) {
        let Some(row) = self.document_row(ui, sheet, true) else {
            return;
        };
        // Dropping a dragged sheet here puts it in this sheet's place.
        if let Some(dragged) = row.dnd_hover_payload::<DraggedSheet>()
            && dragged.0 != sheet.uuid
        {
            let rect = row.rect;
            let stroke = ui.visuals().selection.stroke;
            ui.painter().hline(rect.x_range(), rect.top(), stroke);
        }
        if let Some(dragged) = row.dnd_release_payload::<DraggedSheet>()
            && dragged.0 != sheet.uuid
        {
            let index = self.index(sheet.uuid);
            self.run(
                "document.reorder",
                json!({"document": dragged.0, "index": index}),
            );
        }
        let (up, down) = (
            before.map(|d| self.index(d.uuid)),
            after.map(|d| self.index(d.uuid)),
        );
        row.context_menu(|ui| {
            self.document_menu_top(ui, sheet);
            for (label, target) in [
                (tr!("projects-move-up"), up),
                (tr!("projects-move-down"), down),
            ] {
                let refused = target.is_none().then(|| tr!("projects-cannot-move"));
                if item(ui, label, refused)
                    && let Some(index) = target
                {
                    self.run(
                        "document.reorder",
                        json!({"document": sheet.uuid, "index": index}),
                    );
                }
            }
            if item(ui, tr!("projects-delete"), None) {
                let components = self
                    .project
                    .schematic_sheet(sheet.uuid)
                    .map_or(0, |s| s.units().count());
                if components > 0 {
                    self.state.confirm_delete = Some(sheet.uuid);
                } else {
                    self.run("document.delete", json!({"document": sheet.uuid}));
                }
            }
        });
    }

    /// A Document's row: its title (or the rename field), opened by a double-click.
    /// Returns the row's response unless it is being renamed.
    fn document_row(
        &mut self,
        ui: &mut Ui,
        document: &DocumentEntry,
        draggable: bool,
    ) -> Option<egui::Response> {
        if let Some((uuid, text)) = &mut self.state.renaming
            && *uuid == document.uuid
        {
            let field = ui.add(TextEdit::singleline(text).desired_width(f32::INFINITY));
            let name = tr!("projects-rename-field", name = document.title.as_str());
            field.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, name.clone()));
            if std::mem::take(&mut self.state.focus_rename) {
                field.request_focus();
            }
            if field.lost_focus() {
                let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                let title = text.trim().to_owned();
                if !cancelled && !title.is_empty() && title != document.title {
                    self.run(
                        "document.rename",
                        json!({"document": document.uuid, "title": title}),
                    );
                }
                self.state.renaming = None;
            }
            return None;
        }
        let open = self.shell.layout().active_document() == Some(document.uuid);
        let label = self.label(&document.title, Scope::Document(document.uuid));
        let sense = if draggable {
            Sense::click_and_drag()
        } else {
            Sense::click()
        };
        let path = self.file_path(document);
        let row = ui
            .horizontal(|ui| {
                self.number(ui, document);
                let row = tree_item(ui, open, &label, sense);
                self.sheet_number(ui, document.uuid);
                row
            })
            .inner
            .on_hover_text(path);
        if draggable && row.dragged() {
            row.dnd_set_drag_payload(DraggedSheet(document.uuid));
        }
        if row.double_clicked() {
            self.run("document.open", json!({"document": document.uuid}));
        }
        Some(row)
    }

    /// The menu items every Document has first: Open and Rename.
    fn document_menu_top(&mut self, ui: &mut Ui, document: &DocumentEntry) {
        if item(ui, tr!("projects-open"), None) {
            self.run("document.open", json!({"document": document.uuid}));
        }
        if item(ui, tr!("projects-rename"), None) {
            self.state.renaming = Some((document.uuid, document.title.clone()));
            self.state.focus_rename = true;
        }
    }

    fn pcb(&mut self, ui: &mut Ui) {
        let pcb =
            (self.project.document_list().into_iter()).find(|d| d.entry.kind == DocumentKind::Pcb);
        let present = pcb.as_ref().filter(|d| !d.missing).map(|d| d.entry.clone());
        let header = CollapsingHeader::new(tr!("projects-pcb"))
            .id_salt("projects-pcb")
            .default_open(true)
            .show(ui, |ui| match &pcb {
                Some(missing) if missing.missing => self.missing_row(ui, &missing.entry),
                Some(ListedDocument { entry: pcb, .. }) => {
                    if let Some(row) = self.document_row(ui, pcb, false) {
                        row.context_menu(|ui| {
                            self.document_menu_top(ui, pcb);
                            if item(ui, tr!("projects-board-setup"), None) {
                                self.action = Some(ProjectsAction::BoardSetup);
                            }
                            if item(ui, tr!("projects-delete"), None) {
                                self.run("document.delete", json!({"document": pcb.uuid}));
                            }
                        });
                    }
                }
                None => {
                    ui.weak(tr!("projects-no-pcb"));
                }
            });
        header.header_response.context_menu(|ui| {
            let refused = present.is_some().then(|| tr!("projects-pcb-exists"));
            if item(ui, tr!("projects-add-pcb"), refused) {
                self.run("document.add", json!({"kind": "pcb"}));
            }
        });
    }

    fn jobs(&mut self, ui: &mut Ui) {
        let jobs: Vec<(Uuid, String)> = self
            .project
            .output_jobs()
            .map(|j| (j.uuid, j.name.clone()))
            .collect();
        let header = CollapsingHeader::new(tr!("projects-output-jobs"))
            .id_salt("projects-jobs")
            .default_open(true)
            .show(ui, |ui| {
                if jobs.is_empty() {
                    ui.weak(tr!("projects-no-jobs"));
                }
                for (uuid, name) in &jobs {
                    self.job(ui, *uuid, name);
                }
            });
        let preset = self.project.fab_presets().next().map(|p| p.uuid);
        header.header_response.context_menu(|ui| {
            let refused = jobs.is_empty().then(|| tr!("projects-no-jobs"));
            if item(ui, tr!("projects-run-all"), refused) {
                self.action = Some(ProjectsAction::RunAllJobs);
            }
            let refused = preset.is_none().then(|| tr!("projects-no-fab-preset"));
            if item(ui, tr!("projects-new-job"), refused)
                && let Some(preset) = preset
            {
                self.run("output.job.add", json!({"fab_preset": preset}));
            }
        });
    }

    fn job(&mut self, ui: &mut Ui, job: Uuid, name: &str) {
        let broken = crate::settings::output_job::broken(self.project, job);
        let label = if broken {
            tr!("output-job-broken-label", job = name)
        } else {
            name.to_owned()
        };
        let mut row = tree_item(ui, false, &label, Sense::click());
        if broken {
            row = row.on_hover_text(tr!("output-job-broken"));
        }
        if row.double_clicked() {
            self.action = Some(ProjectsAction::EditJob(job));
        }
        row.context_menu(|ui| {
            let refused = broken.then(|| tr!("output-job-broken"));
            if item(ui, tr!("projects-run-job"), refused) {
                self.action = Some(ProjectsAction::RunJob(job));
            }
            if item(ui, tr!("projects-edit-job"), None) {
                self.action = Some(ProjectsAction::EditJob(job));
            }
            if item(ui, tr!("projects-duplicate"), None) {
                self.run("output.job.duplicate", json!({"job": job}));
            }
            if item(ui, tr!("projects-delete"), None) {
                self.run("output.job.delete", json!({"job": job}));
            }
        });
    }

    fn libraries(&mut self, ui: &mut Ui) {
        let paths = self.project.props().libraries.clone();
        let header = CollapsingHeader::new(tr!("projects-libraries"))
            .id_salt("projects-libraries")
            .default_open(true)
            .show(ui, |ui| {
                if paths.is_empty() {
                    ui.weak(tr!("projects-no-libraries"));
                }
                for path in &paths {
                    let row = tree_item(ui, false, path, Sense::click());
                    row.context_menu(|ui| {
                        if item(ui, tr!("projects-remove"), None) {
                            self.run("project.library.remove", json!({"path": path}));
                        }
                        if item(ui, tr!("projects-show-in-folder"), None)
                            && let Some(folder) = self.folder()
                        {
                            self.action = Some(ProjectsAction::ShowInFolder(folder.join(path)));
                        }
                    });
                }
            });
        header.header_response.context_menu(|ui| {
            if item(ui, tr!("projects-add-folder"), None) {
                self.action = Some(ProjectsAction::AddLibraryFolder);
            }
        });
    }

    /// Asks before deleting a sheet that holds Components.
    fn confirm_delete(&mut self, ctx: &egui::Context) {
        let Some(sheet) = self.state.confirm_delete else {
            return;
        };
        let Some(entry) = self.project.document(sheet) else {
            self.state.confirm_delete = None;
            return;
        };
        let count = self
            .project
            .schematic_sheet(sheet)
            .map_or(0, |s| s.units().count());
        let mut close = false;
        let modal = crate::modal::Dialog::new(
            Id::NULL.with("projects-confirm-delete"),
            tr!("projects-confirm-delete-title"),
        )
        .width(360.0)
        .show(ctx, |ui| {
            ui.label(tr!(
                "projects-confirm-delete",
                name = entry.title.as_str(),
                count = count
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("projects-delete-sheet")).clicked() {
                    self.commands
                        .push(("document.delete", json!({"document": sheet})));
                    close = true;
                }
                if ui.button(tr!("dialog-cancel")).clicked() {
                    close = true;
                }
            });
        });
        if close || modal.should_close() {
            self.state.confirm_delete = None;
        }
    }
}

/// One item of the tree, announced as a tree item.
fn tree_item(ui: &mut Ui, selected: bool, label: &str, sense: Sense) -> egui::Response {
    let row = ui.add(Button::selectable(selected, label).sense(sense));
    let label = label.to_owned();
    row.widget_info(|| WidgetInfo::labeled(Role::TreeItem, true, label.clone()));
    row
}

/// A menu item that closes the menu when clicked; `refused` greys it out with a reason.
fn item(ui: &mut Ui, label: String, refused: Option<String>) -> bool {
    let response = ui.add_enabled(refused.is_none(), Button::new(label));
    let clicked = response.clicked();
    if let Some(reason) = refused {
        response.on_disabled_hover_text(reason);
    }
    if clicked {
        ui.close();
    }
    clicked
}

/// Registers the Package Project dialog: the generated form of `project.package`, with the
/// zip beside the Project folder filled in and a warning for each Project Library outside
/// the folder (and for unsaved changes, which the package leaves out).
pub fn register_package_dialog(dialogs: &mut crate::command_dialog::Dialogs) {
    dialogs.register("project.package", |shell, mut params| {
        let engine = shell.engine();
        if let (Some(project), Some(folder), Some(given)) =
            (engine.project(), engine.location(), params.as_object_mut())
            && !given.contains_key("path")
        {
            let zip = folder.with_file_name(format!("{}.zip", project.props().name));
            given.insert("path".into(), json!(reda_engine::fs::display_path(&zip)));
        }
        let outside = (engine.project()).map(reda_engine::commands::package::outside_libraries);
        let mut notes: Vec<String> = (outside.into_iter().flatten())
            .map(|l| tr!("package-outside-library", path = l.as_str()))
            .collect();
        if engine.is_dirty() {
            notes.push(tr!("package-unsaved"));
        }
        let form =
            crate::command_dialog::ParamForm::new(engine.registry(), "project.package", &params);
        Box::new(form.with_notes(notes))
    });
}
