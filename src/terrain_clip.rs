//! Copier-coller d'un morceau de terrain : relief, peinture du sol, herbe du jeu, routes entièrement dans la zone
//! et objets du projet posés dessus. Le morceau collé prend la hauteur moyenne du bord de l'endroit où il est
//! posé, avec un raccord en douceur sur 3 cases ; il peut être tourné d'un quart de tour. Les routes collées sont
//! reliées aux routes voisines (réseau refait), leurs feux et lampadaires reposés.

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use crate::model::Entity;
use eframe::egui::{self, Color32, PointerButton, Pos2};

/// Raccord du relief collé avec le terrain autour (cases) — **estimé**.
const BLEND_CELLS: f32 = 3.0;

/// Morceau copié (en mémoire, le temps de la session).
#[derive(Clone)]
pub struct Clip {
    /// sommets dans chaque direction
    pub ni: usize,
    pub nj: usize,
    pub cell: f32,
    /// hauteurs relatives à la moyenne du bord
    pub h: Vec<f32>,
    pub mat: Vec<u8>,
    pub grass: Vec<u8>,
    /// routes et objets, positions relatives au coin bas-gauche (hauteur des objets relative à la moyenne du bord)
    pub roads: Vec<crate::roads::Road>,
    pub ents: Vec<Entity>,
}

impl Clip {
    fn size(&self) -> (f32, f32) {
        ((self.ni - 1) as f32 * self.cell, (self.nj - 1) as f32 * self.cell)
    }

    /// Le même morceau tourné d'un quart de tour (sens inverse des aiguilles d'une montre).
    fn rotated(&self) -> Clip {
        let (ni, nj) = (self.ni, self.nj);
        // sommets : nouveau (i, j) = ancien (a = j, b = nj - 1 - i), dimensions échangées
        let mut h = vec![0.0; ni * nj];
        for j in 0..ni {
            for i in 0..nj {
                h[j * nj + i] = self.h[(nj - 1 - i) * ni + j];
            }
        }
        let (ci, cj) = (ni - 1, nj - 1);
        let rot_cells = |src: &Vec<u8>| -> Vec<u8> {
            if src.len() != ci * cj {
                return Vec::new();
            }
            let mut out = vec![0u8; ci * cj];
            for j in 0..ci {
                for i in 0..cj {
                    out[j * cj + i] = src[(cj - 1 - i) * ci + j];
                }
            }
            out
        };
        let (w, d) = self.size();
        // point (x, y) → (d - y, x) : même rotation, ramenée dans le nouveau rectangle (d × w)
        let rp = |x: f32, y: f32| (d - y, x);
        let mut roads = self.roads.clone();
        for r in roads.iter_mut() {
            for p in r.pts.iter_mut() {
                let (x, y) = rp(p[0], p[1]);
                *p = [x, y];
            }
        }
        let mut ents = self.ents.clone();
        for e in ents.iter_mut() {
            let (x, y) = rp(e.pos.x, e.pos.y);
            e.pos = v3(x, y, e.pos.z);
            e.rot = Quat::from_z(std::f32::consts::FRAC_PI_2).mul(e.rot).normalized();
        }
        let _ = w;
        Clip { ni: nj, nj: ni, cell: self.cell, h, mat: rot_cells(&self.mat), grass: rot_cells(&self.grass), roads, ents }
    }
}

impl DwApp {
    /// Outil « Copier une zone » / « Coller » du créateur de terrain.
    pub fn clip_tool(&mut self, resp: &egui::Response, g: Option<V3>) {
        if !resp.clicked_by(PointerButton::Primary) {
            return;
        }
        let Some(p) = g else { return };
        if self.terrain.clip_mode == 1 {
            match self.terrain.clip_a.take() {
                None => {
                    self.terrain.clip_a = Some(p);
                    self.status = "Copier : premier coin posé, clique le coin opposé.".into();
                }
                Some(a) => {
                    self.status = self.clip_copy(a, p);
                    self.terrain.clip_mode = 0;
                }
            }
        } else if self.terrain.clip_mode == 2 {
            self.status = self.clip_paste(p);
        }
    }

    /// Copie le rectangle de coins `a` et `b`.
    pub fn clip_copy(&mut self, a: V3, b: V3) -> String {
        let Some(t) = self.proj.terrain.as_ref() else { return String::new() };
        let to_i = |x: f32| ((x - t.origin.x) / t.cell).round().clamp(0.0, (t.nx - 1) as f32) as usize;
        let to_j = |y: f32| ((y - t.origin.y) / t.cell).round().clamp(0.0, (t.ny - 1) as f32) as usize;
        let (i0, i1) = (to_i(a.x.min(b.x)), to_i(a.x.max(b.x)));
        let (j0, j1) = (to_j(a.y.min(b.y)), to_j(a.y.max(b.y)));
        if i1 < i0 + 2 || j1 < j0 + 2 {
            return "Zone trop petite : au moins 3 cases de côté.".into();
        }
        let (ni, nj) = (i1 - i0 + 1, j1 - j0 + 1);
        let mut border = Vec::new();
        for j in j0..=j1 {
            for i in i0..=i1 {
                if i == i0 || i == i1 || j == j0 || j == j1 {
                    border.push(t.h[j * t.nx + i]);
                }
            }
        }
        let base = border.iter().sum::<f32>() / border.len().max(1) as f32;
        let mut h = Vec::with_capacity(ni * nj);
        for j in j0..=j1 {
            for i in i0..=i1 {
                h.push(t.h[j * t.nx + i] - base);
            }
        }
        let (mut mat, mut grass) = (Vec::new(), Vec::new());
        for j in j0..j1 {
            for i in i0..i1 {
                mat.push(t.mat[j * (t.nx - 1) + i]);
                grass.push(t.grass_at(i, j));
            }
        }
        let corner = v3(t.origin.x + i0 as f32 * t.cell, t.origin.y + j0 as f32 * t.cell, 0.0);
        let (w, d) = ((ni - 1) as f32 * t.cell, (nj - 1) as f32 * t.cell);
        let inside = |x: f32, y: f32| x >= corner.x && y >= corner.y && x <= corner.x + w && y <= corner.y + d;
        let mut roads = Vec::new();
        for r in &t.roads {
            if !r.samples.is_empty() && r.samples.iter().all(|s| inside(s[0], s[1])) {
                let mut c = r.clone();
                for p in c.pts.iter_mut() {
                    *p = [p[0] - corner.x, p[1] - corner.y];
                }
                c.props.clear();
                roads.push(c);
            }
        }
        // objets du projet sur la zone, sauf ceux que les routes reposent elles-mêmes (feux, panneaux, lampadaires)
        let auto: std::collections::HashSet<u64> = t.junctions.iter().flat_map(|j| j.props.iter().copied()).chain(t.roads.iter().flat_map(|r| r.props.iter().copied())).collect();
        let mut ents = Vec::new();
        for y in &self.proj.ymaps {
            for e in &y.entities {
                if !auto.contains(&e.id) && e.mlo.is_none() && inside(e.pos.x, e.pos.y) {
                    let mut c = e.clone();
                    c.pos = v3(e.pos.x - corner.x, e.pos.y - corner.y, e.pos.z - base);
                    ents.push(c);
                }
            }
        }
        let msg = format!("Zone de {w:.0} × {d:.0} m copiée : relief, sol, {} route(s), {} objet(s). Choisis « Coller » puis clique où la poser.", roads.len(), ents.len());
        self.terrain.clip = Some(Clip { ni, nj, cell: t.cell, h, mat, grass, roads, ents });
        self.terrain.clip_rot = 0;
        msg
    }

    /// Morceau copié, tourné comme demandé.
    fn clip_turned(&self) -> Option<Clip> {
        let mut c = self.terrain.clip.clone()?;
        for _ in 0..self.terrain.clip_rot % 4 {
            c = c.rotated();
        }
        Some(c)
    }

    /// Colle le morceau copié centré en `p`.
    pub fn clip_paste(&mut self, p: V3) -> String {
        let Some(c) = self.clip_turned() else { return "Rien à coller : copie d'abord une zone.".into() };
        let Some(t) = self.proj.terrain.as_ref() else { return String::new() };
        if (c.cell - t.cell).abs() > 1e-3 {
            return "Le morceau copié vient d'un terrain aux cases d'une autre taille.".into();
        }
        let i0 = ((p.x - t.origin.x) / t.cell - (c.ni - 1) as f32 * 0.5).round() as i64;
        let j0 = ((p.y - t.origin.y) / t.cell - (c.nj - 1) as f32 * 0.5).round() as i64;
        if i0 < 0 || j0 < 0 || i0 as usize + c.ni > t.nx || j0 as usize + c.nj > t.ny {
            return "Le morceau dépasserait du terrain : colle-le plus loin du bord.".into();
        }
        let (i0, j0) = (i0 as usize, j0 as usize);
        let mut border = Vec::new();
        for j in 0..c.nj {
            for i in 0..c.ni {
                if i == 0 || j == 0 || i == c.ni - 1 || j == c.nj - 1 {
                    border.push(t.h[(j0 + j) * t.nx + i0 + i]);
                }
            }
        }
        let base = border.iter().sum::<f32>() / border.len().max(1) as f32;
        let corner = v3(t.origin.x + i0 as f32 * t.cell, t.origin.y + j0 as f32 * t.cell, 0.0);
        self.push_undo();
        let mut new_roads = Vec::new();
        if let Some(t) = self.proj.terrain.as_mut() {
            let (nx, ny) = (t.nx, t.ny);
            {
                let hm = std::sync::Arc::make_mut(&mut t.h);
                for j in 0..c.nj {
                    for i in 0..c.ni {
                        let e = (i.min(c.ni - 1 - i).min(j).min(c.nj - 1 - j)) as f32;
                        let w = (e / BLEND_CELLS).min(1.0);
                        let w = w * w * (3.0 - 2.0 * w);
                        let k = (j0 + j) * nx + i0 + i;
                        hm[k] += (c.h[j * c.ni + i] + base - hm[k]) * w;
                    }
                }
            }
            if c.mat.len() == (c.ni - 1) * (c.nj - 1) {
                let mm = std::sync::Arc::make_mut(&mut t.mat);
                for j in 0..c.nj - 1 {
                    for i in 0..c.ni - 1 {
                        mm[(j0 + j) * (nx - 1) + i0 + i] = c.mat[j * (c.ni - 1) + i];
                    }
                }
            }
            if c.grass.iter().any(|g| *g != 0) {
                if t.grass.len() != (nx - 1) * (ny - 1) {
                    t.grass = std::sync::Arc::new(vec![0; (nx - 1) * (ny - 1)]);
                }
                let gm = std::sync::Arc::make_mut(&mut t.grass);
                for j in 0..c.nj - 1 {
                    for i in 0..c.ni - 1 {
                        gm[(j0 + j) * (nx - 1) + i0 + i] = c.grass[j * (c.ni - 1) + i];
                    }
                }
            }
            for r in &c.roads {
                let mut r = r.clone();
                for p in r.pts.iter_mut() {
                    *p = [p[0] + corner.x, p[1] + corner.y];
                }
                r.props.clear();
                t.roads.push(r);
                new_roads.push(t.roads.len() - 1);
            }
        }
        let yi = self.ensure_target_ymap();
        let mut ne = 0;
        for e in &c.ents {
            let mut n = e.clone();
            n.id = crate::model::new_id();
            n.pos = v3(e.pos.x + corner.x, e.pos.y + corner.y, e.pos.z + base);
            self.proj.ymaps[yi].entities.push(n);
            ne += 1;
        }
        if !new_roads.is_empty() {
            self.rebuild_road_network(&new_roads);
        }
        self.sync_game_project();
        self.terrain.dirty = true;
        let (w, d) = c.size();
        format!("Morceau de {w:.0} × {d:.0} m collé : {} route(s) reliée(s) aux autres, {ne} objet(s). Clique ailleurs pour coller encore, Échap pour finir.", new_roads.len())
    }

    /// Cadre de la zone à copier, ou du morceau à coller, sous la souris.
    pub fn draw_clip(&self, painter: &egui::Painter, c: V3) {
        let Some(t) = self.proj.terrain.as_ref() else { return };
        let rect = |a: V3, b: V3| -> Vec<Pos2> {
            let mut out = Vec::new();
            let pts = [(a.x, a.y), (b.x, a.y), (b.x, b.y), (a.x, b.y), (a.x, a.y)];
            for w in pts.windows(2) {
                for k in 0..=16 {
                    let f = k as f32 / 16.0;
                    let (x, y) = (w[0].0 + (w[1].0 - w[0].0) * f, w[0].1 + (w[1].1 - w[0].1) * f);
                    if let Some((q, _)) = self.cam.project(v3(x, y, t.height_at(x, y).unwrap_or(c.z) + 0.4)) {
                        out.push(q);
                    }
                }
            }
            out
        };
        let col = Color32::from_rgb(90, 220, 255);
        if self.terrain.clip_mode == 1 {
            if let Some(a) = self.terrain.clip_a {
                painter.add(egui::Shape::line(rect(a, c), egui::Stroke::new(2.0_f32, col)));
            } else {
                painter.circle_stroke(self.cam.project(c).map(|q| q.0).unwrap_or_default(), 8.0, egui::Stroke::new(2.0_f32, col));
            }
        } else if let Some(cl) = self.clip_turned() {
            let (w, d) = cl.size();
            painter.add(egui::Shape::line(rect(c - v3(w * 0.5, d * 0.5, 0.0), c + v3(w * 0.5, d * 0.5, 0.0)), egui::Stroke::new(2.0_f32, Color32::from_rgb(255, 190, 90))));
        }
    }

    /// Boutons du copier-coller (partie Sculpter).
    pub fn clip_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label(egui::RichText::new("Copier-coller une zone").strong());
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(self.terrain.clip_mode == 1, "Copier une zone").on_hover_text("Clique deux coins opposés : relief, peinture du sol, herbe, routes entièrement dans la zone et objets du projet posés dessus.").clicked() {
                self.terrain.clip_mode = 1;
                self.terrain.clip_a = None;
                self.set_terrain_brush(crate::terrain::Brush::Clip);
            }
            let has = self.terrain.clip.is_some();
            if ui.add_enabled(has, egui::SelectableLabel::new(self.terrain.clip_mode == 2, "Coller")).on_hover_text("Clique où poser le centre du morceau (autant de fois que tu veux). Il prend la hauteur moyenne de l'endroit, avec un raccord en douceur ; ses routes se relient aux routes voisines.").clicked() {
                self.terrain.clip_mode = 2;
                self.set_terrain_brush(crate::terrain::Brush::Clip);
            }
            if ui.add_enabled(has, egui::Button::new("Tourner d'un quart")).clicked() {
                self.terrain.clip_rot = (self.terrain.clip_rot + 1) % 4;
            }
        });
        if let Some(c) = &self.terrain.clip {
            ui.label(egui::RichText::new(format!("Copié : {:.0} × {:.0} m, {} route(s), {} objet(s), tourné de {}°", (c.ni - 1) as f32 * c.cell, (c.nj - 1) as f32 * c.cell, c.roads.len(), c.ents.len(), self.terrain.clip_rot as u32 * 90)).small());
        }
    }
}
