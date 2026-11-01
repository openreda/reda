//! Property tests: save → load → save gives the same bytes for every Document type that
//! exists, and hand-reformatted files load as the same Project and save back canonical.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use reda_engine::format::LoadError;
use reda_engine::model::{DocumentEntry, DocumentKind, Object, Project, ProjectProps, Uuid};
use reda_engine::store::{encode, load_from};
use serde_json::Value;

/// A file stem that is a valid, portable file name: a fixed first letter keeps clear of
/// Windows device names such as `CON`.
fn stem(first: &'static str) -> impl Strategy<Value = String> {
    "[a-z0-9_ -]{0,10}".prop_map(move |rest| format!("{first}{rest}"))
}

fn uuid() -> impl Strategy<Value = Uuid> {
    any::<u128>().prop_map(Uuid::v4_from_bits)
}

prop_compose! {
    fn document()(uuid in uuid(), name in stem("s"), title in any::<String>()) -> DocumentEntry {
        DocumentEntry { uuid, kind: DocumentKind::Schematic, name, title }
    }
}

prop_compose! {
    fn project()(
        uuid in uuid(),
        name in stem("p"),
        documents in prop::collection::vec(document(), 0..6),
        libraries in prop::collection::vec(any::<String>(), 0..3),
    ) -> Project {
        let mut project = Project::new(ProjectProps { uuid, name, documents: vec![], libraries });
        let mut tx = project.transaction();
        let mut order = vec![];
        let mut names = std::collections::BTreeSet::new();
        for entry in documents {
            // UUIDs and file names (case-insensitively, for macOS and Windows) are unique.
            if entry.uuid == uuid || order.contains(&entry.uuid)
                || !names.insert(entry.name.to_lowercase())
            {
                continue;
            }
            order.push(entry.uuid);
            tx.insert(Object::Document(entry)).unwrap();
        }
        let mut props = tx.project().props().clone();
        props.documents = order;
        tx.replace(Object::Project(props)).unwrap();
        tx.commit().unwrap();
        project
    }
}

type Files = BTreeMap<String, Vec<u8>>;

fn save(project: &Project) -> Files {
    encode(project).unwrap().into_iter().collect()
}

fn load(project: &Project, files: &Files) -> Result<Project, LoadError> {
    let folder = PathBuf::from("Demo");
    let project_file = folder.join(format!("{}.reda", project.props().name));
    load_from(&project_file, |path: &Path| {
        let rel = path.strip_prefix(&folder).unwrap().to_str().unwrap();
        files
            .get(rel)
            .cloned()
            .ok_or_else(|| LoadError::Missing(path.to_owned()))
    })
    .map(|(project, _)| project)
}

/// Rewrites a file the way a hand edit might: keys reordered, pretty-printed, UUIDs in
/// upper case.
fn reformat(bytes: &[u8]) -> Vec<u8> {
    fn upper(value: &mut Value) {
        match value {
            Value::String(s) if s.parse::<Uuid>().is_ok() => *s = s.to_uppercase(),
            Value::Array(items) => items.iter_mut().for_each(upper),
            Value::Object(map) => map.values_mut().for_each(upper),
            _ => {}
        }
    }
    let mut value: Value = serde_json::from_slice(bytes).unwrap();
    upper(&mut value);
    serde_json::to_vec_pretty(&value).unwrap()
}

proptest! {
    #[test]
    fn save_load_save_is_byte_identical(project in project()) {
        let first = save(&project);
        let loaded = load(&project, &first).unwrap();
        prop_assert_eq!(&loaded, &project);
        prop_assert_eq!(save(&loaded), first);
    }

    #[test]
    fn hand_reformatted_files_load_the_same_and_save_canonical(project in project()) {
        let canonical = save(&project);
        let messy: Files = canonical
            .iter()
            .map(|(rel, bytes)| {
                let bytes = if rel.starts_with(".reda/") { bytes.clone() } else { reformat(bytes) };
                (rel.clone(), bytes)
            })
            .collect();
        let loaded = load(&project, &messy).unwrap();
        prop_assert_eq!(&loaded, &project);
        prop_assert_eq!(save(&loaded), canonical);
    }
}

#[test]
fn every_saved_project_includes_the_state_folder_gitignore() {
    let project = Project::new(ProjectProps {
        uuid: Uuid::from_u128(1),
        name: "Demo".into(),
        documents: vec![],
        libraries: vec![],
    });
    assert_eq!(save(&project)[".reda/.gitignore"], b"*\n");
}
