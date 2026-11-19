//! Reading a `.kicad_mod`, in either the legacy `(module …)` or the `(footprint …)`
//! syntax, and building the REDA Footprint.
//!
//! KiCad footprint y points down and REDA's points up, so every y is negated. Graphics
//! on Layers REDA's Footprints do not have (fabrication notes, user drawings, edge cuts)
//! are left out; their copyright lines are still kept.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    ConnectionStyle, Footprint, FootprintGraphic, FootprintLayer, FootprintText, Hole, Pad,
    PadKind, PadShape, Provenance, Ratio, Shape, Side, TextStyle, Uuid, ZoneConnection,
};

use crate::node::{self, Read, fail};
use crate::provenance::is_copyright;
use crate::sexpr::Sexp;
use crate::shapes;

mod pad_stack;

#[derive(Clone, Debug)]
pub struct GraphicSource {
    pub layer: FootprintLayer,
    pub shape: Shape,
    pub width: Nm,
    pub fill: bool,
}

/// The footprint as read, before UUIDs are handed out.
#[derive(Clone, Debug)]
pub struct FootprintSource {
    pub name: String,
    pub description: String,
    pub pads: Vec<Pad>,
    pub graphics: Vec<GraphicSource>,
    pub designator: FootprintText,
    pub value: FootprintText,
    /// Further field texts, such as the assembly Designator KiCad writes as reference
    /// user text on a Fab Layer.
    pub field_texts: Vec<FootprintText>,
    /// Copyright lines from the footprint's texts, properties and description.
    pub copyright: Vec<String>,
    /// False for `exclude_from_bom` (or the legacy `virtual`).
    pub in_bom: bool,
    /// False for `exclude_from_pos_files` (or the legacy `virtual`).
    pub in_cpl: bool,
    pub warnings: Vec<String>,
}

/// y negated: KiCad footprints point y down.
fn flip(p: Point) -> Point {
    Point::new(p.x, Nm(-p.y.0))
}

fn flipped_at(node: &Sexp, name: &str) -> Read<Point> {
    Ok(flip(node::point_of(node, name)?))
}

fn layer(name: &str) -> Option<FootprintLayer> {
    Some(match name {
        "F.SilkS" | "F.Silkscreen" => FootprintLayer::SilkscreenTop,
        "B.SilkS" | "B.Silkscreen" => FootprintLayer::SilkscreenBottom,
        "F.CrtYd" | "F.Courtyard" => FootprintLayer::CourtyardTop,
        "B.CrtYd" | "B.Courtyard" => FootprintLayer::CourtyardBottom,
        "F.Fab" => FootprintLayer::AssemblyTop,
        "B.Fab" => FootprintLayer::AssemblyBottom,
        _ => return None,
    })
}

fn layer_of(item: &Sexp) -> Option<FootprintLayer> {
    item.child("layer").and_then(|l| l.arg(0)).and_then(layer)
}

pub fn read(root: &Sexp) -> Read<FootprintSource> {
    if !matches!(root.head(), Some("footprint" | "module")) {
        return fail(
            root,
            "this is not a KiCad footprint (`footprint` or `module`)",
        );
    }
    let full = node::text(root, 0)?;
    // Some files name the footprint `<library>:<name>`.
    let name = full.rsplit(':').next().unwrap_or(full).to_owned();
    let description = root
        .child("descr")
        .and_then(|d| d.arg(0))
        .unwrap_or("")
        .to_owned();
    let mut out = FootprintSource {
        name,
        description: description.clone(),
        pads: vec![],
        graphics: vec![],
        designator: FootprintText::designator(Uuid::NIL),
        value: FootprintText::value(Uuid::NIL),
        field_texts: vec![],
        copyright: vec![],
        in_bom: true,
        in_cpl: true,
        warnings: vec![],
    };
    if let Some(attr) = root.child("attr") {
        read_attributes(attr, &mut out);
    }
    let note_copyright = |text: &str, out: &mut FootprintSource| {
        if is_copyright(text) && !out.copyright.iter().any(|c| c == text) {
            out.copyright.push(text.to_owned());
        }
    };
    note_copyright(&description, &mut out);
    let mut windows = Vec::new();
    for item in root.args() {
        match item.head() {
            Some("pad") => {
                let layers = pad_layers(item);
                let copper = layers.is_empty() || layers.iter().any(|l| l.ends_with(".Cu"));
                if copper {
                    out.pads.push(read_pad(item, &mut out.warnings)?);
                } else if layers.iter().any(|l| l.ends_with(".Paste")) {
                    windows.push(read_pad(item, &mut out.warnings)?);
                } else {
                    out.warnings
                        .push("dropped a mask-only Pad, which REDA cannot represent".into());
                }
            }
            Some("fp_text") => {
                let kind = node::text(item, 0)?;
                let text = node::text(item, 1)?;
                note_copyright(text, &mut out);
                read_text(item, kind, text, &mut out)?;
            }
            Some("property") => {
                let key = node::text(item, 0)?;
                let value = node::text(item, 1)?;
                note_copyright(value, &mut out);
                let kind = match key {
                    "Reference" => "reference",
                    "Value" => "value",
                    _ => continue,
                };
                if item.child("at").is_some() {
                    read_text(item, kind, value, &mut out)?;
                }
            }
            Some("fp_line" | "fp_rect" | "fp_circle" | "fp_arc" | "fp_poly") => {
                read_graphic(item, &mut out)?
            }
            Some("fp_curve") => out
                .warnings
                .push("skipped a footprint curve, which REDA cannot draw".into()),
            _ => {}
        }
    }
    for window in windows {
        attach_window(window, &mut out);
    }
    crate::courtyard::chain(&mut out.graphics, &mut out.warnings);
    Ok(out)
}

/// `(attr smd exclude_from_pos_files exclude_from_bom board_only dnp …)`. `dnp` is
/// ignored, because Not Fitted belongs to a placed Component. KiCad 5's `virtual` is what
/// KiCad 6 reads as board-only and excluded from both.
fn read_attributes(attr: &Sexp, out: &mut FootprintSource) {
    let words: Vec<&str> = attr.args().iter().filter_map(Sexp::text).collect();
    let virtual_ = words.contains(&"virtual");
    if virtual_ || words.contains(&"exclude_from_bom") {
        out.in_bom = false;
    }
    if virtual_ || words.contains(&"exclude_from_pos_files") {
        out.in_cpl = false;
    }
    if virtual_ || words.contains(&"board_only") {
        out.warnings.push(
            "the footprint is marked board-only, which was ignored: an import always makes a \
             Symbol and Footprint pair"
                .into(),
        );
    }
}

fn pad_layers(pad: &Sexp) -> Vec<&str> {
    pad.child("layers")
        .map(|l| l.args().iter().filter_map(Sexp::text).collect())
        .unwrap_or_default()
}

fn attach_window(window: Pad, out: &mut FootprintSource) {
    if window.shape == PadShape::ChamferedRect {
        out.warnings.push(
            "dropped a paste-only Pad: its chamfers cannot be represented by a paste window".into(),
        );
        return;
    }
    for copper in &mut out.pads {
        if copper.kind != PadKind::Smd || copper.side != window.side {
            continue;
        }
        let at = (window.at - copper.at).rotated_about(Point::default(), -copper.rotation);
        let angle = (window.rotation - copper.rotation).0.rem_euclid(180_000);
        if angle % 90_000 != 0 && window.shape != PadShape::Circle {
            continue;
        }
        let (width, height) = if angle == 90_000 {
            (window.height, window.width)
        } else {
            (window.width, window.height)
        };
        let candidate = reda_model::PasteWindow {
            at,
            width,
            height,
            corner_ratio: if matches!(window.shape, PadShape::Circle | PadShape::Oval) {
                Some(Ratio::FULL)
            } else {
                window.corner_ratio
            },
        };
        if candidate.fits(&copper.outline()) {
            if !matches!(copper.paste, Some(reda_model::Paste::Windows { .. })) {
                copper.paste = Some(reda_model::Paste::Windows { windows: vec![] });
            }
            if let Some(reda_model::Paste::Windows { windows }) = &mut copper.paste {
                windows.push(candidate);
            }
            return;
        }
    }
    out.warnings
        .push("dropped a paste-only Pad: no containing copper Pad can represent its window".into());
}

fn read_text(item: &Sexp, kind: &str, text: &str, out: &mut FootprintSource) -> Read<()> {
    let at = node::need(item, "at")?;
    let placed = |default: FootprintText| -> Read<FootprintText> {
        let height = node::font_size(item)?.unwrap_or(default.style.height);
        // Field text goes on a Silkscreen or Assembly Layer only.
        let layer = layer_of(item)
            .filter(|l| l.is_silkscreen() || l.is_assembly())
            .unwrap_or(default.layer);
        Ok(FootprintText {
            at: flip(node::point(at)?),
            rotation: node::degrees_or_zero(at, 2)?,
            style: TextStyle::of_height(height),
            layer,
            visible: !node::hidden(item),
            ..default
        })
    };
    match kind {
        "reference" => out.designator = placed(FootprintText::designator(Uuid::NIL))?,
        "value" => out.value = placed(FootprintText::value(Uuid::NIL))?,
        _ => {
            let Some(layer) = layer_of(item) else {
                return Ok(());
            };
            if node::hidden(item) {
                return Ok(());
            }
            // User text naming the reference or value is that field's text.
            let field = match text {
                "${REFERENCE}" | "%R" => Some(FootprintText::designator(Uuid::NIL)),
                "${VALUE}" | "%V" => Some(FootprintText::value(Uuid::NIL)),
                _ => None,
            };
            if let Some(field) = field.filter(|_| layer.is_silkscreen() || layer.is_assembly()) {
                let placed = placed(field)?;
                out.field_texts.push(placed);
                return Ok(());
            }
            let size = node::font_size(item)?.unwrap_or(Nm(1_000_000));
            let width = item
                .child("effects")
                .and_then(|e| e.child("font"))
                .and_then(|f| f.child("thickness"))
                .map(|t| node::mm(t, 0))
                .transpose()?
                .unwrap_or(Nm(150_000));
            out.graphics.push(GraphicSource {
                layer,
                shape: Shape::Text {
                    at: flip(node::point(at)?),
                    text: reda_model::text::escape(text).into_owned(),
                    size,
                    rotation: node::degrees_or_zero(at, 2)?,
                },
                width,
                fill: false,
            });
        }
    }
    Ok(())
}

fn read_graphic(item: &Sexp, out: &mut FootprintSource) -> Read<()> {
    let Some(layer) = layer_of(item) else {
        return Ok(());
    };
    let width = node::width(item)?.unwrap_or(Nm(120_000));
    let fill = match item.child("fill") {
        // `(fill solid)`, `(fill yes)` or `(fill (type solid))`; `none` and `no` are open.
        Some(f) => {
            let word = f.arg(0).or_else(|| f.child("type").and_then(|t| t.arg(0)));
            matches!(word, Some("solid" | "yes"))
        }
        // Polygons are filled unless they say otherwise.
        None => item.head() == Some("fp_poly"),
    };
    let shapes = match item.head() {
        Some("fp_line") => vec![Shape::Line {
            start: flipped_at(item, "start")?,
            end: flipped_at(item, "end")?,
        }],
        Some("fp_rect") => vec![Shape::Rect {
            start: flipped_at(item, "start")?,
            end: flipped_at(item, "end")?,
        }],
        Some("fp_circle") => {
            let center = flipped_at(item, "center")?;
            let end = flipped_at(item, "end")?;
            let (dx, dy) = ((end.x.0 - center.x.0) as f64, (end.y.0 - center.y.0) as f64);
            vec![Shape::Circle {
                center,
                radius: Nm(dx.hypot(dy).round() as i64),
            }]
        }
        Some("fp_arc") => vec![read_arc(item)?],
        Some("fp_poly") => {
            let points = node::points(item)?.into_iter().map(flip).collect();
            shapes::polyline(points, fill)
        }
        _ => vec![],
    };
    for shape in shapes {
        out.graphics.push(GraphicSource {
            layer,
            shape,
            width,
            fill,
        });
    }
    Ok(())
}

/// `(fp_arc (start) (mid) (end))`, or the legacy `(fp_arc (start <centre>) (end <arc
/// start>) (angle <degrees>))`, whose positive angle turns clockwise on screen.
fn read_arc(item: &Sexp) -> Read<Shape> {
    if let Some(mid) = item.child("mid") {
        return Ok(shapes::arc_through(
            flipped_at(item, "start")?,
            flip(node::point(mid)?),
            flipped_at(item, "end")?,
        ));
    }
    let angle = node::degrees_or_zero(node::need(item, "angle")?, 0)?;
    Ok(Shape::Arc {
        center: flipped_at(item, "start")?,
        start: flipped_at(item, "end")?,
        sweep: Millideg(-angle.0),
    })
}

fn read_pad(pad: &Sexp, warnings: &mut Vec<String>) -> Read<Pad> {
    let number = node::text(pad, 0)?.to_owned();
    let kind = match node::text(pad, 1)? {
        "smd" | "connect" => PadKind::Smd,
        "thru_hole" => PadKind::PlatedThroughHole,
        "np_thru_hole" => PadKind::NonPlated,
        other => return fail(pad, format!("unknown pad type {other:?}")),
    };
    let at = node::need(pad, "at")?;
    let (hole, offset) = match pad.child("drill") {
        None => (None, None),
        Some(drill) => {
            let offset = match drill.child("offset") {
                Some(o) => Some(flip(node::point(o)?)).filter(|o| *o != Point::default()),
                None => None,
            };
            let (w, h) = if drill.arg(0) == Some("oval") {
                let w = node::mm(drill, 1)?;
                let h = match drill.arg(2).and_then(crate::number::mm) {
                    Some(h) => h,
                    None => w,
                };
                (w, h)
            } else {
                let d = node::mm(drill, 0)?;
                (d, d)
            };
            ((w.0 > 0 && h.0 > 0).then_some(Hole::new(w, h)), offset)
        }
    };
    if kind != PadKind::Smd && hole.is_none() {
        return fail(pad, format!("through-hole pad {number:?} has no drill"));
    }
    let layers = pad_layers(pad);
    let side = (kind == PadKind::Smd
        && (layers.contains(&"B.Cu") || layers.contains(&"B.Paste"))
        && !layers.contains(&"F.Cu"))
    .then_some(Side::Bottom);
    let margin = |name: &str| pad.child(name).map(|m| node::mm(m, 0)).transpose();
    let zone_connection = if kind == PadKind::NonPlated {
        None
    } else {
        zone_connection(pad, kind, &number, warnings)?
    };
    let mask_expansion = margin("solder_mask_margin")?;
    let own = |present: bool| {
        if present {
            mask_expansion.map(|expansion| reda_model::MaskSide::Expansion { expansion })
        } else {
            Some(reda_model::MaskSide::Mode(reda_model::Tenting::Tent))
        }
    };
    let has = |name: &str, all: &str| layers.contains(&name) || layers.contains(&all);
    let mask = reda_model::Mask {
        top: if kind != PadKind::Smd || side != Some(Side::Bottom) {
            own(has("F.Mask", "*.Mask"))
        } else {
            None
        },
        bottom: if kind != PadKind::Smd || side == Some(Side::Bottom) {
            own(has("B.Mask", "*.Mask"))
        } else {
            None
        },
        from_hole_edge: None,
    };
    let has_paste = has(
        if side == Some(Side::Bottom) {
            "B.Paste"
        } else {
            "F.Paste"
        },
        "*.Paste",
    );
    let paste = if kind == PadKind::Smd {
        if !has_paste {
            Some(reda_model::Paste::Mode(reda_model::PasteMode::None))
        } else {
            margin("solder_paste_margin")?
                .map(|expansion| reda_model::Paste::Expansion { expansion })
        }
    } else {
        if layers.iter().any(|l| l.ends_with(".Paste")) {
            warnings.push(format!(
                "pad {number:?}: dropped paste on a through-hole Pad"
            ));
        }
        None
    };
    let stack = pad_stack::read(pad, kind, &number, offset, warnings)?;
    let copper = stack.top;
    Ok(Pad {
        uuid: Uuid::NIL,
        middle: stack.middle,
        bottom: stack.bottom,
        unused_layers: if kind == PadKind::PlatedThroughHole && pad.flag("remove_unused_layers") {
            reda_model::UnusedLayers::Remove
        } else {
            reda_model::UnusedLayers::Keep
        },
        number,
        kind,
        shape: copper.shape,
        at: flip(node::point(at)?),
        rotation: node::degrees_or_zero(at, 2)?,
        width: copper.width,
        height: copper.height,
        corner_ratio: copper.corner_ratio,
        chamfer_ratio: copper.chamfer_ratio,
        corners: copper.corners,
        offset: stack.offset,
        hole: if kind == PadKind::Smd { None } else { hole },
        side,
        mask: (mask != reda_model::Mask::default()).then_some(mask),
        paste,
        zone_connection,
    })
}

/// `(zone_connect n)` with `(thermal_bridge_width)`, `(thermal_gap)` and
/// `(thermal_bridge_angle)`, as a Zone connection; `None` when it inherits everything.
fn zone_connection(
    pad: &Sexp,
    kind: PadKind,
    number: &str,
    warnings: &mut Vec<String>,
) -> Read<Option<ZoneConnection>> {
    let style = match pad.child("zone_connect") {
        None => None,
        Some(z) => match node::text(z, 0)? {
            "0" => Some(ConnectionStyle::None),
            "1" => Some(ConnectionStyle::Relief),
            "2" => Some(ConnectionStyle::Solid),
            "3" if kind == PadKind::Smd => Some(ConnectionStyle::Solid),
            "3" => Some(ConnectionStyle::Relief),
            other => return fail(z, format!("unknown zone connection {other:?}")),
        },
    };
    let length = |name: &str| pad.child(name).map(|m| node::mm(m, 0)).transpose();
    let spoke_width = length("thermal_bridge_width")?
        .or(length("thermal_width")?)
        .filter(|w| w.0 > 0);
    let gap = length("thermal_gap")?.filter(|g| g.0 > 0);
    let angle = match pad.child("thermal_bridge_angle") {
        None => None,
        Some(a) => {
            let angle = node::degrees_or_zero(a, 0)?.normalized();
            match angle.0 {
                0 | 45_000 | 90_000 => Some(angle),
                _ => {
                    warnings.push(format!(
                        "pad {number:?}: a spoke angle other than 0, 45 or 90 degrees was dropped"
                    ));
                    None
                }
            }
        }
    };
    let connection = ZoneConnection {
        style,
        spokes: None,
        spoke_width,
        gap,
        angle,
    };
    Ok((connection != ZoneConnection::default()).then_some(connection))
}

pub fn build(
    source: FootprintSource,
    name: String,
    provenance: Provenance,
    new_id: &mut dyn FnMut() -> Uuid,
) -> Footprint {
    let uuid = new_id();
    let mut pads: Vec<Pad> = source
        .pads
        .into_iter()
        .map(|p| Pad {
            uuid: new_id(),
            ..p
        })
        .collect();
    pads.sort_by_key(|p| p.uuid);
    let mut graphics: Vec<FootprintGraphic> = source
        .graphics
        .into_iter()
        .map(|g| FootprintGraphic {
            uuid: new_id(),
            layer: g.layer,
            shape: g.shape,
            width: g.width,
            fill: g.fill,
        })
        .collect();
    graphics.sort_by_key(|g| g.uuid);
    let mut texts = vec![
        FootprintText {
            uuid: new_id(),
            ..source.designator
        },
        FootprintText {
            uuid: new_id(),
            ..source.value
        },
    ];
    texts.extend(source.field_texts.into_iter().map(|t| FootprintText {
        uuid: new_id(),
        ..t
    }));
    texts.sort_by_key(|t| t.uuid);
    Footprint {
        uuid,
        name,
        description: source.description,
        height: None,
        pads,
        graphics,
        texts,
        generator: None,
        provenance: Some(Box::new(provenance)),
    }
}
