//! Studio MLO : un espace à part, comme un petit Blender, pour construire un intérieur FiveM de A à Z.
//!
//! On y entre avec un intérieur du projet (ou un nouveau, vide ou avec une pièce de départ). Tant qu'on y est :
//! seul cet intérieur est dessiné (ni carte du jeu, ni terrain, ni autres objets), la barre du haut porte les
//! outils du plan (pièces, murs, ouvertures, blocs, lumières, miroirs, peinture), le panneau de gauche les
//! pièces, portails, variantes et réglages, celui de droite la pièce choisie (ambiance, meubles) et l'objet
//! choisi. « Placer sur la carte » ramène dans le monde avec l'intérieur choisi et les poignées de
//! déplacement ; « Quitter » rend la vue telle qu'elle était.

use crate::app::{DwApp, Tool};
use crate::look::{self, ic};
use crate::math::V3;
use eframe::egui::{self, Color32, RichText};
use std::collections::HashSet;

/// Ce qu'on remet en sortant du studio.
struct Saved {
    target: V3,
    yaw: f32,
    pitch: f32,
    dist: f32,
    map_view: bool,
    show_grid: bool,
    show_rooms: bool,
    isolate: Option<HashSet<u64>>,
    tool: Tool,
}

#[derive(Default)]
pub struct StudioState {
    pub on: bool,
    /// fenêtre « Studio MLO » (ouvrir un intérieur ou en créer un)
    pub choose: bool,
    /// intérieur ouvert (par son nom : les indices bougent quand on en ajoute ou en retire)
    pub mlo: String,
    pub new_name: String,
    /// départ d'un nouvel intérieur : 0 vide, 1 une pièce, 2 deux pièces
    pub new_kind: u8,
    /// coupe au-dessus de l'étage : on voit dans les pièces, sans plafond
    pub cut: bool,
    saved: Option<Saved>,
}

/// Hauteur de la coupe du studio au-dessus du sol de l'étage (m) — choix de DragoWalker (sous un plafond de 2,6 m).
pub const CUT_ABOVE_FLOOR: f32 = 2.4;

/// Outils du plan dans la barre du studio : (mode de plan.rs, icône, nom, aide).
const PLAN_TOOLS: [(u8, &str, &str, &str); 11] = [
    (6, ic::POLYGON, "Pièce", "Pièce de n'importe quelle forme : clique ses coins, double-clic ou Entrée pour finir. Sol, plafond et murs autour faits tout seuls."),
    (1, ic::SQUARE, "Rectangle", "Pièce rectangulaire : glisse sur le sol de l'étage ; une porte se fait avec chaque pièce voisine."),
    (4, ic::WALL, "Mur", "Clique les points du mur (aimantés à la grille et aux autres murs) ; double-clic ou Entrée : fini."),
    (5, ic::DOOR_OPEN, "Ouverture", "Clic sur un mur : porte, porte double, fenêtre ou passage (au choix dans le panneau de gauche), avec son portail."),
    (2, ic::SQUARE_SPLIT_HORIZONTAL, "Couper", "Clic dans une pièce rectangulaire : un mur la coupe en deux, avec une porte."),
    (3, ic::STAIRS, "Escalier", "Clic dans une pièce : escalier vers la pièce de l'étage du dessus (trace-la d'abord)."),
    (7, ic::CUBE, "Bloc", "Glisse un rectangle : comptoir, banquette, estrade, muret ou colonne."),
    (9, ic::LIGHTBULB, "Lumière", "Clic sur le sol : spot, ampoule ou néon, couleur et intensité au choix."),
    (10, ic::PAINT_ROLLER, "Peindre", "Clic près d'un mur, du côté à repeindre : texture et couleur choisies, de ce côté seulement."),
    (11, ic::FRAME_CORNERS, "Miroir", "Clic près d'un mur, du côté du miroir : miroir avec son portail « miroir »."),
    (8, ic::ERASER, "Effacer", "Clic : efface le miroir, la lumière, l'ouverture, le bloc, le mur ou la pièce sous la souris."),
];

impl DwApp {
    /// Indices (ytyp, intérieur) de l'intérieur nommé.
    fn studio_index(&self, name: &str) -> Option<(usize, usize)> {
        self.proj.ytyps.iter().enumerate().find_map(|(ti, t)| t.mlos.iter().position(|m| m.name.eq_ignore_ascii_case(name)).map(|mi| (ti, mi)))
    }

    /// Ce qui reste visible dans le studio : le placement de l'intérieur, ses objets et ceux de ses variantes.
    fn studio_ids(&self, ti: usize, mi: usize) -> HashSet<u64> {
        let m = &self.proj.ytyps[ti].mlos[mi];
        let mut ids: HashSet<u64> = m.entities.iter().map(|e| e.id).collect();
        ids.extend(m.entity_sets.iter().flat_map(|s| s.entities.iter().map(|e| e.id)));
        for y in &self.proj.ymaps {
            for e in &y.entities {
                if e.mlo.is_some() && e.archetype.eq_ignore_ascii_case(&m.name) {
                    ids.insert(e.id);
                }
            }
        }
        ids
    }

    /// Centre de l'intérieur dans le monde et taille (pour cadrer la vue).
    fn studio_frame(&self, ti: usize, mi: usize) -> Option<(V3, f32)> {
        let m = &self.proj.ytyps[ti].mlos[mi];
        let (p, q) = self.proj.mlo_placement(&m.name)?;
        if m.rooms.len() <= 1 {
            return Some((p, 18.0));
        }
        let (a, b) = (m.bb_min(), m.bb_max());
        let c = q.rotate((a + b) * 0.5) + p;
        let size = (b - a).len().max(6.0);
        Some((c, size * 1.1))
    }

    /// Entre dans le studio avec l'intérieur nommé (il doit être placé dans une carte du projet).
    pub fn studio_enter(&mut self, name: &str) {
        let Some((ti, mi)) = self.studio_index(name) else { return };
        if self.proj.mlo_placement(name).is_none() {
            self.status = format!("« {name} » n'est placé dans aucune carte du projet : impossible de l'ouvrir dans le studio.");
            return;
        }
        if !self.studio.on {
            self.studio.saved = Some(Saved {
                target: self.cam.target,
                yaw: self.cam.yaw,
                pitch: self.cam.pitch,
                dist: self.cam.dist,
                map_view: self.cam.map_view,
                show_grid: self.opts.show_grid,
                show_rooms: self.opts.show_rooms,
                isolate: self.isolate_sel.clone(),
                tool: self.tool,
            });
        }
        self.studio.on = true;
        self.studio.cut = true;
        self.studio.choose = false;
        self.studio.mlo = self.proj.ytyps[ti].mlos[mi].name.clone();
        self.cur_mlo = Some((ti, mi));
        self.opts.show_grid = true;
        // les boîtes des pièces cachent la coque : bouton dans la barre du studio pour les revoir
        self.opts.show_rooms = false;
        self.tool = Tool::Select;
        self.plan.mode = 0;
        self.plan.pts.clear();
        self.plan.drag = None;
        self.isolate_sel = Some(self.studio_ids(ti, mi));
        self.opts.studio = self.proj.mlo_placement(name).map(|(p, _)| p.z);
        if let Some((c, d)) = self.studio_frame(ti, mi) {
            self.cam.pan_left = V3::ZERO;
            self.cam.target = c;
            self.ground_z = self.proj.mlo_placement(name).map(|(p, _)| p.z).unwrap_or(c.z);
            self.cam.dist = d;
            self.cam.ortho = d;
            self.cam.snap();
        }
        // un intérieur sans pièce se dessine d'abord de dessus
        self.cam.map_view = self.proj.ytyps[ti].mlos[mi].rooms.len() <= 1;
        self.set_selection(vec![]);
        self.status = format!("Studio MLO : « {} ». Outils à gauche de la vue, réglages dans le panneau de gauche, ambiance, meubles et objet choisi à droite.", self.studio.mlo);
    }

    /// Sort du studio ; `place` : garde la vue sur l'intérieur, choisi, avec les poignées de déplacement.
    pub fn studio_exit(&mut self, place: bool) {
        if !self.studio.on {
            return;
        }
        self.studio.on = false;
        self.opts.studio = None;
        self.plan.mode = 0;
        self.plan.pts.clear();
        self.plan.drag = None;
        let name = self.studio.mlo.clone();
        if let Some(s) = self.studio.saved.take() {
            self.opts.show_grid = s.show_grid;
            self.opts.show_rooms = s.show_rooms;
            self.isolate_sel = s.isolate;
            self.tool = s.tool;
            self.cam.map_view = s.map_view;
            if !place {
                self.cam.target = s.target;
                self.cam.yaw = s.yaw;
                self.cam.pitch = s.pitch;
                self.cam.dist = s.dist;
                self.cam.ortho = s.dist;
                self.cam.snap();
            }
        } else {
            self.isolate_sel = None;
        }
        if place {
            let inst: Vec<u64> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.mlo.is_some() && e.archetype.eq_ignore_ascii_case(&name)).map(|e| e.id).take(1).collect();
            if let Some((p, _)) = self.proj.mlo_placement(&name) {
                self.goto(p, 40.0);
            }
            self.tool = Tool::Select;
            self.gizmo_mode = crate::gizmo::GizmoMode::Move;
            self.set_selection(inst);
            self.status = format!("« {name} » est choisi : déplace-le avec les flèches (G), tourne-le avec R, puis Ctrl+S.");
        } else {
            self.status = format!("Sorti du Studio MLO (« {name} »).");
        }
    }

    /// Crée un nouvel intérieur au centre de la vue et l'ouvre dans le studio.
    pub fn studio_new(&mut self) {
        let name = self.studio.new_name.trim().to_string();
        let (cols, rows) = if self.studio.new_kind == 2 { (2, 1) } else { (1, 1) };
        self.create_mlo(&name, cols, rows, 6.0, 5.0, 3.0);
        let Some((ti, mi)) = self.cur_mlo else { return };
        if self.studio.new_kind == 0 {
            // vide : seulement la pièce « limbo » (le dehors) ; tout se dessine ensuite avec les outils du plan
            let m = &mut self.proj.ytyps[ti].mlos[mi];
            m.rooms.truncate(1);
            m.portals.clear();
            m.entities.clear();
            let n = m.name.clone();
            for y in &mut self.proj.ymaps {
                for e in &mut y.entities {
                    if e.archetype.eq_ignore_ascii_case(&n) {
                        if let Some(d) = e.mlo.as_mut() {
                            d.num_exit_portals = 0;
                        }
                    }
                }
            }
        }
        let n = self.proj.ytyps[ti].mlos[mi].name.clone();
        self.studio.new_name.clear();
        self.studio_enter(&n);
        if self.studio.new_kind == 0 {
            self.plan.mode = 6;
            self.status = format!("Studio MLO : « {n} » est vide. Outil « Pièce » : clique les coins de la première pièce sur la grille, double-clic pour finir.");
        }
    }

    /// À chaque image : garde l'isolement à jour (objets ajoutés, meubles…) ; sort si l'intérieur a disparu.
    pub fn studio_tick(&mut self) {
        if !self.studio.on {
            return;
        }
        let name = self.studio.mlo.clone();
        let Some((ti, mi)) = self.studio_index(&name) else {
            self.studio_exit(false);
            self.status = format!("L'intérieur « {name} » n'existe plus : sortie du studio.");
            return;
        };
        let Some((p, _)) = self.proj.mlo_placement(&name) else {
            self.studio_exit(false);
            return;
        };
        self.opts.studio = Some(p.z + self.plan.floor as f32 * if self.plan.floor_h < 2.0 { 3.0 } else { self.plan.floor_h });
        self.cur_mlo = Some((ti, mi));
        let ids = self.studio_ids(ti, mi);
        if self.isolate_sel.as_ref() != Some(&ids) {
            self.isolate_sel = Some(ids);
        }
    }

    /// Fenêtre « Studio MLO » : intérieurs du projet à ouvrir, ou un nouveau.
    pub fn studio_window(&mut self, ctx: &egui::Context) {
        if !self.studio.choose {
            return;
        }
        let mut open = true;
        let mut enter: Option<String> = None;
        let mut new = false;
        let houses: HashSet<String> = self.proj.houses.iter().map(|h| format!("{}_int", h.name).to_lowercase()).collect();
        let list: Vec<(String, usize, bool, bool)> = self
            .proj
            .ytyps
            .iter()
            .enumerate()
            .flat_map(|(ti, t)| t.mlos.iter().map(move |m| (ti, m)))
            .map(|(ti, m)| (m.name.clone(), m.rooms.len().saturating_sub(1), self.proj.mlo_placement(&m.name).is_some(), self.mlo_ytyp_imported(ti)))
            .collect();
        egui::Window::new(RichText::new(look::label(ic::CUBE_TRANSPARENT, "Studio MLO")).strong())
            .id(egui::Id::new("studio_choose"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -40.0))
            .show(ctx, |ui| {
                ui.label(RichText::new("Un espace à part pour construire un intérieur FiveM de A à Z : rien d'autre à l'écran, les outils de construction à gauche de la vue, les pièces et réglages dans le panneau de gauche, l'ambiance, les meubles et l'objet choisi à droite. Tout reste dans ton projet et s'exporte comme d'habitude.").weak());
                ui.add_space(6.0);
                look::heading(ui, ic::PLUS_CIRCLE, "Nouvel intérieur");
                ui.horizontal(|ui| {
                    ui.label("Nom");
                    ui.add(egui::TextEdit::singleline(&mut self.studio.new_name).hint_text("ex. mon_garage (vide : choisi par DragoWalker)").desired_width(260.0));
                });
                ui.horizontal(|ui| {
                    for (k, icon, n, tip) in [
                        (0u8, ic::SQUARE_HALF, "Vide", "Rien que le dehors : tu dessines toutes les pièces avec l'outil « Pièce » ou « Mur »"),
                        (1, ic::SQUARE, "Une pièce", "Une pièce de 6 × 5 m avec une porte d'entrée"),
                        (2, ic::COLUMNS, "Deux pièces", "Deux pièces de 6 × 5 m reliées par une porte, avec une porte d'entrée"),
                    ] {
                        if look::tool(ui, self.studio.new_kind == k, icon, n, tip).clicked() {
                            self.studio.new_kind = k;
                        }
                    }
                });
                ui.label(RichText::new("Créé au centre de la vue actuelle ; « Placer sur la carte » le déplace ensuite où tu veux.").small().weak());
                if look::accent(ui, ic::CUBE_TRANSPARENT, "Créer et ouvrir dans le studio", "").clicked() {
                    new = true;
                }
                ui.add_space(8.0);
                look::heading(ui, ic::FOLDER_OPEN, "Ouvrir un intérieur du projet");
                if list.is_empty() {
                    ui.label(RichText::new("Aucun intérieur dans le projet pour l'instant.").weak());
                }
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for (name, rooms, placed, imported) in &list {
                        ui.horizontal(|ui| {
                            let r = ui.add_enabled(*placed, egui::Button::new(look::label(ic::HOUSE, name)).min_size(egui::vec2(220.0, 0.0)));
                            if r.clicked() {
                                enter = Some(name.clone());
                            }
                            let mut info = format!("{rooms} pièce(s)");
                            if !placed {
                                info.push_str(" · pas placé");
                            }
                            if *imported {
                                info.push_str(" · MLO importé");
                            }
                            if houses.contains(&name.to_lowercase()) {
                                info.push_str(" · bâtiment de l'atelier");
                            }
                            ui.label(RichText::new(info).small().weak());
                        });
                    }
                });
            });
        if !open {
            self.studio.choose = false;
        }
        if new {
            self.studio.choose = false;
            self.studio_new();
        } else if let Some(n) = enter {
            self.studio_enter(&n);
        }
    }

    /// Barre du haut du studio (à la place de la barre d'outils habituelle).
    pub fn studio_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            egui::Frame::new().fill(Color32::from_rgb(28, 74, 96)).corner_radius(egui::CornerRadius::same(7)).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(ic::CUBE_TRANSPARENT).size(17.0).color(Color32::from_rgb(120, 220, 255)));
                    ui.label(RichText::new("STUDIO MLO").strong().color(Color32::from_rgb(120, 220, 255)));
                    ui.label(RichText::new(&self.studio.mlo).strong().color(Color32::WHITE));
                });
            });
            look::group(ui, |ui| {
                if look::tool(ui, self.plan.mode == 0 && self.tool == Tool::Select, ic::CURSOR, "", "Sélection : choisir et déplacer les objets de l'intérieur (1)").clicked() {
                    self.plan.mode = 0;
                    self.plan.pts.clear();
                    self.tool = Tool::Select;
                }
                if look::tool(ui, self.plan.mode == 0 && self.tool == Tool::Place, ic::PLUS_CIRCLE, "", "Poser le prop choisi dans la bibliothèque (2)").clicked() {
                    self.plan.mode = 0;
                    self.tool = Tool::Place;
                }
            });
            look::group(ui, |ui| {
                ui.label(RichText::new(ic::STACK).size(15.0)).on_hover_text("Étage dessiné");
                ui.add(egui::DragValue::new(&mut self.plan.floor).range(0..=30).prefix("étage "));
            });
            look::group(ui, |ui| {
                for (mode, icon, tip) in [
                    (crate::gizmo::GizmoMode::Move, ic::ARROWS_OUT_CARDINAL, "Déplacer (G)"),
                    (crate::gizmo::GizmoMode::Rotate, ic::ARROW_CLOCKWISE, "Tourner (R)"),
                    (crate::gizmo::GizmoMode::Scale, ic::RESIZE, "Agrandir (T)"),
                ] {
                    if look::tool(ui, self.gizmo_mode == mode && self.plan.mode == 0, icon, "", tip).clicked() {
                        self.gizmo_mode = mode;
                        self.plan.mode = 0;
                        self.tool = Tool::Select;
                    }
                }
                if look::tool(ui, self.snap, ic::MAGNET, "", "Aimanter les positions").clicked() {
                    self.snap = !self.snap;
                }
                if look::action(ui, !self.undo.is_empty(), ic::ARROW_COUNTER_CLOCKWISE, "", "Annuler (Ctrl+Z)").clicked() {
                    self.do_undo();
                }
                if look::action(ui, !self.redo.is_empty(), ic::ARROW_CLOCKWISE, "", "Rétablir (Ctrl+Y)").clicked() {
                    self.do_redo();
                }
            });
            look::group(ui, |ui| {
                if look::tool(ui, !self.cam.map_view, ic::CUBE, "", "Vue 3D (M)").clicked() {
                    self.cam.map_view = false;
                }
                if look::tool(ui, self.cam.map_view, ic::MAP_TRIFOLD, "", "Vue de dessus, comme un plan (M)").clicked() {
                    self.cam.map_view = true;
                }
                if look::tool(ui, self.studio.cut, ic::KNIFE, "", "Coupe : enlever ce qui est au-dessus de l'étage (plafonds) pour voir dans les pièces").clicked() {
                    self.studio.cut = !self.studio.cut;
                }
                if look::tool(ui, self.opts.show_rooms, ic::BOUNDING_BOX, "", "Montrer les boîtes des pièces et les portails").clicked() {
                    self.opts.show_rooms = !self.opts.show_rooms;
                }
                if look::tool(ui, self.opts.show_grid, ic::GRID_FOUR, "", "Grille de 1 m au sol de l'étage").clicked() {
                    self.opts.show_grid = !self.opts.show_grid;
                }
                if look::action(ui, true, ic::CROSSHAIR, "", "Recadrer la vue sur l'intérieur").clicked() {
                    if let Some((ti, mi)) = self.cur_mlo {
                        if let Some((c, d)) = self.studio_frame(ti, mi) {
                            self.cam.target = c;
                            self.cam.dist = d;
                            self.cam.ortho = d;
                        }
                    }
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(look::label(ic::SIGN_OUT, "Quitter")).on_hover_text("Revenir à la carte, là où tu étais").clicked() {
                    self.studio_exit(false);
                }
                if ui.button(look::label(ic::MAP_PIN, "Placer sur la carte")).on_hover_text("Revenir à la carte avec l'intérieur choisi et les poignées de déplacement, pour le poser où tu veux").clicked() {
                    self.studio_exit(true);
                }
                if ui.button(look::label(ic::EXPORT, "Exporter")).on_hover_text("Exporter le projet en ressource FiveM (Ctrl+Maj+E)").clicked() {
                    self.export_xml(true);
                }
            });
        });
    }

    /// Panneaux du studio : à gauche l'intérieur (pièces, portails, variantes, réglages du plan), à droite la
    /// pièce choisie (ambiance, meubles, portails) et l'objet choisi.
    pub fn studio_panels(&mut self, ctx: &egui::Context) {
        if !self.studio.on {
            return;
        }
        egui::SidePanel::left("studio_left").resizable(true).default_width(330.0).min_width(260.0).show(ctx, |ui| {
            ui.add_space(4.0);
            look::heading(ui, ic::HOUSE, "Intérieur");
            ui.separator();
            egui::ScrollArea::vertical().id_salt("studio_left_scroll").auto_shrink([false; 2]).show(ui, |ui| {
                self.mlo_editor(ui);
            });
        });
        // étagère d'outils du plan, entre le panneau de gauche et la vue
        egui::SidePanel::left("studio_shelf").resizable(false).exact_width(128.0).frame(egui::Frame::new().fill(Color32::from_rgb(29, 31, 37)).inner_margin(egui::Margin::symmetric(6, 8))).show(ctx, |ui| {
            ui.label(RichText::new("CONSTRUIRE").small().strong().color(Color32::from_rgb(150, 156, 170)));
            ui.add_space(2.0);
            for (k, (m, icon, n, tip)) in PLAN_TOOLS.iter().enumerate() {
                if k == 6 {
                    ui.add_space(4.0);
                    ui.label(RichText::new("AMÉNAGER").small().strong().color(Color32::from_rgb(150, 156, 170)));
                    ui.add_space(2.0);
                }
                let on = self.plan.mode == *m;
                let r = ui.add(egui::Button::new(RichText::new(look::label(icon, n)).size(14.0)).selected(on).min_size(egui::vec2(116.0, 26.0))).on_hover_text(*tip);
                if r.clicked() {
                    self.plan.mode = if on { 0 } else { *m };
                    self.plan.pts.clear();
                    self.plan.drag = None;
                    self.tool = Tool::Select;
                }
            }
            ui.add_space(4.0);
            ui.label(RichText::new("OBJETS").small().strong().color(Color32::from_rgb(150, 156, 170)));
            ui.add_space(2.0);
            if ui.add(egui::Button::new(RichText::new(look::label(ic::PACKAGE, "Props")).size(14.0)).selected(self.library.open).min_size(egui::vec2(116.0, 26.0))).on_hover_text("Bibliothèque des props du jeu : choisis-en un puis pose-le avec l'outil Poser (B)").clicked() {
                self.library.open = !self.library.open;
            }
            if ui.add(egui::Button::new(RichText::new(look::label(ic::PLUS_CIRCLE, "Poser")).size(14.0)).selected(self.plan.mode == 0 && self.tool == Tool::Place).min_size(egui::vec2(116.0, 26.0))).on_hover_text("Poser le prop choisi (2)").clicked() {
                self.plan.mode = 0;
                self.tool = Tool::Place;
            }
            if self.plan.mode != 0 {
                ui.add_space(8.0);
                ui.label(RichText::new("Échap : finir le tracé, puis quitter l'outil. Réglages de l'outil : panneau de gauche.").small().weak());
            }
        });
        egui::SidePanel::right("studio_right").resizable(true).default_width(330.0).min_width(260.0).show(ctx, |ui| {
            ui.add_space(4.0);
            egui::ScrollArea::vertical().id_salt("studio_right_scroll").auto_shrink([false; 2]).show(ui, |ui| {
                look::heading(ui, ic::COUCH, "Pièce choisie : ambiance et meubles");
                let before = ui.min_rect().height();
                self.interior_room_tools(ui);
                if (ui.min_rect().height() - before).abs() < 1.0 {
                    ui.label(RichText::new("Dessine d'abord une pièce (outil « Pièce », à gauche de la vue).").weak());
                }
                ui.add_space(10.0);
                look::heading(ui, ic::CURSOR_CLICK, "Objet choisi");
                if self.selected.is_empty() {
                    ui.label(RichText::new("Clique un objet de l'intérieur pour le régler ici.").weak());
                } else {
                    self.selection_tab(ui);
                }
            });
        });
    }
}
