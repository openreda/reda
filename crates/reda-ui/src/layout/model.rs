//! The layout model: what is where, and the actions that change it.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use reda_engine::command::CommandError;
use reda_engine::model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A Panel: a shared, dockable view of the Project or the selection.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PanelId {
    Projects,
    Properties,
    Messages,
    Library,
    Layers,
    Journal,
}

impl PanelId {
    /// Every Panel, in the order the Panels button lists them.
    pub const ALL: [PanelId; 6] = [
        PanelId::Projects,
        PanelId::Properties,
        PanelId::Messages,
        PanelId::Library,
        PanelId::Layers,
        PanelId::Journal,
    ];

    /// The id in Commands and Presets, such as `properties` in `panel.show:properties`.
    pub fn id(self) -> &'static str {
        match self {
            PanelId::Projects => "projects",
            PanelId::Properties => "properties",
            PanelId::Messages => "messages",
            PanelId::Library => "library",
            PanelId::Layers => "layers",
            PanelId::Journal => "journal",
        }
    }

    /// The Dock a Panel reopens in once closed.
    pub fn home(self) -> DockSide {
        match self {
            PanelId::Projects | PanelId::Library => DockSide::Left,
            PanelId::Properties | PanelId::Layers => DockSide::Right,
            PanelId::Messages | PanelId::Journal => DockSide::Bottom,
        }
    }
}

/// What a tab shows. Written in Commands as a string: `start`, a Panel id such as
/// `properties`, or a Document UUID.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TabKey {
    /// The Start page, shown while no Project is open.
    Start,
    Document(Uuid),
    Panel(PanelId),
}

impl fmt::Display for TabKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TabKey::Start => f.write_str("start"),
            TabKey::Document(uuid) => write!(f, "{uuid}"),
            TabKey::Panel(panel) => f.write_str(panel.id()),
        }
    }
}

impl Serialize for TabKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for TabKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<TabKey, D::Error> {
        let raw = String::deserialize(d)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

impl FromStr for TabKey {
    type Err = String;

    fn from_str(s: &str) -> Result<TabKey, String> {
        if s == "start" {
            return Ok(TabKey::Start);
        }
        if let Some(panel) = PanelId::ALL.into_iter().find(|p| p.id() == s) {
            return Ok(TabKey::Panel(panel));
        }
        s.parse::<Uuid>().map(TabKey::Document).map_err(|_| {
            format!("{s:?} is not a tab: give \"start\", a Panel id or a Document UUID")
        })
    }
}

/// A window edge that holds a Dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DockSide {
    Left,
    Right,
    Bottom,
}

impl DockSide {
    pub const ALL: [DockSide; 3] = [DockSide::Left, DockSide::Right, DockSide::Bottom];
}

impl fmt::Display for DockSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DockSide::Left => "left",
            DockSide::Right => "right",
            DockSide::Bottom => "bottom",
        })
    }
}

/// Where a split puts the new group, beside the group it splits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SplitSide {
    Right,
    Down,
    Left,
    Up,
}

impl SplitSide {
    pub const ALL: [SplitSide; 4] = [
        SplitSide::Left,
        SplitSide::Right,
        SplitSide::Up,
        SplitSide::Down,
    ];

    /// The axis of the split it makes.
    pub fn axis(self) -> Axis {
        match self {
            SplitSide::Left | SplitSide::Right => Axis::Horizontal,
            SplitSide::Up | SplitSide::Down => Axis::Vertical,
        }
    }

    /// Whether the new group goes before the split group: to its left or above it.
    fn before(self) -> bool {
        matches!(self, SplitSide::Left | SplitSide::Up)
    }
}

impl fmt::Display for SplitSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SplitSide::Right => "right",
            SplitSide::Down => "down",
            SplitSide::Left => "left",
            SplitSide::Up => "up",
        })
    }
}

/// How a split lays out its children: side by side, or stacked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// A tab group's permanent id, the target of layout Commands.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct GroupId(pub u64);

/// A floating window's permanent id.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct WindowId(pub u64);

/// Tabs shown one at a time, with a tab strip. A Saved Layout holds groups of
/// [`Slot`](super::Slot)s instead of tabs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group<T = TabKey> {
    pub id: GroupId,
    pub tabs: Vec<T>,
    /// The index of the tab shown.
    #[serde(default)]
    pub active: usize,
}

impl<T> Group<T> {
    pub(super) fn new(id: GroupId, tabs: Vec<T>) -> Group<T> {
        Group {
            id,
            tabs,
            active: 0,
        }
    }

    /// The tab shown, if the group has any.
    pub fn active_tab(&self) -> Option<&T> {
        self.tabs.get(self.active)
    }
}

/// A tree of groups: the centre and each floating window hold one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Node<T = TabKey> {
    Group(Group<T>),
    /// At least two children, none a split along the same axis.
    Split {
        axis: Axis,
        children: Vec<Node<T>>,
    },
}

impl<T> Node<T> {
    /// Every group in the tree, in order.
    pub(super) fn groups<'a>(&'a self, out: &mut Vec<&'a Group<T>>) {
        match self {
            Node::Group(g) => out.push(g),
            Node::Split { children, .. } => children.iter().for_each(|c| c.groups(out)),
        }
    }

    pub(super) fn groups_mut<'a>(&'a mut self, out: &mut Vec<&'a mut Group<T>>) {
        match self {
            Node::Group(g) => out.push(g),
            Node::Split { children, .. } => children.iter_mut().for_each(|c| c.groups_mut(out)),
        }
    }

    /// Puts `new` beside the group `id`, on `side`. Returns whether it found the group.
    fn split(&mut self, id: GroupId, side: SplitSide, new: &mut Option<Group<T>>) -> bool {
        let axis = side.axis();
        match self {
            Node::Group(g) if g.id == id => {
                let old = std::mem::replace(self, Node::Group(Group::new(id, Vec::new())));
                let new = Node::Group(new.take().expect("split once"));
                let children = if side.before() {
                    vec![new, old]
                } else {
                    vec![old, new]
                };
                *self = Node::Split { axis, children };
                true
            }
            Node::Group(_) => false,
            Node::Split {
                axis: own,
                children,
            } => {
                let own = *own;
                for i in 0..children.len() {
                    // Beside a direct child along the split's own axis: a new sibling.
                    if own == axis && matches!(&children[i], Node::Group(g) if g.id == id) {
                        let new = new.take().expect("split once");
                        let at = if side.before() { i } else { i + 1 };
                        children.insert(at, Node::Group(new));
                        return true;
                    }
                    if children[i].split(id, side, new) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Drops empty groups, unwraps one-child splits and merges splits along the same axis.
    pub(super) fn tidy(self) -> Option<Node<T>> {
        match self {
            Node::Group(g) => (!g.tabs.is_empty()).then_some(Node::Group(g)),
            Node::Split { axis, children } => {
                let mut kept = Vec::new();
                for child in children.into_iter().filter_map(Node::tidy) {
                    match child {
                        Node::Split {
                            axis: inner,
                            children,
                        } if inner == axis => kept.extend(children),
                        other => kept.push(other),
                    }
                }
                match kept.len() {
                    0 => None,
                    1 => kept.pop(),
                    _ => Some(Node::Split {
                        axis,
                        children: kept,
                    }),
                }
            }
        }
    }

    pub(super) fn check(&self) -> Result<(), String> {
        match self {
            Node::Group(_) => Ok(()),
            Node::Split { axis, children } => {
                if children.len() < 2 {
                    return Err("a split needs at least two children".into());
                }
                for child in children {
                    if matches!(child, Node::Split { axis: a, .. } if a == axis) {
                        return Err("a split holds a split along its own axis".into());
                    }
                    child.check()?;
                }
                Ok(())
            }
        }
    }
}

/// The groups stacked in one column along a window edge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dock<T = TabKey> {
    pub groups: Vec<Group<T>>,
    /// Shown as an icon strip whose icons open their tabs as flyouts.
    #[serde(default)]
    pub collapsed: bool,
}

impl<T> Default for Dock<T> {
    fn default() -> Dock<T> {
        Dock {
            groups: Vec::new(),
            collapsed: false,
        }
    }
}

/// A window of its own, holding a tree of groups.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloatingWindow<T = TabKey> {
    pub id: WindowId,
    pub root: Node<T>,
}

/// Which part of the layout holds a group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    Centre,
    Dock(DockSide),
    Window(WindowId),
}

/// Where a tab is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub area: Area,
    pub group: GroupId,
    pub index: usize,
}

/// The arrangement of tabs, splits, Docks and windows.
///
/// Its JSON form (in `.reda/view.json`) is checked as a whole when read: a layout that
/// breaks the rules does not deserialise.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawLayout")]
pub struct Layout {
    /// The main window's middle. It is never empty of groups, but its one group may hold
    /// no tabs: an empty centre is allowed.
    pub centre: Node,
    pub left: Dock,
    pub right: Dock,
    pub bottom: Dock,
    pub windows: Vec<FloatingWindow>,
    /// The tab last activated, if it is still open.
    pub focus: Option<TabKey>,
    /// The tab of a collapsed Dock shown as a flyout.
    #[serde(skip)]
    pub flyout: Option<TabKey>,
    /// The Document last activated: menus and toolbars follow it.
    last_document: Option<Uuid>,
    next_id: u64,
}

/// [`Layout`] as read, before it is checked.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLayout {
    centre: Node,
    left: Dock,
    right: Dock,
    bottom: Dock,
    #[serde(default)]
    windows: Vec<FloatingWindow>,
    #[serde(default)]
    focus: Option<TabKey>,
    #[serde(default)]
    last_document: Option<Uuid>,
    #[serde(default)]
    #[allow(dead_code, reason = "worked out again from the ids")]
    next_id: u64,
}

impl TryFrom<RawLayout> for Layout {
    type Error = String;

    fn try_from(raw: RawLayout) -> Result<Layout, String> {
        Layout::from_parts(
            raw.centre,
            [raw.left, raw.right, raw.bottom],
            raw.windows,
            raw.focus,
            raw.last_document,
        )
    }
}

impl Default for Layout {
    /// The Default Layout around the Start page.
    fn default() -> Layout {
        Layout::with_centre(vec![TabKey::Start])
    }
}

impl Layout {
    /// The Default Layout: `centre` in the middle, Projects in the left Dock, Properties
    /// in the right Dock and Messages in the bottom Dock.
    pub fn with_centre(centre: Vec<TabKey>) -> Layout {
        let dock = |id: u64, panel: PanelId| Dock {
            groups: vec![Group::new(GroupId(id), vec![TabKey::Panel(panel)])],
            collapsed: false,
        };
        let mut layout = Layout {
            centre: Node::Group(Group::new(GroupId(1), centre)),
            left: dock(2, PanelId::Projects),
            right: dock(3, PanelId::Properties),
            bottom: dock(4, PanelId::Messages),
            windows: Vec::new(),
            focus: None,
            flyout: None,
            last_document: None,
            next_id: 5,
        };
        if let Some(first) = layout
            .group(GroupId(1))
            .and_then(|g| g.tabs.first())
            .cloned()
        {
            layout.activate_in_place(&first);
        }
        layout
    }

    /// A layout from its parts, tidied and checked. Group and window ids are kept, and
    /// new ones follow the largest.
    pub(super) fn from_parts(
        centre: Node,
        [left, right, bottom]: [Dock; 3],
        windows: Vec<FloatingWindow>,
        focus: Option<TabKey>,
        last_document: Option<Uuid>,
    ) -> Result<Layout, String> {
        let mut layout = Layout {
            centre,
            left,
            right,
            bottom,
            windows,
            focus,
            flyout: None,
            last_document,
            next_id: 0,
        };
        // Tidying needs a centre with a group in it.
        let mut centre = Vec::new();
        layout.centre.groups(&mut centre);
        if centre.is_empty() {
            return Err("the centre holds no tab group".into());
        }
        layout.tidy();
        layout.check()?;
        let groups = layout.groups().into_iter().map(|(_, g)| g.id.0);
        let windows = layout.windows.iter().map(|w| w.id.0);
        layout.next_id = groups.chain(windows).max().unwrap_or(0) + 1;
        Ok(layout)
    }

    pub fn dock(&self, side: DockSide) -> &Dock {
        match side {
            DockSide::Left => &self.left,
            DockSide::Right => &self.right,
            DockSide::Bottom => &self.bottom,
        }
    }

    fn dock_mut(&mut self, side: DockSide) -> &mut Dock {
        match side {
            DockSide::Left => &mut self.left,
            DockSide::Right => &mut self.right,
            DockSide::Bottom => &mut self.bottom,
        }
    }

    /// Every group with the area that holds it: the centre, the Docks, then the windows.
    pub fn groups(&self) -> Vec<(Area, &Group)> {
        let mut out = Vec::new();
        let mut centre = Vec::new();
        self.centre.groups(&mut centre);
        out.extend(centre.into_iter().map(|g| (Area::Centre, g)));
        for side in DockSide::ALL {
            out.extend(self.dock(side).groups.iter().map(|g| (Area::Dock(side), g)));
        }
        for w in &self.windows {
            let mut groups = Vec::new();
            w.root.groups(&mut groups);
            out.extend(groups.into_iter().map(|g| (Area::Window(w.id), g)));
        }
        out
    }

    fn groups_mut(&mut self) -> Vec<&mut Group> {
        let mut out = Vec::new();
        self.centre.groups_mut(&mut out);
        out.extend(self.left.groups.iter_mut());
        out.extend(self.right.groups.iter_mut());
        out.extend(self.bottom.groups.iter_mut());
        for w in &mut self.windows {
            w.root.groups_mut(&mut out);
        }
        out
    }

    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups()
            .into_iter()
            .find(|(_, g)| g.id == id)
            .map(|(_, g)| g)
    }

    fn group_mut(&mut self, id: GroupId) -> Option<&mut Group> {
        self.groups_mut().into_iter().find(|g| g.id == id)
    }

    /// Where `tab` is, or `None` when it is not open.
    pub fn place(&self, tab: &TabKey) -> Option<Place> {
        self.groups().into_iter().find_map(|(area, g)| {
            g.tabs.iter().position(|t| t == tab).map(|index| Place {
                area,
                group: g.id,
                index,
            })
        })
    }

    /// The Panels not open anywhere, in [`PanelId::ALL`] order: what the Panels button
    /// offers to reopen.
    pub fn closed_panels(&self) -> Vec<PanelId> {
        PanelId::ALL
            .into_iter()
            .filter(|p| self.place(&TabKey::Panel(*p)).is_none())
            .collect()
    }

    /// The active Document: the one last activated, or else the first open one.
    pub fn active_document(&self) -> Option<Uuid> {
        if let Some(uuid) = self.last_document
            && self.place(&TabKey::Document(uuid)).is_some()
        {
            return Some(uuid);
        }
        self.groups()
            .into_iter()
            .flat_map(|(_, g)| g.tabs.iter())
            .find_map(|t| match t {
                TabKey::Document(uuid) => Some(*uuid),
                _ => None,
            })
    }

    /// The group new Documents open in: the active Document's group, else the first
    /// centre group.
    fn document_group(&self) -> GroupId {
        if let Some(uuid) = self.active_document()
            && let Some(place) = self.place(&TabKey::Document(uuid))
        {
            return place.group;
        }
        let mut centre = Vec::new();
        self.centre.groups(&mut centre);
        centre[0].id
    }

    /// Opens `document` in the Document group (or shows it where it is) and activates it;
    /// returns its group. The `document.open` Command does this, and so does the app when
    /// a Project opens.
    pub fn open_document(&mut self, document: Uuid) -> GroupId {
        let tab = TabKey::Document(document);
        if self.place(&tab).is_none() {
            let group = self.document_group();
            if let Some(g) = self.group_mut(group) {
                g.tabs.push(tab.clone());
            }
        }
        self.activate_in_place(&tab)
    }

    /// Closes every tab `keep` refuses, such as Documents that no longer exist.
    pub fn retain_tabs(&mut self, mut keep: impl FnMut(&TabKey) -> bool) {
        for g in self.groups_mut() {
            g.tabs.retain(&mut keep);
        }
        self.tidy();
    }

    // ---- Actions. Each runs on a copy that must pass `check` before it is kept. ----

    /// Runs `action` on a copy of the layout, tidies and checks it, and keeps it only if
    /// both the action and the check pass.
    fn apply<R>(
        &mut self,
        action: impl FnOnce(&mut Layout) -> Result<R, CommandError>,
    ) -> Result<R, CommandError> {
        let mut next = self.clone();
        let out = action(&mut next)?;
        next.tidy();
        next.check().map_err(CommandError::rejected)?;
        *self = next;
        Ok(out)
    }

    /// Shows `tab` in its group and focuses it; a tab of a collapsed Dock opens as a
    /// flyout.
    pub fn activate(&mut self, tab: &TabKey) -> Result<GroupId, CommandError> {
        self.apply(|l| {
            l.find(tab)?;
            Ok(l.activate_in_place(tab))
        })
    }

    /// Moves `tab` into the group `group`, at `index` (default: last).
    pub fn move_to_group(
        &mut self,
        tab: &TabKey,
        group: GroupId,
        index: Option<usize>,
    ) -> Result<GroupId, CommandError> {
        self.apply(|l| {
            l.find(tab)?;
            if l.group(group).is_none() {
                return Err(CommandError::not_found(format!("no tab group {}", group.0)));
            }
            l.take(tab)?;
            let g = l.group_mut(group).expect("checked above");
            let at = index.unwrap_or(g.tabs.len()).min(g.tabs.len());
            g.tabs.insert(at, tab.clone());
            Ok(l.activate_in_place(tab))
        })
    }

    /// Moves `tab` to the end of the Dock at `side`.
    pub fn move_to_dock(&mut self, tab: &TabKey, side: DockSide) -> Result<GroupId, CommandError> {
        self.apply(|l| {
            l.find(tab)?;
            l.take(tab)?;
            l.put_in_dock(tab.clone(), side);
            Ok(l.activate_in_place(tab))
        })
    }

    /// Moves `tab` into a new window of its own.
    pub fn move_to_new_window(&mut self, tab: &TabKey) -> Result<GroupId, CommandError> {
        self.apply(|l| {
            l.find(tab)?;
            l.take(tab)?;
            let window = WindowId(l.fresh_id());
            let group = GroupId(l.fresh_id());
            l.windows.push(FloatingWindow {
                id: window,
                root: Node::Group(Group::new(group, vec![tab.clone()])),
            });
            Ok(l.activate_in_place(tab))
        })
    }

    /// Moves `tab` out of its group into a new group to the right of it or below it.
    pub fn split(&mut self, tab: &TabKey, side: SplitSide) -> Result<GroupId, CommandError> {
        self.split_beside(tab, None, side)
    }

    /// Moves `tab` into a new group on `side` of the group `beside` (by default the tab's
    /// own group). A tab alone in its group has nothing to split from, and a Dock holds
    /// one column of groups, so a group in a Dock splits only up or down.
    pub fn split_beside(
        &mut self,
        tab: &TabKey,
        beside: Option<GroupId>,
        side: SplitSide,
    ) -> Result<GroupId, CommandError> {
        self.apply(|l| {
            let place = l.find(tab)?;
            let target = beside.unwrap_or(place.group);
            let area = l
                .groups()
                .into_iter()
                .find(|(_, g)| g.id == target)
                .map(|(area, _)| area)
                .ok_or_else(|| CommandError::not_found(format!("no tab group {}", target.0)))?;
            let alone = l.group(place.group).is_some_and(|g| g.tabs.len() == 1);
            if alone && target == place.group {
                return Err(CommandError::rejected(format!(
                    "{tab} is alone in its group, so there is nothing to split it from"
                )));
            }
            if matches!(area, Area::Dock(_)) && side.axis() == Axis::Horizontal {
                return Err(CommandError::rejected(
                    "a Dock holds one column of groups, so a group in it splits only up or down",
                ));
            }
            l.take(tab)?;
            let id = GroupId(l.fresh_id());
            let mut new = Some(Group::new(id, vec![tab.clone()]));
            match area {
                Area::Dock(dock) => {
                    let groups = &mut l.dock_mut(dock).groups;
                    let at = groups.iter().position(|g| g.id == target).unwrap_or(0);
                    let at = if side.before() { at } else { at + 1 };
                    groups.insert(at, new.take().expect("one new group"));
                }
                Area::Window(w) => {
                    let window = l.windows.iter_mut().find(|win| win.id == w);
                    window.expect("placed").root.split(target, side, &mut new);
                }
                Area::Centre => {
                    l.centre.split(target, side, &mut new);
                }
            }
            Ok(l.activate_in_place(tab))
        })
    }

    /// Closes `tab`.
    pub fn close(&mut self, tab: &TabKey) -> Result<(), CommandError> {
        self.apply(|l| {
            l.find(tab)?;
            l.take(tab)
        })
    }

    /// Collapses the Dock at `side` to an icon strip, or expands it. Collapsing a
    /// collapsed Dock dismisses its flyout.
    pub fn collapse(&mut self, side: DockSide, collapsed: bool) -> Result<(), CommandError> {
        self.apply(|l| {
            if l.dock(side).groups.is_empty() {
                return Err(CommandError::rejected(format!(
                    "the {side} Dock is empty, so there is nothing to collapse"
                )));
            }
            l.dock_mut(side).collapsed = collapsed;
            l.flyout = None;
            Ok(())
        })
    }

    /// Shows and focuses `panel`, reopening it in its home Dock when it is closed.
    pub fn show_panel(&mut self, panel: PanelId) -> Result<GroupId, CommandError> {
        let tab = TabKey::Panel(panel);
        self.apply(|l| {
            if l.place(&tab).is_none() {
                l.put_in_dock(tab.clone(), panel.home());
            }
            Ok(l.activate_in_place(&tab))
        })
    }

    // ---- Helpers for the actions. ----

    fn find(&self, tab: &TabKey) -> Result<Place, CommandError> {
        self.place(tab)
            .ok_or_else(|| CommandError::not_found(format!("the tab {tab} is not open")))
    }

    /// Removes `tab` from its group, keeping the group's active tab where it can.
    fn take(&mut self, tab: &TabKey) -> Result<(), CommandError> {
        let place = self.find(tab)?;
        let g = self.group_mut(place.group).expect("placed");
        g.tabs.remove(place.index);
        if g.active > place.index || g.active >= g.tabs.len() {
            g.active = g.active.saturating_sub(1);
        }
        Ok(())
    }

    fn put_in_dock(&mut self, tab: TabKey, side: DockSide) {
        if self.dock(side).groups.is_empty() {
            let id = GroupId(self.fresh_id());
            self.dock_mut(side).groups.push(Group::new(id, Vec::new()));
        }
        let dock = self.dock_mut(side);
        dock.groups.last_mut().expect("added above").tabs.push(tab);
    }

    /// Activates an open tab; returns its group.
    fn activate_in_place(&mut self, tab: &TabKey) -> GroupId {
        let place = self.place(tab).expect("activating an open tab");
        self.group_mut(place.group).expect("placed").active = place.index;
        self.focus = Some(tab.clone());
        if let TabKey::Document(uuid) = tab {
            self.last_document = Some(*uuid);
        }
        self.flyout = match place.area {
            Area::Dock(side) if self.dock(side).collapsed => Some(tab.clone()),
            _ => None,
        };
        place.group
    }

    /// Whether both layouts put the same tabs in the same places, whatever has focus.
    pub(super) fn same_arrangement(&self, other: &Layout) -> bool {
        self.centre == other.centre
            && self.left == other.left
            && self.right == other.right
            && self.bottom == other.bottom
            && self.windows == other.windows
    }

    fn fresh_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Drops empty groups, splits and windows, keeping one group in the centre.
    fn tidy(&mut self) {
        let mut centre = Vec::new();
        self.centre.groups(&mut centre);
        let first_centre = centre[0].id;
        let root = std::mem::replace(
            &mut self.centre,
            Node::Group(Group::new(first_centre, Vec::new())),
        );
        if let Some(root) = root.tidy() {
            self.centre = root;
        }
        for side in DockSide::ALL {
            let dock = self.dock_mut(side);
            dock.groups.retain(|g| !g.tabs.is_empty());
            if dock.groups.is_empty() {
                dock.collapsed = false;
            }
        }
        self.windows = std::mem::take(&mut self.windows)
            .into_iter()
            .filter_map(|w| w.root.tidy().map(|root| FloatingWindow { id: w.id, root }))
            .collect();
        for g in self.groups_mut() {
            g.active = g.active.min(g.tabs.len().saturating_sub(1));
        }
        if let Some(tab) = &self.focus
            && self.place(tab).is_none()
        {
            self.focus = None;
        }
        let flyout_ok = self.flyout.as_ref().is_some_and(|tab| {
            matches!(self.place(tab), Some(Place { area: Area::Dock(side), .. })
                if self.dock(side).collapsed)
        });
        if !flyout_ok {
            self.flyout = None;
        }
    }

    /// The rules every layout keeps, checked after each action as a whole.
    pub fn check(&self) -> Result<(), String> {
        let groups = self.groups();
        let mut tabs = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for (area, g) in &groups {
            if !ids.insert(g.id) {
                return Err(format!("the group id {} is used twice", g.id.0));
            }
            if g.tabs.is_empty() && !(*area == Area::Centre && groups[0].1.id == g.id) {
                return Err("only the centre may hold an empty group".into());
            }
            if !g.tabs.is_empty() && g.active >= g.tabs.len() {
                return Err(format!("group {} shows a tab it does not hold", g.id.0));
            }
            for tab in &g.tabs {
                if !tabs.insert(tab.clone()) {
                    return Err(format!("the tab {tab} is open twice"));
                }
            }
        }
        let windows: BTreeSet<WindowId> = self.windows.iter().map(|w| w.id).collect();
        if windows.len() != self.windows.len() {
            return Err("a window id is used twice".into());
        }
        self.centre.check()?;
        for w in &self.windows {
            w.root.check()?;
        }
        Ok(())
    }
}
