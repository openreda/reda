// REDA's canvas shader: every Scene primitive is one instance. Capsules, rects and rings are
// signed-distance shapes drawn on a padded quad with analytic anti-aliasing; triangles are
// drawn as they are and smoothed by MSAA. Coordinates are Document nm relative to the
// Scene's origin, as f32.

struct View {
    // The point (relative to the Scene origin) at the image centre.
    center: vec2<f32>,
    // Pixels per nm along x and y; x is negative when the view is mirrored.
    scale: vec2<f32>,
    // Half the image size in pixels.
    half_size: vec2<f32>,
    // 1 when the target stores sRGB, so the shader must output linear colour.
    srgb: u32,
    _pad: u32,
    // How far objects marked MOVING are drawn from where they are, in nm.
    moving: vec2<f32>,
    _pad2: vec2<f32>,
}

struct Palette {
    background: vec4<f32>,
    colors: array<vec4<f32>, 128>,
}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> palette: Palette;

const CAPSULE: u32 = 0u;
const RECT: u32 = 1u;
const RING: u32 = 2u;
const TRIANGLE: u32 = 3u;

const SELECTED: u32 = 1u;
const HOVERED: u32 = 2u;
const HIGHLIGHTED: u32 = 4u;
const DIMMED: u32 = 8u;
const MOVING: u32 = 16u;
const HIDDEN: u32 = 32u;

// The selection halo's width in pixels.
const HALO_PX: f32 = 3.0;
// View-only labels smaller than this many pixels are hidden.
const MIN_LABEL_PX: f32 = 4.0;

struct Instance {
    @location(0) p0: vec2<f32>,
    @location(1) p1: vec2<f32>,
    @location(2) p2: vec2<f32>,
    @location(3) p3: vec2<f32>,
    @location(4) kind_color: vec2<u32>,
    @location(5) extra: vec2<f32>,
    @location(6) flags: u32,
}

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec2<f32>,
    @location(1) @interpolate(flat) p0: vec2<f32>,
    @location(2) @interpolate(flat) p1: vec2<f32>,
    @location(3) @interpolate(flat) p2: vec2<f32>,
    @location(4) @interpolate(flat) p3: vec2<f32>,
    @location(5) @interpolate(flat) kind_color: vec2<u32>,
    @location(6) @interpolate(flat) flags: u32,
    @location(7) @interpolate(flat) hidden: u32,
}

fn px_per_nm() -> f32 {
    return abs(view.scale.y);
}

// Quad corners for the six vertices of two triangles.
fn corner(i: u32) -> vec2<f32> {
    switch i {
        case 0u: { return vec2(-1.0, -1.0); }
        case 1u: { return vec2(1.0, -1.0); }
        case 2u: { return vec2(1.0, 1.0); }
        case 3u: { return vec2(-1.0, -1.0); }
        case 4u: { return vec2(1.0, 1.0); }
        default: { return vec2(-1.0, 1.0); }
    }
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> Varyings {
    let kind = inst.kind_color.x;
    // Room around the shape for the anti-aliased edge and the halo, in nm.
    let pad = (1.5 + HALO_PX) / px_per_nm();
    let c = corner(vi);
    var world: vec2<f32>;
    switch kind {
        case CAPSULE: {
            let d = inst.p1 - inst.p0;
            let len = length(d);
            var dir = vec2(1.0, 0.0);
            if len > 0.0 {
                dir = d / len;
            }
            let normal = vec2(-dir.y, dir.x);
            let r = max(inst.p2.x, 0.5 / px_per_nm()) + pad;
            let mid = (inst.p0 + inst.p1) * 0.5;
            world = mid + dir * c.x * (len * 0.5 + r) + normal * c.y * r;
        }
        case RECT: {
            let stroke = max(inst.p3.y, 0.0);
            let half = inst.p1 + vec2(stroke + pad);
            let local = c * half;
            let cs = inst.p2;
            world = inst.p0 + vec2(local.x * cs.x - local.y * cs.y, local.x * cs.y + local.y * cs.x);
        }
        case RING: {
            world = inst.p0 + c * (inst.p1.x + pad);
        }
        default: {
            switch vi {
                case 0u: { world = inst.p0; }
                case 1u: { world = inst.p1; }
                case 2u: { world = inst.p2; }
                default: { world = inst.p0; }
            }
        }
    }
    var out: Varyings;
    // A moving object is drawn at the offset; the fragment stage still measures its shape
    // against the unmoved geometry, as both are passed on unmoved.
    var shift = vec2(0.0);
    if (inst.flags & MOVING) != 0u {
        shift = view.moving;
    }
    let ndc = (world + shift - view.center) * view.scale / view.half_size;
    out.position = vec4(ndc, 0.0, 1.0);
    out.world = world;
    out.p0 = inst.p0;
    out.p1 = inst.p1;
    out.p2 = inst.p2;
    out.p3 = inst.p3;
    out.kind_color = inst.kind_color;
    out.flags = inst.flags;
    // A view-only label below its minimum on-screen size is hidden.
    out.hidden = select(0u, 1u, inst.extra.x > 0.0 && inst.extra.x * px_per_nm() < MIN_LABEL_PX);
    if (inst.flags & HIDDEN) != 0u {
        out.hidden = 1u;
    }
    return out;
}

fn segment_distance(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let len2 = dot(ba, ba);
    var h = 0.0;
    if len2 > 0.0 {
        h = clamp(dot(pa, ba) / len2, 0.0, 1.0);
    }
    return length(pa - ba * h);
}

fn rounded_box(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - (half - vec2(r));
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

@fragment
fn fs_main(in: Varyings) -> @location(0) vec4<f32> {
    if in.hidden != 0u {
        discard;
    }
    let kind = in.kind_color.x;
    let scale = px_per_nm();
    // Signed distance to the shape's edge in pixels (negative inside), and a coverage
    // factor for strokes thinner than a pixel.
    var d_px = -1.0;
    var thin = 1.0;
    switch kind {
        case CAPSULE: {
            let r_px = in.p2.x * scale;
            let r = max(r_px, 0.5);
            thin = min(r_px / 0.5, 1.0);
            d_px = segment_distance(in.world, in.p0, in.p1) * scale - r;
        }
        case RECT: {
            let rel = in.world - in.p0;
            let cs = in.p2;
            let local = vec2(rel.x * cs.x + rel.y * cs.y, -rel.x * cs.y + rel.y * cs.x);
            let d = rounded_box(local, in.p1, min(in.p3.x, min(in.p1.x, in.p1.y))) * scale;
            if in.p3.y < 0.0 {
                d_px = d;
            } else {
                let s_px = in.p3.y * scale;
                let s = max(s_px, 0.5);
                thin = min(s_px / 0.5, 1.0);
                d_px = abs(d) - s;
            }
        }
        case RING: {
            let len = length(in.world - in.p0);
            if in.p1.y > 0.0 {
                let w_px = (in.p1.x - in.p1.y) * scale * 0.5;
                let w = max(w_px, 0.5);
                thin = min(w_px / 0.5, 1.0);
                let mid = (in.p1.x + in.p1.y) * 0.5;
                d_px = abs(len - mid) * scale - w;
            } else {
                d_px = (len - in.p1.x) * scale;
            }
        }
        default: {}
    }
    var alpha = clamp(0.5 - d_px, 0.0, 1.0) * thin;
    if kind == TRIANGLE {
        alpha = 1.0;
    }

    var color = palette.colors[in.kind_color.y];
    var rgb = color.rgb;
    if (in.flags & HOVERED) != 0u {
        rgb = mix(rgb, vec3(1.0), 0.15);
    }
    if (in.flags & SELECTED) != 0u {
        rgb = mix(rgb, vec3(1.0), 0.35);
    }
    if (in.flags & DIMMED) != 0u {
        rgb = mix(rgb, palette.background.rgb, 0.7);
    }
    var a = alpha * color.a;

    // The halo: a soft band just outside a selected shape.
    var halo_rgb = vec3(0.0);
    var halo_a = 0.0;
    if (in.flags & SELECTED) != 0u && kind != TRIANGLE {
        halo_a = clamp(0.5 - (d_px - HALO_PX), 0.0, 1.0) * 0.45 * (1.0 - a);
        halo_rgb = mix(rgb, vec3(1.0), 0.5);
    }
    if a + halo_a <= 0.0 {
        discard;
    }
    if view.srgb != 0u {
        rgb = to_linear(rgb);
        halo_rgb = to_linear(halo_rgb);
    }
    // Premultiplied: the shape over its halo.
    return vec4(rgb * a + halo_rgb * halo_a, a + halo_a);
}
