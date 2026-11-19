//! Edit the ordered Via types stored with the Stackup.
use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Unavailable,
};
use reda_model::geom::Nm;
use reda_model::{InDocument, LayerSpan, Object, PcbDocument, Uuid, ViaDiameter, ViaType};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Created, needs_pcb, pcb};

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum DiameterParam {
    Uniform(Length),
    Stacked(Diameters),
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Diameters {
    start: Length,
    middle: Option<Length>,
    end: Option<Length>,
}

impl DiameterParam {
    pub(super) fn diameter(self) -> ViaDiameter {
        match self {
            Self::Uniform(l) => l.nm().into(),
            Self::Stacked(d) => ViaDiameter {
                start: d.start.nm(),
                middle: d.middle.map(Length::nm),
                end: d.end.map(Length::nm),
            },
        }
    }
}

pub(super) fn check_sizes(d: ViaDiameter, hole: Nm, pointer: &str) -> Result<(), CommandError> {
    if hole <= Nm::ZERO {
        return Err(CommandError::invalid_params(
            "/hole",
            "the hole must be positive",
        ));
    }
    if !d.fits_hole(hole) {
        return Err(CommandError::invalid_params(
            pointer,
            "every diameter must be larger than the hole",
        ));
    }
    Ok(())
}

pub(super) fn check_span(pcb: &PcbDocument, span: LayerSpan) -> Result<(), CommandError> {
    for (pointer, layer) in [("/from", span.from), ("/to", span.to)] {
        if pcb.copper_number(layer).is_none() {
            return Err(CommandError::invalid_params(
                pointer,
                "the Via must join copper Layers",
            ));
        }
    }
    if span.from == span.to {
        return Err(CommandError::invalid_params(
            "/to",
            "the Via must join two different Layers",
        ));
    }
    Ok(())
}

pub(super) struct Add;
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddParams {
    document: Uuid,
    #[serde(default)]
    unused_layers: reda_model::UnusedLayers,
    from: Uuid,
    to: Uuid,
    uuid: Option<Uuid>,
    name: Option<String>,
    #[serde(default)]
    micro: bool,
    hole: Option<Length>,
    diameter: Option<DiameterParam>,
}

impl Command for Add {
    const META: Meta = Meta {
        id: "pcb.via_type.add",
        title: "Add Via Type",
        description: "Adds a Via type for a distinct pair of copper Layers.",
        category: "pcb",
        icon: "via",
        kind: Kind::Edit,
    };
    type Params = AddParams;
    type Output = Created;
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: AddParams) -> Result<Created, CommandError> {
        let uuid = p.uuid.unwrap_or_else(|| cx.new_id());
        let pcb = pcb(cx)?;
        if pcb.uuid != p.document {
            return Err(CommandError::not_found("no such PCB Document"));
        }
        let mut stackup = pcb.stackup().clone();
        if stackup.entries.iter().any(|e| e.uuid() == uuid)
            || stackup.via_types.iter().any(|v| v.uuid == uuid)
            || pcb.layers().layer(uuid).is_some()
            || cx.project()?.object(uuid).is_some()
        {
            return Err(CommandError::invalid_params(
                "/uuid",
                "that UUID is already in use",
            ));
        }
        let through = stackup.through_type();
        let kind = ViaType {
            uuid,
            unused_layers: p.unused_layers,
            from: p.from,
            to: p.to,
            name: p.name.filter(|n| !n.trim().is_empty()),
            micro: p.micro,
            hole: p.hole.map_or(through.hole, Length::nm),
            diameter: p.diameter.map_or(through.diameter, DiameterParam::diameter),
        };
        validate(pcb, &kind)?;
        let vias = pcb
            .vias()
            .filter(|v| v.span.joins(kind.span()) && v.span != kind.span())
            .map(|v| {
                let mut via = v.clone();
                via.set_span(kind.span());
                via
            })
            .collect();
        stackup.via_types.push(kind);
        Plan {
            stackup,
            vias,
            rules: Vec::new(),
            counts: Default::default(),
        }
        .apply(cx)?;
        Ok(Created { uuid })
    }
}

fn validate(pcb: &PcbDocument, kind: &ViaType) -> Result<(), CommandError> {
    check_span(pcb, kind.span())?;
    if pcb
        .stackup()
        .via_types
        .iter()
        .any(|v| v.uuid != kind.uuid && v.span().joins(kind.span()))
    {
        return Err(CommandError::rejected(
            "another Via type already joins these Layers",
        ));
    }
    if kind.micro
        && pcb
            .copper_number(kind.from)
            .unwrap()
            .abs_diff(pcb.copper_number(kind.to).unwrap())
            > 2
    {
        return Err(CommandError::invalid_params(
            "/micro",
            "a micro Via may skip at most one copper Layer",
        ));
    }
    check_sizes(kind.diameter, kind.hole, "/diameter")
}

pub(super) struct Set;
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetParams {
    via_type: Uuid,
    unused_layers: Option<reda_model::UnusedLayers>,
    from: Option<Uuid>,
    to: Option<Uuid>,
    #[serde(default, deserialize_with = "super::nullable")]
    #[schemars(with = "Option<String>")]
    name: Option<Option<String>>,
    micro: Option<bool>,
    diameter: Option<DiameterParam>,
    hole: Option<Length>,
    #[serde(default)]
    move_vias: bool,
}

pub(super) struct Plan {
    pub stackup: reda_model::Stackup,
    pub vias: Vec<reda_model::Via>,
    pub rules: Vec<reda_model::DesignRule>,
    pub counts: super::stackup::preview::Counts,
}

impl Plan {
    fn apply(self, cx: &mut Context<'_>) -> Result<(), CommandError> {
        let document = pcb(cx)?.uuid;
        let tx = cx.tx()?;
        tx.replace(Object::Stackup(InDocument::new(document, self.stackup)))?;
        for rule in self.rules {
            tx.replace(Object::DesignRule(InDocument::new(document, rule)))?;
        }
        for via in self.vias {
            tx.replace(Object::Via(InDocument::new(document, via)))?;
        }
        Ok(())
    }
}

fn index(stackup: &reda_model::Stackup, uuid: Uuid) -> Result<usize, CommandError> {
    stackup
        .via_types
        .iter()
        .position(|v| v.uuid == uuid)
        .ok_or_else(|| CommandError::not_found("no such Via type"))
}

pub(super) fn set_plan(pcb: &PcbDocument, p: SetParams) -> Result<Plan, CommandError> {
    let mut stackup = pcb.stackup().clone();
    let i = index(&stackup, p.via_type)?;
    let through = stackup.through_type().uuid;
    let kind = &mut stackup.via_types[i];
    let old = kind.clone();
    if let Some(mode) = p.unused_layers {
        kind.unused_layers = mode;
    }
    kind.from = p.from.unwrap_or(kind.from);
    kind.to = p.to.unwrap_or(kind.to);
    kind.micro = p.micro.unwrap_or(kind.micro);
    if kind.uuid == through && (kind.span() != old.span() || kind.micro) {
        return Err(CommandError::rejected(
            "the through type must be drilled from top to bottom",
        ));
    }
    if let Some(name) = p.name {
        kind.name = name.filter(|n| !n.trim().is_empty());
    }
    if let Some(hole) = p.hole {
        kind.hole = hole.nm();
    }
    if let Some(d) = p.diameter {
        kind.diameter = d.diameter();
    }
    validate(pcb, kind)?;
    let mut counts = super::stackup::preview::Counts::default();
    let mut vias = Vec::new();
    if kind != &old {
        counts.via_types_changed = 1;
    }
    if kind.span() != old.span() {
        let (old_len, new_len) = (
            pcb.span_layers(old.span()).len(),
            pcb.span_layers(kind.span()).len(),
        );
        for via in pcb.vias() {
            let used = via.span.joins(old.span());
            counts.vias_using_span += usize::from(used);
            if (used && p.move_vias) || via.span.joins(kind.span()) {
                if via.span != kind.span() {
                    let mut moved = via.clone();
                    moved.set_span(kind.span());
                    vias.push(moved);
                    if used {
                        counts.vias_stretched += usize::from(new_len > old_len);
                        counts.vias_shortened += usize::from(new_len < old_len);
                    }
                }
            } else if used {
                counts.vias_without_type += 1;
            }
        }
    }
    Ok(Plan {
        stackup,
        vias,
        counts,
        rules: Vec::new(),
    })
}

impl Command for Set {
    const META: Meta = Meta {
        id: "pcb.via_type.set",
        title: "Set Via Type",
        description: "Changes a Via type, optionally moving Vias on its original span.",
        category: "pcb",
        icon: "via",
        kind: Kind::Edit,
    };
    type Params = SetParams;
    type Output = ();
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: SetParams) -> Result<(), CommandError> {
        set_plan(pcb(cx)?, p)?.apply(cx)
    }
}

pub(super) struct Remove;
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveParams {
    via_type: Uuid,
}

pub(super) fn remove_plan(pcb: &PcbDocument, p: RemoveParams) -> Result<Plan, CommandError> {
    let mut stackup = pcb.stackup().clone();
    let i = index(&stackup, p.via_type)?;
    if stackup.through_type().uuid == p.via_type {
        return Err(CommandError::rejected(
            "the through Via type cannot be removed",
        ));
    }
    let removed = stackup.via_types.remove(i);
    let used = pcb.vias().filter(|v| v.span.joins(removed.span())).count();
    let mut counts = super::stackup::preview::Counts {
        vias_using_span: used,
        vias_without_type: used,
        via_types_removed: 1,
        ..Default::default()
    };
    let mut rules = Vec::new();
    for rule in pcb.rules().filter(|r| {
        r.values
            .allowed
            .as_ref()
            .is_some_and(|a| a.contains(&p.via_type))
    }) {
        let mut rule = rule.clone();
        let allowed = rule.values.allowed.as_mut().unwrap();
        allowed.retain(|id| *id != p.via_type);
        counts.allowed_lists_emptied += usize::from(allowed.is_empty());
        counts.rules_changed += 1;
        rules.push(rule);
    }
    Ok(Plan {
        stackup,
        vias: Vec::new(),
        counts,
        rules,
    })
}

impl Command for Remove {
    const META: Meta = Meta {
        id: "pcb.via_type.remove",
        title: "Remove Via Type",
        description: "Removes a Via type from the Stackup and all Net Class permissions. Existing Vias keep their spans.",
        category: "pcb",
        icon: "via",
        kind: Kind::Edit,
    };
    type Params = RemoveParams;
    type Output = ();
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: RemoveParams) -> Result<(), CommandError> {
        remove_plan(pcb(cx)?, p)?.apply(cx)
    }
}

pub(super) struct Reorder;
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReorderParams {
    via_type: Uuid,
    before: Option<Uuid>,
}

impl Command for Reorder {
    const META: Meta = Meta {
        id: "pcb.via_type.reorder",
        title: "Reorder Via Type",
        description: "Moves a Via type before another, or to the end of the table.",
        category: "pcb",
        icon: "via",
        kind: Kind::Edit,
    };
    type Params = ReorderParams;
    type Output = ();
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: ReorderParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut stack = pcb.stackup().clone();
        let i = index(&stack, p.via_type)?;
        if p.before == Some(p.via_type) {
            return Ok(());
        }
        let kind = stack.via_types.remove(i);
        let j = p
            .before
            .map(|id| index(&stack, id))
            .transpose()?
            .unwrap_or(stack.via_types.len());
        stack.via_types.insert(j, kind);
        cx.tx()?
            .replace(Object::Stackup(InDocument::new(document, stack)))?;
        Ok(())
    }
}
