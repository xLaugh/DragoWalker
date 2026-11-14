//! Vie de la carte : véhicules garés, PNJ immobiles ou qui font une ronde, circulation sur les routes tracées.
//!
//! - Véhicules garés : « générateurs de véhicules » du jeu (CCarGen dans le ymap : position, orientX/orientY,
//!   perpendicularLength, carModel, flags, couleurs, popGroup, livrée) ; valeurs par défaut de CodeWalker
//!   (ProjectForm::NewCarGen 0x06000d4c : flags 3680, orientX 5, perpendicularLength 2,6, couleurs et livrée -1).
//! - PNJ : un script client FiveM (dw_peds.lua) les fait apparaître près du joueur, avec une activité du jeu
//!   (scénario WORLD_HUMAN_…), et leur fait suivre leur ronde point par point.
//! - Circulation : voir traffic.rs.

use crate::app::DwApp;
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, Pos2, RichText};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CarGen {
    pub pos: V3,
    /// direction du capot (degrés, 0 = vers l'est)
    pub heading: f32,
    /// longueur de orientX/orientY (CodeWalker : 5)
    pub length: f32,
    /// perpendicularLength (CodeWalker : 2,6)
    pub width: f32,
    /// modèle (vide = au hasard, selon le quartier)
    pub model: String,
    pub flags: u32,
    pub colors: [i32; 4],
    pub livery: i32,
}

impl CarGen {
    pub fn new(pos: V3, heading: f32, model: &str) -> CarGen {
        CarGen { pos, heading, length: CARGEN_ORIENT, width: CARGEN_WIDTH, model: model.to_string(), flags: CARGEN_FLAGS, colors: [-1; 4], livery: -1 }
    }
}

/// Valeurs de CodeWalker pour un nouveau véhicule garé (ProjectForm::NewCarGen 0x06000d4c).
pub const CARGEN_FLAGS: u32 = 3680;
pub const CARGEN_ORIENT: f32 = 5.0;
pub const CARGEN_WIDTH: f32 = 2.6;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PedSpot {
    pub pos: V3,
    pub heading: f32,
    pub model: String,
    /// activité du jeu (vide = debout sans rien faire)
    pub scenario: String,
    /// ronde : points à suivre en boucle (vide = immobile)
    #[serde(default)]
    pub route: Vec<V3>,
    /// pause à chaque point de la ronde (s)
    #[serde(default)]
    pub wait: f32,
}

/// Activités du jeu (scénarios) — noms des scénarios de GTA V utilisés par TaskStartScenarioInPlace.
pub const SCENARIOS: [(&str, &str); 22] = [
    ("", "Debout, sans rien faire"),
    ("WORLD_HUMAN_SMOKING", "Fume une cigarette"),
    ("WORLD_HUMAN_STAND_MOBILE", "Au téléphone"),
    ("WORLD_HUMAN_DRINKING", "Boit"),
    ("WORLD_HUMAN_AA_COFFEE", "Boit un café"),
    ("WORLD_HUMAN_LEANING", "Adossé au mur"),
    ("WORLD_HUMAN_GUARD_STAND", "Garde debout"),
    ("WORLD_HUMAN_CLIPBOARD", "Avec un bloc-notes"),
    ("WORLD_HUMAN_HANG_OUT_STREET", "Traîne dans la rue"),
    ("WORLD_HUMAN_TOURIST_MAP", "Touriste avec un plan"),
    ("WORLD_HUMAN_MUSCLE_FLEX", "Fait le beau"),
    ("WORLD_HUMAN_SUNBATHE_BACK", "Bronze"),
    ("WORLD_HUMAN_PICNIC", "Pique-nique"),
    ("WORLD_HUMAN_HAMMERING", "Bricole au marteau"),
    ("WORLD_HUMAN_WELDING", "Soude"),
    ("WORLD_HUMAN_GARDENER_PLANT", "Jardine"),
    ("WORLD_HUMAN_JANITOR", "Fait le ménage"),
    ("WORLD_HUMAN_BUM_SLUMPED", "Sans-abri endormi"),
    ("WORLD_HUMAN_CHEERING", "Applaudit"),
    ("WORLD_HUMAN_PARTYING", "Fait la fête"),
    ("PROP_HUMAN_SEAT_BENCH", "Assis sur un banc"),
    ("PROP_HUMAN_BBQ", "Fait un barbecue"),
];

/// Personnages du jeu proposés.
pub const PED_MODELS: [&str; 14] = [
    "a_m_y_business_01",
    "a_f_y_business_01",
    "a_m_y_hipster_01",
    "a_f_y_hipster_01",
    "a_m_m_beach_01",
    "a_f_y_beach_01",
    "a_m_m_tourist_01",
    "a_m_y_skater_01",
    "a_m_m_farmer_01",
    "s_m_m_security_01",
    "s_m_y_construct_01",
    "s_m_m_gardener_01",
    "s_m_y_cop_01",
    "a_m_m_bevhills_01",
];

/// Véhicules du jeu proposés pour les places (vide = au hasard).
pub const CAR_MODELS: [&str; 13] = ["", "asea", "premier", "emperor", "blista", "sultan", "buffalo", "tailgater", "granger", "bison", "rumpo", "felon", "pounder"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LifeTool {
    Car,
    Parking,
    Ped,
    Route,
}

pub struct LifeState {
    pub open: bool,
    pub tool: LifeTool,
    pub car_model: usize,
    /// rangée de parking : en épi (perpendiculaire) ou en créneau (dans l'axe)
    pub parallel: bool,
    pub ped_model: usize,
    pub scenario: usize,
    pub wait: f32,
    pub drag_start: Option<V3>,
    pub route: Vec<V3>,
    pub show_traffic: bool,
}

impl Default for LifeState {
    fn default() -> Self {
        LifeState { open: std::env::var("DW_LIFE").is_ok(), tool: LifeTool::Car, car_model: 0, parallel: false, ped_model: 0, scenario: 1, wait: 6.0, drag_start: None, route: Vec::new(), show_traffic: true }
    }
}

/// Écrit les générateurs de véhicules au format XML de CodeWalker (CMapData.carGenerators, CCarGen).
pub fn cargens_xml(list: &[CarGen]) -> String {
    let mut s = String::from(" <carGenerators>\n");
    for c in list {
        let a = c.heading.to_radians();
        s += &format!(
            "  <Item>\n   <position x=\"{}\" y=\"{}\" z=\"{}\" />\n   <orientX value=\"{}\" />\n   <orientY value=\"{}\" />\n   <perpendicularLength value=\"{}\" />\n   <carModel>{}</carModel>\n   <flags value=\"{}\" />\n   <bodyColorRemap1 value=\"{}\" />\n   <bodyColorRemap2 value=\"{}\" />\n   <bodyColorRemap3 value=\"{}\" />\n   <bodyColorRemap4 value=\"{}\" />\n   <popGroup />\n   <livery value=\"{}\" />\n  </Item>\n",
            c.pos.x,
            c.pos.y,
            c.pos.z,
            a.cos() * c.length,
            a.sin() * c.length,
            c.width,
            c.model,
            c.flags,
            c.colors[0],
            c.colors[1],
            c.colors[2],
            c.colors[3],
            c.livery
        );
    }
    s += " </carGenerators>\n";
    s
}

impl DwApp {
    fn life_models_ok(&self, name: &str) -> bool {
        name.is_empty() || self.game.world.as_ref().map(|w| w.models.contains_key(&crate::gta::crypto::joaat(name))).unwrap_or(true)
    }

    /// Outil « Vie » dans la vue : poser un véhicule, tracer un parking, poser un PNJ, tracer une ronde.
    pub fn life_tool(&mut self, resp: &egui::Response, ptr: Option<Pos2>) {
        use eframe::egui::PointerButton;
        let g = ptr.and_then(|_| self.mouse_ground);
        let (_, right) = self.cam.flat_axes();
        let cam_heading = right.y.atan2(right.x).to_degrees() + 90.0;
        match self.life.tool {
            LifeTool::Car => {
                if resp.clicked_by(PointerButton::Primary) {
                    if let Some(p) = g {
                        self.push_undo();
                        let model = CAR_MODELS[self.life.car_model].to_string();
                        let yi = self.ensure_target_ymap();
                        self.proj.ymaps[yi].car_gens.push(CarGen::new(p + v3(0.0, 0.0, 0.1), cam_heading, &model));
                        self.status = "Véhicule garé posé (capot vers l'avant de la caméra) · glisser avec l'outil Parking pour une rangée.".into();
                    }
                }
            }
            LifeTool::Parking => {
                if resp.drag_started_by(PointerButton::Primary) {
                    self.life.drag_start = g;
                }
                if resp.drag_stopped() {
                    if let (Some(a), Some(b)) = (self.life.drag_start.take(), g) {
                        let d = v3(b.x - a.x, b.y - a.y, 0.0);
                        let len = d.len();
                        if len > 1.0 {
                            self.push_undo();
                            let dir = d.norm();
                            // en épi : une place tous les 2,6 m (largeur CodeWalker), capot perpendiculaire à la rangée ;
                            // en créneau : une place tous les 6 m, dans l'axe
                            let (step, heading) = if self.life.parallel { (6.0, dir.y.atan2(dir.x).to_degrees()) } else { (CARGEN_WIDTH, dir.y.atan2(dir.x).to_degrees() + 90.0) };
                            let n = (len / step).floor() as usize + 1;
                            let model = CAR_MODELS[self.life.car_model].to_string();
                            let yi = self.ensure_target_ymap();
                            for i in 0..n {
                                let p = a + dir * (i as f32 * step);
                                let z = self.proj.terrain.as_ref().and_then(|t| t.height_at(p.x, p.y)).unwrap_or(a.z + (b.z - a.z) * (i as f32 * step / len));
                                self.proj.ymaps[yi].car_gens.push(CarGen::new(v3(p.x, p.y, z + 0.1), heading, &model));
                            }
                            self.status = format!("Parking : {n} place(s).");
                        }
                    }
                }
            }
            LifeTool::Ped => {
                if resp.clicked_by(PointerButton::Primary) {
                    if let Some(p) = g {
                        self.push_undo();
                        let yi = self.ensure_target_ymap();
                        self.proj.ymaps[yi].peds.push(PedSpot { pos: p, heading: cam_heading - 180.0, model: PED_MODELS[self.life.ped_model].into(), scenario: SCENARIOS[self.life.scenario].0.into(), route: Vec::new(), wait: self.life.wait });
                        self.status = format!("PNJ posé : {} (face à la caméra).", SCENARIOS[self.life.scenario].1);
                    }
                }
            }
            LifeTool::Route => {
                if resp.double_clicked_by(PointerButton::Primary) {
                    self.finish_ped_route();
                } else if resp.clicked_by(PointerButton::Primary) {
                    if let Some(p) = g {
                        self.life.route.push(p);
                        self.status = format!("Ronde : {} point(s) · double-clic ou Entrée pour créer le PNJ.", self.life.route.len());
                    }
                }
            }
        }
    }

    /// Crée un PNJ qui fait la ronde cliquée (au moins 2 points).
    pub fn finish_ped_route(&mut self) {
        let pts = std::mem::take(&mut self.life.route);
        if pts.len() < 2 {
            self.status = "Il faut au moins 2 points pour une ronde.".into();
            self.life.route = pts;
            return;
        }
        self.push_undo();
        let d = pts[1] - pts[0];
        let yi = self.ensure_target_ymap();
        self.proj.ymaps[yi].peds.push(PedSpot { pos: pts[0], heading: d.y.atan2(d.x).to_degrees() - 90.0, model: PED_MODELS[self.life.ped_model].into(), scenario: SCENARIOS[self.life.scenario].0.into(), route: pts.clone(), wait: self.life.wait });
        self.status = format!("PNJ avec une ronde de {} points créé.", pts.len());
    }

    /// Véhicules garés (silhouettes), PNJ (pastille + regard) et rondes, nœuds de circulation.
    pub fn draw_life_overlay(&self, painter: &egui::Painter, hover: Option<Pos2>) {
        let proj = |p: V3| self.cam.project(p).map(|q| q.0);
        for y in self.proj.ymaps.iter().filter(|y| y.visible) {
            for c in &y.car_gens {
                if (c.pos - self.cam.eye).len() > 500.0 {
                    continue;
                }
                let a = c.heading.to_radians();
                let (f, r) = (v3(a.cos(), a.sin(), 0.0), v3(a.sin(), -a.cos(), 0.0));
                let (hl, hw) = (2.3, 0.95);
                let z = v3(0.0, 0.0, 0.4);
                let corners = [c.pos + f * hl + r * hw + z, c.pos + f * hl - r * hw + z, c.pos - f * hl - r * hw + z, c.pos - f * hl + r * hw + z];
                let pts: Vec<Pos2> = corners.iter().filter_map(|p| proj(*p)).collect();
                if pts.len() == 4 {
                    painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(2.0_f32, Color32::from_rgb(255, 210, 60))));
                    if let (Some(a1), Some(b1)) = (proj(c.pos + z), proj(c.pos + f * (hl + 0.8) + z)) {
                        painter.arrow(a1, b1 - a1, egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 210, 60)));
                    }
                }
            }
            for p in &y.peds {
                if (p.pos - self.cam.eye).len() > 400.0 {
                    continue;
                }
                let pts: Vec<Pos2> = p.route.iter().filter_map(|q| proj(*q + v3(0.0, 0.0, 0.2))).collect();
                if pts.len() > 1 {
                    let mut l = pts.clone();
                    l.push(pts[0]);
                    painter.add(egui::Shape::line(l, egui::Stroke::new(1.5_f32, Color32::from_rgb(120, 230, 160))));
                }
                let Some(base) = proj(p.pos) else { continue };
                let head = proj(p.pos + v3(0.0, 0.0, 1.8)).unwrap_or(base);
                painter.line_segment([base, head], egui::Stroke::new(3.0_f32, Color32::from_rgb(120, 230, 160)));
                painter.circle_filled(head, 5.0, Color32::from_rgb(120, 230, 160));
                let a = p.heading.to_radians() + std::f32::consts::FRAC_PI_2;
                if let Some(look) = proj(p.pos + v3(a.cos(), a.sin(), 1.6)) {
                    painter.line_segment([head, look], egui::Stroke::new(1.5_f32, Color32::WHITE));
                }
                if hover.map(|h| h.distance(head) < 30.0).unwrap_or(false) {
                    let act = SCENARIOS.iter().find(|s| s.0 == p.scenario).map(|s| s.1).unwrap_or(&p.scenario);
                    painter.text(head + egui::vec2(8.0, -8.0), egui::Align2::LEFT_BOTTOM, format!("{} · {act}", p.model), egui::FontId::proportional(12.0), Color32::WHITE);
                }
            }
        }
        // ronde en cours
        let pts: Vec<Pos2> = self.life.route.iter().filter_map(|q| proj(*q + v3(0.0, 0.0, 0.2))).collect();
        for q in &pts {
            painter.circle_filled(*q, 4.0, Color32::from_rgb(120, 230, 160));
        }
        if pts.len() > 1 {
            painter.add(egui::Shape::line(pts, egui::Stroke::new(2.0_f32, Color32::from_rgb(120, 230, 160))));
        }
        // circulation : réseau de nœuds généré depuis les routes
        if self.life.open && self.life.show_traffic {
            if let Some(t) = self.proj.terrain.as_ref() {
                let net = crate::traffic::build_network(t);
                for (a, b, _, _) in &net.links {
                    if let (Some(pa), Some(pb)) = (proj(net.nodes[*a].pos + v3(0.0, 0.0, 0.3)), proj(net.nodes[*b].pos + v3(0.0, 0.0, 0.3))) {
                        painter.line_segment([pa, pb], egui::Stroke::new(1.5_f32, Color32::from_rgb(80, 170, 255)));
                    }
                }
                for n in &net.nodes {
                    if let Some(q) = proj(n.pos + v3(0.0, 0.0, 0.3)) {
                        painter.circle_filled(q, 2.5, Color32::from_rgb(80, 170, 255));
                    }
                }
            }
        }
    }

    /// Fenêtre « Vie de la carte ».
    pub fn life_window(&mut self, ctx: &egui::Context) {
        if !self.life.open {
            return;
        }
        let mut open = true;
        self.show_panel(ctx, "vie", "Vie de la carte", crate::docking::Dock::Floating, [380.0, 560.0], &mut open, |s, ui| {
            egui::ScrollArea::vertical().id_salt("lifewin").show(ui, |ui| s.life_ui(ui));
        });
        self.life.open = open;
        if !open && self.tool == crate::app::Tool::Life {
            self.tool = crate::app::Tool::Select;
        }
    }

    fn life_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (t, n, tip) in [
                (LifeTool::Car, "Véhicule garé", "Clic : un véhicule garé, capot vers l'avant de la caméra"),
                (LifeTool::Parking, "Parking", "Glisser : une rangée de places (en épi tous les 2,6 m, ou en créneau)"),
                (LifeTool::Ped, "PNJ", "Clic : un PNJ qui fait l'activité choisie, face à la caméra"),
                (LifeTool::Route, "PNJ avec ronde", "Clic : points de la ronde · double-clic ou Entrée : créer le PNJ"),
            ] {
                if ui.selectable_label(self.life.tool == t && self.tool == crate::app::Tool::Life, RichText::new(n).strong()).on_hover_text(tip).clicked() {
                    self.life.tool = t;
                    self.tool = crate::app::Tool::Life;
                }
            }
        });
        ui.separator();
        match self.life.tool {
            LifeTool::Car | LifeTool::Parking => {
                ui.label(RichText::new("Véhicules garés (générateurs de véhicules du jeu)").strong());
                egui::ComboBox::from_label("Modèle").selected_text(if CAR_MODELS[self.life.car_model].is_empty() { "Au hasard (selon le quartier)" } else { CAR_MODELS[self.life.car_model] }).show_ui(ui, |ui| {
                    for (i, m) in CAR_MODELS.iter().enumerate() {
                        if self.life_models_ok(m) {
                            ui.selectable_value(&mut self.life.car_model, i, if m.is_empty() { "Au hasard (selon le quartier)" } else { m });
                        }
                    }
                });
                if self.life.tool == LifeTool::Parking {
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.life.parallel, false, "En épi");
                        ui.radio_value(&mut self.life.parallel, true, "En créneau");
                    });
                }
                ui.label(RichText::new("Le jeu fait apparaître une voiture à chaque place quand on approche (flags 3680, comme un nouveau véhicule dans CodeWalker).").small().weak());
            }
            LifeTool::Ped | LifeTool::Route => {
                ui.label(RichText::new("PNJ").strong());
                egui::ComboBox::from_label("Personnage").selected_text(PED_MODELS[self.life.ped_model]).show_ui(ui, |ui| {
                    for (i, m) in PED_MODELS.iter().enumerate() {
                        if self.life_models_ok(m) {
                            ui.selectable_value(&mut self.life.ped_model, i, *m);
                        }
                    }
                });
                egui::ComboBox::from_label("Activité").selected_text(SCENARIOS[self.life.scenario].1).show_ui(ui, |ui| {
                    for (i, (_, l)) in SCENARIOS.iter().enumerate() {
                        ui.selectable_value(&mut self.life.scenario, i, *l);
                    }
                });
                if self.life.tool == LifeTool::Route {
                    ui.add(egui::Slider::new(&mut self.life.wait, 0.0..=60.0).text("pause à chaque point").suffix(" s"));
                    let n = self.life.route.len();
                    ui.horizontal(|ui| {
                        if ui.add_enabled(n >= 2, egui::Button::new(format!("Créer le PNJ ({n} points)"))).clicked() {
                            self.finish_ped_route();
                        }
                        if ui.add_enabled(n > 0, egui::Button::new("Annuler")).clicked() {
                            self.life.route.clear();
                        }
                    });
                }
                ui.label(RichText::new("En jeu, un petit script de la ressource les fait apparaître quand on approche (150 m) et les retire quand on s'éloigne.").small().weak());
            }
        }
        ui.separator();
        ui.label(RichText::new("Circulation").strong());
        ui.checkbox(&mut self.life.show_traffic, "Montrer le réseau des voitures (points bleus)");
        let (n, l) = self.proj.terrain.as_ref().map(|t| {
            let net = crate::traffic::build_network(t);
            (net.nodes.len(), net.links.len())
        }).unwrap_or((0, 0));
        ui.label(format!("{n} points de route, {l} liens générés depuis tes routes."));
        ui.label(RichText::new("Les voitures du jeu rouleront sur tes routes (une voie par sens pour une rue, deux pour une avenue, trois pour l'autoroute). Ce réseau n'est écrit que dans les carrés de 512 m où le jeu n'a pas de points de route ; là où le jeu en a, les siens sont gardés (les remplacer faisait planter le jeu) : idéal sur une carte custom loin de la ville. Charge le jeu avant d'exporter pour savoir quels carrés sont libres.").small().weak());
        if self.proj.terrain.is_some() {
            ui.add_space(4.0);
            ui.label(RichText::new("Voitures de PNJ qui roulent").strong());
            let models_ok: Vec<&str> = CAR_MODELS.iter().copied().filter(|m| !m.is_empty() && self.life_models_ok(m)).collect();
            let peds_ok: Vec<&str> = PED_MODELS.iter().copied().filter(|m| self.life_models_ok(m)).collect();
            if let Some(before) = self.proj.terrain.as_ref().map(|t| t.npc_traffic.clone()) {
                let mut cfg = before.clone();
                let c = &mut cfg;
                ui.checkbox(&mut c.enabled, "Activer (script de la ressource)");
                ui.add_enabled_ui(c.enabled, |ui| {
                    ui.add(egui::Slider::new(&mut c.count, 1..=40).text("voitures en même temps"));
                    ui.add(egui::Slider::new(&mut c.speed_kmh, 20.0..=130.0).text("vitesse").suffix(" km/h"));
                    ui.add(egui::Slider::new(&mut c.radius, 150.0..=600.0).text("rayon autour du joueur").suffix(" m"));
                    egui::ComboBox::from_label("Conduite").selected_text(crate::traffic::DRIVING_STYLES[(c.style as usize).min(2)].0).show_ui(ui, |ui| {
                        for (i, (n, _)) in crate::traffic::DRIVING_STYLES.iter().enumerate() {
                            ui.selectable_value(&mut c.style, i as u8, *n);
                        }
                    });
                    ui.label(RichText::new("Voitures").small());
                    ui.horizontal_wrapped(|ui| {
                        for m in &models_ok {
                            let mut on = c.models.iter().any(|x| x == m);
                            if ui.checkbox(&mut on, *m).changed() {
                                if on {
                                    c.models.push(m.to_string());
                                } else {
                                    c.models.retain(|x| x != m);
                                }
                            }
                        }
                    });
                    ui.label(RichText::new("Conducteurs").small());
                    ui.horizontal_wrapped(|ui| {
                        for m in &peds_ok {
                            let mut on = c.drivers.iter().any(|x| x == m);
                            if ui.checkbox(&mut on, *m).changed() {
                                if on {
                                    c.drivers.push(m.to_string());
                                } else {
                                    c.drivers.retain(|x| x != m);
                                }
                            }
                        }
                    });
                    if c.models.is_empty() || c.drivers.is_empty() {
                        ui.label(RichText::new("Choisis au moins une voiture et un conducteur.").color(Color32::from_rgb(255, 160, 80)));
                    }
                });
                // modification annulable et projet marqué modifié (un seul pas par glisser de curseur)
                if cfg != before {
                    self.undo_once(ui.ctx());
                    if let Some(t) = self.proj.terrain.as_mut() {
                        t.npc_traffic = cfg;
                    }
                }
            }
            ui.label(RichText::new("Les voitures apparaissent sur tes routes autour du joueur (pas à moins de 60 m), roulent au hasard sur le réseau, et disparaissent quand on s'éloigne. Réglages par route (densité, vitesse, gros véhicules) : créateur de terrain, partie Routes.").small().weak());
        }
        ui.separator();
        let cars: usize = self.proj.ymaps.iter().map(|y| y.car_gens.len()).sum();
        let peds: usize = self.proj.ymaps.iter().map(|y| y.peds.len()).sum();
        ui.label(format!("{cars} véhicule(s) garé(s) · {peds} PNJ"));
        ui.horizontal(|ui| {
            if ui.small_button("Retirer les véhicules").clicked() {
                self.push_undo();
                for y in self.proj.ymaps.iter_mut() {
                    y.car_gens.clear();
                }
            }
            if ui.small_button("Retirer les PNJ").clicked() {
                self.push_undo();
                for y in self.proj.ymaps.iter_mut() {
                    y.peds.clear();
                }
            }
        });
    }

    /// Export : script client des PNJ (dw_peds.lua) à la racine de la ressource ; renvoie vrai s'il y en a.
    pub fn export_peds(&self, root: &std::path::Path) -> Result<bool, String> {
        let peds: Vec<&PedSpot> = self.proj.ymaps.iter().filter(|y| !y.sample).flat_map(|y| y.peds.iter()).collect();
        if peds.is_empty() {
            return Ok(false);
        }
        let mut data = String::new();
        for p in peds {
            let route: Vec<String> = p.route.iter().map(|q| format!("{{{:.2}, {:.2}, {:.2}}}", q.x, q.y, q.z)).collect();
            data += &format!(
                "    {{ model = \"{}\", x = {:.2}, y = {:.2}, z = {:.2}, h = {:.1}, scenario = \"{}\", wait = {}, route = {{ {} }} }},\n",
                p.model,
                p.pos.x,
                p.pos.y,
                p.pos.z,
                p.heading.rem_euclid(360.0),
                p.scenario,
                (p.wait * 1000.0) as i32,
                route.join(", ")
            );
        }
        let lua = format!(
            r#"-- DragoWalker : PNJ posés sur la carte (immobiles avec une activité, ou qui font une ronde)
local PEDS = {{
{data}}}
local NEAR, FAR = 150.0, 200.0
local spawned = {{}}

local function load(model)
    local h = GetHashKey(model)
    RequestModel(h)
    local t = GetGameTimer()
    while not HasModelLoaded(h) and GetGameTimer() - t < 5000 do Wait(10) end
    return h
end

local function activity(ped, p)
    if p.scenario ~= "" then TaskStartScenarioInPlace(ped, p.scenario, 0, true) end
end

local function spawn(i, p)
    local h = load(p.model)
    local ped = CreatePed(4, h, p.x, p.y, p.z, p.h, false, true)
    SetEntityAsMissionEntity(ped, true, true)
    SetBlockingOfNonTemporaryEvents(ped, true)
    SetModelAsNoLongerNeeded(h)
    spawned[i] = ped
    if #p.route < 2 then
        activity(ped, p)
        return
    end
    -- ronde : chaque point à pied, pause avec l'activité, puis le suivant, en boucle
    CreateThread(function()
        local k = 1
        while spawned[i] == ped and DoesEntityExist(ped) do
            local q = p.route[k]
            TaskGoStraightToCoord(ped, q[1], q[2], q[3], 1.0, 30000, 0.0, 0.5)
            local t = GetGameTimer()
            while DoesEntityExist(ped) and #(GetEntityCoords(ped) - vector3(q[1], q[2], q[3])) > 1.5 and GetGameTimer() - t < 30000 do Wait(250) end
            if p.wait > 0 then
                activity(ped, p)
                Wait(p.wait)
                ClearPedTasks(ped)
            end
            k = k % #p.route + 1
        end
    end)
end

CreateThread(function()
    while true do
        local me = GetEntityCoords(PlayerPedId())
        for i, p in ipairs(PEDS) do
            local d = #(me - vector3(p.x, p.y, p.z))
            if not spawned[i] and d < NEAR then
                spawn(i, p)
            elseif spawned[i] and d > FAR then
                DeleteEntity(spawned[i])
                spawned[i] = nil
            end
        end
        Wait(1000)
    end
end)
"#
        );
        std::fs::write(root.join("dw_peds.lua"), lua).map_err(|e| e.to_string())?;
        Ok(true)
    }
}
