//! « Transformer en intérieur » : enchaîne analyse, pièces de tous les étages, type de chaque pièce, meubles,
//! portes dans les passages, lumières et ambiance — en un clic, à corriger ensuite.

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use crate::model::{self, Entity, MloInstData};

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
/// Ouverture de l'entrée : profondeur de part et d'autre du plan de la porte (épaisseur de façade, m) et plus
/// grand triangle de façade retiré (m²) — **estimé** (un mur d'un seul triangle n'est pas percé : il disparaîtrait).
const ENTRY_DEPTH: f32 = 1.5;
const ENTRY_MAX_TRI_M2: f32 = 6.0;

/// Plus grand numéro de pièce d'une surface de collision : 5 bits (CodeWalker BoundMaterial_s::set_RoomId
/// 0x060042e3 : Data1 = (Data1 & ~0x1F0000) | ((valeur & 31) << 16) ; get_RoomId 0x060042e2 : (Data1 >> 16) & 31).
const ROOM_ID_MAX: u32 = 31;

/// Épaisseur de la dalle de collision du sol de chaque pièce (m) : posée de 2 cm sous le sol de la pièce.
const ROOM_FLOOR_SINK: f32 = 0.02;

/// Ancien projet : intérieur « <bâtiment>_int » relié à la copie du bâtiment la plus proche, à moins de ce
/// rayon (m) — **estimé**.
const LINK_LEGACY_RADIUS: f32 = 5.0;

fn quat_close(a: Quat, b: Quat) -> bool {
    (a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w).abs() > 0.99999
}

impl DwApp {
    /// Garde chaque intérieur créé depuis un bâtiment au même endroit et dans le même sens que lui : si l'un des deux
    /// a bougé ou tourné (poignées, clavier, Aller voir…), l'autre le suit ; s'ils diffèrent sans qu'on sache
    /// lequel a bougé, le bâtiment donne la référence.
    pub fn mlo_link_tick(&mut self) {
        if self.frame % 60 == 0 {
            self.mlo_link_legacy();
        }
        // analyse d'un bâtiment qui a bougé ou tourné depuis : ses couleurs ne sont plus à sa place, elle est retirée
        // (« Transformer en intérieur » la refait)
        if let Some(a) = self.game.analysis.as_ref() {
            let now = self.proj.find_id(a.id).and_then(|r| self.proj.world_entity(r)).map(|e| (e.pos, e.rot));
            if now.map(|(p, q)| (p - a.pos).len() > 1e-3 || !quat_close(q, a.rot)).unwrap_or(false) {
                self.game.analysis = None;
            }
        }
        for li in 0..self.proj.mlo_links.len() {
            let (iid, bid) = self.proj.mlo_links[li];
            let (Some(ri), Some(rb)) = (self.proj.find_id(iid), self.proj.find_id(bid)) else { continue };
            // l'intérieur est tourné de `yaw` par rapport au bâtiment (direction des murs dans le modèle) : on compare
            // donc le bâtiment avec l'intérieur ramené dans son sens
            let yaw = self.proj.mlo_link_yaw.iter().find(|y| y.0 == iid).map(|y| y.1).unwrap_or(0.0);
            let (yq, yqi) = (Quat::from_z(yaw), Quat::from_z(-yaw));
            let (Some(i), Some(b)) = (self.proj.ent(ri).map(|e| (e.pos, e.rot.mul(yqi).normalized())), self.proj.ent(rb).map(|e| (e.pos, e.rot))) else { continue };
            if (i.0 - b.0).len() < 1e-3 && quat_close(i.1, b.1) {
                self.mlo_link_seen.insert(iid, b);
                continue;
            }
            let last = self.mlo_link_seen.get(&iid).copied();
            let moved = |t: (V3, Quat)| last.map(|l| (t.0 - l.0).len() > 1e-3 || !quat_close(t.1, l.1)).unwrap_or(true);
            let (p, q) = if moved(i) && !moved(b) { i } else { b };
            if let Some(e) = self.proj.ent_mut(rb) {
                e.pos = p;
                e.rot = q;
            }
            if let Some(e) = self.proj.ent_mut(ri) {
                e.pos = p;
                e.rot = q.mul(yq).normalized();
            }
            self.mlo_link_seen.insert(iid, (p, q));
        }
    }

    /// Projets d'avant le lien : un intérieur « <bâtiment>_int » (ou « _intN ») est relié à la copie de ce bâtiment
    /// (ou au modèle découpé qui en vient) la plus proche, à moins de LINK_LEGACY_RADIUS.
    fn mlo_link_legacy(&mut self) {
        let linked: std::collections::HashSet<u64> = self.proj.mlo_links.iter().map(|l| l.0).collect();
        let mut add = Vec::new();
        for e in self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.mlo.is_some() && !linked.contains(&e.id)) {
            let a = e.archetype.to_lowercase();
            let Some(k) = a.rfind("_int") else { continue };
            if !a[k + 4..].chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let base = &a[..k];
            let cuts: Vec<&str> = self.proj.building_cuts.iter().filter(|c| c.source == base).map(|c| c.name.as_str()).collect();
            let best = self
                .proj
                .ymaps
                .iter()
                .flat_map(|y| y.entities.iter())
                .filter(|b| b.mlo.is_none() && (b.archetype.eq_ignore_ascii_case(base) || cuts.iter().any(|c| b.archetype.eq_ignore_ascii_case(c))))
                .map(|b| ((b.pos - e.pos).len(), b.id))
                .filter(|(d, _)| *d <= LINK_LEGACY_RADIUS)
                .min_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
            if let Some((_, bid)) = best {
                add.push((e.id, bid));
            }
        }
        if !add.is_empty() {
            self.status = format!("{} intérieur(s) relié(s) à leur bâtiment : ils bougent et tournent maintenant ensemble.", add.len());
            self.proj.mlo_links.extend(add);
        }
    }

    /// Collision des intérieurs créés dans DragoWalker : le sol de chaque pièce (boîte de la pièce, à sa base),
    /// avec le numéro de la pièce (RoomID) : le jeu sait ainsi dans quelle pièce est le joueur et dessine ses objets ;
    /// fichier « <intérieur>.ybn », repère de l'intérieur, déclaré dans _manifest.ymf (Interiors > Bounds = son nom,
    /// GenerateProjectManifest 0x06001194). Pas pour les intérieurs des MLO importés (ils ont la leur) ni pour ceux
    /// du jeu. Rend le compte rendu.
    pub fn export_interior_bounds(&self, stream: &std::path::Path) -> Vec<String> {
        let mut report = Vec::new();
        let external = self.mlo_external_ytyps();
        let w = self.game.world.as_deref();
        let names = crate::terrain::material_names(w);
        let concrete = names.iter().position(|n| n == "CONCRETE").unwrap_or(1) as u32;
        for t in self.proj.ytyps.iter().filter(|t| !external.contains(&t.name.to_lowercase())) {
            for m in &t.mlos {
                // intérieur du jeu (même sans le jeu chargé : shell::mlo_is_game) : sa collision du jeu reste
                if self.mlo_is_game(m) {
                    continue;
                }
                let (mut verts, mut tris, mut mats) = (Vec::new(), Vec::new(), Vec::new());
                let mut too_many = 0;
                // bâtiment de l'atelier : sol et plafond de chaque pièce dans l'emprise réelle, sans murs (ceux du
                // bâtiment sont dans sa collision) ; la boîte des pièces fermait le creux d'une maison en L
                if let Some(h) = self.proj.houses.iter().find(|h| h.interior.as_deref().map(|n| n.eq_ignore_ascii_case(&m.name)).unwrap_or(false)) {
                    let mut by_room: std::collections::BTreeMap<usize, Vec<[V3; 3]>> = std::collections::BTreeMap::new();
                    for (ri, t) in h.room_collision(&m.rooms) {
                        by_room.entry(ri).or_default().push(t);
                    }
                    for (ri, ts) in by_room {
                        if ri as u32 > ROOM_ID_MAX {
                            too_many += 1;
                            continue;
                        }
                        let mi = mats.len() as u8;
                        mats.push((concrete, ri as u32));
                        for t in ts {
                            let base = verts.len() as u32;
                            verts.extend(t.iter().copied());
                            tris.push((base, base + 1, base + 2, mi));
                        }
                    }
                }
                // coque modelable (design.rs) : murs, sols, plafonds et blocs, avec le numéro de leur pièce
                let dcol = self.design_collision(m);
                if !dcol.is_empty() {
                    let mut by_room: std::collections::BTreeMap<usize, Vec<[V3; 3]>> = std::collections::BTreeMap::new();
                    for (ri, t) in dcol {
                        by_room.entry(ri).or_default().push(t);
                    }
                    for (ri, ts) in by_room {
                        if ri as u32 > ROOM_ID_MAX {
                            continue;
                        }
                        let mi = mats.len() as u8;
                        mats.push((concrete, ri as u32));
                        for t in ts {
                            let base = verts.len() as u32;
                            verts.extend(t.iter().copied());
                            tris.push((base, base + 1, base + 2, mi));
                        }
                    }
                }
                // boîtes des pièces (pas pour un bâtiment de l'atelier : sa collision vient de lui ; une pièce de forme libre n'y
                // garde que son escalier, voir shell::room_quads)
                let is_house = self.proj.houses.iter().any(|h| h.interior.as_deref().map(|n| n.eq_ignore_ascii_case(&m.name)).unwrap_or(false));
                let generic = !is_house && too_many == 0;
                for (ri, r) in m.rooms.iter().enumerate().skip(1).filter(|_| generic) {
                    let (a, b) = (r.bb_min, r.bb_max);
                    if b.x - a.x < 0.1 || b.y - a.y < 0.1 {
                        continue;
                    }
                    // le numéro de pièce d'une surface tient sur 5 bits : pièces 1 à 31 seulement (une pièce 32
                    // deviendrait 0, « dehors »)
                    if ri as u32 > ROOM_ID_MAX {
                        too_many += 1;
                        continue;
                    }
                    // toute la coque de la pièce (shell.rs) : sol un peu enfoncé, murs ouverts aux passages, plafond
                    let mi = mats.len() as u8;
                    mats.push((concrete, ri as u32));
                    for (part, q, _) in crate::shell::room_quads(m, ri) {
                        let base = verts.len() as u32;
                        let sink = if part == 0 { ROOM_FLOOR_SINK } else { 0.0 };
                        verts.extend(q.iter().map(|p| v3(p.x, p.y, p.z - sink)));
                        tris.push((base, base + 1, base + 2, mi));
                        tris.push((base, base + 2, base + 3, mi));
                    }
                    let _ = (a, b);
                }
                if tris.is_empty() {
                    continue;
                }
                if std::fs::write(stream.join(format!("{}.ybn.xml", m.name.to_lowercase())), crate::roads::mesh_collision_xml_rooms(&verts, &tris, &mats)).is_ok() {
                    report.push(format!("Intérieur « {} » : collision de {} pièce(s) (sol, murs, plafond ; numéro de pièce).", m.name, mats.len()));
                }
                if too_many > 0 {
                    report.push(format!("Intérieur « {} » : {too_many} pièce(s) au-delà de la 31e sans collision (le jeu ne numérote que 31 pièces dans une collision) : regroupe des pièces ou fais un second intérieur.", m.name));
                }
            }
        }
        report
    }
}

impl DwApp {
    /// Cherche dans les types d'objets du jeu le meilleur candidat : nom contenant tous les mots de `must`,
    /// taille dans les limites, le plus proche de `target` (largeur, profondeur, hauteur).
    pub(crate) fn find_game_prop(&self, must: &[&str], prefer: &[&str], min: V3, max: V3, target: V3) -> Option<(String, V3, V3)> {
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
            // à score égal, le nom le plus petit (ordre alphabétique) : le parcours de la table des types suit un
            // ordre qui change d'un lancement à l'autre, le choix ne doit pas en dépendre
            if best.as_ref().map(|b| score < b.0 || (score == b.0 && n < b.1)).unwrap_or(true) {
                best = Some((score, n, a.bb_min, a.bb_max));
            }
        }
        best.map(|b| (b.1, b.2, b.3))
    }

    /// Ambiances (timecycle) d'intérieur du jeu : noms lus dans les fichiers timecycle_mods*.xml.
    pub(crate) fn game_timecycles(&self) -> Vec<String> {
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
        // (et refaite si le bâtiment a bougé ou tourné depuis : l'intérieur prend sa position et son sens actuels)
        let sel = self.selected.first().copied();
        let now = sel.and_then(|id| self.proj.find_id(id)).and_then(|r| self.proj.world_entity(r)).map(|e| (e.pos, e.rot));
        let stale = |a: &crate::analyse::Analysis| Some(a.id) != sel || now.map(|(p, q)| (p - a.pos).len() > 1e-3 || !quat_close(q, a.rot)).unwrap_or(true);
        if self.game.analysis.as_ref().map(stale).unwrap_or(true) {
            self.analyse_selected();
        }
        let Some(a) = self.game.analysis.as_ref() else { return "Choisis d'abord un bâtiment du jeu (clic dessus).".into() };
        // 2. pièces de tous les étages (sauf le toit), réunies dans un seul intérieur
        let mut rooms: Vec<(String, V3, V3, usize)> = Vec::new(); // nom, coins, étage
        let mut portals: Vec<(u32, u32, [V3; 4])> = Vec::new();
        let mut entry_done = false;
        let mut level_rooms: Vec<Vec<u32>> = Vec::new();
        // (les toits ne sont plus des étages : l'analyse les range à part, K_ROOF)
        for li in 0..a.levels.len() {
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
        let (bpos, bname, bid) = (a.pos, a.name.clone(), a.id);
        // repère des pièces : le bâtiment tourné de la direction de ses murs (analyse.wall_yaw)
        let (brot, wall_yaw) = (a.room_rot(), a.wall_yaw);
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
        // jamais dans le .ytyp d'une ressource MLO importée (recopié tel quel à l'export : l'intérieur n'y serait pas)
        let ti = self.own_ytyp();
        self.proj.ytyps[ti].mlos.push(mlo);
        let mi = self.proj.ytyps[ti].mlos.len() - 1;
        self.cur_mlo = Some((ti, mi));
        let yi = self.ensure_target_ymap();
        let mut inst = Entity::new(&name, bpos);
        inst.rot = brot;
        inst.mlo = Some(MloInstData { num_exit_portals: exits, ..Default::default() });
        inst.flags = crate::mech::MLO_INSTANCE_FLAGS;
        inst.lod_dist = 200.0;
        // relié au bâtiment : ils bougent et tournent ensemble (mlo_link_tick)
        self.proj.mlo_links.retain(|l| l.1 != bid);
        self.proj.mlo_links.push((inst.id, bid));
        self.proj.mlo_link_yaw.push((inst.id, wall_yaw));
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
        // 7. entrée ouverte dans la façade : les triangles de la façade (pas plus grands que ENTRY_MAX_TRI_M2) dont le
        //    centre est dans l'ouverture de la porte d'entrée, sur ENTRY_DEPTH de part et d'autre, sont retirés de la
        //    copie (modèle découpé, comme « Éditer ce bâtiment »)
        let mut opened = String::new();
        if let Some((_, _, cr)) = portals.iter().find(|(f, t, _)| *f == 0 || *t == 0) {
            // coins du portail : repère des pièces → repère du bâtiment (où se mesurent les triangles du modèle)
            let yq = Quat::from_z(wall_yaw);
            let c = cr.map(|p| yq.rotate(p));
            let along = c[3] - c[0];
            let len = along.len().max(0.01);
            let dir = along * (1.0 / len);
            let n = v3(dir.y, -dir.x, 0.0);
            let mid = (c[0] + c[3]) * 0.5;
            let (zb, zt) = (c[0].z, c[1].z);
            let test = |p: V3, area: f32| -> bool { area <= ENTRY_MAX_TRI_M2 && (p - mid).dot(dir).abs() <= len * 0.5 + 0.1 && (p - mid).dot(n).abs() <= ENTRY_DEPTH && p.z >= zb - 0.1 && p.z <= zt + 0.2 };
            opened = match self.cut_remove_where(bid, &test) {
                Ok(0) => " Entrée : la façade est d'un seul grand morceau à cet endroit, ouvre-la avec « Éditer ce bâtiment » (ou dans Blender).".into(),
                Ok(k) => format!(" Entrée ouverte dans la façade ({k} triangle(s) retiré(s) de cette copie)."),
                Err(e) => format!(" Entrée non ouverte : {e}."),
            };
        }
        self.opts.show_rooms = true;
        let types: Vec<String> = counts.iter().map(|(k, n)| format!("{n} {k}")).collect();
        format!(
            "« {name} » : {} pièce(s) sur {n_levels} étage(s) ({}), {} passage(s), {n_doors} porte(s) ({}), {n_lights} lumière(s) ({}). La version lointaine du jeu est gardée. Chaque pièce a sa coque (sol, murs ouverts aux passages, plafond) et sa collision complète.{opened}",
            rooms.len(),
            types.join(", "),
            portals.len(),
            door.as_ref().map(|d| d.0.as_str()).unwrap_or("aucune trouvée"),
            light.as_ref().map(|d| d.0.as_str()).unwrap_or("aucune trouvée")
        )
    }
}
