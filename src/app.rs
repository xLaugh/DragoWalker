//! Fenêtre principale de DragoWalker : état, actions communes et boucle d'affichage.
//! Les panneaux sont dans ui.rs, la vue 3D dans view.rs.

use crate::ai::{self, AiReply, ChatLine};
use crate::camera::Camera;
use crate::math::{Rng, V3};
use crate::mech;
use crate::model::{Entity, Project, Ymap};
use crate::render::ViewOpts;
use crate::sample;
use eframe::egui::{self, Color32, Pos2};
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
    /// sauvegarde automatique, récupération, journal d'erreurs, confirmation à la fermeture (safety.rs)
    pub safe: crate::safety::Safety,
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
    #[allow(dead_code)]
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
    /// images par seconde (lissé) pour le compteur de performance
    pub fps: f32,
    /// analyse de plantage FiveM (crashlog.rs)
    pub crash: crate::crashlog::CrashState,
    /// résultat du dernier « Contrôler un dossier » (check.rs)
    pub check_folder_report: Vec<(crate::check::Level, String)>,
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
            safe: crate::safety::Safety::new(shot_path.is_none() || std::env::var_os("DW_SAFETY_TEST").is_some()),
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
            opts: ViewOpts { show_grid: true, show_rooms: true, show_sample: true, show_perf: std::env::var_os("DW_TEST_PERF").is_some() },
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
            fps: 60.0,
            crash: crate::crashlog::CrashState::new(),
            check_folder_report: Vec::new(),
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
}

impl eframe::App for DwApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        crate::safety::clean_exit(self.safe.recovery.is_some());
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.run_frame(ctx);
    }
}
