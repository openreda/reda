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
    ErcSettings, FabPreset, FileNaming, FootprintCopy, FootprintInstance, FrameSize, FreePad,
    GerberSettings, GraphicShape, Hole, InDocument, Justify, Keepout, KeepoutShape, Layer,
    LayerKind, LayerSet, LayerSpan, Net, NetClass, NetLabel, NoConnect, Object, Orientation,
    OutputJob, OutputOrigin, Outputs, PadKind, PadNet, PadShape, PartEntry, PcbGraphic, PcbText,
    Port, PortDirection, PourOver, PowerPort, PowerPortStyle, Project, ProjectFabPreset,
    ProjectProperties, ProjectProps, Restriction, Severity, SheetFrame, SheetGraphic, SheetShape,
    SheetSize, SheetText, Side, Stackup, StackupEntry, Track, TrackGeometry, UnitInstance, Uuid,
    Via, Waiver, Wire, Zone, ZoneCutout, ZoneNameScheme,
};
use reda_engine::model::{
    ConnectionStyle, CopperRole, DesignRule, DielectricKind, FabRules, LayerGroup, PadCopperShape,
    PreferredSeeds, RuleKind, RuleLayers, RuleScope, RuleValues, UnusedLayers, ViaDiameter,
    ViaType, ZoneConnection,
};
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
    /// A Track with its Zone, Keepout and Waiver: ends, an arc's sweep (0 for a line),
    /// width, Net index (None for no Net), and a flag that varies the rest.
    copper: Option<(Point, Point, i64, Nm, Option<usize>, bool)>,
    /// The copper Layers, Stackup entries and Via types.
    stack: StackSeed,
    /// The finish and colours.
    finish: [Option<String>; 3],
    vias: Vec<ViaSeed>,
    rules_v2: RuleSeed,
    /// The through-hole Free Pad's Pad Stack, `unused_layers: remove`, and Zone connection.
    pad_stack: (
        Option<PadCopperShape>,
        Option<PadCopperShape>,
        bool,
        Option<ZoneConnection>,
    ),
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

fn unused_layers() -> impl Strategy<Value = UnusedLayers> {
    prop_oneof![
        Just(UnusedLayers::Keep),
        Just(UnusedLayers::Remove),
        Just(UnusedLayers::RemoveKeepEnds),
    ]
}

fn zone_connection() -> BoxedStrategy<ZoneConnection> {
    let style = prop_oneof![
        Just(ConnectionStyle::Relief),
        Just(ConnectionStyle::Solid),
        Just(ConnectionStyle::None),
    ];
    let angle = prop_oneof![Just(0), Just(45_000), Just(90_000)].prop_map(Millideg);
    (
        prop::option::of(style),
        prop::option::of(prop_oneof![Just(2u8), Just(4)]),
        prop::option::of((1i64..1_000_000).prop_map(Nm)),
        prop::option::of((0i64..1_000_000).prop_map(Nm)),
        prop::option::of(angle),
    )
        .prop_map(|(style, spokes, spoke_width, gap, angle)| ZoneConnection {
            style,
            spokes,
            spoke_width,
            gap,
            angle,
        })
        .boxed()
}

/// A Via hole and diameters that each clear it.
fn via_size() -> BoxedStrategy<(Nm, ViaDiameter)> {
    let grow = 1i64..1_000_000;
    (
        1i64..1_000_000,
        grow.clone(),
        prop::option::of(grow.clone()),
        prop::option::of(grow),
    )
        .prop_map(|(hole, start, middle, end)| {
            let over = |d: i64| Nm(hole + d);
            (
                Nm(hole),
                ViaDiameter {
                    start: over(start),
                    middle: middle.map(over),
                    end: end.map(over),
                },
            )
        })
        .boxed()
}

/// A Pad Stack group's copper, larger than the 1 mm hole it surrounds.
fn pad_copper() -> BoxedStrategy<PadCopperShape> {
    let shape = prop_oneof![
        Just(PadShape::Circle),
        Just(PadShape::Rect),
        Just(PadShape::Oval)
    ];
    (shape, 1_500_000i64..5_000_000, 1_500_000i64..5_000_000)
        .prop_map(|(shape, w, h)| PadCopperShape {
            shape,
            width: Nm(w),
            height: Nm(if shape == PadShape::Circle { w } else { h }),
            corner_ratio: None,
            chamfer_ratio: None,
            corners: None,
        })
        .boxed()
}

/// One dielectric sheet: prepreg or core, and each optional field set or left out.
#[derive(Clone, Debug)]
struct DielectricSeed {
    prepreg: bool,
    name: Option<String>,
    thickness: Option<Nm>,
    material: Option<String>,
    dk: Option<u32>,
    df: Option<u32>,
    test_frequency: Option<u64>,
}

fn dielectric_seed() -> BoxedStrategy<DielectricSeed> {
    (
        any::<bool>(),
        prop::option::of(any::<String>()),
        prop::option::of((1i64..500_000).prop_map(Nm)),
        prop::option::of(any::<String>()),
        prop::option::of(any::<u32>()),
        prop::option::of(any::<u32>()),
        prop::option::of(any::<u64>()),
    )
        .prop_map(
            |(prepreg, name, thickness, material, dk, df, test_frequency)| DielectricSeed {
                prepreg,
                name,
                thickness,
                material,
                dk,
                df,
                test_frequency,
            },
        )
        .boxed()
}

/// A Via type beyond the through type; its ends are indices into the copper Layers.
#[derive(Clone, Debug)]
struct ViaTypeSeed {
    from: usize,
    to: usize,
    micro: bool,
    name: Option<String>,
    size: (Nm, ViaDiameter),
    unused_layers: UnusedLayers,
}

fn via_type_seed() -> BoxedStrategy<ViaTypeSeed> {
    (
        any::<usize>(),
        any::<usize>(),
        any::<bool>(),
        prop::option::of(any::<String>()),
        via_size(),
        unused_layers(),
    )
        .prop_map(|(from, to, micro, name, size, unused_layers)| ViaTypeSeed {
            from,
            to,
            micro,
            name,
            size,
            unused_layers,
        })
        .boxed()
}

/// The copper Layers and their physical build: 2 to 6 copper Layers.
#[derive(Clone, Debug)]
struct StackSeed {
    /// Per copper Layer, top to bottom: copper thickness, role, and a name that only inner
    /// Layers keep.
    copper: Vec<(Nm, CopperRole, Option<String>)>,
    /// One or two sheets in each gap between copper Layers.
    gaps: Vec<Vec<DielectricSeed>>,
    via_types: Vec<ViaTypeSeed>,
}

fn stack_seed() -> BoxedStrategy<StackSeed> {
    let role = prop_oneof![
        Just(CopperRole::Signal),
        Just(CopperRole::Plane),
        Just(CopperRole::Mixed),
    ];
    (2usize..=6)
        .prop_flat_map(move |n| {
            (
                prop::collection::vec(
                    (
                        (1i64..200_000).prop_map(Nm),
                        role.clone(),
                        prop::option::of(any::<String>()),
                    ),
                    n,
                ),
                prop::collection::vec(prop::collection::vec(dielectric_seed(), 1..3), n - 1),
                prop::collection::vec(via_type_seed(), 0..4),
            )
                .prop_map(|(copper, gaps, via_types)| StackSeed {
                    copper,
                    gaps,
                    via_types,
                })
        })
        .boxed()
}

/// The copper Layers and the Stackup built from a seed, with the through type sized from
/// the preferred seeds. The Layers run top to bottom.
fn build_stack(
    seed: StackSeed,
    preferred: PreferredSeeds,
    id: &mut dyn FnMut() -> Uuid,
) -> (Vec<Layer>, Stackup) {
    let n = seed.copper.len();
    let layers: Vec<Layer> = seed
        .copper
        .iter()
        .enumerate()
        .map(|(i, (_, role, name))| Layer {
            uuid: id(),
            kind: LayerKind::Copper,
            side: match i {
                0 => Some(Side::Top),
                i if i == n - 1 => Some(Side::Bottom),
                _ => None,
            },
            name: (i != 0 && i != n - 1).then(|| name.clone()).flatten(),
            role: *role,
        })
        .collect();
    let mut entries = vec![];
    let mut typed = 0;
    for (i, (thickness, _, _)) in seed.copper.iter().enumerate() {
        typed += thickness.0;
        entries.push(StackupEntry::Copper(CopperEntry {
            layer: layers[i].uuid,
            thickness: *thickness,
        }));
        for sheet in seed.gaps.get(i).into_iter().flatten() {
            typed += sheet.thickness.map_or(0, |t| t.0);
            let kind = if sheet.prepreg {
                DielectricKind::Prepreg
            } else {
                DielectricKind::Core
            };
            entries.push(StackupEntry::Dielectric(DielectricEntry {
                name: sheet.name.clone(),
                thickness: sheet.thickness,
                material: sheet.material.clone(),
                dk: sheet.dk,
                df: sheet.df,
                test_frequency: sheet.test_frequency,
                ..DielectricEntry::new(id(), kind)
            }));
        }
    }
    let mut via_types = vec![ViaType::through(
        id(),
        layers[0].uuid,
        layers[n - 1].uuid,
        preferred.via_hole,
        preferred.via_diameter,
    )];
    let mut spans = std::collections::BTreeSet::from([(0, n - 1)]);
    for kind in seed.via_types {
        let (a, b) = (kind.from % n, kind.to % n);
        if a == b || !spans.insert((a.min(b), a.max(b))) {
            continue;
        }
        let (hole, diameter) = kind.size;
        via_types.push(ViaType {
            uuid: id(),
            name: kind.name,
            from: layers[a].uuid,
            to: layers[b].uuid,
            micro: kind.micro,
            hole,
            diameter,
            unused_layers: kind.unused_layers,
        });
    }
    let stackup = Stackup {
        uuid: id(),
        construction: None,
        thickness: Nm(typed + 1_000_000),
        entries,
        via_types,
        finish: None,
        mask_color: None,
        silk_color: None,
    };
    (layers, stackup)
}

/// A Via on the board; its ends are indices into the copper Layers.
#[derive(Clone, Debug)]
struct ViaSeed {
    from: usize,
    to: usize,
    at: Point,
    size: (Nm, ViaDiameter),
    unused_layers: UnusedLayers,
    zone_connection: Option<ZoneConnection>,
    net: Option<usize>,
    teardrop: bool,
    locked: bool,
}

fn via_seed() -> BoxedStrategy<ViaSeed> {
    (
        (any::<usize>(), any::<usize>()),
        point(),
        via_size(),
        unused_layers(),
        prop::option::of(zone_connection()),
        prop::option::of(any::<usize>()),
        (any::<bool>(), any::<bool>()),
    )
        .prop_map(
            |((from, to), at, size, unused_layers, zone_connection, net, (teardrop, locked))| {
                ViaSeed {
                    from,
                    to,
                    at,
                    size,
                    unused_layers,
                    zone_connection,
                    net,
                    teardrop,
                    locked,
                }
            },
        )
        .boxed()
}

/// A rule record's Layer scope: a group, or copper Layers by index.
#[derive(Clone, Debug)]
enum LayersSeed {
    Group(LayerGroup),
    Named(Vec<usize>),
}

fn layers_seed() -> BoxedStrategy<LayersSeed> {
    prop_oneof![
        prop_oneof![
            Just(LayerGroup::Outer),
            Just(LayerGroup::Inner),
            Just(LayerGroup::Plane)
        ]
        .prop_map(LayersSeed::Group),
        prop::collection::vec(any::<usize>(), 1..4).prop_map(LayersSeed::Named),
    ]
    .boxed()
}

impl LayersSeed {
    fn resolve(self, copper: &[Uuid]) -> RuleLayers {
        match self {
            Self::Group(group) => RuleLayers::Group(group),
            Self::Named(picks) => {
                let ids: std::collections::BTreeSet<Uuid> = picks
                    .into_iter()
                    .map(|i| copper[i % copper.len()])
                    .collect();
                RuleLayers::Named(ids.into_iter().collect())
            }
        }
    }
}

/// The v0.2 rule records: Layer-scoped records (kind, owner class, Layers, value),
/// class pair clearances (two classes, the Default among them, optional Layers, value), and
/// Net Classes' allowed Via types (class, Via type picks).
#[derive(Clone, Debug)]
struct RuleSeed {
    layered: Vec<(usize, Option<usize>, LayersSeed, Nm)>,
    pairs: Vec<(usize, usize, Option<LayersSeed>, Nm)>,
    allowed: Vec<(usize, Vec<usize>)>,
}

fn rule_seed() -> BoxedStrategy<RuleSeed> {
    (
        prop::collection::vec(
            (
                0usize..3,
                prop::option::of(any::<usize>()),
                layers_seed(),
                length(),
            ),
            0..4,
        ),
        prop::collection::vec(
            (
                any::<usize>(),
                any::<usize>(),
                prop::option::of(layers_seed()),
                length(),
            ),
            0..3,
        ),
        prop::collection::vec(
            (any::<usize>(), prop::collection::vec(any::<usize>(), 0..4)),
            0..2,
        ),
    )
        .prop_map(|(layered, pairs, allowed)| RuleSeed {
            layered,
            pairs,
            allowed,
        })
        .boxed()
}

/// Whether `rule` would clash with a record already in `rules`: the same kind and owner,
/// with the same Layer scope or a named Layer in common.
fn clashes(rules: &[DesignRule], rule: &DesignRule) -> bool {
    rules.iter().any(|r| {
        r.kind == rule.kind
            && r.scope.net_class == rule.scope.net_class
            && r.scope.net_class_pair == rule.scope.net_class_pair
            && (r.scope.layers == rule.scope.layers
                || matches!(
                    (&r.scope.layers, &rule.scope.layers),
                    (Some(RuleLayers::Named(a)), Some(RuleLayers::Named(b)))
                        if a.iter().any(|id| b.contains(id))
                ))
    })
}

/// The v0.2 rule records of a seed, against the board's copper Layers, Net Classes and Via
/// types. Records that would clash are left out.
fn v2_rules(
    seed: RuleSeed,
    copper: &[Uuid],
    classes: &[Uuid],
    via_types: &[Uuid],
    rules: &mut Vec<DesignRule>,
    id: &mut dyn FnMut() -> Uuid,
) {
    let push = |rules: &mut Vec<DesignRule>, rule: DesignRule| {
        if !clashes(rules, &rule) {
            rules.push(rule);
        }
    };
    for (kind, class, layers, value) in seed.layered {
        let (kind, values) = match kind {
            0 => (
                RuleKind::Clearance,
                RuleValues {
                    value: Some(value),
                    ..RuleValues::default()
                },
            ),
            1 => (
                RuleKind::TrackWidth,
                RuleValues {
                    preferred: Some(value),
                    ..RuleValues::default()
                },
            ),
            _ => (
                RuleKind::AnnularRing,
                RuleValues {
                    min: Some(value),
                    ..RuleValues::default()
                },
            ),
        };
        let net_class = class
            .filter(|_| !classes.is_empty())
            .map(|c| classes[c % classes.len()]);
        let rule = DesignRule {
            uuid: id(),
            kind,
            scope: RuleScope {
                net_class,
                layers: Some(layers.resolve(copper)),
                net_class_pair: None,
            },
            values,
            enabled: value.0 % 2 == 0,
            comment: None,
        };
        push(rules, rule);
    }
    let owners: Vec<Uuid> = std::iter::once(Uuid::NIL)
        .chain(classes.iter().copied())
        .collect();
    for (a, b, layers, value) in seed.pairs {
        let mut pair = [owners[a % owners.len()], owners[b % owners.len()]];
        pair.sort();
        let rule = DesignRule {
            uuid: id(),
            kind: RuleKind::Clearance,
            scope: RuleScope {
                net_class: None,
                layers: layers.map(|l| l.resolve(copper)),
                net_class_pair: Some(pair),
            },
            values: RuleValues {
                value: Some(value),
                ..RuleValues::default()
            },
            enabled: true,
            comment: Some("pair".into()),
        };
        push(rules, rule);
    }
    for (class, picks) in seed.allowed {
        if classes.is_empty() {
            break;
        }
        let mut allowed: Vec<Uuid> = vec![];
        for pick in picks {
            let kind = via_types[pick % via_types.len()];
            if !allowed.contains(&kind) {
                allowed.push(kind);
            }
        }
        let rule = DesignRule {
            uuid: id(),
            kind: RuleKind::ViaSpan,
            scope: RuleScope::class(classes[class % classes.len()]),
            values: RuleValues {
                allowed: Some(allowed),
                ..RuleValues::default()
            },
            enabled: true,
            comment: None,
        };
        push(rules, rule);
    }
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
        stack in stack_seed(),
        finish in prop::array::uniform3(prop::option::of(any::<String>())),
        vias in prop::collection::vec(via_seed(), 0..4),
        rules_v2 in rule_seed(),
        pad_stack in (
            prop::option::of(pad_copper()),
            prop::option::of(pad_copper()),
            any::<bool>(),
            prop::option::of(zone_connection()),
        ),
    ) -> PcbSeed {
        PcbSeed {
            base, name, title, rules, severities, outline, cutouts, classes, nets, texts,
            graphics, footprint, copper, stack, finish, vias, rules_v2, pad_stack,
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
    let preferred = fab_rules(seed.rules).preferred;
    let set = id();
    let (copper_layers, mut stackup) = build_stack(seed.stack, preferred, &mut id);
    let copper_ids: Vec<Uuid> = copper_layers.iter().map(|l| l.uuid).collect();
    let mut layers = LayerSet {
        uuid: set,
        layers: copper_layers,
    };
    layers.layers.push(Layer {
        uuid: id(),
        kind: LayerKind::BoardOutline,
        side: None,
        name: None,
        role: Default::default(),
    });
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
        zone_name_scheme: ZoneNameScheme::LayernameNetPxxx,
        fab_notes: Some("Route the slots.\nNo X-outs.".into()),
    };
    let seeded = reda_engine::pcb::seed_rules(&fab_rules(seed.rules));
    for (kind, values) in seeded.into_iter().chain(reda_engine::pcb::default_rules()) {
        rules.push(DesignRule {
            uuid: id(),
            kind,
            scope: RuleScope::BOARD,
            values,
            enabled: true,
            comment: None,
        });
    }
    let mut added = reda_engine::pcb::multilayer_rules(|_| id());
    let teardrop = added
        .iter_mut()
        .find(|r| r.kind == RuleKind::Teardrop)
        .unwrap();
    let pad = teardrop.values.through_hole_pad.as_mut().unwrap();
    pad.enabled = Some(seed.base & 1 != 0);
    pad.curved = Some(seed.base & 2 != 0);
    pad.length = Some(1 + (seed.base % 100) as u32);
    rules.extend(added);
    let class_ids: Vec<Uuid> = classes.iter().map(|c| c.uuid).collect();
    let type_ids: Vec<Uuid> = stackup.via_types.iter().map(|v| v.uuid).collect();
    v2_rules(
        seed.rules_v2,
        &copper_ids,
        &class_ids,
        &type_ids,
        &mut rules,
        &mut id,
    );
    let [finish, mask_color, silk_color] = seed.finish;
    stackup.finish = finish;
    stackup.mask_color = mask_color;
    stackup.silk_color = silk_color;
    stackup.construction = (seed.base & 64 != 0).then(|| "JLC04161H-7628".into());
    let typed = stackup.clone();
    let mut objects = vec![
        Object::LayerSet(InDocument::new(doc, layers)),
        Object::Stackup(InDocument::new(doc, stackup)),
        Object::BoardSetup(InDocument::new(doc, setup)),
    ];
    if let Some(vertices) = seed.outline {
        objects.push(Object::BoardOutline(InDocument::new(
            doc,
            BoardOutline {
                // The first edge plated on some seeds, so the per-vertex mark round-trips.
                plated: (seed.base & 128 != 0)
                    .then(|| (0, reda_model::PlatedEdge::default()))
                    .into_iter()
                    .collect(),
                ..BoardOutline::new(id(), vertices)
            },
        )));
    }
    for vertices in seed.cutouts {
        objects.push(Object::BoardCutout(InDocument::new(
            doc,
            BoardCutout::new(id(), vertices),
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
                color: (!classes.is_empty()).then(|| "#ff8800".to_string()),
                loop_removal: classes.is_empty(),
                classes,
            },
        )));
    }
    let pad_stack = seed.pad_stack;
    if let Some((designator, at, flag, net)) = seed.footprint {
        let mut footprint = read_footprint(Path::new("x"), FOOTPRINT.as_bytes()).unwrap();
        footprint.pads[0].teardrop = seed.base & 4 != 0;
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
            graphics_unlocked: false,
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
            teardrop: seed.base & 8 != 0,
            locked: false,
            mask: None,
            paste: None,
            uuid: id(),
            // A plated hole is castellated, so the field round-trips too.
            castellated: flag,
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
            middle: pad_stack.0.filter(|_| flag),
            bottom: pad_stack.1.filter(|_| flag),
            unused_layers: if flag && pad_stack.2 {
                UnusedLayers::Remove
            } else {
                UnusedLayers::Keep
            },
            corner_ratio: None,
            chamfer_ratio: None,
            corners: None,
            offset: None,
            hole: flag.then_some(Hole::new(Nm(1_000_000), Nm(1_000_000))),
            side: (!flag).then_some(Side::Bottom),
            net: if flag { nets.first().copied() } else { None },
            zone_connection: pad_stack.3,
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
                locked: false,
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
        let layer = copper_ids[width.0 as usize % copper_ids.len()];
        objects.push(Object::Track(InDocument::new(
            doc,
            Track {
                teardrop: seed.base & 16 != 0,
                locked: false,
                uuid: id(),
                layer,
                net,
                width,
                geometry,
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
                    locked: false,
                    uuid: id(),
                    name: bottom.then(|| format!("Pour {}", width.0)),
                    layer,
                    net,
                    vertices: vertices.clone(),
                    follows_outline: false,
                    clearance: bottom.then_some(width),
                    min_width: (!bottom).then_some(Nm(width.0 + 1)),
                    pour_over: if bottom {
                        PourOver::All
                    } else {
                        PourOver::ZonesOnly
                    },
                    remove_unconnected: bottom,
                    min_island_area: width.0 * 3,
                    priority: width.0 as u32 % 7,
                    shelved: !bottom,
                },
            )));
            objects.push(Object::ZoneCutout(InDocument::new(
                doc,
                ZoneCutout {
                    uuid: id(),
                    layer,
                    vertices: vertices.clone(),
                },
            )));
            objects.push(Object::Keepout(InDocument::new(
                doc,
                Keepout {
                    locked: false,
                    uuid: id(),
                    layer: (!bottom).then_some(layer),
                    shape: if bottom {
                        KeepoutShape::Line {
                            vertices,
                            width: Nm(250_000),
                        }
                    } else {
                        KeepoutShape::Polygon { vertices }
                    },
                    restrict: if bottom {
                        Restriction::ALL.into_iter().collect()
                    } else {
                        Restriction::defaults()
                    },
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
    let n = copper_ids.len();
    for via in seed.vias {
        let (a, b) = (via.from % n, via.to % n);
        let b = if a == b { (a + 1) % n } else { b };
        let span = LayerSpan {
            from: copper_ids[a],
            to: copper_ids[b],
        };
        let (hole, diameter) = via.size;
        objects.push(Object::Via(InDocument::new(
            doc,
            Via {
                unused_layers: via.unused_layers,
                teardrop: via.teardrop,
                zone_connection: via.zone_connection,
                locked: via.locked,
                mask: None,
                uuid: id(),
                at: via.at,
                net: via
                    .net
                    .filter(|_| !nets.is_empty())
                    .map(|i| nets[i % nets.len()]),
                diameter,
                hole,
                // A Via on a typed span follows its type's drilling direction.
                span: typed.type_for_span(span).map_or(span, ViaType::span),
            },
        )));
    }
    for (layer, width, shape) in seed.graphics {
        objects.push(Object::PcbGraphic(InDocument::new(
            doc,
            PcbGraphic {
                locked: false,
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

fn placement_correction() -> impl Strategy<Value = reda_model::PlacementCorrection> {
    use reda_model::{PlacementCorrection, PlacementMatch};
    let matcher = prop_oneof![
        any::<String>().prop_map(|footprint| PlacementMatch::Footprint { footprint }),
        (any::<String>(), any::<String>())
            .prop_map(|(field, value)| PlacementMatch::Field { field, value }),
    ];
    (matcher, any::<i64>(), prop::option::of(point())).prop_map(|(matcher, rotation, offset)| {
        PlacementCorrection::new(matcher, Millideg(rotation), offset)
    })
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
        placement_corrections in prop::collection::vec(placement_correction(), 0..4),
        job_name in stem("j"),
        outputs in prop::array::uniform4(any::<bool>()),
        destination in prop::option::of(stem("d")),
        stack in stack_seed(),
        notes in prop::option::of(any::<String>()),
        inner in prop::option::of(prop::array::uniform3(prop::option::of(length()))),
    ) -> (ProjectFabPreset, OutputJob) {
        let mut rules = fab_rules(lengths);
        rules.inner = inner.map(|[track_width_min, clearance, annular_ring_min]| {
            reda_model::InnerFabRules { track_width_min, clearance, annular_ring_min }
        });
        let gerber = GerberSettings { x2_attributes, naming: FileNaming::Classic, ..Default::default() };
        // A Construction of 2 to 6 copper Layers, saved from a board Stackup.
        let mut next = 0u128;
        let (_, stackup) = build_stack(stack, rules.preferred, &mut || {
            next += 1;
            Uuid::from_u128(next)
        });
        let mut construction = reda_model::Construction::from_stackup(job_name.clone(), &stackup);
        construction.default = x2_attributes;
        construction.notes = notes;
        let copy = ProjectFabPreset::new(preset, FabPreset { name, rules, placement_corrections, constructions: vec![construction], stackup_suggestions: Default::default(), zip_assembly: false, gerber, bom: reda_engine::model::BomSettings { columns: bom, ..Default::default() }, cpl: reda_engine::model::CplSettings { columns: cpl, ..Default::default() } });
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
        cells in prop::collection::vec((0..101usize, 0..4u8), 0..4),
    ) -> Project {
        let mut severities = ErcSettings::DEFAULT;
        let level = |n: u8| [Severity::Error, Severity::Warning, Severity::Off].get(n as usize).copied();
        for (check, severity) in erc {
            severities.severities.set(ErcCheck::ALL[check], level(severity).unwrap());
        }
        for (cell, severity) in cells {
            let cell = reda_model::Cell::all().nth(cell).unwrap();
            severities.pin_matrix_overrides.set(cell, level(severity));
        }
        let [title, revision, date, author, company] =
            <[String; 5]>::try_from(properties).unwrap();
        let properties = ProjectProperties { title, revision, date, author, company };
        let mut project = Project::new(ProjectProps { uuid, name, documents: vec![], libraries, properties, erc: severities, schematic: Default::default(), reannotate: None, library_update: None, missing: vec![] });
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

/// Every key, and every `key=value` for string and boolean values, anywhere in `value`,
/// with the copper count of each Layer list and Construction.
fn facts(value: &Value, out: &mut std::collections::BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                out.insert(key.clone());
                match value {
                    Value::String(s) => {
                        out.insert(format!("{key}={s}"));
                    }
                    Value::Bool(b) => {
                        out.insert(format!("{key}={b}"));
                    }
                    Value::Array(items) if key == "layers" => {
                        let copper = items.iter().filter(|l| l["kind"] == "copper").count();
                        out.insert(format!("copper={copper}"));
                        if items.iter().any(|l| l.get("name").is_some()) {
                            out.insert("named Layer".into());
                        }
                        if !items.is_empty() && items.iter().all(Value::is_string) {
                            out.insert("layers=[named]".into());
                        }
                    }
                    Value::Array(items) if key == "entries" => {
                        let copper = items.iter().filter(|e| e.get("copper").is_some()).count();
                        out.insert(format!("construction copper={copper}"));
                    }
                    _ => {}
                }
                facts(value, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| facts(v, out)),
        _ => {}
    }
}

/// The generator reaches every v0.2 field, so the property tests above cover them: Layer
/// names and roles, 2 to 6 copper Layers, every Via type form, Dielectric values,
/// `unused_layers` and Zone connection on Vias and Pads, Pad Stacks, Layer and class pair
/// rule scopes, allowed Via types, and multilayer Constructions.
#[test]
fn the_project_generator_reaches_every_v2_field() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
    let rng = TestRng::deterministic_rng(RngAlgorithm::ChaCha);
    let mut runner = TestRunner::new_with_rng(Config::default(), rng);
    let strategy = project();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..200 {
        let project = strategy.new_tree(&mut runner).unwrap().current();
        for bytes in save(&project).values() {
            if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
                facts(&value, &mut seen);
            }
        }
    }
    let wanted = [
        "copper=2",
        "copper=4",
        "copper=6",
        "named Layer",
        "role=plane",
        "role=mixed",
        "micro=true",
        "kind=prepreg",
        "material",
        "dk",
        "df",
        "test_frequency",
        "unused_layers=remove",
        "unused_layers=remove_keep_ends",
        "zone_connection",
        "spokes",
        "middle",
        "layers=inner",
        "layers=plane",
        "layers=outer",
        "layers=[named]",
        "net_class_pair",
        "allowed",
        "construction copper=5",
        "notes",
    ];
    let missing: Vec<_> = wanted.iter().filter(|f| !seen.contains(**f)).collect();
    assert!(missing.is_empty(), "never generated: {missing:?}");
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
        reannotate: None,
        library_update: None,
        missing: vec![],
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
    // Preferred seeds become a physical through Via when the PCB is created.
    let preferred_hole = Nm(via_hole.0.abs() + 1);
    let preferred_diameter = Nm(preferred_hole.0 + via_diameter.0.abs() + 1);
    FabRules {
        inner: None,
        silk_width: None,
        silk_text_height: None,
        microvia_size: None,
        via_aspect_ratio: None,
        clearance_hole: reda_model::HoleClearance::GENERIC,
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
            via_diameter: preferred_diameter,
            via_hole: preferred_hole,
        },
    }
}
