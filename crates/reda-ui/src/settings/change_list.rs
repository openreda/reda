//! The Change List window: Update PCB from Schematic. It shows the pending Sync changes
//! (what `sync.preview` returns, which the Shell keeps after every edit) as a tree grid,
//! one group per kind, every change ticked. Unticking a change also unticks every change
//! that requires it, and ticking one ticks what it requires. Problem rows can't be ticked
//! and show why; Notes show what follows. Apply runs one `sync.apply` with the unticked
//! ids left out; unapplied changes simply show up again. A standing `duplicate_nets`
//! Violation shows a banner and leaves nothing to apply.
//!
//! The toolbar filters by text and to Problem and Note rows, expands and collapses the
//! groups and includes or excludes every row shown. Rows multi-select; Space toggles
//! their ticks, and their context menu includes, excludes or inverts them, ticks their
//! whole kind, or Shows them on the board or the sheet. Export writes the list through
//! `checks.report`. After Apply the window stays open and the applied rows stay, greyed,
//! as Applied, until Close.

use std::collections::BTreeSet;
use std::path::Path;

use egui::Modifiers;
use reda_engine::model::{Project, Uuid};
use reda_engine::sync::{self, Change, ChangeList, GroupKind, Status};
use serde_json::json;

use super::{FileRequest, Page, PageCx, Problem, SettingsDialog, Step};
use crate::shell::Shell;
use crate::tr;

mod cells;
mod draw;
mod sides;

pub use cells::{Cells, describe, group_title};
pub use sides::sides;

/// The Change List dialog on `project`.
pub fn dialog(project: &Project) -> SettingsDialog {
    SettingsDialog::new(
        tr!("change-list-title"),
        vec![Box::new(ChangeListPage::default())],
        project,
    )
    .apply_and_close()
}

/// The dialog's one page: the Change List and which changes the user unticked.
#[derive(Clone, Debug)]
pub struct ChangeListPage {
    list: ChangeList,
    /// The changes the user unticked. Their dependants are left out too
    /// ([`ChangeList::cascade`]), without being listed here.
    unticked: BTreeSet<String>,
    /// PCB Net names by UUID, for a merge target.
    pcb_nets: Vec<(Uuid, String)>,
    /// The changes earlier Applies carried out, shown greyed until Close.
    applied: Vec<Change>,
    /// The text filter, matched against every column, ignoring case.
    filter: String,
    /// Only Problem and Note rows.
    only_problems: bool,
    /// The groups folded away.
    collapsed: BTreeSet<GroupKind>,
    /// The selected rows, and the row a Shift+click extends from.
    selected: BTreeSet<String>,
    anchor: Option<String>,
    /// What the last Export said.
    notice: Option<String>,
    /// The last Apply found the list stale (`conflict`); the banner shows until the next
    /// Apply.
    stale: bool,
}

impl Default for ChangeListPage {
    fn default() -> ChangeListPage {
        ChangeListPage {
            list: ChangeList {
                hash: String::new(),
                count: 0,
                groups: Vec::new(),
                duplicate_nets: None,
            },
            unticked: BTreeSet::new(),
            pcb_nets: Vec::new(),
            applied: Vec::new(),
            filter: String::new(),
            only_problems: false,
            collapsed: BTreeSet::new(),
            selected: BTreeSet::new(),
            anchor: None,
            notice: None,
            stale: false,
        }
    }
}

impl ChangeListPage {
    /// The Change List shown.
    pub fn list(&self) -> &ChangeList {
        &self.list
    }

    /// Every change left out: the unticked ones and those that require them.
    pub fn excluded(&self) -> BTreeSet<String> {
        self.list.cascade(&self.unticked)
    }

    pub fn is_ticked(&self, id: &str) -> bool {
        !self.excluded().contains(id)
    }

    /// How many changes Apply carries out.
    pub fn ticked_count(&self) -> usize {
        if self.list.duplicate_nets.is_some() {
            return 0;
        }
        let excluded = self.excluded();
        self.list
            .changes()
            .filter(|c| !excluded.contains(&c.id))
            .count()
    }

    /// Ticks or unticks change `id`. Unticking leaves out its dependants as well;
    /// ticking also ticks every change it requires.
    pub fn set_ticked(&mut self, id: &str, ticked: bool) {
        if !self.tickable(id) {
            return;
        }
        if !ticked {
            self.unticked.insert(id.to_owned());
            return;
        }
        let mut todo = vec![id.to_owned()];
        let mut seen = BTreeSet::new();
        while let Some(id) = todo.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            self.unticked.remove(&id);
            if let Some(change) = self.list.changes().find(|c| c.id == id) {
                todo.extend(change.requires.iter().cloned());
            }
        }
    }

    /// Takes a newer Change List (the Shell's after an edit), keeping the unticks of the
    /// changes it still has.
    pub fn follow(&mut self, list: Option<&ChangeList>) {
        let Some(list) = list else {
            self.list = ChangeListPage::default().list;
            self.unticked.clear();
            return;
        };
        if list.hash == self.list.hash {
            return;
        }
        self.list = list.clone();
        let ids: BTreeSet<&str> = self.list.changes().map(|c| c.id.as_str()).collect();
        self.unticked.retain(|id| ids.contains(id.as_str()));
    }

    fn change(&self, id: &str) -> Option<&Change> {
        self.list.changes().find(|c| c.id == id)
    }

    /// Whether row `id` is a pending change that can be ticked.
    fn tickable(&self, id: &str) -> bool {
        self.change(id).is_some_and(|c| !c.is_problem())
    }

    /// The changes earlier Applies carried out, in the order they were listed.
    pub fn applied(&self) -> &[Change] {
        &self.applied
    }

    fn is_applied(&self, id: &str) -> bool {
        self.change(id).is_none() && self.applied.iter().any(|c| c.id == id)
    }

    /// The columns of row `id`, pending or Applied.
    pub fn cells(&self, id: &str) -> Option<Cells> {
        match self.change(id) {
            Some(change) => Some(cells::cells(change, &self.pcb_nets, false)),
            None => (self.applied.iter().find(|c| c.id == id))
                .map(|c| cells::cells(c, &self.pcb_nets, true)),
        }
    }

    /// The rows of `group` in order, pending first and then Applied, whether or not they
    /// pass the filter.
    fn group_rows(&self, group: GroupKind) -> Vec<&Change> {
        let pending = (self.list.groups.iter())
            .filter(|g| g.group == group)
            .flat_map(|g| &g.changes);
        let applied = (self.applied.iter())
            .filter(|c| c.change.group() == group && self.change(&c.id).is_none());
        pending.chain(applied).collect()
    }

    /// Whether row `change` passes the text filter and Only Problems.
    fn passes(&self, change: &Change) -> bool {
        if self.only_problems && (change.status == Status::Ok || self.is_applied(&change.id)) {
            return false;
        }
        let needle = self.filter.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        let cells = self.cells(&change.id).expect("a listed row");
        [
            describe(change, &self.pcb_nets),
            cells.kind,
            cells.target,
            cells.from_to,
            cells.status,
        ]
        .iter()
        .any(|text| text.to_lowercase().contains(&needle))
    }

    /// The groups with a row shown, in order.
    pub fn groups(&self) -> Vec<GroupKind> {
        (GroupKind::ALL.into_iter())
            .filter(|g| self.group_rows(*g).iter().any(|c| self.passes(c)))
            .collect()
    }

    /// The rows of `group` that the filter shows.
    pub fn rows_of(&self, group: GroupKind) -> Vec<String> {
        (self.group_rows(group).into_iter())
            .filter(|c| self.passes(c))
            .map(|c| c.id.clone())
            .collect()
    }

    /// Every row the filter shows, in order, folded groups' too.
    pub fn rows(&self) -> Vec<String> {
        GroupKind::ALL
            .into_iter()
            .flat_map(|g| self.rows_of(g))
            .collect()
    }

    /// The rows on screen, in order: those of unfolded groups.
    fn rows_on_screen(&self) -> Vec<String> {
        (GroupKind::ALL.into_iter())
            .filter(|g| self.is_expanded(*g))
            .flat_map(|g| self.rows_of(g))
            .collect()
    }

    /// A group header's count: its pending rows the filter shows.
    pub fn group_count(&self, group: GroupKind) -> usize {
        (self.rows_of(group).iter())
            .filter(|id| self.change(id).is_some())
            .count()
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn set_filter(&mut self, text: &str) {
        self.filter = text.to_owned();
    }

    pub fn only_problems(&self) -> bool {
        self.only_problems
    }

    /// Shows only Problem and Note rows, or every row again.
    pub fn set_only_problems(&mut self, on: bool) {
        self.only_problems = on;
    }

    pub fn is_expanded(&self, group: GroupKind) -> bool {
        !self.collapsed.contains(&group)
    }

    pub fn set_expanded(&mut self, group: GroupKind, expanded: bool) {
        if expanded {
            self.collapsed.remove(&group);
        } else {
            self.collapsed.insert(group);
        }
    }

    pub fn expand_all(&mut self) {
        self.collapsed.clear();
    }

    pub fn collapse_all(&mut self) {
        self.collapsed = GroupKind::ALL.into_iter().collect();
    }

    /// Ticks or unticks each of `ids` that can be ticked: unticks first, so ticking one
    /// back ticks what it requires.
    fn set_all(&mut self, ids: &[String], ticked: bool) {
        for id in ids {
            self.set_ticked(id, ticked);
        }
    }

    /// Include All: ticks every row shown.
    pub fn include_all(&mut self) {
        self.set_all(&self.rows(), true);
    }

    /// Exclude All: unticks every row shown.
    pub fn exclude_all(&mut self) {
        self.set_all(&self.rows(), false);
    }

    /// The tick-all box of `group`'s header: ticks or unticks its rows shown.
    pub fn set_kind(&mut self, group: GroupKind, ticked: bool) {
        self.set_all(&self.rows_of(group), ticked);
    }

    /// Selects row `id`: alone, or added and removed with `Mod`, or the range from the
    /// last row clicked with `Shift`.
    pub fn select(&mut self, id: &str, modifiers: Modifiers) {
        if modifiers.shift
            && let Some(anchor) = &self.anchor
        {
            let rows = self.rows_on_screen();
            let at = |id: &str| rows.iter().position(|r| r == id);
            if let (Some(a), Some(b)) = (at(anchor), at(id)) {
                let (lo, hi) = (a.min(b), a.max(b));
                if !modifiers.command {
                    self.selected.clear();
                }
                self.selected.extend(rows[lo..=hi].iter().cloned());
                return;
            }
        }
        if modifiers.command {
            if !self.selected.remove(id) {
                self.selected.insert(id.to_owned());
            }
        } else {
            self.selected = BTreeSet::from([id.to_owned()]);
        }
        self.anchor = Some(id.to_owned());
    }

    /// Whether row `id` is selected.
    pub fn is_selected(&self, id: &str) -> bool {
        self.selected.contains(id)
    }

    /// The selected rows that are shown, in order.
    pub fn selected(&self) -> Vec<String> {
        (self.rows().into_iter())
            .filter(|id| self.selected.contains(id))
            .collect()
    }

    fn selected_tickable(&self) -> Vec<String> {
        (self.selected().into_iter())
            .filter(|id| self.tickable(id))
            .collect()
    }

    /// Space: unticks the selected rows when all of them are ticked, else ticks them all.
    pub fn toggle_selected(&mut self) {
        let ids = self.selected_tickable();
        let all = ids.iter().all(|id| self.is_ticked(id));
        self.set_all(&ids, !all);
    }

    pub fn include_selected(&mut self) {
        self.set_all(&self.selected_tickable(), true);
    }

    pub fn exclude_selected(&mut self) {
        self.set_all(&self.selected_tickable(), false);
    }

    /// Invert: flips the tick of each selected row.
    pub fn invert_selected(&mut self) {
        let ids = self.selected_tickable();
        let (on, off): (Vec<String>, Vec<String>) =
            ids.into_iter().partition(|id| !self.is_ticked(id));
        self.set_all(&off, false);
        self.set_all(&on, true);
    }

    /// Include or Exclude All of This Kind: every row of the selected rows' groups.
    pub fn set_kind_of_selected(&mut self, ticked: bool) {
        let groups: BTreeSet<GroupKind> = (self.selected().iter())
            .filter_map(|id| self.change(id))
            .map(|c| c.change.group())
            .collect();
        for group in groups {
            self.set_kind(group, ticked);
        }
    }

    /// Whether the last Apply found the list stale, which the window's banner says.
    pub fn is_stale(&self) -> bool {
        self.stale
    }

    /// What the last Export said.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Show: the main window on what row `id` concerns, the board first.
    pub fn show_row(&self, shell: &mut Shell, id: &str) {
        let change = self
            .change(id)
            .or_else(|| self.applied.iter().find(|c| c.id == id));
        let Some(change) = change else {
            return;
        };
        let Some(project) = shell.engine().project() else {
            return;
        };
        let sides = sides(project, change);
        shell.show_change(sides);
    }

    /// Export: writes the Change List to `path` through `checks.report`, as JSON for a
    /// `.json` file and HTML otherwise.
    fn export(&mut self, shell: &mut Shell, path: &Path) {
        let format = match path.extension().and_then(|e| e.to_str()) {
            Some(e) if e.eq_ignore_ascii_case("json") => "json",
            _ => "html",
        };
        let params = json!({"kind": "sync", "format": format, "path": path});
        self.notice = Some(match shell.run("checks.report", params) {
            Ok(_) => tr!("change-list-exported", path = path.display().to_string()),
            Err(e) => tr!("error-command", error = e.message),
        });
    }
}

impl Page for ChangeListPage {
    fn title(&self) -> String {
        tr!("change-list-page")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        // Planning is not free, so only on opening and after Apply; edits made while the
        // window is open arrive through the Shell's list (`follow`, as it draws).
        if reset {
            self.stale = false;
            let fresh = sync::plan(project).unwrap_or_else(|| ChangeListPage::default().list);
            // After an Apply, the ticked changes no longer pending were carried out.
            let excluded = self.excluded();
            let still: BTreeSet<&str> = fresh.changes().map(|c| c.id.as_str()).collect();
            let done: Vec<Change> = (self.list.changes())
                .filter(|c| !c.is_problem() && !excluded.contains(&c.id))
                .filter(|c| !still.contains(c.id.as_str()))
                .cloned()
                .collect();
            self.applied.retain(|c| !done.iter().any(|d| d.id == c.id));
            self.applied.extend(done);
            self.unticked.retain(|id| still.contains(id.as_str()));
            self.list = fresh;
        }
        self.pcb_nets = project
            .pcb()
            .map(|pcb| pcb.nets().map(|n| (n.uuid, n.name.clone())).collect())
            .unwrap_or_default();
    }

    fn steps(&self) -> Vec<Step> {
        if self.ticked_count() == 0 {
            return Vec::new();
        }
        let exclude: Vec<&String> = self.unticked.iter().collect();
        vec![Step::new(
            "sync.apply",
            json!({"hash": self.list.hash, "exclude": exclude}),
        )]
    }

    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }

    /// Nothing was applied: the list is planned afresh, keeping the unticks of the changes
    /// it still has, and the banner says so.
    fn conflict(&mut self, project: &Project) -> bool {
        let fresh = sync::plan(project).unwrap_or_else(|| ChangeListPage::default().list);
        self.follow(Some(&fresh));
        self.stale = true;
        true
    }

    fn show(&mut self, ui: &mut egui::Ui, cx: &mut PageCx<'_>) {
        self.follow(cx.shell.change_list());
        self.draw(ui, cx);
    }

    fn footer(&mut self, ui: &mut egui::Ui) -> Option<FileRequest> {
        self.draw_footer(ui)
    }

    fn picked_file(&mut self, shell: &mut Shell, request: FileRequest, path: &Path) {
        if request == FileRequest::ExportChangeList {
            self.export(shell, path);
        }
    }
}
