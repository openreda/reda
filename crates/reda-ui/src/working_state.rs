//! Working state: per Document, the Selection Filter and the object snap options
//! (mode and Gridless), changed by `selection.filter.set` and `view.snap.set`. With the
//! grid ([`crate::grid`]) and the board's units ([`crate::pcb::BoardView`]) it is kept in
//! `.reda/view.json`, never in the Document, so changing it never dirties one. The
//! Preferences hold only the defaults.
//!
//! The Filter has one toggle per kind of object, with the kind list set per Document type.
//! The kinds are named as `objects.find_similar` names them (`type`). The Part editor has
//! no Filter.

use std::collections::{BTreeMap, BTreeSet};

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::{LibraryElement, ObjectType, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::bars::DocumentType;

/// Adds `selection.filter.set` and `view.snap.set` to `registry`. The Shell does this.
pub fn register_all(registry: &mut Registry) {
    registry.add(FilterSet);
    registry.add(SnapSet);
}

/// One kind of object a Selection Filter toggles, named as `objects.find_similar` names
/// it (`type`) on the wire.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FilterKind {
    Unit,
    FieldText,
    Wire,
    NetLabel,
    Port,
    PowerPort,
    NoConnect,
    SheetText,
    SheetGraphic,
    FootprintInstance,
    Pad,
    Track,
    Via,
    Zone,
    Keepout,
    FreePad,
    PcbText,
    FootprintText,
    PcbGraphic,
    BoardOutline,
    BoardCutout,
    OutputOrigin,
    /// A Symbol's Pin.
    Pin,
    /// A Symbol's or Footprint's drawing.
    Graphic,
    /// A Footprint's own text.
    Text,
}

impl FilterKind {
    /// The kind's name, as Commands and `.reda/view.json` spell it.
    pub fn name(self) -> &'static str {
        use FilterKind::*;
        match self {
            Unit => "unit",
            FieldText => "field_text",
            Wire => "wire",
            NetLabel => "net_label",
            Port => "port",
            PowerPort => "power_port",
            NoConnect => "no_connect",
            SheetText => "sheet_text",
            SheetGraphic => "sheet_graphic",
            FootprintInstance => "footprint_instance",
            Pad => "pad",
            Track => "track",
            Via => "via",
            Zone => "zone",
            Keepout => "keepout",
            FreePad => "free_pad",
            PcbText => "pcb_text",
            FootprintText => "footprint_text",
            PcbGraphic => "pcb_graphic",
            BoardOutline => "board_outline",
            BoardCutout => "board_cutout",
            OutputOrigin => "output_origin",
            Pin => "pin",
            Graphic => "graphic",
            Text => "text",
        }
    }

    /// The kind named `name`, if any Document type filters one by that name.
    pub fn parse(name: &str) -> Option<FilterKind> {
        [SCHEMATIC_KINDS, PCB_KINDS, SYMBOL_KINDS, FOOTPRINT_KINDS]
            .into_iter()
            .flatten()
            .copied()
            .find(|k| k.name() == name)
    }

    /// The kind of a persisted object of type `ty`; the Project's own records have none.
    pub fn of_object(ty: ObjectType) -> Option<FilterKind> {
        use FilterKind as K;
        Some(match ty {
            ObjectType::Unit => K::Unit,
            ObjectType::Wire => K::Wire,
            ObjectType::NetLabel => K::NetLabel,
            ObjectType::Port => K::Port,
            ObjectType::PowerPort => K::PowerPort,
            ObjectType::NoConnect => K::NoConnect,
            ObjectType::SheetText => K::SheetText,
            ObjectType::SheetGraphic => K::SheetGraphic,
            ObjectType::FootprintInstance => K::FootprintInstance,
            ObjectType::Track => K::Track,
            ObjectType::Via => K::Via,
            ObjectType::Zone => K::Zone,
            ObjectType::Keepout => K::Keepout,
            ObjectType::FreePad => K::FreePad,
            ObjectType::PcbText => K::PcbText,
            ObjectType::PcbGraphic => K::PcbGraphic,
            ObjectType::BoardOutline => K::BoardOutline,
            ObjectType::BoardCutout => K::BoardCutout,
            ObjectType::OutputOrigin => K::OutputOrigin,
            _ => return None,
        })
    }
}

/// The kinds a sheet's Filter lists, in its order.
pub const SCHEMATIC_KINDS: &[FilterKind] = &[
    FilterKind::Unit,
    FilterKind::FieldText,
    FilterKind::Wire,
    FilterKind::NetLabel,
    FilterKind::Port,
    FilterKind::PowerPort,
    FilterKind::NoConnect,
    FilterKind::SheetText,
    FilterKind::SheetGraphic,
];

/// The kinds a board's Filter lists, in its order.
pub const PCB_KINDS: &[FilterKind] = &[
    FilterKind::FootprintInstance,
    FilterKind::Pad,
    FilterKind::Track,
    FilterKind::Via,
    FilterKind::Zone,
    FilterKind::Keepout,
    FilterKind::FreePad,
    FilterKind::PcbText,
    FilterKind::FootprintText,
    FilterKind::PcbGraphic,
    FilterKind::BoardOutline,
    FilterKind::BoardCutout,
    FilterKind::OutputOrigin,
];

/// The kinds the Symbol editor's Filter lists.
pub const SYMBOL_KINDS: &[FilterKind] = &[FilterKind::Pin, FilterKind::Graphic];

/// The kinds the Footprint editor's Filter lists.
pub const FOOTPRINT_KINDS: &[FilterKind] =
    &[FilterKind::Pad, FilterKind::Graphic, FilterKind::Text];

/// The Filter's kinds for a Document type.
pub fn filter_kinds(ty: DocumentType) -> &'static [FilterKind] {
    match ty {
        DocumentType::Schematic => SCHEMATIC_KINDS,
        DocumentType::Pcb => PCB_KINDS,
        DocumentType::Symbol => SYMBOL_KINDS,
        DocumentType::Footprint => FOOTPRINT_KINDS,
    }
}

/// One Document's Selection Filter: the kinds turned off. Everything starts on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionFilter {
    pub off: BTreeSet<FilterKind>,
}

impl SelectionFilter {
    /// Whether objects of `kind` can be picked.
    pub fn allows(&self, kind: FilterKind) -> bool {
        !self.off.contains(&kind)
    }
}

/// How the cursor snaps to objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SnapMode {
    /// Boards and Footprints: objects on the active Layer, with through-hole Pads and
    /// Vias on every copper Layer.
    ActiveLayer,
    /// Boards and Footprints: objects on every Layer.
    AllLayers,
    /// Sheets and Symbols: snap to objects.
    On,
    /// Don't snap to objects.
    Off,
}

impl SnapMode {
    /// The modes a Document type offers; the first is its default.
    pub fn offered(ty: DocumentType) -> &'static [SnapMode] {
        match ty {
            DocumentType::Pcb | DocumentType::Footprint => {
                &[SnapMode::ActiveLayer, SnapMode::AllLayers, SnapMode::Off]
            }
            DocumentType::Schematic | DocumentType::Symbol => &[SnapMode::On, SnapMode::Off],
        }
    }
}

/// One Document's snap options.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SnapOptions {
    pub mode: SnapMode,
    /// The grid level is off; Hotspots still snap.
    pub gridless: bool,
}

impl SnapOptions {
    /// What a Document of type `ty` starts with.
    pub fn default_for(ty: DocumentType) -> SnapOptions {
        SnapOptions {
            mode: SnapMode::offered(ty)[0],
            gridless: false,
        }
    }
}

/// Every Document's working state beside the grid and units, in the engine's view state.
/// Documents left out have the defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkingState {
    pub filters: BTreeMap<Uuid, SelectionFilter>,
    pub snap: BTreeMap<Uuid, SnapOptions>,
}

impl WorkingState {
    pub fn filter(&self, document: Uuid) -> SelectionFilter {
        self.filters.get(&document).cloned().unwrap_or_default()
    }

    /// `document`'s snap options; `ty` gives the defaults.
    pub fn snap(&self, document: Uuid, ty: DocumentType) -> SnapOptions {
        self.snap
            .get(&document)
            .copied()
            .unwrap_or_else(|| SnapOptions::default_for(ty))
    }
}

/// The type of the Document or open Library element `document`; a Part has none.
fn document_type(cx: &mut Context<'_>, document: Uuid) -> Result<DocumentType, CommandError> {
    let missing = || CommandError::not_found(format!("no Document {document}"));
    let project = cx.project().map_err(|_| missing())?;
    if let Some(entry) = project.document(document) {
        return Ok(entry.kind.into());
    }
    match project.element(document) {
        Some(LibraryElement::Symbol(_)) => Ok(DocumentType::Symbol),
        Some(LibraryElement::Footprint(_)) => Ok(DocumentType::Footprint),
        Some(LibraryElement::Part(_)) => Err(CommandError::rejected(
            "the Part editor has no Selection Filter or snapping",
        )),
        None => Err(missing()),
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FilterSetParams {
    /// The Document, or open Library element, whose Filter to change.
    document: Uuid,
    /// `true` turns every kind on (All), `false` every kind off (None), before `kinds`
    /// applies.
    #[serde(default)]
    all: Option<bool>,
    /// Kinds to turn on (`true`) or off (`false`), by name, such as `wire` or `track`.
    #[serde(default)]
    kinds: BTreeMap<String, bool>,
}

/// One kind's toggle.
#[derive(Serialize, JsonSchema)]
pub struct FilterToggle {
    kind: FilterKind,
    /// Objects of this kind can be picked.
    on: bool,
}

/// The Filter now in force.
#[derive(Serialize, JsonSchema)]
pub struct FilterResult {
    /// Every kind of the Document type, in the Filter's order.
    kinds: Vec<FilterToggle>,
}

struct FilterSet;

impl Command for FilterSet {
    const META: Meta = Meta {
        id: "selection.filter.set",
        title: "Set Selection Filter",
        description: "Turns kinds of object on or off in a Document's Selection Filter, or \
                      all of them with `all`. Filtered kinds stay visible but can't be \
                      clicked, boxed or found with Select Similar; `selection.set` ignores \
                      the Filter. With no changes it returns the Filter as it is.",
        category: "selection",
        icon: "select",
        kind: Kind::View,
    };
    type Params = FilterSetParams;
    type Output = FilterResult;

    fn run(&self, cx: &mut Context<'_>, p: FilterSetParams) -> Result<FilterResult, CommandError> {
        let ty = document_type(cx, p.document)?;
        let list = filter_kinds(ty);
        let mut changes = Vec::new();
        for (name, on) in &p.kinds {
            match FilterKind::parse(name).filter(|k| list.contains(k)) {
                Some(kind) => changes.push((kind, *on)),
                None => {
                    let names: Vec<_> = list.iter().map(|k| k.name()).collect();
                    return Err(CommandError::invalid_params(
                        &format!("/kinds/{}", name.replace('~', "~0").replace('/', "~1")),
                        format!(
                            "not a kind this Document type filters; pick one of {}",
                            names.join(", ")
                        ),
                    ));
                }
            }
        }
        let state = cx.view::<WorkingState>()?;
        let filter = state.filters.entry(p.document).or_default();
        match p.all {
            Some(true) => filter.off.clear(),
            Some(false) => filter.off = list.iter().copied().collect(),
            None => {}
        }
        for (kind, on) in changes {
            match on {
                true => filter.off.remove(&kind),
                false => filter.off.insert(kind),
            };
        }
        let kinds = list
            .iter()
            .map(|&kind| FilterToggle {
                kind,
                on: filter.allows(kind),
            })
            .collect();
        if filter.off.is_empty() {
            state.filters.remove(&p.document);
        }
        Ok(FilterResult { kinds })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnapSetParams {
    /// The Document, or open Library element, whose snapping to change.
    document: Uuid,
    /// The object snap mode: `active_layer`, `all_layers` or `off` on a board or
    /// Footprint, `on` or `off` on a sheet or Symbol.
    #[serde(default)]
    mode: Option<SnapMode>,
    /// Turn the grid level off; Hotspots keep snapping.
    #[serde(default)]
    gridless: Option<bool>,
}

struct SnapSet;

impl Command for SnapSet {
    const META: Meta = Meta {
        id: "view.snap.set",
        title: "Set Snap Options",
        description: "Sets a Document's object snap mode (Active Layer, All Layers or Off on \
                      a board or Footprint; On or Off on a sheet or Symbol) and Gridless. \
                      With no changes it returns them as they are.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = SnapSetParams;
    type Output = SnapOptions;

    fn run(&self, cx: &mut Context<'_>, p: SnapSetParams) -> Result<SnapOptions, CommandError> {
        let ty = document_type(cx, p.document)?;
        let offered = SnapMode::offered(ty);
        if let Some(mode) = p.mode
            && !offered.contains(&mode)
        {
            let names: Vec<String> = offered
                .iter()
                .map(|m| {
                    serde_json::to_value(m)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default()
                })
                .collect();
            return Err(CommandError::invalid_params(
                "/mode",
                format!("this Document type offers {}", names.join(", ")),
            ));
        }
        let state = cx.view::<WorkingState>()?;
        let mut options = state.snap(p.document, ty);
        if let Some(mode) = p.mode {
            options.mode = mode;
        }
        if let Some(gridless) = p.gridless {
            options.gridless = gridless;
        }
        match options == SnapOptions::default_for(ty) {
            true => state.snap.remove(&p.document),
            false => state.snap.insert(p.document, options),
        };
        Ok(options)
    }
}
