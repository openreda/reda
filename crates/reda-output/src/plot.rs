//! Plotting a PCB Document to Gerber files: the outer copper Layers (Zone fills, Tracks,
//! Vias and Pads) and the Board Outline as the Profile.

use reda_command::Timestamp;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    GerberSettings, PadKind, PadShape, PcbDocument, Project, Side, TrackGeometry, Uuid, ZoneFills,
};
use reda_pcb::placed::{self, PlacedPad};

use crate::gerber::{Gerber, Template, escape};

/// The pen the Profile is drawn with.
const PROFILE_PEN: Nm = Nm(100_000);

/// What a job plots.
pub struct Plot<'a> {
    /// The Project, for its name, UUID and revision.
    pub project: &'a Project,
    pub pcb: &'a PcbDocument,
    /// The board's current Zone fills ([`reda_pcb::zone::current`]).
    pub fills: &'a ZoneFills,
    /// The Fab Preset's Gerber switches.
    pub gerber: GerberSettings,
    /// The creation time stamped into the files, from the injected `Clock`.
    pub created: Timestamp,
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

/// The Gerber files of `plot`: top copper, bottom copper, then the Profile.
pub fn gerber_files(plot: &Plot<'_>) -> Vec<OutputFile> {
    let name = &plot.project.props().name;
    let copper: Vec<Uuid> = plot.pcb.copper_layers().map(|l| l.uuid).collect();
    let mut files = Vec::new();
    if let (Some(&top), Some(&bottom)) = (copper.first(), copper.last()) {
        files.push(OutputFile {
            name: format!("{name}.GTL"),
            function: "copper_top",
            bytes: copper_layer(plot, top, "Copper,L1,Top".into()),
        });
        files.push(OutputFile {
            name: format!("{name}.GBL"),
            function: "copper_bottom",
            bytes: copper_layer(plot, bottom, format!("Copper,L{},Bot", copper.len())),
        });
    }
    files.push(OutputFile {
        name: format!("{name}.GKO"),
        function: "profile",
        bytes: profile(plot),
    });
    files
}

/// The file attributes every file starts with, then `function` and the polarity.
fn file_attributes(plot: &Plot<'_>, function: String) -> Vec<(&'static str, String)> {
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
        (".SameCoordinates", plot.pcb.uuid.to_string()),
        (".FileFunction", function),
        (".FilePolarity", "Positive".into()),
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

fn copper_layer(plot: &Plot<'_>, layer: Uuid, function: String) -> Vec<u8> {
    let pcb = plot.pcb;
    let mut g = Gerber::new(plot.gerber.x2_attributes);

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
        g.region(&shape.outer, true, &net_attribute(&net));
        for hole in &shape.holes {
            g.region(hole, false, &[]);
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
                flash_pad(&mut g, &pad, &attrs);
            }
        }
    }
    for free in pcb.free_pads() {
        let pad = placed::placed_free_pad(free);
        if on_layer(&pad) {
            let net = net_name(pcb, pad.net);
            flash_pad(&mut g, &pad, &net_attribute(&net));
        }
    }

    g.finish(&file_attributes(plot, function))
}

/// The aperture of a Pad: a standard one when the turn allows, otherwise a macro.
fn pad_template(pad: &PlacedPad) -> Template {
    let rot = Millideg(pad.rotation.0.rem_euclid(360_000));
    let quarter = rot.0 % 90_000 == 0;
    let (w, h) = if rot.0 % 180_000 == 0 {
        (pad.width, pad.height)
    } else {
        (pad.height, pad.width)
    };
    let radius = pad.corner_radius.unwrap_or(Nm::ZERO);
    match pad.shape {
        PadShape::Circle => Template::Circle(pad.width),
        PadShape::Oval if pad.width == pad.height => Template::Circle(pad.width),
        PadShape::Rect if quarter => Template::Rect(w, h),
        PadShape::RoundedRect if quarter && radius == Nm::ZERO => Template::Rect(w, h),
        PadShape::Oval if quarter => Template::Oval(w, h),
        PadShape::Rect => Template::RotRect(pad.width, pad.height, rot),
        PadShape::RoundedRect if radius == Nm::ZERO => {
            Template::RotRect(pad.width, pad.height, rot)
        }
        PadShape::RoundedRect => Template::RoundRect(pad.width, pad.height, radius, rot),
        PadShape::Oval if pad.width > pad.height => Template::RotOval(pad.width, pad.height, rot),
        PadShape::Oval => Template::RotOval(pad.height, pad.width, Millideg(rot.0 + 90_000)),
    }
}

fn flash_pad(g: &mut Gerber, pad: &PlacedPad, attrs: &[(&str, &str)]) {
    let function = match pad.kind {
        PadKind::Smd => "SMDPad,CuDef",
        _ => "ComponentPad",
    };
    g.flash(pad_template(pad), function, pad.at, attrs);
}

/// The Board Outline and the Board Cutouts drawn with a thin pen.
fn profile(plot: &Plot<'_>) -> Vec<u8> {
    let pcb = plot.pcb;
    let mut g = Gerber::new(plot.gerber.x2_attributes);
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
    g.finish(&file_attributes(plot, "Profile,NP".into()))
}
