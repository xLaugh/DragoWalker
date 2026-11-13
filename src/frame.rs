//! Une image de l'application : tout ce qui est fait à chaque rafraîchissement de la fenêtre.
//! (Corps de `update`, déplacé de app.rs sans modification.)

use crate::ai::{self, ChatLine};
use crate::app::*;
use crate::math::{v3, V3};
use crate::mech;
use eframe::egui::{self, Color32, RichText};

/// Secondes sans activité avant de redessiner moins souvent, puis encore moins — choix de DragoWalker.
const IDLE_AFTER: f64 = 1.5;
const IDLE_SLOW_AFTER: f64 = 20.0;

impl DwApp {
    pub fn run_frame(&mut self, ctx: &egui::Context) {
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
                        d.filter(|d| !d.children.is_empty()).map(|d| d.children.iter().fold(V3::ZERO, |a, k| a + e.pos + e.rot().rotate(k.pos)) * (1.0 / d.children.len() as f32)).unwrap_or(e.pos)
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
                // DW_TEST_ROADS2=n : 1 tunnel sous une colline, 2 rond-point et 4 routes, 3 avenue prolongée par une rue
                // (raccord), 4 autoroute et bretelle, 5 route de campagne supprimée (peinture du sol effacée) ;
                // DW_TEST_ROADS2_LOOK="x,y,dist,yaw,pitch" (décalage depuis le centre)
                if let Some(mode) = std::env::var("DW_TEST_ROADS2").ok().and_then(|v| v.parse::<u8>().ok()) {
                    let h = |s: &DwApp, x: f32, y: f32| s.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                    let road = |s: &mut DwApp, kind: u8, pts: &[V3]| {
                        s.terrain.road_kind = kind;
                        s.terrain.road_pts = pts.iter().map(|p| v3(p.x, p.y, h(s, p.x, p.y))).collect();
                        // comme un clic : accroché aux routes existantes
                        for i in [0, s.terrain.road_pts.len() - 1] {
                            let p = s.terrain.road_pts[i];
                            if let Some((q, _)) = s.road_snap(p) {
                                s.terrain.road_pts[i] = q;
                            }
                        }
                        s.finish_road();
                        println!("{}", s.status);
                    };
                    match mode {
                        1 => {
                            if let Some(t) = self.proj.terrain.as_mut() {
                                for _ in 0..40 {
                                    t.brush(crate::terrain::Brush::Raise, c, 70.0, 6.0, 1.0, 0, 0.0);
                                }
                            }
                            self.terrain.road_tunnel = true;
                            road(self, 0, &[c + v3(-170.0, 0.0, 0.0), c + v3(170.0, 0.0, 0.0)]);
                            self.terrain.road_tunnel = false;
                        }
                        2 => {
                            self.terrain.road_kind = 0;
                            self.terrain.ring_radius = 22.0;
                            self.place_ring(c);
                            println!("{}", self.status);
                            for a in [0.0f32, 90.0, 200.0, 290.0] {
                                let d = v3(a.to_radians().cos(), a.to_radians().sin(), 0.0);
                                road(self, 0, &[c + d * 130.0, c + d * 22.0]);
                            }
                        }
                        3 => {
                            road(self, 1, &[c + v3(-150.0, -40.0, 0.0), c + v3(0.0, 0.0, 0.0)]);
                            road(self, 0, &[c, c + v3(70.0, 50.0, 0.0), c + v3(150.0, 60.0, 0.0)]);
                        }
                        4 => {
                            road(self, 2, &[c + v3(-200.0, 0.0, 0.0), c + v3(200.0, 0.0, 0.0)]);
                            road(self, 0, &[c + v3(-200.0, -90.0, 0.0), c + v3(200.0, -90.0, 0.0)]);
                            // entrée : de la rue vers l'autoroute
                            road(self, 6, &[c + v3(-120.0, -90.0, 0.0), c + v3(-60.0, -40.0, 0.0), c + v3(10.0, -9.0, 0.0)]);
                        }
                        _ => {
                            road(self, 3, &[c + v3(-150.0, -60.0, 0.0), c + v3(0.0, 20.0, 0.0), c + v3(150.0, -10.0, 0.0)]);
                            let painted = |s: &DwApp| s.proj.terrain.as_ref().map(|t| t.mat.iter().filter(|m| **m == 6).count()).unwrap_or(0);
                            println!("cases peintes « route » : {}", painted(self));
                            if let Some(t) = self.proj.terrain.as_mut() {
                                t.roads.clear();
                                t.repaint_roads();
                            }
                            println!("après suppression : {}", painted(self));
                            road(self, 3, &[c + v3(-150.0, 60.0, 0.0), c + v3(150.0, 60.0, 0.0)]);
                        }
                    }
                    if let Some(t) = self.proj.terrain.as_ref() {
                        println!("routes {}, carrefours {}, lampadaires {}, trous {}", t.roads.len(), t.junctions.len(), self.lamps_count(), t.holes.len());
                        for r in &t.roads {
                            println!("   {} : {:.0} m, tunnels {:?}, raccord {:?}, sens unique {}, anneau {}", r.kind().name, r.length(), r.tunnels, r.taper, r.oneway, r.ring);
                        }
                    }
                    if let Some(net) = self.proj.terrain.as_ref().map(crate::traffic::build_network) {
                        let one = net.links.iter().filter(|l| l.3 == 0).count();
                        println!("circulation : {} points, {} liens dont {one} à sens unique", net.nodes.len(), net.links.len());
                    }
                    let look: Vec<f32> = std::env::var("DW_TEST_ROADS2_LOOK").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
                    let at = c + v3(look.first().copied().unwrap_or(0.0), look.get(1).copied().unwrap_or(0.0), 0.0);
                    let at = v3(at.x, at.y, h(self, at.x, at.y));
                    self.goto(at, look.get(2).copied().unwrap_or(90.0));
                    if look.len() >= 5 {
                        // 6e valeur : hauteur au-dessus de la route la plus proche (dans un tunnel)
                        let road_z = self.proj.terrain.as_ref().and_then(|t| t.roads.iter().filter_map(|r| r.nearest(at.x, at.y)).min_by(|a, b| a.0.partial_cmp(&b.0).unwrap()).map(|n| n.1));
                        self.cam.target.z = match (look.get(5), road_z) {
                            (Some(o), Some(z)) => z + o,
                            _ => at.z + 3.0,
                        };
                        self.cam.yaw = look[3];
                        self.cam.pitch = look[4];
                        self.cam.snap();
                    }
                }
                // DW_TEST_ROADEDIT=1 : deux rues qui se croisent, un point déplacé (plus de croisement), type changé, suppression
                if std::env::var("DW_TEST_ROADEDIT").is_ok() {
                    let h = |s: &DwApp, x: f32, y: f32| s.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                    for (a, b) in [(v3(-120.0, 0.0, 0.0), v3(120.0, 0.0, 0.0)), (v3(0.0, -100.0, 0.0), v3(0.0, 100.0, 0.0))] {
                        let (a, b) = (c + a, c + b);
                        self.terrain.road_kind = 0;
                        self.terrain.road_pts = vec![v3(a.x, a.y, h(self, a.x, a.y)), v3(b.x, b.y, h(self, b.x, b.y))];
                        self.finish_road();
                    }
                    let state = |s: &DwApp, what: &str| {
                        if let Some(t) = s.proj.terrain.as_ref() {
                            let (l, st) = s.signals_count();
                            println!("{what} : {} routes, {} carrefours, ouvertures de la 1re : {}, feux {l} stops {st}, lampadaires {}", t.roads.len(), t.junctions.len(), t.roads[0].gaps.len(), s.lamps_count());
                        }
                    };
                    state(self, "au départ");
                    if let Some(t) = self.proj.terrain.as_mut() {
                        t.roads[1].pts[1] = [c.x + 60.0, c.y - 30.0];
                        t.roads[1].pts[0] = [c.x + 60.0, c.y - 100.0];
                    }
                    self.rebuild_road_network(&[1]);
                    state(self, "point déplacé");
                    if let Some(t) = self.proj.terrain.as_mut() {
                        t.roads[1].pts[1] = [c.x, c.y + 100.0];
                        t.roads[1].pts[0] = [c.x, c.y - 100.0];
                        t.roads[1].kind = 1;
                    }
                    self.rebuild_road_network(&[1]);
                    state(self, "remis et devenu avenue");
                    if let Some(t) = self.proj.terrain.as_mut() {
                        t.roads.remove(1);
                    }
                    self.rebuild_road_network(&[]);
                    state(self, "supprimé");
                    self.goto(c, 120.0);
                }
                // DW_TEST_CLIP=1 : une colline et une route copiées, collées 120 m plus loin, tournées d'un quart
                if std::env::var("DW_TEST_CLIP").is_ok() {
                    let h = |s: &DwApp, x: f32, y: f32| s.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                    if let Some(t) = self.proj.terrain.as_mut() {
                        for _ in 0..10 {
                            t.brush(crate::terrain::Brush::Raise, c + v3(-30.0, -60.0, 0.0), 20.0, 6.0, 1.0, 0, 0.0);
                        }
                    }
                    self.terrain.road_kind = 3;
                    let (a, b) = (c + v3(-60.0, -70.0, 0.0), c + v3(40.0, -50.0, 0.0));
                    self.terrain.road_pts = vec![v3(a.x, a.y, h(self, a.x, a.y)), v3(b.x, b.y, h(self, b.x, b.y))];
                    self.finish_road();
                    println!("{}", self.clip_copy(c + v3(-70.0, -100.0, 0.0), c + v3(50.0, -20.0, 0.0)));
                    self.terrain.clip_rot = 1;
                    println!("{}", self.clip_paste(c + v3(0.0, 90.0, 0.0)));
                    if let Some(t) = self.proj.terrain.as_ref() {
                        println!("routes {} ; hauteur colline d'origine {:.1}, copie {:.1}", t.roads.len(), h(self, c.x - 30.0, c.y - 60.0), t.height_at(c.x + 0.0 - 0.0 + 20.0, c.y + 90.0 - 0.0 + 0.0).unwrap_or(0.0));
                    }
                    self.goto(c + v3(0.0, 30.0, 0.0), 160.0);
                }
                // DW_TEST_HOUSE=1 : trois bâtiments de l'atelier (toit plat, deux pans, quatre pans), le premier transformé
                // en intérieur ; DW_TEST_HOUSE_LOOK="x,y,dist,yaw,pitch"
                if std::env::var("DW_TEST_HOUSE").is_ok() {
                    let h = |s: &DwApp, x: f32, y: f32| s.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                    let z = h(self, c.x, c.y);
                    let r = |x: f32, y: f32, w: f32, d: f32| vec![v3(x - w, y - d, z), v3(x + w, y - d, z), v3(x + w, y + d, z), v3(x - w, y + d, z)];
                    println!("{}", self.create_house(&r(c.x - 25.0, c.y, 8.0, 6.0)));
                    println!("{}", self.create_house(&r(c.x, c.y, 6.0, 5.0)));
                    println!("{}", self.create_house(&[v3(c.x + 15.0, c.y - 6.0, z), v3(c.x + 30.0, c.y - 6.0, z), v3(c.x + 30.0, c.y + 8.0, z), v3(c.x + 23.0, c.y + 8.0, z), v3(c.x + 23.0, c.y, z), v3(c.x + 15.0, c.y, z)]));
                    if self.proj.houses.len() >= 3 {
                        self.proj.houses[0].floors = 4;
                        self.proj.houses[0].shop = true;
                        self.proj.houses[1].roof = 1;
                        self.proj.houses[1].facade = 0;
                        self.proj.houses[2].roof = 2;
                        self.proj.houses[2].facade = 3;
                        self.proj.houses[2].floors = 1;
                        println!("{}", self.house_to_interior(0));
                        // DW_TEST_DOORS=1 : 2e porte (autre modèle) au dos du 2e bâtiment, le 3e transformé en intérieur
                        if std::env::var("DW_TEST_DOORS").is_ok() {
                            self.proj.houses[1].more_doors.push(crate::atelier::DoorSpec { edge: 2, t: 0.3, ..Default::default() });
                            println!("{}", self.house_set_door(1, 1, Some("prop_ron_door_01".into())));
                            println!("{}", self.house_to_interior(2));
                            let h = &self.proj.houses[2];
                            println!("porte de l'intérieur : {:?}", h.door_fit(0));
                        }
                        // DW_TEST_INSIDE=1 : 2e bâtiment, sol en terre cuite du jeu, cloison avec passage, plafonnier
                        if std::env::var("DW_TEST_INSIDE").is_ok() {
                            self.proj.houses[1].floor_kind = 9;
                            self.proj.houses[1].walls.push(crate::atelier::Partition { floor: 0, a: [1.0, -4.7], b: [1.0, 4.7], door: true });
                            println!("{}", self.house_add_light(1, 0, [-3.0, 1.0]));
                            self.opts.show_rooms = false;
                        }
                        // DW_TEST_WINDOWS=1 : 2e bâtiment petits carreaux, une fenêtre retirée, une baie vitrée ; 3e volets
                        if std::env::var("DW_TEST_WINDOWS").is_ok() {
                            self.proj.houses[1].win_style = 1;
                            let c0 = self.proj.houses[1].holes(0, 1).first().map(|h| (h.s0 + h.s1) * 0.5).unwrap_or(2.0);
                            self.proj.houses[1].win_off.push(crate::atelier::WinRef { edge: 0, floor: 1, s: c0 });
                            self.proj.houses[1].win_add.push(crate::atelier::WinSpec { edge: 0, floor: 0, s: 2.5, w: 2.8, h: 2.4, sill: 0.05, style: 2 });
                            self.proj.houses[2].win_style = 3;
                        }
                        // DW_TEST_HOUSE_GAME=1 : textures et salissures du jeu
                        if std::env::var("DW_TEST_HOUSE_GAME").is_ok() {
                            self.proj.houses[1].game_facade = 1;
                            self.proj.houses[1].game_roof = 9;
                            self.proj.houses[1].grime = 0.7;
                            self.proj.houses[2].game_facade = 5;
                            self.proj.houses[2].grime = 0.9;
                            self.proj.houses[2].grime_kind = 12;
                        }
                    }
                    // DW_TEST_PLAN=1 : le rez-de-chaussée coupé en deux, portes posées, intérieur vu de dedans
                    if std::env::var("DW_TEST_PLAN").is_ok() {
                        self.plan.floor = 0;
                        self.plan.floor_h = 3.2;
                        println!("{}", self.plan_split(v3(2.0, 0.0, 0.0)));
                        println!("{}", self.plan_doors());
                        if let Some((ti, mi)) = self.cur_mlo {
                            let m = &self.proj.ytyps[ti].mlos[mi];
                            println!("intérieur : {} pièces, {} portails, {} objets dont {} portes", m.rooms.len(), m.portals.len(), m.entities.len(), m.entities.iter().filter(|e| e.archetype.contains("door")).count());
                        }
                        self.opts.show_rooms = false;
                    }
                    // DW_TEST_HOUSE_FREE=1 : emprise de forme libre, 2 étages, toit plat, à 45 m au nord
                    if std::env::var("DW_TEST_HOUSE_FREE").is_ok() {
                        let pts: Vec<V3> = [[-12.0f32, -4.0], [6.0, -6.0], [13.0, 1.0], [10.0, 7.0], [-2.0, 5.0], [-11.0, 6.0]].iter().map(|p| v3(c.x + p[0], c.y + 45.0 + p[1], z)).collect();
                        println!("{}", self.create_house(&pts));
                        let k = self.proj.houses.len() - 1;
                        self.proj.houses[k].roof = 0;
                        self.proj.houses[k].floors = 2;
                        self.set_selection(vec![]);
                    }
                    for hs in &self.proj.houses {
                        let (pos, _, parts) = hs.mesh(false);
                        println!("   {} : {} sommets, {} triangles, boîte {:?}", hs.name, pos.len(), parts.iter().map(|p| p.1.len() / 3).sum::<usize>(), hs.bbox());
                    }
                    self.set_selection(vec![]);
                    let look: Vec<f32> = std::env::var("DW_TEST_HOUSE_LOOK").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
                    let at = c + v3(look.first().copied().unwrap_or(0.0), look.get(1).copied().unwrap_or(0.0), 0.0);
                    self.goto(v3(at.x, at.y, z), look.get(2).copied().unwrap_or(70.0));
                    if look.len() >= 5 {
                        self.cam.target.z = z + look.get(5).copied().unwrap_or(4.0);
                        self.cam.yaw = look[3];
                        self.cam.pitch = look[4];
                        self.cam.snap();
                    }
                }
                // DW_TEST_EXTEND="k[,angle]" : une route de type k, puis une autre qui part de son bout (relier au bout)
                // en tournant de `angle` degrés ; la caméra regarde le raccord
                if let Ok(spec) = std::env::var("DW_TEST_EXTEND") {
                    let v: Vec<f32> = spec.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    let k = v.first().copied().unwrap_or(0.0) as u8;
                    let ang = v.get(1).copied().unwrap_or(45.0).to_radians();
                    let h = |s: &DwApp, x: f32, y: f32| s.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                    self.terrain.road_kind = k;
                    let (a, j) = (c + v3(-120.0, -30.0, 0.0), c);
                    self.terrain.road_pts = vec![v3(a.x, a.y, h(self, a.x, a.y)), v3(j.x, j.y, h(self, j.x, j.y))];
                    self.finish_road();
                    println!("{}", self.status);
                    let d = v3(j.x - a.x, j.y - a.y, 0.0).norm();
                    let d2 = v3(d.x * ang.cos() - d.y * ang.sin(), d.x * ang.sin() + d.y * ang.cos(), 0.0);
                    let b = j + d2 * 110.0;
                    let js = self.road_snap(v3(j.x + 1.5, j.y - 1.0, 0.0)).map(|q| q.0).unwrap_or(j);
                    self.terrain.road_pts = vec![js, v3(b.x, b.y, h(self, b.x, b.y))];
                    self.finish_road();
                    println!("{}", self.status);
                    if let Some(t) = self.proj.terrain.as_ref() {
                        println!("routes : {}, carrefours : {}", t.roads.len(), t.junctions.len());
                        for r in &t.roads {
                            println!("   {} points cliqués, {:.0} m, ouvertures {:?}", r.pts.len(), r.length(), r.gaps);
                        }
                    }
                    self.goto(v3(j.x, j.y, h(self, j.x, j.y)), 45.0);
                }
                // DW_TEST_BRIDGE=1 : une avenue, une rue en mode pont qui passe au-dessus, une passerelle ; 2 : un pont
                // au-dessus d'un ravin creusé ; DW_TEST_BRIDGE_LOOK=x,y,dist : caméra (décalage depuis le centre)
                if let Some(mode) = std::env::var("DW_TEST_BRIDGE").ok().and_then(|v| v.parse::<u8>().ok()) {
                    let h = |s: &DwApp, x: f32, y: f32| s.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                    let build = |s: &mut DwApp, kind: u8, bridge: bool, a: V3, b: V3| {
                        s.terrain.road_kind = kind;
                        s.terrain.road_bridge = bridge;
                        let (ha, hb) = (h(s, a.x, a.y), h(s, b.x, b.y));
                        s.terrain.road_pts = vec![v3(a.x, a.y, ha), v3(b.x, b.y, hb)];
                        s.finish_road();
                        println!("{}", s.status);
                        if let Some(r) = s.proj.terrain.as_ref().and_then(|t| t.roads.last()) {
                            println!("   tronçons {:?} piles {:?} culées {:?} escaliers {:?}", r.spans, r.piers, r.abutments, r.stairs);
                        }
                    };
                    if mode == 2 {
                        if let Some(t) = self.proj.terrain.as_mut() {
                            for k in 0..14 {
                                let p = c + v3(0.0, -60.0 + k as f32 * 9.0, 0.0);
                                t.brush(crate::terrain::Brush::Flatten, p, 30.0, 60.0, 1.0, 0, -16.0);
                            }
                        }
                        build(self, 3, true, c + v3(-110.0, 0.0, 0.0), c + v3(110.0, 0.0, 0.0));
                    } else {
                        build(self, 1, false, c + v3(-150.0, 0.0, 0.0), c + v3(150.0, 0.0, 0.0));
                        build(self, 0, true, c + v3(0.0, -150.0, 0.0), c + v3(0.0, 150.0, 0.0));
                        build(self, 5, false, c + v3(60.0, -18.0, 0.0), c + v3(60.0, 18.0, 0.0));
                        // une route ordinaire qui passe sous le pont : pas de carrefour
                        build(self, 3, false, c + v3(-30.0, 60.0, 0.0), c + v3(30.0, 60.0, 0.0));
                    }
                    let n = self.proj.terrain.as_ref().map(|t| t.junctions.len()).unwrap_or(0);
                    println!("carrefours : {n}");
                    let look: Vec<f32> = std::env::var("DW_TEST_BRIDGE_LOOK").ok().map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
                    let at = c + v3(look.first().copied().unwrap_or(0.0), look.get(1).copied().unwrap_or(0.0), 0.0);
                    let at = v3(at.x, at.y, h(self, at.x, at.y));
                    self.goto(at, look.get(2).copied().unwrap_or(90.0));
                    if look.len() >= 5 {
                        self.cam.target.z = at.z + 4.0;
                        self.cam.yaw = look[3];
                        self.cam.pitch = look[4];
                        self.cam.snap();
                    }
                }
                // DW_TEST_TJOIN="angle,type principal,type branche[,courbe]" : une route qui finit en T sur une autre,
                // en biais ; courbe = 1 ajoute un virage serré à la branche ; la caméra regarde le carrefour
                if let Ok(spec) = std::env::var("DW_TEST_TJOIN") {
                    let v: Vec<f32> = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    let (ang, km, kb) = (v.first().copied().unwrap_or(60.0), v.get(1).copied().unwrap_or(0.0) as u8, v.get(2).copied().unwrap_or(0.0) as u8);
                    let curve = v.get(3).copied().unwrap_or(0.0) > 0.5;
                    let h = |s: &DwApp, x: f32, y: f32| s.proj.terrain.as_ref().and_then(|t| t.height_at(x, y)).unwrap_or(0.0);
                    self.terrain.road_kind = km;
                    self.terrain.road_pts = vec![v3(c.x - 90.0, c.y, h(self, c.x - 90.0, c.y)), v3(c.x + 90.0, c.y, h(self, c.x + 90.0, c.y))];
                    self.finish_road();
                    println!("{}", self.status);
                    let a = ang.to_radians();
                    let far = v3(c.x - 70.0 * a.cos(), c.y - 70.0 * a.sin(), 0.0);
                    self.terrain.road_kind = kb;
                    let mut pts = vec![v3(far.x, far.y, h(self, far.x, far.y))];
                    if curve {
                        // virage serré : 30 m plus loin, la route repart à angle droit
                        let k = v3(far.x - 30.0 * a.sin(), far.y + 30.0 * a.cos(), 0.0);
                        pts.insert(0, v3(k.x, k.y, h(self, k.x, k.y)));
                    }
                    pts.push(v3(c.x, c.y, h(self, c.x, c.y)));
                    self.terrain.road_pts = pts;
                    self.finish_road();
                    println!("{}", self.status);
                    let (l, s) = self.signals_count();
                    println!("signalisation : {l} feu(x), {s} stop(s)");
                    if let Some(j) = self.proj.terrain.as_ref().and_then(|t| t.junctions.first()).cloned() {
                        println!("   branches {:?}", j.arms.iter().map(|a| (a.dir, a.hw, a.total, a.len)).collect::<Vec<_>>());
                        for (n, p, _) in crate::signals::junction_props(&j) {
                            println!("      {n} à ({:.1}, {:.1}, {:.2})", p.x - j.pos[0], p.y - j.pos[1], p.z);
                        }
                        for e in self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| j.props.contains(&e.id)) {
                            let s = self.proj.terrain.as_ref().and_then(|t| t.surface_at(e.pos.x, e.pos.y));
                            println!("      posé : {} à ({:.2}, {:.2}, {:.3}), surface {:?}", e.archetype, e.pos.x, e.pos.y, e.pos.z, s);
                        }
                        // DW_TEST_TJOIN_LOOK=v : la caméra sur le virage, sinon sur le carrefour
                        let look = std::env::var("DW_TEST_TJOIN_LOOK").ok();
                        let at = if look.as_deref() == Some("v") { v3(far.x, far.y, j.pos[2]) } else { v3(j.pos[0], j.pos[1], j.pos[2]) };
                        self.goto(at, 40.0);
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
                    // DW_TEST_VEG_GAP=m : écart choisi entre les objets
                    self.terrain.veg_spacing = std::env::var("DW_TEST_VEG_GAP").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
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
                                    std::sync::Arc::make_mut(&mut t.h)[j * t.nx + i] = z - 2.3;
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
                if let Ok(mode) = std::env::var("DW_TEST_OCCLUDER") {
                    let m = if mode == "auto" { self.add_occluders_auto() } else { self.add_occluders_for_selection() };
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
        // DW_TEST_PROPOSE="x,y" : à l'image 100, l'ouverture proposée sous ce point de l'écran (la même fonction que le clic)
        if self.frame == 100 && self.bedit.is_some() {
            if let Ok(spec) = std::env::var("DW_TEST_PROPOSE") {
                let v: Vec<f32> = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if v.len() == 2 {
                    println!("proposition sous {:?} : {:?}", v, self.bedit_opening_at(egui::pos2(v[0], v[1])));
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
                // DW_TEST_PROPOSE="x,y" : ouverture proposée sous ce point de l'écran (la même fonction que le clic)
                if std::env::var("DW_TEST_PROPOSE").is_ok() {
                    if let Some(b) = self.bedit.as_mut() {
                        b.mode = 1;
                    }
                }
                // DW_TEST_OPENING="x,y,z,dirx,diry,largeur,hauteur,profondeur" : perce une ouverture (repère du modèle) au
                // lieu de retirer la moitié du modèle, puis regarde de face
                if let Ok(spec) = std::env::var("DW_TEST_OPENING") {
                    let v: Vec<f32> = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    if v.len() == 8 {
                        let cut = self.bedit.as_ref().map(|b| b.cut.clone()).unwrap_or_default();
                        let o = crate::openings::Opening { pos: [v[0], v[1], v[2]], dir: [v[3], v[4]], w: v[5], h: v[6], depth: v[7], kind: "test".into(), sill: false, prop: String::new() };
                        if let Some(c) = self.proj.building_cuts.iter_mut().find(|c| c.name == cut) {
                            c.openings.push(o);
                        }
                        if let Some(e) = self.bedit.as_ref().and_then(|b| self.proj.find_id(b.entity)).and_then(|r| self.proj.ent(r)) {
                            let target = e.pos + e.rot.rotate(v3(v[0], v[1], v[2]).mulv(e.scale()));
                            self.goto(target, 14.0);
                        }
                        println!("ouverture percée : {spec}");
                        // DW_TEST_OPENING_PROP="modèle" : pose aussi cette porte dans l'ouverture
                        if let Ok(pn) = std::env::var("DW_TEST_OPENING_PROP") {
                            if let Some(c) = self.proj.building_cuts.iter_mut().find(|c| c.name == cut) {
                                if let Some(o) = c.openings.last_mut() {
                                    o.prop = pn;
                                }
                            }
                            let last = self.proj.building_cuts.iter().find(|c| c.name == cut).map(|c| c.openings.len().saturating_sub(1)).unwrap_or(0);
                            self.bedit_place_prop(&cut, last);
                            println!("porte : {}", self.status);
                            // la porte posée, dans le repère de l'ouverture (r : le long du mur, n : épaisseur, u : hauteur)
                            let ob = self.proj.building_cuts.iter().find(|c| c.name == cut).and_then(|c| c.openings.get(last)).map(|o| (o.obox(), o.w, o.h));
                            let be_ent = self.bedit.as_ref().and_then(|b| self.proj.find_id(b.entity)).and_then(|r| self.proj.ent(r)).cloned();
                            let door = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.archetype.eq_ignore_ascii_case(&std::env::var("DW_TEST_OPENING_PROP").unwrap_or_default())).cloned();
                            if let (Some((ob, w, h)), Some(be_e), Some(d)) = (ob, be_ent, door) {
                                let inv = be_e.rot.inverse();
                                let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
                                for c in d.corners(&self.proj) {
                                    let local = inv.rotate(c - be_e.pos);
                                    let s = be_e.scale();
                                    let l = v3(local.x / s.x, local.y / s.y, local.z / s.z) - ob.c;
                                    for (k, v) in [l.dot(ob.r), l.dot(ob.n), l.dot(ob.u)].iter().enumerate() {
                                        mn[k] = mn[k].min(*v);
                                        mx[k] = mx[k].max(*v);
                                    }
                                }
                                println!("ouverture : largeur {w} hauteur {h} ; porte dans son repère : r {:.2}..{:.2}, n {:.2}..{:.2}, u {:.2}..{:.2} (échelle {:.2})", mn[0], mx[0], mn[1], mx[1], mn[2], mx[2], d.scale_xy);
                            }
                        }
                        if std::env::var("DW_TEST_OPENINGS_PANEL").is_ok() {
                            if let Some(b) = self.bedit.as_mut() {
                                b.mode = 1;
                                b.sel = Some(0);
                            }
                        }
                    }
                } else {
                    let half = self.bedit_test_half();
                    println!("triangles retirés (x > 0) : {half}");
                }
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
                    self.house_doors_follow();
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
                    while *self.server.installing.lock().unwrap_or_else(|e| e.into_inner()) && t0.elapsed().as_secs() < 600 {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
                println!("installation : {} (installé : {})", self.server.install_msg.lock().unwrap_or_else(|e| e.into_inner()), self.server.installed());
                // DW_TEST_ADDONS=1 : télécharge et coche les ressources de la communauté
                if self.server.installed() && std::env::var("DW_TEST_ADDONS").is_ok() {
                    for a in crate::server::ADDONS.iter() {
                        self.server.install_addon(a.key);
                        let t0 = std::time::Instant::now();
                        while *self.server.addon_busy.lock().unwrap_or_else(|e| e.into_inner()) && t0.elapsed().as_secs() < 600 {
                            std::thread::sleep(std::time::Duration::from_millis(300));
                        }
                        println!("ressource {} : {} (installée : {})", a.key, self.server.addon_msg.lock().unwrap_or_else(|e| e.into_inner()), self.server.addon_installed(a.key));
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
                    for l in self.server.console.lock().unwrap_or_else(|e| e.into_inner()).iter() {
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
            } else if self.frame >= std::env::var("DW_SHOT_FRAME").ok().and_then(|v| v.parse().ok()).unwrap_or(120) && !self.shot_sent {
                self.shot_sent = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }

        self.shortcuts(ctx);

        crate::update::banner(ctx);
        self.safety_ui(ctx);
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
            self.crash_window(ctx);
        }
        // fenêtres simples (egui::Window) : une seule fois par image, hors des deux passes du rangement des panneaux
        // (dessinées dans les deux, chaque lettre tapée était lue deux fois)
        self.groups_window(ctx);
        self.atelier_window(ctx);
        self.dock_overlay(ctx);
        self.isolate_banner(ctx);
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(0.0)).show(ctx, |ui| {
            self.view(ui, dt);
        });
        self.schedule_repaint(ctx);
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
