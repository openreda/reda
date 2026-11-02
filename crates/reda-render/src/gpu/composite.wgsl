// Blends one Layer, drawn opaque into a texture of its own, over the frame at the Layer's
// opacity. A full-screen triangle; the opacity comes in thousandths as the draw's first
// vertex divided by three (a first vertex works on every backend), so no buffer is written
// per Layer. The texture holds premultiplied colour.

@group(0) @binding(0) var layer: texture_2d<f32>;

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) opacity: f32,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> Varyings {
    let corner = vi % 3u;
    let uv = vec2(f32((corner << 1u) & 2u), f32(corner & 2u));
    var out: Varyings;
    out.position = vec4(uv * 2.0 - 1.0, 0.0, 1.0);
    out.opacity = f32(vi / 3u) / 1000.0;
    return out;
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    return textureLoad(layer, vec2<i32>(in.position.xy), 0) * in.opacity;
}
