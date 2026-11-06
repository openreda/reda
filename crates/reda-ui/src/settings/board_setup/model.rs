//! What Board Setup edits, as drafts: the board-wide rule values, the DRC severities, the
//! Net Classes with their rule overrides, and which classes each Net is in. Every page of
//! the dialog shares one of these, so a check's severity is the same draft on its rule's
//! page and on the DRC page.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use reda_engine::model::geom::Nm;
use reda_engine::model::{
    self as m, DrcCheck, FabRules, PcbDocument, Project, RuleKind, Severity, Uuid,
};
use reda_engine::schematic::natural_cmp;
use serde_json::{Map, Value, json};

use super::length::{mm, optional, required};
use crate::settings::{Draft, Problem, Step};
use crate::tr;

/// One rule value the dialog edits: a value of one rule kind's records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleField {
    Clearance,
    TrackWidth,
    TrackPreferred,
    ViaDiameter,
    ViaDiameterPreferred,
    ViaHole,
    ViaHolePreferred,
    HoleMin,
    HoleMax,
    AnnularRing,
    HoleToHole,
    EdgeClearance,
}

impl RuleField {
    pub const ALL: [RuleField; 12] = [
        RuleField::Clearance,
        RuleField::TrackWidth,
        RuleField::TrackPreferred,
        RuleField::ViaDiameter,
        RuleField::ViaDiameterPreferred,
        RuleField::ViaHole,
        RuleField::ViaHolePreferred,
        RuleField::HoleMin,
        RuleField::HoleMax,
        RuleField::AnnularRing,
        RuleField::HoleToHole,
        RuleField::EdgeClearance,
    ];

    /// The rule kind and the value of its records.
    pub fn rule(self) -> (RuleKind, m::RuleField) {
        use m::RuleField as F;
        match self {
            RuleField::Clearance => (RuleKind::Clearance, F::Value),
            RuleField::TrackWidth => (RuleKind::TrackWidth, F::Min),
            RuleField::TrackPreferred => (RuleKind::TrackWidth, F::Preferred),
            RuleField::ViaDiameter => (RuleKind::ViaSize, F::DiameterMin),
            RuleField::ViaDiameterPreferred => (RuleKind::ViaSize, F::DiameterPreferred),
            RuleField::ViaHole => (RuleKind::ViaSize, F::HoleMin),
            RuleField::ViaHolePreferred => (RuleKind::ViaSize, F::HolePreferred),
            RuleField::HoleMin => (RuleKind::HoleSize, F::Min),
            RuleField::HoleMax => (RuleKind::HoleSize, F::Max),
            RuleField::AnnularRing => (RuleKind::AnnularRing, F::Min),
            RuleField::HoleToHole => (RuleKind::HoleToHole, F::Min),
            RuleField::EdgeClearance => (RuleKind::EdgeClearance, F::Value),
        }
    }

    /// Its name in problem fields, such as `track_width.preferred`.
    pub fn id(self) -> String {
        let (kind, field) = self.rule();
        format!("{}.{}", kind.id(), field.path())
    }

    /// Its value on `pcb`'s board-wide record.
    pub fn get(self, pcb: &PcbDocument) -> Option<Nm> {
        let (kind, field) = self.rule();
        pcb.board_rule(kind).values.length(field)
    }

    fn of_class(self, pcb: &PcbDocument, class: Uuid) -> Option<Nm> {
        let (kind, field) = self.rule();
        pcb.class_rule(kind, class)?.values.length(field)
    }

    /// The value a Fab Preset seeds it with.
    pub fn seed(self, rules: &FabRules) -> Option<Nm> {
        let (kind, field) = self.rule();
        reda_engine::pcb::seed_rules(rules)
            .into_iter()
            .find(|(k, _)| *k == kind)?
            .1
            .length(field)
    }

    /// The name of what it limits, such as "Track width", for the min, preferred and max
    /// values alike.
    fn base_name(self) -> String {
        match self {
            RuleField::Clearance => tr!("board-setup-clearance"),
            RuleField::TrackWidth | RuleField::TrackPreferred => tr!("board-setup-track-width"),
            RuleField::ViaDiameter | RuleField::ViaDiameterPreferred => {
                tr!("board-setup-via-diameter")
            }
            RuleField::ViaHole | RuleField::ViaHolePreferred => tr!("board-setup-via-hole"),
            RuleField::HoleMin | RuleField::HoleMax => tr!("board-setup-pad-hole"),
            RuleField::AnnularRing => tr!("board-setup-annular-ring"),
            RuleField::HoleToHole => tr!("board-setup-hole-to-hole"),
            RuleField::EdgeClearance => tr!("board-setup-edge-clearance"),
        }
    }

    /// Its name for the user: "Track width" for a minimum, "Preferred Track width".
    pub fn name(self) -> String {
        let base = self.base_name();
        match self.rule().1 {
            f if f.is_preferred() => tr!("board-setup-preferred", rule = base),
            m::RuleField::Max | m::RuleField::DiameterMax | m::RuleField::HoleMax => {
                tr!("board-setup-maximum", rule = base)
            }
            _ => base,
        }
    }

    /// The label of its board-wide field, such as "Board-wide minimum Clearance (mm)".
    pub fn board_wide_label(self) -> String {
        let rule = self.base_name();
        match self.rule().1 {
            f if f.is_preferred() => tr!("board-setup-board-wide-preferred", rule = rule),
            m::RuleField::Max | m::RuleField::DiameterMax | m::RuleField::HoleMax => {
                tr!("board-setup-board-wide-maximum", rule = rule)
            }
            _ => tr!("board-setup-board-wide", rule = rule),
        }
    }

    /// The problem field of the board-wide value.
    pub fn problem_field(self) -> String {
        format!("rule/{}", self.id())
    }
}

/// `values` as `pcb.rules.set` takes them: `{"min": 1}` or `{"diameter": {"min": 1}}`.
fn values_param(values: &[(m::RuleField, Option<Nm>)]) -> Value {
    let mut out = Map::new();
    for (field, value) in values {
        let value = value.map_or(Value::Null, |v| Value::from(v.0));
        match field.path().split_once('.') {
            Some((outer, inner)) => {
                let entry = out
                    .entry(outer.to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Value::Object(map) = entry {
                    map.insert(inner.to_owned(), value);
                }
            }
            None => {
                out.insert(field.path().to_owned(), value);
            }
        }
    }
    Value::Object(out)
}

/// Changed values grouped by rule kind, in kind order.
fn by_kind(
    changes: impl Iterator<Item = (RuleField, Option<Nm>)>,
) -> BTreeMap<RuleKind, Vec<(m::RuleField, Option<Nm>)>> {
    let mut out: BTreeMap<RuleKind, Vec<_>> = BTreeMap::new();
    for (rule, value) in changes {
        let (kind, field) = rule.rule();
        out.entry(kind).or_default().push((field, value));
    }
    out
}

/// A Net Class row: the Default Net Class, one the board has, or one added in the dialog
/// and not applied yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClassKey {
    /// The Default Net Class, which every Net is in and whose values are the board-wide
    /// records.
    Default,
    Existing(Uuid),
    New(u32),
}

impl fmt::Display for ClassKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClassKey::Default => write!(f, "default"),
            ClassKey::Existing(uuid) => write!(f, "{uuid}"),
            ClassKey::New(n) => write!(f, "new-{n}"),
        }
    }
}

/// The problem field of a class's name or one of its overrides.
pub fn class_field(key: ClassKey, what: &str) -> String {
    format!("class/{key}/{what}")
}

/// A Net Class as drafted.
#[derive(Clone, Debug)]
pub struct ClassRow {
    key: ClassKey,
    name: Draft<String>,
    overrides: BTreeMap<RuleField, Draft<String>>,
    /// Deleted in the dialog; it goes on Apply.
    deleted: bool,
}

impl ClassRow {
    pub fn key(&self) -> ClassKey {
        self.key
    }

    pub fn name(&self) -> &str {
        self.name.value()
    }
}

/// A Net and the classes it is drafted into besides the Default Net Class.
#[derive(Clone, Debug)]
pub struct NetRow {
    pub uuid: Uuid,
    pub name: String,
    classes: Draft<BTreeSet<ClassKey>>,
}

impl NetRow {
    /// The class the Net table shows: the Default Net Class when it is in no other, else
    /// its first.
    pub fn class(&self) -> ClassKey {
        self.classes
            .value()
            .iter()
            .next()
            .copied()
            .unwrap_or(ClassKey::Default)
    }

    /// Its classes besides the Default Net Class.
    pub fn classes(&self) -> &BTreeSet<ClassKey> {
        self.classes.value()
    }
}

/// One rule a Fab Preset would change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleChange {
    pub rule: RuleField,
    /// The value in the dialog now, as typed.
    pub board: String,
    pub preset: Nm,
}

/// What applying a Fab Preset's rules would change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FabPreview {
    pub preset: Uuid,
    pub name: String,
    pub changes: Vec<RuleChange>,
}

/// The drafts of a PCB Document's Board Setup.
#[derive(Clone, Debug, Default)]
pub struct BoardSetupDraft {
    /// The PCB Document, while there is one.
    pcb: Option<Uuid>,
    rules: BTreeMap<RuleField, Draft<String>>,
    severities: Vec<(DrcCheck, Draft<Severity>)>,
    classes: Vec<ClassRow>,
    nets: Vec<NetRow>,
    next_new: u32,
    /// The Net table's filter, as typed.
    pub net_filter: String,
    presets: Vec<(Uuid, String, FabRules)>,
    preview: Option<FabPreview>,
}

/// Loads a length draft: `fresh` shown in mm, and a typed value that means the same
/// (`0.20` for `0.2`) is not a change.
fn load_length(draft: &mut Draft<String>, fresh: Option<Nm>, reset: bool) {
    let same = optional(draft.value()).ok() == Some(fresh);
    draft.load(fresh.map(mm).unwrap_or_default(), reset || same);
}

/// Whether a length draft holds a valid value other than the Project's.
fn length_changed(draft: &Draft<String>) -> Option<Option<Nm>> {
    let value = optional(draft.value()).ok()?;
    (value != optional(draft.base()).ok().flatten()).then_some(value)
}

impl BoardSetupDraft {
    /// Takes in the Project as it is now (see [`crate::settings::Page::load`]).
    pub fn load(&mut self, project: &Project, reset: bool) {
        self.presets = project
            .fab_presets()
            .map(|p| (p.uuid, p.name.clone(), p.rules))
            .collect();
        let Some(pcb) = project.pcb() else {
            *self = BoardSetupDraft {
                presets: std::mem::take(&mut self.presets),
                ..BoardSetupDraft::default()
            };
            return;
        };
        if self.pcb != Some(pcb.uuid) {
            *self = BoardSetupDraft {
                pcb: Some(pcb.uuid),
                presets: std::mem::take(&mut self.presets),
                ..BoardSetupDraft::default()
            };
        }
        if reset {
            self.preview = None;
        }
        let setup = pcb.setup();
        for field in RuleField::ALL {
            let draft = self.rules.entry(field).or_default();
            load_length(draft, field.get(pcb), reset);
        }
        if self.severities.is_empty() {
            self.severities = DrcCheck::ALL
                .iter()
                .map(|&c| (c, Draft::new(setup.severities.get(c))))
                .collect();
        }
        for (check, draft) in &mut self.severities {
            draft.load(setup.severities.get(*check), reset);
        }
        self.load_classes(pcb, reset);
        self.load_nets(pcb, reset);
    }

    fn load_classes(&mut self, pcb: &PcbDocument, reset: bool) {
        let present: BTreeSet<Uuid> = pcb.net_classes().map(|c| c.uuid).collect();
        self.classes.retain(|row| match row.key {
            ClassKey::Default => false,
            ClassKey::Existing(uuid) => present.contains(&uuid),
            ClassKey::New(_) => !reset,
        });
        let mut added = false;
        for class in pcb.net_classes() {
            let key = ClassKey::Existing(class.uuid);
            let row = match self.classes.iter_mut().position(|r| r.key == key) {
                Some(i) => &mut self.classes[i],
                None => {
                    added = true;
                    self.classes.push(ClassRow {
                        key,
                        name: Draft::new(class.name.clone()),
                        overrides: BTreeMap::new(),
                        deleted: false,
                    });
                    self.classes.last_mut().expect("just pushed")
                }
            };
            row.name.load(class.name.clone(), reset);
            if reset {
                row.deleted = false;
            }
            for field in RuleField::ALL {
                let draft = row.overrides.entry(field).or_default();
                load_length(draft, field.of_class(pcb, class.uuid), reset);
            }
        }
        if added {
            // The board's classes in rank order, then new ones.
            let rank = |key: ClassKey| match key {
                ClassKey::Existing(uuid) => (0, pcb.net_class(uuid).map_or(0, |c| c.rank)),
                ClassKey::New(n) => (1, n),
                ClassKey::Default => (2, 0),
            };
            self.classes.sort_by_key(|r| rank(r.key));
        }
    }

    fn load_nets(&mut self, pcb: &PcbDocument, reset: bool) {
        let mut old: BTreeMap<Uuid, NetRow> = std::mem::take(&mut self.nets)
            .into_iter()
            .map(|n| (n.uuid, n))
            .collect();
        let mut nets: Vec<NetRow> = pcb
            .nets()
            .map(|net| {
                let fresh: BTreeSet<ClassKey> =
                    net.classes.iter().map(|c| ClassKey::Existing(*c)).collect();
                let mut classes = old
                    .remove(&net.uuid)
                    .map_or_else(|| Draft::new(fresh.clone()), |row| row.classes);
                classes.load(fresh, reset);
                NetRow {
                    uuid: net.uuid,
                    name: net.name.clone(),
                    classes,
                }
            })
            .collect();
        nets.sort_by(|a, b| natural_cmp(&a.name, &b.name).then(a.uuid.cmp(&b.uuid)));
        self.nets = nets;
        // A Net drafted into a class that is gone leaves it.
        let live: BTreeSet<ClassKey> = self.live_classes().map(|c| c.key).collect();
        for net in &mut self.nets {
            if !net.classes.value().is_subset(&live) {
                let kept = net.classes.value() & &live;
                net.classes.set(kept);
            }
        }
    }

    /// Whether the Project has a PCB Document to set up.
    pub fn has_pcb(&self) -> bool {
        self.pcb.is_some()
    }

    /// The board-wide `rule` as typed.
    pub fn rule(&self, rule: RuleField) -> &str {
        self.rules.get(&rule).map_or("", |d| d.value())
    }

    pub fn set_rule(&mut self, rule: RuleField, text: impl Into<String>) {
        self.rules.entry(rule).or_default().set(text.into());
    }

    pub fn rule_mut(&mut self, rule: RuleField) -> &mut String {
        self.rules.entry(rule).or_default().value_mut()
    }

    /// The severity drafted for `check`.
    pub fn severity(&self, check: DrcCheck) -> Severity {
        self.severities
            .iter()
            .find(|(c, _)| *c == check)
            .map_or(Severity::Error, |(_, d)| *d.value())
    }

    pub fn set_severity(&mut self, check: DrcCheck, severity: Severity) {
        if let Some((_, d)) = self.severities.iter_mut().find(|(c, _)| *c == check) {
            d.set(severity);
        }
    }

    /// The Net Classes the board will have, besides the Default Net Class, in rank order.
    pub fn live_classes(&self) -> impl Iterator<Item = &ClassRow> {
        self.classes.iter().filter(|c| !c.deleted)
    }

    fn class_row(&self, key: ClassKey) -> Option<&ClassRow> {
        self.live_classes().find(|c| c.key == key)
    }

    fn class_row_mut(&mut self, key: ClassKey) -> Option<&mut ClassRow> {
        self.classes.iter_mut().find(|c| c.key == key && !c.deleted)
    }

    /// Whether `key` is the Default Net Class, which cannot be deleted.
    pub fn is_default(&self, key: ClassKey) -> bool {
        key == ClassKey::Default
    }

    /// Adds a Net Class with a free name like `Class 2`.
    pub fn add_class(&mut self) -> ClassKey {
        let taken: BTreeSet<String> = self
            .live_classes()
            .map(|c| c.name.value().trim().to_lowercase())
            .collect();
        let name = (1..)
            .map(|n| tr!("board-setup-new-class", number = n))
            .find(|name| !taken.contains(&name.to_lowercase()))
            .expect("some number is free");
        self.next_new += 1;
        let key = ClassKey::New(self.next_new);
        let mut row = ClassRow {
            key,
            name: Draft::new(String::new()),
            overrides: RuleField::ALL
                .iter()
                .map(|&f| (f, Draft::default()))
                .collect(),
            deleted: false,
        };
        row.name.set(name);
        self.classes.push(row);
        key
    }

    /// The class's name as typed.
    pub fn class_name_mut(&mut self, key: ClassKey) -> Option<&mut String> {
        self.class_row_mut(key).map(|r| r.name.value_mut())
    }

    pub fn rename_class(&mut self, key: ClassKey, name: impl Into<String>) {
        if let Some(row) = self.class_row_mut(key) {
            row.name.set(name.into());
        }
    }

    /// Deletes a class (not the Default one); its Nets leave it.
    pub fn delete_class(&mut self, key: ClassKey) {
        if self.is_default(key) {
            return;
        }
        let Some(i) = self.classes.iter().position(|c| c.key == key) else {
            return;
        };
        match key {
            ClassKey::Existing(_) => self.classes[i].deleted = true,
            ClassKey::New(_) | ClassKey::Default => {
                self.classes.remove(i);
            }
        }
        for net in &mut self.nets {
            if net.classes.value().contains(&key) {
                let mut kept = net.classes.value().clone();
                kept.remove(&key);
                net.classes.set(kept);
            }
        }
    }

    /// A class's override of `rule` as typed; empty uses the board-wide rule.
    pub fn class_override(&self, key: ClassKey, rule: RuleField) -> &str {
        self.class_row(key)
            .and_then(|r| r.overrides.get(&rule))
            .map_or("", |d| d.value())
    }

    pub fn class_override_mut(&mut self, key: ClassKey, rule: RuleField) -> Option<&mut String> {
        self.class_row_mut(key)
            .and_then(|r| r.overrides.get_mut(&rule))
            .map(|d| d.value_mut())
    }

    pub fn set_class_override(&mut self, key: ClassKey, rule: RuleField, text: &str) {
        if let Some(value) = self.class_override_mut(key, rule) {
            *value = text.to_owned();
        }
    }

    /// Every Net, in name order.
    pub fn nets(&self) -> &[NetRow] {
        &self.nets
    }

    /// The Nets whose names contain the filter, ignoring case.
    pub fn filtered_nets(&self) -> impl Iterator<Item = &NetRow> {
        let filter = self.net_filter.trim().to_lowercase();
        self.nets
            .iter()
            .filter(move |n| n.name.to_lowercase().contains(&filter))
    }

    /// Drafts `net` into the class `key` alone; the Default Net Class takes it out of every
    /// other.
    pub fn set_net_class(&mut self, net: Uuid, key: ClassKey) {
        if !self.is_default(key) && self.class_row(key).is_none() {
            return;
        }
        if let Some(row) = self.nets.iter_mut().find(|n| n.uuid == net) {
            let classes = match key {
                ClassKey::Default => BTreeSet::new(),
                key => BTreeSet::from([key]),
            };
            row.classes.set(classes);
        }
    }

    /// The Project's Fab Preset copies, by UUID and name.
    pub fn presets(&self) -> impl Iterator<Item = (Uuid, &str)> {
        self.presets.iter().map(|(u, n, _)| (*u, n.as_str()))
    }

    /// Previews what applying `preset`'s rules would change in the rules as drafted.
    pub fn preview_fab_preset(&mut self, preset: Uuid) {
        let Some((_, name, rules)) = self.presets.iter().find(|(u, ..)| *u == preset) else {
            return;
        };
        let changes = RuleField::ALL
            .iter()
            .filter_map(|&rule| {
                let to = rule.seed(rules)?;
                let typed = self.rule(rule);
                (required(typed).ok() != Some(to)).then(|| RuleChange {
                    rule,
                    board: typed.to_owned(),
                    preset: to,
                })
            })
            .collect();
        self.preview = Some(FabPreview {
            preset,
            name: name.clone(),
            changes,
        });
    }

    pub fn fab_preview(&self) -> Option<&FabPreview> {
        self.preview.as_ref()
    }

    /// Takes the previewed preset's rules into the drafts; Apply commits them.
    pub fn accept_fab_preview(&mut self) {
        if let Some(preview) = self.preview.take() {
            for change in preview.changes {
                self.set_rule(change.rule, mm(change.preset));
            }
        }
    }

    pub fn cancel_fab_preview(&mut self) {
        self.preview = None;
    }

    /// Every invalid value, by problem field.
    pub fn problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        for (rule, draft) in &self.rules {
            let (kind, field) = rule.rule();
            let check = if kind.required().contains(&field) {
                required(draft.value()).map(|_| ())
            } else {
                optional(draft.value()).map(|_| ())
            };
            if let Err(message) = check {
                problems.push(Problem::new(rule.problem_field(), message));
            }
        }
        // No class may take the Default Net Class's name.
        let mut seen: BTreeMap<String, ClassKey> = BTreeMap::from([(
            reda_engine::pcb::DEFAULT_NET_CLASS.to_lowercase(),
            ClassKey::Default,
        )]);
        for row in self.live_classes() {
            let name = row.name.value().trim();
            if name.is_empty() {
                problems.push(Problem::new(
                    class_field(row.key, "name"),
                    tr!("board-setup-class-blank"),
                ));
            } else if seen.insert(name.to_lowercase(), row.key).is_some() {
                problems.push(Problem::new(
                    class_field(row.key, "name"),
                    tr!("board-setup-class-duplicate", name = name),
                ));
            }
            for (rule, draft) in &row.overrides {
                if let Err(message) = optional(draft.value()) {
                    problems.push(Problem::new(class_field(row.key, &rule.id()), message));
                }
            }
        }
        problems
    }

    /// The Commands that make the PCB Document match the drafts, in order: board-wide
    /// rules, severities, deleted classes, renames, class rules, added classes, then Nets
    /// leaving and joining classes.
    pub fn steps(&self) -> Vec<Step> {
        if self.pcb.is_none() {
            return Vec::new();
        }
        let mut steps = Vec::new();
        let board = by_kind(
            self.rules
                .iter()
                .filter_map(|(rule, draft)| Some((*rule, length_changed(draft)?))),
        );
        for (kind, values) in board {
            steps.push(Step::new(
                "pcb.rules.set",
                json!({"kind": kind, "values": values_param(&values)}),
            ));
        }
        for (check, draft) in &self.severities {
            if draft.changed() {
                steps.push(Step::new(
                    "drc.set_severity",
                    json!({"check": check, "severity": draft.value()}),
                ));
            }
        }
        let existing = |row: &&ClassRow| matches!(row.key, ClassKey::Existing(_));
        for row in self.classes.iter().filter(existing) {
            let ClassKey::Existing(uuid) = row.key else {
                continue;
            };
            if row.deleted {
                steps.push(Step::new("pcb.net_class.delete", json!({"class": uuid})));
            }
        }
        let renames: Vec<(Uuid, &str)> = self
            .live_classes()
            .filter(existing)
            .filter(|row| row.name.value().trim() != row.name.base().trim())
            .filter_map(|row| match row.key {
                ClassKey::Existing(uuid) => Some((uuid, row.name.value().trim())),
                _ => None,
            })
            .collect();
        // A rename onto a name another kept class still has (two classes swapping
        // names) goes through a name of its own first: names are unique at every step.
        let taken = |name: &str| {
            self.live_classes()
                .filter(existing)
                .any(|row| row.name.base().trim() == name)
        };
        if renames.iter().any(|&(_, name)| taken(name)) {
            for &(uuid, _) in &renames {
                steps.push(Step::new(
                    "pcb.net_class.rename",
                    json!({"class": uuid, "name": uuid.to_string()}),
                ));
            }
        }
        for (uuid, name) in renames {
            steps.push(Step::new(
                "pcb.net_class.rename",
                json!({"class": uuid, "name": name}),
            ));
        }
        for row in self.live_classes().filter(existing) {
            let ClassKey::Existing(uuid) = row.key else {
                continue;
            };
            let changed = by_kind(
                row.overrides
                    .iter()
                    .filter_map(|(rule, d)| Some((*rule, length_changed(d)?))),
            );
            for (kind, values) in changed {
                steps.push(Step::new(
                    "pcb.rules.set",
                    json!({"kind": kind, "scope": {"net_class": uuid},
                           "values": values_param(&values)}),
                ));
            }
        }
        for row in self.live_classes().filter(|r| !existing(r)) {
            let rules: Vec<Value> =
                by_kind(row.overrides.iter().filter_map(|(rule, d)| {
                    Some((*rule, Some(optional(d.value()).ok().flatten()?)))
                }))
                .into_iter()
                .map(|(kind, values)| json!({"kind": kind, "values": values_param(&values)}))
                .collect();
            let mut params = Map::new();
            params.insert("name".to_owned(), json!(row.name.value().trim()));
            let nets: Vec<Uuid> = self
                .nets
                .iter()
                .filter(|n| n.classes.value().contains(&row.key))
                .map(|n| n.uuid)
                .collect();
            if !nets.is_empty() {
                params.insert("nets".to_owned(), json!(nets));
            }
            if !rules.is_empty() {
                params.insert("rules".to_owned(), json!(rules));
            }
            steps.push(Step::new("pcb.net_class.add", Value::Object(params)));
        }
        let deleted: BTreeSet<ClassKey> = self
            .classes
            .iter()
            .filter(|r| r.deleted)
            .map(|r| r.key)
            .collect();
        let mut leaving: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
        let mut joining: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
        for net in &self.nets {
            let (now, was) = (net.classes.value(), net.classes.base());
            for key in was.difference(now) {
                // Deleting a class takes its Nets out of it already.
                if let ClassKey::Existing(class) = key
                    && !deleted.contains(key)
                {
                    leaving.entry(*class).or_default().push(net.uuid);
                }
            }
            for key in now.difference(was) {
                if let ClassKey::Existing(class) = key {
                    joining.entry(*class).or_default().push(net.uuid);
                }
            }
        }
        for (class, nets) in leaving {
            steps.push(Step::new(
                "pcb.net.remove_from_class",
                json!({"nets": nets, "class": class}),
            ));
        }
        for (class, nets) in joining {
            steps.push(Step::new(
                "pcb.net.add_to_class",
                json!({"nets": nets, "class": class}),
            ));
        }
        steps
    }
}
