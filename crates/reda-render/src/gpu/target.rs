//! The canvas's offscreen render target: a 4× MSAA colour texture resolved into a
//! single-sample texture that can be copied into a window or read back.

use crate::layers::Composite;
use crate::scene::SceneLayer;

use super::{Canvas, GpuScene, Renderer, SAMPLE_COUNT};

/// An offscreen MSAA target and its resolve texture, plus a texture each Layer is drawn
/// into before it is blended over the frame.
pub struct Target {
    msaa: wgpu::TextureView,
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
    layer_view: wgpu::TextureView,
    layer_source: wgpu::BindGroup,
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
        let layer = texture(
            "reda canvas layer",
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let layer_view = layer.create_view(&Default::default());
        let layer_source = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reda canvas layer"),
            layout: &renderer.composite_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&layer_view),
            }],
        });
        Target {
            msaa: msaa.create_view(&Default::default()),
            resolve_view: resolve.create_view(&Default::default()),
            resolve,
            layer_view,
            layer_source,
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
        self.record_with(encoder, renderer, canvas, scene, layers, |_| {});
    }

    /// [`record`](Self::record), then `over` draws into the same pass before it resolves:
    /// the grid, or other overlays in the canvas's format and sample count.
    pub fn record_with(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        renderer: &Renderer,
        canvas: &Canvas,
        scene: &GpuScene,
        layers: Option<&[SceneLayer]>,
        over: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) {
        let bg = background(renderer, canvas);
        let mut pass = self.msaa_pass(encoder, &self.resolve_view, bg);
        renderer.draw(&mut pass, canvas, scene, layers);
        over(&mut pass);
    }

    /// Records a frame that composites Layers: each of `layers`, bottom first, is drawn
    /// opaque into a Layer texture and then blended over the frame at its opacity. `over`
    /// draws last, at full opacity (the grid). Layers the Scene does not have are skipped.
    pub fn record_layers(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        renderer: &Renderer,
        canvas: &Canvas,
        scene: &GpuScene,
        layers: &[Composite],
        over: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) {
        drop(self.pass(
            encoder,
            &self.resolve_view,
            None,
            background(renderer, canvas),
        ));
        let transparent = Some(wgpu::Color::TRANSPARENT);
        for c in layers {
            if c.opacity <= 0.0 || scene.layer_range(c.layer).is_none() {
                continue;
            }
            {
                let mut pass = self.msaa_pass(encoder, &self.layer_view, transparent);
                renderer.draw(&mut pass, canvas, scene, Some(&[c.layer]));
            }
            let mut pass = self.pass(encoder, &self.resolve_view, None, None);
            renderer.composite(&mut pass, &self.layer_source, c.opacity);
        }
        {
            let mut pass = self.msaa_pass(encoder, &self.layer_view, transparent);
            over(&mut pass);
        }
        let mut pass = self.pass(encoder, &self.resolve_view, None, None);
        renderer.composite(&mut pass, &self.layer_source, 1.0);
    }

    /// A pass into the MSAA texture, cleared to `clear`, resolving into `resolve`.
    fn msaa_pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
        resolve: &wgpu::TextureView,
        clear: Option<wgpu::Color>,
    ) -> wgpu::RenderPass<'e> {
        self.pass(encoder, &self.msaa, Some(resolve), clear)
    }

    /// A pass into `view`, cleared to `clear` or else keeping what is there.
    fn pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
        clear: Option<wgpu::Color>,
    ) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("reda canvas pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target,
                ops: wgpu::Operations {
                    load: clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                    store: if resolve_target.is_some() {
                        wgpu::StoreOp::Discard
                    } else {
                        wgpu::StoreOp::Store
                    },
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }
}

/// The canvas's background as the clear colour of the renderer's format.
fn background(renderer: &Renderer, canvas: &Canvas) -> Option<wgpu::Color> {
    let bg = canvas.background().0.map(|v| f64::from(v) / 255.0);
    let bg = if renderer.format().is_srgb() {
        bg.map(srgb_to_linear)
    } else {
        bg
    };
    Some(wgpu::Color {
        r: bg[0],
        g: bg[1],
        b: bg[2],
        a: 1.0,
    })
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
