//! The canvas's offscreen render target: a 4× MSAA colour texture resolved into a
//! single-sample texture that can be copied into a window or read back.

use crate::scene::SceneLayer;

use super::{Canvas, GpuScene, Renderer, SAMPLE_COUNT};

/// An offscreen MSAA target and its resolve texture.
pub struct Target {
    msaa: wgpu::TextureView,
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
    size: (u32, u32),
}

impl Target {
    /// A `width` × `height` target in `renderer`'s format.
    pub fn new(device: &wgpu::Device, renderer: &Renderer, width: u32, height: u32) -> Target {
        let texture = |label, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: renderer.format(),
                usage,
                view_formats: &[],
            })
        };
        let msaa = texture(
            "reda canvas msaa",
            SAMPLE_COUNT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let resolve = texture(
            "reda canvas resolve",
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );
        Target {
            msaa: msaa.create_view(&Default::default()),
            resolve_view: resolve.create_view(&Default::default()),
            resolve,
            size: (width, height),
        }
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The resolved image: what to copy into the window or read back.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.resolve
    }

    /// Records a frame into `encoder`: clears to the canvas's background, draws `scene`
    /// and resolves. See [`Renderer::draw`] for `layers`.
    pub fn record(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        renderer: &Renderer,
        canvas: &Canvas,
        scene: &GpuScene,
        layers: Option<&[SceneLayer]>,
    ) {
        let bg = canvas.background().0.map(|v| f64::from(v) / 255.0);
        let bg = if renderer.format().is_srgb() {
            bg.map(srgb_to_linear)
        } else {
            bg
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("reda canvas pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.msaa,
                depth_slice: None,
                resolve_target: Some(&self.resolve_view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: bg[0],
                        g: bg[1],
                        b: bg[2],
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        renderer.draw(&mut pass, canvas, scene, layers);
    }
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
