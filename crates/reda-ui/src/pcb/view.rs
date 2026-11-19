//! The board's view state: the Layer state (kept per Document in `view.json`) and the units
//! lengths show in (the snap grid is per Document, in [`crate::grid`]). The PCB view Commands
//! change it in the engine's view state; the Shell hands it to the PCB tab.

use std::collections::HashMap;
use std::sync::Arc;

use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{
    Layer, LayerKind, LayerSet, ObjectType, PcbDocument, Project, Side, Uuid,
};
use reda_engine::preferences::{InactiveLayerMode, Preferences, Units};
use reda_engine::render::{
    self, BoardColor, BoardStyle, ColorKey, Composite, InactiveLayers, Item, NetColorPattern,
    NetColorStyle, NetColorZoomOut, NetLabels, Primitive, Rgba, Scene, SceneLayer,
    board_composites,
};

use crate::canvas::PaletteHandle;
use crate::view_state::{DocumentView, LayerView};

/// The kinds of board object the Layers panel can hide, in its order.
pub const VIEW_TYPES: [ObjectType; 9] = [
    ObjectType::FootprintInstance,
    ObjectType::Track,
    ObjectType::Via,
    ObjectType::Zone,
    ObjectType::Keepout,
    ObjectType::ZoneCutout,
    ObjectType::FreePad,
    ObjectType::PcbText,
    ObjectType::PcbGraphic,
];

/// The PCB view: the Layer state and units.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardView {
    pub(crate) document: Option<Uuid>,
    pub layers: LayerView,
    /// Session selection for standalone Via placement.
    pub stitching_type: Option<Uuid>,
    /// The units lengths show in.
    pub units: Units,
    /// Consumed by the focused board canvas when it opens the Layer list.
    pub(crate) pick_requested: bool,
    /// Temporary Layer inspection while the pointer rests on a chip.
    pub(crate) hover_solo: Option<Uuid>,
}

impl Default for BoardView {
    fn default() -> BoardView {
        BoardView::new(Units::Mm)
    }
}

impl BoardView {
    /// A fresh view in `units`.
    pub fn new(units: Units) -> BoardView {
        BoardView {
            document: None,
            layers: LayerView::default(),
            stitching_type: None,
            units,
            pick_requested: false,
            hover_solo: None,
        }
    }

    /// Restores this Document's view, with the opposite outer copper as its
    /// first Previous Layer for a fresh board or an older view file.
    pub(crate) fn for_document(
        pcb: &PcbDocument,
        saved: Option<&DocumentView>,
        default_units: Units,
    ) -> Self {
        let mut view = Self::new(saved.and_then(|v| v.units).unwrap_or(default_units));
        view.document = Some(pcb.uuid);
        if let Some(saved) = saved {
            view.layers = saved.layers.clone();
        }
        let set = pcb.layers();
        // Older view files lack a physical order. Preserve every reference that
        // still exists and fall back to top copper when the old position is unknown.
        if view.layers.copper_order.is_empty() {
            view.layers.copper_order = pcb.copper_layers().map(|l| l.uuid).collect();
            let exists = |id: &Uuid| set.layer(*id).is_some();
            view.layers.pinned.retain(exists);
            view.layers.hidden.retain(exists);
            view.layers.active = view.layers.active.filter(exists);
            view.layers.last_copper = view.layers.last_copper.filter(exists);
        }
        if view
            .layers
            .previous
            .and_then(|id| set.layer(id))
            .is_none_or(|l| l.kind != LayerKind::Copper)
        {
            view.layers.previous = view.opposite_outer(set);
        }
        view
    }

    pub(crate) fn opposite_outer(&self, set: &LayerSet) -> Option<Uuid> {
        let active = self.layers.active.and_then(|id| set.layer(id));
        let opposite = match active.and_then(|l| l.side) {
            Some(Side::Bottom) => Side::Top,
            _ => Side::Bottom,
        };
        set.find(LayerKind::Copper, Some(opposite)).map(|l| l.uuid)
    }

    /// Repair references while the previous physical order still identifies the copper
    /// above a removed active Layer. Undo restores document data only.
    pub(crate) fn update_copper_order(
        &mut self,
        current: Vec<Uuid>,
        opposite_outer: Option<Uuid>,
        sets: &mut [reda_engine::preferences::SavedLayerSet],
    ) {
        let old = std::mem::replace(&mut self.layers.copper_order, current);
        let current = &self.layers.copper_order;
        let gone = |id: &Uuid| old.contains(id) && !current.contains(id);
        if !old.iter().any(gone) {
            return;
        }
        if let Some(active) = self.layers.active.filter(gone) {
            let fallback = old
                .iter()
                .take_while(|id| **id != active)
                .filter(|id| current.contains(id))
                .last()
                .copied()
                .unwrap_or(current[0]);
            self.layers.active = Some(fallback);
            self.layers.hidden.remove(&fallback);
        }
        if self.layers.previous.is_some_and(|id| gone(&id)) {
            self.layers.previous = opposite_outer;
        }
        self.layers.last_copper = self.layers.last_copper.filter(|id| !gone(id));
        self.layers.highlighted = self.layers.highlighted.filter(|id| !gone(id));
        self.hover_solo = self.hover_solo.filter(|id| !gone(id));
        self.layers.hidden.retain(|id| !gone(id));
        self.layers.pinned.retain(|id| !gone(id));
        // Saved sets use portable inner positions. Translate only positions on this
        // board, leaving entries for larger boards and legacy kind/sided keys intact.
        for set in sets {
            set.layers.retain_mut(|key| {
                let Some(position) = key
                    .strip_prefix("inner_")
                    .and_then(|n| n.parse::<usize>().ok())
                else {
                    return true;
                };
                if position == 0 || position >= old.len() - 1 {
                    return true;
                }
                let Some(now) = current.iter().position(|id| *id == old[position]) else {
                    return false;
                };
                *key = format!("inner_{now}");
                true
            });
        }
    }

    pub fn stitching_type<'p>(
        &self,
        pcb: &'p reda_engine::model::PcbDocument,
    ) -> &'p reda_engine::model::ViaType {
        let stack = pcb.stackup();
        stack
            .via_types
            .iter()
            .find(|v| Some(v.uuid) == self.stitching_type)
            .unwrap_or_else(|| stack.through_type())
    }

    /// The Layer edited: the chosen one while the board has it, else the top copper.
    pub fn active_layer(&self, project: &Project) -> Option<Uuid> {
        let set = project.pcb()?.layers();
        self.layers
            .active
            .filter(|l| set.layer(*l).is_some())
            .or_else(|| set.find(LayerKind::Copper, Some(Side::Top)).map(|l| l.uuid))
    }

    pub(crate) fn activate(&mut self, set: &LayerSet, target: Uuid) {
        let copper = |id| {
            set.layer(id)
                .filter(|l| l.kind == LayerKind::Copper)
                .map(|l| l.uuid)
        };
        let old = self
            .layers
            .active
            .and_then(copper)
            .or_else(|| self.layers.last_copper.and_then(copper))
            .or_else(|| set.find(LayerKind::Copper, Some(Side::Top)).map(|l| l.uuid));
        if copper(target).is_some() {
            if old != Some(target) {
                self.layers.previous = old;
            }
            self.layers.last_copper = None;
        } else {
            self.layers.last_copper = old;
        }
        self.layers.active = Some(target);
        self.layers.hidden.remove(&target);
    }

    /// The Layers to composite for `pcb`, bottom first; `default_mode` is the
    /// Preference used while the view has no Inactive-Layer mode of its own.
    pub fn composites(&self, pcb: &PcbDocument, default_mode: InactiveLayerMode) -> Vec<Composite> {
        let set = pcb.layers();
        let active = self
            .layers
            .active
            .filter(|l| set.layer(*l).is_some())
            .or_else(|| set.find(LayerKind::Copper, Some(Side::Top)).map(|l| l.uuid));
        let solo = self.hover_solo;
        let view = render::LayerView {
            visible: Some(
                set.layers
                    .iter()
                    .map(|l| l.uuid)
                    .filter(|l| solo == Some(*l) || !self.layers.hidden.contains(l))
                    .collect(),
            ),
            active: solo.or(active),
            inactive: if solo.is_some() {
                InactiveLayers::Hidden
            } else {
                inactive_layers(self.layers.inactive_mode.unwrap_or(default_mode))
            },
            from_bottom: self.layers.from_bottom,
            ratsnest: !self.layers.ratsnest.hidden,
            pinned: self.layers.pinned.iter().copied().collect(),
        };
        let mut out = board_composites(&set.layers, &view);
        if solo.is_none()
            && let Some(lit) = self.layers.highlighted
        {
            highlight_layer(&mut out, lit);
        }
        out
    }

    /// How the board is drawn in this view with `prefs`: Smart Track Ends, `F5` Net
    /// colours, the label toggles and per-Footprint Ratsnest hiding.
    pub fn style(&self, prefs: &Preferences) -> BoardStyle {
        use reda_engine::preferences::{NetColorPattern as P, NetColorZoomOut as Z};
        let view = &prefs.pcb.view;
        let net_colors = self.layers.net_colors.then_some(NetColorStyle {
            pattern: match view.net_color_pattern {
                P::Solid => NetColorPattern::Solid,
                P::Checker => NetColorPattern::Checker,
            },
            zoom_out: match view.net_color_zoom_out {
                Z::NetColor => NetColorZoomOut::NetColor,
                Z::LayerColor => NetColorZoomOut::LayerColor,
            },
        });
        let labels = self.layers.labels;
        BoardStyle {
            smart_track_ends: prefs.pcb.ratsnest.smart_track_ends,
            net_colors,
            labels: NetLabels {
                pad_nets: labels.pad_nets,
                via_nets: labels.via_nets,
                via_spans: labels.via_spans,
                repeated_net_names: labels.repeated_net_names,
            },
            hidden_ratsnest_footprints: self.layers.ratsnest.hidden_footprints.clone(),
        }
    }

    /// Drops from `scene` what this view hides (objects of hidden kinds and the Ratsnest
    /// lines of hidden Nets), and draws each kind at its transparency, in outline where
    /// it is in Draft.
    pub fn filter_scene(&self, project: &Project, scene: &mut Scene) {
        let types = &self.layers.hidden_types;
        let nets = &self.layers.ratsnest.hidden_nets;
        if !self.layers.kinds.is_empty() {
            self.display_kinds(project, scene);
        }
        if types.is_empty() && nets.is_empty() {
            return;
        }
        let mut kinds: HashMap<Uuid, Option<ObjectType>> = HashMap::new();
        scene.items.retain(|item| {
            if item.layer == SceneLayer::Ratsnest {
                return !nets.contains(&item.object);
            }
            if types.is_empty() {
                return true;
            }
            let kind = *kinds
                .entry(item.object)
                .or_insert_with(|| project.object(item.object).map(|o| o.object_type()));
            kind.is_none_or(|k| !types.contains(&k))
        });
    }
}

impl BoardView {
    /// Applies each kind's transparency and Draft to its items.
    fn display_kinds(&self, project: &Project, scene: &mut Scene) {
        let mut kinds: HashMap<Uuid, Option<ObjectType>> = HashMap::new();
        let items = std::mem::take(&mut scene.items);
        for mut item in items {
            let kind = *kinds
                .entry(item.object)
                .or_insert_with(|| project.object(item.object).map(|o| o.object_type()));
            let display = kind.and_then(|k| self.layers.kinds.get(&k)).copied();
            let Some(display) = display.filter(|_| matches!(item.layer, SceneLayer::Board(_)))
            else {
                scene.items.push(item);
                continue;
            };
            item.look.transparency = display.transparency.min(100);
            if display.draft {
                for primitive in draft(&item.primitive) {
                    scene.items.push(Item {
                        primitive,
                        ..item.clone()
                    });
                }
            } else {
                scene.items.push(item);
            }
        }
    }
}

/// Holes and their labels are composited above the board. Apply the strength of
/// the strongest visible copper in their physical span to that overlay too.
pub(crate) fn filter_drills(pcb: &PcbDocument, scene: &mut Scene, layers: &[Composite]) {
    use reda_engine::model::PadKind;
    use reda_engine::render::Child;
    let opacity: HashMap<_, _> = layers
        .iter()
        .filter_map(|c| match c.layer {
            SceneLayer::Board(id) => Some((id, c.opacity)),
            _ => None,
        })
        .collect();
    let through = pcb
        .copper_layers()
        .filter_map(|l| opacity.get(&l.uuid))
        .copied()
        .fold(0.0_f32, f32::max);
    let mut spans = HashMap::new();
    scene.items.retain_mut(|item| {
        if item.layer != SceneLayer::Drill {
            return true;
        }
        let strength = if let Some(via) = pcb.via(item.object) {
            *spans
                .entry((via.span.from, via.span.to))
                .or_insert_with(|| {
                    pcb.span_layers(via.span)
                        .iter()
                        .filter_map(|id| opacity.get(id))
                        .copied()
                        .fold(0.0_f32, f32::max)
                })
        } else {
            let kind = match &item.child {
                Some(Child::Pad { footprint, pad }) => pcb
                    .footprint(*footprint)
                    .and_then(|fp| pcb.footprint_copy(fp.footprint))
                    .and_then(|copy| copy.footprint.pads.iter().find(|p| p.uuid == *pad))
                    .map(|p| p.kind),
                _ => pcb.free_pad(item.object).map(|p| p.kind),
            };
            if kind != Some(PadKind::PlatedThroughHole) {
                return true;
            }
            through
        };
        item.look.transparency =
            100 - ((100 - item.look.transparency) as f32 * strength).round() as u8;
        strength > 0.0
    });
}

/// How strong the other Layers stay while one Layer's content is highlighted.
pub const HIGHLIGHT_REST_OPACITY: f32 = 0.15;

/// Draws Layer `lit` on top of the other board Layers at full strength and fades them,
/// when it is drawn at all.
fn highlight_layer(composites: &mut Vec<Composite>, lit: Uuid) {
    let is_board = |c: &Composite| matches!(c.layer, SceneLayer::Board(_));
    let Some(i) = composites
        .iter()
        .position(|c| c.layer == SceneLayer::Board(lit))
    else {
        return;
    };
    let mut top = composites.remove(i);
    top.opacity = 1.0;
    for c in composites.iter_mut().filter(|c| is_board(c)) {
        c.opacity = c.opacity.min(HIGHLIGHT_REST_OPACITY);
    }
    let end = composites.iter().rposition(is_board).map_or(0, |e| e + 1);
    composites.insert(end, top);
}

/// The width of a Draft outline: 0.05 mm.
pub const DRAFT_WIDTH: Nm = Nm(50_000);

/// `primitive` in outline only (Draft): a filled shape becomes its edge, a stroke its
/// outline, a triangle list its boundary edges. Lines and text stay.
pub fn draft(primitive: &Primitive) -> Vec<Primitive> {
    let half = DRAFT_WIDTH.0 / 2;
    match primitive {
        Primitive::Capsule { a, b, width } => {
            let (dx, dy) = ((b.x - a.x).0 as f64, (b.y - a.y).0 as f64);
            let length = dx.hypot(dy).round() as i64;
            let angle = (dy.atan2(dx).to_degrees() * 1000.0).round() as i64;
            vec![Primitive::Rect {
                center: Point::new(Nm((a.x.0 + b.x.0) / 2), Nm((a.y.0 + b.y.0) / 2)),
                size: (Nm(length + width.0), *width),
                rotation: Millideg(angle),
                corner_radius: Nm(width.0 / 2),
                stroke: Some(DRAFT_WIDTH),
            }]
        }
        Primitive::Rect {
            center,
            size,
            rotation,
            corner_radius,
            stroke: None,
        } => vec![Primitive::Rect {
            center: *center,
            size: *size,
            rotation: *rotation,
            corner_radius: *corner_radius,
            stroke: Some(DRAFT_WIDTH),
        }],
        Primitive::Ring {
            center,
            outer,
            inner,
        } => {
            let edge = |r: Nm| Primitive::Ring {
                center: *center,
                outer: Nm(r.0 + half),
                inner: Nm((r.0 - half).max(0)),
            };
            if inner.0 > 0 {
                vec![edge(*outer), edge(*inner)]
            } else {
                vec![edge(*outer)]
            }
        }
        Primitive::Triangles(mesh) => {
            // An edge used by one triangle only is on the boundary.
            let mut uses: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
            for t in mesh.indices.chunks_exact(3) {
                for (i, j) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    *uses.entry((i.min(j), i.max(j))).or_default() += 1;
                }
            }
            uses.into_iter()
                .filter(|(_, n)| *n == 1)
                .map(|((i, j), _)| Primitive::Hairline {
                    a: mesh.points[i as usize],
                    b: mesh.points[j as usize],
                })
                .collect()
        }
        other => vec![other.clone()],
    }
}

/// `length` in `units` with up to 4 decimals (mm) or 2 (mil), trailing zeros dropped,
/// and the unit: `12.5 mm`, `492.13 mil`.
pub fn format_length(length: Nm, units: Units) -> String {
    let (value, decimals, unit) = match units {
        Units::Mm => (length.0 as f64 / 1e6, 4, "mm"),
        Units::Mil => (length.0 as f64 / 25_400.0, 2, "mil"),
    };
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    format!("{text} {unit}")
}

/// The `display.layer_colors.<theme>.<key>` key of a Layer's colour: its kind and side,
/// such as `copper_top` or `board_outline`.
pub fn color_key(kind: LayerKind, side: Option<Side>) -> String {
    let kind = serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    match side {
        Some(Side::Top) => format!("{kind}_top"),
        Some(Side::Bottom) => format!("{kind}_bottom"),
        None => kind,
    }
}

/// Preference key shared by the Layer's swatch override and saved Layer Sets.
pub fn layer_key(set: &LayerSet, layer: &Layer) -> String {
    if layer.kind == LayerKind::Copper
        && layer.side.is_none()
        && let Some(n) = set.copper_number(layer.uuid)
    {
        format!("inner_{}", n - 1)
    } else {
        color_key(layer.kind, layer.side)
    }
}

/// `#rrggbb` as a colour.
pub fn parse_hex(text: &str) -> Option<Rgba> {
    if !reda_engine::preferences::is_hex_color(text) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&text[i..i + 2], 16).ok();
    Some(Rgba::rgb(byte(1)?, byte(3)?, byte(5)?))
}

/// A colour as `#rrggbb`.
pub fn to_hex(color: Rgba) -> String {
    let [r, g, b, _] = color.0;
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The Layer colour overrides in `prefs`, for the canvas.
pub fn palette_handle(prefs: &Preferences, generation: u64) -> PaletteHandle {
    let overrides = |colors: &reda_engine::preferences::LayerColors| -> Arc<[(ColorKey, Rgba)]> {
        let mut out = Vec::new();
        for (key, text) in colors.overrides() {
            let Some(color) = parse_hex(text) else {
                continue;
            };
            if let Some(inner) = key
                .strip_prefix("inner_")
                .and_then(|n| n.parse().ok())
                .and_then(BoardColor::inner)
            {
                out.push((ColorKey::Board(inner), color));
                continue;
            }
            for kind in LAYER_KINDS {
                for side in [Some(Side::Top), Some(Side::Bottom), None] {
                    if color_key(kind, side) == key {
                        out.push((ColorKey::Board(BoardColor::layer(kind, side)), color));
                    }
                }
            }
        }
        out.into()
    };
    let colors = &prefs.display.layer_colors;
    PaletteHandle {
        generation,
        dark: overrides(&colors.dark),
        light: overrides(&colors.light),
    }
}

/// Every Layer kind.
const LAYER_KINDS: [LayerKind; 9] = [
    LayerKind::Copper,
    LayerKind::Silkscreen,
    LayerKind::SolderMask,
    LayerKind::Paste,
    LayerKind::Courtyard,
    LayerKind::Assembly,
    LayerKind::BoardOutline,
    LayerKind::Mechanical,
    LayerKind::Documentation,
];

/// The renderer's Inactive-Layer mode for a Preference value.
pub fn inactive_layers(mode: InactiveLayerMode) -> InactiveLayers {
    match mode {
        InactiveLayerMode::Normal => InactiveLayers::Normal,
        InactiveLayerMode::Dimmed => InactiveLayers::Dimmed,
        InactiveLayerMode::Hidden => InactiveLayers::Hidden,
    }
}
