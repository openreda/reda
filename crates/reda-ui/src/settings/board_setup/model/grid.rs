//! The Net Classes grid and Net table over the drafts: cells with what they inherit,
//! typing that fills a new record's range, `enabled` and comments, rank order, copy and
//! paste, filters, and assigning Nets to classes.

use std::collections::BTreeSet;

use reda_engine::model::geom::Nm;
use reda_engine::model::net_name::name_key;
use reda_engine::model::{Limit, RuleKind, SilkMaskMode, Uuid};

use super::{BoardSetupDraft, ClassKey, GRID_FIELDS, RuleField, strictest_limit};
use crate::tr;

impl BoardSetupDraft {
    /// What `silk_to_mask` measures against, board-wide, as drafted.
    pub fn silk_mode(&self) -> SilkMaskMode {
        *self.silk_mode.value()
    }

    pub fn set_silk_mode(&mut self, mode: SilkMaskMode) {
        self.silk_mode.set(mode);
    }

    /// The strictest limit the job presets set on `field`, which way it limits, and the
    /// preset's name. `None` for a preferred value, or with no job presets.
    pub fn strictest(&self, field: RuleField) -> Option<(Limit, Nm, &str)> {
        let (kind, f) = field.rule();
        strictest_limit(&self.job_presets, kind, f)
    }

    /// The grid's rows: the Default Net Class, then the classes in rank order, filtered
    /// by the search (ignoring case) and, with "show only overrides", to classes that
    /// override a grid value.
    pub fn grid_rows(&self) -> Vec<ClassKey> {
        let search = self.grid_search.trim().to_lowercase();
        let matches = |name: &str| name.to_lowercase().contains(&search);
        let mut rows = Vec::new();
        if matches(&tr!("board-setup-default-class-name")) {
            rows.push(ClassKey::Default);
        }
        for row in self.live_classes() {
            let overrides = GRID_FIELDS
                .iter()
                .any(|f| !self.cell(row.key, *f).trim().is_empty());
            if matches(row.name.value()) && (!self.only_overrides || overrides) {
                rows.push(row.key);
            }
        }
        rows
    }

    /// Every row in order, unfiltered.
    fn all_rows(&self) -> Vec<ClassKey> {
        std::iter::once(ClassKey::Default)
            .chain(self.live_classes().map(|r| r.key))
            .collect()
    }

    /// A cell as typed: the board-wide value on the Default row, else the class's
    /// override (empty when it inherits).
    pub fn cell(&self, key: ClassKey, field: RuleField) -> &str {
        match key {
            ClassKey::Default => self.rule(field),
            key => self.class_override(key, field),
        }
    }

    /// What a class cell shows greyed when empty: the board-wide value as drafted.
    pub fn inherited(&self, key: ClassKey, field: RuleField) -> String {
        match key {
            ClassKey::Default => String::new(),
            _ => self.rule(field).trim().to_owned(),
        }
    }

    fn set_cell(&mut self, key: ClassKey, field: RuleField, text: &str) {
        match key {
            ClassKey::Default => self.set_rule(field, text),
            key => self.set_class_override(key, field, text),
        }
    }

    /// Whether the row has (or will have, as drafted) a record of `kind`: always on the
    /// Default row, else while a value of that kind is typed.
    pub fn has_record(&self, key: ClassKey, kind: RuleKind) -> bool {
        key == ClassKey::Default
            || RuleField::ALL
                .iter()
                .filter(|f| f.rule().0 == kind)
                .any(|f| !self.class_override(key, *f).trim().is_empty())
    }

    /// Whether the board has a record of `kind` for the class now (not as drafted).
    fn had_record(&self, key: ClassKey, kind: RuleKind) -> bool {
        match key {
            ClassKey::Default => true,
            key => self.class_row(key).is_some_and(|row| {
                row.overrides
                    .iter()
                    .any(|(f, d)| f.rule().0 == kind && !d.base().trim().is_empty())
            }),
        }
    }

    /// Types `text` into a cell. Typing a minimum or preferred value for a class with no
    /// record of that kind fills the range's empty values with the same value, and they
    /// keep following it while it is typed, until typed into themselves.
    pub fn type_cell(&mut self, key: ClassKey, field: RuleField, text: &str) {
        self.set_cell(key, field, text);
        self.follow.remove(&(key, field));
        if key == ClassKey::Default || field.is_max() || field.range().is_empty() {
            return;
        }
        if self.had_record(key, field.rule().0) {
            return;
        }
        for &other in field.range() {
            if other == field {
                continue;
            }
            let empty = self.cell(key, other).trim().is_empty();
            if empty || self.follow.contains(&(key, other)) {
                self.set_cell(key, other, text);
                self.follow.insert((key, other));
            }
        }
    }

    /// Whether the row's record of `kind` counts, as drafted.
    pub fn enabled(&self, key: ClassKey, kind: RuleKind) -> bool {
        let meta = match key {
            ClassKey::Default => self.board_meta.get(&kind),
            key => self.class_row(key).and_then(|r| r.meta.get(&kind)),
        };
        meta.is_none_or(|m| *m.enabled.value())
    }

    fn meta_mut(&mut self, key: ClassKey, kind: RuleKind) -> Option<&mut super::Meta> {
        match key {
            ClassKey::Default => Some(self.board_meta.entry(kind).or_default()),
            key => self
                .class_row_mut(key)
                .map(|r| r.meta.entry(kind).or_default()),
        }
    }

    pub fn set_enabled(&mut self, key: ClassKey, kind: RuleKind, enabled: bool) {
        if let Some(meta) = self.meta_mut(key, kind) {
            meta.enabled.set(enabled);
        }
    }

    /// The comment on the row's record of `kind`, as typed.
    pub fn comment(&self, key: ClassKey, kind: RuleKind) -> &str {
        let meta = match key {
            ClassKey::Default => self.board_meta.get(&kind),
            key => self.class_row(key).and_then(|r| r.meta.get(&kind)),
        };
        meta.map_or("", |m| m.comment.value())
    }

    pub fn comment_mut(&mut self, key: ClassKey, kind: RuleKind) -> Option<&mut String> {
        self.meta_mut(key, kind).map(|m| m.comment.value_mut())
    }

    pub fn set_comment(&mut self, key: ClassKey, kind: RuleKind, comment: &str) {
        if let Some(meta) = self.meta_mut(key, kind) {
            meta.comment.set(comment.to_owned());
        }
    }

    /// Moves a class to `index` in rank order (0 ranks highest); the Default Net Class
    /// always ranks lowest and does not move.
    pub fn move_class(&mut self, key: ClassKey, index: usize) {
        let Some(from) = self.classes.iter().position(|r| r.key == key && !r.deleted) else {
            return;
        };
        let row = self.classes.remove(from);
        let at = self
            .classes
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.deleted)
            .nth(index)
            .map_or(self.classes.len(), |(i, _)| i);
        self.classes.insert(at, row);
    }

    /// The selected rows' grid values as tab-separated lines, top to bottom.
    pub fn copy_rows(&self) -> String {
        self.all_rows()
            .into_iter()
            .filter(|k| self.selected_rows.contains(k))
            .map(|k| {
                GRID_FIELDS
                    .iter()
                    .map(|f| self.cell(k, *f).trim())
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Pastes tab-separated lines into the grid: the first into the first selected row,
    /// each next one into the row below, cell by cell from the first column.
    pub fn paste_rows(&mut self, text: &str) {
        let rows = self.all_rows();
        let Some(start) = rows.iter().position(|k| self.selected_rows.contains(k)) else {
            return;
        };
        let lines = text.trim_end_matches(['\n', '\r']).lines();
        for (key, line) in rows[start..].iter().zip(lines) {
            for (field, value) in GRID_FIELDS
                .iter()
                .zip(line.trim_end_matches('\r').split('\t'))
            {
                self.set_cell(*key, *field, value.trim());
                self.follow.remove(&(*key, *field));
            }
        }
    }

    /// Delete: clears the selected classes' grid values back to inherited.
    pub fn clear_rows(&mut self) {
        let keys: Vec<ClassKey> = self
            .selected_rows
            .iter()
            .copied()
            .filter(|k| *k != ClassKey::Default)
            .collect();
        for key in keys {
            for field in GRID_FIELDS {
                self.set_cell(key, field, "");
                self.follow.remove(&(key, field));
            }
        }
    }

    /// Adds a class holding the selected Nets.
    pub fn new_class_from_selection(&mut self) -> ClassKey {
        let key = self.add_class();
        let selected = self.selected_nets.clone();
        for net in &mut self.nets {
            if selected.contains(&net.uuid) {
                let mut classes = net.classes.value().clone();
                classes.insert(key);
                net.classes.set(classes);
            }
        }
        key
    }

    /// Adds the Nets whose names match the glob `pattern` now (`*` and `?`, ignoring
    /// case) to the class `key`; returns how many joined it.
    pub fn assign_by_pattern(&mut self, pattern: &str, key: ClassKey) -> usize {
        if key == ClassKey::Default || self.class_row(key).is_none() {
            return 0;
        }
        let pattern = name_key(pattern.trim());
        let mut joined = 0;
        for net in &mut self.nets {
            if reda_engine::pcb::glob_match(&pattern, &name_key(&net.name))
                && !net.classes.value().contains(&key)
            {
                let mut classes = net.classes.value().clone();
                classes.insert(key);
                net.classes.set(classes);
                joined += 1;
            }
        }
        joined
    }

    /// Puts `net` in or takes it out of the class `key`; the Default Net Class takes it
    /// out of every other.
    pub fn toggle_net_class(&mut self, net: Uuid, key: ClassKey) {
        if key != ClassKey::Default && self.class_row(key).is_none() {
            return;
        }
        if let Some(row) = self.nets.iter_mut().find(|n| n.uuid == net) {
            let mut classes: BTreeSet<ClassKey> = row.classes.value().clone();
            match key {
                ClassKey::Default => classes.clear(),
                key if classes.contains(&key) => {
                    classes.remove(&key);
                }
                key => {
                    classes.insert(key);
                }
            }
            row.classes.set(classes);
        }
    }
}
