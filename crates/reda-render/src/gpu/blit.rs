//! Copies a canvas's resolved frame ([`Target::texture`](super::Target::texture)) into a
//! window's render pass, such as egui's inside a paint callback.

use super::Target;

/// The copy pipeline for passes into one format and sample count.
pub struct Blit {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

/// A target's frame, bound for [`Blit::draw`]. Make a new one when the target is replaced.
pub struct BlitSource {
    bind_group: wgpu::BindGroup,
}

impl Blit {
    /// A pipeline drawing into passes of `format` with `sample_count` samples. The frames
    /// it copies must be in the same format, so colour passes through unchanged.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, sample_count: u32) -> Blit {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("reda blit shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reda blit source"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("reda blit layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("reda blit pipeline"),
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
                count: sample_count,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("reda blit sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Blit {
            pipeline,
            layout,
            sampler,
        }
    }

    /// Binds `target`'s resolved frame.
    pub fn source(&self, device: &wgpu::Device, target: &Target) -> BlitSource {
        let view = target.texture().create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reda blit source"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        BlitSource { bind_group }
    }

    /// Copies the frame over the pass's whole viewport (set it to the canvas's rectangle).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, source: &BlitSource) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &source.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
