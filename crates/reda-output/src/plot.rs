//! Plotting a PCB Document to Gerber files: the outer copper Layers (Zone fills, Tracks,
//! Vias, Pads and copper text and drawing), the Solder Mask and Paste ([`openings`]), the
//! Silkscreen ([`artwork`]) and the Board Outline as the Profile.

mod artwork;
mod openings;

use reda_command::{Fonts, Timestamp};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    GerberSettings, LayerKind, PadKind, PcbDocument, Project, Side, TrackGeometry, Uuid, ZoneFills,
};
use reda_pcb::placed::{self, PlacedPad};

use crate::gerber::{Gerber, MacroShape, NO_FUNCTION, Template, escape};

/// The pen the Profile is drawn with.
const PROFILE_PEN: Nm = Nm(100_000);

/// What a job plots. All geometry writers consume the same board instances.
pub struct Plot<'a> {
    pub project: &'a Project,
    pub instances: &'a [crate::BoardInstance<'a>],
    pub gerber: &'a GerberSettings,
    pub fonts: &'a Fonts,
    pub created: Timestamp,
}

/// Per-board geometry context; the writer owns the instance transform.
pub(crate) struct BoardPlot<'a> {
    pub pcb: &'a PcbDocument,
    pub fills: &'a ZoneFills,
    pub fonts: &'a Fonts,
}

/// One written file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputFile {
    /// The file name, such as `Demo.GTL`.
    pub name: String,
    /// What it holds, such as `copper_top` or `profile`.
    pub function: &'static str,
    pub bytes: Vec<u8>,
}

/// The Gerber files of `plot`: every copper Layer top to bottom, Solder Mask, Paste and Silkscreen,
/// then the Profile.
pub fn gerber_files(plot: &Plot<'_>) -> Vec<OutputFile> {
    let Some(first) = plot.instances.first() else {
        return Vec::new();
    };
    let name = &plot.project.props().name;
    let file = |extension: &str, function, bytes| OutputFile {
        name: format!("{name}.{extension}"),
        function,
        bytes,
    };
    let mut files = Vec::new();
    let copper = first.pcb.copper_layers().count();
    for i in 0..copper {
        let number = i + 1;
        let (extension, function, attribute) = match i {
            0 => ("GTL".to_owned(), "copper_top", "Top"),
            i if i + 1 == copper => ("GBL".to_owned(), "copper_bottom", "Bot"),
            i => (format!("G{i}"), "copper_inner", "Inr"),
        };
        let bytes = render(
            plot,
            format!("Copper,L{number},{attribute}"),
            "Positive",
            |board, g| {
                if let Some(layer) = board.pcb.copper_layers().nth(i) {
                    copper_layer(board, layer.uuid, g);
                }
            },
        );
        files.push(file(&extension, function, bytes));
    }
    for (extension, function, side) in [
        ("GTS", "solder_mask_top", Side::Top),
        ("GBS", "solder_mask_bottom", Side::Bottom),
    ] {
        let bytes = render(
            plot,
            format!("Soldermask,{}", openings::side_name(side)),
            "Negative",
            |board, g| openings::mask(board, side, g),
        );
        files.push(file(extension, function, bytes));
    }
    for (extension, function, side) in [
        ("GTP", "paste_top", Side::Top),
        ("GBP", "paste_bottom", Side::Bottom),
    ] {
        let bytes = render(
            plot,
            format!("Paste,{}", openings::side_name(side)),
            "Positive",
            |board, g| openings::paste(board, side, g),
        );
        files.push(file(extension, function, bytes));
    }
    for (extension, function, side) in [
        ("GTO", "silkscreen_top", Side::Top),
        ("GBO", "silkscreen_bottom", Side::Bottom),
    ] {
        if first
            .pcb
            .layers()
            .find(LayerKind::Silkscreen, Some(side))
            .is_some()
        {
            let bytes = render(
                plot,
                format!("Legend,{}", openings::side_name(side)),
                "Positive",
                |board, g| {
                    if let Some(layer) = board.pcb.layers().find(LayerKind::Silkscreen, Some(side))
                    {
                        artwork::silkscreen(board, layer.uuid, side, g);
                    }
                },
            );
            files.push(file(extension, function, bytes));
        }
    }
    files.push(file(
        "GKO",
        "profile",
        render(plot, "Profile,NP".into(), "Positive", profile),
    ));
    files
}

fn render(
    plot: &Plot<'_>,
    function: String,
    polarity: &str,
    draw: impl Fn(&BoardPlot<'_>, &mut Gerber),
) -> Vec<u8> {
    let mut g = Gerber::new(plot.gerber.x2_attributes);
    for instance in plot.instances {
        g.set_transform(instance.transform);
        draw(
            &BoardPlot {
                pcb: instance.pcb,
                fills: instance.fills,
                fonts: plot.fonts,
            },
            &mut g,
        );
    }
    g.finish(&file_attributes_with(plot, function, polarity))
}

/// The file attributes of a positive image: what every file starts with, then `function`
/// and the polarity.
pub(crate) fn file_attributes(plot: &Plot<'_>, function: String) -> Vec<(&'static str, String)> {
    file_attributes_with(plot, function, "Positive")
}

/// The file attributes every file starts with, then `function` and `polarity`.
fn file_attributes_with(
    plot: &Plot<'_>,
    function: String,
    polarity: &str,
) -> Vec<(&'static str, String)> {
    let props = plot.project.props();
    let revision = match props.properties.revision.as_str() {
        "" => "-",
        r => r,
    };
    vec![
        (".GenerationSoftware", "openreda,REDA".into()),
        (".CreationDate", creation_date(plot.created)),
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
        (".FilePolarity", polarity.into()),
    ]
}

/// `ms` since the Unix epoch as an ISO 8601 UTC time, `2026-01-01T00:00:00+00:00`.
fn creation_date(created: Timestamp) -> String {
    let c = crate::civil::Civil::of(created);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
        c.year, c.month, c.day, c.hour, c.minute, c.second
    )
}

/// The `.N` attribute value of `net`, escaped; `None` for no Net.
fn net_name(pcb: &PcbDocument, net: Option<Uuid>) -> Option<String> {
    net.and_then(|n| pcb.net(n)).map(|n| escape(&n.name))
}

/// The `.N` object attribute of `net`, if it has one.
fn net_attribute(net: &Option<String>) -> Vec<(&'static str, &str)> {
    net.as_deref().map(|n| (".N", n)).into_iter().collect()
}

/// Twice the signed area of a ring (positive when counter-clockwise).
fn twice_area(ring: &[Point]) -> i128 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            i128::from(a.x.0) * i128::from(b.y.0) - i128::from(b.x.0) * i128::from(a.y.0)
        })
        .sum()
}

fn copper_layer(plot: &BoardPlot<'_>, layer: Uuid, g: &mut Gerber) {
    let pcb = plot.pcb;
    // Zone fills first, biggest outline first, each shape's holes cleared right after it:
    // copper inside a hole is smaller than the hole, so it is drawn after the clearing.
    let mut shapes: Vec<(i128, Option<String>, &reda_model::FillShape)> = Vec::new();
    let fills = reda_pcb::zone::layer_fills(pcb, plot.fills);
    for fill in fills.iter().filter(|f| f.layer == layer) {
        for shape in &fill.shapes {
            let area = twice_area(&shape.outer).abs();
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
    for instance in pcb.footprints() {
        for pad in placed::placed_pads(pcb, instance) {
            if on_layer(&pad) {
                let pin = format!("{},{}", escape(&instance.designator), escape(&pad.number));
                let net = net_name(pcb, pad.net);
                let mut attrs = vec![(".P", pin.as_str())];
                attrs.extend(net_attribute(&net));
                flash_pad(g, &pad, &attrs);
            }
        }
    }
    for free in pcb.free_pads() {
        let pad = placed::placed_free_pad(free);
        if on_layer(&pad) {
            let net = net_name(pcb, pad.net);
            flash_pad(g, &pad, &net_attribute(&net));
        }
    }

    // Copper text and drawing carry no current.
    artwork::draw_layer(g, plot, layer, "NonConductor");
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

fn flash_pad(g: &mut Gerber, pad: &PlacedPad, attrs: &[(&str, &str)]) {
    let function = match pad.kind {
        PadKind::Smd => "SMDPad,CuDef",
        PadKind::PlatedThroughHole => "ComponentPad",
        // Only the Solder Mask opens over a non-plated hole; no pad function fits it.
        PadKind::NonPlated => NO_FUNCTION,
    };
    let (template, at) = pad_template(pad);
    g.flash(template, function, at, attrs);
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
