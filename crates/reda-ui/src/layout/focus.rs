//! Moving keyboard focus between areas: F6 / Shift+F6 visit the tab groups and Docks in
//! layout order (`layout.focus.cycle`), and Ctrl+Tab / Ctrl+Shift+Tab walk the open tabs
//! most recently used first, through a popup (`layout.tab.switch`). The Keymap binds the
//! keys; these view Commands do the work, so scripts and MCP can move focus too.

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Preset};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{GroupId, Layout, TabKey};

/// Which way to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Next,
    Previous,
}

/// Keyboard focus beyond the [`Layout`]'s own `focus`, as view state.
#[derive(Clone, Debug, Default)]
pub struct Focus {
    /// Tabs most recently focused first. Closed tabs may linger; readers skip them.
    pub recent: Vec<TabKey>,
    /// The tab switcher, while it is open.
    pub switching: Option<TabSwitch>,
    /// A tab whose strip button should take the keyboard focus on the next frame, after
    /// a Command moved focus to it from the keyboard.
    pub request: Option<TabKey>,
}

impl Focus {
    /// Notes that `tab` has focus now.
    pub fn used(&mut self, tab: &TabKey) {
        if self.recent.first() == Some(tab) {
            return;
        }
        self.recent.retain(|t| t != tab);
        self.recent.insert(0, tab.clone());
    }
}

/// The open tab switcher: the tabs it offers, most recently used first, and the one
/// picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabSwitch {
    pub tabs: Vec<TabKey>,
    pub index: usize,
}

impl TabSwitch {
    /// The tab picked.
    pub fn picked(&self) -> Option<&TabKey> {
        self.tabs.get(self.index)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DirectionParams {
    /// `next` or `previous`.
    direction: Direction,
}

fn presets(next: &str, previous: &str, icon: &str) -> Vec<Preset> {
    vec![
        Preset::new("next", next, icon, json!({"direction": "next"})),
        Preset::new("previous", previous, icon, json!({"direction": "previous"})),
    ]
}

/// Where focus went.
#[derive(Serialize, JsonSchema)]
pub struct Focused {
    /// The tab group that holds the focused tab.
    pub group: GroupId,
}

pub(super) struct CycleFocus;

impl Command for CycleFocus {
    const META: Meta = Meta {
        id: "layout.focus.cycle",
        title: "Cycle Focus",
        description: "Moves focus to the next or previous tab group or Dock, in layout order (left Dock, centre, right Dock, bottom Dock, then floating windows), wrapping around.",
        category: "layout",
        icon: "layout",
        kind: Kind::View,
    };
    type Params = DirectionParams;
    type Output = Focused;

    fn presets(&self) -> Vec<Preset> {
        presets("Focus Next Group", "Focus Previous Group", "layout")
    }

    fn run(&self, cx: &mut Context<'_>, p: DirectionParams) -> Result<Focused, CommandError> {
        let layout = cx.view::<Layout>()?;
        let group = layout.cycle_focus(p.direction == Direction::Next)?;
        let tab = layout.focus.clone();
        cx.view::<Focus>()?.request = tab;
        Ok(Focused { group })
    }
}

/// The tab the switcher picked.
#[derive(Serialize, JsonSchema)]
pub struct Picked {
    #[doc = "The tab: \"start\", a Panel id such as \"properties\", or a Document UUID."]
    pub tab: Option<String>,
}

pub(super) struct SwitchTab;

impl Command for SwitchTab {
    const META: Meta = Meta {
        id: "layout.tab.switch",
        title: "Switch Tab",
        description: "Opens the tab switcher, which lists the open tabs most recently used first, or moves its pick to the next or previous tab. The picked tab is shown and focused when the switcher closes.",
        category: "layout",
        icon: "tab",
        kind: Kind::View,
    };
    type Params = DirectionParams;
    type Output = Picked;

    fn presets(&self) -> Vec<Preset> {
        presets("Switch to Next Tab", "Switch to Previous Tab", "tab")
    }

    fn run(&self, cx: &mut Context<'_>, p: DirectionParams) -> Result<Picked, CommandError> {
        let layout = cx.view::<Layout>()?;
        let (open, current) = (layout.tabs_in_order(), layout.focus.clone());
        let focus = cx.view::<Focus>()?;
        let switch = focus.switching.get_or_insert_with(|| {
            // The tab in use, then the others most recently used first, then the rest in
            // layout order.
            let mut tabs: Vec<TabKey> = Vec::new();
            for tab in current.iter().chain(&focus.recent).chain(&open) {
                if open.contains(tab) && !tabs.contains(tab) {
                    tabs.push(tab.clone());
                }
            }
            // Opening starts on the tab in use, so the first step leaves it.
            TabSwitch { tabs, index: 0 }
        });
        let n = switch.tabs.len().max(1);
        switch.index = match p.direction {
            Direction::Next => (switch.index + 1) % n,
            Direction::Previous => (switch.index + n - 1) % n,
        };
        Ok(Picked {
            tab: switch.picked().map(ToString::to_string),
        })
    }
}
