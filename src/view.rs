//! Vue 3D / carte : caméra, outils à la souris, raccourcis clavier.

use crate::app::{Drag, DwApp, Tab, Tool};
use crate::math::{v3, V3};
use crate::mech;
use crate::model::{Entity, ItemRef};
use crate::render;
use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, PointerButton, Pos2, Rect, Sense, Stroke};
use std::collections::HashSet;

impl DwApp {
    pub fn pick(&self, s: Pos2) -> Option<u64> {
        if self.game.world.is_some() {
            return self.under_cursor(s).into_iter().find_map(|u| if let crate::game_ui::Under::Proj(id) = u { Some(id) } else { None });
        }
        let (o, d) = self.cam.ray(s);
        let mut best: Option<(f32, u64, f32)> = None;
        for it in self.proj.all_items() {
            if self.sample_hidden(it) {
                continue;
            }
            let Some(e) = self.proj.world_entity(it) else { continue };
            if let Some(t) = render::ray_hit(&e, &self.proj, o, d) {
                let (mn, mx) = e.local_box(&self.proj);
                let size = (mx - mn).len();
                // un placement d'intérieur ne passe jamais devant ses objets
                let score = t + if e.mlo.is_some() { 1e6 } else { 0.0 };
                let better = match best {
                    None => true,
                    Some(b) => score < b.0 - 0.5 || ((score - b.0).abs() <= 0.5 && size < b.2),
                };
                if better {
                    best = Some((score, e.id, size));
                }
            }
        }
        best.map(|b| b.1)
    }

    fn erase_at(&mut self, g: V3) {
        let f = self.erase_filter.clone();
        let ids: HashSet<u64> = self.items_in_radius(g, self.erase_radius, &f, self.erase_buildings).into_iter().collect();
        let mut n = 0;
        if !ids.is_empty() {
            n += self.proj.delete_ids(&ids);
        }
        n += self.game_erase_at(g);
        if n > 0 {
            self.clean_selection();
            self.status = format!("Gomme : {n} objet(s) supprimé(s).");
        }
    }

    pub fn view(&mut self, ui: &mut egui::Ui, dt: f32) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.cam.rect = rect;
        let ctx = ui.ctx().clone();
        let hover_pos = resp.hover_pos();
        // mode test : la souris est simulée au centre de la vue
        let hover_pos = hover_pos.or_else(|| std::env::var("DW_TEST_PROBE").or_else(|_| std::env::var("DW_TEST_CLICKS")).or_else(|_| std::env::var("DW_TEST_ANALYSE")).ok().map(|_| crate::app::test_point(rect)));
        let mods = ctx.input(|i| i.modifiers);

        // caméra : clic droit tourne (vue carte : déplace), clic molette déplace, molette zoome
        if resp.dragged_by(PointerButton::Secondary) {
            let d = resp.drag_delta();
            if self.cam.map_view {
                let mpp = self.cam.meters_per_pixel();
                self.cam.target = self.cam.target + v3(-d.x * mpp, d.y * mpp, 0.0);
            } else {
                self.cam.mouse_rotate(d.x, d.y);
            }
        }
        if resp.dragged_by(PointerButton::Middle) {
            let d = resp.drag_delta();
            let mpp = self.cam.meters_per_pixel();
            let (f, r) = self.cam.flat_axes();
            self.cam.target = self.cam.target - r * (d.x * mpp) + f * (d.y * mpp);
        }
        if resp.hovered() {
            let scroll = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                if self.group_place.is_some() {
                    // pose d'un groupe : la molette le tourne (groups.rs)
                } else if self.tool == Tool::Erase && mods.alt {
                    self.erase_radius = (self.erase_radius * if scroll > 0.0 { 1.1 } else { 0.9 }).clamp(1.0, 500.0);
                } else if mods.alt && self.game.cut_on {
                    // la molette + Alt règle la hauteur de coupe (plus bas)
                } else {
                    // vers le point sous la souris (et au-delà, tout près)
                    let at = self.mouse_ground;
                    self.cam.zoom_at(scroll.signum(), at);
                }
            }
        }

        // déplacement clavier (WorldForm::UpdateControlInputs)
        if !ctx.wants_keyboard_input() {
            let free_arrows = self.selected.is_empty();
            let keys = ctx.input(|i| {
                [
                    i.key_down(Key::W) || (i.key_down(Key::Z) && !i.modifiers.command) || (free_arrows && i.key_down(Key::ArrowUp)),
                    i.key_down(Key::S) && !i.modifiers.command || (free_arrows && i.key_down(Key::ArrowDown)),
                    i.key_down(Key::A) && !i.modifiers.command || i.key_down(Key::Q) || (free_arrows && i.key_down(Key::ArrowLeft)),
                    (i.key_down(Key::D) && !i.modifiers.command) || (free_arrows && i.key_down(Key::ArrowRight)),
                    i.key_down(Key::PageUp),
                    i.key_down(Key::PageDown),
                ]
            });
            let (f, r) = self.cam.flat_axes();
            let dirs = [f, -f, -r, r, v3(0.0, 0.0, 1.0), v3(0.0, 0.0, -1.0)];
            let mut mv = V3::ZERO;
            for (k, d) in keys.iter().zip(dirs.iter()) {
                if *k {
                    mv = mv + *d;
                }
            }
            if mv.len() > 0.0 {
                let mut speed = mech::BASE_MOVE_SPEED * self.speed_mult;
                if mods.shift {
                    speed *= mech::SHIFT_MULT;
                }
                if mods.ctrl {
                    speed *= mech::CTRL_MULT;
                }
                self.cam.target = self.cam.target + mv.norm() * (dt * speed * self.cam.move_scale());
            }
        }
        self.cam.update(dt);

        // survol
        // sol sous la souris : la vraie surface visible (profondeur lue dans la carte graphique), sinon le relief
        let surface = match (hover_pos, self.game.real_models, &self.game.gpu) {
            (Some(p), true, Some(g)) => g.lock().ok().and_then(|g| g.probe).filter(|(pp, _)| (pp[0] - p.x).abs() < 2.0 && (pp[1] - p.y).abs() < 2.0).and_then(|(_, d)| self.cam.unproject_depth(p, d)),
            _ => None,
        };
        self.game.surface = surface;
        // pendant la pose d'un groupe, ses modèles sont dessinés sous la souris : la profondeur lue toucherait le
        // groupe lui-même, qui reculerait vers la caméra à chaque image ; on prend alors le sol du jeu (relief)
        let surface = if self.group_place.is_some() { None } else { surface };
        if !ctx.wants_keyboard_input() && self.game.world.is_some() {
            let (kc, kh, alt_scroll) = ctx.input(|i| (i.key_pressed(Key::C) && !i.modifiers.command, i.key_pressed(Key::H) && !i.modifiers.command, if i.modifiers.alt { i.raw_scroll_delta.y } else { 0.0 }));
            if kc {
                self.game.cut_on = !self.game.cut_on;
                self.status = if self.game.cut_on { format!("Coupe activée à {:.1} m au-dessus du centre de la vue (molette + Alt pour la monter ou la descendre).", self.game.cut_h) } else { "Coupe désactivée.".into() };
            }
            if alt_scroll != 0.0 && self.game.cut_on && hover_pos.is_some() {
                self.game.cut_h = (self.game.cut_h + alt_scroll.signum() * 0.5).clamp(-10.0, 60.0);
                self.status = format!("Coupe à {:.1} m.", self.game.cut_h);
            }
            if kh && self.hover.is_some() {
                let id = self.hover.unwrap_or(0);
                self.game.hidden_ids.insert(id);
                let n = self.proj.find_id(id).and_then(|r| self.proj.ent(r)).map(|e| e.archetype.clone()).unwrap_or_default();
                self.selected.retain(|x| *x != id);
                self.sel_set.remove(&id);
                self.status = format!("« {n} » caché (pas supprimé). Onglet Jeu : « Tout réafficher ».");
                self.hover = None;
            } else if kh {
                if let Some(ei) = self.game.hover {
                    self.game.hidden.insert(ei);
                    let n = self.game.world.as_ref().map(|w| w.name_of(w.ents[ei as usize].arch)).unwrap_or_default();
                    self.status = format!("« {n} » caché (pas supprimé). Onglet Jeu : « Tout réafficher ».");
                    self.game.hover = None;
                }
            }
        }
        self.mouse_ground = hover_pos.and_then(|p| if self.proj.terrain.is_some() && (self.game.empty_map || surface.is_none()) { surface.or_else(|| self.terrain_ground(p)).or_else(|| self.cam.ground_point(p, 0.0)) } else if self.game.world.is_some() { surface.or_else(|| self.game_ground(p)).or_else(|| self.cam.ground_point(p, self.ground_z)) } else { self.cam.ground_point(p, self.ground_z) });
        // pose d'un groupe : le relief du jeu est grossier en ville ; comme CodeWalker prend le point touché sur les
        // collisions du monde (WorldForm::GetSpaceMouseRay 0x060002a5 → Space::RayIntersect), on recale la hauteur
        // sur la vraie surface (rue, trottoir) juste au-dessus ou au-dessous du relief, sans monter sur les toits
        if self.group_place.is_some() {
            if let Some(g) = self.mouse_ground {
                let up = crate::groups::GROUP_STREET_PROBE;
                if let Some(z) = self.ground_below(g.x, g.y, g.z + up, up * 2.0, &HashSet::new()) {
                    self.mouse_ground = Some(v3(g.x, g.y, z));
                }
            }
        }
        if self.game.world.is_some() {
            // survol : exactement l'objet qu'un clic prendrait (y compris celui de derrière après un reclic)
            if self.frame % 2 == 0 {
                // pas de survol quand la souris ne choisit pas (pose d'un objet ou d'un groupe), comme CodeWalker qui
                // ne garde la liste des objets dessinés que si la sélection à la souris est active
                // (WorldForm::BeginMouseHitTest 0x060002a4 : RenderedDrawablesListEnable)
                let u = if self.tool != Tool::Place && self.group_place.is_none() { hover_pos.and_then(|p| self.hover_under(p, !(mods.command || mods.shift))) } else { None };
                self.hover = match u {
                    Some(crate::game_ui::Under::Proj(id)) if self.tool == Tool::Select => Some(id),
                    _ => None,
                };
                self.game.hover = match u {
                    Some(crate::game_ui::Under::Game(ei)) => Some(ei),
                    _ => None,
                };
            } else if self.tool != Tool::Select {
                self.hover = None;
            }
        } else {
            self.hover = if self.tool == Tool::Select && self.frame % 2 == 0 { hover_pos.and_then(|p| self.pick(p)) } else if self.tool == Tool::Select { self.hover } else { None };
        }

        self.gizmo_hover = if self.tool == Tool::Select && !matches!(self.drag, Drag::Gizmo(_)) { hover_pos.and_then(|p| self.gizmo_pick(p)) } else { None };
        if self.gizmo_hover.is_some() {
            self.hover = None;
            self.game.hover = None;
        }
        let ptr = resp.interact_pointer_pos().or(hover_pos);
        let press_origin = ctx.input(|i| i.pointer.press_origin());
        // édition d'un bâtiment posé : la souris retire des morceaux, à la place de l'outil en cours
        let editing = self.bedit.is_some();
        if editing {
            self.bedit_input(&resp, ptr, press_origin, mods);
        }
        // pose d'un groupe : la souris le pose, la molette le tourne
        let grouping = !editing && self.group_input(&ctx, &resp);
        match self.tool {
            _ if editing || grouping => {}
            Tool::Select | Tool::Rect => self.select_tool(&resp, ptr, press_origin, mods),
            Tool::Terrain => self.terrain_tool(&resp, ptr, &ctx),
            Tool::Life => self.life_tool(&resp, ptr),
            Tool::Place => {
                if resp.drag_started_by(PointerButton::Primary) {
                    let start = press_origin.or(ptr).unwrap_or_default();
                    if let Some(g) = self.cam.ground_point(start, self.ground_z) {
                        self.push_undo();
                        let arch = self.place_arch.clone();
                        let id = self.place_prop(&arch, g, None);
                        self.set_selection(vec![id]);
                        self.snap_if_shift(&[id]);
                        self.drag = Drag::Place(g);
                    }
                }
                if resp.dragged_by(PointerButton::Primary) {
                    if let (Drag::Place(last), Some(g)) = (&self.drag, self.mouse_ground) {
                        let last = *last;
                        let d = g - last;
                        if d.len() >= self.stroke_spacing {
                            // les props suivent la direction du trait
                            let heading = if self.rand_rot { None } else { Some(((-d.x).atan2(d.y).to_degrees() + 90.0).rem_euclid(360.0)) };
                            let arch = self.place_arch.clone();
                            let p = last + d.norm() * self.stroke_spacing;
                            let id = self.place_prop(&arch, p, heading);
                            self.add_selection([id]);
                            self.snap_if_shift(&[id]);
                            self.drag = Drag::Place(p);
                        }
                    }
                }
                if resp.drag_stopped() {
                    if matches!(self.drag, Drag::Place(_)) {
                        self.status = format!("{} prop(s) posé(s) le long du trait.", self.selected.len());
                    }
                    self.drag = Drag::None;
                }
                if resp.clicked_by(PointerButton::Primary) {
                    if let Some(g) = self.mouse_ground {
                        self.push_undo();
                        let arch = self.place_arch.clone();
                        let id = self.place_prop(&arch, g, None);
                        self.set_selection(vec![id]);
                        self.status = format!("« {arch} » posé.");
                    }
                }
            }
            Tool::Erase => {
                if resp.drag_started_by(PointerButton::Primary) || resp.clicked_by(PointerButton::Primary) {
                    self.push_undo();
                    self.drag = Drag::Erase;
                    if let Some(g) = self.mouse_ground {
                        self.erase_at(g);
                    }
                }
                if resp.dragged_by(PointerButton::Primary) {
                    if let Some(g) = self.mouse_ground {
                        self.erase_at(g);
                    }
                }
                if resp.drag_stopped() {
                    self.drag = Drag::None;
                }
            }
        }

        // dessin
        let painter = ui.painter_at(rect);
        let mut drawn = std::mem::take(&mut self.game.drawn);
        let gpu = if self.game.real_models { self.game.gpu.clone().filter(|g| g.lock().map(|g| g.error.is_none()).unwrap_or(false)) } else { None };
        if let Some(g) = &gpu {
            if g.lock().map(|g| g.busy()).unwrap_or(false) {
                ctx.request_repaint();
            }
        }
        // aperçu de l'export d'un MLO (fenêtre « Pièces du MLO ») : ses façades là où elles seront posées
        let mut mlo_extra = self.mlo_preview_insts();
        // groupe en cours de pose, sous la souris
        mlo_extra.extend(self.group_ghost());
        let gctx = self.game.world.as_ref().map(|w| render::GameCtx { world: w, lod_scale: self.game.lod, hover: self.game.hover, max_draw: self.game.max_draw, show_terrain: self.game.terrain, gpu: gpu.clone(), probe: hover_pos, shadows: self.game.shadows, cut_z: self.cut_z(), hidden: &self.game.hidden, hidden_ids: &self.game.hidden_ids, interiors: self.game.interiors, empty_map: self.game.empty_map, hidden_lod: &self.game.hidden_lod, isolate: self.game.isolate, only: self.isolate_sel.as_ref(), terrain: self.proj.terrain.as_ref().map(|t| { let (mn, mx) = t.bounds(); (crate::gta::crypto::joaat(crate::terrain::TERRAIN_ARCH), t.origin, mn, mx) }), waters: self.proj.terrain.as_ref().map(|t| t.waters.iter().map(|w| [w.min[0], w.min[1], w.max[0], w.max[1], w.z]).collect()).unwrap_or_default(), extra: mlo_extra });
        render::draw_scene(&painter, &self.cam, &self.proj, &self.opts, &self.sel_set, self.hover, self.cur_mlo, gctx.as_ref(), &mut drawn);
        drop(gctx);
        self.game.drawn = drawn;
        // analyse d'un bâtiment : triangles en couleurs par-dessus la vue
        if let Some(a) = self.game.analysis.as_ref().filter(|a| a.show) {
            let cut = self.cut_z();
            let mut mesh = egui::Mesh::default();
            for (t, k) in &a.tris {
                let Some(c) = crate::analyse::kind_color(*k) else { continue };
                if let Some(cz) = cut {
                    if t[0].z > cz && t[1].z > cz && t[2].z > cz {
                        continue;
                    }
                }
                let (Some((p0, _)), Some((p1, _)), Some((p2, _))) = (self.cam.project(t[0]), self.cam.project(t[1]), self.cam.project(t[2])) else { continue };
                let col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                let base = mesh.vertices.len() as u32;
                for p in [p0, p1, p2] {
                    mesh.colored_vertex(p, col);
                }
                mesh.add_triangle(base, base + 1, base + 2);
                if mesh.vertices.len() > 60000 {
                    painter.add(egui::Shape::mesh(std::mem::take(&mut mesh)));
                }
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        if self.tool == Tool::Select {
            self.draw_gizmo(&painter, self.gizmo_hover);
        }
        self.draw_lists_overlay(&painter);
        self.mlo_pieces_view(&painter, hover_pos, ui.ctx());
        self.bedit_draw(&painter, hover_pos, ui.ctx());
        // aperçu de l'herbe du jeu : retiré avec le pinceau (voir A_FAIRE.md)
        self.draw_occluders(&painter);
        self.draw_perf(&painter, rect);
        self.draw_group_ghost(&painter);
        self.draw_scenery_overlay(&painter, hover_pos);
        self.draw_life_overlay(&painter, hover_pos);
        // Ctrl enfoncé : montre le point exact que la souris touche (WorldForm::BeginMouseHitTest 0x060002a4,
        // MouseRayCollisionVisible quand CtrlPressed) — Ctrl + clic y prend l'objet
        if (mods.command || std::env::var("DW_TEST_CTRL").is_ok()) && self.tool == Tool::Select {
            if let (Some(g), Some(hp)) = (self.mouse_ground, hover_pos) {
                if self.cam.project(g).is_some() {
                    let col = Color32::from_rgb(255, 200, 60);
                    painter.circle_stroke(hp, 7.0, Stroke::new(2.0_f32, col));
                    painter.line_segment([hp - egui::vec2(12.0, 0.0), hp + egui::vec2(12.0, 0.0)], Stroke::new(1.5_f32, col));
                    painter.line_segment([hp - egui::vec2(0.0, 12.0), hp + egui::vec2(0.0, 12.0)], Stroke::new(1.5_f32, col));
                    painter.text(hp + egui::vec2(14.0, -18.0), egui::Align2::LEFT_BOTTOM, format!("Ctrl + clic : ajouter · {:.1} {:.1} {:.1}", g.x, g.y, g.z), egui::FontId::proportional(13.0), col);
                }
            }
        }
        match self.tool {
            Tool::Place => {
                if let Some(g) = self.mouse_ground {
                    let mut e = Entity::new(&self.place_arch, self.snap_pos(g));
                    e.id = 0;
                    render::draw_ghost(&painter, &self.cam, &self.proj, &e);
                    if self.place_into_mlo && self.mlo_at(g).is_some() {
                        if let Some(p) = hover_pos {
                            painter.text(p + egui::vec2(14.0, 14.0), egui::Align2::LEFT_TOP, "» dans l'intérieur", egui::FontId::proportional(13.0), Color32::from_rgb(80, 220, 255));
                        }
                    }
                }
            }
            Tool::Erase => {
                if let Some(g) = self.mouse_ground {
                    render::draw_ground_circle(&painter, &self.cam, g, self.erase_radius, Stroke::new(2.0_f32, Color32::from_rgb(255, 90, 70)));
                }
            }
            Tool::Select | Tool::Rect => {}
            Tool::Terrain => self.draw_terrain_brush(&painter, hover_pos),
            Tool::Life => {
                // parking en cours de tracé
                if let (Some(a), Some(g)) = (self.life.drag_start, self.mouse_ground) {
                    if let (Some((pa, _)), Some((pb, _))) = (self.cam.project(a), self.cam.project(g)) {
                        painter.line_segment([pa, pb], Stroke::new(2.0_f32, Color32::from_rgb(255, 210, 60)));
                    }
                }
            }
        }
        if let (Drag::Rect(a), Some(b)) = (&self.drag, ptr) {
            let r = Rect::from_two_pos(*a, b);
            painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(255, 200, 60, 30));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(255, 200, 60)), egui::StrokeKind::Inside);
        }
        // objet du jeu survolé
        if let (None, Some(t), Some(p)) = (self.hover, self.game_hover_text(), hover_pos) {
            let galley = painter.layout_no_wrap(format!("{t}\nclic : le copier dans le projet pour le modifier"), egui::FontId::proportional(13.0), Color32::WHITE);
            let r = Rect::from_min_size(p + egui::vec2(14.0, 4.0), galley.size() + egui::vec2(8.0, 4.0));
            painter.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(0, 0, 0, 170));
            painter.galley(r.min + egui::vec2(4.0, 2.0), galley, Color32::WHITE);
        }
        // nom de l'objet survolé
        if let (Some(id), Some(p)) = (self.hover, hover_pos) {
            if let Some(e) = self.proj.find_id(id).and_then(|r| self.proj.ent(r)) {
                let galley = painter.layout_no_wrap(e.archetype.clone(), egui::FontId::proportional(13.0), Color32::WHITE);
                let r = Rect::from_min_size(p + egui::vec2(14.0, 4.0), galley.size() + egui::vec2(8.0, 4.0));
                painter.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(0, 0, 0, 170));
                painter.galley(r.min + egui::vec2(4.0, 2.0), galley, Color32::WHITE);
            }
        }
        // boussole
        let c = rect.right_top() + egui::vec2(-36.0, 36.0);
        painter.circle_filled(c, 22.0, Color32::from_rgba_unmultiplied(0, 0, 0, 120));
        let ang = if self.cam.map_view { 0.0 } else { self.cam.cur_yaw };
        let n = egui::vec2(-ang.sin(), -ang.cos());
        painter.line_segment([c, c + n * 17.0], Stroke::new(3.0_f32, Color32::from_rgb(230, 70, 60)));
        painter.line_segment([c, c - n * 12.0], Stroke::new(2.0_f32, Color32::from_gray(200)));
        painter.text(c + n * 29.0, egui::Align2::CENTER_CENTER, "N", egui::FontId::proportional(12.0), Color32::WHITE);
        // aide
        let help = match self.tool {
            Tool::Select => "Sélection : clic = choisir (reclic = celui de derrière) · glisser = déplacer ou rectangle · H = cacher · C = couper le haut · Suppr = supprimer",
            Tool::Place => "Poser : clic = poser · glisser = poser le long d'un trait",
            Tool::Erase => "Gomme : glisser pour effacer · Alt + molette = taille du cercle",
            Tool::Terrain => match self.terrain.brush {
                crate::terrain::Brush::Road => "Route : clic pour poser des points · double-clic ou Entrée pour construire · Retour arrière / Échap",
                crate::terrain::Brush::Vegetation => "Végétation : glisser pour semer · Ctrl + glisser pour retirer · + / - rayon",
                crate::terrain::Brush::Paint => "Peindre : glisser pour changer le sol · + / - rayon · F2–F9 autres pinceaux",
                _ => "Terrain : glisser pour sculpter · + / - rayon · F2–F9 pinceaux (menu Terrain)",
            },
            Tool::Rect => "Rectangle : glisser n'importe où pour sélectionner tout ce qui est dedans · Ctrl ou Maj = ajouter · Suppr = tout supprimer",
            Tool::Life => match self.life.tool {
                crate::life::LifeTool::Car => "Véhicule garé : clic pour en poser un (capot vers l'avant de la caméra)",
                crate::life::LifeTool::Parking => "Parking : glisser pour tracer une rangée de places",
                crate::life::LifeTool::Ped => "PNJ : clic pour en poser un (face à la caméra)",
                crate::life::LifeTool::Route => "Ronde : clic pour les points · double-clic ou Entrée pour créer le PNJ · Échap pour annuler",
            },
        };
        let galley = painter.layout_no_wrap(help.to_string(), egui::FontId::proportional(13.0), Color32::WHITE);
        let pos = rect.left_bottom() + egui::vec2(10.0, -10.0 - galley.size().y);
        painter.rect_filled(Rect::from_min_size(pos - egui::vec2(5.0, 3.0), galley.size() + egui::vec2(10.0, 6.0)), 4.0, Color32::from_rgba_unmultiplied(0, 0, 0, 130));
        painter.galley(pos, galley, Color32::WHITE);
    }

    /// Sélectionne tout ce qui est dans un rectangle de l'écran (objets du projet et du jeu ; ce qui est caché
    /// avec H ou au-dessus de la coupe est ignoré). `add` : ajouter à la sélection.
    pub fn rect_select(&mut self, r: Rect, add: bool) -> usize {
        let cut = self.cut_z();
        let mut ids = Vec::new();
        for it in self.proj.all_items() {
            if self.sample_hidden(it) {
                continue;
            }
            if let Some(e) = self.proj.world_entity(it) {
                if self.game.hidden_ids.contains(&e.id) || self.hidden_by_isolation(e.id) || cut.map(|c| e.pos.z > c).unwrap_or(false) {
                    continue;
                }
                if let Some((sp, _)) = self.cam.project(e.pos) {
                    if r.contains(sp) {
                        ids.push(e.id);
                    }
                }
            }
        }
        // vue « Isoler » : pas d'objets du jeu (non dessinés)
        let game_ents: Vec<u32> = self.game_in_rect(r).into_iter().filter(|_| self.isolate_sel.is_none()).filter(|ei| self.game.world.as_ref().map(|w| cut.map(|c| w.ents[*ei as usize].pos.z <= c).unwrap_or(true)).unwrap_or(true)).collect();
        if !game_ents.is_empty() {
            self.push_undo();
            ids.extend(self.game_ents_to_project(&game_ents));
        }
        let n = ids.len();
        if add {
            self.add_selection(ids);
        } else {
            self.set_selection(ids);
        }
        self.status = format!("{n} objet(s) pris dans le rectangle ({} en tout). Suppr pour tout supprimer d'un coup.", self.selected.len());
        if n > 0 {
            self.tab = Tab::Selection;
        }
        n
    }

    fn select_tool(&mut self, resp: &egui::Response, ptr: Option<Pos2>, press_origin: Option<Pos2>, mods: Modifiers) {
        // outil Rectangle, ou Alt + glisser : toujours un rectangle de sélection
        if resp.drag_started_by(PointerButton::Primary) && (self.tool == Tool::Rect || mods.alt) {
            self.drag = Drag::Rect(press_origin.or(ptr).unwrap_or_default());
        } else if resp.drag_started_by(PointerButton::Primary) && self.gizmo_begin(press_origin.or(ptr).unwrap_or_default()) {
            // glisser d'une poignée : géré plus bas
        } else if resp.drag_started_by(PointerButton::Primary) {
            let start = press_origin.or(ptr).unwrap_or_default();
            match self.pick_for_drag(start) {
                Some(id) => {
                    if !self.sel_set.contains(&id) {
                        // un groupe posé se déplace d'un bloc
                        let block = self.group_block(id);
                        if mods.command || mods.shift {
                            self.add_selection(block);
                        } else {
                            self.set_selection(block);
                        }
                    }
                    self.push_undo();
                    let gz = self.proj.find_id(id).and_then(|r| self.proj.world_entity(r)).map(|e| e.pos.z).unwrap_or(self.ground_z);
                    self.drag = match self.cam.ground_point(start, gz) {
                        Some(g) => Drag::Move(g),
                        None => Drag::None,
                    };
                }
                None => self.drag = Drag::Rect(start),
            }
        }
        if resp.dragged_by(PointerButton::Primary) && matches!(self.drag, Drag::Gizmo(_)) {
            if let Some(p) = ptr {
                self.gizmo_update(p);
            }
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let (Drag::Move(last), Some(p)) = (&self.drag, ptr) {
                let last = *last;
                if let Some(g) = self.cam.ground_point(p, last.z) {
                    let mut d = g - last;
                    d.z = 0.0;
                    if self.snap && self.snap_size > 0.0 {
                        let s = self.snap_size;
                        d = v3((d.x / s).round() * s, (d.y / s).round() * s, 0.0);
                    }
                    if d.len() > 0.0 {
                        self.move_selected(d);
                        self.drag = Drag::Move(last + d);
                        // Maj maintenue : l'objet suit le sol
                        if mods.shift {
                            self.snap_selected_to_ground(false);
                        }
                    }
                }
            }
            if mods.shift && matches!(self.drag, Drag::Gizmo(_)) && self.gizmo_mode == crate::gizmo::GizmoMode::Move {
                self.snap_selected_to_ground(false);
            }
        }
        if resp.drag_stopped() {
            if let (Drag::Rect(a), Some(b)) = (&self.drag, ptr) {
                let r = Rect::from_two_pos(*a, b);
                self.rect_select(r, mods.command || mods.shift);
            }
            self.drag = Drag::None;
        }
        // clic gauche (ou clic droit sans glisser, comme dans CodeWalker) : choisir
        let click = resp.clicked_by(PointerButton::Primary) || resp.clicked_by(PointerButton::Secondary);
        // un clic sur une poignée ne change pas la sélection
        let click = click && !ptr.map(|p| self.gizmo_pick(p).is_some()).unwrap_or(false);
        // double-clic sur un objet d'un groupe posé : n'en prendre que cette pièce
        if resp.double_clicked_by(PointerButton::Primary) {
            // la pièce du bloc choisi qui est sous la souris
            if let Some(id) = ptr.and_then(|p| self.pick_for_drag(p)) {
                if self.group_open_piece(id) {
                    return;
                }
            }
        }
        if click {
            if let Some(p) = ptr {
                let add = mods.command || mods.shift;
                match self.pick_any(p, !add) {
                    // objet d'un groupe posé : tout le bloc (sauf s'il est ouvert)
                    Some(id) if self.group_block(id).len() > 1 => {
                        let block = self.group_block(id);
                        let n = block.len();
                        // Ctrl + clic : interrupteur, comme CodeWalker (WorldForm::SelectItem 0x060002b5 avec
                        // addSelection : déjà dans la sélection multiple → retiré, sinon ajouté)
                        if add && block.iter().all(|b| self.sel_set.contains(b)) {
                            let rm: std::collections::HashSet<u64> = block.into_iter().collect();
                            self.selected.retain(|x| !rm.contains(x));
                            self.sel_set.retain(|x| !rm.contains(x));
                            self.status = format!("Groupe posé retiré de la sélection ({n} objet(s)) : {} objet(s) sélectionné(s).", self.selected.len());
                        } else {
                            if add {
                                self.add_selection(block);
                            } else {
                                self.set_selection(block);
                            }
                            self.status = format!("Groupe posé : {n} objet(s) pris d'un bloc (déplace-le ou tourne-le d'un coup) · double-clic pour une seule pièce.");
                        }
                        self.tab = Tab::Selection;
                    }
                    Some(id) => {
                        // clic hors du bloc ouvert : il se referme
                        let inst = self.proj.find_id(id).and_then(|r| self.proj.ent(r)).map(|e| e.group_inst).unwrap_or(0);
                        if self.group_open.is_some() && self.group_open != Some(inst) {
                            self.group_open = None;
                        }
                        if add {
                            // Ctrl + clic : ajoute (ou retire) l'objet sous la souris, comme CodeWalker
                            let name = self.proj.find_id(id).and_then(|r| self.proj.ent(r)).map(|e| e.archetype.clone()).unwrap_or_default();
                            if self.sel_set.contains(&id) {
                                self.selected.retain(|x| *x != id);
                                self.sel_set.remove(&id);
                                self.status = format!("« {name} » retiré de la sélection ({} objet(s) sélectionné(s)).", self.selected.len());
                            } else {
                                self.add_selection([id]);
                                self.status = format!("« {name} » ajouté : {} objet(s) sélectionné(s). Ctrl + clic pour en ajouter d'autres.", self.selected.len());
                            }
                        } else {
                            self.set_selection(vec![id]);
                        }
                        self.tab = Tab::Selection;
                    }
                    None => {
                        if !(mods.command || mods.shift) {
                            self.set_selection(vec![]);
                        }
                    }
                }
            }
        }
        let _ = ItemRef::Ent(0, 0);
    }

    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z))) {
            self.do_undo();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y))) {
            self.do_redo();
        }
        // Ctrl+Maj+S avant Ctrl+S (comme SaveAll / Save dans WorldForm_KeyDown 0x06000332)
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S))) {
            self.save_project(true);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S))) {
            self.save_project(false);
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::D))) {
            { let off = self.copy_offset(); self.duplicate_selected(off); }
        }
        // raccourcis Ctrl de CodeWalker (WorldForm_KeyDown 0x06000332, ProjectForm 0x06000e92)
        let ck = ctx.input_mut(|i| {
            let mut c = |m: Modifiers, k: Key| i.consume_shortcut(&KeyboardShortcut::new(m, k));
            [
                c(Modifiers::COMMAND, Key::N),
                c(Modifiers::COMMAND, Key::O),
                c(Modifiers::COMMAND, Key::C),
                c(Modifiers::COMMAND, Key::V),
                c(Modifiers::COMMAND, Key::X),
                c(Modifiers::COMMAND, Key::U),
                c(Modifiers::COMMAND, Key::I),
                c(Modifiers::COMMAND | Modifiers::SHIFT, Key::E),
                c(Modifiers::COMMAND, Key::E),
            ]
        });
        if ck[0] {
            self.new_project();
        }
        if ck[1] {
            self.open_project_dialog();
        }
        if ck[2] {
            self.copy_selected();
        }
        if ck[3] {
            self.paste();
        }
        if ck[4] && !self.selected.is_empty() {
            self.copy_selected();
            self.delete_selected();
        }
        if ck[5] {
            self.panels_hidden = !self.panels_hidden;
        }
        if ck[6] {
            self.import_dialog();
        }
        if ck[7] {
            self.export_xml(true);
        } else if ck[8] {
            self.export_xml(false);
        }
        // F12 : tester en jeu (serveur local ; s'il tourne déjà, la map est rechargée sans déconnexion)
        if ctx.input(|i| i.key_pressed(Key::F12)) {
            self.server.open = true;
            self.server_test();
        }
        // éditeur de terrain : F2–F9 pinceaux, + / - rayon, route en cours (Entrée, Retour arrière, Échap)
        if self.proj.terrain.is_some() {
            use crate::terrain::Brush;
            let fk = ctx.input(|i| [Key::F2, Key::F3, Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10, Key::F11].map(|k| i.key_pressed(k)));
            let brushes = [Brush::Raise, Brush::Lower, Brush::Flatten, Brush::Smooth, Brush::Noise, Brush::Paint, Brush::Road, Brush::Vegetation, Brush::Water, Brush::Particles];
            for (p, b) in fk.iter().zip(brushes) {
                if *p {
                    self.set_terrain_brush(b);
                }
            }
        }
        if self.tool == Tool::Terrain {
            let (plus, minus) = ctx.input(|i| (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals), i.key_pressed(Key::Minus)));
            if plus || minus {
                self.terrain.radius = (self.terrain.radius * if plus { 1.25 } else { 0.8 }).clamp(2.0, 120.0);
                self.status = format!("Rayon du pinceau : {:.0} m", self.terrain.radius);
            }
            if !self.terrain.road_pts.is_empty() {
                let (enter, back, esc) = ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::Enter), i.consume_key(Modifiers::NONE, Key::Backspace), i.consume_key(Modifiers::NONE, Key::Escape)));
                if enter {
                    self.finish_road();
                }
                if back {
                    self.terrain.road_pts.pop();
                    self.status = format!("{} point(s) dans la route en cours.", self.terrain.road_pts.len());
                }
                if esc {
                    self.terrain.road_pts.clear();
                    self.status = "Route annulée.".into();
                }
            }
        }
        // poser au sol : Maj seule (appuyer puis relâcher sans rien d'autre) ou Fin
        let (shift, other, now) = ctx.input(|i| (i.modifiers.shift, i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::PointerButton { pressed: true, .. } | egui::Event::MouseWheel { .. })), i.time));
        match (shift, self.shift_since) {
            (true, None) => {
                self.shift_since = Some(now);
                self.shift_tap = !other;
            }
            (true, Some(_)) => {
                if other {
                    self.shift_tap = false;
                }
            }
            (false, Some(t0)) => {
                self.shift_since = None;
                if self.shift_tap && now - t0 < 0.45 && !self.selected.is_empty() && matches!(self.drag, Drag::None) {
                    let n = self.snap_selected_to_ground(true);
                    self.status = format!("{n} objet(s) posé(s) au sol (Maj).");
                }
            }
            (false, None) => {}
        }
        // Ctrl+H : isoler la sélection (le contraire de H, cacher) ; de nouveau pour revenir
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::H))) {
            self.toggle_isolate();
        }
        if ctx.input(|i| i.key_pressed(Key::End)) && !self.selected.is_empty() {
            let n = self.snap_selected_to_ground(true);
            self.status = format!("{n} objet(s) posé(s) au sol (Fin).");
        }
        if ctx.input(|i| i.key_pressed(Key::I) && !i.modifiers.command) {
            self.intlib.open = !self.intlib.open;
        }
        // ronde de PNJ en cours : Entrée pour créer, Échap pour annuler
        if self.tool == Tool::Life && !self.life.route.is_empty() {
            let (enter, esc) = ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::Enter), i.consume_key(Modifiers::NONE, Key::Escape)));
            if enter {
                self.finish_ped_route();
            }
            if esc {
                self.life.route.clear();
            }
        }
        if ctx.input(|i| i.key_pressed(Key::N) && !i.modifiers.command) {
            self.buildings.open = !self.buildings.open;
        }
        let k = ctx.input(|i| {
            [
                i.key_pressed(Key::Delete),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Num1),
                i.key_pressed(Key::Num2),
                i.key_pressed(Key::Num3),
                i.key_pressed(Key::M),
                i.key_pressed(Key::OpenBracket),
                i.key_pressed(Key::CloseBracket),
            ]
        });
        // poignées : G déplacer, R tourner, T taille
        // E comme « Edit Rotation: E » des raccourcis de CodeWalker (W et R y servent aussi, mais déplacent la caméra ici)
        let gk = ctx.input(|i| [i.key_pressed(Key::G), i.key_pressed(Key::R) || i.key_pressed(Key::E), i.key_pressed(Key::T)]);
        for (pressed, mode, name) in [(gk[0], crate::gizmo::GizmoMode::Move, "déplacer"), (gk[1], crate::gizmo::GizmoMode::Rotate, "tourner"), (gk[2], crate::gizmo::GizmoMode::Scale, "agrandir")] {
            if pressed {
                self.gizmo_mode = mode;
                self.tool = Tool::Select;
                self.status = format!("Poignées : {name} (G déplacer · R ou E tourner · T agrandir).");
            }
        }
        if k[0] {
            self.delete_selected();
        }
        if k[1] {
            self.game.isolate = None;
            self.isolate_sel = None;
            self.set_selection(vec![]);
            self.drag = Drag::None;
        }
        if k[2] {
            self.tool = Tool::Select;
        }
        if k[3] {
            self.tool = Tool::Place;
        }
        if k[4] {
            self.tool = Tool::Erase;
        }
        if ctx.input(|i| i.key_pressed(Key::Num4)) {
            self.tool = Tool::Rect;
        }
        if ctx.input(|i| i.key_pressed(Key::B)) {
            self.library.open = !self.library.open;
        }
        if ctx.input(|i| i.key_pressed(Key::Num5)) {
            self.tool = Tool::Terrain;
            self.tab = Tab::Terrain;
        }
        if ctx.input(|i| i.key_pressed(Key::J)) {
            self.smart_select();
        }
        if k[5] {
            self.cam.map_view = !self.cam.map_view;
        }
        if self.selected.is_empty() {
            return;
        }
        if k[6] || k[7] {
            self.push_undo();
            self.rotate_selected(if k[6] { -15.0 } else { 15.0 });
        }
        let step = if ctx.input(|i| i.modifiers.shift) { 10.0 } else { 1.0 };
        let dirs = ctx.input(|i| [i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight)]);
        let (f, r) = self.cam.flat_axes();
        let mut d = V3::ZERO;
        for (pressed, dir) in dirs.iter().zip([f, -f, -r, r]) {
            if *pressed {
                d = d + dir;
            }
        }
        if d.len() > 0.0 {
            self.push_undo();
            // pas aligné sur les axes du monde
            let d = if d.x.abs() > d.y.abs() { v3(d.x.signum(), 0.0, 0.0) } else { v3(0.0, d.y.signum(), 0.0) };
            self.move_selected(d * step);
        }
    }
}
