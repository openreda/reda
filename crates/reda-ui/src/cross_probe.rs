//! Cross-select and Net cross-probing between the schematic and the PCB: what the canvases
//! mark as highlighted (everything else on a canvas that shows any of it dims). It is
//! worked out from the one highlight state, [`Highlight`] (`view.highlight`, Alt+click on
//! a schematic Net), and from the [`Selection`]. All of it is view state: never undone,
//! never saved in a Document.
//!
//! - **Highlight**: what `view.highlight` holds is always marked.
//! - **Cross-select** mirrors by Component UUID: a selected Unit highlights its Footprint
//!   instance, a selected Footprint every Unit of its Component, whatever sheet it is on.
//!   Un-synced Components have nothing on the other side.
//! - **Net cross-probing** maps through Sync's Net matching ([`sync::net_links`]): a
//!   highlighted schematic Net (Alt+click) or a selected schematic Net object (a Wire, Net
//!   Label, Port or Power Port) lights up its PCB Net, and a highlighted or selected PCB
//!   Net lights up the schematic objects on its schematic Net. A PCB Net shows as its
//!   copper (Tracks, Vias, Zones, Free Pads) and the Footprints with a Pad on it.
//!
//! [`marks`] is pure; other cross-probe features (Find, Select Similar) can use
//! [`pcb_net_of`] and [`schematic_objects_of`] for the same mapping.

use std::collections::BTreeSet;

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::{DocumentKind, Object, PcbDocument, Project, Uuid};
use reda_engine::sync::{self, NetLink};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::highlight::Highlight;
use crate::selection::Selection;

/// Which cross-probes are on. Both are by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrossProbe {
    /// Selection mirrors between Units and Footprints.
    pub selection: bool,
    /// Net highlighting maps between schematic Nets and PCB Nets.
    pub nets: bool,
}

impl Default for CrossProbe {
    fn default() -> CrossProbe {
        CrossProbe {
            selection: true,
            nets: true,
        }
    }
}

/// What the canvases mark highlighted: `highlight`'s objects, and what the cross-probes
/// that are `on` carry across from them and from `selection`; sorted by UUID.
pub fn marks(
    project: &Project,
    selection: &Selection,
    highlight: &Highlight,
    on: CrossProbe,
) -> Vec<Uuid> {
    let mut out = BTreeSet::new();
    if on.selection {
        for uuid in &selection.objects {
            match project.object(*uuid) {
                Some(Object::Unit(unit)) => {
                    let component = &unit.object.component;
                    out.extend(
                        project
                            .pcb()
                            .into_iter()
                            .flat_map(|pcb| pcb.footprints())
                            .filter(|f| f.component.as_ref() == Some(component))
                            .map(|f| f.uuid),
                    );
                }
                Some(Object::FootprintInstance(footprint)) => {
                    if let Some(component) = &footprint.object.component {
                        out.extend(units_of(project, component.uuid));
                    }
                }
                _ => {}
            }
        }
    }
    if on.nets {
        let is_schematic_net = |uuid: &Uuid| {
            matches!(
                project.object(*uuid),
                Some(
                    Object::Wire(_) | Object::NetLabel(_) | Object::Port(_) | Object::PowerPort(_)
                )
            )
        };
        let is_pcb_net = |uuid: &Uuid| matches!(project.object(*uuid), Some(Object::Net(_)));
        // The schematic objects whose Net to carry to the board: the highlighted Net's,
        // and a selected Net object's.
        let schematic: Vec<Uuid> = highlight
            .objects
            .iter()
            .chain(selection.objects.iter().filter(|o| is_schematic_net(o)))
            .copied()
            .collect();
        let mut pcb_nets: BTreeSet<Uuid> = selection.nets.iter().copied().collect();
        pcb_nets.extend(highlight.objects.iter().copied().filter(is_pcb_net));
        if !pcb_nets.is_empty() || schematic.iter().any(is_schematic_net) {
            let links = sync::net_links(project);
            for net in &pcb_nets {
                out.extend(schematic_objects_in(&links, *net));
            }
            for uuid in &schematic {
                pcb_nets.extend(pcb_net_in(&links, *uuid));
            }
            for net in pcb_nets {
                if let Some(pcb) = project.pcb() {
                    out.extend(pcb_objects_of(pcb, net));
                }
            }
        }
    }
    // What is selected is marked as selected already.
    for uuid in &selection.objects {
        out.remove(uuid);
    }
    out.extend(highlight.objects.iter().copied());
    out.into_iter().collect()
}

/// Every Unit of Component `component`, on every sheet.
fn units_of(project: &Project, component: Uuid) -> Vec<Uuid> {
    project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|sheet| sheet.units())
        .filter(|u| u.component.uuid == component)
        .map(|u| u.uuid)
        .collect()
}

/// The PCB Net that schematic object `object` (a Wire, Net Label, Port or Power Port) is
/// on, through Sync's matching; `None` when its Net is not on the board yet.
pub fn pcb_net_of(project: &Project, object: Uuid) -> Option<Uuid> {
    pcb_net_in(&sync::net_links(project), object)
}

/// The schematic objects on the schematic Net that PCB Net `net` matches, on every sheet.
pub fn schematic_objects_of(project: &Project, net: Uuid) -> Vec<Uuid> {
    schematic_objects_in(&sync::net_links(project), net).collect()
}

fn pcb_net_in(links: &[NetLink], object: Uuid) -> Option<Uuid> {
    links
        .iter()
        .find(|l| l.objects.binary_search(&object).is_ok())
        .map(|l| l.pcb)
}

fn schematic_objects_in(links: &[NetLink], net: Uuid) -> impl Iterator<Item = Uuid> + '_ {
    links
        .iter()
        .filter(move |l| l.pcb == net)
        .flat_map(|l| l.objects.iter().copied())
}

/// What draws PCB Net `net`: its copper, and the Footprints with a Pad on it.
pub(crate) fn pcb_objects_of(pcb: &PcbDocument, net: Uuid) -> Vec<Uuid> {
    let on = Some(net);
    pcb.tracks()
        .filter(|t| t.net == on)
        .map(|t| t.uuid)
        .chain(pcb.vias().filter(|v| v.net == on).map(|v| v.uuid))
        .chain(pcb.zones().filter(|z| z.net == on).map(|z| z.uuid))
        .chain(pcb.free_pads().filter(|p| p.net == on).map(|p| p.uuid))
        .chain(
            pcb.footprints()
                .filter(|f| f.pads.iter().any(|p| p.net == on))
                .map(|f| f.uuid),
        )
        .collect()
}

/// Adds the view Commands that switch the cross-probes. The Shell does this.
pub fn register_all(registry: &mut Registry) {
    registry.add(CrossProbeObjects);
    registry.add(SelectionToggle);
    registry.add(NetsToggle);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToggleParams {
    /// On or off; it flips when left out.
    #[serde(default)]
    on: Option<bool>,
}

/// Whether the cross-probe is now on.
#[derive(Serialize, JsonSchema)]
pub struct ToggleResult {
    pub on: bool,
}

/// Sets (or flips) one switch: Net cross-probing when `nets`, else cross-select.
fn toggle(cx: &mut Context<'_>, p: ToggleParams, nets: bool) -> Result<ToggleResult, CommandError> {
    let state = cx.view::<CrossProbe>()?;
    let flag = if nets {
        &mut state.nets
    } else {
        &mut state.selection
    };
    *flag = p.on.unwrap_or(!*flag);
    Ok(ToggleResult { on: *flag })
}

struct SelectionToggle;

impl Command for SelectionToggle {
    const META: Meta = Meta {
        id: "view.cross_select.toggle",
        title: "Cross-Select",
        description: "Switches cross-select on or off: selecting a Unit highlights its \
                      Footprint on the board, and selecting a Footprint highlights every \
                      Unit of its Component on the sheets. On by default; view state only.",
        category: "view",
        icon: "select",
        kind: Kind::View,
    };
    type Params = ToggleParams;
    type Output = ToggleResult;

    fn run(&self, cx: &mut Context<'_>, p: ToggleParams) -> Result<ToggleResult, CommandError> {
        toggle(cx, p, false)
    }
}

struct NetsToggle;

impl Command for NetsToggle {
    const META: Meta = Meta {
        id: "view.net_cross_probe.toggle",
        title: "Cross-Probe Nets",
        description: "Switches Net cross-probing on or off: a highlighted PCB Net lights up \
                      its schematic Net on every sheet, and a selected schematic Net lights \
                      up its PCB Net, matched as Sync matches them. On by default; view \
                      state only.",
        category: "view",
        icon: "net",
        kind: Kind::View,
    };
    type Params = ToggleParams;
    type Output = ToggleResult;

    fn run(&self, cx: &mut Context<'_>, p: ToggleParams) -> Result<ToggleResult, CommandError> {
        toggle(cx, p, true)
    }
}

/// What stands for `objects` on the other side: a Unit's Footprint, a Footprint's Units,
/// a schematic Net object's PCB Net copper and a PCB Net's schematic objects; sorted.
pub fn counterparts(project: &Project, objects: &[Uuid]) -> Vec<Uuid> {
    let selection = Selection {
        objects: objects.to_vec(),
        ..Selection::default()
    };
    let mut nets: Vec<Uuid> = Vec::new();
    if let Some(pcb) = project.pcb() {
        for o in objects {
            let net = pcb
                .track(*o)
                .and_then(|t| t.net)
                .or_else(|| pcb.via(*o).and_then(|v| v.net))
                .or_else(|| pcb.zone(*o).and_then(|z| z.net))
                .or_else(|| pcb.free_pad(*o).and_then(|f| f.net));
            nets.extend(net);
        }
    }
    let highlight = Highlight {
        objects: nets,
        ..Highlight::default()
    };
    let mut out = marks(project, &selection, &highlight, CrossProbe::default());
    out.retain(|o| !highlight.objects.contains(o) && !objects.contains(o));
    out
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CrossProbeParams {
    /// The objects to find on the other side.
    objects: Vec<Uuid>,
}

#[derive(Serialize, JsonSchema)]
pub struct CrossProbeResult {
    /// The objects found on the other side, now highlighted.
    pub objects: Vec<Uuid>,
}

/// `view.cross_probe`: highlights what stands for the objects on the other side
/// (schematic or board) and zooms that side's canvas to it.
struct CrossProbeObjects;

impl Command for CrossProbeObjects {
    const META: Meta = Meta {
        id: "view.cross_probe",
        title: "Cross-Probe",
        description: "Finds the given objects on the other side: a Unit's Footprint, a \
                      Footprint's Units, a Net's copper or schematic objects. Highlights them \
                      and zooms the canvas showing them. View state only.",
        category: "view",
        icon: "net",
        kind: Kind::View,
    };
    type Params = CrossProbeParams;
    type Output = CrossProbeResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: CrossProbeParams,
    ) -> Result<CrossProbeResult, CommandError> {
        let project = cx.project()?;
        let found = counterparts(project, &p.objects);
        let Some(first) = found.first() else {
            return Err(CommandError::rejected(
                "nothing stands for these objects on the other side",
            ));
        };
        let scope = |o: &Uuid| project.object(*o).map(|o| o.scope());
        let zoom = match scope(first) {
            Some(reda_engine::model::Scope::Document(document)) => {
                let shown: Vec<Uuid> = found
                    .iter()
                    .copied()
                    .filter(|o| scope(o) == Some(reda_engine::model::Scope::Document(document)))
                    .collect();
                Some((document, shown))
            }
            _ => None,
        };
        *cx.view::<Highlight>()? = Highlight {
            objects: found.clone(),
            ..Highlight::default()
        };
        if let Some((document, shown)) = zoom {
            crate::bars::request(cx, crate::bars::Request::ZoomTo(document, shown))?;
        }
        Ok(CrossProbeResult { objects: found })
    }
}
