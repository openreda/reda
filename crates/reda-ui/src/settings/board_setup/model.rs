//! What Board Setup edits, as drafts: the board-wide rules, the DRC severities, the Net
//! Classes and which class each Net is in. Every page of the dialog shares one of these,
//! so a check's severity is the same draft on its rule's page and on the DRC page.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use reda_engine::model::geom::Nm;
use reda_engine::model::{DrcCheck, NetClass, PcbDocument, Project, Rules, Severity, Uuid};
use reda_engine::schematic::natural_cmp;
use serde_json::{Map, Value, json};

use super::length::{mm, optional, required};
use crate::settings::{Draft, Problem, Step};
use crate::tr;

/// One board-wide rule value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleField {
    Clearance,
    TrackWidth,
    ViaDiameter,
    ViaHole,
    AnnularRing,
    HoleToHole,
    EdgeClearance,
}

impl RuleField {
    pub const ALL: [RuleField; 7] = [
        RuleField::Clearance,
        RuleField::TrackWidth,
        RuleField::ViaDiameter,
        RuleField::ViaHole,
        RuleField::AnnularRing,
        RuleField::HoleToHole,
        RuleField::EdgeClearance,
    ];

    /// The rules a Net Class can override.
    pub const OVERRIDES: [RuleField; 4] = [
        RuleField::Clearance,
        RuleField::TrackWidth,
        RuleField::ViaDiameter,
        RuleField::ViaHole,
    ];

    /// The name `pcb.rules.set` and `pcb.net_class.*` give it.
    pub fn id(self) -> &'static str {
        match self {
            RuleField::Clearance => "clearance",
            RuleField::TrackWidth => "track_width",
            RuleField::ViaDiameter => "via_diameter",
            RuleField::ViaHole => "via_hole",
            RuleField::AnnularRing => "annular_ring",
            RuleField::HoleToHole => "hole_to_hole",
            RuleField::EdgeClearance => "edge_clearance",
        }
    }

    pub fn get(self, rules: &Rules) -> Nm {
        match self {
            RuleField::Clearance => rules.clearance,
            RuleField::TrackWidth => rules.track_width,
            RuleField::ViaDiameter => rules.via_diameter,
            RuleField::ViaHole => rules.via_hole,
            RuleField::AnnularRing => rules.annular_ring,
            RuleField::HoleToHole => rules.hole_to_hole,
            RuleField::EdgeClearance => rules.edge_clearance,
        }
    }

    fn of_class(self, class: &NetClass) -> Option<Nm> {
        match self {
            RuleField::Clearance => class.clearance,
            RuleField::TrackWidth => class.track_width,
            RuleField::ViaDiameter => class.via_diameter,
            RuleField::ViaHole => class.via_hole,
            _ => None,
        }
    }

    /// Its name for the user.
    pub fn name(self) -> String {
        match self {
            RuleField::Clearance => tr!("board-setup-clearance"),
            RuleField::TrackWidth => tr!("board-setup-track-width"),
            RuleField::ViaDiameter => tr!("board-setup-via-diameter"),
            RuleField::ViaHole => tr!("board-setup-via-hole"),
            RuleField::AnnularRing => tr!("board-setup-annular-ring"),
            RuleField::HoleToHole => tr!("board-setup-hole-to-hole"),
            RuleField::EdgeClearance => tr!("board-setup-edge-clearance"),
        }
    }

    /// The problem field of the board-wide value.
    pub fn problem_field(self) -> String {
        format!("rule/{}", self.id())
    }
}

/// A Net Class row: one the board has, or one added in the dialog and not applied yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClassKey {
    Existing(Uuid),
    New(u32),
}

impl fmt::Display for ClassKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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

/// A Net and the class it is drafted into.
#[derive(Clone, Debug)]
pub struct NetRow {
    pub uuid: Uuid,
    pub name: String,
    class: Draft<ClassKey>,
}

impl NetRow {
    pub fn class(&self) -> ClassKey {
        *self.class.value()
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
    default_class: Option<Uuid>,
    rules: BTreeMap<RuleField, Draft<String>>,
    severities: Vec<(DrcCheck, Draft<Severity>)>,
    classes: Vec<ClassRow>,
    nets: Vec<NetRow>,
    next_new: u32,
    /// The Net table's filter, as typed.
    pub net_filter: String,
    presets: Vec<(Uuid, String, Rules)>,
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
        self.default_class = Some(setup.default_net_class);
        for field in RuleField::ALL {
            let draft = self.rules.entry(field).or_default();
            load_length(draft, Some(field.get(&setup.rules)), reset);
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
            for field in RuleField::OVERRIDES {
                let draft = row.overrides.entry(field).or_default();
                load_length(draft, field.of_class(class), reset);
            }
        }
        if added {
            let default = self.default_class.map(ClassKey::Existing);
            // The Default class first, then the board's others by name, then new ones.
            self.classes.sort_by(|a, b| {
                let rank = |r: &ClassRow| match r.key {
                    k if Some(k) == default => 0,
                    ClassKey::Existing(_) => 1,
                    ClassKey::New(_) => 2,
                };
                rank(a).cmp(&rank(b)).then_with(|| match (a.key, b.key) {
                    (ClassKey::New(x), ClassKey::New(y)) => x.cmp(&y),
                    _ => natural_cmp(a.name.base(), b.name.base()),
                })
            });
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
                let fresh = ClassKey::Existing(net.class);
                let mut class = old
                    .remove(&net.uuid)
                    .map_or_else(|| Draft::new(fresh), |row| row.class);
                class.load(fresh, reset);
                NetRow {
                    uuid: net.uuid,
                    name: net.name.clone(),
                    class,
                }
            })
            .collect();
        nets.sort_by(|a, b| natural_cmp(&a.name, &b.name).then(a.uuid.cmp(&b.uuid)));
        self.nets = nets;
        // A Net drafted into a class that is gone goes back to the Default class.
        let live: BTreeSet<ClassKey> = self.live_classes().map(|c| c.key).collect();
        let default = self.default_key();
        for net in &mut self.nets {
            if !live.contains(net.class.value())
                && let Some(default) = default
            {
                net.class.set(default);
            }
        }
    }

    /// Whether the Project has a PCB Document to set up.
    pub fn has_pcb(&self) -> bool {
        self.pcb.is_some()
    }

    fn default_key(&self) -> Option<ClassKey> {
        self.default_class.map(ClassKey::Existing)
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

    /// The Net Classes the board will have, Default first.
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
        Some(key) == self.default_key()
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
            overrides: RuleField::OVERRIDES
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

    /// Deletes a class (not the Default one); its Nets go to the Default class.
    pub fn delete_class(&mut self, key: ClassKey) {
        if self.is_default(key) {
            return;
        }
        let Some(i) = self.classes.iter().position(|c| c.key == key) else {
            return;
        };
        match key {
            ClassKey::Existing(_) => self.classes[i].deleted = true,
            ClassKey::New(_) => {
                self.classes.remove(i);
            }
        }
        if let Some(default) = self.default_key() {
            for net in &mut self.nets {
                if *net.class.value() == key {
                    net.class.set(default);
                }
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

    /// Drafts `net` into the class `key`.
    pub fn set_net_class(&mut self, net: Uuid, key: ClassKey) {
        if self.class_row(key).is_none() {
            return;
        }
        if let Some(row) = self.nets.iter_mut().find(|n| n.uuid == net) {
            row.class.set(key);
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
                let to = rule.get(rules);
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
            if let Err(message) = required(draft.value()) {
                problems.push(Problem::new(rule.problem_field(), message));
            }
        }
        let mut seen: BTreeMap<String, ClassKey> = BTreeMap::new();
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
                    problems.push(Problem::new(class_field(row.key, rule.id()), message));
                }
            }
        }
        problems
    }

    /// The Commands that make the PCB Document match the drafts, in order: rules,
    /// severities, deleted classes, renames, overrides, added classes, Net moves.
    pub fn steps(&self) -> Vec<Step> {
        if self.pcb.is_none() {
            return Vec::new();
        }
        let mut steps = Vec::new();
        let mut rules = Map::new();
        for (rule, draft) in &self.rules {
            if let Some(Some(value)) = length_changed(draft) {
                rules.insert(rule.id().to_owned(), Value::from(value.0));
            }
        }
        if !rules.is_empty() {
            steps.push(Step::new("pcb.rules.set", Value::Object(rules)));
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
        let deleted: BTreeSet<ClassKey> = self
            .classes
            .iter()
            .filter(|r| r.deleted)
            .map(|r| r.key)
            .collect();
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
                ClassKey::New(_) => None,
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
            if row.overrides.values().any(|d| length_changed(d).is_some()) {
                let mut params = overrides(row);
                params.insert("class".to_owned(), json!(uuid));
                steps.push(Step::new(
                    "pcb.net_class.set_overrides",
                    Value::Object(params),
                ));
            }
        }
        for row in self.live_classes().filter(|r| !existing(r)) {
            let mut params = overrides(row);
            params.insert("name".to_owned(), json!(row.name.value().trim()));
            let nets: Vec<Uuid> = self
                .nets
                .iter()
                .filter(|n| *n.class.value() == row.key)
                .map(|n| n.uuid)
                .collect();
            if !nets.is_empty() {
                params.insert("nets".to_owned(), json!(nets));
            }
            steps.push(Step::new("pcb.net_class.add", Value::Object(params)));
        }
        let mut moves: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
        for net in &self.nets {
            let (&to, &from) = (net.class.value(), net.class.base());
            let ClassKey::Existing(class) = to else {
                continue;
            };
            // Deleting a class moves its Nets to the Default class already.
            let by_delete = deleted.contains(&from) && self.is_default(to);
            if to != from && !by_delete {
                moves.entry(class).or_default().push(net.uuid);
            }
        }
        for (class, nets) in moves {
            steps.push(Step::new(
                "pcb.net.set_class",
                json!({"nets": nets, "class": class}),
            ));
        }
        steps
    }
}

/// A class's valid overrides as params; left out ones use the board-wide rule.
fn overrides(row: &ClassRow) -> Map<String, Value> {
    row.overrides
        .iter()
        .filter_map(|(rule, d)| {
            let value = optional(d.value()).ok().flatten()?;
            Some((rule.id().to_owned(), Value::from(value.0)))
        })
        .collect()
}
