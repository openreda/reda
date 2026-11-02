//! Saved Layouts: the user's own layouts, kept as `layouts/<name>.json` in the config
//! folder.
//!
//! A Saved Layout holds Panels by id and Document *slots*: whether each tab group held
//! Documents, never which ones, so it fits any Project. Applying one moves the open
//! Documents into its first Document slot and puts each Panel in its saved place.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use reda_engine::command::{CommandError, UserDirs};
use serde::{Deserialize, Serialize};

use super::{Dock, FloatingWindow, Group, Layout, Node, PanelId, TabKey};

/// The format number written into each Saved Layout file.
const FORMAT: u32 = 1;

/// What a Saved Layout keeps of a tab: the Panel, or a slot for Documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Panel(PanelId),
    /// The group held Documents (or the Start page): the open Documents go here.
    Documents,
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Slot::Panel(panel) => f.write_str(panel.id()),
            Slot::Documents => f.write_str("documents"),
        }
    }
}

impl FromStr for Slot {
    type Err = String;

    fn from_str(s: &str) -> Result<Slot, String> {
        if s == "documents" {
            return Ok(Slot::Documents);
        }
        PanelId::ALL
            .into_iter()
            .find(|p| p.id() == s)
            .map(Slot::Panel)
            .ok_or_else(|| format!("{s:?} is not a Panel id or \"documents\""))
    }
}

impl Serialize for Slot {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Slot {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Slot, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// A layout with Document slots in place of Documents: the contents of a Saved Layout file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedLayout {
    format: u32,
    pub centre: Node<Slot>,
    pub left: Dock<Slot>,
    pub right: Dock<Slot>,
    pub bottom: Dock<Slot>,
    #[serde(default)]
    pub windows: Vec<FloatingWindow<Slot>>,
}

impl SavedLayout {
    /// What `layout` keeps when saved: its Panels, and a slot in each group that held
    /// Documents or the Start page.
    pub fn of(layout: &Layout) -> SavedLayout {
        let dock = |d: &Dock| Dock {
            groups: d.groups.iter().map(slots_of).collect(),
            collapsed: d.collapsed,
        };
        SavedLayout {
            format: FORMAT,
            centre: map_node(&layout.centre, &mut slots_of),
            left: dock(&layout.left),
            right: dock(&layout.right),
            bottom: dock(&layout.bottom),
            windows: layout
                .windows
                .iter()
                .map(|w| FloatingWindow {
                    id: w.id,
                    root: map_node(&w.root, &mut slots_of),
                    rect: w.rect,
                })
                .collect(),
        }
    }

    /// The layout this Saved Layout gives when applied to `current`: each Panel in its
    /// saved place (Panels it does not hold close), and the Documents open in `current`
    /// in its first Document slot, or else in the first centre group.
    pub fn apply_to(&self, current: &Layout) -> Result<Layout, CommandError> {
        let documents: Vec<TabKey> = current
            .groups()
            .into_iter()
            .flat_map(|(_, g)| g.tabs.iter())
            .filter(|t| !matches!(t, TabKey::Panel(_)))
            .cloned()
            .collect();
        let active = current.active_document().map(TabKey::Document);
        let mut placed = false;
        let mut fill = |g: &Group<Slot>| {
            let mut out = Group::new(g.id, Vec::new());
            for (i, slot) in g.tabs.iter().enumerate() {
                let start = out.tabs.len();
                match slot {
                    Slot::Panel(panel) => out.tabs.push(TabKey::Panel(*panel)),
                    Slot::Documents if !placed => {
                        out.tabs.extend(documents.iter().cloned());
                        placed = true;
                    }
                    Slot::Documents => {}
                }
                if i == g.active {
                    let shown = active
                        .as_ref()
                        .and_then(|a| out.tabs[start..].iter().position(|t| t == a))
                        .unwrap_or(0);
                    out.active = start + shown;
                }
            }
            out
        };
        let mut centre = map_node(&self.centre, &mut fill);
        let mut dock = |d: &Dock<Slot>| Dock {
            groups: d.groups.iter().map(&mut fill).collect(),
            collapsed: d.collapsed,
        };
        let docks = [dock(&self.left), dock(&self.right), dock(&self.bottom)];
        let windows = self
            .windows
            .iter()
            .map(|w| FloatingWindow {
                id: w.id,
                root: map_node(&w.root, &mut fill),
                rect: w.rect,
            })
            .collect();
        if !placed {
            let mut groups = Vec::new();
            centre.groups_mut(&mut groups);
            if let Some(first) = groups.into_iter().next() {
                first.tabs.extend(documents.iter().cloned());
            }
        }
        let mut layout = Layout::from_parts(
            centre,
            docks,
            windows,
            current.focus.clone(),
            current.active_document(),
        )
        .map_err(|e| CommandError::rejected(format!("the Saved Layout cannot apply: {e}")))?;
        layout.flyout = None;
        Ok(layout)
    }
}

/// A group's slots: its Panels, with one Document slot where its first Document was.
fn slots_of(g: &Group) -> Group<Slot> {
    let mut out = Group::new(g.id, Vec::new());
    for (i, tab) in g.tabs.iter().enumerate() {
        let slot = match tab {
            TabKey::Panel(panel) => Some(Slot::Panel(*panel)),
            TabKey::Start | TabKey::Document(_) => {
                (!out.tabs.contains(&Slot::Documents)).then_some(Slot::Documents)
            }
        };
        if let Some(slot) = slot {
            out.tabs.push(slot);
        }
        if i == g.active {
            let slot = match tab {
                TabKey::Panel(panel) => Slot::Panel(*panel),
                _ => Slot::Documents,
            };
            out.active = out.tabs.iter().position(|s| *s == slot).unwrap_or(0);
        }
    }
    out
}

fn map_node<A, B>(node: &Node<A>, f: &mut impl FnMut(&Group<A>) -> Group<B>) -> Node<B> {
    match node {
        Node::Group(g) => Node::Group(f(g)),
        Node::Split { axis, children } => Node::Split {
            axis: *axis,
            children: children.iter().map(|c| map_node(c, f)).collect(),
        },
    }
}

/// The folder of Saved Layouts.
pub fn dir(dirs: &UserDirs) -> PathBuf {
    dirs.config.join("layouts")
}

/// Refuses a name that cannot be a file name and a Preset: Saved Layout names are
/// lowercase letters, digits and underscores, such as `wide_screen`.
pub fn check_name(name: &str) -> Result<(), CommandError> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(CommandError::invalid_params(
            "/name",
            format!(
                "{name:?} is not a Saved Layout name: use lowercase letters, digits and underscores"
            ),
        ))
    }
}

/// The names of the Saved Layouts, sorted.
pub fn names(dirs: &UserDirs) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir(dirs)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension()? != "json" {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_owned();
            check_name(&name).ok().map(|()| name)
        })
        .collect();
    names.sort();
    names
}

/// Saves `layout` as the Saved Layout `name`, replacing any of that name.
pub fn save(dirs: &UserDirs, name: &str, layout: &Layout) -> Result<(), CommandError> {
    check_name(name)?;
    let mut bytes = serde_json::to_vec_pretty(&SavedLayout::of(layout)).expect("layouts serialise");
    bytes.push(b'\n');
    let folder = dir(dirs);
    let write = std::fs::create_dir_all(&folder)
        .and_then(|()| reda_engine::fs::write_atomic(&folder.join(format!("{name}.json")), &bytes));
    write.map_err(|e| CommandError::internal(format!("saving the Saved Layout {name}: {e}")))
}

/// Reads the Saved Layout `name`.
pub fn load(dirs: &UserDirs, name: &str) -> Result<SavedLayout, CommandError> {
    check_name(name)?;
    let path = dir(dirs).join(format!("{name}.json"));
    let bytes = std::fs::read(&path)
        .map_err(|_| CommandError::not_found(format!("there is no Saved Layout {name}")))?;
    let saved: SavedLayout = serde_json::from_slice(&bytes).map_err(|e| {
        CommandError::rejected(format!("the Saved Layout {name} cannot be read: {e}"))
    })?;
    if saved.format != FORMAT {
        return Err(CommandError::rejected(format!(
            "the Saved Layout {name} has format {}, and this REDA reads format {FORMAT}",
            saved.format
        )));
    }
    Ok(saved)
}
