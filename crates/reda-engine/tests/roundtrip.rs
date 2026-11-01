//! Property tests: save → load → save gives the same bytes for every Document type that
//! exists, and hand-reformatted files load as the same Project and save back canonical.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use reda_engine::format::LoadError;
use reda_engine::format::library::{read_footprint, read_part, read_symbol};
use reda_engine::model::geom::{Millideg, Nm, Point, Vertex};
use reda_engine::model::{
    BoardCutout, BoardOutline, BoardSetup, Column, ColumnSource, ComponentRef, ComponentType,
    DocumentEntry, DocumentKind, DrcCheck, DrcSeverities, FabPreset, FileNaming, FootprintCopy,
    GerberSettings, GraphicShape, InDocument, Layer, LayerKind, LayerSet, Net, NetClass, NetLabel,
    NoConnect, Object, OutputJob, Outputs, PartEntry, PcbGraphic, PcbText, Project,
    ProjectFabPreset, ProjectProperties, ProjectProps, Rules, Severity, SheetFrame, SheetSize,
    Side, UnitInstance, Uuid, Wire,
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
    texts: Vec<TextSeed>,
    graphics: Vec<(usize, Nm, GraphicShape)>,
}

/// A text object; the Layer is an index into the seed's Layers.
#[derive(Clone, Debug)]
struct TextSeed {
    layer: usize,
    text: String,
    font: String,
    position: Point,
    height: Nm,
    rotation: Millideg,
    mirrored: bool,
}

fn point() -> impl Strategy<Value = Point> {
    (length(), length()).prop_map(|(x, y)| Point::new(x, y))
}

fn text_seed() -> impl Strategy<Value = TextSeed> {
    (
        any::<usize>(),
        any::<String>(),
        any::<String>(),
        point(),
        length(),
        0i64..360_000,
        any::<bool>(),
    )
        .prop_map(
            |(layer, text, font, position, height, r, mirrored)| TextSeed {
                layer,
                text,
                font,
                position,
                height,
                rotation: Millideg(r),
                mirrored,
            },
        )
}

fn shape() -> impl Strategy<Value = GraphicShape> {
    prop_oneof![
        (point(), point()).prop_map(|(from, to)| GraphicShape::Line { from, to }),
        (point(), point(), -359_999i64..360_000).prop_map(|(from, to, s)| GraphicShape::Arc {
            from,
            to,
            sweep: Millideg(s)
        }),
        (point(), point(), any::<bool>()).prop_map(|(from, to, filled)| GraphicShape::Rect {
            from,
            to,
            filled
        }),
    ]
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
        texts in prop::collection::vec(text_seed(), 0..3),
        graphics in prop::collection::vec((any::<usize>(), length(), shape()), 0..3),
    ) -> PcbSeed {
        PcbSeed {
            base, name, title, rules, severities, outline, cutouts, classes, nets, texts,
            graphics,
        }
    }
}

/// The UUIDs of the Layers in the Layer set among `objects`.
fn layers_of(objects: &[Object]) -> Vec<Uuid> {
    objects
        .iter()
        .find_map(|o| match o {
            Object::LayerSet(set) => Some(set.layers.iter().map(|l| l.uuid).collect()),
            _ => None,
        })
        .expect("the seed has a Layer set")
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
    let layer_ids: Vec<Uuid> = layers_of(&objects);
    for t in seed.texts {
        objects.push(Object::PcbText(InDocument::new(
            doc,
            PcbText {
                uuid: id(),
                layer: layer_ids[t.layer % layer_ids.len()],
                text: t.text,
                font: t.font,
                position: t.position,
                height: t.height,
                thickness: t.height,
                rotation: t.rotation,
                mirrored: t.mirrored,
            },
        )));
    }
    for (layer, width, shape) in seed.graphics {
        objects.push(Object::PcbGraphic(InDocument::new(
            doc,
            PcbGraphic {
                uuid: id(),
                layer: layer_ids[layer % layer_ids.len()],
                width,
                shape,
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

fn column() -> impl Strategy<Value = Column> {
    let source = prop_oneof![
        Just(ColumnSource::Designator),
        Just(ColumnSource::MidX),
        any::<String>().prop_map(ColumnSource::Field),
    ];
    (any::<String>(), source).prop_map(|(header, source)| Column { header, source })
}

prop_compose! {
    /// A Fab Preset copy and an Output Job using it.
    fn settings()(
        preset in uuid(),
        job in uuid(),
        pcb in uuid(),
        name in any::<String>(),
        lengths in prop::array::uniform7(length()),
        x2_attributes in any::<bool>(),
        bom in prop::collection::vec(column(), 0..4),
        cpl in prop::collection::vec(column(), 0..3),
        job_name in stem("j"),
        outputs in prop::array::uniform4(any::<bool>()),
        destination in prop::option::of(stem("d")),
    ) -> (ProjectFabPreset, OutputJob) {
        let [clearance, track_width, via_diameter, via_hole, annular_ring, hole_to_hole, edge_clearance] = lengths;
        let rules = Rules { clearance, track_width, via_diameter, via_hole, annular_ring, hole_to_hole, edge_clearance };
        let gerber = GerberSettings { x2_attributes, naming: FileNaming::Protel };
        let copy = ProjectFabPreset::new(preset, FabPreset { name, rules, gerber, bom, cpl });
        let [gerber, drill, bom, cpl] = outputs;
        let job = OutputJob {
            uuid: job,
            name: job_name,
            pcb,
            fab_preset: preset,
            outputs: Outputs { gerber, drill, bom, cpl },
            destination,
        };
        (copy, job)
    }
}

const SYMBOL: &str = r#"{"format":1,"symbol":{"uuid":"a0000000-0000-4000-8000-000000000010","name":"R","designator_prefix":"R"},"units":[{"uuid":"a0000000-0000-4000-8000-000000000011","name":"A"}],"pins":[{"uuid":"a0000000-0000-4000-8000-000000000012","unit":"a0000000-0000-4000-8000-000000000011","name":"~","number":"1","electrical_type":"passive","at":{"x":0,"y":3810000},"rotation":270000,"length":1270000}]}"#;
const FOOTPRINT: &str = r#"{"format":1,"footprint":{"uuid":"a0000000-0000-4000-8000-000000000040","name":"R_0603"},"pads":[{"uuid":"a0000000-0000-4000-8000-000000000041","number":"1","kind":"smd","shape":"rect","at":{"x":-800000,"y":0},"rotation":0,"width":800000,"height":950000}]}"#;
const PART: &str = r#"{"format":1,"part":{"uuid":"a0000000-0000-4000-8000-000000000100","name":"R_10k","component_type":"standard","symbol":"a0000000-0000-4000-8000-000000000010"},"footprints":["a0000000-0000-4000-8000-000000000040"],"fields":{"value":"10k"},"pin_map":{"unconnected":["2"]}}"#;

/// What a Schematic Sheet holds besides its entry: an optional frame and an optional
/// placed Component. Its UUIDs are `base`, `base + 1`, …
#[derive(Clone, Debug)]
struct SheetSeed {
    base: u128,
    size: Option<usize>,
    component: Option<(String, String, Nm, Nm, bool)>,
    drawing: Option<(String, Nm, Nm, Nm)>,
}

prop_compose! {
    fn sheet_seed()(
        base in any::<u128>(),
        size in prop::option::of(0usize..3),
        component in prop::option::of(
            (any::<String>(), any::<String>(), length(), length(), any::<bool>())
        ),
        drawing in prop::option::of((any::<String>(), length(), length(), length())),
    ) -> SheetSeed {
        SheetSeed { base, size, component, drawing }
    }
}

fn sheet_objects(sheet: Uuid, seed: SheetSeed) -> Vec<Object> {
    let id = |k: u128| Uuid::v4_from_bits(seed.base.wrapping_add(k));
    let mut objects = vec![];
    if let Some(size) = seed.size {
        let size = [SheetSize::A4, SheetSize::Letter, SheetSize::A0][size];
        objects.push(Object::SheetFrame(InDocument::new(
            sheet,
            SheetFrame { uuid: id(0), size },
        )));
    }
    if let Some((value, designator, x, y, mirror)) = seed.component {
        let path = Path::new("x");
        let footprint = read_footprint(path, FOOTPRINT.as_bytes()).unwrap();
        let mut part = read_part(path, PART.as_bytes()).unwrap();
        part.fields.value = value;
        let symbol = read_symbol(path, SYMBOL.as_bytes()).unwrap();
        let copy = FootprintCopy {
            uuid: id(1),
            hash: "f".repeat(32),
            footprint,
        };
        let entry = PartEntry {
            uuid: id(2),
            hash: "0123456789abcdef".repeat(2),
            footprints: vec![copy.uuid],
            part,
            symbol,
        };
        let unit = UnitInstance {
            uuid: id(3),
            component: ComponentRef::new(id(4)),
            part: entry.uuid,
            unit: entry.symbol.units[0].uuid,
            at: Point::new(x, y),
            rotation: Millideg(90_000),
            mirror,
            designator,
            fields: entry.part.fields.clone(),
            footprint: entry.part.footprints.first().copied(),
            component_type: ComponentType::Standard,
            not_fitted: mirror,
            visible_fields: if mirror {
                ["designator".to_owned(), "LCSC".to_owned()].into()
            } else {
                UnitInstance::default_visible_fields()
            },
        };
        objects.push(Object::FootprintCopy(InDocument::new(sheet, copy)));
        objects.push(Object::PartEntry(InDocument::new(sheet, entry)));
        objects.push(Object::Unit(InDocument::new(sheet, unit)));
    }
    if let Some((name, x, y, len)) = seed.drawing {
        let from = Point::new(x, y);
        let to = Point::new(x, y + Nm(len.0.abs() + 1));
        let name = if name.trim().is_empty() {
            "N".into()
        } else {
            name
        };
        objects.push(Object::Wire(InDocument::new(
            sheet,
            Wire {
                uuid: id(5),
                from,
                to,
            },
        )));
        objects.push(Object::NetLabel(InDocument::new(
            sheet,
            NetLabel {
                uuid: id(6),
                name,
                at: to,
                rotation: Millideg(180_000),
            },
        )));
        objects.push(Object::NoConnect(InDocument::new(
            sheet,
            NoConnect {
                uuid: id(7),
                at: from,
            },
        )));
    }
    objects
}

prop_compose! {
    fn project()(
        uuid in uuid(),
        name in stem("p"),
        documents in prop::collection::vec((document(), sheet_seed()), 0..6),
        pcb in prop::option::of(pcb_seed()),
        libraries in prop::collection::vec(any::<String>(), 0..3),
        settings in prop::option::of(settings()),
        properties in prop::collection::vec(any::<String>(), 5),
    ) -> Project {
        let [title, revision, date, author, company] = <[String; 5]>::try_from(properties).unwrap();
        let properties = ProjectProperties { title, revision, date, author, company };
        let mut project = Project::new(ProjectProps { uuid, name, documents: vec![], libraries, properties });
        let mut tx = project.transaction();
        let mut order = vec![];
        let mut names = std::collections::BTreeSet::new();
        let mut contents = vec![];
        let mut used = std::collections::BTreeSet::from([uuid]);
        for (entry, seed) in documents {
            // UUIDs and file names (case-insensitively, for macOS and Windows) are unique.
            let objects = sheet_objects(entry.uuid, seed);
            if used.contains(&entry.uuid)
                || objects.iter().any(|o| used.contains(&o.uuid()) || o.uuid() == entry.uuid)
                || !names.insert(entry.name.to_lowercase())
            {
                continue;
            }
            used.insert(entry.uuid);
            used.extend(objects.iter().map(Object::uuid));
            order.push(entry.uuid);
            tx.insert(Object::Document(entry)).unwrap();
            contents.extend(objects);
        }
        if let Some(seed) = pcb {
            let (entry, objects) = pcb_objects(seed);
            let taken = |u: Uuid| used.contains(&u);
            if !taken(entry.uuid) && !objects.iter().any(|o| taken(o.uuid())) {
                order.push(entry.uuid);
                tx.insert(Object::Document(entry)).unwrap();
                contents.extend(objects);
            }
        }
        if let Some((copy, job)) = settings {
            let mut taken: Vec<Uuid> = contents.iter().map(Object::uuid).collect();
            taken.push(uuid);
            taken.extend(&order);
            if copy.uuid != job.uuid && !taken.contains(&copy.uuid) && !taken.contains(&job.uuid) {
                contents.push(Object::FabPreset(copy));
                contents.push(Object::OutputJob(job));
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
        properties: Default::default(),
    });
    assert_eq!(save(&project)[".reda/.gitignore"], b"*\n");
}
