//! Undo stacks and dirty state: one stack per Document plus the Project's own stack for
//! `.reda`, all fed by the change sets of committed edits.

use std::collections::{BTreeMap, VecDeque};

use reda_model::{ChangeSet, Scope};

/// One undo or redo step on one stack: the part of a committed edit that landed in that
/// stack's scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Unique within the [`History`]; names the state the entry leaves behind.
    id: u64,
    /// What the user sees in the Edit menu, such as "Add Wire".
    pub label: String,
    /// Applying this redoes the step; applying it inverted undoes it.
    pub changes: ChangeSet,
}

/// One scope's undo stack, redo stack and saved marker.
#[derive(Clone, Debug)]
struct Stack {
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    /// The state when `undo` is empty: 0 at the start, or the id of the last entry the
    /// depth cap dropped off the bottom.
    base: u64,
    /// The state last written to disk, or `None` if this state never was.
    saved: Option<u64>,
}

impl Default for Stack {
    fn default() -> Stack {
        Stack {
            undo: VecDeque::new(),
            redo: Vec::new(),
            base: 0,
            saved: Some(0),
        }
    }
}

impl Stack {
    fn state(&self) -> u64 {
        self.undo.back().map_or(self.base, |e| e.id)
    }

    fn is_dirty(&self) -> bool {
        self.saved != Some(self.state())
    }

    fn trim(&mut self, depth: Option<usize>) {
        let Some(depth) = depth else { return };
        while self.undo.len() > depth {
            let dropped = self.undo.pop_front().expect("longer than the depth");
            self.base = dropped.id;
        }
    }
}

/// Every undo stack of the open Project and its dirty state.
///
/// A committed edit is [`History::record`]ed: its change set is split by scope and each
/// part becomes one entry on that scope's stack, so a cross-Document edit is undone one
/// Document at a time. A scope is dirty when its stack is not at the state last saved,
/// so undoing back to the saved state makes it clean again.
#[derive(Clone, Debug, Default)]
pub struct History {
    stacks: BTreeMap<Scope, Stack>,
    depth: Option<usize>,
    last_id: u64,
}

impl History {
    /// Records a committed edit: one entry per touched scope, each clearing that scope's
    /// redo stack. An empty change set records nothing.
    pub fn record(&mut self, label: &str, changes: &ChangeSet) {
        // A new Document's file is not on disk yet.
        for document in changes.created_documents() {
            self.mark_unsaved(Scope::Document(document));
        }
        for (scope, part) in changes.split_by_stack() {
            let entry = self.entry(label, part);
            let stack = self.stacks.entry(scope).or_default();
            stack.redo.clear();
            stack.undo.push_back(entry);
            stack.trim(self.depth);
        }
    }

    /// The entry [`History::undone`] would move, if any.
    pub fn next_undo(&self, scope: Scope) -> Option<&Entry> {
        self.stacks.get(&scope)?.undo.back()
    }

    /// The entry [`History::redone`] would move, if any.
    pub fn next_redo(&self, scope: Scope) -> Option<&Entry> {
        self.stacks.get(&scope)?.redo.last()
    }

    /// Moves the top undo entry to the redo stack, once its inverse has been applied.
    pub fn undone(&mut self, scope: Scope) {
        if let Some(stack) = self.stacks.get_mut(&scope)
            && let Some(entry) = stack.undo.pop_back()
        {
            stack.redo.push(entry);
        }
    }

    /// Moves the top redo entry back to the undo stack, once it has been applied again.
    pub fn redone(&mut self, scope: Scope) {
        if let Some(stack) = self.stacks.get_mut(&scope)
            && let Some(entry) = stack.redo.pop()
        {
            stack.undo.push_back(entry);
            stack.trim(self.depth);
        }
    }

    /// The cap on each stack's undo entries; `None`, the default, is unlimited.
    pub fn depth(&self) -> Option<usize> {
        self.depth
    }

    /// Sets the cap, dropping the oldest entries of any stack that is now too deep.
    pub fn set_depth(&mut self, depth: Option<usize>) {
        self.depth = depth;
        for stack in self.stacks.values_mut() {
            stack.trim(depth);
        }
    }

    /// Forgets every stack and saved marker, keeping the depth: for a newly opened Project.
    /// Open Library elements' stacks stay, since the elements stay open.
    pub fn clear(&mut self) {
        self.stacks
            .retain(|scope, _| matches!(scope, Scope::Element(_)));
    }

    /// Forgets one scope's stack and saved marker: for a Library element opened or closed.
    pub fn forget(&mut self, scope: Scope) {
        self.stacks.remove(&scope);
    }

    /// Names `scope`'s current state: it changes with every edit, undo and redo on that
    /// stack, so a front end can tell which scopes changed since it last looked.
    pub fn state(&self, scope: Scope) -> u64 {
        self.stacks.get(&scope).map_or(0, Stack::state)
    }

    /// Whether `scope` has changes not yet saved.
    pub fn is_dirty(&self, scope: Scope) -> bool {
        self.stacks.get(&scope).is_some_and(Stack::is_dirty)
    }

    /// Every scope with changes not yet saved, sorted with the Project first.
    pub fn dirty_scopes(&self) -> Vec<Scope> {
        let dirty = self.stacks.iter().filter(|(_, s)| s.is_dirty());
        dirty.map(|(scope, _)| *scope).collect()
    }

    /// Marks `scope`'s current state as written to disk.
    pub fn mark_saved(&mut self, scope: Scope) {
        let stack = self.stacks.entry(scope).or_default();
        stack.saved = Some(stack.state());
    }

    /// Marks the Project file's and every Document's current state as written to disk.
    /// Open Library elements save to their own Libraries, so they are left as they are.
    pub fn mark_all_saved(&mut self) {
        for (scope, stack) in &mut self.stacks {
            if !matches!(scope, Scope::Element(_)) {
                stack.saved = Some(stack.state());
            }
        }
    }

    /// Marks `scope` as never saved, such as every file of a new Project.
    pub fn mark_unsaved(&mut self, scope: Scope) {
        self.stacks.entry(scope).or_default().saved = None;
    }

    /// Where the history stands now, for [`History::prefix_labels_since`].
    pub fn mark(&self) -> u64 {
        self.last_id
    }

    /// Puts `prefix` before the label of every entry recorded since `mark`, such as
    /// "MCP: " for an edit a bridged agent made.
    pub fn prefix_labels_since(&mut self, mark: u64, prefix: &str) {
        let stacks = self.stacks.values_mut();
        let entries = stacks.flat_map(|s| s.undo.iter_mut().chain(s.redo.iter_mut()));
        for entry in entries.filter(|e| e.id > mark) {
            entry.label.insert_str(0, prefix);
        }
    }

    fn entry(&mut self, label: &str, changes: ChangeSet) -> Entry {
        self.last_id += 1;
        Entry {
            id: self.last_id,
            label: label.to_owned(),
            changes,
        }
    }
}
