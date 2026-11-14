//! Une image de l'application : tout ce qui est fait à chaque rafraîchissement de la fenêtre.
//! (Corps de `update`, déplacé de app.rs puis découpé en étapes ; les modes test DW_TEST_* sont dans
//! frame_tests.rs.)

use crate::ai::{self, ChatLine};
use crate::app::*;
use crate::mech;
use eframe::egui::{self, Color32, RichText};

/// Secondes sans activité avant de redessiner moins souvent, puis encore moins — choix de DragoWalker.
const IDLE_AFTER: f64 = 1.5;
const IDLE_SLOW_AFTER: f64 = 20.0;

impl DwApp {
    pub fn run_frame(&mut self, ctx: &egui::Context) {
        let dt = self.frame_begin(ctx);
        self.shot_wait_game_load();
        self.test_hooks();
        self.shot_wait_models();
        self.poll_ai_reply();
        self.screenshot_tick(ctx);
        self.shortcuts(ctx);
        self.top_bars(ctx);
        self.status_bar(ctx);
        self.docked_panels(ctx);
        self.simple_windows(ctx);
        self.central_view(ctx, dt);
        self.schedule_repaint(ctx);
    }

    /// Début de l'image : compteur, images par seconde, sauvegarde, tâches de fond ; renvoie le pas de temps.
    fn frame_begin(&mut self, ctx: &egui::Context) -> f32 {
        self.frame += 1;
        let raw_dt = ctx.input(|i| i.unstable_dt).max(1e-4);
        self.fps += (1.0 / raw_dt - self.fps) * 0.05;
        self.safety_tick(ctx);
        let dt = ctx.input(|i| i.stable_dt).min(mech::MAX_FRAME_STEP);
        self.poll_game_load(ctx);
        self.sollumz_tick(ctx);
        self.mlo_tick();
        self.cuts_tick();
        self.mlo_link_tick();
        self.shells_tick();
        self.houses_tick();
        self.studio_tick();
        self.server_tick(ctx);
        self.meubler_test_tick();
        dt
    }

    /// Capture automatique : attend la fin du chargement du jeu.
    fn shot_wait_game_load(&mut self) {
        // capture automatique : attendre la fin du chargement du jeu (au plus ~90 s)
        if self.shot_path.is_some() && self.game.loading.is_some() && self.frame > 2 && self.frame < 6000 {
            self.frame = 2;
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    }

    /// Capture automatique : attend les vrais modèles visibles.
    fn shot_wait_models(&mut self) {
        // capture : attendre aussi que les vrais modèles visibles soient chargés (au plus ~60 s)
        if self.shot_path.is_some() && self.frame > 60 && self.frame < 120 {
            let busy = self.pending.is_some() || self.library.pending > 0 || self.game.gpu.as_ref().map(|g| g.lock().map(|g| g.busy()).unwrap_or(false)).unwrap_or(false);
            if busy && self.shot_wait < 4000 {
                self.shot_wait += 1;
                self.frame = 60;
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        }
    }

    /// Réponse de Claude, ou repli sur l'interpréteur local en cas d'erreur.
    fn poll_ai_reply(&mut self) {
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
    }

    /// Capture d'écran automatique : demande l'image puis l'enregistre et ferme la fenêtre.
    fn screenshot_tick(&mut self, ctx: &egui::Context) {
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
            } else if self.frame >= std::env::var("DW_SHOT_FRAME").ok().and_then(|v| v.parse().ok()).unwrap_or(120) && !self.shot_sent && window_can_screenshot(ctx) {
                // fenêtre réduite (taille nulle) : egui_glow planterait (« chunk size must be non-zero ») ;
                // on attend qu'elle soit de nouveau visible
                self.shot_sent = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
    }

    /// Bandeaux du haut : mise à jour, sécurité, menu, barre d'outils (et le terrain entre les deux).
    fn top_bars(&mut self, ctx: &egui::Context) {
        crate::update::banner(ctx);
        self.safety_ui(ctx);
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            self.menu_bar(ui, ctx);
        });
        let now = ctx.input(|i| i.time);
        self.terrain_tick(now);
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(3.0);
            if self.studio.on {
                self.studio_toolbar(ui);
            } else {
                self.toolbar(ui);
            }
            ui.add_space(3.0);
        });
    }

    /// Barre d'état en bas de la fenêtre.
    fn status_bar(&mut self, ctx: &egui::Context) {
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
    }

    /// Panneaux rangeables et leurs fenêtres, en deux passes.
    fn docked_panels(&mut self, ctx: &egui::Context) {
        // panneaux et fenêtres, chacun à sa place (flottant, à gauche, à droite, en bas)
        for phase in [0u8, 1] {
            self.dock_phase = phase;
            // dans le studio, ses propres panneaux remplacent Projet et Boîte à outils
            if !self.panels_hidden && !self.studio.on {
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
            self.crash_window(ctx);
        }
    }

    /// Fenêtres simples, superposition du rangement et bandeau d'isolement.
    fn simple_windows(&mut self, ctx: &egui::Context) {
        // fenêtres simples (egui::Window) : une seule fois par image, hors des deux passes du rangement des panneaux
        // (dessinées dans les deux, chaque lettre tapée était lue deux fois)
        self.studio_panels(ctx);
        self.studio_window(ctx);
        self.groups_window(ctx);
        self.atelier_window(ctx);
        self.dock_overlay(ctx);
        self.isolate_banner(ctx);
    }

    /// La vue 3D au centre.
    fn central_view(&mut self, ctx: &egui::Context, dt: f32) {
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(0.0)).show(ctx, |ui| {
            self.view(ui, dt);
        });
    }

    /// Prochaine image : tout de suite tant qu'il se passe quelque chose (souris, clavier, caméra qui glisse encore,
    /// réponse de l'IA attendue, capture de test), puis lentement — 5 images par seconde pendant 20 s (eau et effets
    /// animés encore fluides à l'œil), puis une par seconde. Avant, chaque image en redemandait une aussitôt : un
    /// cœur du processeur restait occupé à 100 % même sans rien faire. Les tâches de fond (chargement du jeu, des
    /// modèles, des vignettes) redemandent elles-mêmes une image tant qu'elles travaillent.
    fn schedule_repaint(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let input = ctx.input(|i| !i.events.is_empty() || i.pointer.is_moving() || i.pointer.any_down() || !i.keys_down.is_empty() || i.smooth_scroll_delta != egui::Vec2::ZERO);
        let c = &self.cam;
        let sig = [c.eye.x, c.eye.y, c.eye.z, c.fwd.x, c.fwd.y, c.fwd.z, c.cur_ortho];
        let moving = sig.iter().zip(self.last_cam_sig.iter()).any(|(a, b)| (a - b).abs() > 1e-4);
        self.last_cam_sig = sig;
        if input || moving || self.pending.is_some() || self.shot_path.is_some() || self.game.loading.is_some() {
            self.last_active = now;
        }
        let idle = now - self.last_active;
        if idle < IDLE_AFTER {
            ctx.request_repaint();
        } else if idle < IDLE_SLOW_AFTER {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
}

/// Vrai si la fenêtre a une taille non nulle et n'est pas réduite : seule condition où l'on peut demander une
/// capture d'écran (egui_glow lit l'image ligne par ligne, une largeur nulle le fait planter).
pub fn window_can_screenshot(ctx: &egui::Context) -> bool {
    let (minimized, rect) = ctx.input(|i| (i.viewport().minimized.unwrap_or(false), i.screen_rect()));
    let ppp = ctx.pixels_per_point();
    !minimized && rect.is_positive() && (rect.width() * ppp).round() >= 1.0 && (rect.height() * ppp).round() >= 1.0
}
