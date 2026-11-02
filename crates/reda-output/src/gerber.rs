//! A Gerber X2 writer: millimetres, coordinates as `4.6` (so a coordinate is a whole
//! number of nanometres), an aperture table built as objects use it, and the X2 file,
//! aperture and object attributes. With attributes off it writes plain X1.
//!
//! Everything is integer nanometres in and exact decimal text out, so a file is byte for
//! byte the same on every machine.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use reda_model::geom::{Millideg, Nm, Point};

/// The aperture function of objects that take none, such as silkscreen drawing: their
/// apertures and regions carry no `.AperFunction`.
pub(crate) const NO_FUNCTION: &str = "";

/// An aperture shape. Rotations are CCW and taken whole by the macros, so the writer needs
/// no trigonometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    fn macro_name(self) -> Option<&'static str> {
        match self {
            Template::RotRect(..) => Some("RotRect"),
            Template::RotOval(..) => Some("RotOval"),
            Template::RoundRect(..) => Some("RoundRect"),
            _ => None,
        }
    }

    /// The `%AD` body after the D-code, such as `C,0.250000`.
    fn definition(self) -> String {
        let join = |name: &str, values: &[String]| format!("{name},{}", values.join("X"));
        match self {
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

/// The centre of the arc from `from` to `to` sweeping `sweep` (non-zero, under a full
/// turn), rounded to the nanometre. Quarter-turn multiples are exact.
pub(crate) fn arc_centre(from: Point, to: Point, sweep: Millideg) -> Point {
    let (fx, fy, tx, ty) = (from.x.0, from.y.0, to.x.0, to.y.0);
    let (dx, dy) = (tx - fx, ty - fy);
    // centre = midpoint + perpendicular(d) / 2 × cot(sweep / 2)
    let cot = match sweep.0 {
        90_000 | -270_000 => Some(1),
        180_000 | -180_000 => Some(0),
        270_000 | -90_000 => Some(-1),
        _ => None,
    };
    if let Some(k) = cot {
        let round_half = |twice: i64| twice.div_euclid(2) + i64::from(twice.rem_euclid(2) == 1);
        return Point::new(
            Nm(round_half(fx + tx - k * dy)),
            Nm(round_half(fy + ty + k * dx)),
        );
    }
    let half = (sweep.0 as f64 / 1000.0).to_radians() / 2.0;
    let k = 1.0 / half.tan();
    let cx = (fx + tx) as f64 / 2.0 - k * dy as f64 / 2.0;
    let cy = (fy + ty) as f64 / 2.0 + k * dx as f64 / 2.0;
    Point::new(Nm(cx.round() as i64), Nm(cy.round() as i64))
}

/// One file being written.
pub(crate) struct Gerber {
    x2: bool,
    /// `(template, aperture function)` → D-code, in the order of first use.
    apertures: BTreeMap<(Template, &'static str), u32>,
    order: Vec<(Template, &'static str)>,
    body: String,
    current: Option<u32>,
    /// The current point, once one is set.
    point: Option<Point>,
    mode: u8,
    dark: bool,
}

impl Gerber {
    pub(crate) fn new(x2: bool) -> Gerber {
        Gerber {
            x2,
            apertures: BTreeMap::new(),
            order: Vec::new(),
            body: String::new(),
            current: None,
            point: None,
            mode: 1,
            dark: true,
        }
    }

    /// The D-code of `template` with aperture function `function` (such as `SMDPad,CuDef`),
    /// defining it on first use, and selects it.
    fn select(&mut self, template: Template, function: &'static str) {
        let next = 10 + self.order.len() as u32;
        let code = *self.apertures.entry((template, function)).or_insert(next);
        if code == next {
            self.order.push((template, function));
        }
        if self.current != Some(code) {
            let _ = writeln!(self.body, "D{code}*");
            self.current = Some(code);
        }
    }

    /// Sets the object attributes for the next object (`(".N", "GND")`, …); the values
    /// are already escaped field by field ([`escape`]).
    fn attributes(&mut self, attributes: &[(&str, &str)]) {
        if !self.x2 {
            return;
        }
        for (name, value) in attributes {
            let _ = writeln!(self.body, "%TO{name},{value}*%");
        }
    }

    fn clear_attributes(&mut self, attributes: &[(&str, &str)]) {
        if self.x2 && !attributes.is_empty() {
            self.body.push_str("%TD*%\n");
        }
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
        self.select(template, function);
        self.attributes(attributes);
        self.at(at, 3);
        self.clear_attributes(attributes);
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
        self.polarity(true);
        self.select(Template::Circle(width), function);
        self.attributes(attributes);
        self.move_to(from);
        self.segment(from, to, sweep);
        self.clear_attributes(attributes);
    }

    /// Draws the arc from `from` to `to` about a known `centre` with a round pen of
    /// `width`, counter-clockwise when `ccw`.
    pub(crate) fn arc_about(
        &mut self,
        width: Nm,
        function: &'static str,
        from: Point,
        to: Point,
        centre: Point,
        ccw: bool,
    ) {
        self.polarity(true);
        self.select(Template::Circle(width), function);
        self.move_to(from);
        self.arc_to(from, to, centre, ccw);
    }

    /// The plot operation from the current point `from` to `to`.
    fn segment(&mut self, from: Point, to: Point, sweep: Millideg) {
        if sweep.0 == 0 {
            self.mode(1);
            self.at(to, 1);
        } else {
            self.arc_to(from, to, arc_centre(from, to, sweep), sweep.0 > 0);
        }
    }

    /// The arc plot operation from the current point `from` to `to` about `c`.
    fn arc_to(&mut self, from: Point, to: Point, c: Point, ccw: bool) {
        self.mode(if ccw { 3 } else { 2 });
        let _ = writeln!(
            self.body,
            "X{}Y{}I{}J{}D01*",
            to.x.0,
            to.y.0,
            c.x.0 - from.x.0,
            c.y.0 - from.y.0
        );
        self.point = Some(to);
    }

    /// A filled region bounded by `points` (closed back to the first), dark or clear, with
    /// the aperture function `function` ([`NO_FUNCTION`] for none).
    pub(crate) fn region(
        &mut self,
        points: &[Point],
        dark: bool,
        function: &'static str,
        attributes: &[(&str, &str)],
    ) {
        let Some(&first) = points.first() else {
            return;
        };
        self.polarity(dark);
        let tagged = self.x2 && function != NO_FUNCTION;
        if tagged {
            let _ = writeln!(self.body, "%TA.AperFunction,{function}*%");
        }
        self.attributes(attributes);
        self.body.push_str("G36*\n");
        self.at(first, 2);
        self.mode(1);
        for p in points[1..].iter().chain([&first]) {
            self.at(*p, 1);
        }
        self.body.push_str("G37*\n");
        if tagged || (self.x2 && !attributes.is_empty()) {
            self.body.push_str("%TD*%\n");
        }
    }

    /// The finished file: `file_attributes` (`(".FileFunction", "Copper,L1,Top")`, …,
    /// already escaped) in front, then the format, the macros and apertures used, the
    /// objects and the end.
    pub(crate) fn finish(mut self, file_attributes: &[(&str, String)]) -> Vec<u8> {
        self.polarity(true);
        let mut out = String::new();
        if self.x2 {
            for (name, value) in file_attributes {
                let _ = writeln!(out, "%TF{name},{value}*%");
            }
        }
        out.push_str("%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\nG75*\nG01*\n");
        for (name, body) in MACROS {
            if self.order.iter().any(|(t, _)| t.macro_name() == Some(name)) {
                let _ = writeln!(out, "%AM{name}*\n{body}*%");
            }
        }
        for (i, (template, function)) in self.order.iter().enumerate() {
            let tagged = self.x2 && *function != NO_FUNCTION;
            if tagged {
                let _ = writeln!(out, "%TA.AperFunction,{function}*%");
            }
            let _ = writeln!(out, "%ADD{}{}*%", 10 + i, template.definition());
            if tagged {
                out.push_str("%TD*%\n");
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

    #[test]
    fn arc_centres_of_quarter_turns_are_exact() {
        let p = |x: i64, y: i64| Point::new(Nm(x), Nm(y));
        assert_eq!(arc_centre(p(10, 0), p(0, 10), Millideg(90_000)), p(0, 0));
        assert_eq!(arc_centre(p(0, 10), p(10, 0), Millideg(-90_000)), p(0, 0));
        assert_eq!(arc_centre(p(-10, 0), p(10, 0), Millideg(180_000)), p(0, 0));
        assert_eq!(arc_centre(p(10, 0), p(0, -10), Millideg(270_000)), p(0, 0));
        assert_eq!(
            arc_centre(p(1_000_000, 0), p(500_000, 866_025), Millideg(60_000)),
            p(0, 0)
        );
    }
}
