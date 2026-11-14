//! Coque modelable d'un intérieur créé dans DragoWalker, dessinée dans le plan vu de dessus (onglet Intérieur) :
//! - **murs** tracés point par point (épaisseur, hauteur de l'étage, angles libres), avec leur texture, leur couleur et
//!   des plinthes ;
//! - **ouvertures** posées d'un clic sur un mur (porte, porte double, fenêtre, passage) : le mur est percé et un
//!   passage (portail) relie les deux pièces de part et d'autre (ou la pièce et l'extérieur) ;
//! - **pièces de forme libre** : un polygone cliqué devient une pièce de l'intérieur (sa boîte = le rectangle qui
//!   l'entoure), avec son sol (texture au choix), son plafond (faux plafond possible) et, au choix, des murs autour ;
//! - **blocs maçonnés** : comptoir, banquette, estrade, muret, colonne.
//!
//! Chaque pièce reçoit un modèle « dw_dsg_<intérieur>_<pièce> » (objet de la pièce) : chaque face va à la pièce du
//! côté où elle se voit (le jeu ne dessine une pièce qu'à travers ses passages). La collision des pièces (numéro de
//! pièce compris) reprend les mêmes faces. Textures : celles de l'atelier (dictionnaire « dw_design_txd »).
//!
//! Mesures par défaut (épaisseur 15 cm, plinthes 10 cm, hauteurs des blocs, tailles des ouvertures) : **estimées**.

use crate::app::DwApp;
use crate::atelier::{self, Mesher, C_NONE, C_WALL};
use crate::gta::drawable::{RModel, TexData};
use crate::math::{v3, V3};
use crate::model::{self, MloArchetype};
use serde::{Deserialize, Serialize};

pub const DESIGN_PREFIX: &str = "dw_dsg_";
pub const DESIGN_TXD: &str = "dw_design_txd";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Design {
    /// intérieur (nom du type MLO)
    pub mlo: String,
    #[serde(default)]
    pub walls: Vec<DWall>,
    #[serde(default)]
    pub rooms: Vec<DRoom>,
    #[serde(default)]
    pub blocks: Vec<DBlock>,
    #[serde(default)]
    pub lights: Vec<DLight>,
    #[serde(default)]
    pub mirrors: Vec<DMirror>,
    /// éclairage cuit : ombre des coins dans les couleurs des sommets (comme les coques des MLO FiveM)
    #[serde(default = "yes")]
    pub baked: bool,
}

fn yes() -> bool {
    true
}

/// Miroir : rectangle (coins dans l'ordre bas-gauche, haut-gauche, haut-droit, bas-droit, repère de l'intérieur)
/// collé à 1 cm d'un mur, et la direction vers laquelle il regarde.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct DMirror {
    pub c: [[f32; 3]; 4],
    pub n: [f32; 2],
}

/// Miroir : passage « miroir » (drapeau 4 des passages d'intérieur, d'après Sollumz ; les MLO analysés utilisent
/// bien 64 « cacher porte fermée » et 8192 « pas de coupure au loin » sur leurs passages) — **à vérifier en jeu**.
pub const PORTAL_MIRROR: u32 = 4;
/// Taille par défaut d'un miroir posé : largeur, hauteur, bas au-dessus du sol (m) — **estimé**.
pub const MIRROR_SIZE: (f32, f32, f32) = (0.9, 1.1, 1.0);

/// Lumière posée sans objet (rangée dans le modèle de la pièce, comme les lumières des intérieurs du jeu) : position
/// (repère de l'intérieur), genre (LIGHT_KINDS), couleur, intensité, portée.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct DLight {
    pub p: [f32; 3],
    pub kind: u8,
    pub color: [u8; 3],
    pub intensity: f32,
    pub range: f32,
}

/// Lumières : (nom, spot vers le bas ?, hauteur sous le plafond (m) ; une ampoule est à 2,2 m du sol).
pub const LIGHT_KINDS: [(&str, bool); 3] = [("Spot au plafond", true), ("Ampoule", false), ("Néon / ruban", false)];

/// Texture importée : nom, image PNG (en base64, au plus 512 px de côté), taille d'un motif sur le mur (m).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CustomTex {
    pub name: String,
    pub png: String,
    pub size: f32,
}

/// Numéro de texture des murs et des sols à partir duquel ce sont les textures importées (100 + rang).
pub const CUSTOM_BASE: u8 = 100;

static CUSTOM: std::sync::RwLock<Vec<(u64, f32, Option<TexData>)>> = std::sync::RwLock::new(Vec::new());

/// Clé de texture d'une texture importée (d'après son nom : stable d'un export à l'autre).
pub fn custom_key(name: &str) -> u64 {
    let mut h: u32 = 0x811C_9DC5;
    for b in name.to_lowercase().bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    0xD7E7_0000_0000_0000 | (h | 0x8000_0000) as u64
}

/// Charge les textures importées du projet (à chaque changement de la liste).
pub fn set_custom_textures(list: &[CustomTex]) {
    let v: Vec<(u64, f32, Option<TexData>)> = list.iter().map(|c| (custom_key(&c.name), c.size.max(0.1), decode_png_tex(&c.png))).collect();
    if let Ok(mut g) = CUSTOM.write() {
        *g = v;
    }
}

/// Clé, taille du motif et image d'une texture importée (par rang).
fn custom_at(i: usize) -> Option<(u64, f32, Option<TexData>)> {
    CUSTOM.read().ok()?.get(i).cloned()
}

fn custom_by_key(k: u64) -> Option<(f32, Option<TexData>)> {
    CUSTOM.read().ok()?.iter().find(|c| c.0 == k).map(|c| (c.1, c.2.clone()))
}

fn decode_png_tex(b64: &str) -> Option<TexData> {
    let bytes = b64_decode(b64)?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(crate::roads::with_mips(w, h, img.into_raw()))
}

/// Lit une image PNG, la met à une taille puissance de 2 (au plus 512 px de côté) et la garde en base64.
pub fn import_png(path: &std::path::Path) -> Result<CustomTex, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| format!("image illisible ({e}) : seules les images PNG sont prises"))?.to_rgba8();
    let pow = |v: u32| -> u32 { let mut p = 1; while p * 2 <= v.min(512) { p *= 2; } p.max(4) };
    let (w, h) = (pow(img.width()), pow(img.height()));
    let img = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "texture".into());
    Ok(CustomTex { name, png: b64_encode(&out), size: 2.0 })
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(d: &[u8]) -> String {
    let mut s = String::with_capacity(d.len() * 4 / 3 + 4);
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                s.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' => continue,
            _ => return None,
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xFF) as u8);
        }
    }
    Some(out)
}

/// Mur : de a à b (repère de l'intérieur), pied à z0, hauteur, épaisseur, texture (WALL_TEX), couleur, plinthes.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DWall {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub z0: f32,
    pub h: f32,
    pub t: f32,
    #[serde(default)]
    pub tex: u8,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    #[serde(default)]
    pub base: bool,
    #[serde(default)]
    pub holes: Vec<DHole>,
    /// texture et couleur de l'autre côté (côté -normale) ; absent : la même que ce côté-ci
    #[serde(default)]
    pub back: Option<(u8, Option<[u8; 3]>)>,
}

/// Ouverture d'un mur : milieu le long du mur (depuis a), largeur, allège (au-dessus du pied), hauteur, genre.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct DHole {
    pub s: f32,
    pub w: f32,
    pub sill: f32,
    pub h: f32,
    pub kind: u8,
}

/// Pièce de forme libre : pièce de l'intérieur (par son nom), contour, sol à z0, hauteur sous plafond, sol (FLOOR_TEX),
/// couleur du plafond, faux plafond (descente sous le plafond).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DRoom {
    pub room: String,
    pub pts: Vec<[f32; 2]>,
    pub z0: f32,
    pub h: f32,
    #[serde(default)]
    pub floor: u8,
    #[serde(default)]
    pub ceil_color: Option<[u8; 3]>,
    #[serde(default)]
    pub drop: f32,
}

/// Bloc maçonné : rectangle a-b, pied à z0, hauteur, genre (BLOCK_KINDS), texture (WALL_TEX), couleur.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DBlock {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub z0: f32,
    pub h: f32,
    #[serde(default)]
    pub kind: u8,
    #[serde(default)]
    pub tex: u8,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}

/// Textures des murs et des blocs (celles de l'atelier).
pub const WALL_TEX: [&str; 6] = ["Enduit", "Brique", "Béton", "Bois", "Carrelage", "Pierre"];
/// Textures des sols.
pub const FLOOR_TEX: [&str; 4] = ["Parquet", "Carrelage", "Moquette", "Béton ciré"];
/// Ouvertures : (nom, largeur, hauteur, allège) (m) — **estimé**.
pub const HOLE_KINDS: [(&str, f32, f32, f32); 4] = [("Porte", 0.9, 2.1, 0.0), ("Porte double", 1.6, 2.1, 0.0), ("Fenêtre", 1.2, 1.3, 0.9), ("Passage", 1.6, 2.5, 0.0)];
/// Blocs : (nom, hauteur ; 0 = jusqu'au plafond) (m) — **estimé**.
pub const BLOCK_KINDS: [(&str, f32); 5] = [("Comptoir", 0.95), ("Banquette", 0.45), ("Estrade", 0.2), ("Muret", 1.1), ("Colonne", 0.0)];
/// Plinthes : hauteur, saillie (m).
const BASE_H: f32 = 0.1;
const BASE_D: f32 = 0.015;
/// Sol d'une pièce libre au-dessus de son pied (m).
const FLOOR_LIFT: f32 = 0.03;

pub fn wall_key(tex: u8, color: Option<[u8; 3]>) -> u64 {
    if tex >= CUSTOM_BASE {
        if let Some(c) = custom_at((tex - CUSTOM_BASE) as usize) {
            return c.0;
        }
    }
    let base = match tex {
        1 => atelier::k_facade(0),
        2 => atelier::k_facade(2),
        3 => atelier::k_facade(3),
        4 => atelier::K_FLOOR_TILE,
        5 => atelier::K_PLINTH,
        _ => atelier::K_INNER,
    };
    match color {
        Some(c) => atelier::tint_key(base, c),
        None => base,
    }
}

pub fn floor_key(i: u8) -> u64 {
    if i >= CUSTOM_BASE {
        if let Some(c) = custom_at((i - CUSTOM_BASE) as usize) {
            return c.0;
        }
    }
    match i {
        1 => atelier::K_FLOOR_TILE,
        2 => atelier::K_FLOOR_CARPET,
        3 => atelier::K_FLOOR_CONCRETE,
        _ => atelier::K_FLOOR,
    }
}

/// Le point est-il dans le polygone ?
pub fn in_poly(pts: &[[f32; 2]], x: f32, y: f32) -> bool {
    let n = pts.len();
    let mut c = false;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
    }
    c
}

/// Pièce de l'intérieur qui contient ce point (pièce libre par son contour, sinon par sa boîte) ; 0 si aucune.
pub fn room_at(d: &Design, m: &MloArchetype, p: V3) -> usize {
    for dr in &d.rooms {
        if p.z >= dr.z0 - 0.3 && p.z <= dr.z0 + dr.h + 0.3 && in_poly(&dr.pts, p.x, p.y) {
            if let Some(i) = m.rooms.iter().position(|r| r.name.eq_ignore_ascii_case(&dr.room)) {
                return i;
            }
        }
    }
    (1..m.rooms.len())
        .find(|&i| {
            let r = &m.rooms[i];
            !r.free && p.x >= r.bb_min.x && p.x <= r.bb_max.x && p.y >= r.bb_min.y && p.y <= r.bb_max.y && p.z >= r.bb_min.z - 0.3 && p.z <= r.bb_max.z + 0.3
        })
        .unwrap_or(0)
}

/// Pièce la plus proche à cette hauteur (pour une face qui tombe entre deux pièces) ; 0 si aucune.
fn room_near(d: &Design, m: &MloArchetype, p: V3) -> usize {
    let r = room_at(d, m, p);
    if r != 0 {
        return r;
    }
    (1..m.rooms.len())
        .filter(|&i| p.z >= m.rooms[i].bb_min.z - 0.3 && p.z <= m.rooms[i].bb_max.z + 0.3)
        .min_by(|&i, &j| {
            let dist = |k: usize| { let r = &m.rooms[k]; v3(p.x.clamp(r.bb_min.x, r.bb_max.x) - p.x, p.y.clamp(r.bb_min.y, r.bb_max.y) - p.y, 0.0).len() };
            dist(i).partial_cmp(&dist(j)).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0)
}

/// Rectangles des trémies (portails horizontaux) d'une pièce à la hauteur z (à 0,35 m près).
fn flat_holes(m: &MloArchetype, ri: usize, z: f32) -> Vec<(f32, f32, f32, f32)> {
    m.portals
        .iter()
        .filter(|p| p.room_from as usize == ri || p.room_to as usize == ri)
        .filter(|p| p.corners.iter().all(|c| (c.z - z).abs() < 0.35))
        .map(|p| {
            let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for c in p.corners {
                x0 = x0.min(c.x);
                x1 = x1.max(c.x);
                y0 = y0.min(c.y);
                y1 = y1.max(c.y);
            }
            (x0, x1, y0, y1)
        })
        .collect()
}

/// Morceaux convexes d'un polygone sans des rectangles.
fn minus_rects(poly: &[[f32; 2]], rects: &[(f32, f32, f32, f32)]) -> Vec<Vec<[f32; 2]>> {
    let mut pieces = vec![poly.to_vec()];
    for &(x0, x1, y0, y1) in rects {
        pieces = pieces
            .into_iter()
            .flat_map(|pc| {
                let mid = atelier::clip_half(&atelier::clip_half(&pc, &|p| p[0] - x0), &|p| x1 - p[0]);
                vec![atelier::clip_half(&pc, &|p| x0 - p[0]), atelier::clip_half(&pc, &|p| p[0] - x1), atelier::clip_half(&mid, &|p| y0 - p[1]), atelier::clip_half(&mid, &|p| p[1] - y1)]
            })
            .filter(|p| p.len() >= 3)
            .collect();
    }
    pieces
}

/// Maillage de la coque : un Mesher par pièce de l'intérieur (indice de pièce > 0).
pub fn design_meshes(d: &Design, m: &MloArchetype) -> Vec<(usize, Mesher)> {
    let mut out: Vec<(usize, Mesher)> = Vec::new();
    fn get(out: &mut Vec<(usize, Mesher)>, ri: usize) -> &mut Mesher {
        if let Some(i) = out.iter().position(|x| x.0 == ri) {
            return &mut out[i].1;
        }
        out.push((ri, Mesher::default()));
        &mut out.last_mut().unwrap().1
    }
    let up = v3(0.0, 0.0, 1.0);
    // ---- murs
    for w in &d.walls {
        let (a, b) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
        let len = (b - a).len();
        if len < 0.05 {
            continue;
        }
        let dir = (b - a) * (1.0 / len);
        let n = v3(-dir.y, dir.x, 0.0);
        let ht = w.t.max(0.02) * 0.5;
        let (s0, s1) = (-ht, len + ht); // prolongé d'une demi-épaisseur : les angles se referment
        let (z0, z1) = (w.z0, w.z0 + w.h);
        let p = |s: f32, o: f32, z: f32| { let q = a + dir * s + n * o; v3(q.x, q.y, z) };
        let key = wall_key(w.tex, w.color);
        let key_b = w.back.map(|(t, c)| wall_key(t, c)).unwrap_or(key);
        let holes: Vec<(f32, f32, f32, f32)> = w.holes.iter().map(|h| (h.s - h.w * 0.5, h.s + h.w * 0.5, z0 + h.sill, (z0 + h.sill + h.h).min(z1))).collect();
        let pieces = crate::shell::wall_pieces(s0, s1, z0, z1, &holes);
        let mid = (z0 + z1) * 0.5;
        for side in [1.0f32, -1.0] {
            let probe = p(len * 0.5, side * (ht + 0.25), mid);
            let ri = room_near(d, m, probe);
            if ri == 0 {
                continue;
            }
            let mm = get(&mut out, ri);
            let sk = if side > 0.0 { key } else { key_b };
            for &(a0, a1, za, zb) in &pieces {
                mm.quad([p(a0, side * ht, za), p(a1, side * ht, za), p(a1, side * ht, zb), p(a0, side * ht, zb)], n * side, sk, C_WALL);
            }
            // plinthes, sauf sous les portes et passages
            if w.base {
                let mut segs = vec![(s0, s1)];
                for h in holes.iter().filter(|h| h.2 - z0 < 0.05) {
                    segs = segs.into_iter().flat_map(|(x0, x1)| { let mut v = Vec::new(); if h.0 > x0 { v.push((x0, h.0.min(x1))); } if h.1 < x1 { v.push((h.1.max(x0), x1)); } v }).filter(|s| s.1 - s.0 > 0.02).collect();
                }
                let bk = atelier::k_facade(3);
                let o = side * (ht + BASE_D);
                for (x0, x1) in segs {
                    mm.quad([p(x0, o, z0), p(x1, o, z0), p(x1, o, z0 + BASE_H), p(x0, o, z0 + BASE_H)], n * side, bk, C_NONE);
                    mm.quad([p(x0, side * ht, z0 + BASE_H), p(x1, side * ht, z0 + BASE_H), p(x1, o, z0 + BASE_H), p(x0, o, z0 + BASE_H)], up, bk, C_NONE);
                }
            }
        }
        // dessus, bouts et embrasures : à la pièce d'un côté
        let ri = { let r = room_near(d, m, p(len * 0.5, ht + 0.25, mid)); if r != 0 { r } else { room_near(d, m, p(len * 0.5, -ht - 0.25, mid)) } };
        if ri == 0 {
            continue;
        }
        let mm = get(&mut out, ri);
        mm.quad([p(s0, -ht, z1), p(s1, -ht, z1), p(s1, ht, z1), p(s0, ht, z1)], up, key, C_WALL);
        mm.quad([p(s0, -ht, z0), p(s0, ht, z0), p(s0, ht, z1), p(s0, -ht, z1)], dir * -1.0, key, C_WALL);
        mm.quad([p(s1, -ht, z0), p(s1, ht, z0), p(s1, ht, z1), p(s1, -ht, z1)], dir, key, C_WALL);
        for &(h0, h1, hz0, hz1) in &holes {
            mm.quad([p(h0, -ht, hz0), p(h0, ht, hz0), p(h0, ht, hz1), p(h0, -ht, hz1)], dir, key, C_WALL);
            mm.quad([p(h1, -ht, hz0), p(h1, ht, hz0), p(h1, ht, hz1), p(h1, -ht, hz1)], dir * -1.0, key, C_WALL);
            mm.quad([p(h0, -ht, hz1), p(h1, -ht, hz1), p(h1, ht, hz1), p(h0, ht, hz1)], up * -1.0, key, C_WALL);
            if hz0 > z0 + 0.01 {
                mm.quad([p(h0, -ht, hz0), p(h1, -ht, hz0), p(h1, ht, hz0), p(h0, ht, hz0)], up, key, C_WALL);
            }
        }
    }
    // ---- pièces libres : sol et plafond, percés des trémies
    for dr in &d.rooms {
        let Some(ri) = m.rooms.iter().position(|r| r.name.eq_ignore_ascii_case(&dr.room)) else { continue };
        if dr.pts.len() < 3 {
            continue;
        }
        let mut poly = dr.pts.clone();
        if atelier::signed_area(&poly) < 0.0 {
            poly.reverse();
        }
        let zc = dr.z0 + dr.h - dr.drop.max(0.0);
        let floor_holes = flat_holes(m, ri, dr.z0);
        let ceil_holes = flat_holes(m, ri, dr.z0 + dr.h);
        let mm = get(&mut out, ri);
        let ck = match dr.ceil_color {
            Some(c) => atelier::tint_key(atelier::K_INNER, c),
            None => atelier::K_INNER,
        };
        // sol 3 cm au-dessus du pied de la pièce (sinon le sol du bâtiment ou du terrain dessous le traverse)
        for (z, holes, want, key) in [(dr.z0 + FLOOR_LIFT, &floor_holes, up, floor_key(dr.floor)), (zc, &ceil_holes, up * -1.0, ck)] {
            for t in atelier::triangulate(&poly) {
                let tri: Vec<[f32; 2]> = t.iter().map(|&k| poly[k]).collect();
                for pc in minus_rects(&tri, holes) {
                    for k in 1..pc.len() - 1 {
                        mm.tri([v3(pc[0][0], pc[0][1], z), v3(pc[k][0], pc[k][1], z), v3(pc[k + 1][0], pc[k + 1][1], z)], want, key, C_WALL);
                    }
                }
            }
        }
        // bord du faux plafond : bandeau vertical jusqu'au vrai plafond
        if dr.drop > 0.01 {
            let ztop = dr.z0 + dr.h;
            for i in 0..poly.len() {
                let (pa, pb) = (poly[i], poly[(i + 1) % poly.len()]);
                let (a, b) = (v3(pa[0], pa[1], 0.0), v3(pb[0], pb[1], 0.0));
                let e = b - a;
                let inward = v3(-e.y, e.x, 0.0);
                mm.quad([v3(a.x, a.y, zc), v3(b.x, b.y, zc), v3(b.x, b.y, ztop), v3(a.x, a.y, ztop)], inward, ck, C_NONE);
            }
        }
    }
    // ---- blocs
    for bl in &d.blocks {
        let (x0, x1, y0, y1) = (bl.a[0].min(bl.b[0]), bl.a[0].max(bl.b[0]), bl.a[1].min(bl.b[1]), bl.a[1].max(bl.b[1]));
        if x1 - x0 < 0.05 || y1 - y0 < 0.05 || bl.h < 0.02 {
            continue;
        }
        let c = v3((x0 + x1) * 0.5, (y0 + y1) * 0.5, bl.z0 + 0.1);
        let ri = room_near(d, m, c);
        if ri == 0 {
            continue;
        }
        let key = wall_key(bl.tex, bl.color);
        let (za, zb) = (bl.z0, bl.z0 + bl.h);
        let mm = get(&mut out, ri);
        let q = |x: f32, y: f32, z: f32| v3(x, y, z);
        mm.quad([q(x0, y0, zb), q(x1, y0, zb), q(x1, y1, zb), q(x0, y1, zb)], up, key, C_WALL);
        mm.quad([q(x0, y0, za), q(x1, y0, za), q(x1, y0, zb), q(x0, y0, zb)], v3(0.0, -1.0, 0.0), key, C_WALL);
        mm.quad([q(x0, y1, za), q(x1, y1, za), q(x1, y1, zb), q(x0, y1, zb)], v3(0.0, 1.0, 0.0), key, C_WALL);
        mm.quad([q(x0, y0, za), q(x0, y1, za), q(x0, y1, zb), q(x0, y0, zb)], v3(-1.0, 0.0, 0.0), key, C_WALL);
        mm.quad([q(x1, y0, za), q(x1, y1, za), q(x1, y1, zb), q(x1, y0, zb)], v3(1.0, 0.0, 0.0), key, C_WALL);
    }
    // ---- miroirs : surface à part (matériau miroir à l'export), sans collision (le mur derrière en a une)
    for mi in &d.mirrors {
        let c: Vec<V3> = mi.c.iter().map(|p| v3(p[0], p[1], p[2])).collect();
        let n = v3(mi.n[0], mi.n[1], 0.0);
        let ctr = (c[0] + c[2]) * 0.5;
        let ri = room_near(d, m, ctr + n * 0.3);
        if ri == 0 {
            continue;
        }
        let mm = get(&mut out, ri);
        mm.quad([c[0], c[3], c[2], c[1]], n, atelier::K_MIRROR, C_NONE);
    }
    // textures importées : un motif fait `size` m (le maillage compte 3 m par motif)
    for (_, mm) in out.iter_mut() {
        for pi in 0..mm.parts.len() {
            let Some((size, _)) = custom_by_key(mm.parts[pi].0) else { continue };
            let f = 3.0 / size;
            let mut seen = std::collections::HashSet::new();
            for &i in &mm.parts[pi].1 {
                if seen.insert(i) {
                    let uv = &mut mm.uv[i as usize];
                    uv[0] *= f;
                    uv[1] *= f;
                }
            }
        }
    }
    out
}

/// Lumières de la coque dans la pièce `ri` (repère de l'intérieur), prêtes à ranger dans un modèle.
pub fn room_design_lights(d: &Design, m: &MloArchetype, ri: usize) -> Vec<crate::lights::Light> {
    d.lights
        .iter()
        .filter(|l| room_near(d, m, v3(l.p[0], l.p[1], l.p[2] - 0.5)) == ri)
        .map(|l| crate::lights::Light { pos: v3(l.p[0], l.p[1], l.p[2]), falloff: l.range.max(1.0), colour: Some(l.color), intensity: Some(l.intensity.max(0.1)), point: !LIGHT_KINDS[(l.kind as usize).min(2)].1 })
        .collect()
}

/// Découpe les triangles en morceaux d'au plus `max` m de côté (l'ombre cuite dans les sommets se dessine alors
/// le long des coins au lieu de s'étaler sur tout le mur).
pub fn subdivide(mm: &Mesher, max: f32) -> Mesher {
    let mut out = Mesher::default();
    for (key, idx, coll) in &mm.parts {
        let mut list: Vec<u32> = Vec::new();
        let mut stack: Vec<[(V3, [f32; 2]); 3]> = idx.chunks_exact(3).map(|t| [0, 1, 2].map(|k| (mm.pos[t[k] as usize], mm.uv[t[k] as usize]))).collect();
        let mut guard = 0;
        while let Some(t) = stack.pop() {
            guard += 1;
            let e = [(t[1].0 - t[0].0).len(), (t[2].0 - t[1].0).len(), (t[0].0 - t[2].0).len()];
            let (k, l) = e.iter().enumerate().fold((0, 0.0f32), |b, (i, &v)| if v > b.1 { (i, v) } else { b });
            if l > max && guard < 200_000 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let mid = ((a.0 + b.0) * 0.5, [(a.1[0] + b.1[0]) * 0.5, (a.1[1] + b.1[1]) * 0.5]);
                let o = t[(k + 2) % 3];
                stack.push([a, mid, o]);
                stack.push([mid, b, o]);
                continue;
            }
            let base = out.pos.len() as u32;
            for (p, uv) in t {
                out.pos.push(p);
                out.uv.push(uv);
            }
            list.extend_from_slice(&[base, base + 1, base + 2]);
        }
        out.parts.push((*key, list, *coll));
    }
    out
}

/// Ombre cuite d'un sommet (0,5 = très sombre, 1 = pleine lumière) : coins entre murs, pied et haut des murs, bord
/// des blocs — approximation de l'occultation ambiante des coques des MLO FiveM (couleur des sommets).
pub fn baked_ao(d: &Design, m: &MloArchetype, p: V3, n: V3) -> f32 {
    let mut occ = 0.0f32;
    // sol et plafond de la pièce où tombe le point
    let ri = room_near(d, m, p);
    let (zf, zc) = match d.rooms.iter().find(|r| p.z >= r.z0 - 0.3 && p.z <= r.z0 + r.h + 0.3 && in_poly(&r.pts, p.x, p.y)) {
        Some(r) => (r.z0, r.z0 + r.h - r.drop.max(0.0)),
        None => m.rooms.get(ri).map(|r| (r.bb_min.z, r.bb_max.z)).unwrap_or((p.z - 10.0, p.z + 10.0)),
    };
    if n.z < 0.5 {
        occ += 0.30 * (-(p.z - zf).max(0.0) / 0.30).exp();
    }
    if n.z > -0.5 {
        occ += 0.18 * (-(zc - p.z).max(0.0) / 0.35).exp();
    }
    // murs voisins (pas celui dont le point est la face)
    for w in &d.walls {
        if p.z < w.z0 - 0.1 || p.z > w.z0 + w.h + 0.1 {
            continue;
        }
        let (a, b) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
        let len = (b - a).len().max(1e-3);
        let dir = (b - a) * (1.0 / len);
        let wn = v3(-dir.y, dir.x, 0.0);
        let q = v3(p.x, p.y, 0.0) - a;
        let s = q.dot(dir);
        let perp = q.dot(wn).abs() - w.t * 0.5;
        let along = if s < -w.t * 0.5 { -w.t * 0.5 - s } else if s > len + w.t * 0.5 { s - len - w.t * 0.5 } else { 0.0 };
        if perp.abs() < 0.03 && n.dot(wn).abs() > 0.8 && along == 0.0 {
            continue; // sa propre face
        }
        let dist = (perp.max(0.0).powi(2) + along * along).sqrt();
        occ += 0.28 * (-dist / 0.35).exp();
    }
    for bl in &d.blocks {
        if p.z > bl.z0 + bl.h + 0.05 {
            continue;
        }
        let (x0, x1, y0, y1) = (bl.a[0].min(bl.b[0]), bl.a[0].max(bl.b[0]), bl.a[1].min(bl.b[1]), bl.a[1].max(bl.b[1]));
        let dx = (x0 - p.x).max(p.x - x1).max(0.0);
        let dy = (y0 - p.y).max(p.y - y1).max(0.0);
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 0.02 && n.z.abs() < 0.5 {
            continue; // sa propre face
        }
        occ += 0.22 * (-dist / 0.25).exp();
    }
    (1.0 - occ).clamp(0.5, 1.0)
}

/// Modèle prêt pour la carte graphique ou l'export, avec l'ombre cuite dans les couleurs des sommets si la coque la
/// demande.
pub fn mesher_rmodel_baked(mm: &Mesher, d: &Design, m: &MloArchetype) -> (RModel, Vec<u64>, Vec<(u64, TexData)>, (V3, V3)) {
    if !d.baked {
        return mesher_rmodel(mm);
    }
    let fine = subdivide(mm, 0.5);
    let (mut model, keys, texs, bx) = mesher_rmodel(&fine);
    for v in model.verts.iter_mut() {
        let ao = baked_ao(d, m, v3(v.pos[0], v.pos[1], v.pos[2]), v3(v.nrm[0], v.nrm[1], v.nrm[2]));
        let c = (ao * 255.0).round() as u8;
        v.col = [c, c, c, 255];
    }
    (model, keys, texs, bx)
}

/// Matériau miroir à l'export : le matériau de la texture du miroir devient « mirror_default » (shader des miroirs
/// du jeu, paramètres relevés dans ShadersGen9Conversion.xml de CodeWalker ; valeurs **estimées**).
pub fn mirror_shader(path: &std::path::Path) -> Result<bool, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let tn = crate::sollumz::shared_tex_name(DESIGN_TXD, atelier::K_MIRROR);
    let Some(t) = s.find(&format!("<Name>{tn}</Name>")) else { return Ok(false) };
    let Some(item_start) = s[..t].rfind("   <Item>\n    <Name>") else { return Ok(false) };
    let item_end = s[t..].find("   </Item>").map(|e| t + e).unwrap_or(t);
    let mut item = s[item_start..item_end].to_string();
    item = item.replacen("<Name>default</Name>\n    <FileName>default.sps</FileName>", "<Name>mirror_default</Name>\n    <FileName>mirror_default.sps</FileName>", 1);
    if let Some(p) = item.rfind("    </Parameters>") {
        item.insert_str(
            p,
            concat!(
                "     <Item name=\"specularFresnel\" type=\"Vector\" x=\"0.97\" y=\"0\" z=\"0\" w=\"0\" />\n",
                "     <Item name=\"specularFalloffMult\" type=\"Vector\" x=\"100\" y=\"0\" z=\"0\" w=\"0\" />\n",
                "     <Item name=\"specularIntensityMult\" type=\"Vector\" x=\"1\" y=\"0\" z=\"0\" w=\"0\" />\n",
                "     <Item name=\"gMirrorBounds\" type=\"Vector\" x=\"0\" y=\"0\" z=\"0\" w=\"0\" />\n",
            ),
        );
    }
    let out = format!("{}{}{}", &s[..item_start], item, &s[item_end..]);
    std::fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Modèle prêt pour la carte graphique ou l'export (textures de l'atelier, teintes comprises), et sa boîte.
pub fn mesher_rmodel(mm: &Mesher) -> (RModel, Vec<u64>, Vec<(u64, TexData)>, (V3, V3)) {
    let used: std::collections::HashSet<u64> = mm.parts.iter().map(|p| p.0).collect();
    let mut texs: Vec<(u64, TexData)> = atelier::house_textures().iter().filter(|t| used.contains(&t.0)).cloned().collect();
    for k in &used {
        if atelier::tint_of(*k).is_some() && !texs.iter().any(|t| t.0 == *k) {
            if let Some(t) = atelier::tinted_cached(*k) {
                texs.push((*k, t));
            }
        }
        if let Some((_, Some(t))) = custom_by_key(*k) {
            if !texs.iter().any(|x| x.0 == *k) {
                texs.push((*k, t));
            }
        }
    }
    let (mut verts, mut idx, mut geoms, mut keys) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    crate::terrain::append_mesh(&mut verts, &mut idx, &mut geoms, &mut keys, &texs, &mm.pos, &mm.uv, &mm.parts);
    let big = 3.0e38f32;
    let (mut mn, mut mx) = (v3(big, big, big), v3(-big, -big, -big));
    for p in &mm.pos {
        mn = mn.min(*p);
        mx = mx.max(*p);
    }
    (RModel { verts, idx, geoms, embedded_dict: None }, keys, texs, (mn, mx))
}

pub fn design_name(mlo: &str, ri: usize) -> String {
    format!("{DESIGN_PREFIX}{}_{ri}", mlo.to_lowercase())
}

impl DwApp {
    /// Coque modelable d'un intérieur (créée vide au besoin).
    pub fn design_mut(&mut self, mlo: &str) -> &mut Design {
        if let Some(i) = self.proj.designs.iter().position(|d| d.mlo.eq_ignore_ascii_case(mlo)) {
            return &mut self.proj.designs[i];
        }
        self.proj.designs.push(Design { mlo: mlo.to_string(), baked: true, ..Default::default() });
        self.proj.designs.last_mut().unwrap()
    }

    /// Modèle prêt (éclairage cuit compris) d'une coque modelable.
    pub fn design_rmodel(&self, ti: usize, mi: usize, mm: &Mesher) -> (RModel, Vec<u64>, Vec<(u64, TexData)>, (V3, V3)) {
        let m = &self.proj.ytyps[ti].mlos[mi];
        match self.proj.designs.iter().find(|d| d.mlo.eq_ignore_ascii_case(&m.name)) {
            Some(d) => mesher_rmodel_baked(mm, d, m),
            None => mesher_rmodel(mm),
        }
    }

    /// Modèles des coques modelables : (intérieur (ti, mi), pièce, nom, maillage).
    pub fn design_models(&self) -> Vec<((usize, usize), usize, String, Mesher)> {
        let mut out = Vec::new();
        for d in &self.proj.designs {
            let Some((ti, mi)) = self.find_mlo_index(&d.mlo) else { continue };
            let m = &self.proj.ytyps[ti].mlos[mi];
            for (ri, mm) in design_meshes(d, m) {
                if !mm.parts.is_empty() {
                    out.push(((ti, mi), ri, design_name(&m.name, ri), mm));
                }
            }
        }
        out
    }

    fn find_mlo_index(&self, name: &str) -> Option<(usize, usize)> {
        for (ti, t) in self.proj.ytyps.iter().enumerate() {
            if let Some(mi) = t.mlos.iter().position(|m| m.name.eq_ignore_ascii_case(name)) {
                return Some((ti, mi));
            }
        }
        None
    }

    /// Objets « coque modelable » des pièces (un par pièce qui a des murs, un sol ou des blocs) ; ceux qui ne
    /// correspondent plus à rien partent.
    pub fn design_ensure_entities(&mut self) {
        let models = self.design_models();
        let mut want: std::collections::HashMap<(usize, usize), Vec<(usize, String)>> = std::collections::HashMap::new();
        for (k, ri, n, _) in &models {
            want.entry(*k).or_default().push((*ri, n.clone()));
        }
        for t in self.proj.ytyps.iter_mut().enumerate() {
            for (mi, m) in t.1.mlos.iter_mut().enumerate() {
                let w = want.get(&(t.0, mi)).cloned().unwrap_or_default();
                m.entities.retain(|e| !e.archetype.starts_with(DESIGN_PREFIX) || w.iter().any(|(ri, n)| *n == e.archetype && *ri == e.room));
                for (ri, n) in w {
                    if !m.entities.iter().any(|e| e.archetype == n) {
                        let mut e = model::Entity::new(&n, V3::ZERO);
                        e.flags = crate::mech::MLO_ENTITY_FLAGS;
                        e.room = ri;
                        m.entities.push(e);
                    }
                }
            }
        }
    }

    /// Triangles de collision des coques modelables d'un intérieur : (pièce, triangle).
    pub fn design_collision(&self, m: &MloArchetype) -> Vec<(usize, [V3; 3])> {
        let Some(d) = self.proj.designs.iter().find(|d| d.mlo.eq_ignore_ascii_case(&m.name)) else { return Vec::new() };
        let mut out = Vec::new();
        for (ri, mm) in design_meshes(d, m) {
            for (_, idx, coll) in &mm.parts {
                if *coll == C_NONE {
                    continue;
                }
                for t in idx.chunks_exact(3) {
                    out.push((ri, [mm.pos[t[0] as usize], mm.pos[t[1] as usize], mm.pos[t[2] as usize]]));
                }
            }
        }
        out
    }

    /// Passage pour une ouverture (porte, fenêtre, passage) du mur `wi` : entre les pièces de part et d'autre (ou la
    /// pièce et l'extérieur), dans le plan du milieu du mur. Rend un compte rendu.
    pub fn design_hole_portal(&mut self, ti: usize, mi: usize, wi: usize, hi: usize) -> String {
        let m = &self.proj.ytyps[ti].mlos[mi];
        let Some(d) = self.proj.designs.iter().find(|d| d.mlo.eq_ignore_ascii_case(&m.name)) else { return String::new() };
        let Some(w) = d.walls.get(wi) else { return String::new() };
        let Some(h) = w.holes.get(hi) else { return String::new() };
        let (a, b) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
        let len = (b - a).len().max(1e-3);
        let dir = (b - a) * (1.0 / len);
        let n = v3(-dir.y, dir.x, 0.0);
        let (z0, z1) = (w.z0 + h.sill, (w.z0 + h.sill + h.h).min(w.z0 + w.h));
        let at = |s: f32, z: f32| { let q = a + dir * s; v3(q.x, q.y, z) };
        let (s0, s1) = (h.s - h.w * 0.5, h.s + h.w * 0.5);
        let c = [at(s0, z0), at(s0, z1), at(s1, z1), at(s1, z0)];
        let probe = |side: f32| { let q = a + dir * h.s + n * (side * (w.t * 0.5 + 0.3)); v3(q.x, q.y, (z0 + z1) * 0.5) };
        let (ra, rb) = (room_at(d, m, probe(1.0)), room_at(d, m, probe(-1.0)));
        if ra == rb {
            return if ra == 0 { "Ouverture faite, mais aucune pièce de chaque côté : pas de passage.".into() } else { "Ouverture faite (même pièce des deux côtés : pas de passage).".into() };
        }
        let m = &mut self.proj.ytyps[ti].mlos[mi];
        m.portals.push(model::Portal::new(ra.max(rb) as u32, ra.min(rb) as u32, c));
        if ra == 0 || rb == 0 {
            "Ouverture et passage vers l'extérieur faits.".into()
        } else {
            format!("Ouverture et passage entre les pièces {ra} et {rb} faits.")
        }
    }

    /// Pose un miroir sur le mur le plus proche du point `p` (côté du point), centré sur lui : surface et passage
    /// « miroir » de la pièce. Rend un compte rendu.
    pub fn design_add_mirror(&mut self, ti: usize, mi: usize, p: V3, z0: f32) -> String {
        let mname = self.proj.ytyps[ti].mlos[mi].name.clone();
        let Some(d) = self.proj.designs.iter().find(|d| d.mlo.eq_ignore_ascii_case(&mname)).cloned() else { return "Trace d'abord un mur.".into() };
        let mut best: Option<(f32, usize, f32, f32)> = None;
        for (wi, w) in d.walls.iter().enumerate().filter(|(_, w)| (w.z0 - z0).abs() < 0.3) {
            let (a, b) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
            let len = (b - a).len().max(1e-3);
            let dir = (b - a) * (1.0 / len);
            let wn = v3(-dir.y, dir.x, 0.0);
            let q = v3(p.x, p.y, 0.0) - a;
            let s = q.dot(dir).clamp(0.0, len);
            let dist = (a + dir * s - v3(p.x, p.y, 0.0)).len();
            if dist < w.t * 0.5 + 0.6 && best.map(|x| dist < x.0).unwrap_or(true) {
                best = Some((dist, wi, s, if q.dot(wn) >= 0.0 { 1.0 } else { -1.0 }));
            }
        }
        let Some((_, wi, s, side)) = best else { return "Clique tout près d'un mur de l'étage affiché, du côté où le miroir doit être.".into() };
        let w = &d.walls[wi];
        let (a, b) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
        let len = (b - a).len().max(1e-3);
        let dir = (b - a) * (1.0 / len);
        let wn = v3(-dir.y, dir.x, 0.0) * side;
        let (mw, mh, mz) = MIRROR_SIZE;
        if len < mw + 0.1 {
            return "Mur trop court pour un miroir.".into();
        }
        let s = s.clamp(mw * 0.5 + 0.05, len - mw * 0.5 - 0.05);
        let off = wn * (w.t * 0.5 + 0.01);
        let at = |t: f32, z: f32| { let q = a + dir * t + off; [q.x, q.y, z] };
        let (zb, zt) = (w.z0 + mz, (w.z0 + mz + mh).min(w.z0 + w.h - 0.05));
        // dans l'ordre bas-gauche, haut-gauche, haut-droit, bas-droit vu de devant
        let (l, r) = if side > 0.0 { (s + mw * 0.5, s - mw * 0.5) } else { (s - mw * 0.5, s + mw * 0.5) };
        let c = [at(l, zb), at(l, zt), at(r, zt), at(r, zb)];
        let m = &self.proj.ytyps[ti].mlos[mi];
        let ctr = v3((c[0][0] + c[2][0]) * 0.5, (c[0][1] + c[2][1]) * 0.5, (zb + zt) * 0.5);
        let room = room_near(&d, m, ctr + wn * 0.3);
        if room == 0 {
            return "Pas de pièce de ce côté du mur : le miroir doit être dans une pièce.".into();
        }
        self.push_undo();
        self.design_mut(&mname).mirrors.push(DMirror { c, n: [wn.x, wn.y] });
        let mut pt = model::Portal::new(room as u32, room as u32, c.map(|p| v3(p[0], p[1], p[2])));
        pt.flags = PORTAL_MIRROR;
        self.proj.ytyps[ti].mlos[mi].portals.push(pt);
        format!("Miroir de {mw:.1} × {:.1} m posé dans la pièce {room} (surface miroir et passage « miroir » ; à vérifier en jeu).", zt - zb)
    }

    /// Retire le passage d'un miroir.
    pub fn design_remove_mirror_portal(&mut self, ti: usize, mi: usize, mr: &DMirror) {
        let c = v3((mr.c[0][0] + mr.c[2][0]) * 0.5, (mr.c[0][1] + mr.c[2][1]) * 0.5, (mr.c[0][2] + mr.c[2][2]) * 0.5);
        self.proj.ytyps[ti].mlos[mi].portals.retain(|p| {
            let pc = (p.corners[0] + p.corners[1] + p.corners[2] + p.corners[3]) * 0.25;
            !(p.flags & PORTAL_MIRROR != 0 && (pc - c).len() < 0.05)
        });
    }

    /// Retire le passage d'une ouverture (même coins, à 2 cm près).
    pub fn design_remove_hole_portal(&mut self, ti: usize, mi: usize, w: &DWall, h: &DHole) {
        let (a, b) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
        let len = (b - a).len().max(1e-3);
        let dir = (b - a) * (1.0 / len);
        let mid = a + dir * h.s;
        let zc = w.z0 + h.sill + h.h * 0.5;
        let m = &mut self.proj.ytyps[ti].mlos[mi];
        m.portals.retain(|p| {
            let c = (p.corners[0] + p.corners[1] + p.corners[2] + p.corners[3]) * 0.25;
            !(v3(c.x - mid.x, c.y - mid.y, 0.0).len() < 0.05 && (c.z - zc).abs() < 0.3)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deux pièces libres côte à côte, un mur entre elles avec une porte : chaque face va à sa pièce, la porte perce
    /// le mur, la collision ne contient pas les plinthes.
    #[test]
    fn murs_pieces_blocs() {
        let mut m = MloArchetype { name: "dw_test".into(), lod_dist: 100.0, flags: 0, mlo_flags: 0, entities: Vec::new(), rooms: vec![model::Room::new("limbo", V3::ZERO, V3::ZERO)], portals: Vec::new(), entity_sets: Vec::new() };
        for (n, x0, x1) in [("a", 0.0, 4.0), ("b", 4.0, 8.0)] {
            let mut r = model::Room::new(n, v3(x0, 0.0, 0.0), v3(x1, 5.0, 2.75));
            r.free = true;
            m.rooms.push(r);
        }
        let mut d = Design { mlo: "dw_test".into(), ..Default::default() };
        d.rooms.push(DRoom { room: "a".into(), pts: vec![[0.0, 0.0], [4.0, 0.0], [4.0, 5.0], [0.0, 5.0]], z0: 0.0, h: 2.75, ..Default::default() });
        d.rooms.push(DRoom { room: "b".into(), pts: vec![[4.0, 0.0], [8.0, 0.0], [8.0, 5.0], [4.0, 5.0]], z0: 0.0, h: 2.75, drop: 0.3, ..Default::default() });
        d.walls.push(DWall { a: [4.0, 0.0], b: [4.0, 5.0], z0: 0.0, h: 2.75, t: 0.15, base: true, holes: vec![DHole { s: 2.5, w: 0.9, sill: 0.0, h: 2.1, kind: 0 }], ..Default::default() });
        d.blocks.push(DBlock { a: [5.0, 1.0], b: [7.0, 1.6], z0: 0.0, h: 0.95, ..Default::default() });
        let meshes = design_meshes(&d, &m);
        let rooms: Vec<usize> = meshes.iter().map(|x| x.0).collect();
        assert!(rooms.contains(&1) && rooms.contains(&2), "{rooms:?}");
        // la porte : aucune face du mur dans l'ouverture (y entre 2,05 et 2,95, z < 2,1), hors embrasures
        for (_, mm) in &meshes {
            for (_, idx, _) in &mm.parts {
                for t in idx.chunks_exact(3) {
                    let c = (mm.pos[t[0] as usize] + mm.pos[t[1] as usize] + mm.pos[t[2] as usize]) * (1.0 / 3.0);
                    let on_face = (c.x - 4.075).abs() < 0.01 || (c.x - 3.925).abs() < 0.01;
                    assert!(!(on_face && c.y > 2.1 && c.y < 2.9 && c.z < 2.0), "mur dans la porte : {c:?}");
                }
            }
        }
        let mut app_col = 0;
        for (_, mm) in &meshes {
            app_col += mm.parts.iter().filter(|p| p.2 != C_NONE).map(|p| p.1.len()).sum::<usize>();
        }
        assert!(app_col > 0);
    }

    /// Image importée : base64 aller-retour, taille ramenée à une puissance de 2, motif appliqué aux UV.
    #[test]
    fn texture_importee() {
        for d in [vec![], vec![1u8], vec![1, 2], vec![1, 2, 3], (0..=255).collect::<Vec<u8>>()] {
            assert_eq!(b64_decode(&b64_encode(&d)).unwrap(), d);
        }
        let dir = std::env::temp_dir().join("dw_test_tex");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("carreaux bleus.png");
        let img = image::RgbaImage::from_fn(300, 200, |x, y| image::Rgba([(x % 255) as u8, (y % 255) as u8, 200, 255]));
        img.save(&f).unwrap();
        let c = import_png(&f).unwrap();
        assert_eq!(c.name, "carreaux bleus");
        let t = decode_png_tex(&c.png).unwrap();
        assert_eq!((t.w, t.h), (256, 128));
        set_custom_textures(&[CustomTex { size: 1.5, ..c.clone() }]);
        assert_eq!(wall_key(CUSTOM_BASE, None), custom_key("carreaux bleus"));
        set_custom_textures(&[]);
    }

    /// Éclairage cuit : un coin entre deux murs est plus sombre que le milieu d'un mur ; la subdivision garde la
    /// surface.
    #[test]
    fn eclairage_cuit() {
        let mut m = MloArchetype { name: "dw_t".into(), lod_dist: 100.0, flags: 0, mlo_flags: 0, entities: Vec::new(), rooms: vec![model::Room::new("limbo", V3::ZERO, V3::ZERO)], portals: Vec::new(), entity_sets: Vec::new() };
        let mut r = model::Room::new("a", v3(0.0, 0.0, 0.0), v3(6.0, 6.0, 2.8));
        r.free = true;
        m.rooms.push(r);
        let mut d = Design { mlo: "dw_t".into(), baked: true, ..Default::default() };
        d.rooms.push(DRoom { room: "a".into(), pts: vec![[0.0, 0.0], [6.0, 0.0], [6.0, 6.0], [0.0, 6.0]], z0: 0.0, h: 2.8, ..Default::default() });
        for (a, b) in [([0.0, 0.0], [6.0, 0.0]), ([6.0, 0.0], [6.0, 6.0])] {
            d.walls.push(DWall { a, b, z0: 0.0, h: 2.8, t: 0.15, ..Default::default() });
        }
        let up = v3(0.0, 0.0, 1.0);
        let corner = baked_ao(&d, &m, v3(5.85, 0.15, 0.03), up);
        let middle = baked_ao(&d, &m, v3(3.0, 3.0, 0.03), up);
        assert!(corner < middle - 0.2, "coin {corner} / milieu {middle}");
        let wall_mid = baked_ao(&d, &m, v3(3.0, 0.075, 1.4), v3(0.0, 1.0, 0.0));
        assert!(wall_mid > 0.9, "milieu du mur {wall_mid}");
        let meshes = design_meshes(&d, &m);
        let area = |mm: &Mesher| mm.parts.iter().flat_map(|p| p.1.chunks_exact(3)).map(|t| (mm.pos[t[1] as usize] - mm.pos[t[0] as usize]).cross(mm.pos[t[2] as usize] - mm.pos[t[0] as usize]).len() * 0.5).sum::<f32>();
        let (a0, a1) = (area(&meshes[0].1), area(&subdivide(&meshes[0].1, 0.5)));
        assert!((a0 - a1).abs() < a0 * 1e-3, "{a0} {a1}");
    }

    #[test]
    fn polygone() {
        let p = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [2.0, 2.0], [0.0, 4.0]];
        assert!(in_poly(&p, 1.0, 1.0));
        assert!(!in_poly(&p, 2.0, 3.5));
    }
}
