//! Cross-select and Net cross-probing between the schematic and the PCB
//! (docs/spec/v0.1/sync.md#cross-select-and-net-highlighting). All of it is view state:
//! never undone, never saved in a Document.
//!
//! - **Cross-select** mirrors the selection by Component UUID: [`mirror`] gives what a
//!   pick selects on the other side (a Unit's Footprint, one per Component at its first
//!   picked Unit; a Footprint's Units on every sheet), in pick order. The Shell keeps it
//!   in `Selection.mirrored`, so it is really selected: the active Document's share of
//!   it is in `$selection` ([`in_pick_order`]), and edit Commands still only get the
//!   active Document's objects. Un-synced Components have nothing on the other side.
//!   With the `cross_select.dim` Preference it is also marked highlighted, so the rest
//!   of its canvas dims.
//! - **Highlight**: what `view.highlight` holds is always marked.
//! - **Net cross-probing** maps through Sync's Net matching ([`sync::net_links`]): a
//!   highlighted schematic Net (Alt+click) or a selected schematic Net object (a Wire, Net
//!   Label, Port or Power Port) lights up its PCB Net, and a highlighted or selected PCB
//!   Net lights up the schematic objects on its schematic Net. A PCB Net shows as its
//!   copper (Tracks, Vias, Zones, Free Pads) and the Footprints with a Pad on it.
//! - **Cross-probe** (`view.cross_probe`, `Mod`+Shift+C) always highlights and zooms to
//!   the other side; its `jump` option makes that tab active.
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

/// What the canvases mark highlighted: `highlight`'s objects, what Net cross-probing
/// carries across from it and from `selection`, and with `dim` the cross-selected
/// objects (`selection.mirrored`, so everything else on their canvas dims); sorted by
/// UUID.
pub fn marks(
    project: &Project,
    selection: &Selection,
    highlight: &Highlight,
    on: CrossProbe,
    dim: bool,
) -> Vec<Uuid> {
    let mut out = BTreeSet::new();
    if on.selection && dim {
        out.extend(selection.mirrored.iter().copied());
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

/// What cross-select selects on the other side for `picks`, in pick order: a picked
/// Unit's Footprint, once per Component at its first picked Unit, and a picked
/// Footprint's Units on every sheet. Nothing that is picked itself, and nothing for
/// un-synced Components.
pub fn mirror(project: &Project, picks: &[Uuid]) -> Vec<Uuid> {
    let mut out: Vec<Uuid> = Vec::new();
    for pick in picks {
        for o in counterparts_of(project, *pick) {
            if !out.contains(&o) && !picks.contains(&o) {
                out.push(o);
            }
        }
    }
    out
}

/// What stands for `object` on the other side: a Unit's Footprints, a Footprint's Units.
fn counterparts_of(project: &Project, object: Uuid) -> Vec<Uuid> {
    match project.object(object) {
        Some(Object::Unit(unit)) => project
            .pcb()
            .into_iter()
            .flat_map(|pcb| pcb.footprints())
            .filter(|f| f.component.as_ref() == Some(&unit.object.component))
            .map(|f| f.uuid)
            .collect(),
        Some(Object::FootprintInstance(footprint)) => match &footprint.object.component {
            Some(component) => units_of(project, component.uuid),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Everything selected, picks and what cross-select mirrored, in pick order: each pick
/// followed by its mirrored objects not yet listed.
pub fn in_pick_order(project: &Project, selection: &Selection) -> Vec<Uuid> {
    let mut out: Vec<Uuid> = Vec::new();
    for pick in &selection.objects {
        if !out.contains(pick) {
            out.push(*pick);
        }
        for o in counterparts_of(project, *pick) {
            if selection.mirrored.contains(&o) && !out.contains(&o) {
                out.push(o);
            }
        }
    }
    out.extend(
        selection
            .mirrored
            .iter()
            .filter(|o| !out.contains(o))
            .copied()
            .collect::<Vec<_>>(),
    );
    out
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
    registry.add(RepositionToggle);
    registry.add(FootprintsOfSheet);
}

/// The Footprints of the Components with a Unit on sheet `sheet`, in natural Designator
/// order; `None` when `sheet` is not a Schematic Sheet.
pub fn footprints_of_sheet(project: &Project, sheet: Uuid) -> Option<Vec<Uuid>> {
    let units: Vec<Uuid> = project
        .schematic_sheet(sheet)?
        .units()
        .map(|u| u.uuid)
        .collect();
    let pcb = project.pcb();
    let mut found: Vec<(String, Uuid)> = mirror(project, &units)
        .into_iter()
        .filter_map(|f| pcb?.footprint(f).map(|fp| (fp.designator.clone(), f)))
        .collect();
    found.sort_by(|a, b| reda_engine::schematic::natural_cmp(&a.0, &b.0).then(a.1.cmp(&b.1)));
    Some(found.into_iter().map(|(_, f)| f).collect())
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetParams {
    /// The Schematic Sheet.
    document: Uuid,
}

/// `selection.footprints_of_sheet`: the sheet tab's Select Footprints of This Sheet.
struct FootprintsOfSheet;

impl Command for FootprintsOfSheet {
    const META: Meta = Meta {
        id: "selection.footprints_of_sheet",
        title: "Select Footprints of This Sheet",
        description: "Selects the board Footprints of every Component with a Unit on the \
                      given sheet, in natural Designator order (R1, R2, R10). Un-synced \
                      Components have none. View state only.",
        category: "selection",
        icon: "select",
        kind: Kind::View,
    };
    type Params = SheetParams;
    type Output = crate::selection::SelectionSetResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SheetParams,
    ) -> Result<crate::selection::SelectionSetResult, CommandError> {
        let footprints = footprints_of_sheet(cx.project()?, p.document)
            .ok_or_else(|| CommandError::not_found(format!("no Schematic Sheet {}", p.document)))?;
        let count = footprints.len();
        *cx.view::<Selection>()? = Selection {
            objects: footprints,
            ..Selection::default()
        };
        Ok(crate::selection::SelectionSetResult { count })
    }
}

/// Whether Reposition is to be on now.
#[derive(Serialize, JsonSchema)]
pub struct RepositionAsked {}

/// `view.cross_select.reposition.toggle` (`Mod`+Shift+R): the Shell flips the
/// `cross_select.reposition` Preference after it runs.
struct RepositionToggle;

impl Command for RepositionToggle {
    const META: Meta = Meta {
        id: "view.cross_select.reposition.toggle",
        title: "Cross-Select Reposition",
        description: "Switches the cross-select Reposition Preference on or off: with it on, \
                      picking a Unit floats its Footprint on the PCB cursor when the PCB \
                      canvas is visible (locked Footprints are skipped).",
        category: "view",
        icon: "move",
        kind: Kind::View,
    };
    type Params = reda_engine::command::NoParams;
    type Output = RepositionAsked;

    fn run(
        &self,
        _: &mut Context<'_>,
        _: reda_engine::command::NoParams,
    ) -> Result<RepositionAsked, CommandError> {
        Ok(RepositionAsked {})
    }
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
        mirrored: mirror(project, objects),
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
    let mut out = marks(project, &selection, &highlight, CrossProbe::default(), true);
    out.retain(|o| !highlight.objects.contains(o) && !objects.contains(o));
    out
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CrossProbeParams {
    /// The objects to find on the other side.
    objects: Vec<Uuid>,
    /// Jump: also make the other side's tab the active one (opening it if needed).
    #[serde(default)]
    jump: bool,
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
                      Footprint's Units, a Net's copper or schematic objects. As the shared \
                      highlight Preference says (not the cross-select switches), it \
                      highlights them so everything else dims, and zooms the canvas showing \
                      them; with `jump` that tab also becomes the active one. View state \
                      only.",
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
        // The shared highlight Preference: dim and zoom.
        let prefs = cx
            .view::<reda_engine::preferences::HighlightPreferences>()?
            .clone();
        if prefs.dim {
            *cx.view::<Highlight>()? = Highlight {
                objects: found.clone(),
                ..Highlight::default()
            };
        }
        if let Some((document, shown)) = zoom {
            if p.jump {
                cx.view::<crate::layout::Layout>()?.open_document(document);
            }
            if prefs.zoom {
                let request = crate::bars::Request::ZoomClose(document, shown, prefs.closeness);
                crate::bars::request(cx, request)?;
            }
        }
        Ok(CrossProbeResult { objects: found })
    }
}
