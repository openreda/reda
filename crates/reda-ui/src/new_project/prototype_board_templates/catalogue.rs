//! PROTOTYPE (throwaway, #398): a hand-coded stand-in for the built-in Board Template
//! catalogue. Each template's frame is computed in Rust here, not loaded from a
//! `.redaboard` file, and the numbers are rough: close to the #397 decisions, good enough
//! to judge the picker and preview, never to fab from.

use std::collections::BTreeMap;

use egui::{Pos2, pos2};

/// One parameter shown in the dialog.
#[derive(Clone, Debug)]
pub enum Param {
    Choice {
        key: &'static str,
        label: &'static str,
        options: Vec<(&'static str, &'static str)>,
        default: &'static str,
        /// Shown only while this other choice has one of these options.
        when: Option<(&'static str, &'static [&'static str])>,
    },
    Length {
        key: &'static str,
        label: &'static str,
        default: f32,
        min: f32,
        max: f32,
    },
}

impl Param {
    pub fn key(&self) -> &'static str {
        match self {
            Param::Choice { key, .. } | Param::Length { key, .. } => key,
        }
    }

    pub fn default_text(&self) -> String {
        match self {
            Param::Choice { default, .. } => (*default).to_owned(),
            Param::Length { default, .. } => trim(*default),
        }
    }

    /// Whether the parameter applies under `values`.
    pub fn shown(&self, values: &Values) -> bool {
        match self {
            Param::Choice {
                when: Some((key, options)),
                ..
            } => values
                .get(*key)
                .is_some_and(|v| options.contains(&v.as_str())),
            _ => true,
        }
    }
}

pub fn trim(value: f32) -> String {
    let s = format!("{value:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

pub type Values = BTreeMap<String, String>;

#[derive(Clone, Debug)]
pub struct Template {
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub params: Vec<Param>,
    pub build: fn(&Values) -> Result<Frame, String>,
}

impl Template {
    pub fn defaults(&self) -> Values {
        self.params
            .iter()
            .map(|p| (p.key().to_owned(), p.default_text()))
            .collect()
    }
}

/// A shape in board mm, y up, the lower-left corner at (0, 0).
#[derive(Clone, Debug)]
pub enum Shape {
    Poly(Vec<Pos2>),
    Circle(Pos2, f32),
}

/// A connector or hole that the dialog can tick off.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    /// What the tick box says, such as `J1 Socket_2x20 (bottom)`.
    pub label: String,
    pub kind: ItemKind,
}

#[derive(Clone, Debug)]
pub enum ItemKind {
    Hole {
        at: Pos2,
        diameter: f32,
    },
    Connector {
        body: Vec<Pos2>,
        pads: Vec<(Pos2, f32, f32)>,
        bottom: bool,
    },
}

#[derive(Clone, Debug)]
pub struct Keepout {
    pub shape: Shape,
    /// The item it protects: unticking that item drops it.
    pub protects: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Dim {
    pub a: Pos2,
    pub b: Pos2,
    /// Offset from the measured edge, in mm, outward (+) along the normal.
    pub offset: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub outline: Vec<Pos2>,
    pub cutouts: Vec<Shape>,
    pub items: Vec<Item>,
    pub keepouts: Vec<Keepout>,
    pub dims: Vec<Dim>,
}

impl Frame {
    pub fn bounds(&self) -> egui::Rect {
        egui::Rect::from_points(&self.outline)
    }
}

/// The picker's catalogue. Index 0 is "no outline yet".
pub fn all() -> Vec<Template> {
    vec![
        Template {
            name: "No outline (draw it later)",
            category: "Generic",
            description: "An empty board. Draw the Board Outline yourself.",
            params: vec![],
            build: |_| Ok(Frame::default()),
        },
        Template {
            name: "Custom rectangle",
            category: "Generic",
            description: "A rectangle of any size, with optional rounded corners.",
            params: vec![
                len("width", "Width", 100.0, 5.0, 1000.0),
                len("height", "Height", 80.0, 5.0, 1000.0),
                len("corner_radius", "Corner radius", 0.0, 0.0, 50.0),
            ],
            build: custom_rectangle,
        },
        Template {
            name: "Eurocard",
            category: "Generic",
            description: "A 3U or 6U Eurocard for a 19-inch subrack. Outline only.",
            params: vec![
                choice(
                    "height",
                    "Height",
                    &[("3u", "3U (100 mm)"), ("6u", "6U (233.35 mm)")],
                    "3u",
                ),
                choice(
                    "depth",
                    "Depth",
                    &[
                        ("100", "100 mm"),
                        ("160", "160 mm"),
                        ("220", "220 mm"),
                        ("280", "280 mm"),
                        ("340", "340 mm"),
                        ("400", "400 mm"),
                    ],
                    "160",
                ),
            ],
            build: eurocard,
        },
        Template {
            name: "Add-on board for Raspberry Pi",
            category: "Raspberry Pi",
            description: "Fits the 40-pin header and mounting holes of a Raspberry Pi \
                          (models with the 40-pin header). Not a certified HAT.",
            params: vec![
                choice(
                    "size",
                    "Size",
                    &[("full", "Full (65 × 56.5)"), ("micro", "Micro (65 × 30)")],
                    "full",
                ),
                choice(
                    "gpio",
                    "GPIO header",
                    &[("smt", "SMT"), ("th", "Through-hole")],
                    "smt",
                ),
                Param::Choice {
                    key: "cutouts",
                    label: "Display and camera cutouts",
                    options: vec![("yes", "Yes"), ("no", "No")],
                    default: "yes",
                    when: Some(("size", &["full"])),
                },
            ],
            build: pi_addon,
        },
        Template {
            name: "Add-on board for Raspberry Pi Pico",
            category: "Raspberry Pi",
            description: "Sits on a Raspberry Pi Pico or Pico 2 by its two 20-pin rows.",
            params: vec![len("height", "Height", 51.0, 51.0, 120.0)],
            build: pico_addon,
        },
        Template {
            name: "Shield for Arduino Uno",
            category: "Arduino",
            description: "Stacks on an Arduino Uno (R3 header layout).",
            params: vec![],
            build: |_| Ok(arduino(false)),
        },
        Template {
            name: "Shield for Arduino Mega",
            category: "Arduino",
            description: "Stacks on an Arduino Mega 2560.",
            params: vec![],
            build: |_| Ok(arduino(true)),
        },
        Template {
            name: "Feather or FeatherWing",
            category: "Adafruit Feather",
            description: "The Feather format: a mainboard, or a wing that stacks on one.",
            params: vec![choice(
                "format",
                "Format",
                &[("main", "Mainboard"), ("wing", "Wing")],
                "wing",
            )],
            build: feather,
        },
        Template {
            name: "M.2 card",
            category: "PC expansion",
            description: "An M.2 card, 22 mm wide. Cards are 0.8 mm thick: pick a Fab \
                          Preset that offers it.",
            params: vec![
                choice(
                    "length",
                    "Length",
                    &[
                        ("30", "2230"),
                        ("42", "2242"),
                        ("60", "2260"),
                        ("80", "2280"),
                        ("110", "22110"),
                    ],
                    "80",
                ),
                choice(
                    "key",
                    "Key",
                    &[
                        ("a", "A"),
                        ("b", "B"),
                        ("e", "E"),
                        ("m", "M"),
                        ("ae", "A+E"),
                        ("bm", "B+M"),
                    ],
                    "m",
                ),
            ],
            build: m2,
        },
        Template {
            name: "PCI Express add-in card",
            category: "PC expansion",
            description: "A PCI Express card up to half length, for a full-height or \
                          low-profile bracket.",
            params: vec![
                choice(
                    "lanes",
                    "Lanes",
                    &[("1", "x1"), ("4", "x4"), ("8", "x8"), ("16", "x16")],
                    "1",
                ),
                choice(
                    "bracket",
                    "Bracket",
                    &[("full", "Full height"), ("low", "Low profile")],
                    "full",
                ),
                len("card_length", "Card length", 167.65, 30.0, 167.65),
            ],
            build: pcie,
        },
    ]
}

fn len(key: &'static str, label: &'static str, default: f32, min: f32, max: f32) -> Param {
    Param::Length {
        key,
        label,
        default,
        min,
        max,
    }
}

fn choice(
    key: &'static str,
    label: &'static str,
    options: &[(&'static str, &'static str)],
    default: &'static str,
) -> Param {
    Param::Choice {
        key,
        label,
        options: options.to_vec(),
        default,
        when: None,
    }
}

fn length(values: &Values, key: &str, label: &str, min: f32, max: f32) -> Result<f32, String> {
    let text = values.get(key).map(String::as_str).unwrap_or_default();
    let value: f32 = text
        .trim()
        .parse()
        .map_err(|_| format!("{label} must be a number in mm."))?;
    if value < min || value > max {
        return Err(format!(
            "{label} must be between {} and {} mm.",
            trim(min),
            trim(max)
        ));
    }
    Ok(value)
}

fn get<'a>(values: &'a Values, key: &str) -> &'a str {
    values.get(key).map(String::as_str).unwrap_or_default()
}

/// A rectangle with corners rounded by `r` (0 for sharp), counter-clockwise.
fn rounded(x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> Vec<Pos2> {
    if r <= 0.0 {
        return vec![pos2(x0, y0), pos2(x1, y0), pos2(x1, y1), pos2(x0, y1)];
    }
    let mut out = Vec::new();
    for (cx, cy, start) in [
        (x1 - r, y0 + r, -90.0f32),
        (x1 - r, y1 - r, 0.0),
        (x0 + r, y1 - r, 90.0),
        (x0 + r, y0 + r, 180.0),
    ] {
        for i in 0..=8 {
            let a = (start + 90.0 * i as f32 / 8.0).to_radians();
            out.push(pos2(cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    out
}

fn obround(cx: f32, y0: f32, y1: f32, w: f32) -> Vec<Pos2> {
    let r = w / 2.0;
    let mut out = Vec::new();
    for i in 0..=12 {
        let a = (180.0 + 180.0 * i as f32 / 12.0).to_radians();
        out.push(pos2(cx + r * a.cos(), y0 + r + r * a.sin()));
    }
    for i in 0..=12 {
        let a = (180.0 * i as f32 / 12.0).to_radians();
        out.push(pos2(cx + r * a.cos(), y1 - r + r * a.sin()));
    }
    out
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Pos2> {
    rounded(x0, y0, x1, y1, 0.0)
}

fn size_dims(w: f32, h: f32) -> Vec<Dim> {
    vec![
        Dim {
            a: pos2(0.0, h),
            b: pos2(w, h),
            offset: 0.0,
        },
        Dim {
            a: pos2(w, 0.0),
            b: pos2(w, h),
            offset: 0.0,
        },
    ]
}

fn hole(id: &str, n: usize, part: &str, at: Pos2, diameter: f32) -> Item {
    Item {
        id: id.to_owned(),
        label: format!("H{n} {part}"),
        kind: ItemKind::Hole { at, diameter },
    }
}

fn hole_keepout(id: &str, at: Pos2, d: f32) -> Keepout {
    Keepout {
        shape: Shape::Circle(at, d / 2.0),
        protects: Some(id.to_owned()),
    }
}

/// A pin header of `cols` × `rows` at 2.54 mm, pin 1 at `first`, running along +x (or +y
/// when `vertical`).
fn header(
    id: &str,
    n: usize,
    part: &str,
    first: Pos2,
    cols: usize,
    rows: usize,
    vertical: bool,
    bottom: bool,
) -> Item {
    let p = 2.54;
    let mut pads = Vec::new();
    for c in 0..cols {
        for r in 0..rows {
            let (dx, dy) = if vertical {
                (r as f32 * p, c as f32 * p)
            } else {
                (c as f32 * p, r as f32 * p)
            };
            pads.push((pos2(first.x + dx, first.y + dy), 1.7, 1.7));
        }
    }
    let (w, h) = if vertical {
        (rows as f32 * p, cols as f32 * p)
    } else {
        (cols as f32 * p, rows as f32 * p)
    };
    let x0 = first.x - p / 2.0;
    let y0 = first.y - p / 2.0;
    Item {
        id: id.to_owned(),
        label: format!("J{n} {part} ({})", if bottom { "bottom" } else { "top" }),
        kind: ItemKind::Connector {
            body: rect(x0, y0, x0 + w, y0 + h),
            pads,
            bottom,
        },
    }
}

fn custom_rectangle(v: &Values) -> Result<Frame, String> {
    let w = length(v, "width", "Width", 5.0, 1000.0)?;
    let h = length(v, "height", "Height", 5.0, 1000.0)?;
    let r = length(v, "corner_radius", "Corner radius", 0.0, 50.0)?;
    if r * 2.0 > w.min(h) {
        return Err("Corner radius is more than half the shorter side.".into());
    }
    Ok(Frame {
        outline: rounded(0.0, 0.0, w, h, r),
        dims: size_dims(w, h),
        ..Frame::default()
    })
}

fn eurocard(v: &Values) -> Result<Frame, String> {
    let h = if get(v, "height") == "6u" {
        233.35
    } else {
        100.0
    };
    let d: f32 = get(v, "depth").parse().unwrap_or(160.0);
    Ok(Frame {
        outline: rect(0.0, 0.0, d, h),
        dims: size_dims(d, h),
        ..Frame::default()
    })
}

fn pi_addon(v: &Values) -> Result<Frame, String> {
    let full = get(v, "size") == "full";
    let smt = get(v, "gpio") == "smt";
    let h = match (full, smt) {
        (true, true) => 56.5,
        (true, false) => 56.0,
        (false, true) => 30.5,
        (false, false) => 30.0,
    };
    let w = 65.0;
    let top_row = if full { 52.5 } else { 26.5 };
    let mut f = Frame {
        outline: rounded(0.0, 0.0, w, h, 3.0),
        dims: size_dims(w, h),
        ..Frame::default()
    };
    for (i, at) in [
        pos2(3.5, 3.5),
        pos2(61.5, 3.5),
        pos2(61.5, top_row),
        pos2(3.5, top_row),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("h{i}");
        f.items.push(hole(&id, i + 1, "MountingHole_M2.5", at, 2.7));
        f.keepouts.push(hole_keepout(&id, at, 6.2));
    }
    f.items.insert(
        0,
        header(
            "j1",
            1,
            "Socket_2x20",
            pos2(32.5 - 19.0 * 1.27, top_row - 1.27),
            20,
            2,
            false,
            true,
        ),
    );
    if full && get(v, "cutouts") == "yes" {
        f.cutouts
            .push(Shape::Poly(rounded(-1.0, 19.5, 5.0, 36.5, 1.0)));
        f.cutouts.push(Shape::Poly(obround(45.0, 11.5, 28.5, 2.0)));
    }
    Ok(f)
}

fn pico_addon(v: &Values) -> Result<Frame, String> {
    let h = length(v, "height", "Height", 51.0, 120.0)?;
    let w = 21.0;
    let up = h - 51.0;
    let mut f = Frame {
        outline: rounded(0.0, 0.0, w, h, 1.0),
        dims: size_dims(w, h),
        ..Frame::default()
    };
    f.items.push(header(
        "j1",
        1,
        "Socket_1x20",
        pos2(1.61, up + 1.37),
        20,
        1,
        true,
        true,
    ));
    f.items.push(header(
        "j2",
        2,
        "Socket_1x20",
        pos2(19.39, up + 1.37),
        20,
        1,
        true,
        true,
    ));
    for (i, at) in [
        pos2(4.8, up + 2.0),
        pos2(16.2, up + 2.0),
        pos2(16.2, up + 49.0),
        pos2(4.8, up + 49.0),
    ]
    .into_iter()
    .enumerate()
    {
        f.items
            .push(hole(&format!("h{i}"), i + 1, "MountingHole_M2", at, 2.2));
    }
    Ok(f)
}

fn arduino(mega: bool) -> Frame {
    let w = if mega { 101.6 } else { 68.58 };
    let h = 53.34;
    let c = w - 2.54;
    let outline = vec![
        pos2(1.0, 0.0),
        pos2(c, 0.0),
        pos2(c, 1.27),
        pos2(w, 3.81),
        pos2(w, 38.1),
        pos2(c, 40.64),
        pos2(c, 51.82),
        pos2(c - 1.52, h),
        pos2(1.0, h),
        pos2(0.0, h - 1.0),
        pos2(0.0, 1.0),
    ];
    let mut f = Frame {
        outline,
        dims: size_dims(w, h),
        ..Frame::default()
    };
    let mut j = 0;
    let mut add =
        |f: &mut Frame, part: &str, first: Pos2, cols: usize, rows: usize, vertical: bool| {
            j += 1;
            f.items.push(header(
                &format!("j{j}"),
                j,
                part,
                first,
                cols,
                rows,
                vertical,
                true,
            ));
        };
    add(&mut f, "Header_1x10", pos2(17.526, 50.8), 10, 1, false);
    add(&mut f, "Header_1x08", pos2(44.45, 50.8), 8, 1, false);
    add(&mut f, "Header_1x08", pos2(26.67, 2.54), 8, 1, false);
    add(&mut f, "Header_1x06", pos2(49.53, 2.54), 6, 1, false);
    if mega {
        add(&mut f, "Header_1x08", pos2(66.04, 50.8), 8, 1, false);
        add(&mut f, "Header_1x08", pos2(66.04, 2.54), 8, 1, false);
        add(
            &mut f,
            "Header_1x08",
            pos2(71.12 + 10.16, 2.54),
            8,
            1,
            false,
        );
        add(&mut f, "Header_2x18", pos2(93.98, 5.08), 18, 2, true);
    }
    let mut holes = vec![
        pos2(13.97, 2.54),
        pos2(15.24, 50.8),
        pos2(66.04, 7.62),
        pos2(66.04, 35.56),
    ];
    if mega {
        holes.truncate(2);
        holes.extend([pos2(90.17, 50.8), pos2(96.52, 2.54), pos2(74.93, 35.56)]);
    }
    for (i, at) in holes.into_iter().enumerate() {
        f.items
            .push(hole(&format!("h{i}"), i + 1, "MountingHole_M3", at, 3.2));
    }
    f
}

fn feather(v: &Values) -> Result<Frame, String> {
    let (w, h) = (50.8, 22.86);
    let wing = get(v, "format") == "wing";
    let mut f = Frame {
        outline: rounded(0.0, 0.0, w, h, 2.54),
        dims: size_dims(w, h),
        ..Frame::default()
    };
    f.items.push(header(
        "j1",
        1,
        "Header_1x16",
        pos2(6.35, 1.27),
        16,
        1,
        false,
        true,
    ));
    f.items.push(header(
        "j2",
        2,
        "Header_1x12",
        pos2(16.51, 21.59),
        12,
        1,
        false,
        true,
    ));
    let part = if wing {
        "MountingHole_M2.5"
    } else {
        "MountingHole_M2.5_Pad"
    };
    for (i, at) in [
        pos2(2.54, 2.54),
        pos2(48.26, 2.54),
        pos2(48.26, 20.32),
        pos2(2.54, 20.32),
    ]
    .into_iter()
    .enumerate()
    {
        f.items.push(hole(&format!("h{i}"), i + 1, part, at, 2.5));
    }
    Ok(f)
}

fn m2(v: &Values) -> Result<Frame, String> {
    let l: f32 = get(v, "length").parse().unwrap_or(80.0);
    let w = 22.0;
    let key = get(v, "key");
    let mut notches: Vec<f32> = match key {
        "a" => vec![6.625],
        "b" => vec![5.625],
        "e" => vec![2.625],
        "m" => vec![-6.125],
        "ae" => vec![6.625, 2.625],
        _ => vec![5.625, -6.125],
    }
    .into_iter()
    .map(|o| w / 2.0 + o)
    .collect();
    notches.sort_by(f32::total_cmp);
    let (t0, t1) = (1.075, 20.925);
    let mut outline = vec![
        pos2(0.0, 4.0),
        pos2(t0 - 0.5, 4.0),
        pos2(t0, 3.5),
        pos2(t0, 0.0),
    ];
    for &x in &notches {
        outline.extend([
            pos2(x - 0.6, 0.0),
            pos2(x - 0.6, 3.5),
            pos2(x + 0.6, 3.5),
            pos2(x + 0.6, 0.0),
        ]);
    }
    outline.extend([
        pos2(t1, 0.0),
        pos2(t1, 3.5),
        pos2(t1 + 0.5, 4.0),
        pos2(w, 4.0),
        pos2(w, l),
    ]);
    for i in 0..=12 {
        let a = (180.0 * i as f32 / 12.0).to_radians();
        outline.push(pos2(w / 2.0 + 1.75 * a.cos(), l - 1.75 * a.sin()));
    }
    outline.push(pos2(0.0, l));
    let mut f = Frame {
        outline,
        ..Frame::default()
    };
    f.dims = vec![
        Dim {
            a: pos2(0.0, l),
            b: pos2(w, l),
            offset: 0.0,
        },
        Dim {
            a: pos2(w, 0.0),
            b: pos2(w, l),
            offset: 0.0,
        },
    ];
    let mut pads = Vec::new();
    let mut x = t0 + 0.6;
    while x < t1 - 0.4 {
        if notches.iter().all(|n| (x - n).abs() > 1.0) {
            pads.push((pos2(x, 1.6), 0.35, 2.6));
        }
        x += 0.5;
    }
    let label = match key {
        "ae" => "A+E",
        "bm" => "B+M",
        k => match k {
            "a" => "A",
            "b" => "B",
            "e" => "E",
            _ => "M",
        },
    };
    f.items.push(Item {
        id: "j1".into(),
        label: format!("J1 M.2 card edge, key {label}"),
        kind: ItemKind::Connector {
            body: rect(t0, 0.0, t1, 3.2),
            pads,
            bottom: false,
        },
    });
    f.keepouts.push(Keepout {
        shape: Shape::Poly(rect(0.0, l - 3.8, w, l)),
        protects: None,
    });
    f.keepouts.push(Keepout {
        shape: Shape::Poly(rect(0.0, 4.0, w, 5.2)),
        protects: None,
    });
    Ok(f)
}

fn pcie(v: &Values) -> Result<Frame, String> {
    let lanes = get(v, "lanes");
    let long = match lanes {
        "4" => 22.15,
        "8" => 39.15,
        "16" => 72.15,
        _ => 8.15,
    };
    let height = if get(v, "bracket") == "low" {
        68.90
    } else {
        111.15
    };
    let card = length(v, "card_length", "Card length", 30.0, 167.65)?;
    let key_mid = 57.15;
    let short0 = key_mid - 0.95 - 12.15;
    let long1 = key_mid + 0.95 + long;
    let needed = long1 + 4.0;
    if card < needed {
        return Err(format!(
            "A {} mm card is too short for x{lanes} fingers: it needs at least {} mm.",
            trim(card),
            trim(needed)
        ));
    }
    let tab = 8.4;
    let c = 0.5;
    let outline = vec![
        pos2(0.0, tab + 3.0),
        pos2(short0 - 2.0, tab + 3.0),
        pos2(short0 - 2.0, tab),
        pos2(short0, tab),
        pos2(short0, c),
        pos2(short0 + c, 0.0),
        pos2(key_mid - 0.95 - c, 0.0),
        pos2(key_mid - 0.95, c),
        pos2(key_mid - 0.95, tab - 1.0),
        pos2(key_mid + 0.95, tab - 1.0),
        pos2(key_mid + 0.95, c),
        pos2(key_mid + 0.95 + c, 0.0),
        pos2(long1 - c, 0.0),
        pos2(long1, c),
        pos2(long1, tab),
        pos2(card, tab),
        pos2(card, height),
        pos2(0.0, height),
    ];
    let mut f = Frame {
        outline,
        ..Frame::default()
    };
    f.dims = vec![
        Dim {
            a: pos2(0.0, height),
            b: pos2(card, height),
            offset: 0.0,
        },
        Dim {
            a: pos2(card, 0.0),
            b: pos2(card, height),
            offset: 0.0,
        },
        Dim {
            a: pos2(0.0, 0.0),
            b: pos2(key_mid, 0.0),
            offset: -1.0,
        },
    ];
    let mut pads = Vec::new();
    let mut x = short0 + 1.0;
    while x < long1 - 0.6 {
        if (x - key_mid).abs() > 1.4 {
            pads.push((pos2(x, 1.4 + 2.1), 0.7, 4.2));
        }
        x += 1.0;
    }
    f.items.push(Item {
        id: "j1".into(),
        label: format!("J1 PCIe card edge x{lanes}"),
        kind: ItemKind::Connector {
            body: rect(short0, 0.0, long1, tab - 1.5),
            pads,
            bottom: false,
        },
    });
    f.items.push(hole(
        "h0",
        1,
        "Bracket hole Ø3.2",
        pos2(4.0, tab + 10.0),
        3.2,
    ));
    Ok(f)
}
