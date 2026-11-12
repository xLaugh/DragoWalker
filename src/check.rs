//! Contrôle avant l'export : ce que le jeu exige d'un intérieur (MLO), et un compteur de performance comparé aux
//! tailles des réserves (pools) du jeu, lues dans gameconfig.xml.
//!
//! Intérieurs créés ou modifiés dans le projet (pas les .ytyp recopiés tels quels depuis une ressource MLO) :
//! - placé dans une carte (CMloInstanceDef), sinon il n'apparaît nulle part ;
//! - au moins un portail relié à l'extérieur (pièce 0), sinon on ne peut ni entrer ni voir dedans ;
//! - pièces de taille non nulle, qui ne se chevauchent pas ;
//! - 31 pièces au plus avec une collision (RoomId sur 5 bits : BoundMaterial_s::set_RoomId 0x060042e3) ;
//! - portails entre pièces existantes, non plats ;
//! - chaque objet rangé dans une pièce qui existe (ProjectForm::NewMloEntity 0x06000d68 refuse sinon) et dont le
//!   centre est dans la boîte de cette pièce ;
//! - une collision d'intérieur (générée à l'export pour les intérieurs créés dans DragoWalker).

use crate::app::DwApp;
use crate::math::V3;
use eframe::egui::{self, Color32, RichText};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Error,
    Warning,
    Ok,
}

/// Marge pour dire qu'un objet est dans la boîte de sa pièce (m) — **estimé**.
const ROOM_SLACK: f32 = 0.5;
/// Volume commun à partir duquel deux pièces se chevauchent (m³) — **estimé**.
const OVERLAP_M3: f32 = 0.5;

/// Réserves du jeu affichées par le compteur (noms de gameconfig.xml).
const POOLS: [(&str, &str); 6] = [
    ("InteriorInst", "intérieurs chargés"),
    ("PortalInst", "portails d'intérieurs chargés"),
    ("InteriorProxy", "intérieurs déclarés"),
    ("Dummy Object", "objets fixes chargés"),
    ("Object", "objets physiques chargés"),
    ("Peds", "PNJ"),
];

/// Tailles des réserves du jeu : gameconfig.xml (update.rpf d'abord) ; (nom, taille).
pub fn game_pools(w: Option<&crate::gta::world::GameWorld>) -> Vec<(String, u32)> {
    let Some(w) = w else { return Vec::new() };
    let mut files: Vec<&crate::gta::rpf::FileEntry> = w.vfs.files.iter().filter(|f| f.name.eq_ignore_ascii_case("gameconfig.xml")).collect();
    files.sort_by_key(|f| !f.path.contains("update"));
    let Some(d) = files.first().and_then(|f| w.vfs.read_binary(f, &w.keys)) else { return Vec::new() };
    let s = String::from_utf8_lossy(&d);
    let mut out = Vec::new();
    for item in s.split("<PoolName>").skip(1) {
        let Some(e) = item.find("</PoolName>") else { continue };
        let name = item[..e].trim().to_string();
        let size = item.find("<PoolSize value=\"").and_then(|p| item[p + 17..].split('"').next()).and_then(|v| v.parse::<u32>().ok());
        if let Some(size) = size {
            out.push((name, size));
        }
    }
    out
}

impl DwApp {
    /// Contrôle des intérieurs du projet : (gravité, message).
    pub fn mlo_checks(&self) -> Vec<(Level, String)> {
        let mut out = Vec::new();
        let external = self.mlo_external_ytyps();
        let w = self.game.world.as_deref();
        for t in self.proj.ytyps.iter().filter(|t| !external.contains(&t.name.to_lowercase())) {
            for m in &t.mlos {
                let n = &m.name;
                let placed = self.proj.ymaps.iter().filter(|y| !y.sample).flat_map(|y| y.entities.iter()).any(|e| e.mlo.is_some() && e.archetype.eq_ignore_ascii_case(n));
                if !placed {
                    out.push((Level::Error, format!("« {n} » n'est posé dans aucune carte : il n'apparaîtra pas en jeu.")));
                }
                let nr = m.rooms.len();
                // CodeWalker NewMloEntity 0x06000d68 : « Mlo … has no rooms! Cannot create entity. » ; la pièce 0
                // (limbo, l'extérieur) ne compte pas
                if nr <= 1 {
                    out.push((Level::Error, format!("« {n} » n'a aucune pièce (seulement l'extérieur) : impossible d'y ranger des objets.")));
                    continue;
                }
                if !m.portals.iter().any(|p| p.room_from == 0 || p.room_to == 0) {
                    out.push((Level::Error, format!("« {n} » n'a aucun portail vers l'extérieur (pièce 0) : on ne peut ni y entrer ni voir dedans.")));
                }
                if nr - 1 > 31 {
                    out.push((Level::Warning, format!("« {n} » a {} pièces : au-delà de 31, les pièces n'ont pas de collision (numéro de pièce sur 5 bits).", nr - 1)));
                }
                for (ri, r) in m.rooms.iter().enumerate().skip(1) {
                    let s = r.bb_max - r.bb_min;
                    if s.x <= 0.05 || s.y <= 0.05 || s.z <= 0.05 {
                        out.push((Level::Error, format!("« {n} », pièce {ri} « {} » : taille nulle ou négative.", r.name)));
                    }
                    for (rj, q) in m.rooms.iter().enumerate().skip(ri + 1) {
                        let ox = (r.bb_max.x.min(q.bb_max.x) - r.bb_min.x.max(q.bb_min.x)).max(0.0);
                        let oy = (r.bb_max.y.min(q.bb_max.y) - r.bb_min.y.max(q.bb_min.y)).max(0.0);
                        let oz = (r.bb_max.z.min(q.bb_max.z) - r.bb_min.z.max(q.bb_min.z)).max(0.0);
                        if ox * oy * oz > OVERLAP_M3 {
                            out.push((Level::Warning, format!("« {n} » : les pièces {ri} « {} » et {rj} « {} » se chevauchent ({:.1} m³) : le jeu peut hésiter sur la pièce où l'on est.", r.name, q.name, ox * oy * oz)));
                        }
                    }
                }
                for (pi, p) in m.portals.iter().enumerate() {
                    if p.room_from as usize >= nr || p.room_to as usize >= nr {
                        out.push((Level::Error, format!("« {n} », portail {pi} : relie une pièce qui n'existe pas ({} → {}).", p.room_from, p.room_to)));
                    }
                    let c = p.corners;
                    let area = (c[1] - c[0]).cross(c[3] - c[0]).len();
                    if area < 0.01 {
                        out.push((Level::Warning, format!("« {n} », portail {pi} : plat (surface nulle), on ne voit rien au travers.")));
                    }
                }
                // objets de l'intérieur et de ses variantes (entity sets) : même règle que CodeWalker
                // ProjectForm::NewMloEntity 0x06000d68 (« Room at index {0} does not exist in {1}! {2} only has {3}
                // rooms. », et pareil pour les variantes)
                let (mut bad_room, mut outside) = (0, 0);
                for e in m.entities.iter().chain(m.entity_sets.iter().flat_map(|s| s.entities.iter())) {
                    match m.rooms.get(e.room) {
                        None => bad_room += 1,
                        Some(r) => {
                            let inside = |a: f32, lo: f32, hi: f32| a >= lo - ROOM_SLACK && a <= hi + ROOM_SLACK;
                            // (coques des pièces, shell.rs : origine à celle de l'intérieur, voulu)
                            if e.room > 0 && !e.archetype.starts_with(crate::shell::SHELL_PREFIX) && !(inside(e.pos.x, r.bb_min.x, r.bb_max.x) && inside(e.pos.y, r.bb_min.y, r.bb_max.y) && inside(e.pos.z, r.bb_min.z, r.bb_max.z)) {
                                outside += 1;
                            }
                        }
                    }
                }
                if bad_room > 0 {
                    out.push((Level::Error, format!("« {n} » : {bad_room} objet(s) rangé(s) dans une pièce qui n'existe pas (rangés à l'export dans la pièce qui les contient).")));
                }
                if outside > 0 {
                    out.push((Level::Warning, format!("« {n} » : {outside} objet(s) hors de la boîte de leur pièce : ils peuvent disparaître selon l'endroit d'où l'on regarde.")));
                }
                let game_mlo = w.and_then(|w| w.archetypes.get(&crate::gta::crypto::joaat(&n.to_lowercase()))).map(|a| a.mlo).unwrap_or(false);
                if game_mlo {
                    out.push((Level::Warning, format!("« {n} » porte le nom d'un intérieur du jeu : il le remplacera partout.")));
                } else {
                    out.push((Level::Ok, format!("« {n} » : {} pièce(s), {} portail(s), {} objet(s) ; collision des sols générée à l'export.", nr - 1, m.portals.len(), m.entities.len())));
                }
            }
        }
        // boîtes d'occlusion : CodeWalker ne les compte pas dans les limites de la carte (YmapFile::CalcExtents
        // 0x06002cd5 : objets, véhicules garés, lumières lointaines seulement) : une boîte hors de la zone de
        // chargement de ses objets n'est jamais chargée
        for y in self.proj.ymaps.iter().filter(|y| !y.sample && !y.occluders.is_empty()) {
            let (_, _, smn, smx) = y.extents(&self.proj);
            let lost = y.occluders.iter().filter(|b| b.pos.x < smn.x || b.pos.y < smn.y || b.pos.z < smn.z || b.pos.x > smx.x || b.pos.y > smx.y || b.pos.z > smx.z).count();
            if lost > 0 {
                out.push((Level::Warning, format!("Carte « {} » : {lost} boîte(s) d'occlusion hors de la zone de chargement de ses objets : elles ne serviront pas (pose-les dans la carte du bâtiment).", y.name)));
            }
        }
        out.sort_by_key(|x| x.0);
        out
    }

    /// Compteur de performance : (libellé, valeur, réserve du jeu concernée et sa taille, alerte).
    pub fn perf_stats(&self) -> Vec<(String, usize, Option<(String, u32)>, bool)> {
        let pools = game_pools(self.game.world.as_deref());
        let pool = |n: &str| pools.iter().find(|p| p.0 == n).map(|p| (p.0.clone(), p.1));
        let ents: Vec<&crate::model::Entity> = self.proj.ymaps.iter().filter(|y| !y.sample).flat_map(|y| y.entities.iter()).collect();
        let interiors = ents.iter().filter(|e| e.mlo.is_some()).count();
        let mlo_objs: usize = self.proj.ytyps.iter().flat_map(|t| t.mlos.iter()).map(|m| m.entities.len()).sum();
        let portals: usize = self.proj.ytyps.iter().flat_map(|t| t.mlos.iter()).map(|m| m.portals.len()).sum();
        let archs: std::collections::HashSet<String> = ents.iter().map(|e| e.archetype.to_lowercase()).collect();
        let biggest = self.proj.ymaps.iter().filter(|y| !y.sample).map(|y| y.entities.len()).max().unwrap_or(0);
        // une part de chaque réserve est déjà prise par la carte du jeu : alerte au-delà de la moitié — **estimé**
        let warn = |v: usize, p: &Option<(String, u32)>| p.as_ref().map(|p| v * 2 > p.1 as usize).unwrap_or(false);
        let mut out = Vec::new();
        let p = pool("Dummy Object");
        out.push(("Objets posés (cartes du projet)".to_string(), ents.len(), p.clone(), warn(ents.len(), &p)));
        let p = pool("InteriorInst");
        out.push(("Intérieurs posés".to_string(), interiors, p.clone(), warn(interiors, &p)));
        let p = pool("PortalInst");
        out.push(("Portails d'intérieurs".to_string(), portals, p.clone(), warn(portals, &p)));
        out.push(("Objets dans les intérieurs".to_string(), mlo_objs, None, false));
        out.push(("Modèles différents".to_string(), archs.len(), None, false));
        out.push(("Plus grosse carte (objets)".to_string(), biggest, None, false));
        out
    }

    /// Lignes de compte rendu pour l'export : erreurs et alertes du contrôle, alertes du compteur.
    pub fn check_report(&self) -> Vec<String> {
        let mut out = Vec::new();
        let checks = self.mlo_checks();
        let (e, w) = (checks.iter().filter(|c| c.0 == Level::Error).count(), checks.iter().filter(|c| c.0 == Level::Warning).count());
        if e + w > 0 {
            out.push(format!("Contrôle des intérieurs : {e} erreur(s), {w} alerte(s) — menu Fichier » Contrôle avant l'export."));
            for c in checks.iter().filter(|c| c.0 == Level::Error).take(5) {
                out.push(format!("  ✖ {}", c.1));
            }
        }
        for (label, v, p, alert) in self.perf_stats() {
            if alert {
                if let Some((pn, ps)) = p {
                    out.push(format!("Performance : {label} = {v}, plus de la moitié de la réserve « {pn} » du jeu ({ps}) : risque de « Pool Full ».", ));
                }
            }
        }
        out
    }

    /// Fenêtre « Contrôle avant l'export ».
    pub fn check_window(&mut self, ctx: &egui::Context) {
        if !self.check_open {
            return;
        }
        let mut open = true;
        let checks = self.mlo_checks();
        let stats = self.perf_stats();
        self.show_panel(ctx, "controle", "Contrôle avant l'export", crate::docking::Dock::Floating, [560.0, 520.0], &mut open, |_s, ui| {
            ui.label(RichText::new("Intérieurs (MLO)").strong());
            if checks.is_empty() {
                ui.label(RichText::new("Aucun intérieur créé dans le projet.").weak());
            }
            egui::ScrollArea::vertical().id_salt("checks").max_height(280.0).show(ui, |ui| {
                for (lvl, msg) in &checks {
                    let (mark, col) = match lvl {
                        Level::Error => ("✖", Color32::from_rgb(255, 100, 90)),
                        Level::Warning => ("⚠", Color32::from_rgb(255, 200, 80)),
                        Level::Ok => ("✔", Color32::from_rgb(120, 220, 120)),
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(mark).color(col));
                        ui.label(RichText::new(msg).color(col));
                    });
                }
            });
            ui.separator();
            ui.label(RichText::new("Performance").strong());
            egui::Grid::new("perf").num_columns(3).striped(true).show(ui, |ui| {
                for (label, v, p, alert) in &stats {
                    ui.label(label);
                    ui.label(RichText::new(v.to_string()).color(if *alert { Color32::from_rgb(255, 160, 80) } else { Color32::WHITE }));
                    ui.label(RichText::new(p.as_ref().map(|(n, s)| format!("réserve du jeu « {n} » : {s}")).unwrap_or_default()).small().weak());
                    ui.end_row();
                }
            });
            ui.label(RichText::new("Tailles des réserves lues dans gameconfig.xml du jeu ; la carte du jeu en occupe déjà une part : orange au-delà de la moitié.").small().weak());
            let _ = V3::ZERO;
            let _ = POOLS;
        });
        self.check_open = open;
    }
}
