//! Object Defaults (docs/spec/v0.3/input-and-defaults.md#object-defaults): one record of
//! starting values per kind of object, which a placing Tool starts from. They are the
//! `defaults.*` Preferences, per user; nothing is stored in a Project.
//!
//! A record holds properties of the object only: never position, identity, Net or text
//! content, and never a value the board's rules decide (Track width, Via size,
//! clearances). Commands never read a record, so an agent or the CLI gets the same result
//! on every machine: the Tools put the values in the Command's params.
//!
//! The Line family and Text on boards and in Footprints hold one set per Layer kind, and
//! the two editors share them (`defaults.pcb.<kind>.<layer_kind>.<field>`).

use reda_command::{AreaValue, Length};
use reda_model::geom::Nm;
use reda_model::{
    ArrowPosition, DimensionFormat, DimensionUnits, FootprintLayer, HERSHEY_FONT, LayerKind,
    PadKind, PadShape, PinType, PortDirection, PourOver, Ratio, Restriction, TextHJustify,
    TextVJustify,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::KeepoutToolShape;

/// Every kind's Object Defaults, and whether Tool-option edits are kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ObjectDefaults {
    /// Remember last used: a change to a placing Tool's options in Properties is written
    /// here, so the next placement and the next session start from it. Off, a change
    /// lasts until the Tool is left.
    pub remember_last_used: bool,
    /// Boards, and the Line family and Text in Footprints too.
    pub pcb: PcbDefaults,
    /// The Footprint editor's own kinds.
    pub footprint: FootprintDefaults,
    /// The Symbol editor's kinds.
    pub symbol: SymbolDefaults,
    /// Schematic Sheets, and Text in Symbols too.
    pub schematic: SchematicDefaults,
}

impl Default for ObjectDefaults {
    fn default() -> ObjectDefaults {
        ObjectDefaults {
            remember_last_used: true,
            pcb: PcbDefaults::default(),
            footprint: FootprintDefaults::default(),
            symbol: SymbolDefaults::default(),
            schematic: SchematicDefaults::default(),
        }
    }
}

/// The Layer kinds a board or Footprint drawing can be on, each with its own set of
/// Line-family and Text values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrawingKind {
    Copper,
    Silkscreen,
    Courtyard,
    Assembly,
    Mechanical,
}

impl DrawingKind {
    pub const ALL: [DrawingKind; 5] = [
        DrawingKind::Copper,
        DrawingKind::Silkscreen,
        DrawingKind::Courtyard,
        DrawingKind::Assembly,
        DrawingKind::Mechanical,
    ];

    /// The key segment, such as `silkscreen`.
    pub fn key(self) -> &'static str {
        match self {
            DrawingKind::Copper => "copper",
            DrawingKind::Silkscreen => "silkscreen",
            DrawingKind::Courtyard => "courtyard",
            DrawingKind::Assembly => "assembly",
            DrawingKind::Mechanical => "mechanical",
        }
    }

    /// The set a board Layer of `kind` uses; `None` for kinds nothing is drawn on.
    pub fn of_layer(kind: LayerKind) -> Option<DrawingKind> {
        Some(match kind {
            LayerKind::Copper => DrawingKind::Copper,
            LayerKind::Silkscreen => DrawingKind::Silkscreen,
            LayerKind::Courtyard => DrawingKind::Courtyard,
            LayerKind::Assembly => DrawingKind::Assembly,
            LayerKind::Mechanical => DrawingKind::Mechanical,
            LayerKind::SolderMask | LayerKind::Paste | LayerKind::BoardOutline => return None,
        })
    }

    /// The set a Footprint Layer uses.
    pub fn of_footprint(layer: FootprintLayer) -> DrawingKind {
        if layer.is_silkscreen() {
            DrawingKind::Silkscreen
        } else if layer.is_courtyard() {
            DrawingKind::Courtyard
        } else {
            DrawingKind::Assembly
        }
    }

    /// The shipped Line-family width of this kind.
    fn width(self) -> Length {
        Length(match self {
            DrawingKind::Courtyard => Nm::from_um(50),
            DrawingKind::Assembly => Nm::from_um(100),
            DrawingKind::Copper | DrawingKind::Silkscreen | DrawingKind::Mechanical => {
                Nm::from_um(150)
            }
        })
    }
}

/// An open shape's values: its pen width.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Stroke {
    /// The pen width.
    pub width: Length,
}

impl Default for Stroke {
    fn default() -> Stroke {
        Stroke {
            width: DrawingKind::Copper.width(),
        }
    }
}

/// A closed shape's values: its pen width and whether it is filled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Outline {
    /// The outline's pen width.
    pub width: Length,
    /// Filled as well as outlined.
    pub fill: bool,
}

impl Default for Outline {
    fn default() -> Outline {
        Outline {
            width: DrawingKind::Copper.width(),
            fill: false,
        }
    }
}

impl From<Stroke> for Outline {
    fn from(s: Stroke) -> Outline {
        Outline {
            width: s.width,
            fill: false,
        }
    }
}

/// A text's look: height, pen and font.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TextLook {
    /// The capital height.
    pub height: Length,
    /// The pen width of stroke-font text.
    pub thickness: Length,
    /// The font family, such as `Hershey Roman Simplex` (the stroke font) or `Noto Sans`.
    pub font: String,
}

impl Default for TextLook {
    fn default() -> TextLook {
        TextLook {
            height: Length(Nm::from_mm(1)),
            thickness: Length(Nm::from_um(150)),
            font: HERSHEY_FONT.to_owned(),
        }
    }
}

/// One set of values per [`DrawingKind`]: `by_kind!(Name, Item, make)` makes the record.
macro_rules! by_kind {
    ($(#[$doc:meta])* $name:ident, $item:ty, $make:expr) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(default, deny_unknown_fields)]
        pub struct $name {
            pub copper: $item,
            pub silkscreen: $item,
            pub courtyard: $item,
            pub assembly: $item,
            pub mechanical: $item,
        }

        impl Default for $name {
            fn default() -> $name {
                let make: fn(DrawingKind) -> $item = $make;
                $name {
                    copper: make(DrawingKind::Copper),
                    silkscreen: make(DrawingKind::Silkscreen),
                    courtyard: make(DrawingKind::Courtyard),
                    assembly: make(DrawingKind::Assembly),
                    mechanical: make(DrawingKind::Mechanical),
                }
            }
        }

        impl $name {
            /// The set of Layer kind `kind`.
            pub fn of(&self, kind: DrawingKind) -> &$item {
                match kind {
                    DrawingKind::Copper => &self.copper,
                    DrawingKind::Silkscreen => &self.silkscreen,
                    DrawingKind::Courtyard => &self.courtyard,
                    DrawingKind::Assembly => &self.assembly,
                    DrawingKind::Mechanical => &self.mechanical,
                }
            }
        }
    };
}

by_kind!(
    /// An open shape's values on each Layer kind.
    StrokeByKind,
    Stroke,
    |k| Stroke { width: k.width() }
);
by_kind!(
    /// A closed shape's values on each Layer kind.
    OutlineByKind,
    Outline,
    |k| Outline {
        width: k.width(),
        fill: false
    }
);

/// Board kinds, and the Line family and Text that Footprints share.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PcbDefaults {
    /// Lines, per Layer kind.
    pub line: StrokeByKind,
    /// Rectangles, per Layer kind.
    pub rectangle: OutlineByKind,
    /// Arcs, per Layer kind.
    pub arc: StrokeByKind,
    /// Circles, per Layer kind.
    pub circle: OutlineByKind,
    /// Polygons, per Layer kind.
    pub polygon: OutlineByKind,
    /// Text, per Layer kind, and how it sits on its anchor on a board.
    pub text: PcbTextDefaults,
    /// Dimensions.
    pub dimension: DimensionDefaults,
    /// Free Pads.
    pub free_pad: FreePadDefaults,
    /// Zones.
    pub zone: ZoneDefaults,
    /// Keepouts.
    pub keepout: KeepoutDefaults,
    /// Vias.
    pub via: ViaDefaults,
}

/// Text on boards and in Footprints: a look per Layer kind, and the board's justify
/// (a Footprint's is [`FootprintTextDefaults`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PcbTextDefaults {
    pub copper: TextLook,
    pub silkscreen: TextLook,
    pub courtyard: TextLook,
    pub assembly: TextLook,
    pub mechanical: TextLook,
    /// How board text sits on its anchor: left and bottom.
    pub justify: Justify,
}

impl PcbTextDefaults {
    /// The look of text on Layer kind `kind`.
    pub fn of(&self, kind: DrawingKind) -> &TextLook {
        match kind {
            DrawingKind::Copper => &self.copper,
            DrawingKind::Silkscreen => &self.silkscreen,
            DrawingKind::Courtyard => &self.courtyard,
            DrawingKind::Assembly => &self.assembly,
            DrawingKind::Mechanical => &self.mechanical,
        }
    }
}

/// A new Dimension's style. Where its line sits comes from the Tool's third click; the
/// `offset` is used when the Tool finishes on two ends (Enter).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DimensionDefaults {
    /// The pen width of the dimension line and arrows.
    pub width: Length,
    /// The pen width of the extension lines.
    pub extension_width: Length,
    /// How far the dimension line sits from its first end.
    pub offset: Length,
    /// How far each extension line stops short of its point and runs past the line.
    pub gap: Length,
    /// The space between the dimension line and the text.
    pub text_gap: Length,
    /// The length of each arrowhead side.
    pub arrow_size: Length,
    /// With outside arrows, how far the line runs past each extension line.
    pub arrow_length: Length,
    /// Where the arrows sit: `inside` or `outside`.
    pub arrow_position: ArrowPosition,
    /// Digits after the decimal point, 0 to 6.
    pub precision: u8,
    /// The units the value is shown in: `mm`, `mil`, `in`, `cm` or `automatic`.
    pub units: DimensionUnits,
    /// How the value is written: `none`, `value`, `value_unit` or `value_bracketed_unit`.
    pub format: DimensionFormat,
    /// The text's capital height.
    pub text_height: Length,
}

impl Default for DimensionDefaults {
    fn default() -> DimensionDefaults {
        DimensionDefaults {
            width: Length(Nm::from_um(150)),
            extension_width: Length(Nm::from_um(150)),
            offset: Length(Nm::from_mm(2)),
            gap: Length(Nm::from_um(500)),
            text_gap: Length(Nm::from_um(250)),
            arrow_size: Length(Nm::from_mm(1)),
            arrow_length: Length(Nm::from_mm(2)),
            arrow_position: ArrowPosition::Inside,
            precision: 2,
            units: DimensionUnits::Automatic,
            format: DimensionFormat::ValueUnit,
            text_height: Length(Nm::from_mm(1)),
        }
    }
}

/// A new Free Pad.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct FreePadDefaults {
    /// `smd`, `plated_through_hole` or `non_plated`.
    pub kind: PadKind,
    /// `circle`, `rect`, `rounded_rect`, `oval` or `chamfered_rect`.
    pub shape: PadShape,
    /// The copper's width.
    pub width: Length,
    /// The copper's height.
    pub height: Length,
    /// The round hole's diameter, for a Pad with a hole.
    pub hole: Length,
}

impl Default for FreePadDefaults {
    fn default() -> FreePadDefaults {
        FreePadDefaults {
            kind: PadKind::PlatedThroughHole,
            shape: PadShape::Circle,
            width: Length(Nm::from_mm(2)),
            height: Length(Nm::from_mm(2)),
            hole: Length(Nm::from_mm(1)),
        }
    }
}

/// A new Zone (docs/spec/v0.1/pcb.md#creating-zones). Unset values follow the rules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ZoneDefaults {
    /// The gap to other Nets' copper, where it is larger than the clearance rule's;
    /// unset, the rule alone.
    pub clearance: Option<Length>,
    /// The narrowest copper the fill may leave; unset, the Net's `track_width` minimum.
    pub min_width: Option<Length>,
    /// How the fill treats copper of its own Net: `zones_only`, `all` or `none`.
    pub pour_over: PourOver,
    /// Remove fill pieces that touch no copper of the Zone's Net.
    pub remove_unconnected: bool,
    /// Fill pieces smaller than this area are dropped, such as `0.5mm2`.
    pub min_island_area: AreaValue,
}

impl Default for ZoneDefaults {
    fn default() -> ZoneDefaults {
        ZoneDefaults {
            clearance: None,
            min_width: None,
            pour_over: PourOver::default(),
            remove_unconnected: true,
            min_island_area: AreaValue(0),
        }
    }
}

/// A new Keepout, and how the Keepout Tool draws it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct KeepoutDefaults {
    /// How the Keepout Tool draws: `polygon` (a click per corner), `rectangle` (two
    /// corners) or `circle` (centre, then a point on the edge). All store polygons.
    pub shape: KeepoutToolShape,
    /// Keep out on every copper Layer, including any added later; off, only the active
    /// copper Layer.
    pub all_layers: bool,
    /// What it keeps out: any of `tracks`, `vias`, `zones`, `smd_pads`, `tht_pads` and
    /// `footprints`.
    pub restrict: BTreeSet<Restriction>,
}

impl Default for KeepoutDefaults {
    fn default() -> KeepoutDefaults {
        KeepoutDefaults {
            shape: KeepoutToolShape::default(),
            all_layers: true,
            restrict: Restriction::defaults(),
        }
    }
}

/// A new Via. Its size and hole come from the board's Via types and Net Classes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ViaDefaults {
    /// Derive Teardrops where Tracks meet it.
    pub teardrop: bool,
}

impl Default for ViaDefaults {
    fn default() -> ViaDefaults {
        ViaDefaults { teardrop: true }
    }
}

/// The Footprint editor's own kinds.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct FootprintDefaults {
    /// Pads.
    pub pad: PadDefaults,
    /// How Footprint text sits on its anchor (its look is shared with boards).
    pub text: FootprintTextDefaults,
}

/// What the Pad Tool places.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PadDefaults {
    /// `smd`, `plated_through_hole` or `non_plated`.
    pub kind: PadKind,
    /// `circle`, `rect`, `rounded_rect`, `oval` or `chamfered_rect`.
    pub shape: PadShape,
    /// The copper's width.
    pub width: Length,
    /// The copper's height.
    pub height: Length,
    /// The hole's width, for a Pad with a hole.
    pub hole_width: Length,
    /// The hole's height: equal to the width is round, else a slot.
    pub hole_height: Length,
    /// The chamfer of a chamfered rectangle, in thousandths of a percent of half the
    /// shorter side.
    pub chamfer_ratio: Ratio,
    /// The solder mask margin; unset, the board's.
    pub mask_margin: Option<Length>,
    /// The paste margin of an SMD Pad; unset, the board's.
    pub paste_margin: Option<Length>,
}

impl Default for PadDefaults {
    fn default() -> PadDefaults {
        PadDefaults {
            kind: PadKind::Smd,
            shape: PadShape::Rect,
            width: Length(Nm::from_mm(1)),
            height: Length(Nm::from_um(1500)),
            hole_width: Length(Nm::from_um(800)),
            hole_height: Length(Nm::from_um(800)),
            chamfer_ratio: Ratio::percent(50),
            mask_margin: None,
            paste_margin: None,
        }
    }
}

/// Footprint text: centred on its anchor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct FootprintTextDefaults {
    /// How Footprint text sits on its anchor.
    pub justify: Justify,
}

impl Default for FootprintTextDefaults {
    fn default() -> FootprintTextDefaults {
        FootprintTextDefaults {
            justify: Justify {
                h: TextHJustify::Center,
                v: TextVJustify::Center,
            },
        }
    }
}

/// How text sits on its anchor; it writes as the text's own `justify`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Justify {
    /// Across: left, centre or right of the anchor.
    pub h: TextHJustify,
    /// Up and down: top, centre or bottom of the anchor.
    pub v: TextVJustify,
}

impl From<Justify> for reda_model::TextJustify {
    fn from(j: Justify) -> reda_model::TextJustify {
        reda_model::TextJustify { h: j.h, v: j.v }
    }
}

/// The Symbol editor's kinds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SymbolDefaults {
    /// Pins.
    pub pin: PinDefaults,
    /// Lines.
    pub line: Stroke,
    /// Rectangles.
    pub rectangle: Outline,
    /// Arcs.
    pub arc: Stroke,
    /// Circles.
    pub circle: Outline,
    /// Polygons.
    pub polygon: Outline,
}

impl Default for SymbolDefaults {
    fn default() -> SymbolDefaults {
        let stroke = Stroke {
            width: Length(Nm::from_mil(10)),
        };
        SymbolDefaults {
            pin: PinDefaults::default(),
            line: stroke.clone(),
            rectangle: stroke.clone().into(),
            arc: stroke.clone(),
            circle: stroke.clone().into(),
            polygon: stroke.into(),
        }
    }
}

/// What the Pin Tool places.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PinDefaults {
    /// From the connection end to the body.
    pub length: Length,
    /// The electrical type ERC checks.
    pub electrical_type: PinType,
    /// Show the Pin's name.
    pub name_visible: bool,
    /// Show the Pin's number.
    pub number_visible: bool,
}

impl Default for PinDefaults {
    fn default() -> PinDefaults {
        PinDefaults {
            length: Length(Nm::from_mil(100)),
            electrical_type: PinType::Passive,
            name_visible: true,
            number_visible: true,
        }
    }
}

/// Schematic Sheet kinds, and Text in Symbols.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SchematicDefaults {
    /// Text on sheets and in Symbols.
    pub text: SheetTextDefaults,
    /// Rectangles on sheets.
    pub rectangle: SheetRectangleDefaults,
    /// Ports.
    pub port: PortDefaults,
}

/// Text on sheets and in Symbols.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SheetTextDefaults {
    /// The capital height.
    pub height: Length,
}

impl Default for SheetTextDefaults {
    fn default() -> SheetTextDefaults {
        SheetTextDefaults {
            height: Length(Nm::from_mil(50)),
        }
    }
}

/// A rectangle on a sheet.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SheetRectangleDefaults {
    /// Filled as well as outlined.
    pub fill: bool,
}

/// A new Port.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PortDefaults {
    /// `in`, `out` or `bidirectional`.
    pub direction: PortDirection,
}

impl ObjectDefaults {
    /// Range rules serde cannot express; returns the offending key and why.
    pub(super) fn check(&self) -> Result<(), (String, String)> {
        let mut lengths: Vec<(String, Nm, bool)> = Vec::new();
        let mut add = |key: String, value: &Length, zero_ok: bool| {
            lengths.push((key, value.nm(), zero_ok));
        };
        let pcb = &self.pcb;
        for kind in DrawingKind::ALL {
            let k = kind.key();
            add(
                format!("pcb.line.{k}.width"),
                &pcb.line.of(kind).width,
                false,
            );
            add(format!("pcb.arc.{k}.width"), &pcb.arc.of(kind).width, false);
            // A filled closed shape may have no outline.
            add(
                format!("pcb.rectangle.{k}.width"),
                &pcb.rectangle.of(kind).width,
                true,
            );
            add(
                format!("pcb.circle.{k}.width"),
                &pcb.circle.of(kind).width,
                true,
            );
            add(
                format!("pcb.polygon.{k}.width"),
                &pcb.polygon.of(kind).width,
                true,
            );
            let text = pcb.text.of(kind);
            add(format!("pcb.text.{k}.height"), &text.height, false);
            add(format!("pcb.text.{k}.thickness"), &text.thickness, false);
        }
        let d = &pcb.dimension;
        for (name, value, zero_ok) in [
            ("width", &d.width, false),
            ("extension_width", &d.extension_width, false),
            ("gap", &d.gap, true),
            ("text_gap", &d.text_gap, true),
            ("arrow_size", &d.arrow_size, false),
            ("arrow_length", &d.arrow_length, true),
            ("text_height", &d.text_height, false),
        ] {
            add(format!("pcb.dimension.{name}"), value, zero_ok);
        }
        let f = &pcb.free_pad;
        for (name, value) in [
            ("width", &f.width),
            ("height", &f.height),
            ("hole", &f.hole),
        ] {
            add(format!("pcb.free_pad.{name}"), value, false);
        }
        for (name, value) in [
            ("clearance", &pcb.zone.clearance),
            ("min_width", &pcb.zone.min_width),
        ] {
            if let Some(value) = value {
                add(format!("pcb.zone.{name}"), value, true);
            }
        }
        let p = &self.footprint.pad;
        for (name, value) in [
            ("width", &p.width),
            ("height", &p.height),
            ("hole_width", &p.hole_width),
            ("hole_height", &p.hole_height),
        ] {
            add(format!("footprint.pad.{name}"), value, false);
        }
        let s = &self.symbol;
        add("symbol.pin.length".into(), &s.pin.length, false);
        add("symbol.line.width".into(), &s.line.width, false);
        add("symbol.arc.width".into(), &s.arc.width, false);
        add("symbol.rectangle.width".into(), &s.rectangle.width, true);
        add("symbol.circle.width".into(), &s.circle.width, true);
        add("symbol.polygon.width".into(), &s.polygon.width, true);
        add(
            "schematic.text.height".into(),
            &self.schematic.text.height,
            false,
        );
        for (key, value, zero_ok) in lengths {
            if value < Nm::ZERO || (!zero_ok && value == Nm::ZERO) {
                let why = if zero_ok {
                    "it cannot be negative"
                } else {
                    "it must be positive"
                };
                return Err((format!("defaults.{key}"), why.to_owned()));
            }
        }
        if d.precision > 6 {
            return Err((
                "defaults.pcb.dimension.precision".into(),
                "a Dimension shows at most 6 digits after the point".into(),
            ));
        }
        if pcb.zone.min_island_area.0 < 0 {
            return Err((
                "defaults.pcb.zone.min_island_area".into(),
                "it cannot be negative".into(),
            ));
        }
        if pcb.keepout.restrict.is_empty() {
            return Err((
                "defaults.pcb.keepout.restrict".into(),
                "a Keepout must keep something out".into(),
            ));
        }
        if p.chamfer_ratio > Ratio::FULL {
            return Err((
                "defaults.footprint.pad.chamfer_ratio".into(),
                "it is at most 100000 (100%)".into(),
            ));
        }
        for kind in DrawingKind::ALL {
            if pcb.text.of(kind).font.trim().is_empty() {
                return Err((
                    format!("defaults.pcb.text.{}.font", kind.key()),
                    "a font family needs a name".into(),
                ));
            }
        }
        Ok(())
    }
}
