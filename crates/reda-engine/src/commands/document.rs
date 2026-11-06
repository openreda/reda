//! `document.add`, `document.delete`, `document.rename` and `document.reorder`: Documents
//! come, go, change name and move on the Project's own undo stack, each with its contents.
//! The files follow on save (see `store::save`). `document.revert` drops a Document's
//! unsaved changes by reading its file again.

use std::collections::BTreeMap;

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, Registry,
    Unavailable,
};
use reda_model::{DocumentEntry, DocumentKind, Object, Project, Scope, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::project::check_file_stem;
use crate::builders::{add_document, remove_document};
use crate::format::{read_bytes, read_pcb_file, read_sheet_file};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(DocumentAdd);
    registry.add(DocumentDelete);
    registry.add(DocumentRename);
    registry.add(DocumentReorder);
    registry.add(DocumentRevert);
}

pub struct DocumentAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentAddParams {
    pub kind: DocumentKind,
    /// The title. Defaults to "Sheet <n>" for a sheet and the Project name for the PCB
    /// Document.
    #[serde(default)]
    pub title: Option<String>,
}

impl Command for DocumentAdd {
    const META: Meta = Meta {
        id: "document.add",
        title: "Add Document",
        description: "Adds a Document of `kind` at the end of the Project. A Schematic Sheet \
                      is empty; the PCB Document starts with its Layers, the Generic design \
                      rule records and DRC severities, and is available only \
                      while the Project has none. Its file is written on save.",
        category: "project",
        icon: "document_add",
        kind: Kind::Edit,
    };
    type Params = DocumentAddParams;
    type Output = DocumentEntry;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: DocumentAddParams,
    ) -> Result<DocumentEntry, CommandError> {
        let project = cx.project()?;
        if p.kind == DocumentKind::Pcb && project.pcb().is_some() {
            return Err(CommandError::unavailable(
                "the Project already has a PCB Document",
            ));
        }
        let (title, name) = match p.title {
            Some(title) if title.trim().is_empty() => {
                return Err(CommandError::invalid_params("/title", "the title is empty"));
            }
            Some(title) => {
                let name = free_name(project, p.kind, &title, None);
                (title, name)
            }
            None => default_names(project, p.kind),
        };
        let uuid = cx.new_id();
        let contents = match p.kind {
            DocumentKind::Schematic => vec![],
            DocumentKind::Pcb => {
                reda_pcb::new_board(uuid, &reda_output::generic().rules, || cx.new_id())
            }
        };
        let entry = DocumentEntry {
            uuid,
            kind: p.kind,
            name,
            title,
        };
        let tx = cx.tx()?;
        add_document(tx, entry.clone())?;
        for object in contents {
            tx.insert(object)?;
        }
        Ok(entry)
    }
}

/// The title and file stem of a Document added without a title: "Sheet <n>" in
/// `sheet-<n>` for a sheet, with the first `n` from the sheet count up that neither takes;
/// the Project name for the PCB Document.
fn default_names(project: &Project, kind: DocumentKind) -> (String, String) {
    match kind {
        DocumentKind::Schematic => {
            let sheets: Vec<&DocumentEntry> =
                project.documents().filter(|d| d.kind == kind).collect();
            let taken = |title: &str, name: &str| {
                sheets.iter().any(|d| {
                    d.title.to_lowercase() == title.to_lowercase() || d.name.to_lowercase() == name
                })
            };
            let mut n = sheets.len() + 1;
            while taken(&format!("Sheet {n}"), &format!("sheet-{n}")) {
                n += 1;
            }
            (format!("Sheet {n}"), format!("sheet-{n}"))
        }
        DocumentKind::Pcb => {
            let title = project.props().name.clone();
            let name = free_name(project, kind, &title, None);
            (title, name)
        }
    }
}

/// A file stem for a Document titled `title`: the title with characters file names cannot
/// hold replaced, made unique among the Documents of the same kind (other than `except`)
/// by a number suffix.
fn free_name(project: &Project, kind: DocumentKind, title: &str, except: Option<Uuid>) -> String {
    let mut base: String = title
        .trim()
        .chars()
        .map(|c| match check_file_stem(&c.to_string()) {
            Ok(()) => c,
            Err(_) if c == ' ' => c,
            Err(_) => '_',
        })
        .collect();
    while check_file_stem(&base).is_err() {
        base.push('_');
    }
    let taken = |name: &str| {
        project
            .documents()
            .filter(|d| Some(d.uuid) != except)
            .any(|d| d.kind == kind && d.name.to_lowercase() == name.to_lowercase())
    };
    let mut name = base.clone();
    let mut n = 2;
    while taken(&name) {
        name = format!("{base} {n}");
        n += 1;
    }
    name
}

pub struct DocumentDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentDeleteParams {
    /// The Document's UUID.
    pub document: Uuid,
}

impl Command for DocumentDelete {
    const META: Meta = Meta {
        id: "document.delete",
        title: "Delete Document",
        description: "Deletes a Document and everything in it, as one undoable step on the \
                      Project's stack. Its file goes to the OS trash when the Project file is next saved.",
        category: "project",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = DocumentDeleteParams;
    type Output = ();

    fn run(&self, cx: &mut Context<'_>, p: DocumentDeleteParams) -> Result<(), CommandError> {
        if cx.project()?.document(p.document).is_none() {
            return Err(CommandError::not_found(format!(
                "no Document has UUID {}",
                p.document
            )));
        }
        remove_document(cx.tx()?, p.document)?;
        Ok(())
    }
}

/// The Document with this UUID, or `not_found`.
fn entry(project: &Project, uuid: Uuid) -> Result<&DocumentEntry, CommandError> {
    project
        .document(uuid)
        .ok_or_else(|| CommandError::not_found(format!("no Document has UUID {uuid}")))
}

pub struct DocumentRename;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentRenameParams {
    /// The Document's UUID.
    pub document: Uuid,
    /// The new title, kept as typed.
    pub title: String,
}

impl Command for DocumentRename {
    const META: Meta = Meta {
        id: "document.rename",
        title: "Rename Document",
        description: "Sets a Document's title. Its file stem becomes the title with the \
                      characters file names cannot hold replaced, made unique by a number \
                      suffix. The file is renamed on save.",
        category: "project",
        icon: "rename",
        kind: Kind::Edit,
    };
    type Params = DocumentRenameParams;
    type Output = DocumentEntry;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: DocumentRenameParams,
    ) -> Result<DocumentEntry, CommandError> {
        if p.title.trim().is_empty() {
            return Err(CommandError::invalid_params("/title", "the title is empty"));
        }
        let project = cx.project()?;
        let old = entry(project, p.document)?;
        let entry = DocumentEntry {
            name: free_name(project, old.kind, &p.title, Some(old.uuid)),
            title: p.title,
            ..old.clone()
        };
        cx.tx()?.replace(Object::Document(entry.clone()))?;
        Ok(entry)
    }
}

pub struct DocumentReorder;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentReorderParams {
    /// The Document's UUID.
    pub document: Uuid,
    /// Its new 0-based position in the Project's Document list; past the end moves it last.
    pub index: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DocumentOrder {
    /// Every Document's UUID in the new order.
    pub documents: Vec<Uuid>,
}

impl Command for DocumentReorder {
    const META: Meta = Meta {
        id: "document.reorder",
        title: "Reorder Documents",
        description: "Moves a Document to `index` in the Project's Document list. The order \
                      of the Schematic Sheets gives \"sheet n of m\" and the PDF page order.",
        category: "project",
        icon: "reorder",
        kind: Kind::Edit,
    };
    type Params = DocumentReorderParams;
    type Output = DocumentOrder;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: DocumentReorderParams,
    ) -> Result<DocumentOrder, CommandError> {
        let project = cx.project()?;
        entry(project, p.document)?;
        let mut props = project.props().clone();
        props.documents.retain(|d| *d != p.document);
        let index = p.index.min(props.documents.len());
        props.documents.insert(index, p.document);
        let documents = props.documents.clone();
        cx.tx()?.replace(Object::Project(props))?;
        Ok(DocumentOrder { documents })
    }
}

pub struct DocumentRevert;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentRevertParams {
    /// The Document's UUID.
    pub document: Uuid,
}

impl Command for DocumentRevert {
    const META: Meta = Meta {
        id: "document.revert",
        title: "Revert Document",
        description: "Drops a Document's unsaved changes: its contents are read again from \
                      its file, and its undo stack is forgotten, leaving it clean. The \
                      Project file and other Documents keep their changes. A Document whose \
                      file was never written is refused.",
        category: "project",
        icon: "undo",
        kind: Kind::File,
    };
    type Params = DocumentRevertParams;
    type Output = DocumentEntry;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            Some(_) => Ok(()),
            None => Err(Unavailable {
                reason: NO_PROJECT.into(),
            }),
        }
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: DocumentRevertParams,
    ) -> Result<DocumentEntry, CommandError> {
        let session = cx.session()?;
        let project = session
            .project
            .as_mut()
            .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
        let entry = entry(project, p.document)?.clone();
        let never_saved = || {
            CommandError::rejected(format!(
                "{} has never been saved, so there is no file to revert to",
                entry.title
            ))
        };
        let (Some(folder), Some(file)) =
            (&session.location, session.files_on_disk.get(&entry.uuid))
        else {
            return Err(never_saved());
        };
        let path = folder.join(file);
        let bytes = match read_bytes(&path) {
            Ok(bytes) => bytes,
            Err(crate::format::LoadError::Missing(_)) => return Err(never_saved()),
            Err(e) => return Err(e.into()),
        };
        let objects = match entry.kind {
            DocumentKind::Schematic => read_sheet_file(&path, &bytes, entry.uuid)?,
            DocumentKind::Pcb => read_pcb_file(&path, &bytes, entry.uuid)?,
        };
        let want: BTreeMap<Uuid, Object> = objects.into_iter().map(|o| (o.uuid(), o)).collect();
        let have: BTreeMap<Uuid, Object> = (project.objects().into_iter())
            .filter(|o| o.scope() == Scope::Document(entry.uuid))
            .map(|o| (o.uuid(), o))
            .collect();
        let mut tx = project.transaction();
        let invalid = |e: reda_model::TxError| {
            CommandError::rejected(format!("{} cannot be read back: {e}", path.display()))
        };
        for uuid in have.keys().filter(|u| !want.contains_key(u)) {
            tx.remove(*uuid).map_err(invalid)?;
        }
        for (uuid, object) in &want {
            match have.get(uuid) {
                None => tx.insert(object.clone()).map_err(invalid)?,
                Some(old) if old != object => tx.replace(object.clone()).map_err(invalid)?,
                Some(_) => {}
            }
        }
        tx.commit().map_err(invalid)?;
        if entry.kind == DocumentKind::Pcb {
            super::zone_fill::load_caches(project, folder);
        }
        session.history.forget(Scope::Document(entry.uuid));
        Ok(entry)
    }
}
