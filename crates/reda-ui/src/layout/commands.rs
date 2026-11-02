//! The layout Commands: view Commands that change the [`Layout`] in the engine's view
//! state. Each is refused headless, and a refused action leaves the layout as it was.

use reda_engine::command::{
    Command, CommandError, Context, Kind, Meta, Preset, Registry, UserDirs,
};
use reda_engine::model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{DockSide, GroupId, Layout, PanelId, SplitSide, TabKey, saved};

/// Adds the layout Commands to `registry`, with no `layout.apply:<name>` Presets.
pub fn register_all(registry: &mut Registry) {
    register_with_layouts(registry, None);
}

/// Adds the layout Commands to `registry`, offering the Saved Layouts in `dirs` as
/// `layout.apply:<name>` Presets. The Shell does this for its engine.
pub fn register_with_layouts(registry: &mut Registry, dirs: Option<UserDirs>) {
    registry.add(Activate);
    registry.add(MoveToGroup);
    registry.add(MoveToDock);
    registry.add(MoveToNewWindow);
    registry.add(Split);
    registry.add(Close);
    registry.add(Collapse);
    registry.add(ShowPanel);
    registry.add(OpenDocument);
    registry.add(SaveLayout);
    registry.add(ApplyLayout { dirs });
}

fn user_dirs<'c>(cx: &'c Context<'_>) -> Result<&'c UserDirs, CommandError> {
    cx.user_dirs()
        .ok_or_else(|| CommandError::unavailable("there is no per-user folder for Saved Layouts"))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenDocumentParams {
    /// The Document's UUID.
    document: Uuid,
}

struct OpenDocument;

impl Command for OpenDocument {
    const META: Meta = Meta {
        id: "document.open",
        title: "Open Document",
        description: "Opens a Document in a tab beside the active Document, or shows its tab if it is open, and focuses it.",
        category: "layout",
        icon: "document_open",
        kind: Kind::View,
    };
    type Params = OpenDocumentParams;
    type Output = TabPlaced;

    fn run(&self, cx: &mut Context<'_>, p: OpenDocumentParams) -> Result<TabPlaced, CommandError> {
        if cx.project()?.document(p.document).is_none() {
            return Err(CommandError::not_found(format!(
                "no Document {}",
                p.document
            )));
        }
        let group = layout(cx)?.open_document(p.document);
        Ok(TabPlaced { group })
    }
}

fn tab(raw: &str) -> Result<TabKey, CommandError> {
    raw.parse()
        .map_err(|e: String| CommandError::invalid_params("/tab", e))
}

fn layout<'c>(cx: &'c mut Context<'_>) -> Result<&'c mut Layout, CommandError> {
    cx.view::<Layout>()
}

/// Where a tab ended up.
#[derive(Serialize, JsonSchema)]
pub struct TabPlaced {
    /// The tab group that now holds the tab.
    pub group: GroupId,
}

/// The result of a layout Command with nothing to report.
#[derive(Serialize, JsonSchema)]
pub struct Done {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabParams {
    #[doc = "The tab: \"start\", a Panel id such as \"properties\", or a Document UUID."]
    tab: String,
}

struct Activate;

impl Command for Activate {
    const META: Meta = Meta {
        id: "layout.tab.activate",
        title: "Show Tab",
        description: "Shows a tab in its group and focuses it. A tab in a collapsed Dock opens as a flyout.",
        category: "layout",
        icon: "tab",
        kind: Kind::View,
    };
    type Params = TabParams;
    type Output = TabPlaced;

    fn run(&self, cx: &mut Context<'_>, p: TabParams) -> Result<TabPlaced, CommandError> {
        let tab = tab(&p.tab)?;
        let group = layout(cx)?.activate(&tab)?;
        Ok(TabPlaced { group })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveToGroupParams {
    #[doc = "The tab: \"start\", a Panel id such as \"properties\", or a Document UUID."]
    tab: String,
    /// The tab group to move it into.
    group: GroupId,
    /// Its position among the group's tabs; the end by default.
    #[serde(default)]
    index: Option<usize>,
}

struct MoveToGroup;

impl Command for MoveToGroup {
    const META: Meta = Meta {
        id: "layout.tab.move_to_group",
        title: "Move Tab to Group",
        description: "Moves a tab into a tab group, anywhere in the layout. Documents and Panels can go into any group.",
        category: "layout",
        icon: "tab_move",
        kind: Kind::View,
    };
    type Params = MoveToGroupParams;
    type Output = TabPlaced;

    fn run(&self, cx: &mut Context<'_>, p: MoveToGroupParams) -> Result<TabPlaced, CommandError> {
        let tab = tab(&p.tab)?;
        let group = layout(cx)?.move_to_group(&tab, p.group, p.index)?;
        Ok(TabPlaced { group })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveToDockParams {
    #[doc = "The tab: \"start\", a Panel id such as \"properties\", or a Document UUID."]
    tab: String,
    /// The Dock to move it into.
    dock: DockSide,
}

struct MoveToDock;

impl Command for MoveToDock {
    const META: Meta = Meta {
        id: "layout.tab.move_to_dock",
        title: "Move Tab to Dock",
        description: "Moves a tab into the left, right or bottom Dock, after the tabs already there.",
        category: "layout",
        icon: "dock",
        kind: Kind::View,
    };
    type Params = MoveToDockParams;
    type Output = TabPlaced;

    fn run(&self, cx: &mut Context<'_>, p: MoveToDockParams) -> Result<TabPlaced, CommandError> {
        let tab = tab(&p.tab)?;
        let group = layout(cx)?.move_to_dock(&tab, p.dock)?;
        Ok(TabPlaced { group })
    }
}

struct MoveToNewWindow;

impl Command for MoveToNewWindow {
    const META: Meta = Meta {
        id: "layout.tab.move_to_new_window",
        title: "Move Tab to New Window",
        description: "Moves a tab into a new window of its own.",
        category: "layout",
        icon: "window_new",
        kind: Kind::View,
    };
    type Params = TabParams;
    type Output = TabPlaced;

    fn run(&self, cx: &mut Context<'_>, p: TabParams) -> Result<TabPlaced, CommandError> {
        let tab = tab(&p.tab)?;
        let group = layout(cx)?.move_to_new_window(&tab)?;
        Ok(TabPlaced { group })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SplitParams {
    #[doc = "The tab: \"start\", a Panel id such as \"properties\", or a Document UUID."]
    tab: String,
    /// Where the new group goes: to the right of the tab's group, or below it.
    side: SplitSide,
}

struct Split;

impl Command for Split {
    const META: Meta = Meta {
        id: "layout.tab.split",
        title: "Split Tab",
        description: "Moves a tab out of its group into a new group to the right of it or below it. A tab alone in its group cannot split, and a tab in a Dock splits only down.",
        category: "layout",
        icon: "split",
        kind: Kind::View,
    };
    type Params = SplitParams;
    type Output = TabPlaced;

    fn run(&self, cx: &mut Context<'_>, p: SplitParams) -> Result<TabPlaced, CommandError> {
        let tab = tab(&p.tab)?;
        let group = layout(cx)?.split(&tab, p.side)?;
        Ok(TabPlaced { group })
    }
}

struct Close;

impl Command for Close {
    const META: Meta = Meta {
        id: "layout.tab.close",
        title: "Close Tab",
        description: "Closes a tab. A closed Panel reopens from the Panels button or its panel.show Preset.",
        category: "layout",
        icon: "close",
        kind: Kind::View,
    };
    type Params = TabParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, p: TabParams) -> Result<Done, CommandError> {
        let tab = tab(&p.tab)?;
        layout(cx)?.close(&tab)?;
        Ok(Done {})
    }
}

fn collapsed_default() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CollapseParams {
    dock: DockSide,
    /// `true` (the default) collapses the Dock to an icon strip; `false` expands it.
    #[serde(default = "collapsed_default")]
    collapsed: bool,
}

struct Collapse;

impl Command for Collapse {
    const META: Meta = Meta {
        id: "layout.dock.collapse",
        title: "Collapse Dock",
        description: "Collapses a Dock to an icon strip whose icons open its tabs as flyouts, or expands it again.",
        category: "layout",
        icon: "dock_collapse",
        kind: Kind::View,
    };
    type Params = CollapseParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, p: CollapseParams) -> Result<Done, CommandError> {
        layout(cx)?.collapse(p.dock, p.collapsed)?;
        Ok(Done {})
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShowPanelParams {
    panel: PanelId,
}

struct ShowPanel;

impl ShowPanel {
    fn title(panel: PanelId) -> &'static str {
        match panel {
            PanelId::Projects => "Show Projects",
            PanelId::Properties => "Show Properties",
            PanelId::Messages => "Show Messages",
            PanelId::Library => "Show Library",
            PanelId::Layers => "Show Layers",
            PanelId::Journal => "Show Journal",
        }
    }
}

impl Command for ShowPanel {
    const META: Meta = Meta {
        id: "panel.show",
        title: "Show Panel",
        description: "Shows and focuses a Panel, reopening it in its usual Dock if it was closed.",
        category: "layout",
        icon: "panel",
        kind: Kind::View,
    };
    type Params = ShowPanelParams;
    type Output = TabPlaced;

    fn presets(&self) -> Vec<Preset> {
        PanelId::ALL
            .into_iter()
            .map(|panel| {
                Preset::new(
                    panel.id(),
                    ShowPanel::title(panel),
                    format!("panel_{}", panel.id()),
                    json!({ "panel": panel }),
                )
            })
            .collect()
    }

    fn run(&self, cx: &mut Context<'_>, p: ShowPanelParams) -> Result<TabPlaced, CommandError> {
        let group = layout(cx)?.show_panel(p.panel)?;
        Ok(TabPlaced { group })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutNameParams {
    /// The Saved Layout's name: lowercase letters, digits and underscores.
    name: String,
}

struct SaveLayout;

impl Command for SaveLayout {
    const META: Meta = Meta {
        id: "layout.save",
        title: "Save Layout",
        description: "Saves the layout as a Saved Layout of the user's: where each Panel is, and which tab groups hold Documents, never which Documents.",
        category: "layout",
        icon: "layout_save",
        kind: Kind::View,
    };
    type Params = LayoutNameParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, p: LayoutNameParams) -> Result<Done, CommandError> {
        saved::check_name(&p.name)?;
        let current = layout(cx)?.clone();
        saved::save(user_dirs(cx)?, &p.name, &current)?;
        Ok(Done {})
    }
}

struct ApplyLayout {
    /// Where the Saved Layouts offered as Presets are.
    dirs: Option<UserDirs>,
}

impl Command for ApplyLayout {
    const META: Meta = Meta {
        id: "layout.apply",
        title: "Apply Layout",
        description: "Applies a Saved Layout: each Panel goes to its saved place, and the open Documents move into its first Document group.",
        category: "layout",
        icon: "layout",
        kind: Kind::View,
    };
    type Params = LayoutNameParams;
    type Output = Done;

    fn presets(&self) -> Vec<Preset> {
        let Some(dirs) = &self.dirs else {
            return Vec::new();
        };
        saved::names(dirs)
            .into_iter()
            .map(|name| {
                Preset::new(
                    name.clone(),
                    format!("Apply Layout {name}"),
                    "layout",
                    json!({ "name": name }),
                )
            })
            .collect()
    }

    fn run(&self, cx: &mut Context<'_>, p: LayoutNameParams) -> Result<Done, CommandError> {
        let saved = saved::load(user_dirs(cx)?, &p.name)?;
        let current = layout(cx)?;
        *current = saved.apply_to(current)?;
        Ok(Done {})
    }
}
