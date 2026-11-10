//! Bibliothèque des intérieurs : tous les intérieurs (MLO) du jeu, avec un plan vu de dessus, et deux façons
//! de les voir : « Aller voir » (on se téléporte dessus) ou « Voir isolé » (on se téléporte et on ne voit
//! que cet intérieur, sans le reste de la carte).

use crate::app::DwApp;
use crate::math::V3;
use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke};

#[derive(Default)]
pub struct IntLib {
    pub open: bool,
    pub filter: String,
    pub chosen: Option<u32>,
}

/// Un type d'intérieur et ses placements dans la carte.
struct IntEntry {
    arch: u32,
    name: String,
    places: Vec<u32>,
    objects: usize,
    sets: usize,
}

impl DwApp {
    fn int_entries(&self) -> Vec<IntEntry> {
        let Some(w) = self.game.world.as_ref() else { return Vec::new() };
        let f = self.intlib.filter.to_lowercase();
        let mut map: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
        for (i, e) in w.ents.iter().enumerate() {
            if e.mlo && w.ymaps[e.ymap as usize].visible {
                map.entry(e.arch).or_default().push(i as u32);
            }
        }
        let mut v: Vec<IntEntry> = map
            .into_iter()
            .filter_map(|(arch, places)| {
                let name = w.name_of(arch);
                if !f.is_empty() && !name.to_lowercase().contains(&f) {
                    return None;
                }
                let def = w.mlo_defs.get(&arch);
                Some(IntEntry { arch, name, places, objects: def.map(|d| d.children.len()).unwrap_or(0), sets: def.map(|d| d.sets.len()).unwrap_or(0) })
            })
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// Placement le plus proche du centre de la vue.
    fn nearest_place(&self, places: &[u32]) -> Option<u32> {
        let w = self.game.world.as_ref()?;
        let t = self.cam.target;
        places.iter().copied().min_by(|a, b| w.ents[*a as usize].pos.dist2d(t).partial_cmp(&w.ents[*b as usize].pos.dist2d(t)).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Plan vu de dessus : les objets de l'intérieur en points, dans le cadre.
    fn draw_int_plan(&self, painter: &egui::Painter, rect: egui::Rect, arch: u32) {
        let Some(w) = self.game.world.as_ref() else { return };
        let Some(def) = w.mlo_defs.get(&arch) else {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "pas de plan", egui::FontId::proportional(10.0), Color32::GRAY);
            return;
        };
        if def.children.is_empty() {
            return;
        }
        let (mut mn, mut mx) = (Pos2::new(f32::MAX, f32::MAX), Pos2::new(f32::MIN, f32::MIN));
        for c in &def.children {
            mn.x = mn.x.min(c.pos.x);
            mn.y = mn.y.min(c.pos.y);
            mx.x = mx.x.max(c.pos.x);
            mx.y = mx.y.max(c.pos.y);
        }
        let sx = (mx.x - mn.x).max(1.0);
        let sy = (mx.y - mn.y).max(1.0);
        let s = (rect.width() - 10.0) / sx.max(sy);
        let c0 = rect.center();
        let mid = Pos2::new((mn.x + mx.x) * 0.5, (mn.y + mx.y) * 0.5);
        for c in &def.children {
            let p = Pos2::new(c0.x + (c.pos.x - mid.x) * s, c0.y - (c.pos.y - mid.y) * s);
            let (mn3, mx3) = w.arch_box(c.arch);
            let size = ((mx3 - mn3).len() * s * 0.25).clamp(1.0, 6.0);
            let col = if c.set >= 0 { Color32::from_rgb(255, 170, 60) } else { Color32::from_rgb(110, 190, 255) };
            painter.circle_filled(p, size, col.gamma_multiply(0.75));
        }
    }

    /// Fenêtre « Bibliothèque des intérieurs ».
    pub fn intlib_window(&mut self, ctx: &egui::Context) {
        // bandeau quand un intérieur est vu seul (une seule fois par image)
        if let Some(iso) = self.game.isolate.filter(|_| self.dock_phase == 1) {
            let name = self.game.world.as_ref().map(|w| w.name_of(w.ents[iso as usize].arch)).unwrap_or_default();
            egui::Area::new(egui::Id::new("iso_banner")).fixed_pos(self.cam.rect.center_top() + egui::vec2(-160.0, 8.0)).order(egui::Order::Foreground).show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Vu seul : {name}")).strong().color(Color32::from_rgb(255, 200, 90)));
                        if ui.button("Revenir à la carte (Échap)").clicked() {
                            self.game.isolate = None;
                        }
                    });
                });
            });
        }
        if !self.intlib.open {
            return;
        }
        let mut open = self.intlib.open;
        let mut go: Option<(u32, bool)> = None;
        self.show_panel(ctx, "interieurs", "Bibliothèque des intérieurs", crate::docking::Dock::Floating, [860.0, 600.0], &mut open, |s, ui| {
            if s.game.world.is_none() {
                ui.label("Charge d'abord le jeu (onglet Jeu).");
                return;
            }
            ui.horizontal(|ui| {
                ui.label("🔍");
                ui.add(egui::TextEdit::singleline(&mut s.intlib.filter).hint_text("Chercher : garage, apartment, office, bank…").desired_width(260.0));
            });
            let list = s.int_entries();
            ui.label(RichText::new(format!("{} intérieurs · plan vu de dessus (bleu : objets fixes, orange : variantes) · « Aller voir » ou « Voir isolé »", list.len())).small().weak());
            ui.separator();
            let avail = ui.available_size();
            let side = 260.0f32.min(avail.x * 0.35);
            let grid_w = (avail.x - side - 16.0).max(200.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(grid_w, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(grid_w);
                    let cell = 120.0;
                    let cols = ((grid_w / (cell + 8.0)).floor() as usize).max(1);
                    let rows = list.len().div_ceil(cols);
                    egui::ScrollArea::vertical().id_salt("intlib_grid").auto_shrink([false; 2]).show_rows(ui, cell + 26.0, rows, |ui, range| {
                        for r in range {
                            ui.horizontal(|ui| {
                                for c in 0..cols {
                                    let i = r * cols + c;
                                    if i >= list.len() {
                                        break;
                                    }
                                    let e = &list[i];
                                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(cell + 4.0, cell + 22.0), Sense::click());
                                    let img = egui::Rect::from_min_size(rect.min + egui::vec2(2.0, 2.0), egui::vec2(cell, cell));
                                    let p = ui.painter_at(rect);
                                    p.rect_filled(img, 5.0, if resp.hovered() { Color32::from_rgb(44, 50, 60) } else { Color32::from_rgb(30, 33, 39) });
                                    s.draw_int_plan(&p, img.shrink(6.0), e.arch);
                                    if s.intlib.chosen == Some(e.arch) {
                                        p.rect_stroke(img, 5.0, Stroke::new(2.0_f32, Color32::from_rgb(255, 170, 60)), egui::StrokeKind::Inside);
                                    }
                                    let short: String = if e.name.chars().count() > 20 { format!("{}…", e.name.chars().take(19).collect::<String>()) } else { e.name.clone() };
                                    p.text(egui::pos2(img.center().x, img.max.y + 3.0), egui::Align2::CENTER_TOP, short, egui::FontId::proportional(11.0), Color32::from_gray(200));
                                    let resp = resp.on_hover_text(&e.name);
                                    if resp.clicked() {
                                        s.intlib.chosen = Some(e.arch);
                                    }
                                    if resp.double_clicked() {
                                        if let Some(pl) = s.nearest_place(&e.places) {
                                            go = Some((pl, true));
                                        }
                                    }
                                }
                            });
                        }
                    });
                });
                ui.vertical(|ui| {
                    ui.set_width(side);
                    let Some(arch) = s.intlib.chosen else {
                        ui.label("Choisis un intérieur (double-clic : le voir isolé).");
                        return;
                    };
                    let Some(e) = list.iter().find(|e| e.arch == arch) else { return };
                    ui.label(RichText::new(&e.name).strong());
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), Sense::hover());
                    let p = ui.painter_at(rect);
                    p.rect_filled(rect, 6.0, Color32::from_rgb(28, 31, 36));
                    s.draw_int_plan(&p, rect.shrink(10.0), arch);
                    ui.label(format!("{} placement(s) dans la carte · {} objet(s) · {} variante(s)", e.places.len(), e.objects, e.sets));
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Aller voir").on_hover_text("Se téléporter sur l'intérieur (le plus proche s'il y en a plusieurs)").clicked() {
                            if let Some(pl) = s.nearest_place(&e.places) {
                                go = Some((pl, false));
                            }
                        }
                        if ui.button(RichText::new("Voir isolé").strong()).on_hover_text("Se téléporter et ne voir que cet intérieur").clicked() {
                            if let Some(pl) = s.nearest_place(&e.places) {
                                go = Some((pl, true));
                            }
                        }
                    });
                });
            });
        });
        self.intlib.open = open;
        if let Some((pl, iso)) = go {
            let pos: Option<V3> = self.game.world.as_ref().map(|w| w.ents[pl as usize].pos);
            if let Some(p) = pos {
                // centre de l'intérieur : moyenne de ses objets
                let c = self
                    .game
                    .world
                    .as_ref()
                    .and_then(|w| {
                        let e = &w.ents[pl as usize];
                        let d = w.mlo_defs.get(&e.arch)?;
                        if d.children.is_empty() {
                            return None;
                        }
                        let s = d.children.iter().fold(V3::ZERO, |a, k| a + e.pos + e.rot.rotate(k.pos));
                        Some(s * (1.0 / d.children.len() as f32))
                    })
                    .unwrap_or(p);
                self.cam.target = c;
                self.cam.dist = 22.0;
                self.game.interiors = true;
                self.game.isolate = if iso { Some(pl) } else { None };
                let name = self.game.world.as_ref().map(|w| w.name_of(w.ents[pl as usize].arch)).unwrap_or_default();
                self.status = if iso { format!("« {name} » vu seul. Échap ou « Revenir à la carte » pour tout réafficher.") } else { format!("Téléporté sur « {name} ».") };
            }
        }
    }
}
