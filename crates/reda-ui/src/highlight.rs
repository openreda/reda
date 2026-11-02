//! Highlighting: objects lit up on every canvas while everything else is dimmed, such as a
//! Net picked on the schematic. It is view state (never saved, never undone, never
//! dirtying a Document), changed only by the view Command `view.highlight`, so scripts,
//! menus and the canvas highlight the same way.
//!
//! The objects are UUIDs, which are unique across Documents, so one list serves every
//! canvas: each marks the items of the objects its Scene draws. Cross-probing between the
//! schematic and the board can add the other side's objects to the same list.

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::Uuid;
use reda_engine::schematic::connectivity::nets_named;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What is highlighted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Highlight {
    /// The highlighted objects, by UUID, on any Document.
    pub objects: Vec<Uuid>,
    /// The name of the highlighted Net, if a Net is what is highlighted.
    pub net: Option<String>,
}

/// Adds `view.highlight` to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(SetHighlight);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HighlightParams {
    /// A schematic Net to highlight, by name, on every sheet.
    #[serde(default)]
    net: Option<String>,
    /// Objects to highlight, by UUID (with `net`: the objects of the Net picked, whose
    /// name it then only labels).
    #[serde(default)]
    objects: Vec<Uuid>,
}

/// How many objects are now highlighted.
#[derive(Serialize, JsonSchema)]
pub struct HighlightResult {
    pub count: usize,
}

struct SetHighlight;

impl Command for SetHighlight {
    const META: Meta = Meta {
        id: "view.highlight",
        title: "Highlight Net",
        description: "Highlights a schematic Net by name on every sheet, or the given \
                      objects, dimming everything else on every canvas. With no params it \
                      clears the highlight. It is view state: not undoable, never saved.",
        category: "view",
        icon: "net",
        kind: Kind::View,
    };
    type Params = HighlightParams;
    type Output = HighlightResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: HighlightParams,
    ) -> Result<HighlightResult, CommandError> {
        let mut objects = p.objects;
        if !objects.is_empty() || p.net.is_some() {
            let project = cx.project()?;
            for (i, uuid) in objects.iter().enumerate() {
                if project.object(*uuid).is_none() {
                    return Err(CommandError::invalid_params(
                        &format!("/objects/{i}"),
                        format!("no object {uuid}"),
                    ));
                }
            }
            if let (Some(net), true) = (&p.net, objects.is_empty()) {
                objects = nets_named(project, net);
                if objects.is_empty() {
                    return Err(CommandError::invalid_params(
                        "/net",
                        format!("no Net is called {net}"),
                    ));
                }
            }
        }
        objects.sort();
        objects.dedup();
        let count = objects.len();
        *cx.view::<Highlight>()? = Highlight {
            objects,
            net: p.net,
        };
        Ok(HighlightResult { count })
    }
}
