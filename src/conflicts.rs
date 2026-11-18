//! Conflits entre les MLO du projet (importés, créés, repris du jeu) et avec la carte du jeu, et leur réglage.
//!
//! - deux intérieurs qui se recouvrent : le jeu hésite sur la pièce où l'on est, les murs se traversent ;
//! - un intérieur du jeu au même endroit qu'un MLO : deux intérieurs l'un dans l'autre (scintillement) → caché ;
//! - objets du jeu qui traversent un intérieur (arbres, poteaux, murets…) → retirés (comme la gomme) ;
//! - fichiers de même nom dans deux ressources : FiveM n'en charge qu'un. Identiques : sans souci. Différents :
//!   l'un est renommé à l'export (références corrigées dans son .ytyp et ses cartes). Fichiers du jeu remplacés
//!   (façade, route, collision du quartier) par deux MLO : un seul est gardé, celui du MLO qui est à cet endroit ;
//! - le même nom d'intérieur défini deux fois : signalé.

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use eframe::egui::{self, Color32, RichText};
use std::path::Path;

/// Recouvrement (m³) à partir duquel deux intérieurs sont en conflit — **estimé**.
pub const OVERLAP_MIN: f32 = 2.0;
/// Objets du jeu examinés : diagonale de leur boîte au plus (m) ; au-delà ce sont des bâtiments (creux : on ne
/// peut pas juger à leur boîte) — **estimé**.
pub const CROSS_MAX_SIZE: f32 = 15.0;
/// Marge rentrée dans les pièces pour les objets qui les traversent (m) — **estimé**.
pub const CROSS_MARGIN: f32 = 0.25;

#[derive(Clone, Debug)]
pub enum Fix {
    None,
    /// renommer ce fichier de cette ressource à l'export (nom sans extension)
    Rename { res: String, stem: String, ext: String },
    /// fichier du jeu remplacé par plusieurs ressources : garder celle-ci, décocher chez les autres
    Pick { file: String, keep: String, drop: Vec<String> },
    /// retirer ces objets du jeu (placements du jeu)
    HideGame(Vec<u32>),
    /// décaler un intérieur (objets du MLO importé, ou placement de l'intérieur)
    Shift { ids: Vec<u64>, by: V3 },
}

#[derive(Clone, Debug)]
pub struct Conflict {
    /// 0 information, 1 à régler, 2 grave
    pub level: u8,
    pub text: String,
    pub at: V3,
    pub boxes: Vec<[V3; 8]>,
    pub fix: Fix,
}

impl Conflict {
    pub fn fix_label(&self) -> Option<&'static str> {
        match self.fix {
            Fix::None => None,
            Fix::Rename { .. } => Some("Renommer à l'export"),
            Fix::Pick { .. } => Some("Garder celui-là"),
            Fix::HideGame(_) => Some("Retirer du jeu"),
            Fix::Shift { .. } => Some("Décaler"),
        }
    }
}

/// Un intérieur placé dans le projet : nom, à qui il est (ressource importée ou « projet »), objets à déplacer
/// avec lui, pièces (coins dans le monde, boîte dans son repère), placement.
struct Placed {
    name: String,
    owner: String,
    ids: Vec<u64>,
    rooms: Vec<(V3, V3)>,
    pos: V3,
    rot: Quat,
    house: bool,
}

fn box_corners(a: V3, b: V3, p: V3, q: Quat) -> [V3; 8] {
    let mut c = [V3::ZERO; 8];
    for (i, x) in c.iter_mut().enumerate() {
        let l = v3(if i & 1 == 0 { a.x } else { b.x }, if i & 2 == 0 { a.y } else { b.y }, if i & 4 == 0 { a.z } else { b.z });
        *x = p + q.rotate(l);
    }
    c
}

fn aabb(c: &[V3; 8]) -> (V3, V3) {
    c.iter().fold((c[0], c[0]), |(a, b), p| (a.min(*p), b.max(*p)))
}

/// Volume commun de deux boîtes alignées.
pub fn overlap(a: (V3, V3), b: (V3, V3)) -> f32 {
    let d = |lo1: f32, hi1: f32, lo2: f32, hi2: f32| (hi1.min(hi2) - lo1.max(lo2)).max(0.0);
    d(a.0.x, a.1.x, b.0.x, b.1.x) * d(a.0.y, a.1.y, b.0.y, b.1.y) * d(a.0.z, a.1.z, b.0.z, b.1.z)
}

/// Décalage le plus court (en x ou en y) qui sépare la boîte `b` de la boîte `a`, plus `gap`.
pub fn separate(a: (V3, V3), b: (V3, V3), gap: f32) -> V3 {
    let cands = [v3(a.1.x - b.0.x + gap, 0.0, 0.0), v3(a.0.x - b.1.x - gap, 0.0, 0.0), v3(0.0, a.1.y - b.0.y + gap, 0.0), v3(0.0, a.0.y - b.1.y - gap, 0.0)];
    cands.into_iter().min_by(|x, y| x.len().total_cmp(&y.len())).unwrap_or(V3::ZERO)
}

/// Nom libre pour un fichier renommé : « nom_dw2 », « nom_dw3 »… (63 caractères au plus).
pub fn renamed(stem: &str, taken: &dyn Fn(&str) -> bool) -> String {
    for k in 2..100 {
        let suffix = format!("_dw{k}");
        let base: String = stem.chars().take(63 - suffix.len()).collect();
        let n = format!("{base}{suffix}");
        if !taken(&n) {
            return n;
        }
    }
    format!("{stem}_dwx")
}

/// Remplace un nom (contenu exact d'une balise, sans tenir compte des majuscules) dans un XML de CodeWalker, écrit
/// en clair ou en « hash_XXXXXXXX » (CodeWalker ne connaît pas les noms des créateurs).
pub fn replace_name_xml(xml: &str, old: &str, new: &str) -> (String, usize) {
    let (a, n1) = replace_name_xml_plain(xml, old, new);
    let (b, n2) = replace_name_xml_plain(&a, &format!("hash_{:08X}", crate::gta::crypto::joaat(&old.to_lowercase())), new);
    (b, n1 + n2)
}

fn replace_name_xml_plain(xml: &str, old: &str, new: &str) -> (String, usize) {
    let mut out = String::with_capacity(xml.len());
    let mut n = 0;
    let low = xml.to_lowercase();
    let pat = format!(">{}<", old.to_lowercase());
    let mut i = 0;
    while let Some(k) = low[i..].find(&pat) {
        let at = i + k;
        out.push_str(&xml[i..at]);
        out.push('>');
        out.push_str(new);
        out.push('<');
        i = at + pat.len();
        n += 1;
    }
    out.push_str(&xml[i..]);
    (out, n)
}

#[derive(Default)]
pub struct ConflictState {
    pub open: bool,
    pub list: Vec<Conflict>,
    pub checked: bool,
}

impl DwApp {
    fn placed_interiors(&self) -> Vec<Placed> {
        let mut out = Vec::new();
        for y in self.proj.ymaps.iter().filter(|y| !y.sample) {
            let owner = self.proj.mlo_res.iter().find(|r| r.ymaps.contains(&y.name));
            for e in y.entities.iter().filter(|e| e.mlo.is_some()) {
                let Some(m) = self.proj.find_mlo(&e.archetype) else { continue };
                let rooms: Vec<(V3, V3)> = m.rooms.iter().skip(1).filter(|r| (r.bb_max - r.bb_min).len() > 0.5).map(|r| (r.bb_min, r.bb_max)).collect();
                if rooms.is_empty() {
                    continue;
                }
                let ids = match owner {
                    Some(r) => self.proj.ymaps.iter().filter(|y2| r.ymaps.contains(&y2.name)).flat_map(|y2| y2.entities.iter().map(|x| x.id)).collect(),
                    None => vec![e.id],
                };
                let house = self.proj.houses.iter().any(|h| h.interior.as_deref().map(|n| n.eq_ignore_ascii_case(&m.name)).unwrap_or(false));
                out.push(Placed { name: m.name.clone(), owner: owner.map(|r| r.name.clone()).unwrap_or_else(|| "projet".into()), ids, rooms, pos: e.pos, rot: e.rot, house });
            }
        }
        out
    }

    /// Cherche tous les conflits.
    pub fn find_conflicts(&self) -> Vec<Conflict> {
        let mut out = Vec::new();
        let placed = self.placed_interiors();
        // 1. intérieurs qui se recouvrent
        for i in 0..placed.len() {
            for j in i + 1..placed.len() {
                let (a, b) = (&placed[i], &placed[j]);
                let ab: Vec<(V3, V3)> = a.rooms.iter().map(|r| aabb(&box_corners(r.0, r.1, a.pos, a.rot))).collect();
                let bb: Vec<(V3, V3)> = b.rooms.iter().map(|r| aabb(&box_corners(r.0, r.1, b.pos, b.rot))).collect();
                let vol: f32 = ab.iter().map(|x| bb.iter().map(|y| overlap(*x, *y)).sum::<f32>()).sum();
                if vol < OVERLAP_MIN {
                    continue;
                }
                let union = |l: &[(V3, V3)]| l.iter().fold((l[0].0, l[0].1), |(p, q), r| (p.min(r.0), q.max(r.1)));
                let (ua, ub) = (union(&ab), union(&bb));
                let by = separate(ua, ub, 1.0);
                let mut boxes: Vec<[V3; 8]> = a.rooms.iter().map(|r| box_corners(r.0, r.1, a.pos, a.rot)).collect();
                boxes.extend(b.rooms.iter().map(|r| box_corners(r.0, r.1, b.pos, b.rot)));
                out.push(Conflict {
                    level: 2,
                    text: format!("Les intérieurs « {} » ({}) et « {} » ({}) se recouvrent ({vol:.0} m³) : le jeu hésite sur la pièce où l'on est, les murs se traversent.", a.name, a.owner, b.name, b.owner),
                    at: (ub.0 + ub.1) * 0.5,
                    boxes,
                    fix: if b.house { Fix::None } else { Fix::Shift { ids: b.ids.clone(), by } },
                });
            }
        }
        // 2 et 3. avec la carte du jeu : intérieurs du jeu au même endroit, objets qui traversent
        if let Some(w) = self.game.world.as_deref() {
            // cartes du jeu remplacées par les ressources (elles s'occupent déjà de ces objets)
            let replaced: std::collections::HashSet<String> = self.proj.mlo_res.iter().flat_map(|r| r.assets.iter()).filter_map(|a| {
                let p = Path::new(a);
                (p.extension().map(|e| e.eq_ignore_ascii_case("ymap")).unwrap_or(false)).then(|| p.file_stem().map(|s| s.to_string_lossy().to_lowercase())).flatten()
            }).collect();
            let modified: std::collections::HashSet<u32> = self.proj.mlo_res.iter().flat_map(|r| r.assets.iter()).filter_map(|a| Path::new(a).file_stem().map(|s| crate::gta::crypto::joaat(&s.to_string_lossy().to_lowercase()))).collect();
            for p in &placed {
                let inv = p.rot.inverse();
                let world_boxes: Vec<(V3, V3)> = p.rooms.iter().map(|r| aabb(&box_corners(r.0, r.1, p.pos, p.rot))).collect();
                let (lo, hi) = world_boxes.iter().fold((world_boxes[0].0, world_boxes[0].1), |(a, b), r| (a.min(r.0), b.max(r.1)));
                let c = (lo + hi) * 0.5;
                let rad = (hi - lo).len() * 0.5;
                let my_hash = crate::gta::crypto::joaat(&p.name.to_lowercase());
                let mut crossing: Vec<u32> = Vec::new();
                let mut boxes = Vec::new();
                for ei in w.ents_in_radius(c, rad + 40.0) {
                    let e = &w.ents[ei as usize];
                    // objets détaillés : HD (0) et HD orphelins (5, la plupart des props)
                    if !(e.mlo || e.lod_level == 0 || e.lod_level == 5) || w.hidden_archs.contains(&e.arch) || self.game.hidden.contains(&ei) {
                        continue;
                    }
                    let ym = &w.ymaps[e.ymap as usize];
                    if ym.in_project || replaced.contains(&ym.name.to_lowercase()) || modified.contains(&e.arch) {
                        continue;
                    }
                    let corners = w.corners(ei);
                    if e.mlo {
                        // intérieur du jeu : pas celui que cet intérieur reprend (même nom, ou « nom_dw »)
                        let gname = w.name_of(e.arch).to_lowercase();
                        if e.arch == my_hash || p.name.to_lowercase().starts_with(&gname) {
                            continue;
                        }
                        // ses pièces (la boîte de son type est vide dans les fichiers du jeu)
                        let Some(def) = w.mlo_defs.get(&e.arch) else { continue };
                        let groom: Vec<[V3; 8]> = def.rooms.iter().skip(1).map(|r| box_corners(v3(r.bb_min[0], r.bb_min[1], r.bb_min[2]), v3(r.bb_max[0], r.bb_max[1], r.bb_max[2]), e.pos, e.rot())).collect();
                        let v: f32 = world_boxes.iter().map(|b| groom.iter().map(|g| overlap(*b, aabb(g))).sum::<f32>()).sum();
                        if v >= OVERLAP_MIN {
                            out.push(Conflict { level: 2, text: format!("L'intérieur du jeu « {gname} » est au même endroit que « {} » ({}) : deux intérieurs l'un dans l'autre ({v:.0} m³).", p.name, p.owner), at: e.pos, boxes: groom, fix: Fix::HideGame(vec![ei]) });
                        }
                        continue;
                    }
                    let (amn, amx) = w.arch_box(e.arch);
                    if (amx - amn).len() * e.sxy().max(e.sz()) > CROSS_MAX_SIZE {
                        continue;
                    }
                    // dans le repère de l'intérieur
                    let lc: Vec<V3> = corners.iter().map(|q| inv.rotate(*q - p.pos)).collect();
                    let (la, lb) = lc.iter().fold((lc[0], lc[0]), |(a, b), q| (a.min(*q), b.max(*q)));
                    let ctr = (la + lb) * 0.5;
                    let vol = ((lb.x - la.x) * (lb.y - la.y) * (lb.z - la.z)).max(1e-4);
                    let inside = p.rooms.iter().any(|r| {
                        let (a, b) = (r.0 + v3(CROSS_MARGIN, CROSS_MARGIN, CROSS_MARGIN), r.1 - v3(CROSS_MARGIN, CROSS_MARGIN, CROSS_MARGIN));
                        let in_c = ctr.x > a.x && ctr.x < b.x && ctr.y > a.y && ctr.y < b.y && ctr.z > a.z && ctr.z < b.z;
                        in_c || overlap((la, lb), (a, b)) > 0.5 * vol
                    });
                    if inside {
                        crossing.push(ei);
                        boxes.push(corners);
                    }
                }
                if std::env::var("DW_DEBUG_CONFLICTS").is_ok() {
                    println!("intérieur {} en {:?} : {} objets autour, {} qui traversent ; pièces {:?}", p.name, p.pos, w.ents_in_radius(c, rad + 40.0).len(), crossing.len(), p.rooms);
                }
                if !crossing.is_empty() {
                    let names: Vec<String> = {
                        let mut v: Vec<String> = crossing.iter().map(|&ei| w.name_of(w.ents[ei as usize].arch)).collect();
                        v.sort();
                        v.dedup();
                        v.into_iter().take(6).collect()
                    };
                    out.push(Conflict { level: 1, text: format!("{} objet(s) de la carte du jeu traversent l'intérieur « {} » ({}) : {}.", crossing.len(), p.name, p.owner, names.join(", ")), at: p.pos, boxes, fix: Fix::HideGame(crossing) });
                }
            }
        }
        // 4. fichiers de même nom dans plusieurs ressources
        let mut files: std::collections::BTreeMap<String, Vec<(usize, String)>> = std::collections::BTreeMap::new();
        for (ri, r) in self.proj.mlo_res.iter().enumerate() {
            for a in r.assets.iter().chain(r.ybns.iter()) {
                if let Some(n) = Path::new(a).file_name() {
                    let n = n.to_string_lossy().to_lowercase();
                    if n == "_manifest.ymf" {
                        continue;
                    }
                    files.entry(n).or_default().push((ri, a.clone()));
                }
            }
            // fichiers de types : sous leur nom d'export (renommés à l'import si besoin)
            for (n, f) in r.ytyp_files.iter().filter(|f| !f.1.is_empty()) {
                files.entry(format!("{}.ytyp", n.to_lowercase())).or_default().push((ri, f.clone()));
            }
        }
        let w = self.game.world.as_deref();
        let mut identical: Vec<String> = Vec::new();
        for (file, list) in files.iter().filter(|(_, l)| {
            let mut rs: Vec<usize> = l.iter().map(|x| x.0).collect();
            rs.dedup();
            rs.len() > 1
        }) {
            let data: Vec<Option<Vec<u8>>> = list.iter().map(|(_, p)| std::fs::read(p).ok()).collect();
            let same = data.iter().all(|d| d.is_some() && *d == data[0]);
            let names: Vec<String> = list.iter().map(|(ri, _)| self.proj.mlo_res[*ri].name.clone()).collect();
            let p = Path::new(file);
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let ext = p.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            if same {
                identical.push(file.clone());
                continue;
            }
            // intérieur défini deux fois (collision de l'intérieur au même nom) : pas renommable
            let interior = ext == "ybn" && self.proj.ytyps.iter().flat_map(|t| t.mlos.iter()).any(|m| m.name.eq_ignore_ascii_case(&stem));
            if crate::mlo::is_vanilla_name(w, file) {
                // fichier du jeu remplacé : gardé chez le MLO le plus proche de son placement dans le jeu
                let h = crate::gta::crypto::joaat(&stem.to_lowercase());
                let spot = w.and_then(|w| w.ents.iter().find(|e| e.arch == h).map(|e| e.pos));
                let keep = list
                    .iter()
                    .map(|(ri, _)| *ri)
                    .min_by(|a, b| {
                        let d = |ri: usize| spot.map(|s| (self.proj.mlo_res[ri].ref_pos - s).len()).unwrap_or(ri as f32);
                        d(*a).total_cmp(&d(*b))
                    })
                    .unwrap_or(list[0].0);
                let keep_name = self.proj.mlo_res[keep].name.clone();
                let drop: Vec<String> = names.iter().filter(|n| **n != keep_name).cloned().collect();
                out.push(Conflict {
                    level: 1,
                    text: format!("« {file} » (fichier du jeu modifié) est remplacé par {} ressources ({}) : FiveM n'en garde qu'une version, les modifications des autres sont perdues. À garder : celle de « {keep_name} », le MLO le plus proche de cet endroit.", names.len(), names.join(", ")),
                    at: spot.unwrap_or(V3::ZERO),
                    boxes: Vec::new(),
                    fix: Fix::Pick { file: file.clone(), keep: keep_name, drop },
                });
            } else if interior {
                out.push(Conflict { level: 2, text: format!("« {file} » : deux ressources ({}) définissent l'intérieur « {stem} » avec des collisions différentes : FiveM n'en charge qu'un. Retire l'une des deux ressources.", names.join(", ")), at: V3::ZERO, boxes: Vec::new(), fix: Fix::None });
            } else {
                // fichier du créateur de même nom : le second renommé à l'export
                let res = names.last().cloned().unwrap_or_default();
                let already = self.proj.mlo_res.iter().find(|r| r.name == res).map(|r| r.renames.iter().any(|x| x.0 == stem.to_lowercase())).unwrap_or(false);
                if already {
                    continue;
                }
                out.push(Conflict { level: 1, text: format!("« {file} » existe dans {} ressources ({}) avec un contenu différent : FiveM n'en charge qu'un, l'autre MLO aurait de mauvais modèles ou textures. Réglage : celui de « {res} » est renommé à l'export (références corrigées).", names.len(), names.join(", ")), at: V3::ZERO, boxes: Vec::new(), fix: Fix::Rename { res, stem, ext } });
            }
        }
        if !identical.is_empty() {
            out.push(Conflict { level: 0, text: format!("{} fichier(s) sont dans plusieurs ressources mais identiques (FiveM n'en charge qu'un, pas de souci) : {}{}.", identical.len(), identical.iter().take(8).cloned().collect::<Vec<_>>().join(", "), if identical.len() > 8 { "…" } else { "" }), at: V3::ZERO, boxes: Vec::new(), fix: Fix::None });
        }
        // 5. le même intérieur défini deux fois
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for t in &self.proj.ytyps {
            for m in &t.mlos {
                let k = m.name.to_lowercase();
                if let Some(prev) = seen.get(&k) {
                    if prev != &t.name {
                        out.push(Conflict { level: 2, text: format!("L'intérieur « {} » est défini deux fois (dans « {prev} » et « {} ») : le jeu n'en garde qu'un.", m.name, t.name), at: V3::ZERO, boxes: Vec::new(), fix: Fix::None });
                    }
                } else {
                    seen.insert(k, t.name.clone());
                }
            }
        }
        out.sort_by(|a, b| b.level.cmp(&a.level));
        out
    }

    /// Applique un réglage. Rend le compte rendu.
    pub fn apply_fix(&mut self, fix: &Fix) -> String {
        match fix {
            Fix::None => "Rien à régler automatiquement.".into(),
            Fix::Rename { res, stem, ext } => {
                let taken_names: std::collections::HashSet<String> = self.proj.mlo_res.iter().flat_map(|r| r.assets.iter().chain(r.ybns.iter())).filter_map(|a| Path::new(a).file_stem().map(|s| s.to_string_lossy().to_lowercase())).chain(self.proj.ymaps.iter().map(|y| y.name.to_lowercase())).collect();
                let new = renamed(&stem.to_lowercase(), &|n| taken_names.contains(n));
                self.push_undo();
                if ext == "ymap" {
                    // carte du projet : renommée tout de suite
                    for y in self.proj.ymaps.iter_mut().filter(|y| y.name.eq_ignore_ascii_case(stem)) {
                        y.name = new.clone();
                    }
                    if let Some(r) = self.proj.mlo_res.iter_mut().find(|r| &r.name == res) {
                        for y in r.ymaps.iter_mut().filter(|y| y.eq_ignore_ascii_case(stem)) {
                            *y = new.clone();
                        }
                    }
                } else if let Some(r) = self.proj.mlo_res.iter_mut().find(|r| &r.name == res) {
                    r.renames.retain(|x| x.0 != stem.to_lowercase());
                    r.renames.push((stem.to_lowercase(), new.clone()));
                }
                format!("« {stem}.{ext} » de « {res} » sera exporté sous le nom « {new}.{ext} » (ses références corrigées).")
            }
            Fix::Pick { file, keep, drop } => {
                self.push_undo();
                for r in self.proj.mlo_res.iter_mut().filter(|r| drop.contains(&r.name)) {
                    r.set_pick(file, false);
                }
                format!("« {file} » : gardé chez « {keep} », décoché chez {}.", drop.join(", "))
            }
            Fix::HideGame(list) => {
                self.push_undo();
                let ids: std::collections::HashSet<u64> = self.game_ents_to_project(list).into_iter().collect();
                let n = self.proj.delete_ids(&ids);
                self.sync_game_project();
                format!("{n} objet(s) du jeu retiré(s) (ils disparaissent aussi en jeu une fois exporté ; Ctrl+Z pour les remettre).")
            }
            Fix::Shift { ids, by } => {
                self.push_undo();
                let set: std::collections::HashSet<u64> = ids.iter().copied().collect();
                let mut n = 0;
                for y in self.proj.ymaps.iter_mut() {
                    for e in y.entities.iter_mut().filter(|e| set.contains(&e.id)) {
                        e.pos = e.pos + *by;
                        n += 1;
                    }
                }
                self.sync_game_project();
                format!("{n} objet(s) décalé(s) de {:.1} m pour que les intérieurs ne se recouvrent plus.", by.len())
            }
        }
    }

    /// Règle tous les conflits qui se règlent seuls (pas les décalages : c'est à toi de choisir où va un MLO).
    pub fn fix_all_conflicts(&mut self) -> String {
        let list = self.find_conflicts();
        let mut done = Vec::new();
        for c in list.iter().filter(|c| !matches!(c.fix, Fix::None | Fix::Shift { .. })) {
            done.push(self.apply_fix(&c.fix));
        }
        let left = self.find_conflicts().into_iter().filter(|c| c.level > 0).count();
        format!("{} conflit(s) réglé(s). {}{}", done.len(), if left > 0 { format!("{left} restent (à voir dans la fenêtre des conflits). ") } else { "Plus aucun conflit. ".into() }, done.join(" "))
    }

    /// Importe tous les MLO d'un dossier (chaque sous-dossier qui contient un fxmanifest.lua ou __resource.lua),
    /// puis cherche les conflits.
    pub fn import_mlo_folder(&mut self, dir: &Path) -> String {
        let mut found: Vec<std::path::PathBuf> = Vec::new();
        // une ressource MLO : un manifeste et au moins un .ytyp (les scripts et les cartes seules sont laissés)
        fn has_ytyp(d: &Path, depth: u32) -> bool {
            let Ok(rd) = std::fs::read_dir(d) else { return false };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if depth > 0 && has_ytyp(&p, depth - 1) {
                        return true;
                    }
                } else if p.extension().map(|x| x.eq_ignore_ascii_case("ytyp")).unwrap_or(false) || p.to_string_lossy().to_lowercase().ends_with(".ytyp.xml") {
                    return true;
                }
            }
            false
        }
        fn walk(d: &Path, depth: u32, out: &mut Vec<std::path::PathBuf>) {
            if d.join("fxmanifest.lua").is_file() || d.join("__resource.lua").is_file() {
                if has_ytyp(d, 4) {
                    out.push(d.to_path_buf());
                }
                return;
            }
            if depth == 0 {
                return;
            }
            let Ok(rd) = std::fs::read_dir(d) else { return };
            let mut subs: Vec<std::path::PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
            subs.sort();
            for s in subs {
                walk(&s, depth - 1, out);
            }
        }
        walk(dir, 4, &mut found);
        if found.is_empty() {
            return "Aucune ressource MLO dans ce dossier (il faut des dossiers avec un fxmanifest.lua et au moins un .ytyp).".into();
        }
        let mut ok = 0;
        let mut msgs = Vec::new();
        for d in &found {
            let m = self.import_mlo(d);
            if m.contains("importé") {
                ok += 1;
            } else {
                msgs.push(format!("{} : {m}", d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
            }
        }
        self.conflicts.list = self.find_conflicts();
        self.conflicts.checked = true;
        let nc = self.conflicts.list.iter().filter(|c| c.level > 0).count();
        if nc > 0 {
            self.conflicts.open = true;
        }
        format!("{ok} ressource(s) MLO importée(s) sur {}. {} {}", found.len(), if nc > 0 { format!("{nc} conflit(s) trouvé(s) : fenêtre « Conflits des MLO » ouverte.") } else { "Aucun conflit.".into() }, msgs.join(" ; "))
    }

    pub fn import_mlo_folder_dialog(&mut self) {
        if let Some(d) = rfd::FileDialog::new().set_title("Dossier qui contient des ressources MLO (ex. [mlo])").pick_folder() {
            self.status = self.import_mlo_folder(&d);
        }
    }

    /// Fenêtre « Conflits des MLO ».
    pub fn conflicts_window(&mut self, ctx: &egui::Context) {
        if !self.conflicts.open {
            return;
        }
        if !self.conflicts.checked {
            self.conflicts.list = self.find_conflicts();
            self.conflicts.checked = true;
        }
        let mut open = true;
        let mut action: Option<(usize, bool)> = None; // (conflit, régler ? sinon aller voir)
        let mut all = false;
        let mut recheck = false;
        egui::Window::new("Conflits des MLO").open(&mut open).default_width(560.0).show(ctx, |ui| {
            ui.label(RichText::new("Intérieurs qui se recouvrent, intérieurs du jeu au même endroit, objets du jeu qui traversent un intérieur, fichiers de même nom dans plusieurs ressources. Les boîtes rouges les montrent dans la vue.").small().weak());
            ui.horizontal(|ui| {
                if ui.button("Vérifier de nouveau").clicked() {
                    recheck = true;
                }
                let n = self.conflicts.list.iter().filter(|c| !matches!(c.fix, Fix::None | Fix::Shift { .. })).count();
                if ui.add_enabled(n > 0, egui::Button::new(format!("Tout régler ({n})"))).on_hover_text("Renomme les fichiers en double à l'export, garde la bonne version des fichiers du jeu modifiés, retire les objets et intérieurs du jeu qui gênent. Les intérieurs qui se recouvrent restent à déplacer toi-même (bouton « Décaler »). Ctrl+Z pour revenir.").clicked() {
                    all = true;
                }
            });
            ui.separator();
            if self.conflicts.list.is_empty() {
                ui.label(RichText::new("Aucun conflit.").color(Color32::from_rgb(120, 220, 140)));
            }
            egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                for (i, c) in self.conflicts.list.iter().enumerate() {
                    let col = match c.level {
                        2 => Color32::from_rgb(255, 120, 100),
                        1 => Color32::from_rgb(255, 200, 90),
                        _ => Color32::from_rgb(150, 200, 255),
                    };
                    ui.label(RichText::new(&c.text).color(col));
                    ui.horizontal(|ui| {
                        if c.at != V3::ZERO && ui.small_button("Aller voir").clicked() {
                            action = Some((i, false));
                        }
                        if let Some(l) = c.fix_label() {
                            if ui.small_button(l).clicked() {
                                action = Some((i, true));
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
            });
        });
        if all {
            self.status = self.fix_all_conflicts();
            recheck = true;
        }
        if let Some((i, fix)) = action {
            if let Some(c) = self.conflicts.list.get(i).cloned() {
                if fix {
                    self.status = self.apply_fix(&c.fix);
                    recheck = true;
                } else {
                    self.goto(c.at, 40.0);
                }
            }
        }
        if recheck {
            self.conflicts.list = self.find_conflicts();
        }
        if !open {
            self.conflicts.open = false;
            self.conflicts.checked = false;
        }
    }

    /// Boîtes rouges des conflits dans la vue (fenêtre ouverte).
    pub fn draw_conflicts(&self, painter: &egui::Painter) {
        if !self.conflicts.open {
            return;
        }
        for c in &self.conflicts.list {
            let col = if c.level >= 2 { Color32::from_rgb(255, 70, 60) } else { Color32::from_rgb(255, 170, 40) };
            for b in c.boxes.iter().take(200) {
                crate::render::draw_box_edges(painter, &self.cam, b, egui::Stroke::new(2.0_f32, col));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recouvrement_et_decalage() {
        let a = (v3(0.0, 0.0, 0.0), v3(10.0, 8.0, 3.0));
        let b = (v3(6.0, 2.0, 0.0), v3(14.0, 6.0, 3.0));
        assert!((overlap(a, b) - 4.0 * 4.0 * 3.0).abs() < 1e-3);
        let s = separate(a, b, 1.0);
        assert!((s - v3(5.0, 0.0, 0.0)).len() < 1e-4, "{s:?}");
        let b2 = (b.0 + s, b.1 + s);
        assert_eq!(overlap(a, b2), 0.0);
    }

    #[test]
    fn renommer_dans_le_xml() {
        let x = "<Item><name>Ma_Porte</name><assetName>ma_porte</assetName><textureDictionary>ma_porte_txd</textureDictionary></Item>";
        let (y, n) = replace_name_xml(x, "ma_porte", "ma_porte_dw2");
        assert_eq!(n, 2);
        assert!(y.contains("<name>ma_porte_dw2</name>") && y.contains("<textureDictionary>ma_porte_txd</textureDictionary>"));
        assert_eq!(renamed("abc", &|n| n == "abc_dw2"), "abc_dw3");
        let h = format!("<textureDictionary>hash_{:08X}</textureDictionary>", crate::gta::crypto::joaat("ma_porte_txd"));
        let (z, n) = replace_name_xml(&h, "ma_porte_txd", "ma_porte_txd_dw2");
        assert_eq!((z.as_str(), n), ("<textureDictionary>ma_porte_txd_dw2</textureDictionary>", 1));
    }
}
