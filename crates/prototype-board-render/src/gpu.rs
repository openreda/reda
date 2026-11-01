// PROTOTYPE — custom wgpu renderer inside egui via a paint callback.
//
// Board geometry is uploaded ONCE as instanced SDF primitives (capsule / rect / ring) plus a
// triangle list for pours. Per frame, only a 48-byte uniform (view transform) is written.
// Anti-aliasing is analytic in the fragment shader, so it stays crisp at every zoom.

use crate::board::*;
use eframe::egui;
use eframe::egui_wgpu::{self, wgpu, wgpu::util::DeviceExt};
use std::sync::Arc;

const SHADER: &str = r#"
struct U { view: vec4f, params: vec4f, pour: vec4f };
@group(0) @binding(0) var<uniform> u: U;

struct VOut {
  @builtin(position) pos: vec4f,
  @location(0) world: vec2f,
  @location(1) @interpolate(flat) a: vec2f,
  @location(2) @interpolate(flat) b: vec2f,
  @location(3) @interpolate(flat) rk: vec2f,
  @location(4) color: vec4f,
};

@vertex
fn vs_inst(@builtin(vertex_index) vi: u32,
           @location(0) ab: vec4f, @location(1) rk: vec2f, @location(2) color: vec4f) -> VOut {
  var corners = array<vec2f, 6>(vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0),
                                vec2f(0.0, 0.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0));
  let c = corners[vi];
  let a = ab.xy; let b = ab.zw; let r = rk.x; let kind = rk.y;
  let aa = u.params.x * 1.5;
  var p: vec2f;
  if (kind < 0.5) {
    let d = b - a; let len = length(d);
    var dir = vec2f(1.0, 0.0);
    if (len > 1e-6) { dir = d / len; }
    let nrm = vec2f(-dir.y, dir.x);
    let e = r + aa;
    p = a + dir * mix(-e, len + e, c.x) + nrm * mix(-e, e, c.y);
  } else if (kind < 1.5) {
    let h = b + vec2f(aa);
    p = a + mix(-h, h, c);
  } else {
    let h = vec2f(r + aa);
    p = a + mix(-h, h, c);
  }
  var o: VOut;
  o.pos = vec4f(p * u.view.xy + u.view.zw, 0.0, 1.0);
  o.world = p; o.a = a; o.b = b; o.rk = rk; o.color = color;
  return o;
}

@fragment
fn fs_inst(i: VOut) -> @location(0) vec4f {
  let kind = i.rk.y; let r = i.rk.x;
  var d: f32;
  if (kind < 0.5) {
    let pa = i.world - i.a; let ba = i.b - i.a;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-12), 0.0, 1.0);
    d = length(pa - ba * h) - r;
  } else if (kind < 1.5) {
    let q = abs(i.world - i.a) - i.b;
    d = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0);
  } else {
    let ri = i.b.x;
    d = abs(length(i.world - i.a) - (r + ri) * 0.5) - (r - ri) * 0.5;
  }
  let cov = clamp(0.5 - d / u.params.x, 0.0, 1.0);
  let alpha = i.color.a * cov;
  if (alpha <= 0.001) { discard; }
  return vec4f(i.color.rgb * alpha, alpha);
}

@vertex
fn vs_pour(@location(0) p: vec2f) -> @builtin(position) vec4f {
  return vec4f(p * u.view.xy + u.view.zw, 0.0, 1.0);
}

@fragment
fn fs_pour() -> @location(0) vec4f {
  return vec4f(u.pour.rgb * u.pour.a, u.pour.a);
}
"#;

/// Draw ranges (in instances) for the layer-visibility toggles.
#[derive(Default, Clone)]
struct Ranges {
    bottom: std::ops::Range<u32>,
    top: std::ops::Range<u32>,
    pads: std::ops::Range<u32>,
    vias: std::ops::Range<u32>,
}

pub struct GpuBoard {
    inst_pipeline: wgpu::RenderPipeline,
    pour_pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    generation: u64,
    instances: Option<wgpu::Buffer>,
    pour_verts: Option<wgpu::Buffer>,
    pour_count: u32,
    ranges: Ranges,
}

pub fn install(rs: &egui_wgpu::RenderState, msaa: u32) {
    let device = &rs.device;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("board"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("board_uniform"),
        size: 48,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("board_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("board_bg"),
        layout: &bgl,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("board_layout"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let target = [Some(wgpu::ColorTargetState {
        format: rs.target_format,
        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    let make = |vs: &str, fs: &str, buffers: &[Option<wgpu::VertexBufferLayout>]| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(vs),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some(vs),
                buffers,
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: msaa.max(1), mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(fs),
                targets: &target,
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let inst_pipeline = make(
        "vs_inst",
        "fs_inst",
        &[Some(wgpu::VertexBufferLayout {
            array_stride: 10 * 4,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2, 2 => Float32x4],
        })],
    );
    let pour_pipeline = make(
        "vs_pour",
        "fs_pour",
        &[Some(wgpu::VertexBufferLayout {
            array_stride: 2 * 4,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2],
        })],
    );
    rs.renderer.write().callback_resources.insert(GpuBoard {
        inst_pipeline,
        pour_pipeline,
        uniform,
        bind_group,
        generation: u64::MAX,
        instances: None,
        pour_verts: None,
        pour_count: 0,
        ranges: Ranges::default(),
    });
}

fn push(v: &mut Vec<f32>, a: [f32; 2], b: [f32; 2], r: f32, kind: f32, c: [f32; 4]) {
    v.extend_from_slice(&[a[0], a[1], b[0], b[1], r, kind]);
    v.extend_from_slice(&c);
}

fn bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

impl GpuBoard {
    fn upload(&mut self, device: &wgpu::Device, board: &Board) {
        let mut v = Vec::new();
        let n = |v: &Vec<f32>| (v.len() / 10) as u32;
        let s0 = n(&v);
        for t in board.tracks.iter().filter(|t| t.layer == BOTTOM) {
            push(&mut v, t.a, t.b, t.half_width, 0.0, BOTTOM_COLOR);
        }
        let s1 = n(&v);
        for t in board.tracks.iter().filter(|t| t.layer == TOP) {
            push(&mut v, t.a, t.b, t.half_width, 0.0, TOP_COLOR);
        }
        let s2 = n(&v);
        for p in &board.pads {
            push(&mut v, p.center, p.half, 0.0, 1.0, PAD_COLOR);
            if let Some(d) = p.drill {
                push(&mut v, p.center, [0.0, 0.0], d, 2.0, HOLE_COLOR);
            }
        }
        let s3 = n(&v);
        for via in &board.vias {
            push(&mut v, via.center, [0.0, 0.0], via.radius, 2.0, VIA_COLOR);
            push(&mut v, via.center, [0.0, 0.0], via.drill, 2.0, HOLE_COLOR);
        }
        let s4 = n(&v);
        self.ranges = Ranges { bottom: s0..s1, top: s1..s2, pads: s2..s3, vias: s3..s4 };
        self.instances = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("board_instances"),
            contents: &bytes(&v),
            usage: wgpu::BufferUsages::VERTEX,
        }));
        let mut p = Vec::with_capacity(board.pours.len() * 12);
        for r in &board.pours {
            let (a, b) = (r.min, r.max);
            p.extend_from_slice(&[a[0], a[1], b[0], a[1], b[0], b[1], a[0], a[1], b[0], b[1], a[0], b[1]]);
        }
        self.pour_count = (p.len() / 2) as u32;
        self.pour_verts = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("board_pours"),
            contents: &bytes(&p),
            usage: wgpu::BufferUsages::VERTEX,
        }));
        self.generation = board.generation;
    }
}

pub struct Visible {
    pub pour: bool,
    pub bottom: bool,
    pub top: bool,
    pub pads: bool,
    pub vias: bool,
}

pub struct BoardCallback {
    pub board: Arc<Board>,
    /// clip = world * view.xy + view.zw
    pub view: [f32; 4],
    pub world_per_px: f32,
    pub vis: Visible,
}

impl egui_wgpu::CallbackTrait for BoardCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _sd: &egui_wgpu::ScreenDescriptor,
        _enc: &mut wgpu::CommandEncoder,
        res: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let g: &mut GpuBoard = res.get_mut().unwrap();
        if g.generation != self.board.generation {
            g.upload(device, &self.board);
        }
        let mut u = Vec::with_capacity(12);
        u.extend_from_slice(&self.view);
        u.extend_from_slice(&[self.world_per_px, 0.0, 0.0, 0.0]);
        u.extend_from_slice(&POUR_COLOR);
        queue.write_buffer(&g.uniform, 0, &bytes(&u));
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        rp: &mut wgpu::RenderPass<'static>,
        res: &egui_wgpu::CallbackResources,
    ) {
        let g: &GpuBoard = res.get().unwrap();
        rp.set_bind_group(0, &g.bind_group, &[]);
        if self.vis.pour
            && let Some(b) = &g.pour_verts
        {
            rp.set_pipeline(&g.pour_pipeline);
            rp.set_vertex_buffer(0, b.slice(..));
            rp.draw(0..g.pour_count, 0..1);
        }
        if let Some(b) = &g.instances {
            rp.set_pipeline(&g.inst_pipeline);
            rp.set_vertex_buffer(0, b.slice(..));
            let r = &g.ranges;
            for (on, range) in [
                (self.vis.bottom, &r.bottom),
                (self.vis.top, &r.top),
                (self.vis.pads, &r.pads),
                (self.vis.vias, &r.vias),
            ] {
                if on {
                    rp.draw(0..6, range.clone());
                }
            }
        }
    }
}
