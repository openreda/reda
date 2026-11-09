//! Drawing the Pad rows of [`super`]: a two-column grid, each row labelled for
//! accessibility, giving back the edits made this frame.

use egui::{ComboBox, Grid, Id, Role, Ui, WidgetInfo};
use reda_engine::model::geom::{Millideg, Nm};
use reda_engine::model::{
    ConnectionStyle, Corner, Corners, HoleTolerance, Pad, PadKind, PadShape, Paste, Side,
};
use reda_engine::preferences::Units;

use super::{
    MaskChoice, PadEdit, PasteChoice, cornered, format_ratio, mask_choice, mask_sides, parse_ratio,
    paste_choice,
};
use crate::element_editor::text_field;
use crate::tr;

const FIELD_WIDTH: f32 = 110.0;

pub const KINDS: [PadKind; 3] = [PadKind::Smd, PadKind::PlatedThroughHole, PadKind::NonPlated];
pub const SHAPES: [PadShape; 5] = [
    PadShape::Circle,
    PadShape::Rect,
    PadShape::RoundedRect,
    PadShape::ChamferedRect,
    PadShape::Oval,
];

pub fn kind_name(kind: PadKind) -> String {
    match kind {
        PadKind::Smd => tr!("footprint-pad-smd"),
        PadKind::PlatedThroughHole => tr!("footprint-pad-pth"),
        PadKind::NonPlated => tr!("footprint-pad-npth"),
    }
}

pub fn shape_name(shape: PadShape) -> String {
    match shape {
        PadShape::Circle => tr!("footprint-shape-circle"),
        PadShape::Rect => tr!("footprint-shape-rect"),
        PadShape::RoundedRect => tr!("footprint-shape-rounded-rect"),
        PadShape::ChamferedRect => tr!("footprint-shape-chamfered-rect"),
        PadShape::Oval => tr!("footprint-shape-oval"),
    }
}

fn side_name(side: Side) -> String {
    match side {
        Side::Top => tr!("pad-side-top"),
        Side::Bottom => tr!("pad-side-bottom"),
    }
}

fn corner_name(corner: Corner) -> String {
    match corner {
        Corner::TopLeft => tr!("pad-corner-top-left"),
        Corner::TopRight => tr!("pad-corner-top-right"),
        Corner::BottomLeft => tr!("pad-corner-bottom-left"),
        Corner::BottomRight => tr!("pad-corner-bottom-right"),
    }
}

/// How the form shows and reads lengths: a bare number in `units`, or any length typed
/// with its unit.
fn show_length(n: Nm, units: Units) -> String {
    crate::pcb::view::format_length(n, units)
}

pub fn read_length(text: &str, units: Units) -> Option<Nm> {
    let text = text.trim();
    match text.parse::<f64>() {
        Ok(v) => Some(Nm(match units {
            Units::Mm => (v * 1e6).round() as i64,
            Units::Mil => (v * 25_400.0).round() as i64,
        })),
        Err(_) => Nm::parse(&text.replace(' ', "")).ok(),
    }
}

fn read_angle(text: &str) -> Option<Millideg> {
    let t = text
        .trim()
        .trim_end_matches('°')
        .trim_end_matches("deg")
        .trim();
    t.parse::<f64>()
        .ok()
        .map(|deg| Millideg((deg * 1000.0).round() as i64))
}

fn show_angle(a: Millideg) -> String {
    let s = format!("{:.3}", a.0 as f64 / 1000.0);
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The Pad rows for `pad`; `numbered` adds the number row (a Footprint's Pad), `enabled`
/// false shows them read-only. Returns the edits made.
pub fn rows(
    ui: &mut Ui,
    id: Id,
    pad: &Pad,
    numbered: bool,
    enabled: bool,
    units: Units,
) -> Vec<PadEdit> {
    let mut out = Vec::new();
    ui.add_enabled_ui(enabled, |ui| {
        Grid::new(id.with("pad-form"))
            .num_columns(2)
            .show(ui, |ui| {
                let mut row = Row { ui, id, units };
                if numbered
                    && let Some(n) = row.text("number", &tr!("footprint-pad-number"), &pad.number)
                {
                    out.push(PadEdit::Number(n));
                }
                if let Some(k) = row.combo(
                    "kind",
                    &tr!("footprint-pad-kind"),
                    pad.kind,
                    &KINDS,
                    kind_name,
                ) {
                    out.push(PadEdit::Kind(k));
                }
                if let Some(s) = row.combo(
                    "shape",
                    &tr!("footprint-pad-shape"),
                    pad.shape,
                    &SHAPES,
                    shape_name,
                ) {
                    out.push(PadEdit::Shape(s));
                }
                if let Some(w) = row.length("width", &tr!("footprint-pad-width"), pad.width) {
                    out.push(PadEdit::Width(w));
                }
                if let Some(h) = row.length("height", &tr!("footprint-pad-height"), pad.height) {
                    out.push(PadEdit::Height(h));
                }
                if let Some(r) = row.angle("rotation", &tr!("footprint-rotation"), pad.rotation) {
                    out.push(PadEdit::Rotation(r));
                }
                shape_rows(&mut row, pad, &mut out);
                let offset = pad.offset.unwrap_or_default();
                if let Some(x) = row.length("offset-x", &tr!("pad-offset-x"), offset.x) {
                    out.push(PadEdit::OffsetX(x));
                }
                if let Some(y) = row.length("offset-y", &tr!("pad-offset-y"), offset.y) {
                    out.push(PadEdit::OffsetY(y));
                }
                hole_rows(&mut row, pad, &mut out);
                if pad.kind == PadKind::Smd {
                    let side = pad.side.unwrap_or(Side::Top);
                    if let Some(s) = row.combo(
                        "side",
                        &tr!("pad-side"),
                        side,
                        &[Side::Top, Side::Bottom],
                        side_name,
                    ) {
                        out.push(PadEdit::Side(s));
                    }
                }
                opening_rows(&mut row, pad, &mut out);
                zone_rows(&mut row, pad, &mut out);
            });
    });
    out
}

fn shape_rows(row: &mut Row<'_>, pad: &Pad, out: &mut Vec<PadEdit>) {
    if !cornered(pad.shape) {
        return;
    }
    let (w, h) = (pad.width, pad.height);
    let corner = pad.corner_ratio.map(format_ratio).unwrap_or_default();
    if let Some(text) = row.text("corner-ratio", &tr!("pad-corner-ratio"), &corner)
        && let Some(r) = parse_ratio(&text, w, h)
    {
        out.push(PadEdit::CornerRatio(r));
    }
    if pad.shape == PadShape::ChamferedRect {
        let chamfer = pad.chamfer_ratio.map(format_ratio).unwrap_or_default();
        if let Some(text) = row.text("chamfer-ratio", &tr!("pad-chamfer-ratio"), &chamfer)
            && let Some(r) = parse_ratio(&text, w, h)
        {
            out.push(PadEdit::ChamferRatio(r));
        }
    }
    row.ui.label(tr!("pad-corners"));
    let corners = pad.corners.unwrap_or(Corners::ALL);
    row.ui.horizontal_wrapped(|ui| {
        for c in Corner::ALL {
            let mut on = corners.contains(c);
            let name = corner_name(c);
            let check = ui.checkbox(&mut on, &name);
            check.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, name.clone()));
            if check.changed() {
                out.push(PadEdit::Corner(c, on));
            }
        }
    });
    row.ui.end_row();
}

fn hole_rows(row: &mut Row<'_>, pad: &Pad, out: &mut Vec<PadEdit>) {
    let Some(hole) = pad.hole else {
        return;
    };
    if let Some(w) = row.length("hole-w", &tr!("footprint-hole-width"), hole.width) {
        out.push(PadEdit::HoleWidth(w));
    }
    if let Some(h) = row.length("hole-h", &tr!("footprint-hole-height"), hole.height) {
        out.push(PadEdit::HoleHeight(h));
    }
    if let Some(r) = row.angle("hole-rotation", &tr!("pad-hole-rotation"), hole.rotation) {
        out.push(PadEdit::HoleRotation(r));
    }
    let tolerance = hole.tolerance;
    let plus = tolerance
        .map(|t| show_length(t.plus, row.units))
        .unwrap_or_default();
    if let Some(text) = row.text("tol-plus", &tr!("pad-hole-tolerance-plus"), &plus) {
        let minus = tolerance.map_or(Nm(0), |t| t.minus);
        match read_length(&text, row.units) {
            _ if text.is_empty() => out.push(PadEdit::HoleTolerance(None)),
            Some(p) => out.push(PadEdit::HoleTolerance(Some(HoleTolerance {
                plus: p,
                minus,
            }))),
            None => {}
        }
    }
    let minus = tolerance
        .map(|t| show_length(t.minus, row.units))
        .unwrap_or_default();
    if let Some(text) = row.text("tol-minus", &tr!("pad-hole-tolerance-minus"), &minus) {
        let plus = tolerance.map_or(Nm(0), |t| t.plus);
        match read_length(&text, row.units) {
            _ if text.is_empty() => out.push(PadEdit::HoleTolerance(None)),
            Some(m) => out.push(PadEdit::HoleTolerance(Some(HoleTolerance {
                plus,
                minus: m,
            }))),
            None => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MaskMode {
    Inherit,
    Tent,
    Expansion,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PasteMode {
    Inherit,
    None,
    Expansion,
    Percent,
    Windows,
}

fn opening_rows(row: &mut Row<'_>, pad: &Pad, out: &mut Vec<PadEdit>) {
    for side in mask_sides(pad) {
        let choice = mask_choice(pad, side);
        let mode = match choice {
            MaskChoice::Inherit => MaskMode::Inherit,
            MaskChoice::Tent => MaskMode::Tent,
            MaskChoice::Expansion(_) => MaskMode::Expansion,
        };
        let key = match side {
            Side::Top => "mask-top",
            Side::Bottom => "mask-bottom",
        };
        let label = match side {
            Side::Top => tr!("pad-mask-top"),
            Side::Bottom => tr!("pad-mask-bottom"),
        };
        let modes = [MaskMode::Inherit, MaskMode::Tent, MaskMode::Expansion];
        let name = |m: MaskMode| match m {
            MaskMode::Inherit => tr!("pad-inherit"),
            MaskMode::Tent => tr!("pad-mask-tent"),
            MaskMode::Expansion => tr!("pad-expansion"),
        };
        if let Some(m) = row.combo(key, &label, mode, &modes, name) {
            out.push(PadEdit::Mask(
                side,
                match m {
                    MaskMode::Inherit => MaskChoice::Inherit,
                    MaskMode::Tent => MaskChoice::Tent,
                    MaskMode::Expansion => MaskChoice::Expansion(Nm(0)),
                },
            ));
        }
        if let MaskChoice::Expansion(e) = choice {
            let label = tr!("pad-mask-expansion-of", side = side_name(side));
            if let Some(v) = row.length(&format!("{key}-value"), &label, e) {
                out.push(PadEdit::Mask(side, MaskChoice::Expansion(v)));
            }
        }
    }
    if pad.kind != PadKind::Smd {
        let now = pad.mask.and_then(|m| m.from_hole_edge);
        let options = [None, Some(true), Some(false)];
        let name = |o: Option<bool>| match o {
            None => tr!("pad-inherit"),
            Some(true) => tr!("pad-on"),
            Some(false) => tr!("pad-off"),
        };
        if let Some(o) = row.combo(
            "from-hole-edge",
            &tr!("pad-from-hole-edge"),
            now,
            &options,
            name,
        ) {
            out.push(PadEdit::FromHoleEdge(o));
        }
    }
    if pad.kind != PadKind::Smd {
        return;
    }
    let choice = paste_choice(pad);
    let mode = match &choice {
        Some(PasteChoice::Inherit) => PasteMode::Inherit,
        Some(PasteChoice::None) => PasteMode::None,
        Some(PasteChoice::Expansion(_)) => PasteMode::Expansion,
        Some(PasteChoice::Percent(_)) => PasteMode::Percent,
        None => PasteMode::Windows,
    };
    let modes: &[PasteMode] = if mode == PasteMode::Windows {
        &[
            PasteMode::Inherit,
            PasteMode::None,
            PasteMode::Expansion,
            PasteMode::Percent,
            PasteMode::Windows,
        ]
    } else {
        &[
            PasteMode::Inherit,
            PasteMode::None,
            PasteMode::Expansion,
            PasteMode::Percent,
        ]
    };
    let windows = match &pad.paste {
        Some(Paste::Windows { windows }) => windows.len(),
        _ => 0,
    };
    let name = move |m: PasteMode| match m {
        PasteMode::Inherit => tr!("pad-inherit"),
        PasteMode::None => tr!("pad-paste-none"),
        PasteMode::Expansion => tr!("pad-expansion"),
        PasteMode::Percent => tr!("pad-paste-percent"),
        PasteMode::Windows => tr!("pad-paste-windows", count = windows),
    };
    if let Some(m) = row.combo("paste", &tr!("pad-paste"), mode, modes, name) {
        let choice = match m {
            PasteMode::Inherit => Some(PasteChoice::Inherit),
            PasteMode::None => Some(PasteChoice::None),
            PasteMode::Expansion => Some(PasteChoice::Expansion(Nm(0))),
            PasteMode::Percent => Some(PasteChoice::Percent(0)),
            PasteMode::Windows => None,
        };
        out.extend(choice.map(PadEdit::Paste));
    }
    match choice {
        Some(PasteChoice::Expansion(e)) => {
            if let Some(v) = row.length("paste-value", &tr!("pad-paste-expansion"), e) {
                out.push(PadEdit::Paste(PasteChoice::Expansion(v)));
            }
        }
        Some(PasteChoice::Percent(p)) => {
            if let Some(text) = row.text("paste-percent", &tr!("pad-paste-area"), &format!("{p}%"))
                && let Ok(v) = text.trim().trim_end_matches('%').trim().parse::<i32>()
            {
                out.push(PadEdit::Paste(PasteChoice::Percent(v)));
            }
        }
        _ => {}
    }
}

fn zone_rows(row: &mut Row<'_>, pad: &Pad, out: &mut Vec<PadEdit>) {
    if pad.kind == PadKind::NonPlated {
        return;
    }
    let z = pad.zone_connection.unwrap_or_default();
    let styles = [
        None,
        Some(ConnectionStyle::Relief),
        Some(ConnectionStyle::Solid),
        Some(ConnectionStyle::None),
    ];
    let name = |s: Option<ConnectionStyle>| match s {
        None => tr!("pad-inherit"),
        Some(ConnectionStyle::Relief) => tr!("pad-zone-relief"),
        Some(ConnectionStyle::Solid) => tr!("pad-zone-solid"),
        Some(ConnectionStyle::None) => tr!("pad-zone-none"),
    };
    if let Some(s) = row.combo(
        "zone-style",
        &tr!("pad-zone-connection"),
        z.style,
        &styles,
        name,
    ) {
        out.push(PadEdit::ZoneStyle(s));
    }
    let spokes = [None, Some(2), Some(4)];
    let name = |s: Option<u8>| s.map_or_else(|| tr!("pad-inherit"), |n| n.to_string());
    if let Some(s) = row.combo(
        "zone-spokes",
        &tr!("pad-zone-spokes"),
        z.spokes,
        &spokes,
        name,
    ) {
        out.push(PadEdit::ZoneSpokes(s));
    }
    if let Some(v) = row.optional_length(
        "zone-spoke-width",
        &tr!("pad-zone-spoke-width"),
        z.spoke_width,
    ) {
        out.push(PadEdit::ZoneSpokeWidth(v));
    }
    if let Some(v) = row.optional_length("zone-gap", &tr!("pad-zone-gap"), z.gap) {
        out.push(PadEdit::ZoneGap(v));
    }
    let angles = [
        None,
        Some(Millideg(0)),
        Some(Millideg(45_000)),
        Some(Millideg(90_000)),
    ];
    let name = |a: Option<Millideg>| {
        a.map_or_else(|| tr!("pad-inherit"), |a| format!("{}°", show_angle(a)))
    };
    if let Some(a) = row.combo("zone-angle", &tr!("pad-zone-angle"), z.angle, &angles, name) {
        out.push(PadEdit::ZoneAngle(a));
    }
}

struct Row<'a> {
    ui: &'a mut Ui,
    id: Id,
    units: Units,
}

impl Row<'_> {
    fn text(&mut self, key: &str, label: &str, current: &str) -> Option<String> {
        self.ui.label(label);
        let typed = text_field(self.ui, self.id.with(key), label, current, FIELD_WIDTH);
        self.ui.end_row();
        typed
    }

    fn length(&mut self, key: &str, label: &str, current: Nm) -> Option<Nm> {
        let shown = show_length(current, self.units);
        let typed = self.text(key, label, &shown)?;
        read_length(&typed, self.units).filter(|v| *v != current)
    }

    /// An empty field inherits (`Some(None)`).
    fn optional_length(
        &mut self,
        key: &str,
        label: &str,
        current: Option<Nm>,
    ) -> Option<Option<Nm>> {
        let shown = current
            .map(|n| show_length(n, self.units))
            .unwrap_or_default();
        let typed = self.text(key, label, &shown)?;
        if typed.is_empty() {
            return current.is_some().then_some(None);
        }
        read_length(&typed, self.units)
            .map(Some)
            .filter(|v| *v != current)
    }

    fn angle(&mut self, key: &str, label: &str, current: Millideg) -> Option<Millideg> {
        let typed = self.text(key, label, &show_angle(current))?;
        read_angle(&typed).filter(|v| *v != current)
    }

    fn combo<T: Copy + PartialEq>(
        &mut self,
        key: &str,
        label: &str,
        current: T,
        all: &[T],
        name: impl Fn(T) -> String,
    ) -> Option<T> {
        self.ui.label(label);
        let mut value = current;
        let response = ComboBox::from_id_salt(self.id.with(key))
            .selected_text(name(current))
            .show_ui(self.ui, |ui| {
                for v in all {
                    ui.selectable_value(&mut value, *v, name(*v));
                }
            })
            .response;
        let label = label.to_owned();
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
        self.ui.end_row();
        (value != current).then_some(value)
    }
}
