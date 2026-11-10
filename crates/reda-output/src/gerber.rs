//! A Gerber X2 writer: millimetres, coordinates as `4.6` (so a coordinate is a whole
//! number of nanometres), an aperture table built as objects use it, and the X2 file,
//! aperture and object attributes. With attributes off it writes plain X1.
//!
//! Everything is integer nanometres in and exact decimal text out, so a file is byte for
//! byte the same on every machine.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use reda_model::geom::{Millideg, Nm, Point, arc_centre};

pub(crate) mod contour;

/// The aperture function of objects that take none, such as silkscreen drawing: their
/// apertures and regions carry no `.AperFunction`.
pub(crate) const NO_FUNCTION: &str = "";

/// An aperture shape. Rotations are CCW and taken whole by the macros, so the writer needs
/// no trigonometry.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Template {
    /// A disc of this diameter.
    Circle(Nm),
    /// An axis-aligned rectangle, width × height.
    Rect(Nm, Nm),
    /// An axis-aligned obround, width × height.
    Oval(Nm, Nm),
    /// A rectangle turned by an angle that is not a quarter turn.
    RotRect(Nm, Nm, Millideg),
    /// An obround turned by an angle, its long side along the turned x axis.
    RotOval(Nm, Nm, Millideg),
    /// A rounded rectangle (width, height, corner radius), turned by an angle.
    RoundRect(Nm, Nm, Nm, Millideg),
    /// A shape of its own, written as a macro generated for it: a polygon with discs,
    /// about the flash point, turned by an angle.
    Shape(MacroShape),
}

/// The primitives of a generated macro: a filled polygon through `outline` (at least three
/// points, counter-clockwise) and filled discs `(centre, diameter)`, all turned by
/// `rotation` about the flash point.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MacroShape {
    pub outline: Vec<Point>,
    pub discs: Vec<(Point, Nm)>,
    pub rotation: Millideg,
}

impl MacroShape {
    /// The macro body: an outline primitive, then a circle primitive per disc.
    fn body(&self) -> String {
        let rot = degrees(self.rotation);
        let mut parts = Vec::new();
        let mut outline = format!("4,1,{}", self.outline.len());
        for p in self.outline.iter().chain(self.outline.first()) {
            let _ = write!(outline, ",{},{}", mm(p.x), mm(p.y));
        }
        let _ = write!(outline, ",{rot}");
        parts.push(outline);
        for (c, d) in &self.discs {
            parts.push(format!("1,1,{},{},{},{rot}", mm(*d), mm(c.x), mm(c.y)));
        }
        parts.join("*\n")
    }
}

/// The aperture macros, written only when a file uses them.
const MACROS: [(&str, &str); 3] = [
    // $1 width, $2 height, $3 rotation.
    ("RotRect", "21,1,$1,$2,0,0,$3"),
    // $1 straight length, $2 diameter, $3 half the length, $4 minus half, $5 rotation.
    (
        "RotOval",
        "21,1,$1,$2,0,0,$5*\n1,1,$2,$3,0,$5*\n1,1,$2,$4,0,$5",
    ),
    // $1 width, $2 height less the corners, $3 width less the corners, $4 height,
    // $5 corner diameter, $6/$7 corner centre, $8/$9 its negatives, $10 rotation.
    (
        "RoundRect",
        "21,1,$1,$2,0,0,$10*\n21,1,$3,$4,0,0,$10*\n1,1,$5,$6,$7,$10*\n1,1,$5,$8,$7,$10*\n\
         1,1,$5,$8,$9,$10*\n1,1,$5,$6,$9,$10",
    ),
];

impl Template {
    fn rotated(self, angle: Millideg) -> Self {
        if angle == Millideg::ZERO {
            return self;
        }
        match self {
            Self::Circle(d) => Self::Circle(d),
            Self::Rect(w, h) => Self::RotRect(w, h, angle),
            Self::Oval(w, h) if w >= h => Self::RotOval(w, h, angle),
            Self::Oval(w, h) => Self::RotOval(h, w, Millideg(angle.0 + 90_000)),
            Self::RotRect(w, h, rot) => Self::RotRect(w, h, rot + angle),
            Self::RotOval(w, h, rot) => Self::RotOval(w, h, rot + angle),
            Self::RoundRect(w, h, r, rot) => Self::RoundRect(w, h, r, rot + angle),
            Self::Shape(mut shape) => {
                shape.rotation = shape.rotation + angle;
                Self::Shape(shape)
            }
        }
    }

    fn macro_name(&self) -> Option<&'static str> {
        match self {
            Template::RotRect(..) => Some("RotRect"),
            Template::RotOval(..) => Some("RotOval"),
            Template::RoundRect(..) => Some("RoundRect"),
            _ => None,
        }
    }

    /// The `%AD` body after the D-code, such as `C,0.250000`. A [`Template::Shape`] is
    /// the name of its generated macro, `shape`.
    fn definition(&self, shape: &str) -> String {
        let join = |name: &str, values: &[String]| format!("{name},{}", values.join("X"));
        match *self {
            Template::Shape(_) => shape.to_string(),
            Template::Circle(d) => join("C", &[mm(d)]),
            Template::Rect(w, h) => join("R", &[mm(w), mm(h)]),
            Template::Oval(w, h) => join("O", &[mm(w), mm(h)]),
            Template::RotRect(w, h, rot) => join("RotRect", &[mm(w), mm(h), degrees(rot)]),
            Template::RotOval(w, h, rot) => {
                let len = Nm(w.0 - h.0);
                join(
                    "RotOval",
                    &[
                        mm(len),
                        mm(h),
                        half_mm(len.0),
                        half_mm(-len.0),
                        degrees(rot),
                    ],
                )
            }
            Template::RoundRect(w, h, r, rot) => {
                let (cx, cy) = (w.0 - 2 * r.0, h.0 - 2 * r.0);
                join(
                    "RoundRect",
                    &[
                        mm(w),
                        mm(Nm(cy)),
                        mm(Nm(cx)),
                        mm(h),
                        mm(Nm(2 * r.0)),
                        half_mm(cx),
                        half_mm(cy),
                        half_mm(-cx),
                        half_mm(-cy),
                        degrees(rot),
                    ],
                )
            }
        }
    }
}

/// `nm` as millimetres with six decimals: `1.250000`, `-0.000100`.
pub(crate) fn mm(nm: Nm) -> String {
    let sign = if nm.0 < 0 { "-" } else { "" };
    let a = nm.0.unsigned_abs();
    format!("{sign}{}.{:06}", a / 1_000_000, a % 1_000_000)
}

/// Half of `nm` as millimetres, exact: a seventh decimal `5` when `nm` is odd.
fn half_mm(nm: i64) -> String {
    let sign = if nm < 0 { "-" } else { "" };
    let a = nm.unsigned_abs();
    let whole = mm(Nm((a / 2) as i64));
    if a.is_multiple_of(2) {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}5")
    }
}

/// An angle as degrees in `[0, 360)` with three decimals: `45.000`.
pub(crate) fn degrees(angle: Millideg) -> String {
    let a = angle.0.rem_euclid(360_000);
    format!("{}.{:03}", a / 1000, a % 1000)
}

/// Escapes the characters Gerber reserves in attribute values.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '%' | '*' | ',' | '\\' => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// The largest coordinate a Gerber file holds: integers must fit 32 bits, so
/// ±2147.483647 mm at six decimals (Gerber specification §3.4.1).
pub const MAX_COORDINATE: Nm = Nm(i32::MAX as i64);

/// Arcs shorter than this are written as draws (Gerber specification §4.7.2).
const MIN_ARC: f64 = 2_000.0;

/// One file being written.
pub(crate) struct Gerber {
    x2: bool,
    transform: crate::InstanceTransform,
    /// `(template, aperture function)` → D-code, in the order of first use.
    apertures: BTreeMap<(Template, &'static str), u32>,
    order: Vec<(Template, &'static str)>,
    body: String,
    current: Option<u32>,
    /// The current point, once one is set.
    point: Option<Point>,
    mode: u8,
    dark: bool,
    /// The object attributes in force, as last set.
    object_attributes: Vec<(String, String)>,
    /// How many dark objects (flashes, draws, regions) the file holds: clear objects put
    /// no ink down, so a file of only clear objects is empty.
    objects: usize,
    /// The first point written that the format can't hold, in output coordinates.
    out_of_range: Option<Point>,
}

/// An attribute command, `%TO.N,GND*%`, or with X2 attributes off the same as a
/// comment, `G04 #@! TO.N,GND*`, so readers that take them still get the whole set.
fn attribute(out: &mut String, x2: bool, command: &str) {
    if x2 {
        let _ = writeln!(out, "%{command}*%");
    } else {
        let _ = writeln!(out, "G04 #@! {command}*");
    }
}

impl Gerber {
    pub(crate) fn new(x2: bool) -> Gerber {
        Gerber {
            x2,
            transform: Default::default(),
            apertures: BTreeMap::new(),
            order: Vec::new(),
            body: String::new(),
            current: None,
            point: None,
            mode: 1,
            dark: true,
            object_attributes: Vec::new(),
            objects: 0,
            out_of_range: None,
        }
    }

    pub(crate) fn set_transform(&mut self, transform: crate::InstanceTransform) {
        self.transform = transform;
    }

    /// Whether no dark object has been plotted yet.
    pub(crate) fn is_empty(&self) -> bool {
        self.objects == 0
    }

    /// The first point plotted outside ±[`MAX_COORDINATE`], in output coordinates.
    pub(crate) fn out_of_range(&self) -> Option<Point> {
        self.out_of_range
    }

    fn check(&mut self, p: Point) {
        let outside = |v: Nm| v.0.abs() > MAX_COORDINATE.0;
        if self.out_of_range.is_none() && (outside(p.x) || outside(p.y)) {
            self.out_of_range = Some(p);
        }
    }

    /// The D-code of `template` with aperture function `function` (such as `SMDPad,CuDef`),
    /// defining it on first use, and selects it.
    fn select(&mut self, template: Template, function: &'static str) {
        let next = 10 + self.order.len() as u32;
        let code = *self
            .apertures
            .entry((template.clone(), function))
            .or_insert(next);
        if code == next {
            self.order.push((template, function));
        }
        if self.current != Some(code) {
            let _ = writeln!(self.body, "D{code}*");
            self.current = Some(code);
        }
    }

    /// Sets the object attributes of the next object (`(".N", "GND")`, …; the values
    /// already escaped field by field, [`escape`]). Attributes stay in force until they
    /// change, so a run of objects with the same ones sets them once.
    fn attributes(&mut self, attributes: &[(&str, &str)]) {
        let wanted: Vec<(String, String)> = attributes
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect();
        if wanted == self.object_attributes {
            return;
        }
        if !self.object_attributes.is_empty() {
            attribute(&mut self.body, self.x2, "TD");
        }
        for (name, value) in &wanted {
            attribute(&mut self.body, self.x2, &format!("TO{name},{value}"));
        }
        self.object_attributes = wanted;
    }

    fn polarity(&mut self, dark: bool) {
        if self.dark != dark {
            self.body
                .push_str(if dark { "%LPD*%\n" } else { "%LPC*%\n" });
            self.dark = dark;
        }
    }

    fn mode(&mut self, mode: u8) {
        if self.mode != mode {
            let _ = writeln!(self.body, "G0{mode}*");
            self.mode = mode;
        }
    }

    fn at(&mut self, p: Point, op: u8) {
        self.check(p);
        let _ = writeln!(self.body, "X{}Y{}D0{op}*", p.x.0, p.y.0);
        self.point = Some(p);
    }

    /// Moves to `p` unless the current point is already there.
    fn move_to(&mut self, p: Point) {
        if self.point != Some(p) {
            self.at(p, 2);
        }
    }

    /// Flashes `template` at `at`.
    pub(crate) fn flash(
        &mut self,
        template: Template,
        function: &'static str,
        at: Point,
        attributes: &[(&str, &str)],
    ) {
        self.polarity(true);
        self.select(template.rotated(self.transform.rotation), function);
        self.attributes(attributes);
        self.at(self.transform.point(at), 3);
        self.objects += 1;
    }

    /// Flashes `template` at `at` in clear polarity, removing what is under it.
    pub(crate) fn flash_clear(&mut self, template: Template, at: Point) {
        self.polarity(false);
        self.select(template.rotated(self.transform.rotation), NO_FUNCTION);
        self.attributes(&[]);
        self.at(self.transform.point(at), 3);
    }

    /// Draws a straight line (`sweep` zero) or an arc with a round pen of `width`.
    pub(crate) fn draw(
        &mut self,
        width: Nm,
        function: &'static str,
        from: Point,
        to: Point,
        sweep: Millideg,
        attributes: &[(&str, &str)],
    ) {
        let from = self.transform.point(from);
        let to = self.transform.point(to);
        self.polarity(true);
        self.select(Template::Circle(width), function);
        self.attributes(attributes);
        self.move_to(from);
        self.segment(from, to, sweep);
        self.objects += 1;
    }

    /// Draws the arc between `ends` (from, to) about a known `centre` with a round pen
    /// of `width`, counter-clockwise when `ccw`.
    pub(crate) fn arc_about(
        &mut self,
        width: Nm,
        function: &'static str,
        (from, to): (Point, Point),
        centre: Point,
        ccw: bool,
        attributes: &[(&str, &str)],
    ) {
        let from = self.transform.point(from);
        let to = self.transform.point(to);
        self.polarity(true);
        self.select(Template::Circle(width), function);
        self.attributes(attributes);
        self.move_to(from);
        self.arc_to(from, to, self.transform.point(centre), ccw);
        self.objects += 1;
    }

    /// The plot operation from the current point `from` to `to`.
    fn segment(&mut self, from: Point, to: Point, sweep: Millideg) {
        if sweep.0 == 0 {
            self.line_to(to);
        } else {
            self.arc_to(from, to, arc_centre(from, to, sweep), sweep.0 > 0);
        }
    }

    fn line_to(&mut self, to: Point) {
        self.mode(1);
        self.at(to, 1);
    }

    /// The arc plot operation from the current point `from` to `to` about `c`. An arc
    /// shorter than 2 µm is a straight draw instead, and one whose ends are under 1 µm
    /// apart (a near-full or full turn) is cut in two at its middle, because readers
    /// can't tell either from a tiny arc or a full circle (Gerber specification §4.7.2).
    fn arc_to(&mut self, from: Point, to: Point, c: Point, ccw: bool) {
        let (fx, fy) = ((from.x - c.x).0 as f64, (from.y - c.y).0 as f64);
        let (tx, ty) = ((to.x - c.x).0 as f64, (to.y - c.y).0 as f64);
        let mut sweep = (fx * ty - fy * tx).atan2(fx * tx + fy * ty);
        if !ccw {
            sweep = -sweep;
        }
        if sweep <= 0.0 {
            sweep += std::f64::consts::TAU;
        }
        if fx.hypot(fy) * sweep < MIN_ARC {
            self.line_to(to);
            return;
        }
        if contour::close_points(from, to) {
            let half = sweep / 2.0 * if ccw { 1.0 } else { -1.0 };
            let half = Millideg((half.to_degrees() * 1000.0).round() as i64);
            let mid = from.rotated_about(c, half);
            self.arc_to(from, mid, c, ccw);
            self.arc_to(mid, to, c, ccw);
            return;
        }
        self.mode(if ccw { 3 } else { 2 });
        let (i, j) = (c.x - from.x, c.y - from.y);
        self.check(to);
        self.check(Point::new(i, j));
        let _ = writeln!(self.body, "X{}Y{}I{}J{}D01*", to.x.0, to.y.0, i.0, j.0);
        self.point = Some(to);
    }

    /// A filled region bounded by `points` (closed back to the first), dark or clear, with
    /// the aperture function `function` ([`NO_FUNCTION`] for none). Points under 1 µm
    /// apart are merged; a region left with no area is not written.
    pub(crate) fn region(
        &mut self,
        points: &[Point],
        dark: bool,
        function: &'static str,
        attributes: &[(&str, &str)],
    ) {
        let points: Vec<_> = points.iter().map(|p| self.transform.point(*p)).collect();
        self.contour(&contour::merged(&points), dark, function, attributes);
    }

    /// A dark region of `outer` less `holes`, written as one contour with the holes
    /// joined by cut-ins, so it erases nothing underneath.
    pub(crate) fn region_with_holes(
        &mut self,
        outer: &[Point],
        holes: &[Vec<Point>],
        function: &'static str,
        attributes: &[(&str, &str)],
    ) {
        let place = |ring: &[Point]| -> Vec<Point> {
            ring.iter().map(|p| self.transform.point(*p)).collect()
        };
        let holes: Vec<Vec<Point>> = holes.iter().map(|h| place(h)).collect();
        let contour = contour::joined(&place(outer), &holes);
        self.contour(&contour, true, function, attributes);
    }

    /// A filled disc of `diameter` about `centre`: a region of two half-turn arcs, so a
    /// filled circle is never a flash (only Pads are flashed on copper).
    pub(crate) fn disc(
        &mut self,
        centre: Point,
        diameter: Nm,
        function: &'static str,
        attributes: &[(&str, &str)],
    ) {
        let c = self.transform.point(centre);
        let r = Nm(diameter.0 / 2);
        if r <= Nm::ZERO {
            return;
        }
        let (east, west) = (Point::new(c.x + r, c.y), Point::new(c.x - r, c.y));
        self.open_region(true, function, attributes);
        self.at(east, 2);
        self.arc_to(east, west, c, true);
        self.arc_to(west, east, c, true);
        self.close_region(function);
    }

    fn contour(
        &mut self,
        points: &[Point],
        dark: bool,
        function: &'static str,
        attributes: &[(&str, &str)],
    ) {
        let Some(&first) = points.first() else {
            return;
        };
        self.open_region(dark, function, attributes);
        self.at(first, 2);
        self.mode(1);
        for p in points[1..].iter().chain([&first]) {
            self.at(*p, 1);
        }
        self.close_region(function);
    }

    fn open_region(&mut self, dark: bool, function: &'static str, attributes: &[(&str, &str)]) {
        self.polarity(dark);
        // Object attributes first: changing them clears every attribute in force.
        self.attributes(attributes);
        if function != NO_FUNCTION {
            attribute(
                &mut self.body,
                self.x2,
                &format!("TA.AperFunction,{function}"),
            );
        }
        self.body.push_str("G36*\n");
    }

    fn close_region(&mut self, function: &'static str) {
        self.body.push_str("G37*\n");
        if function != NO_FUNCTION {
            attribute(&mut self.body, self.x2, "TD.AperFunction");
        }
        if self.dark {
            self.objects += 1;
        }
    }

    /// The finished file: `file_attributes` (`(".FileFunction", "Copper,L1,Top")`, …,
    /// already escaped) in front, then the format, the macros and apertures used, the
    /// objects and the end.
    pub(crate) fn finish(mut self, file_attributes: &[(&str, String)]) -> Vec<u8> {
        self.polarity(true);
        let mut out = String::new();
        for (name, value) in file_attributes {
            attribute(&mut out, self.x2, &format!("TF{name},{value}"));
        }
        out.push_str("%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\nG75*\nG01*\n");
        for (name, body) in MACROS {
            if self.order.iter().any(|(t, _)| t.macro_name() == Some(name)) {
                let _ = writeln!(out, "%AM{name}*\n{body}*%");
            }
        }
        // Generated macros are numbered in the order their apertures were first used.
        let mut shapes = 0;
        for (i, (template, function)) in self.order.iter().enumerate() {
            let mut name = String::new();
            if let Template::Shape(shape) = template {
                name = format!("Shape{shapes}");
                shapes += 1;
                let _ = writeln!(out, "%AM{name}*\n{}*%", shape.body());
            }
            let tagged = *function != NO_FUNCTION;
            if tagged {
                attribute(&mut out, self.x2, &format!("TA.AperFunction,{function}"));
            }
            let _ = writeln!(out, "%ADD{}{}*%", 10 + i, template.definition(&name));
            if tagged {
                attribute(&mut out, self.x2, "TD");
            }
        }
        out.push_str(&self.body);
        out.push_str("M02*\n");
        out.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_exact_decimals() {
        assert_eq!(mm(Nm(1_250_000)), "1.250000");
        assert_eq!(mm(Nm(-100)), "-0.000100");
        assert_eq!(half_mm(3), "0.0000015");
        assert_eq!(half_mm(-4), "-0.000002");
        assert_eq!(degrees(Millideg(-90_000)), "270.000");
        assert_eq!(escape("a,b*c%d\\"), "a\\u002Cb\\u002Ac\\u0025d\\u005C");
    }
}
