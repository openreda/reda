//! The canvas grid, drawn by a shader: dots computed per pixel, no geometry.

use crate::color::Rgba;
use crate::view::View;

use super::{Renderer, SAMPLE_COUNT};

const UNIFORM_SIZE: u64 = 48;

/// What grid to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridSpec {
    /// A Document point (nm) that lies on the grid.
    pub origin: (f64, f64),
    /// The dot spacing in nm.
    pub pitch: f64,
    /// The dot radius in pixels.
    pub radius_px: f32,
    pub color: Rgba,
}

/// The grid pipeline and its uniforms, for one canvas. Draw it into the canvas pass after
/// the Scene ([`Target::record_with`](super::Target::record_with)).
pub struct Grid {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    srgb: bool,
    visible: bool,
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
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reda grid uniforms"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reda grid uniforms"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        Grid {
            pipeline,
            uniforms,
            bind_group,
            srgb: renderer.format().is_srgb(),
            visible: false,
        }
    }

    /// Writes the grid for `view`; `None` hides it.
    pub fn set(&mut self, queue: &wgpu::Queue, view: &View, spec: Option<&GridSpec>) {
        self.visible = spec.is_some();
        let Some(spec) = spec else { return };
        let pitch_px = spec.pitch / view.nm_per_px;
        let mirror = if view.mirrored { -1.0 } else { 1.0 };
        let dx = (spec.origin.0 - view.center.0) / view.nm_per_px * mirror;
        let dy = -(spec.origin.1 - view.center.1) / view.nm_per_px;
        let floats = [
            view.width as f32 / 2.0,
            view.height as f32 / 2.0,
            dx.rem_euclid(pitch_px) as f32,
            dy.rem_euclid(pitch_px) as f32,
            pitch_px as f32,
            spec.radius_px,
        ];
        let mut bytes: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
        bytes.extend_from_slice(&u32::from(self.srgb).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        for v in spec.color.0 {
            bytes.extend_from_slice(&(f32::from(v) / 255.0).to_le_bytes());
        }
        queue.write_buffer(&self.uniforms, 0, &bytes);
    }

    /// Records the grid into `pass` (nothing when hidden).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if !self.visible {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
