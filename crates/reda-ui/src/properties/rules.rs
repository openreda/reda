//! The Rules region: which design rule records apply to the selected copper
//! ([`reda_engine::pcb::rules::explain::applicable`], what `pcb.rules.applicable` returns),
//! with the records that won, the ones that lost and the disabled ones. It shows for one
//! or two copper objects (Tracks, Vias, Free Pads, Zones, Pads or highlighted Nets), and
//! hides above two.

use reda_engine::model::{PcbDocument, Project, RuleKind, RuleValue, Severity, Uuid};
use reda_engine::pcb::rules::explain::{self, KindRules};

use crate::selection::Selection;

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
    /// The values it sets, such as `min` or `diameter.min`: the one won, or those lost.
    pub fields: Vec<String>,
    /// The value it supplies, for a record that won.
    pub value: Option<RuleValue>,
}

/// One kind of rule as it applies to the selection.
#[derive(Clone, Debug, PartialEq)]
pub struct KindRows {
    pub kind: RuleKind,
    pub layer_numbers: Vec<usize>,
    /// The DRC check's severity; `None` for kinds that shape outputs.
    pub severity: Option<Severity>,
    pub rows: Vec<RuleRow>,
    pub pair: Option<String>,
    /// For two objects: the value that applies between them and the object it came from.
    pub between: Option<(String, RuleValue, Option<usize>)>,
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
                fields: vec![winner.field],
                value: Some(winner.value),
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
                    fields: loser.fields,
                    value: None,
                });
            }
        }
    }
    KindRows {
        kind: kind.kind,
        layer_numbers: kind
            .layers
            .iter()
            .filter_map(|id| pcb.copper_number(*id))
            .collect(),
        pair: kind
            .between
            .as_ref()
            .and_then(|b| b.net_class_pair)
            .map(|ids| {
                ids.map(|id| {
                    pcb.net_class(id)
                        .map_or(reda_engine::pcb::DEFAULT_NET_CLASS, |c| c.name.as_str())
                })
                .join(" / ")
            }),
        severity: kind.severity,
        rows,
        between: kind.between.map(|b| (b.field, b.value, b.object)),
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
        RuleKind::Teardrop
        | RuleKind::CourtyardClearance
        | RuleKind::Dangling
        | RuleKind::MicroSegment
        | RuleKind::DeadCopper => return crate::tr!("board-setup-page-drc"),
    };
    rule.title()
}
