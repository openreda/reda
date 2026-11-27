//! The Rules region: which design rule records apply to the selected copper
//! ([`reda_engine::pcb::rules::explain::applicable`], what `pcb.rules.applicable` returns),
//! with the records that won, the ones that lost and the disabled ones. It shows for one
//! or two copper objects (Tracks, Vias, Free Pads, Zones, Pads or highlighted Nets), and
//! hides above two.

use reda_engine::model::{PcbDocument, Project, RuleKind, RuleValue, Severity, Uuid};
use reda_engine::pcb::rules::explain::{self, KindRules};

use crate::selection::Selection;
use crate::tr;
use reda_engine::model::{LayerGroup, RuleLayers};

pub(crate) fn layer_name(pcb: &PcbDocument, id: Uuid) -> String {
    pcb.layers()
        .layer(id)
        .map(|l| crate::panels::layers::layer_name(pcb, l))
        .unwrap_or_default()
}

pub(crate) fn layer_scope_label(pcb: &PcbDocument, layers: &RuleLayers) -> String {
    match layers {
        RuleLayers::Group(LayerGroup::Outer) => tr!("board-layer-outer"),
        RuleLayers::Group(LayerGroup::Inner) => tr!("board-layer-inner"),
        RuleLayers::Group(LayerGroup::Plane) => tr!("board-layer-plane"),
        RuleLayers::Named(ids) => pcb
            .copper_layers()
            .filter(|l| ids.contains(&l.uuid))
            .map(|l| layer_name(pcb, l.uuid))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Whether a record applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// It supplies the value shown.
    Won,
    /// It sets a value another record wins.
    Lost,
    /// It is turned off.
    Disabled,
}

/// One record as it applies to one object.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleRow {
    pub standing: Standing,
    /// Which selected object (0 or 1) it is about.
    pub object: usize,
    /// The record.
    pub rule: Uuid,
    /// Its Net Class's name, or `None` for the board-wide record.
    pub class: Option<String>,
    pub layers: Option<String>,
    /// The values it sets, such as `min` or `diameter.min`: the one won, or those lost.
    pub fields: Vec<String>,
    /// The value it supplies, for a record that won.
    pub value: Option<RuleValue>,
    /// A won value shown as text instead, such as a `via_span` record's allowed Via types.
    pub text: Option<String>,
}

/// A clearance between two selected objects and the source that supplied it.
#[derive(Clone, Debug, PartialEq)]
pub struct PairLimit {
    pub field: String,
    pub value: RuleValue,
    pub object: Option<usize>,
    pub pair: Option<String>,
}

/// One kind of rule as it applies to the selection.
#[derive(Clone, Debug, PartialEq)]
pub struct KindRows {
    pub kind: RuleKind,
    pub layer_names: Vec<String>,
    /// The DRC check's severity; `None` for kinds that shape outputs.
    pub severity: Option<Severity>,
    pub rows: Vec<RuleRow>,
    pub between_holes: Vec<PairLimit>,
    /// For two objects: the value that applies between them and the object it came from.
    pub between: Option<PairLimit>,
}

/// The Rules region's content.
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    /// How many objects it explains: one or two.
    pub objects: usize,
    pub kinds: Vec<KindRows>,
}

impl Rules {
    /// The rules for the selection, when it is one or two copper objects on the board.
    pub fn of(project: &Project, selection: &Selection) -> Option<Rules> {
        let pcb = project.pcb()?;
        let mut objects = Vec::new();
        for &id in selection.objects.iter().chain(&selection.nets) {
            objects.push(explain::Measured::object(pcb, id)?);
        }
        for &pad in &selection.pads {
            objects.push(explain::Measured::placed(pcb, pad)?);
        }
        if !(1..=2).contains(&objects.len()) {
            return None;
        }
        let kinds = explain::applicable_objects(pcb, &objects)
            .into_iter()
            .map(|k| rows(pcb, k))
            .filter(|k| !k.rows.is_empty())
            .collect();
        Some(Rules {
            objects: objects.len(),
            kinds,
        })
    }
}

fn rows(pcb: &PcbDocument, kind: KindRules) -> KindRows {
    let class = |uuid: Option<Uuid>| {
        uuid.map(|c| {
            pcb.net_class(c)
                .map_or_else(|| c.to_string(), |c| c.name.clone())
        })
    };
    let mut rows = Vec::new();
    for (object, rules) in kind.objects.into_iter().enumerate() {
        for winner in rules.values {
            rows.push(RuleRow {
                standing: Standing::Won,
                object,
                rule: winner.rule,
                class: class(winner.net_class),
                layers: winner.layers.as_ref().map(|l| layer_scope_label(pcb, l)),
                fields: vec![winner.field],
                value: Some(winner.value),
                text: None,
            });
        }
        if let Some(allowed) = rules.allowed {
            let names: Vec<String> = allowed
                .via_types
                .iter()
                .filter_map(|id| pcb.stackup().via_types.iter().find(|t| t.uuid == *id))
                .map(|t| crate::pcb::via_type_name(pcb, t))
                .collect();
            let text = if names.is_empty() {
                crate::tr!("properties-rules-allowed-every")
            } else {
                names.join(", ")
            };
            rows.push(RuleRow {
                standing: Standing::Won,
                object,
                rule: allowed.rule,
                class: class(allowed.net_class),
                layers: None,
                fields: vec!["allowed".to_owned()],
                value: None,
                text: Some(text),
            });
        }
        for (standing, list) in [
            (Standing::Lost, rules.losers),
            (Standing::Disabled, rules.disabled),
        ] {
            for loser in list {
                rows.push(RuleRow {
                    standing,
                    object,
                    rule: loser.rule,
                    class: class(loser.net_class),
                    layers: loser.layers.as_ref().map(|l| layer_scope_label(pcb, l)),
                    fields: loser.fields,
                    value: None,
                    text: None,
                });
            }
        }
    }
    let pair_label = |b: &explain::Between| {
        let ids = b.net_class_pair?;
        let names = ids
            .map(|id| {
                pcb.net_class(id)
                    .map_or(reda_engine::pcb::DEFAULT_NET_CLASS, |c| c.name.as_str())
            })
            .join(" / ");
        Some(match &b.layers {
            Some(layers) => format!("{names} · {}", layer_scope_label(pcb, layers)),
            None => names,
        })
    };
    let limit = |b: explain::Between| PairLimit {
        pair: pair_label(&b),
        field: b.field,
        value: b.value,
        object: b.object,
    };
    KindRows {
        kind: kind.kind,
        layer_names: kind.layers.iter().map(|id| layer_name(pcb, *id)).collect(),
        between_holes: kind.between_holes.into_iter().map(limit).collect(),
        severity: kind.severity,
        rows,
        between: kind.between.map(limit),
    }
}

/// The Board Setup page that edits `kind`, by its title.
pub fn page(kind: RuleKind) -> String {
    use crate::settings::board_setup::Rule;
    let rule = match kind {
        RuleKind::Clearance => Rule::Clearance,
        RuleKind::TrackWidth => Rule::TrackWidth,
        RuleKind::ViaSize | RuleKind::MicroviaSize | RuleKind::ViaSpan => Rule::Vias,
        RuleKind::HoleSize | RuleKind::AnnularRing | RuleKind::HoleToHole => Rule::Holes,
        RuleKind::EdgeClearance | RuleKind::SilkToEdge => Rule::BoardEdge,
        RuleKind::SilkToMask | RuleKind::SilkWidth | RuleKind::SilkTextHeight => Rule::Silkscreen,
        RuleKind::MaskExpansion | RuleKind::PasteExpansion => Rule::MaskPaste,
        RuleKind::ZoneConnection => Rule::ZoneConnections,
        RuleKind::Teardrop => Rule::Teardrops,
        RuleKind::CourtyardClearance
        | RuleKind::Dangling
        | RuleKind::MicroSegment
        | RuleKind::DeadCopper
        // Its own page arrives with the Component height check.
        | RuleKind::ComponentHeight => return crate::tr!("board-setup-page-drc"),
    };
    rule.title()
}
