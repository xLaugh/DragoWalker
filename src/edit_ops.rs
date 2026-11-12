//! Opérations d'édition de DwApp : annulation, sélection, pose, duplication, déplacement, création d'intérieurs.
//! (Déplacé de app.rs sans modification.)

use crate::app::*;
use crate::math::{v3, Quat, V3};
use crate::mech;
use crate::model::{self, Entity, ItemRef, MloInstData, Ymap, Ytyp};
use std::collections::HashSet;

impl DwApp {
    // ------------------------------------------------------------ outils communs

    pub fn push_undo(&mut self) {
        self.undo.push(self.proj.clone());
        self.redo.clear();
        self.safe.touch();
        crate::safety::trim_undo(&mut self.undo, &self.proj);
    }

    pub fn do_undo(&mut self) {
        if let Some(p) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.proj, p));
            self.safe.touch();
            self.terrain.dirty = true;
            self.clean_selection();
            self.sync_game_project();
            self.status = "Annulé.".into();
        }
    }

    pub fn do_redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.proj, p));
            self.safe.touch();
            self.terrain.dirty = true;
            self.clean_selection();
            self.sync_game_project();
            self.status = "Rétabli.".into();
        }
    }

    pub fn clean_selection(&mut self) {
        let proj = &self.proj;
        self.selected.retain(|id| proj.find_id(*id).is_some());
        self.sel_set = self.selected.iter().copied().collect();
        if let Some((t, m)) = self.cur_mlo {
            if self.proj.ytyps.get(t).and_then(|y| y.mlos.get(m)).is_none() {
                self.cur_mlo = None;
            }
        }
        if self.target_ymap >= self.proj.ymaps.len() {
            self.target_ymap = 0;
        }
    }

    pub fn set_selection(&mut self, ids: Vec<u64>) {
        self.selected = ids;
        self.sel_set = self.selected.iter().copied().collect();
    }

    pub fn add_selection(&mut self, ids: impl IntoIterator<Item = u64>) {
        for id in ids {
            if self.sel_set.insert(id) {
                self.selected.push(id);
            }
        }
    }

    /// Ymap de destination des nouveaux props (jamais la carte d'exemple).
    pub fn ensure_target_ymap(&mut self) -> usize {
        if let Some(y) = self.proj.ymaps.get(self.target_ymap) {
            if !y.sample {
                return self.target_ymap;
            }
        }
        if let Some(i) = self.proj.ymaps.iter().position(|y| !y.sample) {
            self.target_ymap = i;
            return i;
        }
        let n = self.proj.next_ymap_name();
        self.proj.ymaps.push(Ymap::new(&n));
        self.target_ymap = self.proj.ymaps.len() - 1;
        self.target_ymap
    }

    pub fn snap_pos(&self, p: V3) -> V3 {
        if self.snap && self.snap_size > 0.0 {
            let s = self.snap_size;
            v3((p.x / s).round() * s, (p.y / s).round() * s, p.z)
        } else {
            p
        }
    }

    /// Trouve l'intérieur (et sa pièce) qui contient un point du monde.
    pub fn mlo_at(&self, world: V3) -> Option<(usize, usize, usize, V3)> {
        for (ti, t) in self.proj.ytyps.iter().enumerate() {
            for (mi, m) in t.mlos.iter().enumerate() {
                let Some((p, q)) = self.proj.mlo_placement(&m.name) else { continue };
                let local = q.inverse().rotate(world - p);
                for (ri, r) in m.rooms.iter().enumerate().skip(1) {
                    if local.x >= r.bb_min.x && local.x <= r.bb_max.x && local.y >= r.bb_min.y && local.y <= r.bb_max.y {
                        return Some((ti, mi, ri, local));
                    }
                }
            }
        }
        None
    }

    /// Pose un prop (dans le monde, ou dans un intérieur si le point est dans une pièce).
    pub fn place_prop(&mut self, arch: &str, world: V3, heading_deg: Option<f32>) -> u64 {
        let p = self.snap_pos(world);
        let heading = heading_deg.unwrap_or_else(|| if self.rand_rot { self.rng.range(0.0, 360.0) } else { 0.0 });
        let scale = if self.rand_scale { self.rng.range(0.85, 1.15) } else { 1.0 };
        if self.place_into_mlo {
            if let Some((ti, mi, ri, local)) = self.mlo_at(p) {
                let name = self.proj.ytyps[ti].mlos[mi].name.clone();
                let (_, q) = self.proj.mlo_placement(&name).unwrap_or((V3::ZERO, Quat::IDENTITY));
                let mut e = Entity::new(arch, v3(local.x, local.y, 0.0));
                e.flags = mech::MLO_ENTITY_FLAGS;
                e.room = ri;
                e.rot = q.inverse().mul(Quat::from_z(heading.to_radians())).normalized();
                e.scale_xy = scale;
                e.scale_z = scale;
                let id = e.id;
                self.proj.ytyps[ti].mlos[mi].entities.push(e);
                self.cur_mlo = Some((ti, mi));
                return id;
            }
        }
        let yi = self.ensure_target_ymap();
        // avec le jeu chargé, le prop se pose à la hauteur réelle du sol
        let z = if self.game.world.is_some() { p.z } else { self.ground_z };
        let mut e = Entity::new(arch, v3(p.x, p.y, z));
        // distance d'affichage du type dans le jeu (CBaseArchetypeDef.lodDist), comme le jeu le pose
        if let Some(a) = self.game.world.as_ref().and_then(|w| w.archetypes.get(&crate::gta::crypto::joaat(&arch.to_lowercase()))) {
            if a.lod_dist > 0.0 {
                e.lod_dist = a.lod_dist;
            }
        }
        e.rot = Quat::from_z(heading.to_radians());
        e.scale_xy = scale;
        e.scale_z = scale;
        let id = e.id;
        self.proj.ymaps[yi].entities.push(e);
        id
    }

    pub fn delete_selected(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let ids: HashSet<u64> = self.sel_set.clone();
        let n = self.proj.delete_ids(&ids);
        self.set_selection(vec![]);
        self.status = format!("{n} objet(s) supprimé(s) d'un coup.");
    }

    pub fn sample_hidden(&self, it: ItemRef) -> bool {
        matches!(it, ItemRef::Ent(y, _) if self.proj.ymaps[y].sample && !self.opts.show_sample)
    }

    pub fn items_in_radius(&self, c: V3, r: f32, filter: &str, buildings: bool) -> Vec<u64> {
        let f = filter.to_lowercase();
        let mut out = Vec::new();
        for it in self.proj.all_items() {
            if self.sample_hidden(it) {
                continue;
            }
            let Some(e) = self.proj.world_entity(it) else { continue };
            if e.mlo.is_some() {
                continue;
            }
            if !buildings && e.archetype.starts_with("dw_batiment") {
                continue;
            }
            if !f.is_empty() && !e.archetype.to_lowercase().contains(&f) {
                continue;
            }
            if e.pos.dist2d(c) <= r {
                out.push(e.id);
            }
        }
        out
    }

    /// Décalage d'une copie, comme ProjectForm::NewEntity : rayon englobant le plus grand × 2.5,
    /// sur l'axe « droite » de la caméra (à plat) pour que la copie reste visible.
    pub fn copy_offset(&self) -> V3 {
        let mut radius: f32 = 0.0;
        for id in &self.selected {
            if let Some(e) = self.proj.find_id(*id).and_then(|r| self.proj.ent(r)) {
                let (mn, mx) = e.local_box(&self.proj);
                let s = e.scale();
                let half = v3((mx.x - mn.x) * s.x, (mx.y - mn.y) * s.y, (mx.z - mn.z) * s.z) * 0.5;
                radius = radius.max(half.dot(half).sqrt());
            }
        }
        let (_, right) = self.cam.flat_axes();
        right * (radius.max(0.5) * mech::SPAWN_COPY_RADIUS_MULT)
    }

    pub fn duplicate_selected(&mut self, off: V3) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let mut new_ids = Vec::new();
        // la copie d'un groupe posé forme son propre bloc
        let mut inst_map: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
        for id in self.selected.clone() {
            let Some(r) = self.proj.find_id(id) else { continue };
            let Some(src_e) = self.proj.ent(r) else { continue };
            let mut e = src_e.clone();
            e.id = model::new_id();
            e.game_index = -1;
            e.pos = e.pos + off;
            if e.group_inst != 0 {
                e.group_inst = *inst_map.entry(e.group_inst).or_insert_with(model::new_id);
            }
            new_ids.push(e.id);
            match r {
                ItemRef::Ent(y, _) => {
                    if self.proj.ymaps[y].sample {
                        let t = self.ensure_target_ymap();
                        self.proj.ymaps[t].entities.push(e);
                    } else {
                        self.proj.ymaps[y].entities.push(e);
                    }
                }
                ItemRef::MloEnt(t, m, _) => self.proj.ytyps[t].mlos[m].entities.push(e),
            }
        }
        let n = new_ids.len();
        self.set_selection(new_ids);
        self.status = format!("{n} objet(s) dupliqué(s).");
    }

    pub fn move_selected(&mut self, d: V3) {
        for id in self.selected.clone() {
            if let Some(r) = self.proj.find_id(id) {
                if let Some(w) = self.proj.world_entity(r) {
                    self.proj.set_world_pos(r, w.pos + d);
                }
            }
        }
    }

    pub fn rotate_selected(&mut self, deg: f32) {
        let q = Quat::from_z(deg.to_radians());
        let center = self.selection_center();
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

    pub fn selection_center(&self) -> V3 {
        let mut c = V3::ZERO;
        let mut n = 0.0;
        for id in &self.selected {
            if let Some(w) = self.proj.find_id(*id).and_then(|r| self.proj.world_entity(r)) {
                c = c + w.pos;
                n += 1.0;
            }
        }
        if n > 0.0 {
            c * (1.0 / n)
        } else {
            self.cam.target
        }
    }

    pub fn create_mlo(&mut self, name: &str, cols: u32, rows: u32, w: f32, d: f32, h: f32) {
        self.push_undo();
        let mut name = name.trim().to_string();
        if name.is_empty() || self.proj.find_mlo(&name).is_some() {
            let mut i = 1;
            loop {
                let n = format!("dw_interieur_{i}");
                if self.proj.find_mlo(&n).is_none() {
                    name = n;
                    break;
                }
                i += 1;
            }
        }
        let mlo = model::build_mlo(&name, cols as usize, rows as usize, w, d, h);
        let exits = mlo.portals.iter().filter(|p| p.room_to == 0 || p.room_from == 0).count() as u32;
        if self.proj.ytyps.is_empty() {
            let n = self.proj.next_ytyp_name();
            self.proj.ytyps.push(Ytyp { name: n, mlos: vec![], visible: true, raw_archetypes: vec![] });
        }
        let ti = self.cur_mlo.map(|(t, _)| t).unwrap_or(0).min(self.proj.ytyps.len() - 1);
        self.proj.ytyps[ti].mlos.push(mlo);
        let mi = self.proj.ytyps[ti].mlos.len() - 1;
        self.cur_mlo = Some((ti, mi));
        // placement de l'intérieur dans le ymap de destination, au centre de la vue
        let yi = self.ensure_target_ymap();
        let mut inst = Entity::new(&name, self.snap_pos(v3(self.cam.target.x, self.cam.target.y, self.ground_z)));
        inst.mlo = Some(MloInstData { num_exit_portals: exits, ..Default::default() });
        inst.lod_dist = 200.0;
        let id = inst.id;
        self.proj.ymaps[yi].entities.push(inst);
        self.set_selection(vec![id]);
        self.mlo_name = format!("dw_interieur_{}", self.proj.ytyps.iter().map(|t| t.mlos.len()).sum::<usize>() + 1);
        self.status = format!("Intérieur « {name} » créé : {} pièce(s), portails et porte compris. Pose des props dedans avec l'outil Poser.", cols * rows);
    }

    /// Crée un intérieur avec les pièces proposées pour l'étage `li` du bâtiment analysé, placé exactement sur
    /// le bâtiment ; les pièces se corrigent ensuite dans l'onglet Intérieur.
    pub fn rooms_from_level(&mut self, li: usize) {
        let Some(a) = self.game.analysis.as_ref() else { return };
        let Some(plan) = crate::analyse::propose_rooms(a, li) else {
            self.status = "Aucune pièce trouvée à cet étage (pas assez de sol).".into();
            return;
        };
        let (bpos, brot, bname) = (a.pos, a.rot, a.name.clone());
        self.push_undo();
        let mut name = format!("{}_etage{}", bname, li + 1);
        let mut k = 2;
        while self.proj.find_mlo(&name).is_some() {
            name = format!("{}_etage{}_{k}", bname, li + 1);
            k += 1;
        }
        let mut rooms = vec![model::Room::new("limbo", V3::ZERO, V3::ZERO)];
        for (n, lo, hi) in &plan.rooms {
            rooms.push(model::Room::new(n, *lo, *hi));
        }
        let portals: Vec<model::Portal> = plan.portals.iter().map(|(f, t, c)| model::Portal::new(*f, *t, *c)).collect();
        let exits = portals.iter().filter(|p| p.room_to == 0 || p.room_from == 0).count() as u32;
        let mlo = model::MloArchetype { name: name.clone(), lod_dist: 200.0, flags: 0, mlo_flags: 0, entities: Vec::new(), rooms, portals, entity_sets: Vec::new() };
        let n_rooms = plan.rooms.len();
        let n_portals = mlo.portals.len();
        if self.proj.ytyps.is_empty() {
            let n = self.proj.next_ytyp_name();
            self.proj.ytyps.push(Ytyp { name: n, mlos: vec![], visible: true, raw_archetypes: vec![] });
        }
        let ti = self.cur_mlo.map(|(t, _)| t).unwrap_or(0).min(self.proj.ytyps.len() - 1);
        self.proj.ytyps[ti].mlos.push(mlo);
        let mi = self.proj.ytyps[ti].mlos.len() - 1;
        self.cur_mlo = Some((ti, mi));
        // placement : exactement sur le bâtiment (même position et orientation)
        let yi = self.ensure_target_ymap();
        let mut inst = Entity::new(&name, bpos);
        inst.rot = brot;
        inst.mlo = Some(MloInstData { num_exit_portals: exits, ..Default::default() });
        inst.lod_dist = 200.0;
        self.proj.ymaps[yi].entities.push(inst);
        self.opts.show_rooms = true;
        self.status = format!("Étage {} : {n_rooms} pièce(s) et {n_portals} passage(s) proposés ({:.0} m² de sol) dans l'intérieur « {name} ». Corrige-les dans la liste des pièces (nom, coins, hauteur).", li + 1, plan.floor_area);
    }

    pub fn furnish_mlo(&mut self, style: &str) -> String {
        self.furnish_rooms(style, None)
    }

    /// Meuble toutes les pièces (`only` = None) ou une seule, selon un type de pièce ; les meubles sont posés sur
    /// le sol de la pièce (bas de sa boîte), donc à la bonne hauteur pour les pièces proposées par étage.
    pub fn furnish_rooms(&mut self, style: &str, only: Option<usize>) -> String {
        if self.cur_mlo.is_none() {
            return "Aucun intérieur courant : crée-en un d'abord.".into();
        }
        self.push_undo();
        self.furnish_rooms_inner(style, only)
    }

    /// Même chose, sans point d'annulation (pour l'enchaîner dans « Transformer en intérieur »).
    pub fn furnish_rooms_inner(&mut self, style: &str, only: Option<usize>) -> String {
        let Some((t, m)) = self.cur_mlo else { return "Aucun intérieur courant : crée-en un d'abord.".into() };
        let mut added = 0;
        let rooms: Vec<(usize, V3, V3)> = self.proj.ytyps[t].mlos[m].rooms.iter().enumerate().skip(1).filter(|(i, _)| only.map(|o| o == *i).unwrap_or(true)).map(|(i, r)| (i, r.bb_min, r.bb_max)).collect();
        for (ri, mn, mx) in rooms {
            let c = (mn + mx) * 0.5;
            let w = mx.x - mn.x;
            let d = mx.y - mn.y;
            let mut items: Vec<(&str, f32, f32, f32)> = Vec::new(); // (prop, x, y, cap)
            match style {
                "bureau" => {
                    let nx = ((w - 1.0) / 2.4).floor().max(1.0) as i32;
                    for i in 0..nx {
                        let x = mn.x + 1.2 + i as f32 * 2.4 + 0.8;
                        items.push(("prop_off_desk_01", x, c.y, 0.0));
                        items.push(("prop_chair_01a", x, c.y - 0.9, 180.0));
                    }
                    items.push(("prop_plant_01a", mx.x - 0.5, mx.y - 0.5, 0.0));
                }
                "bar" => {
                    items.push(("v_ret_ml_tablea", c.x, mx.y - 1.0, 0.0));
                    for i in -1..=1 {
                        items.push(("prop_bar_stool_01", c.x + i as f32 * 0.8, mx.y - 1.9, 0.0));
                    }
                    items.push(("prop_table_03", c.x, c.y - d * 0.2, 0.0));
                }
                "magasin" => {
                    items.push(("v_ret_ml_tablea", c.x, mn.y + 1.2, 0.0));
                    items.push(("prop_atm_01", mx.x - 0.6, c.y, 270.0));
                    items.push(("prop_vend_soda_01", mn.x + 0.6, c.y, 90.0));
                }
                "cuisine" => {
                    items.push(("v_res_fridgemodsml", mn.x + 0.6, mx.y - 0.6, 180.0));
                    items.push(("prop_table_03", c.x, c.y, 0.0));
                    items.push(("prop_chair_01a", c.x - 0.6, c.y - 0.8, 0.0));
                    items.push(("prop_chair_01a", c.x + 0.6, c.y + 0.8, 180.0));
                }
                _ => {
                    items.push(("prop_couch_01", c.x, mn.y + 0.7, 0.0));
                    items.push(("prop_table_03", c.x, c.y, 0.0));
                    items.push(("prop_tv_flat_01", c.x, mx.y - 0.3, 180.0));
                    items.push(("prop_plant_01a", mx.x - 0.5, mn.y + 0.5, 0.0));
                }
            }
            for (arch, x, y, cap) in items {
                let mut e = Entity::new(arch, v3(x, y, mn.z));
                e.flags = mech::MLO_ENTITY_FLAGS;
                e.room = ri;
                e.rot = Quat::from_z(cap.to_radians());
                self.proj.ytyps[t].mlos[m].entities.push(e);
                added += 1;
            }
        }
        match only {
            Some(ri) => format!("{added} meuble(s) ajouté(s) dans la pièce {ri} (type {style})."),
            None => format!("{added} meuble(s) ajouté(s) (style {style})."),
        }
    }

    pub fn select_filtered(&mut self, filter: &str, radius: f32) -> usize {
        let f = filter.to_lowercase();
        let c = self.cam.target;
        let mut ids = Vec::new();
        for it in self.proj.all_items() {
            if self.sample_hidden(it) {
                continue;
            }
            let Some(e) = self.proj.world_entity(it) else { continue };
            if !f.is_empty() && !e.archetype.to_lowercase().contains(&f) {
                continue;
            }
            if radius > 0.0 && e.pos.dist2d(c) > radius {
                continue;
            }
            ids.push(e.id);
        }
        let n = ids.len();
        self.set_selection(ids);
        n
    }
}
