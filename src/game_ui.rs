//! Le jeu GTA V dans DragoWalker : chargement, panneau « Jeu » (carte de base, North Yankton,
//! Cayo Perico, mises à jour, DLC), catalogue de tous les props avec aperçu 3D, et copie
//! automatique d'un ymap du jeu dans le projet dès qu'on modifie ou supprime un de ses objets.

use crate::app::{DwApp, Tab, Tool};
use crate::catalog;
use crate::gta::drawable::Mesh;
use crate::gta::world::{GameWorld, GroupKind, Progress, ProgressHandle};
use crate::math::{v3, Quat, V3};
use crate::model::{Entity, MloInstData, Ymap};
use eframe::egui::{self, epaint::Vertex, Color32, Pos2, RichText, Sense, Stroke};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

/// État « jeu » de l'application.
pub struct GameState {
    pub world: Option<Box<GameWorld>>,
    pub loading: Option<(ProgressHandle, Receiver<Result<GameWorld, String>>)>,
    pub dir: String,
    pub cw_dir: String,
    pub msg: String,
    pub drawn: Vec<u32>,
    pub hover: Option<u32>,
    pub lod: f32,
    pub max_draw: usize,
    pub terrain: bool,
    pub cat_filter: String,
    pub cat_source: u8,
    pub cat_props_only: bool,
    /// catégorie de la bibliothèque (0 = tous), voir library::CATEGORIES
    pub cat_group: u8,
    pub cat_view: Vec<u32>,
    pub cat_key: String,
    pub preview: Option<Mesh>,
    pub preview_for: u32,
    pub preview_yaw: f32,
    /// aperçu de la bibliothèque : dessiner aussi l'intérieur (caché derrière la façade)
    pub preview_inside: bool,
    pub preview_pitch: f32,
    /// ymap du jeu copié dans le projet → identifiants des objets (dans l'ordre du fichier)
    pub links: HashMap<String, Vec<u64>>,
    pub disabled_groups: Vec<String>,
    /// rendu des vrais modèles (carte graphique)
    pub gpu: Option<Arc<Mutex<crate::gpu::Gpu>>>,
    pub real_models: bool,
    /// ombres du soleil
    pub shadows: bool,
    /// coupe : on ne dessine rien au-dessus de (centre de la vue + cut_h) pour voir dans les bâtiments
    pub cut_on: bool,
    pub cut_h: f32,
    /// objets du jeu cachés à la main (touche H), sans les supprimer
    pub hidden: std::collections::HashSet<u32>,
    /// point de la surface visible sous la souris (profondeur de la carte graphique)
    pub surface: Option<crate::math::V3>,
    /// objets des intérieurs dessinés (renderinteriors de CodeWalker)
    pub interiors: bool,
    /// carte vide : la carte du jeu n'est pas dessinée (éditeur de terrain)
    pub empty_map: bool,
    /// groupes de LOD cachés : (niveau, distance d'apparition arrondie à 10 m)
    pub hidden_lod: std::collections::HashSet<(u8, u32)>,
    /// intérieur vu seul (index de l'objet de carte)
    pub isolate: Option<u32>,
    /// objets du projet cachés à la main (touche H)
    pub hidden_ids: HashSet<u64>,
    /// analyse d'un bâtiment (vitres, étages, murs)
    pub analysis: Option<crate::analyse::Analysis>,
    /// dernier clic de choix (écran) et rang de l'objet pris sous la souris : recliquer au même endroit prend celui de derrière
    pub click_at: Option<Pos2>,
    pub click_idx: usize,
}

impl GameState {
    pub fn new() -> GameState {
        let s = crate::app::load_settings();
        let dir = s["game_dir"].as_str().map(|x| x.to_string()).unwrap_or_else(detect_game_dir);
        let cw_dir = s["codewalker_dir"].as_str().map(|x| x.to_string()).unwrap_or_default();
        let disabled_groups = s["disabled_groups"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(|x| x.to_string())).collect()).unwrap_or_default();
        GameState {
            world: None,
            loading: None,
            dir,
            cw_dir,
            msg: String::new(),
            drawn: Vec::new(),
            hover: None,
            lod: s["game_lod"].as_f64().unwrap_or(1.0) as f32,
            max_draw: s["game_max_draw"].as_u64().unwrap_or(7000) as usize,
            terrain: true,
            cat_filter: std::env::var("DW_LIB_FILTER").unwrap_or_default(),
            cat_source: 0,
            cat_props_only: true,
            cat_group: 0,
            cat_view: Vec::new(),
            cat_key: String::from("-"),
            preview: None,
            preview_for: 0,
            preview_yaw: 0.6,
            preview_inside: std::env::var("DW_PREVIEW_INSIDE").is_ok(),
            preview_pitch: 0.45,
            links: HashMap::new(),
            disabled_groups,
            gpu: None,
            real_models: s["real_models"].as_bool().unwrap_or(true),
            shadows: s["shadows"].as_bool().unwrap_or(true),
            // test : DW_CUT="hauteur" active la coupe
            cut_on: std::env::var("DW_CUT").is_ok(),
            cut_h: std::env::var("DW_CUT").ok().and_then(|v| v.parse().ok()).unwrap_or(s["cut_h"].as_f64().unwrap_or(2.5) as f32),
            hidden: std::collections::HashSet::new(),
            surface: None,
            interiors: s["interiors"].as_bool().unwrap_or(true) && std::env::var("DW_NO_INTERIORS").is_err(),
            empty_map: std::env::var("DW_EMPTY").is_ok(),
            hidden_lod: std::collections::HashSet::new(),
            isolate: None,
            hidden_ids: HashSet::new(),
            analysis: None,
            click_at: None,
            click_idx: 0,
        }
    }
}

/// Cherche une installation de GTA V : réglage de CodeWalker, Steam, Rockstar, Epic, X:\Games\GTAV.
pub fn detect_game_dir() -> String {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Some(dll) = crate::gta::keys::find_codewalker_dll("") {
        if let Some(dir) = dll.parent() {
            if let Ok(cfg) = std::fs::read_to_string(dir.join("CodeWalker.exe.config")) {
                if let Some(i) = cfg.find("name=\"GTAFolder\"") {
                    let rest = &cfg[i..];
                    if let (Some(a), Some(b)) = (rest.find("<value>"), rest.find("</value>")) {
                        if a + 7 < b {
                            cands.push(PathBuf::from(&rest[a + 7..b]));
                        }
                    }
                }
            }
        }
    }
    for drive in ["C", "D", "E", "F", "G", "H"] {
        for sub in [
            "Program Files (x86)\\Steam\\steamapps\\common\\Grand Theft Auto V",
            "Program Files\\Rockstar Games\\Grand Theft Auto V",
            "Program Files\\Epic Games\\GTAV",
            "SteamLibrary\\steamapps\\common\\Grand Theft Auto V",
            "Steam\\steamapps\\common\\Grand Theft Auto V",
            "Games\\GTAV",
            "Games\\Grand Theft Auto V",
            "GTAV",
            "Grand Theft Auto V",
        ] {
            cands.push(PathBuf::from(format!("{drive}:\\{sub}")));
        }
    }
    cands.into_iter().find(|p| p.join("x64a.rpf").is_file() && (p.join("GTA5.exe").is_file() || p.join("gta5.exe").is_file())).map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

fn lod_name(l: u8) -> &'static str {
    match l {
        0 => "LODTYPES_DEPTH_HD",
        1 => "LODTYPES_DEPTH_LOD",
        2 => "LODTYPES_DEPTH_SLOD1",
        3 => "LODTYPES_DEPTH_SLOD2",
        4 => "LODTYPES_DEPTH_SLOD3",
        6 => "LODTYPES_DEPTH_SLOD4",
        _ => "LODTYPES_DEPTH_ORPHANHD",
    }
}

fn priority_name(p: u32) -> &'static str {
    match p {
        1 => "PRI_OPTIONAL_HIGH",
        2 => "PRI_OPTIONAL_MEDIUM",
        3 => "PRI_OPTIONAL_LOW",
        _ => "PRI_REQUIRED",
    }
}

/// Un objet du jeu peut-il être choisi / effacé ? (objets détaillés, pas les volumes lointains ni les intérieurs)
fn editable(w: &GameWorld, ei: u32) -> bool {
    let e = &w.ents[ei as usize];
    !e.mlo && (e.lod_level == 0 || e.lod_level == 5)
}

/// Un objet sous la souris : du projet (identifiant) ou du jeu (index dans la carte).
#[derive(Clone, Copy, PartialEq)]
pub enum Under {
    Proj(u64),
    Game(u32),
}

/// Rayon contre la boîte d'un objet du jeu (pour les tests).
pub fn ray_box_pub(w: &GameWorld, ei: u32, o: V3, d: V3) -> Option<f32> {
    ray_box(w, ei, o, d)
}

/// Rayon contre la boîte d'un objet du jeu.
fn ray_box(w: &GameWorld, ei: u32, o: V3, d: V3) -> Option<f32> {
    let e = &w.ents[ei as usize];
    let (mn, mx) = w.arch_box(e.arch);
    let s = v3(e.sxy.max(0.001), e.sxy.max(0.001), e.sz.max(0.001));
    let inv = e.rot.inverse();
    let lo = inv.rotate(o - e.pos);
    let ld = inv.rotate(d);
    let lo = v3(lo.x / s.x, lo.y / s.y, lo.z / s.z);
    let ld = v3(ld.x / s.x, ld.y / s.y, ld.z / s.z);
    let mut t0 = 0.0f32;
    let mut t1 = f32::MAX;
    for k in 0..3 {
        let (o_, d_, a, b) = match k {
            0 => (lo.x, ld.x, mn.x, mx.x),
            1 => (lo.y, ld.y, mn.y, mx.y),
            _ => (lo.z, ld.z, mn.z, mx.z),
        };
        if d_.abs() < 1e-9 {
            if o_ < a || o_ > b {
                return None;
            }
        } else {
            let mut ta = (a - o_) / d_;
            let mut tb = (b - o_) / d_;
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return None;
            }
        }
    }
    Some(t0)
}

impl DwApp {
    // ------------------------------------------------------------ chargement

    pub fn start_game_load(&mut self) {
        if self.game.loading.is_some() || self.game.dir.trim().is_empty() {
            return;
        }
        let dir = PathBuf::from(self.game.dir.trim());
        let cw = self.game.cw_dir.clone();
        let prog: ProgressHandle = Arc::new(Mutex::new(Progress { text: "Démarrage…".into(), frac: 0.0 }));
        let (tx, rx) = std::sync::mpsc::channel();
        let p2 = prog.clone();
        std::thread::spawn(move || {
            let r = GameWorld::load(&dir, &cw, p2);
            if let Ok(w) = &r {
                let mut boxes = HashMap::with_capacity(w.archetypes.len());
                for a in w.archetypes.values() {
                    boxes.insert(w.name_of(a.name), (a.bb_min, a.bb_max));
                }
                catalog::set_game_boxes(boxes);
            }
            let _ = tx.send(r);
        });
        self.game.loading = Some((prog, rx));
        self.game.msg.clear();
        crate::app::save_setting("game_dir", serde_json::json!(self.game.dir.trim()));
        if !self.game.cw_dir.trim().is_empty() {
            crate::app::save_setting("codewalker_dir", serde_json::json!(self.game.cw_dir.trim()));
        }
    }

    pub fn poll_game_load(&mut self, ctx: &egui::Context) {
        let Some((_, rx)) = &self.game.loading else { return };
        ctx.request_repaint();
        let Ok(res) = rx.try_recv() else { return };
        self.game.loading = None;
        match res {
            Ok(mut w) => {
                for g in w.groups.iter_mut() {
                    if self.game.disabled_groups.contains(&g.key) {
                        g.enabled = false;
                    }
                }
                w.refresh_visibility();
                self.game.msg = format!(
                    "Jeu chargé : {} cartes, {} objets placés, {} types d'objets.",
                    w.ymaps.len(),
                    w.ents.len(),
                    w.archetypes.len()
                );
                self.game.gpu = Some(Arc::new(Mutex::new(crate::gpu::Gpu::new(w.model_source()))));
                // vraies textures du sol (terrain, routes) lues dans les dictionnaires partagés du jeu
                crate::groundtex::load(&w);
                self.terrain.dirty = true;
                self.game.world = Some(Box::new(w));
                // la ville d'exemple laisse la place à la vraie carte : Legion Square
                self.opts.show_sample = false;
                self.proj.ymaps.retain(|y| !y.sample);
                if self.proj.ymaps.is_empty() {
                    let n = self.proj.next_ymap_name();
                    self.proj.ymaps.push(Ymap::new(&n));
                }
                self.target_ymap = self.proj.ymaps.iter().position(|y| !y.from_game).unwrap_or(0);
                self.undo.clear();
                self.redo.clear();
                self.clean_selection();
                self.goto(v3(195.0, -934.0, 30.0), 380.0);
                self.opts.show_grid = false;
                // mode test : DW_GOTO="x,y,distance[,carte]"
                if let Ok(g) = std::env::var("DW_GOTO") {
                    let v: Vec<f32> = g.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    if v.len() >= 3 {
                        self.goto(v3(v[0], v[1], 0.0), v[2]);
                    }
                    if v.len() >= 4 && v[3] > 0.0 {
                        self.cam.map_view = true;
                        self.cam.snap();
                    }
                    if v.len() >= 5 {
                        self.cam.target.z = v[4];
                        if v.len() >= 6 {
                            self.cam.pitch = v[5];
                        }
                        self.cam.snap();
                    }
                    // mode test : DW_TEST_ERASE="x,y,rayon" gomme les objets du jeu et écrit le ymap modifié
                    if let Ok(t) = std::env::var("DW_TEST_ERASE") {
                        let v: Vec<f32> = t.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                        if v.len() == 3 {
                            self.erase_radius = v[2];
                            let z = self.game.world.as_ref().and_then(|w| w.ground_min(v[0], v[1])).unwrap_or(0.0);
                            self.push_undo();
                            let n = self.game_erase_at(v3(v[0], v[1], z));
                            let copies: Vec<(String, usize)> = self.proj.ymaps.iter().filter(|y| y.from_game).map(|y| (y.name.clone(), y.entities.len())).collect();
                            println!("gomme : {n} objets supprimés ; copies {copies:?}");
                            for y in self.proj.ymaps.iter().filter(|y| y.from_game) {
                                let _ = std::fs::write(format!("shots/{}.ymap.xml", y.name), crate::xmlio::write_ymap(y, &self.proj));
                            }
                            self.do_undo();
                            println!("après annulation : copies {}, cachés dans le jeu {}", self.proj.ymaps.iter().filter(|y| y.from_game).count(), self.game.world.as_ref().map(|w| w.ymaps.iter().filter(|y| y.in_project).count()).unwrap_or(0));
                            self.do_redo();
                            if let Ok(dir) = std::env::var("DW_TEST_EXPORT") {
                                let r = self.export_project(std::path::Path::new(&dir), true);
                                println!("export : {r}");
                            }
                            println!("après rétablir : copies {}, cachés dans le jeu {}", self.proj.ymaps.iter().filter(|y| y.from_game).count(), self.game.world.as_ref().map(|w| w.ymaps.iter().filter(|y| y.in_project).count()).unwrap_or(0));
                        }
                    }
                    // mode test : DW_TEST_AI="demande" envoie la demande au moteur d'IA choisi
                    if let Ok(q) = std::env::var("DW_TEST_AI") {
                        self.tab = Tab::Ai;
                        self.chat_input = q;
                        self.send_chat();
                    }
                    // mode test : DW_TEST_BLENDER="nom" écrit le modèle pour Sollumz sans ouvrir Blender
                    if let Ok(n) = std::env::var("DW_TEST_BLENDER") {
                        let arch = crate::render::arch_hash(&n);
                        if let Some(w) = self.game.world.as_ref() {
                            let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
                            let l = w.model_source().load(arch, &claimed, &mut Vec::new());
                            if let Some(m) = l.model {
                                let r = crate::sollumz::write_ydr_xml(std::path::Path::new(&self.sollumz.dir), &n, &m, &l.part_keys, &l.textures);
                                println!("modèle pour Blender : {:?} ({} parties, {} textures)", r, m.geoms.len(), l.textures.len());
                            }
                        }
                    }
                    if let Ok(t) = std::env::var("DW_TAB") {
                        if t == "sollumz" {
                            self.tab = Tab::Props;
                            self.place_arch = "dw_caisse".into();
                            let c = self.cam.target;
                            let gz: f32 = std::env::var("DW_Z").ok().and_then(|z| z.parse().ok()).unwrap_or(c.z);
                            for i in 0..6 {
                                self.place_prop("dw_caisse", v3(c.x + i as f32 * 1.6 - 4.0, c.y, gz), Some(i as f32 * 15.0));
                            }
                        }
                        if t == "props" {
                            self.tab = Tab::Props;
                            self.place_arch = "prop_bench_01a".into();
                        }
                        if t == "interieur" {
                            self.tab = Tab::Interior;
                        }
                    }
                }
                self.sync_game_project();
                self.game.cat_key = "-".into();
                self.status = self.game.msg.clone();
            }
            Err(e) => {
                self.game.msg = format!("Impossible de charger le jeu : {e}");
                self.status = self.game.msg.clone();
            }
        }
    }

    /// Place la caméra au-dessus d'un point (hauteur du sol d'après le relief du jeu).
    pub fn goto(&mut self, p: V3, dist: f32) {
        // le relief du jeu donne le point le plus bas (tunnels) et le plus haut (toits) : on vise entre les deux
        let z = self
            .game
            .world
            .as_ref()
            .and_then(|w| w.heightmaps.iter().rev().find(|h| h.contains(p.x, p.y)).and_then(|h| Some((h.height_min(p.x, p.y)? + h.height(p.x, p.y)?) * 0.5)))
            .unwrap_or(p.z);
        self.cam.pan_left = V3::ZERO;
        self.cam.target = v3(p.x, p.y, z);
        self.ground_z = z;
        self.cam.dist = dist;
        self.cam.ortho = dist;
        self.cam.snap();
    }

    fn save_groups(&mut self) {
        if let Some(w) = &self.game.world {
            self.game.disabled_groups = w.groups.iter().filter(|g| !g.enabled).map(|g| g.key.clone()).collect();
            crate::app::save_setting("disabled_groups", serde_json::json!(self.game.disabled_groups));
        }
    }

    // ------------------------------------------------------------ copie dans le projet

    /// Les ymaps du jeu copiés dans le projet sont cachés dans la vue (après annuler / rétablir aussi).
    pub fn sync_game_project(&mut self) {
        let names: HashSet<String> = self.proj.ymaps.iter().filter(|y| y.from_game).map(|y| y.name.to_lowercase()).collect();
        self.game.links.retain(|k, _| names.contains(k));
        let Some(w) = self.game.world.as_mut() else { return };
        let mut changed = false;
        for y in w.ymaps.iter_mut() {
            let v = names.contains(&y.name);
            if y.in_project != v {
                y.in_project = v;
                changed = true;
            }
        }
        if changed {
            w.refresh_visibility();
        }
    }

    /// Copie des ymaps du jeu dans le projet. Rend : (ymap, index local) → identifiant dans le projet.
    pub fn import_game_ymaps(&mut self, list: &[usize]) -> HashMap<(usize, usize), u64> {
        let mut out = HashMap::new();
        let Some(w) = self.game.world.as_mut() else { return out };
        let mut imported = Vec::new();
        for &yi in list {
            if yi >= w.ymaps.len() || w.ymaps[yi].in_project {
                continue;
            }
            let Some(r) = w.ymap_resource(yi) else { continue };
            let Some(g) = crate::gta::meta::parse_ymap(&r) else { continue };
            let name = w.ymaps[yi].name.clone();
            let mut y = Ymap::new(&name);
            y.from_game = true;
            y.parent = if g.parent != 0 { w.name_of(g.parent) } else { String::new() };
            y.flags = g.flags;
            y.content_flags = g.content_flags;
            let mut ids = Vec::with_capacity(g.entities.len());
            for (li, ge) in g.entities.iter().enumerate() {
                let mut e = Entity::new(&w.name_of(ge.arch), v3(ge.pos[0], ge.pos[1], ge.pos[2]));
                let q = Quat { x: ge.rot[0], y: ge.rot[1], z: ge.rot[2], w: ge.rot[3] };
                // le fichier stocke l'inverse de l'orientation, placements d'intérieur compris
                e.rot = if q.is_identity() { q } else { q.inverse().normalized() };
                e.scale_xy = ge.scale_xy;
                e.scale_z = ge.scale_z;
                e.flags = ge.flags;
                e.guid = ge.guid;
                e.parent_index = ge.parent;
                e.lod_dist = ge.lod_dist;
                e.child_lod_dist = ge.child_lod_dist;
                e.lod_level = lod_name(ge.lod_level).to_string();
                e.num_children = ge.num_children as u32;
                e.priority = priority_name(ge.priority).to_string();
                e.ao_mult = ge.ao_mult;
                e.art_ao = ge.art_ao;
                e.tint = ge.tint;
                e.game_index = li as i32;
                if ge.mlo {
                    e.mlo = Some(MloInstData {
                        group_id: ge.group_id,
                        floor_id: ge.floor_id,
                        // garder les variantes d'origine (sinon l'export les perdait)
                        default_entity_sets: g.mlo_sets.iter().find(|(i, _)| *i == li).map(|(_, v)| v.iter().map(|h| w.name_of(*h)).collect()).unwrap_or_default(),
                        num_exit_portals: ge.num_exit_portals,
                        mlo_inst_flags: ge.mlo_inst_flags,
                    });
                }
                ids.push(e.id);
                out.insert((yi, li), e.id);
                y.entities.push(e);
            }
            w.ymaps[yi].in_project = true;
            self.game.links.insert(name.clone(), ids);
            imported.push(name);
            self.proj.ymaps.push(y);
        }
        if !imported.is_empty() {
            w.refresh_visibility();
            self.status = format!(
                "Copié dans le projet pour modification : {} (les générateurs de véhicules et lumières lointaines de ces fichiers ne sont pas encore repris).",
                imported.join(", ")
            );
        }
        // objets de ymaps déjà copiés
        for &yi in list {
            let name = &w.ymaps[yi].name;
            if let Some(ids) = self.game.links.get(name) {
                for (li, id) in ids.iter().enumerate() {
                    out.entry((yi, li)).or_insert(*id);
                }
            }
        }
        out
    }

    fn game_ent_key(&self, ei: u32) -> Option<(usize, usize)> {
        let w = self.game.world.as_ref()?;
        let e = &w.ents[ei as usize];
        let y = &w.ymaps[e.ymap as usize];
        Some((e.ymap as usize, ei as usize - y.first))
    }

    /// Copie dans le projet les ymaps de ces objets du jeu ; rend leurs identifiants.
    pub fn game_ents_to_project(&mut self, ents: &[u32]) -> Vec<u64> {
        let keys: Vec<(usize, usize)> = ents.iter().filter_map(|&e| self.game_ent_key(e)).collect();
        let mut ymaps: Vec<usize> = keys.iter().map(|k| k.0).collect();
        ymaps.sort();
        ymaps.dedup();
        let map = self.import_game_ymaps(&ymaps);
        keys.iter().filter_map(|k| map.get(k).copied()).filter(|id| self.proj.find_id(*id).is_some()).collect()
    }

    // ------------------------------------------------------------ choix à la souris

    /// Objet du jeu sous la souris (parmi ceux dessinés) : (distance, index).
    pub fn game_pick(&self, s: Pos2) -> Option<(f32, u32)> {
        let w = self.game.world.as_ref()?;
        let (o, d) = self.cam.ray(s);
        let cut = self.cut_z();
        if let Some(p) = self.game.surface {
            // le plus petit objet dont la boîte contient le point visible sous la souris
            let mut best: Option<(f32, u32)> = None;
            for &ei in &self.game.drawn {
                if !editable(w, ei) {
                    continue;
                }
                let e = &w.ents[ei as usize];
                let (mn, mx) = w.arch_box(e.arch);
                let l = e.rot.inverse().rotate(p - e.pos);
                let (sx, sz) = (e.sxy.max(0.01), e.sz.max(0.01));
                let l = crate::math::v3(l.x / sx, l.y / sx, l.z / sz);
                let m = 0.15;
                if l.x >= mn.x - m && l.x <= mx.x + m && l.y >= mn.y - m && l.y <= mx.y + m && l.z >= mn.z - m && l.z <= mx.z + m {
                    let size = (mx - mn).len();
                    if best.map(|b| size < b.0).unwrap_or(true) {
                        best = Some((size, ei));
                    }
                }
            }
            if let Some((_, ei)) = best {
                return Some(((p - o).len(), ei));
            }
        }
        let mut best: Option<(f32, u32, f32)> = None;
        for &ei in &self.game.drawn {
            if let Some(cz) = cut {
                if w.ents[ei as usize].pos.z > cz {
                    continue;
                }
            }
            if !editable(w, ei) {
                continue;
            }
            if let Some(t) = ray_box(w, ei, o, d) {
                let (mn, mx) = w.arch_box(w.ents[ei as usize].arch);
                let size = (mx - mn).len();
                let better = match best {
                    None => true,
                    Some(b) => t < b.0 - 0.5 || ((t - b.0).abs() <= 0.5 && size < b.2),
                };
                if better {
                    best = Some((t, ei, size));
                }
            }
        }
        best.map(|b| (b.0, b.1))
    }

    /// Analyse le bâtiment choisi : vitres, sols et étages, murs extérieurs.
    pub fn analyse_selected(&mut self) {
        let Some(id) = self.selected.first().copied() else {
            self.status = "Choisis d'abord un bâtiment (clic dessus).".into();
            return;
        };
        let Some(e) = self.proj.find_id(id).and_then(|r| self.proj.world_entity(r)) else { return };
        let Some(w) = self.game.world.as_ref() else {
            self.status = "Le jeu n'est pas chargé.".into();
            return;
        };
        let h = crate::render::arch_hash(&e.archetype);
        let src = w.model_source();
        let claimed = std::sync::Mutex::new(HashSet::new());
        let mut cache = Vec::new();
        let l = src.load(h, &claimed, &mut cache);
        let Some(m) = l.model else {
            self.status = format!("Modèle de « {} » introuvable.", e.archetype);
            return;
        };
        let a = crate::analyse::analyse(id, &e.archetype, &m, e.pos, e.rot, e.scale_xy, e.scale_z);
        self.status = format!(
            "« {} » : {} étage(s), {:.0} m² de vitres, {:.0} m² de murs extérieurs, {:.0} × {:.0} × {:.0} m.",
            a.name,
            a.levels.len(),
            a.glass_area,
            a.wall_out_area,
            a.size.x,
            a.size.y,
            a.size.z
        );
        self.game.analysis = Some(a);
    }

    /// Hauteur de coupe actuelle (rien n'est dessiné au-dessus), si la coupe est active.
    pub fn cut_z(&self) -> Option<f32> {
        if self.game.cut_on {
            Some(self.cam.target.z + self.game.cut_h)
        } else {
            None
        }
    }

    /// Tout ce qui est sous la souris, du premier au dernier : d'abord les objets qui contiennent le point
    /// visible (le plus petit d'abord), puis les autres du plus proche au plus loin. Ce qui est caché (H) ou
    /// au-dessus de la coupe (C) est ignoré ; un placement d'intérieur passe après ses objets.
    pub fn under_cursor(&self, s: Pos2) -> Vec<Under> {
        let (o, d) = self.cam.ray(s);
        let cut = self.cut_z();
        let surf = self.game.surface;
        // (priorité, clé de tri, objet) ; priorité 0 = contient le point visible
        let mut c: Vec<(u8, f32, Under)> = Vec::new();
        // distance du rayon une fois la coupe passée (None : objet entièrement au-dessus)
        let after_cut = |t: f32, bottom: f32| -> Option<f32> {
            let Some(cz) = cut else { return Some(t) };
            if bottom > cz {
                return None;
            }
            if (o + d * t).z <= cz {
                Some(t)
            } else if d.z < -1e-6 {
                Some(t.max((cz - o.z) / d.z))
            } else {
                None
            }
        };
        let inside = |p: V3, pos: V3, rot: Quat, sxy: f32, sz: f32, mn: V3, mx: V3| -> bool {
            let l = rot.inverse().rotate(p - pos);
            let l = v3(l.x / sxy.max(0.01), l.y / sxy.max(0.01), l.z / sz.max(0.01));
            let m = 0.15;
            l.x >= mn.x - m && l.x <= mx.x + m && l.y >= mn.y - m && l.y <= mx.y + m && l.z >= mn.z - m && l.z <= mx.z + m
        };
        for it in self.proj.all_items() {
            if self.sample_hidden(it) {
                continue;
            }
            let Some(e) = self.proj.world_entity(it) else { continue };
            if self.game.hidden_ids.contains(&e.id) {
                continue;
            }
            let Some(t) = crate::render::ray_hit(&e, &self.proj, o, d) else { continue };
            let (mn, mx) = e.local_box(&self.proj);
            let Some(t) = after_cut(t, e.pos.z + mn.z.min(0.0) * e.scale_z) else { continue };
            let size = (mx - mn).len();
            if e.mlo.is_some() {
                c.push((2, t, Under::Proj(e.id)));
            } else if surf.map(|p| inside(p, e.pos, e.rot, e.scale_xy, e.scale_z, mn, mx)).unwrap_or(false) {
                c.push((0, size, Under::Proj(e.id)));
            } else {
                // comme CodeWalker (UpdateMouseHits 0x060002ab, IL_063b-IL_06b0) : le plus petit rayon gagne
                let _ = t;
                c.push((1, size * 0.5 * e.scale_xy.max(e.scale_z), Under::Proj(e.id)));
            }
        }
        if let Some(w) = self.game.world.as_ref() {
            for &ei in &self.game.drawn {
                if !editable(w, ei) || w.ymaps[w.ents[ei as usize].ymap as usize].in_project {
                    continue;
                }
                let Some(t) = ray_box(w, ei, o, d) else { continue };
                let e = &w.ents[ei as usize];
                let (mn, mx) = w.arch_box(e.arch);
                let Some(t) = after_cut(t, e.pos.z + mn.z.min(0.0) * e.sz) else { continue };
                if surf.map(|p| inside(p, e.pos, e.rot, e.sxy, e.sz, mn, mx)).unwrap_or(false) {
                    c.push((0, (mx - mn).len(), Under::Game(ei)));
                } else {
                    let _ = t;
                    let r = w.archetypes.get(&e.arch).map(|a| a.radius).filter(|r| *r > 0.0).unwrap_or((mx - mn).len() * 0.5);
                    c.push((1, r * e.sxy.max(e.sz), Under::Game(ei)));
                }
            }
        }
        c.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)));
        c.into_iter().map(|x| x.2).collect()
    }

    /// Ce que le survol montre : l'objet qu'un clic prendrait (tient compte des clics répétés au même endroit).
    /// Avec Ctrl (`cycle` faux) : toujours ce que la souris touche en premier, comme CodeWalker.
    pub fn hover_under(&self, s: Pos2, cycle: bool) -> Option<Under> {
        let list = self.under_cursor(s);
        if list.is_empty() {
            return None;
        }
        let i = match self.game.click_at {
            Some(c) if cycle && c.distance(s) < 5.0 => self.game.click_idx % list.len(),
            _ => 0,
        };
        Some(list[i])
    }

    fn take_under(&mut self, u: Under) -> Option<u64> {
        match u {
            Under::Proj(id) => Some(id),
            Under::Game(ei) => {
                self.push_undo();
                self.game_ents_to_project(&[ei]).into_iter().next()
            }
        }
    }

    /// Choix d'un objet par un clic : le premier sous la souris ; recliquer au même endroit prend celui de
    /// derrière (pas besoin de déplacer ce qui est au-dessus). Un objet du jeu est copié dans le projet.
    /// Avec Ctrl (`cycle` faux) : l'objet que la souris touche, sans passer à celui de derrière
    /// (WorldForm::SelectMousedItem 0x060002b7 → SelectItem(LastMouseHit, addSelection = Ctrl)).
    pub fn pick_any(&mut self, s: Pos2, cycle: bool) -> Option<u64> {
        let list = self.under_cursor(s);
        if list.is_empty() {
            self.game.click_at = None;
            return None;
        }
        if !cycle {
            self.game.click_at = None;
            return self.take_under(list[0]);
        }
        let again = self.game.click_at.map(|c| c.distance(s) < 5.0).unwrap_or(false);
        let i = if again { (self.game.click_idx + 1) % list.len() } else { 0 };
        self.game.click_at = Some(s);
        self.game.click_idx = i;
        let n = list.len();
        let id = self.take_under(list[i]);
        if n > 1 {
            let name = id.and_then(|id| self.proj.find_id(id)).and_then(|r| self.proj.ent(r)).map(|e| e.archetype.clone()).unwrap_or_default();
            self.status = format!("« {name} » : objet {} sur {} sous la souris. Reclique au même endroit pour prendre celui de derrière · H = le cacher · C = couper le haut.", i + 1, n);
        }
        id
    }

    /// Choix au début d'un glisser : garde l'objet déjà sélectionné s'il est sous la souris (pour le déplacer).
    pub fn pick_for_drag(&mut self, s: Pos2) -> Option<u64> {
        let list = self.under_cursor(s);
        for u in &list {
            if let Under::Proj(id) = u {
                if self.sel_set.contains(id) {
                    return Some(*id);
                }
            }
        }
        let first = *list.first()?;
        self.game.click_at = Some(s);
        self.game.click_idx = 0;
        self.take_under(first)
    }

    /// Objets du jeu dont le centre est dans un rectangle de l'écran.
    pub fn game_in_rect(&self, r: egui::Rect) -> Vec<u32> {
        let Some(w) = self.game.world.as_ref() else { return Vec::new() };
        self.game
            .drawn
            .iter()
            .copied()
            .filter(|&ei| editable(w, ei))
            .filter(|&ei| self.cam.project(w.ents[ei as usize].pos).map(|(sp, _)| r.contains(sp)).unwrap_or(false))
            .collect()
    }

    /// Gomme sur les objets du jeu : copie leurs ymaps dans le projet puis les supprime.
    pub fn game_erase_at(&mut self, g: V3) -> usize {
        let Some(w) = self.game.world.as_ref() else { return 0 };
        let f = self.erase_filter.to_lowercase();
        let ents: Vec<u32> = w
            .ents_in_radius(g, self.erase_radius)
            .into_iter()
            .filter(|&ei| editable(w, ei))
            .filter(|&ei| {
                let e = &w.ents[ei as usize];
                if !self.erase_buildings {
                    let (mn, mx) = w.arch_box(e.arch);
                    if (mx - mn).len() > 20.0 {
                        return false;
                    }
                }
                f.is_empty() || w.name_of(e.arch).contains(&f)
            })
            .collect();
        if ents.is_empty() {
            return 0;
        }
        let ids: HashSet<u64> = self.game_ents_to_project(&ents).into_iter().collect();
        self.proj.delete_ids(&ids)
    }

    pub fn game_hover_text(&self) -> Option<String> {
        let w = self.game.world.as_ref()?;
        let ei = self.game.hover?;
        let e = &w.ents[ei as usize];
        let y = &w.ymaps[e.ymap as usize];
        Some(format!("{}  ·  {}.ymap ({})", w.name_of(e.arch), y.name, w.groups[y.group].label))
    }

    /// Hauteur du sol sous la souris (relief du jeu) si le jeu est chargé.
    pub fn game_ground(&self, s: Pos2) -> Option<V3> {
        let w = self.game.world.as_ref()?;
        let (o, d) = self.cam.ray(s);
        w.ray_ground(o, d)
    }

    // ------------------------------------------------------------ panneau « Jeu »

    pub fn game_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("GTA V installé sur cet ordinateur").strong());
        ui.label(RichText::new("DragoWalker lit les fichiers du jeu sans jamais les modifier. Les clés de lecture viennent de GTA5.exe et de CodeWalker, comme dans CodeWalker.").weak().small());
        egui::Grid::new("game_dirs").num_columns(3).show(ui, |ui| {
            ui.label("Dossier du jeu");
            ui.add(egui::TextEdit::singleline(&mut self.game.dir).desired_width(190.0).hint_text("ex. E:\\Games\\GTAV"));
            if ui.small_button("…").on_hover_text("Choisir le dossier").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.game.dir = p.to_string_lossy().to_string();
                }
            }
            ui.end_row();
            ui.label("Dossier CodeWalker");
            ui.add(egui::TextEdit::singleline(&mut self.game.cw_dir).desired_width(190.0).hint_text("trouvé tout seul"));
            if ui.small_button("…").on_hover_text("Choisir le dossier de CodeWalker").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.game.cw_dir = p.to_string_lossy().to_string();
                }
            }
            ui.end_row();
        });
        if let Some((prog, _)) = &self.game.loading {
            let (t, f) = prog.lock().map(|g| (g.text.clone(), g.frac)).unwrap_or_default();
            ui.add(egui::ProgressBar::new(f).text(t).animate(true));
        } else {
            let label = if self.game.world.is_some() { "Recharger le jeu" } else { "Charger la carte de GTA V" };
            if ui.add_enabled(!self.game.dir.trim().is_empty(), egui::Button::new(RichText::new(label).strong())).clicked() {
                self.start_game_load();
            }
        }
        if !self.game.msg.is_empty() {
            ui.label(RichText::new(&self.game.msg).small());
        }
        let Some(w) = self.game.world.as_mut() else {
            if self.game.dir.trim().is_empty() {
                ui.label(RichText::new("Indique le dossier où GTA V est installé (celui qui contient GTA5.exe).").weak());
            }
            return;
        };
        ui.separator();
        let mut changed = false;
        let mut goto: Option<(V3, f32)> = None;
        ui.label(RichText::new("Zones de la carte").strong());
        let special = ["base", "north_yankton", "mpheist4"];
        for key in special {
            let Some(gi) = w.groups.iter().position(|g| g.key == key) else { continue };
            ui.horizontal(|ui| {
                let g = &mut w.groups[gi];
                let txt = format!("{}  ({} cartes)", g.label, g.ymaps);
                if ui.checkbox(&mut g.enabled, txt).changed() {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                if ui.small_button("Aller voir").clicked() {
                    goto = Some(match key {
                        "north_yankton" => (group_center(w, gi).unwrap_or(v3(3360.0, -4849.0, 110.0)), 700.0),
                        "mpheist4" => (v3(4890.0, -5170.0, 2.0), 1500.0),
                        _ => (v3(195.0, -934.0, 30.0), 320.0),
                    });
                }
            });
        }
        for (title, kind) in [("Mises à jour du jeu", GroupKind::Update), ("DLC (contenus additionnels)", GroupKind::Dlc)] {
            let idx: Vec<usize> = (0..w.groups.len()).filter(|&i| w.groups[i].kind == kind && !special.contains(&w.groups[i].key.as_str())).collect();
            let on = idx.iter().filter(|&&i| w.groups[i].enabled).count();
            egui::CollapsingHeader::new(format!("{title} — {on} / {} actifs", idx.len())).id_salt(title).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("Tout activer").clicked() {
                        for &i in &idx {
                            w.groups[i].enabled = true;
                        }
                        changed = true;
                    }
                    if ui.small_button("Tout désactiver").clicked() {
                        for &i in &idx {
                            w.groups[i].enabled = false;
                        }
                        changed = true;
                    }
                });
                for &i in &idx {
                    let g = &mut w.groups[i];
                    let txt = if g.ymaps > 0 { format!("{}  ({} cartes)", g.label, g.ymaps) } else { format!("{}  (objets seulement)", g.label) };
                    if ui.checkbox(&mut g.enabled, txt).changed() {
                        changed = true;
                    }
                }
            });
        }
        if changed {
            w.refresh_visibility();
        }
        ui.label(RichText::new(format!("{} objets du jeu actifs sur la carte.", w.visible_ents)).small().weak());
        ui.separator();
        ui.label(RichText::new("Affichage").strong());
        ui.horizontal(|ui| {
            ui.label("Détail");
            if ui.add(egui::Slider::new(&mut self.game.lod, 0.4..=3.0).text("").logarithmic(true)).on_hover_text("À gauche : plus d'objets détaillés de loin (plus lent)").changed() {
                crate::app::save_setting("game_lod", serde_json::json!(self.game.lod));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Objets max");
            if ui.add(egui::Slider::new(&mut self.game.max_draw, 1000..=20000)).changed() {
                crate::app::save_setting("game_max_draw", serde_json::json!(self.game.max_draw));
            }
        });
        if ui.checkbox(&mut self.game.real_models, "Vrais modèles 3D et textures du jeu").on_hover_text("Sinon : des boîtes à la taille réelle (plus léger)").changed() {
            crate::app::save_setting("real_models", serde_json::json!(self.game.real_models));
        }
        if let Some(g) = &self.game.gpu {
            if let Ok(g) = g.lock() {
                if let Some(e) = &g.error {
                    ui.label(RichText::new(format!("Vrais modèles indisponibles : {e}")).color(Color32::from_rgb(255, 140, 110)).small());
                } else if self.game.real_models {
                    ui.label(RichText::new(format!("{} modèles, {} en chargement, {} k triangles affichés", g.stats.0, g.in_flight, g.stats.2 / 1000)).small().weak());
                }
            }
        }
        if self.game.real_models {
            if ui.checkbox(&mut self.game.shadows, "Ombres du soleil").changed() {
                crate::app::save_setting("shadows", serde_json::json!(self.game.shadows));
            }
            if ui.checkbox(&mut self.game.interiors, "Intérieurs des bâtiments (meubles, murs intérieurs)").changed() {
                crate::app::save_setting("interiors", serde_json::json!(self.game.interiors));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Voir dans les bâtiments").strong());
            ui.checkbox(&mut self.game.cut_on, "Couper au-dessus d'une hauteur (touche C)");
            ui.horizontal(|ui| {
                ui.label("Hauteur");
                if ui.add_enabled(self.game.cut_on, egui::Slider::new(&mut self.game.cut_h, -10.0..=60.0).suffix(" m")).changed() {
                    crate::app::save_setting("cut_h", serde_json::json!(self.game.cut_h));
                }
            });
            ui.label(RichText::new("Hauteur comptée depuis le centre de la vue ; molette + Alt pour l'ajuster dans la vue.").weak().small());
            ui.horizontal(|ui| {
                let n = self.game.hidden.len() + self.game.hidden_ids.len();
                ui.label(format!("Objets cachés (touche H sur un objet) : {n}"));
                if ui.add_enabled(n > 0, egui::Button::new("Tout réafficher")).clicked() {
                    self.game.hidden.clear();
                    self.game.hidden_ids.clear();
                }
            });
        } else {
            ui.checkbox(&mut self.game.terrain, "Relief du terrain");
        }
        let copies = self.proj.ymaps.iter().filter(|y| y.from_game).count();
        ui.separator();
        ui.label(RichText::new(format!("Fichiers du jeu copiés dans le projet : {copies}")).small());
        ui.label(RichText::new("Clique, sélectionne au rectangle ou gomme un objet du jeu : son fichier .ymap est copié dans ton projet, où tu peux le modifier. À l'export, la copie remplace l'original (dossier stream de FiveM).").weak().small());
        if changed {
            self.save_groups();
        }
        if let Some((p, d)) = goto {
            self.goto(p, d);
        }
    }

    // ------------------------------------------------------------ catalogue de tous les props

    pub fn refresh_catalog_pub(&mut self) {
        self.refresh_catalog();
    }

    fn refresh_catalog(&mut self) {
        let key = format!("{}|{}|{}|{}", self.game.cat_filter.to_lowercase(), self.game.cat_source, self.game.cat_props_only, self.game.cat_group);
        if key == self.game.cat_key {
            return;
        }
        self.game.cat_key = key;
        let Some(w) = self.game.world.as_ref() else { return };
        let f = self.game.cat_filter.to_lowercase();
        let words: Vec<&str> = f.split_whitespace().collect();
        let src = self.game.cat_source;
        let props_only = self.game.cat_props_only;
        let group = crate::library::CATEGORIES.get(self.game.cat_group as usize).map(|c| c.2).unwrap_or(&[]);
        self.game.cat_view = w
            .catalog
            .iter()
            .copied()
            .filter(|h| {
                let Some(a) = w.archetypes.get(h) else { return false };
                let name = w.names.get(h).map(|s| s.as_str()).unwrap_or("");
                if props_only && !(name.starts_with("prop_") || name.starts_with("p_") || name.starts_with("v_") || name.contains("_prop_")) {
                    return false;
                }
                let g = &w.groups[a.group];
                let ok_src = match src {
                    1 => g.kind == GroupKind::Base,
                    2 => g.key == "mpheist4",
                    3 => g.kind == GroupKind::Dlc || g.kind == GroupKind::Update,
                    _ => true,
                };
                ok_src && words.iter().all(|wd| name.contains(wd)) && crate::library::in_category(name, group)
            })
            .collect();
    }

    pub fn game_catalog(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut self.game.cat_filter).hint_text("Chercher dans tous les props du jeu…").desired_width(220.0));
        });
        ui.horizontal_wrapped(|ui| {
            for (i, l) in ["Tous", "Jeu de base", "Cayo Perico", "DLC / mises à jour"].iter().enumerate() {
                if ui.selectable_label(self.game.cat_source == i as u8, *l).clicked() {
                    self.game.cat_source = i as u8;
                }
            }
        });
        ui.checkbox(&mut self.game.cat_props_only, "Props seulement (prop_, p_, v_…)");
        self.refresh_catalog();
        let Some(w) = self.game.world.as_ref() else { return };
        ui.label(RichText::new(format!("{} modèles", self.game.cat_view.len())).small().weak());
        let row_h = 18.0;
        let mut picked: Option<u32> = None;
        let place_hash = crate::gta::crypto::joaat(&self.place_arch);
        egui::ScrollArea::vertical().max_height(190.0).id_salt("game_cat").show_rows(ui, row_h, self.game.cat_view.len(), |ui, range| {
            for i in range {
                let h = self.game.cat_view[i];
                let name = w.name_of(h);
                if ui.selectable_label(place_hash == h, RichText::new(&name).size(12.5)).clicked() {
                    picked = Some(h);
                }
            }
        });
        if let Some(h) = picked {
            self.place_arch = w.name_of(h);
            self.tool = Tool::Place;
        }
        ui.horizontal(|ui| {
            if ui.button("Modifier ce modèle dans Blender").on_hover_text("Exporte le modèle et ses textures pour Sollumz et ouvre Blender").clicked() {
                let n = self.place_arch.clone();
                self.edit_in_blender(&n);
            }
        });
        // aperçu 3D du modèle choisi
        let h = crate::gta::crypto::joaat(&self.place_arch);
        if h != self.game.preview_for {
            self.game.preview_for = h;
            self.game.preview = match self.sollumz.find(&self.place_arch) {
                Some(c) => Some(crate::sollumz::preview_mesh(c)),
                None => self.game.world.as_ref().and_then(|w| w.load_model(h, 30000)),
            };
        }
        ui.add_space(4.0);
        if self.sollumz.find(&self.place_arch).is_none() {
            let n = self.place_arch.clone();
            self.textured_preview(ui, h, &n);
        } else {
            self.preview_widget(ui);
        }
    }

    pub fn preview_widget(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width().min(330.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 210.0), Sense::drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, Color32::from_rgb(28, 31, 36));
        if resp.dragged() {
            let d = resp.drag_delta();
            self.game.preview_yaw += d.x * 0.012;
            self.game.preview_pitch = (self.game.preview_pitch + d.y * 0.01).clamp(-0.2, 1.4);
        } else {
            self.game.preview_yaw += 0.006;
            ui.ctx().request_repaint();
        }
        let info = self.game.world.as_ref().map(|w| w.arch_box(self.game.preview_for));
        let Some(m) = &self.game.preview else {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Pas d'aperçu pour ce modèle", egui::FontId::proportional(13.0), Color32::GRAY);
            return;
        };
        let c = [(m.min[0] + m.max[0]) * 0.5, (m.min[1] + m.max[1]) * 0.5, (m.min[2] + m.max[2]) * 0.5];
        let ext = ((m.max[0] - m.min[0]).powi(2) + (m.max[1] - m.min[1]).powi(2) + (m.max[2] - m.min[2]).powi(2)).sqrt().max(0.01);
        let scale = rect.height().min(rect.width()) * 0.82 / ext;
        let (sy, cy) = self.game.preview_yaw.sin_cos();
        let (sp, cp) = self.game.preview_pitch.sin_cos();
        let tr = |p: &[f32; 3]| -> [f32; 3] {
            let x = p[0] - c[0];
            let y = p[1] - c[1];
            let z = p[2] - c[2];
            let rx = x * cy - y * sy;
            let ry = x * sy + y * cy;
            // vue légèrement plongeante
            let up = z * cp - ry * sp;
            let depth = ry * cp + z * sp;
            [rx, up, depth]
        };
        let pts: Vec<[f32; 3]> = m.pos.iter().map(tr).collect();
        let light = V3 { x: -0.4, y: 0.6, z: -0.7 }.norm();
        let mut tris: Vec<(f32, [u32; 3], Color32)> = Vec::with_capacity(m.tris.len());
        for t in &m.tris {
            let a = pts[t[0] as usize];
            let b = pts[t[1] as usize];
            let cc = pts[t[2] as usize];
            let ab = v3(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
            let ac = v3(cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]);
            let n = ab.cross(ac).norm();
            let k = 0.35 + 0.65 * n.dot(light).abs();
            let col = Color32::from_rgb((205.0 * k) as u8, (185.0 * k) as u8, (150.0 * k) as u8);
            tris.push(((a[2] + b[2] + cc[2]) / 3.0, *t, col));
        }
        tris.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
        let ctr = rect.center();
        let to_s = |p: [f32; 3]| egui::pos2(ctr.x + p[0] * scale, ctr.y - p[1] * scale);
        let mut mesh = egui::Mesh::default();
        for (_, t, col) in tris {
            let base = mesh.vertices.len() as u32;
            for &i in &t {
                mesh.vertices.push(Vertex { pos: to_s(pts[i as usize]), uv: egui::epaint::WHITE_UV, color: col });
            }
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
            if mesh.vertices.len() > 60000 {
                painter.add(egui::Shape::mesh(std::mem::take(&mut mesh)));
            }
        }
        painter.add(egui::Shape::mesh(mesh));
        if let Some((mn, mx)) = info {
            let sz = mx - mn;
            painter.text(
                rect.left_bottom() + egui::vec2(8.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{}  ·  {:.2} × {:.2} × {:.2} m  ·  {} triangles", self.place_arch, sz.x, sz.y, sz.z, m.tris.len()),
                egui::FontId::proportional(11.5),
                Color32::from_gray(210),
            );
        }
        painter.rect_stroke(rect, 6.0, Stroke::new(1.0, Color32::from_gray(70)), egui::StrokeKind::Inside);
        let _ = Path::new("");
        let _ = Tab::Props;
    }
}

/// Centre (médian) des objets d'un groupe.
fn group_center(w: &GameWorld, gi: usize) -> Option<V3> {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for y in w.ymaps.iter().filter(|y| y.group == gi) {
        xs.push((y.min.x + y.max.x) * 0.5);
        ys.push((y.min.y + y.max.y) * 0.5);
    }
    if xs.is_empty() {
        return None;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let x = xs[xs.len() / 2];
    let y = ys[ys.len() / 2];
    let z = w.ground_min(x, y).unwrap_or(100.0);
    Some(v3(x, y, z))
}
