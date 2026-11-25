//! `read <file>`: OpenCascade through opencascade-rs, for comparison only. Prints one JSON line.
use opencascade::primitives::Shape;
use std::collections::HashMap;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 || args[1] != "read" {
        eprintln!("usage: step-trial-occt read <file.step>");
        std::process::exit(2);
    }
    let t = Instant::now();
    let shape = match Shape::read_step(&args[2]) {
        Ok(s) => s,
        Err(e) => {
            println!("{{\"parse_ok\":false,\"error\":{:?}}}", format!("{e:?}"));
            return;
        }
    };
    let parse_ms = t.elapsed().as_secs_f64() * 1e3;
    let faces_total = shape.faces().count();
    let t = Instant::now();
    let mesh = match shape.mesh_with_tolerance(0.01) {
        Ok(m) => m,
        Err(e) => {
            println!("{{\"parse_ok\":true,\"parse_ms\":{parse_ms:.2},\"faces_total\":{faces_total},\"error\":{:?}}}", format!("{e:?}"));
            return;
        }
    };
    let tess_ms = t.elapsed().as_secs_f64() * 1e3;
    let v = &mesh.vertices;
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for p in v {
        for (i, c) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[i] = lo[i].min(c);
            hi[i] = hi[i].max(c);
        }
    }
    let (mut area, mut volume) = (0.0, 0.0);
    let key = |i: usize| ((v[i].x / 1e-4).round() as i64, (v[i].y / 1e-4).round() as i64, (v[i].z / 1e-4).round() as i64);
    let mut edges: HashMap<((i64, i64, i64), (i64, i64, i64)), u32> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (v[t[0]], v[t[1]], v[t[2]]);
        area += (b - a).cross(c - a).length() / 2.0;
        volume += a.dot(b.cross(c)) / 6.0;
        for (p, q) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (kp, kq) = (key(p), key(q));
            *edges.entry(if kp < kq { (kp, kq) } else { (kq, kp) }).or_default() += 1;
        }
    }
    let open = edges.values().filter(|&&n| n == 1).count();
    println!(
        "{{\"parse_ok\":true,\"parse_ms\":{parse_ms:.2},\"error\":null,\"faces_total\":{faces_total},\"faces_meshed\":{faces_total},\"faces_failed\":0,\"triangles\":{},\"tess_ms\":{tess_ms:.2},\"bbox_min\":{lo:?},\"bbox_max\":{hi:?},\"area\":{area:.4},\"volume\":{volume:.4},\"open_edges\":{open},\"colours_distinct\":null,\"faces_coloured\":null,\"products\":null,\"notes\":\"opencascade-rs exposes one merged shape; no colours or assembly; faces_meshed is assumed equal to faces_total\"}}",
        mesh.indices.len() / 3
    );
}
