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
    CopperEntry, DielectricEntry, DocumentEntry, DocumentKind, DrcCheck, DrcSeverities, ErcCheck,
    ErcSeverities, FabPreset, FileNaming, FootprintCopy, FootprintInstance, FrameSize, FreePad,
    GerberSettings, GraphicShape, Hole, InDocument, Justify, Keepout, Layer, LayerKind, LayerSet,
    LayerSpan, Net, NetClass, NetLabel, NoConnect, Object, Orientation, OutputJob, OutputOrigin,
    Outputs, PadConnection, PadKind, PadNet, PadShape, PartEntry, PcbGraphic, PcbText, Port,
    PortDirection, PowerPort, PowerPortStyle, Project, ProjectFabPreset, ProjectProperties,
    ProjectProps, Severity, SheetFrame, SheetGraphic, SheetShape, SheetSize, SheetText, Side,
    Stackup, StackupEntry, Track, TrackGeometry, UnitInstance, Uuid, Via, Waiver, Wire, Zone,
};
use reda_engine::model::{DesignRule, FabRules, PreferredSeeds, RuleKind, RuleScope, RuleValues};
use reda_engine::store::{encode, load_from};
use serde_json::Value;

/// A file stem that is a valid, portable file name: a fixed first letter keeps clear of
/// Windows device names such as `CON`.
fn stem(first: &'static str) -> impl Strategy<Value = String> {
    "[a-z0-9_ -]{0,10}".prop_map(move |rest| format!("{first}{rest}"))
}

/// Any stored text: any string, escaped, since an unescaped `${` is refused.
fn stored_text() -> impl Strategy<Value = String> {
    any::<String>().prop_map(|s| reda_engine::model::text::escape(&s).into_owned())
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
    /// A Footprint instance: Designator, position, linked and flags, first Pad's Net index.
    footprint: Option<(String, Point, bool, usize)>,
    /// A Track and a Via: ends, an arc's sweep (0 for a line), width, Net index (None for
    /// no Net), bottom Layer.
    copper: Option<(Point, Point, i64, Nm, Option<usize>, bool)>,
    /// The Stackup: copper per Layer, the dielectric (None for blank), finish and colours.
    stackup: ([Nm; 2], Option<Nm>, [Option<String>; 3]),
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
        stored_text(),
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
        footprint in prop::option::of((any::<String>(), point(), any::<bool>(), any::<usize>())),
        copper in prop::option::of((
            point(), point(), prop_oneof![Just(0i64), -359_999i64..360_000],
            (1i64..10_000_000).prop_map(Nm), prop::option::of(any::<usize>()), any::<bool>(),
        )),
        stackup in (
            prop::array::uniform2((1i64..200_000).prop_map(Nm)),
            prop::option::of((1i64..5_000_000).prop_map(Nm)),
            prop::array::uniform3(prop::option::of(any::<String>())),
        ),
    ) -> PcbSeed {
        PcbSeed {
            base, name, title, rules, severities, outline, cutouts, classes, nets, texts,
            graphics, footprint, copper, stackup,
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
    let mut classes: Vec<NetClass> = Vec::new();
    let mut rules: Vec<DesignRule> = Vec::new();
    for (name, clearance, track_width) in seed.classes {
        if classes
            .iter()
            .any(|c| c.name.to_lowercase() == name.to_lowercase())
            || name.trim().is_empty()
            || name.trim().eq_ignore_ascii_case("default")
        {
            continue;
        }
        let class = NetClass {
            uuid: id(),
            name,
            rank: classes.len() as u32 + 1,
        };
        let records = [
            (
                RuleKind::Clearance,
                clearance.map(|value| RuleValues {
                    value: Some(value),
                    ..RuleValues::default()
                }),
            ),
            (
                RuleKind::TrackWidth,
                track_width.map(|preferred| RuleValues {
                    preferred: Some(preferred),
                    ..RuleValues::default()
                }),
            ),
        ];
        for (kind, values) in records {
            if let Some(values) = values {
                rules.push(DesignRule {
                    uuid: id(),
                    kind,
                    scope: RuleScope::class(class.uuid),
                    values,
                    enabled: track_width.is_some(),
                    comment: clearance.map(|c| format!("{c:?}")),
                });
            }
        }
        classes.push(class);
    }
    let mut severities = DrcSeverities::DEFAULT;
    for (check, s) in DrcCheck::ALL.iter().zip(seed.severities) {
        severities.set(
            *check,
            [Severity::Error, Severity::Warning, Severity::Off][usize::from(s)],
        );
    }
    let setup = BoardSetup {
        uuid: id(),
        severities,
    };
    for (kind, values) in reda_engine::pcb::seed_rules(&fab_rules(seed.rules)) {
        rules.push(DesignRule {
            uuid: id(),
            kind,
            scope: RuleScope::BOARD,
            values,
            enabled: true,
            comment: None,
        });
    }
    let ([top, bottom], dielectric, [finish, mask_color, silk_color]) = seed.stackup;
    let stackup = Stackup {
        uuid: id(),
        thickness: Nm(top.0 + bottom.0 + 1_000_000),
        entries: vec![
            StackupEntry::Copper(CopperEntry {
                layer: layers.layers[0].uuid,
                thickness: top,
            }),
            StackupEntry::Dielectric(DielectricEntry {
                uuid: id(),
                thickness: dielectric,
            }),
            StackupEntry::Copper(CopperEntry {
                layer: layers.layers[1].uuid,
                thickness: bottom,
            }),
        ],
        finish,
        mask_color,
        silk_color,
    };
    let mut objects = vec![
        Object::LayerSet(InDocument::new(doc, layers)),
        Object::Stackup(InDocument::new(doc, stackup)),
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
    objects.extend(
        rules
            .into_iter()
            .map(|r| Object::DesignRule(InDocument::new(doc, r))),
    );
    let mut nets = vec![];
    for (name, class) in seed.nets {
        // 0 is the Default Net Class only.
        let classes = match class % (classes.len() + 1) {
            0 => vec![],
            n => vec![classes[n - 1].uuid],
        };
        let uuid = id();
        nets.push(uuid);
        objects.push(Object::Net(InDocument::new(
            doc,
            Net {
                uuid,
                name,
                classes,
            },
        )));
    }
    if let Some((designator, at, flag, net)) = seed.footprint {
        let footprint = read_footprint(Path::new("x"), FOOTPRINT.as_bytes()).unwrap();
        let copy = FootprintCopy {
            uuid: id(),
            hash: "e".repeat(32),
            edited_from: None,
            footprint,
        };
        let mut pads: Vec<PadNet> = copy
            .footprint
            .pads
            .iter()
            .map(|p| PadNet {
                pad: p.uuid,
                net: None,
            })
            .collect();
        pads.sort_by_key(|p| p.pad);
        if !nets.is_empty() {
            pads[0].net = Some(nets[net % nets.len()]);
        }
        let instance = FootprintInstance {
            uuid: id(),
            component: flag.then(|| ComponentRef::new(Uuid::v4_from_bits(seed.base ^ 7))),
            footprint: copy.uuid,
            at,
            rotation: Millideg(if flag { 180_000 } else { 0 }),
            side: if flag { Side::Bottom } else { Side::Top },
            locked: flag,
            designator,
            fields: Default::default(),
            component_type: if flag {
                ComponentType::NetTie
            } else {
                ComponentType::Mechanical
            },
            in_bom: flag,
            in_cpl: !flag,
            not_fitted: !flag,
            text_overrides: if flag {
                vec![reda_model::TextOverride {
                    rotation: Some(Millideg(270_000)),
                    visible: Some(false),
                    ..reda_model::TextOverride::new(Uuid::v4_from_bits(seed.base ^ 11))
                }]
            } else {
                vec![]
            },
            pads,
        };
        objects.push(Object::FootprintCopy(InDocument::new(doc, copy)));
        objects.push(Object::FootprintInstance(InDocument::new(doc, instance)));
        // A Free Pad and the Output Origin come with it: through-hole on a Net, or SMD.
        let free_pad = FreePad {
            mask: None,
            paste: None,
            uuid: id(),
            kind: if flag {
                PadKind::PlatedThroughHole
            } else {
                PadKind::Smd
            },
            shape: PadShape::Circle,
            at,
            rotation: Millideg(if flag { 45_000 } else { 0 }),
            width: Nm(3_000_000),
            height: Nm(3_000_000),
            corner_ratio: None,
            chamfer_ratio: None,
            corners: None,
            offset: None,
            hole: flag.then_some(Hole::new(Nm(1_000_000), Nm(1_000_000))),
            side: (!flag).then_some(Side::Bottom),
            net: if flag { nets.first().copied() } else { None },
            zone_connection: None,
        };
        objects.push(Object::FreePad(InDocument::new(doc, free_pad)));
        objects.push(Object::OutputOrigin(InDocument::new(
            doc,
            OutputOrigin { uuid: id(), at },
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
                style: reda_model::TextStyle {
                    justify: Default::default(),
                    knockout: None,
                    font: t.font,
                    height: t.height,
                    thickness: t.height,
                },
                frame: None,
                position: t.position,
                rotation: t.rotation,
                mirrored: t.mirrored,
            },
        )));
    }
    if let Some((from, to, sweep, width, net, bottom)) = seed.copper
        && from != to
    {
        let net = net
            .filter(|_| !nets.is_empty())
            .map(|n| nets[n % nets.len()]);
        let geometry = match sweep {
            0 => TrackGeometry::Line { from, to },
            s => TrackGeometry::Arc {
                from,
                to,
                sweep: Millideg(s),
            },
        };
        let layer = layer_ids[usize::from(bottom)];
        objects.push(Object::Track(InDocument::new(
            doc,
            Track {
                uuid: id(),
                layer,
                net,
                width,
                geometry,
            },
        )));
        objects.push(Object::Via(InDocument::new(
            doc,
            Via {
                mask: None,
                uuid: id(),
                at: from,
                net,
                diameter: Nm(width.0 + 1),
                hole: width,
                span: LayerSpan {
                    from: layer_ids[0],
                    to: layer_ids[1],
                },
            },
        )));
        let third = Point::new(Nm(to.x.0 + 1000), Nm(to.y.0 + 7));
        if third != from {
            let vertices = vec![
                Vertex::arc(from, Millideg(sweep)),
                Vertex::at(to),
                Vertex::at(third),
            ];
            objects.push(Object::Zone(InDocument::new(
                doc,
                Zone {
                    uuid: id(),
                    layer,
                    net,
                    vertices: vertices.clone(),
                    clearance: width,
                    min_width: Nm(width.0 + 1),
                    pad_connection: if bottom {
                        PadConnection::Solid
                    } else {
                        PadConnection::Thermal
                    },
                    thermal_gap: width,
                    thermal_spoke_width: Nm(width.0 + 2),
                    remove_islands: bottom,
                    priority: width.0 as u32 % 7,
                },
            )));
            objects.push(Object::Keepout(InDocument::new(
                doc,
                Keepout {
                    uuid: id(),
                    layers: vec![layer],
                    vertices,
                    copper: true,
                    vias: !bottom,
                },
            )));
            let mut waived = vec![id(), layer];
            waived.sort();
            objects.push(Object::Waiver(InDocument::new(
                doc,
                Waiver {
                    uuid: id(),
                    check: "keepout".into(),
                    objects: waived,
                    author: "Ada".into(),
                    created: 1_767_225_600_000,
                    reason: if bottom {
                        "fixture".into()
                    } else {
                        String::new()
                    },
                },
            )));
        }
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
        Just(ColumnSource::CentreX),
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
        let rules = fab_rules(lengths);
        let gerber = GerberSettings { x2_attributes, naming: FileNaming::Classic, ..Default::default() };
        let copy = ProjectFabPreset::new(preset, FabPreset { name, rules, stackup: Default::default(), stackup_suggestions: Default::default(), zip_assembly: false, gerber, bom: reda_engine::model::BomSettings { columns: bom, ..Default::default() }, cpl: reda_engine::model::CplSettings { columns: cpl, ..Default::default() } });
        let [gerber, drill, bom, cpl] = outputs;
        let job = OutputJob {
            uuid: job,
            name: job_name,
            pcb,
            fab_preset: preset,
            outputs: Outputs { gerber, drill, bom, cpl, ..Outputs::ALL },
            name_template: reda_model::NameTemplate::default(),
            not_fitted_paste: Default::default(),
            schematic_pdf_colour: Default::default(),
            destination,
        };
        (copy, job)
    }
}

const SYMBOL: &str = r#"{"format":1,"symbol":{"uuid":"a0000000-0000-4000-8000-000000000010","name":"R","designator_prefix":"R"},"units":[{"uuid":"a0000000-0000-4000-8000-000000000011","name":"A"}],"pins":[{"uuid":"a0000000-0000-4000-8000-000000000012","unit":"a0000000-0000-4000-8000-000000000011","name":"~","number":"1","electrical_type":"passive","at":{"x":0,"y":3810000},"rotation":270000,"length":1270000}]}"#;
const FOOTPRINT: &str = r#"{"format":1,"footprint":{"uuid":"a0000000-0000-4000-8000-000000000040","name":"R_0603"},"pads":[{"uuid":"a0000000-0000-4000-8000-000000000041","number":"1","kind":"smd","shape":"rect","at":{"x":-800000,"y":0},"rotation":0,"width":800000,"height":950000}]}"#;
const PART: &str = r#"{"format":1,"part":{"uuid":"a0000000-0000-4000-8000-000000000100","name":"R_10k","component_type":"standard","in_bom":true,"in_cpl":true,"symbol":"a0000000-0000-4000-8000-000000000010"},"footprints":["a0000000-0000-4000-8000-000000000040"],"fields":{"value":"10k"},"pin_map":{"unconnected":["2"]}}"#;

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
        size in prop::option::of(0usize..4),
        component in prop::option::of(
            (stored_text(), any::<String>(), length(), length(), any::<bool>())
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
        let mut frame = SheetFrame::defaults(id(0));
        match size {
            0 => frame.size = FrameSize::Standard(SheetSize::A4),
            1 => {
                frame.size = FrameSize::Standard(SheetSize::Letter);
                frame.orientation = Orientation::Portrait;
                frame.border_zones = false;
            }
            2 => frame.size = FrameSize::Standard(SheetSize::A0),
            _ => {
                frame.size = FrameSize::Custom {
                    width: Nm(123_456_789),
                    height: Nm(98_765_432),
                };
                frame.title_block = false;
                frame.drawing_number = "DWG-1\u{e9}".into();
                frame.revision = "C".into();
            }
        }
        objects.push(Object::SheetFrame(InDocument::new(sheet, frame)));
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
            edited_from: None,
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
            designator_locked: mirror,
            fields: entry.part.fields.clone(),
            footprint: entry.part.footprints.first().copied(),
            component_type: ComponentType::Standard,
            in_bom: !mirror,
            in_cpl: mirror,
            not_fitted: mirror,
            visible_fields: if mirror {
                ["designator".to_owned(), "LCSC".to_owned()].into()
            } else {
                UnitInstance::default_visible_fields()
            },
            field_text: if mirror {
                vec![reda_engine::model::FieldText {
                    field: "value".to_owned(),
                    offset: Point::new(y, x),
                    size: Some(Nm(1_524_000)),
                }]
            } else {
                Default::default()
            },
            locked_fields: if mirror {
                ["LCSC".to_owned()].into()
            } else {
                Default::default()
            },
        };
        objects.push(Object::FootprintCopy(InDocument::new(sheet, copy)));
        objects.push(Object::PartEntry(InDocument::new(sheet, entry)));
        objects.push(Object::Unit(InDocument::new(sheet, unit)));
    }
    if let Some((name, x, y, len)) = seed.drawing {
        let from = Point::new(x, y);
        let to = Point::new(x, y + Nm(len.0.abs() + 1));
        // Net names refuse `[ ] { } , ..` and control characters.
        let name = reda_model::net_name::sanitize(&name);
        let name = if name.trim().is_empty() {
            "N".into()
        } else {
            name
        };
        objects.push(Object::Wire(InDocument::new(
            sheet,
            Wire {
                uuid: id(5),
                vertices: vec![from, to],
            },
        )));
        objects.push(Object::NetLabel(InDocument::new(
            sheet,
            NetLabel {
                uuid: id(6),
                name: name.clone(),
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
        objects.push(Object::Port(InDocument::new(
            sheet,
            Port {
                uuid: id(8),
                name: name.clone(),
                direction: PortDirection::In,
                at: to,
                rotation: Millideg(90_000),
                hide_cross_reference: len.0 % 2 == 0,
            },
        )));
        objects.push(Object::PowerPort(InDocument::new(
            sheet,
            PowerPort {
                uuid: id(9),
                name: name.clone(),
                style: PowerPortStyle::Ground,
                at: from,
                rotation: Millideg(0),
                hide_name: len.0 % 2 != 0,
            },
        )));
        objects.push(Object::SheetText(InDocument::new(
            sheet,
            SheetText {
                uuid: id(10),
                text: format!("{name}\nnote"),
                at: to,
                height: Nm(len.0.abs() + 1),
                rotation: Millideg(90_000),
                justify: if len.0 % 2 == 0 {
                    Justify::TopRight
                } else {
                    Justify::default()
                },
            },
        )));
        objects.push(Object::SheetGraphic(InDocument::new(
            sheet,
            SheetGraphic {
                uuid: id(11),
                shape: SheetShape::Line {
                    from,
                    to: Point::new(x + Nm(3), y + Nm(5)),
                },
            },
        )));
        objects.push(Object::SheetGraphic(InDocument::new(
            sheet,
            SheetGraphic {
                uuid: id(12),
                shape: SheetShape::Rect {
                    from,
                    to: Point::new(x + Nm(7), to.y),
                    filled: len.0 % 2 == 0,
                },
            },
        )));
        objects.push(Object::Waiver(InDocument::new(
            sheet,
            Waiver {
                uuid: id(13),
                check: "unconnected_pin".into(),
                objects: {
                    let mut o = vec![id(10), id(11)];
                    o.sort();
                    o
                },
                author: "Ada".into(),
                created: 1_767_225_600_000,
                reason: if len.0 % 2 == 0 {
                    String::new()
                } else {
                    format!("{name} \"ok\"")
                },
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
        properties in prop::collection::vec(stored_text(), 5),
        erc in prop::collection::vec((0..ErcCheck::ALL.len(), 0..3u8), 0..4),
    ) -> Project {
        let mut severities = ErcSeverities::DEFAULT;
        for (check, severity) in erc {
            let severity = [Severity::Error, Severity::Warning, Severity::Off][severity as usize];
            severities.set(ErcCheck::ALL[check], severity);
        }
        let [title, revision, date, author, company] =
            <[String; 5]>::try_from(properties).unwrap();
        let properties = ProjectProperties { title, revision, date, author, company };
        let mut project = Project::new(ProjectProps { uuid, name, documents: vec![], libraries, properties, erc: severities, schematic: Default::default() });
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
        erc: Default::default(),
        schematic: Default::default(),
    });
    assert_eq!(save(&project)[".reda/.gitignore"], b"*\n");
}

/// Fab Preset rules from seven lengths: the flat limits, reused for the hole range and the
/// preferred seeds.
fn fab_rules(lengths: [Nm; 7]) -> FabRules {
    let [
        clearance,
        track_width,
        via_diameter,
        via_hole,
        annular_ring,
        hole_to_hole,
        edge_clearance,
    ] = lengths;
    FabRules {
        mask_expansion: Default::default(),
        paste_expansion: Default::default(),
        clearance,
        track_width,
        via_diameter,
        via_hole,
        hole_min: annular_ring,
        hole_max: hole_to_hole,
        annular_ring,
        hole_to_hole,
        edge_clearance,
        preferred: PreferredSeeds {
            track_width,
            via_diameter,
            via_hole,
        },
    }
}
