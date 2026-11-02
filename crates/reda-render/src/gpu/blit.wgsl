// Copies a canvas's resolved frame into the window: a full-viewport triangle sampling the
// texture pixel for pixel. Source and destination share a format, so colour passes as is.

@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var frame_sampler: sampler;

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> Varyings {
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    var out: Varyings;
    out.position = vec4(x, y, 0.0, 1.0);
    out.uv = vec2((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    return textureSample(frame, frame_sampler, in.uv);
}
