//! Outils MCP pour s'appuyer sur les intérieurs du jeu : les étudier (pièces, passages, meubles par pièce),
//! reprendre un intérieur du jeu à sa place exacte (structure seule ou complet) pour le réaménager, cacher celui du
//! jeu, choisir un bâtiment du jeu, ajouter une entrée, recouper une pièce, placer un intérieur.

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use serde_json::{json, Value};

fn tool(name: &str, desc: &str, props: Value, required: &[&str]) -> Value {
    json!({"name": name, "description": desc, "inputSchema": {"type": "object", "properties": props, "required": required}})
}

fn txt(s: impl Into<String>) -> Vec<Value> {
    vec![json!({"type": "text", "text": s.into()})]
}

fn num(a: &Value, k: &str) -> Option<f32> {
    a.get(k).and_then(|v| v.as_f64()).map(|v| v as f32)
}

/// Murs d'un modèle coupé à la hauteur `z` (triangles dans un même repère) : morceaux des faces à peu près
/// verticales, réunis par droite (angle à 2° près, décalage à 5 cm près) et triés du plus long au plus court ; seules
/// les droites d'au moins 1 m restent.
pub fn footprint_walls(tris: &[[V3; 3]], z: f32) -> Vec<([f32; 2], [f32; 2])> {
    let mut segs: Vec<([f32; 2], [f32; 2])> = Vec::new();
    for t in tris {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        if n.len() < 1e-6 || (n.z / n.len()).abs() > 0.3 {
            continue;
        }
        let mut pts: Vec<[f32; 2]> = Vec::new();
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if (a.z - z) * (b.z - z) < 0.0 {
                let f = (z - a.z) / (b.z - a.z);
                pts.push([a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f]);
            }
        }
        if pts.len() == 2 && ((pts[1][0] - pts[0][0]).powi(2) + (pts[1][1] - pts[0][1]).powi(2)) > 1e-4 {
            segs.push((pts[0], pts[1]));
        }
    }
    // droites : (angle 0..180° arrondi à 2°, distance à l'origine arrondie à 5 cm) → étendue le long de la droite
    let mut lines: std::collections::HashMap<(i32, i32), (f32, f32, f32, f32)> = std::collections::HashMap::new();
    for (p, q) in &segs {
        let mut ang = (q[1] - p[1]).atan2(q[0] - p[0]).to_degrees();
        if ang < 0.0 {
            ang += 180.0;
        }
        let ka = ((ang / 2.0).round() as i32).rem_euclid(90);
        let a = (ka as f32 * 2.0).to_radians();
        let (dx, dy) = (a.cos(), a.sin());
        let off = -dy * p[0] + dx * p[1];
        let ko = (off / 0.05).round() as i32;
        let (s0, s1) = (dx * p[0] + dy * p[1], dx * q[0] + dy * q[1]);
        let e = lines.entry((ka, ko)).or_insert((f32::MAX, f32::MIN, 0.0, off));
        e.0 = e.0.min(s0.min(s1));
        e.1 = e.1.max(s0.max(s1));
        e.2 += (s1 - s0).abs();
    }
    let mut out: Vec<(f32, [f32; 2], [f32; 2])> = lines
        .iter()
        .filter(|(_, v)| v.2 >= 1.0)
        .map(|(k, v)| {
            let a = (k.0 as f32 * 2.0).to_radians();
            let (dx, dy) = (a.cos(), a.sin());
            let at = |s: f32| [dx * s - dy * v.3, dy * s + dx * v.3];
            (v.2, at(v.0), at(v.1))
        })
        .collect();
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    out.into_iter().map(|(_, p, q)| (p, q)).collect()
}

pub const NAMES: [&str; 11] = ["repair_game_interior", "building_windows", "add_window", "building_bounds", "study_game_interior", "copy_game_interior", "hide_game_interior", "select_game_object", "add_entrance", "split_room", "place_interior"];

pub fn tools_game() -> Vec<Value> {
    let interior = json!({"type": "string", "description": "nom de l'intérieur du projet"});
    vec![
        tool(
            "repair_game_interior",
            "Répare un intérieur repris du jeu (fait avec une ancienne version) : rattache la collision d'origine de Rockstar, mesure le vrai sol de ses pièces, remet les réglages d'origine de ses objets (portes comprises). game_name : l'intérieur du jeu d'origine (sinon celui placé au même endroit).",
            json!({"interior": interior, "game_name": {"type": "string"}}),
            &["interior"],
        ),
        tool(
            "building_windows",
            "Vitres du bâtiment du jeu qui entoure un intérieur du projet, dans le repère de l'intérieur : pour chaque fenêtre, le mur (normale), le milieu (x, y), la largeur, l'allège (hauteur du bas au-dessus du sol de l'intérieur) et la hauteur. Les pièces contre la façade doivent avoir une fenêtre (add_window) en face de chaque vitre, sinon on voit un mur derrière la vitre.",
            json!({"interior": interior}),
            &["interior"],
        ),
        tool(
            "add_window",
            "Perce une fenêtre (portail vers le dehors, avec allège) dans le mur d'une pièce, au point du mur le plus proche de (x, y) : width, sill (hauteur du bas au-dessus du sol de la pièce), height. À placer en face des vitres du bâtiment (building_windows).",
            json!({"interior": interior, "room": {"type": ["integer", "string"]}, "x": {"type": "number"}, "y": {"type": "number"}, "width": {"type": "number"}, "sill": {"type": "number"}, "height": {"type": "number"}}),
            &["interior", "room", "x", "y"],
        ),
        tool(
            "building_bounds",
            "Mesure le bâtiment du jeu qui entoure un intérieur du projet (ou le centre de la vue) : sa boîte exprimée dans le repère de l'intérieur (x, y min/max, hauteurs), l'emprise des pièces déjà là, et l'EMPRISE EXACTE DES MURS (le modèle coupé à hauteur d'homme : chaque mur de (x, y) à (x, y), sans toit ni débords). Pour ajouter des pièces qui restent DANS le bâtiment : les rectangles de add_room doivent tenir entre ces murs (garder 0,3 m pour leur épaisseur). z : hauteur de la coupe (sinon 1,2 m au-dessus du sol des pièces).",
            json!({"interior": interior, "z": {"type": "number"}}),
            &[],
        ),
        tool(
            "study_game_interior",
            "Étudie un intérieur du jeu de base (nom de find_game_interiors) : ses placements, chaque pièce (nom, taille, sol, ambiance, meubles qu'elle contient), ses passages (portes, fenêtres, sorties). Pour s'inspirer de l'architecture de Rockstar (commissariat, hôpital, appartements…) ou avant de reprendre un intérieur.",
            json!({"name": {"type": "string"}, "objects_per_room": {"type": "integer", "description": "12 par défaut"}}),
            &["name"],
        ),
        tool(
            "copy_game_interior",
            "Reprend un intérieur du jeu dans le projet, à sa place exacte (le placement le plus proche de la vue), et cache celui du jeu : mêmes pièces, mêmes passages, mêmes murs. mode « structure » (par défaut) : murs, sols, plafonds, escaliers, sans les meubles, pour le réaménager ; « full » : tout, meubles compris. C'est la bonne façon de faire un intérieur dans un bâtiment qui en a déjà un.",
            json!({"name": {"type": "string"}, "mode": {"type": "string", "enum": ["structure", "full"]}, "new_name": {"type": "string"}}),
            &["name"],
        ),
        tool(
            "hide_game_interior",
            "Cache un intérieur du jeu (son placement le plus proche de la vue) pour qu'il n'entre pas en conflit avec un intérieur du projet au même endroit ; annulable avec undo.",
            json!({"name": {"type": "string"}}),
            &["name"],
        ),
        tool(
            "select_game_object",
            "Choisit un objet du jeu (bâtiment, etc.) par un mot de son nom, le plus proche du centre de la vue, comme un clic de la personne : il est copié dans le projet et sélectionné (puis analyze_building, building_to_interior…).",
            json!({"name": {"type": "string"}, "radius": {"type": "number", "description": "150 m par défaut"}}),
            &["name"],
        ),
        tool(
            "add_entrance",
            "Perce une entrée vers l'extérieur (portail vers la pièce 0) dans le mur d'une pièce, au point du mur le plus proche de (x, y) (coordonnées locales de l'intérieur). width : largeur (1,2 m), height : hauteur (2,2 m). Ensuite place_doors y met une vraie porte.",
            json!({"interior": interior, "room": {"type": ["integer", "string"]}, "x": {"type": "number"}, "y": {"type": "number"}, "width": {"type": "number"}, "height": {"type": "number"}}),
            &["interior", "room", "x", "y"],
        ),
        tool(
            "split_room",
            "Coupe une pièce rectangulaire en deux par un mur avec une porte : axis « x » coupe à x = at (mur parallèle à Y), « y » coupe à y = at. Les objets et passages de l'autre côté vont à la nouvelle pièce. Pour faire cellules, bureaux, salle d'interrogatoire dans une grande salle.",
            json!({"interior": interior, "room": {"type": ["integer", "string"]}, "axis": {"type": "string", "enum": ["x", "y"]}, "at": {"type": "number"}, "name_a": {"type": "string"}, "name_b": {"type": "string"}}),
            &["interior", "room", "axis", "at"],
        ),
        tool(
            "place_interior",
            "Déplace ou tourne le placement d'un intérieur du projet dans le monde (x y z, cap en degrés).",
            json!({"interior": interior, "x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}, "heading": {"type": "number"}}),
            &["interior"],
        ),
    ]
}

/// Ambiance d'après son hash (catalogue des ambiances du jeu).
fn tc_name(app: &mut DwApp, h: u32) -> String {
    if h == 0 {
        return String::new();
    }
    let list = app.ambiances().clone();
    list.iter().find(|a| crate::gta::crypto::joaat(&a.name.to_lowercase()) == h).map(|a| a.name.clone()).unwrap_or_else(|| format!("hash_{h:08X}"))
}

impl DwApp {
    pub fn mcp_game(&mut self, name: &str, a: &Value) -> Result<Vec<Value>, String> {
        match name {
            "study_game_interior" => self.mcp_study(a),
            "copy_game_interior" => self.mcp_copy_game(a),
            "hide_game_interior" => {
                let (arch, ei, _) = self.game_int_nearest(a["name"].as_str().unwrap_or(""))?;
                self.push_undo();
                let ids: std::collections::HashSet<u64> = self.game_ents_to_project(&[ei]).into_iter().collect();
                let n = self.proj.delete_ids(&ids);
                self.sync_game_project();
                Ok(txt(format!("Intérieur du jeu « {arch} » caché ({n} placement retiré ; il disparaît aussi en jeu une fois exporté).")))
            }
            "select_game_object" => {
                let q = a["name"].as_str().unwrap_or("").trim().to_lowercase();
                let radius = num(a, "radius").unwrap_or(150.0).clamp(5.0, 2000.0);
                let c = self.cam.target;
                let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
                let best = w
                    .ents_in_radius(c, radius)
                    .into_iter()
                    .filter(|&ei| {
                        let e = &w.ents[ei as usize];
                        e.lod_level == 0 && !e.mlo && w.name_of(e.arch).to_lowercase().contains(&q)
                    })
                    .min_by(|&x, &y| (w.ents[x as usize].pos - c).len().partial_cmp(&(w.ents[y as usize].pos - c).len()).unwrap_or(std::cmp::Ordering::Equal))
                    .ok_or(format!("Aucun objet du jeu « {q} » à moins de {radius:.0} m du centre de la vue."))?;
                let nm = w.name_of(w.ents[best as usize].arch);
                let ids = self.game_ents_to_project(&[best]);
                let id = *ids.first().ok_or("Impossible de prendre cet objet.")?;
                self.set_selection(vec![id]);
                Ok(txt(format!("« {nm} » choisi (id {id}). Suite possible : analyze_building, building_to_interior.")))
            }
            "add_entrance" => self.mcp_entrance(a),
            "add_window" => self.mcp_window(a),
            "building_windows" => self.mcp_building_windows(a),
            "repair_game_interior" => self.mcp_repair(a),
            "building_bounds" => self.mcp_bounds(a),
            "split_room" => self.mcp_split(a),
            "place_interior" => {
                let (t, m) = self.find_interior(a["interior"].as_str().unwrap_or(""))?;
                let mname = self.proj.ytyps[t].mlos[m].name.clone();
                let id = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.mlo.is_some() && e.archetype.eq_ignore_ascii_case(&mname)).map(|e| e.id).ok_or("Cet intérieur n'est pas placé.")?;
                let r = self.proj.find_id(id).ok_or("placement introuvable")?;
                self.push_undo();
                if let Some(e) = self.proj.ent_mut(r) {
                    e.pos = v3(num(a, "x").unwrap_or(e.pos.x), num(a, "y").unwrap_or(e.pos.y), num(a, "z").unwrap_or(e.pos.z));
                    if let Some(h) = num(a, "heading") {
                        e.rot = Quat::from_z(h.to_radians());
                    }
                }
                self.sync_game_project();
                let e = self.proj.ent(r).cloned().ok_or("placement introuvable")?;
                Ok(txt(format!("« {mname} » placé en ({:.2}, {:.2}, {:.2}), cap {:.0}°.", e.pos.x, e.pos.y, e.pos.z, e.rot.heading_deg())))
            }
            _ => Err(format!("Outil inconnu : {name}")),
        }
    }

    /// Vrai sol d'une pièce reprise du jeu (repère de l'intérieur) : la boîte des pièces de Rockstar descend sous le
    /// sol ; on lance des rayons sur les modèles de la pièce (médiane de quelques points).
    pub(crate) fn game_room_floor(&mut self, t: usize, m: usize, ri: usize) -> Option<f32> {
        let mlo = &self.proj.ytyps[t].mlos[m];
        let r = mlo.rooms.get(ri)?.clone();
        let (p, q) = self.proj.mlo_placement(&mlo.name)?;
        let skip = std::collections::HashSet::new();
        let mut hits = Vec::new();
        for (fx, fy) in [(0.5, 0.5), (0.3, 0.3), (0.7, 0.3), (0.3, 0.7), (0.7, 0.7), (0.5, 0.2), (0.5, 0.8)] {
            let l = v3(r.bb_min.x + (r.bb_max.x - r.bb_min.x) * fx, r.bb_min.y + (r.bb_max.y - r.bb_min.y) * fy, r.bb_min.z + 1.0);
            let w = p + q.rotate(l);
            if let Some(z) = self.ground_below(w.x, w.y, w.z, 1.6, &skip) {
                hits.push(q.inverse().rotate(v3(w.x, w.y, z) - p).z);
            }
        }
        if hits.is_empty() {
            return None;
        }
        hits.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(hits[hits.len() / 2])
    }

    /// Sol d'une pièce reprise du jeu : la texture de la plus grande surface horizontale au niveau de son vrai sol
    /// (objets de la pièce), devenue texture du projet (PNG, au plus 256 px) avec la taille réelle de son motif.
    /// Rend le numéro de sol de la coque modelable (CUSTOM_BASE + rang) et le nom de la texture.
    pub(crate) fn game_floor_texture(&mut self, t: usize, m: usize, gi: usize) -> Option<(u8, String)> {
        let floor = self.game_room_floor(t, m, gi)?;
        let w = self.game.world.as_ref()?;
        let mlo = &self.proj.ytyps[t].mlos[m];
        let r = mlo.rooms.get(gi)?.clone();
        let ents: Vec<crate::model::Entity> = mlo.entities.iter().filter(|e| e.room == gi || e.room == 0).cloned().collect();
        let src = w.model_source();
        let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
        let mut cache = Vec::new();
        // clé de texture → (surface, longueur dans les UV, longueur dans le monde, texture)
        let mut best: std::collections::HashMap<u64, (f32, f32, f32, Option<crate::gta::drawable::TexData>)> = std::collections::HashMap::new();
        for e in ents.iter().take(200) {
            let l = src.load(crate::gta::crypto::joaat(&e.archetype.to_lowercase()), &claimed, &mut cache);
            let Some(model) = l.model.as_ref() else { continue };
            let sc = v3(e.scale_xy, e.scale_xy, e.scale_z);
            for (gk, g) in model.geoms.iter().enumerate() {
                let key = l.part_keys.get(gk).copied().unwrap_or(0);
                if key == 0 || g.blend != 0 || g.glass {
                    continue;
                }
                let end = ((g.first + g.count) as usize).min(model.idx.len());
                for tri in model.idx[g.first as usize..end].chunks_exact(3) {
                    let v = [0, 1, 2].map(|j| model.verts[tri[j] as usize]);
                    let p = v.map(|x| e.pos + e.rot.rotate(v3(x.pos[0], x.pos[1], x.pos[2]).mulv(sc)));
                    let n = (p[1] - p[0]).cross(p[2] - p[0]);
                    let area = n.len() * 0.5;
                    if area < 1e-4 || n.z / (2.0 * area) < 0.9 {
                        continue;
                    }
                    let c = (p[0] + p[1] + p[2]) * (1.0 / 3.0);
                    if (c.z - floor).abs() > 0.4 || c.x < r.bb_min.x || c.x > r.bb_max.x || c.y < r.bb_min.y || c.y > r.bb_max.y {
                        continue;
                    }
                    let duv = ((v[1].uv[0] - v[0].uv[0]).powi(2) + (v[1].uv[1] - v[0].uv[1]).powi(2)).sqrt();
                    let dp = (p[1] - p[0]).len();
                    let ent = best.entry(key).or_insert((0.0, 0.0, 0.0, None));
                    ent.0 += area;
                    ent.1 += duv;
                    ent.2 += dp;
                    if ent.3.is_none() {
                        ent.3 = l.textures.iter().find(|x| x.0 == key).map(|x| x.1.clone());
                    }
                }
            }
        }
        let (key, (_, uvl, wl, tex)) = best.into_iter().filter(|b| b.1 .3.is_some()).max_by(|a, b| a.1 .0.total_cmp(&b.1 .0))?;
        let img = crate::texdecode::decode_level(&tex?, 256)?;
        let rgba = image::RgbaImage::from_raw(img.w as u32, img.h as u32, img.px)?;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(rgba).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
        // taille d'un motif : longueur dans le monde pour une unité d'UV
        let size = if uvl > 1e-4 { (wl / uvl).clamp(0.2, 8.0) } else { 1.0 };
        let name = format!("sol_jeu_{key:08x}");
        let i = match self.proj.custom_textures.iter().position(|c| c.name == name) {
            Some(i) => i,
            None if self.proj.custom_textures.len() < 150 => {
                self.proj.custom_textures.push(crate::design::CustomTex { name: name.clone(), png: crate::design::b64_encode(&png), size });
                self.proj.custom_textures.len() - 1
            }
            None => return None,
        };
        Some((crate::design::CUSTOM_BASE + i as u8, name))
    }

    /// Intérieur du jeu par son nom : (nom exact, index du placement le plus proche de la vue, définition).
    fn game_int_nearest(&self, q: &str) -> Result<(String, u32, std::sync::Arc<crate::gta::world::MloDef>), String> {
        let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
        let q = q.trim().to_lowercase();
        let c = self.cam.target;
        let mut best: Option<(f32, u32, u32)> = None;
        for (i, e) in w.ents.iter().enumerate().filter(|(_, e)| e.mlo) {
            let n = w.name_of(e.arch).to_lowercase();
            if n != q && !(q.len() >= 4 && n.contains(&q)) {
                continue;
            }
            let d = (e.pos - c).len() + if n == q { 0.0 } else { 1e6 };
            if best.map(|b| d < b.0).unwrap_or(true) {
                best = Some((d, i as u32, e.arch));
            }
        }
        let (_, ei, arch) = best.ok_or(format!("Intérieur du jeu « {q} » introuvable (voir find_game_interiors)."))?;
        let def = w.mlo_defs.get(&arch).cloned().ok_or("Définition de cet intérieur introuvable dans le jeu chargé.")?;
        Ok((w.name_of(arch), ei, def))
    }

    fn mcp_study(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let (name, ei, def) = self.game_int_nearest(a["name"].as_str().unwrap_or(""))?;
        let per = a["objects_per_room"].as_u64().unwrap_or(12).clamp(0, 60) as usize;
        let (places, names): (Vec<String>, Vec<String>) = {
            let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
            let arch = w.ents[ei as usize].arch;
            let places = w.ents.iter().filter(|e| e.mlo && e.arch == arch).take(8).map(|e| format!("({:.1}, {:.1}, {:.1}) cap {:.0}°", e.pos.x, e.pos.y, e.pos.z, e.rot().heading_deg())).collect();
            let names = def.children.iter().map(|c| w.name_of(c.arch)).collect();
            (places, names)
        };
        let mut s = format!("Intérieur du jeu « {name} » : {} pièce(s), {} passage(s), {} objet(s), {} groupe(s) d'objets.\nPlacements : {}\n", def.rooms.len().saturating_sub(1), def.portals.len(), def.children.len(), def.sets.len(), places.join(", "));
        s += "Pièces (repère de l'intérieur) :\n";
        for (i, r) in def.rooms.iter().enumerate().skip(1) {
            let tc = tc_name(self, r.timecycle);
            let mut counts: Vec<(String, usize)> = Vec::new();
            for &o in &r.objects {
                let Some(n) = names.get(o as usize) else { continue };
                match counts.iter_mut().find(|c| &c.0 == n) {
                    Some(c) => c.1 += 1,
                    None => counts.push((n.clone(), 1)),
                }
            }
            counts.sort_by(|x, y| y.1.cmp(&x.1));
            let list: Vec<String> = counts.iter().take(per).map(|(n, k)| if *k > 1 { format!("{n} ×{k}") } else { n.clone() }).collect();
            s += &format!(
                "  {i} « {} » : x {:.2}..{:.2}, y {:.2}..{:.2} ({:.1} × {:.1} m), sol z {:.2}, hauteur {:.1}, ambiance « {tc} », {} objet(s) : {}\n",
                r.name,
                r.bb_min[0],
                r.bb_max[0],
                r.bb_min[1],
                r.bb_max[1],
                r.bb_max[0] - r.bb_min[0],
                r.bb_max[1] - r.bb_min[1],
                r.bb_min[2],
                r.bb_max[2] - r.bb_min[2],
                r.objects.len(),
                list.join(", ")
            );
        }
        s += "Passages :\n";
        for p in &def.portals {
            let zs: Vec<f32> = p.corners.iter().map(|c| c[2]).collect();
            let (z0, z1) = (zs.iter().cloned().fold(f32::MAX, f32::min), zs.iter().cloned().fold(f32::MIN, f32::max));
            let w = p.corners.windows(2).map(|c| (c[0][0] - c[1][0]).hypot(c[0][1] - c[1][1])).fold(0.0, f32::max);
            let rn = |i: u32| def.rooms.get(i as usize).map(|r| if i == 0 { "dehors".to_string() } else { r.name.clone() }).unwrap_or_default();
            let kind = if z1 - z0 < 0.1 { "trémie (escalier)" } else if p.flags & 0x2 != 0 { "miroir" } else { "ouverture" };
            s += &format!("  {} ↔ {} : {kind} {:.1} × {:.1} m\n", rn(p.from), rn(p.to), w, z1 - z0);
        }
        Ok(txt(s))
    }

    fn mcp_copy_game(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let (gname, ei, def) = self.game_int_nearest(a["name"].as_str().unwrap_or(""))?;
        if def.rooms.len() < 2 {
            return Err("Cet intérieur du jeu n'a pas de pièces lisibles.".into());
        }
        let full = a["mode"].as_str() == Some("full");
        let new_name = a["new_name"].as_str().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).unwrap_or_else(|| format!("{gname}_dw"));
        if self.proj.find_mlo(&new_name).is_some() {
            return Err(format!("Un intérieur « {new_name} » existe déjà dans le projet."));
        }
        // objets : (modèle, position, orientation, échelles, pièce)
        let children: Vec<(String, V3, Quat, f32, f32, usize, bool, u32)> = {
            let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
            let mut room_of = vec![0usize; def.children.len()];
            for (ri, r) in def.rooms.iter().enumerate() {
                for &o in &r.objects {
                    if let Some(x) = room_of.get_mut(o as usize) {
                        *x = ri;
                    }
                }
            }
            def.children
                .iter()
                .enumerate()
                .filter(|(_, c)| c.set < 0)
                .map(|(i, c)| {
                    let n = w.name_of(c.arch);
                    let (mn, mx) = w.arch_box(c.arch);
                    let furniture = crate::meubler::furn_cats(&n, mx - mn) != 0 && (mx - mn).len() < 6.0;
                    (n, c.pos, c.rot, c.sxy, c.sz, room_of[i], furniture, c.flags)
                })
                .collect()
        };
        self.push_undo();
        // placement du jeu : copié dans le projet (pour son orientation exacte), puis retiré (caché en jeu)
        let ids = self.game_ents_to_project(&[ei]);
        let inst = ids.first().and_then(|id| self.proj.find_id(*id)).and_then(|r| self.proj.ent(r).cloned()).ok_or("Placement du jeu introuvable.")?;
        let ids: std::collections::HashSet<u64> = ids.into_iter().collect();
        self.proj.delete_ids(&ids);
        let mut mlo = crate::model::MloArchetype { name: new_name.clone(), lod_dist: 200.0, flags: 0, mlo_flags: 0, entities: Vec::new(), rooms: Vec::new(), portals: Vec::new(), entity_sets: Vec::new() };
        for r in def.rooms.iter() {
            let mut room = crate::model::Room::new(if r.name.is_empty() { "limbo" } else { &r.name }, v3(r.bb_min[0], r.bb_min[1], r.bb_min[2]), v3(r.bb_max[0], r.bb_max[1], r.bb_max[2]));
            room.timecycle = tc_name(self, r.timecycle);
            room.flags = r.flags;
            room.floor_id = r.floor_id.max(0) as u32;
            room.game = true;
            mlo.rooms.push(room);
        }
        for p in &def.portals {
            if p.corners.len() < 4 {
                continue;
            }
            let c = |i: usize| v3(p.corners[i][0], p.corners[i][1], p.corners[i][2]);
            let mut po = crate::model::Portal::new(p.from, p.to, [c(0), c(1), c(2), c(3)]);
            po.flags = p.flags;
            mlo.portals.push(po);
        }
        let (mut kept, mut left) = (0, 0);
        for (n, pos, rot, sxy, sz, room, furniture, flags) in children {
            if furniture && !full {
                left += 1;
                continue;
            }
            let mut e = crate::model::Entity::new(&n, pos);
            e.rot = rot;
            e.scale_xy = sxy;
            e.scale_z = sz;
            e.flags = if flags != 0 { flags } else { crate::mech::MLO_ENTITY_FLAGS };
            e.room = room;
            mlo.entities.push(e);
            kept += 1;
        }
        let exits = mlo.portals.iter().filter(|p| p.room_to == 0 || p.room_from == 0).count() as u32;
        let rooms: Vec<String> = mlo.rooms.iter().enumerate().skip(1).map(|(i, r)| format!("{i}:{}", r.name)).collect();
        let ti = self.own_ytyp();
        self.proj.ytyps[ti].mlos.push(mlo);
        let yi = self.ensure_target_ymap();
        let mut e = crate::model::Entity::new(&new_name, inst.pos);
        e.rot = inst.rot;
        e.mlo = Some(crate::model::MloInstData { num_exit_portals: exits, ..Default::default() });
        e.flags = crate::mech::MLO_INSTANCE_FLAGS;
        e.lod_dist = inst.lod_dist.max(200.0);
        self.proj.ymaps[yi].entities.push(e);
        let mi = self.proj.ytyps[ti].mlos.len() - 1;
        self.cur_mlo = Some((ti, mi));
        self.sync_game_project();
        // collision de l'intérieur du jeu (fichier du même nom) : rattachée au nouvel intérieur à l'export
        let gl = gname.to_lowercase();
        let has_ybn = self.game.world.as_ref().map(|w| w.vfs.files.iter().any(|f| f.name.eq_ignore_ascii_case(&format!("{gl}.ybn")))).unwrap_or(false);
        let col = if has_ybn {
            self.proj.mlo_bounds.retain(|(n, _)| !n.eq_ignore_ascii_case(&new_name));
            self.proj.mlo_bounds.push((new_name.clone(), vec![gl.clone()]));
            format!("Collision d'origine « {gl} » rattachée (murs, sols, comptoirs de Rockstar).")
        } else {
            "Collision d'origine introuvable : seuls les sols seront solides (à vérifier en jeu).".to_string()
        };
        // vrai sol de chaque pièce (pour que le jeu sache dans quelle pièce est le joueur)
        let nr = self.proj.ytyps[ti].mlos[mi].rooms.len();
        for ri in 1..nr {
            if let Some(f) = self.game_room_floor(ti, mi, ri) {
                self.proj.ytyps[ti].mlos[mi].rooms[ri].floor_z = Some(f);
            }
        }
        Ok(txt(format!(
            "{col}\n« {gname} » repris dans le projet sous le nom « {new_name} », à sa place exacte ({:.2}, {:.2}, {:.2}), cap {:.0}° ; l'intérieur du jeu est caché.\n{kept} objet(s) de structure gardé(s){}.\nPièces : {}.\nSuite : get_interior, rename_room / split_room, furnish_room avec keep_existing=true (la structure reste), place_doors, check_project.",
            inst.pos.x,
            inst.pos.y,
            inst.pos.z,
            inst.rot.heading_deg(),
            if full { String::new() } else { format!(", {left} meuble(s) laissé(s) de côté") },
            rooms.join(", ")
        )))
    }

    fn mcp_bounds(&self, a: &Value) -> Result<Vec<Value>, String> {
        let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
        // repère : celui de l'intérieur (s'il est donné et placé), sinon le monde autour du centre de la vue
        let (origin, rot, rooms) = match a["interior"].as_str() {
            Some(n) => {
                let (t, m) = self.find_interior(n)?;
                let mlo = &self.proj.ytyps[t].mlos[m];
                let (p, q) = self.proj.mlo_placement(&mlo.name).ok_or("Cet intérieur n'est pas placé.")?;
                (p, q, Some(mlo.rooms.clone()))
            }
            None => (self.cam.target, Quat::IDENTITY, None),
        };
        let inv = rot.inverse();
        let mut s = String::new();
        if let Some(rooms) = &rooms {
            let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
            for r in rooms.iter().skip(1) {
                mn = v3(mn.x.min(r.bb_min.x), mn.y.min(r.bb_min.y), mn.z.min(r.bb_min.z));
                mx = v3(mx.x.max(r.bb_max.x), mx.y.max(r.bb_max.y), mx.z.max(r.bb_max.z));
            }
            if mn.x < mx.x {
                s += &format!("Pièces actuelles de l'intérieur : x {:.2}..{:.2}, y {:.2}..{:.2}, z {:.2}..{:.2} (repère de l'intérieur).\n", mn.x, mx.x, mn.y, mx.y, mn.z, mx.z);
            }
        }
        // bâtiments du jeu (modèles de plus de 6 m) dont la boîte contient le point
        let mut found: Vec<(f32, String)> = Vec::new();
        let mut biggest: Option<(f32, u32, V3, Quat, f32, f32)> = None;
        for ei in w.ents_in_radius(origin, 80.0) {
            let e = &w.ents[ei as usize];
            if e.mlo || e.lod_level != 0 {
                continue;
            }
            let (bmn, bmx) = w.arch_box(e.arch);
            let size = bmx - bmn;
            if size.x.max(size.y) < 6.0 || size.z < 2.5 {
                continue;
            }
            let er = e.rot();
            let lp = er.inverse().rotate(origin - e.pos);
            let (sxy, sz) = (e.sxy(), e.sz());
            if lp.x < bmn.x * sxy - 0.5 || lp.x > bmx.x * sxy + 0.5 || lp.y < bmn.y * sxy - 0.5 || lp.y > bmx.y * sxy + 0.5 {
                continue;
            }
            // coins de la boîte du bâtiment, dans le repère de l'intérieur
            let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
            for i in 0..8 {
                let c = v3(if i & 1 == 0 { bmn.x } else { bmx.x } * sxy, if i & 2 == 0 { bmn.y } else { bmx.y } * sxy, if i & 4 == 0 { bmn.z } else { bmx.z } * sz);
                let wpt = e.pos + er.rotate(c);
                let l = inv.rotate(wpt - origin);
                mn = v3(mn.x.min(l.x), mn.y.min(l.y), mn.z.min(l.z));
                mx = v3(mx.x.max(l.x), mx.y.max(l.y), mx.z.max(l.z));
            }
            let dyaw = (er.heading_deg() - rot.heading_deg()).rem_euclid(90.0);
            let aligned = dyaw < 2.0 || dyaw > 88.0;
            if biggest.map(|b| size.x * size.y > b.0).unwrap_or(true) {
                biggest = Some((size.x * size.y, e.arch, e.pos, er, sxy, sz));
            }
            found.push((
                size.x * size.y,
                format!(
                    "« {} » ({:.1} × {:.1} × {:.1} m) : x {:.2}..{:.2}, y {:.2}..{:.2}, z {:.2}..{:.2}{}",
                    w.name_of(e.arch),
                    size.x * sxy,
                    size.y * sxy,
                    size.z * sz,
                    mn.x,
                    mx.x,
                    mn.y,
                    mx.y,
                    mn.z,
                    mx.z,
                    if aligned { "" } else { " (bâtiment pas aligné sur l'intérieur : boîte englobante, plus grande que le bâtiment)" }
                ),
            ));
        }
        found.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
        if found.is_empty() {
            s += "Aucun bâtiment du jeu ne contient ce point.";
        } else {
            s += &format!("Bâtiment(s) du jeu autour (boîte dans le repère {}) :\n", if rooms.is_some() { "de l'intérieur" } else { "du monde, centrée sur la vue" });
            for (_, l) in found.iter().take(5) {
                s += &format!("  {l}\n");
            }
            s += "La boîte du modèle inclut toit, auvents et débords : les murs exacts sont mesurés ci-dessous.\n";
            // emprise exacte : le modèle coupé à hauteur d'homme (1,2 m au-dessus du sol des pièces, sinon 1,5 m
            // au-dessus du bas du bâtiment), murs à peu près verticaux seulement
            if let Some((_, arch, bpos, brot, bsxy, bsz)) = biggest {
                let src = w.model_source();
                let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
                let mut cache = Vec::new();
                if let Some(model) = src.load(arch, &claimed, &mut cache).model {
                    let an = crate::analyse::analyse(0, &w.name_of(arch), &model, bpos, brot, bsxy, bsz);
                    let local: Vec<[V3; 3]> = an.tris.iter().map(|(t, _)| t.map(|p| inv.rotate(p - origin))).collect();
                    let zmin = local.iter().flat_map(|t| t.iter()).fold(f32::MAX, |m, p| m.min(p.z));
                    let room_floor = rooms.as_ref().and_then(|r| r.iter().skip(1).map(|r| r.floor_z.unwrap_or(r.bb_min.z)).reduce(f32::min));
                    let zs = a["z"].as_f64().map(|v| v as f32).unwrap_or_else(|| room_floor.map(|f| f + 1.2).unwrap_or(zmin + 1.5));
                    let walls = footprint_walls(&local, zs);
                    if walls.is_empty() {
                        s += &format!("Aucun mur coupé à z {zs:.2} (essaie un autre « z »).");
                    } else {
                        let (mut a0, mut a1) = ([f32::MAX; 2], [f32::MIN; 2]);
                        for (p, q) in &walls {
                            for v in [p, q] {
                                a0 = [a0[0].min(v[0]), a0[1].min(v[1])];
                                a1 = [a1[0].max(v[0]), a1[1].max(v[1])];
                            }
                        }
                        s += &format!("Emprise exacte des murs (coupe à z {zs:.2}) : x {:.2}..{:.2}, y {:.2}..{:.2} ({:.1} × {:.1} m).\nMurs (faces, de (x, y) à (x, y), les plus longs d'abord) :\n", a0[0], a1[0], a0[1], a1[1], a1[0] - a0[0], a1[1] - a0[1]);
                        for (p, q) in walls.iter().take(24) {
                            let l = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
                            s += &format!("  ({:.2}, {:.2}) → ({:.2}, {:.2})  {:.1} m\n", p[0], p[1], q[0], q[1], l);
                        }
                        s += "Pour rester dedans : garde 0,3 m entre ces murs (les deux faces d'un même mur sont deux lignes parallèles proches).";
                    }
                }
            }
        }
        Ok(txt(s))
    }

    fn mcp_repair(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let (t, m) = self.find_interior(a["interior"].as_str().unwrap_or(""))?;
        let mname = self.proj.ytyps[t].mlos[m].name.clone();
        let (p, _) = self.proj.mlo_placement(&mname).ok_or("Cet intérieur n'est pas placé.")?;
        // intérieur du jeu d'origine : donné, sinon celui placé au même endroit
        let gname = match a["game_name"].as_str().filter(|s| !s.trim().is_empty()) {
            Some(n) => n.trim().to_lowercase(),
            None => {
                let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
                w.ents.iter().filter(|e| e.mlo && (e.pos - p).len() < 3.0).map(|e| w.name_of(e.arch).to_lowercase()).next().ok_or("Aucun intérieur du jeu à cet endroit : donne game_name.")?
            }
        };
        let def = {
            let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
            w.mlo_defs.get(&crate::gta::crypto::joaat(&gname)).cloned().ok_or(format!("Intérieur du jeu « {gname} » introuvable."))?
        };
        self.push_undo();
        let mut s = String::new();
        let has_ybn = self.game.world.as_ref().map(|w| w.vfs.files.iter().any(|f| f.name.eq_ignore_ascii_case(&format!("{gname}.ybn")))).unwrap_or(false);
        if has_ybn {
            self.proj.mlo_bounds.retain(|(n, _)| !n.eq_ignore_ascii_case(&mname));
            self.proj.mlo_bounds.push((mname.clone(), vec![gname.clone()]));
            s += &format!("Collision d'origine « {gname} » rattachée.\n");
        }
        // pièces du jeu : celles dont la boîte est celle d'une pièce d'origine (ou contenue dedans, pièce recoupée)
        let mut n_game = 0;
        {
            let mlo = &mut self.proj.ytyps[t].mlos[m];
            for r in mlo.rooms.iter_mut().skip(1) {
                let inside = def.rooms.iter().skip(1).any(|g| r.bb_min.x >= g.bb_min[0] - 0.05 && r.bb_max.x <= g.bb_max[0] + 0.05 && r.bb_min.y >= g.bb_min[1] - 0.05 && r.bb_max.y <= g.bb_max[1] + 0.05 && (r.bb_min.z - g.bb_min[2]).abs() < 0.1);
                if inside {
                    r.game = true;
                    n_game += 1;
                }
            }
        }
        let nr = self.proj.ytyps[t].mlos[m].rooms.len();
        let mut floors = 0;
        for ri in 1..nr {
            if self.proj.ytyps[t].mlos[m].rooms[ri].game {
                if let Some(f) = self.game_room_floor(t, m, ri) {
                    self.proj.ytyps[t].mlos[m].rooms[ri].floor_z = Some(f);
                    floors += 1;
                }
            }
        }
        // réglages d'origine des objets de Rockstar (même modèle, même place)
        let originals: Vec<(u32, V3, u32)> = def.children.iter().map(|c| (c.arch, c.pos, c.flags)).collect();
        let mut fixed = 0;
        for e in self.proj.ytyps[t].mlos[m].entities.iter_mut() {
            let h = crate::gta::crypto::joaat(&e.archetype.to_lowercase());
            if let Some(o) = originals.iter().find(|o| o.0 == h && (o.1 - e.pos).len() < 0.05 && o.2 != 0) {
                if e.flags != o.2 {
                    e.flags = o.2;
                    fixed += 1;
                }
            }
        }
        s += &format!("{n_game} pièce(s) reconnue(s) comme pièces du jeu, vrai sol mesuré pour {floors} ; {fixed} objet(s) remis à leurs réglages d'origine. Réexporte (Tester en jeu) pour en profiter.");
        Ok(txt(s))
    }

    /// Bâtiment du jeu (le plus grand) qui contient le placement d'un intérieur : (modèle chargé, position, orientation, échelles).
    fn building_around(&self, origin: V3) -> Option<(u32, V3, Quat, f32, f32)> {
        let w = self.game.world.as_ref()?;
        let mut best: Option<(f32, u32, V3, Quat, f32, f32)> = None;
        for ei in w.ents_in_radius(origin, 80.0) {
            let e = &w.ents[ei as usize];
            if e.mlo || e.lod_level != 0 {
                continue;
            }
            let (bmn, bmx) = w.arch_box(e.arch);
            let size = bmx - bmn;
            if size.x.max(size.y) < 6.0 || size.z < 2.5 {
                continue;
            }
            let lp = e.rot().inverse().rotate(origin - e.pos);
            if lp.x < bmn.x * e.sxy() - 0.5 || lp.x > bmx.x * e.sxy() + 0.5 || lp.y < bmn.y * e.sxy() - 0.5 || lp.y > bmx.y * e.sxy() + 0.5 {
                continue;
            }
            let area = size.x * size.y;
            if best.map(|b| area > b.0).unwrap_or(true) {
                best = Some((area, e.arch, e.pos, e.rot(), e.sxy(), e.sz()));
            }
        }
        best.map(|b| (b.1, b.2, b.3, b.4, b.5))
    }

    fn mcp_building_windows(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let (t, m) = self.find_interior(a["interior"].as_str().unwrap_or(""))?;
        let mname = self.proj.ytyps[t].mlos[m].name.clone();
        let (origin, rot) = self.proj.mlo_placement(&mname).ok_or("Cet intérieur n'est pas placé.")?;
        let floor = self.proj.ytyps[t].mlos[m].rooms.iter().skip(1).map(|r| r.floor_z.unwrap_or(r.bb_min.z)).fold(f32::MAX, f32::min);
        let (arch, pos, brot, sxy, sz) = self.building_around(origin).ok_or("Aucun bâtiment du jeu autour de cet intérieur.")?;
        let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
        let bname = w.name_of(arch);
        // le bâtiment et les objets du jeu posés sur lui (les fenêtres sont souvent des modèles à part)
        let (bmn, bmx) = w.arch_box(arch);
        let mut parts: Vec<(u32, V3, Quat, f32, f32)> = vec![(arch, pos, brot, sxy, sz)];
        for ei in w.ents_in_radius(origin, 60.0) {
            let e = &w.ents[ei as usize];
            if e.mlo || e.lod_level != 0 || e.arch == arch {
                continue;
            }
            let (mn, mx) = w.arch_box(e.arch);
            if (mx - mn).len() > 30.0 {
                continue;
            }
            let lp = brot.inverse().rotate(e.pos - pos);
            if lp.x < bmn.x * sxy - 3.0 || lp.x > bmx.x * sxy + 3.0 || lp.y < bmn.y * sxy - 3.0 || lp.y > bmx.y * sxy + 3.0 {
                continue;
            }
            parts.push((e.arch, e.pos, e.rot(), e.sxy(), e.sz()));
        }
        let src = w.model_source();
        let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
        let mut cache = Vec::new();
        let mut glass: Vec<[V3; 3]> = Vec::new();
        for (pa, pp, pr, psxy, psz) in parts.iter().take(400) {
            let l = src.load(*pa, &claimed, &mut cache);
            let Some(model) = l.model else { continue };
            if !model.geoms.iter().any(|g| g.glass) {
                continue;
            }
            let an = crate::analyse::analyse(0, &bname, &model, *pp, *pr, *psxy, *psz);
            glass.extend(an.tris.iter().filter(|(_, k)| *k == crate::analyse::K_GLASS).map(|(t, _)| *t));
        }
        let inv = rot.inverse();
        // vitres dans le repère de l'intérieur, regroupées par plan (normale, distance) puis par voisinage
        let mut tris: Vec<([V3; 3], V3)> = Vec::new();
        for tr in &glass {
            let l3 = [inv.rotate(tr[0] - origin), inv.rotate(tr[1] - origin), inv.rotate(tr[2] - origin)];
            let n = (l3[1] - l3[0]).cross(l3[2] - l3[0]);
            if n.len() < 1e-6 {
                continue;
            }
            let n = n * (1.0 / n.len());
            if n.z.abs() > 0.3 {
                continue; // verrière, pas une fenêtre de mur
            }
            tris.push((l3, n));
        }
        let mut panels: Vec<(V3, V3, V3)> = Vec::new(); // normale, min, max
        for (tr, n) in &tris {
            let (mut mn, mut mx) = (tr[0], tr[0]);
            for p in tr {
                mn = v3(mn.x.min(p.x), mn.y.min(p.y), mn.z.min(p.z));
                mx = v3(mx.x.max(p.x), mx.y.max(p.y), mx.z.max(p.z));
            }
            let near = panels.iter_mut().find(|(pn, pmn, pmx)| pn.dot(*n).abs() > 0.95 && mn.x <= pmx.x + 0.3 && mx.x >= pmn.x - 0.3 && mn.y <= pmx.y + 0.3 && mx.y >= pmn.y - 0.3 && mn.z <= pmx.z + 0.3 && mx.z >= pmn.z - 0.3);
            match near {
                Some((_, pmn, pmx)) => {
                    *pmn = v3(pmn.x.min(mn.x), pmn.y.min(mn.y), pmn.z.min(mn.z));
                    *pmx = v3(pmx.x.max(mx.x), pmx.y.max(mx.y), pmx.z.max(mx.z));
                }
                None => panels.push((*n, mn, mx)),
            }
        }
        // seulement les vitres à hauteur de l'intérieur (pas les étages du dessus)
        let rooms_top = self.proj.ytyps[t].mlos[m].rooms.iter().skip(1).map(|r| r.bb_max.z).fold(f32::MIN, f32::max);
        panels.retain(|(_, mn, mx)| mx.z > floor + 0.3 && mn.z < rooms_top - 0.2 && (mx.x - mn.x).max(mx.y - mn.y) > 0.25);
        if panels.is_empty() {
            return Ok(txt(format!("Aucune vitre à la hauteur de l'intérieur sur « {bname} » et les {} objet(s) du jeu posés dessus ({} triangle(s) de verre en tout) : ses fenêtres sont peintes sur la façade ou à un autre étage. Pas de fenêtre à percer.", parts.len().saturating_sub(1), glass.len())));
        }
        let mut s = format!("Vitres de « {bname} » à hauteur de l'intérieur (repère de l'intérieur, sol à z {floor:.2}) :\n");
        for (n, mn, mx) in panels.iter().take(40) {
            let c = (*mn + *mx) * 0.5;
            let width = if n.x.abs() > n.y.abs() { mx.y - mn.y } else { mx.x - mn.x };
            let side = if n.x.abs() > n.y.abs() { if c.x > 0.0 { "est (x max)" } else { "ouest (x min)" } } else if c.y > 0.0 { "nord (y max)" } else { "sud (y min)" };
            s += &format!("  mur {side} : milieu ({:.2}, {:.2}), largeur {width:.2}, allège {:.2}, hauteur {:.2}\n", c.x, c.y, mn.z - floor, mx.z - mn.z);
        }
        Ok(txt(s))
    }

    fn mcp_window(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let (t, m) = self.find_interior(a["interior"].as_str().unwrap_or(""))?;
        let ri = self.room_arg(t, m, &a["room"])?;
        let r = self.proj.ytyps[t].mlos[m].rooms[ri].clone();
        let (px, py) = (num(a, "x").unwrap_or(0.0), num(a, "y").unwrap_or(0.0));
        let floor = r.floor_z.unwrap_or(r.bb_min.z);
        let w = num(a, "width").unwrap_or(1.2).clamp(0.3, 6.0);
        let sill = num(a, "sill").unwrap_or(0.9).clamp(0.0, 3.0);
        let h = num(a, "height").unwrap_or(1.2).clamp(0.3, (r.bb_max.z - floor - sill - 0.05).max(0.3));
        let d = [(px - r.bb_min.x).abs(), (r.bb_max.x - px).abs(), (py - r.bb_min.y).abs(), (r.bb_max.y - py).abs()];
        let side = (0..4).min_by(|&i, &j| d[i].partial_cmp(&d[j]).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(0);
        let (z0, z1) = (floor + sill, floor + sill + h);
        let corners = if side < 2 {
            let x = if side == 0 { r.bb_min.x } else { r.bb_max.x };
            let c = py.clamp(r.bb_min.y + w * 0.5 + 0.05, r.bb_max.y - w * 0.5 - 0.05);
            [v3(x, c - w * 0.5, z0), v3(x, c - w * 0.5, z1), v3(x, c + w * 0.5, z1), v3(x, c + w * 0.5, z0)]
        } else {
            let y = if side == 2 { r.bb_min.y } else { r.bb_max.y };
            let c = px.clamp(r.bb_min.x + w * 0.5 + 0.05, r.bb_max.x - w * 0.5 - 0.05);
            [v3(c - w * 0.5, y, z0), v3(c - w * 0.5, y, z1), v3(c + w * 0.5, y, z1), v3(c + w * 0.5, y, z0)]
        };
        self.push_undo();
        let mname = {
            let mlo = &mut self.proj.ytyps[t].mlos[m];
            mlo.portals.push(crate::model::Portal::new(ri as u32, 0, corners));
            mlo.name.clone()
        };
        let exits = self.proj.ytyps[t].mlos[m].portals.iter().filter(|p| p.room_to == 0 || p.room_from == 0).count() as u32;
        for y in &mut self.proj.ymaps {
            for e in &mut y.entities {
                if e.archetype.eq_ignore_ascii_case(&mname) {
                    if let Some(dd) = e.mlo.as_mut() {
                        dd.num_exit_portals = exits;
                    }
                }
            }
        }
        let wall = ["ouest (x min)", "est (x max)", "sud (y min)", "nord (y max)"][side];
        Ok(txt(format!("Fenêtre de {w:.2} × {h:.2} m (allège {sill:.2}) percée dans le mur {wall} de la pièce {ri} « {} » (portail vers le dehors : on voit et on entend la rue à travers).", r.name)))
    }

    fn mcp_entrance(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let (t, m) = self.find_interior(a["interior"].as_str().unwrap_or(""))?;
        let ri = self.room_arg(t, m, &a["room"])?;
        let r = self.proj.ytyps[t].mlos[m].rooms[ri].clone();
        let (px, py) = (num(a, "x").unwrap_or(0.0), num(a, "y").unwrap_or(0.0));
        let w = num(a, "width").unwrap_or(1.2).clamp(0.6, 4.0);
        let h = num(a, "height").unwrap_or(2.2).clamp(1.8, (r.bb_max.z - r.bb_min.z).max(1.9));
        // mur le plus proche du point
        let d = [(px - r.bb_min.x).abs(), (r.bb_max.x - px).abs(), (py - r.bb_min.y).abs(), (r.bb_max.y - py).abs()];
        let side = (0..4).min_by(|&i, &j| d[i].partial_cmp(&d[j]).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(0);
        let z = r.bb_min.z;
        let corners = if side < 2 {
            let x = if side == 0 { r.bb_min.x } else { r.bb_max.x };
            let c = py.clamp(r.bb_min.y + w * 0.5 + 0.1, r.bb_max.y - w * 0.5 - 0.1);
            [v3(x, c - w * 0.5, z), v3(x, c - w * 0.5, z + h), v3(x, c + w * 0.5, z + h), v3(x, c + w * 0.5, z)]
        } else {
            let y = if side == 2 { r.bb_min.y } else { r.bb_max.y };
            let c = px.clamp(r.bb_min.x + w * 0.5 + 0.1, r.bb_max.x - w * 0.5 - 0.1);
            [v3(c - w * 0.5, y, z), v3(c - w * 0.5, y, z + h), v3(c + w * 0.5, y, z + h), v3(c + w * 0.5, y, z)]
        };
        self.push_undo();
        let mname = {
            let mlo = &mut self.proj.ytyps[t].mlos[m];
            mlo.portals.push(crate::model::Portal::new(ri as u32, 0, corners));
            mlo.name.clone()
        };
        let exits = self.proj.ytyps[t].mlos[m].portals.iter().filter(|p| p.room_to == 0 || p.room_from == 0).count() as u32;
        for y in &mut self.proj.ymaps {
            for e in &mut y.entities {
                if e.archetype.eq_ignore_ascii_case(&mname) {
                    if let Some(d) = e.mlo.as_mut() {
                        d.num_exit_portals = exits;
                    }
                }
            }
        }
        let wall = ["ouest (x min)", "est (x max)", "sud (y min)", "nord (y max)"][side];
        Ok(txt(format!("Entrée de {w:.1} × {h:.1} m percée dans le mur {wall} de la pièce {ri} « {} ». {exits} sortie(s) au total. Pour une vraie porte : place_doors.", r.name)))
    }

    fn mcp_split(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let (t, m) = self.find_interior(a["interior"].as_str().unwrap_or(""))?;
        let ri = self.room_arg(t, m, &a["room"])?;
        let along_x = a["axis"].as_str() == Some("x");
        let cut = num(a, "at").ok_or("at manquant")?;
        let r = self.proj.ytyps[t].mlos[m].rooms[ri].clone();
        if r.free {
            return Err("Cette pièce est de forme libre (coque modelable) : utilise l'outil Mur du Studio MLO.".into());
        }
        let (lo, hi) = if along_x { (r.bb_min.x, r.bb_max.x) } else { (r.bb_min.y, r.bb_max.y) };
        if cut - lo < 1.0 || hi - cut < 1.0 {
            return Err(format!("Coupe trop près d'un mur : at doit être entre {:.2} et {:.2}.", lo + 1.0, hi - 1.0));
        }
        if self.proj.ytyps[t].mlos[m].rooms.len() >= 32 {
            return Err("31 pièces au plus dans un intérieur.".into());
        }
        self.push_undo();
        let mlo = &mut self.proj.ytyps[t].mlos[m];
        let ni = mlo.rooms.len();
        let mut b = r.clone();
        b.name = a["name_b"].as_str().filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string()).unwrap_or_else(|| format!("{}_b", r.name));
        if let Some(na) = a["name_a"].as_str().filter(|s| !s.trim().is_empty()) {
            mlo.rooms[ri].name = na.trim().to_string();
        }
        if along_x {
            mlo.rooms[ri].bb_max.x = cut;
            b.bb_min.x = cut;
        } else {
            mlo.rooms[ri].bb_max.y = cut;
            b.bb_min.y = cut;
        }
        mlo.rooms.push(b);
        let beyond = |q: V3| if along_x { q.x > cut } else { q.y > cut };
        // portail coupé en deux par la cloison : il ne marcherait que d'un côté (pièce qui disparaît en le traversant)
        let straddle: Vec<usize> = mlo
            .portals
            .iter()
            .enumerate()
            .filter(|(_, pt)| (pt.room_from as usize == ri || pt.room_to as usize == ri) && pt.corners.iter().any(|c| beyond(*c)) && pt.corners.iter().any(|c| !beyond(*c)))
            .map(|(i, _)| i)
            .collect();
        if !straddle.is_empty() {
            return Err(format!("La coupe passerait à travers le(s) passage(s) {:?} (porte ou fenêtre) : choisis une autre position pour at, à au moins 0,3 m de chaque ouverture.", straddle));
        }
        for pt in mlo.portals.iter_mut() {
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
        for e in mlo.entities.iter_mut() {
            if e.room == ri && beyond(e.pos) {
                e.room = ni;
            }
        }
        let (ra, rb) = (mlo.rooms[ri].clone(), mlo.rooms[ni].clone());
        let door = crate::plan::shared_door(&ra, &rb, ra.bb_min.z);
        if let Some(c) = door {
            mlo.portals.push(crate::model::Portal::new(ri as u32, ni as u32, c));
        }
        let mname = mlo.name.clone();
        // intérieur repris du jeu (sans coque en boîte) : la cloison est un mur de la coque modelable, percé à la porte
        let mut wall = String::new();
        if ra.game {
            let (a2, b2) = if along_x { ([cut, ra.bb_min.y], [cut, ra.bb_max.y]) } else { ([ra.bb_min.x, cut], [ra.bb_max.x, cut]) };
            let h = (ra.bb_max.z - ra.bb_min.z).clamp(2.2, 4.0);
            let holes = door
                .map(|c| {
                    let mid = [(c[0].x + c[3].x) * 0.5, (c[0].y + c[3].y) * 0.5];
                    vec![crate::design::DHole { s: (mid[0] - a2[0]).hypot(mid[1] - a2[1]), w: (c[3] - c[0]).len(), sill: 0.0, h: (c[1].z - c[0].z).min(h - 0.05), kind: 0 }]
                })
                .unwrap_or_default();
            self.design_mut(&mname).walls.push(crate::design::DWall { a: a2, b: b2, z0: ra.bb_min.z, h, t: 0.12, holes, ..Default::default() });
            wall = " Une cloison percée d'une porte est posée le long de la coupe (mur de la coque modelable).".into();
        }
        Ok(txt(format!("Pièce {ri} « {} » coupée : nouvelle pièce {ni} « {} », avec une porte entre les deux.{wall}", ra.name, rb.name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::v3;

    /// Une boîte de 10 × 6 m coupée à mi-hauteur : quatre murs, le toit qui déborde ignoré.
    #[test]
    fn murs_d_une_boite() {
        let c = |x: f32, y: f32, z: f32| v3(x, y, z);
        let quad = |a: V3, b: V3, cc: V3, d: V3| vec![[a, b, cc], [a, cc, d]];
        let mut t = Vec::new();
        t.extend(quad(c(0.0, 0.0, 0.0), c(10.0, 0.0, 0.0), c(10.0, 0.0, 3.0), c(0.0, 0.0, 3.0)));
        t.extend(quad(c(10.0, 0.0, 0.0), c(10.0, 6.0, 0.0), c(10.0, 6.0, 3.0), c(10.0, 0.0, 3.0)));
        t.extend(quad(c(10.0, 6.0, 0.0), c(0.0, 6.0, 0.0), c(0.0, 6.0, 3.0), c(10.0, 6.0, 3.0)));
        t.extend(quad(c(0.0, 6.0, 0.0), c(0.0, 0.0, 0.0), c(0.0, 0.0, 3.0), c(0.0, 6.0, 3.0)));
        t.extend(quad(c(-1.0, -1.0, 3.0), c(11.0, -1.0, 3.0), c(11.0, 7.0, 3.0), c(-1.0, 7.0, 3.0)));
        let w = footprint_walls(&t, 1.5);
        assert_eq!(w.len(), 4, "{w:?}");
        let l = |p: [f32; 2], q: [f32; 2]| ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
        assert!((l(w[0].0, w[0].1) - 10.0).abs() < 0.01);
        let xs: Vec<f32> = w.iter().flat_map(|(p, q)| [p[0], q[0]]).collect();
        assert!(xs.iter().all(|x| *x > -0.01 && *x < 10.01), "pas de débord du toit : {xs:?}");
    }
}
