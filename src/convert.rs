//! « Transformer en intérieur » : enchaîne analyse, pièces de tous les étages, type de chaque pièce, meubles,
//! portes dans les passages, lumières et ambiance — en un clic, à corriger ensuite.

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use crate::model::{self, Entity, MloInstData, Ytyp};

/// Type de pièce deviné d'après sa surface (m²) — estimé.
pub fn guess_room_type(area: f32) -> &'static str {
    if area < 15.0 {
        "cuisine"
    } else if area < 30.0 {
        "salon"
    } else if area < 60.0 {
        "bureau"
    } else {
        "magasin"
    }
}

/// Une lumière pour 25 m² de pièce (au moins une) — estimé.
const LIGHT_M2: f32 = 25.0;

impl DwApp {
    /// Cherche dans les types d'objets du jeu le meilleur candidat : nom contenant tous les mots de `must`,
    /// taille dans les limites, le plus proche de `target` (largeur, profondeur, hauteur).
    fn find_game_prop(&self, must: &[&str], prefer: &[&str], min: V3, max: V3, target: V3) -> Option<(String, V3, V3)> {
        let w = self.game.world.as_ref()?;
        let mut best: Option<(f32, String, V3, V3)> = None;
        for (h, a) in &w.archetypes {
            if a.mlo {
                continue;
            }
            let Some(name) = w.names.get(h) else { continue };
            let n = name.to_lowercase();
            if !must.iter().all(|m| n.contains(m)) || ["frame", "lod", "slide", "garage", "gate", "bars", "tunnel", "street", "traffic", "neon", "broken", "damage", "dam", "glass"].iter().any(|x| n.contains(x)) {
                continue;
            }
            let s = a.bb_max - a.bb_min;
            if s.x < min.x || s.y < min.y || s.z < min.z || s.x > max.x || s.y > max.y || s.z > max.z {
                continue;
            }
            let mut score = (s - target).len();
            // préférences dans l'ordre : la première trouvée compte le plus
            if let Some(k) = prefer.iter().position(|p| n.starts_with(p) || n.contains(p)) {
                score -= 3.0 - k as f32 * 0.5;
            }
            if best.as_ref().map(|b| score < b.0).unwrap_or(true) {
                best = Some((score, n, a.bb_min, a.bb_max));
            }
        }
        best.map(|b| (b.1, b.2, b.3))
    }

    /// Ambiances (timecycle) d'intérieur du jeu : noms lus dans les fichiers timecycle_mods*.xml.
    fn game_timecycles(&self) -> Vec<String> {
        let Some(w) = self.game.world.as_ref() else { return Vec::new() };
        let mut out = Vec::new();
        for f in w.vfs.files.iter().filter(|f| f.name.starts_with("timecycle_mods") && f.name.ends_with(".xml")) {
            let Some(d) = w.vfs.read_binary(f, &w.keys) else { continue };
            let s = String::from_utf8_lossy(&d);
            for part in s.split("<modifier name=\"").skip(1) {
                if let Some(e) = part.find('"') {
                    out.push(part[..e].to_string());
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// Bouton « Transformer en intérieur » : tout le bâtiment choisi devient un intérieur prêt à corriger.
    pub fn convert_to_interior(&mut self) -> String {
        // 1. analyse (si pas déjà faite pour cet objet)
        let sel = self.selected.first().copied();
        if self.game.analysis.as_ref().map(|a| Some(a.id) != sel).unwrap_or(true) {
            self.analyse_selected();
        }
        let Some(a) = self.game.analysis.as_ref() else { return "Choisis d'abord un bâtiment du jeu (clic dessus).".into() };
        // 2. pièces de tous les étages (sauf le toit), réunies dans un seul intérieur
        let mut rooms: Vec<(String, V3, V3, usize)> = Vec::new(); // nom, coins, étage
        let mut portals: Vec<(u32, u32, [V3; 4])> = Vec::new();
        let mut entry_done = false;
        let mut level_rooms: Vec<Vec<u32>> = Vec::new();
        for li in 0..a.levels.len() {
            if (a.roof_z - a.levels[li].0).abs() < 3.0 {
                continue;
            }
            let Some(plan) = crate::analyse::propose_rooms(a, li) else { continue };
            let off = rooms.len() as u32;
            let mut ids = Vec::new();
            for (i, (n, lo, hi)) in plan.rooms.iter().enumerate() {
                rooms.push((format!("e{}_{}", li + 1, n), *lo, *hi, li));
                ids.push(off + i as u32 + 1);
            }
            for (f, t, c) in plan.portals {
                if t == 0 || f == 0 {
                    // une seule entrée : celle de l'étage le plus bas
                    if entry_done {
                        continue;
                    }
                    entry_done = true;
                    portals.push((if f == 0 { 0 } else { f + off }, if t == 0 { 0 } else { t + off }, c));
                } else {
                    portals.push((f + off, t + off, c));
                }
            }
            level_rooms.push(ids);
        }
        if rooms.is_empty() {
            return "Aucune pièce trouvée dans ce bâtiment.".into();
        }
        // passages entre étages : trémie de 1,2 m au plancher de l'étage du dessus, là où deux pièces se superposent
        for w in level_rooms.windows(2) {
            let mut best: Option<(f32, u32, u32, V3, V3)> = None;
            for &lo in &w[0] {
                for &hi in &w[1] {
                    let (a0, a1) = (&rooms[lo as usize - 1], &rooms[hi as usize - 1]);
                    let x0 = a0.1.x.max(a1.1.x);
                    let x1 = a0.2.x.min(a1.2.x);
                    let y0 = a0.1.y.max(a1.1.y);
                    let y1 = a0.2.y.min(a1.2.y);
                    let ov = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
                    if ov > 1.5 && best.as_ref().map(|b| ov > b.0).unwrap_or(true) {
                        best = Some((ov, lo, hi, v3(x0, y0, 0.0), v3(x1, y1, 0.0)));
                    }
                }
            }
            if let Some((_, lo, hi, mn, mx)) = best {
                let z = rooms[hi as usize - 1].1.z;
                let c = (mn + mx) * 0.5;
                let s = 0.6f32.min((mx.x - mn.x) * 0.5).min((mx.y - mn.y) * 0.5);
                portals.push((lo, hi, [v3(c.x - s, c.y - s, z), v3(c.x + s, c.y - s, z), v3(c.x + s, c.y + s, z), v3(c.x - s, c.y + s, z)]));
            }
        }
        let (bpos, brot, bname) = (a.pos, a.rot, a.name.clone());
        let n_levels = level_rooms.len();
        // 3. ambiances du jeu et accessoires (porte, lumière) trouvés dans les types d'objets du jeu
        let tcs = self.game_timecycles();
        let pick_tc = |kind: &str| -> String {
            let keys: &[&str] = match kind {
                "bureau" => &["office"],
                "magasin" => &["shop", "store", "clothes"],
                "bar" => &["bar", "club"],
                "cuisine" => &["kitchen", "apart", "house"],
                _ => &["apart", "house", "living", "franklin"],
            };
            tcs.iter().find(|t| t.to_lowercase().starts_with("int") && keys.iter().any(|k| t.to_lowercase().contains(k))).or_else(|| tcs.iter().find(|t| t.to_lowercase().starts_with("int_"))).cloned().unwrap_or_default()
        };
        let door = self.find_game_prop(&["door"], &["v_ilev_ra_door", "v_ilev_fh_door", "v_ilev_"], v3(0.6, 0.02, 1.9), v3(1.4, 0.4, 2.7), v3(1.0, 0.1, 2.2));
        let light = self.find_game_prop(&["light"], &["ceil", "_ceiling", "v_res_", "v_serv_ct"], v3(0.1, 0.1, 0.02), v3(1.6, 1.6, 0.8), v3(0.6, 0.6, 0.15));
        // 4. l'intérieur lui-même, posé exactement sur le bâtiment
        self.push_undo();
        let mut name = format!("{bname}_int");
        let mut k = 2;
        while self.proj.find_mlo(&name).is_some() {
            name = format!("{bname}_int{k}");
            k += 1;
        }
        let mut mrooms = vec![model::Room::new("limbo", V3::ZERO, V3::ZERO)];
        for (n, lo, hi, _) in &rooms {
            mrooms.push(model::Room::new(n, *lo, *hi));
        }
        let mportals: Vec<model::Portal> = portals.iter().map(|(f, t, c)| model::Portal::new(*f, *t, *c)).collect();
        let exits = mportals.iter().filter(|p| p.room_to == 0 || p.room_from == 0).count() as u32;
        let mlo = model::MloArchetype { name: name.clone(), lod_dist: 200.0, flags: 0, mlo_flags: 0, entities: Vec::new(), rooms: mrooms, portals: mportals, entity_sets: Vec::new() };
        if self.proj.ytyps.is_empty() {
            let n = self.proj.next_ytyp_name();
            self.proj.ytyps.push(Ytyp { name: n, mlos: vec![], visible: true, raw_archetypes: vec![] });
        }
        let ti = self.cur_mlo.map(|(t, _)| t).unwrap_or(0).min(self.proj.ytyps.len() - 1);
        self.proj.ytyps[ti].mlos.push(mlo);
        let mi = self.proj.ytyps[ti].mlos.len() - 1;
        self.cur_mlo = Some((ti, mi));
        let yi = self.ensure_target_ymap();
        let mut inst = Entity::new(&name, bpos);
        inst.rot = brot;
        inst.mlo = Some(MloInstData { num_exit_portals: exits, ..Default::default() });
        inst.lod_dist = 200.0;
        self.proj.ymaps[yi].entities.push(inst);
        // 5. type, ambiance, meubles et lumières de chaque pièce
        let mut counts = std::collections::BTreeMap::new();
        let mut n_lights = 0;
        for (ri, (_, lo, hi, _)) in rooms.iter().enumerate() {
            let ri = ri + 1;
            let area = (hi.x - lo.x) * (hi.y - lo.y);
            let kind = guess_room_type(area);
            *counts.entry(kind).or_insert(0) += 1;
            let tc = pick_tc(kind);
            {
                let r = &mut self.proj.ytyps[ti].mlos[mi].rooms[ri];
                r.timecycle = tc;
                r.name = format!("{}_{}", r.name, kind);
            }
            self.furnish_rooms_inner(kind, Some(ri));
            if let Some((ln, lmn, lmx)) = &light {
                let n = ((area / LIGHT_M2).ceil() as usize).max(1);
                let cols = (n as f32).sqrt().ceil() as usize;
                let rows = n.div_ceil(cols);
                for i in 0..n {
                    let (cx, cy) = (i % cols, i / cols);
                    let x = lo.x + (hi.x - lo.x) * (cx as f32 + 0.5) / cols as f32;
                    let y = lo.y + (hi.y - lo.y) * (cy as f32 + 0.5) / rows as f32;
                    // accroché au plafond : le haut du modèle touche le haut de la pièce
                    let z = hi.z - lmx.z.max(0.0) - 0.02;
                    let _ = lmn;
                    let mut e = Entity::new(ln, v3(x, y, z));
                    e.flags = crate::mech::MLO_ENTITY_FLAGS;
                    e.room = ri;
                    self.proj.ytyps[ti].mlos[mi].entities.push(e);
                    n_lights += 1;
                }
            }
        }
        // 6. une porte du jeu dans chaque passage vertical entre deux pièces, charnière sur le premier coin
        let mut n_doors = 0;
        if let Some((dn, dmn, dmx)) = &door {
            let dw = (dmx.x - dmn.x).max(0.1);
            for (f, t, c) in &portals {
                if *f == 0 || *t == 0 || (c[1].z - c[0].z).abs() < 0.5 {
                    continue;
                }
                let along = c[3] - c[0];
                let len = along.len();
                if len < 0.3 {
                    continue;
                }
                let dir = along * (1.0 / len);
                let heading = dir.y.atan2(dir.x);
                // le modèle s'étend vers +x depuis sa charnière (origine) : charnière posée sur le coin c0
                let hinge = c[0] - dir * dmn.x.min(0.0);
                let mut e = Entity::new(dn, hinge);
                e.rot = Quat::from_z(heading);
                e.scale_xy = (len / dw).clamp(0.6, 1.6);
                e.flags = crate::mech::MLO_ENTITY_FLAGS;
                e.room = *f as usize;
                self.proj.ytyps[ti].mlos[mi].entities.push(e);
                n_doors += 1;
            }
        }
        self.opts.show_rooms = true;
        let types: Vec<String> = counts.iter().map(|(k, n)| format!("{n} {k}")).collect();
        format!(
            "« {name} » : {} pièce(s) sur {n_levels} étage(s) ({}), {} passage(s), {n_doors} porte(s) ({}), {n_lights} lumière(s) ({}). La version lointaine du jeu est gardée.",
            rooms.len(),
            types.join(", "),
            portals.len(),
            door.as_ref().map(|d| d.0.as_str()).unwrap_or("aucune trouvée"),
            light.as_ref().map(|d| d.0.as_str()).unwrap_or("aucune trouvée")
        )
    }
}
