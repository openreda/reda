//! Mesh metrics and JSON output shared by the two truck harnesses (same file in both packages).

use std::collections::HashMap;

pub type Tri = [[f64; 3]; 3];

pub struct MeshStats {
    pub bbox: Option<([f64; 3], [f64; 3])>,
    pub area: f64,
    pub volume: f64,
    pub open_edges: usize,
}

const WELD: f64 = 1.0e-4;

pub fn mesh_stats(tris: &[Tri]) -> MeshStats {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let (mut area, mut volume) = (0.0, 0.0);
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    let mut points: Vec<[f64; 3]> = Vec::new();
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    let mut weld = |p: [f64; 3]| -> u32 {
        let c = [0, 1, 2].map(|i| (p[i] / WELD).floor() as i64);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(ids) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &id in ids {
                            let q = points[id as usize];
                            let d2: f64 = (0..3).map(|i| (p[i] - q[i]).powi(2)).sum();
                            if d2 < WELD * WELD {
                                return id;
                            }
                        }
                    }
                }
            }
        }
        let id = points.len() as u32;
        points.push(p);
        grid.entry(c).or_default().push(id);
        id
    };
    for t in tris {
        for p in t {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let [a, b, c] = *t;
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        area += 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        volume += (a[0] * n[0] + a[1] * n[1] + a[2] * n[2]) / 6.0;
        let ids = [weld(a), weld(b), weld(c)];
        if ids[0] == ids[1] || ids[1] == ids[2] || ids[2] == ids[0] {
            continue;
        }
        for i in 0..3 {
            let (p, q) = (ids[i], ids[(i + 1) % 3]);
            *edges.entry((p.min(q), p.max(q))).or_default() += 1;
        }
    }
    MeshStats {
        bbox: (!tris.is_empty()).then_some((min, max)),
        area,
        volume,
        open_edges: edges.values().filter(|&&n| n == 1).count(),
    }
}

pub fn jstr(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn jopt_str(s: &Option<String>) -> String {
    match s {
        Some(s) => jstr(&s.chars().take(300).collect::<String>()),
        None => "null".into(),
    }
}

pub fn jnum(x: f64) -> String {
    if x.is_finite() {
        format!("{x}")
    } else {
        "null".into()
    }
}

pub fn jopt<T: std::fmt::Display>(x: Option<T>) -> String {
    match x {
        Some(x) => format!("{x}"),
        None => "null".into(),
    }
}

pub fn jvec(v: Option<[f64; 3]>) -> String {
    match v {
        Some(v) => format!("[{},{},{}]", jnum(v[0]), jnum(v[1]), jnum(v[2])),
        None => "null".into(),
    }
}

#[derive(Default)]
pub struct ReadReport {
    pub parse_ok: bool,
    pub parse_ms: f64,
    pub error: Option<String>,
    pub solids: Option<usize>,
    pub shells: Option<usize>,
    pub faces_total: Option<usize>,
    pub faces_meshed: Option<usize>,
    pub faces_failed: Option<usize>,
    pub tess_ms: Option<f64>,
    pub tris: Option<Vec<Tri>>,
    pub colours_distinct: Option<usize>,
    pub faces_coloured: Option<usize>,
    pub products: Option<usize>,
    pub notes: Vec<String>,
}

impl ReadReport {
    pub fn fail(&mut self, msg: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(msg.into());
        }
    }
    pub fn print(&self) {
        let stats = self.tris.as_ref().map(|t| mesh_stats(t));
        let bbox = stats.as_ref().and_then(|s| s.bbox);
        println!(
            "{{\"parse_ok\":{},\"parse_ms\":{},\"error\":{},\"solids\":{},\"shells\":{},\"faces_total\":{},\
\"faces_meshed\":{},\"faces_failed\":{},\"triangles\":{},\"tess_ms\":{},\"bbox_min\":{},\"bbox_max\":{},\
\"area\":{},\"volume\":{},\"open_edges\":{},\"colours_distinct\":{},\"faces_coloured\":{},\"products\":{},\"notes\":{}}}",
            self.parse_ok,
            jnum((self.parse_ms * 100.0).round() / 100.0),
            jopt_str(&self.error),
            jopt(self.solids),
            jopt(self.shells),
            jopt(self.faces_total),
            jopt(self.faces_meshed),
            jopt(self.faces_failed),
            jopt(self.tris.as_ref().map(|t| t.len())),
            jopt(self.tess_ms.map(|x| jnum((x * 100.0).round() / 100.0))),
            jvec(bbox.map(|b| b.0)),
            jvec(bbox.map(|b| b.1)),
            jopt(stats.as_ref().map(|s| jnum(s.area))),
            jopt(stats.as_ref().map(|s| jnum(s.volume))),
            jopt(stats.as_ref().map(|s| s.open_edges)),
            jopt(self.colours_distinct),
            jopt(self.faces_coloured),
            jopt(self.products),
            jstr(&self.notes.join("; ")),
        );
    }
}

pub fn print_write(ok: bool, error: Option<String>, bytes: Option<usize>, schema: &str, notes: &[String]) {
    println!(
        "{{\"ok\":{ok},\"error\":{},\"bytes\":{},\"schema\":{},\"notes\":{}}}",
        jopt_str(&error),
        jopt(bytes),
        jstr(schema),
        jstr(&notes.join("; ")),
    );
}

/// Board dimensions from SPEC.md.
pub mod board {
    pub const W: f64 = 50.0;
    pub const H: f64 = 30.0;
    pub const R: f64 = 3.0;
    pub const T: f64 = 1.6;
    pub const HOLE_R: f64 = 1.6;
    pub const HOLE_INSET: f64 = 4.0;
    pub const SLOT_R: f64 = 1.0;
    pub const SLOT_HALF: f64 = 3.0;
    pub const SLOT_C: [f64; 2] = [25.0, 15.0];
}
