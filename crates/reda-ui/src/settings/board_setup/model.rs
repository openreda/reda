//! What Board Setup edits, as drafts: the board-wide rule values, the DRC severities, the
//! Net Classes with their rule overrides, and which classes each Net is in. Every page of
//! the dialog shares one of these, so a check's severity is the same draft on its rule's
//! page and on the DRC page.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use reda_engine::model::{
    self as m, DrcCheck, FabRules, PcbDocument, Project, RuleKind, Severity, SilkMaskMode, Uuid,
    ZoneNameScheme,
};
use reda_engine::pcb::rules::presets::{PresetRules, job_presets, strictest_limit};
use reda_engine::schematic::natural_cmp;
use serde_json::{Map, Value, json};


mod field;
mod grid;
mod stackup;
pub use stackup::{Band, CrossBand, StackupDraft, StackupRow};
pub use field::{
    ANGLES, GRID_FIELDS, GRID_KINDS, RuleField, SPOKES, STYLES, ValueKind, connection_value_name,
    kind_name, style_name, target_name,
};
use field::{by_kind, values_param};
use crate::settings::{Draft, Problem, Step};
use crate::tr;


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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassRow {
    key: ClassKey,
    name: Draft<String>,
    overrides: BTreeMap<RuleField, Draft<String>>,
    /// Each record's `enabled` and comment, by kind.
    meta: BTreeMap<RuleKind, Meta>,
    /// Its rank on the board; `None` for a class added in the dialog.
    rank: Option<u32>,
    /// Deleted in the dialog; it goes on Apply.
    deleted: bool,
}

/// A rule record's `enabled` and comment, as drafted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Meta {
    enabled: Draft<bool>,
    comment: Draft<String>,
}

impl Default for Meta {
    fn default() -> Self {
        Meta {
            enabled: Draft::new(true),
            comment: Draft::default(),
        }
    }
}

impl Meta {
    fn load(&mut self, record: Option<&m::DesignRule>, reset: bool) {
        self.enabled.load(record.is_none_or(|r| r.enabled), reset);
        let comment = record.and_then(|r| r.comment.clone()).unwrap_or_default();
        self.comment.load(comment, reset);
    }

    fn changed(&self) -> bool {
        self.enabled.changed() || self.comment.value().trim() != self.comment.base().trim()
    }

    /// `enabled` and `comment` params for what changed.
    fn params(&self, out: &mut Map<String, Value>) {
        if self.enabled.changed() {
            out.insert("enabled".into(), json!(self.enabled.value()));
        }
        if self.comment.value().trim() != self.comment.base().trim() {
            let comment = self.comment.value().trim();
            let value = if comment.is_empty() {
                Value::Null
            } else {
                json!(comment)
            };
            out.insert("comment".into(), value);
        }
    }
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// The preset's value, as the dialog shows it.
    pub preset: String,
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
    /// The board-wide records' `enabled` and comment, by kind.
    board_meta: BTreeMap<RuleKind, Meta>,
    /// What `silk_to_mask` measures against, board-wide.
    silk_mode: Draft<SilkMaskMode>,
    classes: Vec<ClassRow>,
    nets: Vec<NetRow>,
    next_new: u32,
    /// The Net table's filter, as typed.
    pub net_filter: String,
    /// The Nets selected in the Net table.
    pub selected_nets: BTreeSet<Uuid>,
    /// The Net Classes grid's search, as typed.
    pub grid_search: String,
    /// Whether the grid shows only classes with an override.
    pub only_overrides: bool,
    /// The grid rows selected, for copy, paste and Delete.
    pub selected_rows: BTreeSet<ClassKey>,
    /// "Assign by pattern…" while open: the pattern as typed and the class chosen.
    pub pattern_form: Option<(String, Option<ClassKey>)>,
    /// Grid cells that follow a value typed into their range (see
    /// [`BoardSetupDraft::type_cell`]).
    follow: BTreeSet<(ClassKey, RuleField)>,
    presets: Vec<(Uuid, String, FabRules)>,
    /// The Fab Presets of the Output Jobs for this board.
    job_presets: Vec<PresetRules>,
    preview: Option<FabPreview>,
    /// Whether the top and bottom mask expansions are edited together.
    mask_linked: bool,
    /// How Zones with no typed name are named.
    zone_scheme: Draft<ZoneNameScheme>,
    stackup: StackupDraft,
    /// The edits `Mod+Z` steps back to, oldest first.
    history: Vec<Edits>,
    /// The edits as last checked, the next step back's target once they change.
    settled: Option<Edits>,
}

/// What `Mod+Z` inside Board Setup steps back through: every drafted value, but not the
/// pages' view state (filters, selections, an open preview).
#[derive(Clone, Debug, PartialEq)]
struct Edits {
    rules: BTreeMap<RuleField, Draft<String>>,
    severities: Vec<(DrcCheck, Draft<Severity>)>,
    board_meta: BTreeMap<RuleKind, Meta>,
    silk_mode: Draft<SilkMaskMode>,
    classes: Vec<ClassRow>,
    nets: Vec<NetRow>,
    next_new: u32,
    follow: BTreeSet<(ClassKey, RuleField)>,
    mask_linked: bool,
    zone_scheme: Draft<ZoneNameScheme>,
    stackup: StackupDraft,
}

/// Loads a value's draft from `fresh` text; a typed value that means the same (`0.20`
/// for `0.2`) is not a change.
fn load_text(field: RuleField, draft: &mut Draft<String>, fresh: String, reset: bool) {
    let same = field.parse(draft.value()).ok() == field.parse(&fresh).ok();
    draft.load(fresh, reset || same);
}

/// Whether a value's draft holds a valid value other than the Project's.
fn text_changed(field: RuleField, draft: &Draft<String>) -> Option<Value> {
    field.changed(draft.value(), draft.base())
}

/// How the paste opening is sized board-wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteMode {
    /// Grown or shrunk by a length.
    Absolute,
    /// Its area changed by a percentage.
    Percent,
}

impl BoardSetupDraft {
    /// Takes in the Project as it is now (see [`crate::settings::Page::load`]).
    pub fn load(&mut self, project: &Project, reset: bool) {
        // Values that follow the design are not an edit to step back through.
        let was_settled = self.settled.as_ref() == Some(&self.edits());
        self.load_drafts(project, reset);
        if reset {
            self.history.clear();
        }
        if reset || was_settled {
            self.settled = Some(self.edits());
        }
    }

    fn load_drafts(&mut self, project: &Project, reset: bool) {
        self.presets = project
            .fab_presets()
            .map(|p| (p.uuid, p.name.clone(), p.rules))
            .collect();
        self.job_presets = match project.pcb() {
            Some(pcb) => job_presets(project, pcb),
            None => Vec::new(),
        };
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
                job_presets: std::mem::take(&mut self.job_presets),
                ..BoardSetupDraft::default()
            };
        }
        if reset {
            self.preview = None;
            self.follow.clear();
        }
        let setup = pcb.setup();
        for field in RuleField::ALL {
            let draft = self.rules.entry(field).or_default();
            load_text(field, draft, field.board_text(pcb), reset);
            let kind = field.rule().0;
            self.board_meta
                .entry(kind)
                .or_default()
                .load(Some(pcb.board_rule(kind)), reset);
        }
        if reset {
            self.mask_linked = self.rule(RuleField::MaskTop) == self.rule(RuleField::MaskBottom);
        }
        self.zone_scheme.load(setup.zone_name_scheme, reset);
        let mode = pcb.board_rule(RuleKind::SilkToMask).values.mode;
        self.silk_mode.load(mode.unwrap_or_default(), reset);
        if self.severities.is_empty() {
            self.severities = DrcCheck::ALL
                .iter()
                .map(|&c| (c, Draft::new(setup.severities.get(c))))
                .collect();
        }
        for (check, draft) in &mut self.severities {
            draft.load(setup.severities.get(*check), reset);
        }
        self.stackup.load(project, pcb, reset);
        self.load_classes(pcb, reset);
        self.load_nets(pcb, reset);
    }

    fn edits(&self) -> Edits {
        Edits {
            rules: self.rules.clone(),
            severities: self.severities.clone(),
            board_meta: self.board_meta.clone(),
            silk_mode: self.silk_mode.clone(),
            classes: self.classes.clone(),
            nets: self.nets.clone(),
            next_new: self.next_new,
            follow: self.follow.clone(),
            mask_linked: self.mask_linked,
            zone_scheme: self.zone_scheme.clone(),
            stackup: self.stackup.clone(),
        }
    }

    fn restore(&mut self, edits: Edits) {
        self.rules = edits.rules;
        self.severities = edits.severities;
        self.board_meta = edits.board_meta;
        self.silk_mode = edits.silk_mode;
        self.classes = edits.classes;
        self.nets = edits.nets;
        self.next_new = edits.next_new;
        self.follow = edits.follow;
        self.mask_linked = edits.mask_linked;
        self.zone_scheme = edits.zone_scheme;
        self.stackup = edits.stackup;
    }

    /// Notes the edits as they are now: a change since the last check becomes one step
    /// `Mod+Z` steps back over. The window checks after each frame in which no text
    /// field has the keyboard, so typing into one field is one step.
    pub fn checkpoint(&mut self) {
        let now = self.edits();
        match self.settled.take() {
            Some(before) if before != now => self.history.push(before),
            _ => {}
        }
        self.settled = Some(now);
    }

    /// Steps back over the last pending edit; false when there is none.
    pub fn step_back(&mut self) -> bool {
        self.checkpoint();
        let Some(before) = self.history.pop() else {
            return false;
        };
        self.restore(before.clone());
        self.settled = Some(before);
        true
    }

    /// The Stackup page's drafts.
    pub fn stackup(&self) -> &StackupDraft {
        &self.stackup
    }

    pub fn stackup_mut(&mut self) -> &mut StackupDraft {
        &mut self.stackup
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
                        meta: BTreeMap::new(),
                        rank: Some(class.rank),
                        deleted: false,
                    });
                    self.classes.last_mut().expect("just pushed")
                }
            };
            row.name.load(class.name.clone(), reset);
            row.rank = Some(class.rank);
            if reset {
                row.deleted = false;
            }
            for field in RuleField::ALL {
                let draft = row.overrides.entry(field).or_default();
                load_text(field, draft, field.class_text(pcb, class.uuid), reset);
                let kind = field.rule().0;
                row.meta
                    .entry(kind)
                    .or_default()
                    .load(pcb.class_rule(kind, class.uuid), reset);
            }
        }
        if added || reset {
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

    /// Sets a board-wide value as typed; with the mask sides linked, top and bottom go
    /// together.
    pub fn set_rule(&mut self, rule: RuleField, text: impl Into<String>) {
        let text = text.into();
        if self.mask_linked && matches!(rule, RuleField::MaskTop | RuleField::MaskBottom) {
            for side in [RuleField::MaskTop, RuleField::MaskBottom] {
                self.rules.entry(side).or_default().set(text.clone());
            }
            return;
        }
        self.rules.entry(rule).or_default().set(text);
    }

    /// Whether the top and bottom mask expansions are edited together.
    pub fn mask_linked(&self) -> bool {
        self.mask_linked
    }

    /// Links the mask sides; linking takes the top value for both.
    pub fn set_mask_linked(&mut self, linked: bool) {
        self.mask_linked = linked;
        if linked {
            let top = self.rule(RuleField::MaskTop).to_owned();
            self.set_rule(RuleField::MaskBottom, top);
        }
    }

    /// How the paste opening is sized, as drafted: by a percentage once one is typed.
    pub fn paste_mode(&self) -> PasteMode {
        if self.rule(RuleField::PastePercent).trim().is_empty() {
            PasteMode::Absolute
        } else {
            PasteMode::Percent
        }
    }

    /// Switches how the paste opening is sized: the other value is cleared, and the new
    /// one starts at 0 when empty.
    pub fn set_paste_mode(&mut self, mode: PasteMode) {
        if mode == self.paste_mode() {
            return;
        }
        let (on, off) = match mode {
            PasteMode::Absolute => (RuleField::PasteExpansion, RuleField::PastePercent),
            PasteMode::Percent => (RuleField::PastePercent, RuleField::PasteExpansion),
        };
        self.set_rule(off, "");
        if self.rule(on).trim().is_empty() {
            self.set_rule(on, "0");
        }
    }

    /// How Zones with no typed name are named, as drafted.
    pub fn zone_name_scheme(&self) -> ZoneNameScheme {
        *self.zone_scheme.value()
    }

    pub fn set_zone_name_scheme(&mut self, scheme: ZoneNameScheme) {
        self.zone_scheme.set(scheme);
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
            meta: BTreeMap::new(),
            rank: None,
            deleted: false,
        };
        row.name.set(name);
        self.classes.push(row);
        key
    }

    /// The class's name as typed; empty for an unknown one.
    pub fn class_name(&self, key: ClassKey) -> &str {
        self.class_row(key).map_or("", |r| r.name.value())
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
                let to = rule.seed_text(rules)?;
                let typed = self.rule(rule);
                (rule.parse(typed).ok().flatten() != rule.parse(&to).ok().flatten()).then(|| {
                    RuleChange {
                        rule,
                        board: typed.to_owned(),
                        preset: to,
                    }
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
                // The paste opening is sized one way or the other.
                match change.rule {
                    RuleField::PasteExpansion => self.set_rule(RuleField::PastePercent, ""),
                    RuleField::PastePercent => self.set_rule(RuleField::PasteExpansion, ""),
                    _ => {}
                }
                self.set_rule(change.rule, change.preset);
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
            // The paste opening needs one of its two values.
            let required = match rule {
                RuleField::PasteExpansion | RuleField::PastePercent => false,
                _ => rule.required(),
            };
            if let Err(message) = rule.check(draft.value(), required) {
                problems.push(Problem::new(rule.problem_field(), message));
            }
        }
        problems.extend(self.stackup.problems());
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
                if let Err(message) = rule.check(draft.value(), false) {
                    problems.push(Problem::new(class_field(row.key, &rule.id()), message));
                }
            }
        }
        problems
    }

    /// The values of `kind` drafted for a record (the board-wide one with `None`, else
    /// the class's) that differ from the board, as `pcb.rules.set` takes them.
    fn changed_values(&self, class: Option<&ClassRow>) -> BTreeMap<RuleKind, Map<String, Value>> {
        let drafts = match class {
            None => &self.rules,
            Some(row) => &row.overrides,
        };
        let mut out: BTreeMap<RuleKind, Map<String, Value>> = by_kind(
            drafts
                .iter()
                .filter_map(|(rule, d)| Some((*rule, text_changed(*rule, d)?))),
        )
        .into_iter()
        .map(|(kind, values)| match values_param(&values) {
            Value::Object(map) => (kind, map),
            _ => unreachable!("values_param builds an object"),
        })
        .collect();
        if class.is_none() && self.silk_mode.changed() {
            out.entry(RuleKind::SilkToMask)
                .or_default()
                .insert("mode".into(), json!(self.silk_mode.value()));
        }
        out
    }

    /// The `pcb.rules.set` steps for the board-wide records (`class` `None`) or a class's:
    /// one per kind whose values, `enabled` or comment changed. A class's `enabled` and
    /// comment count only while it keeps a record of that kind.
    fn record_steps(&self, class: Option<(&ClassRow, Uuid)>) -> Vec<Step> {
        let mut values = self.changed_values(class.map(|(row, _)| row));
        let meta = match class {
            None => &self.board_meta,
            Some((row, _)) => &row.meta,
        };
        for (kind, m) in meta {
            let keeps = match class {
                None => true,
                Some((row, _)) => self.has_record(row.key, *kind),
            };
            if m.changed() && keeps {
                values.entry(*kind).or_default();
            }
        }
        values
            .into_iter()
            .map(|(kind, values)| {
                let mut params = Map::new();
                params.insert("kind".into(), json!(kind));
                if let Some((_, uuid)) = class {
                    params.insert("scope".into(), json!({"net_class": uuid}));
                }
                params.insert("values".into(), Value::Object(values));
                if let Some(m) = meta.get(&kind)
                    && class.is_none_or(|(row, _)| self.has_record(row.key, kind))
                {
                    m.params(&mut params);
                }
                Step::new("pcb.rules.set", Value::Object(params))
            })
            .collect()
    }

    /// The Commands that make the PCB Document match the drafts, in order: board-wide
    /// rules, severities, deleted classes, renames, class rules, added classes, the rank
    /// order, then Nets leaving and joining classes.
    pub fn steps(&self) -> Vec<Step> {
        if self.pcb.is_none() {
            return Vec::new();
        }
        let mut steps = self.record_steps(None);
        for (check, draft) in &self.severities {
            if draft.changed() {
                steps.push(Step::new(
                    "drc.set_severity",
                    json!({"check": check, "severity": draft.value()}),
                ));
            }
        }
        steps.extend(self.stackup.steps());
        if self.zone_scheme.changed() {
            steps.push(Step::new(
                "pcb.zone.set_name_scheme",
                json!({"scheme": self.zone_scheme.value()}),
            ));
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
            steps.extend(self.record_steps(Some((row, uuid))));
        }
        for row in self.live_classes().filter(|r| !existing(r)) {
            let rules: Vec<Value> =
                by_kind(
                    row.overrides
                        .iter()
                        .filter_map(|(rule, d)| Some((*rule, rule.parse(d.value()).ok()??))),
                )
                .into_iter()
                .map(|(kind, values)| {
                    let mut rule = Map::new();
                    rule.insert("kind".into(), json!(kind));
                    rule.insert("values".into(), values_param(&values));
                    if let Some(m) = row.meta.get(&kind) {
                        if !*m.enabled.value() {
                            rule.insert("enabled".into(), json!(false));
                        }
                        let comment = m.comment.value().trim();
                        if !comment.is_empty() {
                            rule.insert("comment".into(), json!(comment));
                        }
                    }
                    Value::Object(rule)
                })
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
        if let Some(order) = self.reorder() {
            steps.push(Step::new(
                "pcb.net_class.reorder",
                json!({ "classes": order }),
            ));
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

    /// The classes in the drafted rank order, by UUID (or name, for a class added in the
    /// dialog), when that differs from the order the other steps leave: the board's
    /// classes by rank, then the added ones.
    fn reorder(&self) -> Option<Vec<String>> {
        let drafted: Vec<&ClassRow> = self.live_classes().collect();
        let mut natural = drafted.clone();
        natural.sort_by_key(|r| (r.rank.is_none(), r.rank));
        let same = drafted.iter().zip(&natural).all(|(a, b)| a.key == b.key);
        if same {
            return None;
        }
        Some(
            drafted
                .iter()
                .map(|r| match r.key {
                    ClassKey::Existing(uuid) => uuid.to_string(),
                    _ => r.name.value().trim().to_owned(),
                })
                .collect(),
        )
    }
}
