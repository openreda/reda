// PROTOTYPE — synthetic dense 2-layer board. World units are millimetres (f32), y down.

pub const TOP: u8 = 0;
pub const BOTTOM: u8 = 1;

#[derive(Clone, Copy)]
pub struct Track {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub half_width: f32,
    pub layer: u8,
}

#[derive(Clone, Copy)]
pub struct Pad {
    pub center: [f32; 2],
    pub half: [f32; 2],
    /// Some(drill radius) for through-hole pads (on both layers), None for SMD on TOP.
    pub drill: Option<f32>,
}

#[derive(Clone, Copy)]
pub struct Via {
    pub center: [f32; 2],
    pub radius: f32,
    pub drill: f32,
}

/// Axis-aligned filled run of a Zone pour (the pour is stored pre-cut, as a fill cache would be).
#[derive(Clone, Copy)]
pub struct PourRun {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub layer: u8,
}

pub struct Board {
    pub size: [f32; 2],
    pub tracks: Vec<Track>,
    pub pads: Vec<Pad>,
    pub vias: Vec<Via>,
    pub pours: Vec<PourRun>,
    pub generation: u64,
}

/// Tiny xorshift RNG, deterministic.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }
}

impl Board {
    /// `density` 1.0 ≈ a busy 160×100 mm hobby board: ~12k pads, ~20k track segments,
    /// ~3k vias, a bottom ground pour cut around everything.
    pub fn generate(density: f32, generation: u64) -> Board {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let size = [160.0, 100.0];
        let mut tracks = Vec::new();
        let mut pads = Vec::new();
        let mut vias = Vec::new();

        // Footprints: rows of SOIC/QFN-ish pads and some through-hole headers.
        let n_fp = (750.0 * density) as usize;
        let mut pins: Vec<[f32; 2]> = Vec::new();
        for i in 0..n_fp {
            let cx = rng.range(4.0, size[0] - 4.0);
            let cy = rng.range(4.0, size[1] - 4.0);
            if i % 10 == 0 {
                // 2x8 through-hole header, 2.54 mm pitch
                for r in 0..8 {
                    for c in 0..2 {
                        let p = [cx + c as f32 * 2.54, cy + r as f32 * 2.54 - 9.0];
                        pads.push(Pad { center: p, half: [0.85, 0.85], drill: Some(0.5) });
                        pins.push(p);
                    }
                }
            } else {
                // 2x8 SMD, 0.65 mm pitch
                for r in 0..8 {
                    for c in 0..2 {
                        let p = [cx + c as f32 * 5.0 - 2.5, cy + r as f32 * 0.65 - 2.3];
                        pads.push(Pad { center: p, half: [0.7, 0.2], drill: None });
                        pins.push(p);
                    }
                }
            }
        }

        // Connections: short 45°/90° polylines between nearby pins, sometimes via to bottom.
        let n_conn = (6000.0 * density) as usize;
        for _ in 0..n_conn {
            let a = pins[(rng.next() % pins.len() as u64) as usize];
            let dx = rng.range(-12.0, 12.0);
            let dy = rng.range(-12.0, 12.0);
            let b = [
                (a[0] + dx).clamp(1.0, size[0] - 1.0),
                (a[1] + dy).clamp(1.0, size[1] - 1.0),
            ];
            let hw = if rng.f() < 0.15 { 0.25 } else { 0.1 };
            // 45° then orthogonal
            let (ddx, ddy) = (b[0] - a[0], b[1] - a[1]);
            let d = ddx.abs().min(ddy.abs());
            let mid = [a[0] + d * ddx.signum(), a[1] + d * ddy.signum()];
            if rng.f() < 0.35 {
                // change layers at mid with a via
                tracks.push(Track { a, b: mid, half_width: hw, layer: TOP });
                vias.push(Via { center: mid, radius: 0.3, drill: 0.15 });
                tracks.push(Track { a: mid, b, half_width: hw, layer: BOTTOM });
                vias.push(Via { center: b, radius: 0.3, drill: 0.15 });
            } else {
                tracks.push(Track { a, b: mid, half_width: hw, layer: TOP });
                tracks.push(Track { a: mid, b, half_width: hw, layer: TOP });
            }
        }

        // Bottom ground pour: 0.25 mm cells, cut where bottom copper / holes sit (+clearance),
        // merged into horizontal runs.
        let cell = 0.25f32;
        let (gw, gh) = ((size[0] / cell) as usize, (size[1] / cell) as usize);
        let mut blocked = vec![false; gw * gh];
        let mut stamp = |min: [f32; 2], max: [f32; 2]| {
            let x0 = ((min[0] / cell).floor().max(0.0)) as usize;
            let y0 = ((min[1] / cell).floor().max(0.0)) as usize;
            let x1 = ((max[0] / cell).ceil() as usize).min(gw);
            let y1 = ((max[1] / cell).ceil() as usize).min(gh);
            for y in y0..y1 {
                for x in x0..x1 {
                    blocked[y * gw + x] = true;
                }
            }
        };
        let clr = 0.3;
        for t in tracks.iter().filter(|t| t.layer == BOTTOM) {
            // stamp along the segment
            let len = ((t.b[0] - t.a[0]).hypot(t.b[1] - t.a[1]) / cell).ceil().max(1.0) as usize;
            for s in 0..=len {
                let u = s as f32 / len as f32;
                let p = [t.a[0] + (t.b[0] - t.a[0]) * u, t.a[1] + (t.b[1] - t.a[1]) * u];
                let r = t.half_width + clr;
                stamp([p[0] - r, p[1] - r], [p[0] + r, p[1] + r]);
            }
        }
        for v in &vias {
            let r = v.radius + clr;
            stamp([v.center[0] - r, v.center[1] - r], [v.center[0] + r, v.center[1] + r]);
        }
        for p in pads.iter().filter(|p| p.drill.is_some()) {
            stamp(
                [p.center[0] - p.half[0] - clr, p.center[1] - p.half[1] - clr],
                [p.center[0] + p.half[0] + clr, p.center[1] + p.half[1] + clr],
            );
        }
        let mut pours = Vec::new();
        for y in 0..gh {
            let mut x = 0;
            while x < gw {
                if blocked[y * gw + x] {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < gw && !blocked[y * gw + x] {
                    x += 1;
                }
                pours.push(PourRun {
                    min: [start as f32 * cell, y as f32 * cell],
                    max: [x as f32 * cell, (y + 1) as f32 * cell],
                    layer: BOTTOM,
                });
            }
        }

        Board { size, tracks, pads, vias, pours, generation }
    }
}

/// Layer colours, premultiplied-free RGBA in 0..1 (translucent so both layers show).
pub const TOP_COLOR: [f32; 4] = [0.85, 0.20, 0.20, 0.80];
pub const BOTTOM_COLOR: [f32; 4] = [0.25, 0.45, 0.95, 0.70];
pub const POUR_COLOR: [f32; 4] = [0.20, 0.35, 0.80, 0.35];
pub const PAD_COLOR: [f32; 4] = [0.90, 0.75, 0.25, 0.95];
pub const VIA_COLOR: [f32; 4] = [0.75, 0.75, 0.75, 0.95];
pub const HOLE_COLOR: [f32; 4] = [0.05, 0.05, 0.06, 1.0];
