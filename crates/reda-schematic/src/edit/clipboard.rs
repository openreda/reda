//! The clipboard: `schematic.copy`, `schematic.cut`, `schematic.paste`,
//! `schematic.duplicate` and `schematic.copy_as_text`.
//!
//! The clipboard is plain data that `copy` and `cut` return and `paste` takes back, so a
//! front end can keep it on the system clipboard and paste into any sheet, or any Project.
//! It carries the part-table entries and Footprint copies its Units use, so pasting needs
//! no Library and works after the originals are gone.

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::geom::{Nm, Point};
use reda_model::{
    ComponentRef, FootprintCopy, InDocument, NetLabel, NoConnect, Object, PartEntry, Port,
    PowerPort, Project, SheetGraphic, SheetText, UnitInstance, Uuid, Wire,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::bounds::union;
use super::delete::delete_objects;
use super::selection::{one_sheet, resolve};
use crate::component::{At, next_free_designator};
use crate::normalise::{Wiring, normalise};

/// Copied sheet objects, as `schematic.copy` and `schematic.cut` return them and
/// `schematic.paste` takes them. Treat it as opaque.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Clipboard {
    /// The lower-left corner of the copied objects' extent; pasting puts it at `at`.
    pub origin: Point,
    /// The part-table entries the copied Units use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<PartEntry>,
    /// The Footprint copies those entries list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footprints: Vec<FootprintCopy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<UnitInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wires: Vec<Wire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub net_labels: Vec<NetLabel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_connects: Vec<NoConnect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<Port>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub power_ports: Vec<PowerPort>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<SheetText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphics: Vec<SheetGraphic>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopyParams {
    /// The objects to copy, by UUID, all on one sheet.
    pub objects: Vec<Uuid>,
    /// The reference point a paste puts at its `at` (Clipboard Reference); the lower-left
    /// corner of the copied objects by default.
    pub origin: Option<At>,
}

pub struct Copy;

impl Command for Copy {
    const META: Meta = Meta {
        id: "schematic.copy",
        title: "Copy",
        description: "Copies the selected objects, with the part-table entries their Units \
                      use, and returns them as a clipboard for schematic.paste. Changes \
                      nothing.",
        category: "schematic",
        icon: "copy",
        kind: Kind::Query,
    };
    type Params = CopyParams;
    type Output = Clipboard;

    fn run(&self, cx: &mut Context<'_>, p: CopyParams) -> Result<Clipboard, CommandError> {
        let project = cx.project()?;
        let objects = resolve(project, &p.objects, "/objects")?;
        let mut clipboard = copy(project, &objects)?;
        if let Some(origin) = p.origin {
            clipboard.origin = origin.point();
        }
        Ok(clipboard)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CutParams {
    /// The objects to cut, by UUID, all on one sheet.
    pub objects: Vec<Uuid>,
    /// The reference point a paste puts at its `at` (Clipboard Reference); the lower-left
    /// corner of the cut objects by default.
    pub origin: Option<At>,
    /// How the Wires this edit touches are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

pub struct Cut;

impl Command for Cut {
    const META: Meta = Meta {
        id: "schematic.cut",
        title: "Cut",
        description: "Copies the selected objects as schematic.copy does, then deletes them \
                      as schematic.delete does, and returns the clipboard.",
        category: "schematic",
        icon: "cut",
        kind: Kind::Edit,
    };
    type Params = CutParams;
    type Output = Clipboard;

    fn run(&self, cx: &mut Context<'_>, p: CutParams) -> Result<Clipboard, CommandError> {
        let project = cx.project()?;
        let objects = resolve(project, &p.objects, "/objects")?;
        let mut clipboard = copy(project, &objects)?;
        if let Some(origin) = p.origin {
            clipboard.origin = origin.point();
        }
        delete_objects(cx, &objects, &p.wiring.unwrap_or_default())?;
        Ok(clipboard)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PasteParams {
    /// The Schematic Sheet to paste on.
    pub document: Uuid,
    /// What schematic.copy or schematic.cut returned.
    pub clipboard: Clipboard,
    /// Where the clipboard's origin (the lower-left corner of what was copied) goes; where
    /// it was copied from by default.
    pub at: Option<At>,
    /// How the Wires this edit touches are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

/// The objects a paste made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PasteResult {
    /// The new objects, in clipboard order (Units, Wires, Net Labels, No-Connect Markers,
    /// Ports, Power Ports, texts, graphics), ready to select.
    pub objects: Vec<Uuid>,
}

pub struct Paste;

impl Command for Paste {
    const META: Meta = Meta {
        id: "schematic.paste",
        title: "Paste",
        description: "Pastes a clipboard onto a Schematic Sheet. Everything gets a fresh \
                      UUID, even after a cut; each copied Component becomes a new Component, \
                      its copied Units staying together, and keeps its Designator if no \
                      Component uses it, else takes the next free one. Part-table entries \
                      the sheet lacks are added. schematic.selection.move_to_sheet keeps \
                      UUIDs instead.",
        category: "schematic",
        icon: "paste",
        kind: Kind::Edit,
    };
    type Params = PasteParams;
    type Output = PasteResult;

    fn run(&self, cx: &mut Context<'_>, p: PasteParams) -> Result<PasteResult, CommandError> {
        crate::sheet(cx.project()?, p.document)?;
        let offset =
            p.at.map_or(Point::default(), |at| at.point() - p.clipboard.origin);
        let wiring = p.wiring.unwrap_or_default();
        let objects = paste(cx, p.document, p.clipboard, offset, &wiring)?;
        Ok(PasteResult { objects })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DuplicateParams {
    /// The objects to duplicate, by UUID, all on one sheet.
    pub objects: Vec<Uuid>,
    /// How far from the originals the copies go; 100 mil right and 100 mil down by
    /// default.
    pub offset: Option<At>,
    /// How the Wires this edit touches are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

pub struct Duplicate;

impl Command for Duplicate {
    const META: Meta = Meta {
        id: "schematic.duplicate",
        title: "Duplicate",
        description: "Copies the selected objects and pastes them on the same sheet at an \
                      offset, with Designators following schematic.paste's rule.",
        category: "schematic",
        icon: "duplicate",
        kind: Kind::Edit,
    };
    type Params = DuplicateParams;
    type Output = PasteResult;

    fn run(&self, cx: &mut Context<'_>, p: DuplicateParams) -> Result<PasteResult, CommandError> {
        let project = cx.project()?;
        let objects = resolve(project, &p.objects, "/objects")?;
        let document = one_sheet(&objects, "/objects")?;
        let clipboard = copy(project, &objects)?;
        let step = Nm::from_mil(100);
        let offset = p.offset.map_or(Point::new(step, -step), At::point);
        let objects = paste(
            cx,
            document,
            clipboard,
            offset,
            &p.wiring.unwrap_or_default(),
        )?;
        Ok(PasteResult { objects })
    }
}

/// Copies `objects` (all on one sheet) with the part table their Units need.
fn copy(project: &Project, objects: &[Object]) -> Result<Clipboard, CommandError> {
    let document = one_sheet(objects, "/objects")?;
    let body = crate::sheet(project, document)?;
    let mut clip = Clipboard {
        origin: union(body, objects).min,
        parts: vec![],
        footprints: vec![],
        units: vec![],
        wires: vec![],
        net_labels: vec![],
        no_connects: vec![],
        ports: vec![],
        power_ports: vec![],
        texts: vec![],
        graphics: vec![],
    };
    let mut entries = BTreeSet::new();
    for object in objects {
        match object {
            Object::Unit(u) => {
                entries.insert(u.part);
                clip.units.push(u.object.clone());
            }
            Object::Wire(w) => clip.wires.push(w.object.clone()),
            Object::NetLabel(l) => clip.net_labels.push(l.object.clone()),
            Object::NoConnect(m) => clip.no_connects.push(m.object.clone()),
            Object::Port(p) => clip.ports.push(p.object.clone()),
            Object::PowerPort(p) => clip.power_ports.push(p.object.clone()),
            Object::SheetText(t) => clip.texts.push(t.object.clone()),
            Object::SheetGraphic(g) => clip.graphics.push(g.object.clone()),
            _ => unreachable!("selections hold only sheet objects"),
        }
    }
    let mut copies = BTreeSet::new();
    for uuid in entries {
        let entry = body.part(uuid).expect("a Unit's part entry exists");
        copies.extend(entry.footprints.iter().copied());
        clip.parts.push(entry.clone());
    }
    clip.footprints = copies
        .into_iter()
        .map(|uuid| body.footprint(uuid).expect("listed copies exist").clone())
        .collect();
    Ok(clip)
}

/// Whether no Unit in the Project, the ones pasted so far included, carries `designator`.
/// A blank Designator is never free, so a paste always ends annotated.
fn designator_is_free(project: &Project, designator: &str) -> bool {
    !designator.trim().is_empty()
        && !project
            .documents()
            .filter_map(|d| project.schematic_sheet(d.uuid))
            .flat_map(|s| s.units())
            .any(|u| u.designator == designator)
}

/// Pastes `clip` onto `document`, moved by `offset`, and returns the new objects.
fn paste(
    cx: &mut Context<'_>,
    document: Uuid,
    clip: Clipboard,
    offset: Point,
    wiring: &Wiring,
) -> Result<Vec<Uuid>, CommandError> {
    let bad = |why: String| CommandError::invalid_params("/clipboard", why);
    let body = crate::sheet(cx.project()?, document)?.clone();

    // The part table: reuse the sheet's matching copies, add the rest.
    let mut copy_ids: BTreeMap<Uuid, Uuid> = BTreeMap::new();
    for mut copy in clip.footprints {
        let old = copy.uuid;
        let new = match body.footprint_copy_for(copy.footprint.uuid, &copy.hash) {
            Some(existing) => existing.uuid,
            None => {
                copy.uuid = cx.new_id();
                let uuid = copy.uuid;
                cx.tx()?
                    .insert(Object::FootprintCopy(InDocument::new(document, copy)))?;
                uuid
            }
        };
        copy_ids.insert(old, new);
    }
    let mut entry_ids: BTreeMap<Uuid, (Uuid, String)> = BTreeMap::new();
    for mut entry in clip.parts {
        let old = entry.uuid;
        let prefix = entry.symbol.designator_prefix.clone();
        let new = match body.part_entry_for(entry.part.uuid, &entry.hash) {
            Some(existing) => existing.uuid,
            None => {
                entry.footprints = entry
                    .footprints
                    .iter()
                    .map(|c| {
                        copy_ids.get(c).copied().ok_or_else(|| {
                            bad(format!("part entry {old} lists a Footprint copy it lacks"))
                        })
                    })
                    .collect::<Result<_, _>>()?;
                entry.uuid = cx.new_id();
                let uuid = entry.uuid;
                cx.tx()?
                    .insert(Object::PartEntry(InDocument::new(document, entry)))?;
                uuid
            }
        };
        entry_ids.insert(old, (new, prefix));
    }

    let mut pasted = vec![];
    // Each copied Component becomes a new one; its copied Units stay together.
    let mut components: BTreeMap<Uuid, (Uuid, String)> = BTreeMap::new();
    for mut unit in clip.units {
        let Some((entry, prefix)) = entry_ids.get(&unit.part) else {
            return Err(bad(format!(
                "Unit {} uses part entry {}, which the clipboard lacks",
                unit.uuid, unit.part
            )));
        };
        let (component, designator) = match components.get(&unit.component.uuid) {
            Some(known) => known.clone(),
            None => {
                let project = cx.project()?;
                let designator = match designator_is_free(project, &unit.designator) {
                    true => unit.designator.clone(),
                    false => next_free_designator(project, prefix),
                };
                let fresh = (cx.new_id(), designator);
                components.insert(unit.component.uuid, fresh.clone());
                fresh
            }
        };
        unit.uuid = cx.new_id();
        unit.part = *entry;
        unit.component = ComponentRef::new(component);
        unit.designator = designator;
        unit.at = unit.at + offset;
        pasted.push(unit.uuid);
        cx.tx()?
            .insert(Object::Unit(InDocument::new(document, unit)))?;
    }
    for mut wire in clip.wires {
        wire.uuid = cx.new_id();
        for v in &mut wire.vertices {
            *v = *v + offset;
        }
        pasted.push(wire.uuid);
        cx.tx()?
            .insert(Object::Wire(InDocument::new(document, wire)))?;
    }
    for mut label in clip.net_labels {
        label.uuid = cx.new_id();
        label.at = label.at + offset;
        pasted.push(label.uuid);
        cx.tx()?
            .insert(Object::NetLabel(InDocument::new(document, label)))?;
    }
    for mut marker in clip.no_connects {
        marker.uuid = cx.new_id();
        marker.at = marker.at + offset;
        pasted.push(marker.uuid);
        cx.tx()?
            .insert(Object::NoConnect(InDocument::new(document, marker)))?;
    }
    for mut port in clip.ports {
        port.uuid = cx.new_id();
        port.at = port.at + offset;
        pasted.push(port.uuid);
        cx.tx()?
            .insert(Object::Port(InDocument::new(document, port)))?;
    }
    for mut port in clip.power_ports {
        port.uuid = cx.new_id();
        port.at = port.at + offset;
        pasted.push(port.uuid);
        cx.tx()?
            .insert(Object::PowerPort(InDocument::new(document, port)))?;
    }
    for mut text in clip.texts {
        text.uuid = cx.new_id();
        text.at = text.at + offset;
        pasted.push(text.uuid);
        cx.tx()?
            .insert(Object::SheetText(InDocument::new(document, text)))?;
    }
    for mut graphic in clip.graphics {
        graphic.uuid = cx.new_id();
        let (a, b) = graphic.shape.ends();
        graphic.shape = graphic.shape.with_ends(a + offset, b + offset);
        pasted.push(graphic.uuid);
        cx.tx()?
            .insert(Object::SheetGraphic(InDocument::new(document, graphic)))?;
    }
    let lineage = normalise(cx, document, &body, wiring)?;
    Ok(lineage.trace_all(&pasted))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopyAsTextParams {
    /// The objects whose text to copy, by UUID.
    pub objects: Vec<Uuid>,
}

/// What `schematic.copy_as_text` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CopiedText {
    /// One line per object (more for multi-line text), in the order given.
    pub text: String,
}

pub struct CopyAsText;

impl Command for CopyAsText {
    const META: Meta = Meta {
        id: "schematic.copy_as_text",
        title: "Copy As Text",
        description: "Returns the text of the given Net Labels, Ports, Power Ports, free \
                      texts and Units (their Designators), one per line in the order given, \
                      for the system clipboard. Objects with no text are skipped. Changes \
                      nothing.",
        category: "schematic",
        icon: "copy",
        kind: Kind::Query,
    };
    type Params = CopyAsTextParams;
    type Output = CopiedText;

    fn run(&self, cx: &mut Context<'_>, p: CopyAsTextParams) -> Result<CopiedText, CommandError> {
        let project = cx.project()?;
        let objects = resolve(project, &p.objects, "/objects")?;
        let lines: Vec<&str> = objects
            .iter()
            .filter_map(|object| match object {
                Object::Unit(u) => Some(u.object.designator.as_str()),
                Object::NetLabel(l) => Some(l.object.name.as_str()),
                Object::Port(p) => Some(p.object.name.as_str()),
                Object::PowerPort(p) => Some(p.object.name.as_str()),
                Object::SheetText(t) => Some(t.object.text.as_str()),
                _ => None,
            })
            .collect();
        Ok(CopiedText {
            text: lines.join("\n"),
        })
    }
}
