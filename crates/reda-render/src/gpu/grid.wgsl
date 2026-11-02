// The canvas grid: anti-aliased dots computed per pixel on a full-screen triangle, so the
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
    let local = frag.xy - grid.half_size - grid.phase;
    let d = local - round(local / grid.pitch) * grid.pitch;
    let a = clamp(grid.radius + 0.5 - length(d), 0.0, 1.0) * grid.color.a;
    if a <= 0.0 {
        discard;
    }
    var rgb = grid.color.rgb;
    if grid.srgb != 0u {
        rgb = to_linear(rgb);
    }
    return vec4(rgb * a, a);
}
