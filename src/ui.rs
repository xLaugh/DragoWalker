//! Panneaux : menus, barre d'outils, projet, boîte à outils (sélection, props, intérieur, IA, options).

use crate::ai::Action;
use crate::app::{save_api_key, DwApp, Tab, Tool};
use crate::catalog;
use crate::math::{v3, V3};
use crate::mech;
use crate::model::{self, ItemRef, Project, Ymap, Ytyp};
use crate::sample;
use eframe::egui::{self, Color32, Key, RichText, Sense};

impl DwApp {
    /// Barre de menus. Raccourcis repris de CodeWalker : WorldForm_KeyDown (0x06000332) — Ctrl+N nouveau,
    /// Ctrl+O ouvrir, Ctrl+S enregistrer (Ctrl+Maj+S : tout), Ctrl+Z annuler, Ctrl+Y rétablir, Ctrl+C copier,
    /// Ctrl+V coller, Ctrl+U panneau d'outils, Suppr supprimer, Échap ; menus Édition du ProjectForm
    /// (InitializeComponent 0x06000e92) : Ctrl+Z, Ctrl+Y, Ctrl+X, Ctrl+C, Ctrl+V, Maj+Suppr.
    pub fn menu_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let item = |ui: &mut egui::Ui, enabled: bool, text: &str, keys: &str| ui.add_enabled(enabled, egui::Button::new(text).shortcut_text(keys)).clicked();
        egui::menu::bar(ui, |ui| {
            ui.menu_button(crate::i18n::t("Fichier"), |ui| {
                if item(ui, true, crate::i18n::t("Nouveau projet"), crate::i18n::t("Ctrl+N")) {
                    self.new_project();
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Ouvrir un projet…"), crate::i18n::t("Ctrl+O")) {
                    self.open_project_dialog();
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Enregistrer le projet"), crate::i18n::t("Ctrl+S")) {
                    self.save_project(false);
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Enregistrer sous…"), crate::i18n::t("Ctrl+Maj+S")) {
                    self.save_project(true);
                    ui.close_menu();
                }
                ui.separator();
                if item(ui, true, crate::i18n::t("Importer ymap / ytyp (XML CodeWalker)…"), crate::i18n::t("Ctrl+I")) {
                    self.import_dialog();
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Importer une ressource MLO (dossier FiveM)…"), "") {
                    self.import_mlo_dialog();
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Exporter en XML…"), crate::i18n::t("Ctrl+E")) {
                    self.export_xml(false);
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Exporter une ressource FiveM…"), crate::i18n::t("Ctrl+Maj+E")) {
                    self.export_xml(true);
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Contrôle avant l'export…"), "") {
                    self.check_open = true;
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Analyser un plantage FiveM…"), "") {
                    self.crash.open = true;
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Tester en jeu (serveur local)"), crate::i18n::t("F12")) {
                    self.server.open = true;
                    self.server_test();
                    ui.close_menu();
                }
                ui.separator();
                if item(ui, true, crate::i18n::t("Quitter"), crate::i18n::t("Alt+F4")) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(crate::i18n::t("Édition"), |ui| {
                let has_sel = !self.selected.is_empty();
                if item(ui, !self.undo.is_empty(), crate::i18n::t("Annuler"), crate::i18n::t("Ctrl+Z")) {
                    self.do_undo();
                    ui.close_menu();
                }
                if item(ui, !self.redo.is_empty(), crate::i18n::t("Rétablir"), crate::i18n::t("Ctrl+Y")) {
                    self.do_redo();
                    ui.close_menu();
                }
                ui.separator();
                if item(ui, has_sel, crate::i18n::t("Couper"), crate::i18n::t("Ctrl+X")) {
                    self.copy_selected();
                    self.delete_selected();
                    ui.close_menu();
                }
                if item(ui, has_sel, crate::i18n::t("Copier"), crate::i18n::t("Ctrl+C")) {
                    self.copy_selected();
                    ui.close_menu();
                }
                if item(ui, !self.clipboard.is_empty(), crate::i18n::t("Coller"), crate::i18n::t("Ctrl+V")) {
                    self.paste();
                    ui.close_menu();
                }
                if item(ui, has_sel, crate::i18n::t("Dupliquer la sélection"), crate::i18n::t("Ctrl+D")) {
                    let off = self.copy_offset();
                    self.duplicate_selected(off);
                    ui.close_menu();
                }
                if item(ui, has_sel || self.isolate_sel.is_some(), if self.isolate_sel.is_some() { crate::i18n::t("Revenir à tout voir") } else { crate::i18n::t("Isoler la sélection") }, crate::i18n::t("Ctrl+H")) {
                    self.toggle_isolate();
                    ui.close_menu();
                }
                if item(ui, has_sel, crate::i18n::t("Poser au sol"), crate::i18n::t("Maj / Fin")) {
                    let n = self.snap_selected_to_ground(true);
                    self.status = format!("{n} objet(s) posé(s) au sol.");
                    ui.close_menu();
                }
                if item(ui, has_sel, crate::i18n::t("Supprimer la sélection"), crate::i18n::t("Suppr")) {
                    self.delete_selected();
                    ui.close_menu();
                }
                ui.separator();
                if item(ui, true, crate::i18n::t("Sélection intelligente"), crate::i18n::t("J")) {
                    self.smart_select();
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Tout désélectionner"), crate::i18n::t("Échap")) {
                    self.set_selection(vec![]);
                    ui.close_menu();
                }
            });
            ui.menu_button(crate::i18n::t("Outils"), |ui| {
                for (t, n, k) in [(Tool::Select, crate::i18n::t("Sélection"), "1"), (Tool::Place, crate::i18n::t("Poser"), "2"), (Tool::Erase, crate::i18n::t("Gomme"), "3"), (Tool::Rect, crate::i18n::t("Rectangle"), "4"), (Tool::Terrain, crate::i18n::t("Terrain"), "5")] {
                    if ui.add(egui::Button::new(n).selected(self.tool == t).shortcut_text(k)).clicked() {
                        self.tool = t;
                        if t == Tool::Terrain {
                            self.tab = Tab::Terrain;
                        }
                        ui.close_menu();
                    }
                }
                ui.separator();
                for (m, n, k) in [(crate::gizmo::GizmoMode::Move, crate::i18n::t("Poignées : déplacer"), crate::i18n::t("G")), (crate::gizmo::GizmoMode::Rotate, crate::i18n::t("Poignées : tourner"), crate::i18n::t("R / E")), (crate::gizmo::GizmoMode::Scale, crate::i18n::t("Poignées : agrandir"), crate::i18n::t("T"))] {
                    if ui.add(egui::Button::new(n).selected(self.gizmo_mode == m).shortcut_text(k)).clicked() {
                        self.gizmo_mode = m;
                        self.tool = Tool::Select;
                        ui.close_menu();
                    }
                }
                ui.separator();
                if item(ui, true, crate::i18n::t("Bibliothèque de props"), crate::i18n::t("B")) {
                    self.library.open = !self.library.open;
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Bibliothèque des intérieurs"), crate::i18n::t("I")) {
                    self.intlib.open = !self.intlib.open;
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Bibliothèque des bâtiments"), crate::i18n::t("N")) {
                    self.buildings.open = !self.buildings.open;
                    ui.close_menu();
                }
                if item(ui, true, crate::i18n::t("Couper les bâtiments (vue en coupe)"), crate::i18n::t("C")) {
                    self.game.cut_on = !self.game.cut_on;
                    ui.close_menu();
                }
                ui.label(RichText::new(crate::i18n::t("H : cacher l'objet du jeu sous la souris")).small().weak());
            });
            ui.menu_button(crate::i18n::t("Terrain"), |ui| {
                use crate::terrain::Brush;
                for (s, n) in [(0u8, crate::i18n::t("Créer / modèles de départ"))] {
                    if item(ui, true, n, "") {
                        self.terrain_section(s);
                        self.tool = Tool::Terrain;
                        self.tab = Tab::Terrain;
                        ui.close_menu();
                    }
                }
                ui.separator();
                for (b, n, k) in [
                    (Brush::Raise, crate::i18n::t("Monter"), crate::i18n::t("F2")),
                    (Brush::Lower, crate::i18n::t("Creuser"), crate::i18n::t("F3")),
                    (Brush::Flatten, crate::i18n::t("Pelleteuse"), crate::i18n::t("F4")),
                    (Brush::Smooth, crate::i18n::t("Lisser"), crate::i18n::t("F5")),
                    (Brush::Noise, crate::i18n::t("Relief naturel"), crate::i18n::t("F6")),
                    (Brush::Paint, crate::i18n::t("Peindre"), crate::i18n::t("F7")),
                    (Brush::Road, crate::i18n::t("Route"), crate::i18n::t("F8")),
                    (Brush::Vegetation, crate::i18n::t("Végétation"), crate::i18n::t("F9")),
                    (Brush::Water, crate::i18n::t("Eau"), crate::i18n::t("F10")),
                    (Brush::Particles, crate::i18n::t("Effets (particules)"), crate::i18n::t("F11")),
                ] {
                    let on = self.tool == Tool::Terrain && self.terrain.brush == b;
                    if ui.add_enabled(self.proj.terrain.is_some(), egui::Button::new(n).selected(on).shortcut_text(k)).clicked() {
                        self.set_terrain_brush(b);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if item(ui, self.terrain.road_pts.len() >= 2, crate::i18n::t("Construire la route en cours"), crate::i18n::t("Entrée")) {
                    self.finish_road();
                    ui.close_menu();
                }
                if item(ui, !self.terrain.road_pts.is_empty(), crate::i18n::t("Enlever le dernier point"), crate::i18n::t("Retour arrière")) {
                    self.terrain.road_pts.pop();
                    ui.close_menu();
                }
                ui.label(RichText::new(crate::i18n::t("Rayon du pinceau : + / -  ·  Végétation : Ctrl + pinceau pour retirer")).small().weak());
            });
            ui.menu_button(crate::i18n::t("Affichage"), |ui| {
                if ui.add(egui::Button::new(crate::i18n::t("Vue 3D")).selected(!self.cam.map_view).shortcut_text(crate::i18n::t("M"))).clicked() {
                    self.cam.map_view = false;
                }
                if ui.add(egui::Button::new(crate::i18n::t("Vue carte (de dessus)")).selected(self.cam.map_view).shortcut_text(crate::i18n::t("M"))).clicked() {
                    self.cam.map_view = true;
                }
                if item(ui, true, if self.panels_hidden { crate::i18n::t("Afficher les panneaux") } else { crate::i18n::t("Cacher les panneaux") }, crate::i18n::t("Ctrl+U")) {
                    self.panels_hidden = !self.panels_hidden;
                    ui.close_menu();
                }
                ui.checkbox(&mut self.project_open, crate::i18n::t("Panneau Projet"));
                ui.checkbox(&mut self.toolbox_open, crate::i18n::t("Boîte à outils"));
                ui.label(RichText::new(crate::i18n::t("Pour déplacer un panneau : attrape son titre et lâche-le contre un bord (rangé) ou ailleurs (fenêtre flottante)")).small().weak());
                ui.separator();
                ui.checkbox(&mut self.opts.show_grid, crate::i18n::t("Grille"));
                ui.checkbox(&mut self.opts.show_rooms, crate::i18n::t("Pièces et portails des intérieurs"));
                ui.checkbox(&mut self.opts.show_sample, crate::i18n::t("Carte d'exemple"));
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Langue / Language");
                    let en = crate::i18n::is_english();
                    for (is_en, label) in [(false, "Français"), (true, "English")] {
                        if ui.selectable_label(en == is_en, label).clicked() && en != is_en {
                            crate::i18n::set_english(is_en);
                            crate::app::save_setting("lang", serde_json::json!(if is_en { "en" } else { "fr" }));
                        }
                    }
                });
                ui.checkbox(&mut self.opts.show_perf, crate::i18n::t("Compteur de performance")).on_hover_text(crate::i18n::t("Images par seconde, objets et triangles dessinés, mémoire graphique, historique d'annulation"));
            });
            ui.menu_button(crate::i18n::t("Aide"), |ui| {
                ui.label(crate::i18n::t("Clic droit + glisser : tourner la vue (vue carte : déplacer)"));
                ui.label(crate::i18n::t("Clic molette + glisser : déplacer la vue"));
                ui.label(crate::i18n::t("ZQSD / WASD : avancer · Maj : ×5 · Ctrl : ×0.2"));
                ui.label(crate::i18n::t("Page haut / bas : monter / descendre"));
                ui.label(crate::i18n::t("1 Sélection · 2 Poser · 3 Gomme · 4 Rectangle · 5 Terrain · M vue carte"));
                ui.label(crate::i18n::t("G / R / T : poignées · Flèches : bouger la sélection · [ ] : tourner de 15°"));
                ui.label(crate::i18n::t("Ctrl+C / Ctrl+V / Ctrl+X : copier, coller (devant la caméra, comme CodeWalker), couper"));
                ui.label(crate::i18n::t("Suppr : supprimer · Ctrl+D : dupliquer · Ctrl+Z / Ctrl+Y · Ctrl+U : panneaux"));
                ui.label(crate::i18n::t("Terrain : F2–F11 pinceaux · + / - rayon · Entrée : construire la route"));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(crate::i18n::t("DragoWalker")).strong().color(Color32::from_rgb(255, 150, 50)));
                ui.separator();
                if ui.selectable_label(self.ai_open, RichText::new(crate::i18n::t("Assistant IA")).strong()).on_hover_text(crate::i18n::t("Fenêtre de l'assistant : décris ce que tu veux construire")).clicked() {
                    self.ai_open = !self.ai_open;
                }
                if ui.selectable_label(self.server.open, RichText::new(crate::i18n::t("Tester en jeu")).strong()).on_hover_text(crate::i18n::t("Serveur FiveM local géré par DragoWalker : tester la map et les MLO en jeu, recharger sans se déconnecter")).clicked() {
                    self.server.open = !self.server.open;
                }
                if ui.selectable_label(self.life.open, RichText::new(crate::i18n::t("Vie (PNJ, véhicules)")).strong()).on_hover_text(crate::i18n::t("Véhicules garés, parkings, PNJ avec activité ou ronde, circulation sur tes routes")).clicked() {
                    self.life.open = !self.life.open;
                    if self.life.open {
                        self.tool = Tool::Life;
                    }
                }
                if ui.selectable_label(self.toolbox_open, RichText::new(crate::i18n::t("Boîte à outils")).strong()).on_hover_text(crate::i18n::t("Ouvrir / fermer la boîte à outils")).clicked() {
                    self.toolbox_open = !self.toolbox_open;
                    self.panels_hidden = false;
                }
                if ui.selectable_label(self.project_open, RichText::new(crate::i18n::t("Projet")).strong()).on_hover_text(crate::i18n::t("Ouvrir / fermer le panneau Projet")).clicked() {
                    self.project_open = !self.project_open;
                    self.panels_hidden = false;
                }
                if ui.selectable_label(self.terrain_open, RichText::new(crate::i18n::t("Créateur de terrain")).strong()).on_hover_text(crate::i18n::t("Fenêtre du créateur de terrain : relief, peinture, routes, végétation, eau, effets (5)")).clicked() {
                    self.terrain_open = !self.terrain_open;
                    if self.terrain_open {
                        self.tool = Tool::Terrain;
                    }
                }
            });
        });
    }

    /// Fenêtre de l'assistant IA, en haut à droite de l'écran.
    pub fn ai_window(&mut self, ctx: &egui::Context) {
        if self.tab == Tab::Ai {
            self.ai_open = true;
            self.tab = Tab::Selection;
        }
        if !self.ai_open {
            return;
        }
        let mut open = true;
        self.show_panel(ctx, "ia", crate::i18n::t("Assistant IA"), crate::docking::Dock::Floating, [400.0, 560.0], &mut open, |s, ui| s.ai_tab(ui));
        self.ai_open = open;
    }

    /// Fenêtre du créateur de terrain.
    pub fn terrain_window(&mut self, ctx: &egui::Context) {
        if self.tab == Tab::Terrain {
            self.terrain_open = true;
            self.tab = Tab::Selection;
        }
        if !self.terrain_open {
            return;
        }
        let mut open = true;
        self.show_panel(ctx, "terrain", crate::i18n::t("Créateur de terrain"), crate::docking::Dock::Floating, [380.0, 600.0], &mut open, |s, ui| {
            egui::ScrollArea::vertical().id_salt("terrwin").show(ui, |ui| s.terrain_tab(ui));
        });
        self.terrain_open = open;
        if !open && self.tool == Tool::Terrain {
            self.tool = Tool::Select;
        }
    }

    /// Isoler : seuls les objets choisis restent affichés (ni carte du jeu, ni terrain, ni eau) ; Échap revient.
    pub fn toggle_isolate(&mut self) {
        if self.isolate_sel.take().is_some() {
            self.status = crate::i18n::t("Tout est de nouveau affiché.").into();
            return;
        }
        if self.selected.is_empty() {
            self.status = crate::i18n::t("Choisis d'abord un ou plusieurs objets à isoler.").into();
            return;
        }
        self.isolate_sel = Some(self.sel_set.clone());
        self.status = format!("{} objet(s) vus seuls · Échap ou Ctrl+H pour revenir.", self.selected.len());
    }

    /// Bandeau quand une sélection est vue seule.
    pub fn isolate_banner(&mut self, ctx: &egui::Context) {
        let Some(n) = self.isolate_sel.as_ref().map(|s| s.len()) else { return };
        egui::Area::new(egui::Id::new("iso_sel_banner")).fixed_pos(self.cam.rect.center_top() + egui::vec2(-150.0, 8.0)).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Isolé : {n} objet(s)")).strong().color(Color32::from_rgb(255, 200, 90)));
                    if ui.button(crate::i18n::t("Tout revoir (Échap)")).clicked() {
                        self.isolate_sel = None;
                    }
                });
            });
        });
    }

    /// « Visible jusqu'à » : distance d'affichage (lodDist) de toute la sélection, et « Toujours chargé »
    /// (priorityLevel PRI_REQUIRED ; sinon PRI_OPTIONAL_HIGH, que le jeu peut ne pas charger selon ses réglages).
    pub fn visible_until_ui(&mut self, ui: &mut egui::Ui) {
        let refs: Vec<ItemRef> = self.selected.iter().filter_map(|id| self.proj.find_id(*id)).collect();
        if refs.is_empty() {
            return;
        }
        let dists: Vec<f32> = refs.iter().filter_map(|r| self.proj.ent(*r).map(|e| e.lod_dist)).collect();
        let (lo, hi) = dists.iter().fold((f32::MAX, 0.0f32), |(a, b), d| (a.min(*d), b.max(*d)));
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::i18n::t("Visible jusqu'à"));
            ui.label(RichText::new(if (hi - lo).abs() < 0.5 { format!("{lo:.0} m") } else { format!("{lo:.0}–{hi:.0} m") }).strong());
        });
        let mut set: Option<Option<f32>> = None;
        ui.horizontal_wrapped(|ui| {
            for (n, d, tip) in [
                (crate::i18n::t("Proche"), 100.0, crate::i18n::t("100 m : petits objets, détails")),
                (crate::i18n::t("Normal"), 300.0, crate::i18n::t("300 m")),
                (crate::i18n::t("Loin"), 1000.0, crate::i18n::t("1 000 m : grands bâtiments")),
                (crate::i18n::t("Toujours"), 3000.0, crate::i18n::t("3 000 m : repères visibles de partout")),
            ] {
                if ui.small_button(n).on_hover_text(tip).clicked() {
                    set = Some(Some(d));
                }
            }
            if ui.small_button(crate::i18n::t("Comme le jeu")).on_hover_text(crate::i18n::t("La distance de ce type d'objet dans GTA V ; les bâtiments du jeu copiés emportent en plus leur version lointaine à l'export")).clicked() {
                set = Some(None);
            }
        });
        if let Some(choice) = set {
            self.push_undo();
            let w = self.game.world.as_ref();
            let mut n = 0;
            for r in &refs {
                let game = w.and_then(|w| self.proj.ent(*r).and_then(|e| w.archetypes.get(&crate::gta::crypto::joaat(&e.archetype.to_lowercase()))).map(|a| a.lod_dist)).filter(|d| *d > 0.0);
                if let Some(e) = self.proj.ent_mut(*r) {
                    e.lod_dist = choice.or(game).unwrap_or(crate::mech::ENTITY_LOD_DIST);
                    n += 1;
                }
            }
            self.status = format!("{n} objet(s) : distance d'affichage changée.");
        }
        let all_req = refs.iter().all(|r| self.proj.ent(*r).map(|e| e.priority == crate::i18n::t("PRI_REQUIRED")).unwrap_or(true));
        let mut req = all_req;
        if ui.checkbox(&mut req, crate::i18n::t("Toujours chargé")).on_hover_text(crate::i18n::t("Le jeu charge toujours ces objets (PRI_REQUIRED) ; sinon il peut les sauter selon ses réglages graphiques (PRI_OPTIONAL_HIGH)")).changed() {
            self.push_undo();
            for r in &refs {
                if let Some(e) = self.proj.ent_mut(*r) {
                    e.priority = if req { crate::i18n::t("PRI_REQUIRED").into() } else { crate::i18n::t("PRI_OPTIONAL_HIGH").into() };
                }
            }
        }
    }

    pub fn new_project(&mut self) {
        self.push_undo();
        self.proj = Project { name: crate::i18n::t("Nouveau projet").into(), ymaps: vec![sample::generate(), Ymap::new("map1")], ytyps: vec![], terrain: None, mlo_res: vec![], mlo_rot_fixed: true, building_cuts: vec![], mlo_links: vec![], mlo_link_yaw: vec![], groups: vec![], houses: vec![] };
        self.target_ymap = 1;
        self.cur_mlo = None;
        self.set_selection(vec![]);
        self.project_path = None;
        self.terrain.dirty = true;
        self.status = crate::i18n::t("Nouveau projet.").into();
    }

    pub fn open_project_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter(crate::i18n::t("Projet DragoWalker"), &["dwproj"]).pick_file() {
            self.import_file(&p);
        }
    }

    pub fn import_dialog(&mut self) {
        if let Some(files) = rfd::FileDialog::new().add_filter(crate::i18n::t("XML"), &["xml"]).pick_files() {
            for f in files {
                self.import_file(&f);
            }
        }
    }

    /// Copie la sélection (positions dans le monde).
    pub fn copy_selected(&mut self) {
        self.clipboard = self.selected.iter().filter_map(|id| self.proj.find_id(*id).and_then(|r| self.proj.world_entity(r))).collect();
        self.status = format!("{} objet(s) copié(s) · Ctrl+V pour coller.", self.clipboard.len());
    }

    /// Colle comme CodeWalker : WorldForm::PasteItem (0x06000303) appelle ProjectForm::NewObject(copie,
    /// copyPosition = sélection multiple, selectNew = vrai) ; un seul objet passe par NewEntity (0x06000d3e) et
    /// apparaît devant la caméra (GetSpawnPos 0x06000dfb, à 2,5 × son rayon, 5 m au moins) ; plusieurs objets
    /// gardent leur position d'origine. Les copies sont sélectionnées. Dans la carte de destination.
    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let off = if self.clipboard.len() == 1 {
            let e = &self.clipboard[0];
            self.spawn_pos(&e.archetype) - e.pos
        } else {
            V3::ZERO
        };
        self.push_undo();
        let yi = self.ensure_target_ymap();
        let mut ids = Vec::new();
        // les copies d'un groupe posé forment leur propre bloc
        let mut inst_map: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
        for e in self.clipboard.clone() {
            let mut e = e;
            e.id = model::new_id();
            e.game_index = -1;
            e.pos = e.pos + off;
            e.mlo = None;
            if e.group_inst != 0 {
                e.group_inst = *inst_map.entry(e.group_inst).or_insert_with(model::new_id);
            }
            ids.push(e.id);
            self.proj.ymaps[yi].entities.push(e);
        }
        let k = ids.len();
        self.snap_if_shift(&ids);
        self.set_selection(ids);
        self.sync_game_project();
        self.status = format!("{k} objet(s) collé(s).");
    }

    pub fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let tools = [
                (Tool::Select, crate::i18n::t("🖱 Sélection"), crate::i18n::t("Clic : choisir · glisser dans le vide : rectangle · glisser un objet : le déplacer (1)")),
                (Tool::Place, crate::i18n::t("➕ Poser"), crate::i18n::t("Clic : poser le prop choisi · glisser : en poser le long du trait (2)")),
                (Tool::Erase, crate::i18n::t("⭕ Gomme"), crate::i18n::t("Glisser : supprimer tout ce qui passe sous le cercle (3)")),
                (Tool::Terrain, crate::i18n::t("Terrain"), crate::i18n::t("Pinceaux de terrain : monter, creuser, pelleteuse, lisser, peindre, routes (5)")),
                (Tool::Rect, crate::i18n::t("Rectangle"), crate::i18n::t("Glisser n'importe où : sélectionner tout ce qui est dans le rectangle, objets du jeu compris · Ctrl ou Maj : ajouter à la sélection (4) · avec l'outil Sélection : Alt + glisser")),
            ];
            for (t, label, tip) in tools {
                if ui.selectable_label(self.tool == t, RichText::new(label).size(15.0)).on_hover_text(tip).clicked() {
                    self.tool = t;
                    if t == Tool::Terrain {
                        self.terrain_open = true;
                    }
                }
            }
            // les bibliothèques ne sont pas des outils : séparées
            ui.separator();
            if ui.selectable_label(self.intlib.open, RichText::new(crate::i18n::t("Intérieurs")).size(15.0)).on_hover_text(crate::i18n::t("Bibliothèque des intérieurs du jeu : plan, « Aller voir », « Voir isolé »")).clicked() {
                self.intlib.open = !self.intlib.open;
            }
            if ui.selectable_label(self.buildings.open, RichText::new(crate::i18n::t("Bâtiments")).size(15.0)).on_hover_text(crate::i18n::t("Bâtiments du jeu : les copier pour les poser ailleurs, sur une carte custom (N)")).clicked() {
                self.buildings.open = !self.buildings.open;
            }
            if ui.selectable_label(self.atelier.open, RichText::new(crate::i18n::t("Atelier")).size(15.0)).on_hover_text(crate::i18n::t("Atelier de bâtiment : tracer l'emprise au sol, choisir les étages, le toit et la façade ; modèle, collision, version lointaine et fenêtres éclairées la nuit faits par DragoWalker")).clicked() {
                self.atelier.open = !self.atelier.open;
            }
            if ui.selectable_label(self.groups_open, RichText::new(crate::i18n::t("Groupes")).size(15.0)).on_hover_text(crate::i18n::t("Tes groupes d'objets : les poser à la souris (molette = tourner, Maj + molette = quart de tour)")).clicked() {
                self.groups_open = !self.groups_open;
            }
            if ui.selectable_label(self.library.open, RichText::new(crate::i18n::t("Bibliothèque")).size(15.0)).on_hover_text(crate::i18n::t("Tous les props du jeu en images, avec aperçu 3D ; clic pour choisir, double-clic pour poser, glisser sur la carte pour poser à cet endroit (B)")).clicked() {
                self.library.open = !self.library.open;
            }
            ui.separator();
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("⟲")).on_hover_text(crate::i18n::t("Annuler (Ctrl+Z)")).clicked() {
                self.do_undo();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("⟳")).on_hover_text(crate::i18n::t("Rétablir (Ctrl+Y)")).clicked() {
                self.do_redo();
            }
            if ui.add_enabled(!self.selected.is_empty(), egui::Button::new(crate::i18n::t("🗑 Supprimer"))).on_hover_text(crate::i18n::t("Supprimer toute la sélection d'un coup")).clicked() {
                self.delete_selected();
            }
            if ui.add_enabled(!self.selected.is_empty(), egui::Button::new(crate::i18n::t("📋 Dupliquer"))).clicked() {
                { let off = self.copy_offset(); self.duplicate_selected(off); }
            }
            ui.separator();
            // poignées dans la vue, comme le widget de CodeWalker
            for (mode, label, tip) in [
                (crate::gizmo::GizmoMode::Move, crate::i18n::t("Déplacer"), crate::i18n::t("Flèches pour déplacer la sélection le long d'un axe (G)")),
                (crate::gizmo::GizmoMode::Rotate, crate::i18n::t("Tourner"), crate::i18n::t("Anneaux pour tourner la sélection, par pas de 5° avec « Aimanter » (R ou E)")),
                (crate::gizmo::GizmoMode::Scale, crate::i18n::t("Agrandir"), crate::i18n::t("Carrés pour changer la taille ; carré du centre = partout (T)")),
            ] {
                if ui.selectable_label(self.gizmo_mode == mode, label).on_hover_text(tip).clicked() {
                    self.gizmo_mode = mode;
                    self.tool = Tool::Select;
                }
            }
            ui.checkbox(&mut self.gizmo_object_space, crate::i18n::t("Axes de l'objet")).on_hover_text(crate::i18n::t("Poignées alignées sur l'objet choisi au lieu du nord / de l'est (comme « Object space » dans CodeWalker)"));
            ui.separator();
            let vlabel = if self.cam.map_view { crate::i18n::t("🗺 Vue carte") } else { crate::i18n::t("🎥 Vue 3D") };
            if ui.button(vlabel).on_hover_text(crate::i18n::t("Changer de vue (M)")).clicked() {
                self.cam.map_view = !self.cam.map_view;
            }
            ui.checkbox(&mut self.snap, crate::i18n::t("Aimanter")).on_hover_text(crate::i18n::t("Aligner les positions sur un pas fixe (sans rapport avec Affichage → Grille, qui ne fait que dessiner)"));
            if self.snap {
                ui.add(egui::DragValue::new(&mut self.snap_size).range(0.1..=50.0).speed(0.1).suffix(crate::i18n::t(" m")));
            }
            ui.separator();
            let names: Vec<(usize, String)> = self.proj.ymaps.iter().enumerate().filter(|(_, y)| !y.sample).map(|(i, y)| (i, y.name.clone())).collect();
            let cur = self.proj.ymaps.get(self.target_ymap).map(|y| y.name.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt("target_ymap").selected_text(format!("» {cur}.ymap")).show_ui(ui, |ui| {
                for (i, n) in names {
                    ui.selectable_value(&mut self.target_ymap, i, format!("{n}.ymap"));
                }
            })
            .response
            .on_hover_text(crate::i18n::t("Carte de destination des nouveaux objets"));
        });
    }

    pub fn project_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Projet");
        ui.add(egui::TextEdit::singleline(&mut self.proj.name).hint_text("Nom du projet"));
        ui.horizontal(|ui| {
            if ui.button("+ ymap").on_hover_text("Nouveau fichier de placements").clicked() {
                self.push_undo();
                let n = self.proj.next_ymap_name();
                self.proj.ymaps.push(Ymap::new(&n));
                self.target_ymap = self.proj.ymaps.len() - 1;
            }
            if ui.button("+ ytyp").on_hover_text("Nouveau fichier de types (intérieurs)").clicked() {
                self.push_undo();
                let n = self.proj.next_ytyp_name();
                self.proj.ytyps.push(Ytyp { name: n, mlos: vec![], visible: true, raw_archetypes: vec![] });
            }
            if ui.button("Importer…").clicked() {
                if let Some(files) = rfd::FileDialog::new().add_filter("XML", &["xml"]).pick_files() {
                    for f in files {
                        self.import_file(&f);
                    }
                }
            }
            if ui.button("Importer un MLO…").on_hover_text("Dossier d'une ressource MLO FiveM (fxmanifest.lua, stream/ avec .ydr, .ytd, .ytyp, .ymap, .ybn) : le bâtiment et son intérieur apparaissent, se déplacent et se tournent ; les collisions suivent à l'export ; l'auteur et la licence sont gardés").clicked() {
                self.import_mlo_dialog();
            }
        });
        self.mlo_list_ui(ui);
        ui.separator();
        let mut remove_ymap = None;
        let mut remove_ytyp = None;
        let mut select_ids: Option<Vec<u64>> = None;
        let mut goto: Option<V3> = None;
        let mut new_target = None;
        let mut new_mlo = None;
        let target = self.target_ymap;
        let sel_set = &self.sel_set;
        let cur_mlo = self.cur_mlo;
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            for (yi, y) in self.proj.ymaps.iter_mut().enumerate() {
                let title = format!("{} {}.ymap  ({})", if y.sample { "🗺" } else { "📄" }, y.name, y.entities.len());
                let col = if yi == target { Color32::from_rgb(255, 180, 90) } else { Color32::LIGHT_GRAY };
                egui::CollapsingHeader::new(RichText::new(title).color(col)).id_salt(("ymap", yi)).default_open(!y.sample).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut y.visible, "Visible");
                        if !y.sample {
                            if ui.small_button("Destination").clicked() {
                                new_target = Some(yi);
                            }
                            if ui.small_button("🗑").on_hover_text("Retirer ce ymap du projet").clicked() {
                                remove_ymap = Some(yi);
                            }
                        }
                    });
                    if y.sample {
                        ui.label(RichText::new("Carte d'exemple générée : la vraie carte du jeu n'est pas encore lue. Tout y est modifiable.").small().weak());
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Nom :");
                            ui.text_edit_singleline(&mut y.name);
                        });
                    }
                    if ui.small_button("Tout sélectionner dans ce ymap").clicked() {
                        select_ids = Some(y.entities.iter().map(|e| e.id).collect());
                    }
                    if !y.sample {
                        for e in y.entities.iter().take(200) {
                            let label = if e.mlo.is_some() { format!("🏠 {}", e.archetype) } else { e.archetype.clone() };
                            let r = ui.selectable_label(sel_set.contains(&e.id), label);
                            if r.clicked() {
                                select_ids = Some(vec![e.id]);
                            }
                            if r.double_clicked() {
                                goto = Some(e.pos);
                            }
                        }
                        if y.entities.len() > 200 {
                            ui.label(format!("… et {} autres", y.entities.len() - 200));
                        }
                    }
                });
            }
            for (ti, t) in self.proj.ytyps.iter_mut().enumerate() {
                egui::CollapsingHeader::new(format!("📦 {}.ytyp  ({} intérieur(s))", t.name, t.mlos.len())).id_salt(("ytyp", ti)).default_open(true).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut t.visible, "Visible");
                        if ui.small_button("🗑").on_hover_text("Retirer ce ytyp du projet").clicked() {
                            remove_ytyp = Some(ti);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Nom :");
                        ui.text_edit_singleline(&mut t.name);
                    });
                    if !t.raw_archetypes.is_empty() {
                        ui.label(RichText::new(format!("{} archétype(s) simple(s) conservé(s)", t.raw_archetypes.len())).small().weak());
                    }
                    for (mi, m) in t.mlos.iter().enumerate() {
                        let txt = format!("🏠 {}  — {} pièce(s), {} objet(s)", m.name, m.rooms.len().saturating_sub(1), m.entities.len());
                        if ui.selectable_label(cur_mlo == Some((ti, mi)), txt).clicked() {
                            new_mlo = Some((ti, mi));
                        }
                    }
                });
            }
        });
        if let Some(i) = new_target {
            self.target_ymap = i;
        }
        if let Some(m) = new_mlo {
            self.cur_mlo = Some(m);
            self.tab = Tab::Interior;
        }
        if let Some(i) = remove_ymap {
            self.push_undo();
            self.proj.ymaps.remove(i);
            self.clean_selection();
        }
        if let Some(i) = remove_ytyp {
            self.push_undo();
            self.proj.ytyps.remove(i);
            self.cur_mlo = None;
            self.clean_selection();
        }
        if let Some(ids) = select_ids {
            self.set_selection(ids);
            self.tab = Tab::Selection;
        }
        if let Some(p) = goto {
            self.cam.target = p;
        }
    }

    pub fn toolbox(&mut self, ui: &mut egui::Ui) {
        // l'assistant IA et le créateur de terrain sont maintenant des fenêtres à part
        if self.tab == Tab::Ai {
            self.ai_open = true;
            self.tab = Tab::Selection;
        }
        if self.tab == Tab::Terrain {
            self.terrain_open = true;
            self.tab = Tab::Selection;
        }
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Game, "🗺 Jeu");
            ui.selectable_value(&mut self.tab, Tab::Selection, "Sélection");
            ui.selectable_value(&mut self.tab, Tab::Props, "Props");
            ui.selectable_value(&mut self.tab, Tab::Interior, "Intérieur");
            ui.selectable_value(&mut self.tab, Tab::Lists, "Listes");
            ui.selectable_value(&mut self.tab, Tab::Options, "Options");
        });
        ui.separator();
        match self.tab {
            Tab::Game => {
                egui::ScrollArea::vertical().id_salt("gametab").show(ui, |ui| self.game_tab(ui));
            }
            Tab::Selection => {
                egui::ScrollArea::vertical().id_salt("seltab").show(ui, |ui| self.selection_tab(ui));
            }
            Tab::Props => {
                egui::ScrollArea::vertical().id_salt("proptab").show(ui, |ui| self.props_tab(ui));
            }
            Tab::Interior => {
                egui::ScrollArea::vertical().id_salt("inttab").show(ui, |ui| self.interior_tab(ui));
            }
            Tab::Ai => {}
            Tab::Lists => {
                egui::ScrollArea::vertical().id_salt("liststab").show(ui, |ui| self.lists_tab(ui));
            }
            Tab::Terrain => {}
            Tab::Options => {
                egui::ScrollArea::vertical().id_salt("opttab").show(ui, |ui| self.options_tab(ui));
            }
        }
    }

    fn selection_tab(&mut self, ui: &mut egui::Ui) {
        if !self.selected.is_empty() && ui.button(RichText::new("Sélection intelligente : tout ce qui va avec (J)").strong()).on_hover_text("Prend tout ce qui est dans ou sur l'objet choisi : props, décalques, intérieurs, versions lointaines… (ex. tout un yacht)").clicked() {
            self.smart_select();
        }
        if ui.button("Occlusion automatique (tout le projet)").on_hover_text("Ajoute une boîte d'occlusion (violette) dans chaque grand bâtiment des cartes du projet, sans doublon, 300 au plus par carte : le jeu ne dessine pas ce qui est caché derrière").clicked() {
            self.status = self.add_occluders_auto();
        }
        if !self.selected.is_empty() {
            ui.horizontal(|ui| {
                let label = if self.isolate_sel.is_some() { "Tout revoir" } else { "Isoler" };
                if ui.button(RichText::new(label).strong()).on_hover_text("Ne montrer que la sélection (ni carte du jeu, ni terrain) · Échap ou Ctrl+H pour revenir").clicked() {
                    self.toggle_isolate();
                }
                if ui.button("Boîte d'occlusion").on_hover_text("Ajoute dans chaque grand bâtiment choisi une boîte (violette) derrière laquelle le jeu ne dessine rien : la ville va plus vite").clicked() {
                    self.status = self.add_occluders_for_selection();
                }
                if ui.button(RichText::new("Faire un groupe").strong()).on_hover_text("Range ces objets ensemble (ton propre bâtiment : station-service, coin de rue…) pour les poser ailleurs à la souris, autant de fois que tu veux").clicked() {
                    self.group_new = Some(crate::groups::NewGroup::default());
                }
                if self.bedit_possible() && ui.button("Éditer ce bâtiment").on_hover_text("Retirer à la main les morceaux qui ne te plaisent pas (bâtiments voisins, restes…) : clic sur un morceau ou rectangle. Seule cette copie change.").clicked() {
                    self.bedit_start();
                }
            });
            self.visible_until_ui(ui);
            ui.separator();
        }
        if self.selected.is_empty() {
            ui.label("Rien n'est sélectionné.");
            ui.label(RichText::new("Outil Sélection : clic sur un objet, Ctrl+clic pour en ajouter, ou glisse un rectangle dans le vide pour en prendre plusieurs d'un coup.").weak());
            ui.separator();
            self.quick_select(ui);
            return;
        }
        if self.selected.len() == 1 {
            let id = self.selected[0];
            let Some(r) = self.proj.find_id(id) else { return };
            self.single_entity(ui, r);
        } else {
            ui.label(RichText::new(format!("{} objets sélectionnés", self.selected.len())).strong().size(16.0));
            let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
            for id in &self.selected {
                if let Some(e) = self.proj.find_id(*id).and_then(|r| self.proj.ent(r)) {
                    *counts.entry(e.archetype.clone()).or_default() += 1;
                }
            }
            egui::ScrollArea::vertical().max_height(140.0).id_salt("sel_counts").show(ui, |ui| {
                for (k, v) in counts {
                    ui.label(format!("{v} × {k}"));
                }
            });
        }
        ui.separator();
        ui.label("Transformer toute la sélection :");
        ui.horizontal(|ui| {
            ui.label("Pas");
            ui.add(egui::DragValue::new(&mut self.move_step).range(0.01..=100.0).speed(0.1).suffix(" m"));
        });
        let st = self.move_step;
        let mut mv = None;
        let mut rot = None;
        ui.horizontal(|ui| {
            for (lbl, d) in [("X -", v3(-st, 0.0, 0.0)), ("X +", v3(st, 0.0, 0.0)), ("Y -", v3(0.0, -st, 0.0)), ("Y +", v3(0.0, st, 0.0)), ("Z -", v3(0.0, 0.0, -st)), ("Z +", v3(0.0, 0.0, st))] {
                if ui.button(lbl).clicked() {
                    mv = Some(d);
                }
            }
        });
        ui.horizontal(|ui| {
            for deg in [-90.0f32, -15.0, 15.0, 90.0] {
                if ui.button(format!("{deg:+}°")).clicked() {
                    rot = Some(deg);
                }
            }
        });
        if let Some(d) = mv {
            self.push_undo();
            self.move_selected(d);
        }
        if let Some(d) = rot {
            self.push_undo();
            self.rotate_selected(d);
        }
        ui.horizontal(|ui| {
            if ui.button("📋 Dupliquer").clicked() {
                { let off = self.copy_offset(); self.duplicate_selected(off); }
            }
            if ui.button(RichText::new("🗑 Supprimer tout").color(Color32::from_rgb(255, 120, 100))).clicked() {
                self.delete_selected();
            }
            if ui.button("Désélectionner").clicked() {
                self.set_selection(vec![]);
            }
        });
        if let Some(first) = self.selected.first().and_then(|id| self.proj.find_id(*id)).and_then(|r| self.proj.ent(r)).map(|e| e.archetype.clone()) {
            if ui.button(format!("Sélectionner tous les « {first} »")).on_hover_text("Prendre tous les objets du même modèle d'un coup").clicked() {
                // nom exact : « prop_bench_01 » ne prend pas « prop_bench_01a »
                let n = self.select_matching(&first, 0.0, true);
                self.status = format!("{n} « {first} » sélectionné(s).");
            }
        }
        ui.separator();
        self.quick_select(ui);
    }

    fn single_entity(&mut self, ui: &mut egui::Ui, r: ItemRef) {
        let where_ = match r {
            ItemRef::Ent(y, _) => format!("{}.ymap", self.proj.ymaps[y].name),
            ItemRef::MloEnt(t, m, _) => format!("intérieur {} ({}.ytyp)", self.proj.ytyps[t].mlos[m].name, self.proj.ytyps[t].name),
        };
        let Some(world) = self.proj.world_entity(r) else { return };
        let Some(mut e) = self.proj.ent(r).cloned() else { return };
        let before = e.clone();
        ui.label(RichText::new(&e.archetype).strong().size(16.0));
        ui.label(RichText::new(format!("dans {where_}")).weak());
        if e.mlo.is_some() {
            ui.label(RichText::new("Placement d'un intérieur (CMloInstanceDef)").color(Color32::from_rgb(80, 220, 255)));
        }
        let rooms: Vec<String> = match r {
            ItemRef::MloEnt(t, m, _) => self.proj.ytyps[t].mlos[m].rooms.iter().map(|r| r.name.clone()).collect(),
            _ => vec![],
        };
        egui::Grid::new("ent_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Modèle");
            ui.text_edit_singleline(&mut e.archetype);
            ui.end_row();
            ui.label("Position");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut e.pos.x).speed(0.1).prefix("x "));
                ui.add(egui::DragValue::new(&mut e.pos.y).speed(0.1).prefix("y "));
                ui.add(egui::DragValue::new(&mut e.pos.z).speed(0.1).prefix("z "));
            });
            ui.end_row();
            ui.label("Cap (°)");
            let mut h = e.rot.heading_deg();
            if ui.add(egui::DragValue::new(&mut h).speed(1.0).range(0.0..=360.0)).changed() {
                // seul le cap change : l'inclinaison X/Y de l'objet est gardée
                e.rot = e.rot.with_heading_deg(h);
            }
            ui.end_row();
            ui.label("Échelle XY / Z");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut e.scale_xy).speed(0.01).range(0.01..=100.0));
                ui.add(egui::DragValue::new(&mut e.scale_z).speed(0.01).range(0.01..=100.0));
            });
            ui.end_row();
            ui.label("lodDist");
            ui.add(egui::DragValue::new(&mut e.lod_dist).speed(1.0).range(0.0..=10000.0));
            ui.end_row();
            ui.label("Niveau LOD");
            egui::ComboBox::from_id_salt("lodlevel").selected_text(e.lod_level.replace("LODTYPES_DEPTH_", "")).show_ui(ui, |ui| {
                for l in ["HD", "LOD", "SLOD1", "SLOD2", "SLOD3", "ORPHANHD", "SLOD4"] {
                    ui.selectable_value(&mut e.lod_level, format!("LODTYPES_DEPTH_{l}"), l);
                }
            });
            ui.end_row();
            ui.label("flags");
            ui.add(egui::DragValue::new(&mut e.flags));
            ui.end_row();
            if !rooms.is_empty() {
                ui.label("Pièce");
                egui::ComboBox::from_id_salt("room").selected_text(rooms.get(e.room).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                    for (i, n) in rooms.iter().enumerate() {
                        ui.selectable_value(&mut e.room, i, n);
                    }
                });
                ui.end_row();
            }
        });
        if matches!(r, ItemRef::MloEnt(..)) {
            ui.label(RichText::new(format!("Position monde : {:.2}, {:.2}, {:.2}", world.pos.x, world.pos.y, world.pos.z)).weak().small());
        }
        let changed = e.archetype != before.archetype
            || e.pos != before.pos
            || e.rot != before.rot
            || e.scale_xy != before.scale_xy
            || e.scale_z != before.scale_z
            || e.lod_dist != before.lod_dist
            || e.lod_level != before.lod_level
            || e.flags != before.flags
            || e.room != before.room;
        if changed {
            self.push_undo();
            // un intérieur renommé garde le lien avec son archétype
            if e.mlo.is_some() && e.archetype != before.archetype {
                for t in &mut self.proj.ytyps {
                    for m in &mut t.mlos {
                        if m.name == before.archetype {
                            m.name = e.archetype.clone();
                        }
                    }
                }
            }
            if let Some(t) = self.proj.ent_mut(r) {
                *t = e;
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Centrer la vue").clicked() {
                self.cam.target = world.pos;
            }
            if ui.button("Poser au sol").on_hover_text("Pose l'objet sur ce qu'il y a dessous, au bon niveau (Maj seule ou Fin)").clicked() {
                let _ = (r, &world);
                let n = self.snap_selected_to_ground(true);
                self.status = format!("{n} objet(s) posé(s) au sol.");
            }
        });
    }

    fn quick_select(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Zone autour du centre de la vue").strong());
        ui.horizontal(|ui| {
            ui.label("Rayon");
            ui.add(egui::Slider::new(&mut self.area_radius, 5.0..=500.0).suffix(" m"));
        });
        ui.horizontal(|ui| {
            ui.label("Filtre");
            ui.add(egui::TextEdit::singleline(&mut self.erase_filter).hint_text("ex. tree, bench, dw_batiment").desired_width(160.0));
        });
        ui.horizontal(|ui| {
            if ui.button("Sélectionner la zone").clicked() {
                let f = self.erase_filter.clone();
                let n = self.select_filtered(&f, self.area_radius);
                self.status = format!("{n} objet(s) sélectionné(s) dans {} m.", self.area_radius);
            }
            if ui.button(RichText::new("Supprimer la zone").color(Color32::from_rgb(255, 120, 100))).on_hover_text("Supprime tout ce qui correspond, d'un seul coup (annulable)").clicked() {
                let f = self.erase_filter.clone();
                self.status = self.run_action(&Action::DeleteRadius { radius: self.area_radius, filter: f });
            }
        });
    }

    fn props_tab(&mut self, ui: &mut egui::Ui) {
        self.sollumz_panel(ui);
        ui.separator();
        if self.game.world.is_some() {
            self.game_catalog(ui);
        } else {
            if self.game.loading.is_some() {
                ui.label(RichText::new("Chargement des props du jeu…").weak());
            } else {
                ui.label(RichText::new("Charge GTA V (onglet « Jeu ») pour avoir tous les props du jeu avec leur aperçu. En attendant, voici quelques props d'exemple :").weak().small());
            }
            self.sample_catalog(ui);
        }
        self.props_options(ui);
    }

    fn sample_catalog(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut self.cat_filter).hint_text("Chercher un prop…").desired_width(200.0));
        });
        ui.horizontal_wrapped(|ui| {
            for c in std::iter::once("Tous").chain(catalog::CATEGORIES.iter().copied()) {
                if ui.selectable_label(self.cat_cat == c, c).clicked() {
                    self.cat_cat = c;
                }
            }
        });
        let f = self.cat_filter.to_lowercase();
        let mut picked = None;
        egui::ScrollArea::vertical().max_height(250.0).id_salt("catalog").show(ui, |ui| {
            for p in catalog::PROPS.iter() {
                if self.cat_cat != "Tous" && p.cat != self.cat_cat {
                    continue;
                }
                if !f.is_empty() && !p.name.contains(&f) && !p.label.to_lowercase().contains(&f) {
                    continue;
                }
                ui.horizontal(|ui| {
                    let (resp, painter) = ui.allocate_painter(egui::vec2(14.0, 14.0), Sense::hover());
                    painter.rect_filled(resp.rect, 3.0, Color32::from_rgb(p.color[0], p.color[1], p.color[2]));
                    let tip = format!("{}\n{} × {} × {} m (estimé)", p.name, p.size[0], p.size[1], p.size[2]);
                    if ui.selectable_label(self.place_arch == p.name, p.label).on_hover_text(tip).clicked() {
                        picked = Some(p.name);
                    }
                    ui.label(RichText::new(p.name).small().weak());
                });
            }
        });
        if let Some(n) = picked {
            self.place_arch = n.to_string();
            self.tool = Tool::Place;
        }
    }

    fn props_options(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Modèle :");
            ui.add(egui::TextEdit::singleline(&mut self.place_arch).desired_width(200.0)).on_hover_text("Tu peux taper n'importe quel nom d'archétype du jeu");
        });
        ui.separator();
        ui.label(RichText::new("Options de pose").strong());
        ui.checkbox(&mut self.rand_rot, "Rotation au hasard");
        ui.checkbox(&mut self.rand_scale, "Taille légèrement variée (±15 %)");
        ui.checkbox(&mut self.place_into_mlo, "Dans un intérieur : ranger le prop dans la pièce");
        ui.horizontal(|ui| {
            ui.label("Écart en glissant");
            ui.add(egui::Slider::new(&mut self.stroke_spacing, 0.5..=50.0).suffix(" m"));
        });
        if ui.button(format!("Poser « {} » devant la caméra", self.place_arch)).clicked() {
            self.push_undo();
            let (f, _) = self.cam.flat_axes();
            let p = if self.cam.map_view { self.cam.target } else { self.cam.target + f * mech::SPAWN_DIST };
            let arch = self.place_arch.clone();
            let id = self.place_prop(&arch, p, None);
            self.set_selection(vec![id]);
        }
        ui.label(RichText::new("Astuce : avec l'outil Poser, garde le clic gauche enfoncé et dessine un trait pour aligner des props (barrières, arbres, lampadaires…).").weak().small());
        ui.separator();
        ui.label(RichText::new("Gomme (outil 3)").strong());
        ui.horizontal(|ui| {
            ui.label("Rayon");
            ui.add(egui::Slider::new(&mut self.erase_radius, 1.0..=300.0).suffix(" m"));
        });
        ui.horizontal(|ui| {
            ui.label("Seulement");
            ui.add(egui::TextEdit::singleline(&mut self.erase_filter).hint_text("vide = tout").desired_width(150.0));
        });
        ui.checkbox(&mut self.erase_buildings, "Gommer aussi les gros objets (bâtiments…)").on_hover_text("Décochée, la gomme épargne les bâtiments créés dans DragoWalker et les objets du jeu de plus de 20 m (bâtiments, ponts, gros rochers…). Les objets cachés (H) ne sont jamais gommés.");
    }

    /// Intérieurs du jeu proches de la vue : choisir leurs variantes (groupes d'objets).
    fn game_interiors(&mut self, ui: &mut egui::Ui) {
        // (index de l'objet, ymap, index dans le ymap, nom, distance, groupes, groupes actifs du jeu)
        let mut near: Vec<(u32, usize, usize, String, f32, Vec<u32>, Vec<u32>)> = Vec::new();
        {
            let Some(w) = self.game.world.as_ref() else { return };
            let t = self.cam.target;
            for (ei, e) in w.ents.iter().enumerate() {
                if !e.mlo {
                    continue;
                }
                let d = (e.pos - t).len();
                if d > 300.0 {
                    continue;
                }
                let Some(def) = w.mlo_defs.get(&e.arch) else { continue };
                if def.sets.is_empty() {
                    continue;
                }
                let y = &w.ymaps[e.ymap as usize];
                if !y.visible && !y.in_project {
                    continue;
                }
                let defaults = w.mlo_inst_sets.get(&(ei as u32)).cloned().unwrap_or_default();
                near.push((ei as u32, e.ymap as usize, ei - y.first, w.name_of(e.arch), d, def.sets.clone(), defaults));
            }
        }
        near.sort_by(|a, b| a.4.partial_cmp(&b.4).unwrap_or(std::cmp::Ordering::Equal));
        near.truncate(6);
        // repliée par défaut pour ne pas pousser le reste de l'onglet vers le bas
        let id = ui.make_persistent_id("gmlo_open");
        let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, false)) || std::env::var("DW_TEST_SET").is_ok();
        let title = RichText::new(format!("{} Variantes des intérieurs du jeu ({} proches)", if open { "-" } else { "+" }, near.len())).strong();
        if ui.toggle_value(&mut open, title).clicked() {
            ui.data_mut(|d| d.insert_persisted(id, open));
        }
        if !open {
            ui.separator();
            return;
        }
        if near.is_empty() {
            ui.label(RichText::new("Aucun intérieur à variantes à moins de 300 m du centre de la vue.").weak().small());
            ui.separator();
            return;
        }
        ui.label(RichText::new("Coche les groupes d'objets à montrer (décor, meubles, travaux...). Le fichier de la carte est copié dans le projet et exporté avec ton choix.").weak().small());
        let mut change: Option<(usize, usize, u32, bool)> = None;
        // libellé lisible de la variante changée (objets qu'elle contient), pour la barre d'état
        let mut change_label: Option<String> = None;
        for (ei, yi, li, name, d, sets, game_defaults) in &near {
            let (names, ymap_name) = {
                let w = self.game.world.as_ref().unwrap();
                // les noms des groupes ne sont pas dans les fichiers du jeu (ni dans les .nametable) : on montre
                // le nom s'il est connu, sinon les objets que le groupe contient
                let def = w.mlo_defs.get(&w.ents[*ei as usize].arch);
                let label = |si: usize, h: u32| {
                    let n = w.name_of(h);
                    if !n.starts_with("hash_") {
                        return n;
                    }
                    let kids: Vec<String> = def.map(|d| d.children.iter().filter(|c| c.set == si as i32).map(|c| w.name_of(c.arch)).collect()).unwrap_or_default();
                    match kids.len() {
                        0 => "vide".to_string(),
                        1 => kids[0].clone(),
                        k => format!("{} + {} autre(s)", kids[0], k - 1),
                    }
                };
                (sets.iter().enumerate().map(|(si, h)| label(si, *h)).collect::<Vec<String>>(), w.ymaps[*yi].name.clone())
            };
            // choix actuel : celui du projet si la carte y est copiée, sinon celui du jeu
            let proj_sets: Option<Vec<u32>> = self.game.links.get(&ymap_name).and_then(|ids| ids.get(*li)).and_then(|id| {
                let r = self.proj.find_id(*id)?;
                let e = self.proj.ent(r)?;
                Some(e.mlo.as_ref()?.default_entity_sets.iter().map(|s| crate::render::arch_hash(s)).collect())
            });
            let active = proj_sets.clone().unwrap_or_else(|| game_defaults.clone());
            let tag = if proj_sets.is_some() { " · dans le projet" } else { "" };
            egui::CollapsingHeader::new(format!("{name} ({d:.0} m){tag}")).id_salt(("gmlo", *ei)).default_open(*ei == near[0].0).show(ui, |ui| {
                for (si, h) in sets.iter().enumerate() {
                    let mut on = active.contains(h) || active.contains(&(si as u32));
                    if ui.checkbox(&mut on, format!("Variante {} · {}", si + 1, names[si])).changed() {
                        change = Some((*yi, *li, *h, on));
                        change_label = Some(format!("{} · {}", si + 1, names[si]));
                    }
                }
            });
        }
        // test : DW_TEST_SET="n" coche le groupe n de l'intérieur le plus proche (une fois)
        static SET_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if let Ok(v) = std::env::var("DW_TEST_SET") {
            if !SET_DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
                if let (Ok(n), Some(first)) = (v.trim().parse::<usize>(), near.first()) {
                    if let Some(h) = first.5.get(n) {
                        change = Some((first.1, first.2, *h, true));
                    }
                }
            }
        }
        if let Some((yi, li, h, on)) = change {
            self.push_undo();
            let ids = self.import_game_ymaps(&[yi]);
            if let Some(id) = ids.get(&(yi, li)).copied() {
                let set_name = self.game.world.as_ref().map(|w| w.name_of(h)).unwrap_or_default();
                if let Some(r) = self.proj.find_id(id) {
                    if let Some(m) = self.proj.ent_mut(r).and_then(|e| e.mlo.as_mut()) {
                        m.default_entity_sets.retain(|s| crate::render::arch_hash(s) != h);
                        if on {
                            m.default_entity_sets.push(set_name.clone());
                        }
                    }
                }
                let shown = change_label.clone().unwrap_or_else(|| set_name.clone());
                self.status = format!("Variante {shown} {}.", if on { "affichée" } else { "cachée" });
            }
        }
        ui.separator();
    }

    /// Analyse d'un bâtiment du jeu (première étape pour en faire un intérieur).
    fn building_analysis(&mut self, ui: &mut egui::Ui) {
        if self.game.world.is_none() {
            return;
        }
        ui.label(RichText::new("Analyser un bâtiment du jeu").strong());
        ui.label(RichText::new("Choisis un bâtiment (clic dessus), puis analyse-le : vitres en bleu, sols en vert, murs extérieurs en orange, murs intérieurs en violet.").weak().small());
        ui.horizontal(|ui| {
            if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Analyser l'objet choisi")).clicked() {
                self.analyse_selected();
            }
            if self.game.analysis.is_some() && ui.button("Effacer").clicked() {
                self.game.analysis = None;
            }
        });
        if ui
            .add_enabled(!self.selected.is_empty(), egui::Button::new(RichText::new("Transformer en intérieur").strong()))
            .on_hover_text("Tout d'un coup : pièces de tous les étages, type de chaque pièce d'après sa taille, meubles, portes dans les passages, lumières et ambiance. À corriger ensuite dans la liste des pièces.")
            .clicked()
        {
            self.status = self.convert_to_interior();
        }
        ui.label(RichText::new("« Transformer en intérieur » ouvre l'entrée dans la façade et donne à chaque pièce sa coque (sol, murs, plafond) et sa collision. Pour d'autres portes ou fenêtres : onglet « Percer des ouvertures ».").weak().small());
        let mut cut_to: Option<f32> = None;
        let mut rooms_for: Option<usize> = None;
        if let Some(a) = self.game.analysis.as_mut() {
            ui.checkbox(&mut a.show, "Afficher en couleurs dans la vue");
            egui::Grid::new("analyse").num_columns(2).show(ui, |ui| {
                ui.label("Bâtiment");
                ui.label(&a.name);
                ui.end_row();
                ui.label("Taille");
                ui.label(format!("{:.0} × {:.0} × {:.0} m", a.size.x, a.size.y, a.size.z));
                ui.end_row();
                ui.label("Vitres");
                ui.label(format!("{:.0} m²", a.glass_area));
                ui.end_row();
                ui.label("Murs extérieurs / intérieurs");
                ui.label(format!("{:.0} / {:.0} m²", a.wall_out_area, a.wall_in_area));
                ui.end_row();
                ui.label("Triangles");
                ui.label(format!("{}", a.tri_count));
                ui.end_row();
            });
            ui.label(RichText::new(format!("Étages trouvés : {}", a.levels.len())).strong());
            for (i, (z, s)) in a.levels.iter().enumerate() {
                let roof = (a.roof_z - z).abs() < 3.0;
                ui.horizontal(|ui| {
                    ui.label(format!("{} · hauteur {:.1} m · {:.0} m²{}", i + 1, z, s, if roof { " (toit)" } else { "" }));
                    if ui.small_button("Voir").on_hover_text("Coupe la vue juste au-dessus de cet étage").clicked() {
                        cut_to = Some(*z);
                    }
                    if !roof && ui.small_button("Pièces").on_hover_text("Proposer les pièces de cet étage, à la bonne hauteur, à corriger ensuite").clicked() {
                        rooms_for = Some(i);
                    }
                });
            }
        }
        if let Some(li) = rooms_for {
            self.rooms_from_level(li);
            if let Some(z) = self.game.analysis.as_ref().and_then(|a| a.levels.get(li)).map(|l| l.0) {
                cut_to = Some(z);
            }
        }
        if let Some(z) = cut_to {
            // coupe 2,5 m au-dessus du sol de l'étage, centre de la vue sur l'étage
            self.cam.target.z = z;
            self.game.cut_on = true;
            self.game.cut_h = 2.5;
        }
        ui.separator();
    }

    fn interior_tab(&mut self, ui: &mut egui::Ui) {
        self.building_analysis(ui);
        self.game_interiors(ui);
        ui.label(RichText::new("Créer un intérieur (MLO) en 1 clic").strong());
        egui::Grid::new("mlo_wiz").num_columns(2).show(ui, |ui| {
            ui.label("Nom");
            ui.text_edit_singleline(&mut self.mlo_name);
            ui.end_row();
            ui.label("Pièces");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.mlo_cols).range(1..=8));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut self.mlo_rows).range(1..=8));
            });
            ui.end_row();
            ui.label("Taille d'une pièce");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.mlo_w).range(2.0..=60.0).speed(0.1).suffix(" m"));
                ui.add(egui::DragValue::new(&mut self.mlo_d).range(2.0..=60.0).speed(0.1).suffix(" m"));
                ui.add(egui::DragValue::new(&mut self.mlo_h).range(2.0..=20.0).speed(0.1).suffix(" m"));
            });
            ui.end_row();
        });
        if ui.button(RichText::new("🏠 Créer l'intérieur au centre de la vue").strong()).clicked() {
            let n = self.mlo_name.clone();
            self.create_mlo(&n, self.mlo_cols, self.mlo_rows, self.mlo_w, self.mlo_d, self.mlo_h);
        }
        ui.label(RichText::new("La pièce « limbo » (extérieur), les portails entre pièces voisines et la porte d'entrée sont créés automatiquement.").weak().small());
        ui.separator();
        let Some((t, m)) = self.cur_mlo else {
            ui.label("Aucun intérieur ouvert. Choisis-en un dans le panneau Projet ou crée-en un.");
            return;
        };
        let Some(orig) = self.proj.ytyps.get(t).and_then(|y| y.mlos.get(m)) else { return };
        let mut mlo = orig.clone();
        let snapshot = |m: &model::MloArchetype| format!("{:?}", (&m.name, m.rooms.iter().map(|r| (&r.name, r.bb_min, r.bb_max, &r.timecycle)).collect::<Vec<_>>()));
        let before = snapshot(&mlo);
        ui.label(RichText::new(format!("Intérieur : {}", mlo.name)).strong().size(15.0).color(Color32::from_rgb(80, 220, 255)));
        ui.horizontal(|ui| {
            ui.label("Nom");
            ui.text_edit_singleline(&mut mlo.name);
        });
        ui.label(format!("{} pièce(s), {} portail(s), {} objet(s)", mlo.rooms.len().saturating_sub(1), mlo.portals.len(), mlo.entities.len()));
        let mut del_room = None;
        let mut sel_room = None;
        let mut furnish_room: Option<(usize, &str)> = None;
        egui::ScrollArea::vertical().max_height(220.0).id_salt("rooms").show(ui, |ui| {
            for (ri, r) in mlo.rooms.iter_mut().enumerate() {
                if ri == 0 {
                    ui.label(RichText::new("0 · limbo (extérieur)").weak());
                    continue;
                }
                egui::CollapsingHeader::new(format!("{ri} · {}", r.name)).id_salt(("room", ri)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Nom");
                        ui.text_edit_singleline(&mut r.name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Min");
                        ui.add(egui::DragValue::new(&mut r.bb_min.x).speed(0.1));
                        ui.add(egui::DragValue::new(&mut r.bb_min.y).speed(0.1));
                        ui.add(egui::DragValue::new(&mut r.bb_min.z).speed(0.1));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Max");
                        ui.add(egui::DragValue::new(&mut r.bb_max.x).speed(0.1));
                        ui.add(egui::DragValue::new(&mut r.bb_max.y).speed(0.1));
                        ui.add(egui::DragValue::new(&mut r.bb_max.z).speed(0.1));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Timecycle");
                        ui.add(egui::TextEdit::singleline(&mut r.timecycle).hint_text("ex. int_GasStation").desired_width(140.0));
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("Sélectionner ses objets").clicked() {
                            sel_room = Some(ri);
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Remplir :");
                        for st in ["salon", "bureau", "bar", "magasin", "cuisine"] {
                            if ui.small_button(st).on_hover_text("Poser les meubles de ce type de pièce, sur son sol").clicked() {
                                furnish_room = Some((ri, st));
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("🗑 Supprimer la pièce").clicked() {
                            del_room = Some(ri);
                        }
                    });
                });
            }
        });
        let mut changed = before != snapshot(&mlo);
        ui.horizontal(|ui| {
            if ui.button("+ Pièce").on_hover_text("Ajoute une pièce à droite des autres, reliée par un portail").clicked() {
                let mx = mlo.bb_max();
                let mn = mlo.bb_min();
                let name = if mlo.rooms.iter().any(|r| r.name == mech::ROOM_DEFAULT_NAME) { format!("{}{}", mech::ROOM_DEFAULT_NAME, mlo.rooms.len()) } else { mech::ROOM_DEFAULT_NAME.to_string() };
                mlo.rooms.push(model::Room::new(&name, v3(mx.x, mn.y, 0.0), v3(mx.x + self.mlo_w, mn.y + self.mlo_d, self.mlo_h)));
                let ri = mlo.rooms.len() - 1;
                let door = 1.6f32.min(self.mlo_d * 0.4);
                let cy = mn.y + self.mlo_d * 0.5;
                let neighbour = mlo.rooms.iter().enumerate().skip(1).take(ri - 1).find(|(_, r)| (r.bb_max.x - mx.x).abs() < 0.01 && r.bb_min.y <= cy && r.bb_max.y >= cy).map(|(i, _)| i);
                if let Some(ni) = neighbour {
                    mlo.portals.push(model::Portal::new(ni as u32, ri as u32, [v3(mx.x, cy - door / 2.0, 0.0), v3(mx.x, cy - door / 2.0, 2.2), v3(mx.x, cy + door / 2.0, 2.2), v3(mx.x, cy + door / 2.0, 0.0)]));
                }
                changed = true;
            }
            if ui.button("+ Portail").on_hover_text("Portail par défaut de CodeWalker (de la pièce 1 vers l'extérieur)").clicked() {
                mlo.portals.push(model::Portal::new(mech::PORTAL_ROOM_FROM, mech::PORTAL_ROOM_TO, [v3(0.0, 0.0, 0.0), v3(0.0, 0.0, 1.0), v3(0.0, 1.0, 1.0), v3(0.0, 1.0, 0.0)]));
                changed = true;
            }
            if ui.button("+ Groupe d'objets").clicked() {
                mlo.entity_sets.push(model::EntitySet { name: mech::ENTITY_SET_DEFAULT_NAME.into(), entities: vec![] });
                changed = true;
            }
        });
        if let Some(ri) = del_room {
            mlo.rooms.remove(ri);
            mlo.portals.retain(|p| p.room_from as usize != ri && p.room_to as usize != ri);
            for p in &mut mlo.portals {
                if p.room_from as usize > ri {
                    p.room_from -= 1;
                }
                if p.room_to as usize > ri {
                    p.room_to -= 1;
                }
            }
            mlo.entities.retain(|e| e.room != ri);
            for e in &mut mlo.entities {
                if e.room > ri {
                    e.room -= 1;
                }
            }
            changed = true;
        }
        if !mlo.portals.is_empty() {
            let names: Vec<String> = mlo.rooms.iter().map(|r| r.name.clone()).collect();
            let mut del = None;
            ui.collapsing(format!("Portails ({})", mlo.portals.len()), |ui| {
                for (pi, p) in mlo.portals.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let old = (p.room_from, p.room_to);
                        ui.label(format!("{pi}:"));
                        egui::ComboBox::from_id_salt(("pf", pi)).width(90.0).selected_text(names.get(p.room_from as usize).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                            for (i, n) in names.iter().enumerate() {
                                ui.selectable_value(&mut p.room_from, i as u32, n);
                            }
                        });
                        ui.label("»");
                        egui::ComboBox::from_id_salt(("pt", pi)).width(90.0).selected_text(names.get(p.room_to as usize).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                            for (i, n) in names.iter().enumerate() {
                                ui.selectable_value(&mut p.room_to, i as u32, n);
                            }
                        });
                        if ui.small_button("🗑").clicked() {
                            del = Some(pi);
                        }
                        if old != (p.room_from, p.room_to) {
                            changed = true;
                        }
                    });
                }
            });
            if let Some(d) = del {
                mlo.portals.remove(d);
                changed = true;
            }
        }
        self.plan_ui(ui);
        ui.separator();
        ui.label(RichText::new("Meubler automatiquement").strong());
        let mut furnish = None;
        ui.horizontal_wrapped(|ui| {
            for st in ["salon", "bureau", "bar", "magasin", "cuisine"] {
                if ui.button(st).clicked() {
                    furnish = Some(st);
                }
            }
        });
        let old_name = self.proj.ytyps[t].mlos[m].name.clone();
        if changed {
            self.push_undo();
            if mlo.name != old_name {
                for y in &mut self.proj.ymaps {
                    for e in &mut y.entities {
                        if e.mlo.is_some() && e.archetype == old_name {
                            e.archetype = mlo.name.clone();
                        }
                    }
                }
            }
            self.proj.ytyps[t].mlos[m] = mlo;
        }
        if let Some(ri) = sel_room {
            let ids: Vec<u64> = self.proj.ytyps[t].mlos[m].entities.iter().filter(|e| e.room == ri).map(|e| e.id).collect();
            self.set_selection(ids);
        }
        if let Some(st) = furnish {
            self.status = self.furnish_mlo(st);
        }
        if let Some((ri, st)) = furnish_room {
            self.status = self.furnish_rooms(st, Some(ri));
        }
        let name = self.proj.ytyps[t].mlos[m].name.clone();
        if ui.button("Centrer la vue sur l'intérieur").clicked() {
            if let Some((p, _)) = self.proj.mlo_placement(&name) {
                self.cam.target = p;
                self.cam.dist = 30.0;
            }
        }
    }

    fn ai_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Assistant IA").strong());
        self.ai_settings(ui);
        let h = (ui.available_height() - 90.0).max(120.0);
        egui::ScrollArea::vertical().max_height(h).stick_to_bottom(true).auto_shrink([false; 2]).id_salt("chat").show(ui, |ui| {
            for l in &self.chat {
                let (bg, align) = if l.from_user { (Color32::from_rgb(120, 70, 25), egui::Align::Max) } else { (Color32::from_rgb(48, 54, 64), egui::Align::Min) };
                ui.with_layout(egui::Layout::top_down(align), |ui| {
                    egui::Frame::new().fill(bg).corner_radius(6.0).inner_margin(6.0).show(ui, |ui| {
                        ui.set_max_width(ui.available_width() * 0.92);
                        ui.label(&l.text);
                    });
                });
            }
            if self.pending.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("{} réfléchit…", match self.ai_provider {
                        crate::ai::Provider::ClaudeCode => "Claude Code",
                        crate::ai::Provider::ClaudeApi => "Claude",
                        _ => "L'IA",
                    }));
                });
            }
        });
        ui.separator();
        let r = ui.add(egui::TextEdit::multiline(&mut self.chat_input).hint_text("Ex. : supprime les arbres dans un rayon de 80 m").desired_rows(2).desired_width(f32::INFINITY));
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
        ui.horizontal(|ui| {
            let send = ui.add_enabled(self.pending.is_none(), egui::Button::new("Envoyer ➡")).clicked();
            if send || enter {
                self.chat_input = self.chat_input.trim_end_matches('\n').to_string();
                self.send_chat();
            }
            if ui.button("Effacer").clicked() {
                self.chat.truncate(1);
            }
        });
    }

    /// Choix du moteur d'IA et de ses réglages (enregistrés sur ce PC).
    fn ai_settings(&mut self, ui: &mut egui::Ui) {
        use crate::ai::Provider;
        use crate::app::save_setting;
        let before = self.ai_provider;
        egui::ComboBox::from_id_salt("ai_provider").width(ui.available_width().min(330.0)).selected_text(self.ai_provider.label()).show_ui(ui, |ui| {
            for p in Provider::ALL {
                ui.selectable_value(&mut self.ai_provider, p, p.label());
            }
        });
        if self.ai_provider != before {
            save_setting("ai_provider", serde_json::json!(self.ai_provider.key()));
        }
        let ok = Color32::from_rgb(140, 210, 140);
        let warn = Color32::from_rgb(255, 170, 120);
        egui::CollapsingHeader::new(RichText::new("Réglages de l'IA").small()).id_salt("ai_cfg").default_open(false).show(ui, |ui| match self.ai_provider {
            Provider::ClaudeCode => {
                match &self.claude_code {
                    Some(p) => ui.label(RichText::new(format!("✔ Claude Code trouvé : {}", p.display())).small().color(ok)),
                    None => ui.label(RichText::new("Claude Code introuvable (commande « claude »). Installe-le puis relance DragoWalker.").small().color(warn)),
                };
                ui.label(RichText::new("Utilise ton compte Claude Code, sans clé API. Claude ne reçoit aucun outil : il ne peut que répondre par des actions sur la carte.").weak().small());
                ui.horizontal(|ui| {
                    ui.label("Modèle");
                    let r = egui::ComboBox::from_id_salt("cc_model").selected_text(if self.cc_model.is_empty() { "par défaut" } else { self.cc_model.as_str() }).show_ui(ui, |ui| {
                        let mut c = false;
                        for (v, l) in [("", "par défaut"), ("opus", "opus (le plus fort)"), ("sonnet", "sonnet (équilibré)"), ("haiku", "haiku (le plus rapide)")] {
                            c |= ui.selectable_value(&mut self.cc_model, v.to_string(), l).changed();
                        }
                        c
                    });
                    if r.inner == Some(true) {
                        save_setting("ai_cc_model", serde_json::json!(self.cc_model));
                    }
                });
            }
            Provider::ClaudeApi => {
                ui.label("Clé API Anthropic :");
                let r = ui.add(egui::TextEdit::singleline(&mut self.api_key).password(true).hint_text("sk-ant-…"));
                if r.lost_focus() {
                    save_api_key(self.api_key.trim());
                }
            }
            Provider::OpenAiCompat => {
                egui::Grid::new("oa_grid").num_columns(2).show(ui, |ui| {
                    ui.label("Adresse");
                    if ui.add(egui::TextEdit::singleline(&mut self.oa_base).desired_width(210.0)).lost_focus() {
                        save_setting("ai_oa_base", serde_json::json!(self.oa_base));
                    }
                    ui.end_row();
                    ui.label("Modèle");
                    if ui.add(egui::TextEdit::singleline(&mut self.oa_model).desired_width(210.0).hint_text("ex. llama3.1, gpt-4.1-mini, mistral-small")).lost_focus() {
                        save_setting("ai_oa_model", serde_json::json!(self.oa_model));
                    }
                    ui.end_row();
                    ui.label("Clé");
                    if ui.add(egui::TextEdit::singleline(&mut self.oa_key).password(true).desired_width(210.0).hint_text("vide pour Ollama / LM Studio")).lost_focus() {
                        crate::secret::save_secret("ai_oa_key", &self.oa_key);
                    }
                    ui.end_row();
                });
                ui.horizontal_wrapped(|ui| {
                    for (l, u) in [("Ollama", "http://localhost:11434/v1"), ("LM Studio", "http://localhost:1234/v1"), ("OpenAI", "https://api.openai.com/v1"), ("Mistral", "https://api.mistral.ai/v1"), ("Groq", "https://api.groq.com/openai/v1"), ("OpenRouter", "https://openrouter.ai/api/v1")] {
                        if ui.small_button(l).clicked() {
                            self.oa_base = u.to_string();
                            save_setting("ai_oa_base", serde_json::json!(self.oa_base));
                        }
                    }
                });
            }
            Provider::Cli => {
                ui.label(RichText::new("Commande lancée à chaque demande ; DragoWalker lui envoie la demande sur l'entrée et lit la réponse JSON.").weak().small());
                if ui.add(egui::TextEdit::singleline(&mut self.cli_cmd).desired_width(260.0).hint_text("ex. gemini -p   ·   ollama run llama3.1")).lost_focus() {
                    save_setting("ai_cli_cmd", serde_json::json!(self.cli_cmd));
                }
                if !self.cli_tools.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Trouvés sur ce PC :").small());
                        for (n, c) in self.cli_tools.clone() {
                            if ui.small_button(n).clicked() {
                                self.cli_cmd = c.to_string();
                                save_setting("ai_cli_cmd", serde_json::json!(self.cli_cmd));
                            }
                        }
                    });
                }
            }
            Provider::Local => {
                ui.label(RichText::new("Sans Internet : comprend les commandes courantes (poser, supprimer dans un rayon, créer un intérieur…).").weak().small());
            }
        });
    }

    fn options_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Assistant IA").strong());
        ui.label(RichText::new(format!("Moteur : {} — à changer dans la fenêtre Assistant IA (repli « Réglages de l'IA »).", self.ai_provider.label())).small());
        ui.separator();
        ui.label(RichText::new("Caméra").strong());
        ui.horizontal(|ui| {
            ui.label("Vitesse");
            ui.add(egui::Slider::new(&mut self.speed_mult, 0.05..=4.0).logarithmic(true).suffix(" ×"));
        });
        ui.label(
            RichText::new(format!(
                "Comme CodeWalker : {} m/s × min(distance, {}) · Maj ×{} · Ctrl ×{}",
                mech::BASE_MOVE_SPEED,
                mech::MOVE_DIST_CAP,
                mech::SHIFT_MULT,
                mech::CTRL_MULT
            ))
            .weak()
            .small(),
        );
        ui.horizontal(|ui| {
            ui.label("Hauteur du sol");
            ui.add(egui::DragValue::new(&mut self.ground_z).speed(0.1).suffix(" m"));
        });
        ui.separator();
        ui.label(RichText::new("Affichage").strong());
        ui.checkbox(&mut self.opts.show_grid, "Grille au sol");
        ui.checkbox(&mut self.opts.show_rooms, "Pièces et portails des intérieurs");
        ui.checkbox(&mut self.opts.show_sample, "Carte d'exemple (ville générée)");
        ui.separator();
        ui.label(RichText::new("Mémoire graphique").strong());
        let (vram, budget) = self.game.gpu.as_ref().and_then(|g| g.lock().ok().map(|g| (g.vram_mb, g.budget))).unwrap_or((None, None));
        let mut mb = crate::app::load_settings()["gpu_budget_mb"].as_u64().unwrap_or(0) as u32;
        let mut auto = mb == 0;
        let mut changed = false;
        if ui
            .checkbox(&mut auto, "Automatique (selon la carte graphique)")
            .on_hover_text("La vue garde les modèles et textures du jeu dans la carte graphique jusqu'à cette limite, puis libère les moins utilisés. Automatique : un peu plus de la moitié de la mémoire de la carte, 1 Go supposé si elle est inconnue.")
            .changed()
        {
            mb = if auto { 0 } else { budget.map(|b| ((b.0 + b.1) >> 20) as u32).unwrap_or(1024) };
            changed = true;
        }
        if !auto {
            ui.horizontal(|ui| {
                ui.label("Limite");
                changed |= ui.add(egui::Slider::new(&mut mb, 384..=2100).step_by(64.0).suffix(" Mo")).changed();
            });
        }
        ui.label(
            RichText::new(match (vram, budget) {
                (Some(v), Some(b)) => format!("Carte graphique : {v} Mo · limite actuelle {} Mo (modèles {} + textures {}).", (b.0 + b.1) >> 20, b.0 >> 20, b.1 >> 20),
                (None, Some(b)) => format!("Mémoire de la carte inconnue · limite actuelle {} Mo.", (b.0 + b.1) >> 20),
                _ => "Connue une fois la vue 3D du jeu affichée.".to_string(),
            })
            .weak()
            .small(),
        );
        if changed {
            crate::app::save_setting("gpu_budget_mb", serde_json::json!(mb));
            if let Some(g) = &self.game.gpu {
                if let Ok(mut g) = g.lock() {
                    g.reset_budget();
                }
            }
        }
    }
}
