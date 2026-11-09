//! Plotting a PCB Document to Gerber files: the copper Layers (Zone fills, Tracks, Vias,
//! Pads and copper text and drawing), the Solder Mask and Paste ([`openings`]), the
//! Silkscreen ([`artwork`]) and the Board Outline as the Profile, with the aperture
//! functions and attributes of `outputs.md`.

mod artwork;
mod openings;

use reda_command::Fonts;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    FileNaming, GerberSettings, LayerKind, NotFittedPaste, PadKind, PcbDocument, Project, Side, TrackGeometry, Uuid, ZoneFills,
};
use reda_pcb::openings::{self as resolved, Kind};
use reda_pcb::placed::{self, PlacedPad};

use crate::gerber::{Gerber, MacroShape, Template, escape};

/// The pen the Profile is drawn with.
const PROFILE_PEN: Nm = Nm(100_000);

/// What a job plots. All geometry writers consume the same board instances.
pub struct Plot<'a> {
    pub project: &'a Project,
    pub instances: &'a [crate::BoardInstance<'a>],
    pub gerber: &'a GerberSettings,
    pub fonts: &'a Fonts,
    /// The REDA version the files name as their generator, such as `0.1.0`; tests inject
    /// a fixed one so goldens survive releases.
    pub version: &'a str,
    /// What the files are a part of (`.Part`).
    pub part: Part,
    /// Whether Not Fitted Footprints keep their paste openings (the job's
    /// `not_fitted_paste`). Mask and copper never change.
    pub not_fitted_paste: NotFittedPaste,
}

/// The Gerber `.Part` file attribute: what the files describe. v0.1 writes single boards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Part {
    /// One PCB.
    #[default]
    Single,
}

impl Part {
    fn attribute(self) -> &'static str {
        match self {
            Part::Single => "Single",
        }
    }
}

/// Per-board geometry context; the writer owns the instance transform.
pub(crate) struct BoardPlot<'a> {
    pub pcb: &'a PcbDocument,
    pub fills: &'a ZoneFills,
    pub fonts: &'a Fonts,
    pub not_fitted_paste: NotFittedPaste,
}

/// One written file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputFile {
    /// The file name, such as `Demo.gtl`.
    pub name: String,
    /// What it holds, such as `copper_top` or `profile`.
    pub function: &'static str,
    pub bytes: Vec<u8>,
    /// The `.FileFunction` and `.FilePolarity` of a Gerber or drill file, which the Gerber
    /// job file lists; `None` for other files.
    pub attributes: Option<FileAttributes>,
}

/// A fabrication file's `.FileFunction` and `.FilePolarity`, as its attributes state them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileAttributes {
    /// Such as `Copper,L1,Top` or `Plated,1,2,PTH,Drill`.
    pub function: String,
    /// `Positive` or `Negative`.
    pub polarity: &'static str,
}

/// A file a run leaves out because it would hold nothing: an empty Silkscreen or Paste, or
/// a drill file with no holes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedFile {
    /// The name it would have had, such as `Demo.gbp`.
    pub name: String,
    /// What it would have held, such as `paste_bottom`.
    pub function: &'static str,
}

/// The Gerber files of a plot, and the empty ones left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gerbers {
    pub files: Vec<OutputFile>,
    pub skipped: Vec<SkippedFile>,
}

/// A coordinate the Gerber format can't hold (beyond ±2147.483647 mm, Gerber
/// specification §3.4.1), which blocks the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinateOutOfRange {
    /// The file it was found in, such as `Demo.gtl`.
    pub file: String,
    /// The point, in output coordinates (after the origin).
    pub at: Point,
}

/// The Gerber files of `plot`: every copper Layer top to bottom, Solder Mask, Paste and
/// Silkscreen, then the Profile. Copper, Solder Mask and the Profile are always written,
/// because a missing mask file could read as "no mask"; an empty Silkscreen or Paste is
/// left out and listed in `skipped`. A coordinate beyond the format's range is an error.
pub fn gerber_files(plot: &Plot<'_>) -> Result<Gerbers, CoordinateOutOfRange> {
    let mut out = Gerbers {
        files: Vec::new(),
        skipped: Vec::new(),
    };
    let Some(first) = plot.instances.first() else {
        return Ok(out);
    };
    let project = &plot.project.props().name;
    let mut add = |file: FileName, function, rendered: Rendered, optional: bool| {
        let name = file.name(project, plot.gerber);
        if let Some(at) = rendered.out_of_range {
            return Err(CoordinateOutOfRange { file: name, at });
        }
        if optional && rendered.empty {
            out.skipped.push(SkippedFile { name, function });
        } else {
            out.files.push(OutputFile {
                name,
                function,
                bytes: rendered.bytes,
                attributes: Some(rendered.attributes),
            });
        }
        Ok(())
    };
    let copper = first.pcb.copper_layers().count();
    for i in 0..copper {
        let number = i + 1;
        let (file, function, attribute) = match i {
            0 => (FileName::new("gtl", "Top Copper"), "copper_top", "Top"),
            i if i + 1 == copper => (
                FileName::new("gbl", "Bottom Copper"),
                "copper_bottom",
                "Bot",
            ),
            i => (
                FileName {
                    extension: format!("g{i}"),
                    layer: format!("Inner Copper L{number}"),
                },
                "copper_inner",
                "Inr",
            ),
        };
        let rendered = render(
            plot,
            format!("Copper,L{number},{attribute}"),
            "Positive",
            |board, g| {
                if let Some(layer) = board.pcb.copper_layers().nth(i) {
                    copper_layer(board, layer.uuid, g);
                }
            },
        );
        add(file, function, rendered, false)?;
    }
    for (extension, function, side) in [
        ("gts", "solder_mask_top", Side::Top),
        ("gbs", "solder_mask_bottom", Side::Bottom),
    ] {
        let rendered = render(
            plot,
            format!("Soldermask,{}", openings::side_name(side)),
            "Negative",
            |board, g| openings::mask(board, side, g),
        );
        add(FileName::sided(extension, side, "Solder Mask"), function, rendered, false)?;
    }
    for (extension, function, side) in [
        ("gtp", "paste_top", Side::Top),
        ("gbp", "paste_bottom", Side::Bottom),
    ] {
        let rendered = render(
            plot,
            format!("Paste,{}", openings::side_name(side)),
            "Positive",
            |board, g| openings::paste(board, side, g),
        );
        add(FileName::sided(extension, side, "Paste"), function, rendered, true)?;
    }
    for (extension, function, side) in [
        ("gto", "silkscreen_top", Side::Top),
        ("gbo", "silkscreen_bottom", Side::Bottom),
    ] {
        let rendered = render(
            plot,
            format!("Legend,{}", openings::side_name(side)),
            "Positive",
            |board, g| {
                if let Some(layer) = board.pcb.layers().find(LayerKind::Silkscreen, Some(side)) {
                    artwork::silkscreen(board, layer.uuid, side, g);
                }
            },
        );
        add(FileName::sided(extension, side, "Silkscreen"), function, rendered, true)?;
    }
    let rendered = render(plot, "Profile,NP".into(), "Positive", profile);
    add(FileName::new("gm1", "Board Outline"), "profile", rendered, false)?;
    Ok(out)
}

/// What a Gerber file's name is made from: its classic extension and its Layer's name.
struct FileName {
    /// Lowercase, such as `gtl`.
    extension: String,
    /// Such as "Top Copper"; what `file_names` and `descriptive` names go by.
    layer: String,
}

impl FileName {
    fn new(extension: &str, layer: &str) -> FileName {
        FileName {
            extension: extension.into(),
            layer: layer.into(),
        }
    }

    fn sided(extension: &str, side: Side, kind: &str) -> FileName {
        let side = match side {
            Side::Top => "Top",
            Side::Bottom => "Bottom",
        };
        FileName {
            extension: extension.into(),
            layer: format!("{side} {kind}"),
        }
    }

    /// The file's name under `settings`: a `file_names` entry for its Layer, else
    /// `<project>.<ext>` (`classic`) or `<project>-<Layer>.gbr` with spaces as
    /// underscores (`descriptive`).
    fn name(&self, project: &str, settings: &GerberSettings) -> String {
        if let Some(own) = settings.file_names.iter().find(|f| f.layer == self.layer) {
            return own.name.clone();
        }
        match settings.naming {
            FileNaming::Classic => format!("{project}.{}", self.extension),
            FileNaming::Descriptive => format!("{project}-{}.gbr", self.layer.replace(' ', "_")),
        }
    }
}

/// One plotted file: its bytes, whether it plots nothing of its own (the Board Outline
/// that `outline_on_every_plot` adds aside), and any coordinate out of range.
struct Rendered {
    bytes: Vec<u8>,
    attributes: FileAttributes,
    empty: bool,
    out_of_range: Option<Point>,
}

fn render(
    plot: &Plot<'_>,
    function: String,
    polarity: &'static str,
    draw: impl Fn(&BoardPlot<'_>, &mut Gerber),
) -> Rendered {
    let is_profile = function.starts_with("Profile");
    let mut g = Gerber::new(plot.gerber.x2_attributes);
    let boards = || {
        plot.instances.iter().map(|instance| {
            (
                instance.transform,
                BoardPlot {
                    pcb: instance.pcb,
                    fills: instance.fills,
                    fonts: plot.fonts,
                    not_fitted_paste: plot.not_fitted_paste,
                },
            )
        })
    };
    for (transform, board) in boards() {
        g.set_transform(transform);
        draw(&board, &mut g);
    }
    let empty = g.is_empty();
    if plot.gerber.outline_on_every_plot && !is_profile {
        for (transform, board) in boards() {
            g.set_transform(transform);
            profile(&board, &mut g);
        }
    }
    let out_of_range = g.out_of_range();
    Rendered {
        bytes: g.finish(&gerber_attributes(plot, function.clone(), polarity)),
        attributes: FileAttributes { function, polarity },
        empty,
        out_of_range,
    }
}

/// The file attributes every fabrication file starts with: the generator with REDA's
/// version, the Project and the coordinate arrangement. No `.CreationDate`: the same
/// design and REDA version give the same bytes.
pub(crate) fn file_attributes(plot: &Plot<'_>, function: String) -> Vec<(&'static str, String)> {
    let props = plot.project.props();
    let revision = match props.properties.revision.as_str() {
        "" => "-",
        r => r,
    };
    vec![
        (
            ".GenerationSoftware",
            format!("openreda,REDA,{}", escape(plot.version)),
        ),
        (
            ".ProjectId",
            format!(
                "{},{},{}",
                escape(&props.name),
                props.uuid,
                escape(revision)
            ),
        ),
        (
            ".SameCoordinates",
            crate::instance::coordinates_id(plot.instances).to_string(),
        ),
        (".FileFunction", function),
    ]
}

/// A Gerber file's attributes: the common ones, then `.Part` and the polarity.
fn gerber_attributes(
    plot: &Plot<'_>,
    function: String,
    polarity: &str,
) -> Vec<(&'static str, String)> {
    let mut attributes = file_attributes(plot, function);
    attributes.insert(3, (".Part", plot.part.attribute().into()));
    attributes.push((".FilePolarity", polarity.into()));
    attributes
}

/// The `.N` attribute value of `net`, escaped; `None` for no Net.
fn net_name(pcb: &PcbDocument, net: Option<Uuid>) -> Option<String> {
    net.and_then(|n| pcb.net(n)).map(|n| escape(&n.name))
}

/// The `.N` object attribute every copper object carries: empty for no Net.
fn net_attribute(net: &Option<String>) -> Vec<(&'static str, &str)> {
    vec![(".N", net.as_deref().unwrap_or(""))]
}

/// The attributes of copper that belongs to no Net: text, drawing.
pub(crate) const NO_NET: [(&str, &str); 1] = [(".N", "")];

fn copper_layer(plot: &BoardPlot<'_>, layer: Uuid, g: &mut Gerber) {
    let pcb = plot.pcb;
    // Zone fills first, biggest outline first, each shape's holes cleared right after it:
    // copper inside a hole is smaller than the hole, so it is drawn after the clearing.
    let mut shapes: Vec<(i128, Option<String>, &reda_model::FillShape)> = Vec::new();
    let fills = reda_pcb::zone::layer_fills(pcb, plot.fills);
    for fill in fills.iter().filter(|f| f.layer == layer) {
        for shape in &fill.shapes {
            let area = crate::gerber::contour::twice_area(&shape.outer).abs();
            shapes.push((area, net_name(pcb, fill.net), shape));
        }
    }
    shapes.sort_by_key(|s| std::cmp::Reverse(s.0));
    for (_, net, shape) in shapes {
        g.region(&shape.outer, true, "Conductor", &net_attribute(&net));
        for hole in &shape.holes {
            g.region(hole, false, "Conductor", &[]);
        }
    }

    for track in pcb.tracks().filter(|t| t.layer == layer) {
        let net = net_name(pcb, track.net);
        let (from, to, sweep) = match track.geometry {
            TrackGeometry::Line { from, to } => (from, to, Millideg(0)),
            TrackGeometry::Arc { from, to, sweep } => (from, to, sweep),
        };
        g.draw(
            track.width,
            "Conductor",
            from,
            to,
            sweep,
            &net_attribute(&net),
        );
    }

    for via in pcb.vias() {
        if pcb.span_layers(via.span).contains(&layer) {
            let net = net_name(pcb, via.net);
            g.flash(
                Template::Circle(via.diameter),
                "ViaPad",
                via.at,
                &net_attribute(&net),
            );
        }
    }

    let copper: Vec<Uuid> = pcb.copper_layers().map(|l| l.uuid).collect();
    let on_layer = |pad: &PlacedPad| match pad.kind {
        PadKind::NonPlated => false,
        PadKind::PlatedThroughHole => true,
        PadKind::Smd => {
            let outer = match pad.side.unwrap_or(Side::Top) {
                Side::Top => copper.first(),
                Side::Bottom => copper.last(),
            };
            outer == Some(&layer)
        }
    };
    let mask_defined = mask_defined_pads(pcb);
    for instance in pcb.footprints() {
        for pad in placed::placed_pads(pcb, instance) {
            if on_layer(&pad) {
                let pin = format!("{},{}", escape(&instance.designator), escape(&pad.number));
                let net = net_name(pcb, pad.net);
                let mut attrs = vec![(".P", pin.as_str())];
                attrs.extend(net_attribute(&net));
                let function = match pad.kind {
                    PadKind::Smd if mask_defined.contains(&(instance.uuid, pad.pad)) => {
                        "SMDPad,SMDef"
                    }
                    PadKind::Smd => "SMDPad,CuDef",
                    _ => "ComponentPad",
                };
                let (template, at) = pad_template(&pad);
                g.flash(template, function, at, &attrs);
            }
        }
    }
    for free in pcb.free_pads() {
        let pad = placed::placed_free_pad(free);
        if on_layer(&pad) {
            let net = net_name(pcb, pad.net);
            let (template, at) = pad_template(&pad);
            g.flash(template, "OtherPad,FreePad", at, &net_attribute(&net));
        }
    }

    // Copper text and drawing carry no current.
    artwork::draw_layer(g, plot, layer, "NonConductor", &NO_NET);
}

/// The Footprint SMD Pads, as `(Footprint, Pad)`, whose resolved Solder Mask opening lies
/// inside the copper on all sides: the mask, not the copper, defines what gets solder.
fn mask_defined_pads(pcb: &PcbDocument) -> std::collections::BTreeSet<(Uuid, Uuid)> {
    resolved::resolve(pcb)
        .into_iter()
        .filter(|o| o.kind == Kind::Mask && o.copper.kind == PadKind::Smd)
        .filter_map(|o| {
            let opening = o.geometry?;
            let [footprint, pad] = o.objects[..] else {
                return None;
            };
            inside_on_all_sides(&o.copper, &opening).then_some((footprint, pad))
        })
        .collect()
}

/// Whether `opening` is `copper` shrunk on every side: the same frame and shape, grown
/// by one negative amount.
fn inside_on_all_sides(copper: &PlacedPad, opening: &PlacedPad) -> bool {
    let (c, o) = (&copper.outline, &opening.outline);
    let shrink = Nm((o.width.0 - c.width.0) / 2);
    shrink < Nm::ZERO
        && o.height.0 - c.height.0 == 2 * shrink.0
        && copper.at == opening.at
        && copper.rotation == opening.rotation
        && c.grown(shrink).as_ref() == Some(o)
}

/// The aperture of a Pad and where it is flashed: a standard aperture at the copper's
/// centre when the turn allows, a fixed macro when the corners are all alike, and
/// otherwise one generated macro flashed at the anchor.
fn pad_template(pad: &PlacedPad) -> (Template, Point) {
    let o = &pad.outline;
    let rot = Millideg(pad.rotation.0.rem_euclid(360_000));
    let quarter = rot.0 % 90_000 == 0;
    let (w, h) = (o.width, o.height);
    let (tw, th) = if rot.0 % 180_000 == 0 { (w, h) } else { (h, w) };
    let centre = pad.copper_center();
    let Some(radius) = o.uniform_radius() else {
        let shape = MacroShape {
            outline: o.contour().iter().map(|v| v.point()).collect(),
            discs: o
                .round_corners()
                .into_iter()
                .map(|(c, r)| (c, Nm(2 * r.0)))
                .collect(),
            rotation: rot,
        };
        return (Template::Shape(shape), pad.at);
    };
    let oval = radius == Nm(w.0.min(h.0) / 2);
    let template = match () {
        _ if o.is_circle() => Template::Circle(w),
        _ if radius == Nm::ZERO && quarter => Template::Rect(tw, th),
        _ if radius == Nm::ZERO => Template::RotRect(w, h, rot),
        _ if oval && quarter => Template::Oval(tw, th),
        _ if oval && w > h => Template::RotOval(w, h, rot),
        _ if oval => Template::RotOval(h, w, Millideg(rot.0 + 90_000)),
        _ => Template::RoundRect(w, h, radius, rot),
    };
    (template, centre)
}

/// The Board Outline and the Board Cutouts drawn with a thin pen.
fn profile(plot: &BoardPlot<'_>, g: &mut Gerber) {
    let pcb = plot.pcb;
    let contours = pcb
        .outline()
        .map(|o| &o.vertices)
        .into_iter()
        .chain(pcb.cutouts().map(|c| &c.vertices));
    for vertices in contours {
        for (i, v) in vertices.iter().enumerate() {
            let next = vertices[(i + 1) % vertices.len()];
            g.draw(
                PROFILE_PEN,
                "Profile",
                v.point(),
                next.point(),
                v.sweep,
                &[],
            );
        }
    }
}
