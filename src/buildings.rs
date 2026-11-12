//! Bibliothèque des bâtiments du jeu : tous les grands bâtiments posés sur la carte d'origine, rangés par
//! quartier, avec aperçu texturé. « Poser » copie le bâtiment (même modèle du jeu, même distance d'affichage)
//! à l'endroit voulu, par exemple sur une carte custom.
//!
//! À l'export, pour chaque bâtiment copié :
//! - dépendance de la carte vers le .ytyp du jeu qui le définit (_manifest.ymf, CPackFileMetaData >
//!   imapDependencies_2 > imapName / itypDepArray — format du _manifest.ymf.xml du vaisseau, validé en jeu) :
//!   les .ytyp de la carte du jeu ne sont chargés qu'avec leurs propres cartes ;
//! - collision générée à partir de son modèle (les bâtiments du jeu n'ont pas de collision propre : elle est
//!   dans les .ybn de leur quartier d'origine, qui ne suivent pas la copie).

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use eframe::egui::{self, Color32, RichText};

pub struct Building {
    pub arch: u32,
    pub name: String,
    pub size: V3,
    /// un des endroits où le jeu le pose, et combien de fois
    pub pos: V3,
    pub count: usize,
    pub zone: usize,
}

pub struct BuildingLib {
    pub open: bool,
    pub filter: String,
    pub zone: usize,
    pub list: Option<Vec<Building>>,
    pub chosen: Option<u32>,
    /// liste affichée : morceaux de route du jeu au lieu des bâtiments
    pub roads: bool,
    pub road_list: Option<Vec<Building>>,
    /// liste affichée : bâtiments complets (tous leurs morceaux ensemble)
    pub whole: bool,
    pub whole_list: Option<Vec<WholeBuilding>>,
}

impl BuildingLib {
    pub fn new() -> BuildingLib {
        BuildingLib {
            open: std::env::var("DW_BUILDINGS").is_ok(),
            filter: std::env::var("DW_BUILDINGS_FILTER").unwrap_or_default(),
            zone: 0,
            list: None,
            chosen: std::env::var("DW_BUILDINGS_CHOSEN").ok().map(|n| crate::gta::crypto::joaat(&n.to_lowercase())),
            roads: std::env::var("DW_ROAD_PIECES").is_ok(),
            road_list: None,
            whole: std::env::var("DW_WHOLE_BUILDINGS").is_ok(),
            whole_list: None,
        }
    }
}

/// Un bâtiment complet : le morceau principal et tout ce que le jeu pose avec lui (détails, rambardes, enseignes,
/// décalques…), placés les uns par rapport aux autres comme dans la carte d'origine.
#[derive(Clone)]
pub struct WholeBuilding {
    /// morceau principal (le plus grand bâtiment du groupe)
    pub core: u32,
    pub name: String,
    /// (modèle, position et orientation dans le repère du morceau principal, échelles xy et z) ; le principal
    /// en premier
    pub parts: Vec<(u32, V3, Quat, f32, f32)>,
    /// taille de l'ensemble
    pub size: V3,
    /// endroit d'origine (morceau principal) et nombre de fois où le jeu le pose
    pub pos: V3,
    pub count: usize,
    pub zone: usize,
}

/// Morceaux de carte jamais rattachés à un bâtiment complet : sol, routes, eau, versions lointaines, occlusion.
const NOT_PART: [&str; 15] = ["_lod", "slod", "proxy", "ground", "land", "_rd", "rd_", "road", "water", "occl", "terrain", "grnd", "shadow", "_glue", "_weed"];

/// Marge autour de la boîte du morceau principal pour y rattacher un morceau voisin (m) — **estimé**.
const WHOLE_TOUCH: f32 = 0.5;

/// Début de nom commun d'un pâté de maisons (« dt1_05 » pour « dt1_05_build1_dtl »).
fn block_of(name: &str) -> String {
    name.split('_').take(2).collect::<Vec<_>>().join("_")
}

/// Bâtiments complets : dans chaque carte du jeu (version détaillée), chaque bâtiment (mêmes règles que la liste
/// des bâtiments) reçoit les morceaux de la même carte dont le nom commence par le sien (dt1_05_build1_dtl,
/// …_railing, …_emissive), sinon ceux du même pâté (même début de nom) dont la boîte touche la sienne (à
/// WHOLE_TOUCH près) et qui ne sont pas plus grands que lui ; un morceau va au bâtiment le plus proche. Ni props, ni
/// sol, ni routes, ni versions lointaines — **estimé** (le jeu ne dit pas quels morceaux forment un bâtiment).
pub fn build_whole_list(w: &crate::gta::world::GameWorld) -> Vec<WholeBuilding> {
    let mut map: std::collections::HashMap<u32, WholeBuilding> = std::collections::HashMap::new();
    for y in &w.ymaps {
        // (objet, nom, boîte dans le monde, principal ?)
        let mut cands: Vec<(usize, String, V3, V3, bool)> = Vec::new();
        for ei in y.first..(y.first + y.count).min(w.ents.len()) {
            let e = &w.ents[ei];
            if e.lod_level != 0 || e.mlo || w.hidden_archs.contains(&e.arch) {
                continue;
            }
            let Some(a) = w.archetypes.get(&e.arch) else { continue };
            if a.mlo {
                continue;
            }
            let name = w.name_of(e.arch);
            if is_prop_name(&name) || name.starts_with("hash_") || NOT_PART.iter().any(|k| name.contains(k)) {
                continue;
            }
            let s = a.bb_max - a.bb_min;
            let core = s.x.max(s.y) >= 12.0 && s.z >= 6.0 && !NOT_BUILDING.iter().any(|k| name.contains(k));
            let c = w.corners(ei as u32);
            let (mut mn, mut mx) = (c[0], c[0]);
            for p in c {
                mn = mn.min(p);
                mx = mx.max(p);
            }
            cands.push((ei, name, mn, mx, core));
        }
        let cores: Vec<usize> = (0..cands.len()).filter(|&i| cands[i].4).collect();
        if cores.is_empty() {
            continue;
        }
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); cores.len()];
        for (i, c) in cands.iter().enumerate() {
            if c.4 {
                continue;
            }
            // nom qui commence par celui d'un bâtiment (le plus long)
            let by_name = cores.iter().enumerate().filter(|(_, &k)| c.1.starts_with(&cands[k].1)).max_by_key(|(_, &k)| cands[k].1.len()).map(|(j, _)| j);
            let chosen = by_name.or_else(|| {
                let ctr = (c.2 + c.3) * 0.5;
                let diag = (c.3 - c.2).len();
                let blk = block_of(&c.1);
                cores
                    .iter()
                    .enumerate()
                    .filter(|(_, &k)| {
                        let (kmn, kmx) = (cands[k].2 - v3(WHOLE_TOUCH, WHOLE_TOUCH, WHOLE_TOUCH), cands[k].3 + v3(WHOLE_TOUCH, WHOLE_TOUCH, WHOLE_TOUCH));
                        let touch = c.2.x <= kmx.x && c.3.x >= kmn.x && c.2.y <= kmx.y && c.3.y >= kmn.y && c.2.z <= kmx.z && c.3.z >= kmn.z;
                        touch && block_of(&cands[k].1) == blk && diag <= (cands[k].3 - cands[k].2).len() * 1.2 && !c.1.contains("decal")
                    })
                    .min_by(|(_, &a), (_, &b)| {
                        let da = ((cands[a].2 + cands[a].3) * 0.5 - ctr).len();
                        let db = ((cands[b].2 + cands[b].3) * 0.5 - ctr).len();
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(j, _)| j)
            });
            if let Some(j) = chosen {
                members[j].push(i);
            }
        }
        for (j, &k) in cores.iter().enumerate() {
            let (ei, ref name, _, _, _) = cands[k];
            let e0 = &w.ents[ei];
            if let Some(b) = map.get_mut(&e0.arch) {
                b.count += 1;
                continue;
            }
            let inv = e0.rot().inverse();
            let mut parts = vec![(e0.arch, V3::ZERO, Quat::IDENTITY, e0.sxy(), e0.sz())];
            let (mut mn, mut mx) = (cands[k].2, cands[k].3);
            for &i in &members[j] {
                let p = &w.ents[cands[i].0];
                parts.push((p.arch, inv.rotate(p.pos - e0.pos), inv.mul(p.rot()).normalized(), p.sxy(), p.sz()));
                mn = mn.min(cands[i].2);
                mx = mx.max(cands[i].3);
            }
            map.insert(e0.arch, WholeBuilding { core: e0.arch, name: name.clone(), parts, size: mx - mn, pos: e0.pos, count: 1, zone: zone_of(name) });
        }
    }
    let mut v: Vec<WholeBuilding> = map.into_values().collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

/// Mots des noms de modèles de route du jeu (ss1_rd1_…, id2_rd_r4_03, …) — relevés par la sonde DW_ARCH_GREP.
const ROAD_TOKENS: [&str; 4] = ["_rd", "rd_", "road", "_junc"];
/// Morceaux portant ces mots qui ne sont pas des chaussées (versions lointaines, câbles, décors…).
const NOT_ROAD: [&str; 24] = ["_lod", "slod", "proxy", "decal", "detail", "_dtl", "emissive", "shadow", "occl", "cable", "banner", "wire", "_alpha", "logo", "sign", "bakedfur", "props", "_ovly", "overlay", "_weed", "light", "railing", "billboard", "armco"];

/// Morceaux de route du jeu : objets HD de la carte (pas des props) dont le nom parle de route, plats (moins
/// de 10 m de haut) et d'au moins 10 m de long — **estimé**. Les routes de GTA V sont modelées par quartier :
/// ce sont des morceaux de vraies rues (carrefours, virages, lignes droites), pas des pièces standard.
pub fn build_road_list(w: &crate::gta::world::GameWorld) -> Vec<Building> {
    let mut map: std::collections::HashMap<u32, Building> = std::collections::HashMap::new();
    for e in &w.ents {
        if e.lod_level != 0 || e.mlo {
            continue;
        }
        if let Some(b) = map.get_mut(&e.arch) {
            b.count += 1;
            continue;
        }
        let Some(a) = w.archetypes.get(&e.arch) else { continue };
        if a.mlo {
            continue;
        }
        let s = a.bb_max - a.bb_min;
        if s.x.max(s.y) < 10.0 || s.z > 10.0 {
            continue;
        }
        let name = w.name_of(e.arch);
        if is_prop_name(&name) || name.starts_with("hash_") || !ROAD_TOKENS.iter().any(|k| name.contains(k)) || NOT_ROAD.iter().any(|k| name.contains(k)) {
            continue;
        }
        let zone = zone_of(&name);
        map.insert(e.arch, Building { arch: e.arch, name, size: s, pos: e.pos, count: 1, zone });
    }
    let mut v: Vec<Building> = map.into_values().collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

/// Quartiers, d'après le début du nom des modèles de la carte (dt1_…, hw1_…) — **estimé**.
pub const ZONES: [(&str, &[&str]); 13] = [
    ("Tous", &[]),
    ("Centre-ville", &["dt1"]),
    ("Vinewood", &["hw1"]),
    ("Rockford Hills", &["bh1"]),
    ("Mirror Park", &["ss1"]),
    ("Del Perro / Vespucci", &["sm", "vb"]),
    ("Little Seoul", &["kt1"]),
    ("Sud de Los Santos", &["sc1"]),
    ("Zone industrielle", &["id1", "id2"]),
    ("Port", &["po1"]),
    ("Aéroport", &["ap1"]),
    ("Campagne", &["cs", "ch", "ce", "cn"]),
    ("Autres", &[]),
];

fn zone_of(name: &str) -> usize {
    let n = name.strip_prefix("hei_").unwrap_or(name);
    let tok = n.split('_').next().unwrap_or("");
    for (i, (_, pre)) in ZONES.iter().enumerate().skip(1) {
        if pre.iter().any(|p| tok.starts_with(p)) {
            return i;
        }
    }
    ZONES.len() - 1
}

/// Morceaux de carte qui ne sont pas des bâtiments (sol, routes, détails, versions lointaines…).
const NOT_BUILDING: [&str; 29] = ["_lod", "slod", "proxy", "decal", "detail", "_dtl", "emissive", "shadow", "ground", "land", "_rd", "rd_", "road", "water", "occl", "terrain", "_ovly", "overlay", "_glue", "railing", "hedge", "_weed", "cable", "logo", "grnd", "wire", "_alpha", "_repair", "_cut"];

/// Liste des bâtiments du jeu : objets HD (pas des props, pas des intérieurs) d'au moins 12 m de côté et 6 m
/// de haut — **estimé**.
/// Prop (et non morceau de carte) : même règle que « Props seulement » du catalogue.
pub fn is_prop_name(n: &str) -> bool {
    n.starts_with("prop_") || n.starts_with("p_") || n.starts_with("v_") || n.contains("_prop_")
}

pub fn build_list(w: &crate::gta::world::GameWorld) -> Vec<Building> {
    let mut map: std::collections::HashMap<u32, Building> = std::collections::HashMap::new();
    for e in &w.ents {
        if e.lod_level != 0 || e.mlo || w.hidden_archs.contains(&e.arch) {
            continue;
        }
        if let Some(b) = map.get_mut(&e.arch) {
            b.count += 1;
            continue;
        }
        let Some(a) = w.archetypes.get(&e.arch) else { continue };
        if a.mlo {
            continue;
        }
        let s = a.bb_max - a.bb_min;
        if s.x.max(s.y) < 12.0 || s.z < 6.0 {
            continue;
        }
        let name = w.name_of(e.arch);
        if is_prop_name(&name) || name.starts_with("hash_") || NOT_BUILDING.iter().any(|k| name.contains(k)) {
            continue;
        }
        let zone = zone_of(&name);
        map.insert(e.arch, Building { arch: e.arch, name, size: s, pos: e.pos, count: 1, zone });
    }
    let mut v: Vec<Building> = map.into_values().collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

impl DwApp {
    /// Pose une copie du bâtiment (distance d'affichage du jeu, orientation 0) à un point du monde.
    pub fn building_place(&mut self, h: u32, at: V3) {
        let Some(w) = self.game.world.as_ref() else { return };
        let name = w.name_of(h);
        let lod = w.archetypes.get(&h).map(|a| a.lod_dist).unwrap_or(300.0).max(150.0);
        self.push_undo();
        let yi = self.ensure_target_ymap();
        let mut e = crate::model::Entity::new(&name, at);
        e.rot = Quat::IDENTITY;
        e.lod_dist = lod;
        let id = e.id;
        self.proj.ymaps[yi].entities.push(e);
        self.set_selection(vec![id]);
        self.snap_if_shift(&[id]);
        self.sync_game_project();
        self.status = format!("Bâtiment « {name} » copié à {:.0} {:.0} {:.0} (visible jusqu'à {lod:.0} m). À l'export : dépendance vers son .ytyp et collision générée.", at.x, at.y, at.z);
    }

    /// Pose un bâtiment complet : tous ses morceaux, à leur place les uns par rapport aux autres (le principal à
    /// `at`, orientation 0), sélectionnés ensemble.
    pub fn whole_place(&mut self, b: &WholeBuilding, at: V3) {
        let Some(w) = self.game.world.as_ref() else { return };
        let parts: Vec<(String, V3, Quat, f32, f32, f32)> = b.parts.iter().map(|(h, p, q, sxy, sz)| (w.name_of(*h), *p, *q, *sxy, *sz, w.archetypes.get(h).map(|a| a.lod_dist).unwrap_or(300.0).max(150.0))).collect();
        self.push_undo();
        let yi = self.ensure_target_ymap();
        let mut ids = Vec::new();
        for (name, p, q, sxy, sz, lod) in parts {
            let mut e = crate::model::Entity::new(&name, at + p);
            e.rot = q;
            e.scale_xy = sxy;
            e.scale_z = sz;
            e.lod_dist = lod;
            ids.push(e.id);
            self.proj.ymaps[yi].entities.push(e);
        }
        let n = ids.len();
        self.set_selection(ids);
        self.sync_game_project();
        self.status = format!("Bâtiment complet « {} » posé : {n} morceau(x), sélectionnés ensemble (déplace-les ou tourne-les d'un bloc). À l'export : dépendances .ytyp et collisions générées.", b.name);
    }

    /// Pose un morceau (de route) du jeu au bout de l'objet sélectionné : même orientation, le long du plus grand
    /// côté de l'objet sélectionné, le bord de départ du nouveau contre le bord d'arrivée de l'ancien (boîtes des
    /// types d'objets du jeu).
    pub fn place_after_selected(&mut self, h: u32) {
        let Some(sel) = self.selected.first().and_then(|id| self.proj.find_id(*id)).and_then(|r| self.proj.world_entity(r)) else {
            self.status = "Sélectionne d'abord le morceau de route au bout duquel poser le suivant.".into();
            return;
        };
        let Some(w) = self.game.world.as_ref() else { return };
        let name = w.name_of(h);
        let (smn, smx) = sel.local_box(&self.proj);
        let (nmn, nmx) = w.arch_box(h);
        let along_x = (smx.x - smn.x) >= (smx.y - smn.y);
        // décalage local : bord max de l'ancien contre bord min du nouveau sur l'axe long, centres alignés sur l'autre
        let off = if along_x {
            v3(smx.x - nmn.x, (smn.y + smx.y) * 0.5 - (nmn.y + nmx.y) * 0.5, smn.z - nmn.z)
        } else {
            v3((smn.x + smx.x) * 0.5 - (nmn.x + nmx.x) * 0.5, smx.y - nmn.y, smn.z - nmn.z)
        };
        let at = sel.pos + sel.rot.rotate(off.mulv(sel.scale()));
        let lod = w.archetypes.get(&h).map(|a| a.lod_dist).unwrap_or(300.0).max(150.0);
        self.push_undo();
        let yi = self.ensure_target_ymap();
        let mut e = crate::model::Entity::new(&name, at);
        e.rot = sel.rot;
        e.lod_dist = lod;
        let id = e.id;
        self.proj.ymaps[yi].entities.push(e);
        self.set_selection(vec![id]);
        self.sync_game_project();
        self.status = format!("« {name} » posé au bout de « {} ». Sélectionné : « Poser à la suite » continue la route.", sel.archetype);
    }

    /// Fenêtre « Bibliothèque des bâtiments ».
    pub fn buildings_window(&mut self, ctx: &egui::Context) {
        if !self.buildings.open {
            return;
        }
        let mut open = true;
        let mut place: Option<(u32, Option<V3>)> = None;
        let mut place_next: Option<u32> = None;
        let mut goto: Option<(V3, f32)> = None;
        let mut place_whole: Option<(WholeBuilding, Option<V3>)> = None;
        self.show_panel(ctx, "batiments", "Bibliothèque des bâtiments", crate::docking::Dock::Floating, [860.0, 600.0], &mut open, |s, ui| {
                let Some(w) = s.game.world.as_ref() else {
                    ui.label("Charge d'abord le jeu (onglet Jeu).");
                    return;
                };
                ui.horizontal(|ui| {
                    if ui.selectable_label(!s.buildings.roads && !s.buildings.whole, RichText::new("Bâtiments").strong()).on_hover_text("Chaque modèle du jeu, un par un").clicked() {
                        s.buildings.roads = false;
                        s.buildings.whole = false;
                        s.buildings.chosen = None;
                    }
                    if ui.selectable_label(s.buildings.whole, RichText::new("Bâtiments complets").strong()).on_hover_text("Chaque bâtiment avec tous ses morceaux (détails, rambardes, enseignes…), posés d'un coup ; seulement des bâtiments").clicked() {
                        s.buildings.roads = false;
                        s.buildings.whole = true;
                        s.buildings.chosen = None;
                    }
                    if ui.selectable_label(s.buildings.roads, RichText::new("Morceaux de route du jeu").strong()).on_hover_text("Chaussées, carrefours et virages modelés par Rockstar, à copier et poser bout à bout").clicked() {
                        s.buildings.roads = true;
                        s.buildings.whole = false;
                        s.buildings.chosen = None;
                    }
                });
                if s.buildings.whole {
                    if s.buildings.whole_list.is_none() {
                        s.buildings.whole_list = Some(build_whole_list(w));
                    }
                    let (p, g) = s.whole_list_ui(ui);
                    if p.is_some() {
                        place_whole = p;
                    }
                    if g.is_some() {
                        goto = g;
                    }
                    return;
                }
                if s.buildings.roads {
                    if s.buildings.road_list.is_none() {
                        s.buildings.road_list = Some(build_road_list(w));
                    }
                    ui.label(RichText::new("Les routes de GTA V sont modelées quartier par quartier : ce sont des morceaux de vraies rues (lignes droites, virages, carrefours), pas des pièces standard. « Poser à la suite » aligne le morceau choisi au bout de celui qui est sélectionné ; les bords ne se raccordent pas toujours parfaitement.").small().weak());
                } else if s.buildings.list.is_none() {
                    s.buildings.list = Some(build_list(w));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("🔍");
                    ui.add(egui::TextEdit::singleline(&mut s.buildings.filter).hint_text("nom : dt1_05, build, tower…").desired_width(220.0));
                    for (i, (n, _)) in ZONES.iter().enumerate() {
                        let on = s.buildings.zone == i;
                        let b = egui::Button::new(if on { RichText::new(*n).strong().color(Color32::BLACK) } else { RichText::new(*n) }).fill(if on { Color32::from_rgb(120, 190, 255) } else { Color32::from_gray(45) }).corner_radius(10.0);
                        if ui.add(b).clicked() {
                            s.buildings.zone = i;
                        }
                    }
                });
                let f = s.buildings.filter.to_lowercase();
                let zone = s.buildings.zone;
                // la liste est sortie le temps de dessiner (l'aperçu a besoin de toute l'application)
                let roads = s.buildings.roads;
                let list = if roads { s.buildings.road_list.take() } else { s.buildings.list.take() }.unwrap_or_default();
                let idx: Vec<usize> = (0..list.len()).filter(|&i| (zone == 0 || list[i].zone == zone) && (f.is_empty() || list[i].name.contains(&f))).collect();
                ui.label(RichText::new(format!("{} {} · clic : voir · double-clic : poser devant la caméra", idx.len(), if roads { "morceaux de route" } else { "bâtiments" })).small().weak());
                ui.separator();
                let mut chosen = s.buildings.chosen;
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(360.0);
                        egui::ScrollArea::vertical().id_salt("bld_list").auto_shrink([false; 2]).max_height(ui.available_height().max(300.0)).show_rows(ui, 22.0, idx.len(), |ui, range| {
                            for k in range {
                                let b = &list[idx[k]];
                                let r = ui.selectable_label(chosen == Some(b.arch), format!("{}  ·  {:.0} × {:.0} × {:.0} m", b.name, b.size.x, b.size.y, b.size.z));
                                if r.clicked() {
                                    chosen = Some(b.arch);
                                }
                                if r.double_clicked() {
                                    place = Some((b.arch, None));
                                }
                            }
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        let Some(h) = chosen else {
                            ui.label("Choisis un bâtiment dans la liste.");
                            return;
                        };
                        let Some(b) = list.iter().find(|b| b.arch == h) else { return };
                        let (name, size, pos, count, zone_n) = (b.name.clone(), b.size, b.pos, b.count, ZONES[b.zone].0);
                        ui.label(RichText::new(&name).strong());
                        ui.label(format!("{:.0} × {:.0} × {:.0} m · {zone_n} · posé {count} fois dans le jeu", size.x, size.y, size.z));
                        s.textured_preview(ui, h, &name);
                        ui.horizontal_wrapped(|ui| {
                            if ui.button(RichText::new("Poser devant la caméra").strong()).clicked() {
                                place = Some((h, None));
                            }
                            if ui.button("Poser au centre de la vue").clicked() {
                                place = Some((h, Some(s.cam.target)));
                            }
                            if ui.button("Aller voir l'original").clicked() {
                                goto = Some((pos, size.x.max(size.y).max(size.z) * 1.6 + 20.0));
                            }
                            if roads && ui.add_enabled(s.selected.len() == 1, egui::Button::new("Poser à la suite")).on_hover_text("Au bout du morceau sélectionné, dans le même sens (le long de son plus grand côté)").clicked() {
                                place_next = Some(h);
                            }
                        });
                        ui.label(RichText::new("La copie utilise le modèle et les textures du jeu ; à l'export, DragoWalker déclare son .ytyp d'origine et génère sa collision.").small().weak());
                    });
                });
                s.buildings.chosen = chosen;
                if roads {
                    s.buildings.road_list = Some(list);
                } else {
                    s.buildings.list = Some(list);
                }
            });
        self.buildings.open = open;
        if let Some(h) = place_next {
            self.place_after_selected(h);
        }
        if let Some((h, at)) = place {
            let name = self.game.world.as_ref().map(|w| w.name_of(h)).unwrap_or_default();
            let p = at.unwrap_or_else(|| {
                // devant la caméra, posé au sol (GetSpawnPos 0x06000dfb : 2,5 × le rayon)
                let s = self.spawn_pos(&name);
                let z = self.mouse_ground.map(|g| g.z).or_else(|| self.proj.terrain.as_ref().and_then(|t| t.height_at(s.x, s.y))).unwrap_or(s.z);
                v3(s.x, s.y, z)
            });
            self.building_place(h, p);
        }
        if let Some((b, at)) = place_whole {
            let p = at.unwrap_or_else(|| {
                let s = self.spawn_pos(&b.name);
                let z = self.mouse_ground.map(|g| g.z).or_else(|| self.proj.terrain.as_ref().and_then(|t| t.height_at(s.x, s.y))).unwrap_or(s.z);
                v3(s.x, s.y, z)
            });
            self.whole_place(&b, p);
        }
        if let Some((p, d)) = goto {
            self.game.empty_map = false;
            self.goto(p, d);
        }
    }

    /// Liste « Bâtiments complets » : recherche, quartiers, aperçu de l'ensemble, pose.
    /// Rend (bâtiment à poser et où, endroit à aller voir).
    fn whole_list_ui(&mut self, ui: &mut egui::Ui) -> (Option<(WholeBuilding, Option<V3>)>, Option<(V3, f32)>) {
        let (mut place, mut goto) = (None, None);
        ui.horizontal_wrapped(|ui| {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut self.buildings.filter).hint_text("nom : dt1_05, build, tower…").desired_width(220.0));
            for (i, (n, _)) in ZONES.iter().enumerate() {
                let on = self.buildings.zone == i;
                let b = egui::Button::new(if on { RichText::new(*n).strong().color(Color32::BLACK) } else { RichText::new(*n) }).fill(if on { Color32::from_rgb(120, 190, 255) } else { Color32::from_gray(45) }).corner_radius(10.0);
                if ui.add(b).clicked() {
                    self.buildings.zone = i;
                }
            }
        });
        let f = self.buildings.filter.to_lowercase();
        let zone = self.buildings.zone;
        let list = self.buildings.whole_list.take().unwrap_or_default();
        let idx: Vec<usize> = (0..list.len()).filter(|&i| (zone == 0 || list[i].zone == zone) && (f.is_empty() || list[i].name.contains(&f))).collect();
        ui.label(RichText::new(format!("{} bâtiments complets · clic : voir · double-clic : poser devant la caméra", idx.len())).small().weak());
        ui.separator();
        let mut chosen = self.buildings.chosen;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(360.0);
                egui::ScrollArea::vertical().id_salt("bld_whole_list").auto_shrink([false; 2]).max_height(ui.available_height().max(300.0)).show_rows(ui, 22.0, idx.len(), |ui, range| {
                    for k in range {
                        let b = &list[idx[k]];
                        let r = ui.selectable_label(chosen == Some(b.core), format!("{}  ·  {} morceau(x)  ·  {:.0} × {:.0} × {:.0} m", b.name, b.parts.len(), b.size.x, b.size.y, b.size.z));
                        if r.clicked() {
                            chosen = Some(b.core);
                        }
                        if r.double_clicked() {
                            place = Some((b.clone(), None));
                        }
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                let Some(b) = chosen.and_then(|h| list.iter().find(|b| b.core == h)) else {
                    ui.label("Choisis un bâtiment dans la liste.");
                    return;
                };
                ui.label(RichText::new(&b.name).strong());
                ui.label(format!("{} morceau(x) · {:.0} × {:.0} × {:.0} m · {} · posé {} fois dans le jeu", b.parts.len(), b.size.x, b.size.y, b.size.z, ZONES[b.zone].0, b.count));
                let parts = b.parts.clone();
                let key = crate::gta::crypto::joaat(&format!("dw_whole_{}", b.name));
                self.textured_preview_with(ui, key, &b.name, &|w| crate::library::merged_tex_model(w, &parts));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(RichText::new("Poser devant la caméra").strong()).clicked() {
                        place = Some((b.clone(), None));
                    }
                    if ui.button("Poser au centre de la vue").clicked() {
                        place = Some((b.clone(), Some(self.cam.target)));
                    }
                    if ui.button("Aller voir l'original").clicked() {
                        goto = Some((b.pos, b.size.x.max(b.size.y).max(b.size.z) * 1.6 + 20.0));
                    }
                });
                if let Some(w) = self.game.world.as_ref() {
                    let names: Vec<String> = b.parts.iter().map(|p| w.name_of(p.0)).collect();
                    ui.label(RichText::new(format!("Morceaux : {}", names.join(", "))).small().weak());
                }
                ui.label(RichText::new("Les morceaux sont posés ensemble et restent sélectionnés : déplace-les ou tourne-les d'un bloc. Si le modèle principal contient aussi un voisin, pose-le puis « Éditer ce bâtiment » pour l'enlever.").small().weak());
            });
        });
        self.buildings.chosen = chosen;
        self.buildings.whole_list = Some(list);
        (place, goto)
    }

    /// Export : manifeste des dépendances (_manifest.ymf) du projet et collisions des bâtiments du jeu copiés.
    pub fn export_buildings(&self, stream: &std::path::Path, lod_ymaps: &[crate::model::Ymap]) -> Result<Option<String>, String> {
        // fichiers d'un export précédent
        if let Ok(rd) = std::fs::read_dir(stream) {
            for f in rd.flatten() {
                let n = f.file_name().to_string_lossy().to_lowercase();
                if (n.starts_with("dw_bcol_") || n.starts_with("_manifest.ymf")) && n.ends_with(".xml") && f.path().is_file() {
                    let _ = std::fs::remove_file(f.path());
                }
            }
        }
        let w = self.game.world.as_deref();
        let names = crate::terrain::material_names(w);
        let concrete = names.iter().position(|n| n == "CONCRETE").unwrap_or(1) as u32;
        // .ytyp qui définit un type d'objet : d'abord les .ytyp du projet, puis ceux du jeu
        let ytyp_of = |arch: &str| -> Option<String> {
            let low = arch.to_lowercase();
            for t in &self.proj.ytyps {
                if t.mlos.iter().any(|m| m.name.eq_ignore_ascii_case(&low)) || t.raw_archetypes.iter().any(|r| r.to_lowercase().contains(&format!("<name>{low}</name>"))) {
                    return Some(t.name.clone());
                }
            }
            let w = w?;
            // « hash_XXXXXXXX » : type dont le nom n'est pas connu, écrit par son code
            let h = low.strip_prefix("hash_").and_then(|x| u32::from_str_radix(x, 16).ok()).unwrap_or_else(|| crate::gta::crypto::joaat(&low));
            let a = w.archetypes.get(&h)?;
            Some(w.vfs.files[a.ytyp_file].name.trim_end_matches(".ytyp").to_string())
        };
        // comme EditProjectManifestPanel::GenerateProjectManifest (0x06001194) :
        // chaque carte déclare les .ytyp de tous ses objets ; INTERIOR_DATA si elle place un intérieur ;
        // chaque .ytyp d'intérieur déclare les .ytyp des objets de l'intérieur (sauf lui-même) ;
        // les intérieurs sont listés dans <Interiors> (Name, Bounds = leur nom)
        let mut imaps: Vec<(String, bool, Vec<String>)> = Vec::new();
        let mut ncol = 0;
        for y in self.proj.ymaps.iter().filter(|y| !y.sample).chain(lod_ymaps.iter()) {
            let mut itypes: Vec<String> = Vec::new();
            let mut interior = false;
            for e in &y.entities {
                if e.mlo.is_some() {
                    interior = true;
                }
                if let Some(yt) = ytyp_of(&e.archetype) {
                    if !itypes.contains(&yt) {
                        itypes.push(yt);
                    }
                }
                // bâtiment du jeu copié : collision depuis son modèle, dans le monde (pas pour une version lointaine)
                let Some(w) = w else { continue };
                if e.lod_level == "LODTYPES_DEPTH_LOD" {
                    continue;
                }
                let h = crate::gta::crypto::joaat(&e.archetype.to_lowercase());
                let Some(a) = w.archetypes.get(&h) else { continue };
                if is_prop_name(&e.archetype.to_lowercase()) || a.mlo || e.archetype.starts_with("dw_") {
                    continue;
                }
                if let Some(m) = w.load_model(h, 60000) {
                    let s = v3(e.scale_xy, e.scale_xy, e.scale_z);
                    let verts: Vec<V3> = m.pos.iter().map(|p| e.pos + e.rot.rotate(v3(p[0], p[1], p[2]).mulv(s))).collect();
                    let tris: Vec<(u32, u32, u32, u8)> = m.tris.iter().map(|t| (t[0], t[1], t[2], 0u8)).collect();
                    if !tris.is_empty() {
                        std::fs::write(stream.join(format!("dw_bcol_{ncol}.ybn.xml")), crate::roads::mesh_collision_xml(&verts, &tris, &[concrete])).map_err(|e| e.to_string())?;
                        ncol += 1;
                    }
                }
            }
            if !itypes.is_empty() {
                imaps.push((y.name.clone(), interior, itypes));
            }
        }
        if self.proj.terrain.is_some() {
            imaps.push(("dw_terrain_placement".into(), false, vec!["dw_terrain_types".into()]));
            imaps.push(("dw_terrain_lod".into(), false, vec!["dw_terrain_types".into()]));
        }
        let mut ityps: Vec<(String, Vec<String>)> = Vec::new();
        let mut interiors: Vec<String> = Vec::new();
        for t in &self.proj.ytyps {
            let mut list: Vec<String> = Vec::new();
            for m in &t.mlos {
                interiors.push(m.name.clone());
                for e in m.entities.iter().chain(m.entity_sets.iter().flat_map(|s| s.entities.iter())) {
                    if let Some(yt) = ytyp_of(&e.archetype) {
                        if yt != t.name && !list.contains(&yt) {
                            list.push(yt);
                        }
                    }
                }
            }
            if !list.is_empty() {
                ityps.push((t.name.clone(), list));
            }
        }
        if imaps.is_empty() && ityps.is_empty() {
            return Ok(None);
        }
        let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n<CPackFileMetaData>\n  <MapDataGroups/>\n  <HDTxdBindingArray/>\n  <imapDependencies/>\n");
        if imaps.is_empty() {
            s += "  <imapDependencies_2/>\n";
        } else {
            s += "  <imapDependencies_2>\n";
            for (ymap, interior, list) in &imaps {
                s += &format!("    <Item>\n      <imapName>{ymap}</imapName>\n");
                s += if *interior { "      <manifestFlags>INTERIOR_DATA</manifestFlags>\n" } else { "      <manifestFlags/>\n" };
                s += "      <itypDepArray>\n";
                for t in list {
                    s += &format!("        <Item>{t}</Item>\n");
                }
                s += "      </itypDepArray>\n    </Item>\n";
            }
            s += "  </imapDependencies_2>\n";
        }
        if ityps.is_empty() {
            s += "  <itypDependencies_2/>\n";
        } else {
            s += "  <itypDependencies_2>\n";
            for (ytyp, list) in &ityps {
                s += &format!("    <Item>\n      <itypName>{ytyp}</itypName>\n      <manifestFlags>INTERIOR_DATA</manifestFlags>\n      <itypDepArray>\n");
                for t in list {
                    s += &format!("        <Item>{t}</Item>\n");
                }
                s += "      </itypDepArray>\n    </Item>\n";
            }
            s += "  </itypDependencies_2>\n";
        }
        if interiors.is_empty() {
            s += "  <Interiors/>\n";
        } else {
            s += "  <Interiors itemType=\"CInteriorBoundsFiles\">\n";
            for n in &interiors {
                s += &format!("    <Item>\n      <Name>{n}</Name>\n      <Bounds>\n        <Item>{n}</Item>\n      </Bounds>\n    </Item>\n");
            }
            s += "  </Interiors>\n";
        }
        s += "</CPackFileMetaData>\n";
        std::fs::write(stream.join("_manifest.ymf.xml"), s).map_err(|e| e.to_string())?;
        let n: usize = imaps.iter().map(|d| d.2.len()).sum::<usize>() + ityps.iter().map(|d| d.1.len()).sum::<usize>();
        Ok(Some(format!("Manifeste _manifest.ymf : {n} dépendance(s) .ytyp ({} carte(s), {} intérieur(s)) ; {ncol} collision(s) de bâtiment générée(s).", imaps.len(), interiors.len())))
    }
}
