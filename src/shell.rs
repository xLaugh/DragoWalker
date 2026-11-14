//! Coque des intérieurs créés dans DragoWalker : pour chaque pièce, un vrai modèle avec sol, plafond et murs
//! (ouverts là où passe un portail), textures fabriquées par DragoWalker ; posé dans la pièce comme objet de
//! l'intérieur (« dw_shell_<intérieur>_<pièce> ») et repris en entier dans la collision de l'intérieur, avec le
//! numéro de la pièce sur chaque surface (RoomId, BoundMaterial_s::set_RoomId 0x060042e3).

use crate::app::DwApp;
use crate::gta::drawable::{RGeom, RModel, RVert, TexData, TexFmt};
use crate::math::{v3, V3};
use crate::model::{Entity, MloArchetype};

pub const SHELL_PREFIX: &str = "dw_shell_";
/// Dictionnaire de textures des coques, déclaré par leurs types d'objets (sans lui : texture blanche en jeu).
pub const SHELL_TXD: &str = "dw_shell_txd";
/// Clés des trois textures (mur, sol, plafond).
pub const K_WALL: u64 = 0xD5_5E11_0001;
pub const K_FLOOR: u64 = 0xD5_5E11_0002;
pub const K_CEIL: u64 = 0xD5_5E11_0003;
/// Un portail est sur un mur quand ses quatre coins sont à moins de cette distance du plan du mur (m) — **estimé**.
const ON_WALL: f32 = 0.2;
/// Une trémie (portail horizontal entre deux étages) perce le sol ou le plafond d'une pièce quand ses coins sont à
/// moins de cette distance de son plan (m) — **estimé**.
const STAIR_PLANE: f32 = 0.6;
/// Escalier : hauteur de marche au plus, profondeur de marche, et ce qu'il monte de plus que la pièce (épaisseur du
/// plancher jusqu'au sol de l'étage du dessus) (m) — **estimé**.
const STAIR_RISE: f32 = 0.18;
const STAIR_RUN: f32 = 0.28;
const STAIR_CEIL_GAP: f32 = 0.27;
/// Distance d'affichage de l'objet « coque » : celle d'un nouvel objet d'intérieur de CodeWalker
/// (ProjectForm::NewMloEntity 0x06000d68, IL_0398 : ldc.r4 200 → CEntityDef.lodDist).
const SHELL_LOD: f32 = 200.0;
/// Distance d'affichage du type d'objet : celle d'un nouveau type de CodeWalker (YtypFile::AddArchetype
/// 0x060030f8 : lodDist 60, hdTextureDist 15, assetType 2 = drawable).
const SHELL_ARCH_LOD: f32 = 60.0;

pub fn shell_name(mlo: &str, ri: usize) -> String {
    format!("{SHELL_PREFIX}{}_{ri}", mlo.to_lowercase())
}

/// Triangles de la coque d'une pièce (repère de l'intérieur), rangés par partie : 0 sol, 1 murs, 2 plafond.
/// Chaque triangle : (3 sommets, normale vers l'intérieur de la pièce).
pub fn room_quads(m: &MloArchetype, ri: usize) -> Vec<(usize, [V3; 4], V3)> {
    let Some(r) = m.rooms.get(ri) else { return Vec::new() };
    let (a, b) = (r.bb_min, r.bb_max);
    let mut out = Vec::new();
    // trémies : portails horizontaux de cette pièce au niveau de son sol ou de son plafond (passage entre étages)
    let flat_holes = |zz: f32| -> Vec<(f32, f32, f32, f32)> {
        m.portals
            .iter()
            .filter(|p| p.room_from as usize == ri || p.room_to as usize == ri)
            .flat_map(|p| p.holes())
            .filter(|h| h.iter().all(|c| (c.z - zz).abs() < STAIR_PLANE))
            .map(|h| {
                let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                for c in h {
                    x0 = x0.min(c.x);
                    x1 = x1.max(c.x);
                    y0 = y0.min(c.y);
                    y1 = y1.max(c.y);
                }
                (x0.max(a.x), x1.min(b.x), y0.max(a.y), y1.min(b.y))
            })
            .filter(|h| h.1 - h.0 > 0.2 && h.3 - h.2 > 0.2)
            .collect()
    };
    let (floor_holes, ceil_holes) = (flat_holes(a.z), flat_holes(b.z));
    // pièce de forme libre : sol, plafond et murs viennent de sa coque modelable (design.rs) ; seulement l'escalier
    let free = r.free;
    // sol (vers le haut) et plafond (vers le bas), percés des trémies
    for (part, zz, n, holes) in [(0usize, a.z, v3(0.0, 0.0, 1.0), &floor_holes), (2, b.z, v3(0.0, 0.0, -1.0), &ceil_holes)].into_iter().filter(|_| !free) {
        let mut rects = vec![(a.x, b.x, a.y, b.y)];
        for h in holes.iter() {
            let mut next = Vec::new();
            for r in rects {
                if h.1 <= r.0 || h.0 >= r.1 || h.3 <= r.2 || h.2 >= r.3 {
                    next.push(r);
                    continue;
                }
                let (hx0, hx1, hy0, hy1) = (h.0.max(r.0), h.1.min(r.1), h.2.max(r.2), h.3.min(r.3));
                next.push((r.0, hx0, r.2, r.3));
                next.push((hx1, r.1, r.2, r.3));
                next.push((hx0, hx1, r.2, hy0));
                next.push((hx0, hx1, hy1, r.3));
            }
            rects = next.into_iter().filter(|r| r.1 - r.0 > 0.01 && r.3 - r.2 > 0.01).collect();
        }
        for (x0, x1, y0, y1) in rects {
            let q = if part == 0 { [v3(x0, y0, zz), v3(x1, y0, zz), v3(x1, y1, zz), v3(x0, y1, zz)] } else { [v3(x0, y0, zz), v3(x0, y1, zz), v3(x1, y1, zz), v3(x1, y0, zz)] };
            out.push((part, q, n));
        }
    }
    // escalier sous chaque trémie du plafond : le long du grand côté de la trémie, la dernière marche à son bord
    // le plus loin, marches de STAIR_RISE au plus et STAIR_RUN de profondeur (raccourcies si la pièce est trop
    // petite)
    // pas pour un bâtiment de l'atelier : il a son propre escalier (avec sa rampe) ; celui-ci montait parfois dans
    // l'autre sens et bloquait le personnage (retour de la personne, 13 nov. 2026)
    let own_stairs = !m.name.to_lowercase().starts_with("dw_bat_");
    for &(x0, x1, y0, y1) in ceil_holes.iter().filter(|_| own_stairs) {
        let along_x = x1 - x0 >= y1 - y0;
        let rise_total = b.z - a.z + STAIR_CEIL_GAP;
        let n = (rise_total / STAIR_RISE).ceil().max(2.0) as usize;
        let (lo, hi) = if along_x { (a.x, x1) } else { (a.y, y1) };
        let run = (STAIR_RUN).min(((hi - lo) - 0.1) / n as f32).max(0.12);
        let start = hi - run * n as f32;
        let up = v3(0.0, 0.0, 1.0);
        let pt = |s: f32, w: f32, zz: f32| if along_x { v3(s, w, zz) } else { v3(w, s, zz) };
        let (w0, w1) = if along_x { (y0, y1) } else { (x0, x1) };
        let fwd = if along_x { v3(1.0, 0.0, 0.0) } else { v3(0.0, 1.0, 0.0) };
        let side = if along_x { v3(0.0, 1.0, 0.0) } else { v3(1.0, 0.0, 0.0) };
        for i in 0..n {
            let (s0, s1) = (start + i as f32 * run, start + (i + 1) as f32 * run);
            let (zl, zh) = (a.z + i as f32 * rise_total / n as f32, a.z + (i + 1) as f32 * rise_total / n as f32);
            // contremarche (vers le bas de l'escalier), marche, joues
            out.push((0, [pt(s0, w0, zl), pt(s0, w1, zl), pt(s0, w1, zh), pt(s0, w0, zh)], fwd * -1.0));
            out.push((0, [pt(s0, w0, zh), pt(s1, w0, zh), pt(s1, w1, zh), pt(s0, w1, zh)], up));
            out.push((1, [pt(s0, w0, a.z), pt(s1, w0, a.z), pt(s1, w0, zh), pt(s0, w0, zh)], side * -1.0));
            out.push((1, [pt(s0, w1, a.z), pt(s1, w1, a.z), pt(s1, w1, zh), pt(s0, w1, zh)], side));
        }
    }
    // murs : (axe fixe 0 = x / 1 = y, valeur, début et fin le long du mur, normale vers l'intérieur)
    let walls = [(0usize, a.x, a.y, b.y, v3(1.0, 0.0, 0.0)), (0, b.x, a.y, b.y, v3(-1.0, 0.0, 0.0)), (1, a.y, a.x, b.x, v3(0.0, 1.0, 0.0)), (1, b.y, a.x, b.x, v3(0.0, -1.0, 0.0))];
    for (axis, val, s0, s1, n) in walls.into_iter().filter(|_| !free) {
        let fixed = |p: V3| if axis == 0 { p.x } else { p.y };
        let along = |p: V3| if axis == 0 { p.y } else { p.x };
        // ouvertures : portails de cette pièce posés sur ce mur (d'un portail réuni, seulement ses ouvertures
        // d'origine)
        let holes: Vec<(f32, f32, f32, f32)> = m
            .portals
            .iter()
            .filter(|p| p.room_from as usize == ri || p.room_to as usize == ri)
            .flat_map(|p| p.holes())
            .filter(|h| h.iter().all(|c| (fixed(*c) - val).abs() < ON_WALL))
            .map(|h| {
                let (mut lo, mut hi, mut zl, mut zh) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                for c in h {
                    lo = lo.min(along(c));
                    hi = hi.max(along(c));
                    zl = zl.min(c.z);
                    zh = zh.max(c.z);
                }
                (lo.max(s0), hi.min(s1), zl.max(a.z), zh.min(b.z))
            })
            .filter(|h| h.1 - h.0 > 0.05 && h.3 > h.2)
            .collect();
        let pt = |s: f32, z: f32| if axis == 0 { v3(val, s, z) } else { v3(s, val, z) };
        for (s0, s1, z0, z1) in wall_pieces(s0, s1, a.z, b.z, &holes) {
            out.push((1, [pt(s0, z0), pt(s1, z0), pt(s1, z1), pt(s0, z1)], n));
        }
    }
    out
}

/// Morceaux pleins d'un mur (début, fin le long du mur, bas, haut) une fois percé des ouvertures `holes` (même
/// forme), qui peuvent se chevaucher : le mur est coupé à chaque bord d'ouverture ; sur chaque tranche, les hauteurs
/// des ouvertures qui la couvrent sont réunies et le reste de la hauteur est plein (tranches voisines identiques
/// réunies).
pub fn wall_pieces(s0: f32, s1: f32, z0: f32, z1: f32, holes: &[(f32, f32, f32, f32)]) -> Vec<(f32, f32, f32, f32)> {
    let mut cuts: Vec<f32> = vec![s0, s1];
    for h in holes {
        cuts.push(h.0.clamp(s0, s1));
        cuts.push(h.1.clamp(s0, s1));
    }
    cuts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    cuts.dedup_by(|x, y| (*x - *y).abs() < 1e-4);
    // (début, fin, morceaux pleins en hauteur)
    let mut slices: Vec<(f32, f32, Vec<(f32, f32)>)> = Vec::new();
    for w in cuts.windows(2) {
        let (l, r) = (w[0], w[1]);
        if r - l < 1e-4 {
            continue;
        }
        let mid = (l + r) * 0.5;
        let mut spans: Vec<(f32, f32)> = holes.iter().filter(|h| h.0 <= mid && h.1 >= mid).map(|h| (h.2.max(z0), h.3.min(z1))).filter(|s| s.1 > s.0).collect();
        spans.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut solid = Vec::new();
        let mut cur = z0;
        for (a, b) in spans {
            if a > cur {
                solid.push((cur, a));
            }
            cur = cur.max(b);
        }
        if z1 > cur {
            solid.push((cur, z1));
        }
        match slices.last_mut() {
            Some(last) if (last.1 - l).abs() < 1e-4 && last.2 == solid => last.1 = r,
            _ => slices.push((l, r, solid)),
        }
    }
    let mut out = Vec::new();
    for (l, r, solid) in slices {
        for (a, b) in solid {
            if r - l > 0.01 && b - a > 0.01 {
                out.push((l, r, a, b));
            }
        }
    }
    out
}

/// Deux triangles d'un quadrilatère, tournés vers la normale voulue.
fn tri_pair(q: &[V3; 4], n: V3) -> [[V3; 3]; 2] {
    let t = [[q[0], q[1], q[2]], [q[0], q[2], q[3]]];
    if (q[1] - q[0]).cross(q[2] - q[0]).dot(n) < 0.0 {
        [[q[0], q[2], q[1]], [q[0], q[3], q[2]]]
    } else {
        t
    }
}

/// Modèle de la coque d'une pièce : 3 parties (sol, murs, plafond), coordonnées de texture en mètres / 2.
pub fn room_model(m: &MloArchetype, ri: usize) -> Option<(RModel, (V3, V3))> {
    let quads = room_quads(m, ri);
    if quads.is_empty() {
        return None;
    }
    let mut verts = Vec::new();
    let mut idx = Vec::new();
    let mut geoms = Vec::new();
    let big = 3.0e38f32;
    let (mut mn, mut mx) = (v3(big, big, big), v3(-big, -big, -big));
    for part in 0..3 {
        let first = idx.len() as u32;
        for (p, q, n) in quads.iter().filter(|x| x.0 == part) {
            let _ = p;
            for t in tri_pair(q, *n) {
                for c in t {
                    let uv = if n.z.abs() > 0.5 { [c.x * 0.5, c.y * 0.5] } else if n.x.abs() > 0.5 { [c.y * 0.5, -c.z * 0.5] } else { [c.x * 0.5, -c.z * 0.5] };
                    idx.push(verts.len() as u32);
                    verts.push(RVert { pos: [c.x, c.y, c.z], nrm: [n.x, n.y, n.z], uv, col: [255, 255, 255, 255], uv1: [0.0, 0.0] });
                    mn = mn.min(c);
                    mx = mx.max(c);
                }
            }
        }
        geoms.push(RGeom { first, count: idx.len() as u32 - first, tex: 0, cutout: false, blend: 0, glass: false, ..Default::default() });
    }
    Some((RModel { verts, idx, geoms, embedded_dict: None }, (mn, mx)))
}

/// Textures de la coque : enduit clair, parquet, plafond blanc (64 × 64, léger grain) — **estimé**.
pub fn shell_textures() -> Vec<(u64, TexData)> {
    let make = |base: [u8; 3], grain: u8, planks: bool| {
        let n = 64usize;
        let mut px = Vec::with_capacity(n * n * 4);
        for y in 0..n {
            for x in 0..n {
                let h = ((x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663)) % (grain as u32 * 2 + 1);
                let mut d = h as i32 - grain as i32;
                if planks && y % 16 == 0 {
                    d -= 25;
                }
                for c in base {
                    px.push((c as i32 + d).clamp(0, 255) as u8);
                }
                px.push(255);
            }
        }
        TexData { w: n as u32, h: n as u32, fmt: TexFmt::Rgba8, levels: vec![px] }
    };
    vec![(K_WALL, make([214, 208, 196], 6, false)), (K_FLOOR, make([150, 108, 72], 10, true)), (K_CEIL, make([236, 234, 228], 3, false))]
}

pub const SHELL_KEYS: [u64; 3] = [K_FLOOR, K_WALL, K_CEIL];

impl DwApp {
    /// Intérieurs à doter d'une coque : créés dans DragoWalker (ni MLO importé, ni intérieur du jeu).
    /// Intérieurs qui reçoivent une coque visible : ceux créés dans DragoWalker, sauf ceux d'un bâtiment de l'atelier
    /// (le bâtiment a déjà ses murs, ses planchers et son escalier).
    pub fn shell_mlos(&self) -> Vec<(usize, usize)> {
        self.created_mlos().into_iter().filter(|&(ti, mi)| !self.proj.ytyps[ti].mlos[mi].name.to_lowercase().starts_with(crate::atelier::HOUSE_PREFIX)).collect()
    }

    /// Intérieurs créés ou modifiés dans DragoWalker (pas ceux du jeu, ni ceux recopiés d'une ressource MLO).
    pub fn created_mlos(&self) -> Vec<(usize, usize)> {
        let external = self.mlo_external_ytyps();
        let mut out = Vec::new();
        for (ti, t) in self.proj.ytyps.iter().enumerate().filter(|(_, t)| !external.contains(&t.name.to_lowercase())) {
            for (mi, m) in t.mlos.iter().enumerate() {
                if !self.mlo_is_game(m) {
                    out.push((ti, mi));
                }
            }
        }
        out
    }

    /// Vrai si l'intérieur est un intérieur du jeu (relu en XML) : il garde sa collision et sa coque du jeu. Jeu
    /// chargé : il est dans les types du jeu. Sinon on ne peut pas le savoir — **estimé** : un intérieur créé dans
    /// DragoWalker est vide à sa création, a reçu ses objets « coque » (dw_shell_…) dès que le jeu était chargé ou au
    /// premier export, ou porte un nom de DragoWalker (dw_…) ; un intérieur du jeu relu a toujours ses objets et
    /// jamais de coque.
    pub fn mlo_is_game(&self, m: &MloArchetype) -> bool {
        match self.game.world.as_deref() {
            Some(w) => w.archetypes.get(&crate::gta::crypto::joaat(&m.name.to_lowercase())).map(|a| a.mlo).unwrap_or(false),
            None => !(m.entities.is_empty() || m.name.to_lowercase().starts_with("dw_") || m.entities.iter().any(|e| e.archetype.starts_with(SHELL_PREFIX))),
        }
    }

    /// Chaque pièce d'un intérieur créé a son objet « coque » (et rien pour les pièces disparues).
    pub fn ensure_shell_entities(&mut self) {
        for (ti, mi) in self.shell_mlos() {
            let m = &mut self.proj.ytyps[ti].mlos[mi];
            let name = m.name.clone();
            let nr = m.rooms.len();
            // pas d'objet pour une pièce sans coque en boîte (pièce de forme libre sans escalier : sa coque modelable
            // la remplace)
            let has: Vec<bool> = (0..nr).map(|ri| ri > 0 && !room_quads(m, ri).is_empty()).collect();
            m.entities.retain(|e| !e.archetype.starts_with(SHELL_PREFIX) || (e.room > 0 && e.room < nr && has[e.room] && e.archetype == shell_name(&name, e.room)));
            for ri in 1..nr {
                if !has[ri] {
                    continue;
                }
                let sn = shell_name(&name, ri);
                if !m.entities.iter().any(|e| e.archetype == sn) {
                    let mut e = Entity::new(&sn, V3::ZERO);
                    e.flags = crate::mech::MLO_ENTITY_FLAGS;
                    e.room = ri;
                    e.lod_dist = SHELL_LOD;
                    m.entities.push(e);
                }
            }
        }
    }

    /// Coques dans la vue : modèles envoyés à la carte graphique et tailles au catalogue, quand les pièces ou les
    /// portails changent.
    pub fn shells_tick(&mut self) {
        let mlos = self.shell_mlos();
        if mlos.is_empty() {
            return;
        }
        let Some(gpu) = self.game.gpu.clone() else { return };
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (std::sync::Arc::as_ptr(&gpu) as usize).hash(&mut h);
        for &(ti, mi) in &mlos {
            let m = &self.proj.ytyps[ti].mlos[mi];
            m.name.hash(&mut h);
            for r in &m.rooms {
                for v in [r.bb_min, r.bb_max] {
                    v.x.to_bits().hash(&mut h);
                    v.y.to_bits().hash(&mut h);
                    v.z.to_bits().hash(&mut h);
                }
            }
            for p in &m.portals {
                (p.room_from, p.room_to).hash(&mut h);
                for c in p.corners {
                    c.x.to_bits().hash(&mut h);
                    c.y.to_bits().hash(&mut h);
                    c.z.to_bits().hash(&mut h);
                }
            }
        }
        serde_json::to_string(&self.proj.designs).unwrap_or_default().hash(&mut h);
        for c in &self.proj.custom_textures {
            (c.name.as_str(), c.size.to_bits(), c.png.len()).hash(&mut h);
        }
        let sig = h.finish();
        if sig == self.shell_sig {
            return;
        }
        self.shell_sig = sig;
        crate::design::set_custom_textures(&self.proj.custom_textures);
        self.ensure_shell_entities();
        self.design_ensure_entities();
        let designs = self.design_models();
        let texs = shell_textures();
        let mut boxes = std::collections::HashMap::new();
        if let Ok(mut g) = gpu.lock() {
            for &(ti, mi) in &mlos {
                let m = &self.proj.ytyps[ti].mlos[mi];
                for ri in 1..m.rooms.len() {
                    let Some((model, bx)) = room_model(m, ri) else { continue };
                    let sn = shell_name(&m.name, ri);
                    boxes.insert(sn.clone(), bx);
                    g.add_custom(crate::gta::loader::Loaded { arch: crate::gta::crypto::joaat(&sn), model: Some(model), part_keys: SHELL_KEYS.to_vec(), layer_keys: Vec::new(), textures: texs.clone() });
                }
            }
            // coques modelables (design.rs)
            for ((ti, mi), _, name, mm) in &designs {
                let (model, keys, dt, bx) = self.design_rmodel(*ti, *mi, mm);
                boxes.insert(name.clone(), bx);
                g.add_custom(crate::gta::loader::Loaded { arch: crate::gta::crypto::joaat(name), model: Some(model), part_keys: keys, layer_keys: Vec::new(), textures: dt });
            }
        }
        crate::catalog::set_shell_boxes(boxes);
    }

    /// Export des coques : un modèle par pièce (textures intégrées), leurs types dans « dw_shell_types ».
    /// Rend (compte rendu, lignes du fxmanifest).
    pub fn export_shells(&self, stream: &std::path::Path) -> (Vec<String>, String) {
        for f in std::fs::read_dir(stream).into_iter().flatten().flatten() {
            let n = f.file_name().to_string_lossy().to_lowercase();
            if n.starts_with(SHELL_PREFIX) && n.ends_with(".xml") && f.path().is_file() {
                let _ = std::fs::remove_file(f.path());
            }
        }
        let texs = shell_textures();
        let mut archs = Vec::new();
        let mut n = 0;
        let mut nl = 0;
        let mut lit_rooms = 0;
        let src = self.game.world.as_ref().map(|w| w.model_source());
        let mut lit_cache: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();
        for (ti, mi) in self.shell_mlos() {
            let m = &self.proj.ytyps[ti].mlos[mi];
            for ri in 1..m.rooms.len() {
                let Some((model, (mn, mx))) = room_model(m, ri) else { continue };
                let sn = shell_name(&m.name, ri);
                let Ok(path) = crate::sollumz::write_ydr_xml_ex(stream, &sn, &model, &SHELL_KEYS, &texs, Some(SHELL_TXD)) else { continue };
                // spots du plafond, rangés dans le modèle comme les lumières des intérieurs du jeu
                let r = &m.rooms[ri];
                // pièce déjà éclairée par un objet du jeu qui porte ses propres lumières (plafonnier…) : pas de
                // spots en plus ; sinon (objet sans lumière, ou aucun), spots au plafond
                let lit = m.entities.iter().filter(|e| e.room == ri).any(|e| {
                    let h = crate::gta::crypto::joaat(&e.archetype);
                    *lit_cache.entry(h).or_insert_with(|| src.as_ref().map(|s| !s.lights(h).is_empty()).unwrap_or(false))
                });
                let placed: Vec<crate::lights::Light> = self.proj.designs.iter().find(|d| d.mlo.eq_ignore_ascii_case(&m.name)).map(|d| crate::design::room_design_lights(d, m, ri)).unwrap_or_default();
                let lights = if !placed.is_empty() { placed } else if lit { Vec::new() } else { crate::lights::room_lights(r.bb_min, r.bb_max) };
                if lit {
                    lit_rooms += 1;
                }
                if crate::lights::add_lights_to_ydr_xml(&path, &lights).is_ok() {
                    nl += lights.len();
                }
                let (c, r) = ((mn + mx) * 0.5, (mx - mn).len() * 0.5);
                archs.push(format!(
                    "  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"{SHELL_ARCH_LOD}\" />\n   <flags value=\"0\" />\n   <specialAttribute value=\"0\" />\n   <bbMin x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bbMax x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsCentre x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsRadius value=\"{r}\" />\n   <hdTextureDist value=\"15\" />\n   <name>{sn}</name>\n   <textureDictionary>{SHELL_TXD}</textureDictionary>\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>ASSET_TYPE_DRAWABLE</assetType>\n   <assetName>{sn}</assetName>\n   <extensions />\n  </Item>",
                    mn.x, mn.y, mn.z, mx.x, mx.y, mx.z, c.x, c.y, c.z
                ));
                n += 1;
            }
        }
        crate::design::set_custom_textures(&self.proj.custom_textures);
        // coques modelables (design.rs) : un modèle par pièce, textures dans « dw_design_txd », lumières au plafond
        // si la pièce n'a pas d'objet éclairant
        let mut dtex: Vec<(u64, TexData)> = Vec::new();
        let mut nd = 0;
        for f in std::fs::read_dir(stream).into_iter().flatten().flatten() {
            let n = f.file_name().to_string_lossy().to_lowercase();
            if n.starts_with(crate::design::DESIGN_PREFIX) && n.ends_with(".xml") && f.path().is_file() {
                let _ = std::fs::remove_file(f.path());
            }
        }
        let mut nmir = 0;
        for ((ti, mi), ri, name, mm) in self.design_models() {
            let (model, keys, t, (mn, mx)) = self.design_rmodel(ti, mi, &mm);
            let Ok(path) = crate::sollumz::write_ydr_xml_ex(stream, &name, &model, &keys, &t, Some(crate::design::DESIGN_TXD)) else { continue };
            if crate::design::mirror_shader(&path).unwrap_or(false) {
                nmir += 1;
            }
            for x in t {
                if !dtex.iter().any(|y| y.0 == x.0) {
                    dtex.push(x);
                }
            }
            let m = &self.proj.ytyps[ti].mlos[mi];
            // lumières posées dans cette pièce (pièce libre ou pas) : dans son modèle de coque modelable
            let placed: Vec<crate::lights::Light> = self.proj.designs.iter().find(|d| d.mlo.eq_ignore_ascii_case(&m.name)).map(|d| crate::design::room_design_lights(d, m, ri)).unwrap_or_default();
            let box_shell = m.rooms.get(ri).map(|r| !r.free).unwrap_or(false);
            if !placed.is_empty() && !box_shell {
                if crate::lights::add_lights_to_ydr_xml(&path, &placed).is_ok() {
                    nl += placed.len();
                }
            } else if let Some(r) = m.rooms.get(ri).filter(|r| r.free) {
                let lit = m.entities.iter().filter(|e| e.room == ri).any(|e| {
                    let h = crate::gta::crypto::joaat(&e.archetype);
                    *lit_cache.entry(h).or_insert_with(|| src.as_ref().map(|s| !s.lights(h).is_empty()).unwrap_or(false))
                });
                if !lit {
                    let lights = crate::lights::room_lights(r.bb_min, r.bb_max);
                    if crate::lights::add_lights_to_ydr_xml(&path, &lights).is_ok() {
                        nl += lights.len();
                    }
                }
            }
            let (c, r) = ((mn + mx) * 0.5, (mx - mn).len() * 0.5);
            archs.push(format!(
                "  <Item type=\"CBaseArchetypeDef\">
   <lodDist value=\"{SHELL_ARCH_LOD}\" />
   <flags value=\"0\" />
   <specialAttribute value=\"0\" />
   <bbMin x=\"{}\" y=\"{}\" z=\"{}\" />
   <bbMax x=\"{}\" y=\"{}\" z=\"{}\" />
   <bsCentre x=\"{}\" y=\"{}\" z=\"{}\" />
   <bsRadius value=\"{r}\" />
   <hdTextureDist value=\"15\" />
   <name>{name}</name>
   <textureDictionary>{}</textureDictionary>
   <clipDictionary />
   <drawableDictionary />
   <physicsDictionary />
   <assetType>ASSET_TYPE_DRAWABLE</assetType>
   <assetName>{name}</assetName>
   <extensions />
  </Item>",
                mn.x, mn.y, mn.z, mx.x, mx.y, mx.z, c.x, c.y, c.z, crate::design::DESIGN_TXD
            ));
            nd += 1;
        }
        if !dtex.is_empty() {
            let _ = crate::sollumz::write_ytd_xml(stream, crate::design::DESIGN_TXD, &dtex);
        }
        if archs.is_empty() {
            return (Vec::new(), String::new());
        }
        let _ = crate::sollumz::write_ytd_xml(stream, SHELL_TXD, &texs);
        let ytyp = crate::model::Ytyp { name: "dw_shell_types".into(), mlos: Vec::new(), visible: true, raw_archetypes: archs };
        let _ = std::fs::write(stream.join("dw_shell_types.ytyp.xml"), crate::xmlio::write_ytyp(&ytyp));
        let n = n + 0 * nd;
        let design_txt = if nd > 0 { format!(" Coques modelables : {nd} modèle(s) (murs, sols, plafonds, blocs, ombre cuite dans les sommets){}.", if nmir > 0 { format!(", miroirs dans {nmir} modèle(s)") } else { String::new() }) } else { String::new() };
        (vec![design_txt + &format!("Coques des intérieurs : {n} pièce(s) avec sol, murs (ouverts aux passages) et plafond ; collision complète avec le numéro de chaque pièce ; {nl} spot(s) au plafond ajouté(s) ; {lit_rooms} pièce(s) déjà éclairée(s) par les lumières de leurs objets du jeu.")], "data_file 'DLC_ITYP_REQUEST' 'stream/dw_shell_types.ytyp'\n".into())
    }
}

#[cfg(test)]
mod stair_tests {
    use super::*;
    use crate::model::{MloArchetype, Portal, Room};

    #[test]
    fn tremie_et_escalier() {
        let rooms = vec![Room::new("limbo", V3::ZERO, V3::ZERO), Room::new("bas", v3(0.0, 0.0, 0.0), v3(8.0, 6.0, 2.95)), Room::new("haut", v3(0.0, 0.0, 3.2), v3(8.0, 6.0, 6.15))];
        let z = 2.95;
        let portals = vec![Portal::new(1, 2, [v3(3.0, 0.0, z), v3(6.0, 0.0, z), v3(6.0, 1.2, z), v3(3.0, 1.2, z)])];
        let m = MloArchetype { name: "t".into(), lod_dist: 100.0, flags: 0, mlo_flags: 0, entities: vec![], rooms, portals, entity_sets: vec![] };
        let low = room_quads(&m, 1);
        // marches : chacune contremarche + marche + 2 joues ; la dernière arrive au sol de la pièce du dessus
        let treads: Vec<f32> = low.iter().filter(|q| q.0 == 0 && q.2.z > 0.5 && q.1[0].z > 0.01).map(|q| q.1[0].z).collect();
        assert!(treads.len() >= 15, "{} marches", treads.len());
        let top = treads.iter().cloned().fold(0.0f32, f32::max);
        assert!((top - 3.22).abs() < 0.05, "haut de l'escalier à {top}");
        // le plafond du bas est percé (plusieurs morceaux), le sol du haut aussi
        assert!(low.iter().filter(|q| q.0 == 2).count() >= 3);
        assert!(room_quads(&m, 2).iter().filter(|q| q.0 == 0).count() >= 3);
    }

    #[test]
    fn ouvertures_qui_se_chevauchent() {
        // porte (1 à 2 m, du sol à 2 m) et fenêtre haute qui la chevauche (1,5 à 3 m, de 1 à 2,5 m), mur de 5 × 3 m
        let holes = [(1.0, 2.0, 0.0, 2.0), (1.5, 3.0, 1.0, 2.5)];
        let pieces = wall_pieces(0.0, 5.0, 0.0, 3.0, &holes);
        let area: f32 = pieces.iter().map(|p| (p.1 - p.0) * (p.3 - p.2)).sum();
        // surface pleine = 15 − (porte 2 + fenêtre 2,25 − chevauchement 0,5)
        assert!((area - (15.0 - 3.75)).abs() < 1e-4, "{area} {pieces:?}");
        // aucun morceau plein dans une ouverture
        for p in &pieces {
            for h in &holes {
                let (u0, u1, z0, z1) = (p.0.max(h.0), p.1.min(h.1), p.2.max(h.2), p.3.min(h.3));
                assert!(u1 - u0 < 1e-4 || z1 - z0 < 1e-4, "{p:?} dans {h:?}");
            }
        }
    }
}
