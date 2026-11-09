//! The polygon editor (docs/spec/v0.1/pcb.md#polygon-editor): one editor for the Board
//! Outline, Board Cutouts, Zones, Zone Cutouts and polygon Keepouts.
//!
//! A selected polygon shows a handle at each corner and at each edge's middle:
//!
//! - **Corner handles** move the vertex, chamfer the corner (a bevel whose middle follows
//!   the cursor) or fillet it (the same with an arc).
//! - **Edge-midpoint handles** move the edge keeping the adjacent angles, split it (a
//!   new vertex that follows the cursor) or bow it into an arc through the cursor.
//! - Dragging an edge away from its handles slides it, as moving the edge does.
//! - Space (`tool.rotate`) cycles the handle's modes while dragging.
//! - `Mod`+click on an edge inserts a vertex there; Del while holding a vertex removes it.
//!
//! A drag runs nothing in flight: the board is drawn from a preview, and the release runs
//! one `pcb.polygon.set_vertices`. While the shape crosses itself it shows as invalid,
//! and a release then puts the last committed shape back, running nothing.
//!
//! A moved vertex or a split snaps by the shared [`crate::snap`] model, with alignment
//! guides through the polygon's other vertices.
//!
//! A Zone that follows the Board Outline, and a line Keepout, have no handles.

use reda_engine::model::geom::{self, Millideg, Nm, Point, Vertex};
use reda_engine::model::{InDocument, KeepoutShape, Object, PcbDocument, Project, Uuid};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::snap::{self, Guide, Hit, Snapped, Target};

/// What a corner handle does while dragged; Space cycles them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerMode {
    Move,
    Chamfer,
    Fillet,
}

/// What an edge-midpoint handle does while dragged; Space cycles them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeMode {
    /// Moves the edge across itself, the adjacent edges keeping their angles.
    Move,
    /// Adds a vertex at the middle, which follows the cursor.
    Split,
    /// Makes the edge an arc through the cursor.
    Bow,
}

/// What was grabbed, by vertex index (an edge runs from its index to the next vertex).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    Corner(usize),
    Midpoint(usize),
    /// The edge away from its handles: dragging slides it.
    Edge(usize),
}

/// The handle's mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Corner(CornerMode),
    Edge(EdgeMode),
}

/// A reshape in flight.
#[derive(Clone, Debug)]
pub struct PolygonEdit {
    pub(super) document: Uuid,
    object: Uuid,
    original: Vec<Vertex>,
    grab: Grab,
    mode: Mode,
    /// The point on the grabbed handle or edge the cursor's offset counts from.
    from: Point,
    shape: Vec<Vertex>,
    valid: bool,
    targets: Vec<Target>,
    guides: Vec<Guide>,
    /// The guides the cursor snapped across last.
    shown_guides: Vec<Guide>,
    snapped: Option<Snapped>,
}

impl PolygonEdit {
    /// The object being reshaped.
    pub fn object(&self) -> Uuid {
        self.object
    }

    pub fn grab(&self) -> Grab {
        self.grab
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The shape as it now stands.
    pub fn vertices(&self) -> &[Vertex] {
        &self.shape
    }

    /// Whether the shape as it stands is valid (does not cross itself): a release keeps
    /// only a valid one.
    pub fn valid(&self) -> bool {
        self.valid
    }

    /// The alignment guides the cursor is snapped across.
    pub fn guide_lines(&self) -> &[Guide] {
        &self.shown_guides
    }

    pub(super) fn snap(&self) -> Option<Snapped> {
        self.snapped
    }

    /// Whether a vertex is held, so Del removes it.
    pub fn holds_vertex(&self) -> bool {
        matches!(self.grab, Grab::Corner(_))
    }

    /// Whether the shape changed from the committed one.
    pub(super) fn changed(&self) -> bool {
        self.shape != self.original
    }

    /// Space: the handle's next mode; the shape follows from the cursor anew.
    pub(super) fn cycle(&mut self, shell: &mut Shell) {
        self.mode = match self.mode {
            Mode::Corner(CornerMode::Move) => Mode::Corner(CornerMode::Chamfer),
            Mode::Corner(CornerMode::Chamfer) => Mode::Corner(CornerMode::Fillet),
            Mode::Corner(CornerMode::Fillet) => Mode::Corner(CornerMode::Move),
            Mode::Edge(EdgeMode::Move) => Mode::Edge(EdgeMode::Split),
            Mode::Edge(EdgeMode::Split) => Mode::Edge(EdgeMode::Bow),
            Mode::Edge(EdgeMode::Bow) => Mode::Edge(EdgeMode::Move),
        };
        self.guides = guides(&self.original, self.grab, self.mode);
        if let Some(at) = self.snapped.map(|s| s.at) {
            self.follow_snapped(shell, at, None);
        }
    }

    /// The cursor is at `at`: reshapes and shows the preview.
    pub(super) fn follow(&mut self, shell: &mut Shell, at: Point) {
        let mut rules = snap::rules(shell, self.document);
        if !matches!(
            self.mode,
            Mode::Corner(CornerMode::Move) | Mode::Edge(EdgeMode::Split)
        ) {
            rules.guides = false;
        }
        let snapped = snap::snap(
            (at.x.0 as f64, at.y.0 as f64),
            &self.targets,
            &self.guides,
            &rules,
        );
        self.follow_snapped(shell, snapped.at, Some(snapped));
    }

    fn follow_snapped(&mut self, shell: &mut Shell, at: Point, snapped: Option<Snapped>) {
        if let Some(s) = snapped {
            self.shown_guides = if s.hit == Hit::Guide {
                self.guides
                    .iter()
                    .copied()
                    .filter(|g| match g {
                        Guide::X(x) => *x == s.at.x,
                        Guide::Y(y) => *y == s.at.y,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            self.snapped = Some(s);
        }
        self.shape = reshape(&self.original, self.grab, self.mode, self.from, at)
            .unwrap_or_else(|| self.original.clone());
        self.valid = valid(&self.shape);
        show(shell, self.document, self.object, &self.shape);
    }

    /// The Command a release runs, when the shape is valid and changed.
    pub(super) fn command(&self) -> Option<(&'static str, Value)> {
        (self.valid && self.changed()).then(|| set_vertices(self.object, &self.shape))
    }

    /// Del while holding a vertex: the shape without it, when enough is left.
    pub(super) fn without_held(&self) -> Option<(&'static str, Value)> {
        let Grab::Corner(i) = self.grab else {
            return None;
        };
        let mut shape = self.original.clone();
        shape.remove(i);
        (geom::check_contour(&shape).is_ok() && valid(&shape))
            .then(|| set_vertices(self.object, &shape))
    }
}

/// `pcb.polygon.set_vertices` giving `object` the shape `vertices`.
fn set_vertices(object: Uuid, vertices: &[Vertex]) -> (&'static str, Value) {
    let vertices: Vec<Value> = vertices
        .iter()
        .map(|v| {
            let mut p = json!({"x": v.x.0, "y": v.y.0});
            if v.sweep != Millideg::ZERO {
                p["sweep"] = json!(v.sweep.0);
            }
            p
        })
        .collect();
    (
        "pcb.polygon.set_vertices",
        json!({"object": object, "vertices": vertices}),
    )
}

/// The vertices of polygon object `uuid` when the editor can edit it: not a Zone that
/// follows the Board Outline, nor a line Keepout.
pub fn editable(pcb: &PcbDocument, uuid: Uuid) -> Option<&[Vertex]> {
    if let Some(o) = pcb.outline().filter(|o| o.uuid == uuid) {
        return Some(&o.vertices);
    }
    if let Some(c) = pcb.cutout(uuid) {
        return Some(&c.vertices);
    }
    if let Some(z) = pcb.zone(uuid) {
        return (!z.follows_outline).then_some(&z.vertices[..]);
    }
    if let Some(c) = pcb.zone_cutout(uuid) {
        return Some(&c.vertices);
    }
    match &pcb.keepout(uuid)?.shape {
        KeepoutShape::Polygon { vertices } => Some(vertices),
        KeepoutShape::Line { .. } => None,
    }
}

/// The polygon whose handles show: the one selected object, when it is editable.
pub fn selected(shell: &Shell) -> Option<(Uuid, Vec<Vertex>)> {
    let [object] = shell.selection().objects[..] else {
        return None;
    };
    let pcb = shell.engine().project()?.pcb()?;
    Some((object, editable(pcb, object)?.to_vec()))
}

/// The corner handles' and edge-midpoint handles' points of `vertices`.
pub fn handles(vertices: &[Vertex]) -> (Vec<Point>, Vec<Point>) {
    let corners = vertices.iter().map(Vertex::point).collect();
    let middles = (0..vertices.len())
        .map(|i| edge_middle(vertices, i))
        .collect();
    (corners, middles)
}

/// The middle of edge `i`: of the chord, or of the arc.
fn edge_middle(vertices: &[Vertex], i: usize) -> Point {
    let (a, b) = (
        vertices[i].point(),
        vertices[(i + 1) % vertices.len()].point(),
    );
    let (ax, ay, bx, by) = (a.x.0 as f64, a.y.0 as f64, b.x.0 as f64, b.y.0 as f64);
    let (mx, my) = ((ax + bx) / 2.0, (ay + by) / 2.0);
    let sweep = (vertices[i].sweep.0 as f64 / 1000.0).to_radians();
    if sweep == 0.0 {
        return pt(mx, my);
    }
    let chord = (bx - ax).hypot(by - ay);
    if chord == 0.0 {
        return pt(mx, my);
    }
    let r = chord / (2.0 * (sweep.abs() / 2.0).sin());
    let sagitta = r * (1.0 - (sweep.abs() / 2.0).cos());
    // A counter-clockwise arc bulges to the right of its chord.
    let (rx, ry) = ((by - ay) / chord, -(bx - ax) / chord);
    let s = sagitta * sweep.signum();
    pt(mx + rx * s, my + ry * s)
}

/// What a press at `at` within `tolerance` grabs on `vertices`: a corner first, then an
/// edge middle, then an edge.
pub fn grab_at(vertices: &[Vertex], at: Point, tolerance: Nm) -> Option<Grab> {
    let t = tolerance.0 as f64;
    let near = |p: Point| dist(p, at) <= t;
    let (corners, middles) = handles(vertices);
    if let Some(i) = corners.iter().position(|p| near(*p)) {
        return Some(Grab::Corner(i));
    }
    if let Some(i) = middles.iter().position(|p| near(*p)) {
        return Some(Grab::Midpoint(i));
    }
    (0..vertices.len())
        .map(|i| {
            let (a, b) = (
                vertices[i].point(),
                vertices[(i + 1) % vertices.len()].point(),
            );
            (i, segment_distance(a, b, at))
        })
        .filter(|(_, d)| *d <= t)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| Grab::Edge(i))
}

/// Starts reshaping `object` (shape `vertices` on Document `document`) from grab `grab`
/// at `at`.
pub(super) fn start(
    shell: &Shell,
    document: Uuid,
    object: Uuid,
    vertices: Vec<Vertex>,
    grab: Grab,
    at: Point,
) -> PolygonEdit {
    let mode = match grab {
        Grab::Corner(_) => Mode::Corner(CornerMode::Move),
        Grab::Midpoint(_) | Grab::Edge(_) => Mode::Edge(EdgeMode::Move),
    };
    let from = match grab {
        Grab::Corner(i) => vertices[i].point(),
        Grab::Midpoint(i) => edge_middle(&vertices, i),
        Grab::Edge(i) => {
            let (a, b) = (
                vertices[i].point(),
                vertices[(i + 1) % vertices.len()].point(),
            );
            project_on_segment(a, b, at)
        }
    };
    let targets = super::tool::board_hotspots(shell);
    PolygonEdit {
        document,
        object,
        guides: guides(&vertices, grab, mode),
        shape: vertices.clone(),
        original: vertices,
        grab,
        mode,
        from,
        valid: true,
        targets: snap::hotspots::without(targets, &[object]),
        shown_guides: Vec::new(),
        snapped: None,
    }
}

/// `Mod`+click on edge `i` of `object` at `at`: the shape with a vertex there.
pub(super) fn insert(
    object: Uuid,
    vertices: &[Vertex],
    i: usize,
    at: Point,
) -> (&'static str, Value) {
    let (a, b) = (
        vertices[i].point(),
        vertices[(i + 1) % vertices.len()].point(),
    );
    let p = project_on_segment(a, b, at);
    let mut shape = vertices.to_vec();
    shape[i].sweep = Millideg::ZERO;
    shape.insert(i + 1, Vertex::at(p));
    set_vertices(object, &shape)
}

/// The alignment guides of a grab in `mode`: through every vertex but the one moved.
fn guides(vertices: &[Vertex], grab: Grab, mode: Mode) -> Vec<Guide> {
    let moved = match (grab, mode) {
        (Grab::Corner(i), Mode::Corner(CornerMode::Move)) => Some(i),
        (Grab::Midpoint(_), Mode::Edge(EdgeMode::Split)) => None,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for (i, v) in vertices.iter().enumerate() {
        if Some(i) == moved {
            continue;
        }
        for g in [Guide::X(v.x), Guide::Y(v.y)] {
            if !out.contains(&g) {
                out.push(g);
            }
        }
    }
    out
}

/// The shape `original` takes when the grab `grab` at `from` in `mode` has the cursor at
/// `at`; `None` when the mode cannot apply there (a fillet too large, a straight bow).
pub fn reshape(
    original: &[Vertex],
    grab: Grab,
    mode: Mode,
    from: Point,
    at: Point,
) -> Option<Vec<Vertex>> {
    let n = original.len();
    let mut shape = original.to_vec();
    match (grab, mode) {
        (Grab::Corner(i), Mode::Corner(CornerMode::Move)) => {
            shape[i].x = at.x;
            shape[i].y = at.y;
            Some(shape)
        }
        (Grab::Corner(i), Mode::Corner(corner)) => {
            let v = original[i].point();
            let (pa, pb) = (
                original[(i + n - 1) % n].point(),
                original[(i + 1) % n].point(),
            );
            let (ua, ub) = (unit(v, pa)?, unit(v, pb)?);
            let (cx, cy) = ((at.x.0 - v.x.0) as f64, (at.y.0 - v.y.0) as f64);
            if corner == CornerMode::Chamfer {
                // The bevel's middle sits at d·(ua + ub)/2 from the corner.
                let w = ((ua.0 + ub.0) / 2.0, (ua.1 + ub.1) / 2.0);
                let ww = w.0 * w.0 + w.1 * w.1;
                if ww < 1e-12 {
                    return None;
                }
                let d = (cx * w.0 + cy * w.1) / ww;
                if d < 1.0 {
                    return None;
                }
                geom::chamfer(original, Some(&[i]), Nm(d.round() as i64)).ok()
            } else {
                // The arc's middle sits on the bisector, r/sin(θ/2) − r from the corner.
                let bis = (ua.0 + ub.0, ua.1 + ub.1);
                let len = bis.0.hypot(bis.1);
                if len < 1e-9 {
                    return None;
                }
                let bis = (bis.0 / len, bis.1 / len);
                let half = (ua.0 * ub.0 + ua.1 * ub.1).clamp(-1.0, 1.0).acos() / 2.0;
                let along = cx * bis.0 + cy * bis.1;
                let k = 1.0 / half.sin() - 1.0;
                if along < 1.0 || k <= 1e-9 {
                    return None;
                }
                let r = along / k;
                geom::fillet(original, Some(&[i]), Nm(r.round() as i64)).ok()
            }
        }
        (Grab::Midpoint(i) | Grab::Edge(i), Mode::Edge(EdgeMode::Move)) => {
            let j = (i + 1) % n;
            let (a, b) = (original[i].point(), original[j].point());
            let e = unit(a, b)?;
            let normal = (-e.1, e.0);
            let s = (at.x.0 - from.x.0) as f64 * normal.0 + (at.y.0 - from.y.0) as f64 * normal.1;
            let shift = |p: Point| pt(p.x.0 as f64 + s * normal.0, p.y.0 as f64 + s * normal.1);
            let (a2, b2) = (shift(a), shift(b));
            let prev = original[(i + n - 1) % n].point();
            let next = original[(j + 1) % n].point();
            let na = intersect(a2, b2, prev, a).unwrap_or(a2);
            let nb = intersect(a2, b2, b, next).unwrap_or(b2);
            shape[i].x = na.x;
            shape[i].y = na.y;
            shape[j].x = nb.x;
            shape[j].y = nb.y;
            Some(shape)
        }
        (Grab::Midpoint(i) | Grab::Edge(i), Mode::Edge(EdgeMode::Split)) => {
            shape[i].sweep = Millideg::ZERO;
            shape.insert(i + 1, Vertex::at(at));
            Some(shape)
        }
        (Grab::Midpoint(i) | Grab::Edge(i), Mode::Edge(EdgeMode::Bow)) => {
            let (a, b) = (original[i].point(), original[(i + 1) % n].point());
            shape[i].sweep = super::draw::arc_through(a, b, at)?;
            Some(shape)
        }
        _ => None,
    }
}

/// Whether `vertices` make a usable shape: a closed contour that does not cross itself.
fn valid(vertices: &[Vertex]) -> bool {
    geom::check_contour(vertices).is_ok() && geom::crossing(vertices).is_none()
}

/// Draws the board with `object` shaped as `vertices`.
fn show(shell: &mut Shell, document: Uuid, object: Uuid, vertices: &[Vertex]) {
    let Some(project) = shell
        .engine()
        .project()
        .and_then(|p| with_vertices(p, object, vertices))
    else {
        return;
    };
    shell.show_preview(document, &project);
}

/// `project` with polygon object `object` shaped as `vertices`.
fn with_vertices(project: &Project, object: Uuid, vertices: &[Vertex]) -> Option<Project> {
    let pcb = project.pcb()?;
    let document = pcb.uuid;
    let v = vertices.to_vec();
    let replaced = if let Some(o) = pcb.outline().filter(|o| o.uuid == object) {
        let mut o = o.clone();
        o.vertices = v;
        Object::BoardOutline(InDocument::new(document, o))
    } else if let Some(c) = pcb.cutout(object) {
        let mut c = c.clone();
        c.vertices = v;
        Object::BoardCutout(InDocument::new(document, c))
    } else if let Some(z) = pcb.zone(object) {
        let mut z = z.clone();
        z.vertices = v;
        Object::Zone(InDocument::new(document, z))
    } else if let Some(c) = pcb.zone_cutout(object) {
        let mut c = c.clone();
        c.vertices = v;
        Object::ZoneCutout(InDocument::new(document, c))
    } else {
        let mut k = pcb.keepout(object)?.clone();
        k.shape = KeepoutShape::Polygon { vertices: v };
        Object::Keepout(InDocument::new(document, k))
    };
    let mut project = project.clone();
    let mut tx = project.transaction();
    tx.replace(replaced).ok()?;
    tx.commit().ok()?;
    Some(project)
}

fn pt(x: f64, y: f64) -> Point {
    Point::new(Nm(x.round() as i64), Nm(y.round() as i64))
}

fn dist(a: Point, b: Point) -> f64 {
    ((a.x.0 - b.x.0) as f64).hypot((a.y.0 - b.y.0) as f64)
}

/// The unit vector from `a` towards `b`.
fn unit(a: Point, b: Point) -> Option<(f64, f64)> {
    let (dx, dy) = ((b.x.0 - a.x.0) as f64, (b.y.0 - a.y.0) as f64);
    let len = dx.hypot(dy);
    (len > 0.0).then(|| (dx / len, dy / len))
}

/// Where the line through `a`, `b` meets the line through `c`, `d`; `None` if parallel.
fn intersect(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let f = |p: Point| (p.x.0 as f64, p.y.0 as f64);
    let (a, b, c, d) = (f(a), f(b), f(c), f(d));
    let r = (b.0 - a.0, b.1 - a.1);
    let s = (d.0 - c.0, d.1 - c.1);
    let cross = r.0 * s.1 - r.1 * s.0;
    let scale = r.0.hypot(r.1) * s.0.hypot(s.1);
    if scale == 0.0 || (cross / scale).abs() < 1e-9 {
        return None;
    }
    let t = ((c.0 - a.0) * s.1 - (c.1 - a.1) * s.0) / cross;
    Some(pt(a.0 + t * r.0, a.1 + t * r.1))
}

/// The point of segment `a`–`b` nearest `p`.
fn project_on_segment(a: Point, b: Point, p: Point) -> Point {
    let (ax, ay) = (a.x.0 as f64, a.y.0 as f64);
    let (dx, dy) = ((b.x.0 - a.x.0) as f64, (b.y.0 - a.y.0) as f64);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return a;
    }
    let t = (((p.x.0 as f64 - ax) * dx + (p.y.0 as f64 - ay) * dy) / len2).clamp(0.0, 1.0);
    pt(ax + t * dx, ay + t * dy)
}

fn segment_distance(a: Point, b: Point, p: Point) -> f64 {
    dist(project_on_segment(a, b, p), p)
}

/// Points along the closed outline `vertices`, arcs at most about 5° a step.
pub fn outline_points(vertices: &[Vertex]) -> Vec<Point> {
    let n = vertices.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (vertices[i].point(), vertices[(i + 1) % n].point());
        out.push(a);
        let sweep = (vertices[i].sweep.0 as f64 / 1000.0).to_radians();
        if sweep == 0.0 {
            continue;
        }
        let (ax, ay, bx, by) = (a.x.0 as f64, a.y.0 as f64, b.x.0 as f64, b.y.0 as f64);
        let (ex, ey) = (bx - ax, by - ay);
        let chord = ex.hypot(ey);
        if chord == 0.0 {
            continue;
        }
        // The centre is to the left of a counter-clockwise arc under a half turn.
        let d = (chord / 2.0) / (sweep / 2.0).tan();
        let (cx, cy) = (
            (ax + bx) / 2.0 - ey / chord * d,
            (ay + by) / 2.0 + ex / chord * d,
        );
        let steps = ((sweep.abs().to_degrees() / 5.0).ceil() as usize).max(1);
        for k in 1..steps {
            let t = sweep * k as f64 / steps as f64;
            let (s, c) = t.sin_cos();
            let (rx, ry) = (ax - cx, ay - cy);
            out.push(pt(cx + rx * c - ry * s, cy + rx * s + ry * c));
        }
    }
    out
}

/// Draws the selected polygon's handles and, while a reshape is in flight, its shape:
/// in the violation colour while it crosses itself.
pub fn draw(ui: &egui::Ui, shell: &Shell, input: &crate::canvas::CanvasInputs) {
    if shell.tool() != crate::tools::Tool::Select {
        return;
    }
    let edit = shell.pcb_tool().polygon_edit();
    let vertices = match edit {
        Some(e) => e.vertices().to_vec(),
        None => match selected(shell) {
            Some((_, v)) => v,
            None => return,
        },
    };
    let painter = ui.painter_at(input.rect);
    let screen = |p: Point| input.screen_point((p.x.0 as f64, p.y.0 as f64));
    let accent = ui.visuals().selection.stroke.color;
    let bad = ui.visuals().error_fg_color;
    if let Some(e) = edit {
        let color = if e.valid() { accent } else { bad };
        let mut points: Vec<egui::Pos2> =
            outline_points(&vertices).into_iter().map(screen).collect();
        if let Some(first) = points.first().copied() {
            points.push(first);
        }
        let width = if e.valid() { 1.5 } else { 3.0 };
        painter.add(egui::Shape::line(points, egui::Stroke::new(width, color)));
    }
    let fill = ui.visuals().extreme_bg_color;
    let stroke = egui::Stroke::new(1.5, accent);
    let (corners, middles) = handles(&vertices);
    for p in corners {
        let rect = egui::Rect::from_center_size(screen(p), egui::vec2(8.0, 8.0));
        painter.rect(rect, 0.0, fill, stroke, egui::StrokeKind::Middle);
    }
    for p in middles {
        painter.circle(screen(p), 3.5, fill, stroke);
    }
}
