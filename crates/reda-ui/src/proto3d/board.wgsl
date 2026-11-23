// PROTOTYPE (issue #433): the 3D board. Throwaway.

struct U {
    mvp: mat4x4<f32>,
    // Everything with dot(clip.xyz, world) > clip.w is cut away.
    clip: vec4<f32>,
    // x: how far the Layers are spread (0..1), y: 1 when the target is sRGB,
    // z: 1 for material shading and 0 for flat Layer colours.
    params: vec4<f32>,
    alpha0: vec4<f32>,
    alpha1: vec4<f32>,
    light: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: U;

struct In {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) lift: f32,
    @location(4) cat: u32,
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) cat: u32,
}

@vertex
fn vs_main(v: In) -> Out {
    var o: Out;
    let world = v.pos + vec3<f32>(0.0, 0.0, v.lift * u.params.x);
    o.clip = u.mvp * vec4<f32>(world, 1.0);
    o.world = world;
    o.normal = v.normal;
    o.color = v.color;
    o.cat = v.cat;
    return o;
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(i: Out) -> @location(0) vec4<f32> {
    if dot(u.clip.xyz, i.world) > u.clip.w {
        discard;
    }
    var a = u.alpha0[i.cat & 3u];
    if i.cat >= 4u {
        a = u.alpha1[i.cat & 3u];
    }
    a = a * i.color.a;
    if a <= 0.0 {
        discard;
    }
    let d = dot(normalize(i.normal), u.light.xyz);
    let diffuse = max(d, 0.0) + 0.3 * max(-d, 0.0);
    let ambient = mix(0.80, 0.42, u.params.z);
    var rgb = i.color.rgb * (ambient + (1.0 - ambient) * diffuse);
    if u.params.y > 0.5 {
        rgb = to_linear(rgb);
    }
    return vec4<f32>(rgb * a, a);
}
