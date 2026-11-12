//! Éditer un bâtiment du jeu posé dans le projet : retirer à la main ce qui ne plaît pas (les « restes » d'un
//! pâté de maisons : un modèle du jeu contient souvent plusieurs bâtiments voisins).
//!
//! - « Éditer ce bâtiment » (sélection d'une copie d'un modèle .ydr du jeu) : la copie devient un modèle découpé
//!   « dw_cut_N » (les autres copies du même bâtiment ne changent pas), vue seule ;
//!   - clic : retire le morceau sous la souris (triangles d'une même partie du modèle reliés par leurs sommets),
//!     surligné en rouge avant le clic ;
//!   - glisser un rectangle : retire tout ce dont le centre tombe dedans, même derrière ; Ctrl : remet ;
//!   - Ctrl+Z annule.
//! - Le projet garde, pour chaque modèle découpé, le centre (repère du modèle) de chaque triangle retiré.
//! - Vue : le modèle du jeu sans ces triangles est envoyé à la carte graphique sous le nom du modèle découpé.
//! - Export : le .ydr du jeu est relu en XML par CodeWalker, chaque triangle (de tous les niveaux de détail) est
//!   gardé si le triangle détaillé le plus proche de son centre est gardé, puis converti sous le nouveau nom
//!   (textures du jeu), avec son type (.ytyp « dw_cut_types ») et une collision générée sur ce qui reste.

use crate::app::DwApp;
use crate::gta::loader::Loaded;
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, Pos2, RichText};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Un modèle du jeu découpé (gardé dans le projet).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BuildingCut {
    /// nom du modèle découpé (« dw_cut_N »), porté par les objets du projet
    pub name: String,
    /// modèle du jeu d'origine
    pub source: String,
    /// centre (repère du modèle) de chaque triangle retiré
    pub removed: Vec<[f32; 3]>,
    /// ouvertures percées dans les murs (portes, fenêtres) : voir openings.rs
    #[serde(default)]
    pub openings: Vec<crate::openings::Opening>,
}

/// Tolérance pour reconnaître un triangle retiré par son centre (m) — **estimé**.
pub const CUT_MATCH: f32 = 0.01;
/// Préfixe des modèles découpés.
pub const CUT_PREFIX: &str = "dw_cut_";

/// Centres des triangles retirés, rangés par case de 5 cm.
pub struct RemovedSet {
    cells: HashMap<(i32, i32, i32), Vec<V3>>,
}

const RS_CELL: f32 = 0.05;

impl RemovedSet {
    pub fn new(list: &[[f32; 3]]) -> RemovedSet {
        let mut cells: HashMap<(i32, i32, i32), Vec<V3>> = HashMap::new();
        for p in list {
            let v = v3(p[0], p[1], p[2]);
            cells.entry(cell(v, RS_CELL)).or_default().push(v);
        }
        RemovedSet { cells }
    }

    pub fn contains(&self, c: V3) -> bool {
        let (x, y, z) = cell(c, RS_CELL);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(v) = self.cells.get(&(x + dx, y + dy, z + dz)) {
                        if v.iter().any(|p| (*p - c).len() <= CUT_MATCH) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

fn cell(v: V3, s: f32) -> (i32, i32, i32) {
    ((v.x / s).floor() as i32, (v.y / s).floor() as i32, (v.z / s).floor() as i32)
}

/// Classe n'importe quel triangle (y compris des niveaux de détail plus grossiers) d'après le triangle détaillé
/// dont le centre est le plus proche : gardé ou retiré.
pub struct Classifier {
    cells: HashMap<(i32, i32, i32), Vec<(V3, bool)>>,
}

const CL_CELL: f32 = 1.0;

impl Classifier {
    pub fn new(centers: &[V3], kept: &[bool]) -> Classifier {
        let mut cells: HashMap<(i32, i32, i32), Vec<(V3, bool)>> = HashMap::new();
        for (c, k) in centers.iter().zip(kept.iter()) {
            cells.entry(cell(*c, CL_CELL)).or_default().push((*c, *k));
        }
        Classifier { cells }
    }

    /// Gardé si le triangle détaillé le plus proche (jusqu'à 8 m) l'est ; rien autour : gardé.
    pub fn keep(&self, c: V3) -> bool {
        let (x, y, z) = cell(c, CL_CELL);
        for r in 0..=8i32 {
            let mut best: Option<(f32, bool)> = None;
            for dx in -r..=r {
                for dy in -r..=r {
                    for dz in -r..=r {
                        if dx.abs().max(dy.abs()).max(dz.abs()) != r {
                            continue;
                        }
                        if let Some(v) = self.cells.get(&(x + dx, y + dy, z + dz)) {
                            for (p, k) in v {
                                let d = (*p - c).len();
                                if best.map(|b| d < b.0).unwrap_or(true) {
                                    best = Some((d, *k));
                                }
                            }
                        }
                    }
                }
            }
            // un point d'une case voisine peut être plus proche qu'un point trouvé à cet anneau : un anneau de plus
            if let Some((d, k)) = best {
                if d <= r as f32 * CL_CELL {
                    return k;
                }
            }
        }
        true
    }
}

pub fn clone_loaded(l: &Loaded) -> Loaded {
    Loaded { arch: l.arch, model: l.model.clone(), part_keys: l.part_keys.clone(), layer_keys: l.layer_keys.clone(), textures: l.textures.clone() }
}

/// Triangles d'un modèle : (sommets, partie, centre).
pub fn model_tris(l: &Loaded) -> Vec<([u32; 3], u16, V3)> {
    let mut out = Vec::new();
    let Some(m) = l.model.as_ref() else { return out };
    let p = |i: u32| m.verts.get(i as usize).map(|v| v3(v.pos[0], v.pos[1], v.pos[2])).unwrap_or(V3::ZERO);
    for (gi, g) in m.geoms.iter().enumerate() {
        let end = (g.first + g.count).min(m.idx.len() as u32);
        let mut i = g.first;
        while i + 2 < end {
            let t = [m.idx[i as usize], m.idx[i as usize + 1], m.idx[i as usize + 2]];
            out.push((t, gi as u16, (p(t[0]) + p(t[1]) + p(t[2])) * (1.0 / 3.0)));
            i += 3;
        }
    }
    out
}

/// Le modèle sans les triangles retirés (mêmes parties, mêmes textures), sous un autre nom ; et la boîte de ce
/// qui reste.
pub fn cut_loaded(src: &Loaded, arch: u32, removed: &[bool]) -> (Loaded, Option<(V3, V3)>) {
    let mut out = clone_loaded(src);
    out.arch = arch;
    let Some(m) = out.model.as_mut() else { return (out, None) };
    let mut idx = Vec::with_capacity(m.idx.len());
    let mut ti = 0usize;
    let big = 3.0e38f32;
    let (mut mn, mut mx) = (v3(big, big, big), v3(-big, -big, -big));
    for g in m.geoms.iter_mut() {
        let end = (g.first + g.count).min(m.idx.len() as u32);
        let first = idx.len() as u32;
        let mut i = g.first;
        while i + 2 < end {
            if !removed.get(ti).copied().unwrap_or(false) {
                for k in 0..3 {
                    let vi = m.idx[(i + k) as usize];
                    idx.push(vi);
                    if let Some(v) = m.verts.get(vi as usize) {
                        let p = v3(v.pos[0], v.pos[1], v.pos[2]);
                        mn = mn.min(p);
                        mx = mx.max(p);
                    }
                }
            }
            ti += 1;
            i += 3;
        }
        g.first = first;
        g.count = idx.len() as u32 - first;
    }
    m.idx = idx;
    let bx = (mn.x <= mx.x).then_some((mn, mx));
    (out, bx)
}

/// Signature d'un modèle découpé : triangles retirés et ouvertures.
fn sig_cut(c: &BuildingCut) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    sig(&c.removed).hash(&mut h);
    for o in &c.openings {
        for v in o.pos.iter().chain(o.dir.iter()).chain([o.w, o.h, o.depth].iter()) {
            v.to_bits().hash(&mut h);
        }
        o.sill.hash(&mut h);
    }
    h.finish()
}

/// Signature des triangles retirés (pour savoir s'il faut renvoyer le modèle).
fn sig(removed: &[[f32; 3]]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    removed.len().hash(&mut h);
    for p in removed {
        p[0].to_bits().hash(&mut h);
        p[1].to_bits().hash(&mut h);
        p[2].to_bits().hash(&mut h);
    }
    h.finish()
}

/// Édition en cours.
pub struct BEdit {
    pub entity: u64,
    pub cut: String,
    pub source: String,
    tris: Vec<([u32; 3], u16, V3)>,
    verts: Vec<V3>,
    /// (partie, sommet arrondi au centimètre) → triangles
    groups: HashMap<(u16, i32, i32, i32), Vec<u32>>,
    removed: Vec<bool>,
    removed_sig: u64,
    pub drag: Option<Pos2>,
    hover: Vec<u32>,
    /// 0 : retirer des morceaux ; 1 : percer des ouvertures
    pub mode: u8,
    /// préréglage d'ouverture choisi (openings::PRESETS)
    pub preset: usize,
    /// ouverture choisie dans la liste
    pub sel: Option<usize>,
    /// ouverture proposée sous la souris (mode 1)
    pub proposal: Option<crate::openings::Opening>,
}

/// Modèles découpés envoyés à la carte graphique.
#[derive(Default)]
pub struct CutState {
    /// modèle du jeu entier, par nom d'origine
    src: HashMap<String, Loaded>,
    sent: HashMap<String, u64>,
    gpu_addr: usize,
    /// boîte de ce qui reste de chaque modèle découpé (catalogue : sélection, survol, export)
    boxes: HashMap<String, (V3, V3)>,
}

fn vkey(v: V3) -> (i32, i32, i32) {
    ((v.x * 100.0).round() as i32, (v.y * 100.0).round() as i32, (v.z * 100.0).round() as i32)
}

fn ray_tri(o: V3, d: V3, a: V3, b: V3, c: V3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

impl DwApp {
    /// Le modèle du jeu d'origine est-il un .ydr (seul format découpable) ?
    fn is_ydr_model(&self, h: u32) -> bool {
        let Some(w) = self.game.world.as_deref() else { return false };
        w.models.get(&h).map(|v| v.iter().any(|&i| w.vfs.files[i].name.ends_with(".ydr"))).unwrap_or(false)
    }

    /// L'objet sélectionné peut-il être édité (copie d'un modèle .ydr du jeu, ou modèle déjà découpé) ?
    pub fn bedit_possible(&self) -> bool {
        if self.selected.len() != 1 || self.game.world.is_none() {
            return false;
        }
        let Some(e) = self.selected.first().and_then(|id| self.proj.find_id(*id)).and_then(|r| self.proj.ent(r)) else { return false };
        let a = e.archetype.to_lowercase();
        if self.proj.building_cuts.iter().any(|c| c.name == a) {
            return true;
        }
        let h = crate::gta::crypto::joaat(&a);
        self.game.world.as_deref().and_then(|w| w.archetypes.get(&h)).map(|x| !x.mlo).unwrap_or(false) && self.is_ydr_model(h)
    }

    /// « Éditer ce bâtiment » sur l'objet sélectionné.
    pub fn bedit_start(&mut self) {
        if !self.bedit_possible() {
            self.status = "Choisis une seule copie d'un bâtiment du jeu (modèle .ydr) pour l'éditer.".into();
            return;
        }
        let id = self.selected[0];
        let Some(r) = self.proj.find_id(id) else { return };
        let Some(e) = self.proj.ent(r).cloned() else { return };
        let a = e.archetype.to_lowercase();
        let existing = self.proj.building_cuts.iter().find(|c| c.name == a).cloned();
        let source = existing.as_ref().map(|c| c.source.clone()).unwrap_or_else(|| a.clone());
        let h_src = crate::gta::crypto::joaat(&source);
        let (loaded, lod) = {
            let Some(w) = self.game.world.as_deref() else { return };
            let src = w.model_source();
            let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
            let mut cache = Vec::new();
            let l = src.load(h_src, &claimed, &mut cache);
            // la version de loin du jeu montre le bloc entier : le modèle découpé n'en a pas et reste affiché
            // jusqu'à la distance de cette version de loin (1 500 m au plus), comme les façades de MLO
            let lod = w.ents.iter().position(|x| x.arch == h_src && x.lod_level == 0).and_then(|oi| w.lod_parent(oi)).map(|pi| w.ents[pi].lod_dist().min(1500.0)).unwrap_or(0.0);
            (l, lod)
        };
        if loaded.model.is_none() {
            self.status = format!("Modèle « {source} » illisible : édition impossible.");
            return;
        }
        let cut_name = match &existing {
            Some(c) => c.name.clone(),
            None => {
                let mut n = 1;
                while self.proj.building_cuts.iter().any(|c| c.name == format!("{CUT_PREFIX}{n}")) {
                    n += 1;
                }
                let name = format!("{CUT_PREFIX}{n}");
                self.push_undo();
                self.proj.building_cuts.push(BuildingCut { name: name.clone(), source: source.clone(), removed: Vec::new(), openings: Vec::new() });
                if let Some(em) = self.proj.ent_mut(r) {
                    em.archetype = name.clone();
                    em.lod_dist = em.lod_dist.max(lod);
                }
                name
            }
        };
        let tris = model_tris(&loaded);
        let verts: Vec<V3> = loaded.model.as_ref().map(|m| m.verts.iter().map(|v| v3(v.pos[0], v.pos[1], v.pos[2])).collect()).unwrap_or_default();
        let mut groups: HashMap<(u16, i32, i32, i32), Vec<u32>> = HashMap::new();
        for (ti, (t, g, _)) in tris.iter().enumerate() {
            for vi in t {
                let k = vkey(verts.get(*vi as usize).copied().unwrap_or(V3::ZERO));
                groups.entry((*g, k.0, k.1, k.2)).or_default().push(ti as u32);
            }
        }
        let n = tris.len();
        self.cut_state.src.insert(source.clone(), loaded);
        self.cut_state.sent.remove(&cut_name);
        self.bedit = Some(BEdit { entity: id, cut: cut_name.clone(), source: source.clone(), tris, verts, groups, removed: vec![false; n], removed_sig: u64::MAX, drag: None, hover: Vec::new(), mode: 0, preset: 0, sel: None, proposal: None });
        self.isolate_sel = Some([id].into_iter().collect());
        let (mn, mx) = e.local_box(&self.proj);
        self.goto(e.pos, (mx - mn).len().max(20.0) * 1.3);
        self.status = format!("Édition de « {source} » ({cut_name}) : clic = retirer le morceau surligné, glisser = retirer une zone, Ctrl = remettre, Ctrl+Z = annuler.");
    }

    /// Retire d'une copie de bâtiment du jeu les triangles choisis par `remove(centre, surface)` (repère de l'objet,
    /// échelle comprise), sans point d'annulation (l'appelant en pose un). La copie devient un modèle découpé si
    /// elle ne l'est pas déjà. Rend le nombre de triangles retirés.
    pub fn cut_remove_where(&mut self, id: u64, remove: &dyn Fn(V3, f32) -> bool) -> Result<usize, String> {
        let r = self.proj.find_id(id).ok_or("objet introuvable")?;
        let e = self.proj.ent(r).cloned().ok_or("objet introuvable")?;
        let a = e.archetype.to_lowercase();
        let existing = self.proj.building_cuts.iter().find(|c| c.name == a).map(|c| c.source.clone());
        let source = existing.clone().unwrap_or_else(|| a.clone());
        let h = crate::gta::crypto::joaat(&source);
        if existing.is_none() {
            let ok = self.game.world.as_deref().and_then(|w| w.archetypes.get(&h)).map(|x| !x.mlo).unwrap_or(false) && self.is_ydr_model(h);
            if !ok {
                return Err(format!("« {source} » n'est pas un modèle .ydr du jeu"));
            }
        }
        let (l, lod) = {
            let w = self.game.world.as_deref().ok_or("jeu non chargé")?;
            let src = w.model_source();
            let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
            let mut cache = Vec::new();
            let l = src.load(h, &claimed, &mut cache);
            let lod = w.ents.iter().position(|x| x.arch == h && x.lod_level == 0).and_then(|oi| w.lod_parent(oi)).map(|pi| w.ents[pi].lod_dist().min(1500.0)).unwrap_or(0.0);
            (l, lod)
        };
        let m = l.model.as_ref().ok_or("modèle illisible")?;
        let s = e.scale();
        let p = |i: u32| m.verts.get(i as usize).map(|v| v3(v.pos[0], v.pos[1], v.pos[2]).mulv(s)).unwrap_or(V3::ZERO);
        let picked: Vec<[f32; 3]> = model_tris(&l)
            .iter()
            .filter(|(t, _, c)| {
                let (a, b, cc) = (p(t[0]), p(t[1]), p(t[2]));
                remove(c.mulv(s), (b - a).cross(cc - a).len() * 0.5)
            })
            .map(|(_, _, c)| [c.x, c.y, c.z])
            .collect();
        if picked.is_empty() {
            return Ok(0);
        }
        let cut = match self.proj.building_cuts.iter().find(|c| c.name == a) {
            Some(c) => c.name.clone(),
            None => {
                let mut n = 1;
                while self.proj.building_cuts.iter().any(|c| c.name == format!("{CUT_PREFIX}{n}")) {
                    n += 1;
                }
                let name = format!("{CUT_PREFIX}{n}");
                self.proj.building_cuts.push(BuildingCut { name: name.clone(), source: source.clone(), removed: Vec::new(), openings: Vec::new() });
                if let Some(em) = self.proj.ent_mut(r) {
                    em.archetype = name.clone();
                    em.lod_dist = em.lod_dist.max(lod);
                }
                name
            }
        };
        let n = picked.len();
        if let Some(c) = self.proj.building_cuts.iter_mut().find(|c| c.name == cut) {
            let set = RemovedSet::new(&c.removed);
            c.removed.extend(picked.into_iter().filter(|q| !set.contains(v3(q[0], q[1], q[2]))));
        }
        self.cut_state.src.insert(source, l);
        self.cut_state.sent.remove(&cut);
        Ok(n)
    }

    /// Triangles retirés de l'édition en cours, à jour avec le projet (Ctrl+Z compris).
    fn bedit_refresh(&mut self) {
        let Some(be) = self.bedit.as_mut() else { return };
        let Some(cut) = self.proj.building_cuts.iter().find(|c| c.name == be.cut) else { return };
        let s = sig(&cut.removed);
        if s == be.removed_sig {
            return;
        }
        let set = RemovedSet::new(&cut.removed);
        be.removed = be.tris.iter().map(|t| set.contains(t.2)).collect();
        be.removed_sig = s;
    }

    /// Modèles découpés : envoyés à la carte graphique quand ils changent, et leur taille au catalogue.
    pub fn cuts_tick(&mut self) {
        if self.proj.building_cuts.is_empty() {
            return;
        }
        let Some(gpu) = self.game.gpu.clone() else { return };
        let addr = std::sync::Arc::as_ptr(&gpu) as usize;
        if addr != self.cut_state.gpu_addr {
            self.cut_state.sent.clear();
            self.cut_state.gpu_addr = addr;
        }
        let mut boxes_changed = false;
        for ci in 0..self.proj.building_cuts.len() {
            let c = &self.proj.building_cuts[ci];
            let s = sig_cut(c);
            if self.cut_state.sent.get(&c.name) == Some(&s) {
                continue;
            }
            let (name, source, removed, openings) = (c.name.clone(), c.source.clone(), c.removed.clone(), c.openings.clone());
            if !self.cut_state.src.contains_key(&source) {
                let Some(w) = self.game.world.as_deref() else { return };
                let src = w.model_source();
                let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
                let mut cache = Vec::new();
                let l = src.load(crate::gta::crypto::joaat(&source), &claimed, &mut cache);
                self.cut_state.src.insert(source.clone(), l);
            }
            let Some(l) = self.cut_state.src.get(&source) else { continue };
            let set = RemovedSet::new(&removed);
            let mask: Vec<bool> = model_tris(l).iter().map(|t| set.contains(t.2)).collect();
            let (mut cut, bx) = cut_loaded(l, crate::gta::crypto::joaat(&name), &mask);
            if let Some(m) = cut.model.as_mut() {
                crate::openings::apply_to_rmodel(m, &openings);
            }
            if let Ok(mut g) = gpu.lock() {
                g.add_custom(cut);
            }
            if let Some(b) = bx {
                self.cut_state.boxes.insert(name.clone(), b);
                boxes_changed = true;
            }
            self.cut_state.sent.insert(name, s);
        }
        if boxes_changed {
            crate::catalog::set_cut_boxes(self.cut_state.boxes.clone());
        }
    }

    /// Fenêtre de l'édition en cours.
    pub fn bedit_window(&mut self, ctx: &egui::Context) {
        let Some(be) = self.bedit.as_ref() else { return };
        // objet disparu, ou « Tout revoir » / Échap : fin de l'édition
        if self.proj.find_id(be.entity).is_none() || self.isolate_sel.is_none() {
            self.bedit = None;
            self.isolate_sel = None;
            return;
        }
        self.bedit_refresh();
        let Some(be) = self.bedit.as_ref() else { return };
        let (cut, source) = (be.cut.clone(), be.source.clone());
        let total = be.tris.len();
        let gone = be.removed.iter().filter(|r| **r).count();
        let mut open = true;
        let mut act: Option<&'static str> = None;
        self.show_panel(ctx, "bedit", "Éditer le bâtiment", crate::docking::Dock::Floating, [440.0, 440.0], &mut open, |s, ui| {
            ui.label(RichText::new(format!("{source}  (copie éditée : {cut})")).strong());
            let mut mode = s.bedit.as_ref().map(|b| b.mode).unwrap_or(0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut mode, 0, "Retirer des morceaux");
                ui.selectable_value(&mut mode, 1, "Percer des ouvertures");
            });
            if let Some(b) = s.bedit.as_mut() {
                b.mode = mode;
            }
            if mode == 0 {
                ui.label(format!("{gone} triangle(s) retiré(s) sur {total}"));
                ui.add_space(4.0);
                ui.label(RichText::new("Clic : retire le morceau surligné en rouge sous la souris.").small());
                ui.label(RichText::new("Glisser un rectangle : retire tout ce qui est dedans (même derrière). Avec Ctrl : le remet.").small());
                ui.label(RichText::new("Clic droit : tourner · molette : zoomer · Ctrl+Z : annuler.").small().weak());
                ui.label(RichText::new("À l'export : un nouveau modèle avec seulement ce qui reste (textures du jeu, collision générée). Les autres copies de ce bâtiment ne changent pas.").small().weak());
            } else {
                s.openings_panel(ui);
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Annuler (Ctrl+Z)").clicked() {
                    act = Some("undo");
                }
                if ui.button("Tout remettre").clicked() {
                    act = Some("reset");
                }
                if ui.button(RichText::new("Terminer").strong()).clicked() {
                    act = Some("done");
                }
            });
        });
        match act {
            Some("undo") => self.do_undo(),
            Some("reset") => {
                self.push_undo();
                if let Some(c) = self.proj.building_cuts.iter_mut().find(|c| c.name == cut) {
                    c.removed.clear();
                }
            }
            Some("done") => open = false,
            _ => {}
        }
        if !open {
            self.bedit = None;
            self.isolate_sel = None;
            self.status = format!("Édition de « {source} » terminée.");
        }
    }

    /// Points du modèle de l'objet édité → monde.
    fn bedit_xf(&self) -> Option<(V3, crate::math::Quat, V3)> {
        let be = self.bedit.as_ref()?;
        let e = self.proj.find_id(be.entity).and_then(|r| self.proj.ent(r))?;
        Some((e.pos, e.rot, e.scale()))
    }

    /// Morceau sous la souris : triangle touché, puis triangles de la même partie reliés par leurs sommets.
    fn bedit_pick(&self, s: Pos2) -> Vec<u32> {
        let (Some(be), Some((p, q, sc))) = (self.bedit.as_ref(), self.bedit_xf()) else { return Vec::new() };
        let (o, d) = self.cam.ray(s);
        let w = |v: V3| p + q.rotate(v.mulv(sc));
        let mut best: Option<(f32, usize)> = None;
        for (ti, (t, _, _)) in be.tris.iter().enumerate() {
            if be.removed.get(ti).copied().unwrap_or(false) {
                continue;
            }
            let g = |i: u32| w(be.verts.get(i as usize).copied().unwrap_or(V3::ZERO));
            if let Some(tt) = ray_tri(o, d, g(t[0]), g(t[1]), g(t[2])) {
                if best.map(|b| tt < b.0).unwrap_or(true) {
                    best = Some((tt, ti));
                }
            }
        }
        let Some((_, start)) = best else { return Vec::new() };
        let mut seen = vec![false; be.tris.len()];
        let mut stack = vec![start as u32];
        seen[start] = true;
        let mut out = Vec::new();
        while let Some(ti) = stack.pop() {
            out.push(ti);
            let (t, g, _) = &be.tris[ti as usize];
            for vi in t {
                let k = vkey(be.verts.get(*vi as usize).copied().unwrap_or(V3::ZERO));
                if let Some(list) = be.groups.get(&(*g, k.0, k.1, k.2)) {
                    for &n in list {
                        if !seen[n as usize] && !be.removed[n as usize] {
                            seen[n as usize] = true;
                            stack.push(n);
                        }
                    }
                }
            }
        }
        out
    }

    /// Retire (ou remet) des triangles de l'édition en cours.
    fn bedit_apply(&mut self, tris: &[u32], remove: bool) {
        let Some(be) = self.bedit.as_ref() else { return };
        if tris.is_empty() {
            return;
        }
        let cut = be.cut.clone();
        let centers: Vec<[f32; 3]> = tris.iter().filter_map(|&t| be.tris.get(t as usize)).map(|t| [t.2.x, t.2.y, t.2.z]).collect();
        self.push_undo();
        let Some(c) = self.proj.building_cuts.iter_mut().find(|c| c.name == cut) else { return };
        if remove {
            let set = RemovedSet::new(&c.removed);
            for p in centers {
                if !set.contains(v3(p[0], p[1], p[2])) {
                    c.removed.push(p);
                }
            }
        } else {
            let back = RemovedSet::new(&centers);
            c.removed.retain(|p| !back.contains(v3(p[0], p[1], p[2])));
        }
        self.bedit_refresh();
    }

    /// Mode test : retire les triangles dont le centre est du côté x > 0 du modèle. Rend leur nombre.
    pub fn bedit_test_half(&mut self) -> usize {
        let Some(be) = self.bedit.as_ref() else { return 0 };
        let tris: Vec<u32> = be.tris.iter().enumerate().filter(|(_, t)| t.2.x > 0.0).map(|(i, _)| i as u32).collect();
        let n = tris.len();
        self.bedit_apply(&tris, true);
        n
    }

    /// Souris dans la vue pendant l'édition (à la place de l'outil en cours).
    pub fn bedit_input(&mut self, resp: &egui::Response, ptr: Option<Pos2>, press_origin: Option<Pos2>, mods: egui::Modifiers) {
        if self.bedit.is_none() {
            return;
        }
        if self.bedit.as_ref().map(|b| b.mode == 1).unwrap_or(false) {
            if resp.clicked_by(egui::PointerButton::Primary) {
                if let Some(p) = ptr {
                    self.bedit_place_opening(p);
                }
            }
            return;
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(be) = self.bedit.as_mut() {
                be.drag = press_origin.or(ptr);
            }
        }
        if resp.drag_stopped() {
            let start = self.bedit.as_mut().and_then(|b| b.drag.take());
            if let (Some(a), Some(b)) = (start, ptr) {
                let r = egui::Rect::from_two_pos(a, b);
                if r.width() > 3.0 && r.height() > 3.0 {
                    let restore = mods.command;
                    let picked = self.bedit_in_rect(r, restore);
                    let n = picked.len();
                    self.bedit_apply(&picked, !restore);
                    self.status = format!("{n} triangle(s) {} (Ctrl+Z pour annuler).", if restore { "remis" } else { "retirés" });
                }
            }
        }
        if resp.clicked_by(egui::PointerButton::Primary) {
            if let Some(p) = ptr {
                let island = self.bedit_pick(p);
                let n = island.len();
                if n > 0 {
                    self.bedit_apply(&island, true);
                    self.status = format!("Morceau retiré : {n} triangle(s) (Ctrl+Z pour annuler).");
                }
            }
        }
    }

    /// Triangles dont le centre est dans le rectangle à l'écran (retirés ou non, selon `removed_ones`).
    fn bedit_in_rect(&self, r: egui::Rect, removed_ones: bool) -> Vec<u32> {
        let (Some(be), Some((p, q, sc))) = (self.bedit.as_ref(), self.bedit_xf()) else { return Vec::new() };
        be.tris
            .iter()
            .enumerate()
            .filter(|(ti, _)| be.removed.get(*ti).copied().unwrap_or(false) == removed_ones)
            .filter(|(_, t)| self.cam.project(p + q.rotate(t.2.mulv(sc))).map(|(s, _)| r.contains(s)).unwrap_or(false))
            .map(|(ti, _)| ti as u32)
            .collect()
    }

    /// Dessin pendant l'édition : morceau sous la souris en rouge, rectangle en cours.
    pub fn bedit_draw(&mut self, painter: &egui::Painter, hover_pos: Option<Pos2>, ctx: &egui::Context) {
        if self.bedit.is_none() {
            return;
        }
        self.bedit_refresh();
        if self.bedit.as_ref().map(|b| b.mode == 1).unwrap_or(false) {
            self.bedit_draw_openings(painter, hover_pos, ctx);
            return;
        }
        let dragging = self.bedit.as_ref().and_then(|b| b.drag);
        if let (Some(a), Some(b)) = (dragging, hover_pos) {
            let r = egui::Rect::from_two_pos(a, b);
            let restore = ctx.input(|i| i.modifiers.command);
            let col = if restore { Color32::from_rgb(110, 230, 120) } else { Color32::from_rgb(255, 90, 70) };
            painter.rect_filled(r, 0.0, col.gamma_multiply(0.15));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.5_f32, col), egui::StrokeKind::Inside);
            return;
        }
        let island = match hover_pos {
            Some(p) if !ctx.is_pointer_over_area() => self.bedit_pick(p),
            _ => Vec::new(),
        };
        let (Some(be), Some((p, q, sc))) = (self.bedit.as_ref(), self.bedit_xf()) else { return };
        let mut mesh = egui::Mesh::default();
        let col = Color32::from_rgba_unmultiplied(255, 60, 40, 110);
        for &ti in &island {
            let (t, _, _) = &be.tris[ti as usize];
            let pts: Vec<Option<(Pos2, f32)>> = t.iter().map(|vi| self.cam.project(p + q.rotate(be.verts.get(*vi as usize).copied().unwrap_or(V3::ZERO).mulv(sc)))).collect();
            if let (Some((a, _)), Some((b, _)), Some((c, _))) = (pts[0], pts[1], pts[2]) {
                let base = mesh.vertices.len() as u32;
                mesh.colored_vertex(a, col);
                mesh.colored_vertex(b, col);
                mesh.colored_vertex(c, col);
                mesh.add_triangle(base, base + 1, base + 2);
            }
        }
        if !mesh.vertices.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
            if let Some(hp) = hover_pos {
                painter.text(hp + egui::vec2(16.0, 14.0), egui::Align2::LEFT_TOP, format!("clic : retirer ce morceau ({} triangles)", island.len()), egui::FontId::proportional(13.0), Color32::WHITE);
            }
        }
        if let Some(be) = self.bedit.as_mut() {
            be.hover = island;
        }
    }

    /// Export des modèles découpés : modèle (relu en XML par CodeWalker, découpé), type, collision par copie.
    /// Rend (compte rendu, lignes du fxmanifest).
    pub fn export_cuts(&self, stream: &Path, dll: Option<&Path>) -> (Vec<String>, String) {
        let mut report = Vec::new();
        // restes d'un export précédent
        for f in std::fs::read_dir(stream).into_iter().flatten().flatten() {
            let n = f.file_name().to_string_lossy().to_lowercase();
            if n.starts_with("dw_cut") && n.ends_with(".xml") && f.path().is_file() {
                let _ = std::fs::remove_file(f.path());
            }
        }
        let used: Vec<&BuildingCut> = self.proj.building_cuts.iter().filter(|c| self.proj.ymaps.iter().filter(|y| !y.sample).flat_map(|y| y.entities.iter()).any(|e| e.archetype.eq_ignore_ascii_case(&c.name))).collect();
        if used.is_empty() {
            return (report, String::new());
        }
        let Some(w) = self.game.world.as_deref() else {
            return (vec!["Bâtiments édités non exportés : charge le jeu avant d'exporter.".into()], String::new());
        };
        let Some(dll) = dll else {
            return (vec!["Bâtiments édités non exportés : CodeWalker introuvable (il relit le modèle du jeu).".into()], String::new());
        };
        let names = crate::terrain::material_names(Some(w));
        let concrete = names.iter().position(|n| n == "CONCRETE").unwrap_or(1) as u32;
        let tmp = std::env::temp_dir().join("dragowalker_cw").join("cuts");
        let _ = std::fs::create_dir_all(&tmp);
        let mut archs = Vec::new();
        let mut ncol = 0;
        for c in used {
            let h = crate::gta::crypto::joaat(&c.source);
            let Some(arch) = w.archetypes.get(&h) else { continue };
            let Some(f) = w.models.get(&h).and_then(|v| v.iter().map(|&i| &w.vfs.files[i]).find(|f| f.name.ends_with(".ydr"))) else {
                report.push(format!("« {} » : modèle du jeu introuvable en .ydr, bâtiment édité non exporté.", c.source));
                continue;
            };
            // triangles détaillés du jeu : gardés ou retirés (pour classer aussi ceux des niveaux de détail)
            let src = w.model_source();
            let claimed = std::sync::Mutex::new(std::collections::HashSet::new());
            let mut cache = Vec::new();
            let l = src.load(h, &claimed, &mut cache);
            let tris = model_tris(&l);
            let set = RemovedSet::new(&c.removed);
            let kept: Vec<bool> = tris.iter().map(|t| !set.contains(t.2)).collect();
            let centers: Vec<V3> = tris.iter().map(|t| t.2).collect();
            let cl = Classifier::new(&centers, &kept);
            let ydr = tmp.join(format!("{}.ydr", c.source));
            let xml = tmp.join(format!("{}.ydr.xml", c.source));
            let Some(raw) = w.vfs.raw_resource_file(f) else { continue };
            // même fichier qu'au dernier export : laissé tel quel (cache de la lecture en XML)
            let same = std::fs::read(&ydr).map(|old| old == raw).unwrap_or(false);
            if !same && std::fs::write(&ydr, &raw).is_err() {
                continue;
            }
            if crate::cwbridge::to_xml(dll, &[(ydr.clone(), xml.clone())]).is_err() {
                report.push(format!("« {} » : CodeWalker n'a pas pu lire le modèle du jeu, bâtiment édité non exporté.", c.source));
                continue;
            }
            let Some((cropped, mn, mx, kept_n, total)) = std::fs::read_to_string(&xml).ok().and_then(|s| crate::mlo::crop_ydr_xml(&s, &|p| cl.keep(p), &c.name)) else {
                report.push(format!("« {} » : tout a été retiré, rien à exporter.", c.name));
                continue;
            };
            // ouvertures : vraie coupe des murs sur le modèle du jeu (tous les niveaux de détail)
            let mut cropped = cropped;
            if !c.openings.is_empty() {
                match crate::openings::apply_to_ydr_xml(&cropped, &c.openings) {
                    Some((x, n)) => {
                        cropped = x;
                        report.push(format!("« {} » : {} ouverture(s) percée(s) dans les murs ({n} triangle(s) découpé(s)).", c.name, c.openings.len()));
                    }
                    None => report.push(format!("« {} » : les ouvertures n'ont pas pu être appliquées au modèle (disposition des sommets inconnue).", c.name)),
                }
            }
            if std::fs::write(stream.join(format!("{}.ydr.xml", c.name)), cropped).is_err() {
                continue;
            }
            // textures du modèle : dossier du même nom que le .ydr.xml (cw_convert -ToBinary)
            let tdir = stream.join(&c.name);
            let _ = std::fs::create_dir_all(&tdir);
            for t in std::fs::read_dir(tmp.join(&c.source)).into_iter().flatten().flatten() {
                let _ = std::fs::copy(t.path(), tdir.join(t.file_name()));
            }
            let txd = if arch.texture_dict != 0 { w.name_of(arch.texture_dict) } else { String::new() };
            let lod = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.archetype.eq_ignore_ascii_case(&c.name)).map(|e| e.lod_dist).fold(arch.lod_dist.max(150.0), f32::max);
            let (bc, br) = ((mn + mx) * 0.5, (mx - mn).len() * 0.5);
            archs.push(format!(
                "  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"{lod}\" />\n   <flags value=\"{}\" />\n   <specialAttribute value=\"0\" />\n   <bbMin x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bbMax x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsCentre x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsRadius value=\"{br}\" />\n   <hdTextureDist value=\"{}\" />\n   <name>{}</name>\n   <textureDictionary>{txd}</textureDictionary>\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>ASSET_TYPE_DRAWABLE</assetType>\n   <assetName>{}</assetName>\n   <extensions />\n  </Item>",
                arch.flags,
                mn.x, mn.y, mn.z, mx.x, mx.y, mx.z, bc.x, bc.y, bc.z,
                // celle du type du jeu ; absente, celle d'un nouveau type de CodeWalker (YtypFile::AddArchetype 0x060030f8)
                if arch.hd_texture_dist > 0.0 { arch.hd_texture_dist } else { 15.0 },
                c.name,
                c.name
            ));
            // collision de chaque copie, sur ce qui reste (les bâtiments du jeu n'ont pas de collision propre)
            // avec des ouvertures, la collision suit le modèle percé : on passe par les portes
            let col_model = if c.openings.is_empty() {
                l.model.clone()
            } else {
                let mask: Vec<bool> = kept.iter().map(|k| !k).collect();
                let (mut fin, _) = cut_loaded(&l, h, &mask);
                if let Some(fm) = fin.model.as_mut() {
                    crate::openings::apply_to_rmodel(fm, &c.openings);
                }
                fin.model
            };
            if let Some(m) = col_model.as_ref() {
                let ktris: Vec<(u32, u32, u32, u8)> = if c.openings.is_empty() {
                    tris.iter().zip(kept.iter()).filter(|(_, k)| **k).map(|(t, _)| (t.0[0], t.0[1], t.0[2], 0u8)).collect()
                } else {
                    m.geoms.iter().flat_map(|g| m.idx[(g.first as usize).min(m.idx.len())..((g.first + g.count) as usize).min(m.idx.len())].chunks_exact(3).map(|t| (t[0], t[1], t[2], 0u8))).collect()
                };
                for e in self.proj.ymaps.iter().filter(|y| !y.sample).flat_map(|y| y.entities.iter()).filter(|e| e.archetype.eq_ignore_ascii_case(&c.name)) {
                    let s = e.scale();
                    let verts: Vec<V3> = m.verts.iter().map(|v| e.pos + e.rot.rotate(v3(v.pos[0], v.pos[1], v.pos[2]).mulv(s))).collect();
                    if !ktris.is_empty() && std::fs::write(stream.join(format!("dw_cutcol_{ncol}.ybn.xml")), crate::roads::mesh_collision_xml(&verts, &ktris, &[concrete])).is_ok() {
                        ncol += 1;
                    }
                }
            }
            report.push(format!("Bâtiment édité « {} » (d'après {}) : {kept_n} triangle(s) gardé(s) sur {total}, textures du jeu.", c.name, c.source));
        }
        if archs.is_empty() {
            return (report, String::new());
        }
        let ytyp = crate::model::Ytyp { name: "dw_cut_types".into(), mlos: Vec::new(), visible: true, raw_archetypes: archs };
        let _ = std::fs::write(stream.join("dw_cut_types.ytyp.xml"), crate::xmlio::write_ytyp(&ytyp));
        if ncol > 0 {
            report.push(format!("{ncol} collision(s) générée(s) pour les bâtiments édités."));
        }
        (report, "data_file 'DLC_ITYP_REQUEST' 'stream/dw_cut_types.ytyp'\n".into())
    }
}


impl DwApp {
    /// Hauteur du sol sous un point (repère du modèle) : le triangle le plus proche en dessous.
    fn bedit_floor(&self, p: V3) -> Option<f32> {
        let be = self.bedit.as_ref()?;
        let mut best: Option<f32> = None;
        for (ti, (t, _, _)) in be.tris.iter().enumerate() {
            if be.removed.get(ti).copied().unwrap_or(false) {
                continue;
            }
            let g = |i: u32| be.verts.get(i as usize).copied().unwrap_or(V3::ZERO);
            if let Some(tt) = ray_tri(p, v3(0.0, 0.0, -1.0), g(t[0]), g(t[1]), g(t[2])) {
                if tt > 0.02 {
                    best = Some(best.map_or(tt, |b| b.min(tt)));
                }
            }
        }
        best.map(|t| p.z - t)
    }

    /// L'ouverture que le clic percerait sous la souris : le mur touché, son orientation, le sol dessous.
    pub fn bedit_opening_at(&self, s: Pos2) -> Option<crate::openings::Opening> {
        let (be, (p, q, sc)) = (self.bedit.as_ref()?, self.bedit_xf()?);
        let (o, d) = self.cam.ray(s);
        let inv = q.inverse();
        let un = |v: V3| v3(v.x / sc.x, v.y / sc.y, v.z / sc.z);
        let (ol, dl) = (un(inv.rotate(o - p)), un(inv.rotate(d)));
        let mut best: Option<(f32, usize)> = None;
        for (ti, (t, _, _)) in be.tris.iter().enumerate() {
            if be.removed.get(ti).copied().unwrap_or(false) {
                continue;
            }
            let g = |i: u32| be.verts.get(i as usize).copied().unwrap_or(V3::ZERO);
            if let Some(tt) = ray_tri(ol, dl, g(t[0]), g(t[1]), g(t[2])) {
                if best.map(|b| tt < b.0).unwrap_or(true) {
                    best = Some((tt, ti));
                }
            }
        }
        let (t, ti) = best?;
        let tri = &be.tris[ti].0;
        let g = |i: u32| be.verts.get(i as usize).copied().unwrap_or(V3::ZERO);
        let mut n = (g(tri[1]) - g(tri[0])).cross(g(tri[2]) - g(tri[0])).norm();
        if n.dot(dl) > 0.0 {
            n = -n;
        }
        // un mur : normale à peu près horizontale (pas un sol ni un plafond)
        if n.z.abs() > 0.5 {
            return None;
        }
        let hit = ol + dl * t;
        let dir = v3(n.x, n.y, 0.0).norm();
        let (name, w, h, sill, bottom) = crate::openings::PRESETS[be.preset.min(crate::openings::PRESETS.len() - 1)];
        let min_z = be.verts.iter().fold(f32::MAX, |m, v| m.min(v.z));
        let floor = self.bedit_floor(hit - dir * 0.5).or_else(|| self.bedit_floor(hit + dir * 0.5)).unwrap_or(min_z);
        Some(crate::openings::Opening { pos: [hit.x, hit.y, floor + bottom + h * 0.5], dir: [dir.x, dir.y], w, h, depth: 0.8, kind: name.to_string(), sill, prop: String::new() })
    }

    /// Panneau « Percer des ouvertures » : types, liste, réglages.
    pub fn openings_panel(&mut self, ui: &mut egui::Ui) {
        let Some(be) = self.bedit.as_ref() else { return };
        let cut = be.cut.clone();
        let (mut preset, mut sel) = (be.preset, be.sel);
        ui.label(RichText::new("Choisis un type, puis clique sur un mur : l'ouverture est percée là où le cadre vert l'indique (la hauteur part du sol trouvé sous le clic).").small());
        ui.horizontal_wrapped(|ui| {
            for (i, pr) in crate::openings::PRESETS.iter().enumerate() {
                ui.selectable_value(&mut preset, i, format!("{} {:.2}×{:.2}", pr.0, pr.1, pr.2));
            }
        });
        ui.label(RichText::new("Une ouverture coupe vraiment le mur (triangles découpés) ; si le mur a une épaisseur, elle reçoit des embrasures. La collision est percée aussi : on passe par la porte.").small().weak());
        let list: Vec<crate::openings::Opening> = self.proj.building_cuts.iter().find(|c| c.name == cut).map(|c| c.openings.clone()).unwrap_or_default();
        ui.separator();
        ui.label(RichText::new(format!("Ouvertures de ce bâtiment : {}", list.len())).strong());
        for (i, o) in list.iter().enumerate() {
            if ui.selectable_label(sel == Some(i), format!("{}. {} — {:.2} × {:.2} m", i + 1, o.kind, o.w, o.h)).clicked() {
                sel = Some(i);
            }
        }
        let mut edited: Option<(usize, crate::openings::Opening, bool)> = None;
        let mut delete: Option<usize> = None;
        let mut place: Option<usize> = None;
        let mut find_door = false;
        if let Some(i) = sel.filter(|i| *i < list.len()) {
            let mut o = list[i].clone();
            let mut started = false;
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Largeur");
                let r = ui.add(egui::Slider::new(&mut o.w, 0.3..=5.0).suffix(" m"));
                started |= r.drag_started();
                changed |= r.changed();
            });
            ui.horizontal(|ui| {
                ui.label("Hauteur");
                let r = ui.add(egui::Slider::new(&mut o.h, 0.4..=4.0).suffix(" m"));
                started |= r.drag_started();
                changed |= r.changed();
            });
            ui.horizontal(|ui| {
                ui.label("Profondeur de coupe");
                let r = ui.add(egui::Slider::new(&mut o.depth, 0.1..=2.5).suffix(" m")).on_hover_text("Doit dépasser l'épaisseur du mur pour le traverser ; au-delà, ça coupe aussi ce qu'il y a derrière");
                started |= r.drag_started();
                changed |= r.changed();
            });
            if ui.checkbox(&mut o.sill, "Rebord en bas (fenêtre)").changed() {
                started = true;
                changed = true;
            }
            ui.horizontal(|ui| {
                let n = v3(o.dir[0], o.dir[1], 0.0).norm();
                let right = v3(0.0, 0.0, 1.0).cross(n);
                let mut nudge = |label: &str, d: V3, o: &mut crate::openings::Opening| {
                    if ui.small_button(label).clicked() {
                        o.pos[0] += d.x;
                        o.pos[1] += d.y;
                        o.pos[2] += d.z;
                        return true;
                    }
                    false
                };
                if nudge("gauche 10 cm", right * -0.1, &mut o) | nudge("droite 10 cm", right * 0.1, &mut o) | nudge("monter 10 cm", v3(0.0, 0.0, 0.1), &mut o) | nudge("descendre 10 cm", v3(0.0, 0.0, -0.1), &mut o) {
                    started = true;
                    changed = true;
                }
            });
            ui.separator();
            ui.label(RichText::new("Porte ou fenêtre du jeu").strong());
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut o.prop).desired_width(190.0).hint_text("nom du modèle, ex. v_ilev_ra_door3"));
                if r.changed() {
                    changed = true;
                    started = true;
                }
                if ui.small_button("Chercher une porte").on_hover_text("Prend la porte du jeu qui va le mieux à la taille de l'ouverture (la même recherche que « Transformer en intérieur »)").clicked() {
                    find_door = true;
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!o.prop.trim().is_empty(), egui::Button::new("Poser dans l'ouverture")).on_hover_text("Pose ce modèle, charnière à gauche, tourné comme le mur ; ajuste-le ensuite avec les poignées. Il ne suit pas l'ouverture si tu la déplaces ensuite.").clicked() {
                    place = Some(i);
                }
                if ui.button("Supprimer cette ouverture").clicked() {
                    delete = Some(i);
                }
            });
            if find_door {
                let (w, h) = (o.w, o.h);
                match self.find_game_prop(&["door"], &["v_ilev_ra_door", "v_ilev_fh_door", "v_ilev_"], v3(0.4, 0.02, 1.2), v3(3.0, 0.5, 3.2), v3(w, 0.1, h)) {
                    Some((name, _, _)) => {
                        o.prop = name;
                        started = true;
                        changed = true;
                    }
                    None => self.status = "Aucune porte du jeu trouvée pour cette taille : tape un nom de modèle.".into(),
                }
            }
            if changed {
                edited = Some((i, o, started));
            }
        }
        if let Some(b) = self.bedit.as_mut() {
            b.preset = preset;
            b.sel = sel;
        }
        if let Some((i, o, started)) = edited {
            if started {
                self.push_undo();
            }
            if let Some(c) = self.proj.building_cuts.iter_mut().find(|c| c.name == cut) {
                if let Some(slot) = c.openings.get_mut(i) {
                    *slot = o;
                }
            }
        }
        if let Some(i) = place {
            self.bedit_place_prop(&cut, i);
        }
        if let Some(i) = delete {
            self.push_undo();
            if let Some(c) = self.proj.building_cuts.iter_mut().find(|c| c.name == cut) {
                if i < c.openings.len() {
                    c.openings.remove(i);
                }
            }
            if let Some(b) = self.bedit.as_mut() {
                b.sel = None;
            }
        }
    }

    /// Mode « ouvertures » dans la vue : cadres des ouvertures (orange : la choisie), cadre vert sous la souris.
    fn bedit_draw_openings(&mut self, painter: &egui::Painter, hover_pos: Option<Pos2>, ctx: &egui::Context) {
        let Some((p, q, sc)) = self.bedit_xf() else { return };
        let Some(be) = self.bedit.as_ref() else { return };
        let (cut, sel) = (be.cut.clone(), be.sel);
        let list: Vec<crate::openings::Opening> = self.proj.building_cuts.iter().find(|c| c.name == cut).map(|c| c.openings.clone()).unwrap_or_default();
        let tw = |v: V3| p + q.rotate(v.mulv(sc));
        for (i, o) in list.iter().enumerate() {
            let c = o.obox().world_corners();
            let w: [V3; 8] = [tw(c[0]), tw(c[1]), tw(c[2]), tw(c[3]), tw(c[4]), tw(c[5]), tw(c[6]), tw(c[7])];
            let col = if sel == Some(i) { Color32::from_rgb(255, 170, 40) } else { Color32::from_rgb(80, 200, 255) };
            crate::render::draw_box_edges(painter, &self.cam, &w, egui::Stroke::new(2.0_f32, col));
        }
        let prop = match hover_pos {
            Some(hp) if !ctx.is_pointer_over_area() => self.bedit_opening_at(hp),
            _ => None,
        };
        if let Some(o) = &prop {
            let c = o.obox().world_corners();
            let w: [V3; 8] = [tw(c[0]), tw(c[1]), tw(c[2]), tw(c[3]), tw(c[4]), tw(c[5]), tw(c[6]), tw(c[7])];
            crate::render::draw_box_edges(painter, &self.cam, &w, egui::Stroke::new(2.5_f32, Color32::from_rgb(110, 240, 120)));
            if let Some(hp) = hover_pos {
                painter.text(hp + egui::vec2(16.0, 14.0), egui::Align2::LEFT_TOP, format!("clic : percer une {} ({:.2} × {:.2} m)", o.kind, o.w, o.h), egui::FontId::proportional(13.0), Color32::WHITE);
            }
        }
        if let Some(b) = self.bedit.as_mut() {
            b.proposal = prop;
        }
    }

    /// Pose dans l'ouverture n° `i` la porte ou la fenêtre du jeu choisie : charnière sur le bord gauche, au bas de
    /// l'ouverture, le modèle s'étendant vers la droite (comme les portes posées par « Transformer en intérieur »),
    /// largeur ajustée à l'ouverture (60 % à 160 %).
    pub(crate) fn bedit_place_prop(&mut self, cut: &str, i: usize) {
        let Some(o) = self.proj.building_cuts.iter().find(|c| c.name == cut).and_then(|c| c.openings.get(i)).cloned() else { return };
        let name = o.prop.trim().to_lowercase();
        if name.is_empty() {
            return;
        }
        let Some(e) = self.bedit.as_ref().and_then(|b| self.proj.find_id(b.entity)).and_then(|r| self.proj.ent(r)).cloned() else { return };
        let ob = o.obox();
        let (mn, mx) = crate::catalog::box_for(&name);
        let (dw, dh) = ((mx.x - mn.x).max(0.1), (mx.z - mn.z).max(0.1));
        // charnière sur le bord gauche ; le bas du modèle sur le bas de l'ouverture (l'origine d'un modèle n'est pas
        // toujours à ses pieds : celle de v_ilev_ra_door3 est à mi-hauteur)
        let sz = (o.h / dh).clamp(0.7, 1.3);
        let hinge = ob.c - ob.r * ob.hw - ob.u * ob.hh - ob.r * mn.x.min(0.0) - ob.u * (mn.z * sz);
        let world = e.pos + e.rot.rotate(hinge.mulv(e.scale()));
        let rw = e.rot.rotate(ob.r);
        let heading = rw.y.atan2(rw.x);
        self.push_undo();
        let yi = self.ensure_target_ymap();
        let mut pe = crate::model::Entity::new(&name, world);
        pe.rot = crate::math::Quat::from_z(heading);
        pe.scale_xy = (o.w / dw).clamp(0.6, 1.6);
        pe.scale_z = sz;
        let id = pe.id;
        self.proj.ymaps[yi].entities.push(pe);
        if let Some(s) = self.isolate_sel.as_mut() {
            s.insert(id);
        }
        self.status = format!("« {name} » posé dans l'ouverture {} : ajuste-le avec les poignées si besoin (orientation et charnière à vérifier en jeu).", i + 1);
    }

    /// Clic en mode « ouvertures » : perce l'ouverture proposée.
    fn bedit_place_opening(&mut self, s: Pos2) {
        let Some(o) = self.bedit_opening_at(s) else {
            self.status = "Clique sur un mur du bâtiment (pas sur un sol ni un plafond).".into();
            return;
        };
        let Some(cut) = self.bedit.as_ref().map(|b| b.cut.clone()) else { return };
        self.push_undo();
        let kind = o.kind.clone();
        let mut n = 0;
        if let Some(c) = self.proj.building_cuts.iter_mut().find(|c| c.name == cut) {
            c.openings.push(o);
            n = c.openings.len();
        }
        if let Some(b) = self.bedit.as_mut() {
            b.sel = n.checked_sub(1);
        }
        self.status = format!("Ouverture « {kind} » percée (n° {n}) : ajuste-la dans le panneau, Ctrl+Z pour annuler.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_set_matches_with_tolerance() {
        let s = RemovedSet::new(&[[1.0, 2.0, 3.0], [10.0, 0.0, 0.0]]);
        assert!(s.contains(v3(1.004, 2.0, 3.0)));
        assert!(s.contains(v3(10.0, 0.006, 0.0)));
        assert!(!s.contains(v3(1.05, 2.0, 3.0)));
    }

    #[test]
    fn classifier_follows_nearest_detail_triangle() {
        let centers = [v3(0.0, 0.0, 0.0), v3(30.0, 0.0, 0.0)];
        let cl = Classifier::new(&centers, &[true, false]);
        assert!(cl.keep(v3(2.0, 1.0, 0.0)));
        assert!(!cl.keep(v3(27.5, 0.5, 0.0)));
        // loin de tout : gardé
        assert!(cl.keep(v3(500.0, 0.0, 0.0)));
    }
}
