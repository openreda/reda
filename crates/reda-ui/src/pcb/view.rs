//! The board's view state: the Layer state (kept per Document in `view.json`) and the units
//! lengths show in (the snap grid is per Document, in [`crate::grid`]). The PCB view Commands
//! change it in the engine's view state; the Shell hands it to the PCB tab.

use std::collections::HashMap;
use std::sync::Arc;

use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{LayerKind, ObjectType, PcbDocument, Project, Side, Uuid};
use reda_engine::preferences::{InactiveLayerMode, Preferences, Units};
use reda_engine::render::{
    self, BoardColor, BoardStyle, ColorKey, Composite, InactiveLayers, Item, NetColorPattern,
    NetColorStyle, NetColorZoomOut, NetLabels, Primitive, Rgba, Scene, SceneLayer,
    board_composites,
};

use crate::canvas::PaletteHandle;
use crate::view_state::LayerView;

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
    pub layers: LayerView,
    /// The units lengths show in.
    pub units: Units,
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
            layers: LayerView::default(),
            units,
        }
    }

    /// The Layer edited: the chosen one while the board has it, else the top copper.
    pub fn active_layer(&self, project: &Project) -> Option<Uuid> {
        let set = project.pcb()?.layers();
        self.layers
            .active
            .filter(|l| set.layer(*l).is_some())
            .or_else(|| set.find(LayerKind::Copper, Some(Side::Top)).map(|l| l.uuid))
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
        let view = render::LayerView {
            visible: Some(
                set.layers
                    .iter()
                    .map(|l| l.uuid)
                    .filter(|l| !self.layers.hidden.contains(l))
                    .collect(),
            ),
            active,
            inactive: inactive_layers(self.layers.inactive_mode.unwrap_or(default_mode)),
            from_bottom: self.layers.from_bottom,
            ratsnest: !self.layers.ratsnest.hidden,
            pinned: self.layers.pinned.iter().copied().collect(),
        };
        let mut out = board_composites(&set.layers, &view);
        if let Some(lit) = self.layers.highlighted {
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
