//! « Poser au sol » : le bas de l'objet (sa boîte) est posé sur ce qu'il y a juste dessous — terrain du
//! créateur, objets du projet, modèles du jeu (vrais triangles), sinon le relief du jeu.
//! Touche Maj seule (appuyer puis relâcher), touche Fin, ou Maj maintenue pendant qu'on déplace / pose.

use crate::app::DwApp;
use crate::math::{v3, V3};
use std::collections::HashSet;
use std::sync::Arc;

/// Départ du rayon au-dessus du point, longueur, remontée au second essai (WorldForm::GetGroundPoint
/// 0x0600027c : ldc.r4 0.1, 20, 3).
pub const GROUND_RAY_START: f32 = 0.1;
pub const GROUND_RAY_LEN: f32 = 20.0;
pub const GROUND_RAY_RETRY_UP: f32 = 3.0;

/// Rayon / triangle (Möller–Trumbore) : distance le long de `d` (non normalisé), si touché.
fn ray_tri(o: V3, d: V3, a: V3, b: V3, c: V3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    if t >= 0.0 { Some(t) } else { None }
}

impl DwApp {
    /// Modèle d'un type d'objet du jeu pour le lancer de rayon (gardé en mémoire).
    fn snap_mesh(&mut self, h: u32) -> Option<Arc<crate::gta::drawable::Mesh>> {
        if let Some(m) = self.snap_cache.get(&h) {
            return m.clone();
        }
        let m = self.game.world.as_ref().and_then(|w| w.load_model(h, 40000)).map(Arc::new);
        if self.snap_cache.len() > 400 {
            self.snap_cache.clear();
        }
        self.snap_cache.insert(h, m.clone());
        m
    }

    /// Plus haut point de sol sous (x, y) en partant de `z_from` vers le bas, sans les objets de `skip`.
    /// `range` : longueur du rayon (au plus `range` m sous `z_from`).
    pub fn ground_below(&mut self, x: f32, y: f32, z_from: f32, range: f32, skip: &HashSet<u64>) -> Option<f32> {
        let o = v3(x, y, z_from);
        let down = v3(0.0, 0.0, -1.0);
        let mut best: Option<f32> = None;
        let mut take = |z: f32, best: &mut Option<f32>| {
            if z <= z_from + 1e-3 && z >= z_from - range && best.map(|b| z > b).unwrap_or(true) {
                *best = Some(z);
            }
        };
        if let Some(t) = self.proj.terrain.as_ref() {
            if let Some(h) = t.height_at(x, y) {
                take(h, &mut best);
            }
            for r in &t.roads {
                if let Some((d, z)) = r.nearest(x, y) {
                    if d <= r.kind().width * 0.5 {
                        take(z + 0.08, &mut best);
                    }
                }
            }
        }
        // objets : du projet, puis du jeu (ceux affichés), triangle par triangle
        let mut cands: Vec<(u32, V3, crate::math::Quat, V3)> = Vec::new();
        for it in self.proj.all_items() {
            let Some(e) = self.proj.world_entity(it) else { continue };
            if skip.contains(&e.id) || self.sample_hidden(it) {
                continue;
            }
            let (mn, mx) = e.local_box(&self.proj);
            let r = ((mx.x - mn.x).powi(2) + (mx.y - mn.y).powi(2)).sqrt() * 0.5 * e.scale_xy + 1.0;
            if (e.pos.x - x).powi(2) + (e.pos.y - y).powi(2) > r * r {
                continue;
            }
            cands.push((crate::render::arch_hash(&e.archetype), e.pos, e.rot, v3(e.scale_xy, e.scale_xy, e.scale_z)));
        }
        if let Some(w) = self.game.world.as_ref() {
            for &ei in &self.game.drawn {
                let e = &w.ents[ei as usize];
                if w.ymaps[e.ymap as usize].in_project || self.game.hidden.contains(&ei) || e.lod_level != 0 {
                    continue;
                }
                let (mn, mx) = w.arch_box(e.arch);
                let r = ((mx.x - mn.x).powi(2) + (mx.y - mn.y).powi(2)).sqrt() * 0.5 * e.sxy + 1.0;
                if (e.pos.x - x).powi(2) + (e.pos.y - y).powi(2) > r * r || e.pos.z + mn.z * e.sz > z_from {
                    continue;
                }
                cands.push((e.arch, e.pos, e.rot, v3(e.sxy, e.sxy, e.sz)));
            }
        }
        for (h, pos, rot, s) in cands {
            let Some(m) = self.snap_mesh(h) else { continue };
            // rayon dans le repère de l'objet (même paramètre t)
            let inv = rot.inverse();
            let lo = inv.rotate(o - pos);
            let lo = v3(lo.x / s.x.max(1e-3), lo.y / s.y.max(1e-3), lo.z / s.z.max(1e-3));
            let ld = inv.rotate(down);
            let ld = v3(ld.x / s.x.max(1e-3), ld.y / s.y.max(1e-3), ld.z / s.z.max(1e-3));
            for t in &m.tris {
                let p = |i: u32| {
                    let q = m.pos[i as usize];
                    v3(q[0], q[1], q[2])
                };
                if let Some(tt) = ray_tri(lo, ld, p(t[0]), p(t[1]), p(t[2])) {
                    take(z_from - tt, &mut best);
                }
            }
        }
        if best.is_none() {
            if let Some(w) = self.game.world.as_ref() {
                if let Some(g) = w.ray_ground(o, down) {
                    take(g.z, &mut best);
                }
            }
        }
        best
    }

    /// Pose chaque objet sélectionné sur le sol juste en dessous (le bas de sa boîte touche le sol).
    pub fn snap_selected_to_ground(&mut self, undo: bool) -> usize {
        if self.selected.is_empty() {
            return 0;
        }
        if undo {
            self.push_undo();
        }
        let ids = self.selected.clone();
        self.snap_ids(&ids)
    }

    /// Avec Maj maintenue : pose au sol les objets qu'on vient de poser.
    pub fn snap_if_shift(&mut self, ids: &[u64]) {
        if self.shift_since.is_some() {
            self.snap_ids(ids);
        }
    }

    /// Point de contact avec le sol, sous l'origine de l'objet (valeur négative = sous l'origine).
    /// Prop : le bas de sa boîte. Morceau de carte du jeu (bâtiment…) : la hauteur à laquelle le jeu le pose
    /// au-dessus du sol à son emplacement d'origine (relief du jeu, bas de case), car beaucoup ont des
    /// fondations sous terre et le bas de leur boîte n'est pas le niveau de la rue.
    fn contact_offset(&mut self, e: &crate::model::Entity) -> f32 {
        let (mn, _) = e.local_box(&self.proj);
        let boxed = mn.z * e.scale_z;
        if crate::buildings::is_prop_name(&e.archetype.to_lowercase()) || e.mlo.is_some() {
            return boxed;
        }
        let h = crate::render::arch_hash(&e.archetype);
        if let Some(o) = self.snap_offsets.get(&h) {
            return o.map(|o| o * e.scale_z).unwrap_or(boxed);
        }
        let off = self.game.world.as_ref().and_then(|w| {
            let orig = w.ents.iter().find(|x| x.arch == h && x.lod_level == 0)?;
            let g = w.ground_min(orig.pos.x, orig.pos.y)?;
            // au plus 1 m sous l'origine (un bâtiment posé sur une butte), sinon le bas de la boîte
            Some((g - orig.pos.z).clamp(mn.z, 1.0))
        });
        self.snap_offsets.insert(h, off);
        off.map(|o| o * e.scale_z).unwrap_or(boxed)
    }

    pub fn snap_ids(&mut self, ids: &[u64]) -> usize {
        let skip: HashSet<u64> = ids.iter().copied().collect();
        let mut n = 0;
        for &id in ids {
            let Some(r) = self.proj.find_id(id) else { continue };
            let Some(e) = self.proj.world_entity(r) else { continue };
            let bottom = self.contact_offset(&e);
            // comme WorldForm::GetGroundPoint (0x0600027c) : rayon vers le bas depuis 0,1 m au-dessus du point, sur
            // 20 m ; sans résultat, de nouveau 3 m plus haut ; sinon l'objet ne bouge pas. DragoWalker ajoute deux
            // essais de plus haut (30 m, 300 m) pour un objet enterré, et part du point de contact (bas de l'objet)
            let c = e.pos.z + bottom;
            let found = [(GROUND_RAY_START, GROUND_RAY_LEN), (GROUND_RAY_START + GROUND_RAY_RETRY_UP, GROUND_RAY_LEN), (30.0, 60.0), (300.0, 600.0)]
                .iter()
                .find_map(|(up, len)| self.ground_below(e.pos.x, e.pos.y, c + up, *len, &skip));
            if let Some(g) = found {
                self.proj.set_world_pos(r, v3(e.pos.x, e.pos.y, g - bottom));
                n += 1;
            }
        }
        if n > 0 {
            self.sync_game_project();
        }
        n
    }
}
