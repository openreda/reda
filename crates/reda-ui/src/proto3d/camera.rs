//! PROTOTYPE (issue #433): the orbit camera and its small vector maths. Throwaway.

pub type V3 = [f32; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
pub fn norm(a: V3) -> V3 {
    let l = len(a);
    if l > 0.0 { scale(a, 1.0 / l) } else { a }
}

/// The vertical field of view in perspective, and what an orthographic view matches at
/// the target's depth.
pub const FOV: f32 = 0.61; // 35°

/// A turntable camera: it looks at `target` from `dist` away. `pitch` 90° is straight
/// down with +y up, as the 2D canvas; `yaw` turns about the board's normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cam {
    pub target: V3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub ortho: bool,
}

impl Cam {
    /// (right, up, forward).
    pub fn basis(&self) -> (V3, V3, V3) {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let fwd = [sy * cp, cy * cp, -sp];
        let up = [sy * sp, cy * sp, cp];
        (cross(fwd, up), up, fwd)
    }

    pub fn eye(&self) -> V3 {
        let (_, _, f) = self.basis();
        sub(self.target, scale(f, self.dist))
    }

    /// Half the view's height at the target's depth, in mm.
    pub fn half_h(&self) -> f32 {
        self.dist * (FOV / 2.0).tan()
    }

    /// The view-projection matrix, column-major, for a view `aspect` wide over high of a
    /// scene within `radius` of the origin.
    pub fn view_proj(&self, aspect: f32, radius: f32) -> [f32; 16] {
        let (r, u, f) = self.basis();
        let e = self.eye();
        let view = [
            r[0],
            u[0],
            -f[0],
            0.0,
            r[1],
            u[1],
            -f[1],
            0.0,
            r[2],
            u[2],
            -f[2],
            0.0,
            -dot(r, e),
            -dot(u, e),
            dot(f, e),
            1.0,
        ];
        let reach = radius * 2.5 + len(self.target);
        let far = self.dist + reach;
        let proj = if self.ortho {
            let near = self.dist - reach;
            let h = self.half_h();
            [
                1.0 / (h * aspect),
                0.0,
                0.0,
                0.0,
                0.0,
                1.0 / h,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0 / (near - far),
                0.0,
                0.0,
                0.0,
                near / (near - far),
                1.0,
            ]
        } else {
            let near = (self.dist - reach).max(self.dist * 0.02).max(0.05);
            let t = 1.0 / (FOV / 2.0).tan();
            [
                t / aspect,
                0.0,
                0.0,
                0.0,
                0.0,
                t,
                0.0,
                0.0,
                0.0,
                0.0,
                far / (near - far),
                -1.0,
                0.0,
                0.0,
                near * far / (near - far),
                0.0,
            ]
        };
        mul(&proj, &view)
    }

    fn ndc(rect: egui::Rect, pos: egui::Pos2) -> (f32, f32) {
        (
            (pos.x - rect.left()) / rect.width() * 2.0 - 1.0,
            1.0 - (pos.y - rect.top()) / rect.height() * 2.0,
        )
    }

    /// The ray under `pos`: (origin, direction).
    pub fn ray(&self, rect: egui::Rect, pos: egui::Pos2) -> (V3, V3) {
        let (r, u, f) = self.basis();
        let (nx, ny) = Self::ndc(rect, pos);
        let aspect = rect.width() / rect.height();
        if self.ortho {
            let h = self.half_h();
            let o = add(self.eye(), add(scale(r, nx * h * aspect), scale(u, ny * h)));
            (o, f)
        } else {
            let t = (FOV / 2.0).tan();
            let d = add(f, add(scale(r, nx * t * aspect), scale(u, ny * t)));
            (self.eye(), norm(d))
        }
    }

    /// Where `p` lands on screen; `None` behind a perspective eye.
    pub fn project(&self, rect: egui::Rect, p: V3) -> Option<egui::Pos2> {
        let (r, u, f) = self.basis();
        let v = sub(p, self.eye());
        let (x, y, z) = (dot(v, r), dot(v, u), dot(v, f));
        let aspect = rect.width() / rect.height();
        let (nx, ny) = if self.ortho {
            let h = self.half_h();
            (x / (h * aspect), y / h)
        } else {
            if z <= 0.01 {
                return None;
            }
            let t = (FOV / 2.0).tan();
            (x / (z * t * aspect), y / (z * t))
        };
        Some(egui::pos2(
            rect.left() + (nx + 1.0) / 2.0 * rect.width(),
            rect.top() + (1.0 - ny) / 2.0 * rect.height(),
        ))
    }

    pub fn pan(&mut self, rect: egui::Rect, delta: egui::Vec2) {
        let (r, u, _) = self.basis();
        let per_px = 2.0 * self.half_h() / rect.height();
        self.target = add(
            self.target,
            add(scale(r, -delta.x * per_px), scale(u, delta.y * per_px)),
        );
    }

    /// Scales the distance by `k`, keeping what is under `pos` under it.
    pub fn zoom(&mut self, rect: egui::Rect, pos: egui::Pos2, k: f32) {
        let (_, _, f) = self.basis();
        let (o, d) = self.ray(rect, pos);
        let t = dot(sub(self.target, o), f) / dot(d, f);
        let p = add(o, scale(d, t));
        self.dist *= k;
        self.target = add(p, scale(sub(self.target, p), k));
    }

    /// Turns the view. With a `pivot` that point stays where it is on screen; without,
    /// the view turns about its target.
    pub fn orbit(&mut self, dyaw: f32, dpitch: f32, pivot: Option<V3>) {
        let half = std::f32::consts::FRAC_PI_2;
        let before = pivot.map(|p| {
            let (r, u, f) = self.basis();
            let v = sub(p, self.eye());
            (p, dot(v, r), dot(v, u), dot(v, f))
        });
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(-half, half);
        if let Some((p, x, y, z)) = before {
            let (r, u, f) = self.basis();
            let eye = sub(p, add(scale(r, x), add(scale(u, y), scale(f, z))));
            self.target = add(eye, scale(f, self.dist));
        }
    }

    pub fn lerp(a: &Cam, b: &Cam, t: f32) -> Cam {
        let mix = |x: f32, y: f32| x + (y - x) * t;
        // The short way round.
        let turn = std::f32::consts::TAU;
        let dyaw = (b.yaw - a.yaw + turn * 1.5).rem_euclid(turn) - turn / 2.0;
        Cam {
            target: add(a.target, scale(sub(b.target, a.target), t)),
            yaw: a.yaw + dyaw * t,
            pitch: mix(a.pitch, b.pitch),
            dist: mix(a.dist, b.dist),
            ortho: if t < 1.0 { a.ortho } else { b.ortho },
        }
    }
}

/// `a * b`, column-major.
fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut m = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            m[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    m
}
