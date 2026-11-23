//! PROTOTYPE (issue #433): the 3D pass. Throwaway.
//!
//! One pipeline pair (opaque, and blended without depth writes) drawing into a 4× MSAA
//! colour and depth target of its own; `reda_render`'s [`Blit`] copies the resolved frame
//! into egui's pass, as the 2D canvas does.

use std::collections::HashMap;
use std::sync::Arc;

use reda_engine::render::gpu::{Blit, BlitSource, wgpu};

use super::mesh::{CATS, VERTEX_SIZE};

const SAMPLES: u32 = 4;
const UNIFORM_SIZE: u64 = 144;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// One frame of one 3D view.
pub struct Frame {
    pub id: egui::Id,
    /// Changes when `verts` does.
    pub generation: u64,
    pub verts: Arc<Vec<u8>>,
    pub ranges: [(u32, u32); CATS],
    pub uniform: Vec<u8>,
    /// In pixels.
    pub size: (u32, u32),
    /// sRGB.
    pub clear: [f32; 3],
    /// The kinds to draw solid, then the ones to blend, in draw order.
    pub opaque: Vec<u32>,
    pub blended: Vec<u32>,
}

struct Target {
    size: (u32, u32),
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolve: wgpu::TextureView,
    source: BlitSource,
}

struct ViewGpu {
    vertices: Option<(u64, wgpu::Buffer)>,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    target: Option<Target>,
}

pub struct Resources {
    opaque: wgpu::RenderPipeline,
    blended: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    blit: Blit,
    format: wgpu::TextureFormat,
    views: HashMap<egui::Id, ViewGpu>,
}

fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    depth_write: bool,
) -> wgpu::RenderPipeline {
    let attrs = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32, 4 => Uint32,
    ];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("reda proto 3d pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: VERTEX_SIZE,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attrs,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(depth_write),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: SAMPLES,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
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
    })
}

impl Resources {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, samples: u32) -> Resources {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("reda proto 3d shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("board.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reda proto 3d uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(UNIFORM_SIZE),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("reda proto 3d layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        Resources {
            opaque: pipeline(device, &shader, &pipeline_layout, format, true),
            blended: pipeline(device, &shader, &pipeline_layout, format, false),
            layout,
            blit: Blit::new(device, format, samples),
            format,
            views: HashMap::new(),
        }
    }

    fn target(&self, device: &wgpu::Device, size: (u32, u32)) -> Target {
        let texture = |format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("reda proto 3d target"),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let resolve = texture(
            self.format,
            1,
            attachment | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        Target {
            size,
            msaa: texture(self.format, SAMPLES, attachment),
            depth: texture(DEPTH, SAMPLES, attachment),
            source: self.blit.source_view(device, &resolve),
            resolve,
        }
    }

    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
    ) {
        if !self.views.contains_key(&frame.id) {
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("reda proto 3d uniform"),
                size: UNIFORM_SIZE,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("reda proto 3d uniforms"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
            self.views.insert(
                frame.id,
                ViewGpu {
                    vertices: None,
                    uniform,
                    bind,
                    target: None,
                },
            );
        }
        let stale = (self.views[&frame.id].target.as_ref()).is_none_or(|t| t.size != frame.size);
        if stale {
            let target = self.target(device, frame.size);
            self.views.get_mut(&frame.id).expect("made above").target = Some(target);
        }
        let srgb = self.format.is_srgb();
        let view = self.views.get_mut(&frame.id).expect("made above");
        if view.vertices.as_ref().map(|(g, _)| *g) != Some(frame.generation) {
            use wgpu::util::DeviceExt;
            let mut bytes = frame.verts.as_ref().clone();
            bytes.resize(bytes.len().max(VERTEX_SIZE as usize), 0);
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("reda proto 3d vertices"),
                contents: &bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });
            view.vertices = Some((frame.generation, buffer));
        }
        queue.write_buffer(&view.uniform, 0, &frame.uniform);
        // params.y: whether the target is sRGB, which only this side knows.
        let flag: f32 = if srgb { 1.0 } else { 0.0 };
        queue.write_buffer(&view.uniform, 84, &flag.to_le_bytes());
        let target = view.target.as_ref().expect("made above");
        let clear = frame.clear.map(|c| {
            let c = f64::from(c);
            if !srgb {
                c
            } else if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("reda proto 3d pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.msaa,
                depth_slice: None,
                resolve_target: Some(&target.resolve),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear[0],
                        g: clear[1],
                        b: clear[2],
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let (_, vertices) = view.vertices.as_ref().expect("made above");
        pass.set_bind_group(0, &view.bind, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        for (pipeline, cats) in [
            (&self.opaque, &frame.opaque),
            (&self.blended, &frame.blended),
        ] {
            pass.set_pipeline(pipeline);
            for cat in cats {
                let (first, count) = frame.ranges[*cat as usize];
                if count > 0 {
                    pass.draw(first..first + count, 0..1);
                }
            }
        }
    }
}

struct Paint(Frame);

impl egui_wgpu::CallbackTrait for Paint {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(res) = resources.get_mut::<Resources>() {
            res.prepare(device, queue, encoder, &self.0);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(res) = resources.get::<Resources>() else {
            return;
        };
        if let Some(target) = res.views.get(&self.0.id).and_then(|v| v.target.as_ref()) {
            res.blit.draw(pass, &target.source);
        }
    }
}

/// Adds the paint callback that draws `frame` into `rect`.
pub fn paint(painter: &egui::Painter, rect: egui::Rect, frame: Frame) {
    painter.add(egui_wgpu::Callback::new_paint_callback(rect, Paint(frame)));
}
