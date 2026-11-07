//! Sparse field text placement, including the legacy single-Designator command.

use std::collections::BTreeSet;

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry,
    Unavailable,
};
use reda_model::{InDocument, Object, TextAutoposition, TextOverride, TextStyle, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::Placed;
use crate::commands::outline::PointParam;
use crate::commands::text::StyleParam;
use crate::commands::{needs_pcb, pcb};
use crate::placed::Frame;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(FootprintTextSet);
    registry.add(DesignatorTextSet);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintTextSetParams {
    pub instances: Vec<Uuid>,
    /// Select every entry drawing this field, across the instances.
    pub field: Option<String>,
    /// Select individual entries by their stable Footprint text UUIDs.
    pub texts: Option<Vec<Uuid>>,
    pub autoposition: Option<TextAutoposition>,
    /// Anchor in board coordinates, for a selection of exactly one text entry. Moving text
    /// sets `manual`.
    pub at: Option<PointParam>,
    /// Reading direction in board coordinates.
    pub rotation: Option<Angle>,
    pub style: Option<StyleParam>,
    pub visible: Option<bool>,
    #[serde(default)]
    pub reset: bool,
}

pub struct FootprintTextSet;
impl Command for FootprintTextSet {
    const META: Meta = Meta {
        id: "pcb.footprint.text.set",
        title: "Set Footprint Text",
        description: "Changes field text across placed Footprints with sparse overrides. Select by field or text entry UUIDs. Anchors and rotations use board coordinates; an anchor moves exactly one text entry and sets manual placement. Reset restores the Footprint defaults. Allowed on locked Footprints.",
        category: "pcb",
        icon: "text_edit",
        kind: Kind::Edit,
    };
    type Params = FootprintTextSetParams;
    type Output = Placed;
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: Self::Params) -> Result<Placed, CommandError> {
        set_texts(cx, p)
    }
}

fn set_texts(cx: &mut Context<'_>, p: FootprintTextSetParams) -> Result<Placed, CommandError> {
    if p.instances.is_empty() {
        return Err(CommandError::invalid_params(
            "/instances",
            "choose at least one Footprint",
        ));
    }
    if p.field.is_some() == p.texts.is_some() {
        return Err(CommandError::invalid_params(
            "/field",
            "choose exactly one of field or texts",
        ));
    }
    if p.field.as_ref().is_some_and(|f| f.trim().is_empty())
        || p.texts.as_ref().is_some_and(Vec::is_empty)
    {
        return Err(CommandError::invalid_params(
            "/field",
            "choose a field or text entries",
        ));
    }
    let edits = p.at.is_some()
        || p.rotation.is_some()
        || p.style.is_some()
        || p.visible.is_some()
        || p.autoposition.is_some();
    if p.reset && edits {
        return Err(CommandError::invalid_params(
            "/reset",
            "reset takes no other changes",
        ));
    }
    if p.at.is_some()
        && p.autoposition
            .is_some_and(|a| a != TextAutoposition::Manual)
    {
        return Err(CommandError::invalid_params(
            "/autoposition",
            "an explicit anchor uses manual placement",
        ));
    }
    let pcb = pcb(cx)?;
    let mut updates = Vec::new();
    let mut seen = BTreeSet::new();
    let mut selected = 0;
    for uuid in &p.instances {
        if !seen.insert(*uuid) {
            continue;
        }
        let mut instance = pcb.footprint(*uuid).cloned().ok_or_else(|| {
            CommandError::invalid_params(
                "/instances",
                format!("{uuid} is not a Footprint on the board"),
            )
        })?;
        let copy = pcb
            .footprint_copy(instance.footprint)
            .expect("committed Footprint copy");
        let entries: Vec<_> = copy
            .footprint
            .texts
            .iter()
            .filter(|text| {
                p.field
                    .as_ref()
                    .is_some_and(|field| text.field.eq_ignore_ascii_case(field))
                    || p.texts.as_ref().is_some_and(|ids| ids.contains(&text.uuid))
            })
            .collect();
        if entries.is_empty() {
            return Err(CommandError::rejected(format!(
                "{} has no matching text entries",
                instance.designator
            )));
        }
        if let Some(ids) = &p.texts
            && ids.iter().any(|id| !entries.iter().any(|t| t.uuid == *id))
        {
            return Err(CommandError::invalid_params(
                "/texts",
                "a text entry does not belong to a selected Footprint",
            ));
        }
        selected += entries.len();
        if p.at.is_some() && selected > 1 {
            // One board point for several texts would stack them all there.
            return Err(CommandError::invalid_params(
                "/at",
                "an anchor moves one text entry; use autoposition to place many",
            ));
        }
        let frame = Frame::of(&instance);
        for entry in entries {
            let current = instance.text_override(entry.uuid).cloned();
            instance.text_overrides.retain(|o| o.text != entry.uuid);
            if p.reset {
                continue;
            }
            let mut change = current.unwrap_or_else(|| TextOverride::new(entry.uuid));
            // Overrides stay sparse: a value equal to the entry's, or `manual` (the default),
            // is left out.
            if let Some(at) = &p.at {
                let at = frame.local(at.point());
                change.at = (at != entry.at).then_some(at);
                change.autoposition = None;
            }
            if let Some(rotation) = p.rotation {
                let rotation = frame.local_angle(rotation.millideg());
                change.rotation = (rotation != entry.rotation.normalized()).then_some(rotation);
            }
            if let Some(position) = p.autoposition {
                change.autoposition = (position != TextAutoposition::Manual).then_some(position);
            }
            if let Some(style) = &p.style {
                let mut resolved = change.style.clone().unwrap_or_else(|| entry.style.clone());
                style.apply(&mut resolved)?;
                change.style = (resolved != entry.style).then_some(resolved);
            }
            if let Some(visible) = p.visible {
                change.visible = (visible != entry.visible).then_some(visible);
            }
            if !change.is_empty() {
                instance.text_overrides.push(change);
            }
        }
        instance.text_overrides.sort_by_key(|o| o.text);
        updates.push(instance);
    }
    let document = pcb.uuid;
    let objects = updates.iter().map(|i| i.uuid).collect();
    for instance in updates {
        cx.tx()?.replace(Object::FootprintInstance(InDocument::new(
            document, instance,
        )))?;
    }
    Ok(Placed { objects })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesignatorTextSetParams {
    pub footprint: Uuid,
    pub at: Option<PointParam>,
    pub rotation: Option<Angle>,
    pub size: Option<Length>,
    pub visible: Option<bool>,
    #[serde(default)]
    pub reset: bool,
}

pub struct DesignatorTextSet;
impl Command for DesignatorTextSet {
    const META: Meta = Meta {
        id: "pcb.designator_text.set",
        title: "Set Designator Text",
        description: "Moves, turns, resizes or hides one Footprint's primary Designator text using sparse text overrides. Anchors and rotations are on the board. Reset restores the Footprint defaults. Allowed on locked Footprints.",
        category: "pcb",
        icon: "text",
        kind: Kind::Edit,
    };
    type Params = DesignatorTextSetParams;
    type Output = Placed;
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: Self::Params) -> Result<Placed, CommandError> {
        let board = pcb(cx)?;
        let instance = board.footprint(p.footprint).ok_or_else(|| {
            CommandError::invalid_params("/footprint", "not a Footprint on the board")
        })?;
        let primary = crate::placed::primary_designator(board, instance)
            .ok_or_else(|| CommandError::rejected("this Footprint draws no Designator text"))?;
        let style = p
            .size
            .map(|size| {
                let height = crate::commands::positive("/size", size)?;
                Ok::<_, CommandError>(StyleParam {
                    font: None,
                    height: Some(size),
                    thickness: Some(Length::from(TextStyle::of_height(height).thickness)),
                    justify: None,
                    knockout: None,
                    clear_knockout: false,
                })
            })
            .transpose()?;
        set_texts(
            cx,
            FootprintTextSetParams {
                instances: vec![p.footprint],
                field: None,
                texts: Some(vec![primary]),
                at: p.at,
                rotation: p.rotation,
                style,
                visible: p.visible,
                reset: p.reset,
                autoposition: None,
            },
        )
    }
}
