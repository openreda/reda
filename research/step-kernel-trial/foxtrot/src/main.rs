//! Trial harness for issue #442: Foxtrot (Formlabs) as a kernel-free STEP reader.
//! Contract: ../SPEC.md. Only `read` applies; Foxtrot has no writer.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use step::ap214::Entity;
use step::step_file::StepFile;
use triangulate::triangulate::triangulate;

/// Foxtrot reports some failures only through `log`. This logger counts them.
struct Counter;
static PARSE_FAILED: AtomicUsize = AtomicUsize::new(0);
static FACE_REJECTED: AtomicUsize = AtomicUsize::new(0);
static SKIPPED: AtomicUsize = AtomicUsize::new(0);
static FIRST_ERROR: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static FIRST_PARSE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

impl log::Log for Counter {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let msg = r.args().to_string();
        if msg.starts_with("Failed to parse") {
            PARSE_FAILED.fetch_add(1, Ordering::Relaxed);
        } else if msg.starts_with("Failed to triangulate") {
            // A face rejected before triangulation (unsupported surface or curve);
            // these are not in Stats::num_errors.
            FACE_REJECTED.fetch_add(1, Ordering::Relaxed);
        } else if msg.starts_with("Skipping") {
            SKIPPED.fetch_add(1, Ordering::Relaxed);
        }
        if r.level() == log::Level::Error {
            let mut first = FIRST_ERROR.lock().unwrap();
            if first.is_none() {
                *first = Some(msg);
            }
        } else if msg.starts_with("Failed to parse") {
            let mut first = FIRST_PARSE.lock().unwrap();
            if first.is_none() {
                *first = Some(msg.chars().take(60).collect());
            }
        }
    }
    fn flush(&self) {}
}

fn jstr(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn jnum(v: f64) -> String {
    if v.is_finite() { format!("{v}") } else { "null".into() }
}

fn jvec(v: [f64; 3]) -> String {
    format!("[{},{},{}]", jnum(v[0]), jnum(v[1]), jnum(v[2]))
}

fn shorten(s: &str) -> String {
    s.chars().take(300).collect()
}

fn fail(msg: &str) {
    println!(
        "{{\"parse_ok\":false,\"parse_ms\":null,\"error\":{},\"solids\":null,\"shells\":null,\
         \"faces_total\":null,\"faces_meshed\":null,\"faces_failed\":null,\"triangles\":null,\
         \"tess_ms\":null,\"bbox_min\":null,\"bbox_max\":null,\"area\":null,\"volume\":null,\
         \"open_edges\":null,\"colours_distinct\":null,\"faces_coloured\":null,\"products\":null,\
         \"notes\":\"\"}}",
        jstr(&shorten(msg))
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("read") if args.len() == 3 => read(&args[2]),
        Some("write") => println!(
            "{{\"ok\":false,\"error\":\"Foxtrot has no STEP writer\",\"bytes\":null,\"schema\":null,\"notes\":\"\"}}"
        ),
        _ => {
            eprintln!("usage: step-trial-foxtrot read <file.step>");
            std::process::exit(2);
        }
    }
}

fn read(path: &str) {
    log::set_logger(&Counter).unwrap();
    log::set_max_level(log::LevelFilter::Warn);

    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return fail(&format!("read: {e}")),
    };

    // --- parse -----------------------------------------------------------
    let t0 = Instant::now();
    let flat = StepFile::strip_flatten(&data);
    let file = StepFile::parse(&flat);
    let parse_ms = t0.elapsed().as_secs_f64() * 1e3;

    let mut entities = 0usize;
    let (mut solids, mut shells, mut faces_total) = (0usize, 0usize, 0usize);
    let (mut products, mut nauo) = (0usize, 0usize);
    let mut solid_ids = HashSet::new();
    for (i, e) in file.0.iter().enumerate() {
        match e {
            Entity::_EmptySlot => continue,
            Entity::ManifoldSolidBrep(_) | Entity::BrepWithVoids(_) => {
                solids += 1;
                solid_ids.insert(i);
            }
            Entity::ClosedShell(_) | Entity::OpenShell(_) => shells += 1,
            Entity::AdvancedFace(_) => faces_total += 1,
            Entity::Product(_) => products += 1,
            Entity::NextAssemblyUsageOccurrence(_) => nauo += 1,
            _ => {}
        }
        entities += 1;
    }
    // Solids that a STYLED_ITEM points at (Foxtrot colours whole solids only).
    let mut styled_solids = HashSet::new();
    let mut styled_other = 0usize;
    for e in &file.0 {
        if let Entity::StyledItem(s) = e {
            if solid_ids.contains(&s.item.0) {
                styled_solids.insert(s.item.0);
            } else {
                styled_other += 1;
            }
        }
    }
    let parse_failed = PARSE_FAILED.load(Ordering::Relaxed);
    let millimetre = flat.windows(22).any(|w| w == b"SI_UNIT(.MILLI.,.METRE");

    if entities == 0 {
        return fail("no entities parsed");
    }

    // --- tessellate ------------------------------------------------------
    let t1 = Instant::now();
    let (mesh, stats) = triangulate(&file);
    let tess_ms = t1.elapsed().as_secs_f64() * 1e3;

    let rejected = FACE_REJECTED.load(Ordering::Relaxed);
    let faces_failed = stats.num_errors + stats.num_panics + rejected;
    let faces_meshed = stats.num_faces.saturating_sub(faces_failed);

    // --- mesh measures ---------------------------------------------------
    let p = |i: u32| {
        let v = &mesh.verts[i as usize].pos;
        [v.x, v.y, v.z]
    };
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let (mut area, mut vol6) = (0.0f64, 0.0f64);
    // Weld vertices closer than 1e-4 mm: grid of 1e-4 cells, each new point looks in
    // the 27 cells around it for an earlier point within the distance.
    const WELD: f64 = 1e-4;
    let mut cells: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    let mut welded: Vec<[f64; 3]> = Vec::new();
    let mut weld = |q: [f64; 3]| -> u32 {
        let k = [(q[0] / WELD).floor() as i64, (q[1] / WELD).floor() as i64, (q[2] / WELD).floor() as i64];
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(c) = cells.get(&[k[0] + dx, k[1] + dy, k[2] + dz]) {
                        for &i in c {
                            let r = welded[i as usize];
                            let d2 = (r[0] - q[0]).powi(2) + (r[1] - q[1]).powi(2) + (r[2] - q[2]).powi(2);
                            if d2 < WELD * WELD {
                                return i;
                            }
                        }
                    }
                }
            }
        }
        let i = welded.len() as u32;
        welded.push(q);
        cells.entry(k).or_default().push(i);
        i
    };
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    let mut colours = HashSet::new();
    let mut non_finite = 0usize;
    for t in &mesh.triangles {
        let (a, b, c) = (p(t.verts.x), p(t.verts.y), p(t.verts.z));
        if [a, b, c].iter().flatten().any(|v| !v.is_finite()) {
            non_finite += 1;
            continue;
        }
        let mut w = [0u32; 3];
        for (k, q) in [a, b, c].iter().enumerate() {
            for j in 0..3 {
                lo[j] = lo[j].min(q[j]);
                hi[j] = hi[j].max(q[j]);
            }
            w[k] = weld(*q);
        }
        for vi in [t.verts.x, t.verts.y, t.verts.z] {
            let col = &mesh.verts[vi as usize].color;
            colours.insert([
                (col.x * 255.0).round() as i64,
                (col.y * 255.0).round() as i64,
                (col.z * 255.0).round() as i64,
            ]);
        }
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        area += 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        vol6 += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
            continue; // collapsed by welding
        }
        for (i, j) in [(w[0], w[1]), (w[1], w[2]), (w[2], w[0])] {
            *edges.entry((i.min(j), i.max(j))).or_insert(0) += 1;
        }
    }
    let open_edges = edges.values().filter(|&&n| n == 1).count();
    let has_mesh = !mesh.triangles.is_empty() && lo[0].is_finite();

    let error = if !has_mesh {
        Some("no triangles produced".to_string())
    } else if faces_failed > 0 {
        FIRST_ERROR.lock().unwrap().clone()
    } else {
        None
    };

    let mut notes = vec![
        "no tolerance setting (fixed sampling)".to_string(),
        "placements applied by triangulate()".to_string(),
        "colour is per solid, face styles ignored; colours_distinct is 0 when no solid has a STYLED_ITEM (mesh is then all fallback grey 0.5), otherwise distinct vertex colours, fallback grey included".to_string(),
        "faces_coloured counts solids targeted by a STYLED_ITEM".to_string(),
        format!("products = PRODUCT entities in the file ({nauo} NEXT_ASSEMBLY_USAGE_OCCURRENCE); no assembly tree API"),
        "no unit conversion".to_string(),
    ];
    if !millimetre {
        notes.push("file length unit is not SI millimetre: numbers are in file units".into());
    }
    if styled_other > 0 {
        notes.push(format!("{styled_other} STYLED_ITEMs on non-solid items ignored"));
    }
    if parse_failed > 0 {
        notes.push(format!(
            "{parse_failed} entities failed the typed AP214 parse, first: {}",
            FIRST_PARSE.lock().unwrap().clone().unwrap_or_default()
        ));
    }
    if faces_failed > 0 {
        notes.push(format!(
            "faces_failed = {} cdt errors + {} cdt panics (caught inside Foxtrot) + {} rejected surfaces/curves",
            stats.num_errors, stats.num_panics, rejected
        ));
    }
    if stats.num_faces != faces_total {
        notes.push(format!("triangulate() visited {} faces ({} shells)", stats.num_faces, stats.num_shells));
    }
    let skipped = SKIPPED.load(Ordering::Relaxed);
    if skipped > 0 {
        notes.push(format!("{skipped} items skipped by triangulate()"));
    }
    if non_finite > 0 {
        notes.push(format!("{non_finite} triangles with non-finite coordinates left out of measures"));
    }

    let bbox = |v: [f64; 3]| if has_mesh { jvec(v) } else { "null".into() };
    println!(
        "{{\"parse_ok\":true,\"parse_ms\":{},\"error\":{},\"solids\":{},\"shells\":{},\"faces_total\":{},\
         \"faces_meshed\":{},\"faces_failed\":{},\"triangles\":{},\"tess_ms\":{},\"bbox_min\":{},\
         \"bbox_max\":{},\"area\":{},\"volume\":{},\"open_edges\":{},\"colours_distinct\":{},\
         \"faces_coloured\":{},\"products\":{},\"notes\":{}}}",
        jnum((parse_ms * 1000.0).round() / 1000.0),
        error.map(|e| jstr(&shorten(&e))).unwrap_or("null".into()),
        solids,
        shells,
        faces_total,
        faces_meshed,
        faces_failed,
        mesh.triangles.len(),
        jnum((tess_ms * 1000.0).round() / 1000.0),
        bbox(lo),
        bbox(hi),
        jnum(area),
        jnum(vol6 / 6.0),
        open_edges,
        if styled_solids.is_empty() { 0 } else { colours.len() },
        styled_solids.len(),
        products,
        jstr(&notes.join("; ")),
    );
}
