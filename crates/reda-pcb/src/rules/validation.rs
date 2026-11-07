//! Rule validation: the one function behind Board Setup, `pcb.rules.set` and
//! `pcb.rules.validate`. Blocking problems are judged on a record's effective values;
//! notes never block.

use reda_model::geom::Nm;
use reda_model::{DesignRule, Limit, PcbDocument, RuleField, RuleKind, Uuid};
use schemars::JsonSchema;
use serde::Serialize;

use super::presets::{PresetRules, outside_presets};
use super::{ProblemKind, blocking_problems, net_rules, range_problems, triples};

/// A rule record that cannot be used as it stands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Problem {
    /// The record.
    pub rule: Uuid,
    pub kind: RuleKind,
    /// The record's Net Class; left out for a board-wide record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class: Option<Uuid>,
    /// Which value, such as `diameter.min`.
    pub value: String,
    pub problem: ProblemKind,
    /// The problem in words.
    pub message: String,
}

/// What a note is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// A value allows less than a Fab Preset limit (`preset` names it).
    OutsidePreset,
    /// A Net Class record allows less than the board-wide record.
    WeakerThanBoard,
    /// A Net Class that no Net is in.
    ClassWithoutNets,
    /// A Net Class record value equal to the board-wide one it would inherit.
    SameAsInherited,
    /// A disabled board-wide `mask_expansion` or `paste_expansion` record, which changes
    /// the outputs.
    OutputsChange,
    /// A Net whose classes combine into an invalid range, though neither is wrong alone.
    NetRangeInvalid,
}

/// Something worth knowing about the rules that never blocks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Note {
    pub note: NoteKind,
    /// The record it is about, if one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<RuleKind>,
    /// The Net Class it is about, if one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class: Option<Uuid>,
    /// The Net it is about (`net_range_invalid`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    /// Which value, such as `min`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The Fab Preset (`outside_preset`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// The note in words.
    pub message: String,
}

impl Note {
    fn new(note: NoteKind, message: String) -> Note {
        Note {
            note,
            rule: None,
            kind: None,
            net_class: None,
            net: None,
            value: None,
            preset: None,
            message,
        }
    }

    fn on(mut self, record: &DesignRule) -> Note {
        self.rule = Some(record.uuid);
        self.kind = Some(record.kind);
        self.net_class = record.scope.net_class;
        self
    }

    fn value(mut self, field: RuleField) -> Note {
        self.value = Some(field.path().to_owned());
        self
    }
}

/// The result of validating the rules.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Validation {
    /// Blocking problems, record by record in file order.
    pub problems: Vec<Problem>,
    /// Notes, which never block.
    pub notes: Vec<Note>,
}

/// The blocking problems of every record, in file order.
pub fn problems(pcb: &PcbDocument) -> Vec<Problem> {
    let mut out = Vec::new();
    for record in pcb.rules() {
        for p in blocking_problems(pcb.board_rule(record.kind), record) {
            out.push(Problem {
                rule: record.uuid,
                kind: record.kind,
                net_class: record.scope.net_class,
                value: p.field.path().to_owned(),
                problem: p.problem,
                message: p.message(),
            });
        }
    }
    out
}

/// Validates every rule record of `pcb`, comparing limits with `presets` (the Fab Presets
/// of its Output Jobs).
pub fn validate(pcb: &PcbDocument, presets: &[PresetRules]) -> Validation {
    let mut notes = Vec::new();
    let class_name = |record: &DesignRule| {
        record
            .scope
            .net_class
            .and_then(|c| pcb.net_class(c))
            .map_or_else(
                || "the board".to_owned(),
                |c| format!("Net Class {:?}", c.name),
            )
    };
    for f in outside_presets(pcb, presets) {
        let way = match f.limit {
            Limit::Min => "below",
            Limit::Max => "above",
        };
        let mut note = Note::new(
            NoteKind::OutsidePreset,
            format!(
                "`{}` {} of {} is {way} {}'s limit {}",
                f.field.path(),
                f.record.kind.id(),
                class_name(f.record),
                f.preset,
                mm(f.preset_limit)
            ),
        )
        .on(f.record)
        .value(f.field);
        note.preset = Some(f.preset);
        notes.push(note);
    }
    for record in pcb.rules().filter(|r| !r.scope.is_board_wide()) {
        let board = pcb.board_rule(record.kind);
        for field in record.values.set_fields() {
            let (Some(mine), Some(theirs)) = (record.values.get(field), board.values.get(field))
            else {
                continue;
            };
            if mine == theirs {
                notes.push(
                    Note::new(
                        NoteKind::SameAsInherited,
                        format!(
                            "`{}` of {} equals the board's",
                            field.path(),
                            class_name(record)
                        ),
                    )
                    .on(record)
                    .value(field),
                );
                continue;
            }
            if !record.enabled || field.is_preferred() {
                continue;
            }
            let (Some(mine), Some(theirs)) = (mine.length(), theirs.length()) else {
                continue;
            };
            if matches!(
                record.kind,
                RuleKind::MaskExpansion | RuleKind::PasteExpansion
            ) {
                continue;
            }
            let is_max = matches!(
                field,
                RuleField::Max | RuleField::DiameterMax | RuleField::HoleMax
            );
            let weaker = if is_max { mine > theirs } else { mine < theirs };
            if weaker {
                notes.push(
                    Note::new(
                        NoteKind::WeakerThanBoard,
                        format!(
                            "`{}` of {} allows less than the board's {}",
                            field.path(),
                            class_name(record),
                            mm(theirs)
                        ),
                    )
                    .on(record)
                    .value(field),
                );
            }
        }
    }
    for class in pcb.net_classes() {
        if !pcb.nets().any(|n| n.classes.contains(&class.uuid)) {
            let mut note = Note::new(
                NoteKind::ClassWithoutNets,
                format!("Net Class {:?} has no Nets", class.name),
            );
            note.net_class = Some(class.uuid);
            notes.push(note);
        }
    }
    for kind in [RuleKind::MaskExpansion, RuleKind::PasteExpansion] {
        let record = pcb.board_rule(kind);
        if !record.enabled {
            notes.push(
                Note::new(
                    NoteKind::OutputsChange,
                    format!(
                        "the board-wide `{}` rule is disabled, so the outputs have no expansion{}",
                        kind.id(),
                        if kind == RuleKind::MaskExpansion {
                            " and untented Vias"
                        } else {
                            ""
                        }
                    ),
                )
                .on(record),
            );
        }
    }
    notes.extend(net_range_notes(pcb));
    Validation {
        problems: problems(pcb),
        notes,
    }
}

/// Nets in Net Classes whose resolved ranges are invalid, where no single record of the
/// kind on the Net has a blocking problem of its own.
fn net_range_notes(pcb: &PcbDocument) -> Vec<Note> {
    let mut notes = Vec::new();
    for net in pcb.nets().filter(|n| !n.classes.is_empty()) {
        let rules = net_rules(pcb, Some(net.uuid));
        for kind in [RuleKind::TrackWidth, RuleKind::ViaSize, RuleKind::HoleSize] {
            let own_problem = pcb
                .rules()
                .filter(|r| r.kind == kind)
                .filter(|r| r.scope.net_class.is_none_or(|c| net.classes.contains(&c)))
                .any(|r| !blocking_problems(pcb.board_rule(kind), r).is_empty());
            if own_problem {
                continue;
            }
            for &triple in triples(kind) {
                let get = |field: RuleField| -> Option<Nm> {
                    use RuleField::*;
                    match field {
                        Min if kind == RuleKind::TrackWidth => rules.track_width.min,
                        Preferred => Some(rules.track_width.preferred),
                        Max if kind == RuleKind::TrackWidth => rules.track_width.max,
                        Min => rules.hole_min,
                        Max => rules.hole_max,
                        DiameterMin => rules.via_diameter.min,
                        DiameterPreferred => Some(rules.via_diameter.preferred),
                        DiameterMax => rules.via_diameter.max,
                        HoleMin => rules.via_hole.min,
                        HolePreferred => Some(rules.via_hole.preferred),
                        HoleMax => rules.via_hole.max,
                        _ => None,
                    }
                };
                for p in range_problems(triple, get) {
                    let mut note = Note::new(
                        NoteKind::NetRangeInvalid,
                        format!(
                            "the classes of Net {:?} combine into an invalid `{}` range: `{}` {}",
                            net.name,
                            kind.id(),
                            p.field.path(),
                            match p.problem {
                                ProblemKind::MinAboveMax => "is above the maximum",
                                _ => "is outside the range",
                            }
                        ),
                    )
                    .value(p.field);
                    note.kind = Some(kind);
                    note.net = Some(net.uuid);
                    notes.push(note);
                }
            }
        }
    }
    notes
}

/// A length in mm, for messages.
fn mm(n: Nm) -> String {
    let s = format!("{:.4}", n.0 as f64 / 1e6);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s} mm")
}
