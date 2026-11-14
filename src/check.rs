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
    files.sort_by_key(|f| !f.path().contains("update"));
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

    /// Contrôle du contenu du projet (hors intérieurs) : modèles inconnus, doublons, PNJ et circulation.
    pub fn project_checks(&self) -> Vec<(Level, String)> {
        let mut out: Vec<(Level, String)> = Vec::new();
        let ymaps: Vec<&crate::model::Ymap> = self.proj.ymaps.iter().filter(|y| !y.sample).collect();
        // --- modèles inconnus : ni dans le jeu, ni dans un .ytyp du projet
        if let Some(w) = self.game.world.as_deref() {
            let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
            for t in &self.proj.ytyps {
                for m in &t.mlos {
                    known.insert(m.name.to_lowercase());
                }
                for r in &t.raw_archetypes {
                    let mut rest = r.as_str();
                    while let Some(p) = rest.to_lowercase().find("<name>") {
                        let after = &rest[p + 6..];
                        let Some(e) = after.to_lowercase().find("</name>") else { break };
                        known.insert(after[..e].trim().to_lowercase());
                        rest = &after[e..];
                    }
                }
            }
            // modèles personnels (Sollumz) : types des .ytyp.xml du dossier d'échange, copiés à l'export
            // (copy_custom_models) ; un .ydr sans type dans un .ytyp reste signalé
            known.extend(self.sollumz.archetypes.keys().map(|k| k.to_lowercase()));
            let mut unknown: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
            for y in &ymaps {
                for e in &y.entities {
                    let low = e.archetype.to_lowercase();
                    // « dw_… » : modèles fabriqués par DragoWalker à l'export ; « hash_XXXXXXXX » : nom inconnu, écrit par son code
                    if low.starts_with("dw_") || low.is_empty() || known.contains(&low) {
                        continue;
                    }
                    let h = low.strip_prefix("hash_").and_then(|x| u32::from_str_radix(x, 16).ok()).unwrap_or_else(|| crate::gta::crypto::joaat(&low));
                    if !w.archetypes.contains_key(&h) {
                        *unknown.entry(e.archetype.clone()).or_insert(0) += 1;
                    }
                }
            }
            if unknown.is_empty() {
                out.push((Level::Ok, "Tous les modèles posés existent dans le jeu ou dans un .ytyp du projet.".into()));
            } else {
                let ex: Vec<String> = unknown.iter().take(5).map(|(n, c)| format!("{n} ({c})")).collect();
                out.push((Level::Warning, format!("{} modèle(s) inconnu(s) du jeu et du projet : {}{} — ils n'apparaîtront pas en jeu.", unknown.len(), ex.join(", "), if unknown.len() > 5 { "…" } else { "" })));
            }
        }
        // --- deux .ytyp du projet qui déclarent le même intérieur
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for t in &self.proj.ytyps {
            for m in &t.mlos {
                if let Some(other) = seen.insert(m.name.to_lowercase(), t.name.clone()) {
                    out.push((Level::Error, format!("L'intérieur « {} » est déclaré dans deux .ytyp du projet ({other} et {}) : le jeu garde un seul des deux.", m.name, t.name)));
                }
            }
        }
        // --- PNJ : la réserve des PNJ a 256 places, partagée avec ceux que le jeu fait apparaître
        let peds: Vec<crate::math::V3> = ymaps.iter().flat_map(|y| y.peds.iter().map(|p| p.pos)).collect();
        if !peds.is_empty() {
            let near = max_in_radius(&peds, 150.0);
            let (lvl, msg) = if near >= 200 {
                (Level::Error, format!("{near} PNJ dans un même cercle de 150 m : la réserve des PNJ (256 places, partagée avec la population du jeu) sera pleine, le jeu plante (« Pool Full »)."))
            } else if near > 50 {
                (Level::Warning, format!("{near} PNJ dans un même cercle de 150 m : risque de réserve des PNJ pleine (256 places, partagée avec la population du jeu) — **estimé**, à tester."))
            } else {
                (Level::Ok, format!("PNJ : {} posé(s), au plus {near} dans un cercle de 150 m.", peds.len()))
            };
            out.push((lvl, msg));
        }
        if let Some(t) = &self.proj.terrain {
            if t.npc_traffic.enabled && t.npc_traffic.count > 30 {
                out.push((Level::Warning, format!("Circulation : {} voitures de PNJ en même temps — chacune a son conducteur dans la réserve des PNJ (256 places) : risque de « Pool Full ».", t.npc_traffic.count)));
            }
        }
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
        for (lvl, msg) in self.project_checks() {
            if lvl != Level::Ok {
                out.push(format!("Contrôle du projet : {msg}"));
            }
        }
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
        let proj_checks = self.project_checks();
        let folder_report = self.check_folder_report.clone();
        let mut pick_folder = false;
        self.show_panel(ctx, "controle", "Contrôle avant l'export", crate::docking::Dock::Floating, [560.0, 520.0], &mut open, |s, ui| {
            ui.label(RichText::new("Projet").strong());
            for (lvl, msg) in &proj_checks {
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
            ui.separator();
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
            // portails près de la limite : les réunir (meubler.rs)
            if stats.iter().any(|(_, _, p, alert)| *alert && p.as_ref().map(|p| p.0 == "PortalInst").unwrap_or(false)) {
                s.portals_ui(ui, None);
            }
            ui.separator();
            ui.label(RichText::new("Dossier de ressource FiveM").strong());
            if ui.button("Contrôler un dossier…").on_hover_text("Vérifie une ressource déjà exportée (la tienne ou une autre) : fichiers .xml ou .zip oubliés dans stream/, fichiers vides, fichiers déclarés absents").clicked() {
                pick_folder = true;
            }
            for (lvl, msg) in &folder_report {
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
            let _ = V3::ZERO;
            let _ = POOLS;
        });
        self.check_open = open;
        if pick_folder {
            if let Some(d) = rfd::FileDialog::new().set_title("Dossier d'une ressource FiveM (avec fxmanifest.lua)").pick_folder() {
                self.check_folder_report = scan_resource_dir(&d);
            }
        }
    }
}

/// Plus grand nombre de points dans un même cercle de rayon `r` centré sur l'un d'eux (plan XY).
pub fn max_in_radius(pts: &[V3], r: f32) -> usize {
    let r2 = r * r;
    pts.iter().map(|a| pts.iter().filter(|b| (b.x - a.x).powi(2) + (b.y - a.y).powi(2) <= r2).count()).max().unwrap_or(0)
}

/// Contrôle d'un dossier de ressource FiveM exporté (ce que le jeu ne pourra pas charger).
pub fn scan_resource_dir(dir: &std::path::Path) -> Vec<(Level, String)> {
    let mut out: Vec<(Level, String)> = Vec::new();
    let manifest = dir.join("fxmanifest.lua");
    let man_text = std::fs::read_to_string(&manifest).or_else(|_| std::fs::read_to_string(dir.join("__resource.lua")));
    let Ok(man) = man_text else {
        out.push((Level::Error, format!("Pas de fxmanifest.lua dans {} : FiveM ne reconnaît pas ce dossier comme une ressource.", dir.display())));
        return out;
    };
    // fichiers du dossier stream/
    let stream = dir.join("stream");
    let mut xml: Vec<String> = Vec::new();
    let mut archives: Vec<String> = Vec::new();
    let mut empty: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut stack = vec![stream.clone()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            total += 1;
            let name = e.file_name().to_string_lossy().to_lowercase();
            let rel = p.strip_prefix(&stream).unwrap_or(&p).display().to_string();
            if name.ends_with(".xml") {
                xml.push(rel.clone());
            } else if name.ends_with(".zip") || name.ends_with(".rar") || name.ends_with(".7z") {
                archives.push(rel.clone());
            }
            if e.metadata().map(|m| m.len() == 0).unwrap_or(false) {
                empty.push(rel);
            }
        }
    }
    let list = |v: &Vec<String>| v.iter().take(4).cloned().collect::<Vec<_>>().join(", ") + if v.len() > 4 { "…" } else { "" };
    if !stream.is_dir() {
        out.push((Level::Warning, "Pas de dossier stream/ : cette ressource ne diffuse aucun fichier de carte ou de modèle.".into()));
    }
    if !xml.is_empty() {
        out.push((Level::Error, format!("{} fichier(s) .xml dans stream/ ({}) : le jeu ne les charge pas. Il faut les convertir en fichiers binaires (CodeWalker / Sollumz) ; c'est ce qui arrive quand CodeWalker est introuvable pendant l'export.", xml.len(), list(&xml))));
    }
    if !archives.is_empty() {
        out.push((Level::Error, format!("{} archive(s) dans stream/ ({}) : FiveM ne les ouvre pas. Décompresse-les.", archives.len(), list(&archives))));
    }
    if !empty.is_empty() {
        out.push((Level::Error, format!("{} fichier(s) vide(s) dans stream/ ({}) : le jeu peut planter en les lisant.", empty.len(), list(&empty))));
    }
    // fichiers .ytyp déclarés par data_file mais absents
    let mut missing: Vec<String> = Vec::new();
    for line in man.lines() {
        if line.contains("data_file") {
            let parts: Vec<&str> = line.split(['\'', '"']).filter(|s| !s.trim().is_empty() && !s.contains("data_file")).collect();
            if let Some(f) = parts.get(1) {
                // données audio : déclarées par leur nom de base « x.dat », le fichier est « x.dat151.rel » (jeu),
                // « x.dat54.rel » (sons)… — FiveM ajoute le numéro et « .rel »
                let audio_rel = |f: &str| -> bool {
                    let p = dir.join(f);
                    let (Some(parent), Some(base)) = (p.parent(), p.file_name().map(|n| n.to_string_lossy().to_lowercase())) else { return false };
                    base.ends_with(".dat") && std::fs::read_dir(parent).into_iter().flatten().flatten().any(|e| {
                        let n = e.file_name().to_string_lossy().to_lowercase();
                        n.starts_with(&base) && n.ends_with(".rel") && n[base.len()..n.len() - 4].chars().all(|c| c.is_ascii_digit())
                    })
                };
                if f.contains('/') && !f.contains('*') && !dir.join(f).is_file() && !audio_rel(f) {
                    missing.push(f.to_string());
                }
            }
        }
    }
    if !missing.is_empty() {
        out.push((Level::Error, format!("Le fxmanifest.lua déclare {} fichier(s) absent(s) : {}.", missing.len(), list(&missing))));
    }
    if out.is_empty() {
        out.push((Level::Ok, format!("Rien d'anormal dans « {} » ({total} fichier(s) dans stream/).", dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::v3;

    #[test]
    fn cercle_le_plus_peuple() {
        let mut pts: Vec<V3> = (0..60).map(|i| v3(i as f32, 0.0, 0.0)).collect(); // 60 PNJ sur 60 m
        pts.push(v3(5000.0, 0.0, 0.0));
        assert_eq!(max_in_radius(&pts, 150.0), 60);
        assert_eq!(max_in_radius(&[], 150.0), 0);
    }

    #[test]
    fn dossier_de_ressource() {
        let d = std::env::temp_dir().join(format!("dw_scan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("stream")).unwrap();
        // pas de manifeste
        assert!(scan_resource_dir(&d).iter().any(|r| r.0 == Level::Error && r.1.contains("fxmanifest")));
        std::fs::write(d.join("fxmanifest.lua"), "fx_version 'cerulean'\nfiles { 'stream/*.ytyp' }\ndata_file 'DLC_ITYP_REQUEST' 'stream/a.ytyp'\ndata_file 'DLC_ITYP_REQUEST' 'stream/absent.ytyp'\n").unwrap();
        std::fs::write(d.join("stream").join("a.ytyp"), b"RSC7data").unwrap();
        std::fs::write(d.join("stream").join("prop.yft.xml"), b"<x/>").unwrap();
        std::fs::write(d.join("stream").join("pack.zip"), b"PK").unwrap();
        std::fs::write(d.join("stream").join("vide.ybn"), b"").unwrap();
        let r = scan_resource_dir(&d);
        assert!(r.iter().any(|x| x.1.contains("1 fichier(s) .xml") && x.1.contains("prop.yft.xml")));
        assert!(r.iter().any(|x| x.1.contains("archive") && x.1.contains("pack.zip")));
        assert!(r.iter().any(|x| x.1.contains("vide") && x.1.contains("vide.ybn")));
        assert!(r.iter().any(|x| x.1.contains("absent.ytyp")), "{r:?}");
        assert!(!r.iter().any(|x| x.1.contains("a.ytyp,")));
        // dossier propre
        std::fs::remove_file(d.join("stream").join("prop.yft.xml")).unwrap();
        std::fs::remove_file(d.join("stream").join("pack.zip")).unwrap();
        std::fs::remove_file(d.join("stream").join("vide.ybn")).unwrap();
        std::fs::write(d.join("stream").join("absent.ytyp"), b"RSC7data").unwrap();
        let ok = scan_resource_dir(&d);
        assert!(ok.len() == 1 && ok[0].0 == Level::Ok, "{ok:?}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
