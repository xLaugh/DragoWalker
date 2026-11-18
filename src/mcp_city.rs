//! Outils MCP pour bâtir une ville ou un quartier entier : plan montré dans la vue, terrain, routes, bâtiments de
//! l'atelier, bâtiments du jeu recopiés, végétation. Avec le skill dragowalker-miniville, l'IA propose d'abord un
//! plan (montré dans la vue avec show_plan), pose ses questions, puis construit quand la personne a dit oui.

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use eframe::egui::{self, Color32, Pos2};
use serde_json::{json, Value};

fn tool(name: &str, desc: &str, props: Value, required: &[&str]) -> Value {
    json!({"name": name, "description": desc, "inputSchema": {"type": "object", "properties": props, "required": required}})
}

fn txt(s: impl Into<String>) -> Vec<Value> {
    vec![json!({"type": "text", "text": s.into()})]
}

fn num(a: &Value, k: &str) -> Option<f32> {
    a[k].as_f64().map(|v| v as f32)
}

/// Points [[x, y], …] d'un argument.
fn pts2(a: &Value) -> Vec<[f32; 2]> {
    a.as_array().map(|l| l.iter().filter_map(|p| Some([p.get(0)?.as_f64()? as f32, p.get(1)?.as_f64()? as f32])).collect()).unwrap_or_default()
}

/// Indice dans une liste de noms : nombre, ou nom (sans tenir compte des majuscules ni des accents simples).
fn pick(v: &Value, names: &[&str]) -> Option<usize> {
    if let Some(i) = v.as_u64() {
        return ((i as usize) < names.len()).then_some(i as usize);
    }
    let s = fold(v.as_str()?);
    if s.is_empty() {
        return None;
    }
    names.iter().position(|n| fold(n) == s).or_else(|| names.iter().position(|n| fold(n).contains(&s) || s.contains(&fold(n))))
}

fn fold(s: &str) -> String {
    s.trim().to_lowercase().chars().map(|c| match c {
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'à' | 'â' => 'a',
        'î' | 'ï' => 'i',
        'ô' => 'o',
        'û' | 'ù' => 'u',
        'ç' => 'c',
        _ => c,
    }).collect()
}

/// Quartiers réels de la carte du jeu (centre approximatif, m) pour chercher des bâtiments d'un style — **estimé**
/// d'après la carte de GTA V : (nom, style, x, y).
pub const DISTRICTS: [(&str, &str, f32, f32); 20] = [
    ("Grove Street", "maisons modestes de plain-pied, gangs", 100.0, -1900.0),
    ("Davis", "maisons et petits commerces populaires", 100.0, -1650.0),
    ("Rancho", "maisons ouvrières, ateliers", 400.0, -1850.0),
    ("Strawberry", "commerces de quartier, motels", 200.0, -1300.0),
    ("Mirror Park", "maisons de banlieue proprettes, lac", 1150.0, -600.0),
    ("El Burro Heights", "maisons pauvres sur la colline", 1300.0, -1700.0),
    ("Vinewood Hills", "villas de luxe à flanc de colline", -300.0, 700.0),
    ("Rockford Hills", "villas et boutiques chics", -850.0, -50.0),
    ("Richman", "grandes propriétés", -1600.0, 150.0),
    ("Vinewood", "boulevard, théâtres, enseignes", 300.0, 200.0),
    ("Downtown", "tours, bureaux, hôtels", 150.0, -800.0),
    ("Little Seoul", "commerces asiatiques, immeubles", -650.0, -900.0),
    ("Vespucci Canals", "maisons sur les canaux", -1050.0, -1000.0),
    ("Del Perro", "bord de mer, immeubles", -1500.0, -500.0),
    ("Cypress Flats", "usines et entrepôts", 850.0, -2200.0),
    ("Port", "conteneurs, hangars", 800.0, -3000.0),
    ("Sandy Shores", "mobil-homes, désert", 1800.0, 3700.0),
    ("Grapeseed", "fermes", 1700.0, 4800.0),
    ("Paleto Bay", "petite ville de montagne, bois", -200.0, 6300.0),
    ("Chumash", "maisons de plage", -3150.0, 1050.0),
];

/// Morceaux qui ne sont pas des bâtiments à recopier (enseignes, pylônes, guirlandes, versions éclairées…).
const NOT_COPY: [&str; 22] = ["sign", "pylon", "bunting", "emis", "_vpr", "cloth", "fence", "bridge", "tunnel", "billboard", "_dec", "stair", "railing", "_wall", "pipe", "tank", "crane", "tower_", "antenna", "_ladder", "scaff", "hoarding"];

pub const NAMES: [&str; 11] = ["mlo_conflicts", "city_catalog", "show_plan", "create_terrain", "flatten_area", "add_road", "add_building", "find_game_buildings", "copy_game_building", "plant_vegetation", "ground_height"];

/// Un élément du plan montré dans la vue (show_plan).
#[derive(Clone, Debug)]
pub struct PlanItem {
    pub kind: String,
    pub pts: Vec<[f32; 2]>,
    pub label: String,
}

/// Plan proposé par l'IA, dessiné dans la vue jusqu'à ce qu'elle le retire (ou le bouton « Retirer le plan »).
#[derive(Clone, Debug, Default)]
pub struct AiPlan {
    pub title: String,
    pub items: Vec<PlanItem>,
}

pub fn tools_city() -> Vec<Value> {
    let pts = json!({"type": "array", "items": {"type": "array", "items": {"type": "number"}}, "description": "points [[x, y], …] du monde (m)"});
    vec![
        tool(
            "mlo_conflicts",
            "Conflits entre les MLO du projet (importés, créés) et avec la carte du jeu : intérieurs qui se recouvrent, intérieur du jeu au même endroit, objets du jeu qui traversent un intérieur, fichiers de même nom dans plusieurs ressources, intérieur défini deux fois. fix : true règle tout ce qui se règle seul (renommage à l'export, bonne version des fichiers du jeu modifiés, objets du jeu retirés) ; les intérieurs qui se recouvrent sont à déplacer (move_object), après accord de la personne.",
            json!({"fix": {"type": "boolean"}}),
            &[],
        ),
        tool(
            "city_catalog",
            "Tout ce qu'on peut choisir pour bâtir une ville : modèles de terrain, types de route, modèles et façades des bâtiments de l'atelier (couleurs, toits, fenêtres, textures du jeu), ensembles de végétation, quartiers du jeu où chercher des bâtiments à recopier. À lire avant de proposer un plan.",
            json!({}),
            &[],
        ),
        tool(
            "show_plan",
            "Montre un PLAN dans la vue de DragoWalker (lignes et contours colorés au sol, avec leur nom) sans rien construire : routes (kind « road », ligne), bâtiments (« building », contour), parcs (« park »), places (« plaza »), eau (« water »), zones (« zone »), notes (« note », un point). À utiliser AVANT de construire : montre le plan, fais un screenshot, explique-le et demande l'accord. clear : true retire le plan.",
            json!({"title": {"type": "string"}, "items": {"type": "array", "items": {"type": "object", "properties": {"kind": {"type": "string"}, "points": pts, "label": {"type": "string"}}}}, "clear": {"type": "boolean"}}),
            &[],
        ),
        tool(
            "create_terrain",
            "Crée (ou remplace) le terrain du projet : preset (nom ou numéro, voir city_catalog : Plat, Collines, Île…), size (m, 128 à 4000), height (hauteur de base, m), island (bords qui plongent dans la mer), x / y (centre ; sinon le centre de la vue), empty_map (cacher la carte du jeu, vrai par défaut), paint (peindre selon le relief, vrai par défaut). add : true AJOUTE ce modèle au terrain déjà là (plusieurs îles, une montagne à côté…) au lieu de le remplacer : le terrain s'agrandit, le reste est gardé. Pour une ville : « Plat » ou « Île » ; une ville se bâtit sur un sol plat.",
            json!({"add": {"type": "boolean"}, "preset": {"type": ["integer", "string"]}, "size": {"type": "number"}, "height": {"type": "number"}, "island": {"type": "boolean"}, "seed": {"type": "integer"}, "x": {"type": "number"}, "y": {"type": "number"}, "empty_map": {"type": "boolean"}, "paint": {"type": "boolean"}}),
            &[],
        ),
        tool(
            "flatten_area",
            "Aplanit le terrain du projet dans un polygone (points) ou un rectangle (x0, y0, x1, y1), à la hauteur z (sinon la moyenne), raccordé en pente douce sur « margin » m (6 par défaut). Avant de poser des bâtiments sur un terrain en relief.",
            json!({"points": pts, "x0": {"type": "number"}, "y0": {"type": "number"}, "x1": {"type": "number"}, "y1": {"type": "number"}, "z": {"type": "number"}, "margin": {"type": "number"}}),
            &[],
        ),
        tool(
            "add_road",
            "Trace une route sur le terrain du projet (il en faut un) : points [[x, y], …] (au moins 2, au plus 255 m entre deux points ; une courbe passe par tous), kind (type, nom ou numéro : Rue de ville, Avenue…). Les bouts près d'une autre route font un carrefour tout seuls (feux, passages piétons, trottoirs). lights : lampadaires (sinon selon le type).",
            json!({"points": pts, "kind": {"type": ["integer", "string"]}, "lights": {"type": "boolean"}, "bridge": {"type": "boolean"}, "tunnel": {"type": "boolean"}}),
            &["points"],
        ),
        tool(
            "add_building",
            "Construit un bâtiment de l'atelier (modèle fait par DragoWalker, entièrement réglable) : emprise par points [[x, y], …] (sens quelconque ; le 1er côté porte l'entrée) OU x, y, width, depth, heading (cap de la façade d'entrée, 0 = vers le sud, 90 = vers l'est). preset : Maison de banlieue, Petit immeuble, Commerce, Entrepôt. Réglages : floors, floor_height, roof (Plat, Deux pans, Quatre pans), facade (Brique, Enduit clair, Béton, Bardage bois), game_facade (texture du jeu, voir city_catalog), color (nom de couleur ou [r, g, b]), shop (vitrines au rez-de-chaussée), balconies (0, 1, 2), awnings, decor (on n'y entre pas : plus léger), interior (vrai : intérieur MLO complet avec pièces, meubles, lumières).",
            json!({"points": pts, "x": {"type": "number"}, "y": {"type": "number"}, "width": {"type": "number"}, "depth": {"type": "number"}, "heading": {"type": "number"}, "preset": {"type": ["integer", "string"]}, "floors": {"type": "integer"}, "floor_height": {"type": "number"}, "roof": {"type": ["integer", "string"]}, "facade": {"type": ["integer", "string"]}, "game_facade": {"type": ["integer", "string"]}, "color": {}, "shop": {"type": "boolean"}, "balconies": {"type": "integer"}, "awnings": {"type": "boolean"}, "decor": {"type": "boolean"}, "interior": {"type": "boolean"}}),
            &[],
        ),
        tool(
            "find_game_buildings",
            "Cherche des bâtiments du jeu à recopier (bâtiments complets : le corps et tous ses détails) : district (quartier réel de la carte, voir city_catalog : Grove Street, Mirror Park, Vinewood Hills, Paleto Bay…) ou x / y + radius (m, 250 par défaut), sinon zone (groupe de cartes) ; query (mot du nom), min_width / max_width / max_height (m), limit (20). Rend nom, taille, nombre de morceaux, cap d'origine et sa position dans le jeu (pour aller le voir avec set_camera avant de le choisir).",
            json!({"district": {"type": "string"}, "x": {"type": "number"}, "y": {"type": "number"}, "radius": {"type": "number"}, "zone": {"type": ["integer", "string"]}, "query": {"type": "string"}, "min_width": {"type": "number"}, "max_width": {"type": "number"}, "max_height": {"type": "number"}, "limit": {"type": "integer"}}),
            &[],
        ),
        tool(
            "copy_game_building",
            "Recopie un bâtiment du jeu (nom de find_game_buildings) à un endroit : x, y (z : le sol, sinon calculé), heading (rotation en degrés par rapport à son orientation dans le modèle ; regarde avec screenshot de quel côté est sa porte), whole (tous ses morceaux, vrai par défaut), flatten (aplanir le terrain du projet dessous, vrai par défaut). À l'export : sa collision est générée et son .ytyp est mis en dépendance.",
            json!({"name": {"type": "string"}, "x": {"type": "number"}, "y": {"type": "number"}, "z": {"type": "number"}, "heading": {"type": "number"}, "whole": {"type": "boolean"}, "flatten": {"type": "boolean"}}),
            &["name", "x", "y"],
        ),
        tool(
            "plant_vegetation",
            "Sème de la végétation sur le terrain du projet dans un disque (x, y, radius) : set (ensemble, nom ou numéro : Feuillus et parc, Palmiers, Buissons…), density (0,05 à 1). Évite les routes, le béton et l'eau.",
            json!({"set": {"type": ["integer", "string"]}, "x": {"type": "number"}, "y": {"type": "number"}, "radius": {"type": "number"}, "density": {"type": "number"}}),
            &["x", "y", "radius"],
        ),
        tool(
            "ground_height",
            "Hauteur du sol (terrain du projet, sinon relief du jeu) en des points [[x, y], …].",
            json!({"points": pts}),
            &["points"],
        ),
    ]
}

/// Couleurs de plan par genre d'élément.
fn plan_color(kind: &str) -> Color32 {
    match kind {
        "road" => Color32::from_rgb(250, 210, 80),
        "building" => Color32::from_rgb(255, 140, 90),
        "park" => Color32::from_rgb(110, 220, 120),
        "plaza" => Color32::from_rgb(200, 200, 210),
        "water" => Color32::from_rgb(90, 170, 255),
        "zone" => Color32::from_rgb(220, 140, 255),
        _ => Color32::WHITE,
    }
}

impl DwApp {
    pub fn mcp_city(&mut self, name: &str, a: &Value) -> Result<Vec<Value>, String> {
        match name {
            "city_catalog" => Ok(txt(self.city_catalog())),
            "mlo_conflicts" => {
                if a["fix"].as_bool().unwrap_or(false) {
                    return Ok(txt(self.fix_all_conflicts()));
                }
                let list = self.find_conflicts();
                if list.is_empty() {
                    return Ok(txt("Aucun conflit."));
                }
                Ok(txt(list.iter().map(|c| format!("- [{}] {}{}", ["info", "à régler", "grave"][c.level as usize], c.text, c.fix_label().map(|l| format!(" (réglage : {l})")).unwrap_or_default())).collect::<Vec<_>>().join("\n")))
            }
            "show_plan" => {
                if a["clear"].as_bool().unwrap_or(false) {
                    self.ai_plan = None;
                    return Ok(txt("Plan retiré de la vue."));
                }
                let items: Vec<PlanItem> = a["items"]
                    .as_array()
                    .map(|l| l.iter().map(|it| PlanItem { kind: it["kind"].as_str().unwrap_or("note").to_lowercase(), pts: pts2(&it["points"]), label: it["label"].as_str().unwrap_or("").to_string() }).filter(|p| !p.pts.is_empty()).collect())
                    .unwrap_or_default();
                if items.is_empty() {
                    return Err("Aucun élément avec des points.".into());
                }
                // la vue se recentre sur le plan, vue de dessus
                let (mut mn, mut mx) = ([f32::MAX; 2], [f32::MIN; 2]);
                for p in items.iter().flat_map(|i| i.pts.iter()) {
                    mn = [mn[0].min(p[0]), mn[1].min(p[1])];
                    mx = [mx[0].max(p[0]), mx[1].max(p[1])];
                }
                let c = v3((mn[0] + mx[0]) * 0.5, (mn[1] + mx[1]) * 0.5, 0.0);
                let z = self.ground_z_at(c.x, c.y);
                self.cam.target = v3(c.x, c.y, z);
                self.cam.pitch = 1.35;
                self.cam.dist = ((mx[0] - mn[0]).max(mx[1] - mn[1]) * 1.1).clamp(40.0, 3000.0);
                let n = items.len();
                self.ai_plan = Some(AiPlan { title: a["title"].as_str().unwrap_or("Plan").to_string(), items });
                self.status = format!("Plan de l'IA montré dans la vue ({n} élément(s)) : rien n'est encore construit.");
                Ok(txt(format!("Plan montré dans la vue ({n} élément(s), {:.0} × {:.0} m), rien n'est construit. Fais un screenshot pour le voir, puis demande l'accord de la personne.", mx[0] - mn[0], mx[1] - mn[1])))
            }
            "create_terrain" => {
                let preset = pick(&a["preset"], &crate::terrain::PRESETS.iter().map(|p| p.0).collect::<Vec<_>>()).unwrap_or(0) as u8;
                let size = num(a, "size").unwrap_or(400.0).clamp(128.0, crate::terrain::MAX_SIZE);
                let height = num(a, "height").unwrap_or(if preset == 0 { 4.0 } else { 0.0 }).clamp(-20.0, 60.0);
                let c = v3(num(a, "x").unwrap_or(self.cam.target.x), num(a, "y").unwrap_or(self.cam.target.y), 0.0);
                self.push_undo();
                self.terrain.preset = preset;
                self.terrain.island = a["island"].as_bool().unwrap_or(false);
                self.terrain.compose = false;
                if let Some(s) = a["seed"].as_u64() {
                    self.terrain.seed = (s as u32).max(1);
                }
                self.terrain.new_size = size;
                self.terrain.new_height = height;
                let spec = self.gen_spec();
                if a["add"].as_bool().unwrap_or(false) && self.proj.terrain.is_some() {
                    self.proj.terrain.as_mut().map(|t| t.add_region(&spec, c, size)).transpose()?;
                    self.terrain.dirty = true;
                    let (mn, mx) = self.proj.terrain.as_ref().map(|t| t.bounds()).unwrap_or((c, c));
                    return Ok(txt(format!("Modèle « {} »{} ajouté en ({:.0}, {:.0}) sur {size:.0} m, le reste du terrain gardé. Le terrain va maintenant de x {:.0} à {:.0}, y {:.0} à {:.0}.", crate::terrain::PRESETS[preset as usize].0, if self.terrain.island { " en île" } else { "" }, c.x, c.y, mn.x, mx.x, mn.y, mx.y)));
                }
                let mut t = crate::terrain::Terrain::new(c, size, height, self.terrain.new_mat);
                if spec.compose || spec.island || spec.preset != 0 {
                    t.generate_ex(&spec);
                }
                if a["paint"].as_bool().unwrap_or(true) {
                    let style = t.dominant_style();
                    t.auto_paint(style);
                }
                self.proj.terrain = Some(t);
                self.terrain.dirty = true;
                self.game.empty_map = a["empty_map"].as_bool().unwrap_or(true);
                self.cam.target = v3(c.x, c.y, height);
                self.cam.dist = size * 0.9;
                Ok(txt(format!("Terrain « {} »{} de {size:.0} m créé, centré sur ({:.0}, {:.0}), hauteur de base {height:.1} m{}. Il va de x {:.0} à {:.0}, y {:.0} à {:.0}.", crate::terrain::PRESETS[preset as usize].0, if self.terrain.island { " en île" } else { "" }, c.x, c.y, if self.game.empty_map { ", carte du jeu cachée" } else { "" }, c.x - size / 2.0, c.x + size / 2.0, c.y - size / 2.0, c.y + size / 2.0)))
            }
            "flatten_area" => {
                let mut poly = pts2(&a["points"]);
                if poly.len() < 3 {
                    let (x0, y0, x1, y1) = (num(a, "x0").ok_or("points ou x0, y0, x1, y1")?, num(a, "y0").unwrap_or(0.0), num(a, "x1").unwrap_or(0.0), num(a, "y1").unwrap_or(0.0));
                    poly = vec![[x0.min(x1), y0.min(y1)], [x0.max(x1), y0.min(y1)], [x0.max(x1), y0.max(y1)], [x0.min(x1), y0.max(y1)]];
                }
                self.push_undo();
                let n = self.flatten_poly(&poly, num(a, "z"), num(a, "margin").unwrap_or(6.0))?;
                Ok(txt(format!("Terrain aplani : {} point(s) du relief, à {:.2} m.", n.0, n.1)))
            }
            "add_road" => self.mcp_add_road(a),
            "add_building" => self.mcp_add_building(a),
            "find_game_buildings" => self.mcp_find_buildings(a),
            "copy_game_building" => self.mcp_copy_building(a),
            "plant_vegetation" => {
                if self.proj.terrain.is_none() {
                    return Err("Il faut un terrain du projet (create_terrain).".into());
                }
                let names: Vec<&str> = crate::vegetation::SETS.iter().map(|s| s.name).collect();
                let set = pick(&a["set"], &names).unwrap_or(1);
                let (old_set, old_d, old_sp) = (self.terrain.veg_set, self.terrain.veg_density, self.terrain.veg_spacing);
                self.terrain.veg_set = set;
                self.terrain.veg_density = num(a, "density").unwrap_or(0.5).clamp(0.05, 1.0);
                self.terrain.veg_spacing = 0.0;
                self.push_undo();
                let c = v3(num(a, "x").unwrap_or(0.0), num(a, "y").unwrap_or(0.0), 0.0);
                let r = num(a, "radius").unwrap_or(30.0).clamp(2.0, 400.0);
                // par disques de 40 m au plus (veg_paint limite ses essais)
                let mut n = 0;
                let step = 40.0f32;
                let k = (r / step).ceil() as i32;
                for i in -k..=k {
                    for j in -k..=k {
                        let p = v3(c.x + i as f32 * step, c.y + j as f32 * step, 0.0);
                        if (p - c).len() <= r {
                            n += self.veg_paint(p, step.min(r));
                        }
                    }
                }
                self.terrain.veg_set = old_set;
                self.terrain.veg_density = old_d;
                self.terrain.veg_spacing = old_sp;
                Ok(txt(format!("{n} plante(s) « {} » semée(s).", names[set])))
            }
            "ground_height" => {
                let p = pts2(&a["points"]);
                Ok(txt(p.iter().map(|q| format!("({:.1}, {:.1}) : {:.2} m", q[0], q[1], self.ground_z_at(q[0], q[1]))).collect::<Vec<_>>().join("\n")))
            }
            _ => Err(format!("Outil inconnu : {name}")),
        }
    }

    /// Hauteur du sol : terrain du projet, sinon relief du jeu, sinon 0.
    pub fn ground_z_at(&self, x: f32, y: f32) -> f32 {
        if let Some(z) = self.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)) {
            return z;
        }
        if self.game.empty_map {
            return 0.0;
        }
        self.game.world.as_ref().and_then(|w| w.ground_min(x, y)).unwrap_or(self.ground_z)
    }

    fn city_catalog(&self) -> String {
        let mut s = String::new();
        let list = |v: Vec<String>| v.iter().enumerate().map(|(i, n)| format!("{i} {n}")).collect::<Vec<_>>().join(" ; ");
        s += &format!("Terrains (create_terrain preset) : {}\n", list(crate::terrain::PRESETS.iter().map(|p| format!("{} ({})", p.0, p.1)).collect()));
        s += &format!("Routes (add_road kind) : {}\n", list(crate::roads::KINDS.iter().map(|k| format!("{} — {}", k.name, k.tip)).collect()));
        s += &format!("Bâtiments de l'atelier (add_building preset) : {}\n", list(crate::atelier::PRESETS.iter().map(|p| format!("{} (environ {:.0} × {:.0} m)", p.0, p.1, p.2)).collect()));
        s += &format!("Façades : {} ; toits : {} ; fenêtres : {}\n", list(crate::atelier::FACADES.iter().map(|x| x.to_string()).collect()), list(crate::atelier::ROOFS.iter().map(|x| x.to_string()).collect()), list(crate::atelier::WIN_STYLES.iter().map(|x| x.to_string()).collect()));
        s += &format!("Textures du jeu pour game_facade : {}\n", list(crate::atelier::GAME_TEX.iter().filter(|t| t.3 == 0).map(|t| t.0.to_string()).collect()));
        s += &format!("Couleurs : {}\n", crate::atelier::COLORS.iter().map(|c| c.0).collect::<Vec<_>>().join(", "));
        s += &format!("Végétation (plant_vegetation set) : {}\n", list(crate::vegetation::SETS.iter().map(|v| format!("{} ({})", v.name, v.tip)).collect()));
        s += &format!("Quartiers réels (find_game_buildings district) : {}\n", DISTRICTS.iter().map(|d| format!("{} ({})", d.0, d.1)).collect::<Vec<_>>().join(" ; "));
        s += &format!("Groupes de cartes (find_game_buildings zone) : {}\n", list(crate::buildings::ZONES.iter().map(|z| z.0.to_string()).collect()));
        s += "Styles et quartiers du jeu : Vinewood / Rockford Hills = villas et immeubles chics ; Mirror Park = maisons de banlieue ; Sud de Los Santos = maisons modestes, commerces de quartier ; Centre-ville = tours et bureaux ; Del Perro / Vespucci = bord de mer, boutiques ; Zone industrielle / Port = entrepôts, usines ; Campagne = ranchs, fermes, Sandy Shores, Paleto Bay.\n";
        s += "Repère : x vers l'est, y vers le nord, z vers le haut (m). Une rue de ville fait 11 m de chaussée + 2 trottoirs de 3 m : laisser 9 m entre l'axe d'une rue et la façade d'un bâtiment.";
        s
    }

    /// Aplanit le terrain dans un polygone (hauteur `z`, sinon la moyenne), raccord en pente sur `margin` m. Rend le
    /// nombre de points du relief touchés et la hauteur.
    pub fn flatten_poly(&mut self, poly: &[[f32; 2]], z: Option<f32>, margin: f32) -> Result<(usize, f32), String> {
        let t = self.proj.terrain.as_mut().ok_or("Il faut un terrain du projet (create_terrain).")?;
        let inside = |p: [f32; 2]| crate::atelier::point_in_poly(p, poly);
        let dist = |p: [f32; 2]| -> f32 {
            let mut d = f32::MAX;
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let l2 = (dx * dx + dy * dy).max(1e-6);
                let u = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0);
                d = d.min(((p[0] - a[0] - u * dx).powi(2) + (p[1] - a[1] - u * dy).powi(2)).sqrt());
            }
            d
        };
        let (nx, ny, cell, o) = (t.nx, t.ny, t.cell, t.origin);
        let target = match z {
            Some(z) => z,
            None => {
                let (mut s, mut n) = (0.0f32, 0usize);
                for j in 0..ny {
                    for i in 0..nx {
                        if inside([o.x + i as f32 * cell, o.y + j as f32 * cell]) {
                            s += t.h[j * nx + i];
                            n += 1;
                        }
                    }
                }
                if n == 0 {
                    let c = poly.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / poly.len() as f32, a[1] + p[1] / poly.len() as f32]);
                    t.height_at(c[0], c[1]).ok_or("Cette zone est hors du terrain.")?
                } else {
                    s / n as f32
                }
            }
        };
        let hm = std::sync::Arc::make_mut(&mut t.h);
        let mut n = 0;
        for j in 0..ny {
            for i in 0..nx {
                let p = [o.x + i as f32 * cell, o.y + j as f32 * cell];
                let w = if inside(p) {
                    1.0
                } else if margin > 0.0 {
                    let d = dist(p);
                    if d >= margin {
                        continue;
                    }
                    let f = 1.0 - d / margin;
                    f * f * (3.0 - 2.0 * f)
                } else {
                    continue;
                };
                let v = &mut hm[j * nx + i];
                *v += (target - *v) * w;
                n += 1;
            }
        }
        self.terrain.dirty = true;
        Ok((n, target))
    }

    fn mcp_add_road(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        if self.proj.terrain.is_none() {
            return Err("Il faut un terrain du projet pour tracer une route (create_terrain).".into());
        }
        let given = pts2(&a["points"]);
        if given.len() < 2 {
            return Err("Il faut au moins 2 points.".into());
        }
        // points ajoutés sur les longs tronçons (255 m au plus entre deux points, comme les liens du jeu)
        let mut p: Vec<[f32; 2]> = vec![given[0]];
        for w in given.windows(2) {
            let d = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
            let k = (d / 120.0).ceil().max(1.0) as usize;
            for s in 1..=k {
                let f = s as f32 / k as f32;
                p.push([w[0][0] + (w[1][0] - w[0][0]) * f, w[0][1] + (w[1][1] - w[0][1]) * f]);
            }
        }
        let names: Vec<&str> = crate::roads::KINDS.iter().map(|k| k.name).collect();
        let kind = pick(&a["kind"], &names).unwrap_or(0);
        let before = self.proj.terrain.as_ref().map(|t| (t.roads.len(), t.junctions.len())).unwrap_or((0, 0));
        self.push_undo();
        let old = (self.terrain.road_kind, self.terrain.road_bridge, self.terrain.road_tunnel, self.terrain.road_furn);
        self.terrain.road_kind = kind as u8;
        self.terrain.road_bridge = a["bridge"].as_bool().unwrap_or(false);
        self.terrain.road_tunnel = a["tunnel"].as_bool().unwrap_or(false);
        if let Some(l) = a["lights"].as_bool() {
            self.terrain.road_furn[0] = Some(l);
        }
        self.terrain.road_pts = p.iter().map(|q| v3(q[0], q[1], self.ground_z_at(q[0], q[1]))).collect();
        self.finish_road();
        self.terrain.road_kind = old.0;
        self.terrain.road_bridge = old.1;
        self.terrain.road_tunnel = old.2;
        self.terrain.road_furn = old.3;
        self.terrain.road_pts.clear();
        let after = self.proj.terrain.as_ref().map(|t| (t.roads.len(), t.junctions.len())).unwrap_or((0, 0));
        if after.0 <= before.0 && after.1 == before.1 {
            return Err(format!("Route pas tracée : {}", self.status));
        }
        Ok(txt(format!("Route « {} » tracée ({} points). {} route(s), {} carrefour(s) en tout. {}", names[kind], p.len(), after.0, after.1, self.status)))
    }

    fn mcp_add_building(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let mut foot = pts2(&a["points"]);
        if foot.len() < 3 {
            let (x, y) = (num(a, "x").ok_or("points, ou x, y, width, depth")?, num(a, "y").ok_or("y manquant")?);
            let preset = pick(&a["preset"], &crate::atelier::PRESETS.iter().map(|p| p.0).collect::<Vec<_>>()).unwrap_or(0);
            let (w, d) = (num(a, "width").unwrap_or(crate::atelier::PRESETS[preset].1).clamp(3.0, 120.0), num(a, "depth").unwrap_or(crate::atelier::PRESETS[preset].2).clamp(3.0, 120.0));
            let hd = num(a, "heading").unwrap_or(0.0).to_radians();
            let (s, c) = hd.sin_cos();
            foot = [[-w / 2.0, -d / 2.0], [w / 2.0, -d / 2.0], [w / 2.0, d / 2.0], [-w / 2.0, d / 2.0]].iter().map(|p| [x + p[0] * c - p[1] * s, y + p[0] * s + p[1] * c]).collect();
        }
        // terrain du projet : aplani sous le bâtiment (et 3 m autour)
        if self.proj.terrain.is_some() && a["flatten"].as_bool().unwrap_or(true) {
            let _ = self.flatten_poly(&foot, None, 4.0);
        }
        let pts: Vec<V3> = foot.iter().map(|p| v3(p[0], p[1], self.ground_z_at(p[0], p[1]))).collect();
        let msg = self.create_house(&pts);
        let Some(hi) = self.proj.houses.len().checked_sub(1).filter(|_| msg.contains("créé")) else { return Err(msg) };
        let mut h = self.proj.houses[hi].clone();
        if let Some(k) = pick(&a["preset"], &crate::atelier::PRESETS.iter().map(|p| p.0).collect::<Vec<_>>()) {
            h.apply_preset(k);
        }
        if let Some(f) = a["floors"].as_u64() {
            h.floors = (f as u8).clamp(1, 40);
        }
        if let Some(fh) = num(a, "floor_height") {
            h.floor_h = fh.clamp(2.6, 8.0);
        }
        if let Some(r) = pick(&a["roof"], &crate::atelier::ROOFS) {
            h.roof = r as u8;
        }
        if let Some(f) = pick(&a["facade"], &crate::atelier::FACADES) {
            h.facade = f as u8;
            h.game_facade = 0;
        }
        let gnames: Vec<&str> = crate::atelier::GAME_TEX.iter().map(|t| t.0).collect();
        if let Some(g) = pick(&a["game_facade"], &gnames).filter(|g| crate::atelier::GAME_TEX[*g].3 == 0) {
            h.game_facade = g as u8 + 1;
        }
        match &a["color"] {
            Value::String(_) => {
                if let Some(c) = pick(&a["color"], &crate::atelier::COLORS.iter().map(|c| c.0).collect::<Vec<_>>()) {
                    h.color = Some(crate::atelier::COLORS[c].1);
                }
            }
            Value::Array(v) if v.len() == 3 => h.color = Some([0, 1, 2].map(|i| v[i].as_u64().unwrap_or(128).min(255) as u8)),
            _ => {}
        }
        if let Some(b) = a["shop"].as_bool() {
            h.shop = b;
        }
        if let Some(b) = a["balconies"].as_u64() {
            h.details.balconies = (b as u8).min(2);
        }
        if let Some(b) = a["awnings"].as_bool() {
            h.details.awnings = b;
        }
        if let Some(b) = a["decor"].as_bool() {
            h.decor = b;
        }
        let name = h.name.clone();
        let (floors, height) = (h.floors, h.height());
        self.proj.houses[hi] = h;
        self.house_doors_follow();
        self.atelier.sig = 0;
        let mut out = format!("Bâtiment « {name} » construit : {floors} étage(s), {height:.1} m de haut, entrée sur le côté ({:.1}, {:.1}) → ({:.1}, {:.1}).", foot[0][0], foot[0][1], foot[1][0], foot[1][1]);
        if a["interior"].as_bool().unwrap_or(false) {
            if self.proj.houses[hi].decor {
                out += " Pas d'intérieur : c'est un bâtiment de décoration.";
            } else {
                out += &format!(" {}", self.house_to_interior(hi));
            }
        }
        Ok(txt(out))
    }

    fn whole_list(&mut self) -> Result<&Vec<crate::buildings::WholeBuilding>, String> {
        if self.buildings.whole_list.is_none() {
            let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
            self.buildings.whole_list = Some(crate::buildings::build_whole_list(w));
        }
        Ok(self.buildings.whole_list.as_ref().unwrap())
    }

    fn mcp_find_buildings(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let zones: Vec<&str> = crate::buildings::ZONES.iter().map(|z| z.0).collect();
        let zone = pick(&a["zone"], &zones).unwrap_or(0);
        let q = a["query"].as_str().unwrap_or("").trim().to_lowercase();
        let (minw, maxw, maxh) = (num(a, "min_width").unwrap_or(0.0), num(a, "max_width").unwrap_or(1e9), num(a, "max_height").unwrap_or(1e9));
        let limit = a["limit"].as_u64().unwrap_or(20).clamp(1, 80) as usize;
        // autour d'un quartier réel ou d'un point : les bâtiments complets dont le corps est posé là
        let near: Option<(f32, f32, f32)> = match a["district"].as_str().and_then(|d| pick(&json!(d), &DISTRICTS.iter().map(|x| x.0).collect::<Vec<_>>())) {
            Some(i) => Some((DISTRICTS[i].2, DISTRICTS[i].3, num(a, "radius").unwrap_or(300.0))),
            None => num(a, "x").zip(num(a, "y")).map(|(x, y)| (x, y, num(a, "radius").unwrap_or(250.0))),
        };
        self.whole_list()?;
        let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
        let list = self.buildings.whole_list.as_ref().ok_or("Liste des bâtiments indisponible.")?;
        // (bâtiment, position dans le jeu, cap d'origine)
        let mut at: std::collections::HashMap<u32, (V3, f32)> = std::collections::HashMap::new();
        if let Some((x, y, r)) = near {
            for ei in w.ents_in_radius(v3(x, y, 0.0), r.clamp(20.0, 1500.0)) {
                let e = &w.ents[ei as usize];
                if e.lod_level == 0 && !e.mlo {
                    at.entry(e.arch).or_insert((e.pos, e.rot().heading_deg()));
                }
            }
        }
        let good = |b: &crate::buildings::WholeBuilding| {
            let wd = b.size.x.max(b.size.y);
            b.size.x.min(b.size.y) >= 5.0 && b.size.z >= 4.0 && wd >= minw && wd <= maxw && b.size.z <= maxh && !NOT_COPY.iter().any(|k| b.name.contains(k))
        };
        let mut found: Vec<(&crate::buildings::WholeBuilding, V3, f32)> = list
            .iter()
            .filter(|b| (q.is_empty() || b.name.contains(&q)) && good(b))
            .filter_map(|b| match near {
                Some(_) => at.get(&b.core).map(|(p, h)| (b, *p, *h)),
                None => (zone == 0 || b.zone == zone).then(|| (b, b.pos, w.ents.iter().find(|e| e.arch == b.core && e.lod_level == 0).map(|e| e.rot().heading_deg()).unwrap_or(0.0))),
            })
            .collect();
        found.sort_by(|x, y| (y.0.parts.len() * 4 + y.0.count).cmp(&(x.0.parts.len() * 4 + x.0.count)).then(x.0.name.cmp(&y.0.name)));
        let mut s = format!("{} bâtiment(s) trouvé(s){} :\n", found.len(), if found.len() > limit { format!(", les {limit} plus complets") } else { String::new() });
        for (b, p, heading) in found.iter().take(limit) {
            s += &format!("- {} : {:.0} × {:.0} × {:.0} m, {} morceau(x), posé {}× par le jeu, cap d'origine {:.0}°, dans le jeu en ({:.0}, {:.0}, {:.0})\n", b.name, b.size.x, b.size.y, b.size.z, b.parts.len(), b.count, heading, p.x, p.y, p.z);
        }
        if found.is_empty() {
            s += "Rien : élargis radius ou max_width, ou essaie un autre quartier.";
        }
        Ok(txt(s))
    }

    fn mcp_copy_building(&mut self, a: &Value) -> Result<Vec<Value>, String> {
        let name = a["name"].as_str().unwrap_or("").trim().to_lowercase();
        let (x, y) = (num(a, "x").ok_or("x manquant")?, num(a, "y").ok_or("y manquant")?);
        let whole = a["whole"].as_bool().unwrap_or(true);
        let h = crate::gta::crypto::joaat(&name);
        let b = self.whole_list()?.iter().find(|b| b.core == h).cloned();
        let w = self.game.world.as_ref().ok_or("Le jeu n'est pas chargé.")?;
        if !w.archetypes.contains_key(&h) {
            return Err(format!("« {name} » n'est pas un modèle du jeu (voir find_game_buildings)."));
        }
        let parts: Vec<(u32, V3, Quat, f32, f32)> = match (&b, whole) {
            (Some(b), true) => b.parts.clone(),
            _ => vec![(h, V3::ZERO, Quat::IDENTITY, 1.0, 1.0)],
        };
        // hauteur : le bas du modèle posé sur le sol (le relief du jeu est trop grossier pour retrouver la hauteur
        // d'origine au-dessus du sol : jusqu'à 20 m d'écart sur les collines de Mirror Park)
        let (bmn, bmx) = w.arch_box(h);
        let off = -bmn.z - 0.05;
        let heading = num(a, "heading").unwrap_or(0.0);
        let q = Quat::from_z(heading.to_radians());
        // emprise (pour aplanir le terrain du projet dessous)
        let corners: Vec<[f32; 2]> = [[bmn.x, bmn.y], [bmx.x, bmn.y], [bmx.x, bmx.y], [bmn.x, bmx.y]].iter().map(|c| {
            let r = q.rotate(v3(c[0], c[1], 0.0));
            [x + r.x, y + r.y]
        }).collect();
        let lods: Vec<(String, f32)> = parts.iter().map(|p| (w.name_of(p.0), w.archetypes.get(&p.0).map(|a| a.lod_dist).unwrap_or(300.0).max(150.0))).collect();
        if self.proj.terrain.is_some() && a["flatten"].as_bool().unwrap_or(true) {
            let _ = self.flatten_poly(&corners, None, 5.0);
        }
        let ground = num(a, "z").unwrap_or_else(|| corners.iter().map(|c| self.ground_z_at(c[0], c[1])).fold(f32::MAX, f32::min).min(self.ground_z_at(x, y)));
        let at = v3(x, y, ground + off);
        self.push_undo();
        let yi = self.ensure_target_ymap();
        let mut ids = Vec::new();
        for ((_, p, r, sxy, sz), (n, lod)) in parts.iter().zip(lods) {
            let mut e = crate::model::Entity::new(&n, at + q.rotate(*p));
            e.rot = q.mul(*r).normalized();
            e.scale_xy = *sxy;
            e.scale_z = *sz;
            e.lod_dist = lod;
            ids.push(e.id);
            self.proj.ymaps[yi].entities.push(e);
        }
        let n = ids.len();
        self.set_selection(ids.clone());
        self.sync_game_project();
        Ok(txt(format!("« {name} » recopié en ({x:.1}, {y:.1}, {:.1}), cap {heading:.0}° : {n} morceau(x) (ids {}). Regarde-le avec screenshot (de quel côté est l'entrée ?) ; move_object / undo pour corriger.", at.z, ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", "))))
    }

    /// Dessine le plan de l'IA dans la vue (show_plan).
    pub fn draw_ai_plan(&self, painter: &egui::Painter) {
        let Some(plan) = &self.ai_plan else { return };
        let at = |x: f32, y: f32| -> Option<Pos2> { self.cam.project(v3(x, y, self.ground_z_at(x, y) + 0.6)).map(|q| q.0) };
        // un point tous les 4 m au plus, pour suivre le relief
        let line = |pts: &[[f32; 2]], closed: bool| -> Vec<Pos2> {
            let mut out = Vec::new();
            let n = pts.len();
            let segs = if closed { n } else { n.saturating_sub(1) };
            for i in 0..segs {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                let d = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                let k = (d / 4.0).ceil().max(1.0) as usize;
                for s in 0..k {
                    let f = s as f32 / k as f32;
                    if let Some(p) = at(a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f) {
                        out.push(p);
                    }
                }
            }
            if !closed {
                if let Some(p) = pts.last().and_then(|q| at(q[0], q[1])) {
                    out.push(p);
                }
            }
            out
        };
        for it in &plan.items {
            let col = plan_color(&it.kind);
            let closed = !matches!(it.kind.as_str(), "road" | "note") && it.pts.len() >= 3;
            let pts = line(&it.pts, closed);
            if it.kind == "note" || it.pts.len() == 1 {
                if let Some(p) = at(it.pts[0][0], it.pts[0][1]) {
                    painter.circle_filled(p, 5.0, col);
                }
            } else if closed && pts.len() > 2 {
                painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(2.5_f32, col)));
            } else if pts.len() > 1 {
                painter.add(egui::Shape::line(pts, egui::Stroke::new(if it.kind == "road" { 5.0_f32 } else { 2.5 }, col)));
            }
            if !it.label.is_empty() {
                let n = it.pts.len() as f32;
                let c = it.pts.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
                if let Some(p) = at(c[0], c[1]) {
                    painter.text(p + egui::vec2(1.0, 1.0), egui::Align2::CENTER_CENTER, &it.label, egui::FontId::proportional(14.0), Color32::BLACK);
                    painter.text(p, egui::Align2::CENTER_CENTER, &it.label, egui::FontId::proportional(14.0), col);
                }
            }
        }
        // titre en haut de la vue
        let r = painter.clip_rect();
        let g = painter.layout_no_wrap(format!("Plan proposé par l'IA : {} (rien n'est encore construit)", plan.title), egui::FontId::proportional(16.0), Color32::from_rgb(250, 210, 80));
        let at = r.center_top() + egui::vec2(-g.size().x * 0.5, 24.0);
        painter.rect_filled(egui::Rect::from_min_size(at - egui::vec2(8.0, 4.0), g.size() + egui::vec2(16.0, 8.0)), 6.0, Color32::from_black_alpha(170));
        painter.galley(at, g, Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choix_par_nom() {
        assert_eq!(pick(&json!("rue de ville"), &["Rue de ville", "Avenue"]), Some(0));
        assert_eq!(pick(&json!("avenue"), &["Rue de ville", "Avenue"]), Some(1));
        assert_eq!(pick(&json!("Ile"), &crate::terrain::PRESETS.iter().map(|p| p.0).collect::<Vec<_>>()), Some(4));
        assert_eq!(pick(&json!(9), &["a"]), None);
        let t = tools_city();
        assert_eq!(t.len(), NAMES.len());
        for (x, n) in t.iter().zip(NAMES) {
            assert_eq!(x["name"], n);
        }
    }
}
