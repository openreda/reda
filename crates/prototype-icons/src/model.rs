// PROTOTYPE — throwaway, lives only on the `prototype/main-window-shell` branch.
//
// The layout model of REDA's own docking layer: an arena of nodes (tab groups, splits, canvas
// views), a centre region, edge docks made of columns, and floating native windows. It is
// changed only through `Action`s. `Model::apply` runs an action on a copy, tidies it, checks
// every invariant and the variant's rules, and commits only if all of that passes, so a
// rejected action leaves the layout untouched. No egui widgets in here: only geometry types.

use crate::common::{PanelKind, Tab};
use eframe::egui::Pos2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    /// Documents are ordinary tabs; nothing is protected (what the egui_tiles shell does).
    Tabs,
    /// A protected Document area in the centre holds only Document tabs; Panels live in docks.
    Protected,
    /// The centre is a protected canvas: views showing one Document each, no Document tabs.
    Canvas,
}

impl Variant {
    pub const ALL: [Variant; 3] = [Variant::Tabs, Variant::Protected, Variant::Canvas];
    pub fn label(self) -> &'static str {
        match self {
            Variant::Tabs => "A · Documents as tabs, nothing protected",
            Variant::Protected => "B · Protected Document area + edge docks",
            Variant::Canvas => "C · Protected canvas, one Document per view",
        }
    }
}

pub type NodeId = usize;

#[derive(Clone, Debug)]
pub enum Node {
    Tabs { tabs: Vec<Tab>, active: usize },
    /// `across`: children side by side (a left, b right); otherwise stacked (a top, b bottom).
    Split { across: bool, ratio: f32, a: NodeId, b: NodeId },
    /// Variant C: a protected view showing one Document.
    Canvas { doc: Option<usize> },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Side {
    Left,
    Right,
    Bottom,
}

impl Side {
    pub const ALL: [Side; 3] = [Side::Left, Side::Right, Side::Bottom];
    pub fn i(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug)]
pub struct Column {
    pub root: NodeId,
    /// Width for left/right columns (bottom columns share the dock's height).
    pub size: f32,
    /// Collapsed to an icon strip; its Panels open as flyouts.
    pub collapsed: bool,
}

#[derive(Clone, Debug)]
pub struct Win {
    /// Names the OS window across moves.
    pub key: u64,
    pub root: NodeId,
    pub pos: Option<Pos2>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Region {
    Centre,
    Dock(Side, usize),
    Win(u64),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Centre,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dest {
    /// A zone of a tab group or canvas view: Centre joins its tabs (or shows the Document),
    /// the sides split it.
    Node(NodeId, Zone),
    /// A new column on the outer edge of a dock.
    NewColumn(Side),
    /// The centre region when it is empty (variant A only).
    EmptyCentre,
    /// A new native OS window, at a screen position when the platform reports one.
    NewWindow(Option<Pos2>),
    /// Somewhere in an existing window: its first tab group or canvas view.
    WindowOf(u64),
}

/// Every structural layout change. Each has a command id and a printable form, so it could be
/// a REDA Command (ADR-0001) and be journalled or sent over MCP.
#[derive(Clone, Debug)]
pub enum Action {
    Move { tab: Tab, to: Dest },
    Close(Tab),
    /// Reopen a closed Panel or Document at its home.
    Open(Tab),
    /// Send a tab from a window back to its home in the main window.
    Home(Tab),
    CloseWindow(u64),
    Collapse { side: Side, col: usize, collapsed: bool },
    /// Variant C: show a Document in a canvas view.
    Show { view: NodeId, doc: usize },
    /// Variant C: close a canvas view.
    CloseView(NodeId),
}

impl Action {
    pub fn command_id(&self) -> &'static str {
        match self {
            Action::Move { to: Dest::NewWindow(_), .. } => "view.layout.float",
            Action::Move { .. } => "view.layout.move",
            Action::Close(_) => "view.layout.close",
            Action::Open(_) => "view.layout.open",
            Action::Home(_) => "view.layout.home",
            Action::CloseWindow(_) => "view.window.close",
            Action::Collapse { .. } => "view.dock.collapse",
            Action::Show { .. } => "view.canvas.show",
            Action::CloseView(_) => "view.canvas.close",
        }
    }
}

pub fn home_side(p: PanelKind) -> Side {
    match p {
        PanelKind::Projects => Side::Left,
        PanelKind::Properties => Side::Right,
        PanelKind::Messages => Side::Bottom,
    }
}

#[derive(Clone, Debug)]
pub struct Model {
    pub nodes: Vec<Option<Node>>,
    pub centre: Option<NodeId>,
    pub docks: [Vec<Column>; 3],
    pub bottom_height: f32,
    pub windows: Vec<Win>,
    /// Variant C: the open Documents, in the order they were opened.
    pub open_docs: Vec<usize>,
    pub next_key: u64,
}

impl Model {
    pub fn new(variant: Variant, docs: &[usize]) -> Self {
        let mut m = Model {
            nodes: vec![],
            centre: None,
            docks: [vec![], vec![], vec![]],
            bottom_height: 150.0,
            windows: vec![],
            open_docs: docs.to_vec(),
            next_key: 1,
        };
        let doc_tabs = Node::Tabs { tabs: docs.iter().map(|d| Tab::Doc(*d)).collect(), active: 0 };
        match variant {
            Variant::Tabs => {
                // One tree, the same arrangement as the egui_tiles shell: Projects | (Documents /
                // Messages) | Properties. The docks start empty but any edge can grow one.
                let docs = m.alloc(doc_tabs);
                let msg = m.alloc(Node::Tabs { tabs: vec![Tab::Panel(PanelKind::Messages)], active: 0 });
                let mid = m.alloc(Node::Split { across: false, ratio: 0.78, a: docs, b: msg });
                let pj = m.alloc(Node::Tabs { tabs: vec![Tab::Panel(PanelKind::Projects)], active: 0 });
                let pr = m.alloc(Node::Tabs { tabs: vec![Tab::Panel(PanelKind::Properties)], active: 0 });
                let right = m.alloc(Node::Split { across: true, ratio: 0.78, a: mid, b: pr });
                m.centre = Some(m.alloc(Node::Split { across: true, ratio: 0.17, a: pj, b: right }));
            }
            Variant::Protected | Variant::Canvas => {
                m.centre = Some(if variant == Variant::Protected {
                    m.alloc(doc_tabs)
                } else {
                    m.alloc(Node::Canvas { doc: docs.first().copied() })
                });
                for p in [PanelKind::Projects, PanelKind::Properties, PanelKind::Messages] {
                    let root = m.alloc(Node::Tabs { tabs: vec![Tab::Panel(p)], active: 0 });
                    m.docks[home_side(p).i()].push(Column { root, size: 230.0, collapsed: false });
                }
            }
        }
        m
    }

    pub fn alloc(&mut self, n: Node) -> NodeId {
        self.nodes.push(Some(n));
        self.nodes.len() - 1
    }

    pub fn node(&self, id: NodeId) -> &Node {
        self.nodes[id].as_ref().expect("live node")
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.nodes[id].as_mut().expect("live node")
    }

    pub fn roots(&self) -> Vec<(Region, NodeId)> {
        let mut r = vec![];
        if let Some(c) = self.centre {
            r.push((Region::Centre, c));
        }
        for s in Side::ALL {
            for (k, c) in self.docks[s.i()].iter().enumerate() {
                r.push((Region::Dock(s, k), c.root));
            }
        }
        for w in &self.windows {
            r.push((Region::Win(w.key), w.root));
        }
        r
    }

    /// Leaves (tab groups and canvas views) under `root`, in reading order.
    pub fn leaves(&self, root: NodeId) -> Vec<NodeId> {
        match self.node(root) {
            Node::Split { a, b, .. } => {
                let mut v = self.leaves(*a);
                v.extend(self.leaves(*b));
                v
            }
            _ => vec![root],
        }
    }

    pub fn region_of(&self, id: NodeId) -> Option<Region> {
        self.roots().into_iter().find(|(_, r)| self.leaves(*r).contains(&id)).map(|(g, _)| g)
    }

    pub fn group_of(&self, tab: Tab) -> Option<NodeId> {
        self.roots().into_iter().flat_map(|(_, r)| self.leaves(r)).find(|&l| matches!(self.node(l), Node::Tabs { tabs, .. } if tabs.contains(&tab)))
    }

    pub fn contains(&self, tab: Tab) -> bool {
        match tab {
            Tab::Doc(d) if self.has_canvas() => self.open_docs.contains(&d),
            _ => self.group_of(tab).is_some(),
        }
    }

    fn has_canvas(&self) -> bool {
        self.nodes.iter().flatten().any(|n| matches!(n, Node::Canvas { .. }))
    }

    pub fn canvases(&self) -> Vec<NodeId> {
        self.roots().into_iter().flat_map(|(_, r)| self.leaves(r)).filter(|&l| matches!(self.node(l), Node::Canvas { .. })).collect()
    }

    fn take_tab(&mut self, tab: Tab) -> bool {
        let Some(g) = self.group_of(tab) else { return false };
        if let Node::Tabs { tabs, active } = self.node_mut(g) {
            let i = tabs.iter().position(|t| *t == tab).unwrap();
            tabs.remove(i);
            if *active >= i && *active > 0 {
                *active -= 1;
            }
        }
        true
    }

    fn split_at(&mut self, target: NodeId, zone: Zone, fresh: Node) -> Result<(), String> {
        let (across, new_first) = match zone {
            Zone::Left => (true, true),
            Zone::Right => (true, false),
            Zone::Top => (false, true),
            Zone::Bottom => (false, false),
            Zone::Centre => unreachable!(),
        };
        let old = self.nodes[target].take().ok_or("stale target")?;
        let moved = self.alloc(old);
        let fresh = self.alloc(fresh);
        let (a, b) = if new_first { (fresh, moved) } else { (moved, fresh) };
        self.nodes[target] = Some(Node::Split { across, ratio: 0.5, a, b });
        Ok(())
    }

    fn first_leaf_of_window(&self, key: u64) -> Option<NodeId> {
        self.windows.iter().find(|w| w.key == key).map(|w| self.leaves(w.root)[0])
    }

    /// Run an action on a copy, tidy, validate, and commit only if everything holds.
    pub fn apply(&mut self, action: &Action, variant: Variant) -> Result<(), String> {
        let mut m = self.clone();
        m.exec(action, variant)?;
        m.tidy(variant);
        m.validate(variant)?;
        *self = m;
        Ok(())
    }

    /// Dry run, for colouring drop targets before release.
    pub fn check(&self, action: &Action, variant: Variant) -> Result<(), String> {
        self.clone().apply(action, variant)
    }

    fn exec(&mut self, action: &Action, variant: Variant) -> Result<(), String> {
        match *action {
            Action::Move { tab, to } => {
                // In variant C a Document is not a tab: moving it shows it somewhere else and
                // leaves the source view as it is.
                let as_view = variant == Variant::Canvas && matches!(tab, Tab::Doc(_));
                let fresh = match tab {
                    Tab::Doc(d) if as_view => Node::Canvas { doc: Some(d) },
                    _ => {
                        if !self.take_tab(tab) {
                            return Err("that tab is not in the layout".into());
                        }
                        Node::Tabs { tabs: vec![tab], active: 0 }
                    }
                };
                self.place(fresh, to)
            }
            Action::Close(tab) => {
                match tab {
                    Tab::Doc(d) if variant == Variant::Canvas => {
                        if !self.open_docs.contains(&d) {
                            return Err("that Document is not open".into());
                        }
                        self.open_docs.retain(|x| *x != d);
                        let next = self.open_docs.first().copied();
                        for v in self.canvases() {
                            if let Node::Canvas { doc } = self.node_mut(v)
                                && *doc == Some(d)
                            {
                                *doc = next;
                            }
                        }
                    }
                    _ => {
                        if !self.take_tab(tab) {
                            return Err("that tab is not open".into());
                        }
                    }
                }
                Ok(())
            }
            Action::Open(tab) => {
                if self.contains(tab) {
                    return Err("already open".into());
                }
                self.open_home(tab, variant)
            }
            Action::Home(tab) => {
                if !(variant == Variant::Canvas && matches!(tab, Tab::Doc(_))) && !self.take_tab(tab) {
                    return Err("that tab is not in the layout".into());
                }
                if let Tab::Doc(d) = tab
                    && variant == Variant::Canvas
                {
                    let v = *self.leaves(self.centre.ok_or("no canvas")?).first().unwrap();
                    *self.node_mut(v) = Node::Canvas { doc: Some(d) };
                    return Ok(());
                }
                self.open_home(tab, variant)
            }
            Action::CloseWindow(key) => {
                let i = self.windows.iter().position(|w| w.key == key).ok_or("no such window")?;
                let w = self.windows.remove(i);
                // Its tabs go home rather than closing, so nothing is lost by closing a window.
                for l in self.leaves(w.root) {
                    if let Node::Tabs { tabs, .. } = self.node(l).clone() {
                        for t in tabs {
                            self.open_home(t, variant)?;
                        }
                    }
                }
                Ok(())
            }
            Action::Collapse { side, col, collapsed } => {
                self.docks[side.i()].get_mut(col).ok_or("no such column")?.collapsed = collapsed;
                Ok(())
            }
            Action::Show { view, doc } => match self.nodes.get_mut(view) {
                Some(Some(Node::Canvas { doc: d })) => {
                    *d = Some(doc);
                    if !self.open_docs.contains(&doc) {
                        self.open_docs.push(doc);
                    }
                    Ok(())
                }
                _ => Err("not a canvas view".into()),
            },
            Action::CloseView(view) => {
                if !matches!(self.nodes.get(view), Some(Some(Node::Canvas { .. }))) {
                    return Err("not a canvas view".into());
                }
                // An empty tab group is removed by `tidy`, taking the view with it.
                self.nodes[view] = Some(Node::Tabs { tabs: vec![], active: 0 });
                Ok(())
            }
        }
    }

    fn place(&mut self, fresh: Node, to: Dest) -> Result<(), String> {
        match to {
            Dest::Node(id, Zone::Centre) => {
                let fresh_tab = if let Node::Tabs { tabs, .. } = &fresh { tabs.first().copied() } else { None };
                match (self.nodes.get_mut(id), fresh) {
                    (Some(Some(Node::Tabs { tabs, active })), Node::Tabs { tabs: t, .. }) => {
                        tabs.extend(t);
                        *active = tabs.len() - 1;
                        Ok(())
                    }
                    (Some(Some(Node::Tabs { .. })), Node::Canvas { .. }) => Err("Documents are not tabs in this variant".into()),
                    (Some(Some(Node::Canvas { doc })), Node::Canvas { doc: d }) => {
                        *doc = d;
                        Ok(())
                    }
                    (Some(Some(Node::Canvas { .. })), _) => {
                        let _ = fresh_tab;
                        Err("a canvas view shows Documents, not Panels".into())
                    }
                    _ => Err("stale drop target".into()),
                }
            }
            Dest::Node(id, zone) => self.split_at(id, zone, fresh),
            Dest::NewColumn(side) => {
                let root = self.alloc(fresh);
                let col = Column { root, size: 230.0, collapsed: false };
                match side {
                    Side::Left => self.docks[0].insert(0, col),
                    _ => self.docks[side.i()].push(col),
                }
                Ok(())
            }
            Dest::EmptyCentre => {
                if self.centre.is_some() {
                    return Err("the centre is not empty".into());
                }
                self.centre = Some(self.alloc(fresh));
                Ok(())
            }
            Dest::NewWindow(pos) => {
                let root = self.alloc(fresh);
                let key = self.next_key;
                self.next_key += 1;
                self.windows.push(Win { key, root, pos });
                Ok(())
            }
            Dest::WindowOf(key) => {
                let leaf = self.first_leaf_of_window(key).ok_or("no such window")?;
                self.place(fresh, Dest::Node(leaf, Zone::Centre))
            }
        }
    }

    fn open_home(&mut self, tab: Tab, variant: Variant) -> Result<(), String> {
        let fresh = Node::Tabs { tabs: vec![tab], active: 0 };
        match tab {
            Tab::Panel(p) => {
                let side = home_side(p);
                // First tab group of the home dock, else a new column there.
                let group = self.docks[side.i()].iter().flat_map(|c| self.leaves(c.root)).find(|&l| matches!(self.node(l), Node::Tabs { .. }));
                match group {
                    Some(g) => self.place(fresh, Dest::Node(g, Zone::Centre)),
                    None => self.place(fresh, Dest::NewColumn(side)),
                }
            }
            Tab::Doc(d) => {
                if variant == Variant::Canvas {
                    if !self.open_docs.contains(&d) {
                        self.open_docs.push(d);
                    }
                    return Ok(());
                }
                // Beside another Document if there is one, else in the centre.
                let with_docs = self.roots().into_iter().filter(|(g, _)| *g == Region::Centre).flat_map(|(_, r)| self.leaves(r)).find(|&l| {
                    matches!(self.node(l), Node::Tabs { tabs, .. } if tabs.iter().any(|t| matches!(t, Tab::Doc(_))))
                });
                let any = self.centre.map(|c| self.leaves(c)[0]);
                match with_docs.or(any) {
                    Some(g) => self.place(fresh, Dest::Node(g, Zone::Centre)),
                    None => self.place(fresh, Dest::EmptyCentre),
                }
            }
        }
    }

    /// Remove empty tab groups, collapse one-child splits, drop empty columns and windows.
    fn tidy(&mut self, variant: Variant) {
        self.centre = self.centre.and_then(|c| self.simplify(c));
        if self.centre.is_none() && variant == Variant::Protected {
            // The Document area is protected: it stays, empty, when its last Document closes.
            self.centre = Some(self.alloc(Node::Tabs { tabs: vec![], active: 0 }));
        }
        for s in 0..3 {
            let cols = std::mem::take(&mut self.docks[s]);
            self.docks[s] = cols.into_iter().filter_map(|mut c| self.simplify(c.root).map(|r| { c.root = r; c })).collect();
        }
        let wins = std::mem::take(&mut self.windows);
        self.windows = wins.into_iter().filter_map(|mut w| self.simplify(w.root).map(|r| { w.root = r; w })).collect();
        for n in self.nodes.iter_mut().flatten() {
            if let Node::Tabs { tabs, active } = n {
                *active = (*active).min(tabs.len().saturating_sub(1));
            }
        }
    }

    fn simplify(&mut self, id: NodeId) -> Option<NodeId> {
        match self.node(id).clone() {
            Node::Tabs { tabs, .. } if tabs.is_empty() => {
                self.nodes[id] = None;
                None
            }
            Node::Split { across, ratio, a, b } => match (self.simplify(a), self.simplify(b)) {
                (Some(a), Some(b)) => {
                    self.nodes[id] = Some(Node::Split { across, ratio, a, b });
                    Some(id)
                }
                (Some(x), None) | (None, Some(x)) => {
                    self.nodes[id] = None;
                    Some(x)
                }
                (None, None) => {
                    self.nodes[id] = None;
                    None
                }
            },
            _ => Some(id),
        }
    }

    fn validate(&self, variant: Variant) -> Result<(), String> {
        let mut seen = vec![];
        for (region, root) in self.roots() {
            for l in self.leaves(root) {
                match self.node(l) {
                    Node::Tabs { tabs, .. } => {
                        for t in tabs {
                            if seen.contains(t) {
                                return Err(format!("{t:?} would appear twice"));
                            }
                            seen.push(*t);
                            match (variant, region, t) {
                                (Variant::Canvas, _, Tab::Doc(_)) => return Err("Documents are not tabs in this variant".into()),
                                (Variant::Protected, Region::Dock(..), Tab::Doc(_)) => {
                                    return Err("Documents can't dock at the edges; they stay in the Document area or a window".into());
                                }
                                (Variant::Protected | Variant::Canvas, Region::Centre, Tab::Panel(_)) => {
                                    return Err("Panels can't enter the protected Document area; dock them at an edge".into());
                                }
                                _ => {}
                            }
                        }
                    }
                    Node::Canvas { .. } => {
                        if matches!(region, Region::Dock(..)) {
                            return Err("Document views can't dock at the edges".into());
                        }
                    }
                    Node::Split { .. } => unreachable!(),
                }
            }
        }
        match variant {
            Variant::Tabs => {}
            Variant::Protected => {
                if self.centre.is_none() {
                    return Err("the Document area is protected".into());
                }
            }
            Variant::Canvas => {
                let Some(c) = self.centre else { return Err("the canvas is protected: it can't close or move".into()) };
                if !self.leaves(c).iter().any(|&l| matches!(self.node(l), Node::Canvas { .. })) {
                    return Err("the canvas is protected: it can't close or move".into());
                }
            }
        }
        Ok(())
    }

    /// A readable dump for the Layout state window.
    pub fn dump(&self, name: &dyn Fn(&Tab) -> String) -> Vec<String> {
        let mut out = vec![];
        for (region, root) in self.roots() {
            let head = match region {
                Region::Centre => "centre".to_string(),
                Region::Dock(s, k) => {
                    let c = &self.docks[s.i()][k];
                    format!("dock {s:?} column {k}{}", if c.collapsed { " (icons)" } else { "" })
                }
                Region::Win(key) => format!("OS window #{key}"),
            };
            out.push(head);
            self.dump_node(root, 1, name, &mut out);
        }
        out
    }

    fn dump_node(&self, id: NodeId, depth: usize, name: &dyn Fn(&Tab) -> String, out: &mut Vec<String>) {
        let pad = "  ".repeat(depth);
        match self.node(id) {
            Node::Tabs { tabs, active } => {
                let t: Vec<String> = tabs.iter().enumerate().map(|(i, t)| if i == *active { format!("[{}]", name(t)) } else { name(t) }).collect();
                out.push(format!("{pad}#{id} tabs: {}", t.join(", ")));
            }
            Node::Split { across, ratio, a, b } => {
                out.push(format!("{pad}#{id} split {} {ratio:.2}", if *across { "↔" } else { "↕" }));
                self.dump_node(*a, depth + 1, name, out);
                self.dump_node(*b, depth + 1, name, out);
            }
            Node::Canvas { doc } => out.push(format!("{pad}#{id} canvas: {}", doc.map(|d| name(&Tab::Doc(d))).unwrap_or("—".into()))),
        }
    }
}
