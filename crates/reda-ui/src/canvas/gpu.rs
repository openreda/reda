//! The canvas's GPU side, kept in egui-wgpu's callback resources.
//!
//! Each canvas draws its Scene with `reda_render::gpu` into its own 4× MSAA offscreen
//! [`Target`], and a [`Blit`] copies the resolved frame into egui's pass. The frame is
//! re-recorded only when the Scene, theme, view, size or grid changed; otherwise the last
//! one is copied again and nothing is written to the GPU.

use std::collections::HashMap;
use std::sync::Arc;

use reda_engine::model::Uuid;
use reda_engine::render::gpu::{
    Blit, BlitSource, Canvas, Flags, GpuScene, Grid, GridSpec, Highlight, Renderer, Target, Writes,
    wgpu,
};
use reda_engine::render::{Composite, Palette, Scene, Theme, View};

/// A Scene shared with the paint callback. `generation` changes whenever the Scene does,
/// so the GPU knows when to upload it again.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneHandle {
    pub generation: u64,
    pub scene: Arc<Scene>,
    /// For a PCB: the Layers to composite, bottom first, at their opacity. `None` draws
    /// every Layer opaque in Scene order (a sheet).
    pub layers: Option<Arc<[Composite]>>,
}

/// The selected objects shared with the paint callback. `generation` changes whenever the
/// selection does, so the GPU knows when to rewrite the flags.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionHandle {
    pub generation: u64,
    pub objects: Arc<[Uuid]>,
}

impl Default for SelectionHandle {
    fn default() -> SelectionHandle {
        SelectionHandle {
            generation: 0,
            objects: Arc::from([]),
        }
    }
}

/// Everything one frame of one canvas shows.
#[derive(Clone)]
pub struct CanvasFrame {
    /// Which canvas: its GPU state is kept under this id between frames.
    pub canvas: egui::Id,
    pub scene: SceneHandle,
    pub theme: Theme,
    /// The view in physical pixels; its size is the offscreen target's.
    pub view: View,
    pub grid: Option<GridSpec>,
    /// The objects drawn selected.
    pub selection: SelectionHandle,
}

/// What a recorded frame depended on.
#[derive(Clone, Copy, PartialEq)]
struct FrameKey {
    generation: u64,
    selection: u64,
    theme: Theme,
    view: View,
    grid: Option<GridSpec>,
}

impl FrameKey {
    fn of(frame: &CanvasFrame) -> FrameKey {
        FrameKey {
            generation: frame.scene.generation,
            selection: frame.selection.generation,
            theme: frame.theme,
            view: frame.view,
            grid: frame.grid,
        }
    }
}

/// One canvas's GPU state.
pub struct CanvasGpu {
    scene: GpuScene,
    generation: u64,
    /// The selection generation the flags were last written for.
    selection: Option<u64>,
    canvas: Canvas,
    theme: Option<Theme>,
    grid: Grid,
    target: Option<(Target, BlitSource)>,
    last: Option<FrameKey>,
    recorded: u32,
}

impl CanvasGpu {
    fn new(device: &wgpu::Device, renderer: &Renderer, frame: &CanvasFrame) -> CanvasGpu {
        CanvasGpu {
            scene: GpuScene::new(device, &frame.scene.scene),
            generation: frame.scene.generation,
            selection: None,
            canvas: Canvas::new(device, renderer),
            theme: None,
            grid: Grid::new(device, renderer),
            target: None,
            last: None,
            recorded: 0,
        }
    }

    /// How many frames have been recorded.
    pub fn recorded(&self) -> u32 {
        self.recorded
    }

    /// What has been written so far: (Scene buffers, canvas uniforms).
    pub fn writes(&self) -> (Writes, Writes) {
        (self.scene.writes(), self.canvas.writes())
    }
}

/// The pipelines and every canvas's GPU state. The app inserts one into egui-wgpu's
/// callback resources at start-up.
pub struct CanvasResources {
    renderer: Renderer,
    blit: Blit,
    canvases: HashMap<egui::Id, CanvasGpu>,
}

impl CanvasResources {
    /// Pipelines for a window whose egui pass has `format` and `sample_count` samples. The
    /// offscreen frames share the format, so the copy changes no colour.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> CanvasResources {
        CanvasResources {
            renderer: Renderer::new(device, format),
            blit: Blit::new(device, format, sample_count),
            canvases: HashMap::new(),
        }
    }

    /// One canvas's GPU state, once it has been prepared.
    pub fn canvas(&self, id: egui::Id) -> Option<&CanvasGpu> {
        self.canvases.get(&id)
    }

    /// Brings `frame.canvas`'s offscreen frame up to date, recording into `encoder` only
    /// when something it shows changed. Returns whether it recorded.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &CanvasFrame,
    ) -> bool {
        let renderer = &self.renderer;
        let gpu = self
            .canvases
            .entry(frame.canvas)
            .or_insert_with(|| CanvasGpu::new(device, renderer, frame));
        let key = FrameKey::of(frame);
        if gpu.last == Some(key) {
            return false;
        }
        if gpu.generation != frame.scene.generation {
            gpu.scene.replace(device, &frame.scene.scene);
            gpu.generation = frame.scene.generation;
            // A new Scene starts with no marks.
            gpu.selection = None;
        }
        if gpu.selection != Some(frame.selection.generation) {
            // A Scene with no marks needs no write for an empty selection.
            if gpu.selection.is_some() || !frame.selection.objects.is_empty() {
                let mut highlight = Highlight::new();
                highlight.mark_all(frame.selection.objects.iter().copied(), Flags::SELECTED);
                gpu.scene.set_highlight(queue, &highlight);
            }
            gpu.selection = Some(frame.selection.generation);
        }
        if gpu.theme != Some(frame.theme) {
            gpu.canvas
                .set_palette(queue, &Palette::shipped(frame.theme));
            gpu.theme = Some(frame.theme);
        }
        let size = (frame.view.width.max(1), frame.view.height.max(1));
        if gpu.target.as_ref().map(|(t, _)| t.size()) != Some(size) {
            let target = Target::new(device, renderer, size.0, size.1);
            let source = self.blit.source(device, &target);
            gpu.target = Some((target, source));
        }
        gpu.canvas.set_view(queue, &gpu.scene, &frame.view);
        gpu.grid.set(queue, &frame.view, frame.grid.as_ref());
        let (target, _) = gpu.target.as_ref().expect("made above");
        let grid = &gpu.grid;
        match &frame.scene.layers {
            None => target.record_with(encoder, renderer, &gpu.canvas, &gpu.scene, None, |pass| {
                grid.draw(pass)
            }),
            Some(layers) => {
                target.record_layers(encoder, renderer, &gpu.canvas, &gpu.scene, layers, |pass| {
                    grid.draw(pass)
                })
            }
        }
        gpu.last = Some(key);
        gpu.recorded += 1;
        true
    }

    /// Copies `canvas`'s last frame into `pass`, whose viewport is the canvas's rectangle.
    pub fn paint(&self, canvas: egui::Id, pass: &mut wgpu::RenderPass<'_>) {
        if let Some((_, source)) = self.canvases.get(&canvas).and_then(|c| c.target.as_ref()) {
            self.blit.draw(pass, source);
        }
    }

    /// Drops the GPU state of canvases not in `keep` (closed tabs).
    pub fn retain(&mut self, keep: &[egui::Id]) {
        self.canvases.retain(|id, _| keep.contains(id));
    }
}

/// The paint callback egui runs for a canvas.
struct PaintCanvas(CanvasFrame);

impl egui_wgpu::CallbackTrait for PaintCanvas {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(res) = resources.get_mut::<CanvasResources>() {
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
        if let Some(res) = resources.get::<CanvasResources>() {
            res.paint(self.0.canvas, pass);
        }
    }
}

/// Adds the paint callback that draws `frame` into `rect`.
pub fn paint(painter: &egui::Painter, rect: egui::Rect, frame: CanvasFrame) {
    painter.add(egui_wgpu::Callback::new_paint_callback(
        rect,
        PaintCanvas(frame),
    ));
}
