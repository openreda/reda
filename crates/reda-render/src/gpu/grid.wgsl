// The canvas grid: anti-aliased dots or lines computed per pixel on a full-screen triangle, so the
// grid costs no geometry. Everything is in target pixels; the CPU works out the phase in
// f64 so far-off Document coordinates lose no precision.
//
// A grid may be turned (a Local Grid's), drawn only inside one quad (a Local Grid's area)
// and left out of others (the areas of the Local Grids that win over it).

const MAX_HOLES: u32 = 16u;

struct Grid {
    // Half the target size in pixels.
    half_size: vec2<f32>,
    // Where a grid point sits in the grid's own frame (see `frame`), reduced modulo the
    // pitch.
    phase: vec2<f32>,
    // The dot spacing along the grid's X and Y, and the dot radius, in pixels.
    pitch: vec2<f32>,
    radius: f32,
    // 1 when the target stores sRGB, so the shader must output linear colour.
    srgb: u32,
    // Straight-alpha sRGB colour.
    color: vec4<f32>,
    // The paper in target pixels: min x, min y, max x, max y. The grid stops at its edge.
    clip: vec4<f32>,
    // The coarse grid's phase and pitch in pixels; a zero pitch draws none.
    coarse_phase: vec2<f32>,
    coarse_pitch: vec2<f32>,
    // 1 draws lines, 0 dots.
    lines: u32,
    // From a pixel offset off the target centre to the grid's own frame: the rows of a
    // 2 × 2 matrix, (m00, m01, m10, m11).
    frame: vec4<f32>,
    // The quad the grid is drawn inside, as a corner and two edges in pixels off the
    // centre: (corner.x, corner.y, a.x, a.y) and (b.x, b.y, 1 when there is one, holes).
    inside_a: vec4<f32>,
    inside_b: vec4<f32>,
    // The quads it is left out of, two vec4 each as `inside_a` and `inside_b`.
    holes: array<vec4<f32>, 32>,
}

@group(0) @binding(0) var<uniform> grid: Grid;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    return vec4(x, y, 0.0, 1.0);
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

// Whether the pixel offset `p` lies in the quad at `corner` with edges `a` and `b`.
fn in_quad(p: vec2<f32>, corner: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> bool {
    let q = p - corner;
    let s = dot(q, a) / max(dot(a, a), 1e-12);
    let t = dot(q, b) / max(dot(b, b), 1e-12);
    return s >= 0.0 && s <= 1.0 && t >= 0.0 && t <= 1.0;
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    if any(frag.xy < grid.clip.xy) || any(frag.xy > grid.clip.zw) {
        discard;
    }
    let p = frag.xy - grid.half_size;
    if grid.inside_b.z > 0.5
        && !in_quad(p, grid.inside_a.xy, grid.inside_a.zw, grid.inside_b.xy) {
        discard;
    }
    let holes = min(u32(grid.inside_b.w), MAX_HOLES);
    for (var i = 0u; i < holes; i++) {
        let a = grid.holes[2u * i];
        let b = grid.holes[2u * i + 1u];
        if in_quad(p, a.xy, a.zw, b.xy) {
            discard;
        }
    }
    let own = vec2(dot(grid.frame.xy, p), dot(grid.frame.zw, p));
    let local = own - grid.phase;
    let d = abs(local - round(local / grid.pitch) * grid.pitch);
    var coarse = vec2(1e9, 1e9);
    if grid.coarse_pitch.x > 0.0 {
        let c = own - grid.coarse_phase;
        coarse = abs(c - round(c / grid.coarse_pitch) * grid.coarse_pitch);
    }
    var cover = 0.0;
    if grid.lines != 0u {
        // Fine lines one pixel wide at half strength, coarse ones at full strength.
        cover = clamp(1.0 - min(d.x, d.y), 0.0, 1.0) * 0.5;
        cover = max(cover, clamp(1.0 - min(coarse.x, coarse.y), 0.0, 1.0));
    } else {
        // Coarse dots are half again as big.
        cover = clamp(grid.radius + 0.5 - length(d), 0.0, 1.0);
        cover = max(cover, clamp(grid.radius * 1.5 + 0.5 - length(coarse), 0.0, 1.0));
    }
    let a = cover * grid.color.a;
    if a <= 0.0 {
        discard;
    }
    var rgb = grid.color.rgb;
    if grid.srgb != 0u {
        rgb = to_linear(rgb);
    }
    return vec4(rgb * a, a);
}
