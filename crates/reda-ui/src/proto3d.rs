//! PROTOTYPE (issue #433): how should the 3D view look and behave? Throwaway.
//!
//! Three variants of the PCB tab's 3D view mode, drawn in place of the 2D canvas so the
//! real Title bar, Docks, Properties and status bar sit around them:
//!
//! - **A · Same canvas** (`variant_a`): the 2D canvas tilts into 3D and back. The 2D
//!   mouse, the 2D Layers panel, no new chrome.
//! - **B · Viewer** (`variant_b`): a studio you step into. Left-drag orbits, a view cube,
//!   a Show card and a toolbar.
//! - **C · X-ray** (`variant_c`): the 2D Layer colours pulled apart. A Layer rail, a
//!   spread slider and a cut plane; selecting dims the rest.
//!
//! - **D · Same canvas plus** (`variant_d`): added after the first reaction. A, with the
//!   2D | 3D switch at the top left, B's view cube, and B's Show controls in a dropdown
//!   where the Layer chip sits in 2D.
//!
//! Run: `cargo run -p reda-app`, open a Project and its PCB, press `3`. `2` goes back to
//! 2D. F9 and Shift+F9 step the variant. See `docs/prototype/3d-view/README.md`.
//!
//! The view looks, selects and measures; it edits nothing. Selection is the real one
//! (`selection.set`), so Properties and the 2D canvas follow it.

pub mod camera;
pub mod gpu;
pub mod mesh;
mod variant_a;
mod variant_b;
mod variant_c;
mod variant_d;

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use camera::{Cam, FOV, V3, add, len, norm, scale, sub};
use mesh::{BOARD, BODY, CATS, COPPER, HOLE, INNER, Look, Model, SILK};
use reda_engine::model::Uuid;
use reda_engine::render::{SceneLayer, Theme};

use crate::shell::Shell;

/// The variants, in switcher order.
pub const VARIANTS: [&str; 4] = [
    "A · Same canvas",
    "B · Viewer",
    "C · X-ray",
    "D · Same canvas plus",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Preset {
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
    Iso,
}

impl Preset {
    pub const ALL: [(Preset, &'static str, &'static str); 7] = [
        (Preset::Top, "Top", "T"),
        (Preset::Bottom, "Bottom", "B"),
        (Preset::Front, "Front", "F"),
        (Preset::Back, "Back", "K"),
        (Preset::Left, "Left", "L"),
        (Preset::Right, "Right", "R"),
        (Preset::Iso, "Iso", "I"),
    ];

    /// (yaw, pitch).
    pub fn angles(self) -> (f32, f32) {
        let q = std::f32::consts::FRAC_PI_2;
        match self {
            Preset::Top => (0.0, q),
            Preset::Bottom => (2.0 * q, -q),
            Preset::Front => (0.0, 0.0),
            Preset::Back => (2.0 * q, 0.0),
            Preset::Left => (q, 0.0),
            Preset::Right => (-q, 0.0),
            Preset::Iso => (-0.6, 0.7),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Leave,
    View(Preset),
    Fit,
    Projection,
    Measure,
    Cancel,
    Bodies,
}

#[derive(Default)]
pub struct Measure {
    pub on: bool,
    pub a: Option<V3>,
    pub b: Option<V3>,
    pub hover: Option<V3>,
}

struct Anim {
    from: Cam,
    to: Cam,
    start: f64,
    seconds: f64,
    leave: bool,
}

type ModelKey = (u64, Look, Vec<SceneLayer>, u64, bool);
type EncodeKey = (u64, u64, Vec<u16>, bool);

pub struct State {
    on: bool,
    pub variant: usize,
    pub cam: Cam,
    anim: Option<Anim>,
    /// Each kind's opacity; 0 hides it.
    pub alpha: [f32; CATS],
    pub look: Look,
    /// How far the Layers are spread apart, 0 to 1.
    pub explode: f32,
    /// Where the cut plane is across the board, 0 to 1; 1 cuts nothing.
    pub cut: f32,
    pub hidden_layers: HashSet<u16>,
    pub measure: Measure,
    /// What a drag is turning the view about (variant C).
    pub pivot: Option<V3>,
    pending: Vec<Action>,
    /// The camera has been set up for this visit.
    entered: bool,
    model: Option<(ModelKey, u64, Arc<Model>)>,
    encoded: Option<(EncodeKey, mesh::Encoded)>,
    serial: u64,
    /// The last frame's pick and encode time, for the readout.
    pub work_ms: f32,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE
        .get_or_init(|| {
            let mut s = State {
                on: std::env::var_os("REDA_PROTO_3D").is_some(),
                variant: 3,
                cam: Cam {
                    target: [0.0; 3],
                    yaw: 0.0,
                    pitch: 1.0,
                    dist: 100.0,
                    ortho: true,
                },
                anim: None,
                alpha: [1.0; CATS],
                look: Look {
                    realistic: true,
                    mask: true,
                },
                explode: 0.0,
                cut: 1.0,
                hidden_layers: HashSet::new(),
                measure: Measure::default(),
                pivot: None,
                pending: Vec::new(),
                entered: false,
                model: None,
                encoded: None,
                serial: 0,
                work_ms: 0.0,
            };
            defaults(&mut s);
            Mutex::new(s)
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// What a variant starts with.
fn defaults(s: &mut State) {
    s.alpha = [1.0; CATS];
    s.explode = 0.0;
    s.cut = 1.0;
    s.hidden_layers.clear();
    s.measure = Measure::default();
    s.pivot = None;
    s.anim = None;
    s.entered = false;
    match s.variant {
        0 => variant_a::defaults(s),
        1 => variant_b::defaults(s),
        2 => variant_c::defaults(s),
        _ => variant_d::defaults(s),
    }
}

/// Whether PCB tabs show the 3D view.
pub fn on() -> bool {
    state().on
}

pub fn set_on(on: bool) {
    let mut s = state();
    s.on = on;
    if !on {
        s.entered = false;
        s.anim = None;
    }
}

pub fn set_variant(variant: usize) {
    let mut s = state();
    s.variant = variant % VARIANTS.len();
    defaults(&mut s);
}

/// Edits the prototype's state, for the screenshots.
pub fn with<R>(edit: impl FnOnce(&mut State) -> R) -> R {
    edit(&mut state())
}

/// The keys, read once a frame before the app's own: `3` and `2` enter and leave, F9 and
/// Shift+F9 step the variant, and in 3D the view keys.
pub fn keys(ctx: &egui::Context) {
    use egui::{Key, Modifiers};
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let mut s = state();
    let (next, previous) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::F9),
            i.consume_key(Modifiers::SHIFT, Key::F9),
        )
    });
    if next || previous {
        let n = VARIANTS.len();
        s.variant = (s.variant + if next { 1 } else { n - 1 }) % n;
        defaults(&mut s);
    }
    if !s.on {
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Num3)) {
            s.on = true;
            s.entered = false;
        }
        return;
    }
    let mut keys = vec![
        (Key::Num2, Action::Leave),
        (Key::Home, Action::Fit),
        (Key::P, Action::Projection),
        (Key::M, Action::Measure),
        (Key::H, Action::Bodies),
    ];
    let letters = [Key::T, Key::B, Key::F, Key::K, Key::L, Key::R, Key::I];
    for ((preset, _, _), key) in Preset::ALL.into_iter().zip(letters) {
        keys.push((key, Action::View(preset)));
    }
    if s.measure.on {
        keys.push((Key::Escape, Action::Cancel));
    }
    for (key, action) in keys {
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
            s.pending.push(action);
        }
    }
}

/// The distance from which the whole board fits `rect`.
fn fit_dist(model: &Model, rect: egui::Rect) -> f32 {
    let aspect = (rect.width() / rect.height()).min(1.0);
    model.radius * 1.12 / aspect / (FOV / 2.0).tan()
}

fn animate(s: &mut State, to: Cam, now: f64, seconds: f64, leave: bool) {
    s.anim = Some(Anim {
        from: s.cam,
        to,
        start: now,
        seconds,
        leave,
    });
}

/// A point's place on screen with the Layers spread as they are.
pub fn lifted(p: V3, lift: f32, explode: f32) -> V3 {
    add(p, [0.0, 0.0, lift * explode])
}

/// What the variants draw with.
pub struct Cx<'a> {
    pub model: &'a Model,
    pub rect: egui::Rect,
    pub response: &'a egui::Response,
    pub dark: bool,
    /// The selected objects, as indices into the model's.
    pub selected: &'a HashSet<u32>,
}

/// The 3D view, filling a PCB Document's tab.
pub fn show(ui: &mut egui::Ui, shell: &mut Shell, document: Uuid) {
    let (rect, response) =
        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    let theme = crate::theme::canvas_theme(ui.ctx().theme());
    let dark = theme == Theme::Dark;
    let Some(tab) = shell.document_tab(document) else {
        return;
    };
    let (scene, selection, camera_2d) = (tab.scene.clone(), tab.selection.clone(), tab.camera);
    let (palette, palette_generation) = (tab.palette.palette(theme), tab.palette.generation);
    let mut guard = state();
    let s = &mut *guard;
    let now = ui.input(|i| i.time);

    // The model, rebuilt when the board, its visible Layers or the look change.
    let mut visible: Vec<SceneLayer> = match &scene.layers {
        Some(layers) => (layers.iter())
            .filter(|c| c.opacity > 0.0)
            .map(|c| c.layer)
            .collect(),
        None => Vec::new(),
    };
    visible.sort();
    let key: ModelKey = (
        scene.generation,
        s.look,
        visible.clone(),
        palette_generation,
        dark,
    );
    if s.model.as_ref().map(|(k, _, _)| k) != Some(&key) {
        let Some(doc) = shell
            .engine()
            .project()
            .and_then(|p| p.pcb_document(document))
        else {
            return;
        };
        let set: HashSet<SceneLayer> = visible.into_iter().collect();
        let filter = scene.layers.is_some().then_some(&set);
        let model = mesh::build(doc, &scene.scene, &palette, filter, s.look);
        s.serial += 1;
        s.model = Some((key, s.serial, Arc::new(model)));
    }
    let (_, serial, model) = s.model.as_ref().expect("built above");
    let (serial, model) = (*serial, model.clone());

    // Arriving: each variant starts from its own camera.
    if !s.entered {
        s.entered = true;
        let fit = Cam {
            target: [0.0; 3],
            yaw: 0.0,
            pitch: std::f32::consts::FRAC_PI_2,
            dist: fit_dist(&model, rect),
            ortho: s.cam.ortho,
        };
        match s.variant {
            0 | 3 => {
                // From exactly what the 2D canvas showed, tilting into 3D.
                let flat = match camera_2d {
                    Some(c) => Cam {
                        target: [
                            ((c.center.0 - model.center.0) / 1e6) as f32,
                            ((c.center.1 - model.center.1) / 1e6) as f32,
                            0.0,
                        ],
                        dist: (rect.height() as f64 * c.nm_per_point / 2e6) as f32
                            / (FOV / 2.0).tan(),
                        ..fit
                    },
                    None => fit,
                };
                s.cam = flat;
                let tilted = Cam {
                    yaw: -0.35,
                    pitch: 0.9,
                    ..flat
                };
                animate(s, tilted, now, 0.35, false);
            }
            1 => {
                let (yaw, pitch) = Preset::Iso.angles();
                s.cam = Cam { yaw, pitch, ..fit };
            }
            _ => {
                s.cam = Cam {
                    yaw: -0.4,
                    pitch: 0.95,
                    ..fit
                };
            }
        }
    }

    // Keys and buttons.
    let mut leave_now = false;
    for action in std::mem::take(&mut s.pending) {
        act(s, action, &model, rect, now, &mut leave_now);
    }

    // The camera in flight.
    if let Some(anim) = &s.anim {
        let t = ((now - anim.start) / anim.seconds).clamp(0.0, 1.0) as f32;
        let eased = t * t * (3.0 - 2.0 * t);
        s.cam = Cam::lerp(&anim.from, &anim.to, eased);
        if t >= 1.0 {
            leave_now |= anim.leave;
            s.anim = None;
        }
        ui.ctx().request_repaint();
    }
    if leave_now {
        // Back to 2D, looking at what 3D was looking at.
        let cam = s.cam;
        s.on = false;
        s.entered = false;
        s.measure = Measure::default();
        drop(guard);
        if let Some(tab) = shell.document_tab_mut(document)
            && let Some(camera) = tab.camera.as_mut()
        {
            camera.center = (
                model.center.0 + cam.target[0] as f64 * 1e6,
                model.center.1 + cam.target[1] as f64 * 1e6,
            );
            camera.nm_per_point = 2.0 * cam.half_h() as f64 * 1e6 / rect.height() as f64;
        }
        ui.ctx().request_repaint();
        return;
    }

    // The selection, as the model's objects.
    let selected: HashSet<u32> = (model.objects.iter().enumerate())
        .filter(|(_, uuid)| selection.objects.contains(uuid))
        .map(|(i, _)| i as u32)
        .collect();
    let cx = Cx {
        model: &model,
        rect,
        response: &response,
        dark,
        selected: &selected,
    };

    // The mouse.
    let started = std::time::Instant::now();
    if s.anim.is_none() {
        match s.variant {
            0 => variant_a::navigate(ui, s, &cx),
            1 => variant_b::navigate(ui, s, &cx),
            2 => variant_c::navigate(ui, s, &cx),
            _ => variant_d::navigate(ui, s, &cx),
        }
    }
    let cut_x = if s.cut >= 0.999 {
        1e9
    } else {
        model.radius * (2.0 * s.cut - 1.0)
    };
    let hidden = s.hidden_layers.clone();
    let shown = mesh::Shown {
        alpha: s.alpha,
        explode: s.explode,
        cut_x,
        hidden_layers: &hidden,
    };
    let under = |pos: egui::Pos2, cam: &Cam| {
        let (o, d) = cam.ray(rect, pos);
        mesh::pick(&model, o, d, &shown)
    };
    let mut select: Option<Vec<Uuid>> = None;
    if s.measure.on {
        // A point snaps to a corner of what is under the pointer.
        let snap = |pos: egui::Pos2, cam: &Cam| {
            let (i, hit) = under(pos, cam)?;
            let t = &model.tris[i];
            let corner = (t.p.iter())
                .map(|p| lifted(*p, t.lift, shown.explode))
                .filter_map(|p| Some((cam.project(rect, p)?.distance(pos), p)))
                .filter(|(d, _)| *d < 10.0)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            Some(corner.map_or(hit, |(_, p)| p))
        };
        s.measure.hover = response.hover_pos().and_then(|p| snap(p, &s.cam));
        if response.clicked()
            && let Some(p) = response
                .interact_pointer_pos()
                .and_then(|p| snap(p, &s.cam))
        {
            match (s.measure.a, s.measure.b) {
                (Some(_), None) => s.measure.b = Some(p),
                _ => (s.measure.a, s.measure.b) = (Some(p), None),
            }
        }
    } else if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let hit = under(pos, &s.cam).map(|(i, _)| model.tris[i].obj);
        select = Some(match hit {
            Some(obj) if obj != mesh::NO_OBJECT => vec![model.objects[obj as usize]],
            _ => Vec::new(),
        });
    }

    // The triangles, recoloured when the selection or the shown Layers change.
    let dim = s.variant == 2;
    let mut hidden_sorted: Vec<u16> = hidden.iter().copied().collect();
    hidden_sorted.sort();
    let key: EncodeKey = (serial, selection.generation, hidden_sorted, dim);
    if s.encoded.as_ref().map(|(k, _)| k) != Some(&key) {
        let bg = palette.background().0.map(|v| f32::from(v) / 255.0);
        let marks = mesh::Marks {
            selected: &selected,
            hidden_layers: &hidden,
            selected_color: [1.0, 0.74, 0.12],
            dim_to: dim.then_some([bg[0], bg[1], bg[2]]),
        };
        s.encoded = Some((key, mesh::encode(&model, &marks)));
        s.serial += 1;
    }
    s.work_ms = started.elapsed().as_secs_f32() * 1000.0;
    let (_, encoded) = s.encoded.as_ref().expect("encoded above");

    // The frame.
    let aspect = rect.width() / rect.height();
    let reach = model.radius + s.explode * model.radius;
    let (right, up, fwd) = s.cam.basis();
    let light = norm(add(
        scale(fwd, -0.75),
        add(scale(right, 0.35), scale(up, 0.55)),
    ));
    let mut floats: Vec<f32> = s.cam.view_proj(aspect, reach).to_vec();
    floats.extend([1.0, 0.0, 0.0, cut_x]);
    let shading = if s.look.realistic { 1.0 } else { 0.0 };
    floats.extend([s.explode, 0.0, shading, 0.0]);
    floats.extend([s.alpha[0], s.alpha[1], s.alpha[2], s.alpha[3]]);
    floats.extend([s.alpha[4], s.alpha[5], 0.0, 0.0]);
    floats.extend([light[0], light[1], light[2], 0.0]);
    let solid = |cat: &u32| s.alpha[*cat as usize] >= 0.999;
    let faded = |cat: &u32| (0.001..0.999).contains(&s.alpha[*cat as usize]);
    let clear = match s.variant {
        1 if dark => [0.16, 0.17, 0.19],
        1 => [0.80, 0.81, 0.83],
        _ => {
            let bg = palette.background().0.map(|v| f32::from(v) / 255.0);
            [bg[0], bg[1], bg[2]]
        }
    };
    let scale_px = ui.ctx().pixels_per_point();
    let frame = gpu::Frame {
        id: egui::Id::unique(("proto-3d", document)),
        generation: s.serial,
        verts: encoded.bytes.clone(),
        ranges: encoded.ranges,
        uniform: floats.iter().flat_map(|f| f.to_le_bytes()).collect(),
        size: (
            ((rect.width() * scale_px).round() as u32).max(1),
            ((rect.height() * scale_px).round() as u32).max(1),
        ),
        clear,
        opaque: [BOARD, HOLE, INNER, COPPER, SILK, BODY]
            .into_iter()
            .filter(solid)
            .collect(),
        blended: [INNER, HOLE, COPPER, SILK, BOARD, BODY]
            .into_iter()
            .filter(faded)
            .collect(),
    };
    gpu::paint(&ui.painter_at(rect), rect, frame);

    // What each variant lays over it.
    let actions = match s.variant {
        0 => variant_a::chrome(ui, s, &cx),
        1 => variant_b::chrome(ui, s, &cx),
        2 => variant_c::chrome(ui, s, &cx),
        _ => variant_d::chrome(ui, s, &cx),
    };
    readout(ui, s, &cx);
    s.pending.extend(actions);
    if !s.pending.is_empty() {
        ui.ctx().request_repaint();
    }
    drop(guard);
    if let Some(objects) = select {
        let _ = shell.run("selection.set", serde_json::json!({ "objects": objects }));
    }
}

fn act(s: &mut State, action: Action, model: &Model, rect: egui::Rect, now: f64, leave: &mut bool) {
    match action {
        Action::Leave => {
            if s.variant == 0 || s.variant == 3 {
                // Tilt back to flat first, so 2D takes over from the same picture.
                let (yaw, pitch) = Preset::Top.angles();
                let flat = Cam {
                    yaw,
                    pitch,
                    ortho: true,
                    ..s.cam
                };
                animate(s, flat, now, 0.3, true);
            } else {
                *leave = true;
            }
        }
        Action::View(preset) => {
            let (yaw, pitch) = preset.angles();
            let to = Cam {
                yaw,
                pitch,
                ..s.cam
            };
            animate(s, to, now, 0.25, false);
        }
        Action::Fit => {
            let to = Cam {
                target: [0.0; 3],
                dist: fit_dist(model, rect),
                ..s.cam
            };
            animate(s, to, now, 0.25, false);
        }
        Action::Projection => s.cam.ortho = !s.cam.ortho,
        Action::Measure => {
            s.measure = Measure {
                on: !s.measure.on,
                ..Measure::default()
            }
        }
        Action::Cancel => s.measure = Measure::default(),
        Action::Bodies => {
            s.alpha[BODY as usize] = if s.alpha[BODY as usize] > 0.0 {
                0.0
            } else {
                1.0
            }
        }
    }
}

/// The measured span's ends: the second is the pointer until it is placed.
pub fn span(s: &State) -> Option<(V3, V3, bool)> {
    let a = s.measure.a?;
    match s.measure.b {
        Some(b) => Some((a, b, true)),
        None => s.measure.hover.map(|h| (a, h, false)),
    }
}

pub fn mm(v: f32) -> String {
    format!("{v:.3} mm")
}

pub fn distance(a: V3, b: V3) -> f32 {
    len(sub(b, a))
}

/// A floating card inside the view: the variants' chrome is made of these.
pub fn card<R>(
    ui: &egui::Ui,
    id: &str,
    pivot: egui::Align2,
    at: egui::Pos2,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let visuals = ui.visuals().clone();
    egui::Area::new(egui::Id::unique(("proto-3d-card", id)))
        .order(egui::Order::Foreground)
        .pivot(pivot)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(visuals.window_fill)
                .stroke(visuals.window_stroke)
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, add)
                .inner
        })
        .inner
}

/// The prototype's own readout, bottom right: not part of any design under review.
fn readout(ui: &egui::Ui, s: &State, cx: &Cx<'_>) {
    if std::env::var("REDA_PROTO_SWITCHER").as_deref() == Ok("0") {
        return;
    }
    let text = format!(
        "{} triangles · build {:.0} ms · pick {:.1} ms · yaw {:.0}° pitch {:.0}° · {}",
        cx.model.tris.len(),
        cx.model.build_ms,
        s.work_ms,
        s.cam.yaw.to_degrees(),
        s.cam.pitch.to_degrees(),
        if s.cam.ortho { "ortho" } else { "persp" },
    );
    ui.painter_at(cx.rect).text(
        cx.rect.right_bottom() - egui::vec2(10.0, 8.0),
        egui::Align2::RIGHT_BOTTOM,
        text,
        egui::FontId::monospace(10.0),
        ui.visuals().weak_text_color(),
    );
}

/// Over the 2D canvas: how a variant offers the way into 3D.
pub fn over_2d(ui: &egui::Ui, rect: egui::Rect) {
    let variant = state().variant;
    let enter = match variant {
        1 => variant_b::mode_switch(ui, rect, false),
        3 => variant_d::mode_switch(ui, rect, false),
        _ => {
            ui.painter_at(rect).text(
                rect.left_bottom() + egui::vec2(12.0, -10.0),
                egui::Align2::LEFT_BOTTOM,
                "3  3D view",
                egui::FontId::proportional(11.0),
                ui.visuals().weak_text_color(),
            );
            false
        }
    };
    if enter {
        let mut s = state();
        s.on = true;
        s.entered = false;
    }
}

/// The floating bar that steps the variant: not part of any design under review.
/// Hidden with `REDA_PROTO_SWITCHER=0`.
pub fn switcher(ctx: &egui::Context) {
    if std::env::var("REDA_PROTO_SWITCHER").as_deref() == Ok("0") {
        return;
    }
    let mut step = 0isize;
    let variant = state().variant;
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("prototype-3d-switcher"))
        .order(egui::Order::Tooltip)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(screen.center_bottom() - egui::vec2(0.0, 36.0))
        .show(ctx, |ui| {
            // theme-exempt: the prototype's own switcher, deliberately off-theme.
            let ink = egui::Color32::from_rgb(0xff, 0xe0, 0x4a);
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x11, 0x11, 0x11))
                .stroke(egui::Stroke::new(1.0, ink))
                .corner_radius(255)
                .inner_margin(egui::Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.visuals_mut().override_text_color = Some(ink);
                    ui.horizontal(|ui| {
                        let flat = |t: &str| egui::Button::new(t).frame(false);
                        if ui.add(flat("◀")).clicked() {
                            step = -1;
                        }
                        ui.label(format!("PROTOTYPE  {}", VARIANTS[variant]));
                        if ui.add(flat("▶")).clicked() {
                            step = 1;
                        }
                        ui.label("·  F9 / Shift+F9 · 3 into 3D · 2 back");
                    });
                });
        });
    if step != 0 {
        let n = VARIANTS.len() as isize;
        set_variant((variant as isize + step).rem_euclid(n) as usize);
    }
}

/// The zoom factor of this frame's wheel and pinch.
pub fn wheel(ui: &egui::Ui) -> f32 {
    let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
    (-scroll * 0.0025).exp() / pinch
}

/// The point of the board under `pos`, with what is shown as it is.
pub fn point_under(s: &State, cx: &Cx<'_>, pos: egui::Pos2) -> Option<V3> {
    let (o, d) = s.cam.ray(cx.rect, pos);
    let shown = mesh::Shown {
        alpha: s.alpha,
        explode: s.explode,
        cut_x: if s.cut >= 0.999 {
            1e9
        } else {
            cx.model.radius * (2.0 * s.cut - 1.0)
        },
        hidden_layers: &s.hidden_layers,
    };
    mesh::pick(cx.model, o, d, &shown).map(|(_, p)| p)
}
