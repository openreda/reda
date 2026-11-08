//! A small, permissive Gerber reader for the read-back tests: it turns a file back into
//! flashes, draws and regions in nanometres, evaluating the standard apertures and the
//! macro primitives REDA writes (circle and centre line), so tests can compare the plot
//! with the model without trusting the writer's own code.

#![allow(dead_code)]

use std::collections::BTreeMap;

/// An aperture's shape, in nm.
#[derive(Clone, Debug, PartialEq)]
pub enum Aperture {
    Circle(f64),
    Rect(f64, f64),
    Oval(f64, f64),
    /// A macro, evaluated to its primitives.
    Macro(Vec<Primitive>),
}

/// One macro primitive, in nm, with the rotation (degrees, CCW about the macro origin).
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    Circle {
        d: f64,
        x: f64,
        y: f64,
        rot: f64,
    },
    CenterLine {
        w: f64,
        h: f64,
        x: f64,
        y: f64,
        rot: f64,
    },
    /// A filled polygon through `points` (the closing point left out).
    Outline {
        points: Vec<(f64, f64)>,
        rot: f64,
    },
}

/// Whether `p` is inside the polygon `points`, by the even-odd rule, its edge included.
fn in_polygon(points: &[(f64, f64)], p: (f64, f64)) -> bool {
    let n = points.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        // On the edge counts as inside.
        let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
        let within = p.0 >= a.0.min(b.0) - 0.5
            && p.0 <= a.0.max(b.0) + 0.5
            && p.1 >= a.1.min(b.1) - 0.5
            && p.1 <= a.1.max(b.1) + 0.5;
        if within && cross.abs() <= 0.5 * (b.0 - a.0).hypot(b.1 - a.1) {
            return true;
        }
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) {
            inside = !inside;
        }
    }
    inside
}

fn rotated(p: (f64, f64), deg: f64) -> (f64, f64) {
    let (s, c) = deg.to_radians().sin_cos();
    (p.0 * c - p.1 * s, p.0 * s + p.1 * c)
}

impl Aperture {
    /// Whether the point `(dx, dy)` from the flash point is inside the flashed shape.
    pub fn contains(&self, dx: f64, dy: f64) -> bool {
        let in_rect = |w: f64, h: f64, x: f64, y: f64| x.abs() <= w / 2.0 && y.abs() <= h / 2.0;
        let in_oval = |w: f64, h: f64, x: f64, y: f64| {
            let (long, short, a, b) = if w >= h { (w, h, x, y) } else { (h, w, y, x) };
            let half = (long - short) / 2.0;
            let a = (a.abs() - half).max(0.0);
            a.hypot(b) <= short / 2.0
        };
        match self {
            Aperture::Circle(d) => dx.hypot(dy) <= d / 2.0,
            Aperture::Rect(w, h) => in_rect(*w, *h, dx, dy),
            Aperture::Oval(w, h) => in_oval(*w, *h, dx, dy),
            Aperture::Macro(prims) => prims.iter().any(|p| match *p {
                Primitive::Circle { d, x, y, rot } => {
                    let (cx, cy) = rotated((x, y), rot);
                    (dx - cx).hypot(dy - cy) <= d / 2.0
                }
                Primitive::CenterLine { w, h, x, y, rot } => {
                    let (lx, ly) = rotated((dx, dy), -rot);
                    in_rect(w, h, lx - x, ly - y)
                }
                Primitive::Outline { ref points, rot } => {
                    in_polygon(points, rotated((dx, dy), -rot))
                }
            }),
        }
    }
}

/// An arc's centre and direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    pub centre: (i64, i64),
    pub ccw: bool,
}

/// One graphical object of the file.
#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    Flash {
        /// Dark (adds) or clear (removes).
        dark: bool,
        at: (i64, i64),
        aperture: Aperture,
        function: Option<String>,
        attributes: BTreeMap<String, String>,
    },
    Draw {
        from: (i64, i64),
        to: (i64, i64),
        arc: Option<Arc>,
        aperture: Aperture,
        function: Option<String>,
        attributes: BTreeMap<String, String>,
    },
    Region {
        /// Dark (adds copper) or clear (removes it).
        dark: bool,
        /// The contour's points, from the start point round to it again.
        points: Vec<(i64, i64)>,
        /// The arcs among the contour's segments: `(index of the segment's end point in
        /// points, arc)`.
        arcs: Vec<(usize, Arc)>,
        /// The `.AperFunction` aperture attribute in force when the region closed.
        function: Option<String>,
        attributes: BTreeMap<String, String>,
    },
}

/// A read file.
#[derive(Debug, Default)]
pub struct Layer {
    /// The `%TF` file attributes, by name (`.FileFunction` → `Copper,L1,Top`).
    pub file: BTreeMap<String, String>,
    pub objects: Vec<Object>,
    pub ended: bool,
    /// The `%FS…%` coordinate format, such as `LAX46Y46`.
    pub format: String,
    /// The `%MO…%` unit, `MM` or `IN`.
    pub unit: String,
    /// The attributes written as `G04 #@! …` comments rather than commands (X2 off).
    pub comment_attributes: usize,
}

impl Layer {
    pub fn flashes(&self) -> impl Iterator<Item = &Object> {
        self.objects
            .iter()
            .filter(|o| matches!(o, Object::Flash { .. }))
    }

    pub fn draws(&self) -> impl Iterator<Item = &Object> {
        self.objects
            .iter()
            .filter(|o| matches!(o, Object::Draw { .. }))
    }

    pub fn regions(&self) -> impl Iterator<Item = &Object> {
        self.objects
            .iter()
            .filter(|o| matches!(o, Object::Region { .. }))
    }
}

/// Splits the file into its commands: an extended command `%…%` gives one item per
/// `*`-ended block, the others one item each.
fn commands(text: &str) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if c.is_whitespace() {
            rest = &rest[c.len_utf8()..];
        } else if c == '%' {
            let end = rest[1..].find('%').expect("unclosed %") + 1;
            let inner = &rest[1..end];
            let blocks: Vec<String> = inner
                .split('*')
                .map(|b| b.split_whitespace().collect::<String>())
                .filter(|b| !b.is_empty())
                .collect();
            if blocks.first().is_some_and(|b| b.starts_with("AM")) {
                out.push((true, blocks.join("*")));
            } else {
                out.extend(blocks.into_iter().map(|b| (true, b)));
            }
            rest = &rest[end + 1..];
        } else {
            let end = rest.find('*').expect("unended command");
            out.push((false, rest[..end].trim().to_string()));
            rest = &rest[end + 1..];
        }
    }
    out
}

/// Gerber's `\uXXXX` escapes undone.
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("\\u") {
        out.push_str(&rest[..i]);
        let code = u32::from_str_radix(&rest[i + 2..i + 6], 16).unwrap();
        out.push(char::from_u32(code).unwrap());
        rest = &rest[i + 6..];
    }
    out.push_str(rest);
    out
}

/// A region's points and arcs so far.
type OpenRegion = (Vec<(i64, i64)>, Vec<(usize, Arc)>);

struct Reader {
    decimals: u32,
    inch: bool,
    macros: BTreeMap<String, Vec<Vec<String>>>,
    apertures: BTreeMap<u32, (Aperture, Option<String>)>,
    aperture_attributes: BTreeMap<String, String>,
    object_attributes: BTreeMap<String, String>,
    current: Option<u32>,
    point: (i64, i64),
    mode: u32,
    dark: bool,
    /// The open region's points and arcs.
    region: Option<OpenRegion>,
    layer: Layer,
}

impl Reader {
    fn mm(&self, value: &str) -> f64 {
        let v: f64 = value
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("number {value}"));
        if self.inch {
            v * 25_400_000.0
        } else {
            v * 1_000_000.0
        }
    }

    fn coordinate(&self, digits: &str) -> i64 {
        let v: i64 = digits.parse().unwrap();
        let scale = 10f64.powi(self.decimals as i32);
        let unit = if self.inch { 25_400_000.0 } else { 1_000_000.0 };
        (v as f64 / scale * unit).round() as i64
    }

    fn extended(&mut self, cmd: &str) {
        if let Some(fs) = cmd.strip_prefix("FS") {
            self.layer.format = fs.to_string();
            let x = fs.find('X').unwrap();
            self.decimals = fs[x + 2..x + 3].parse().unwrap();
        } else if let Some(mo) = cmd.strip_prefix("MO") {
            self.inch = mo == "IN";
            self.layer.unit = mo.to_string();
        } else if let Some(lp) = cmd.strip_prefix("LP") {
            self.dark = lp == "D";
        } else if let Some(am) = cmd.strip_prefix("AM") {
            let mut blocks = am.split('*');
            let name = blocks.next().unwrap().to_string();
            let prims = blocks
                .filter(|b| !b.starts_with('0'))
                .map(|b| b.split(',').map(str::to_string).collect())
                .collect();
            self.macros.insert(name, prims);
        } else if let Some(ad) = cmd.strip_prefix("ADD") {
            let digits = ad.chars().take_while(char::is_ascii_digit).count();
            let code: u32 = ad[..digits].parse().unwrap();
            let rest = &ad[digits..];
            let (name, params) = rest.split_once(',').unwrap_or((rest, ""));
            let params: Vec<f64> = params
                .split('X')
                .filter(|p| !p.is_empty())
                .map(|p| p.parse().unwrap())
                .collect();
            let unit = if self.inch { 25_400_000.0 } else { 1_000_000.0 };
            let aperture = match name {
                "C" => Aperture::Circle(params[0] * unit),
                "R" => Aperture::Rect(params[0] * unit, params[1] * unit),
                "O" => Aperture::Oval(params[0] * unit, params[1] * unit),
                _ => {
                    let body = self
                        .macros
                        .get(name)
                        .unwrap_or_else(|| panic!("macro {name}"));
                    let value = |s: &str| -> f64 {
                        let (neg, s) = s.strip_prefix('-').map_or((false, s), |s| (true, s));
                        let v = match s.strip_prefix('$') {
                            Some(n) => params[n.parse::<usize>().unwrap() - 1],
                            None => s.parse().unwrap(),
                        };
                        if neg { -v } else { v }
                    };
                    let prims = body
                        .iter()
                        .map(|p| match p[0].as_str() {
                            "1" => Primitive::Circle {
                                d: value(&p[2]) * unit,
                                x: value(&p[3]) * unit,
                                y: value(&p[4]) * unit,
                                rot: p.get(5).map_or(0.0, |r| value(r)),
                            },
                            "21" => Primitive::CenterLine {
                                w: value(&p[2]) * unit,
                                h: value(&p[3]) * unit,
                                x: value(&p[4]) * unit,
                                y: value(&p[5]) * unit,
                                rot: value(&p[6]),
                            },
                            "4" => {
                                let n: usize = p[2].parse().unwrap();
                                let points = (0..n)
                                    .map(|i| {
                                        (value(&p[3 + 2 * i]) * unit, value(&p[4 + 2 * i]) * unit)
                                    })
                                    .collect();
                                Primitive::Outline {
                                    points,
                                    rot: value(&p[5 + 2 * n]),
                                }
                            }
                            other => panic!("macro primitive {other}"),
                        })
                        .collect();
                    Aperture::Macro(prims)
                }
            };
            let function = self.aperture_attributes.get(".AperFunction").cloned();
            self.apertures.insert(code, (aperture, function));
        } else if let Some(tf) = cmd.strip_prefix("TF") {
            let (name, value) = tf.split_once(',').unwrap_or((tf, ""));
            self.layer.file.insert(name.into(), unescape(value));
        } else if let Some(ta) = cmd.strip_prefix("TA") {
            let (name, value) = ta.split_once(',').unwrap_or((ta, ""));
            self.aperture_attributes
                .insert(name.into(), unescape(value));
        } else if let Some(to) = cmd.strip_prefix("TO") {
            let (name, value) = to.split_once(',').unwrap_or((to, ""));
            self.object_attributes.insert(name.into(), unescape(value));
        } else if let Some(td) = cmd.strip_prefix("TD") {
            if td.is_empty() {
                self.aperture_attributes.clear();
                self.object_attributes.clear();
            } else {
                self.aperture_attributes.remove(td);
                self.object_attributes.remove(td);
            }
        } else {
            panic!("unknown extended command {cmd}");
        }
    }

    fn word(&mut self, cmd: &str) {
        let mut rest = cmd;
        let (mut x, mut y, mut i, mut j) = (None, None, 0, 0);
        while !rest.is_empty() {
            let letter = rest.chars().next().unwrap();
            let len = rest[1..]
                .find(|c: char| c.is_ascii_alphabetic())
                .map_or(rest.len(), |n| n + 1);
            let value = &rest[1..len];
            match letter {
                'G' => match value.parse::<u32>().unwrap() {
                    n @ 1..=3 => self.mode = n,
                    36 => self.region = Some((Vec::new(), Vec::new())),
                    37 => {
                        let (points, arcs) = self.region.take().unwrap();
                        self.layer.objects.push(Object::Region {
                            dark: self.dark,
                            points,
                            arcs,
                            function: self.aperture_attributes.get(".AperFunction").cloned(),
                            attributes: self.object_attributes.clone(),
                        });
                    }
                    4 => return, // comment
                    _ => {}
                },
                'X' => x = Some(self.coordinate(value)),
                'Y' => y = Some(self.coordinate(value)),
                'I' => i = self.coordinate(value),
                'J' => j = self.coordinate(value),
                'D' => {
                    let code: u32 = value.parse().unwrap();
                    if code <= 3 {
                        // REDA's dialect writes X and Y on every operation.
                        assert!(x.is_some() && y.is_some(), "X and Y missing in {cmd}");
                    }
                    let to = (x.unwrap_or(self.point.0), y.unwrap_or(self.point.1));
                    let arc = (self.mode != 1).then(|| Arc {
                        centre: (self.point.0 + i, self.point.1 + j),
                        ccw: self.mode == 3,
                    });
                    match code {
                        1 => {
                            if let Some((region, arcs)) = &mut self.region {
                                if region.is_empty() {
                                    region.push(self.point);
                                }
                                region.push(to);
                                if let Some(arc) = arc {
                                    arcs.push((region.len() - 1, arc));
                                }
                            } else {
                                let (aperture, function) =
                                    self.apertures[&self.current.unwrap()].clone();
                                self.layer.objects.push(Object::Draw {
                                    from: self.point,
                                    to,
                                    arc,
                                    aperture,
                                    function,
                                    attributes: self.object_attributes.clone(),
                                });
                            }
                        }
                        2 => {
                            if let Some((region, _)) = &mut self.region {
                                assert!(region.is_empty(), "one contour per region");
                            }
                        }
                        3 => {
                            let (aperture, function) =
                                self.apertures[&self.current.unwrap()].clone();
                            self.layer.objects.push(Object::Flash {
                                dark: self.dark,
                                at: to,
                                aperture,
                                function,
                                attributes: self.object_attributes.clone(),
                            });
                        }
                        n => {
                            assert!(self.apertures.contains_key(&n), "undefined D{n}");
                            self.current = Some(n);
                        }
                    }
                    self.point = to;
                }
                'M' => self.layer.ended = true,
                other => panic!("unknown word {other} in {cmd}"),
            }
            rest = &rest[len..];
        }
    }
}

/// The winding number of `ring` (closed or not) about `q`: non-zero when the region
/// fills `q`. Cut-in pairs cancel out.
pub fn winding(ring: &[(i64, i64)], q: (i64, i64)) -> i32 {
    let mut w = 0;
    let n = ring.len();
    for k in 0..n {
        let (a, b) = (ring[k], ring[(k + 1) % n]);
        let side =
            (b.0 - a.0) as i128 * (q.1 - a.1) as i128 - (b.1 - a.1) as i128 * (q.0 - a.0) as i128;
        if a.1 <= q.1 && b.1 > q.1 && side > 0 {
            w += 1;
        } else if a.1 > q.1 && b.1 <= q.1 && side < 0 {
            w -= 1;
        }
    }
    w
}

/// Reads a Gerber file.
pub fn read(bytes: &[u8]) -> Layer {
    let text = std::str::from_utf8(bytes).expect("Gerber is ASCII");
    let mut reader = Reader {
        decimals: 6,
        inch: false,
        macros: BTreeMap::new(),
        apertures: BTreeMap::new(),
        aperture_attributes: BTreeMap::new(),
        object_attributes: BTreeMap::new(),
        current: None,
        point: (0, 0),
        mode: 1,
        dark: true,
        region: None,
        layer: Layer::default(),
    };
    for (extended, cmd) in commands(text) {
        if extended {
            reader.extended(&cmd);
        } else if let Some(attribute) = cmd.strip_prefix("G04 #@! ") {
            // An attribute written as a comment, for readers without X2.
            reader.layer.comment_attributes += 1;
            reader.extended(attribute);
        } else if !cmd.starts_with("G04") {
            reader.word(&cmd);
        }
    }
    reader.layer
}
