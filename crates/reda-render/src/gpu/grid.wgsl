// The canvas grid: anti-aliased dots or lines computed per pixel on a full-screen triangle, so the
// grid costs no geometry. Everything is in target pixels; the CPU works out the phase in
// f64 so far-off Document coordinates lose no precision.

struct Grid {
    // Half the target size in pixels.
    half_size: vec2<f32>,
    // Where a grid point sits relative to the target centre, reduced modulo the pitch.
    phase: vec2<f32>,
    // The dot spacing and radius in pixels.
    pitch: f32,
    radius: f32,
    // 1 when the target stores sRGB, so the shader must output linear colour.
    srgb: u32,
    _pad: u32,
    // Straight-alpha sRGB colour.
    color: vec4<f32>,
    // The paper in target pixels: min x, min y, max x, max y. The grid stops at its edge.
    clip: vec4<f32>,
    // The coarse grid's phase and pitch in pixels; a zero pitch draws none.
    coarse_phase: vec2<f32>,
    coarse_pitch: f32,
    // 1 draws lines, 0 dots.
    lines: u32,
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

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    if any(frag.xy < grid.clip.xy) || any(frag.xy > grid.clip.zw) {
        discard;
    }
    let local = frag.xy - grid.half_size - grid.phase;
    let d = abs(local - round(local / grid.pitch) * grid.pitch);
    var coarse = vec2(1e9, 1e9);
    if grid.coarse_pitch > 0.0 {
        let c = frag.xy - grid.half_size - grid.coarse_phase;
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
