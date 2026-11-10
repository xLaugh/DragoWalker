//! Fenêtres rangeables par glisser-déposer : on attrape un panneau par son titre (Projet, Boîte à outils,
//! Créateur de terrain, Assistant IA, bibliothèques) et on le lâche où on veut :
//! - contre le bord gauche, droit ou bas de l'écran : il s'y range ;
//! - ailleurs : il devient une fenêtre flottante à cet endroit.
//! Pendant le glisser, les zones de rangement s'allument. La place est retenue d'une fois sur l'autre.

use crate::app::DwApp;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Dock {
    Floating,
    Left,
    Right,
    Bottom,
}

impl Dock {
    pub fn label(self) -> &'static str {
        match self {
            Dock::Floating => "fenêtre flottante",
            Dock::Left => "rangé à gauche",
            Dock::Right => "rangé à droite",
            Dock::Bottom => "rangé en bas",
        }
    }
}

/// Part de l'écran au plus pour un panneau rangé sur le côté / en bas : CodeWalker règle DockLeftPortion et
/// DockRightPortion à 0,3 (ProjectForm::InitializeComponent 0x06000e92) ; DockBottomPortion garde la valeur
/// par défaut de DockPanelSuite, 0,25 (relue sur WeifenLuo.WinFormsUI.Docking.dll).
pub const DOCK_SIDE_PORTION: f32 = 0.3;
pub const DOCK_BOTTOM_PORTION: f32 = 0.25;

/// Largeur des zones de rangement le long des bords (px) — **estimé**.
const EDGE: f32 = 90.0;

/// Places choisies (clé du panneau → place), lues dans les réglages.
pub fn load_docks() -> std::collections::HashMap<String, Dock> {
    let v = crate::app::load_settings();
    v.get("docks").and_then(|d| serde_json::from_value(d.clone()).ok()).unwrap_or_default()
}

/// Panneau qu'on est en train de glisser.
pub struct DockDrag {
    pub key: &'static str,
    pub title: String,
    /// vient d'une fenêtre flottante (elle bouge déjà toute seule)
    pub floating: bool,
}

/// Zone sous le pointeur : un bord (rangement) ou None (fenêtre flottante).
fn zone_at(screen: Rect, p: Pos2) -> Option<Dock> {
    if p.x < screen.left() + EDGE {
        Some(Dock::Left)
    } else if p.x > screen.right() - EDGE {
        Some(Dock::Right)
    } else if p.y > screen.bottom() - EDGE {
        Some(Dock::Bottom)
    } else {
        None
    }
}

impl DwApp {
    pub fn dock_of(&self, key: &str, default: Dock) -> Dock {
        if let Ok(v) = std::env::var(format!("DW_DOCK_{}", key.to_uppercase())) {
            return match v.as_str() {
                "left" => Dock::Left,
                "right" => Dock::Right,
                "bottom" => Dock::Bottom,
                _ => Dock::Floating,
            };
        }
        self.docks.get(key).copied().unwrap_or(default)
    }

    pub fn set_dock(&mut self, key: &str, d: Dock) {
        self.docks.insert(key.to_string(), d);
        // les tests ne touchent pas aux réglages de la personne
        if std::env::var("DW_TEST_DOCKDRAG").is_err() {
            crate::app::save_setting("docks", serde_json::to_value(&self.docks).unwrap_or_default());
        }
    }

    /// Affiche un panneau à sa place. `open` : ouvert / fermé (bouton ✖).
    #[allow(clippy::too_many_arguments)]
    pub fn show_panel(&mut self, ctx: &egui::Context, key: &'static str, title: &str, default: Dock, size: [f32; 2], open: &mut bool, content: impl FnOnce(&mut DwApp, &mut egui::Ui)) {
        if !*open {
            return;
        }
        let dock = self.dock_of(key, default);
        // deux passes par image : d'abord les panneaux rangés en bas (pour qu'ils prennent toute la largeur),
        // puis les autres
        if (self.dock_phase == 0) != (dock == Dock::Bottom) {
            return;
        }
        let mut is_open = true;
        let screen = ctx.screen_rect();
        // en-tête des panneaux rangés : titre à attraper pour déplacer, bouton fermer
        let header = |s: &mut DwApp, ui: &mut egui::Ui, is_open: &mut bool| {
            ui.horizontal(|ui| {
                // largeur fixée par le panneau (sans quoi le panneau grandirait à chaque image)
                let w = (ui.available_width() - 48.0).max(40.0);
                let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 20.0), Sense::drag());
                let hov = resp.hovered() || resp.dragged();
                ui.painter().rect_filled(r, 4.0, if hov { Color32::from_gray(60) } else { Color32::from_gray(38) });
                ui.painter().text(r.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, format!(":: {title}"), egui::FontId::proportional(14.0), Color32::from_gray(230));
                let resp = resp.on_hover_text("Glisser pour déplacer : lâcher contre un bord pour ranger, ailleurs pour une fenêtre flottante").on_hover_cursor(egui::CursorIcon::Grab);
                if resp.drag_started() {
                    s.dock_drag = Some(DockDrag { key, title: title.to_string(), floating: false });
                }
                if ui.small_button("✖").on_hover_text("Fermer (menu Affichage pour rouvrir)").clicked() {
                    *is_open = false;
                }
            });
            ui.separator();
        };
        match dock {
            Dock::Floating => {
                // première place d'une fenêtre flottante : l'assistant en haut à droite, le terrain à côté
                let pos = match key {
                    "ia" => [screen.right() - 420.0, 64.0],
                    "terrain" => [screen.right() - 830.0, 64.0],
                    "projet" => [10.0, 90.0],
                    "outils" => [screen.right() - 360.0, 90.0],
                    _ => [280.0, 80.0],
                };
                let mut w = egui::Window::new(RichText::new(title).strong()).id(egui::Id::new(("dw_panel", key))).default_size(size).default_pos(pos).max_height((screen.height() - 60.0).max(200.0)).resizable(true).collapsible(true).open(&mut is_open);
                // lâchée au milieu de l'écran : la fenêtre apparaît là où on l'a lâchée
                if let Some(p) = self.float_pos.remove(key) {
                    w = w.current_pos(p);
                }
                let r = w.show(ctx, |ui| content(self, ui));
                // une fenêtre flottante qu'on glisse (sa place change bouton enfoncé) : zones de rangement
                if let Some(r) = r {
                    let rect = r.response.rect;
                    let down = ctx.input(|i| i.pointer.primary_down());
                    if let Some(prev) = self.float_rects.insert(key, rect) {
                        if down && (prev.min - rect.min).length() > 0.5 && (prev.size() - rect.size()).length() < 0.5 && self.dock_drag.is_none() {
                            self.dock_drag = Some(DockDrag { key, title: title.to_string(), floating: true });
                        }
                    }
                }
            }
            Dock::Left => {
                egui::SidePanel::left(egui::Id::new(("dw_side", key))).default_width(size[0].min(screen.width() * DOCK_SIDE_PORTION)).max_width(screen.width() * DOCK_SIDE_PORTION).resizable(true).show(ctx, |ui| {
                    header(self, ui, &mut is_open);
                    content(self, ui);
                });
            }
            Dock::Right => {
                egui::SidePanel::right(egui::Id::new(("dw_side", key))).default_width(size[0].min(screen.width() * DOCK_SIDE_PORTION)).max_width(screen.width() * DOCK_SIDE_PORTION).resizable(true).show(ctx, |ui| {
                    header(self, ui, &mut is_open);
                    content(self, ui);
                });
            }
            Dock::Bottom => {
                egui::TopBottomPanel::bottom(egui::Id::new(("dw_bottom", key))).default_height(size[1].min(screen.height() * DOCK_BOTTOM_PORTION)).resizable(true).show(ctx, |ui| {
                    header(self, ui, &mut is_open);
                    content(self, ui);
                });
            }
        }
        *open = is_open;
    }

    /// Pendant le glisser d'un panneau : zones de rangement allumées, puis rangement au lâcher.
    pub fn dock_overlay(&mut self, ctx: &egui::Context) {
        let Some(d) = self.dock_drag.as_ref() else { return };
        let screen = ctx.screen_rect();
        let top = screen.top() + 60.0;
        // test : DW_TEST_DOCKDRAG="clé,x,y[,tenir]" glisse un panneau jusqu'à (x, y)
        let test = std::env::var("DW_TEST_DOCKDRAG").ok().map(|v| v.split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>());
        let test_p = test.as_ref().and_then(|v| Some(egui::pos2(v.get(1)?.parse().ok()?, v.get(2)?.parse().ok()?)));
        let hold = test.as_ref().map(|v| v.len() > 3).unwrap_or(false);
        let p = test_p.or_else(|| ctx.pointer_latest_pos()).unwrap_or(screen.center());
        let zone = zone_at(screen, p);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("dw_dock_overlay")));
        let zones = [
            (Dock::Left, Rect::from_min_max(egui::pos2(screen.left(), top), egui::pos2(screen.left() + EDGE, screen.bottom() - EDGE)), "Ranger à gauche"),
            (Dock::Right, Rect::from_min_max(egui::pos2(screen.right() - EDGE, top), egui::pos2(screen.right(), screen.bottom() - EDGE)), "Ranger à droite"),
            (Dock::Bottom, Rect::from_min_max(egui::pos2(screen.left() + EDGE, screen.bottom() - EDGE), egui::pos2(screen.right() - EDGE, screen.bottom())), "Ranger en bas"),
        ];
        for (z, r, label) in zones {
            let on = zone == Some(z);
            painter.rect_filled(r, 6.0, if on { Color32::from_rgba_unmultiplied(255, 160, 50, 110) } else { Color32::from_rgba_unmultiplied(90, 150, 255, 45) });
            painter.text(r.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(14.0), Color32::WHITE);
        }
        if !d.floating {
            // le panneau suit le pointeur
            let g = Rect::from_min_size(p + egui::vec2(10.0, 10.0), egui::vec2(220.0, 30.0));
            painter.rect_filled(g, 6.0, Color32::from_rgba_unmultiplied(40, 44, 52, 230));
            let txt = if zone.is_some() { d.title.clone() } else { format!("{} : fenêtre flottante ici", d.title) };
            painter.text(g.left_center() + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, txt, egui::FontId::proportional(13.0), Color32::from_rgb(255, 200, 90));
        }
        ctx.request_repaint();
        let released = if test_p.is_some() { !hold && self.frame > 110 } else { ctx.input(|i| i.pointer.any_released() || !i.pointer.primary_down()) };
        if released {
            let d = self.dock_drag.take().unwrap();
            match zone {
                Some(z) => {
                    self.set_dock(d.key, z);
                    self.status = format!("« {} » {}.", d.title, z.label());
                }
                None if !d.floating => {
                    self.set_dock(d.key, Dock::Floating);
                    self.float_pos.insert(d.key, p - egui::vec2(60.0, 12.0));
                    self.status = format!("« {} » : fenêtre flottante.", d.title);
                }
                None => {}
            }
        }
    }
}
