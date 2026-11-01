//! Property tests: save → load → save gives the same bytes for every Document type that
//! exists, and hand-reformatted files load as the same Project and save back canonical.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use reda_engine::format::LoadError;
use reda_engine::model::geom::{Millideg, Nm, Vertex};
use reda_engine::model::{
    BoardCutout, BoardOutline, BoardSetup, DocumentEntry, DocumentKind, DrcCheck, DrcSeverities,
    InDocument, Layer, LayerKind, LayerSet, Net, NetClass, Object, Project, ProjectProps, Rules,
    Severity, Side, Uuid,
};
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

fn length() -> impl Strategy<Value = Nm> {
    (-1_000_000_000i64..1_000_000_000).prop_map(Nm)
}

fn vertices() -> impl Strategy<Value = Vec<Vertex>> {
    let sweep = prop_oneof![Just(0i64), -359_999i64..360_000];
    prop::collection::vec((length(), length(), sweep), 3..7).prop_map(|vs| {
        vs.into_iter()
            .map(|(x, y, s)| Vertex {
                x,
                y,
                sweep: Millideg(s),
            })
            .collect()
    })
}

/// The data of a PCB Document. Its UUIDs are `base`, `base + 1`, …
#[derive(Clone, Debug)]
struct PcbSeed {
    base: u128,
    name: String,
    title: String,
    rules: [Nm; 7],
    severities: Vec<u8>,
    outline: Option<Vec<Vertex>>,
    cutouts: Vec<Vec<Vertex>>,
    classes: Vec<(String, Option<Nm>, Option<Nm>)>,
    nets: Vec<(String, usize)>,
}

prop_compose! {
    fn pcb_seed()(
        base in any::<u128>(),
        name in stem("b"),
        title in any::<String>(),
        rules in prop::array::uniform7(length()),
        severities in prop::collection::vec(0u8..3, DrcCheck::ALL.len()),
        outline in prop::option::of(vertices()),
        cutouts in prop::collection::vec(vertices(), 0..3),
        classes in prop::collection::vec(
            ("[A-Za-z0-9 ]{1,8}", prop::option::of(length()), prop::option::of(length())),
            0..3,
        ),
        nets in prop::collection::vec((any::<String>(), any::<usize>()), 0..4),
    ) -> PcbSeed {
        PcbSeed { base, name, title, rules, severities, outline, cutouts, classes, nets }
    }
}

/// The PCB Document's entry and objects, with UUIDs counted up from the seed's base.
fn pcb_objects(seed: PcbSeed) -> (DocumentEntry, Vec<Object>) {
    let mut next = seed.base;
    let mut id = || {
        next = next.wrapping_add(1);
        Uuid::v4_from_bits(next)
    };
    let doc = id();
    let layers = LayerSet {
        uuid: id(),
        layers: [
            (LayerKind::Copper, Some(Side::Top)),
            (LayerKind::Copper, Some(Side::Bottom)),
            (LayerKind::BoardOutline, None),
        ]
        .into_iter()
        .map(|(kind, side)| Layer {
            uuid: id(),
            kind,
            side,
        })
        .collect(),
    };
    let default = NetClass {
        uuid: id(),
        name: "Default".into(),
        clearance: None,
        track_width: None,
        via_diameter: None,
        via_hole: None,
    };
    let mut classes = vec![default.clone()];
    for (name, clearance, track_width) in seed.classes {
        if classes
            .iter()
            .any(|c| c.name.to_lowercase() == name.to_lowercase())
            || name.trim().is_empty()
        {
            continue;
        }
        classes.push(NetClass {
            uuid: id(),
            name,
            clearance,
            track_width,
            via_diameter: None,
            via_hole: None,
        });
    }
    let [
        clearance,
        track_width,
        via_diameter,
        via_hole,
        annular_ring,
        hole_to_hole,
        edge_clearance,
    ] = seed.rules;
    let mut severities = DrcSeverities::DEFAULT;
    for (check, s) in DrcCheck::ALL.iter().zip(seed.severities) {
        severities.set(
            *check,
            [Severity::Error, Severity::Warning, Severity::Off][usize::from(s)],
        );
    }
    let setup = BoardSetup {
        uuid: id(),
        default_net_class: default.uuid,
        rules: Rules {
            clearance,
            track_width,
            via_diameter,
            via_hole,
            annular_ring,
            hole_to_hole,
            edge_clearance,
        },
        severities,
    };
    let mut objects = vec![
        Object::LayerSet(InDocument::new(doc, layers)),
        Object::BoardSetup(InDocument::new(doc, setup)),
    ];
    if let Some(vertices) = seed.outline {
        objects.push(Object::BoardOutline(InDocument::new(
            doc,
            BoardOutline {
                uuid: id(),
                vertices,
            },
        )));
    }
    for vertices in seed.cutouts {
        objects.push(Object::BoardCutout(InDocument::new(
            doc,
            BoardCutout {
                uuid: id(),
                vertices,
            },
        )));
    }
    for (name, class) in seed.nets {
        let class = classes[class % classes.len()].uuid;
        objects.push(Object::Net(InDocument::new(
            doc,
            Net {
                uuid: id(),
                name,
                class,
            },
        )));
    }
    objects.extend(
        classes
            .into_iter()
            .map(|c| Object::NetClass(InDocument::new(doc, c))),
    );
    let entry = DocumentEntry {
        uuid: doc,
        kind: DocumentKind::Pcb,
        name: seed.name,
        title: seed.title,
    };
    (entry, objects)
}

prop_compose! {
    fn project()(
        uuid in uuid(),
        name in stem("p"),
        documents in prop::collection::vec(document(), 0..6),
        pcb in prop::option::of(pcb_seed()),
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
        let mut contents = vec![];
        if let Some(seed) = pcb {
            let (entry, objects) = pcb_objects(seed);
            let taken = |u: Uuid| u == uuid || order.contains(&u);
            if !taken(entry.uuid) && !objects.iter().any(|o| taken(o.uuid())) {
                order.push(entry.uuid);
                tx.insert(Object::Document(entry)).unwrap();
                contents = objects;
            }
        }
        for object in contents {
            tx.insert(object).unwrap();
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
