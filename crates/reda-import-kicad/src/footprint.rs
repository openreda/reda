//! Reading a `.kicad_mod`, in either the legacy `(module …)` or the `(footprint …)`
//! syntax, and building the REDA Footprint.
//!
//! KiCad footprint y points down and REDA's points up, so every y is negated. Graphics
//! on Layers REDA's Footprints do not have (fabrication notes, user drawings, edge cuts)
//! are left out; their copyright lines are still kept.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    Footprint, FootprintGraphic, FootprintLayer, FootprintText, Hole, Pad, PadKind, PadShape,
    Provenance, Shape, TextStyle, Uuid,
};

use crate::node::{self, Read, fail};
use crate::provenance::is_copyright;
use crate::sexpr::Sexp;
use crate::shapes;

#[derive(Clone, Debug)]
pub struct PadSource {
    pub number: String,
    pub kind: PadKind,
    pub shape: PadShape,
    pub at: Point,
    pub rotation: Millideg,
    pub width: Nm,
    pub height: Nm,
    pub corner_radius: Option<Nm>,
    pub hole: Option<Hole>,
    pub mask_margin: Option<Nm>,
    pub paste_margin: Option<Nm>,
}

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
    pub pads: Vec<PadSource>,
    pub graphics: Vec<GraphicSource>,
    pub designator: FootprintText,
    pub value: FootprintText,
    /// Copyright lines from the footprint's texts, properties and description.
    pub copyright: Vec<String>,
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
        copyright: vec![],
        warnings: vec![],
    };
    let note_copyright = |text: &str, out: &mut FootprintSource| {
        if is_copyright(text) && !out.copyright.iter().any(|c| c == text) {
            out.copyright.push(text.to_owned());
        }
    };
    note_copyright(&description, &mut out);
    for item in root.args() {
        match item.head() {
            Some("pad") => out.pads.push(read_pad(item, &mut out.warnings)?),
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
    Ok(out)
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
                    text: text.to_owned(),
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

fn read_pad(pad: &Sexp, warnings: &mut Vec<String>) -> Read<PadSource> {
    let number = node::text(pad, 0)?.to_owned();
    let kind = match node::text(pad, 1)? {
        "smd" | "connect" => PadKind::Smd,
        "thru_hole" => PadKind::PlatedThroughHole,
        "np_thru_hole" => PadKind::NonPlated,
        other => return fail(pad, format!("unknown pad type {other:?}")),
    };
    let shape_word = node::text(pad, 2)?;
    let shape = match shape_word {
        "circle" => PadShape::Circle,
        "rect" => PadShape::Rect,
        "oval" => PadShape::Oval,
        "roundrect" => PadShape::RoundedRect,
        "trapezoid" => {
            warnings.push(format!(
                "pad {number:?}: a trapezoid was imported as a rectangle"
            ));
            PadShape::Rect
        }
        "custom" => {
            let anchor = pad
                .child("options")
                .and_then(|o| o.child("anchor"))
                .and_then(|a| a.arg(0));
            warnings.push(format!(
                "pad {number:?}: a custom shape was imported as its anchor pad"
            ));
            match anchor {
                Some("circle") => PadShape::Circle,
                _ => PadShape::Rect,
            }
        }
        other => return fail(pad, format!("unknown pad shape {other:?}")),
    };
    let at = node::need(pad, "at")?;
    let size = node::need(pad, "size")?;
    let width = node::mm(size, 0)?;
    let height = if shape == PadShape::Circle {
        width
    } else {
        node::mm(size, 1)?
    };
    if width.0 <= 0 || height.0 <= 0 {
        return fail(size, format!("pad {number:?} has no size"));
    }
    let corner_radius = match (shape, pad.child("roundrect_rratio")) {
        (PadShape::RoundedRect, Some(r)) => {
            let ratio =
                crate::number::fixed(node::text(r, 0)?, 6).filter(|r| (0..=500_000).contains(r));
            let Some(ratio) = ratio else {
                return fail(r, "the corner ratio must be between 0 and 0.5");
            };
            let short = i128::from(width.0.min(height.0));
            Some(Nm((short * i128::from(ratio) / 1_000_000) as i64))
        }
        (PadShape::RoundedRect, None) => Some(Nm(width.0.min(height.0) / 4)),
        _ => None,
    };
    let hole = match pad.child("drill") {
        None => None,
        Some(drill) => {
            if drill.child("offset").is_some() {
                warnings.push(format!("pad {number:?}: the drill offset was dropped"));
            }
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
            (w.0 > 0 && h.0 > 0).then_some(Hole {
                width: w,
                height: h,
            })
        }
    };
    if kind != PadKind::Smd && hole.is_none() {
        return fail(pad, format!("through-hole pad {number:?} has no drill"));
    }
    let layers: Vec<&str> = pad
        .child("layers")
        .map(|l| l.args().iter().filter_map(Sexp::text).collect())
        .unwrap_or_default();
    if kind == PadKind::Smd && layers.contains(&"B.Cu") && !layers.contains(&"F.Cu") {
        warnings.push(format!(
            "pad {number:?}: a bottom-side SMD pad was imported on the top"
        ));
    }
    let margin = |name: &str| pad.child(name).map(|m| node::mm(m, 0)).transpose();
    Ok(PadSource {
        number,
        kind,
        shape,
        at: flip(node::point(at)?),
        rotation: node::degrees_or_zero(at, 2)?,
        width,
        height,
        corner_radius,
        hole: if kind == PadKind::Smd { None } else { hole },
        mask_margin: margin("solder_mask_margin")?,
        paste_margin: margin("solder_paste_margin")?,
    })
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
            number: p.number,
            kind: p.kind,
            shape: p.shape,
            at: p.at,
            rotation: p.rotation,
            width: p.width,
            height: p.height,
            corner_radius: p.corner_radius,
            hole: p.hole,
            mask_margin: p.mask_margin,
            paste_margin: p.paste_margin,
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
