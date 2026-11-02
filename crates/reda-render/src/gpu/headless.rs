//! Rendering without a window: a device of its own, preferring a software adapter, that
//! draws a Scene into an RGBA image.

use std::fmt;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use crate::color::Palette;
use crate::scene::Scene;
use crate::view::View;

use super::{Canvas, GpuScene, Highlight, Renderer, Target};

/// The largest image side `render` accepts, in pixels (WebGPU's default texture limit).
pub const MAX_SIDE: u32 = 8192;

/// Why a headless render failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    /// No GPU or software adapter could be opened. The message names the cause.
    NoAdapter(String),
    /// The adapter was found but would not give a device.
    NoDevice(String),
    /// The image size is zero or larger than [`MAX_SIDE`].
    BadSize { width: u32, height: u32 },
    /// The GPU failed while drawing or reading back.
    Gpu(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::NoAdapter(cause) => write!(
                f,
                "no GPU or software renderer is available ({cause}); on Linux, install \
                 Mesa's software renderer (llvmpipe or lavapipe)"
            ),
            RenderError::NoDevice(cause) => write!(f, "the GPU adapter refused a device: {cause}"),
            RenderError::BadSize { width, height } => write!(
                f,
                "cannot render a {width} × {height} image: each side must be 1 to {MAX_SIDE} \
                 pixels"
            ),
            RenderError::Gpu(cause) => write!(f, "the GPU failed: {cause}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// An RGBA image, 8 bits per channel, sRGB, straight alpha, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// The image as PNG bytes.
    pub fn to_png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Balanced);
        let mut writer = encoder
            .write_header()
            .expect("writing a PNG header to memory cannot fail");
        writer
            .write_image_data(&self.rgba)
            .expect("the RGBA buffer matches the PNG size");
        writer
            .finish()
            .expect("finishing a PNG in memory cannot fail");
        out
    }

    /// Decodes an 8-bit RGBA PNG, such as one [`to_png`](Self::to_png) wrote.
    pub fn from_png(bytes: &[u8]) -> Result<Image, String> {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let mut rgba = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
        let info = reader.next_frame(&mut rgba).map_err(|e| e.to_string())?;
        if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
            return Err(format!(
                "expected 8-bit RGBA, found {:?} {:?}",
                info.color_type, info.bit_depth
            ));
        }
        rgba.truncate(info.buffer_size());
        Ok(Image {
            width: info.width,
            height: info.height,
            rgba,
        })
    }
}

/// A device for rendering without a window.
pub struct Headless {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    adapter: wgpu::AdapterInfo,
}

impl Headless {
    /// Opens a device on the backends `WGPU_BACKEND` names (all by default), preferring a
    /// software adapter (llvmpipe, lavapipe, WARP) so renders match across machines, and
    /// falling back to any GPU.
    pub fn new() -> Result<Headless, RenderError> {
        let backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::all());
        Headless::with_backends(backends)
    }

    /// Like [`new`](Self::new), on the given backends only.
    pub fn with_backends(backends: wgpu::Backends) -> Result<Headless, RenderError> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = backends;
        desc.flags = wgpu::InstanceFlags::from_build_config().with_env();
        let instance = wgpu::Instance::new(desc);
        let request = |fallback| {
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: fallback,
                compatible_surface: None,
                apply_limit_buckets: false,
            }))
        };
        let adapter = match request(true) {
            Ok(adapter) => adapter,
            Err(_) => request(false).map_err(|e| {
                let tried = if backends.is_empty() {
                    "no backends enabled".to_owned()
                } else {
                    format!("backends tried: {backends:?}")
                };
                RenderError::NoAdapter(format!("{e}; {tried}"))
            })?,
        };
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("reda headless"),
            required_features: wgpu::Features::empty(),
            required_limits:
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| RenderError::NoDevice(e.to_string()))?;
        let renderer = Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        Ok(Headless {
            device,
            queue,
            renderer,
            adapter: adapter.get_info(),
        })
    }

    /// The adapter in use, such as "llvmpipe (LLVM 19.1.7, 256 bits)".
    pub fn adapter(&self) -> &wgpu::AdapterInfo {
        &self.adapter
    }

    /// Whether the adapter is a software renderer.
    pub fn is_software(&self) -> bool {
        self.adapter.device_type == wgpu::DeviceType::Cpu
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn renderer(&self) -> &Renderer {
        &self.renderer
    }

    /// Draws `scene` as `view` shows it, with `palette`'s colours and `highlight`'s marks.
    pub fn render(
        &self,
        scene: &Scene,
        view: &View,
        palette: &Palette,
        highlight: &Highlight,
    ) -> Result<Image, RenderError> {
        let mut gpu_scene = GpuScene::new(&self.device, scene);
        gpu_scene.set_highlight(&self.queue, highlight);
        let mut canvas = Canvas::new(&self.device, &self.renderer);
        canvas.set_palette(&self.queue, palette);
        self.render_frame(&gpu_scene, &mut canvas, view)
    }

    /// Draws an uploaded Scene into a new target and reads it back. Writes only the view.
    pub fn render_frame(
        &self,
        scene: &GpuScene,
        canvas: &mut Canvas,
        view: &View,
    ) -> Result<Image, RenderError> {
        let (width, height) = (view.width, view.height);
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err(RenderError::BadSize { width, height });
        }
        canvas.set_view(&self.queue, scene, view);
        let target = Target::new(&self.device, &self.renderer, width, height);
        let row = (4 * width).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reda readback"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("reda headless frame"),
            });
        target.record(&mut encoder, &self.renderer, canvas, scene, None);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: target.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        rx.recv()
            .map_err(|e| RenderError::Gpu(e.to_string()))?
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        let mapped = slice
            .get_mapped_range()
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        let mut rgba = Vec::with_capacity((4 * width * height) as usize);
        for y in 0..height {
            let start = (y * row) as usize;
            rgba.extend_from_slice(&mapped[start..start + (4 * width) as usize]);
        }
        drop(mapped);
        readback.unmap();
        unpremultiply(&mut rgba);
        Ok(Image {
            width,
            height,
            rgba,
        })
    }
}

/// The target holds premultiplied colour; PNGs hold straight alpha.
fn unpremultiply(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3];
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
            }
        }
    }
}

/// Runs a future to completion on this thread. wgpu's native futures are ready at once or
/// woken by its own threads, so parking until woken is enough.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(out) => return out,
            Poll::Pending => std::thread::park(),
        }
    }
}
