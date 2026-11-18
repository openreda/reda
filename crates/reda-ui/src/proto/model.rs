//! PROTOTYPE (issue #330): the in-memory Stackup the three variants edit. Throwaway.
//!
//! Nothing here is the v0.2 model: it holds just enough (materials, a Layer role, a list
//! of Via types, a rough impedance) for the screens to have something real to show.

/// What a copper Layer is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Signal,
    Plane,
    Mixed,
}

impl Role {
    pub const ALL: [Role; 3] = [Role::Signal, Role::Plane, Role::Mixed];

    pub fn name(self) -> &'static str {
        match self {
            Role::Signal => "Signal",
            Role::Plane => "Plane",
            Role::Mixed => "Mixed",
        }
    }
}

/// How a dielectric is made.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Make {
    Core,
    Prepreg,
}

impl Make {
    pub fn name(self) -> &'static str {
        match self {
            Make::Core => "Core",
            Make::Prepreg => "Prepreg",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Kind {
    Copper { role: Role },
    Dielectric { make: Make },
}

/// One entry of the stack, top to bottom.
#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub id: u32,
    pub name: String,
    pub material: String,
    pub kind: Kind,
    /// Thickness in micrometres.
    pub um: f64,
    /// The thickness as typed, in mm.
    pub text: String,
    /// A dielectric's Dk and Df as typed; empty on copper.
    pub dk: String,
    pub df: String,
}

impl Entry {
    pub fn is_copper(&self) -> bool {
        matches!(self.kind, Kind::Copper { .. })
    }

    pub fn dk_value(&self) -> f64 {
        self.dk.trim().parse::<f64>().ok().filter(|v| *v >= 1.0).unwrap_or(4.5)
    }

    pub fn role(&self) -> Option<Role> {
        match self.kind {
            Kind::Copper { role } => Some(role),
            Kind::Dielectric { .. } => None,
        }
    }

    /// Reads the typed thickness back; false when it is not a positive number.
    pub fn commit_text(&mut self) -> bool {
        match self.text.trim().parse::<f64>() {
            Ok(mm) if mm > 0.0 => {
                self.um = mm * 1000.0;
                true
            }
            _ => false,
        }
    }

    pub fn set_um(&mut self, um: f64) {
        self.um = um.max(1.0);
        self.text = mm(self.um);
    }

    /// "Signal", "Core", and so on.
    pub fn kind_name(&self) -> &'static str {
        match &self.kind {
            Kind::Copper { role } => role.name(),
            Kind::Dielectric { make } => make.name(),
        }
    }
}

/// A Via type the board allows: a span between two copper Layers, a drill and a pad.
#[derive(Clone, PartialEq, Debug)]
pub struct ViaType {
    pub name: String,
    /// The copper Layers it starts and ends on, counted from 0 at the top.
    pub from: usize,
    pub to: usize,
    pub drill: String,
    pub pad: String,
}

impl ViaType {
    /// Through, blind, buried or micro: read off the span, never stored.
    pub fn class(&self, coppers: usize) -> &'static str {
        let last = coppers.saturating_sub(1);
        let (a, b) = (self.from.min(self.to), self.from.max(self.to));
        if a == 0 && b == last {
            "Through"
        } else if b - a == 1 && (a == 0 || b == last) {
            "Micro"
        } else if a == 0 || b == last {
            "Blind"
        } else {
            "Buried"
        }
    }
}

/// What is selected in the editor.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Sel {
    #[default]
    None,
    Entry(u32),
    Via(usize),
}

#[derive(Clone, Debug)]
pub struct Stack {
    pub preset: &'static str,
    pub entries: Vec<Entry>,
    pub vias: Vec<ViaType>,
    pub sel: Sel,
    /// The finished thickness ordered, typed in mm.
    pub target: String,
    /// The Track width the impedance readout is worked out at, typed in mm.
    pub track: String,
    pub finish: String,
    pub mask: String,
    pub silk: String,
    next_id: u32,
}

/// Micrometres as mm text, without trailing zeros.
pub fn mm(um: f64) -> String {
    let s = format!("{:.4}", um / 1000.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_owned()
}

pub const PRESETS: [&str; 4] = [
    "2 Layer, 1.6 mm",
    "4 Layer, JLC04161H-7628",
    "6 Layer, JLC06161H-3313",
    "8 Layer, HDI 1+6+1",
];

impl Stack {
    pub fn preset(name: &'static str) -> Stack {
        let mut s = Stack {
            preset: name,
            entries: Vec::new(),
            vias: Vec::new(),
            sel: Sel::None,
            target: "1.6".into(),
            track: "0.2".into(),
            finish: "HASL lead-free".into(),
            mask: "Green".into(),
            silk: "White".into(),
            next_id: 1,
        };
        // (copper um, role) and the dielectric under it (make, material, um, dk).
        type Cu = (f64, Role);
        type Di = (Make, &'static str, f64, f64);
        let (cu, di): (Vec<Cu>, Vec<Di>) = match name {
            "2 Layer, 1.6 mm" => (
                vec![(35.0, Role::Signal), (35.0, Role::Mixed)],
                vec![(Make::Core, "FR-4", 1530.0, 4.5)],
            ),
            "6 Layer, JLC06161H-3313" => (
                vec![
                    (35.0, Role::Signal),
                    (15.2, Role::Plane),
                    (15.2, Role::Signal),
                    (15.2, Role::Signal),
                    (15.2, Role::Plane),
                    (35.0, Role::Signal),
                ],
                vec![
                    (Make::Prepreg, "3313", 99.4, 4.1),
                    (Make::Core, "FR-4 core", 550.0, 4.6),
                    (Make::Prepreg, "2116", 108.8, 4.16),
                    (Make::Core, "FR-4 core", 550.0, 4.6),
                    (Make::Prepreg, "3313", 99.4, 4.1),
                ],
            ),
            "8 Layer, HDI 1+6+1" => (
                vec![
                    (30.0, Role::Signal),
                    (17.5, Role::Plane),
                    (17.5, Role::Signal),
                    (17.5, Role::Plane),
                    (17.5, Role::Plane),
                    (17.5, Role::Signal),
                    (17.5, Role::Plane),
                    (30.0, Role::Signal),
                ],
                vec![
                    (Make::Prepreg, "1080", 70.0, 3.9),
                    (Make::Core, "FR-4 core", 200.0, 4.5),
                    (Make::Prepreg, "2116", 240.0, 4.2),
                    (Make::Core, "FR-4 core", 300.0, 4.5),
                    (Make::Prepreg, "2116", 240.0, 4.2),
                    (Make::Core, "FR-4 core", 200.0, 4.5),
                    (Make::Prepreg, "1080", 70.0, 3.9),
                ],
            ),
            _ => (
                vec![
                    (35.0, Role::Signal),
                    (15.2, Role::Plane),
                    (15.2, Role::Plane),
                    (35.0, Role::Signal),
                ],
                vec![
                    (Make::Prepreg, "7628", 210.4, 4.4),
                    (Make::Core, "FR-4 core", 1065.0, 4.6),
                    (Make::Prepreg, "7628", 210.4, 4.4),
                ],
            ),
        };
        let n = cu.len();
        for (i, (um, role)) in cu.into_iter().enumerate() {
            let cname = match i {
                0 => "Top".to_owned(),
                i if i == n - 1 => "Bottom".to_owned(),
                i => format!("Inner {i}"),
            };
            s.push(cname, "Copper", Kind::Copper { role }, um);
            if let Some((make, material, um, dk)) = di.get(i).copied() {
                let kind = Kind::Dielectric { make };
                s.push(format!("Dielectric {}", i + 1), material, kind, um);
                s.entries.last_mut().unwrap().dk = format!("{dk}");
            }
        }
        s.vias.push(ViaType {
            name: "Through".into(),
            from: 0,
            to: n - 1,
            drill: "0.3".into(),
            pad: "0.6".into(),
        });
        if name == "8 Layer, HDI 1+6+1" {
            let via = |name: &str, from, to, drill: &str, pad: &str| ViaType {
                name: name.into(),
                from,
                to,
                drill: drill.into(),
                pad: pad.into(),
            };
            s.vias.push(via("Micro top", 0, 1, "0.1", "0.25"));
            s.vias.push(via("Buried", 1, 6, "0.2", "0.45"));
            s.vias.push(via("Micro bottom", 7, 6, "0.1", "0.25"));
        }
        s
    }

    fn push(&mut self, name: String, material: &str, kind: Kind, um: f64) {
        let e = self.entry(name, material, kind, um);
        self.entries.push(e);
    }

    fn entry(&mut self, name: String, material: &str, kind: Kind, um: f64) -> Entry {
        let id = self.next_id;
        self.next_id += 1;
        let dielectric = matches!(kind, Kind::Dielectric { .. });
        let typed = |v: &str| if dielectric { v.to_owned() } else { String::new() };
        Entry {
            id,
            name,
            material: material.to_owned(),
            kind,
            um,
            text: mm(um),
            dk: typed("4.2"),
            df: typed("0.02"),
        }
    }

    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    pub fn selected_entry(&self) -> Option<usize> {
        match self.sel {
            Sel::Entry(id) => self.index_of(id),
            _ => None,
        }
    }

    pub fn coppers(&self) -> usize {
        self.entries.iter().filter(|e| e.is_copper()).count()
    }

    /// The place among the copper Layers (0 at the top) of entry `index`.
    pub fn copper_number(&self, index: usize) -> Option<usize> {
        self.entries[index]
            .is_copper()
            .then(|| self.entries[..index].iter().filter(|e| e.is_copper()).count())
    }

    /// The entry index of copper Layer `n`.
    pub fn copper_entry(&self, n: usize) -> usize {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_copper())
            .nth(n)
            .map_or(0, |(i, _)| i)
    }

    /// Adds a copper Layer in the middle of dielectric `index`, which is split in two: the
    /// new Layer takes prepreg above it and leaves the rest below.
    pub fn split_with_copper(&mut self, index: usize) {
        if self.entries.get(index).is_none_or(Entry::is_copper) {
            return;
        }
        let half = (self.entries[index].um / 2.0).max(50.0);
        self.entries[index].set_um(half);
        let copper = self.entry(String::new(), "Copper", Kind::Copper { role: Role::Signal }, 17.5);
        let kind = Kind::Dielectric {
            make: Make::Prepreg,
        };
        let under = self.entry(String::new(), "2116", kind, half);
        let id = copper.id;
        let at = self.copper_number_before(index);
        self.entries.insert(index + 1, copper);
        self.entries.insert(index + 2, under);
        // A Via type keeps the Layers it ended on.
        for v in &mut self.vias {
            for end in [&mut v.from, &mut v.to] {
                if *end >= at {
                    *end += 1;
                }
            }
        }
        self.rename();
        self.sel = Sel::Entry(id);
    }

    fn copper_number_before(&self, index: usize) -> usize {
        self.entries[..index].iter().filter(|e| e.is_copper()).count()
    }

    /// Adds a copper Layer below copper entry `index` (or above the bottom one).
    pub fn add_copper_near(&mut self, index: usize) {
        let below = index + 1;
        let target = if below < self.entries.len() {
            below
        } else {
            index.saturating_sub(1)
        };
        self.split_with_copper(target);
    }

    /// Removes inner copper Layer at entry `index` and joins the dielectrics around it.
    /// The outer two stay.
    pub fn remove_copper(&mut self, index: usize) -> bool {
        let Some(n) = self.copper_number(index) else {
            return false;
        };
        if n == 0 || n == self.coppers() - 1 {
            return false;
        }
        let under = self.entries[index + 1].um;
        self.entries.drain(index..=index + 1);
        let above = &mut self.entries[index - 1];
        above.set_um(above.um + under);
        let last = self.coppers() - 1;
        for v in &mut self.vias {
            for end in [&mut v.from, &mut v.to] {
                if *end > n || *end > last {
                    *end -= 1;
                }
            }
        }
        self.vias.retain(|v| v.from != v.to);
        self.rename();
        self.sel = Sel::None;
        true
    }

    /// Moves copper Layer at entry `index` one place up or down among the copper Layers:
    /// the two Layers swap role and copper, and everything else stays where it is.
    pub fn move_copper(&mut self, index: usize, down: bool) -> bool {
        let Some(n) = self.copper_number(index) else {
            return false;
        };
        let other = if down { n + 1 } else { n.wrapping_sub(1) };
        if other >= self.coppers() {
            return false;
        }
        self.swap_coppers(n, other);
        true
    }

    /// Puts copper Layer `from` at place `to`, shifting the ones between.
    pub fn place_copper(&mut self, from: usize, to: usize) {
        let mut at = from;
        while at != to {
            let next = if to > at { at + 1 } else { at - 1 };
            self.swap_coppers(at, next);
            at = next;
        }
    }

    fn swap_coppers(&mut self, a: usize, b: usize) {
        let (ia, ib) = (self.copper_entry(a), self.copper_entry(b));
        self.entries.swap(ia, ib);
        self.rename();
    }

    /// Names the copper Layers and dielectrics by position, as a new board does.
    fn rename(&mut self) {
        let n = self.coppers();
        let (mut c, mut d) = (0, 0);
        for e in &mut self.entries {
            if e.is_copper() {
                e.name = match c {
                    0 => "Top".to_owned(),
                    c if c == n - 1 => "Bottom".to_owned(),
                    c => format!("Inner {c}"),
                };
                c += 1;
            } else {
                d += 1;
                e.name = format!("Dielectric {d}");
            }
        }
    }

    pub fn add_via(&mut self) {
        let n = self.coppers();
        let (from, to) = if n > 2 { (0, 1) } else { (0, n - 1) };
        self.vias.push(ViaType {
            name: format!("Via {}", self.vias.len() + 1),
            from,
            to,
            drill: "0.2".into(),
            pad: "0.45".into(),
        });
        self.sel = Sel::Via(self.vias.len() - 1);
    }

    pub fn remove_via(&mut self, index: usize) {
        if self.vias.len() > 1 && index < self.vias.len() {
            self.vias.remove(index);
            self.sel = Sel::None;
        }
    }

    /// Copper, dielectric and total thickness, in micrometres.
    pub fn totals(&self) -> (f64, f64, f64) {
        let sum = |copper: bool| -> f64 {
            (self.entries.iter())
                .filter(|e| e.is_copper() == copper)
                .map(|e| e.um)
                .sum()
        };
        let (c, d) = (sum(true), sum(false));
        (c, d, c + d)
    }

    pub fn target_um(&self) -> Option<f64> {
        self.target.trim().parse::<f64>().ok().map(|mm| mm * 1000.0)
    }

    /// "1.57 mm of 1.6 mm", and whether it is within 10% of the target.
    pub fn total_line(&self) -> (String, bool) {
        let total = self.totals().2;
        match self.target_um() {
            Some(target) if target > 0.0 => (
                format!("{} mm of {} mm ordered", mm(total), mm(target)),
                ((total - target) / target).abs() <= 0.1,
            ),
            _ => (format!("{} mm", mm(total)), true),
        }
    }

    /// The pairs of entries that differ across the middle of the stack, by name.
    pub fn asymmetries(&self) -> Vec<String> {
        let n = self.entries.len();
        (0..n / 2)
            .filter_map(|i| {
                let (a, b) = (&self.entries[i], &self.entries[n - 1 - i]);
                let same = (a.um - b.um).abs() < 0.5
                    && a.is_copper() == b.is_copper()
                    && (a.is_copper() || a.material == b.material);
                (!same).then(|| format!("{} and {}", a.name, b.name))
            })
            .collect()
    }

    /// The nearest copper Layer that can be a reference (a plane or mixed Layer), looking
    /// up (`dir` -1) or down (+1) from entry `index`: its entry index, the dielectric
    /// height between in micrometres, and that dielectric's Dk averaged by thickness.
    fn reference(&self, index: usize, dir: isize) -> Option<(usize, f64, f64)> {
        let (mut h, mut dk_h) = (0.0, 0.0);
        let mut i = index as isize + dir;
        while i >= 0 && (i as usize) < self.entries.len() {
            let e = &self.entries[i as usize];
            match e.kind {
                Kind::Dielectric { .. } => {
                    h += e.um;
                    dk_h += e.dk_value() * e.um;
                }
                Kind::Copper { role } if role != Role::Signal => {
                    return (h > 0.0).then(|| (i as usize, h, dk_h / h));
                }
                // Another signal Layer in the way: its copper adds to the height.
                Kind::Copper { .. } => h += e.um,
            }
            i += dir;
        }
        None
    }

    /// The impedance readout of copper entry `index` at the stack's Track width.
    pub fn impedance(&self, index: usize) -> Option<Impedance> {
        let e = &self.entries[index];
        if !e.is_copper() || e.role() == Some(Role::Plane) {
            return None;
        }
        let w = self.track.trim().parse::<f64>().ok().filter(|w| *w > 0.0)? * 1000.0;
        let t = e.um;
        let up = self.reference(index, -1);
        let down = self.reference(index, 1);
        let name = |i: usize| self.entries[i].name.clone();
        match (up, down) {
            (None, None) => None,
            (Some((r, h, er)), None) | (None, Some((r, h, er))) => Some(Impedance {
                ohms: microstrip(w, t, h, er),
                model: "Microstrip",
                over: name(r),
            }),
            (Some((r1, h1, er1)), Some((r2, h2, er2))) => {
                let z1 = stripline(w, t, 2.0 * h1 + t, er1);
                let z2 = stripline(w, t, 2.0 * h2 + t, er2);
                Some(Impedance {
                    ohms: 2.0 * z1 * z2 / (z1 + z2),
                    model: "Stripline",
                    over: format!("{} and {}", name(r1), name(r2)),
                })
            }
        }
    }
}

/// One Layer's impedance readout.
#[derive(Clone, Debug)]
pub struct Impedance {
    pub ohms: f64,
    pub model: &'static str,
    /// The reference Layer or Layers.
    pub over: String,
}

impl Impedance {
    pub fn text(&self) -> String {
        format!("{:.0} Ω", self.ohms)
    }
}

/// Hammerstad's microstrip with a thickness correction. Lengths in any one unit.
fn microstrip(w: f64, t: f64, h: f64, er: f64) -> f64 {
    use std::f64::consts::PI;
    let we = w + t / PI * (1.0 + (2.0 * h / t).ln());
    let u = we / h;
    if u <= 1.0 {
        let ee = (er + 1.0) / 2.0
            + (er - 1.0) / 2.0 * ((1.0 + 12.0 / u).powf(-0.5) + 0.04 * (1.0 - u).powi(2));
        60.0 / ee.sqrt() * (8.0 / u + u / 4.0).ln()
    } else {
        let ee = (er + 1.0) / 2.0 + (er - 1.0) / 2.0 * (1.0 + 12.0 / u).powf(-0.5);
        120.0 * PI / (ee.sqrt() * (u + 1.393 + 0.667 * (u + 1.444).ln()))
    }
}

/// Centred stripline between planes `b` apart.
fn stripline(w: f64, t: f64, b: f64, er: f64) -> f64 {
    use std::f64::consts::PI;
    (60.0 / er.sqrt() * (4.0 * b / (0.67 * PI * (0.8 * w + t))).ln()).max(1.0)
}
