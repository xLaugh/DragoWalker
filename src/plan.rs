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
            self.plan.mode = 0;
            self.plan.drag = None;
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
        ui.label(RichText::new("Plan vu de dessus").strong());
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
        if self.plan.mode != 0 {
            ui.label(RichText::new("Échap : quitter le plan.").small().weak());
        }
    }

    /// Script des portes des intérieurs (dw_doors.lua) : chaque porte du jeu posée dans un intérieur du projet est
    /// enregistrée dans le système de portes et déverrouillée. Rend le nombre de portes.
    pub fn export_doors(&self, out: &std::path::Path) -> Result<usize, String> {
        let mut lines = Vec::new();
        for inst in self.proj.ymaps.iter().filter(|y| !y.sample).flat_map(|y| y.entities.iter()).filter(|e| e.mlo.is_some()) {
            let Some(m) = self.proj.find_mlo(&inst.archetype) else { continue };
            for e in m.entities.iter().filter(|e| e.archetype.to_lowercase().contains("door")) {
                let p = inst.pos + inst.rot.rotate(e.pos);
                lines.push(format!("  {{ model = `{}`, x = {:.3}, y = {:.3}, z = {:.3} }},", e.archetype, p.x, p.y, p.z));
            }
        }
        // portes posées dans l'entrée des bâtiments de l'atelier (objets du monde)
        for h in &self.proj.houses {
            for d in h.doors() {
                let Some(id) = d.prop else { continue };
                if let Some(e) = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.id == id) {
                    lines.push(format!("  {{ model = `{}`, x = {:.3}, y = {:.3}, z = {:.3} }},", e.archetype, e.pos.x, e.pos.y, e.pos.z));
                }
            }
        }
        let path = out.join("dw_doors.lua");
        if lines.is_empty() {
            let _ = std::fs::remove_file(&path);
            return Ok(0);
        }
        let s = format!(
            "-- DragoWalker : portes des intérieurs, enregistrées dans le système de portes du jeu et ouvertes\nlocal doors = {{\n{}\n}}\n\nCreateThread(function()\n  for i, d in ipairs(doors) do\n    local h = GetHashKey('dw_door_' .. i)\n    AddDoorToSystem(h, d.model, d.x, d.y, d.z, false, false, false)\n    DoorSystemSetDoorState(h, 0, false, false)\n  end\nend)\n",
            lines.join("\n")
        );
        std::fs::write(&path, s).map_err(|e| e.to_string())?;
        Ok(lines.len())
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
