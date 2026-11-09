//! Poignées de transformation dans la vue (déplacer, tourner, agrandir), d'après le TransformWidget de
//! CodeWalker : PositionWidget, RotationWidget et ScaleWidget (CodeWalker.World).

use crate::app::{Drag, DwApp};
use crate::math::{v3, Quat, V3};
use eframe::egui::{self, Color32, Painter, Pos2, Stroke};

/// Taille à l'écran des poignées, en pixels : PositionWidget et ScaleWidget 90, RotationWidget 100
/// (constructeurs 0x06000592, 0x060005ce, 0x060005b1).
pub const MOVE_PX: f32 = 90.0;
pub const SCALE_PX: f32 = 90.0;
pub const ROT_PX: f32 = 100.0;
/// Pas de rotation magnétique (RotationWidget.SnapAngleDegrees = 5, 0x060005b1).
pub const ROT_SNAP_DEG: f32 = 5.0;
/// Flèche saisissable de 0,2 à 1,33 fois la taille (PositionWidget::Update 0x06000594, IL_0136–IL_0148).
pub const ARROW_FROM: f32 = 0.2;
pub const ARROW_TO: f32 = 1.33;
/// Tolérance de saisie à l'écran, en pixels — estimé (CodeWalker : boîte de 0,07 × taille autour de l'axe).
const PICK_PX: f32 = 7.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GizmoMode {
    Move,
    Rotate,
    Scale,
}

/// Saisie en cours : axe (0 x, 1 y, 2 z, 3 uniforme) et état de départ.
#[derive(Clone, Debug)]
pub struct GizmoDrag {
    pub axis: u8,
    pub last: f32,
    pub start: f32,
    pub applied: f32,
    /// angle total parcouru (rotation)
    pub total: f32,
    pub center: V3,
    /// axes au début du glisser (monde ou objet)
    pub axes: [V3; 3],
    /// échelles de départ (identifiant, xy, z)
    pub scales: Vec<(u64, f32, f32)>,
    /// positions de départ (dans le monde), pour écarter les objets d'un groupe quand on l'agrandit
    pub positions: Vec<(u64, V3)>,
}

const AXES: [V3; 3] = [V3 { x: 1.0, y: 0.0, z: 0.0 }, V3 { x: 0.0, y: 1.0, z: 0.0 }, V3 { x: 0.0, y: 0.0, z: 1.0 }];

fn axis_color(i: u8) -> Color32 {
    match i {
        0 => Color32::from_rgb(230, 70, 70),
        1 => Color32::from_rgb(80, 210, 80),
        2 => Color32::from_rgb(80, 130, 255),
        _ => Color32::from_rgb(230, 230, 230),
    }
}

/// Distance d'un point à un segment, à l'écran.
fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    let t = if l2 < 1e-6 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
    (a + ab * t - p).length()
}

/// Rotation de `rad` autour de l'axe `a` (unitaire).
pub fn axis_angle(a: V3, rad: f32) -> Quat {
    let (s, c) = (rad * 0.5).sin_cos();
    Quat { x: a.x * s, y: a.y * s, z: a.z * s, w: c }
}

/// Deux directions perpendiculaires à l'axe `i` (pour les anneaux de rotation).
fn ring_basis(ax: &[V3; 3], i: u8) -> (V3, V3) {
    match i {
        0 => (ax[1], ax[2]),
        1 => (ax[2], ax[0]),
        _ => (ax[0], ax[1]),
    }
}

impl DwApp {
    /// Taille des poignées dans le monde (Widget::GetWorldSize 0x0600054c) : pixels ÷ hauteur de la vue ×
    /// distance à la caméra ; en vue carte, pixels ÷ échelle de la carte.
    pub fn gizmo_world_size(&self, center: V3, px: f32) -> f32 {
        let h = self.cam.rect.height().max(1.0);
        if self.cam.map_view {
            px / h * (self.cam.cur_ortho * 2.0)
        } else {
            px / h * (center - self.cam.eye).len().max(0.5)
        }
    }

    /// Axes des poignées : ceux du monde, ou ceux du premier objet choisi (« axes de l'objet », comme
    /// WorldForm::SetWidgetSpace 0x0600031a « World space » / « Object space »).
    pub fn gizmo_axes(&self) -> [V3; 3] {
        if self.gizmo_object_space {
            if let Some(e) = self.selected.first().and_then(|id| self.proj.find_id(*id)).and_then(|r| self.proj.world_entity(r)) {
                if e.mlo.is_none() {
                    return [e.rot.rotate(AXES[0]), e.rot.rotate(AXES[1]), e.rot.rotate(AXES[2])];
                }
            }
        }
        AXES
    }

    fn gizmo_px(&self) -> f32 {
        match self.gizmo_mode {
            GizmoMode::Move => MOVE_PX,
            GizmoMode::Rotate => ROT_PX,
            GizmoMode::Scale => SCALE_PX,
        }
    }

    /// Poignée sous la souris : 0, 1, 2 (axes x, y, z) ou 3 (agrandir partout, au centre).
    pub fn gizmo_pick(&self, m: Pos2) -> Option<u8> {
        if self.selected.is_empty() {
            return None;
        }
        let c = self.selection_center();
        let s = self.gizmo_world_size(c, self.gizmo_px());
        let ax = self.gizmo_axes();
        let mut best: Option<(f32, u8)> = None;
        let mut consider = |d: f32, i: u8| {
            if d <= PICK_PX && best.map(|b| d < b.0).unwrap_or(true) {
                best = Some((d, i));
            }
        };
        match self.gizmo_mode {
            GizmoMode::Move | GizmoMode::Scale => {
                for i in 0..3u8 {
                    if self.cam.map_view && i == 2 {
                        continue;
                    }
                    let a = ax[i as usize];
                    if let (Some((p0, _)), Some((p1, _))) = (self.cam.project(c + a * (s * ARROW_FROM)), self.cam.project(c + a * (s * ARROW_TO))) {
                        consider(seg_dist(m, p0, p1), i);
                    }
                }
                if self.gizmo_mode == GizmoMode::Scale {
                    if let Some((pc, _)) = self.cam.project(c) {
                        consider((pc - m).length() * 0.6, 3);
                    }
                }
            }
            GizmoMode::Rotate => {
                for i in 0..3u8 {
                    if self.cam.map_view && i != 2 {
                        continue;
                    }
                    let (u, v) = ring_basis(&ax, i);
                    let mut prev: Option<Pos2> = None;
                    for k in 0..=48 {
                        let t = k as f32 / 48.0 * std::f32::consts::TAU;
                        let p = self.cam.project(c + (u * t.cos() + v * t.sin()) * s).map(|x| x.0);
                        if let (Some(a), Some(b)) = (prev, p) {
                            consider(seg_dist(m, a, b), i);
                        }
                        prev = p;
                    }
                }
            }
        }
        best.map(|b| b.1)
    }

    /// Position le long d'un axe sous la souris (point du rayon le plus proche de l'axe).
    fn axis_param(&self, m: Pos2, c: V3, a: V3) -> Option<f32> {
        let (o, d) = self.cam.ray(m);
        let w = o - c;
        let (da, wa, wd) = (d.dot(a), w.dot(a), w.dot(d));
        let den = 1.0 - da * da;
        if den.abs() < 1e-4 {
            return None;
        }
        Some((wa - da * wd) / den)
    }

    /// Angle sous la souris autour d'un axe (rayon contre le plan de l'anneau, Widget::GetAxisRayHit 0x0600054a).
    fn axis_angle_at(&self, m: Pos2, c: V3, ax: &[V3; 3], i: u8) -> Option<f32> {
        let a = ax[i as usize];
        let (o, d) = self.cam.ray(m);
        let den = d.dot(a);
        if den.abs() < 1e-5 {
            return None;
        }
        let t = (c - o).dot(a) / den;
        let p = o + d * t - c;
        let (u, v) = ring_basis(ax, i);
        Some(p.dot(v).atan2(p.dot(u)))
    }

    /// Début d'un glisser sur une poignée.
    pub fn gizmo_begin(&mut self, m: Pos2) -> bool {
        let Some(axis) = self.gizmo_pick(m) else { return false };
        let c = self.selection_center();
        let axes = self.gizmo_axes();
        let start = match (self.gizmo_mode, axis) {
            (GizmoMode::Rotate, _) => self.axis_angle_at(m, c, &axes, axis),
            (_, 3) => {
                let p = self.cam.project(c).map(|x| x.0).unwrap_or(m);
                Some(-(m.y - p.y))
            }
            (_, i) => self.axis_param(m, c, axes[i as usize]),
        };
        let Some(start) = start else { return false };
        self.push_undo();
        let scales = self
            .selected
            .iter()
            .filter_map(|id| self.proj.find_id(*id).and_then(|r| self.proj.ent(r)).map(|e| (*id, e.scale_xy, e.scale_z)))
            .collect();
        let positions = self
            .selected
            .iter()
            .filter_map(|id| self.proj.find_id(*id).and_then(|r| self.proj.world_entity(r)).map(|e| (*id, e.pos)))
            .collect();
        self.drag = Drag::Gizmo(GizmoDrag { axis, last: start, start, applied: 0.0, total: 0.0, center: c, axes, scales, positions });
        true
    }

    /// Glisser en cours sur une poignée.
    pub fn gizmo_update(&mut self, m: Pos2) {
        let Drag::Gizmo(g) = &self.drag else { return };
        let mut g = g.clone();
        match self.gizmo_mode {
            GizmoMode::Move => {
                if g.axis > 2 {
                    return;
                }
                let a = g.axes[g.axis as usize];
                let Some(t) = self.axis_param(m, g.center, a) else { return };
                let mut total = t - g.start;
                if self.snap && self.snap_size > 0.0 {
                    total = (total / self.snap_size).round() * self.snap_size;
                }
                let step = total - g.applied;
                if step != 0.0 {
                    self.move_selected(a * step);
                    g.applied = total;
                    self.status = format!("Déplacement {} : {:+.2} m", ["X", "Y", "Z"][g.axis as usize], total);
                }
            }
            GizmoMode::Rotate => {
                let Some(ang) = self.axis_angle_at(m, g.center, &g.axes, g.axis) else { return };
                let mut d = ang - g.last;
                while d > std::f32::consts::PI {
                    d -= std::f32::consts::TAU;
                }
                while d < -std::f32::consts::PI {
                    d += std::f32::consts::TAU;
                }
                g.last = ang;
                g.total += d;
                let mut total = g.total;
                if self.snap {
                    let st = ROT_SNAP_DEG.to_radians();
                    total = (total / st).round() * st;
                }
                let step = total - g.applied;
                if step != 0.0 {
                    self.rotate_selected_axis(g.axes[g.axis as usize], step, g.center);
                    g.applied = total;
                    self.status = format!("Rotation autour de {} : {:+.0}°", ["X", "Y", "Z"][g.axis as usize], total.to_degrees());
                }
            }
            GizmoMode::Scale => {
                let f = if g.axis == 3 {
                    let p = self.cam.project(g.center).map(|x| x.0).unwrap_or(m);
                    let v = -(m.y - p.y);
                    (1.0 + (v - g.start) / 100.0).max(0.05)
                } else {
                    let a = g.axes[g.axis as usize];
                    let Some(t) = self.axis_param(m, g.center, a) else { return };
                    if g.start.abs() < 1e-3 {
                        return;
                    }
                    (t / g.start).clamp(0.05, 50.0)
                };
                for (id, sxy, sz) in g.scales.clone() {
                    if let Some(e) = self.proj.find_id(id).and_then(|r| self.proj.ent_mut(r)) {
                        match g.axis {
                            2 => e.scale_z = sz * f,
                            3 => {
                                e.scale_xy = sxy * f;
                                e.scale_z = sz * f;
                            }
                            _ => e.scale_xy = sxy * f,
                        }
                    }
                }
                // comme MapSelection::SetScale (0x06000472) : chaque objet du groupe s'écarte aussi du centre du
                // même facteur, le long des axes des poignées
                if g.positions.len() > 1 {
                    for (id, p0) in g.positions.clone() {
                        let rel = p0 - g.center;
                        let mut np = g.center;
                        for k in 0..3usize {
                            let comp = rel.dot(g.axes[k]);
                            let fk = if g.axis == 3 || g.axis as usize == k { f } else { 1.0 };
                            np = np + g.axes[k] * (comp * fk);
                        }
                        if let Some(r) = self.proj.find_id(id) {
                            self.proj.set_world_pos(r, np);
                        }
                    }
                }
                self.status = format!("Taille ×{f:.2}");
            }
        }
        self.drag = Drag::Gizmo(g);
    }

    /// Tourne la sélection de `rad` autour de l'axe `a` passant par `center`.
    pub fn rotate_selected_axis(&mut self, a: V3, rad: f32, center: V3) {
        let q = axis_angle(a, rad);
        for id in self.selected.clone() {
            if let Some(r) = self.proj.find_id(id) {
                if let Some(w) = self.proj.world_entity(r) {
                    let np = center + q.rotate(w.pos - center);
                    self.proj.set_world_pos(r, np);
                    if let Some(e) = self.proj.ent_mut(r) {
                        e.rot = q.mul(e.rot).normalized();
                    }
                }
            }
        }
    }

    /// Dessine les poignées autour de la sélection.
    pub fn draw_gizmo(&self, painter: &Painter, hover: Option<u8>) {
        if self.selected.is_empty() {
            return;
        }
        let c = self.selection_center();
        let s = self.gizmo_world_size(c, self.gizmo_px());
        let active = if let Drag::Gizmo(g) = &self.drag { Some(g.axis) } else { hover };
        let ax = self.gizmo_axes();
        let col = |i: u8| if active == Some(i) { Color32::from_rgb(255, 220, 60) } else { axis_color(i) };
        let line = |a: V3, b: V3, st: Stroke| {
            if let (Some((p, _)), Some((q, _))) = (self.cam.project(a), self.cam.project(b)) {
                painter.line_segment([p, q], st);
            }
        };
        match self.gizmo_mode {
            GizmoMode::Move | GizmoMode::Scale => {
                for i in 0..3u8 {
                    if self.cam.map_view && i == 2 {
                        continue;
                    }
                    let a = ax[i as usize];
                    let st = Stroke::new(3.0_f32, col(i));
                    line(c + a * (s * ARROW_FROM), c + a * (s * ARROW_TO), st);
                    let tip = c + a * (s * ARROW_TO);
                    if let Some((p, _)) = self.cam.project(tip) {
                        if self.gizmo_mode == GizmoMode::Move {
                            painter.circle_filled(p, 5.5, col(i));
                        } else {
                            painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(10.0, 10.0)), 1.0, col(i));
                        }
                    }
                }
                if self.gizmo_mode == GizmoMode::Scale {
                    if let Some((p, _)) = self.cam.project(c) {
                        painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(11.0, 11.0)), 1.0, col(3));
                    }
                }
            }
            GizmoMode::Rotate => {
                for i in 0..3u8 {
                    if self.cam.map_view && i != 2 {
                        continue;
                    }
                    let (u, v) = ring_basis(&ax, i);
                    let pts: Vec<Pos2> = (0..=64)
                        .filter_map(|k| {
                            let t = k as f32 / 64.0 * std::f32::consts::TAU;
                            self.cam.project(c + (u * t.cos() + v * t.sin()) * s).map(|x| x.0)
                        })
                        .collect();
                    painter.add(egui::Shape::line(pts, Stroke::new(2.5_f32, col(i))));
                }
            }
        }
        let _ = v3;
    }
}
