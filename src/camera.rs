//! Caméra orbitale (vue 3D) et vue carte (de dessus), valeurs de CodeWalker (voir mech.rs).

use crate::math::{v3, V3};
use crate::mech;
use eframe::egui::{pos2, Pos2, Rect};

pub struct Camera {
    pub target: V3,
    pub yaw: f32,
    pub pitch: f32,
    pub cur_yaw: f32,
    pub cur_pitch: f32,
    pub dist: f32,
    pub cur_dist: f32,
    pub map_view: bool,
    pub ortho: f32,
    pub cur_ortho: f32,
    pub fov: f32,
    pub near: f32,
    /// déplacement de la cible encore à faire (zoom vers la souris), appliqué en douceur comme la distance
    pub pan_left: V3,
    /// cible après le dernier lissage : si elle a changé entre-temps (Aller voir, glisser…), le reste du zoom
    /// vers la souris est abandonné
    seen_target: V3,
    // calculés
    pub eye: V3,
    pub fwd: V3,
    pub right: V3,
    pub up: V3,
    pub rect: Rect,
}

impl Camera {
    pub fn new() -> Camera {
        let mut c = Camera {
            target: v3(0.0, 60.0, 0.0),
            yaw: 0.45,
            pitch: mech::CAM_START_PITCH,
            cur_yaw: 0.45,
            cur_pitch: mech::CAM_START_PITCH,
            dist: mech::CAM_START_DIST,
            cur_dist: mech::CAM_START_DIST,
            map_view: false,
            ortho: 400.0,
            cur_ortho: 400.0,
            fov: mech::CAM_FOV,
            near: 0.1,
            pan_left: V3::ZERO,
            seen_target: V3::ZERO,
            eye: V3::ZERO,
            fwd: V3::ZERO,
            right: V3::ZERO,
            up: V3::ZERO,
            rect: Rect::from_min_size(pos2(0.0, 0.0), eframe::egui::vec2(1280.0, 720.0)),
        };
        c.update_vectors();
        c
    }

    /// Rotation à la souris (Camera::MouseRotate).
    pub fn mouse_rotate(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * mech::CAM_SENSITIVITY;
        self.pitch += dy * mech::CAM_SENSITIVITY;
    }

    /// Molette (Camera::MouseZoom) : ×1.1 en arrière, ÷1.1 en avant.
    #[allow(dead_code)]
    pub fn mouse_zoom(&mut self, steps: f32) {
        let f = if steps > 0.0 { 1.0 / mech::CAM_ZOOM_FACTOR } else { mech::CAM_ZOOM_FACTOR };
        let n = steps.abs().max(1.0);
        for _ in 0..(n as i32) {
            if self.map_view {
                self.ortho *= f;
            } else {
                self.dist *= f;
            }
        }
    }

    /// Molette vers le point `at` sous la souris : ce point reste sous la souris (cible et distance mises à
    /// l'échelle autour de lui, même facteur ×1,1 / ÷1,1 que Camera::MouseZoom). Tout près (moins de
    /// ZOOM_DOLLY_DIST), la distance ne diminue plus : la caméra avance vers le point, pour pouvoir zoomer sans fin
    /// (sinon chaque cran rapproche de moins en moins d'un point qu'on n'atteint jamais) — **estimé**.
    pub fn zoom_at(&mut self, steps: f32, at: Option<V3>) {
        let zoom_in = steps > 0.0;
        let f = if zoom_in { 1.0 / mech::CAM_ZOOM_FACTOR } else { mech::CAM_ZOOM_FACTOR };
        let n = steps.abs().max(1.0) as i32;
        for _ in 0..n {
            let goal = self.target + self.pan_left;
            if self.map_view {
                let new_ortho = (self.ortho * f).clamp(mech::ORTHO_MIN, mech::ORTHO_MAX);
                if let Some(p) = at {
                    let k = new_ortho / self.ortho;
                    let d = goal - p;
                    self.pan_left = self.pan_left + v3(d.x * (k - 1.0), d.y * (k - 1.0), 0.0);
                }
                self.ortho = new_ortho;
                continue;
            }
            if zoom_in && self.dist * f < mech::ZOOM_DOLLY_DIST {
                // tout près : on avance (vers la souris), la distance reste
                let dir = at.map(|p| (p - self.eye).norm()).filter(|d| d.len() > 0.5).unwrap_or(self.fwd);
                let step = self.dist.max(mech::ZOOM_DOLLY_DIST) * (1.0 - f);
                self.pan_left = self.pan_left + dir * step;
                self.dist = self.dist.max(mech::ZOOM_DOLLY_DIST);
                continue;
            }
            let new_dist = (self.dist * f).clamp(mech::CAM_MIN_DIST, mech::CAM_MAX_DIST);
            if let Some(p) = at {
                // le point visé reste sous la souris : cible rapprochée (ou éloignée) de lui du même facteur
                let k = new_dist / self.dist;
                self.pan_left = self.pan_left + (goal - p) * (k - 1.0);
            }
            self.dist = new_dist;
        }
    }

    /// Lissage (Camera::UpdateFollow).
    pub fn update(&mut self, dt: f32) {
        self.pitch = self.pitch.clamp(-mech::CAM_PITCH_LIMIT, mech::CAM_PITCH_LIMIT);
        self.dist = self.dist.clamp(mech::CAM_MIN_DIST, mech::CAM_MAX_DIST);
        self.ortho = self.ortho.clamp(mech::ORTHO_MIN, mech::ORTHO_MAX);
        let k = (mech::CAM_SMOOTHNESS * dt).min(1.0);
        self.cur_yaw += (self.yaw - self.cur_yaw) * k;
        self.cur_pitch += (self.pitch - self.cur_pitch) * k;
        // distance : Camera::UpdateFollow 0x06000166 (IL_00eb–IL_0125) : CurrentDistance += (TargetDistance −
        // CurrentDistance) × ZoomSpeed (0,1, Camera::.ctor 0x06000162) à chaque image ; ramené au temps écoulé
        // (= 0,1 à 60 images/s). L'adoucissement t³ du recul n'y joue que pendant les 2 s (ZoomTargetTime) qui
        // suivent la création de la caméra : ZoomCurrentTime n'est jamais remis à zéro
        let kz = 1.0 - (1.0 - mech::CAM_ZOOM_SPEED).powf(dt * mech::CAM_ZOOM_SPEED_FPS);
        self.cur_dist += (self.dist - self.cur_dist) * kz;
        self.cur_ortho += (self.ortho - self.cur_ortho) * k;
        if (self.target - self.seen_target).len() > 1e-4 {
            self.pan_left = V3::ZERO;
        }
        if self.pan_left.len() > 1e-5 {
            // même pas que la distance : le point visé reste sous la souris pendant le mouvement
            let m = self.pan_left * if self.map_view { k } else { kz };
            self.target = self.target + m;
            self.pan_left = self.pan_left - m;
        } else {
            self.pan_left = V3::ZERO;
        }
        self.seen_target = self.target;
        self.update_vectors();
    }

    pub fn snap(&mut self) {
        self.target = self.target + self.pan_left;
        self.pan_left = V3::ZERO;
        self.cur_yaw = self.yaw;
        self.cur_pitch = self.pitch;
        self.cur_dist = self.dist;
        self.cur_ortho = self.ortho;
        self.update_vectors();
    }

    pub fn update_vectors(&mut self) {
        if self.map_view {
            self.fwd = v3(0.0, 0.0, -1.0);
            self.right = v3(1.0, 0.0, 0.0);
            self.up = v3(0.0, 1.0, 0.0);
            self.eye = self.target + v3(0.0, 0.0, 5000.0);
            return;
        }
        let (sy, cy) = self.cur_yaw.sin_cos();
        let (sp, cp) = self.cur_pitch.sin_cos();
        self.fwd = v3(cp * sy, cp * cy, -sp);
        self.right = self.fwd.cross(v3(0.0, 0.0, 1.0)).norm();
        self.up = self.right.cross(self.fwd).norm();
        self.eye = self.target - self.fwd * self.cur_dist;
    }

    /// Direction de déplacement à plat (avant, droite) pour ZQSD/WASD.
    pub fn flat_axes(&self) -> (V3, V3) {
        if self.map_view {
            return (v3(0.0, 1.0, 0.0), v3(1.0, 0.0, 0.0));
        }
        let f = v3(self.fwd.x, self.fwd.y, 0.0).norm();
        let r = v3(self.right.x, self.right.y, 0.0).norm();
        (f, r)
    }

    /// Vitesse de déplacement (WorldForm::UpdateControlInputs).
    pub fn move_scale(&self) -> f32 {
        if self.map_view {
            (self.ortho * mech::MAP_MOVE_FACTOR).min(mech::MAP_MOVE_CAP)
        } else {
            self.dist.min(mech::MOVE_DIST_CAP)
        }
    }

    fn focal(&self) -> f32 {
        if self.map_view {
            self.rect.height() / (2.0 * self.cur_ortho)
        } else {
            (self.rect.height() * 0.5) / (self.fov * 0.5).tan()
        }
    }

    pub fn to_cam(&self, p: V3) -> V3 {
        let d = p - self.eye;
        v3(d.dot(self.right), d.dot(self.up), d.dot(self.fwd))
    }

    pub fn cam_to_screen(&self, c: V3) -> Pos2 {
        let f = self.focal();
        let ctr = self.rect.center();
        if self.map_view {
            pos2(ctr.x + c.x * f, ctr.y - c.y * f)
        } else {
            pos2(ctr.x + c.x * f / c.z, ctr.y - c.y * f / c.z)
        }
    }

    pub fn project(&self, p: V3) -> Option<(Pos2, f32)> {
        let c = self.to_cam(p);
        if c.z < self.near {
            return None;
        }
        Some((self.cam_to_screen(c), c.z))
    }

    /// Rayon partant de la souris (origine, direction).
    pub fn ray(&self, s: Pos2) -> (V3, V3) {
        let f = self.focal();
        let ctr = self.rect.center();
        let sx = (s.x - ctr.x) / f;
        let sy = -(s.y - ctr.y) / f;
        if self.map_view {
            (self.eye + self.right * sx + self.up * sy, self.fwd)
        } else {
            (self.eye, (self.fwd + self.right * sx + self.up * sy).norm())
        }
    }

    /// Point du sol (plan z = `gz`) sous la souris.
    pub fn ground_point(&self, s: Pos2, gz: f32) -> Option<V3> {
        let (o, d) = self.ray(s);
        if d.z.abs() < 1e-6 {
            return None;
        }
        let t = (gz - o.z) / d.z;
        if t <= 0.0 {
            return None;
        }
        Some(o + d * t)
    }

    /// Mètres par pixel près de la cible (pour les tailles de pinceau).
    pub fn meters_per_pixel(&self) -> f32 {
        if self.map_view {
            1.0 / self.focal()
        } else {
            self.cur_dist / self.focal()
        }
    }

    /// Plans proche et lointain utilisés par la carte graphique.
    pub fn near_far(&self) -> (f32, f32) {
        if self.map_view {
            (1.0, 12000.0)
        } else {
            ((self.cur_dist * 0.01).clamp(mech::ZOOM_NEAR_MIN, 20.0), 30000.0)
        }
    }

    /// Point du monde vu au pixel `s`, à partir de la profondeur lue dans la carte graphique (0..1).
    pub fn unproject_depth(&self, s: Pos2, depth01: f32) -> Option<V3> {
        if !(0.0..0.999_999).contains(&depth01) {
            return None;
        }
        let (n, f) = self.near_far();
        let z = depth01 * 2.0 - 1.0;
        let (o, d) = self.ray(s);
        if self.map_view {
            let dist = n + (z + 1.0) * 0.5 * (f - n);
            Some(o + d * dist)
        } else {
            let along = 2.0 * f * n / ((f + n) - z * (f - n));
            let k = d.dot(self.fwd);
            if k <= 1e-6 {
                return None;
            }
            Some(o + d * (along / k))
        }
    }

    /// Matrice vue × projection (colonnes, convention OpenGL), identique à `cam_to_screen`.
    pub fn view_proj(&self) -> [f32; 16] {
        let a = self.rect.width() / self.rect.height().max(1.0);
        let (r, u, f, e) = (self.right, self.up, self.fwd, self.eye);
        let mut m = [0f32; 16];
        let mut set = |row: usize, v: [f32; 4]| {
            for (c, x) in v.iter().enumerate() {
                m[c * 4 + row] = *x;
            }
        };
        if self.map_view {
            let (n, fa) = self.near_far();
            let sx = 1.0 / (self.cur_ortho * a);
            let sy = 1.0 / self.cur_ortho;
            set(0, [r.x * sx, r.y * sx, r.z * sx, -r.dot(e) * sx]);
            set(1, [u.x * sy, u.y * sy, u.z * sy, -u.dot(e) * sy]);
            let k = 2.0 / (fa - n);
            set(2, [f.x * k, f.y * k, f.z * k, -f.dot(e) * k - 2.0 * n / (fa - n) - 1.0]);
            set(3, [0.0, 0.0, 0.0, 1.0]);
        } else {
            let (n, fa) = self.near_far();
            let t = 1.0 / (self.fov * 0.5).tan();
            set(0, [r.x * t / a, r.y * t / a, r.z * t / a, -r.dot(e) * t / a]);
            set(1, [u.x * t, u.y * t, u.z * t, -u.dot(e) * t]);
            let aa = (fa + n) / (n - fa);
            let bb = 2.0 * fa * n / (n - fa);
            set(2, [-aa * f.x, -aa * f.y, -aa * f.z, aa * f.dot(e) + bb]);
            set(3, [f.x, f.y, f.z, -f.dot(e)]);
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(c: &mut Camera) {
        for _ in 0..400 {
            c.update(1.0 / 60.0);
        }
    }

    #[test]
    fn zoom_goes_through_and_keeps_point_under_mouse() {
        let mut c = Camera::new();
        settle(&mut c);
        let p = v3(30.0, 80.0, 0.0);
        let s0 = c.project(p).unwrap().0;
        // 10 crans : le point visé reste à peu près sous la souris
        for _ in 0..10 {
            c.zoom_at(1.0, Some(p));
        }
        settle(&mut c);
        let s1 = c.project(p).unwrap().0;
        assert!((s1 - s0).length() < 3.0, "point déplacé à l'écran : {s0:?} → {s1:?}");
        // 120 crans de plus : on arrive tout près du point (moins de 2 m), sans rester bloqué loin
        let d0 = (c.eye - p).len();
        for _ in 0..120 {
            c.zoom_at(1.0, Some(p));
            c.update(1.0 / 60.0);
        }
        settle(&mut c);
        let d1 = (c.eye - p).len();
        assert!(d1 < 2.0 && d1 < d0, "distance au point {d0} → {d1}");
    }
}
