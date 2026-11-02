//! Loading chosen snapshots back into the open Project as unsaved changes.
//!
//! The restored Project is built by reading the Project as `store::open_from` would, with
//! each chosen snapshot standing in for its file and the open Project's own content for
//! every other file. The difference between the open Project and that one is then made in
//! one transaction, so the restore lands on the undo stacks like any edit and leaves each
//! restored file dirty.

use std::collections::BTreeMap;
use std::path::Path;

use reda_command::{CommandError, NO_PROJECT, Session};
use reda_model::{ElementSource, Object, Project, Scope, Transaction, Uuid};

use super::{Parsed, Snapshot, manifest_rows, parse_snapshot};
use crate::builders::add_document;
use crate::format::{LoadError, read_bytes, read_pcb_file, read_sheet_file};
use crate::store;

/// The undo label of a restore.
const LABEL: &str = "Restore Recovery Snapshot";

/// Restores the snapshots `files` from `dir` into the session's Project. Returns the
/// scopes restored.
pub(crate) fn restore(
    session: &mut Session,
    dir: &Path,
    files: &[String],
) -> Result<Vec<Scope>, CommandError> {
    let rows = manifest_rows(dir).map_err(CommandError::rejected)?;
    let mut parsed = Vec::new();
    for file in files {
        let row: &Snapshot = rows.get(file).ok_or_else(|| {
            CommandError::not_found(format!("the recovery folder has no snapshot {file:?}"))
        })?;
        let snapshot = parse_snapshot(dir, row)
            .map_err(|why| CommandError::rejected(format!("{file} cannot be restored: {why}")))?;
        parsed.push((row.clone(), snapshot));
    }
    let project = session
        .project
        .as_ref()
        .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
    let folder = session
        .location
        .clone()
        .ok_or_else(|| CommandError::unavailable("the Project has no folder yet"))?;

    let target = restored_project(project, &folder, &parsed)?;
    let mut elements = Vec::new();
    for (row, snapshot) in &parsed {
        if let (Parsed::Element(uuid, element), super::Target::Element { library, .. }) =
            (snapshot, &row.target)
        {
            elements.push((*uuid, *library, (**element).clone()));
        }
    }

    let project = session.project.as_mut().expect("checked above");
    let mut tx = project.transaction();
    make_equal(&mut tx, &target).map_err(|e| CommandError::internal(e.to_string()))?;
    let mut newly_open = Vec::new();
    for (uuid, library, element) in elements {
        if tx.project().element(uuid).is_some() {
            tx.replace(element.into())
                .map_err(|e| CommandError::internal(e.to_string()))?;
        } else {
            newly_open.push((uuid, library, element));
        }
    }
    let changes = tx
        .commit()
        .map_err(|e| CommandError::rejected(format!("the snapshots do not fit together: {e}")))?;
    session.history.record(LABEL, &changes);
    for (uuid, library, element) in newly_open {
        let source = ElementSource {
            library,
            read_only: false,
        };
        let project = session.project.as_mut().expect("checked above");
        project
            .open_element(source, element)
            .map_err(CommandError::conflict)?;
        session.history.forget(Scope::Element(uuid));
        session.history.mark_unsaved(Scope::Element(uuid));
    }
    Ok(parsed
        .iter()
        .filter_map(|(row, _)| row.target.scope())
        .collect())
}

/// The Project as it is with the chosen snapshots restored (Library elements aside).
fn restored_project(
    project: &Project,
    folder: &Path,
    parsed: &[(Snapshot, Parsed)],
) -> Result<Project, CommandError> {
    let fail = |e: String| CommandError::internal(format!("encoding the open Project: {e}"));
    let mut files: BTreeMap<String, Vec<u8>> = store::encode(project)
        .map_err(|e| fail(e.to_string()))?
        .into_iter()
        .collect();
    let project_file = store::project_file_path(folder, &project.props().name);
    let project_name = format!("{}.{}", project.props().name, store::PROJECT_EXTENSION);
    let mut new_documents = Vec::new();
    for (row, snapshot) in parsed {
        match snapshot {
            Parsed::Project(bytes) => {
                files.insert(project_name.clone(), bytes.clone());
            }
            Parsed::Document(entry, bytes) => {
                files.insert(entry.file_name(), bytes.clone());
                new_documents.push((entry.clone(), row.file.clone(), bytes.clone()));
            }
            Parsed::Element(..) => {}
        }
    }
    let read = |path: &Path| -> Result<Vec<u8>, LoadError> {
        let rel = path
            .strip_prefix(folder)
            .ok()
            .and_then(|p| p.to_str())
            .map(|p| p.replace('\\', "/"));
        match rel.and_then(|rel| files.get(&rel)) {
            Some(bytes) => Ok(bytes.clone()),
            None => read_bytes(path),
        }
    };
    let mut opened = store::open_from(&project_file, read)
        .map_err(|e| CommandError::rejected(format!("the restored Project cannot be read: {e}")))?;

    // A restored Document that the Project file does not list (it was added and never
    // saved) comes back at the end of the Document order.
    let mut tx = opened.project.transaction();
    for (entry, file, bytes) in new_documents {
        if tx.project().document(entry.uuid).is_some() {
            continue;
        }
        let path = Path::new(&file);
        let objects = match entry.kind {
            reda_model::DocumentKind::Schematic => read_sheet_file(path, &bytes, entry.uuid),
            reda_model::DocumentKind::Pcb => read_pcb_file(path, &bytes, entry.uuid),
        }
        .map_err(|e| CommandError::rejected(e.to_string()))?;
        let invalid = |e: reda_model::TxError| CommandError::rejected(e.to_string());
        add_document(&mut tx, entry).map_err(invalid)?;
        for object in objects {
            tx.insert(object).map_err(invalid)?;
        }
    }
    tx.commit()
        .map_err(|e| CommandError::rejected(format!("the snapshots do not fit together: {e}")))?;
    Ok(opened.project)
}

/// Makes the transaction's Project hold the same objects as `target` (Library elements
/// aside), in the order a change set applies safely: Project-file objects first (so new
/// Documents exist), then Document contents, then removed Project-file objects (so a
/// removed Document is empty first).
fn make_equal(tx: &mut Transaction<'_>, target: &Project) -> Result<(), reda_model::TxError> {
    let objects = |p: &Project| -> BTreeMap<Uuid, Object> {
        p.objects()
            .into_iter()
            .filter(|o| !matches!(o.scope(), Scope::Element(_)))
            .map(|o| (o.uuid(), o))
            .collect()
    };
    let have = objects(tx.project());
    let want = objects(target);
    let upsert = |tx: &mut Transaction<'_>, uuid: &Uuid, object: &Object| match have.get(uuid) {
        None => tx.insert(object.clone()),
        Some(old) if old != object => tx.replace(object.clone()),
        Some(_) => Ok(()),
    };
    let in_project = |o: &Object| o.scope() == Scope::Project;
    for (uuid, object) in want.iter().filter(|(_, o)| in_project(o)) {
        upsert(tx, uuid, object)?;
    }
    for uuid in have.keys().filter(|u| !in_project(&have[u])) {
        if !want.contains_key(uuid) {
            tx.remove(*uuid)?;
        }
    }
    for (uuid, object) in want.iter().filter(|(_, o)| !in_project(o)) {
        upsert(tx, uuid, object)?;
    }
    for uuid in have.keys().filter(|u| in_project(&have[u])) {
        if !want.contains_key(uuid) {
            tx.remove(*uuid)?;
        }
    }
    Ok(())
}
