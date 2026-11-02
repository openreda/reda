//! The wgpu backend: draws a [`Scene`] as instanced SDF primitives with analytic
//! anti-aliasing into a 4× MSAA target.
//!
//! The pieces are independent of any window or surface, so the app can wrap them in an
//! `egui_wgpu` paint callback and `document.render` can run them headless:
//!
//! - [`Renderer`]: the pipeline for one target format. Make one per device and format.
//! - [`GpuScene`]: a Scene's geometry, uploaded once ([`GpuScene::new`], and
//!   [`GpuScene::replace`] when the Document changes), plus its per-instance flag buffer
//!   ([`GpuScene::set_highlight`]), which is written without touching the geometry.
//! - [`Canvas`]: the per-canvas view transform and palette. A frame writes only the view.
//! - [`Renderer::draw`] records the draw into any render pass with the renderer's format
//!   and [`SAMPLE_COUNT`]; [`Target`] is an offscreen MSAA texture with its resolve texture.
//! - [`Grid`]: the canvas grid, drawn by a shader into the canvas pass after the Scene.
//! - [`Blit`]: copies a [`Target`]'s frame into a window's pass (egui's, in the app).
//! - [`Headless`]: a device of its own (a software adapter when there is one) that renders
//!   a Scene to an [`Image`].

mod blit;
mod grid;
mod headless;
mod instances;
mod target;

use std::ops::Range;

use reda_model::geom::Point;
use wgpu::util::DeviceExt;

use crate::color::{ColorKey, Palette, Rgba};
use crate::scene::{Scene, SceneLayer};
use crate::view::View;

pub use blit::{Blit, BlitSource};
pub use grid::{Grid, GridSpec};
pub use headless::{Headless, Image, RenderError};
pub use instances::{Flags, Highlight};
pub use target::Target;
/// The wgpu this backend is built on, so embedders use the same version.
pub use wgpu;

/// The MSAA sample count every canvas pass uses (4× is guaranteed on WebGPU).
pub const SAMPLE_COUNT: u32 = 4;

const VIEW_SIZE: u64 = 32;
const PALETTE_SIZE: u64 = 16 * (1 + ColorKey::SLOTS as u64);

/// The canvas pipeline for one target format.
pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    composite: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
}

impl Renderer {
    /// Builds the pipeline for render passes into `format` with [`SAMPLE_COUNT`] samples.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Renderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("reda canvas shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu/scene.wgsl").into()),
        });
        let uniform = |binding, size| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size),
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reda canvas uniforms"),
            entries: &[uniform(0, VIEW_SIZE), uniform(1, PALETTE_SIZE)],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("reda canvas layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let instance_attrs = wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x2,
            4 => Uint32x2, 5 => Float32x2,
        ];
        let flag_attrs = wgpu::vertex_attr_array![6 => Uint32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("reda canvas pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: instances::INSTANCE_SIZE,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &instance_attrs,
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 4,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &flag_attrs,
                    }),
                ],
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
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let (composite, composite_layout) = composite_pipeline(device, format);
        Renderer {
            pipeline,
            layout,
            composite,
            composite_layout,
            format,
        }
    }

    /// Blends a Layer texture bound by `source` over a single-sample pass at `opacity`.
    fn composite(&self, pass: &mut wgpu::RenderPass<'_>, source: &wgpu::BindGroup, opacity: f32) {
        let thousandths = (opacity.clamp(0.0, 1.0) * 1000.0).round() as u32;
        pass.set_pipeline(&self.composite);
        pass.set_bind_group(0, source, &[]);
        pass.draw(3 * thousandths..3 * thousandths + 3, 0..1);
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Records the draw of `scene` as `canvas` shows it into `pass`, which must target the
    /// renderer's format with [`SAMPLE_COUNT`] samples. `layers` picks the Layers to draw
    /// and their order (bottom first); `None` draws every Layer in Scene order.
    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        canvas: &Canvas,
        scene: &GpuScene,
        layers: Option<&[SceneLayer]>,
    ) {
        if scene.encoded.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &canvas.bind_group, &[]);
        pass.set_vertex_buffer(0, scene.instances.slice(..));
        pass.set_vertex_buffer(1, scene.flags.slice(..));
        match layers {
            None => pass.draw(0..6, 0..scene.encoded.count),
            Some(layers) => {
                for layer in layers {
                    if let Some(range) = scene.layer_range(*layer) {
                        pass.draw(0..6, range);
                    }
                }
            }
        }
    }
}

/// The pipeline that blends a resolved Layer texture over the frame (`gpu/composite.wgsl`).
fn composite_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("reda composite shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("gpu/composite.wgsl").into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("reda composite source"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("reda composite layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("reda composite pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, layout)
}

/// How many times each kind of buffer has been written, for checking that a frame writes
/// only what it must.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Writes {
    /// Geometry uploads: one per [`GpuScene::new`] or [`GpuScene::replace`].
    pub geometry: u32,
    /// Flag buffer writes.
    pub flags: u32,
    /// View transform writes.
    pub view: u32,
    /// Palette writes.
    pub palette: u32,
}

/// A Scene's geometry on the GPU, with its per-instance flag buffer.
pub struct GpuScene {
    instances: wgpu::Buffer,
    flags: wgpu::Buffer,
    encoded: instances::Encoded,
    bounds: Option<(Point, Point)>,
    writes: Writes,
}

impl GpuScene {
    /// Uploads `scene`'s geometry, with no object marked.
    pub fn new(device: &wgpu::Device, scene: &Scene) -> GpuScene {
        let mut encoded = instances::encode(scene);
        let (instances, flags) = Self::buffers(device, &encoded);
        // The geometry is on the GPU; only the layout is kept.
        encoded.bytes = Vec::new();
        GpuScene {
            instances,
            flags,
            encoded,
            bounds: scene.bounds(),
            writes: Writes {
                geometry: 1,
                ..Writes::default()
            },
        }
    }

    fn buffers(device: &wgpu::Device, e: &instances::Encoded) -> (wgpu::Buffer, wgpu::Buffer) {
        // A zero-sized vertex buffer is not allowed; an empty Scene keeps one dummy record.
        let mut bytes = e.bytes.clone();
        bytes.resize(bytes.len().max(instances::INSTANCE_SIZE as usize), 0);
        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("reda canvas instances"),
            contents: &bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let flags = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reda canvas flags"),
            size: 4 * u64::from(e.count.max(1)),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        (instances, flags)
    }

    /// Uploads the geometry of a changed Document's new Scene, clearing every mark.
    pub fn replace(&mut self, device: &wgpu::Device, scene: &Scene) {
        let writes = self.writes;
        *self = GpuScene::new(device, scene);
        self.writes = Writes {
            geometry: writes.geometry + 1,
            ..writes
        };
    }

    /// Marks objects selected, hovered, highlighted or dimmed. Writes only the flag buffer.
    pub fn set_highlight(&mut self, queue: &wgpu::Queue, highlight: &Highlight) {
        let words = instances::flag_words(&self.encoded, highlight);
        if !words.is_empty() {
            queue.write_buffer(&self.flags, 0, &words);
        }
        self.writes.flags += 1;
    }

    /// The Scene's bounds, as (min, max); `None` when it is empty.
    pub fn bounds(&self) -> Option<(Point, Point)> {
        self.bounds
    }

    /// How many instances the Scene uploaded.
    pub fn instance_count(&self) -> u32 {
        self.encoded.count
    }

    /// What has been written to this Scene's buffers so far.
    pub fn writes(&self) -> Writes {
        self.writes
    }

    fn layer_range(&self, layer: SceneLayer) -> Option<Range<u32>> {
        self.encoded
            .layers
            .iter()
            .find(|(l, _)| *l == layer)
            .map(|(_, r)| r.clone())
    }
}

/// One canvas's view transform and palette.
pub struct Canvas {
    view: wgpu::Buffer,
    palette: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    srgb: bool,
    background: Rgba,
    writes: Writes,
}

impl Canvas {
    pub fn new(device: &wgpu::Device, renderer: &Renderer) -> Canvas {
        let buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let view = buffer("reda canvas view", VIEW_SIZE);
        let palette = buffer("reda canvas palette", PALETTE_SIZE);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reda canvas uniforms"),
            layout: &renderer.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: view.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: palette.as_entire_binding(),
                },
            ],
        });
        Canvas {
            view,
            palette,
            bind_group,
            srgb: renderer.format.is_srgb(),
            background: Rgba::rgb(0, 0, 0),
            writes: Writes::default(),
        }
    }

    /// Writes the view transform for drawing `scene`: the only write a frame needs.
    pub fn set_view(&mut self, queue: &wgpu::Queue, scene: &GpuScene, view: &View) {
        let px_per_nm = (1.0 / view.nm_per_px) as f32;
        let mirror = if view.mirrored { -1.0 } else { 1.0 };
        let floats = [
            (view.center.0 - scene.encoded.origin.x.0 as f64) as f32,
            (view.center.1 - scene.encoded.origin.y.0 as f64) as f32,
            px_per_nm * mirror,
            px_per_nm,
            view.width as f32 / 2.0,
            view.height as f32 / 2.0,
        ];
        let mut bytes: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
        bytes.extend_from_slice(&u32::from(self.srgb).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        queue.write_buffer(&self.view, 0, &bytes);
        self.writes.view += 1;
    }

    /// Writes the palette: on theme change or a colour override.
    pub fn set_palette(&mut self, queue: &wgpu::Queue, palette: &Palette) {
        let mut slots = vec![[0f32; 4]; 1 + ColorKey::SLOTS];
        slots[0] = unit(palette.background());
        for key in ColorKey::all() {
            slots[1 + key.slot()] = unit(palette.color(key));
        }
        let bytes: Vec<u8> = slots
            .iter()
            .flatten()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        queue.write_buffer(&self.palette, 0, &bytes);
        self.background = palette.background();
        self.writes.palette += 1;
    }

    /// The palette's background: what a frame clears to.
    pub fn background(&self) -> Rgba {
        self.background
    }

    /// What has been written to this canvas's buffers so far.
    pub fn writes(&self) -> Writes {
        self.writes
    }
}

fn unit(c: Rgba) -> [f32; 4] {
    c.0.map(|v| f32::from(v) / 255.0)
}
