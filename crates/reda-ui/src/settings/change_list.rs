//! The Change List dialog: Update PCB from Schematic. It shows the pending Sync changes
//! (what `sync.preview` returns, which the Shell keeps after every edit) grouped by kind,
//! every one ticked; unticking a change also unticks every change that requires it, and
//! ticking one ticks what it requires. Apply runs one `sync.apply` with the unticked ids
//! left out; unapplied changes simply show up again.

use std::collections::BTreeSet;

use egui::{Checkbox, Role, Ui, WidgetInfo};
use reda_engine::model::{Fields, Project, Uuid};
use reda_engine::sync::{self, Change, ChangeKind, ChangeList, GroupKind, NetRef};
use serde_json::json;

use super::{Page, PageCx, Problem, SettingsDialog, Step};
use crate::tr;

/// The Change List dialog on `project`.
pub fn dialog(project: &Project) -> SettingsDialog {
    SettingsDialog::new(
        tr!("change-list-title"),
        vec![Box::new(ChangeListPage::default())],
        project,
    )
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
}

impl Default for ChangeListPage {
    fn default() -> ChangeListPage {
        ChangeListPage {
            list: ChangeList {
                hash: String::new(),
                count: 0,
                groups: Vec::new(),
            },
            unticked: BTreeSet::new(),
            pcb_nets: Vec::new(),
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
        self.list.count - self.excluded().len()
    }

    /// Ticks or unticks change `id`. Unticking leaves out its dependants as well;
    /// ticking also ticks every change it requires.
    pub fn set_ticked(&mut self, id: &str, ticked: bool) {
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
            *self = ChangeListPage::default();
            return;
        };
        if list.hash == self.list.hash {
            return;
        }
        self.list = list.clone();
        let ids: BTreeSet<&str> = self.list.changes().map(|c| c.id.as_str()).collect();
        self.unticked.retain(|id| ids.contains(id.as_str()));
    }
}

impl Page for ChangeListPage {
    fn title(&self) -> String {
        tr!("change-list-page")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        // Planning is not free, so only on opening and after Apply; edits made while the
        // dialog is open arrive through the Shell's list (`follow`, as it draws).
        if reset {
            self.list = sync::plan(project).unwrap_or_else(|| ChangeListPage::default().list);
            self.unticked.clear();
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

    fn show(&mut self, ui: &mut Ui, cx: &mut PageCx<'_>) {
        self.follow(cx.shell.change_list());
        if self.list.count == 0 {
            ui.label(tr!("change-list-in-sync"));
            return;
        }
        // Worked out once a frame: the cascade walks the whole list.
        let excluded = self.excluded();
        ui.label(tr!(
            "change-list-summary",
            ticked = self.list.count - excluded.len(),
            count = self.list.count
        ));
        let mut toggled: Vec<(String, bool)> = Vec::new();
        let groups = self.list.groups.clone();
        for group in &groups {
            ui.add_space(6.0);
            let title = group_title(group.group);
            ui.strong(tr!(
                "change-list-group",
                group = title.as_str(),
                count = group.changes.len()
            ));
            let all = group.changes.iter().all(|c| !excluded.contains(&c.id));
            let none = group.changes.iter().all(|c| excluded.contains(&c.id));
            let mut check = all;
            let response = ui.add(
                Checkbox::new(&mut check, tr!("change-list-tick-all")).indeterminate(!all && !none),
            );
            response.widget_info(|| {
                WidgetInfo::selected(
                    Role::CheckBox,
                    true,
                    all,
                    tr!("change-list-tick-group", group = title.as_str()),
                )
            });
            if response.clicked() {
                toggled.extend(group.changes.iter().map(|c| (c.id.clone(), check)));
            }
            ui.indent(("change-list-group", group.group as u8), |ui| {
                for change in &group.changes {
                    let mut ticked = !excluded.contains(&change.id);
                    let text = describe(change, &self.pcb_nets);
                    if ui.checkbox(&mut ticked, text).clicked() {
                        toggled.push((change.id.clone(), ticked));
                    }
                }
            });
        }
        for (id, ticked) in toggled {
            self.set_ticked(&id, ticked);
        }
    }
}

fn group_title(group: GroupKind) -> String {
    match group {
        GroupKind::Components => tr!("change-list-group-components"),
        GroupKind::Footprints => tr!("change-list-group-footprints"),
        GroupKind::ComponentData => tr!("change-list-group-component-data"),
        GroupKind::Nets => tr!("change-list-group-nets"),
        GroupKind::PadNets => tr!("change-list-group-pad-nets"),
    }
}

/// One line describing `change`.
pub fn describe(change: &Change, pcb_nets: &[(Uuid, String)]) -> String {
    match &change.change {
        ChangeKind::AddComponent {
            designator,
            footprint_name,
            ..
        } => tr!(
            "change-add-component",
            designator = designator.as_str(),
            footprint = footprint_name.as_str()
        ),
        ChangeKind::RemoveComponent { designator, .. } => {
            tr!("change-remove-component", designator = designator.as_str())
        }
        ChangeKind::ChangeFootprint {
            designator,
            from,
            to,
            ..
        } => tr!(
            "change-footprint",
            designator = designator.as_str(),
            from = from.name.as_str(),
            to = to.name.as_str()
        ),
        ChangeKind::Designator { from, to, .. } => {
            tr!("change-designator", from = from.as_str(), to = to.as_str())
        }
        ChangeKind::Fields {
            designator,
            from,
            to,
            ..
        } => tr!(
            "change-fields",
            designator = designator.as_str(),
            fields = changed_fields(from, to).join(", ")
        ),
        ChangeKind::ComponentType {
            designator,
            from,
            to,
            ..
        } => tr!(
            "change-component-type",
            designator = designator.as_str(),
            from = crate::panels::library::type_name(*from),
            to = crate::panels::library::type_name(*to)
        ),
        ChangeKind::InBom { designator, to, .. } => {
            if *to {
                tr!("change-in-bom-on", designator = designator.as_str())
            } else {
                tr!("change-in-bom-off", designator = designator.as_str())
            }
        }
        ChangeKind::InCpl { designator, to, .. } => {
            if *to {
                tr!("change-in-cpl-on", designator = designator.as_str())
            } else {
                tr!("change-in-cpl-off", designator = designator.as_str())
            }
        }
        ChangeKind::NotFitted { designator, to, .. } => {
            if *to {
                tr!("change-not-fitted-on", designator = designator.as_str())
            } else {
                tr!("change-not-fitted-off", designator = designator.as_str())
            }
        }
        ChangeKind::AddNet { name } => tr!("change-add-net", net = name.as_str()),
        ChangeKind::RenameNet { from, to, .. } => {
            tr!("change-rename-net", from = from.as_str(), to = to.as_str())
        }
        ChangeKind::RemoveNet {
            name, merge_into, ..
        } => match merge_into.and_then(|m| pcb_nets.iter().find(|(u, _)| *u == m)) {
            Some((_, into)) => tr!(
                "change-merge-net",
                net = name.as_str(),
                into = into.as_str()
            ),
            None => tr!("change-remove-net", net = name.as_str()),
        },
        ChangeKind::PadNet {
            designator,
            number,
            from,
            to,
            ..
        } => tr!(
            "change-pad-net",
            designator = designator.as_str(),
            pad = number.as_str(),
            from = net_name(from.as_ref()),
            to = net_name(to.as_ref())
        ),
    }
}

fn net_name(net: Option<&NetRef>) -> String {
    match net {
        Some(net) => net.name.clone(),
        None => tr!("change-no-net"),
    }
}

/// The names of the fields that differ, built-in ones first.
fn changed_fields(from: &Fields, to: &Fields) -> Vec<String> {
    let mut names: Vec<String> = Fields::BUILT_IN.iter().map(|n| (*n).to_owned()).collect();
    for field in from.custom.iter().chain(&to.custom) {
        if !names.contains(&field.name) {
            names.push(field.name.clone());
        }
    }
    names
        .into_iter()
        .filter(|n| from.get(n) != to.get(n))
        .collect()
}
