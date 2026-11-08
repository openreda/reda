//! The Project value: its properties, its ordered Document list and the Documents it owns.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::change::{ChangeSet, Object};
use crate::document::{DocumentEntry, DocumentKind};
use crate::output::{OutputJob, ProjectFabPreset};
use crate::pcb::{PcbDocument, ZoneFills};
use crate::schematic::SchematicSheet;
use crate::{ElementSource, ErcSettings, LibraryElement, Scope, TxError, Uuid};

/// Project-wide properties, stored in `<name>.reda`.
///
/// The Document order lives here because it is a Project property: it drives sheet
/// numbering and PDF output, and reordering is an edit on the Project's own undo stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectProps {
    pub uuid: Uuid,
    pub name: String,
    /// Every Document's UUID, in Project order, [missing](ProjectProps::missing) ones
    /// included.
    pub documents: Vec<Uuid>,
    /// The Project Libraries: folders relative to the Project folder, `/`-separated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<String>,
    /// The Project properties that fill the sheets' title blocks.
    #[serde(default)]
    pub properties: ProjectProperties,
    /// The ERC severities and Pin Matrix.
    #[serde(default)]
    pub erc: ErcSettings,
    /// The schematic options.
    #[serde(default)]
    pub schematic: SchematicOptions,
    /// The last Re-annotate settings applied, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reannotate: Option<crate::annotate::ReannotateSettings>,
    /// The Update from Library settings page 1 saved, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_update: Option<crate::library_update::LibraryUpdateSettings>,
    /// The Documents the Project file lists whose files were missing when it was opened.
    /// They keep their place in `documents` but have no body: nothing can open or edit
    /// them until they are relinked to a file or removed from the Project.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<DocumentEntry>,
}

/// The Project's schematic options: how Nets are named and Ports cross-referenced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SchematicOptions {
    /// Power Port Names Take Priority: a Power Port's name beats a Net Label's, giving
    /// Power Port > Net Label > Port > auto-name. Off by default (Net Label > Power Port >
    /// Port > auto-name).
    pub power_port_names_priority: bool,
    /// Allow Ports to Name Nets. Off, a Port still joins its Net across sheets but never
    /// names it.
    pub allow_ports_to_name_nets: bool,
    /// Append Sheet Numbers to Local Nets: every Local Net named by a Net Label gets the
    /// board name `<name>_<n>`, `n` being its sheet's number. Sheets keep showing the plain
    /// name.
    pub append_sheet_numbers: bool,
    /// Automatic Cross References: each Port shows where its partners are, such as
    /// `[2:B3]`. Off by default, so a Project opts in.
    pub cross_references: bool,
    /// The Project's cross-reference styles, unless it follows the Preferences.
    pub cross_reference_styles: CrossReferenceStyles,
    /// Follow Cross References settings in Preferences: the styles come from the
    /// Preferences instead of [`SchematicOptions::cross_reference_styles`]. Off by default.
    pub cross_references_follow_preferences: bool,
}

impl SchematicOptions {
    pub const DEFAULT: SchematicOptions = SchematicOptions {
        power_port_names_priority: false,
        allow_ports_to_name_nets: true,
        append_sheet_numbers: false,
        cross_references: false,
        cross_reference_styles: CrossReferenceStyles::DEFAULT,
        cross_references_follow_preferences: false,
    };

    pub fn is_default(&self) -> bool {
        *self == SchematicOptions::DEFAULT
    }

    /// The styles Port cross-references are drawn in, given the Preferences' styles, or
    /// none when the Project shows no cross-references.
    pub fn cross_reference_styles(
        &self,
        preferences: CrossReferenceStyles,
    ) -> Option<CrossReferenceStyles> {
        self.cross_references
            .then_some(match self.cross_references_follow_preferences {
                true => preferences,
                false => self.cross_reference_styles,
            })
    }
}

/// How a Port cross-reference names its partners' places: by sheet, then by location on
/// that sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CrossReferenceStyles {
    /// Sheet Style: `number` by default.
    pub sheet: SheetStyle,
    /// Location Style: `zone` by default.
    pub location: LocationStyle,
}

impl CrossReferenceStyles {
    pub const DEFAULT: CrossReferenceStyles = CrossReferenceStyles {
        sheet: SheetStyle::Number,
        location: LocationStyle::Zone,
    };
}

impl Default for CrossReferenceStyles {
    fn default() -> CrossReferenceStyles {
        CrossReferenceStyles::DEFAULT
    }
}

/// How a cross-reference names a partner's sheet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SheetStyle {
    /// Not at all.
    None,
    /// By the sheet's title, as in the Projects panel.
    Name,
    /// By its number in the Projects panel, from 1.
    #[default]
    Number,
}

/// How a cross-reference names where on its sheet a partner is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocationStyle {
    /// Not at all.
    None,
    /// By its Border Zone, such as `B3`; nothing on a sheet with Border Zones off.
    #[default]
    Zone,
    /// By its X,Y position on the sheet, in millimetres.
    Xy,
}

impl Default for SchematicOptions {
    fn default() -> SchematicOptions {
        SchematicOptions::DEFAULT
    }
}

/// The Project properties: fixed free-text fields that fill each sheet's title block. The
/// board thickness and copper are the PCB Document's [`crate::Stackup`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectProperties {
    pub title: String,
    pub revision: String,
    /// Free text, such as `2026-11-01`.
    pub date: String,
    pub author: String,
    pub company: String,
}

impl ProjectProperties {
    pub fn is_empty(&self) -> bool {
        *self == ProjectProperties::default()
    }
}

/// One electronic design and everything that belongs to it.
///
/// Data changes only through [`Project::transaction`] or by applying a [`ChangeSet`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    props: ProjectProps,
    entries: BTreeMap<Uuid, DocumentEntry>,
    fab_presets: BTreeMap<Uuid, ProjectFabPreset>,
    output_jobs: BTreeMap<Uuid, OutputJob>,
    schematic_sheets: BTreeMap<Uuid, SchematicSheet>,
    /// The Library elements open for editing, each its own undo scope.
    elements: BTreeMap<Uuid, (ElementSource, LibraryElement)>,
    pcb_documents: BTreeMap<Uuid, PcbDocument>,
    /// Computed Zone fills per PCB Document: derived data kept beside the Documents, never
    /// part of them, so transactions and undo do not touch it.
    zone_fills: BTreeMap<Uuid, ZoneFills>,
    /// Data computed from Documents, forgotten when they change.
    derived: crate::derived::Derived,
    #[cfg(feature = "test-objects")]
    probes: BTreeMap<Uuid, crate::Probe>,
}

impl Project {
    /// A Project with no Documents. `props.documents` must be empty.
    pub fn new(props: ProjectProps) -> Project {
        assert!(
            props.documents.is_empty(),
            "Documents are added through a transaction"
        );
        Project {
            props,
            entries: BTreeMap::new(),
            fab_presets: BTreeMap::new(),
            output_jobs: BTreeMap::new(),
            schematic_sheets: BTreeMap::new(),
            elements: BTreeMap::new(),
            pcb_documents: BTreeMap::new(),
            zone_fills: BTreeMap::new(),
            derived: Default::default(),
            #[cfg(feature = "test-objects")]
            probes: BTreeMap::new(),
        }
    }

    /// The `T` computed from Document `document`, computed with `build` the first time it
    /// is asked for after the Document last changed (or its Zone fills were set). For
    /// derived data that is costly to build, such as a board's copper index; a crate
    /// stores its own type, so the key is the type with the Document.
    pub fn derived<T: std::any::Any + Send + Sync>(
        &self,
        document: Uuid,
        build: impl FnOnce() -> T,
    ) -> std::sync::Arc<T> {
        self.derived.get_or(document, build)
    }

    pub fn uuid(&self) -> Uuid {
        self.props.uuid
    }

    pub fn props(&self) -> &ProjectProps {
        &self.props
    }

    /// The open Documents in Project order; missing ones are left out (see
    /// [`Project::document_list`]).
    pub fn documents(&self) -> impl Iterator<Item = &DocumentEntry> {
        (self.props.documents.iter()).filter_map(|uuid| self.entries.get(uuid))
    }

    pub fn document(&self, uuid: Uuid) -> Option<&DocumentEntry> {
        self.entries.get(&uuid)
    }

    /// Every Document the Project file lists, in Project order, the
    /// [missing](ProjectProps::missing) ones included. Document numbers count this list.
    pub fn document_list(&self) -> Vec<crate::ListedDocument> {
        let row = |uuid: &Uuid| match self.entries.get(uuid) {
            Some(entry) => Some(crate::ListedDocument {
                entry: entry.clone(),
                missing: false,
            }),
            None => self
                .missing_document(*uuid)
                .map(|entry| crate::ListedDocument {
                    entry: entry.clone(),
                    missing: true,
                }),
        };
        self.props.documents.iter().filter_map(row).collect()
    }

    /// The missing Document with this UUID, if any.
    pub fn missing_document(&self, uuid: Uuid) -> Option<&DocumentEntry> {
        self.props.missing.iter().find(|e| e.uuid == uuid)
    }

    /// The Project's copies of Fab Presets, by UUID.
    pub fn fab_presets(&self) -> impl Iterator<Item = &ProjectFabPreset> {
        self.fab_presets.values()
    }

    pub fn fab_preset(&self, uuid: Uuid) -> Option<&ProjectFabPreset> {
        self.fab_presets.get(&uuid)
    }

    /// The Output Jobs, by UUID.
    pub fn output_jobs(&self) -> impl Iterator<Item = &OutputJob> {
        self.output_jobs.values()
    }

    pub fn output_job(&self, uuid: Uuid) -> Option<&OutputJob> {
        self.output_jobs.get(&uuid)
    }

    pub fn schematic_sheet(&self, uuid: Uuid) -> Option<&SchematicSheet> {
        self.schematic_sheets.get(&uuid)
    }

    /// The last computed Zone fills of PCB Document `document`, current or stale.
    pub fn zone_fills(&self, document: Uuid) -> Option<&ZoneFills> {
        self.zone_fills.get(&document)
    }

    /// Keeps `fills` as PCB Document `document`'s Zone fills (`None` forgets them). Fills
    /// are derived data: setting them is not an edit.
    pub fn set_zone_fills(&mut self, document: Uuid, fills: Option<ZoneFills>) {
        self.derived.forget(document);
        match fills {
            Some(fills) => self.zone_fills.insert(document, fills),
            None => self.zone_fills.remove(&document),
        };
    }

    /// Opens a Library element for editing, outside any transaction: opening is not an
    /// edit and is never undone. Its edits then [`crate::Transaction::replace`] it, on the
    /// undo scope `Scope::Element(uuid)`. Refused when its UUID is taken.
    pub fn open_element(
        &mut self,
        source: ElementSource,
        element: LibraryElement,
    ) -> Result<(), String> {
        let uuid = element.uuid();
        if self.object(uuid).is_some() {
            return Err(format!("an object with UUID {uuid} is already open"));
        }
        self.elements.insert(uuid, (source, element));
        Ok(())
    }

    /// Closes an open Library element and returns it, discarding nothing on disk.
    pub fn close_element(&mut self, uuid: Uuid) -> Option<LibraryElement> {
        self.elements.remove(&uuid).map(|(_, element)| element)
    }

    /// An open Library element.
    pub fn element(&self, uuid: Uuid) -> Option<&LibraryElement> {
        self.elements.get(&uuid).map(|(_, element)| element)
    }

    /// Where an open Library element came from.
    pub fn element_source(&self, uuid: Uuid) -> Option<ElementSource> {
        self.elements.get(&uuid).map(|(source, _)| *source)
    }

    /// Every open Library element, sorted by UUID.
    pub fn elements(&self) -> impl Iterator<Item = (ElementSource, &LibraryElement)> {
        self.elements
            .values()
            .map(|(source, element)| (*source, element))
    }

    pub fn pcb_document(&self, uuid: Uuid) -> Option<&PcbDocument> {
        self.pcb_documents.get(&uuid)
    }

    /// The Project's PCB Document, if it has one. A v0.1 Project has at most one.
    pub fn pcb(&self) -> Option<&PcbDocument> {
        self.documents()
            .find(|d| d.kind == DocumentKind::Pcb)
            .and_then(|d| self.pcb_documents.get(&d.uuid))
    }

    /// The object with this UUID, cloned, wherever it lives.
    pub fn object(&self, uuid: Uuid) -> Option<Object> {
        if uuid == self.props.uuid {
            return Some(Object::Project(self.props.clone()));
        }
        #[cfg(feature = "test-objects")]
        if let Some(probe) = self.probes.get(&uuid) {
            return Some(Object::Probe(probe.clone()));
        }
        if let Some(preset) = self.fab_presets.get(&uuid) {
            return Some(Object::FabPreset(preset.clone()));
        }
        if let Some(job) = self.output_jobs.get(&uuid) {
            return Some(Object::OutputJob(job.clone()));
        }
        if let Some(element) = self.element(uuid) {
            return Some(element.clone().into());
        }
        if let Some(object) = self.pcb_documents.values().find_map(|pcb| pcb.object(uuid)) {
            return Some(object);
        }
        if let Some(object) = self.schematic_sheets.values().find_map(|s| s.object(uuid)) {
            return Some(object);
        }
        self.entries.get(&uuid).cloned().map(Object::Document)
    }

    /// Checks that the Document an object names exists and can hold it.
    pub(crate) fn check_home(&self, object: &Object) -> Result<(), TxError> {
        let pcb = match object {
            Object::LayerSet(o) => o.document,
            Object::BoardSetup(o) => o.document,
            Object::Stackup(o) => o.document,
            Object::DesignRule(o) => o.document,
            Object::BoardOutline(o) => o.document,
            Object::BoardCutout(o) => o.document,
            Object::NetClass(o) => o.document,
            Object::Net(o) => o.document,
            Object::SheetFrame(o) => return self.check_sheet(o.document),
            Object::PartEntry(o) => return self.check_sheet(o.document),
            // Sheets and the PCB Document both have a part table.
            Object::FootprintCopy(o) if self.pcb_documents.contains_key(&o.document) => {
                return Ok(());
            }
            Object::FootprintCopy(o) => return self.check_sheet(o.document),
            Object::Unit(o) => return self.check_sheet(o.document),
            Object::Wire(o) => return self.check_sheet(o.document),
            Object::NetLabel(o) => return self.check_sheet(o.document),
            Object::NoConnect(o) => return self.check_sheet(o.document),
            Object::Port(o) => return self.check_sheet(o.document),
            Object::PowerPort(o) => return self.check_sheet(o.document),
            Object::SheetText(o) => return self.check_sheet(o.document),
            Object::SheetGraphic(o) => return self.check_sheet(o.document),
            // Sheets hold ERC Waivers, the PCB Document DRC ones.
            Object::Waiver(o) if self.pcb_documents.contains_key(&o.document) => {
                return Ok(());
            }
            Object::Waiver(o) => return self.check_sheet(o.document),
            Object::PcbText(o) => o.document,
            Object::PcbGraphic(o) => o.document,
            Object::FootprintInstance(o) => o.document,
            Object::Track(o) => o.document,
            Object::Via(o) => o.document,
            Object::Zone(o) => o.document,
            Object::Keepout(o) => o.document,
            Object::ZoneCutout(o) => o.document,
            Object::FreePad(o) => o.document,
            Object::OutputOrigin(o) => o.document,
            _ => return Ok(()),
        };
        match self.pcb_documents.contains_key(&pcb) {
            true => Ok(()),
            false => Err(TxError::Invalid(format!("{pcb} is not a PCB Document"))),
        }
    }

    fn check_sheet(&self, sheet: Uuid) -> Result<(), TxError> {
        match self.schematic_sheets.contains_key(&sheet) {
            true => Ok(()),
            false => Err(TxError::Invalid(format!(
                "{sheet} is not a Schematic Sheet"
            ))),
        }
    }

    /// Whether the body of Document `uuid` holds no objects.
    pub(crate) fn body_is_empty(&self, uuid: Uuid) -> bool {
        self.pcb_documents
            .get(&uuid)
            .is_none_or(PcbDocument::is_empty)
            && self
                .schematic_sheets
                .get(&uuid)
                .is_none_or(SchematicSheet::is_empty)
    }

    /// Every object, cloned, sorted by UUID. A new object kind adds its table here.
    pub fn objects(&self) -> Vec<Object> {
        let mut out: Vec<Object> = std::iter::once(Object::Project(self.props.clone()))
            .chain(self.entries.values().cloned().map(Object::Document))
            .chain(self.fab_presets.values().cloned().map(Object::FabPreset))
            .chain(self.output_jobs.values().cloned().map(Object::OutputJob))
            .chain(self.elements.values().map(|(_, e)| e.clone().into()))
            .chain(self.pcb_documents.values().flat_map(PcbDocument::objects))
            .chain(
                self.schematic_sheets
                    .values()
                    .flat_map(SchematicSheet::objects),
            )
            .collect();
        #[cfg(feature = "test-objects")]
        out.extend(self.probes.values().cloned().map(Object::Probe));
        out.sort_by_key(Object::uuid);
        out
    }

    /// The test-only Probe with this UUID.
    #[cfg(feature = "test-objects")]
    pub fn probe(&self, uuid: Uuid) -> Option<&crate::Probe> {
        self.probes.get(&uuid)
    }

    /// Applies every change's `after` state. Apply [`ChangeSet::inverted`] to undo.
    pub fn apply(&mut self, changes: &ChangeSet) {
        for (uuid, change) in changes.in_apply_order() {
            self.put(*uuid, change.after.clone());
        }
    }

    /// Sets the object stored under `uuid`, or removes it with `None`. No bookkeeping.
    pub(crate) fn put(&mut self, uuid: Uuid, object: Option<Object>) {
        match object.as_ref().map(Object::scope) {
            Some(Scope::Document(document) | Scope::Element(document)) => {
                self.derived.forget(document);
            }
            _ => self.derived.clear(),
        }
        match object {
            Some(Object::Project(props)) => self.props = props,
            Some(Object::Document(entry)) => {
                if !self.entries.contains_key(&uuid) {
                    self.create_body(&entry);
                }
                self.entries.insert(uuid, entry);
            }
            Some(Object::FabPreset(o)) => {
                self.fab_presets.insert(uuid, o);
            }
            Some(Object::OutputJob(o)) => {
                self.output_jobs.insert(uuid, o);
            }
            Some(object @ (Object::Symbol(_) | Object::Footprint(_) | Object::Part(_))) => {
                let element = object.into_element().expect("a Library element");
                let open = self
                    .elements
                    .get_mut(&uuid)
                    .expect("Library elements are opened, not inserted");
                open.1 = element;
            }
            Some(Object::LayerSet(o)) => {
                self.pcb_mut(o.document).layer_sets.insert(uuid, o.object);
            }
            Some(Object::BoardSetup(o)) => {
                self.pcb_mut(o.document).setups.insert(uuid, o.object);
            }
            Some(Object::Stackup(o)) => {
                self.pcb_mut(o.document).stackups.insert(uuid, o.object);
            }
            Some(Object::DesignRule(o)) => {
                self.pcb_mut(o.document).rules.insert(uuid, o.object);
            }
            Some(Object::BoardOutline(o)) => {
                self.pcb_mut(o.document).outlines.insert(uuid, o.object);
            }
            Some(Object::BoardCutout(o)) => {
                self.pcb_mut(o.document).cutouts.insert(uuid, o.object);
            }
            Some(Object::NetClass(o)) => {
                self.pcb_mut(o.document).net_classes.insert(uuid, o.object);
            }
            Some(Object::Net(o)) => {
                self.pcb_mut(o.document).nets.insert(uuid, o.object);
            }
            Some(Object::SheetFrame(o)) => {
                self.sheet_mut(o.document).frames.insert(uuid, o.object);
            }
            Some(Object::PartEntry(o)) => {
                self.sheet_mut(o.document).parts.insert(uuid, o.object);
            }
            Some(Object::FootprintCopy(o)) => match self.pcb_documents.get_mut(&o.document) {
                Some(pcb) => {
                    pcb.footprint_copies.insert(uuid, o.object);
                }
                None => {
                    self.sheet_mut(o.document).footprints.insert(uuid, o.object);
                }
            },
            Some(Object::Unit(o)) => {
                self.sheet_mut(o.document).units.insert(uuid, o.object);
            }
            Some(Object::Wire(o)) => {
                self.sheet_mut(o.document).wires.insert(uuid, o.object);
            }
            Some(Object::NetLabel(o)) => {
                self.sheet_mut(o.document).net_labels.insert(uuid, o.object);
            }
            Some(Object::Port(o)) => {
                self.sheet_mut(o.document).ports.insert(uuid, o.object);
            }
            Some(Object::PowerPort(o)) => {
                self.sheet_mut(o.document)
                    .power_ports
                    .insert(uuid, o.object);
            }
            Some(Object::NoConnect(o)) => {
                self.sheet_mut(o.document)
                    .no_connects
                    .insert(uuid, o.object);
            }
            Some(Object::SheetText(o)) => {
                self.sheet_mut(o.document).texts.insert(uuid, o.object);
            }
            Some(Object::SheetGraphic(o)) => {
                self.sheet_mut(o.document).graphics.insert(uuid, o.object);
            }
            Some(Object::Waiver(o)) => match self.pcb_documents.get_mut(&o.document) {
                Some(pcb) => {
                    pcb.waivers.insert(uuid, o.object);
                }
                None => {
                    self.sheet_mut(o.document).waivers.insert(uuid, o.object);
                }
            },
            Some(Object::PcbText(o)) => {
                self.pcb_mut(o.document).texts.insert(uuid, o.object);
            }
            Some(Object::PcbGraphic(o)) => {
                self.pcb_mut(o.document).graphics.insert(uuid, o.object);
            }
            Some(Object::FootprintInstance(o)) => {
                self.pcb_mut(o.document).footprints.insert(uuid, o.object);
            }
            Some(Object::Track(o)) => {
                self.pcb_mut(o.document).tracks.insert(uuid, o.object);
            }
            Some(Object::Via(o)) => {
                self.pcb_mut(o.document).vias.insert(uuid, o.object);
            }
            Some(Object::Zone(o)) => {
                self.pcb_mut(o.document).zones.insert(uuid, o.object);
            }
            Some(Object::Keepout(o)) => {
                self.pcb_mut(o.document).keepouts.insert(uuid, o.object);
            }
            Some(Object::ZoneCutout(o)) => {
                self.pcb_mut(o.document).zone_cutouts.insert(uuid, o.object);
            }
            Some(Object::FreePad(o)) => {
                self.pcb_mut(o.document).free_pads.insert(uuid, o.object);
            }
            Some(Object::OutputOrigin(o)) => {
                self.pcb_mut(o.document)
                    .output_origins
                    .insert(uuid, o.object);
            }
            #[cfg(feature = "test-objects")]
            Some(Object::Probe(probe)) => {
                self.probes.insert(uuid, probe);
            }
            None => {
                for pcb in self.pcb_documents.values_mut() {
                    pcb.remove(uuid);
                }
                self.fab_presets.remove(&uuid);
                self.output_jobs.remove(&uuid);
                for sheet in self.schematic_sheets.values_mut() {
                    sheet.remove(uuid);
                }
                assert_ne!(uuid, self.props.uuid, "the Project itself is never removed");
                if self.entries.remove(&uuid).is_some() {
                    self.schematic_sheets.remove(&uuid);
                    self.pcb_documents.remove(&uuid);
                    self.zone_fills.remove(&uuid);
                }
                #[cfg(feature = "test-objects")]
                self.probes.remove(&uuid);
            }
        }
    }

    fn pcb_mut(&mut self, document: Uuid) -> &mut PcbDocument {
        self.pcb_documents
            .get_mut(&document)
            .expect("the transaction checks an object's Document exists")
    }

    fn sheet_mut(&mut self, sheet: Uuid) -> &mut SchematicSheet {
        self.schematic_sheets
            .get_mut(&sheet)
            .expect("the transaction checks an object's Document exists")
    }

    fn create_body(&mut self, entry: &DocumentEntry) {
        match entry.kind {
            DocumentKind::Pcb => {
                self.pcb_documents
                    .insert(entry.uuid, PcbDocument::new(entry.uuid));
            }
            DocumentKind::Schematic => {
                self.schematic_sheets
                    .insert(entry.uuid, SchematicSheet::new(entry.uuid));
            }
        }
    }

    /// Checks the invariants a committed transaction must leave behind.
    pub(crate) fn check(&self) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        for missing in &self.props.missing {
            if self.entries.contains_key(&missing.uuid) {
                return Err(format!(
                    "Document {} is open and missing at once",
                    missing.uuid
                ));
            }
        }
        for uuid in &self.props.documents {
            if !self.entries.contains_key(uuid) && self.missing_document(*uuid).is_none() {
                return Err(format!("the Document order names unknown Document {uuid}"));
            }
            if !seen.insert(*uuid) {
                return Err(format!("Document {uuid} appears twice in the order"));
            }
        }
        let mut listed = (self.entries.keys()).chain(self.props.missing.iter().map(|m| &m.uuid));
        if let Some(orphan) = listed.find(|uuid| !seen.contains(uuid)) {
            return Err(format!(
                "Document {orphan} is missing from the Document order"
            ));
        }
        for job in self.output_jobs.values() {
            if job.name.trim().is_empty() {
                return Err(format!("Output Job {} has no name", job.uuid));
            }
            if !self.fab_presets.contains_key(&job.fab_preset) {
                return Err(format!(
                    "Output Job {:?} names Fab Preset {}, which the Project has no copy of",
                    job.name, job.fab_preset
                ));
            }
        }
        let missing_pcbs = (self.props.missing.iter())
            .filter(|m| m.kind == DocumentKind::Pcb)
            .count();
        if self.pcb_documents.len() + missing_pcbs > 1 {
            return Err("a Project has at most one PCB Document".into());
        }
        self.pcb_documents
            .values()
            .try_for_each(PcbDocument::check)?;
        self.schematic_sheets
            .values()
            .try_for_each(SchematicSheet::check)
    }
}
