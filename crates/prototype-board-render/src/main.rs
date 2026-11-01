// PROTOTYPE — throwaway, lives only on the `prototype/board-renderer` branch.
//
// Question: can egui's Painter draw a dense 2-layer board view with smooth pan/zoom, or does
// REDA need its own wgpu renderer inside egui (a paint callback)?
//
// One synthetic board (src/board.rs), three ways to draw it, switchable live:
//   1. Painter: shapes   — every frame, emit egui Shapes for the visible objects
//                          (segment + 2 end discs per Track, rects, circles); egui tessellates
//                          them on the CPU and uploads the vertices.
//   2. Painter: mesh     — tessellate the board once into a world-space egui Mesh (no AA),
//                          then each frame transform the vertices to screen and hand egui the
//                          mesh. Skips tessellation but still moves every vertex every frame.
//   3. wgpu callback     — upload instanced SDF primitives once; per frame write one uniform.
//                          Analytic AA at every zoom. (src/gpu.rs)
//
// Drag to pan, scroll to zoom about the pointer. "Run benchmark" sweeps a scripted zoom/pan
// through all three modes and tabulates frame times. Vsync is off so frame time ≈ real cost.
//
// Run: cargo run --release -p prototype-board-render

mod board;
mod gpu;

use board::*;
use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, Vec2, epaint::Vertex};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Shapes,
    Mesh,
    Wgpu,
}
const MODES: [Mode; 3] = [Mode::Shapes, Mode::Mesh, Mode::Wgpu];

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Mode::Shapes => "Painter: shapes",
            Mode::Mesh => "Painter: mesh",
            Mode::Wgpu => "wgpu callback",
        }
    }
}

fn c32(c: [f32; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        (c[3] * 255.0) as u8,
    )
}

/// Rolling stats over the last N frames.
#[derive(Default)]
struct Rolling(Vec<f32>);
impl Rolling {
    fn push(&mut self, v: f32) {
        self.0.push(v);
        if self.0.len() > 120 {
            self.0.remove(0);
        }
    }
    fn mean(&self) -> f32 {
        if self.0.is_empty() { 0.0 } else { self.0.iter().sum::<f32>() / self.0.len() as f32 }
    }
}

struct Bench {
    mode: usize,
    frame: usize,
    dt: Vec<f32>,
    cpu: Vec<f32>,
    build: Vec<f32>,
}
const WARMUP: usize = 30;
const FRAMES: usize = 300;

struct BenchRow {
    mode: Mode,
    mean_dt: f32,
    p95_dt: f32,
    mean_cpu: f32,
    p95_cpu: f32,
    mean_build: f32,
}

struct App {
    board: Arc<Board>,
    density: f32,
    mode: Mode,
    zoom: f32, // points per mm
    pan: Vec2, // screen offset (points) of world origin from canvas min
    fitted: bool,
    vis: [bool; 5], // pour, bottom, top, pads, vias
    world_mesh: Option<(u64, egui::Mesh)>,
    dt: Rolling,
    cpu: Rolling,
    build: Rolling,
    last_counts: (usize, usize), // shapes, vertices
    bench: Option<Bench>,
    results: Vec<BenchRow>,
    quit: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.as_ref().expect("needs the wgpu renderer");
        gpu::install(rs, 1);
        App {
            board: Arc::new(Board::generate(std::env::var("REDA_DENSITY").ok().and_then(|d| d.parse().ok()).unwrap_or(1.0), 0)),
            density: std::env::var("REDA_DENSITY").ok().and_then(|d| d.parse().ok()).unwrap_or(1.0),
            mode: Mode::Wgpu,
            zoom: 1.0,
            pan: Vec2::ZERO,
            fitted: false,
            vis: [true; 5],
            world_mesh: None,
            dt: Rolling::default(),
            cpu: Rolling::default(),
            build: Rolling::default(),
            last_counts: (0, 0),
            bench: std::env::var("REDA_BENCH").is_ok().then(|| Bench { mode: 0, frame: 0, dt: vec![], cpu: vec![], build: vec![] }),
            results: Vec::new(),
            quit: false,
        }
    }

    fn fit(&mut self, rect: Rect) {
        let s = self.board.size;
        self.zoom = (rect.width() / s[0]).min(rect.height() / s[1]) * 0.95;
        self.pan = (rect.size() - Vec2::new(s[0], s[1]) * self.zoom) * 0.5;
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Board renderer prototype");
        ui.label("Drag: pan · Scroll: zoom");
        ui.separator();
        ui.label("Render mode");
        for m in MODES {
            ui.radio_value(&mut self.mode, m, m.label());
        }
        ui.separator();
        ui.add(egui::Slider::new(&mut self.density, 0.25..=8.0).logarithmic(true).text("density"));
        if ui.button("Regenerate board").clicked() {
            let generation = self.board.generation + 1;
            self.board = Arc::new(Board::generate(self.density, generation));
        }
        let b = &self.board;
        ui.label(format!(
            "{} track segments\n{} pads\n{} vias\n{} pour runs",
            b.tracks.len(),
            b.pads.len(),
            b.vias.len(),
            b.pours.len()
        ));
        ui.separator();
        ui.label("Layers");
        for (i, name) in ["Bottom pour", "Bottom tracks", "Top tracks", "Pads", "Vias"].iter().enumerate() {
            ui.checkbox(&mut self.vis[i], *name);
        }
        ui.separator();
        ui.label(format!(
            "frame interval {:.2} ms ({:.0} fps)\nCPU per frame {:.2} ms\n  of which shape/mesh build {:.2} ms\nshapes {} · vertices {}",
            self.dt.mean() * 1000.0,
            1.0 / self.dt.mean().max(1e-6),
            self.cpu.mean() * 1000.0,
            self.build.mean() * 1000.0,
            self.last_counts.0,
            self.last_counts.1,
        ));
        if ui.button("Fit board").clicked() {
            self.fitted = false;
        }
        ui.separator();
        if self.bench.is_none() {
            if ui.button("Run benchmark (all modes)").clicked() {
                self.results.clear();
                self.bench = Some(Bench { mode: 0, frame: 0, dt: vec![], cpu: vec![], build: vec![] });
            }
        } else {
            ui.label("Benchmarking…");
        }
        if !self.results.is_empty() {
            egui::Grid::new("results").striped(true).show(ui, |ui| {
                ui.strong("mode");
                ui.strong("mean ms");
                ui.strong("p95 ms");
                ui.strong("CPU ms");
                ui.strong("CPU p95");
                ui.strong("build ms");
                ui.end_row();
                for r in &self.results {
                    ui.label(r.mode.label());
                    ui.label(format!("{:.2}", r.mean_dt));
                    ui.label(format!("{:.2}", r.p95_dt));
                    ui.label(format!("{:.2}", r.mean_cpu));
                    ui.label(format!("{:.2}", r.p95_cpu));
                    ui.label(format!("{:.2}", r.mean_build));
                    ui.end_row();
                }
            });
        }
    }

    /// Scripted camera: zoom from fit-to-board down to ~40× while panning across the board.
    fn bench_camera(&mut self, rect: Rect, frame: usize) {
        self.fit(rect);
        let t = frame as f32 / FRAMES as f32;
        let f = (40f32.ln() * (std::f32::consts::PI * t).sin()).exp();
        let s = self.board.size;
        let target = [s[0] * (0.2 + 0.6 * t), s[1] * (0.5 + 0.3 * (6.0 * t).sin())];
        self.zoom *= f;
        self.pan = rect.size() * 0.5 - Vec2::new(target[0], target[1]) * self.zoom;
    }

    fn step_bench(&mut self, dt: f32, cpu: f32, build: f32) {
        let Some(b) = &mut self.bench else { return };
        b.frame += 1;
        if b.frame > WARMUP {
            b.dt.push(dt * 1000.0);
            b.cpu.push(cpu * 1000.0);
            b.build.push(build * 1000.0);
        }
        if b.frame >= WARMUP + FRAMES {
            let mean = |v: &Vec<f32>| v.iter().sum::<f32>() / v.len() as f32;
            let mut s = b.dt.clone();
            s.sort_by(|a, b| a.total_cmp(b));
            self.results.push(BenchRow {
                mode: MODES[b.mode],
                mean_dt: mean(&b.dt),
                p95_dt: s[s.len() * 95 / 100],
                mean_cpu: mean(&b.cpu),
                p95_cpu: { let mut c = b.cpu.clone(); c.sort_by(|a, b| a.total_cmp(b)); c[c.len() * 95 / 100] },
                mean_build: mean(&b.build),
            });
            if b.mode + 1 < MODES.len() {
                *b = Bench { mode: b.mode + 1, frame: 0, dt: vec![], cpu: vec![], build: vec![] };
            } else {
                self.bench = None;
                self.fitted = false;
                if std::env::var("REDA_BENCH").is_ok() {
                    let b = &self.board;
                    println!("board: {} tracks, {} pads, {} vias, {} pour runs (density {})", b.tracks.len(), b.pads.len(), b.vias.len(), b.pours.len(), self.density);
                    for r in &self.results {
                        println!("{:16} mean {:7.2} ms  p95 {:7.2} ms  cpu {:7.2} ms  cpu p95 {:7.2} ms  build {:7.2} ms", r.mode.label(), r.mean_dt, r.p95_dt, r.mean_cpu, r.p95_cpu, r.mean_build);
                    }
                    self.quit = true;
                }
            }
        }
    }

    fn to_screen(&self, rect: Rect) -> impl Fn([f32; 2]) -> Pos2 + use<> {
        let (o, z) = (rect.min + self.pan, self.zoom);
        move |p| Pos2::new(o.x + p[0] * z, o.y + p[1] * z)
    }

    fn paint_shapes(&mut self, painter: &egui::Painter, rect: Rect) {
        let ts = self.to_screen(rect);
        let vmin = [-self.pan.x / self.zoom, -self.pan.y / self.zoom];
        let vmax = [vmin[0] + rect.width() / self.zoom, vmin[1] + rect.height() / self.zoom];
        let vis = |min: [f32; 2], max: [f32; 2]| {
            max[0] >= vmin[0] && min[0] <= vmax[0] && max[1] >= vmin[1] && min[1] <= vmax[1]
        };
        let z = self.zoom;
        let mut shapes: Vec<Shape> = Vec::new();
        let b = &self.board;
        if self.vis[0] {
            let c = c32(POUR_COLOR);
            for r in b.pours.iter().filter(|r| vis(r.min, r.max)) {
                shapes.push(Shape::rect_filled(Rect::from_min_max(ts(r.min), ts(r.max)), 0.0, c));
            }
        }
        for (on, layer, col) in [(self.vis[1], BOTTOM, BOTTOM_COLOR), (self.vis[2], TOP, TOP_COLOR)] {
            if !on {
                continue;
            }
            let c = c32(col);
            for t in b.tracks.iter().filter(|t| t.layer == layer) {
                let h = t.half_width;
                let min = [t.a[0].min(t.b[0]) - h, t.a[1].min(t.b[1]) - h];
                let max = [t.a[0].max(t.b[0]) + h, t.a[1].max(t.b[1]) + h];
                if !vis(min, max) {
                    continue;
                }
                let (a, bb) = (ts(t.a), ts(t.b));
                shapes.push(Shape::line_segment([a, bb], Stroke::new(2.0 * h * z, c)));
                shapes.push(Shape::circle_filled(a, h * z, c));
                shapes.push(Shape::circle_filled(bb, h * z, c));
            }
        }
        let hole = c32(HOLE_COLOR);
        if self.vis[3] {
            let c = c32(PAD_COLOR);
            for p in &b.pads {
                let min = [p.center[0] - p.half[0], p.center[1] - p.half[1]];
                let max = [p.center[0] + p.half[0], p.center[1] + p.half[1]];
                if !vis(min, max) {
                    continue;
                }
                shapes.push(Shape::rect_filled(Rect::from_min_max(ts(min), ts(max)), 0.0, c));
                if let Some(d) = p.drill {
                    shapes.push(Shape::circle_filled(ts(p.center), d * z, hole));
                }
            }
        }
        if self.vis[4] {
            let c = c32(VIA_COLOR);
            for v in &b.vias {
                let r = v.radius;
                if !vis([v.center[0] - r, v.center[1] - r], [v.center[0] + r, v.center[1] + r]) {
                    continue;
                }
                shapes.push(Shape::circle_filled(ts(v.center), r * z, c));
                shapes.push(Shape::circle_filled(ts(v.center), v.drill * z, hole));
            }
        }
        self.last_counts = (shapes.len(), 0);
        painter.extend(shapes);
    }

    fn build_world_mesh(b: &Board, vis: [bool; 5]) -> egui::Mesh {
        let mut m = egui::Mesh::default();
        let quad = |m: &mut egui::Mesh, p: [Pos2; 4], c: Color32| {
            let i = m.vertices.len() as u32;
            for q in p {
                m.vertices.push(Vertex { pos: q, uv: egui::epaint::WHITE_UV, color: c });
            }
            m.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
        };
        let disc = |m: &mut egui::Mesh, c0: [f32; 2], r: f32, c: Color32| {
            const N: u32 = 12;
            let i = m.vertices.len() as u32;
            m.vertices.push(Vertex { pos: Pos2::new(c0[0], c0[1]), uv: egui::epaint::WHITE_UV, color: c });
            for k in 0..N {
                let a = k as f32 / N as f32 * std::f32::consts::TAU;
                m.vertices.push(Vertex {
                    pos: Pos2::new(c0[0] + r * a.cos(), c0[1] + r * a.sin()),
                    uv: egui::epaint::WHITE_UV,
                    color: c,
                });
            }
            for k in 0..N {
                m.indices.extend_from_slice(&[i, i + 1 + k, i + 1 + (k + 1) % N]);
            }
        };
        let p = |x: f32, y: f32| Pos2::new(x, y);
        if vis[0] {
            let c = c32(POUR_COLOR);
            for r in &b.pours {
                quad(&mut m, [p(r.min[0], r.min[1]), p(r.max[0], r.min[1]), p(r.max[0], r.max[1]), p(r.min[0], r.max[1])], c);
            }
        }
        for (on, layer, col) in [(vis[1], BOTTOM, BOTTOM_COLOR), (vis[2], TOP, TOP_COLOR)] {
            if !on {
                continue;
            }
            let c = c32(col);
            for t in b.tracks.iter().filter(|t| t.layer == layer) {
                let d = Vec2::new(t.b[0] - t.a[0], t.b[1] - t.a[1]);
                let n = if d.length() > 1e-6 { d.normalized().rot90() * t.half_width } else { Vec2::ZERO };
                let (a, bb) = (p(t.a[0], t.a[1]), p(t.b[0], t.b[1]));
                quad(&mut m, [a + n, bb + n, bb - n, a - n], c);
                disc(&mut m, t.a, t.half_width, c);
                disc(&mut m, t.b, t.half_width, c);
            }
        }
        let hole = c32(HOLE_COLOR);
        if vis[3] {
            let c = c32(PAD_COLOR);
            for pad in &b.pads {
                let (cx, cy, hx, hy) = (pad.center[0], pad.center[1], pad.half[0], pad.half[1]);
                quad(&mut m, [p(cx - hx, cy - hy), p(cx + hx, cy - hy), p(cx + hx, cy + hy), p(cx - hx, cy + hy)], c);
                if let Some(d) = pad.drill {
                    disc(&mut m, pad.center, d, hole);
                }
            }
        }
        if vis[4] {
            let c = c32(VIA_COLOR);
            for v in &b.vias {
                disc(&mut m, v.center, v.radius, c);
                disc(&mut m, v.center, v.drill, hole);
            }
        }
        m
    }

    fn paint_mesh(&mut self, painter: &egui::Painter, rect: Rect) {
        let key = self.board.generation * 64 + self.vis.iter().fold(0, |a, &v| a * 2 + v as u64);
        if self.world_mesh.as_ref().map(|(k, _)| *k) != Some(key) {
            self.world_mesh = Some((key, Self::build_world_mesh(&self.board, self.vis)));
        }
        let world = &self.world_mesh.as_ref().unwrap().1;
        let (o, z) = (rect.min + self.pan, self.zoom);
        let mut m = egui::Mesh {
            indices: world.indices.clone(),
            vertices: world
                .vertices
                .iter()
                .map(|v| Vertex { pos: Pos2::new(o.x + v.pos.x * z, o.y + v.pos.y * z), ..*v })
                .collect(),
            ..Default::default()
        };
        m.texture_id = egui::TextureId::default();
        self.last_counts = (1, m.vertices.len());
        painter.add(Shape::mesh(m));
    }

    fn paint_wgpu(&mut self, painter: &egui::Painter, rect: Rect) {
        let (w, h) = (rect.width(), rect.height());
        let view = [
            2.0 * self.zoom / w,
            -2.0 * self.zoom / h,
            2.0 * self.pan.x / w - 1.0,
            1.0 - 2.0 * self.pan.y / h,
        ];
        let ppp = painter.ctx().pixels_per_point();
        let [pour, bottom, top, pads, vias] = self.vis;
        let cb = gpu::BoardCallback {
            board: self.board.clone(),
            view,
            world_per_px: 1.0 / (self.zoom * ppp),
            vis: gpu::Visible { pour, bottom, top, pads, vias },
        };
        self.last_counts = (1, 0);
        painter.add(eframe::egui_wgpu::Callback::new_paint_callback(rect, cb));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let dt = ui.ctx().input(|i| i.unstable_dt);
        let cpu = frame.info().cpu_usage.unwrap_or(0.0);
        self.dt.push(dt);
        self.cpu.push(cpu);

        egui::Panel::left("side").min_size(260.0).show(ui, |ui| self.side_panel(ui));
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_rgb(18, 18, 22))).show(ui, |ui| {
            let rect = ui.max_rect();
            let resp = ui.allocate_rect(rect, egui::Sense::drag());
            if !self.fitted {
                self.fit(rect);
                self.fitted = true;
            }
            if let Some(b) = &self.bench {
                self.mode = MODES[b.mode];
                let f = b.frame;
                self.bench_camera(rect, f);
            } else {
                self.pan += resp.drag_delta();
                if let Some(ptr) = resp.hover_pos() {
                    let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        let f = (scroll * 0.003).exp();
                        let local = ptr - rect.min;
                        let world = (local - self.pan) / self.zoom;
                        self.zoom = (self.zoom * f).clamp(0.5, 5000.0);
                        self.pan = local - world * self.zoom;
                    }
                }
            }
            let painter = ui.painter_at(rect);
            let t0 = Instant::now();
            match self.mode {
                Mode::Shapes => self.paint_shapes(&painter, rect),
                Mode::Mesh => self.paint_mesh(&painter, rect),
                Mode::Wgpu => self.paint_wgpu(&painter, rect),
            }
            let build = t0.elapsed().as_secs_f32();
            self.build.push(build);
            self.step_bench(dt, cpu, build);
        });
        if self.quit {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ui.ctx().request_repaint();
    }
}

fn main() -> eframe::Result {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    options.wgpu_options.surface.present_mode = eframe::egui_wgpu::wgpu::PresentMode::AutoNoVsync;
    eframe::run_native("reda-prototype-board-render", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
