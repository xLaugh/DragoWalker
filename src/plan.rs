//! Plan des intérieurs vu de dessus, étage par étage : tracer une pièce en glissant (une porte se fait toute seule
//! avec chaque pièce voisine du même étage), couper une pièce en deux par un mur (avec sa porte), poser un escalier
//! vers la pièce de l'étage du dessus (trémie ; la coque y met les marches et leur collision), poser de vraies portes
//! du jeu dans les passages. Export : script qui enregistre les portes des intérieurs dans le système de portes du
//! jeu, ouvertes (dw_doors.lua).

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use crate::model::{self, Entity};
use eframe::egui::{self, Color32, PointerButton, Pos2, RichText};

/// Grille du plan, porte (largeur, hauteur), épaisseur de plancher, trémie d'escalier (longueur, largeur) (m) —
/// **estimé**.
pub const PLAN_GRID: f32 = 0.25;
pub const DOOR_W: f32 = 1.0;
pub const DOOR_H: f32 = 2.2;
pub const SLAB: f32 = 0.25;
pub const STAIR_HOLE: f32 = 3.0;
pub const STAIR_W: f32 = 1.2;

#[derive(Default)]
pub struct PlanState {
    /// 0 rien, 1 tracer une pièce, 2 couper une pièce, 3 poser un escalier
    pub mode: u8,
    pub floor: u8,
    pub floor_h: f32,
    /// début du rectangle en cours (repère de l'intérieur)
    pub drag: Option<V3>,
    /// modèle des portes posées par « Poser les portes » (vide : choisi par DragoWalker)
    pub door_model: String,
    /// coque modelable (design.rs) : points cliqués du mur ou de la pièce en cours, réglages
    pub pts: Vec<V3>,
    pub wall_t: f32,
    pub wall_tex: u8,
    pub wall_color: Option<[u8; 3]>,
    pub base: bool,
    pub hole_kind: u8,
    pub block_kind: u8,
    pub floor_tex: u8,
    pub around: bool,
    pub drop: f32,
    /// lumière à poser : genre, couleur, intensité, portée
    pub light_kind: u8,
    pub light_color: [u8; 3],
    pub light_int: f32,
    pub light_range: f32,
}

fn snap(v: f32) -> f32 {
    (v / PLAN_GRID).round() * PLAN_GRID
}

impl DwApp {
    /// Intérieur en cours (onglet Intérieur) et son placement dans le monde.
    fn plan_target(&self) -> Option<(usize, usize, V3, Quat)> {
        let (ti, mi) = self.cur_mlo?;
        let m = self.proj.ytyps.get(ti)?.mlos.get(mi)?;
        let inst = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.mlo.is_some() && e.archetype.eq_ignore_ascii_case(&m.name))?;
        Some((ti, mi, inst.pos, inst.rot))
    }

    fn plan_fh(&self) -> f32 {
        if self.plan.floor_h < 2.0 { 3.0 } else { self.plan.floor_h }
    }

    fn plan_z(&self) -> f32 {
        self.plan.floor as f32 * self.plan_fh()
    }

    /// Point de la souris sur le sol de l'étage, repère de l'intérieur, aligné sur la grille.
    fn plan_point(&self, mouse: Pos2) -> Option<V3> {
        let (_, _, pos, rot) = self.plan_target()?;
        let g = self.cam.ground_point(mouse, pos.z + self.plan_z())?;
        let l = rot.inverse().rotate(g - pos);
        Some(v3(snap(l.x), snap(l.y), self.plan_z()))
    }

    /// Pièces de l'étage affiché.
    fn plan_rooms(&self, m: &model::MloArchetype) -> Vec<usize> {
        let z = self.plan_z();
        (1..m.rooms.len()).filter(|&i| (m.rooms[i].bb_min.z - z).abs() < 0.6).collect()
    }

    /// Clics dans la vue en mode plan.
    pub fn plan_input(&mut self, resp: &egui::Response, ptr: Option<Pos2>, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            // un mur ou une pièce en cours : Échap le termine (ou l'abandonne), un second Échap quitte le plan
            if !self.plan.pts.is_empty() {
                self.plan.pts.clear();
                return;
            }
            self.plan.mode = 0;
            self.plan.drag = None;
            return;
        }
        if (4..=11).contains(&self.plan.mode) {
            self.design_input(resp, ptr, ctx);
            return;
        }
        let Some(mouse) = ptr else { return };
        match self.plan.mode {
            1 => {
                if resp.drag_started_by(PointerButton::Primary) {
                    let start = ctx.input(|i| i.pointer.press_origin()).unwrap_or(mouse);
                    self.plan.drag = self.plan_point(start);
                }
                if resp.drag_stopped_by(PointerButton::Primary) {
                    if let (Some(a), Some(b)) = (self.plan.drag.take(), self.plan_point(mouse)) {
                        self.status = self.plan_add_room(a, b);
                    }
                }
            }
            2 if resp.clicked_by(PointerButton::Primary) => {
                if let Some(p) = self.plan_point(mouse) {
                    self.status = self.plan_split(p);
                }
            }
            3 if resp.clicked_by(PointerButton::Primary) => {
                if let Some(p) = self.plan_point(mouse) {
                    self.status = self.plan_stairs(p);
                }
            }
            _ => {}
        }
    }

    /// Ajoute une pièce (rectangle a-b de l'étage) et une porte avec chaque pièce voisine du même étage.
    pub fn plan_add_room(&mut self, a: V3, b: V3) -> String {
        let Some((ti, mi, _, _)) = self.plan_target() else { return "Choisis d'abord un intérieur placé dans le projet (onglet Intérieur).".into() };
        let (z0, fh) = (self.plan_z(), self.plan_fh());
        let mn = v3(a.x.min(b.x), a.y.min(b.y), z0);
        let mx = v3(a.x.max(b.x), a.y.max(b.y), z0 + fh - SLAB);
        if mx.x - mn.x < 1.0 || mx.y - mn.y < 1.0 {
            return "Pièce trop petite (au moins 1 m de côté).".into();
        }
        let floor_rooms = { let m = &self.proj.ytyps[ti].mlos[mi]; self.plan_rooms(m) };
        let m = &self.proj.ytyps[ti].mlos[mi];
        if floor_rooms.iter().any(|&i| {
            let r = &m.rooms[i];
            mn.x < r.bb_max.x - 0.01 && mx.x > r.bb_min.x + 0.01 && mn.y < r.bb_max.y - 0.01 && mx.y > r.bb_min.y + 0.01
        }) {
            return "Cette pièce en chevaucherait une autre du même étage.".into();
        }
        if m.rooms.len() >= 32 {
            return "31 pièces au plus dans un intérieur (numéro de pièce de la collision sur 5 bits).".into();
        }
        self.push_undo();
        let m = &mut self.proj.ytyps[ti].mlos[mi];
        let ri = m.rooms.len();
        m.rooms.push(model::Room::new(&format!("e{}_piece{ri}", self.plan.floor + 1), mn, mx));
        let mut doors = 0;
        for &oi in &floor_rooms {
            if let Some(c) = shared_door(&m.rooms[oi], &m.rooms[ri], z0) {
                m.portals.push(model::Portal::new(oi as u32, ri as u32, c));
                doors += 1;
            }
        }
        self.opts.show_rooms = true;
        format!("Pièce {ri} ajoutée ({:.2} × {:.2} m), {doors} porte(s) vers les pièces voisines.", mx.x - mn.x, mx.y - mn.y)
    }

    /// Coupe en deux la pièce sous le clic par un mur perpendiculaire à son grand côté, avec une porte.
    pub fn plan_split(&mut self, p: V3) -> String {
        let Some((ti, mi, _, _)) = self.plan_target() else { return String::new() };
        let floor_rooms = { let m = &self.proj.ytyps[ti].mlos[mi]; self.plan_rooms(m) };
        let m = &self.proj.ytyps[ti].mlos[mi];
        let Some(ri) = floor_rooms.into_iter().find(|&i| { let r = &m.rooms[i]; p.x > r.bb_min.x && p.x < r.bb_max.x && p.y > r.bb_min.y && p.y < r.bb_max.y }) else {
            return "Clique dans une pièce de l'étage affiché.".into();
        };
        let r = m.rooms[ri].clone();
        let along_x = r.bb_max.x - r.bb_min.x >= r.bb_max.y - r.bb_min.y;
        let cut = if along_x { p.x } else { p.y };
        let (lo, hi) = if along_x { (r.bb_min.x, r.bb_max.x) } else { (r.bb_min.y, r.bb_max.y) };
        if cut - lo < 1.0 || hi - cut < 1.0 {
            return "Trop près d'un mur : chaque moitié doit faire au moins 1 m.".into();
        }
        if m.rooms.len() >= 32 {
            return "31 pièces au plus dans un intérieur.".into();
        }
        self.push_undo();
        let m = &mut self.proj.ytyps[ti].mlos[mi];
        let ni = m.rooms.len();
        let mut b = r.clone();
        b.name = format!("{}_b", r.name);
        if along_x {
            m.rooms[ri].bb_max.x = cut;
            b.bb_min.x = cut;
        } else {
            m.rooms[ri].bb_max.y = cut;
            b.bb_min.y = cut;
        }
        m.rooms.push(b);
        // portails et objets de l'autre côté du mur : à la nouvelle pièce
        let beyond = |q: V3| if along_x { q.x > cut } else { q.y > cut };
        for pt in m.portals.iter_mut() {
            let c = (pt.corners[0] + pt.corners[1] + pt.corners[2] + pt.corners[3]) * 0.25;
            if beyond(c) {
                if pt.room_from as usize == ri {
                    pt.room_from = ni as u32;
                }
                if pt.room_to as usize == ri {
                    pt.room_to = ni as u32;
                }
            }
        }
        for e in m.entities.iter_mut() {
            if e.room == ri && beyond(e.pos) {
                e.room = ni;
            }
        }
        let (a, bb) = (m.rooms[ri].clone(), m.rooms[ni].clone());
        if let Some(c) = shared_door(&a, &bb, a.bb_min.z) {
            m.portals.push(model::Portal::new(ri as u32, ni as u32, c));
        }
        format!("Pièce coupée en deux par un mur (avec une porte) : pièces {ri} et {ni}.")
    }

    /// Escalier depuis le clic (pièce de cet étage) vers la pièce de l'étage du dessus : trémie le long du grand côté.
    pub fn plan_stairs(&mut self, p: V3) -> String {
        let Some((ti, mi, _, _)) = self.plan_target() else { return String::new() };
        let m = &self.proj.ytyps[ti].mlos[mi];
        let inside = |r: &model::Room, q: V3| q.x > r.bb_min.x && q.x < r.bb_max.x && q.y > r.bb_min.y && q.y < r.bb_max.y;
        let z = self.plan_z();
        let Some(lo) = (1..m.rooms.len()).find(|&i| (m.rooms[i].bb_min.z - z).abs() < 0.6 && inside(&m.rooms[i], p)) else {
            return "Clique dans une pièce de l'étage affiché.".into();
        };
        let lr = m.rooms[lo].clone();
        let Some(up) = (1..m.rooms.len()).find(|&i| m.rooms[i].bb_min.z > lr.bb_max.z - 0.1 && m.rooms[i].bb_min.z < lr.bb_max.z + 1.0 && inside(&m.rooms[i], p)) else {
            return "Pas de pièce juste au-dessus de ce point : trace d'abord la pièce de l'étage du dessus.".into();
        };
        let ur = &m.rooms[up];
        let along_x = lr.bb_max.x - lr.bb_min.x >= lr.bb_max.y - lr.bb_min.y;
        let (x0, x1, y0, y1) = if along_x {
            let x1 = (p.x + STAIR_HOLE).min(lr.bb_max.x - 0.2).min(ur.bb_max.x - 0.2);
            let y0 = p.y.max(lr.bb_min.y).max(ur.bb_min.y);
            (p.x, x1, y0, (y0 + STAIR_W).min(lr.bb_max.y).min(ur.bb_max.y))
        } else {
            let y1 = (p.y + STAIR_HOLE).min(lr.bb_max.y - 0.2).min(ur.bb_max.y - 0.2);
            let x0 = p.x.max(lr.bb_min.x).max(ur.bb_min.x);
            (x0, (x0 + STAIR_W).min(lr.bb_max.x).min(ur.bb_max.x), p.y, y1)
        };
        if x1 - x0 < 0.6 || y1 - y0 < 0.6 {
            return "Pas assez de place pour la trémie ici.".into();
        }
        self.push_undo();
        let zc = lr.bb_max.z;
        self.proj.ytyps[ti].mlos[mi].portals.push(model::Portal::new(lo as u32, up as u32, [v3(x0, y0, zc), v3(x1, y0, zc), v3(x1, y1, zc), v3(x0, y1, zc)]));
        format!("Escalier de la pièce {lo} vers la pièce {up} : trémie de {:.1} × {:.1} m, marches et collision faites par la coque.", x1 - x0, y1 - y0)
    }

    /// Vraies portes du jeu dans les passages (entre deux pièces, et vers l'extérieur quand le passage a la taille
    /// d'une porte) qui n'en ont pas encore.
    pub fn plan_doors(&mut self) -> String {
        let Some((ti, mi, _, _)) = self.plan_target() else { return "Choisis d'abord un intérieur placé dans le projet.".into() };
        // modèle choisi dans le plan, sinon une porte d'intérieur du jeu
        let chosen = (!self.plan.door_model.is_empty()).then(|| self.plan.door_model.clone()).and_then(|n| self.game.world.as_ref().and_then(|w| w.archetypes.get(&crate::gta::crypto::joaat(&n)).map(|a| (n.clone(), a.bb_min, a.bb_max))));
        let Some((dn, dmn, dmx)) = chosen.or_else(|| self.find_game_prop(&["door"], &["v_ilev_ra_door", "v_ilev_fh_door", "v_ilev_"], v3(0.6, 0.02, 1.9), v3(1.4, 0.4, 2.7), v3(1.0, 0.1, 2.2))) else {
            return "Aucune porte du jeu trouvée : charge le jeu (onglet Jeu).".into();
        };
        let dw = (dmx.x - dmn.x).max(0.1);
        self.push_undo();
        let m = &mut self.proj.ytyps[ti].mlos[mi];
        let mut n = 0;
        let portals = m.portals.clone();
        for p in &portals {
            let c = p.corners;
            let h = c[1].z - c[0].z;
            if !(1.8..=3.2).contains(&h) {
                continue; // fenêtre, trémie ou grande baie
            }
            let room = if p.room_from != 0 { p.room_from as usize } else { p.room_to as usize };
            let Some(r) = m.rooms.get(room) else { continue };
            if c[0].z - r.bb_min.z > 0.3 {
                continue; // allège : une fenêtre
            }
            if m.entities.iter().any(|e| e.archetype.contains("door") && (e.pos - c[0]).len() < 0.8) {
                continue;
            }
            let along = c[3] - c[0];
            let len = along.len();
            if len < 0.5 || len > 2.0 {
                continue;
            }
            let dir = along * (1.0 / len);
            // le côté X le plus petit du modèle sur le bord du passage, son bas au sol, son épaisseur centrée
            // taille d'origine : le jeu ne montre pas une porte étirée ; centrée dans le passage
            let _ = dw;
            let (sxy, sz) = (1.0f32, 1.0f32);
            let c0 = c[0] - dir * (((dmx.x - dmn.x) - len) * 0.5);
            let c = [c0, c[1], c[2], c[3]];
            let side = v3(-dir.y, dir.x, 0.0);
            let hinge = c[0] - dir * (dmn.x * sxy) - side * ((dmn.y + dmx.y) * 0.5 * sxy) + v3(0.0, 0.0, -dmn.z * sz);
            let mut e = Entity::new(&dn, hinge);
            e.rot = Quat::from_z(dir.y.atan2(dir.x));
            e.scale_xy = sxy;
            e.scale_z = sz;
            e.flags = crate::mech::MLO_ENTITY_FLAGS;
            e.room = room;
            m.entities.push(e);
            n += 1;
        }
        format!("{n} porte(s) du jeu posée(s) ({dn}). À l'export, elles sont enregistrées comme portes ouvertes (dw_doors.lua).")
    }

    /// Point de la souris sur le sol de l'étage, aimanté aux bouts des murs de la coque (à 30 cm), sinon à la grille.
    fn design_point(&self, mouse: Pos2) -> Option<V3> {
        let p = self.plan_point(mouse)?;
        let (_, _, pos, rot) = self.plan_target()?;
        let raw = self.cam.ground_point(mouse, pos.z + self.plan_z()).map(|g| rot.inverse().rotate(g - pos))?;
        let z = self.plan_z();
        let d = self.cur_design()?;
        let mut best: Option<(f32, V3)> = None;
        for w in d.walls.iter().filter(|w| (w.z0 - z).abs() < 0.3) {
            for e in [w.a, w.b] {
                let dd = v3(e[0] - raw.x, e[1] - raw.y, 0.0).len();
                if dd < 0.3 && best.map(|b| dd < b.0).unwrap_or(true) {
                    best = Some((dd, v3(e[0], e[1], z)));
                }
            }
        }
        for r in d.rooms.iter().filter(|r| (r.z0 - z).abs() < 0.3) {
            for e in &r.pts {
                let dd = v3(e[0] - raw.x, e[1] - raw.y, 0.0).len();
                if dd < 0.3 && best.map(|b| dd < b.0).unwrap_or(true) {
                    best = Some((dd, v3(e[0], e[1], z)));
                }
            }
        }
        Some(best.map(|b| b.1).unwrap_or(p))
    }

    /// Coque modelable de l'intérieur en cours.
    fn cur_design(&self) -> Option<&crate::design::Design> {
        let (ti, mi, _, _) = self.plan_target()?;
        let n = &self.proj.ytyps[ti].mlos[mi].name;
        self.proj.designs.iter().find(|d| d.mlo.eq_ignore_ascii_case(n))
    }

    /// Clics des outils de la coque modelable.
    fn design_input(&mut self, resp: &egui::Response, ptr: Option<Pos2>, ctx: &egui::Context) {
        let Some((ti, mi, _, _)) = self.plan_target() else { return };
        let mname = self.proj.ytyps[ti].mlos[mi].name.clone();
        let Some(mouse) = ptr else { return };
        let (z0, fh) = (self.plan_z(), self.plan_fh());
        let h = fh - SLAB;
        let finish = ctx.input(|i| i.key_pressed(egui::Key::Enter)) || resp.double_clicked_by(PointerButton::Primary);
        match self.plan.mode {
            // mur : clics successifs, chaque clic ajoute un pan depuis le point précédent ; double-clic / Entrée : fini
            4 => {
                if finish {
                    self.plan.pts.clear();
                    return;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Backspace)) {
                    if self.plan.pts.len() >= 2 {
                        self.push_undo();
                        let d = self.design_mut(&mname);
                        d.walls.pop();
                    }
                    self.plan.pts.pop();
                    return;
                }
                if !resp.clicked_by(PointerButton::Primary) {
                    return;
                }
                let Some(p) = self.design_point(mouse) else { return };
                if let Some(&last) = self.plan.pts.last() {
                    if (p - last).len() < 0.2 {
                        return;
                    }
                    self.push_undo();
                    let (t, tex, color, base) = (self.plan.wall_t, self.plan.wall_tex, self.plan.wall_color, self.plan.base);
                    self.design_mut(&mname).walls.push(crate::design::DWall { a: [last.x, last.y], b: [p.x, p.y], z0, h, t, tex, color, base, holes: Vec::new(), back: None });
                    self.status = format!("Mur de {:.2} m. Clique le point suivant · double-clic ou Entrée : fini · Retour arrière : retirer le dernier pan.", (p - last).len());
                }
                self.plan.pts.push(p);
            }
            // ouverture : clic sur un mur de l'étage
            5 if resp.clicked_by(PointerButton::Primary) => {
                let Some(p) = self.plan_point(mouse) else { return };
                let Some(d) = self.cur_design() else { self.status = "Trace d'abord un mur.".into(); return };
                let mut best: Option<(f32, usize, f32)> = None;
                for (wi, w) in d.walls.iter().enumerate().filter(|(_, w)| (w.z0 - z0).abs() < 0.3) {
                    let (a, b) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
                    let len = (b - a).len().max(1e-3);
                    let dir = (b - a) * (1.0 / len);
                    let s = (v3(p.x, p.y, 0.0) - a).dot(dir).clamp(0.0, len);
                    let dist = (a + dir * s - v3(p.x, p.y, 0.0)).len();
                    if dist < w.t * 0.5 + 0.5 && best.map(|x| dist < x.0).unwrap_or(true) {
                        best = Some((dist, wi, s));
                    }
                }
                let Some((_, wi, s)) = best else { self.status = "Clique sur un mur de l'étage affiché.".into(); return };
                let (name, ww, hh, sill) = crate::design::HOLE_KINDS[(self.plan.hole_kind as usize).min(3)];
                let w = d.walls[wi].clone();
                let len = v3(w.b[0] - w.a[0], w.b[1] - w.a[1], 0.0).len();
                if len < ww + 0.1 {
                    self.status = format!("Mur trop court pour une {} de {ww:.1} m.", name.to_lowercase());
                    return;
                }
                let s = s.clamp(ww * 0.5 + 0.05, len - ww * 0.5 - 0.05);
                if w.holes.iter().any(|o| (o.s - s).abs() < (o.w + ww) * 0.5 + 0.1) {
                    self.status = "Une ouverture est déjà là.".into();
                    return;
                }
                self.push_undo();
                let hk = self.plan.hole_kind;
                let hi = {
                    let d = self.design_mut(&mname);
                    d.walls[wi].holes.push(crate::design::DHole { s, w: ww, sill, h: hh.min(w.h - sill - 0.05), kind: hk });
                    d.walls[wi].holes.len() - 1
                };
                let msg = self.design_hole_portal(ti, mi, wi, hi);
                self.status = format!("{name} : {msg}");
            }
            // pièce libre : clics sur les coins ; double-clic / Entrée : la pièce est créée
            6 => {
                if finish {
                    let pts = std::mem::take(&mut self.plan.pts);
                    self.status = self.design_add_room(&pts);
                    return;
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Backspace)) {
                    self.plan.pts.pop();
                    return;
                }
                if resp.clicked_by(PointerButton::Primary) {
                    if let Some(p) = self.design_point(mouse) {
                        self.plan.pts.push(p);
                        self.status = format!("Pièce : {} coin(s). Double-clic ou Entrée : créer · Retour arrière : retirer le dernier.", self.plan.pts.len());
                    }
                }
            }
            // bloc : glisser un rectangle
            7 => {
                if resp.drag_started_by(PointerButton::Primary) {
                    let start = ctx.input(|i| i.pointer.press_origin()).unwrap_or(mouse);
                    self.plan.drag = self.plan_point(start);
                }
                if resp.drag_stopped_by(PointerButton::Primary) {
                    if let (Some(a), Some(b)) = (self.plan.drag.take(), self.plan_point(mouse)) {
                        if (a.x - b.x).abs() < 0.2 || (a.y - b.y).abs() < 0.2 {
                            self.status = "Bloc trop petit (20 cm au moins de chaque côté).".into();
                            return;
                        }
                        let (name, bh) = crate::design::BLOCK_KINDS[(self.plan.block_kind as usize).min(4)];
                        let bh = if bh <= 0.0 { h } else { bh };
                        self.push_undo();
                        let (bk, tex, color) = (self.plan.block_kind, self.plan.wall_tex, self.plan.wall_color);
                        self.design_mut(&mname).blocks.push(crate::design::DBlock { a: [a.x, a.y], b: [b.x, b.y], z0, h: bh, kind: bk, tex, color });
                        self.status = format!("{name} de {:.2} × {:.2} m, {:.2} m de haut.", (b.x - a.x).abs(), (b.y - a.y).abs(), bh);
                    }
                }
            }
            // lumière : clic sur le sol, sous le plafond (spot, néon) ou à 2,2 m (ampoule)
            9 if resp.clicked_by(PointerButton::Primary) => {
                let Some(p) = self.plan_point(mouse) else { return };
                let (k, color, int, range) = (self.plan.light_kind.min(2), self.plan.light_color, self.plan.light_int, self.plan.light_range);
                let z = if k == 1 { z0 + 2.2 } else { z0 + h - 0.08 - self.cur_design().and_then(|d| d.rooms.iter().find(|r| (r.z0 - z0).abs() < 0.3 && crate::design::in_poly(&r.pts, p.x, p.y)).map(|r| r.drop)).unwrap_or(0.0) };
                self.push_undo();
                self.design_mut(&mname).lights.push(crate::design::DLight { p: [p.x, p.y, z], kind: k, color, intensity: int, range });
                self.status = format!("{} posée (portée {range:.0} m). Elle est rangée dans le modèle de la pièce, comme les lumières des intérieurs du jeu.", crate::design::LIGHT_KINDS[k as usize].0);
            }
            // peindre un côté : la texture et la couleur choisies, sur le côté du mur où l'on clique
            10 if resp.clicked_by(PointerButton::Primary) => {
                let Some(p) = self.plan_point(mouse) else { return };
                let Some(d) = self.cur_design() else { return };
                let mut best: Option<(f32, usize, f32)> = None;
                for (wi, w) in d.walls.iter().enumerate().filter(|(_, w)| (w.z0 - z0).abs() < 0.3) {
                    let (a, b) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
                    let len = (b - a).len().max(1e-3);
                    let dir = (b - a) * (1.0 / len);
                    let q = v3(p.x, p.y, 0.0) - a;
                    let s = q.dot(dir).clamp(0.0, len);
                    let dist = (a + dir * s - v3(p.x, p.y, 0.0)).len();
                    if dist < w.t * 0.5 + 0.6 && best.map(|x| dist < x.0).unwrap_or(true) {
                        best = Some((dist, wi, q.dot(v3(-dir.y, dir.x, 0.0))));
                    }
                }
                let Some((_, wi, side)) = best else { self.status = "Clique près d'un mur, du côté à peindre.".into(); return };
                self.push_undo();
                let (tex, color) = (self.plan.wall_tex, self.plan.wall_color);
                let d = self.design_mut(&mname);
                let w = &mut d.walls[wi];
                if side >= 0.0 {
                    let old = (w.tex, w.color);
                    w.back = Some(w.back.unwrap_or(old));
                    w.tex = tex;
                    w.color = color;
                } else {
                    w.back = Some((tex, color));
                }
                self.status = "Côté du mur repeint (l'autre côté garde sa texture).".into();
            }
            // miroir : clic près d'un mur, du côté où il doit être
            11 if resp.clicked_by(PointerButton::Primary) => {
                let Some(p) = self.plan_point(mouse) else { return };
                self.status = self.design_add_mirror(ti, mi, p, z0);
            }
            // effacer : ouverture, bloc, mur ou pièce libre sous le clic
            8 if resp.clicked_by(PointerButton::Primary) => {
                let Some(p) = self.plan_point(mouse) else { return };
                self.status = self.design_erase(ti, mi, p);
            }
            _ => {}
        }
    }

    /// Pièce de forme libre : une pièce de l'intérieur (sa boîte entoure le contour), son sol, son plafond et, au
    /// choix, des murs autour (sauf là où un mur existe déjà).
    pub fn design_add_room(&mut self, pts: &[V3]) -> String {
        let Some((ti, mi, _, _)) = self.plan_target() else { return String::new() };
        if pts.len() < 3 {
            return "Il faut au moins 3 coins pour une pièce.".into();
        }
        let poly: Vec<[f32; 2]> = pts.iter().map(|p| [p.x, p.y]).collect();
        let area = crate::atelier::signed_area(&poly).abs();
        if area < 1.0 {
            return "Pièce trop petite (moins de 1 m²).".into();
        }
        if self.proj.ytyps[ti].mlos[mi].rooms.len() >= 32 {
            return "31 pièces au plus dans un intérieur (numéro de pièce de la collision sur 5 bits).".into();
        }
        let (z0, fh) = (self.plan_z(), self.plan_fh());
        let h = fh - SLAB;
        let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, z0), v3(f32::MIN, f32::MIN, z0 + h));
        for p in &poly {
            mn.x = mn.x.min(p[0]);
            mn.y = mn.y.min(p[1]);
            mx.x = mx.x.max(p[0]);
            mx.y = mx.y.max(p[1]);
        }
        self.push_undo();
        let mname = self.proj.ytyps[ti].mlos[mi].name.clone();
        let m = &mut self.proj.ytyps[ti].mlos[mi];
        let ri = m.rooms.len();
        let rname = format!("e{}_piece{ri}", self.plan.floor + 1);
        let mut r = model::Room::new(&rname, mn, mx);
        r.free = true;
        m.rooms.push(r);
        let (floor, drop, around) = (self.plan.floor_tex, self.plan.drop, self.plan.around);
        let (t, tex, color, base) = (self.plan.wall_t, self.plan.wall_tex, self.plan.wall_color, self.plan.base);
        let d = self.design_mut(&mname);
        d.rooms.push(crate::design::DRoom { room: rname, pts: poly.clone(), z0, h, floor, ceil_color: None, drop });
        let mut nw = 0;
        if around {
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                let (va, vb) = (v3(a[0], a[1], 0.0), v3(b[0], b[1], 0.0));
                let len = (vb - va).len();
                if len < 0.1 {
                    continue;
                }
                // déjà un mur sur ce côté (pièce voisine) : pas de second mur
                let dir = (vb - va) * (1.0 / len);
                let covered = d.walls.iter().filter(|w| (w.z0 - z0).abs() < 0.3).any(|w| {
                    let (wa, wb) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
                    let off = |q: V3| { let r = q - va; (r - dir * r.dot(dir)).len() };
                    let (sa, sb) = ((wa - va).dot(dir), (wb - va).dot(dir));
                    off(wa) < 0.05 && off(wb) < 0.05 && sa.min(sb) <= 0.1 && sa.max(sb) >= len - 0.1
                });
                if covered {
                    continue;
                }
                d.walls.push(crate::design::DWall { a, b, z0, h, t, tex, color, base, holes: Vec::new(), back: None });
                nw += 1;
            }
        }
        self.opts.show_rooms = true;
        format!("Pièce {ri} de forme libre ({area:.1} m², {} coins), {nw} mur(s) autour. Pose ses portes avec « Ouverture ».", poly.len())
    }

    /// Efface l'élément de la coque sous le point : ouverture (et son passage), bloc, mur, ou pièce libre (la pièce
    /// redevient une boîte simple).
    fn design_erase(&mut self, ti: usize, mi: usize, p: V3) -> String {
        let z = self.plan_z();
        let mname = self.proj.ytyps[ti].mlos[mi].name.clone();
        let Some(d) = self.cur_design().cloned() else { return "Rien à effacer.".into() };
        // miroir (à 40 cm près, vue de dessus)
        if let Some(k) = d.mirrors.iter().position(|mr| {
            let c = v3((mr.c[0][0] + mr.c[2][0]) * 0.5, (mr.c[0][1] + mr.c[2][1]) * 0.5, 0.0);
            mr.c[0][2] >= z - 0.1 && mr.c[0][2] < z + self.plan_fh() && v3(c.x - p.x, c.y - p.y, 0.0).len() < 0.6
        }) {
            self.push_undo();
            let mr = d.mirrors[k];
            self.design_remove_mirror_portal(ti, mi, &mr);
            self.design_mut(&mname).mirrors.remove(k);
            return "Miroir effacé.".into();
        }
        // lumière (à 50 cm près, vue de dessus)
        if let Some(li) = d.lights.iter().position(|l| (l.p[2] - z).abs() < 6.0 && l.p[2] >= z - 0.1 && v3(l.p[0] - p.x, l.p[1] - p.y, 0.0).len() < 0.5) {
            self.push_undo();
            self.design_mut(&mname).lights.remove(li);
            return "Lumière effacée.".into();
        }
        let near_wall = |w: &crate::design::DWall| -> Option<(f32, f32)> {
            let (a, b) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
            let len = (b - a).len().max(1e-3);
            let dir = (b - a) * (1.0 / len);
            let s = (v3(p.x, p.y, 0.0) - a).dot(dir).clamp(0.0, len);
            let dist = (a + dir * s - v3(p.x, p.y, 0.0)).len();
            (dist < w.t * 0.5 + 0.3).then_some((dist, s))
        };
        // ouverture
        for (wi, w) in d.walls.iter().enumerate().filter(|(_, w)| (w.z0 - z).abs() < 0.3) {
            if let Some((_, s)) = near_wall(w) {
                if let Some(hi) = w.holes.iter().position(|h| (h.s - s).abs() <= h.w * 0.5 + 0.1) {
                    self.push_undo();
                    let h = w.holes[hi];
                    self.design_remove_hole_portal(ti, mi, w, &h);
                    self.design_mut(&mname).walls[wi].holes.remove(hi);
                    return "Ouverture (et son passage) effacée.".into();
                }
            }
        }
        // bloc
        if let Some(bi) = d.blocks.iter().position(|b| (b.z0 - z).abs() < 0.3 && p.x >= b.a[0].min(b.b[0]) && p.x <= b.a[0].max(b.b[0]) && p.y >= b.a[1].min(b.b[1]) && p.y <= b.a[1].max(b.b[1])) {
            self.push_undo();
            self.design_mut(&mname).blocks.remove(bi);
            return "Bloc effacé.".into();
        }
        // mur
        if let Some(wi) = d.walls.iter().enumerate().filter(|(_, w)| (w.z0 - z).abs() < 0.3).filter_map(|(i, w)| near_wall(w).map(|x| (i, x.0))).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)).map(|x| x.0) {
            self.push_undo();
            let w = d.walls[wi].clone();
            for h in &w.holes {
                self.design_remove_hole_portal(ti, mi, &w, h);
            }
            self.design_mut(&mname).walls.remove(wi);
            return "Mur effacé (avec ses ouvertures).".into();
        }
        // pièce libre
        if let Some(ri) = d.rooms.iter().position(|r| (r.z0 - z).abs() < 0.3 && crate::design::in_poly(&r.pts, p.x, p.y)) {
            self.push_undo();
            let rn = d.rooms[ri].room.clone();
            self.design_mut(&mname).rooms.remove(ri);
            if let Some(r) = self.proj.ytyps[ti].mlos[mi].rooms.iter_mut().find(|r| r.name.eq_ignore_ascii_case(&rn)) {
                r.free = false;
            }
            return "Contour de la pièce effacé : elle redevient une boîte simple (supprime-la dans la liste des pièces si tu n'en veux plus).".into();
        }
        "Rien à effacer ici.".into()
    }

    /// Pièces de l'étage, rectangle en cours et repère sous la souris, dans la vue.
    pub fn plan_draw(&self, painter: &egui::Painter, hover: Option<Pos2>) {
        if self.plan.mode == 0 {
            return;
        }
        let Some((ti, mi, pos, rot)) = self.plan_target() else { return };
        let m = &self.proj.ytyps[ti].mlos[mi];
        let w = |l: V3| self.cam.project(pos + rot.rotate(l)).map(|q| q.0);
        let rect = |a: V3, b: V3, col: Color32, width: f32| {
            let z = a.z;
            let pts: Vec<Pos2> = [v3(a.x, a.y, z), v3(b.x, a.y, z), v3(b.x, b.y, z), v3(a.x, b.y, z), v3(a.x, a.y, z)].iter().filter_map(|p| w(*p)).collect();
            painter.add(egui::Shape::line(pts, egui::Stroke::new(width, col)));
        };
        for i in self.plan_rooms(m) {
            let r = &m.rooms[i];
            rect(r.bb_min, v3(r.bb_max.x, r.bb_max.y, r.bb_min.z), Color32::from_rgb(90, 220, 255), 2.0);
            if let Some(c) = w(v3((r.bb_min.x + r.bb_max.x) * 0.5, (r.bb_min.y + r.bb_max.y) * 0.5, r.bb_min.z)) {
                painter.text(c, egui::Align2::CENTER_CENTER, format!("{i} · {}", r.name), egui::FontId::proportional(12.0), Color32::WHITE);
            }
        }
        // coque modelable de l'étage : murs (deux traits), ouvertures, pièces libres, blocs ; tracé en cours
        if let Some(d) = self.cur_design() {
            let z = self.plan_z();
            for r in d.rooms.iter().filter(|r| (r.z0 - z).abs() < 0.3) {
                let mut pts: Vec<Pos2> = r.pts.iter().filter_map(|p| w(v3(p[0], p[1], z + 0.02))).collect();
                if let Some(f) = pts.first().copied() {
                    pts.push(f);
                }
                painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5_f32, Color32::from_rgb(140, 230, 160))));
            }
            for wl in d.walls.iter().filter(|wl| (wl.z0 - z).abs() < 0.3) {
                let (a, b) = (v3(wl.a[0], wl.a[1], z + 0.05), v3(wl.b[0], wl.b[1], z + 0.05));
                let len = (b - a).len().max(1e-3);
                let dir = (b - a) * (1.0 / len);
                let n = v3(-dir.y, dir.x, 0.0) * (wl.t * 0.5);
                for side in [n, n * -1.0] {
                    let mut cur = 0.0;
                    let mut holes: Vec<(f32, f32)> = wl.holes.iter().map(|h| (h.s - h.w * 0.5, h.s + h.w * 0.5)).collect();
                    holes.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
                    holes.push((len, len));
                    for (h0, h1) in holes {
                        if h0 > cur {
                            if let (Some(p), Some(q)) = (w(a + dir * cur + side), w(a + dir * h0 + side)) {
                                painter.line_segment([p, q], egui::Stroke::new(2.0_f32, Color32::from_rgb(240, 240, 240)));
                            }
                        }
                        cur = cur.max(h1);
                    }
                }
                for h in &wl.holes {
                    if let Some(c) = w(a + dir * h.s) {
                        painter.text(c, egui::Align2::CENTER_CENTER, crate::design::HOLE_KINDS[(h.kind as usize).min(3)].0, egui::FontId::proportional(11.0), Color32::from_rgb(255, 210, 120));
                    }
                }
            }
            for l in d.lights.iter().filter(|l| l.p[2] >= z - 0.1 && l.p[2] < z + self.plan_fh()) {
                if let Some(q) = w(v3(l.p[0], l.p[1], z + 0.05)) {
                    let c = Color32::from_rgb(l.color[0], l.color[1], l.color[2]);
                    painter.circle_filled(q, 6.0, c);
                    painter.circle_stroke(q, 9.0, egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 230, 120)));
                }
            }
            for mr in d.mirrors.iter().filter(|mr| mr.c[0][2] >= z - 0.1 && mr.c[0][2] < z + self.plan_fh()) {
                if let (Some(p), Some(q)) = (w(v3(mr.c[0][0], mr.c[0][1], z + 0.06)), w(v3(mr.c[3][0], mr.c[3][1], z + 0.06))) {
                    painter.line_segment([p, q], egui::Stroke::new(4.0_f32, Color32::from_rgb(150, 210, 255)));
                }
            }
            for bl in d.blocks.iter().filter(|bl| (bl.z0 - z).abs() < 0.3) {
                rect(v3(bl.a[0], bl.a[1], z + 0.05), v3(bl.b[0], bl.b[1], z + 0.05), Color32::from_rgb(200, 160, 255), 1.5);
            }
        }
        if !self.plan.pts.is_empty() {
            let mut pts: Vec<Pos2> = self.plan.pts.iter().filter_map(|p| w(*p)).collect();
            if let Some(q) = hover.and_then(|h| self.design_point(h)).and_then(|p| w(p)) {
                pts.push(q);
            }
            painter.add(egui::Shape::line(pts, egui::Stroke::new(2.5_f32, Color32::from_rgb(255, 190, 90))));
        }
        if let (Some(a), Some(b)) = (self.plan.drag, hover.and_then(|h| self.plan_point(h))) {
            rect(a, b, Color32::from_rgb(255, 190, 90), 2.5);
            if let Some(c) = w((a + b) * 0.5) {
                painter.text(c, egui::Align2::CENTER_CENTER, format!("{:.2} × {:.2} m", (b.x - a.x).abs(), (b.y - a.y).abs()), egui::FontId::proportional(13.0), Color32::from_rgb(255, 210, 120));
            }
        } else if let Some(p) = hover.and_then(|h| self.plan_point(h)) {
            if let Some(q) = w(p) {
                painter.circle_stroke(q, 6.0, egui::Stroke::new(2.0_f32, Color32::from_rgb(255, 190, 90)));
            }
        }
    }

    /// Partie « Plan vu de dessus » de l'onglet Intérieur.
    pub fn plan_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        crate::look::heading(ui, crate::look::ic::GRID_FOUR, "Plan vu de dessus");
        if self.plan_target().is_none() {
            ui.label(RichText::new("Place d'abord l'intérieur dans le projet pour dessiner son plan.").small().weak());
            return;
        }
        if self.plan.floor_h < 2.0 {
            self.plan.floor_h = 3.0;
        }
        ui.horizontal(|ui| {
            ui.label("Étage");
            ui.add(egui::DragValue::new(&mut self.plan.floor).range(0..=30));
            ui.add(egui::Slider::new(&mut self.plan.floor_h, 2.6..=6.0).step_by(0.1).suffix(" m").text("par étage"));
        });
        ui.horizontal_wrapped(|ui| {
            for (m, n, tip) in [
                (1u8, "Tracer une pièce", "Glisser dans la vue : rectangle de la pièce sur le sol de l'étage (grille de 25 cm). Une porte se fait toute seule avec chaque pièce voisine qui partage un mur."),
                (2, "Couper une pièce", "Clic dans une pièce : un mur la coupe en deux à cet endroit (perpendiculaire à son grand côté), avec une porte."),
                (3, "Escalier", "Clic dans une pièce : escalier vers la pièce juste au-dessus (trace-la d'abord), trémie de 3 × 1,2 m le long du grand côté ; marches et collision faites par la coque."),
            ] {
                // dans le studio, ces outils sont dans sa barre du haut
                if self.studio.on {
                    break;
                }
                if ui.selectable_label(self.plan.mode == m, n).on_hover_text(tip).clicked() {
                    self.plan.mode = if self.plan.mode == m { 0 } else { m };
                    self.plan.drag = None;
                }
            }
            let models = self.door_models();
            if !models.is_empty() {
                let cur = if self.plan.door_model.is_empty() { "porte d'intérieur (par défaut)".to_string() } else { self.plan.door_model.clone() };
                egui::ComboBox::from_id_salt("plan_door_model").width(180.0).selected_text(cur).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.plan.door_model, String::new(), "porte d'intérieur (par défaut)");
                    for (n, label) in &models {
                        ui.selectable_value(&mut self.plan.door_model, n.to_string(), format!("{n} — {label}"));
                    }
                });
            }
            if ui.button("Poser les portes").on_hover_text("Une vraie porte du jeu dans chaque passage qui n'en a pas (entre deux pièces, et vers l'extérieur quand il a la taille d'une porte). Elles s'ouvrent en jeu : à l'export, un petit script les enregistre comme portes ouvertes.").clicked() {
                self.status = self.plan_doors();
            }
        });
        // coque modelable
        if self.plan.wall_t <= 0.0 {
            self.plan.wall_t = 0.15;
            self.plan.base = true;
            self.plan.around = true;
        }
        crate::look::heading(ui, crate::look::ic::WALL, "Coque modelable : murs, ouvertures, blocs, lumières");
        ui.horizontal_wrapped(|ui| {
            for (m, n, tip) in [
                (4u8, "Mur", "Clique les points du mur sur le sol de l'étage (aimanté à la grille et aux bouts des autres murs) : chaque clic ajoute un pan. Double-clic ou Entrée : fini · Retour arrière : retirer le dernier pan."),
                (5, "Ouverture", "Clic sur un mur : une porte, une porte double, une fenêtre ou un passage (choisis-le à droite). Le mur est percé et un passage relie les deux pièces (ou la pièce et l'extérieur)."),
                (6, "Pièce libre", "Clique les coins de la pièce, de n'importe quelle forme. Double-clic ou Entrée : la pièce est créée avec son sol, son plafond et des murs autour (case « murs autour »)."),
                (7, "Bloc", "Glisse un rectangle : comptoir, banquette, estrade, muret ou colonne (choisis-le à droite), avec la texture des murs."),
                (9, "Lumière", "Clic sur le sol : une lumière sans objet, rangée dans le modèle de la pièce (spot au plafond, ampoule à 2,2 m ou néon), de la couleur et de l'intensité choisies. Une pièce avec des lumières posées n'a plus de spots automatiques."),
                (10, "Peindre un côté", "Clic près d'un mur, du côté à repeindre : ce côté prend la texture et la couleur choisies (l'autre côté garde la sienne). Papier peint d'un côté, carrelage de l'autre…"),
                (11, "Miroir", "Clic près d'un mur, du côté où il doit être : un miroir de 0,9 × 1,1 m (bas à 1 m du sol), avec son passage « miroir » ; à vérifier en jeu."),
                (8, "Effacer", "Clic : efface le miroir, la lumière, l'ouverture, le bloc, le mur ou le contour de pièce sous la souris."),
            ] {
                // dans le studio, ces outils sont dans sa barre du haut
                if self.studio.on {
                    break;
                }
                if ui.selectable_label(self.plan.mode == m, n).on_hover_text(tip).clicked() {
                    self.plan.mode = if self.plan.mode == m { 0 } else { m };
                    self.plan.pts.clear();
                    self.plan.drag = None;
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::DragValue::new(&mut self.plan.wall_t).range(0.05..=0.6).speed(0.01).suffix(" m")).on_hover_text("Épaisseur des nouveaux murs");
            let customs: Vec<String> = self.proj.custom_textures.iter().map(|c| c.name.clone()).collect();
            let tex_name = |t: u8| -> String {
                if t >= crate::design::CUSTOM_BASE {
                    customs.get((t - crate::design::CUSTOM_BASE) as usize).cloned().map(|n| format!("{n} (importée)")).unwrap_or_else(|| "?".into())
                } else {
                    crate::design::WALL_TEX[(t as usize).min(5)].to_string()
                }
            };
            egui::ComboBox::from_id_salt("design_wall_tex").selected_text(format!("murs : {}", tex_name(self.plan.wall_tex))).show_ui(ui, |ui| {
                for (i, n) in crate::design::WALL_TEX.iter().enumerate() {
                    ui.selectable_value(&mut self.plan.wall_tex, i as u8, *n);
                }
                for (i, n) in customs.iter().enumerate() {
                    ui.selectable_value(&mut self.plan.wall_tex, crate::design::CUSTOM_BASE + i as u8, format!("{n} (importée)"));
                }
            });
            let mut c = self.plan.wall_color.unwrap_or([220, 214, 200]);
            let mut on = self.plan.wall_color.is_some();
            if ui.checkbox(&mut on, "couleur").changed() {
                self.plan.wall_color = on.then_some(c);
            }
            if on && ui.color_edit_button_srgb(&mut c).changed() {
                self.plan.wall_color = Some(c);
            }
            ui.checkbox(&mut self.plan.base, "plinthes");
        });
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("design_hole").selected_text(crate::design::HOLE_KINDS[(self.plan.hole_kind as usize).min(3)].0).show_ui(ui, |ui| {
                for (i, k) in crate::design::HOLE_KINDS.iter().enumerate() {
                    ui.selectable_value(&mut self.plan.hole_kind, i as u8, format!("{} ({:.1} × {:.1} m)", k.0, k.1, k.2));
                }
            });
            egui::ComboBox::from_id_salt("design_block").selected_text(crate::design::BLOCK_KINDS[(self.plan.block_kind as usize).min(4)].0).show_ui(ui, |ui| {
                for (i, k) in crate::design::BLOCK_KINDS.iter().enumerate() {
                    let h = if k.1 <= 0.0 { "jusqu'au plafond".to_string() } else { format!("{:.2} m", k.1) };
                    ui.selectable_value(&mut self.plan.block_kind, i as u8, format!("{} ({h})", k.0));
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            let customs: Vec<String> = self.proj.custom_textures.iter().map(|c| c.name.clone()).collect();
            let fname = if self.plan.floor_tex >= crate::design::CUSTOM_BASE { customs.get((self.plan.floor_tex - crate::design::CUSTOM_BASE) as usize).cloned().map(|n| format!("{n} (importée)")).unwrap_or_else(|| "?".into()) } else { crate::design::FLOOR_TEX[(self.plan.floor_tex as usize).min(3)].to_string() };
            egui::ComboBox::from_id_salt("design_floor").selected_text(format!("sol : {fname}")).show_ui(ui, |ui| {
                for (i, n) in crate::design::FLOOR_TEX.iter().enumerate() {
                    ui.selectable_value(&mut self.plan.floor_tex, i as u8, *n);
                }
                for (i, n) in customs.iter().enumerate() {
                    ui.selectable_value(&mut self.plan.floor_tex, crate::design::CUSTOM_BASE + i as u8, format!("{n} (importée)"));
                }
            });
            ui.add(egui::DragValue::new(&mut self.plan.drop).range(0.0..=1.5).speed(0.05).suffix(" m")).on_hover_text("Faux plafond : descente sous le plafond des nouvelles pièces libres");
            ui.checkbox(&mut self.plan.around, "murs autour");
        });
        // lumières
        if self.plan.light_int <= 0.0 {
            self.plan.light_int = 8.0;
            self.plan.light_range = 6.0;
            self.plan.light_color = [255, 236, 210];
        }
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("design_light").selected_text(crate::design::LIGHT_KINDS[(self.plan.light_kind as usize).min(2)].0).show_ui(ui, |ui| {
                for (i, k) in crate::design::LIGHT_KINDS.iter().enumerate() {
                    ui.selectable_value(&mut self.plan.light_kind, i as u8, k.0);
                }
            });
            ui.color_edit_button_srgb(&mut self.plan.light_color).on_hover_text("Couleur de la lumière");
            ui.add(egui::DragValue::new(&mut self.plan.light_int).range(0.5..=50.0).speed(0.2).prefix("intensité ")).on_hover_text("8 = comme les spots automatiques");
            ui.add(egui::DragValue::new(&mut self.plan.light_range).range(1.0..=30.0).speed(0.2).prefix("portée ").suffix(" m"));
        });
        // textures importées
        ui.horizontal_wrapped(|ui| {
            if ui.button("Importer une image (PNG)…").on_hover_text("Ta propre texture (carrelage, papier peint, logo…) : elle est mise au plus à 512 px de côté, gardée dans le projet et rangée à l'export dans le dictionnaire des coques. Choisis-la ensuite dans « murs » ou « sol ».").clicked() {
                if let Some(paths) = rfd::FileDialog::new().add_filter("Image PNG", &["png"]).set_title("Textures à importer").pick_files() {
                    let mut ok = 0;
                    let mut err = String::new();
                    for p in paths {
                        match crate::design::import_png(&p) {
                            Ok(mut c) => {
                                let base = c.name.clone();
                                let mut k = 2;
                                while self.proj.custom_textures.iter().any(|x| x.name.eq_ignore_ascii_case(&c.name)) {
                                    c.name = format!("{base}_{k}");
                                    k += 1;
                                }
                                self.push_undo();
                                self.proj.custom_textures.push(c);
                                ok += 1;
                            }
                            Err(e) => err = e,
                        }
                    }
                    self.shell_sig = 0;
                    self.status = if err.is_empty() { format!("{ok} texture(s) importée(s) : choisis-les dans « murs » ou « sol ».") } else { format!("{ok} texture(s) importée(s) ; erreur : {err}") };
                }
            }
        });
        let mut del: Option<usize> = None;
        let mut changed = false;
        for (i, c) in self.proj.custom_textures.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&c.name).small());
                changed |= ui.add(egui::DragValue::new(&mut c.size).range(0.1..=20.0).speed(0.05).prefix("motif ").suffix(" m")).on_hover_text("Taille d'une répétition de l'image sur le mur ou le sol").changed();
                if ui.small_button("Retirer").clicked() {
                    del = Some(i);
                }
            });
        }
        if let Some(i) = del {
            self.push_undo();
            self.proj.custom_textures.remove(i);
            // les murs et sols qui s'en servaient prennent la texture par défaut ; les suivantes reculent d'un rang
            let fix = |t: &mut u8| {
                if *t >= crate::design::CUSTOM_BASE {
                    let k = *t - crate::design::CUSTOM_BASE;
                    if k as usize == i {
                        *t = 0;
                    } else if k as usize > i {
                        *t -= 1;
                    }
                }
            };
            for d in self.proj.designs.iter_mut() {
                d.walls.iter_mut().for_each(|w| fix(&mut w.tex));
                d.blocks.iter_mut().for_each(|b| fix(&mut b.tex));
                d.rooms.iter_mut().for_each(|r| fix(&mut r.floor));
            }
            fix(&mut self.plan.wall_tex);
            fix(&mut self.plan.floor_tex);
            changed = true;
        }
        if changed {
            self.shell_sig = 0;
        }
        if let Some((ti, mi, _, _)) = self.plan_target() {
            let n = self.proj.ytyps[ti].mlos[mi].name.clone();
            if let Some(d) = self.proj.designs.iter().find(|d| d.mlo.eq_ignore_ascii_case(&n)) {
                ui.label(RichText::new(format!("{} mur(s), {} ouverture(s), {} pièce(s) libre(s), {} bloc(s), {} lumière(s), {} miroir(s)", d.walls.len(), d.walls.iter().map(|w| w.holes.len()).sum::<usize>(), d.rooms.len(), d.blocks.len(), d.lights.len(), d.mirrors.len())).small());
                let mut baked = d.baked;
                if ui.checkbox(&mut baked, "Éclairage cuit (ombre dans les coins)").on_hover_text("Comme les coques des MLO FiveM : les coins entre murs, le pied et le haut des murs et le bord des blocs sont assombris dans les couleurs des sommets. Visible en jeu (la vue de DragoWalker ne l'affiche pas).").changed() {
                    self.push_undo();
                    self.design_mut(&n).baked = baked;
                }
            }
        }
        if self.plan.mode != 0 {
            ui.label(RichText::new("Échap : finir le tracé en cours, puis quitter le plan.").small().weak());
        }
    }

    /// Portes exportées : chaque vraie porte (model::is_door_model : pas une porte de meuble, pas « outdoor ») posée
    /// dans un intérieur créé dans DragoWalker — pas celles des MLO importés (leur créateur les gère, parfois
    /// verrouillées exprès) ni des intérieurs du jeu —, puis les portes des bâtiments de l'atelier et de leurs copies.
    /// Position et cap dans le monde.
    pub fn door_exports(&self) -> Vec<DoorOut> {
        let mut doors = Vec::new();
        let ours: std::collections::HashSet<String> = self.created_mlos().into_iter().map(|(ti, mi)| self.proj.ytyps[ti].mlos[mi].name.to_lowercase()).collect();
        for inst in self.proj.ymaps.iter().filter(|y| !y.sample).flat_map(|y| y.entities.iter()).filter(|e| e.mlo.is_some() && ours.contains(&e.archetype.to_lowercase())) {
            let Some(m) = self.proj.find_mlo(&inst.archetype) else { continue };
            for e in m.entities.iter().filter(|e| model::is_door_model(&e.archetype)) {
                let pos = inst.pos + inst.rot.rotate(e.pos);
                let heading = inst.rot.mul(e.rot).normalized().heading_deg();
                doors.push(DoorOut { model: e.archetype.clone(), pos, heading, locked: false });
            }
        }
        // portes posées dans l'entrée des bâtiments de l'atelier (objets du monde)
        for h in &self.proj.houses {
            // portes du premier placement, puis celles des copies du bâtiment (dans le même ordre) ; fermées à clé si
            // demandé, toutes pour un bâtiment de décoration
            let locks: Vec<bool> = (0..h.doors().len()).filter(|&i| h.doors()[i].prop.is_some()).map(|i| h.door_locked_at(i)).collect();
            let mut ids: Vec<(u64, bool)> = h.doors().iter().enumerate().filter_map(|(i, d)| d.prop.map(|p| (p, h.door_locked_at(i)))).collect();
            for c in &h.copies {
                ids.extend(c.doors.iter().enumerate().map(|(k, id)| (*id, locks.get(k).copied().unwrap_or(h.decor))));
            }
            for (id, locked) in ids {
                if let Some(e) = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.id == id) {
                    doors.push(DoorOut { model: e.archetype.clone(), pos: e.pos, heading: e.rot.heading_deg(), locked });
                }
            }
        }
        doors
    }

    /// Script des portes (dw_doors.lua) : les portes de door_exports enregistrées dans le système de portes du jeu,
    /// ouvertes ou fermées à clé ; à côté, des modèles de configuration pour les scripts de portes des serveurs
    /// (doorlock/ : ox_doorlock en SQL, qb-doorlock en Lua, et un lisez-moi), pas chargés par le fxmanifest. Rend le
    /// nombre de portes (0 : fichiers retirés).
    pub fn export_doors(&self, out: &std::path::Path) -> Result<usize, String> {
        let doors = self.door_exports();
        let path = out.join("dw_doors.lua");
        let dl = out.join("doorlock");
        if doors.is_empty() {
            let _ = std::fs::remove_file(&path);
            for f in DOORLOCK_FILES {
                let _ = std::fs::remove_file(dl.join(f));
            }
            let _ = std::fs::remove_dir(&dl);
            return Ok(0);
        }
        std::fs::write(&path, dw_doors_lua(&doors)).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dl).map_err(|e| e.to_string())?;
        // nom des portes dans ox_doorlock : « ressource dw_porte_N » (comme la conversion d'ox_doorlock : fichier + porte)
        let res = crate::export::resource_name(&self.proj.name);
        std::fs::write(dl.join(DOORLOCK_FILES[0]), ox_doorlock_sql(&doors, &res)).map_err(|e| e.to_string())?;
        std::fs::write(dl.join(DOORLOCK_FILES[1]), qb_doorlock_lua(&doors)).map_err(|e| e.to_string())?;
        std::fs::write(dl.join(DOORLOCK_FILES[2]), DOORLOCK_README).map_err(|e| e.to_string())?;
        Ok(doors.len())
    }
}

/// Une porte de l'export : modèle, position et cap (degrés, 0..360, GetEntityHeading) dans le monde.
#[derive(Clone, Debug, PartialEq)]
pub struct DoorOut {
    pub model: String,
    pub pos: V3,
    pub heading: f32,
    pub locked: bool,
}

/// Fichiers du dossier doorlock/ de l'export.
pub const DOORLOCK_FILES: [&str; 3] = ["ox_doorlock.sql", "qb-doorlock.lua", "LISEZMOI.txt"];

/// Nom des portes dans les configurations des scripts de portes (numérotées à partir de 1, dans l'ordre de l'export).
pub fn door_id(i: usize) -> String {
    format!("dw_porte_{}", i + 1)
}

/// Hachage signé du modèle (GetHashKey / joaat, comme le stockent ox_doorlock et les natives).
pub fn door_model_hash(name: &str) -> i32 {
    crate::gta::crypto::joaat(name) as i32
}

/// Cap entier entre 0 et 359 (ox_doorlock arrondit le cap à l'entier).
fn heading_int(h: f32) -> i32 {
    (h.round() as i32).rem_euclid(360)
}

/// dw_doors.lua (format inchangé : une ligne par porte).
pub fn dw_doors_lua(doors: &[DoorOut]) -> String {
    let lines: Vec<String> = doors.iter().map(|d| format!("  {{ model = `{}`, x = {:.3}, y = {:.3}, z = {:.3}, locked = {} }},", d.model, d.pos.x, d.pos.y, d.pos.z, d.locked)).collect();
    format!(
        "-- DragoWalker : portes des intérieurs et des bâtiments, enregistrées dans le système de portes du jeu (ouvertes, ou fermées à clé)\nlocal doors = {{\n{}\n}}\n\nCreateThread(function()\n  for i, d in ipairs(doors) do\n    local h = GetHashKey('dw_door_' .. i)\n    AddDoorToSystem(h, d.model, d.x, d.y, d.z, false, false, false)\n    DoorSystemSetDoorState(h, d.locked and 1 or 0, false, false)\n  end\nend)\n",
        lines.join("\n")
    )
}

/// Données JSON d'une porte pour ox_doorlock (colonne `data`), d'après la conversion des configurations qb
/// d'ox_doorlock (server/convert.lua) : model (hachage), coords, heading entier, state 0/1, maxDistance… Les clés
/// laissées à null (doors, groups, items…) : porte simple, sans métier ni objet requis.
pub fn ox_door_json(d: &DoorOut) -> String {
    format!(
        "{{\"model\":{},\"coords\":{{\"x\":{:.3},\"y\":{:.3},\"z\":{:.3}}},\"heading\":{},\"state\":{},\"maxDistance\":2,\"doorRate\":1.0,\"doors\":null,\"autolock\":null,\"groups\":null,\"items\":null,\"characters\":null,\"lockpick\":false,\"hideUi\":false}}",
        door_model_hash(&d.model),
        d.pos.x,
        d.pos.y,
        d.pos.z,
        heading_int(d.heading),
        d.locked as u8
    )
}

/// doorlock/ox_doorlock.sql : une ligne INSERT par porte dans la table `ox_doorlock` (colonnes `name`, `data`),
/// sautée si une porte du même nom existe déjà (comme la conversion d'ox_doorlock).
pub fn ox_doorlock_sql(doors: &[DoorOut], res: &str) -> String {
    let mut s = String::from(
        "-- DragoWalker : portes de l'export pour ox_doorlock (https://github.com/overextended/ox_doorlock)\n\
         -- À importer dans la base de données du serveur (HeidiSQL, phpMyAdmin...), ox_doorlock arrêté.\n\
         -- Format d'après la conversion des portes d'ox_doorlock (server/convert.lua) : à vérifier avec ta version.\n\
         -- Portes sans métier (groups) : ouvre-les avec /doorlock en jeu pour choisir qui peut les ouvrir.\n\n",
    );
    for (i, d) in doors.iter().enumerate() {
        let name = format!("{} {}", res.replace('\'', "''"), door_id(i));
        s += &format!("-- {} : {}\n", door_id(i), d.model);
        s += &format!(
            "INSERT INTO `ox_doorlock` (`name`, `data`) SELECT '{name}', '{}' WHERE NOT EXISTS (SELECT 1 FROM `ox_doorlock` WHERE `name` = '{name}');\n",
            ox_door_json(d).replace('\'', "''")
        );
    }
    s
}

/// Entrée de qb-doorlock (Config.DoorList) pour une porte.
pub fn qb_door_entry(i: usize, d: &DoorOut) -> String {
    format!(
        "Config.DoorList['{}'] = {{\n    objName = '{}',\n    objYaw = {:.1},\n    objCoords = vector3({:.3}, {:.3}, {:.3}),\n    fixText = false,\n    doorType = 'door',\n    doorRate = 1.0,\n    locked = {},\n    distance = 2,\n    authorizedJobs = {{ }},\n}}\n",
        door_id(i),
        d.model,
        heading_int(d.heading) as f32,
        d.pos.x,
        d.pos.y,
        d.pos.z,
        d.locked
    )
}

/// doorlock/qb-doorlock.lua : les portes au format de configuration de qb-doorlock.
pub fn qb_doorlock_lua(doors: &[DoorOut]) -> String {
    let mut s = String::from(
        "-- DragoWalker : portes de l'export pour qb-doorlock (https://github.com/qbcore-framework/qb-doorlock)\n\
         -- À copier dans le dossier configs/ de qb-doorlock (ou à la fin de son config.lua).\n\
         -- Format d'après les portes de la configuration de qb-doorlock : à vérifier avec ta version.\n\
         -- authorizedJobs vide : ajoute les métiers qui peuvent ouvrir, ex. { ['police'] = 0 }.\n\n",
    );
    for (i, d) in doors.iter().enumerate() {
        s += &format!("-- {}\n", d.model);
        s += &qb_door_entry(i, d);
        s += "\n";
    }
    s
}

/// doorlock/LISEZMOI.txt
pub const DOORLOCK_README: &str = "Portes de la carte pour les scripts de portes des serveurs
==========================================================

DragoWalker a écrit ici les portes de tes intérieurs et des bâtiments de l'atelier, avec leur position, leur cap et
leur état (fermée à clé ou non). Le script dw_doors.lua de la ressource suffit pour les ouvrir en jeu ; ces fichiers
servent si ton serveur gère les portes avec un script de portes (fermer à clé, choisir qui peut ouvrir). Ils ne sont
pas chargés par cette ressource : copie celui de ton script.

ox_doorlock
  1. Arrête ox_doorlock (ou le serveur).
  2. Importe ox_doorlock.sql dans la base de données du serveur (HeidiSQL, phpMyAdmin... : « Exécuter un fichier
     SQL »). Une porte déjà enregistrée sous le même nom n'est pas ajoutée deux fois.
  3. Redémarre ox_doorlock. En jeu, /doorlock permet de régler chaque porte (métiers, code, son...).

qb-doorlock
  1. Copie qb-doorlock.lua dans le dossier configs/ de qb-doorlock (ou colle son contenu à la fin de config.lua).
  2. Ajoute dans authorizedJobs les métiers qui peuvent ouvrir chaque porte, ex. { ['police'] = 0 }.
  3. Redémarre qb-doorlock.

Garde un seul système pour une même porte : si tu utilises ox_doorlock ou qb-doorlock, tu peux retirer la ligne
client_script 'dw_doors.lua' du fxmanifest.lua de la ressource (sinon les deux scripts règlent la même porte).

Les portes s'appellent dw_porte_1, dw_porte_2... dans l'ordre de l'export : si tu ajoutes ou retires des portes dans
DragoWalker, les numéros peuvent changer. Le format de ces fichiers est fait d'après les sources publiques des deux
scripts : vérifie-le avec ta version.
";

#[cfg(test)]
mod door_tests {
    use super::*;

    fn door(model: &str, locked: bool, heading: f32) -> DoorOut {
        DoorOut { model: model.into(), pos: v3(10.0, -20.5, 30.25), heading, locked }
    }

    #[test]
    fn numerotation_et_formats() {
        let d = vec![door("v_ilev_fh_door", false, 90.4), door("prop_com_ls_door_01", true, 359.7)];
        assert_eq!(door_id(0), "dw_porte_1");
        let qb = qb_doorlock_lua(&d);
        assert!(qb.contains("Config.DoorList['dw_porte_1'] = {"));
        assert!(qb.contains("Config.DoorList['dw_porte_2'] = {"));
        assert!(qb.contains("objName = 'prop_com_ls_door_01',"));
        assert!(qb.contains("objCoords = vector3(10.000, -20.500, 30.250),"));
        assert!(qb.contains("objYaw = 90.0,"));
        // 359,7° arrondi : 0
        assert!(qb.contains("objYaw = 0.0,"));
        assert_eq!(qb.matches("locked = true,").count(), 1);
        let sql = ox_doorlock_sql(&d, "ma'carte");
        assert_eq!(sql.matches("INSERT INTO `ox_doorlock` (`name`, `data`)").count(), 2);
        assert!(sql.contains("SELECT 'ma''carte dw_porte_2', '{\"model\":"));
        let j = ox_door_json(&d[1]);
        assert!(j.contains("\"state\":1"));
        assert!(j.contains("\"heading\":0,"));
        assert!(j.contains("\"coords\":{\"x\":10.000,\"y\":-20.500,\"z\":30.250}"));
        assert!(ox_door_json(&d[0]).contains("\"state\":0"));
        let lua = dw_doors_lua(&d);
        assert!(lua.contains("{ model = `v_ilev_fh_door`, x = 10.000, y = -20.500, z = 30.250, locked = false },"));
    }

    #[test]
    fn hachage_signe() {
        // GetHashKey("prop_bench_01a") : nombre signé sur 32 bits
        let h = door_model_hash("PROP_bench_01a");
        assert_eq!(h, crate::gta::crypto::joaat("prop_bench_01a") as i32);
        assert_eq!(door_model_hash("a") as u32, crate::gta::crypto::joaat("a"));
    }
}

/// Porte entre deux pièces qui partagent un mur (au moins DOOR_W + 0,4 m en commun) : au milieu du mur commun.
pub fn shared_door(a: &model::Room, b: &model::Room, z0: f32) -> Option<[V3; 4]> {
    let eps = 0.05;
    let span = |a0: f32, a1: f32, b0: f32, b1: f32| (a0.max(b0), a1.min(b1));
    let door = |fixed_x: bool, at: f32, lo: f32, hi: f32| -> Option<[V3; 4]> {
        if hi - lo < DOOR_W + 0.4 {
            return None;
        }
        let c = (lo + hi) * 0.5;
        let (s0, s1) = (c - DOOR_W * 0.5, c + DOOR_W * 0.5);
        let pt = |s: f32, z: f32| if fixed_x { v3(at, s, z) } else { v3(s, at, z) };
        Some([pt(s0, z0), pt(s0, z0 + DOOR_H), pt(s1, z0 + DOOR_H), pt(s1, z0)])
    };
    if (a.bb_max.x - b.bb_min.x).abs() < eps || (b.bb_max.x - a.bb_min.x).abs() < eps {
        let at = if (a.bb_max.x - b.bb_min.x).abs() < eps { a.bb_max.x } else { a.bb_min.x };
        let (lo, hi) = span(a.bb_min.y, a.bb_max.y, b.bb_min.y, b.bb_max.y);
        return door(true, at, lo, hi);
    }
    if (a.bb_max.y - b.bb_min.y).abs() < eps || (b.bb_max.y - a.bb_min.y).abs() < eps {
        let at = if (a.bb_max.y - b.bb_min.y).abs() < eps { a.bb_max.y } else { a.bb_min.y };
        let (lo, hi) = span(a.bb_min.x, a.bb_max.x, b.bb_min.x, b.bb_max.x);
        return door(false, at, lo, hi);
    }
    None
}
