//! `document.relink`: gives a missing Document (one the Project file lists whose file is
//! gone) a file again, as one undoable step on the Project's stack.

use std::path::{Path, PathBuf};

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, Registry,
    Unavailable,
};
use reda_model::{DocumentEntry, DocumentKind, Object, Scope, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::format::{FileKind, LoadError, migrate, read_bytes, read_pcb_file, read_sheet_file};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(DocumentRelink);
}

pub struct DocumentRelink;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentRelinkParams {
    /// The missing Document's UUID.
    pub document: Uuid,
    /// The Document file (`.redasch` or `.redapcb`, matching the Document's kind) to read
    /// its contents from.
    pub path: PathBuf,
    /// Takes a file that holds another Document. Without it, such a file is a `conflict`
    /// whose details say `confirm: true`, so the caller can ask the user first.
    #[serde(default)]
    pub confirm: bool,
}

impl Command for DocumentRelink {
    const META: Meta = Meta {
        id: "document.relink",
        title: "Relink Document",
        description: "Gives a missing Document a file again: reads `path` into it and puts it \
                      back in its place in the Project, as one undoable step on the \
                      Project's stack. A file in the Project folder becomes the Document's \
                      file (its stem the Document's name); a file elsewhere is read in and \
                      written into the Project folder on save. A file that holds another \
                      Document is taken only with `confirm`, keeping this Document's UUID.",
        category: "project",
        icon: "document_open",
        kind: Kind::File,
    };
    type Params = DocumentRelinkParams;
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
        p: DocumentRelinkParams,
    ) -> Result<DocumentEntry, CommandError> {
        let session = cx.session()?;
        let project =
            (session.project.as_mut()).ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
        let Some(missing) = project.missing_document(p.document).cloned() else {
            return Err(match project.document(p.document) {
                Some(entry) => CommandError::rejected(format!(
                    "{} is not missing: its file is open",
                    entry.title
                )),
                None => CommandError::not_found(format!("no Document has UUID {}", p.document)),
            });
        };
        let kind =
            (p.path.extension().and_then(|e| e.to_str())).and_then(DocumentKind::from_extension);
        if kind != Some(missing.kind) {
            return Err(CommandError::invalid_params(
                "/path",
                format!(
                    "{} is a {} Document, so its file ends in .{}",
                    missing.title,
                    kind_name(missing.kind),
                    missing.kind.extension()
                ),
            ));
        }
        if missing.kind == DocumentKind::Pcb && project.pcb().is_some() {
            return Err(CommandError::rejected(
                "the Project already has a PCB Document",
            ));
        }
        let folder = session.location.clone().unwrap_or_default();
        let in_folder = in_folder(&p.path, &folder);
        let stem = in_folder
            .then(|| {
                p.path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_owned)
            })
            .flatten();
        if let Some(stem) = &stem
            && let Some(owner) = project
                .documents()
                .find(|d| d.kind == missing.kind && d.name.to_lowercase() == stem.to_lowercase())
        {
            return Err(CommandError::rejected(format!(
                "{} is the file of {}",
                p.path.display(),
                owner.title
            )));
        }

        let mut bytes = match read_bytes(&p.path) {
            Ok(bytes) => bytes,
            Err(LoadError::Missing(_)) => {
                return Err(CommandError::not_found(format!(
                    "{} does not exist",
                    p.path.display()
                )));
            }
            Err(e) => return Err(e.into()),
        };
        let held = held_document(missing.kind, &bytes);
        let other = held.is_some_and(|held| held != missing.uuid);
        if other {
            if !p.confirm {
                let mut err = CommandError::conflict(format!(
                    "{} holds another Document, not {}; relink with `confirm` to take it",
                    p.path.display(),
                    missing.title
                ));
                err.details = Some(json!({ "confirm": true, "holds": held }));
                return Err(err);
            }
            bytes = with_document(missing.kind, &bytes, missing.uuid).unwrap_or(bytes);
        }
        let objects = match missing.kind {
            DocumentKind::Schematic => read_sheet_file(&p.path, &bytes, missing.uuid)?,
            DocumentKind::Pcb => read_pcb_file(&p.path, &bytes, missing.uuid)?,
        };

        let entry = DocumentEntry {
            name: stem.clone().unwrap_or_else(|| missing.name.clone()),
            ..missing.clone()
        };
        let mut props = project.props().clone();
        props.missing.retain(|m| m.uuid != missing.uuid);
        let mut tx = project.transaction();
        let refused = |e: reda_model::TxError| {
            CommandError::rejected(format!(
                "{} does not fit the Project: {e}",
                p.path.display()
            ))
        };
        tx.insert(Object::Document(entry.clone()))
            .map_err(refused)?;
        for object in objects {
            tx.insert(object).map_err(refused)?;
        }
        tx.replace(Object::Project(props)).map_err(refused)?;
        let changes = tx.commit().map_err(refused)?;
        session.history.record(Self::META.title, &changes);
        if stem.is_some() && !other {
            // The Document is just as its file holds it.
            session.history.mark_saved(Scope::Document(entry.uuid));
            session.files_on_disk.insert(entry.uuid, entry.file_name());
        }
        if entry.kind == DocumentKind::Pcb {
            super::zone_fill::load_caches(project, &folder);
        }
        Ok(entry)
    }
}

fn kind_name(kind: DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Schematic => "Schematic",
        DocumentKind::Pcb => "PCB",
    }
}

/// Whether `path` is a file directly inside `folder`, where Documents' files live.
fn in_folder(path: &Path, folder: &Path) -> bool {
    let same = |a: &Path, b: &Path| match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    };
    path.parent().is_some_and(|parent| same(parent, folder))
}

/// The header that names the Document a file of `kind` holds.
fn header(kind: DocumentKind) -> (FileKind, &'static str) {
    match kind {
        DocumentKind::Schematic => (FileKind::Sheet, "sheet"),
        DocumentKind::Pcb => (FileKind::Pcb, "pcb"),
    }
}

/// The UUID of the Document a file of `kind` holds, if it can be read.
fn held_document(kind: DocumentKind, bytes: &[u8]) -> Option<Uuid> {
    let (file, key) = header(kind);
    let value = migrate(file, serde_json::from_slice(bytes).ok()?).ok()?;
    value[key]["uuid"].as_str()?.parse().ok()
}

/// The file's bytes with the Document it holds renamed to `uuid`.
fn with_document(kind: DocumentKind, bytes: &[u8], uuid: Uuid) -> Option<Vec<u8>> {
    let (file, key) = header(kind);
    let mut value: Value = migrate(file, serde_json::from_slice(bytes).ok()?).ok()?;
    *value.get_mut(key)?.get_mut("uuid")? = Value::String(uuid.to_string());
    serde_json::to_vec(&value).ok()
}
