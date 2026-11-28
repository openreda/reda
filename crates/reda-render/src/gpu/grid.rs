//! The canvas grid, drawn by a shader: dots or lines computed per pixel, no geometry, with
//! every `coarse`-th one drawn stronger. A canvas may draw several grids: the everyday
//! one with holes where Local Grids win, and each Local Grid turned and inside its area.

use crate::color::Rgba;
use crate::view::View;

use super::{Renderer, SAMPLE_COUNT};

const UNIFORM_SIZE: u64 = 656;

/// How many quads a grid can be left out of; past that the rest are drawn over.
pub const MAX_HOLES: usize = 16;

/// A parallelogram on the Document (nm): a corner and the two edges from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridQuad {
    pub corner: (f64, f64),
    pub a: (f64, f64),
    pub b: (f64, f64),
}

/// What grid to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct GridSpec {
    /// A Document point (nm) that lies on the grid.
    pub origin: (f64, f64),
    /// The fine dot or line spacing in nm, along X and along Y.
    pub pitch: (f64, f64),
    /// Every how many fine dots or lines a coarse one is drawn (bigger dots, stronger
    /// lines); 0 or 1 draws none.
    pub coarse: u32,
    /// Lines instead of dots: fine lines faint, coarse lines at full colour.
    pub lines: bool,
    /// The dot radius in pixels.
    pub radius_px: f32,
    pub color: Rgba,
    /// The sheet's paper, as two opposite Document corners (nm): the grid stops at its
    /// edge. `None` (a board or a Library element) draws it over the whole canvas.
    pub paper: Option<((f64, f64), (f64, f64))>,
    /// The grid's own X axis, counter-clockwise from the Document's, in radians; `pitch`
    /// is along its own axes.
    pub angle: f64,
    /// The area it is drawn inside; `None` is everywhere.
    pub inside: Option<GridQuad>,
    /// Areas it is left out of (at most [`MAX_HOLES`]).
    pub outside: Vec<GridQuad>,
}

impl GridSpec {
    /// An unturned grid over the whole canvas.
    pub fn new(origin: (f64, f64), pitch: (f64, f64), color: Rgba) -> GridSpec {
        GridSpec {
            origin,
            pitch,
            coarse: 1,
            lines: false,
            radius_px: 1.5,
            color,
            paper: None,
            angle: 0.0,
            inside: None,
            outside: Vec::new(),
        }
    }
}

/// One grid's uniforms.
struct Layer {
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// The grid pipeline and its uniforms, for one canvas. Draw it into the canvas pass after
/// the Scene ([`Target::record_with`](super::Target::record_with)).
pub struct Grid {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    layers: Vec<Layer>,
    srgb: bool,
    /// How many of `layers` are drawn.
    visible: usize,
}

impl Grid {
    /// A grid for passes into `renderer`'s format with [`SAMPLE_COUNT`] samples.
    pub fn new(device: &wgpu::Device, renderer: &Renderer) -> Grid {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("reda grid shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("grid.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reda grid uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(UNIFORM_SIZE),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("reda grid layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("reda grid pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: renderer.format(),
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let first = Grid::layer(device, &layout);
        Grid {
            pipeline,
            layout,
            layers: vec![first],
            srgb: renderer.format().is_srgb(),
            visible: 0,
        }
    }

    fn layer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Layer {
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reda grid uniforms"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reda grid uniforms"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        Layer {
            uniforms,
            bind_group,
        }
    }

    /// Writes the grid for `view`; `None` hides it.
    pub fn set(&mut self, queue: &wgpu::Queue, view: &View, spec: Option<&GridSpec>) {
        self.visible = usize::from(spec.is_some());
        if let Some(spec) = spec {
            self.write(queue, 0, view, spec);
        }
    }

    /// Writes every grid of `specs` for `view`, drawn in order; none hides them all.
    pub fn set_all(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &View,
        specs: &[GridSpec],
    ) {
        while self.layers.len() < specs.len() {
            let layer = Grid::layer(device, &self.layout);
            self.layers.push(layer);
        }
        self.visible = specs.len();
        for (i, spec) in specs.iter().enumerate() {
            self.write(queue, i, view, spec);
        }
    }

    fn write(&self, queue: &wgpu::Queue, layer: usize, view: &View, spec: &GridSpec) {
        let pitch_x = spec.pitch.0 / view.nm_per_px;
        let pitch_y = spec.pitch.1 / view.nm_per_px;
        let mirror = if view.mirrored { -1.0 } else { 1.0 };
        // A Document point and vector as pixels off the target centre (y down).
        let px = |(x, y): (f64, f64)| {
            (
                (x - view.center.0) / view.nm_per_px * mirror,
                -(y - view.center.1) / view.nm_per_px,
            )
        };
        let vector = |(x, y): (f64, f64)| (x / view.nm_per_px * mirror, -y / view.nm_per_px);
        // From pixels off the centre to the grid's own frame, turned back by its angle.
        let (s, c) = spec.angle.sin_cos();
        let frame = [c * mirror, -s, -s * mirror, -c];
        let own = |(x, y): (f64, f64)| (frame[0] * x + frame[1] * y, frame[2] * x + frame[3] * y);
        let (dx, dy) = own(px(spec.origin));
        let floats = [
            view.width as f32 / 2.0,
            view.height as f32 / 2.0,
            dx.rem_euclid(pitch_x) as f32,
            dy.rem_euclid(pitch_y) as f32,
            pitch_x as f32,
            pitch_y as f32,
            spec.radius_px,
        ];
        let mut bytes: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
        bytes.extend_from_slice(&u32::from(self.srgb).to_le_bytes());
        for v in spec.color.0 {
            bytes.extend_from_slice(&(f32::from(v) / 255.0).to_le_bytes());
        }
        // The paper in target pixels (y down), as min x, min y, max x, max y.
        let clip = match spec.paper {
            Some((a, b)) => {
                let target = |(x, y): (f64, f64)| {
                    (
                        (x - view.center.0) / view.nm_per_px * mirror + view.width as f64 / 2.0,
                        view.height as f64 / 2.0 - (y - view.center.1) / view.nm_per_px,
                    )
                };
                let (a, b) = (target(a), target(b));
                [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)].map(|v| v as f32)
            }
            None => [f32::MIN, f32::MIN, f32::MAX, f32::MAX],
        };
        for v in clip {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        // The coarse grid: its phase and pitch, or a zero pitch for none.
        let coarse = |pitch_px: f64| {
            if spec.coarse > 1 {
                pitch_px * f64::from(spec.coarse)
            } else {
                0.0
            }
        };
        let (coarse_x, coarse_y) = (coarse(pitch_x), coarse(pitch_y));
        let coarse_phase = |d: f64, coarse_px: f64| {
            if coarse_px > 0.0 {
                d.rem_euclid(coarse_px) as f32
            } else {
                0.0
            }
        };
        for v in [
            coarse_phase(dx, coarse_x),
            coarse_phase(dy, coarse_y),
            coarse_x as f32,
            coarse_y as f32,
        ] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&u32::from(spec.lines).to_le_bytes());
        // `frame` starts on the next 16 bytes.
        bytes.resize(96, 0);
        let mut push = |values: &[f64]| {
            for v in values {
                bytes.extend_from_slice(&(*v as f32).to_le_bytes());
            }
        };
        push(&frame);
        let quad = |q: &GridQuad| {
            let (corner, a, b) = (px(q.corner), vector(q.a), vector(q.b));
            [corner.0, corner.1, a.0, a.1, b.0, b.1]
        };
        let holes = spec.outside.len().min(MAX_HOLES);
        let [cx, cy, ax, ay, bx, by] = spec.inside.as_ref().map(quad).unwrap_or_default();
        let has = if spec.inside.is_some() { 1.0 } else { 0.0 };
        push(&[cx, cy, ax, ay, bx, by, has, holes as f64]);
        for hole in spec.outside.iter().take(MAX_HOLES) {
            let [cx, cy, ax, ay, bx, by] = quad(hole);
            push(&[cx, cy, ax, ay, bx, by, 0.0, 0.0]);
        }
        bytes.resize(UNIFORM_SIZE as usize, 0);
        queue.write_buffer(&self.layers[layer].uniforms, 0, &bytes);
    }

    /// Records the grids into `pass` (nothing when hidden).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.visible == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        for layer in &self.layers[..self.visible] {
            pass.set_bind_group(0, &layer.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
