//! Fenêtre principale de DragoWalker : état, actions communes et boucle d'affichage.
//! Les panneaux sont dans ui.rs, la vue 3D dans view.rs.

use crate::ai::{self, Action, AiReply, ChatLine};
use crate::camera::Camera;
use crate::math::{v3, Quat, Rng, V3};
use crate::mech;
use crate::model::{self, Entity, ItemRef, MloInstData, Project, Ymap, Ytyp};
use crate::render::ViewOpts;
use crate::sample;
use crate::xmlio;
use eframe::egui::{self, Color32, Pos2, RichText};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Place,
    Erase,
    /// sélection en rectangle : glisser n'importe où trace un rectangle (même sur le sol ou un bâtiment)
    Rect,
    /// pinceaux de l'éditeur de terrain
    Terrain,
    /// véhicules garés, PNJ, rondes
    Life,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Game,
    Selection,
    Props,
    Interior,
    Ai,
    Options,
    Lists,
    Terrain,
}

pub enum Drag {
    /// glisser d'une poignée (déplacer / tourner / agrandir)
    Gizmo(crate::gizmo::GizmoDrag),
    None,
    Rect(Pos2),
    Move(V3),
    Erase,
    Place(V3),
}

/// Point de l'écran utilisé par les tests : le centre de la vue, décalé de DW_TEST_AT="dx,dy" pixels.
pub fn test_point(rect: egui::Rect) -> Pos2 {
    let off = std::env::var("DW_TEST_AT").ok().and_then(|v| {
        let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if p.len() == 2 { Some(egui::vec2(p[0], p[1])) } else { None }
    });
    rect.center() + off.unwrap_or(egui::Vec2::ZERO)
}

pub struct DwApp {
    pub proj: Project,
    pub cam: Camera,
    pub selected: Vec<u64>,
    pub sel_set: HashSet<u64>,
    pub hover: Option<u64>,
    pub tool: Tool,
    pub tab: Tab,
    pub place_arch: String,
    pub target_ymap: usize,
    pub rand_rot: bool,
    pub rand_scale: bool,
    pub stroke_spacing: f32,
    pub place_into_mlo: bool,
    pub snap: bool,
    pub snap_size: f32,
    pub erase_radius: f32,
    pub erase_filter: String,
    pub erase_buildings: bool,
    pub area_radius: f32,
    pub undo: Vec<Project>,
    pub redo: Vec<Project>,
    pub status: String,
    pub frame: u64,
    pub shot_path: Option<PathBuf>,
    pub shot_sent: bool,
    pub shot_wait: u32,
    pub speed_mult: f32,
    pub drag: Drag,
    pub cat_filter: String,
    pub cat_cat: &'static str,
    pub chat: Vec<ChatLine>,
    pub chat_input: String,
    pub api_key: String,
    pub use_claude: bool,
    /// moteur d'IA choisi et ses réglages
    pub ai_provider: ai::Provider,
    pub claude_code: Option<PathBuf>,
    pub cc_model: String,
    pub oa_base: String,
    pub oa_key: String,
    pub oa_model: String,
    pub cli_cmd: String,
    pub cli_tools: Vec<(&'static str, &'static str)>,
    pub pending: Option<Receiver<Result<AiReply, String>>>,
    pub mlo_name: String,
    pub mlo_cols: u32,
    pub mlo_rows: u32,
    pub mlo_w: f32,
    pub mlo_d: f32,
    pub mlo_h: f32,
    pub cur_mlo: Option<(usize, usize)>,
    pub opts: ViewOpts,
    pub project_path: Option<PathBuf>,
    pub rng: Rng,
    pub ground_z: f32,
    pub mouse_ground: Option<V3>,
    pub move_step: f32,
    pub game: crate::game_ui::GameState,
    pub sollumz: crate::sollumz::SollumzState,
    /// génération des modèles personnels déjà envoyés à la carte graphique
    pub sollumz_sent: u64,
    /// modèles des ressources MLO importées
    pub mlo_state: crate::mlo::MloState,
    /// édition d'un bâtiment posé (morceaux retirés à la main)
    pub bedit: Option<crate::bedit::BEdit>,
    pub cut_state: crate::bedit::CutState,
    /// dernière position / orientation commune de chaque intérieur relié à son bâtiment (pour savoir lequel a bougé)
    pub mlo_link_seen: std::collections::HashMap<u64, (V3, crate::math::Quat)>,
    /// fenêtre « Contrôle avant l'export »
    pub check_open: bool,
    /// groupe qui suit la souris pour être posé
    pub group_place: Option<crate::groups::GroupPlace>,
    /// fenêtre « Faire un groupe »
    pub group_new: Option<crate::groups::NewGroup>,
    /// fenêtre « Groupes du projet »
    pub groups_open: bool,
    /// bibliothèque « Mes groupes » (tous les projets)
    pub group_lib: crate::groups::GroupLib,
    /// bloc ouvert par double-clic : ses pièces se choisissent une par une
    pub group_open: Option<u64>,
    /// signature des pièces et portails dont les coques sont dans la carte graphique
    pub shell_sig: u64,
    /// serveur FiveM de test
    pub server: crate::server::ServerState,
    /// parties laissées de côté par l'export en cours (test en jeu) : « traffic », « peds », « water », « veg »,
    /// « mlo:<nom> » (voir server.rs, TEST_PARTS)
    pub export_skip: Vec<String>,
    /// export pour le test en jeu : conversion CodeWalker laissée au fil du serveur (dll, stream, dossier)
    pub export_defer_convert: bool,
    pub pending_convert: Option<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)>,
    /// poignées : mode et poignée sous la souris
    pub gizmo_mode: crate::gizmo::GizmoMode,
    pub gizmo_hover: Option<u8>,
    /// axes de l'objet au lieu de ceux du monde
    pub gizmo_object_space: bool,
    /// bibliothèque de props (fenêtre)
    pub library: crate::library::Library,
    /// listes du jeu (scénarios, intérieurs, LOD, lumières)
    pub lists: crate::lists::Lists,
    /// bibliothèque des intérieurs
    pub intlib: crate::intlib::IntLib,
    /// éditeur de terrain
    pub terrain: crate::terrain::TerrainState,
    pub terrain_last_upload: f64,
    /// objets copiés (Ctrl+C), dans le monde
    pub clipboard: Vec<Entity>,
    /// panneaux de gauche et de droite cachés (Ctrl+U, comme le panneau d'outils de CodeWalker)
    pub panels_hidden: bool,
    /// fenêtres à part : Assistant IA, Créateur de terrain
    pub ai_open: bool,
    pub terrain_open: bool,
    /// place de chaque panneau (flottant, à gauche, à droite, en bas)
    pub docks: std::collections::HashMap<String, crate::docking::Dock>,
    /// glisser-déposer des panneaux
    pub dock_drag: Option<crate::docking::DockDrag>,
    pub float_pos: std::collections::HashMap<&'static str, egui::Pos2>,
    pub float_rects: std::collections::HashMap<&'static str, egui::Rect>,
    /// panneaux Projet et Boîte à outils ouverts
    pub project_open: bool,
    pub toolbox_open: bool,
    /// passe d'affichage des panneaux : 0 = ceux rangés en bas, 1 = les autres
    pub dock_phase: u8,
    /// vie de la carte : véhicules garés, PNJ, circulation
    pub life: crate::life::LifeState,
    /// sélection vue seule (bouton « Isoler »)
    pub isolate_sel: Option<std::collections::HashSet<u64>>,
    /// aperçus des types de route
    pub road_thumbs: std::collections::HashMap<u8, egui::TextureHandle>,
    /// poser au sol : modèles gardés pour le lancer de rayon, appui sur Maj seul
    pub snap_cache: std::collections::HashMap<u32, Option<std::sync::Arc<crate::gta::drawable::Mesh>>>,
    pub shift_since: Option<f64>,
    pub shift_tap: bool,
    /// point de contact au sol de chaque morceau de carte du jeu (relevé à son emplacement d'origine)
    pub snap_offsets: std::collections::HashMap<u32, Option<f32>>,
    /// bibliothèque des bâtiments du jeu
    pub buildings: crate::buildings::BuildingLib,
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("DragoWalker").join("settings.json"))
}

/// Réglages enregistrés (clé API, dossier du jeu, zones actives…).
pub fn load_settings() -> serde_json::Value {
    settings_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}))
}

pub fn save_setting(key: &str, value: serde_json::Value) {
    if let Some(p) = settings_path() {
        let mut v = load_settings();
        v[key] = value;
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, v.to_string());
    }
}

fn load_api_key() -> String {
    load_settings()["api_key"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .unwrap_or_default()
}

pub fn save_api_key(k: &str) {
    save_setting("api_key", serde_json::json!(k));
}

fn setup_style(ctx: &egui::Context) {
    // toujours le thème sombre, même si Windows est en mode clair
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(37, 40, 46);
    v.window_fill = Color32::from_rgb(37, 40, 46);
    v.extreme_bg_color = Color32::from_rgb(24, 26, 30);
    v.selection.bg_fill = Color32::from_rgb(214, 120, 30);
    v.hyperlink_color = Color32::from_rgb(255, 170, 70);
    v.widgets.hovered.bg_fill = Color32::from_rgb(70, 76, 88);
    v.widgets.active.bg_fill = Color32::from_rgb(214, 120, 30);
    ctx.set_visuals(v);
    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = egui::vec2(6.0, 5.0);
    s.spacing.button_padding = egui::vec2(7.0, 3.0);
    ctx.set_style(s);
}

impl DwApp {
    pub fn new(cc: &eframe::CreationContext, shot_path: Option<PathBuf>, open_files: Vec<PathBuf>) -> Self {
        setup_style(&cc.egui_ctx);
        crate::update::start();
        let mut proj = Project { name: "Nouveau projet".into(), ymaps: vec![sample::generate()], ytyps: vec![], terrain: None, mlo_res: vec![], mlo_rot_fixed: true, building_cuts: vec![], mlo_links: vec![], mlo_link_yaw: vec![], groups: vec![] };
        let first = proj.next_ymap_name();
        proj.ymaps.push(Ymap::new(&first));
        let mut app = DwApp {
            proj,
            cam: Camera::new(),
            selected: Vec::new(),
            sel_set: HashSet::new(),
            hover: None,
            tool: Tool::Select,
            tab: Tab::Props,
            place_arch: "prop_bench_01a".into(),
            target_ymap: 1,
            rand_rot: false,
            rand_scale: false,
            stroke_spacing: 4.0,
            place_into_mlo: true,
            snap: false,
            snap_size: mech::SNAP_GRID_SIZE,
            erase_radius: 15.0,
            erase_filter: String::new(),
            erase_buildings: true,
            area_radius: 60.0,
            undo: Vec::new(),
            redo: Vec::new(),
            status: "Bienvenue dans DragoWalker. Clic droit + glisser : tourner la vue · ZQSD/WASD : se déplacer · molette : zoom.".into(),
            frame: 0,
            shot_path,
            shot_sent: false,
            shot_wait: 0,
            speed_mult: 1.0,
            drag: Drag::None,
            cat_filter: String::new(),
            cat_cat: "Tous",
            chat: vec![ChatLine {
                from_user: false,
                text: "Salut ! Dis-moi ce que tu veux faire sur la carte, par exemple « pose 12 arbres en cercle », « supprime tout dans un rayon de 50 m » ou « crée un intérieur de 4 pièces ».".into(),
            }],
            chat_input: String::new(),
            api_key: load_api_key(),
            use_claude: true,
            ai_provider: ai::Provider::Local,
            claude_code: ai::find_claude_code(),
            cc_model: load_settings()["ai_cc_model"].as_str().unwrap_or("").to_string(),
            oa_base: load_settings()["ai_oa_base"].as_str().unwrap_or("http://localhost:11434/v1").to_string(),
            oa_key: load_settings()["ai_oa_key"].as_str().unwrap_or("").to_string(),
            oa_model: load_settings()["ai_oa_model"].as_str().unwrap_or("").to_string(),
            cli_cmd: load_settings()["ai_cli_cmd"].as_str().unwrap_or("").to_string(),
            cli_tools: ai::find_cli_tools(),
            pending: None,
            mlo_name: "dw_interieur_1".into(),
            mlo_cols: 2,
            mlo_rows: 2,
            mlo_w: 6.0,
            mlo_d: 6.0,
            mlo_h: 3.2,
            cur_mlo: None,
            opts: ViewOpts { show_grid: true, show_rooms: true, show_sample: true },
            project_path: None,
            rng: Rng::new(7),
            ground_z: 0.0,
            mouse_ground: None,
            move_step: 1.0,
            game: crate::game_ui::GameState::new(),
            sollumz: crate::sollumz::SollumzState::new(),
            sollumz_sent: 0,
            mlo_state: Default::default(),
            bedit: None,
            cut_state: Default::default(),
            mlo_link_seen: Default::default(),
            check_open: std::env::var("DW_TEST_CHECK").is_ok(),
            group_place: None,
            group_new: None,
            groups_open: false,
            group_lib: Default::default(),
            group_open: None,
            shell_sig: 0,
            server: crate::server::ServerState::new(),
            export_skip: Vec::new(),
            export_defer_convert: false,
            pending_convert: None,
            gizmo_mode: crate::gizmo::GizmoMode::Move,
            gizmo_hover: None,
            gizmo_object_space: false,
            library: crate::library::Library::new(),
            lists: crate::lists::Lists::default(),
            intlib: crate::intlib::IntLib { open: std::env::var("DW_INTLIB").is_ok(), filter: std::env::var("DW_INTLIB_FILTER").unwrap_or_default(), chosen: None },
            terrain: crate::terrain::TerrainState::default(),
            terrain_last_upload: 0.0,
            clipboard: Vec::new(),
            panels_hidden: false,
            ai_open: std::env::var("DW_AI_WINDOW").is_ok(),
            terrain_open: std::env::var("DW_TERRAIN_WINDOW").is_ok(),
            docks: crate::docking::load_docks(),
            dock_drag: None,
            float_pos: std::collections::HashMap::new(),
            float_rects: std::collections::HashMap::new(),
            project_open: true,
            toolbox_open: true,
            dock_phase: 1,
            road_thumbs: std::collections::HashMap::new(),
            isolate_sel: None,
            life: crate::life::LifeState::default(),
            snap_cache: std::collections::HashMap::new(),
            shift_since: None,
            shift_tap: false,
            snap_offsets: std::collections::HashMap::new(),
            buildings: crate::buildings::BuildingLib::new(),
        };
        // moteur d'IA : celui enregistré, sinon Claude Code s'il est installé, sinon la clé API, sinon le local
        app.ai_provider = load_settings()["ai_provider"].as_str().and_then(ai::Provider::from_key).unwrap_or(if app.claude_code.is_some() {
            ai::Provider::ClaudeCode
        } else if !app.api_key.trim().is_empty() {
            ai::Provider::ClaudeApi
        } else {
            ai::Provider::Local
        });
        // modes test : choix du moteur d'IA sans toucher aux réglages enregistrés
        if let Ok(p) = std::env::var("DW_AI_PROVIDER") {
            if let Some(pr) = ai::Provider::from_key(&p) {
                app.ai_provider = pr;
            }
        }
        if let Ok(v) = std::env::var("DW_AI_CLI") {
            app.cli_cmd = v;
        }
        if let Ok(v) = std::env::var("DW_AI_BASE") {
            app.oa_base = v;
        }
        if let Ok(v) = std::env::var("DW_AI_MODEL") {
            app.oa_model = v;
        }
        // la carte du jeu se charge toute seule si GTA V est trouvé
        if !app.game.dir.is_empty() {
            app.tab = Tab::Game;
            app.start_game_load();
        }
        for f in open_files {
            app.import_file(&f);
        }
        app
    }

    // ------------------------------------------------------------ outils communs

    pub fn push_undo(&mut self) {
        self.undo.push(self.proj.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn do_undo(&mut self) {
        if let Some(p) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.proj, p));
            self.terrain.dirty = true;
            self.clean_selection();
            self.sync_game_project();
            self.status = "Annulé.".into();
        }
    }

    pub fn do_redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.proj, p));
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
            let mut e = self.proj.ent(r).unwrap().clone();
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

    /// Exécute une action de l'assistant.
    pub fn run_action(&mut self, a: &Action) -> String {
        let c = v3(self.cam.target.x, self.cam.target.y, self.ground_z);
        match a {
            Action::Place { archetype, count, pattern, size } => {
                self.push_undo();
                let n = (*count).clamp(1, 500);
                let mut ids = Vec::new();
                let (fwd, right) = self.cam.flat_axes();
                for i in 0..n {
                    let fi = i as f32;
                    let p = match pattern.as_str() {
                        "ligne" => c + right * ((fi - (n as f32 - 1.0) * 0.5) * size),
                        "cercle" => {
                            let a = fi / n as f32 * std::f32::consts::TAU;
                            c + v3(a.cos() * size, a.sin() * size, 0.0)
                        }
                        "grille" => {
                            let side = (n as f32).sqrt().ceil() as u32;
                            let gx = (i % side) as f32 - (side as f32 - 1.0) * 0.5;
                            let gy = (i / side) as f32 - (side as f32 - 1.0) * 0.5;
                            c + right * (gx * size) + fwd * (gy * size)
                        }
                        "aleatoire" => {
                            let a = self.rng.range(0.0, std::f32::consts::TAU);
                            let r = size * self.rng.f().sqrt();
                            c + v3(a.cos() * r, a.sin() * r, 0.0)
                        }
                        _ => c,
                    };
                    let heading = match pattern.as_str() {
                        "aleatoire" => Some(self.rng.range(0.0, 360.0)),
                        "cercle" => Some((fi / n as f32 * 360.0 + 90.0) % 360.0),
                        _ => Some(0.0),
                    };
                    ids.push(self.place_prop(archetype, p, heading));
                }
                self.set_selection(ids);
                format!("{n} × {archetype} posé(s).")
            }
            Action::DeleteRadius { radius, filter } => {
                let ids: HashSet<u64> = self.items_in_radius(c, *radius, filter, true).into_iter().collect();
                if ids.is_empty() {
                    return "Rien à supprimer dans cette zone.".into();
                }
                self.push_undo();
                let n = self.proj.delete_ids(&ids);
                self.clean_selection();
                format!("{n} objet(s) supprimé(s) dans un rayon de {radius} m.")
            }
            Action::DeleteSelection => {
                let n = self.selected.len();
                self.delete_selected();
                format!("{n} objet(s) supprimé(s).")
            }
            Action::Select { filter, radius } => {
                let n = self.select_filtered(filter, *radius);
                format!("{n} objet(s) sélectionné(s).")
            }
            Action::ClearSelection => {
                self.set_selection(vec![]);
                "Sélection vidée.".into()
            }
            Action::MoveSelection { dx, dy, dz } => {
                if self.selected.is_empty() {
                    return "Rien n'est sélectionné.".into();
                }
                self.push_undo();
                self.move_selected(v3(*dx, *dy, *dz));
                "Sélection déplacée.".into()
            }
            Action::RotateSelection { degrees } => {
                if self.selected.is_empty() {
                    return "Rien n'est sélectionné.".into();
                }
                self.push_undo();
                self.rotate_selected(*degrees);
                format!("Sélection tournée de {degrees}°.")
            }
            Action::ScaleSelection { factor } => {
                if self.selected.is_empty() {
                    return "Rien n'est sélectionné.".into();
                }
                self.push_undo();
                for id in self.selected.clone() {
                    if let Some(r) = self.proj.find_id(id) {
                        if let Some(e) = self.proj.ent_mut(r) {
                            e.scale_xy = (e.scale_xy * factor).clamp(0.05, 50.0);
                            e.scale_z = (e.scale_z * factor).clamp(0.05, 50.0);
                        }
                    }
                }
                format!("Taille ×{factor}.")
            }
            Action::DuplicateSelection { dx, dy } => {
                self.duplicate_selected(v3(*dx, *dy, 0.0));
                "Sélection dupliquée.".into()
            }
            Action::CreateMlo { name, cols, rows, width, depth, height } => {
                self.create_mlo(name, (*cols).clamp(1, 8), (*rows).clamp(1, 8), width.clamp(2.0, 60.0), depth.clamp(2.0, 60.0), height.clamp(2.0, 20.0));
                self.status.clone()
            }
            Action::FurnishMlo { style } => self.furnish_mlo(style),
            Action::Goto { x, y } => {
                self.cam.target = v3(*x, *y, self.ground_z);
                format!("Vue centrée sur ({x}, {y}).")
            }
            Action::Undo => {
                self.do_undo();
                "Annulé.".into()
            }
        }
    }

    pub fn scene_summary(&self) -> String {
        let mut s = format!(
            "Centre de la vue : ({:.1}, {:.1}, {:.1}). Sol à z = {:.1}.\nSélection : {} objet(s).\n",
            self.cam.target.x, self.cam.target.y, self.cam.target.z, self.ground_z, self.selected.len()
        );
        for id in self.selected.iter().take(15) {
            if let Some(e) = self.proj.find_id(*id).and_then(|r| self.proj.world_entity(r)) {
                s += &format!("  - {} à ({:.1}, {:.1}, {:.1})\n", e.archetype, e.pos.x, e.pos.y, e.pos.z);
            }
        }
        for y in &self.proj.ymaps {
            s += &format!("Ymap {} : {} objets{}\n", y.name, y.entities.len(), if y.sample { " (carte d'exemple)" } else { "" });
        }
        for t in &self.proj.ytyps {
            for m in &t.mlos {
                s += &format!("Intérieur {} : {} pièces, {} portails, {} objets\n", m.name, m.rooms.len().saturating_sub(1), m.portals.len(), m.entities.len());
            }
        }
        let near = self.items_in_radius(self.cam.target, 60.0, "", true);
        s += &format!("Objets à moins de 60 m du centre : {}\n", near.len());
        s
    }

    pub fn send_chat(&mut self) {
        let text = self.chat_input.trim().to_string();
        if text.is_empty() || self.pending.is_some() {
            return;
        }
        self.chat_input.clear();
        self.chat.push(ChatLine { from_user: true, text: text.clone() });
        if self.ai_provider != ai::Provider::Local {
            let cfg = ai::AiConfig {
                provider: self.ai_provider,
                claude_key: self.api_key.trim().to_string(),
                claude_code: self.claude_code.clone(),
                cc_model: self.cc_model.clone(),
                oa_base: self.oa_base.clone(),
                oa_key: self.oa_key.clone(),
                oa_model: self.oa_model.clone(),
                cli_cmd: self.cli_cmd.clone(),
            };
            self.pending = Some(ai::ask(cfg, self.chat.clone(), self.scene_summary()));
        } else {
            let r = ai::local_parse(&text);
            self.apply_reply(r);
        }
    }

    pub fn apply_reply(&mut self, r: AiReply) {
        if std::env::var("DW_TEST_AI").is_ok() {
            println!("réponse IA : {} | actions {:?}", r.message, r.actions);
        }
        let mut out = r.message;
        for a in &r.actions {
            let res = self.run_action(a);
            out.push_str(&format!("\n✔ {res}"));
        }
        self.status = out.lines().last().unwrap_or("").trim_start_matches("✔ ").to_string();
        self.chat.push(ChatLine { from_user: false, text: out });
    }

    // ------------------------------------------------------------ fichiers

    pub fn import_file(&mut self, path: &std::path::Path) {
        let Ok(src) = std::fs::read_to_string(path) else {
            self.status = format!("Impossible de lire {}", path.display());
            return;
        };
        let fname = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        let base = fname.split('.').next().unwrap_or("import").to_string();
        if fname.to_lowercase().ends_with(".dwproj") {
            self.open_project_str(&src, path);
            return;
        }
        if src.contains("<CMapTypes") {
            match xmlio::read_ytyp(&src, &base) {
                Ok(t) => {
                    self.push_undo();
                    let n = t.mlos.len();
                    self.proj.ytyps.push(t);
                    model::bump_ids(self.proj.max_id());
                    self.status = format!("ytyp « {base} » importé ({n} intérieur(s)).");
                }
                Err(e) => self.status = e,
            }
        } else {
            match xmlio::read_ymap(&src, &base) {
                Ok(y) => {
                    self.push_undo();
                    let n = y.entities.len();
                    // centrer la vue sur le ymap importé et ajuster le sol
                    if n > 0 {
                        let mut zs: Vec<f32> = y.entities.iter().map(|e| e.pos.z).collect();
                        zs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                        let c = y.entities.iter().fold(V3::ZERO, |a, e| a + e.pos) * (1.0 / n as f32);
                        self.ground_z = zs[zs.len() / 4];
                        self.cam.target = v3(c.x, c.y, self.ground_z);
                        if c.dist2d(V3::ZERO) > sample::CITY_HALF * 2.0 {
                            self.opts.show_sample = false;
                        }
                    }
                    self.proj.ymaps.push(y);
                    self.target_ymap = self.proj.ymaps.len() - 1;
                    model::bump_ids(self.proj.max_id());
                    self.status = format!("ymap « {base} » importé ({n} objets). Il devient le ymap de destination.");
                }
                Err(e) => self.status = e,
            }
        }
    }

    pub fn open_project_str(&mut self, src: &str, path: &std::path::Path) {
        match serde_json::from_str::<Project>(src) {
            Ok(mut p) => {
                if !p.ymaps.iter().any(|y| y.sample) {
                    p.ymaps.insert(0, sample::generate());
                }
                self.proj = p;
                model::bump_ids(self.proj.max_id());
                self.undo.clear();
                self.redo.clear();
                self.set_selection(vec![]);
                self.target_ymap = self.proj.ymaps.iter().position(|y| !y.sample).unwrap_or(0);
                self.project_path = Some(path.to_path_buf());
                self.status = format!("Projet ouvert : {}", path.display());
                // projet d'avant la correction des rotations d'intérieur (CMloInstanceDef non inversé)
                let fixed = self.proj.fix_mlo_rotations();
                if fixed > 0 {
                    self.status += &format!(" · Orientation de {fixed} placement(s) d'intérieur corrigée (le tour donné avec les poignées est gardé) : enregistre le projet.");
                    self.sync_game_project();
                }
            }
            Err(e) => self.status = format!("Projet illisible : {e}"),
        }
    }

    pub fn save_project(&mut self, ask: bool) {
        let path = if ask || self.project_path.is_none() {
            rfd::FileDialog::new().add_filter("Projet DragoWalker", &["dwproj"]).set_file_name(format!("{}.dwproj", self.proj.name)).save_file()
        } else {
            self.project_path.clone()
        };
        let Some(path) = path else { return };
        // la carte d'exemple est regénérée à l'ouverture
        let mut p = self.proj.clone();
        p.ymaps.retain(|y| !y.sample);
        match serde_json::to_string_pretty(&p) {
            Ok(s) => match std::fs::write(&path, s) {
                Ok(_) => {
                    self.status = format!("Projet enregistré : {}", path.display());
                    self.project_path = Some(path);
                }
                Err(e) => self.status = format!("Échec de l'enregistrement : {e}"),
            },
            Err(e) => self.status = format!("Échec : {e}"),
        }
    }

    pub fn export_xml(&mut self, fivem: bool) {
        let Some(dir) = rfd::FileDialog::new().set_title("Dossier d'export").pick_folder() else { return };
        self.status = self.export_project(&dir, fivem);
    }
}

impl eframe::App for DwApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame += 1;
        let dt = ctx.input(|i| i.stable_dt).min(mech::MAX_FRAME_STEP);
        self.poll_game_load(ctx);
        self.sollumz_tick(ctx);
        self.mlo_tick();
        self.cuts_tick();
        self.mlo_link_tick();
        self.shells_tick();
        self.server_tick(ctx);
        // capture automatique : attendre la fin du chargement du jeu (au plus ~90 s)
        if self.shot_path.is_some() && self.game.loading.is_some() && self.frame > 2 && self.frame < 6000 {
            self.frame = 2;
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        // mode test : DW_TEST_PROBE pose un banc sur la surface visible au centre de la vue
        static PROBE_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if self.frame == 100 && std::env::var("DW_TEST_PROBE").is_ok() && !PROBE_DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            match self.mouse_ground {
                Some(g) => {
                    println!("sol sous le centre de la vue : {:.2} {:.2} {:.2}", g.x, g.y, g.z);
                    self.push_undo();
                    let id = self.place_prop("prop_bench_01a", g, None);
                    self.set_selection(vec![id]);
                }
                None => println!("sol sous le centre : rien"),
            }
        }
        // mode test : DW_TEST_SPAWN="nom1,nom2" pose ces props comme un double-clic de la bibliothèque
        static SPAWN_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if self.frame == 100 && !SPAWN_DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            if let Ok(v) = std::env::var("DW_TEST_SPAWN") {
                for n in v.split(',') {
                    let at = self.spawn_pos(n);
                    self.library_place(n, Some(at));
                    println!("{n} posé à {:.1} m devant la caméra ({:.1} {:.1} {:.1})", (at - self.cam.eye).len(), at.x, at.y, at.z);
                }
            }
        }
        // modes test : DW_TEST_LISTS=1 (scénarios + lumières), DW_TEST_SMART="nom" (sélection intelligente),
        // DW_TEST_TERRAIN=1 (carte vide, terrain, île, route, pelleteuse)
        static LISTS_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if self.frame == 100 && !LISTS_DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            if std::env::var("DW_TEST_LODS").is_ok() {
                self.tab = Tab::Lists;
                self.lists.sub = 2;
            }
            if let Ok(n) = std::env::var("DW_TEST_ISOLATE") {
                let h = crate::gta::crypto::joaat(&n);
                let pl = self.game.world.as_ref().and_then(|w| (0..w.ents.len() as u32).find(|i| w.ents[*i as usize].arch == h && w.ents[*i as usize].mlo));
                if let Some(pl) = pl {
                    let c = self.game.world.as_ref().map(|w| {
                        let e = &w.ents[pl as usize];
                        let d = w.mlo_defs.get(&e.arch);
                        d.filter(|d| !d.children.is_empty()).map(|d| d.children.iter().fold(V3::ZERO, |a, k| a + e.pos + e.rot.rotate(k.pos)) * (1.0 / d.children.len() as f32)).unwrap_or(e.pos)
                    }).unwrap_or(self.cam.target);
                    self.cam.target = c;
                    self.cam.dist = 22.0;
                    self.game.isolate = Some(pl);
                    println!("isolé : {n} à {:.0} {:.0} {:.0}", c.x, c.y, c.z);
                }
            }
            if std::env::var("DW_TEST_LISTS").is_ok() {
                self.lists_test();
            }
            if let Ok(n) = std::env::var("DW_TEST_SMART") {
                let h = crate::gta::crypto::joaat(&n);
                let ei = self.game.world.as_ref().and_then(|w| (0..w.ents.len() as u32).filter(|i| w.ents[*i as usize].arch == h && w.ymaps[w.ents[*i as usize].ymap as usize].visible).min_by(|a, b| (w.ents[*a as usize].pos - self.cam.target).len().partial_cmp(&(w.ents[*b as usize].pos - self.cam.target).len()).unwrap_or(std::cmp::Ordering::Equal)));
                if let Some(ei) = ei {
                    let ids = self.game_ents_to_project(&[ei]);
                    self.set_selection(ids);
                    self.smart_select();
                    println!("{}", self.status);
                }
            }
            if std::env::var("DW_TEST_TERRAIN").is_ok() {
                self.game.empty_map = true;
                let c = self.cam.target;
                self.proj.terrain = Some(crate::terrain::Terrain::new(c, 400.0, -4.0, 0));
                if let Some(t) = self.proj.terrain.as_mut() {
                    t.island(c, 110.0, 16.0);
                    t.road(c + v3(-90.0, -30.0, 0.0), c + v3(80.0, 40.0, 0.0), 10.0);
                    t.brush(crate::terrain::Brush::Flatten, c + v3(30.0, -40.0, 0.0), 25.0, 20.0, 1.0, 0, 12.0);
                    t.brush(crate::terrain::Brush::Paint, c + v3(30.0, -40.0, 0.0), 22.0, 1.0, 1.0, 4, 0.0);
                    let hc = t.height_at(c.x, c.y).unwrap_or(0.0);
                    let hf = t.height_at(c.x + 30.0, c.y - 40.0).unwrap_or(0.0);
                    println!("terrain : {}×{} sommets, hauteur au centre {hc:.1} m, dalle aplanie à {hf:.1} m", t.nx, t.ny);
                }
                // DW_TEST_TERRAIN_PRESET=n : modèle de départ n ; DW_TEST_ROADS=1 : une route de chaque type ;
                // DW_TEST_VEG=n : sème l'ensemble de végétation n en quelques coups de pinceau
                if let Some(p) = std::env::var("DW_TEST_TERRAIN_PRESET").ok().and_then(|v| v.parse::<u8>().ok()) {
                    if let Some(t) = self.proj.terrain.as_mut() {
                        t.generate(p, 7, 4.0);
                        let (mn, mx) = t.bounds();
                        println!("modèle {} : hauteurs {:.1} .. {:.1} m", crate::terrain::PRESETS[p as usize].0, mn.z, mx.z);
                    }
                    self.terrain.preset = p;
                }
                if std::env::var("DW_TEST_ROADS").is_ok() {
                    for k in 0..crate::roads::KINDS.len() {
                        let y = c.y - 140.0 + k as f32 * 60.0;
                        self.terrain.road_kind = k as u8;
                        let h = |x: f32, y: f32| self.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                        self.terrain.road_pts = vec![v3(c.x - 170.0, y, h(c.x - 170.0, y)), v3(c.x - 40.0, y + 25.0, h(c.x - 40.0, y + 25.0)), v3(c.x + 60.0, y - 20.0, h(c.x + 60.0, y - 20.0)), v3(c.x + 170.0, y + 10.0, h(c.x + 170.0, y + 10.0))];
                        self.finish_road();
                        println!("{}", self.status);
                    }
                }
                // DW_TEST_CROSS=k : deux routes du type k qui se croisent au centre (feux, stops)
                if let Some(k) = std::env::var("DW_TEST_CROSS").ok().and_then(|v| v.parse::<u8>().ok()) {
                    let h = |s: &DwApp, x: f32, y: f32| s.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                    for (a, b) in [(v3(c.x - 90.0, c.y + 5.0, 0.0), v3(c.x + 90.0, c.y - 5.0, 0.0)), (v3(c.x - 4.0, c.y - 90.0, 0.0), v3(c.x + 4.0, c.y + 90.0, 0.0))] {
                        self.terrain.road_kind = k;
                        self.terrain.road_pts = vec![v3(a.x, a.y, h(self, a.x, a.y)), v3(b.x, b.y, h(self, b.x, b.y))];
                        self.finish_road();
                        println!("{}", self.status);
                    }
                    let (l, s) = self.signals_count();
                    println!("signalisation : {l} feu(x), {s} stop(s)");
                    if let Some(j) = self.proj.terrain.as_ref().and_then(|t| t.junctions.first()).cloned() {
                        println!("   branches {:?} -> {:?}", j.arms.iter().map(|a| (a.hw, a.total, a.len)).collect::<Vec<_>>(), crate::signals::arm_controls(&j));
                        for (n, p, q) in crate::signals::junction_props(&j) {
                            println!("      {n} à ({:.1}, {:.1}, {:.2}) cap {:.0}°", p.x - j.pos[0], p.y - j.pos[1], p.z, q.heading_deg());
                        }
                        self.goto(v3(j.pos[0], j.pos[1], j.pos[2]), 55.0);
                        self.cam.target.z = j.pos[2];
                        self.cam.snap();
                    }
                }
                // DW_TEST_ROADJOIN=1 : une avenue qui croise toutes les routes, une rue qui finit en T sur la première
                if std::env::var("DW_TEST_ROADJOIN").is_ok() {
                    let h = |s: &DwApp, x: f32, y: f32| s.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                    self.terrain.road_kind = 1;
                    let (x0, y0, y1) = (c.x + 100.0, c.y - 175.0, c.y + 175.0);
                    self.terrain.road_pts = vec![v3(x0, y0, h(self, x0, y0)), v3(x0 + 10.0, c.y, h(self, x0 + 10.0, c.y)), v3(x0, y1, h(self, x0, y1))];
                    self.finish_road();
                    println!("{}", self.status);
                    self.terrain.road_kind = 0;
                    let start = v3(c.x - 60.0, c.y - 190.0, 0.0);
                    let end = self.proj.terrain.as_ref().and_then(|t| t.roads.first()).map(|r| { let s = r.samples[r.samples.len() / 3]; v3(s[0], s[1], s[2]) }).unwrap_or(start);
                    let start = v3(end.x - 20.0, end.y - 60.0, 0.0);
                    self.terrain.road_pts = vec![v3(start.x, start.y, h(self, start.x, start.y)), end];
                    self.finish_road();
                    println!("{}", self.status);
                    let (l, s) = self.signals_count();
                    println!("signalisation : {l} feu(x), {s} stop(s)");
                    if let Some(t) = self.proj.terrain.as_ref() {
                        for (i, j) in t.junctions.iter().enumerate() {
                            println!("   carrefour {i} à {:?} : branches {:?} -> {:?}", j.pos, j.arms.iter().map(|a| a.hw).collect::<Vec<_>>(), crate::signals::arm_controls(j));
                            for (n, p, q) in crate::signals::junction_props(j) {
                                println!("      {n} à ({:.1}, {:.1}, {:.2}) cap {:.0}°", p.x, p.y, p.z, q.heading_deg());
                            }
                        }
                    }
                    // DW_TEST_JOIN_LOOK=n : la caméra va sur le carrefour n
                    if let Some(n) = std::env::var("DW_TEST_JOIN_LOOK").ok().and_then(|v| v.parse::<usize>().ok()) {
                        if let Some(j) = self.proj.terrain.as_ref().and_then(|t| t.junctions.get(n)).cloned() {
                            self.goto(v3(j.pos[0], j.pos[1], j.pos[2]), 45.0);
                            self.cam.target.z = j.pos[2];
                            self.cam.snap();
                        }
                    }
                }
                // DW_TEST_LIFE=1 : une rangée de parking, un PNJ qui fume, un PNJ qui fait une ronde
                if std::env::var("DW_TEST_LIFE").is_ok() {
                    let h = |s: &DwApp, x: f32, y: f32| s.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                    let yi = self.ensure_target_ymap();
                    for i in 0..6 {
                        let (x, y) = (c.x - 30.0 + i as f32 * crate::life::CARGEN_WIDTH, c.y + 20.0);
                        let z = h(self, x, y);
                        self.proj.ymaps[yi].car_gens.push(crate::life::CarGen::new(v3(x, y, z + 0.1), 90.0, ""));
                    }
                    let z = h(self, c.x + 5.0, c.y + 25.0);
                    self.proj.ymaps[yi].peds.push(crate::life::PedSpot { pos: v3(c.x + 5.0, c.y + 25.0, z), heading: 180.0, model: "a_m_y_business_01".into(), scenario: "WORLD_HUMAN_SMOKING".into(), route: Vec::new(), wait: 0.0 });
                    let route: Vec<V3> = [(c.x + 15.0, c.y + 15.0), (c.x + 35.0, c.y + 15.0), (c.x + 35.0, c.y + 35.0)].iter().map(|(x, y)| v3(*x, *y, h(self, *x, *y))).collect();
                    self.proj.ymaps[yi].peds.push(crate::life::PedSpot { pos: route[0], heading: 0.0, model: "s_m_m_security_01".into(), scenario: "WORLD_HUMAN_GUARD_STAND".into(), route, wait: 5.0 });
                    self.life.open = true;
                    self.tool = Tool::Life;
                    println!("vie : 6 véhicules garés, 2 PNJ");
                }
                if let Some(sec) = std::env::var("DW_TERRAIN_SECTION").ok().and_then(|v| v.parse::<u8>().ok()) {
                    self.terrain_section(sec);
                }
                if let Some(vs) = std::env::var("DW_TEST_VEG").ok().and_then(|v| v.parse::<usize>().ok()) {
                    self.terrain.veg_set = vs.min(crate::vegetation::SETS.len() - 1);
                    self.terrain.veg_density = 0.6;
                    let mut n = 0;
                    for (dx, dy) in [(-120.0, 150.0), (0.0, 160.0), (120.0, 150.0), (-150.0, -170.0), (150.0, -170.0)] {
                        n += self.veg_paint(c + v3(dx, dy, 0.0), 35.0);
                    }
                    println!("végétation {} : {n} objet(s) posé(s), {} au total", crate::vegetation::SETS[self.terrain.veg_set].name, self.veg_count());
                }
                // DW_TEST_BIOME=n : peuple tout le terrain avec le biome n
                if let Some(bi) = std::env::var("DW_TEST_BIOME").ok().and_then(|v| v.parse::<usize>().ok()) {
                    let msg = self.apply_biome(bi);
                    println!("biome : {msg}");
                    println!("étiquettes d'herbe : {:?}", crate::grass::proc_ids(self.game.world.as_deref()));
                }
                // DW_TEST_WATER=1 : un bassin creusé et quelques effets ; DW_TEST_BUILDING=nom : copie d'un bâtiment
                if std::env::var("DW_TEST_WATER").is_ok() {
                    if let Some(t) = self.proj.terrain.as_mut() {
                        let (x0, y0, x1, y1) = (c.x + 40.0, c.y - 90.0, c.x + 110.0, c.y - 40.0);
                        let z = t.height_at(x0, y0).unwrap_or(5.0) + 0.2;
                        for j in 0..t.ny {
                            for i in 0..t.nx {
                                let (x, y) = (t.origin.x + i as f32 * t.cell, t.origin.y + j as f32 * t.cell);
                                if x > x0 && x < x1 && y > y0 && y < y1 {
                                    t.h[j * t.nx + i] = z - 2.3;
                                }
                            }
                        }
                        t.waters.push(crate::scenery::WaterZone { min: [x0, y0], max: [x1, y1], z });
                        for (k, (dx, dy)) in [(-20.0f32, -60.0f32), (-30.0, -50.0), (-40.0, -60.0)].iter().enumerate() {
                            let (x, y) = (c.x + dx, c.y + dy);
                            let zz = t.height_at(x, y).unwrap_or(0.0) + 0.3;
                            t.emitters.push(crate::scenery::Emitter { pos: v3(x, y, zz), fx: crate::scenery::FX[k * 7].name.to_string(), scale: crate::scenery::FX[k * 7].scale });
                        }
                    }
                    println!("eau et effets ajoutés");
                }
                // DW_TEST_ROADPIECE=nom : liste des morceaux de route du jeu, en pose un puis deux à la suite
                if let Ok(name) = std::env::var("DW_TEST_ROADPIECE") {
                    if let Some(w) = self.game.world.as_ref() {
                        let l = crate::buildings::build_road_list(w);
                        println!("morceaux de route du jeu : {} (ex. {:?})", l.len(), l.iter().step_by((l.len() / 12).max(1)).take(12).map(|b| format!("{} {:.0}x{:.0}x{:.1}", b.name, b.size.x, b.size.y, b.size.z)).collect::<Vec<_>>());
                    }
                    let h = crate::gta::crypto::joaat(&name);
                    let at = v3(c.x, c.y + 120.0, self.proj.terrain.as_ref().and_then(|t| t.height_at(c.x, c.y + 120.0)).unwrap_or(0.0));
                    self.building_place(h, at);
                    for _ in 0..2 {
                        self.place_after_selected(h);
                    }
                    let ps: Vec<V3> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.archetype == name).map(|e| e.pos).collect();
                    println!("posés : {ps:?} · {}", self.status);
                    if let Some(p) = ps.get(1) {
                        self.goto(*p, 90.0);
                    }
                }
                if let Ok(name) = std::env::var("DW_TEST_BUILDING") {
                    let h = crate::gta::crypto::joaat(&name);
                    let at = v3(c.x - 60.0, c.y - 40.0, self.proj.terrain.as_ref().and_then(|t| t.height_at(c.x - 60.0, c.y - 40.0)).unwrap_or(0.0));
                    self.building_place(h, at);
                    let n = self.game.world.as_ref().map(|w| crate::buildings::build_list(w).len()).unwrap_or(0);
                    println!("bâtiments du jeu : {n} · {}", self.status);
                    // DW_TEST_SNAP=1 : monte la copie de 15 m puis la repose au sol
                    if std::env::var("DW_TEST_SNAP").is_ok() {
                        self.move_selected(v3(0.0, 0.0, 15.0));
                        let zb = self.selected.first().and_then(|id| self.proj.find_id(*id)).and_then(|r| self.proj.world_entity(r)).map(|e| e.pos.z).unwrap_or(0.0);
                        let n = self.snap_selected_to_ground(true);
                        let za = self.selected.first().and_then(|id| self.proj.find_id(*id)).and_then(|r| self.proj.world_entity(r)).map(|e| e.pos.z).unwrap_or(0.0);
                        println!("poser au sol : {n} objet(s), z {zb:.2} -> {za:.2} (terrain {:.2})", self.proj.terrain.as_ref().and_then(|t| t.height_at(at.x, at.y)).unwrap_or(0.0));
                    }
                }
                self.terrain.dirty = true;
                self.tool = Tool::Terrain;
                self.tab = Tab::Terrain;
                if let Ok(dir) = std::env::var("DW_TEST_TERRAIN_EXPORT") {
                    let r = self.export_project(std::path::Path::new(&dir), true);
                    println!("export : {r}");
                }
            }
        }
        // mode test : DW_TEST_MLO="dossier" importe une ressource MLO ; DW_TEST_MLO_MOVE="dx,dy,dz,degrés" la
        // déplace et la tourne (tous ses objets autour de l'objet de référence) ; DW_TEST_MLO_EXPORT="dossier" exporte
        if self.frame == 40 {
            // DW_TEST_PIECES_OPEN="nom" : ouvre la fenêtre « Pièces du MLO » de ce MLO (capture)
            if let Ok(n) = std::env::var("DW_TEST_PIECES_OPEN") {
                if let Some(r) = self.proj.mlo_res.iter().find(|r| r.name == n).cloned() {
                    let p = self.mlo_move(&r).p1;
                    self.goto(p, 45.0);
                }
                self.mlo_state.pieces_open = Some(n);
                self.mlo_state.solo = true;
            }
            if let Ok(dir) = std::env::var("DW_TEST_MLO") {
                let msg = self.import_mlo(std::path::Path::new(&dir));
                println!("import MLO : {msg}");
                if let Some(r) = self.proj.mlo_res.last().cloned() {
                    println!("   auteur {:?}, crédits {:?}, fichiers {:?}, collisions {:?}, ytyp {:?}, cartes {:?}, data {:?}, illisibles {:?}", r.author, r.credit_files, r.assets.len(), r.ybns.len(), r.ytyp_files, r.ymaps, r.data_files, r.unreadable);
                    println!("   modèles lus : {} ({} triangles), référence {:?}", self.mlo_state.models.len(), self.mlo_state.models.iter().map(|m| m.1.idx.len() / 3).sum::<usize>(), r.ref_pos);
                    if let Ok(mv) = std::env::var("DW_TEST_MLO_MOVE") {
                        let v: Vec<f32> = mv.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                        let (d, deg) = (v3(v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0), v.get(2).copied().unwrap_or(0.0)), v.get(3).copied().unwrap_or(0.0));
                        let q = crate::math::Quat::from_z(deg.to_radians());
                        for y in self.proj.ymaps.iter_mut().filter(|y| r.ymaps.contains(&y.name)) {
                            for e in y.entities.iter_mut() {
                                e.pos = q.rotate(e.pos - r.ref_pos) + r.ref_pos + d;
                                e.rot = q.mul(e.rot).normalized();
                            }
                        }
                        let m = self.mlo_move(&r);
                        println!("   déplacé : {:?} -> {:?}, cap {:.1}°", m.p0, m.p1, m.q.heading_deg());
                    }
                }
                if let Ok(out) = std::env::var("DW_TEST_MLO_EXPORT") {
                    let rep = self.export_project(std::path::Path::new(&out), true);
                    println!("export : {rep}");
                }
            }
        }
        // mode test : DW_TEST_GROUP="rayon" : prend les objets détaillés du jeu autour du centre de la vue, en fait un
        // groupe et le pose 80 m plus loin tourné d'un quart de tour (DW_TEST_GROUP_MOVE=1 : en « Déplacer ») ;
        // le groupe reste ensuite sous la souris pour la capture
        if self.game.world.as_ref().map(|w| w.visible_ents > 0).unwrap_or(false) && std::env::var("DW_TEST_GROUP_DONE").is_err() {
            if let Some(r) = std::env::var("DW_TEST_GROUP").ok().and_then(|v| v.parse::<f32>().ok()) {
                std::env::set_var("DW_TEST_GROUP_DONE", "1");
                let at = self.cam.target;
                let all: Vec<(u32, u8)> = self.game.world.as_ref().map(|w| w.ents_in_radius(at, r).into_iter().map(|i| (i, w.ents[i as usize].lod_level)).collect()).unwrap_or_default();
                println!("groupe : {} objet(s) dans le rayon, niveaux {:?}", all.len(), all.iter().map(|a| a.1).collect::<std::collections::BTreeSet<_>>());
                let ents: Vec<u32> = all.iter().filter(|a| a.1 == 0).map(|a| a.0).collect();
                self.push_undo();
                let ids = self.game_ents_to_project(&ents);
                println!("groupe : autour de {:.0} {:.0}, {} objet(s) du jeu, {} dans le projet", at.x, at.y, ents.len(), ids.len());
                self.set_selection(ids);
                let mv = std::env::var("DW_TEST_GROUP_MOVE").is_ok();
                let msg = self.group_from_selection("Test", mv);
                println!("groupe : {msg}");
                if let Some(g) = self.proj.groups.last() {
                    println!("   {} objet(s), emprise {:.1} × {:.1} × {:.1} m", g.items.len(), g.size[0], g.size[1], g.size[2]);
                }
                let before = self.proj.ymaps.iter().map(|y| y.entities.len()).sum::<usize>();
                if let Some(g) = self.group_place.as_mut() {
                    g.yaw_deg = crate::groups::next_yaw(g.yaw_deg, 1.0, true);
                }
                let dest = at + crate::math::v3(80.0, 0.0, 0.0);
                let placed = self.group_place_at(dest);
                let after = self.proj.ymaps.iter().map(|y| y.entities.len()).sum::<usize>();
                println!("pose : {} objet(s) posé(s), objets du projet {before} → {after} ; {}", placed.len(), self.status);
                // bloc : un objet posé donne tout le bloc ; une fois ouvert (double-clic), lui seul
                if let Some(&first) = placed.first() {
                    let block = self.group_block(first).len();
                    self.group_open_piece(first);
                    println!("bloc : {block} objet(s) d'un clic, {} après double-clic", self.group_block(first).len());
                    self.group_open = None;
                }
                // bibliothèque : relue depuis le disque, avec la vignette
                self.group_lib.loaded = false;
                self.group_lib_load();
                for it in &self.group_lib.items {
                    println!("bibliothèque : « {} » {} objet(s), vignette {} triangle(s), {}", it.group.name, it.group.items.len(), it.thumb.as_ref().map(|t| t.tris.len()).unwrap_or(0), it.path.display());
                }
                // DW_TEST_GROUPS_OPEN=1 : fenêtre « Mes groupes » ouverte sur le groupe, pour la capture
                if std::env::var("DW_TEST_GROUPS_OPEN").is_ok() {
                    self.groups_open = true;
                    self.group_lib.sel = Some(0);
                    self.group_place = None;
                }
                self.goto(dest, 60.0);
            }
        }
        // mode test : DW_TEST_PLACE_CONVERT="modèle" : pose une copie du bâtiment au centre de la vue et la transforme
        // en intérieur ; DW_TEST_CONVERT_EXPORT="dossier" exporte ensuite
        if self.game.world.is_some() && std::env::var("DW_TEST_PLACE_CONVERT_DONE").is_err() {
            if let Ok(name) = std::env::var("DW_TEST_PLACE_CONVERT") {
                std::env::set_var("DW_TEST_PLACE_CONVERT_DONE", "1");
                let at = self.cam.target;
                self.building_place(crate::gta::crypto::joaat(&name), at);
                // DW_TEST_OCCLUDER=1 : boîte d'occlusion dans le bâtiment posé
                if std::env::var("DW_TEST_OCCLUDER").is_ok() {
                    let m = self.add_occluders_for_selection();
                    println!("occlusion : {m}");
                }
                let msg = self.convert_to_interior();
                println!("conversion : {msg}");
                self.goto(at, 28.0);
                if let Some(a) = self.game.analysis.as_ref() {
                    println!("analyse : pied {:.2}, étages {:?}, toit {:.2}, murs à {:.1}°", a.pos.z, a.levels.iter().map(|l| (l.0 - a.pos.z, l.1)).collect::<Vec<_>>(), a.roof_z - a.pos.z, a.wall_yaw.to_degrees());
                }
                if let Some((t, m)) = self.cur_mlo {
                    for r in self.proj.ytyps[t].mlos[m].rooms.iter().skip(1) {
                        println!("   pièce {} : {:?} → {:?}", r.name, r.bb_min, r.bb_max);
                    }
                }
                self.cuts_tick();
                // DW_TEST_ROTATE_AFTER="degrés" : tourne ensuite le bâtiment seul ; l'intérieur relié doit suivre
                if let Ok(deg) = std::env::var("DW_TEST_ROTATE_AFTER") {
                    let d: f32 = deg.parse().unwrap_or(30.0);
                    if let Some(&(iid, bid)) = self.proj.mlo_links.last() {
                        self.mlo_link_tick();
                        if let Some(r) = self.proj.find_id(bid) {
                            if let Some(e) = self.proj.ent_mut(r) {
                                e.rot = crate::math::Quat::from_z(d.to_radians()).mul(e.rot).normalized();
                            }
                        }
                        self.mlo_link_tick();
                        let get = |id: u64| self.proj.find_id(id).and_then(|r| self.proj.ent(r)).map(|e| (e.pos, e.rot.heading_deg()));
                        println!("après rotation de {d}° : bâtiment {:?}, intérieur {:?}", get(bid), get(iid));
                    }
                }
                if let Ok(dir) = std::env::var("DW_TEST_CONVERT_EXPORT") {
                    let rep = self.export_project(std::path::Path::new(&dir), true);
                    println!("export : {rep}");
                }
            }
        }
        // mode test : DW_TEST_BEDIT="modèle" : pose une copie du bâtiment au centre de la vue, l'édite et retire les
        // triangles dont le centre est du côté x > 0 du modèle ; DW_TEST_BEDIT_EXPORT="dossier" exporte ensuite
        if self.game.world.is_some() && std::env::var("DW_TEST_BEDIT_DONE").is_err() {
            if let Ok(name) = std::env::var("DW_TEST_BEDIT") {
                std::env::set_var("DW_TEST_BEDIT_DONE", "1");
                let h = crate::gta::crypto::joaat(&name);
                let at = self.cam.target;
                self.building_place(h, at);
                self.bedit_start();
                println!("édition : {}", self.status);
                let half = self.bedit_test_half();
                println!("triangles retirés (x > 0) : {half}");
                self.cuts_tick();
                if let Ok(dir) = std::env::var("DW_TEST_BEDIT_EXPORT") {
                    let rep = self.export_project(std::path::Path::new(&dir), true);
                    println!("export : {rep}");
                }
            }
        }
        // mode test : DW_TEST_READY_EXPORT="dossier" : exporte la ressource FiveM une fois le jeu chargé (projet ouvert
        // en argument), après le contrôle des MLO
        if self.game.world.is_some() && !self.shot_sent {
            if let Ok(dir) = std::env::var("DW_TEST_READY_EXPORT") {
                if !dir.is_empty() && std::env::var("DW_TEST_READY_DONE").is_err() {
                    std::env::set_var("DW_TEST_READY_DONE", "1");
                    self.mlo_tick();
                    for r in &self.proj.mlo_res {
                        println!("MLO {} : cartes {:?}, fichiers {}, collisions {}, scripts {:?}, déplacement {:?}", r.name, r.ymaps, r.assets.len(), r.ybns.len(), r.scripts, { let m = self.mlo_move(r); (m.p1 - m.p0, m.q.heading_deg()) });
                    }
                    println!("cartes du projet : {:?}, destination {}", self.proj.ymaps.iter().map(|y| (y.name.clone(), y.entities.len())).collect::<Vec<_>>(), self.target_ymap);
                    // DW_TEST_SKIP="peds,veg,mlo:nom" : parties laissées de côté (comme « Contenu du test »)
                    self.export_skip = std::env::var("DW_TEST_SKIP").map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
                    // DW_TEST_DEFER=1 : conversion laissée de côté (comme « Tester en jeu »), puis faite et chronométrée
                    self.export_defer_convert = std::env::var("DW_TEST_DEFER").is_ok();
                    // DW_TEST_PICK="fichier=0,fichier=1" : choix de la fenêtre « Pièces du MLO » ; la liste des pièces
                    // de chaque MLO est affichée
                    let picks: Vec<(String, bool)> = std::env::var("DW_TEST_PICK").unwrap_or_default().split(',').filter_map(|s| s.split_once('=')).map(|(f, v)| (f.trim().to_lowercase(), v.trim() == "1")).collect();
                    for ri in 0..self.proj.mlo_res.len() {
                        let r = self.proj.mlo_res[ri].clone();
                        let moved = !self.mlo_move(&r).is_identity();
                        for p in self.mlo_pieces(&r) {
                            println!("pièce {} : {} [{:?}] prise par défaut {}", r.name, p.file, p.kind, crate::mlo::piece_default(p.kind, moved));
                        }
                        for (f, v) in &picks {
                            self.proj.mlo_res[ri].set_pick(f, *v);
                        }
                    }
                    let t0 = std::time::Instant::now();
                    let rep = self.export_project(std::path::Path::new(&dir), true);
                    let ta = t0.elapsed().as_secs_f32();
                    self.export_defer_convert = false;
                    self.export_skip.clear();
                    println!("export : {rep}");
                    println!("durée de l'export (fil de l'interface) : {ta:.1} s");
                    if let Some((dll, stream, out)) = self.pending_convert.take() {
                        let t1 = std::time::Instant::now();
                        let (ok, r) = crate::export::convert_and_tidy(&dll, &stream, &out);
                        println!("conversion (arrière-plan) : {:.1} s, réussie {ok}, {}", t1.elapsed().as_secs_f32(), r.join(" "));
                    }
                }
            }
        }
        // mode test : DW_TEST_SERVER="dossier" : installe le serveur FiveM de test dans ce dossier, le démarre avec la
        // ressource d'outils (sans map), affiche sa console puis l'arrête
        if self.frame == 45 {
            if let Ok(dir) = std::env::var("DW_TEST_SERVER") {
                self.server.dir = dir;
                if !self.server.installed() {
                    self.server.install();
                    let t0 = std::time::Instant::now();
                    while *self.server.installing.lock().unwrap() && t0.elapsed().as_secs() < 600 {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
                println!("installation : {} (installé : {})", self.server.install_msg.lock().unwrap(), self.server.installed());
                // DW_TEST_ADDONS=1 : télécharge et coche les ressources de la communauté
                if self.server.installed() && std::env::var("DW_TEST_ADDONS").is_ok() {
                    for a in crate::server::ADDONS.iter() {
                        self.server.install_addon(a.key);
                        let t0 = std::time::Instant::now();
                        while *self.server.addon_busy.lock().unwrap() && t0.elapsed().as_secs() < 600 {
                            std::thread::sleep(std::time::Duration::from_millis(300));
                        }
                        println!("ressource {} : {} (installée : {})", a.key, self.server.addon_msg.lock().unwrap(), self.server.addon_installed(a.key));
                        self.server.addons_on.push(a.key.to_string());
                    }
                }
                if self.server.installed() {
                    self.server.license = std::env::var("DW_TEST_LICENSE").unwrap_or_default();
                    self.server.resource.clear();
                    println!("config : {:?}", self.server.write_config(v3(195.0, -934.0, 30.0)));
                    println!("démarrage : {:?}", self.server.start());
                    std::thread::sleep(std::time::Duration::from_secs(20));
                    println!("en marche : {}", self.server.running());
                    self.server.send("dw_tp 1 2 3");
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    for l in self.server.console.lock().unwrap().iter() {
                        println!("   | {l}");
                    }
                    self.server.stop();
                }
            }
        }
        // mode test : DW_TEST_DOCKDRAG="clé,x,y[,tenir]" : commence à glisser ce panneau (voir dock_overlay)
        if self.frame == 100 {
            if let Some(k) = std::env::var("DW_TEST_DOCKDRAG").ok().and_then(|v| v.split(',').next().map(|s| s.trim().to_string())) {
                let key: &'static str = match k.as_str() { "projet" => "projet", "outils" => "outils", "terrain" => "terrain", "ia" => "ia", "bibliotheque" => "bibliotheque", "batiments" => "batiments", _ => "interieurs" };
                self.dock_drag = Some(crate::docking::DockDrag { key, title: key.to_string(), floating: false });
            }
        }
        // mode test : DW_TEST_RECT="largeur,hauteur" sélectionne un rectangle au centre de la vue avec l'outil Rectangle
        static RECT_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if self.frame == 100 && !RECT_DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            if let Some(v) = std::env::var("DW_TEST_RECT").ok() {
                let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if p.len() == 2 {
                    self.tool = Tool::Rect;
                    let r = egui::Rect::from_center_size(self.cam.rect.center(), egui::vec2(p[0], p[1]));
                    let n = self.rect_select(r, false);
                    println!("rectangle {}x{} : {n} objet(s) | {}", p[0], p[1], self.status);
                    if std::env::var("DW_TEST_RECT_SCALE").is_ok() && n > 1 {
                        let spread = |s: &DwApp| {
                            let c = s.selection_center();
                            let ps: Vec<V3> = s.selected.iter().filter_map(|id| s.proj.find_id(*id).and_then(|r| s.proj.world_entity(r)).map(|e| e.pos)).collect();
                            ps.iter().map(|p| (*p - c).len()).sum::<f32>() / ps.len().max(1) as f32
                        };
                        let before = spread(self);
                        self.gizmo_mode = crate::gizmo::GizmoMode::Scale;
                        let c = self.selection_center();
                        if let Some((pc, _)) = self.cam.project(c) {
                            if self.gizmo_begin(pc) {
                                self.gizmo_update(pc + egui::vec2(0.0, -50.0));
                                self.drag = Drag::None;
                            }
                        }
                        println!("groupe agrandi : écart moyen au centre {before:.2} m -> {:.2} m | {}", spread(self), self.status);
                    }
                }
            }
        }
        // mode test : DW_TEST_ANALYSE=1 prend le bâtiment sous le point de test, l'analyse et ouvre l'onglet Intérieur
        static ANALYSE_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if self.frame == 100 && std::env::var("DW_TEST_ANALYSE").is_ok() && !ANALYSE_DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            let c = test_point(self.cam.rect);
            // DW_TEST_ANALYSE="nom" : l'objet du jeu de ce nom le plus proche du centre de la vue
            let want = std::env::var("DW_TEST_ANALYSE").unwrap_or_default();
            let named: Option<u32> = self.game.world.as_ref().and_then(|w| {
                let h = crate::gta::crypto::joaat(&want);
                (0..w.ents.len() as u32).filter(|i| w.ents[*i as usize].arch == h && w.ymaps[w.ents[*i as usize].ymap as usize].visible).min_by(|a, b| {
                    let da = (w.ents[*a as usize].pos - self.cam.target).len();
                    let db = (w.ents[*b as usize].pos - self.cam.target).len();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
            });
            let picked = match named {
                Some(ei) => {
                    self.push_undo();
                    self.game_ents_to_project(&[ei]).into_iter().next()
                }
                None => self.pick_any(c, false),
            };
            if let Some(id) = picked {
                self.set_selection(vec![id]);
                self.analyse_selected();
                self.tab = Tab::Interior;
                println!("{}", self.status);
                if let Some(a) = &self.game.analysis {
                    for (z, s) in &a.levels {
                        println!("   étage à {z:.1} m : {s:.0} m²");
                    }
                }
                if std::env::var("DW_TEST_CONVERT").is_ok() {
                    let msg = self.convert_to_interior();
                    println!("{msg}");
                    if let Some((t, m)) = self.cur_mlo {
                        let mlo = &self.proj.ytyps[t].mlos[m];
                        for r in mlo.rooms.iter().skip(1).take(6) {
                            println!("   {} · ambiance « {} » · sol {:.1}", r.name, r.timecycle, r.bb_min.z);
                        }
                        // pièces supprimées : les objets orphelins doivent être rangés à l'export
                        let mut y = self.proj.ytyps[t].clone();
                        let nr = y.mlos[m].rooms.len();
                        y.mlos[m].rooms.truncate(nr - 5);
                        let xml = crate::xmlio::write_ytyp(&y);
                        let attached: usize = xml.split("<attachedObjects").skip(1).map(|p| p.split("</attachedObjects>").next().unwrap_or("").split_whitespace().filter(|w| w.trim_start_matches('>').parse::<u32>().is_ok()).count()).sum();
                        println!("   export après suppression de 5 pièces : {attached} objet(s) rangés dans une pièce sur {}", y.mlos[m].entities.len());
                        for e in mlo.entities.iter().filter(|e| e.archetype.contains("door")).take(3) {
                            println!("   porte {} à {:.1} {:.1} {:.1}, cap {:.0}°", e.archetype, e.pos.x, e.pos.y, e.pos.z, e.rot.heading_deg());
                        }
                    }
                    self.status = msg;
                }
                if let Some(li) = std::env::var("DW_TEST_ROOMS").ok().and_then(|v| v.parse::<usize>().ok()) {
                    self.rooms_from_level(li);
                    println!("{}", self.status);
                    let msg = self.furnish_rooms("salon", Some(1));
                    if let Some((t, m)) = self.cur_mlo {
                        let mlo = &self.proj.ytyps[t].mlos[m];
                        let z: Vec<String> = mlo.entities.iter().map(|e| format!("{} z {:.1}", e.archetype, e.pos.z)).collect();
                        println!("{msg} -> {:?} (sol de la pièce 1 : {:.1})", z, mlo.rooms[1].bb_min.z);
                    }
                    if let Some((t, m)) = self.cur_mlo {
                        for r in self.proj.ytyps[t].mlos[m].rooms.iter().skip(1) {
                            println!("   {} : {:.1}..{:.1} x {:.1}..{:.1}, sol {:.1}, haut {:.1}", r.name, r.bb_min.x, r.bb_max.x, r.bb_min.y, r.bb_max.y, r.bb_min.z, r.bb_max.z);
                        }
                    }
                    if let Some(z) = self.game.analysis.as_ref().and_then(|a| a.levels.get(li)).map(|l| l.0) {
                        self.cam.target.z = z;
                        self.game.cut_on = true;
                        self.game.cut_h = 2.5;
                    }
                }
            }
        }
        // mode test : DW_TEST_GIZMO="z" pose un banc au centre de la vue, puis saisit chaque poignée et glisse de 60 pixels
        // (vers la droite pour les axes X/Y, vers le haut pour Z) dans les trois modes ; la poignée reste affichée en mode `z`
        static GIZMO_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if self.frame == 100 && !GIZMO_DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            if let Ok(final_mode) = std::env::var("DW_TEST_GIZMO") {
                use crate::gizmo::GizmoMode;
                let g = self.mouse_ground.unwrap_or(self.cam.target);
                self.push_undo();
                self.gizmo_object_space = std::env::var("DW_GIZMO_OBJ").is_ok();
                let id = self.place_prop("prop_bench_01a", g, Some(if self.gizmo_object_space { 45.0 } else { 0.0 }));
                self.set_selection(vec![id]);
                let show = |s: &DwApp| s.proj.find_id(id).and_then(|r| s.proj.world_entity(r)).map(|e| format!("pos {:.2} {:.2} {:.2} cap {:.0}° taille {:.2}/{:.2}", e.pos.x, e.pos.y, e.pos.z, e.rot.heading_deg(), e.scale_xy, e.scale_z)).unwrap_or_default();
                println!("banc posé : {}", show(self));
                for mode in [GizmoMode::Move, GizmoMode::Rotate, GizmoMode::Scale] {
                    self.gizmo_mode = mode;
                    for axis in 0..3u8 {
                        let c = self.selection_center();
                        let s = self.gizmo_world_size(c, match mode { GizmoMode::Rotate => crate::gizmo::ROT_PX, _ => crate::gizmo::MOVE_PX });
                        let ax = self.gizmo_axes();
                        let a = ax[axis as usize];
                        // point de saisie : sur la flèche (0,8 × taille) ou sur l'anneau
                        let grab = match mode {
                            GizmoMode::Rotate => {
                                // à mi-chemin entre deux axes : sur cet anneau seulement
                                let (u, v) = [(ax[1], ax[2]), (ax[2], ax[0]), (ax[0], ax[1])][axis as usize];
                                c + (u + v).norm() * s
                            }
                            _ => c + a * (s * 0.8),
                        };
                        let Some((p, _)) = self.cam.project(grab) else { continue };
                        if !self.gizmo_begin(p) {
                            println!("{mode:?} axe {axis} : poignée non trouvée");
                            continue;
                        }
                        let dir = if axis == 2 { egui::vec2(0.0, -60.0) } else { egui::vec2(60.0, 0.0) };
                        for k in 1..=6 {
                            self.gizmo_update(p + dir * (k as f32 / 6.0));
                        }
                        self.drag = Drag::None;
                        println!("{mode:?} axe {axis} : {} | {}", show(self), self.status);
                    }
                }
                self.gizmo_mode = match final_mode.as_str() { "r" => GizmoMode::Rotate, "t" => GizmoMode::Scale, _ => GizmoMode::Move };
            }
        }
        // mode test : DW_TEST_CLICKS="n" clique n fois au point de test (centre, ou DW_TEST_AT="dx,dy") (reclic = objet de derrière), puis cache le dernier (H)
        static CLICKS_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if self.frame == 100 && !CLICKS_DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            if let Some(n) = std::env::var("DW_TEST_CLICKS").ok().and_then(|v| v.parse::<usize>().ok()) {
                let c = test_point(self.cam.rect);
                println!("sous le centre : {} objet(s), point visible {:?}", self.under_cursor(c).len(), self.game.surface);
                // tout ce que le rayon traverse dans la carte entière, dessiné ou non (pour chercher un sol manquant)
                if let Some(w) = self.game.world.as_ref() {
                    let (o, d) = self.cam.ray(c);
                    let drawn: std::collections::HashSet<u32> = self.game.drawn.iter().copied().collect();
                    let mut hits: Vec<(f32, u32)> = (0..w.ents.len() as u32).filter(|ei| w.ymaps[w.ents[*ei as usize].ymap as usize].visible).filter_map(|ei| crate::game_ui::ray_box_pub(w, ei, o, d).map(|_| { let (mn, mx) = w.arch_box(w.ents[ei as usize].arch); ((mx - mn).len(), ei) })).collect();
                    hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    for (t, ei) in hits.iter().take(40) {
                        let e = &w.ents[*ei as usize];
                        let y = &w.ymaps[e.ymap as usize];
                        println!("   taille {t:.0} m : {} lod {} intérieur {} dessiné {} carte {} visible {} groupe {}", w.name_of(e.arch), e.lod_level, e.mlo, drawn.contains(ei), y.name, y.visible, y.group);
                    }
                }
                if let Some(w) = self.game.world.as_ref() {
                    for u in self.under_cursor(c).iter().take(6) {
                        if let crate::game_ui::Under::Game(ei) = u {
                            let e = &w.ents[*ei as usize];
                            let (mn, mx) = w.arch_box(e.arch);
                            println!("   {} boîte {:?} .. {:?}", w.name_of(e.arch), mn, mx);
                        }
                    }
                }
                let mut last = None;
                for k in 0..n {
                    let id = self.pick_any(c, true);
                    let name = id.and_then(|id| self.proj.find_id(id)).and_then(|r| self.proj.ent(r)).map(|e| format!("{} #{} lod={} {:.1},{:.1},{:.1}", e.archetype, e.id, e.lod_level, e.pos.x, e.pos.y, e.pos.z)).unwrap_or_default();
                    println!("clic {} : {name} | {}", k + 1, self.status);
                    last = id;
                }
                if std::env::var("DW_TEST_CTRL").is_ok() {
                    // Ctrl + clic à deux endroits : les deux objets touchés s'ajoutent
                    self.set_selection(Vec::new());
                    for dx in [0.0f32, -330.0, 300.0] {
                        let p = c + egui::vec2(dx, 0.0);
                        if let Some(id) = self.pick_any(p, false) {
                            if self.sel_set.contains(&id) { self.selected.retain(|x| *x != id); self.sel_set.remove(&id); } else { self.add_selection([id]); }
                            let name = self.proj.find_id(id).and_then(|r| self.proj.ent(r)).map(|e| e.archetype.clone()).unwrap_or_default();
                            println!("ctrl+clic ({dx}) : {name} -> {} sélectionné(s)", self.selected.len());
                        }
                    }
                    last = None;
                }
                if let Some(id) = last {
                    self.set_selection(vec![id]);
                    self.game.hidden_ids.insert(id);
                    self.game.click_at = None;
                    println!("après H : {} objet(s) sous le centre, premier = {:?}", self.under_cursor(c).len(), self.pick(c).and_then(|id| self.proj.find_id(id)).and_then(|r| self.proj.ent(r)).map(|e| e.archetype.clone()));
                }
            }
        }
        // capture : attendre aussi que les vrais modèles visibles soient chargés (au plus ~60 s)
        if self.shot_path.is_some() && self.frame > 60 && self.frame < 120 {
            let busy = self.pending.is_some() || self.library.pending > 0 || self.game.gpu.as_ref().map(|g| g.lock().map(|g| g.busy()).unwrap_or(false)).unwrap_or(false);
            if busy && self.shot_wait < 4000 {
                self.shot_wait += 1;
                self.frame = 60;
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        }

        // réponse de Claude
        if let Some(rx) = &self.pending {
            if let Ok(res) = rx.try_recv() {
                self.pending = None;
                match res {
                    Ok(r) => self.apply_reply(r),
                    Err(e) => {
                        if std::env::var("DW_TEST_AI").is_ok() {
                            println!("erreur IA : {e}");
                        }
                        // repli sur l'interpréteur local
                        let last = self.chat.iter().rev().find(|l| l.from_user).map(|l| l.text.clone()).unwrap_or_default();
                        self.chat.push(ChatLine { from_user: false, text: format!("{e}\nJe passe par l'interpréteur local.") });
                        let r = ai::local_parse(&last);
                        self.apply_reply(r);
                    }
                }
            }
        }

        // capture d'écran automatique (--screenshot)
        if self.shot_path.is_some() {
            let shot = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
            if let Some(img) = shot {
                let path = self.shot_path.clone().unwrap();
                if let Some(d) = path.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                let [w, h] = img.size;
                let mut buf = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    buf.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
                }
                let _ = image::save_buffer(&path, &buf, w as u32, h as u32, image::ExtendedColorType::Rgba8);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.frame >= 120 && !self.shot_sent {
                self.shot_sent = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }

        self.shortcuts(ctx);

        crate::update::banner(ctx);
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            self.menu_bar(ui, ctx);
        });
        let now = ctx.input(|i| i.time);
        self.terrain_tick(now);
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(2.0);
            self.toolbar(ui);
            ui.add_space(2.0);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status).color(Color32::from_rgb(220, 220, 220)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let t = self.cam.target;
                    ui.label(format!("Cible {:.1}, {:.1}, {:.1}", t.x, t.y, t.z));
                    ui.separator();
                    if let Some(g) = self.mouse_ground {
                        ui.label(format!("Souris {:.1}, {:.1}", g.x, g.y));
                        ui.separator();
                    }
                    ui.label(format!("{} sélectionné(s)", self.selected.len()));
                    ui.separator();
                    ui.label(format!("{} objets", self.proj.entity_count()));
                });
            });
        });
        // panneaux et fenêtres, chacun à sa place (flottant, à gauche, à droite, en bas)
        for phase in [0u8, 1] {
            self.dock_phase = phase;
            if !self.panels_hidden {
                let mut o = self.project_open;
                self.show_panel(ctx, "projet", "Projet", crate::docking::Dock::Left, [250.0, 300.0], &mut o, |s, ui| s.project_panel(ui));
                self.project_open = o;
                let mut o = self.toolbox_open;
                self.show_panel(ctx, "outils", "Boîte à outils", crate::docking::Dock::Right, [340.0, 300.0], &mut o, |s, ui| s.toolbox(ui));
                self.toolbox_open = o;
            }
            self.library_window(ctx);
            self.buildings_window(ctx);
            self.ai_window(ctx);
            self.terrain_window(ctx);
            self.intlib_window(ctx);
            self.life_window(ctx);
            self.server_window(ctx);
            self.mlo_pieces_window(ctx);
            self.bedit_window(ctx);
            self.check_window(ctx);
        }
        // fenêtres simples (egui::Window) : une seule fois par image, hors des deux passes du rangement des panneaux
        // (dessinées dans les deux, chaque lettre tapée était lue deux fois)
        self.groups_window(ctx);
        self.dock_overlay(ctx);
        self.isolate_banner(ctx);
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(0.0)).show(ctx, |ui| {
            self.view(ui, dt);
        });
        ctx.request_repaint();
    }
}
