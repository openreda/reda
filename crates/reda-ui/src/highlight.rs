//! Highlighting: objects lit up on every canvas while everything else is dimmed, such as a
//! Net picked on the schematic. It is view state (never saved, never undone, never
//! dirtying a Document), changed only by the view Command `view.highlight`, so scripts,
//! menus and the canvas highlight the same way.
//!
//! The objects are UUIDs, which are unique across Documents, so one list serves every
//! canvas: each marks the items of the objects its Scene draws. Cross-probing
//! ([`crate::cross_probe`]) carries a highlighted schematic Net to its PCB Net and back,
//! so the canvases mark the other side's objects too.

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
    /// Everything else is masked (greyed, and can't be hit) rather than dimmed.
    pub mask: bool,
}

/// Adds `view.highlight` to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(SetHighlight);
    registry.add(ClearHighlight);
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
    /// Mask everything else (greyed, and skipped by hit-testing) instead of dimming it.
    #[serde(default)]
    mask: bool,
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
                      objects, dimming everything else on every canvas (or, with `mask`, \
                      masking it: greyed and not pickable). With no params it clears the \
                      highlight. It is view state: not undoable, never saved.",
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
            mask: p.mask && !objects.is_empty(),
            objects,
            net: p.net,
        };
        Ok(HighlightResult { count })
    }
}

struct ClearHighlight;

impl Command for ClearHighlight {
    const META: Meta = Meta {
        id: "view.highlight.clear",
        title: "Clear Highlight",
        description: "Clears the highlight, dimming and masking on every canvas, and any \
                      measurements shown. It never clears the selection.",
        category: "view",
        icon: "close",
        kind: Kind::View,
    };
    type Params = reda_engine::command::NoParams;
    type Output = HighlightResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        _: reda_engine::command::NoParams,
    ) -> Result<HighlightResult, CommandError> {
        *cx.view::<Highlight>()? = Highlight::default();
        Ok(HighlightResult { count: 0 })
    }
}
