//! `document.add` and `document.delete`: Documents come and go on the Project's own undo
//! stack, each with its contents.

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::{DocumentEntry, DocumentKind, Project, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::project::check_file_stem;
use crate::builders::{add_document, remove_document};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(DocumentAdd);
    registry.add(DocumentDelete);
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
                      rules and DRC severities and a Default Net Class, and is available only \
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
        let title = match p.title {
            Some(title) if title.trim().is_empty() => {
                return Err(CommandError::invalid_params("/title", "the title is empty"));
            }
            Some(title) => title,
            None => default_title(project, p.kind),
        };
        let name = free_name(project, p.kind, &title);
        let uuid = cx.new_id();
        let contents = match p.kind {
            DocumentKind::Schematic => vec![],
            DocumentKind::Pcb => {
                reda_pcb::new_board(uuid, reda_output::generic().rules, || cx.new_id())
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

fn default_title(project: &Project, kind: DocumentKind) -> String {
    match kind {
        DocumentKind::Schematic => {
            let sheets = project.documents().filter(|d| d.kind == kind).count();
            format!("Sheet {}", sheets + 1)
        }
        DocumentKind::Pcb => project.props().name.clone(),
    }
}

/// A file stem for a Document titled `title`: the title with characters file names cannot
/// hold replaced, made unique among the Documents of the same kind by a number suffix.
fn free_name(project: &Project, kind: DocumentKind, title: &str) -> String {
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
                      Project's stack. Its file is left on disk.",
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
