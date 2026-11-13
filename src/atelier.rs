//! Atelier de bâtiment : on trace l'emprise au sol (clics, angles droits aidés), on choisit le nombre d'étages, le
//! toit et la façade ; DragoWalker fabrique le modèle (murs percés de fenêtres avec leurs vitres, embrasures, porte,
//! toit), sa collision, sa version lointaine (murs pleins et toit) et marque les vitres éclairées la nuit. Le
//! bâtiment est un objet normal du projet (déplacer, tourner, dupliquer) ; « Transformer en intérieur » en fait un
//! intérieur (une pièce par étage, fenêtres et porte en portails vers l'extérieur, escalier entre les étages).
//!
//! Toutes les mesures par défaut (épaisseur des murs, fenêtres, acrotère, pente du toit) sont **estimées** d'après
//! l'aspect des immeubles du jeu.

use crate::app::DwApp;
use crate::gta::drawable::TexData;
use crate::math::{v3, Quat, V3};
use crate::model::Entity;
use eframe::egui::{self, Color32, PointerButton, Pos2, RichText};
use serde::{Deserialize, Serialize};

pub const HOUSE_PREFIX: &str = "dw_bat_";
/// Épaisseur des murs, hauteur de l'acrotère d'un toit plat, pente des toits (montée par mètre).
pub const WALL_T: f32 = 0.3;
pub const PARAPET: f32 = 0.6;
pub const ROOF_SLOPE: f32 = 0.55;
/// Porte d'entrée : largeur et hauteur (m).
pub const DOOR_W: f32 = 1.2;
pub const DOOR_H: f32 = 2.3;
/// Distance d'affichage du modèle détaillé, puis de la version lointaine (m).
pub const HOUSE_LOD: f32 = 150.0;
pub const HOUSE_LOD_FAR: f32 = 1500.0;
/// Vitre posée en retrait du nu de la façade (m).
const GLASS_INSET: f32 = 0.12;
/// Sol du rez-de-chaussée au-dessus du pied du bâtiment, épaisseur des planchers (m).
const FLOOR_LIFT: f32 = 0.05;
const SLAB_T: f32 = 0.25;
/// Dictionnaire de textures partagé par tous les bâtiments de l'atelier (déclaré par leurs types d'objets : sans
/// lui, le jeu ne trouve pas les textures et affiche sa texture blanche par défaut).
pub const HOUSE_TXD: &str = "dw_house_txd";

pub const FACADES: [&str; 4] = ["Brique", "Enduit clair", "Béton", "Bardage bois"];
pub const ROOFS: [&str; 3] = ["Plat (toit-terrasse)", "Deux pans", "Quatre pans"];

pub fn k_facade(i: u8) -> u64 {
    0xD7E3_0000_0000_0100 | (i as u64 & 7)
}
pub const K_INNER: u64 = 0xD7E3_0000_0000_0110;
pub const K_ROOF_FLAT: u64 = 0xD7E3_0000_0000_0200;
pub const K_ROOF_TILE: u64 = 0xD7E3_0000_0000_0201;
pub const K_GLASS: u64 = 0xD7E3_0000_0000_0300;
/// Vitres éclairées la nuit (même aspect le jour ; à l'export, matériau qui s'allume la nuit)
#[allow(dead_code)]
pub const K_GLASS_LIT: u64 = 0xD7E3_0000_0000_0301;
pub const K_DOOR: u64 = 0xD7E3_0000_0000_0302;
pub const K_FLOOR: u64 = 0xD7E3_0000_0000_0400;

/// Vraies textures du jeu pour les bâtiments : (nom affiché, dictionnaire partagé du jeu, texture, genre : 0 façade,
/// 1 toit, 2 salissure). Relevées dans x64g.rpf\levels\gta5\generic\gtxd.rpf (sonde DW_TXD_GREP) et vérifiées
/// (DW_TEX_CHECK) : façades et toits en DXT1 opaques, salissures en DXT5 avec transparence (nom = image + masque).
pub const GAME_TEX: [(&str, &str, &str, u8); 23] = [
    ("Brique rouge", "id1struchouses", "im_wall_brick_37", 0),
    ("Brique sombre", "ssblockgroup3", "im_wall_brick_32", 0),
    ("Brique blanche", "id1struc04", "rsn_mh_whitebricktile01", 0),
    ("Enduit beige", "cs4grp06motel", "im_wall_plaster41", 0),
    ("Enduit clair", "id1struc10", "im_wall_plaster34", 0),
    ("Enduit vieilli", "sm1struc27", "im_wall_plaster38", 0),
    ("Crépi", "ch2struc07bsouth", "rsn_ml_wall_render_0001", 0),
    ("Crépi neuf", "silverlakebuild", "hw13_rk_rsn_newrender", 0),
    ("Tuiles", "bhblockgroup9c", "rsn_am_dc_roof_tile_01", 1),
    ("Tuiles claires", "ch1struct06", "rsn_am_dc_roof_tile_01a", 1),
    ("Coulures", "ch1structtemp", "mw_leaking_conct_01mw_leaking_conct_01_a", 2),
    ("Coulures larges", "globalgluech2", "mw_leaking_conct_02mw_leaking_conct_02_a", 2),
    ("Crasse", "po1struc05bridge", "os_mlcr_grime_bridges_0002os_mlcr_grime_bridges_0002_a", 2),
    ("Crasse épaisse", "railbridges", "os_mlcr_grime_bridges_0001os_mlcr_grime_bridges_0001_a", 2),
    ("Rouille", "portblockgroup4", "rsn_et_ruststain002rsn_et_ruststain002_a", 2),
    // sols (genre 3) : dictionnaires d'intérieurs du jeu (x64h.rpf\levels\gta5\interiors) et partagés, vérifiés
    ("Carrelage", "ch2struc05grp", "im_floortiles03", 3),
    ("Carrelage clair", "ss1strucngrp1", "im_floortiles04", 3),
    ("Parquet", "v_37_shell_txd", "tl_v_hairdresser_wood_floor", 3),
    ("Moquette", "v_16_studio_lo", "gz_v_fa_carpet01", 3),
    ("Moquette usée", "v_26_trailerint", "gz_v_tp_carpet03", 3),
    ("Terre cuite", "v_44_txd", "tl_v_michael_floor_terracotta", 3),
    ("Lino", "v_49_motelmptxd", "os_v_fa_lino01", 3),
    ("Béton", "v_13_recycle_txd", "tl_v_recycle_floor_concrete", 3),
];

pub fn game_key(i: usize) -> u64 {
    0xD7E3_0000_0001_0000 | i as u64
}

/// Texture du jeu d'une clé de l'atelier.
pub fn game_tex_of(key: u64) -> Option<(usize, &'static str, &'static str, u8)> {
    let i = key.checked_sub(game_key(0))? as usize;
    GAME_TEX.get(i).filter(|_| key >> 16 == game_key(0) >> 16).map(|g| (i, g.1, g.2, g.3))
}

/// Salissure : partie découpée par la transparence de la texture.
pub fn is_cutout(key: u64) -> bool {
    game_tex_of(key).map(|g| g.3 == 2).unwrap_or(false)
}

static GAME_LOADED: std::sync::RwLock<Option<std::collections::HashMap<u64, TexData>>> = std::sync::RwLock::new(None);

/// Lit les textures du jeu de l'atelier (quand le jeu est chargé). Rend le nombre trouvé.
pub fn load_game_textures(w: &crate::gta::world::GameWorld) -> usize {
    let mut out = std::collections::HashMap::new();
    for (i, (_, txd, tex, _)) in GAME_TEX.iter().enumerate() {
        let file = format!("{txd}.ytd");
        let r = w.vfs.files.iter().filter(|f| f.name.eq_ignore_ascii_case(&file)).last().and_then(|f| w.vfs.read_resource(f, &w.keys));
        if let Some(t) = r.as_ref().and_then(|r| crate::gta::drawable::find_texture(r, 0, crate::gta::crypto::joaat(tex), 512)) {
            out.insert(game_key(i), t);
        }
    }
    let n = out.len();
    if let Ok(mut l) = GAME_LOADED.write() {
        *l = Some(out);
    }
    n
}

fn game_loaded(key: u64) -> Option<TexData> {
    GAME_LOADED.read().ok()?.as_ref()?.get(&key).cloned()
}

/// Collision : 0 murs et toit (béton), 1 vitres.
pub const C_WALL: u8 = 0;
pub const C_GLASS: u8 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct House {
    /// nom du modèle (« dw_bat_N »), porté par les objets du projet
    pub name: String,
    /// emprise au sol, repère du bâtiment, sens inverse des aiguilles d'une montre ; le 1er côté suit l'axe X
    pub foot: Vec<[f32; 2]>,
    pub floors: u8,
    pub floor_h: f32,
    /// 0 plat, 1 deux pans, 2 quatre pans
    pub roof: u8,
    pub facade: u8,
    /// fenêtres : largeur, hauteur, écart entre deux fenêtres (axe à axe)
    pub win_w: f32,
    pub win_h: f32,
    pub win_step: f32,
    /// rez-de-chaussée en vitrines
    pub shop: bool,
    /// porte d'entrée : côté (numéro) et position le long de ce côté (0..1)
    pub door_edge: usize,
    pub door_t: f32,
    /// part des fenêtres éclairées la nuit (0..1)
    pub lit: f32,
    /// intérieur créé depuis ce bâtiment
    #[serde(default)]
    pub interior: Option<String>,
    /// vraie porte du jeu posée dans l'entrée (objet du projet, qui suit le bâtiment)
    #[serde(default)]
    pub door_prop: Option<u64>,
    /// boîte du modèle de cette porte (repère du modèle), pour l'ajuster à l'ouverture
    #[serde(default)]
    pub door_box: Option<([f32; 3], [f32; 3])>,
    /// vraies textures du jeu : façade et toit (0 = texture DragoWalker, sinon 1 + numéro dans GAME_TEX)
    #[serde(default)]
    pub game_facade: u8,
    #[serde(default)]
    pub game_roof: u8,
    /// salissures du jeu (décalques) : quantité (0..1) et genre (numéro dans GAME_TEX)
    #[serde(default)]
    pub grime: f32,
    #[serde(default)]
    pub grime_kind: u8,
    /// modèle du jeu de la 1re porte (vide : porte de maison par défaut) et portes en plus
    #[serde(default)]
    pub door_model: String,
    #[serde(default)]
    pub more_doors: Vec<DoorSpec>,
    /// sol : 0 parquet, 1 carrelage, 2 moquette, 3 béton ciré (textures DragoWalker), 4 et plus : sol du jeu
    /// (4 + numéro dans GAME_FLOORS)
    #[serde(default)]
    pub floor_kind: u8,
    /// cloisons intérieures (plusieurs pièces par étage)
    #[serde(default)]
    pub walls: Vec<Partition>,
    /// modèle des fenêtres (WIN_STYLES)
    #[serde(default)]
    pub win_style: u8,
    /// fenêtres automatiques retirées (côté, étage, milieu le long du côté)
    #[serde(default)]
    pub win_off: Vec<WinRef>,
    /// fenêtres ajoutées à la main
    #[serde(default)]
    pub win_add: Vec<WinSpec>,
}

/// Une fenêtre retirée : côté, étage, milieu le long du côté (m).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct WinRef {
    pub edge: usize,
    pub floor: u8,
    pub s: f32,
}

/// Une fenêtre ajoutée : côté, étage, milieu le long du côté, largeur, hauteur, allège (au-dessus du sol de
/// l'étage), modèle.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct WinSpec {
    pub edge: usize,
    pub floor: u8,
    pub s: f32,
    pub w: f32,
    pub h: f32,
    pub sill: f32,
    pub style: u8,
}

/// Modèles de fenêtre (textures DragoWalker).
pub const WIN_STYLES: [&str; 4] = ["Deux vantaux", "Petits carreaux", "Une seule vitre", "Volets fermés"];
/// Tailles proposées pour une fenêtre ajoutée : (nom, largeur, hauteur, allège) (m) — **estimé**.
pub const WIN_PRESETS: [(&str, f32, f32, f32); 5] = [("Fenêtre", 1.2, 1.5, 0.9), ("Petite", 0.6, 0.6, 1.5), ("Haute", 0.8, 2.2, 0.3), ("Large", 2.4, 1.5, 0.9), ("Baie vitrée", 2.8, 2.4, 0.05)];

/// Texture d'une vitre : modèle, allumée la nuit ou non (le modèle 0 garde K_GLASS / K_GLASS_LIT).
pub fn glass_key(style: u8, lit: bool) -> u64 {
    K_GLASS + (style.min(3) as u64) * 0x10 + if lit && style != 3 { 1 } else { 0 }
}

/// Cloison : étage, début et fin (repère du bâtiment), avec un passage au milieu ou non.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Partition {
    pub floor: u8,
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub door: bool,
}

/// Épaisseur des cloisons, passage (largeur, hauteur) (m) — **estimé**.
pub const PART_T: f32 = 0.12;
pub const PART_DOOR_W: f32 = 0.9;
pub const PART_DOOR_H: f32 = 2.1;
/// Sols proposés : textures DragoWalker.
pub const FLOORS: [&str; 4] = ["Parquet", "Carrelage", "Moquette", "Béton ciré"];
pub const K_FLOOR_TILE: u64 = 0xD7E3_0000_0000_0401;
pub const K_FLOOR_CARPET: u64 = 0xD7E3_0000_0000_0402;
pub const K_FLOOR_CONCRETE: u64 = 0xD7E3_0000_0000_0403;

/// Une porte d'un bâtiment de l'atelier : côté, position le long du côté (0..1), modèle du jeu (vide : par défaut),
/// porte du jeu posée (objet du projet) et boîte de son modèle.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DoorSpec {
    pub edge: usize,
    pub t: f32,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub prop: Option<u64>,
    #[serde(default)]
    pub bbox: Option<([f32; 3], [f32; 3])>,
}

/// Portes du jeu proposées (relevées par la sonde DW_ARCH_GREP : portes posées dehors par le jeu, puis portes
/// d'intérieur v_ilev_*, à garder pour les intérieurs) : (modèle, description).
pub const DOOR_MODELS: [(&str, &str); 14] = [
    ("prop_magenta_door", "porte de maison"),
    ("prop_ron_door_01", "porte de caravane"),
    ("prop_a_trailer_door_01", "porte de mobil-home"),
    ("prop_cs4_11_door", "porte étroite"),
    ("prop_map_door_01", "porte métallique"),
    ("prop_ret_door_04", "porte de magasin"),
    ("prop_shop_front_door_l", "porte vitrée de boutique (gauche)"),
    ("prop_shop_front_door_r", "porte vitrée de boutique (droite)"),
    ("prop_sc1_12_door", "porte de service"),
    ("prop_ss1_05_mp_door", "porte d'immeuble"),
    ("prop_damdoor_01", "porte industrielle"),
    ("v_ilev_ra_door3", "porte d'intérieur"),
    ("v_ilev_ra_door4l", "porte d'intérieur double (gauche)"),
    ("v_ilev_ra_door4r", "porte d'intérieur double (droite)"),
];

/// Une ouverture dans un mur : début et fin le long du côté, bas et haut, genre (0 fenêtre, 1 porte, 2 vitrine).
#[derive(Clone, Copy, Debug)]
pub struct Hole {
    pub s0: f32,
    pub s1: f32,
    pub z0: f32,
    pub z1: f32,
    pub kind: u8,
    /// modèle de fenêtre
    pub style: u8,
}

/// Maillage en cours : positions, coordonnées de texture, parties (texture, indices, collision).
#[derive(Default)]
struct Mesher {
    pos: Vec<V3>,
    uv: Vec<[f32; 2]>,
    parts: Vec<(u64, Vec<u32>, u8)>,
}

impl Mesher {
    fn part(&mut self, key: u64, coll: u8) -> usize {
        if let Some(i) = self.parts.iter().position(|p| p.0 == key && p.2 == coll) {
            return i;
        }
        self.parts.push((key, Vec::new(), coll));
        self.parts.len() - 1
    }

    /// Quadrilatère tourné vers `want` (ordre inversé s'il regarde de l'autre côté), texture en mètres / 3.
    fn quad(&mut self, q: [V3; 4], want: V3, key: u64, coll: u8) {
        // normale des deux triangles (un quadrilatère à deux coins confondus reste un triangle valable)
        let n = (q[1] - q[0]).cross(q[2] - q[0]) + (q[2] - q[0]).cross(q[3] - q[0]);
        if n.len() < 1e-6 {
            return;
        }
        let q = if n.dot(want) < 0.0 { [q[3], q[2], q[1], q[0]] } else { q };
        // texture projetée sur le plan de la face (rangs de brique alignés d'une face à l'autre)
        let nn = n.norm();
        let b = self.pos.len() as u32;
        self.pos.extend_from_slice(&q);
        for p in q {
            let uv = if nn.z.abs() > 0.7 {
                [p.x / 3.0, p.y / 3.0]
            } else {
                let h = v3(-nn.y, nn.x, 0.0).norm();
                [(p.x * h.x + p.y * h.y) / 3.0, -p.z / 3.0]
            };
            self.uv.push(uv);
        }
        let pi = self.part(key, coll);
        self.parts[pi].1.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }

    /// Quadrilatère avec ses coordonnées de texture (décalques : la texture entière sur le quadrilatère).
    fn quad_uv(&mut self, q: [V3; 4], uv: [[f32; 2]; 4], want: V3, key: u64, coll: u8) {
        let n = (q[1] - q[0]).cross(q[2] - q[0]) + (q[2] - q[0]).cross(q[3] - q[0]);
        if n.len() < 1e-6 {
            return;
        }
        let (q, uv) = if n.dot(want) < 0.0 { ([q[3], q[2], q[1], q[0]], [uv[3], uv[2], uv[1], uv[0]]) } else { (q, uv) };
        let b = self.pos.len() as u32;
        self.pos.extend_from_slice(&q);
        self.uv.extend_from_slice(&uv);
        let pi = self.part(key, coll);
        self.parts[pi].1.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }

    /// Triangle tourné vers `want`, texture projetée (plan horizontal ou vertical selon la normale).
    fn tri(&mut self, t: [V3; 3], want: V3, key: u64, coll: u8) {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        if n.len() < 1e-6 {
            return;
        }
        let t = if n.dot(want) < 0.0 { [t[0], t[2], t[1]] } else { t };
        let nn = n.norm();
        let b = self.pos.len() as u32;
        for p in t {
            self.pos.push(p);
            let uv = if nn.z.abs() > 0.7 { [p.x / 3.0, p.y / 3.0] } else if nn.x.abs() > nn.y.abs() { [p.y / 3.0, -p.z / 3.0] } else { [p.x / 3.0, -p.z / 3.0] };
            self.uv.push(uv);
        }
        let pi = self.part(key, coll);
        self.parts[pi].1.extend_from_slice(&[b, b + 1, b + 2]);
    }
}

/// Aire signée (positive : sens inverse des aiguilles d'une montre).
pub fn signed_area(p: &[[f32; 2]]) -> f32 {
    let mut a = 0.0;
    for i in 0..p.len() {
        let (u, v) = (p[i], p[(i + 1) % p.len()]);
        a += u[0] * v[1] - v[0] * u[1];
    }
    a * 0.5
}

/// Triangulation d'un polygone simple (oreilles), sens inverse des aiguilles d'une montre.
pub fn triangulate(p: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let n = p.len();
    // points en double ou alignés avec leurs voisins retirés (une coupe le long du faîtage en laisse)
    let mut idx: Vec<usize> = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b, c) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
        let (ab, bc) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        let (lab, lbc) = ((ab[0] * ab[0] + ab[1] * ab[1]).sqrt(), (bc[0] * bc[0] + bc[1] * bc[1]).sqrt());
        if lab < 1e-4 || lbc < 1e-4 || (ab[0] * bc[1] - ab[1] * bc[0]).abs() < 1e-4 * lab * lbc {
            continue;
        }
        idx.push(i);
    }
    let mut out = Vec::new();
    let cross = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let mut guard = 0;
    while idx.len() > 3 && guard < 10_000 {
        guard += 1;
        let m = idx.len();
        let mut cut = false;
        for k in 0..m {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (p[ia], p[ib], p[ic]);
            if cross(a, b, c) <= 1e-6 {
                continue; // angle rentrant
            }
            let inside = idx.iter().any(|&j| {
                if j == ia || j == ib || j == ic {
                    return false;
                }
                let q = p[j];
                cross(a, b, q) >= 0.0 && cross(b, c, q) >= 0.0 && cross(c, a, q) >= 0.0
            });
            if inside {
                continue;
            }
            out.push([ia, ib, ic]);
            idx.remove(k);
            cut = true;
            break;
        }
        if !cut {
            break;
        }
    }
    // reste (polygone bloqué) : en éventail
    for k in 1..idx.len().saturating_sub(1) {
        out.push([idx[0], idx[k], idx[k + 1]]);
    }
    out
}

/// Partie d'un polygone du côté `keep(p) >= 0` d'une fonction affine (Sutherland-Hodgman sur une droite).
fn clip_half(p: &[[f32; 2]], f: &dyn Fn([f32; 2]) -> f32) -> Vec<[f32; 2]> {
    let mut out = Vec::new();
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        let (fa, fb) = (f(a), f(b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            let t = fa / (fa - fb);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

impl House {
    pub fn new(name: &str, foot: Vec<[f32; 2]>) -> House {
        House { name: name.to_string(), foot, floors: 2, floor_h: 3.2, roof: 0, facade: 1, win_w: 1.2, win_h: 1.5, win_step: 3.0, shop: false, door_edge: 0, door_t: 0.5, lit: 0.6, interior: None, door_prop: None, door_box: None, game_facade: 0, game_roof: 0, grime: 0.0, grime_kind: 10, door_model: String::new(), more_doors: Vec::new(), floor_kind: 0, walls: Vec::new(), win_style: 0, win_off: Vec::new(), win_add: Vec::new() }
    }

    pub fn height(&self) -> f32 {
        self.floors.max(1) as f32 * self.floor_h
    }

    /// Texture de la façade : celle du jeu choisie, sinon celle de DragoWalker.
    pub fn facade_key(&self) -> u64 {
        match self.game_facade {
            0 => k_facade(self.facade),
            g => game_key(g as usize - 1),
        }
    }

    /// Texture du sol des étages.
    pub fn floor_key(&self) -> u64 {
        match self.floor_kind {
            0 => K_FLOOR,
            1 => K_FLOOR_TILE,
            2 => K_FLOOR_CARPET,
            3 => K_FLOOR_CONCRETE,
            g => GAME_TEX.iter().enumerate().filter(|x| x.1 .3 == 3).nth(g as usize - 4).map(|x| game_key(x.0)).unwrap_or(K_FLOOR),
        }
    }

    /// Texture d'un toit en pente.
    pub fn tile_key(&self) -> u64 {
        match self.game_roof {
            0 => K_ROOF_TILE,
            g => game_key(g as usize - 1),
        }
    }

    /// Côtés : (début, fin, direction, normale vers l'extérieur, longueur).
    pub fn edges(&self) -> Vec<(V3, V3, V3, V3, f32)> {
        let n = self.foot.len();
        (0..n)
            .map(|i| {
                let (a, b) = (self.foot[i], self.foot[(i + 1) % n]);
                let (a, b) = (v3(a[0], a[1], 0.0), v3(b[0], b[1], 0.0));
                let d = b - a;
                let l = d.len().max(1e-4);
                let dir = d * (1.0 / l);
                (a, b, dir, v3(dir.y, -dir.x, 0.0), l)
            })
            .collect()
    }

    /// Ouvertures du côté `e` à l'étage `f` (repère du côté : s le long, z hauteur absolue).
    pub fn holes(&self, e: usize, f: u8) -> Vec<Hole> {
        let edges = self.edges();
        let Some(&(_, _, _, _, len)) = edges.get(e) else { return Vec::new() };
        let z0 = f as f32 * self.floor_h;
        let mut out = Vec::new();
        let ne = edges.len();
        let doors: Vec<Hole> = if f == 0 { self.doors().iter().filter(|d| d.edge.min(ne - 1) == e).map(|d| self.door_hole(len, d, z0)).collect() } else { Vec::new() };
        let (ww, wh, sill, kind) = if f == 0 && self.shop {
            ((self.win_step - 0.5).max(0.8), (self.floor_h - 0.9).max(1.2), 0.4, 2u8)
        } else {
            (self.win_w.min(self.win_step - 0.4).max(0.4), self.win_h.min(self.floor_h - 1.2).max(0.4), ((self.floor_h - self.win_h) * 0.55).max(0.6), 0u8)
        };
        let usable = len - 2.0 * WALL_T - 0.6;
        let n = ((usable + self.win_step - ww) / self.win_step).floor().max(0.0) as usize;
        if n > 0 {
            let span = (n - 1) as f32 * self.win_step;
            let start = len * 0.5 - span * 0.5;
            for k in 0..n {
                let c = start + k as f32 * self.win_step;
                let h = Hole { s0: c - ww * 0.5, s1: c + ww * 0.5, z0: z0 + sill, z1: z0 + sill + wh, kind, style: self.win_style };
                // retirée à la main
                if self.win_off.iter().any(|r| r.edge == e && r.floor == f && (r.s - c).abs() < 0.3) {
                    continue;
                }
                // pas de fenêtre sur la porte
                if doors.iter().any(|d| h.s1 > d.s0 - 0.3 && h.s0 < d.s1 + 0.3) {
                    continue;
                }
                out.push(h);
            }
        }
        // fenêtres ajoutées à la main : elles remplacent les fenêtres automatiques qu'elles chevauchent, pas une porte
        for w in self.win_add.iter().filter(|w| w.edge == e && w.floor == f) {
            let ww = w.w.clamp(0.3, (len - 2.0 * WALL_T - 0.2).max(0.3));
            let c = w.s.clamp(WALL_T + ww * 0.5 + 0.05, (len - WALL_T - ww * 0.5 - 0.05).max(WALL_T + ww * 0.5 + 0.05));
            let zb = z0 + w.sill.clamp(0.02, (self.floor_h - 0.5).max(0.02));
            let zt = (zb + w.h).min(z0 + self.floor_h - 0.3);
            let h = Hole { s0: c - ww * 0.5, s1: c + ww * 0.5, z0: zb, z1: zt, kind: 0, style: w.style };
            if zt - zb < 0.2 || doors.iter().any(|d| h.s1 > d.s0 - 0.1 && h.s0 < d.s1 + 0.1) {
                continue;
            }
            out.retain(|o| !(h.s1 > o.s0 - 0.15 && h.s0 < o.s1 + 0.15));
            out.push(h);
        }
        out.extend(doors.iter().copied());
        out.sort_by(|a, b| a.s0.partial_cmp(&b.s0).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    /// Fenêtre éclairée la nuit ? (toujours la même pour un bâtiment donné)
    fn is_lit(&self, e: usize, f: u8, k: usize) -> bool {
        let mut h: u32 = 2166136261;
        for b in self.name.bytes().chain([e as u8, f, k as u8]) {
            h = (h ^ b as u32).wrapping_mul(16777619);
        }
        (h % 1000) as f32 / 1000.0 < self.lit
    }

    /// Hauteur du toit au point (x, y) au-dessus de la corniche, selon le type de toit (0 pour un toit plat).
    fn roof_frame(&self) -> (V3, V3, V3, f32, f32) {
        // repère du toit : axe du plus long côté (u), perpendiculaire (v), centre, demi-longueurs
        let edges = self.edges();
        let u = edges.iter().max_by(|a, b| a.4.partial_cmp(&b.4).unwrap()).map(|e| e.2).unwrap_or(v3(1.0, 0.0, 0.0));
        let v = v3(-u.y, u.x, 0.0);
        let (mut umin, mut umax, mut vmin, mut vmax) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in &self.foot {
            let q = v3(p[0], p[1], 0.0);
            umin = umin.min(q.dot(u));
            umax = umax.max(q.dot(u));
            vmin = vmin.min(q.dot(v));
            vmax = vmax.max(q.dot(v));
        }
        let c = u * ((umin + umax) * 0.5) + v * ((vmin + vmax) * 0.5);
        (u, v, c, (umax - umin) * 0.5, (vmax - vmin) * 0.5)
    }

    /// Le modèle : (positions, coordonnées de texture, parties). `lod` : version lointaine (murs pleins, toit).
    pub fn mesh(&self, lod: bool) -> (Vec<V3>, Vec<[f32; 2]>, Vec<(u64, Vec<u32>, u8)>) {
        let mut m = Mesher::default();
        if self.foot.len() < 3 {
            return (m.pos, m.uv, m.parts);
        }
        let fk = self.facade_key();
        let h = self.height();
        let edges = self.edges();
        let up = v3(0.0, 0.0, 1.0);
        let z = |p: V3, zz: f32| v3(p.x, p.y, zz);
        let top = if self.roof == 0 { h + PARAPET } else { h };
        for (e, &(a, _b, dir, out, len)) in edges.iter().enumerate() {
            let at = |s: f32, off: f32, zz: f32| z(a + dir * s - out * off, zz);
            if lod {
                m.quad([at(0.0, 0.0, 0.0), at(len, 0.0, 0.0), at(len, 0.0, top), at(0.0, 0.0, top)], out, fk, C_WALL);
                continue;
            }
            for f in 0..self.floors.max(1) {
                let (z0, z1) = (f as f32 * self.floor_h, (f + 1) as f32 * self.floor_h);
                let holes = self.holes(e, f);
                let rect = |m: &mut Mesher, s0: f32, s1: f32, za: f32, zb: f32| {
                    if s1 - s0 > 0.01 && zb - za > 0.01 {
                        m.quad([at(s0, 0.0, za), at(s1, 0.0, za), at(s1, 0.0, zb), at(s0, 0.0, zb)], out, fk, C_WALL);
                        m.quad([at(s0, WALL_T, za), at(s1, WALL_T, za), at(s1, WALL_T, zb), at(s0, WALL_T, zb)], out * -1.0, K_INNER, C_WALL);
                    }
                };
                let mut cur = 0.0;
                for (k, ho) in holes.iter().enumerate() {
                    if ho.s0 > cur {
                        rect(&mut m, cur, ho.s0, z0, z1);
                    }
                    rect(&mut m, ho.s0, ho.s1, z0, ho.z0);
                    rect(&mut m, ho.s0, ho.s1, ho.z1, z1);
                    cur = cur.max(ho.s1);
                    // embrasures : côtés, appui, linteau
                    m.quad([at(ho.s0, 0.0, ho.z0), at(ho.s0, WALL_T, ho.z0), at(ho.s0, WALL_T, ho.z1), at(ho.s0, 0.0, ho.z1)], dir, fk, C_WALL);
                    m.quad([at(ho.s1, 0.0, ho.z0), at(ho.s1, WALL_T, ho.z0), at(ho.s1, WALL_T, ho.z1), at(ho.s1, 0.0, ho.z1)], dir * -1.0, fk, C_WALL);
                    m.quad([at(ho.s0, 0.0, ho.z0), at(ho.s1, 0.0, ho.z0), at(ho.s1, WALL_T, ho.z0), at(ho.s0, WALL_T, ho.z0)], up, fk, C_WALL);
                    m.quad([at(ho.s0, 0.0, ho.z1), at(ho.s1, 0.0, ho.z1), at(ho.s1, WALL_T, ho.z1), at(ho.s0, WALL_T, ho.z1)], up * -1.0, fk, C_WALL);
                    // vitre (ou porte), en retrait
                    let q = [at(ho.s0, GLASS_INSET, ho.z0), at(ho.s1, GLASS_INSET, ho.z0), at(ho.s1, GLASS_INSET, ho.z1), at(ho.s0, GLASS_INSET, ho.z1)];
                    match ho.kind {
                        1 => {
                            // porte d'entrée : un panneau tant qu'il n'y a pas d'intérieur (sinon une vraie porte du jeu)
                            if self.interior.is_none() && !self.door_has_prop(e, ho) {
                                m.quad(q, out, K_DOOR, C_WALL);
                            }
                        }
                        _ => {
                            // la texture entière sur chaque vitre (le cadre et les montants tombent au bon endroit)
                            let key = glass_key(ho.style, self.is_lit(e, f, k));
                            let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
                            m.quad_uv(q, uv, out, key, C_GLASS);
                            // la même vitre vue de dedans (le jeu ne dessine une face que d'un côté)
                            let qi = [at(ho.s0, GLASS_INSET + 0.005, ho.z0), at(ho.s1, GLASS_INSET + 0.005, ho.z0), at(ho.s1, GLASS_INSET + 0.005, ho.z1), at(ho.s0, GLASS_INSET + 0.005, ho.z1)];
                            m.quad_uv(qi, uv, out * -1.0, glass_key(ho.style, false), C_GLASS);
                        }
                    }
                }
                rect(&mut m, cur, len, z0, z1);
            }
            // acrotère d'un toit plat : face extérieure, face intérieure, dessus
            if self.roof == 0 {
                m.quad([at(0.0, 0.0, h), at(len, 0.0, h), at(len, 0.0, h + PARAPET), at(0.0, 0.0, h + PARAPET)], out, fk, C_WALL);
                m.quad([at(0.0, WALL_T, h), at(len, WALL_T, h), at(len, WALL_T, h + PARAPET), at(0.0, WALL_T, h + PARAPET)], out * -1.0, fk, C_WALL);
                m.quad([at(0.0, 0.0, h + PARAPET), at(len, 0.0, h + PARAPET), at(len, WALL_T, h + PARAPET), at(0.0, WALL_T, h + PARAPET)], up, fk, C_WALL);
            }
        }
        // salissures du jeu : coulures sous les fenêtres et le long du bas des murs, collées 1,5 cm devant la façade,
        // toujours les mêmes pour un bâtiment (quantité réglable)
        if !lod && self.grime > 0.0 {
            let gk = game_key((self.grime_kind as usize).min(GAME_TEX.len() - 1));
            let gk = if is_cutout(gk) { gk } else { game_key(10) };
            let mut seed: u32 = self.name.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619)) | 1;
            let mut rnd = move || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed % 10_000) as f32 / 10_000.0
            };
            for (e, &(a, _, dir, out, len)) in edges.iter().enumerate() {
                let at = |s: f32, zz: f32| z(a + dir * s + out * 0.015, zz);
                for f in 0..self.floors.max(1) {
                    for ho in self.holes(e, f) {
                        if ho.kind != 0 || rnd() > self.grime {
                            continue;
                        }
                        let (s0, s1) = (ho.s0 - 0.1, ho.s1 + 0.1);
                        let (zb, zt) = ((ho.z0 - 1.0 - rnd() * 0.8).max(f as f32 * self.floor_h + 0.05), ho.z0 - 0.01);
                        m.quad_uv([at(s0, zb), at(s1, zb), at(s1, zt), at(s0, zt)], [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], out, gk, 255);
                    }
                }
                // bas des murs : bandes de 2 à 4 m
                let mut s = 0.3;
                while s < len - 0.5 {
                    let w = 2.0 + rnd() * 2.0;
                    if rnd() < self.grime {
                        let s1 = (s + w).min(len - 0.1);
                        let zt = 0.6 + rnd() * 0.9;
                        m.quad_uv([at(s, 0.0), at(s1, 0.0), at(s1, zt), at(s, zt)], [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], out, gk, 255);
                    }
                    s += w + 0.5 + rnd() * 2.0;
                }
            }
        }
        // planchers : sol du rez-de-chaussée sur toute l'emprise ; à chaque étage, plancher (dessus et plafond de l'étage
        // du dessous, 25 cm d'épaisseur) percé d'une trémie, et l'escalier qui y monte (le même que celui de
        // « Transformer en intérieur ») : la maison se visite même sans intérieur
        if !lod {
            let z_at = |p: V3, zz: f32| v3(p.x, p.y, zz);
            for t in triangulate(&self.foot) {
                let p = |k: usize| v3(self.foot[t[k]][0], self.foot[t[k]][1], FLOOR_LIFT);
                m.tri([p(0), p(1), p(2)], up, self.floor_key(), C_WALL);
            }
            let stair = self.stair();
            let tris = triangulate(&self.foot);
            for f in 1..self.floors.max(1) {
                let z = f as f32 * self.floor_h;
                // plancher : l'emprise entière, moins la trémie (morceaux convexes de chaque triangle)
                for t in &tris {
                    let tri: Vec<[f32; 2]> = t.iter().map(|&k| self.foot[k]).collect();
                    let pieces: Vec<Vec<[f32; 2]>> = match stair {
                        None => vec![tri.clone()],
                        Some(st) => {
                            let u = move |p: [f32; 2]| (v3(p[0], p[1], 0.0) - st.a).dot(st.dir);
                            let w = move |p: [f32; 2]| (v3(p[0], p[1], 0.0) - st.a).dot(st.inw);
                            let (h0, h1, w0, w1) = (st.hole.0, st.hole.1, st.w.0, st.w.1);
                            let mid = clip_half(&clip_half(&tri, &|p| u(p) - h0), &|p| h1 - u(p));
                            vec![clip_half(&tri, &|p| h0 - u(p)), clip_half(&tri, &|p| u(p) - h1), clip_half(&mid, &|p| w0 - w(p)), clip_half(&mid, &|p| w(p) - w1)]
                        }
                    };
                    for pc in pieces.into_iter().filter(|p| p.len() >= 3) {
                        for k in 1..pc.len() - 1 {
                            let (p0, p1, p2) = (pc[0], pc[k], pc[k + 1]);
                            m.tri([v3(p0[0], p0[1], z), v3(p1[0], p1[1], z), v3(p2[0], p2[1], z)], up, self.floor_key(), C_WALL);
                            m.tri([v3(p0[0], p0[1], z - SLAB_T), v3(p1[0], p1[1], z - SLAB_T), v3(p2[0], p2[1], z - SLAB_T)], up * -1.0, K_INNER, C_WALL);
                        }
                    }
                }
                let Some(st) = stair else { continue };
                // bords de la trémie (tournés vers son milieu)
                let c = st.hole_corners(0.0);
                let centre = (c[0] + c[2]) * 0.5;
                for k in 0..4 {
                    let (p, q) = (c[k], c[(k + 1) % 4]);
                    m.quad([z_at(p, z - SLAB_T), z_at(q, z - SLAB_T), z_at(q, z), z_at(p, z)], centre - (p + q) * 0.5, K_INNER, C_WALL);
                }
                // marches, du sol de l'étage du dessous au plancher, le long du mur
                let z0 = (f - 1) as f32 * self.floor_h + if f == 1 { FLOOR_LIFT } else { 0.0 };
                let rise = (z - z0) / st.n as f32;
                let (w0, w1) = st.w;
                for i in 0..st.n {
                    let (s0, s1) = (st.start + i as f32 * st.run, st.start + (i + 1) as f32 * st.run);
                    let (zl, zh) = (z0 + i as f32 * rise, z0 + (i + 1) as f32 * rise);
                    m.quad([st.at(s0, w0, zl), st.at(s0, w1, zl), st.at(s0, w1, zh), st.at(s0, w0, zh)], st.dir * -1.0, self.floor_key(), C_WALL);
                    m.quad([st.at(s0, w0, zh), st.at(s1, w0, zh), st.at(s1, w1, zh), st.at(s0, w1, zh)], up, self.floor_key(), C_WALL);
                    m.quad([st.at(s0, w1, z0), st.at(s1, w1, z0), st.at(s1, w1, zh), st.at(s0, w1, zh)], st.inw, K_INNER, C_WALL);
                }
            }
        }
        // cloisons : du sol au plafond de leur étage, avec un passage au milieu si demandé
        if !lod {
            for w in &self.walls {
                let (a, b) = (v3(w.a[0], w.a[1], 0.0), v3(w.b[0], w.b[1], 0.0));
                let len = (b - a).len();
                if len < 0.3 || w.floor >= self.floors.max(1) {
                    continue;
                }
                let dir = (b - a) * (1.0 / len);
                let nrm = v3(-dir.y, dir.x, 0.0);
                let z0 = w.floor as f32 * self.floor_h + if w.floor == 0 { FLOOR_LIFT } else { 0.0 };
                let z1 = if w.floor + 1 >= self.floors.max(1) { h - 0.02 } else { (w.floor + 1) as f32 * self.floor_h - SLAB_T };
                let pt = |s: f32, o: f32, zz: f32| { let q = a + dir * s + nrm * o; v3(q.x, q.y, zz) };
                let ht = PART_T * 0.5;
                let slab = |m: &mut Mesher, s0: f32, s1: f32, za: f32, zb: f32| {
                    if s1 - s0 < 0.01 || zb - za < 0.01 {
                        return;
                    }
                    m.quad([pt(s0, ht, za), pt(s1, ht, za), pt(s1, ht, zb), pt(s0, ht, zb)], nrm, K_INNER, C_WALL);
                    m.quad([pt(s0, -ht, za), pt(s1, -ht, za), pt(s1, -ht, zb), pt(s0, -ht, zb)], nrm * -1.0, K_INNER, C_WALL);
                    m.quad([pt(s0, -ht, zb), pt(s1, -ht, zb), pt(s1, ht, zb), pt(s0, ht, zb)], up, K_INNER, C_WALL);
                    for (s, d) in [(s0, dir * -1.0), (s1, dir)] {
                        m.quad([pt(s, -ht, za), pt(s, ht, za), pt(s, ht, zb), pt(s, -ht, zb)], d, K_INNER, C_WALL);
                    }
                };
                if w.door && len > PART_DOOR_W + 0.4 {
                    let (d0, d1) = (len * 0.5 - PART_DOOR_W * 0.5, len * 0.5 + PART_DOOR_W * 0.5);
                    slab(&mut m, 0.0, d0, z0, z1);
                    slab(&mut m, d1, len, z0, z1);
                    slab(&mut m, d0, d1, z0 + PART_DOOR_H, z1);
                } else {
                    slab(&mut m, 0.0, len, z0, z1);
                }
            }
        }
        // plafond du dernier étage, sous le toit (vu de dedans)
        if !lod {
            for t in triangulate(&self.foot) {
                let p = |k: usize| v3(self.foot[t[k]][0], self.foot[t[k]][1], h - 0.02);
                m.tri([p(0), p(1), p(2)], up * -1.0, K_INNER, C_WALL);
            }
        }
        // toit
        let p3 = |p: [f32; 2], zz: f32| v3(p[0], p[1], zz);
        match self.roof {
            0 => {
                for t in triangulate(&self.foot) {
                    m.tri([p3(self.foot[t[0]], h + 0.05), p3(self.foot[t[1]], h + 0.05), p3(self.foot[t[2]], h + 0.05)], up, K_ROOF_FLAT, C_WALL);
                }
            }
            1 => {
                // deux pans : faîtage le long du plus long côté, au milieu
                let (_u, v, c, _hu, hv) = self.roof_frame();
                let vc = c.dot(v);
                let rz = |p: [f32; 2]| h + ROOF_SLOPE * (hv - (v3(p[0], p[1], 0.0).dot(v) - vc).abs());
                for side in [1.0f32, -1.0] {
                    let half = clip_half(&self.foot, &|p| (v3(p[0], p[1], 0.0).dot(v) - vc) * side);
                    if half.len() < 3 {
                        continue;
                    }
                    for t in triangulate(&half) {
                        m.tri([p3(half[t[0]], rz(half[t[0]])), p3(half[t[1]], rz(half[t[1]])), p3(half[t[2]], rz(half[t[2]]))], up, self.tile_key(), C_WALL);
                    }
                }
                // pignons : murs entre la corniche et le toit
                for &(a, b, dir, out, len) in &edges {
                    let (pa, pb) = ([a.x, a.y], [b.x, b.y]);
                    let mut pts = vec![(0.0f32, rz(pa))];
                    let (fa, fb) = (a.dot(v) - vc, b.dot(v) - vc);
                    if fa * fb < 0.0 {
                        let s = len * fa / (fa - fb);
                        let x = a + dir * s;
                        pts.push((s, rz([x.x, x.y])));
                    }
                    pts.push((len, rz(pb)));
                    for w in pts.windows(2) {
                        let (s0, z0) = w[0];
                        let (s1, z1) = w[1];
                        if z0 - h < 0.01 && z1 - h < 0.01 {
                            continue;
                        }
                        let q = [z(a + dir * s0, h), z(a + dir * s1, h), z(a + dir * s1, z1), z(a + dir * s0, z0)];
                        m.quad(q, out, fk, C_WALL);
                    }
                }
            }
            _ => {
                // quatre pans : rectangle → croupes et faîtage ; autre forme → en pointe
                let (u, v, c, hu, hv) = self.roof_frame();
                let rect_like = self.foot.len() == 4 && (signed_area(&self.foot) - 4.0 * hu * hv).abs() < 0.05 * 4.0 * hu * hv;
                if rect_like {
                    let (lu, lv) = if hu >= hv { (u, v) } else { (v, u) };
                    let (a_long, a_short) = (hu.max(hv), hu.min(hv));
                    let rh = h + ROOF_SLOPE * a_short;
                    let r1 = c + lu * -(a_long - a_short) + v3(0.0, 0.0, rh);
                    let r2 = c + lu * (a_long - a_short) + v3(0.0, 0.0, rh);
                    let corner = |su: f32, sv: f32| c + lu * (a_long * su) + lv * (a_short * sv) + v3(0.0, 0.0, h);
                    let (c00, c10, c11, c01) = (corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0));
                    m.quad([c00, c10, r2, r1], up, self.tile_key(), C_WALL);
                    m.quad([c11, c01, r1, r2], up, self.tile_key(), C_WALL);
                    m.tri([c10, c11, r2], up, self.tile_key(), C_WALL);
                    m.tri([c01, c00, r1], up, self.tile_key(), C_WALL);
                } else {
                    let n = self.foot.len() as f32;
                    let g = v3(self.foot.iter().map(|p| p[0]).sum::<f32>() / n, self.foot.iter().map(|p| p[1]).sum::<f32>() / n, h + ROOF_SLOPE * hu.min(hv));
                    for &(a, b, _, _, _) in &edges {
                        m.tri([z(a, h), z(b, h), g], up, self.tile_key(), C_WALL);
                    }
                }
            }
        }
        (m.pos, m.uv, m.parts)
    }

    /// Boîte du modèle (repère du bâtiment).
    pub fn bbox(&self) -> (V3, V3) {
        let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, 0.0), v3(f32::MIN, f32::MIN, 0.0));
        for p in &self.foot {
            mn = mn.min(v3(p[0], p[1], 0.0));
            mx = mx.max(v3(p[0], p[1], 0.0));
        }
        let (_, _, _, hu, hv) = self.roof_frame();
        mx.z = self.height() + if self.roof == 0 { PARAPET } else { ROOF_SLOPE * hu.min(hv) + 0.1 };
        (mn, mx)
    }

    /// Escalier qui monte de l'étage `f` à l'étage du dessus, le long du mur du bas du plan (y le plus petit) :
    /// trémie (x0, x1, y0, y1), début des marches, nombre de marches, profondeur d'une marche.
    pub fn stair(&self) -> Option<Stair> {
        let n = (self.floor_h / 0.18).ceil().max(2.0) as usize;
        let edges = self.edges();
        let ne = edges.len();
        let doors = self.doors();
        // un point (repère du bâtiment) est-il dans l'emprise, à au moins l'épaisseur des murs de chaque mur ?
        let inside = |p: V3| -> bool {
            let mut c = false;
            for i in 0..ne {
                let (a, b) = (self.foot[i], self.foot[(i + 1) % ne]);
                if (a[1] > p.y) != (b[1] > p.y) && p.x < (b[0] - a[0]) * (p.y - a[1]) / (b[1] - a[1]) + a[0] {
                    c = !c;
                }
            }
            c && edges.iter().all(|&(a, _, dir, _, len)| {
                let s = (p - a).dot(dir).clamp(0.0, len);
                let q = a + dir * s;
                v3(p.x - q.x, p.y - q.y, 0.0).len() >= WALL_T - 0.01
            })
        };
        // murs essayés : droits (le long d'un axe) d'abord, puis les plus longs
        let mut order: Vec<usize> = (0..ne).collect();
        let axis = |k: usize| edges[k].2.x.abs().max(edges[k].2.y.abs()) > 0.999;
        order.sort_by(|&i, &j| axis(j).cmp(&axis(i)).then(edges[j].4.partial_cmp(&edges[i].4).unwrap_or(std::cmp::Ordering::Equal)));
        for run in [0.28f32, 0.24, 0.2] {
            let full = n as f32 * run;
            for &e in &order {
                let (a, _, dir, out, len) = edges[e];
                let inw = out * -1.0;
                let (s0, w0) = (WALL_T + 0.3, WALL_T + 0.02);
                let (s1, w1) = (s0 + full, w0 + STAIR_W);
                if s1 > len - WALL_T - 0.3 {
                    continue;
                }
                // pas devant une porte de ce mur
                let blocked = doors.iter().any(|d| {
                    let h = self.door_hole(len, d, 0.0);
                    d.edge.min(ne - 1) == e && h.s1 > s0 - 0.3 && h.s0 < s1 + 0.3
                });
                if blocked {
                    continue;
                }
                let p = |s: f32, w: f32| a + dir * s + inw * w;
                let pts = [p(s0, w0), p(s1, w0), p(s1, w1), p(s0, w1), p((s0 + s1) * 0.5, w1), p((s0 + s1) * 0.5, (w0 + w1) * 0.5)];
                if pts.iter().all(|q| inside(*q)) {
                    return Some(Stair { a, dir, inw, hole: (s1 - (full * STAIR_HOLE_PART).min(STAIR_HOLE_MAX), s1), w: (w0, w1), start: s0, n, run });
                }
            }
        }
        None
    }

    /// Porte d'entrée (repère du bâtiment) : coin de départ, direction le long du mur, normale vers l'extérieur,
    /// largeur.
    pub fn door_frame(&self, i: usize) -> Option<(V3, V3, V3, f32, f32)> {
        let d = self.doors().get(i)?.clone();
        let e = d.edge.min(self.foot.len().saturating_sub(1));
        let (a, _, dir, out, len) = *self.edges().get(e)?;
        let h = self.door_hole(len, &d, 0.0);
        Some((a + dir * h.s0, dir, out, h.s1 - h.s0, h.z1 - FLOOR_LIFT))
    }

    /// Toutes les portes : la 1re (champs door_*), puis les autres.
    pub fn doors(&self) -> Vec<DoorSpec> {
        let mut v = vec![DoorSpec { edge: self.door_edge, t: self.door_t, model: self.door_model.clone(), prop: self.door_prop, bbox: self.door_box }];
        v.extend(self.more_doors.iter().cloned());
        v
    }

    /// Remplace la porte `i`.
    pub fn put_door(&mut self, i: usize, d: DoorSpec) {
        if i == 0 {
            self.door_edge = d.edge;
            self.door_t = d.t;
            self.door_model = d.model;
            self.door_prop = d.prop;
            self.door_box = d.bbox;
        } else if let Some(x) = self.more_doors.get_mut(i - 1) {
            *x = d;
        }
    }

    /// Ouverture d'une porte sur un côté de longueur `len`, centrée à `t` (gardée à distance des coins).
    /// L'ouverture prend la taille du modèle de porte choisi : une porte du jeu est un objet mobile, que le jeu ne
    /// montre pas si on l'étire (elle n'apparaissait pas en jeu, retour de la personne, 13 nov. 2026).
    fn door_hole(&self, len: f32, d: &DoorSpec, z0: f32) -> Hole {
        let (w, h) = d.bbox.map(|(a, b)| (b[0] - a[0], b[2] - a[2])).unwrap_or((DOOR_W, DOOR_H));
        let (w, h) = (w.clamp(0.5, (len - 2.0 * WALL_T - 0.1).max(0.5)), h.min(self.floor_h - 0.3));
        let c = (d.t.clamp(0.0, 1.0) * len).clamp(WALL_T + w * 0.5, (len - WALL_T - w * 0.5).max(WALL_T + w * 0.5));
        Hole { s0: c - w * 0.5, s1: c + w * 0.5, z0: z0 + 0.02, z1: z0 + FLOOR_LIFT + h, kind: 1, style: 0 }
    }

    /// Une vraie porte du jeu est-elle posée dans cette ouverture ?
    fn door_has_prop(&self, e: usize, ho: &Hole) -> bool {
        let ne = self.foot.len();
        let Some(&(_, _, _, _, len)) = self.edges().get(e) else { return false };
        self.doors().iter().any(|d| d.edge.min(ne - 1) == e && d.prop.is_some() && (self.door_hole(len, d, 0.0).s0 - ho.s0).abs() < 0.01)
    }

    /// Porte du jeu ajustée à l'entrée (repère du bâtiment) : position de l'origine du modèle, angle autour de Z,
    /// échelles (largeur, hauteur). Le modèle va le long du mur selon son X et est épais selon son Y : son côté X le
    /// plus petit est posé sur le bord de l'ouverture, son bas sur le sol, son épaisseur centrée dans le mur, et il
    /// est étiré pour remplir l'ouverture.
    pub fn door_fit(&self, i: usize) -> Option<(V3, f32, f32, f32)> {
        let (c0, dir, out, w, h) = self.door_frame(i)?;
        let (mn, mx) = self.doors().get(i)?.bbox.map(|(a, b)| (v3(a[0], a[1], a[2]), v3(b[0], b[1], b[2]))).unwrap_or((v3(0.0, -0.05, 0.0), v3(1.0, 0.05, 2.2)));
        // jamais étirée (voir door_hole) : l'ouverture a déjà la taille du modèle
        let _ = (w, h);
        let (sxy, sz) = (1.0f32, 1.0f32);
        // repère du modèle : X le long du mur (dir), Y vers l'intérieur (-out)
        let inward = out * -1.0;
        let mid_y = (mn.y + mx.y) * 0.5 * sxy;
        let p = c0 - dir * (mn.x * sxy) + inward * (WALL_T * 0.5 - mid_y) + v3(0.0, 0.0, FLOOR_LIFT - mn.z * sz);
        Some((p, dir.y.atan2(dir.x), sxy, sz))
    }

    /// Pièce de l'étage `f` (boîte intérieure du plan, repère du bâtiment).
    pub fn floor_room(&self, f: u8) -> (V3, V3) {
        let (mn, mx) = self.bbox();
        let z0 = f as f32 * self.floor_h;
        (v3(mn.x + WALL_T, mn.y + WALL_T, z0 + 0.02), v3(mx.x - WALL_T, mx.y - WALL_T, z0 + self.floor_h - 0.25))
    }
}

// ------------------------------------------------------------------------------------------------- textures

fn hash(x: u32, y: u32, s: u32) -> f32 {
    let mut h = x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ s.wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) & 0xFFFF) as f32 / 65535.0
}

fn tex(n: u32, f: impl Fn(u32, u32) -> [f32; 3]) -> TexData {
    let mut d = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let c = f(x, y);
            d.extend_from_slice(&[c[0].clamp(0.0, 255.0) as u8, c[1].clamp(0.0, 255.0) as u8, c[2].clamp(0.0, 255.0) as u8, 255]);
        }
    }
    crate::roads::with_mips(n, n, d)
}

/// Textures de l'atelier (3 m de côté chacune), gardées en mémoire.
pub fn house_textures() -> Vec<(u64, TexData)> {
    static CACHE: std::sync::OnceLock<Vec<(u64, TexData)>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let n = 256u32;
            let mut v = Vec::new();
            // brique : rangs de 7,5 cm décalés, joints clairs
            v.push((k_facade(0), tex(n, |x, y| {
                let row = y / 7;
                let off = if row % 2 == 0 { 0 } else { 8 };
                let joint = y % 7 == 0 || (x + off) % 16 == 0;
                let g = hash(x / 16 + row * 7, row, 3) * 0.25 + hash(x, y, 4) * 0.1;
                if joint { [176.0, 168.0, 156.0] } else { [150.0 * (0.85 + g), 74.0 * (0.85 + g), 56.0 * (0.85 + g)] }
            })));
            v.push((k_facade(1), tex(n, |x, y| {
                let g = 0.92 + hash(x, y, 5) * 0.08 + hash(x / 8, y / 8, 6) * 0.05;
                [222.0 * g, 212.0 * g, 192.0 * g]
            })));
            v.push((k_facade(2), tex(n, |x, y| {
                let joint = x % 128 == 0 || y % 64 == 0;
                let g = 0.88 + hash(x, y, 7) * 0.12;
                if joint { [120.0, 120.0, 118.0] } else { [168.0 * g, 168.0 * g, 164.0 * g] }
            })));
            v.push((k_facade(3), tex(n, |x, y| {
                let groove = y % 16 == 0;
                let g = 0.85 + hash(x / 32, y / 16, 8) * 0.2 + hash(x, y, 9) * 0.05;
                if groove { [70.0, 52.0, 38.0] } else { [132.0 * g, 98.0 * g, 66.0 * g] }
            })));
            v.push((K_INNER, tex(n, |x, y| {
                let g = 0.95 + hash(x, y, 10) * 0.05;
                [232.0 * g, 228.0 * g, 218.0 * g]
            })));
            v.push((K_ROOF_FLAT, tex(n, |x, y| {
                let g = 0.75 + hash(x, y, 11) * 0.35;
                [110.0 * g, 108.0 * g, 104.0 * g]
            })));
            v.push((K_ROOF_TILE, tex(n, |x, y| {
                let row = y / 16;
                let off = if row % 2 == 0 { 0 } else { 12 };
                let edge = y % 16 > 13 || (x + off) % 24 == 0;
                let g = 0.85 + hash((x + off) / 24, row, 12) * 0.25;
                if edge { [70.0, 34.0, 28.0] } else { [156.0 * g, 70.0 * g, 52.0 * g] }
            })));
            // vitre : reflet bleu-gris, cadre clair sur le bord
            let glass = |style: u8, warm: bool| {
                tex(64, move |x, y| {
                    if style == 3 {
                        // volets fermés : lames de bois horizontales
                        let slat = y % 8 < 2;
                        let g = 0.85 + hash(x / 16, y / 8, 22) * 0.15;
                        return if x < 2 || x > 61 || (31..33).contains(&x) { [70.0, 52.0, 36.0] } else if slat { [86.0 * g, 64.0 * g, 44.0 * g] } else { [128.0 * g, 96.0 * g, 64.0 * g] };
                    }
                    let frame = x < 3 || y < 3 || x > 60 || y > 60 || match style {
                        0 => (31..33).contains(&x),
                        1 => [20, 21, 42, 43].contains(&x) || [20, 21, 42, 43].contains(&y),
                        _ => false,
                    };
                    if frame {
                        [214.0, 214.0, 208.0]
                    } else if warm {
                        let g = 0.9 + (y as f32 / 64.0) * 0.1;
                        [150.0 * g, 132.0 * g, 98.0 * g]
                    } else {
                        let g = 0.8 + (x + y) as f32 / 128.0 * 0.3;
                        [70.0 * g, 86.0 * g, 100.0 * g]
                    }
                })
            };
            for st in 0..4u8 {
                v.push((glass_key(st, false), glass(st, false)));
                if st != 3 {
                    v.push((glass_key(st, true), glass(st, true)));
                }
            }
            v.push((K_FLOOR, tex(n, |x, y| {
                // parquet : lames de 16 cm
                let plank = y / 14;
                let g = 0.82 + hash(x / 40 + plank * 3, plank, 14) * 0.18 + hash(x, y, 15) * 0.05;
                if y % 14 == 0 { [92.0, 62.0, 40.0] } else { [176.0 * g, 128.0 * g, 84.0 * g] }
            })));
            v.push((K_FLOOR_TILE, tex(n, |x, y| {
                // carreaux de 30 cm, joints gris
                let joint = x % 26 < 2 || y % 26 < 2;
                let g = 0.93 + hash(x / 26, y / 26, 16) * 0.07 + hash(x, y, 17) * 0.03;
                if joint { [150.0, 150.0, 146.0] } else { [222.0 * g, 220.0 * g, 212.0 * g] }
            })));
            v.push((K_FLOOR_CARPET, tex(n, |x, y| {
                let g = 0.88 + hash(x, y, 18) * 0.16 + hash(x / 3, y / 3, 19) * 0.06;
                [118.0 * g, 104.0 * g, 92.0 * g]
            })));
            v.push((K_FLOOR_CONCRETE, tex(n, |x, y| {
                let g = 0.9 + hash(x / 12, y / 12, 20) * 0.08 + hash(x, y, 21) * 0.04;
                [150.0 * g, 150.0 * g, 148.0 * g]
            })));
            v.push((K_DOOR, tex(64, |x, y| {
                let panel = (x > 8 && x < 56) && ((y > 6 && y < 28) || (y > 34 && y < 58));
                let g = 0.9 + hash(x, y, 13) * 0.1;
                if panel { [96.0 * g, 62.0 * g, 40.0 * g] } else { [120.0 * g, 80.0 * g, 52.0 * g] }
            })));
            v
        })
        .clone()
}

/// Morceaux en plus d'un bâtiment à l'export : un par dictionnaire du jeu dont il utilise des textures
/// (nom du modèle, dictionnaire).
pub fn house_pieces(h: &House) -> Vec<(String, &'static str)> {
    let (_, _, parts) = h.mesh(false);
    let mut txds: Vec<&'static str> = Vec::new();
    for (k, idx, _) in &parts {
        if let Some((_, txd, _, _)) = game_tex_of(*k) {
            if !idx.is_empty() && !txds.contains(&txd) {
                txds.push(txd);
            }
        }
    }
    txds.into_iter().enumerate().map(|(i, t)| (format!("{}_g{i}", h.name), t)).collect()
}

/// Modèle prêt pour la carte graphique ou l'export.
pub fn house_rmodel(h: &House, lod: bool) -> (crate::gta::drawable::RModel, Vec<u64>, Vec<(u64, TexData)>) {
    let (pos, uv, parts) = h.mesh(lod);
    let mut texs = house_textures();
    // textures du jeu utilisées : lues dans le jeu ; sans le jeu, celle de DragoWalker la plus proche (façade, tuiles),
    // une texture vide pour une salissure
    for (k, _, _) in &parts {
        if let Some((_, _, _, kind)) = game_tex_of(*k) {
            if texs.iter().any(|t| t.0 == *k) {
                continue;
            }
            let t = game_loaded(*k).unwrap_or_else(|| match kind {
                0 => house_textures().into_iter().find(|t| t.0 == k_facade(h.facade)).map(|t| t.1).unwrap(),
                1 => house_textures().into_iter().find(|t| t.0 == K_ROOF_TILE).map(|t| t.1).unwrap(),
                3 => house_textures().into_iter().find(|t| t.0 == K_FLOOR).map(|t| t.1).unwrap(),
                _ => crate::roads::with_mips(4, 4, vec![0u8; 64]),
            });
            texs.push((*k, t));
        }
    }
    let (mut verts, mut idx, mut geoms, mut keys) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    crate::terrain::append_mesh(&mut verts, &mut idx, &mut geoms, &mut keys, &texs, &pos, &uv, &parts);
    (crate::gta::drawable::RModel { verts, idx, geoms, embedded_dict: None }, keys, texs)
}

// ------------------------------------------------------------------------------------------------- outil

/// État de l'atelier (fenêtre, tracé en cours).
#[derive(Default)]
pub struct AtelierState {
    pub open: bool,
    pub tracing: bool,
    pub pts: Vec<V3>,
    /// angles droits aidés pendant le tracé
    pub square: bool,
    /// signature des bâtiments envoyés à la vue
    pub sig: u64,
    /// rectangle rapide : largeur et profondeur
    pub quick: (f32, f32),
    /// porte à placer d'un clic sur un mur : (bâtiment, porte ; None = nouvelle porte)
    pub door_pick: Option<(usize, Option<usize>)>,
    /// étage choisi pour l'intérieur (cloisons, meubles, lumières)
    pub floor: u8,
    /// cloison en cours : (bâtiment, premier point) ; lumière à poser d'un clic (bâtiment)
    pub wall_pick: Option<(usize, Option<[f32; 2]>)>,
    pub light_pick: Option<usize>,
    /// nouvelle cloison avec un passage
    pub wall_door: bool,
    /// fenêtres d'un clic : (bâtiment, 1 retirer / remettre, 2 ajouter) ; taille et modèle d'une fenêtre ajoutée
    pub win_pick: Option<(usize, u8)>,
    pub win_preset: u8,
    pub win_add_style: u8,
}

impl DwApp {
    /// Bâtiment de l'atelier porté par un objet, s'il y en a un.
    pub fn house_of(&self, archetype: &str) -> Option<usize> {
        let a = archetype.to_lowercase();
        self.proj.houses.iter().position(|h| h.name == a)
    }

    fn new_house_name(&self) -> String {
        let mut n = 1;
        while self.proj.houses.iter().any(|h| h.name == format!("{HOUSE_PREFIX}{n}")) {
            n += 1;
        }
        format!("{HOUSE_PREFIX}{n}")
    }

    /// Crée un bâtiment depuis une emprise (points du monde) : repère au centre, 1er côté le long de X.
    pub fn create_house(&mut self, pts: &[V3]) -> String {
        if pts.len() < 3 {
            return "Il faut au moins 3 points pour l'emprise d'un bâtiment.".into();
        }
        let n = pts.len() as f32;
        let c = v3(pts.iter().map(|p| p.x).sum::<f32>() / n, pts.iter().map(|p| p.y).sum::<f32>() / n, pts.iter().map(|p| p.z).fold(f32::MAX, f32::min));
        let d = pts[1] - pts[0];
        let yaw = d.y.atan2(d.x);
        let q = Quat::from_z(-yaw);
        let mut foot: Vec<[f32; 2]> = pts.iter().map(|p| {
            let l = q.rotate(v3(p.x - c.x, p.y - c.y, 0.0));
            [l.x, l.y]
        }).collect();
        if signed_area(&foot) < 0.0 {
            foot.reverse();
        }
        let area = signed_area(&foot);
        if area < 4.0 {
            return "Emprise trop petite (moins de 4 m²).".into();
        }
        let name = self.new_house_name();
        self.push_undo();
        self.proj.houses.push(House::new(&name, foot));
        let yi = self.ensure_target_ymap();
        let mut e = Entity::new(&name, c);
        e.rot = Quat::from_z(yaw);
        e.lod_dist = HOUSE_LOD;
        let id = e.id;
        self.proj.ymaps[yi].entities.push(e);
        let hi = self.proj.houses.len() - 1;
        let door = if self.game.world.is_some() { format!(" {}", self.house_place_door(hi)) } else { String::new() };
        self.set_selection(vec![id]);
        self.sync_game_project();
        self.atelier.sig = 0;
        let _ = &door;
        format!("Bâtiment « {name} » créé : {area:.0} m² au sol, 2 étages. Règle-le dans l'atelier (il est sélectionné).")
    }

    /// Clics dans la vue pendant le tracé de l'emprise.
    pub fn atelier_input(&mut self, resp: &egui::Response, ctx: &egui::Context) {
        if let Some((hi, mode)) = self.atelier.win_pick {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.atelier.win_pick = None;
                return;
            }
            if resp.clicked_by(PointerButton::Primary) {
                let Some(mouse) = ctx.input(|i| i.pointer.interact_pos()) else { return };
                self.status = match self.house_wall_hit_z(hi, mouse) {
                    Some((e, t, z)) => self.house_window_click(hi, e, t, z, mode),
                    None => "Clique sur un mur de ce bâtiment.".into(),
                };
            }
            return;
        }
        if self.atelier.wall_pick.is_some() || self.atelier.light_pick.is_some() {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.atelier.wall_pick = None;
                self.atelier.light_pick = None;
                return;
            }
            if !resp.clicked_by(PointerButton::Primary) {
                return;
            }
            let Some(mouse) = ctx.input(|i| i.pointer.interact_pos()) else { return };
            let hi = self.atelier.wall_pick.map(|w| w.0).or(self.atelier.light_pick).unwrap_or(0);
            let Some(p) = self.house_floor_hit(hi, self.atelier.floor, mouse) else {
                self.status = "Clique sur le sol de l'étage choisi, à l'intérieur du bâtiment.".into();
                return;
            };
            if let Some((hi, first)) = self.atelier.wall_pick {
                match first {
                    None => {
                        self.atelier.wall_pick = Some((hi, Some(p)));
                        self.status = "Cloison : premier point posé, clique le second.".into();
                    }
                    Some(a) => {
                        // alignée sur les murs quand elle en est proche (à 10° près)
                        let mut b = p;
                        let d = [b[0] - a[0], b[1] - a[1]];
                        let ang = d[1].atan2(d[0]);
                        let q = std::f32::consts::FRAC_PI_2;
                        let k = (ang / q).round();
                        if (ang - k * q).abs() < 10f32.to_radians() {
                            let l = (d[0] * d[0] + d[1] * d[1]).sqrt();
                            b = [a[0] + l * (k * q).cos(), a[1] + l * (k * q).sin()];
                        }
                        self.push_undo();
                        let floor = self.atelier.floor;
                        let door = self.atelier.wall_door;
                        self.proj.houses[hi].walls.push(Partition { floor, a, b, door });
                        self.atelier.wall_pick = Some((hi, None));
                        self.atelier.sig = 0;
                        self.status = "Cloison ajoutée. Clique deux autres points pour une autre cloison, Échap pour finir.".into();
                    }
                }
            } else if let Some(hi) = self.atelier.light_pick {
                self.status = self.house_add_light(hi, self.atelier.floor, p);
            }
            return;
        }
        if let Some((hi, which)) = self.atelier.door_pick {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.atelier.door_pick = None;
                return;
            }
            if resp.clicked_by(PointerButton::Primary) {
                let Some(mouse) = ctx.input(|i| i.pointer.interact_pos()) else { return };
                match self.house_wall_hit(hi, mouse) {
                    Some((edge, t)) => {
                        self.push_undo();
                        let i = match which {
                            Some(i) => {
                                if let Some(mut d) = self.proj.houses[hi].doors().get(i).cloned() {
                                    d.edge = edge;
                                    d.t = t;
                                    self.proj.houses[hi].put_door(i, d);
                                }
                                i
                            }
                            None => {
                                let model = self.proj.houses[hi].door_model.clone();
                                self.proj.houses[hi].more_doors.push(DoorSpec { edge, t, model, prop: None, bbox: None });
                                self.proj.houses[hi].more_doors.len()
                            }
                        };
                        self.atelier.door_pick = None;
                        self.atelier.sig = 0;
                        let placed = self.proj.houses[hi].doors().get(i).map(|d| d.prop.is_none()).unwrap_or(false) && self.game.world.is_some();
                        self.status = if placed { self.house_set_door(hi, i, None) } else { format!("Porte {} placée sur le côté {}.", i + 1, edge + 1) };
                        self.house_doors_follow();
                    }
                    None => self.status = "Clique sur un mur de ce bâtiment.".into(),
                }
            }
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.atelier.tracing = false;
            self.atelier.pts.clear();
            self.status = "Tracé de l'emprise annulé.".into();
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Backspace)) {
            self.atelier.pts.pop();
        }
        let finish = resp.double_clicked_by(PointerButton::Primary) || ctx.input(|i| i.key_pressed(egui::Key::Enter));
        if finish {
            let pts = std::mem::take(&mut self.atelier.pts);
            self.atelier.tracing = false;
            self.status = self.create_house(&pts);
            return;
        }
        if resp.clicked_by(PointerButton::Primary) {
            if let Some(g) = self.mouse_ground {
                let p = self.atelier_snap(g);
                self.atelier.pts.push(p);
                self.status = format!("Emprise : {} point(s). Double-clic ou Entrée pour créer le bâtiment · Retour arrière : enlever le dernier · Échap : annuler.", self.atelier.pts.len());
            }
        }
    }

    /// Clic sur un mur en mode fenêtres : retirer (ou remettre) la fenêtre sous la souris, ou en ajouter une.
    fn house_window_click(&mut self, hi: usize, e: usize, t: f32, z: f32, mode: u8) -> String {
        let h = self.proj.houses[hi].clone();
        let len = h.edges()[e].4;
        let s = t * len;
        let f = ((z / h.floor_h).floor().max(0.0) as u8).min(h.floors.max(1) - 1);
        let hit = h.holes(e, f).into_iter().find(|o| o.kind != 1 && s >= o.s0 - 0.1 && s <= o.s1 + 0.1);
        self.push_undo();
        let hm = &mut self.proj.houses[hi];
        let msg = if mode == 1 {
            match hit {
                Some(o) => {
                    let c = (o.s0 + o.s1) * 0.5;
                    if let Some(k) = hm.win_add.iter().position(|w| w.edge == e && w.floor == f && (w.s - c).abs() < 0.3 + w.w * 0.5) {
                        hm.win_add.remove(k);
                        "Fenêtre ajoutée retirée.".to_string()
                    } else {
                        hm.win_off.push(WinRef { edge: e, floor: f, s: c });
                        format!("Fenêtre retirée (côté {}, étage {f}). Reclique au même endroit pour la remettre.", e + 1)
                    }
                }
                None => match hm.win_off.iter().position(|r| r.edge == e && r.floor == f && (r.s - s).abs() < 1.0) {
                    Some(k) => {
                        hm.win_off.remove(k);
                        "Fenêtre remise.".to_string()
                    }
                    None => "Pas de fenêtre à cet endroit.".to_string(),
                },
            }
        } else {
            let (name, w, hh, sill) = WIN_PRESETS[(self.atelier.win_preset as usize).min(WIN_PRESETS.len() - 1)];
            hm.win_add.push(WinSpec { edge: e, floor: f, s, w, h: hh, sill, style: self.atelier.win_add_style });
            format!("{name} ajoutée (côté {}, étage {f}).", e + 1)
        };
        self.atelier.sig = 0;
        msg
    }

    /// Point du sol de l'étage `f` sous la souris (repère du bâtiment), aligné sur 25 cm.
    fn house_floor_hit(&self, hi: usize, f: u8, mouse: Pos2) -> Option<[f32; 2]> {
        let h = self.proj.houses.get(hi)?;
        let b = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.archetype.to_lowercase() == h.name)?;
        let (o, d) = self.cam.ray(mouse);
        let inv = b.rot.inverse();
        let (lo, ld) = (inv.rotate(o - b.pos), inv.rotate(d));
        let zf = f as f32 * h.floor_h + if f == 0 { FLOOR_LIFT } else { 0.0 };
        if ld.z.abs() < 1e-4 {
            return None;
        }
        let t = (zf - lo.z) / ld.z;
        if t <= 0.0 {
            return None;
        }
        let p = lo + ld * t;
        let snap = |v: f32| (v * 4.0).round() / 4.0;
        Some([snap(p.x), snap(p.y)])
    }

    /// Intérieur créé depuis ce bâtiment : (fichier de types, intérieur).
    fn house_mlo(&self, hi: usize) -> Option<(usize, usize)> {
        let name = self.proj.houses.get(hi)?.interior.clone()?;
        for (ti, t) in self.proj.ytyps.iter().enumerate() {
            if let Some(mi) = t.mlos.iter().position(|m| m.name.eq_ignore_ascii_case(&name)) {
                return Some((ti, mi));
            }
        }
        None
    }

    /// Plafonnier du jeu au point `p` de l'étage `f` : dans l'intérieur s'il y en a un (pièce sous le point), sinon
    /// comme objet du monde, accroché au plafond.
    pub fn house_add_light(&mut self, hi: usize, f: u8, p: [f32; 2]) -> String {
        let Some((ln, _, lmx)) = self.find_game_prop(&["light"], &["ceil", "_ceiling", "v_res_", "v_serv_ct"], v3(0.1, 0.1, 0.02), v3(1.6, 1.6, 0.8), v3(0.6, 0.6, 0.15)) else {
            return "Aucun plafonnier du jeu trouvé : charge le jeu (onglet Jeu).".into();
        };
        let h = self.proj.houses[hi].clone();
        let ceil = if f + 1 >= h.floors.max(1) { h.height() - 0.02 } else { (f + 1) as f32 * h.floor_h - SLAB_T };
        let local = v3(p[0], p[1], ceil - lmx.z.max(0.0) - 0.02);
        self.push_undo();
        if let Some((ti, mi)) = self.house_mlo(hi) {
            let m = &self.proj.ytyps[ti].mlos[mi];
            let room = (1..m.rooms.len()).find(|&r| { let x = &m.rooms[r]; local.x >= x.bb_min.x && local.x <= x.bb_max.x && local.y >= x.bb_min.y && local.y <= x.bb_max.y && local.z >= x.bb_min.z - 0.5 && local.z <= x.bb_max.z + 0.5 }).unwrap_or((f as usize + 1).min(m.rooms.len().saturating_sub(1)));
            let mut e = Entity::new(&ln, local);
            e.flags = crate::mech::MLO_ENTITY_FLAGS;
            e.room = room;
            self.proj.ytyps[ti].mlos[mi].entities.push(e);
            return format!("Plafonnier {ln} ajouté dans la pièce {room} de l'intérieur.");
        }
        let Some(b) = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.archetype.to_lowercase() == h.name).cloned() else { return String::new() };
        let yi = self.ensure_target_ymap();
        let mut e = Entity::new(&ln, b.pos + b.rot.rotate(local));
        e.rot = b.rot;
        e.lod_dist = 60.0;
        self.proj.ymaps[yi].entities.push(e);
        self.sync_game_project();
        format!("Plafonnier {ln} ajouté (objet du monde : transforme le bâtiment en intérieur pour un vrai éclairage d'intérieur).")
    }

    /// Mur du bâtiment sous la souris : (côté, position le long du côté 0..1).
    fn house_wall_hit(&self, hi: usize, mouse: Pos2) -> Option<(usize, f32)> {
        self.house_wall_hit_z(hi, mouse).map(|x| (x.0, x.1))
    }

    /// Mur sous la souris : (côté, position 0..1, hauteur au-dessus du pied du bâtiment).
    fn house_wall_hit_z(&self, hi: usize, mouse: Pos2) -> Option<(usize, f32, f32)> {
        let h = self.proj.houses.get(hi)?;
        let b = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.archetype.to_lowercase() == h.name)?;
        let (o, d) = self.cam.ray(mouse);
        let inv = b.rot.inverse();
        let (lo, ld) = (inv.rotate(o - b.pos), inv.rotate(d));
        let top = h.height();
        let mut best: Option<(f32, usize, f32, f32)> = None;
        for (e, &(a, _, dir, out, len)) in h.edges().iter().enumerate() {
            let den = ld.dot(out);
            if den.abs() < 1e-4 {
                continue;
            }
            let t = (a - lo).dot(out) / den;
            if t <= 0.0 {
                continue;
            }
            let p = lo + ld * t;
            let s = (p - a).dot(dir);
            if s < 0.0 || s > len || p.z < -0.5 || p.z > top + 0.5 {
                continue;
            }
            if best.map(|x| t < x.0).unwrap_or(true) {
                best = Some((t, e, s / len, p.z));
            }
        }
        best.map(|x| (x.1, x.2, x.3))
    }

    /// Point suivant de l'emprise : angle droit (ou alignement) avec le côté précédent s'il en est à moins de 10°,
    /// longueur arrondie à 0,5 m.
    fn atelier_snap(&self, g: V3) -> V3 {
        let pts = &self.atelier.pts;
        let Some(&last) = pts.last() else { return g };
        let d = g - last;
        let len = (d.x * d.x + d.y * d.y).sqrt();
        if len < 0.1 {
            return g;
        }
        let mut ang = d.y.atan2(d.x);
        if self.atelier.square && pts.len() >= 2 {
            let p = pts[pts.len() - 1] - pts[pts.len() - 2];
            let base = p.y.atan2(p.x);
            let q = std::f32::consts::FRAC_PI_2;
            let k = ((ang - base) / q).round();
            if ((ang - base) - k * q).abs() < 10f32.to_radians() {
                ang = base + k * q;
            }
        }
        let len = (len * 2.0).round() / 2.0;
        v3(last.x + len * ang.cos(), last.y + len * ang.sin(), g.z)
    }

    /// Emprise en cours dans la vue.
    pub fn atelier_draw(&self, painter: &egui::Painter) {
        if self.atelier.door_pick.is_some() {
            if let Some(g) = self.mouse_ground.and_then(|g| self.cam.project(g)) {
                painter.text(g.0 + egui::vec2(14.0, -14.0), egui::Align2::LEFT_BOTTOM, "clique sur un mur pour y mettre la porte (Échap : annuler)", egui::FontId::proportional(13.0), Color32::from_rgb(255, 210, 120));
            }
            return;
        }
        if !self.atelier.tracing {
            return;
        }
        let mut pts = self.atelier.pts.clone();
        if let Some(g) = self.mouse_ground {
            pts.push(self.atelier_snap(g));
        }
        let scr: Vec<Pos2> = pts.iter().filter_map(|p| self.cam.project(*p + v3(0.0, 0.0, 0.2)).map(|q| q.0)).collect();
        if scr.len() >= 2 {
            let mut closed = scr.clone();
            closed.push(scr[0]);
            painter.add(egui::Shape::line(closed, egui::Stroke::new(2.0_f32, Color32::from_rgb(255, 190, 90))));
        }
        for w in pts.windows(2) {
            let m = (w[0] + w[1]) * 0.5;
            if let Some((q, _)) = self.cam.project(m + v3(0.0, 0.0, 0.4)) {
                painter.text(q, egui::Align2::CENTER_BOTTOM, format!("{:.1} m", (w[1] - w[0]).len()), egui::FontId::proportional(12.0), Color32::WHITE);
            }
        }
        for q in scr {
            painter.circle_filled(q, 5.0, Color32::from_rgb(255, 150, 50));
        }
    }

    /// Pose une vraie porte du jeu dans l'entrée du bâtiment (objet du projet, qui suit ensuite le bâtiment).
    pub fn house_place_door(&mut self, hi: usize) -> String {
        self.house_set_door(hi, 0, None)
    }

    /// Modèles de porte proposés qui existent dans le jeu chargé.
    pub fn door_models(&self) -> Vec<(&'static str, &'static str)> {
        let Some(w) = self.game.world.as_ref() else { return Vec::new() };
        DOOR_MODELS.iter().copied().filter(|(n, _)| w.archetypes.contains_key(&crate::gta::crypto::joaat(n))).collect()
    }

    /// Pose (ou remplace) la vraie porte du jeu de la porte `i` du bâtiment, avec le modèle donné (sinon celui déjà
    /// choisi, sinon la porte de maison par défaut).
    pub fn house_set_door(&mut self, hi: usize, i: usize, model: Option<String>) -> String {
        let h = self.proj.houses[hi].clone();
        if h.interior.is_some() {
            return "Ce bâtiment est un intérieur : ses portes sont dans l'intérieur.".into();
        }
        let Some(mut d) = h.doors().get(i).cloned() else { return String::new() };
        let want = model.or_else(|| (!d.model.is_empty()).then(|| d.model.clone()));
        let found = match want {
            Some(n) => self.game.world.as_ref().and_then(|w| w.archetypes.get(&crate::gta::crypto::joaat(&n)).map(|a| (n.clone(), a.bb_min, a.bb_max))),
            None => self.exterior_door(),
        };
        let Some((dn, dmn, dmx)) = found else {
            return "Porte du jeu introuvable : charge le jeu (onglet Jeu).".into();
        };
        if let Some(old) = d.prop.take() {
            self.proj.delete_ids(&std::iter::once(old).collect());
        }
        let yi = self.ensure_target_ymap();
        let mut e = Entity::new(&dn, V3::ZERO);
        e.lod_dist = 100.0;
        d.prop = Some(e.id);
        d.bbox = Some(([dmn.x, dmn.y, dmn.z], [dmx.x, dmx.y, dmx.z]));
        d.model = dn.clone();
        self.proj.ymaps[yi].entities.push(e);
        self.proj.houses[hi].put_door(i, d);
        self.house_doors_follow();
        self.sync_game_project();
        self.atelier.sig = 0;
        format!("Porte {} : {dn}, ajustée à l'ouverture ; elle suit le bâtiment et s'ouvre en jeu (dw_doors.lua).", i + 1)
    }

    #[allow(dead_code)]
    fn house_place_door_old(&mut self, hi: usize) -> String {
        let h = self.proj.houses[hi].clone();
        if h.interior.is_some() {
            return "Ce bâtiment est un intérieur : sa porte est dans l'intérieur.".into();
        }
        // une porte extérieure du jeu (objet ordinaire) : les portes v_ilev_* sont des portes d'intérieur, qui ne
        // s'affichaient pas en jeu posées dehors (retour de la personne, 13 nov. 2026)
        let Some((dn, dmn, dmx)) = self.exterior_door() else {
            return "Aucune porte du jeu trouvée : charge le jeu (onglet Jeu).".into();
        };
        if let Some(old) = h.door_prop {
            self.proj.delete_ids(&std::iter::once(old).collect());
        }
        let yi = self.ensure_target_ymap();
        let mut e = Entity::new(&dn, V3::ZERO);
        e.lod_dist = 100.0;
        let id = e.id;
        self.proj.ymaps[yi].entities.push(e);
        self.proj.houses[hi].door_prop = Some(id);
        self.proj.houses[hi].door_box = Some(([dmn.x, dmn.y, dmn.z], [dmx.x, dmx.y, dmx.z]));
        self.house_doors_follow();
        self.sync_game_project();
        format!("Porte {dn} posée dans l'entrée ; elle suit le bâtiment et s'ouvre en jeu (dw_doors.lua).")
    }

    /// Porte extérieure du jeu pour une maison : la première de cette liste qui existe dans le jeu chargé (portes de
    /// maison et de commerce posées dehors par le jeu, sonde DW_ARCH_GREP).
    pub fn exterior_door(&self) -> Option<(String, V3, V3)> {
        let w = self.game.world.as_ref()?;
        for n in ["prop_magenta_door", "prop_ron_door_01", "prop_map_door_01", "prop_ret_door_04", "prop_shop_front_door_l"] {
            if let Some(a) = w.archetypes.get(&crate::gta::crypto::joaat(n)) {
                return Some((n.to_string(), a.bb_min, a.bb_max));
            }
        }
        None
    }

    /// Les portes des bâtiments restent dans leur entrée quand le bâtiment bouge ou que l'entrée change.
    pub fn house_doors_follow(&mut self) {
        // porte notée mais dont l'objet a disparu du projet (vu dans un projet de la personne, 13 nov. 2026) :
        // reposée, sinon rien n'est exporté pour elle
        let mut lost: Vec<(usize, usize)> = Vec::new();
        if self.game.world.is_some() {
            let ids: std::collections::HashSet<u64> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter().map(|e| e.id)).collect();
            let placed: std::collections::HashSet<String> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter().map(|e| e.archetype.to_lowercase())).collect();
            for (hi, h) in self.proj.houses.iter().enumerate() {
                if h.interior.is_some() || !placed.contains(&h.name) {
                    continue;
                }
                for (i, d) in h.doors().iter().enumerate() {
                    if d.prop.map(|p| !ids.contains(&p)).unwrap_or(false) {
                        lost.push((hi, i));
                    }
                }
            }
        }
        for (hi, i) in lost {
            if let Some(mut d) = self.proj.houses[hi].doors().get(i).cloned() {
                d.prop = None;
                self.proj.houses[hi].put_door(i, d);
            }
            let _ = self.house_set_door(hi, i, None);
        }
        let mut moves: Vec<(u64, V3, Quat, f32, f32)> = Vec::new();
        for h in &self.proj.houses {
            let Some(b) = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.archetype.to_lowercase() == h.name) else { continue };
            for (i, d) in h.doors().iter().enumerate() {
                let Some(did) = d.prop else { continue };
                let Some((local, ang, sxy, sz)) = h.door_fit(i) else { continue };
                moves.push((did, b.pos + b.rot.rotate(local), b.rot.mul(Quat::from_z(ang)).normalized(), sxy, sz));
            }
        }
        for (id, p, q, sxy, sz) in moves {
            for y in self.proj.ymaps.iter_mut() {
                if let Some(e) = y.entities.iter_mut().find(|e| e.id == id) {
                    if (e.pos - p).len() > 1e-3 || (e.rot.x - q.x).abs() + (e.rot.y - q.y).abs() + (e.rot.z - q.z).abs() + (e.rot.w - q.w).abs() > 1e-4 || (e.scale_xy - sxy).abs() > 1e-3 || (e.scale_z - sz).abs() > 1e-3 {
                        e.pos = p;
                        e.rot = q;
                        e.scale_xy = sxy;
                        e.scale_z = sz;
                    }
                }
            }
        }
    }

    /// Modèles des bâtiments envoyés à la vue (et leurs tailles au catalogue) quand ils changent.
    pub fn houses_tick(&mut self) {
        if self.proj.houses.is_empty() {
            return;
        }
        if self.frame % 10 == 0 {
            self.house_doors_follow();
        }
        let Some(gpu) = self.game.gpu.clone() else { return };
        let sig = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (std::sync::Arc::as_ptr(&gpu) as usize).hash(&mut h);
            serde_json::to_string(&self.proj.houses).unwrap_or_default().hash(&mut h);
            h.finish() | 1
        };
        if sig == self.atelier.sig {
            return;
        }
        self.atelier.sig = sig;
        let mut boxes = std::collections::HashMap::new();
        if let Ok(mut g) = gpu.lock() {
            for h in &self.proj.houses {
                let (model, keys, texs) = house_rmodel(h, false);
                g.add_custom(crate::gta::loader::Loaded { arch: crate::gta::crypto::joaat(&h.name), model: Some(model), part_keys: keys, layer_keys: Vec::new(), textures: texs });
                boxes.insert(h.name.clone(), h.bbox());
            }
        }
        crate::catalog::set_house_boxes(boxes);
    }

    /// Fenêtre de l'atelier.
    pub fn atelier_window(&mut self, ctx: &egui::Context) {
        if !self.atelier.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Atelier de bâtiment").open(&mut open).default_width(330.0).show(ctx, |ui| {
            ui.label(RichText::new("Trace l'emprise au sol, puis règle les étages, le toit et la façade. Le bâtiment est un objet normal : déplace-le, tourne-le, duplique-le.").small());
            ui.horizontal(|ui| {
                if ui.selectable_label(self.atelier.tracing, "Tracer l'emprise").on_hover_text("Clique les coins du bâtiment sur le sol · double-clic ou Entrée : créer · Retour arrière : enlever le dernier point · Échap : annuler").clicked() {
                    self.atelier.tracing = !self.atelier.tracing;
                    self.atelier.pts.clear();
                }
                ui.checkbox(&mut self.atelier.square, "Angles droits");
            });
            ui.horizontal(|ui| {
                if self.atelier.quick == (0.0, 0.0) {
                    self.atelier.quick = (12.0, 9.0);
                }
                ui.add(egui::DragValue::new(&mut self.atelier.quick.0).range(3.0..=120.0).speed(0.5).suffix(" m"));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut self.atelier.quick.1).range(3.0..=120.0).speed(0.5).suffix(" m"));
                if ui.button("Rectangle au centre de la vue").clicked() {
                    let c = self.cam.target;
                    let z = self.mouse_ground.map(|g| g.z).unwrap_or(c.z);
                    let (w, d) = (self.atelier.quick.0 * 0.5, self.atelier.quick.1 * 0.5);
                    let pts = [v3(c.x - w, c.y - d, z), v3(c.x + w, c.y - d, z), v3(c.x + w, c.y + d, z), v3(c.x - w, c.y + d, z)];
                    self.status = self.create_house(&pts);
                }
            });
            ui.separator();
            // bâtiment sélectionné
            let sel = self.selected.first().copied().and_then(|id| self.proj.find_id(id)).and_then(|r| self.proj.world_entity(r)).and_then(|e| self.house_of(&e.archetype));
            match sel {
                None => {
                    ui.label(RichText::new("Sélectionne un bâtiment de l'atelier pour le régler.").small().weak());
                }
                Some(hi) => {
                    let mut h = self.proj.houses[hi].clone();
                    ui.label(RichText::new(format!("{} · {:.0} m² · {} côtés", h.name, signed_area(&h.foot), h.foot.len())).strong());
                    let mut ch = false;
                    ch |= ui.add(egui::Slider::new(&mut h.floors, 1..=30).text("étages")).changed();
                    ch |= ui.add(egui::Slider::new(&mut h.floor_h, 2.6..=6.0).step_by(0.1).suffix(" m").text("hauteur d'étage")).changed();
                    egui::ComboBox::from_label("toit").selected_text(ROOFS[h.roof.min(2) as usize]).show_ui(ui, |ui| {
                        for (i, n) in ROOFS.iter().enumerate() {
                            ch |= ui.selectable_value(&mut h.roof, i as u8, *n).changed();
                        }
                    });
                    egui::ComboBox::from_label("façade").selected_text(FACADES[h.facade.min(3) as usize]).show_ui(ui, |ui| {
                        for (i, n) in FACADES.iter().enumerate() {
                            ch |= ui.selectable_value(&mut h.facade, i as u8, *n).changed();
                        }
                    });
                    let gname = |g: u8| if g == 0 { "Texture DragoWalker".to_string() } else { GAME_TEX[g as usize - 1].0.to_string() };
                    egui::ComboBox::from_label("façade du jeu").selected_text(gname(h.game_facade)).show_ui(ui, |ui| {
                        ch |= ui.selectable_value(&mut h.game_facade, 0, "Texture DragoWalker").changed();
                        for (i, g) in GAME_TEX.iter().enumerate().filter(|g| g.1 .3 == 0) {
                            ch |= ui.selectable_value(&mut h.game_facade, i as u8 + 1, g.0).changed();
                        }
                    });
                    if h.roof != 0 {
                        egui::ComboBox::from_label("tuiles du jeu").selected_text(gname(h.game_roof)).show_ui(ui, |ui| {
                            ch |= ui.selectable_value(&mut h.game_roof, 0, "Texture DragoWalker").changed();
                            for (i, g) in GAME_TEX.iter().enumerate().filter(|g| g.1 .3 == 1) {
                                ch |= ui.selectable_value(&mut h.game_roof, i as u8 + 1, g.0).changed();
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ch |= ui.add(egui::Slider::new(&mut h.grime, 0.0..=1.0).text("salissures du jeu")).on_hover_text("Vrais décalques du jeu (coulures, crasse, rouille) sous les fenêtres et au bas des murs, toujours les mêmes pour ce bâtiment").changed();
                        egui::ComboBox::from_id_salt("grime_kind").selected_text(GAME_TEX.get(h.grime_kind as usize).map(|g| g.0).unwrap_or("?")).show_ui(ui, |ui| {
                            for (i, g) in GAME_TEX.iter().enumerate().filter(|g| g.1 .3 == 2) {
                                ch |= ui.selectable_value(&mut h.grime_kind, i as u8, g.0).changed();
                            }
                        });
                    });
                    ui.label(RichText::new("Textures et salissures du jeu : lues dans le jeu installé (charge-le pour les voir) ; à l'export, le jeu les charge lui-même, rien n'est copié.").small().weak());
                    ui.label(RichText::new("Fenêtres").strong());
                    egui::ComboBox::from_label("modèle des fenêtres").selected_text(WIN_STYLES[(h.win_style as usize).min(3)]).show_ui(ui, |ui| {
                        for (i, n) in WIN_STYLES.iter().enumerate() {
                            ch |= ui.selectable_value(&mut h.win_style, i as u8, *n).changed();
                        }
                    });
                    ui.horizontal(|ui| {
                        let on1 = self.atelier.win_pick.map(|p| p.1 == 1).unwrap_or(false);
                        if ui.selectable_label(on1, "Retirer / remettre").on_hover_text("Clic sur une fenêtre : elle est retirée (mur plein) ; reclic au même endroit : elle revient. Échap pour finir.").clicked() {
                            self.atelier.win_pick = if on1 { None } else { Some((hi, 1)) };
                        }
                        let on2 = self.atelier.win_pick.map(|p| p.1 == 2).unwrap_or(false);
                        if ui.selectable_label(on2, "Ajouter").on_hover_text("Clic sur un mur : une fenêtre de la taille et du modèle choisis, à cet endroit et à cet étage (elle remplace les fenêtres qu'elle chevauche). Échap pour finir.").clicked() {
                            self.atelier.win_pick = if on2 { None } else { Some((hi, 2)) };
                        }
                    });
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("win_preset").selected_text(WIN_PRESETS[(self.atelier.win_preset as usize).min(4)].0).show_ui(ui, |ui| {
                            for (i, p) in WIN_PRESETS.iter().enumerate() {
                                ui.selectable_value(&mut self.atelier.win_preset, i as u8, format!("{} ({:.1} × {:.1} m)", p.0, p.1, p.2));
                            }
                        });
                        egui::ComboBox::from_id_salt("win_add_style").selected_text(WIN_STYLES[(self.atelier.win_add_style as usize).min(3)]).show_ui(ui, |ui| {
                            for (i, n) in WIN_STYLES.iter().enumerate() {
                                ui.selectable_value(&mut self.atelier.win_add_style, i as u8, *n);
                            }
                        });
                    });
                    if !h.win_off.is_empty() || !h.win_add.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} retirée(s), {} ajoutée(s)", h.win_off.len(), h.win_add.len())).small());
                            if ui.small_button("Tout remettre").on_hover_text("Les fenêtres automatiques reviennent, les fenêtres ajoutées partent").clicked() {
                                h.win_off.clear();
                                h.win_add.clear();
                                ch = true;
                            }
                        });
                    }
                    ch |= ui.add(egui::Slider::new(&mut h.win_w, 0.4..=3.0).step_by(0.1).suffix(" m").text("largeur des fenêtres")).changed();
                    ch |= ui.add(egui::Slider::new(&mut h.win_h, 0.4..=3.0).step_by(0.1).suffix(" m").text("hauteur des fenêtres")).changed();
                    ch |= ui.add(egui::Slider::new(&mut h.win_step, 1.0..=8.0).step_by(0.1).suffix(" m").text("écart entre fenêtres")).changed();
                    ch |= ui.checkbox(&mut h.shop, "Rez-de-chaussée en vitrines").changed();
                    // portes : modèle du jeu au choix, placées d'un clic sur un mur
                    ui.label(RichText::new("Portes").strong());
                    let models = self.door_models();
                    let mut set_model: Option<(usize, String)> = None;
                    let mut remove: Option<usize> = None;
                    for (i, d) in h.doors().iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} · côté {}", i + 1, d.edge.min(h.foot.len().saturating_sub(1)) + 1));
                            let cur = if d.model.is_empty() { "porte de maison (par défaut)".to_string() } else { d.model.clone() };
                            egui::ComboBox::from_id_salt(("door_model", i)).width(170.0).selected_text(cur).show_ui(ui, |ui| {
                                for (n, label) in &models {
                                    if ui.selectable_label(d.model == *n, format!("{n} — {label}")).clicked() {
                                        set_model = Some((i, n.to_string()));
                                    }
                                }
                            });
                            if ui.small_button("Déplacer").on_hover_text("Puis clique sur un mur du bâtiment, à l'endroit voulu").clicked() {
                                self.atelier.door_pick = Some((hi, Some(i)));
                            }
                            if i > 0 && ui.small_button("Retirer").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if ui.button("+ Ajouter une porte").on_hover_text("Puis clique sur un mur du bâtiment (rez-de-chaussée), à l'endroit voulu").clicked() {
                        self.atelier.door_pick = Some((hi, None));
                    }
                    if models.is_empty() {
                        ui.label(RichText::new("Charge le jeu pour choisir les portes du jeu.").small().weak());
                    }
                    ch |= ui.add(egui::Slider::new(&mut h.lit, 0.0..=1.0).text("fenêtres éclairées la nuit")).on_hover_text("Part des vitres qui s'allument la nuit en jeu (matériau du jeu qui brille la nuit) ; dans la vue, elles ont une teinte chaude.").changed();
                    if ch {
                        self.proj.houses[hi] = h.clone();
                    }
                    if let Some((i, n)) = set_model {
                        self.push_undo();
                        if h.interior.is_some() {
                            if let Some(mut d) = self.proj.houses[hi].doors().get(i).cloned() {
                                d.model = n;
                                self.proj.houses[hi].put_door(i, d);
                            }
                            self.status = "Modèle gardé ; dans un bâtiment devenu intérieur, refais « Transformer en intérieur » pour l'appliquer.".into();
                        } else {
                            self.status = self.house_set_door(hi, i, Some(n));
                        }
                    }
                    if let Some(i) = remove {
                        self.push_undo();
                        if let Some(d) = self.proj.houses[hi].more_doors.get(i - 1).cloned() {
                            if let Some(p) = d.prop {
                                self.proj.delete_ids(&std::iter::once(p).collect());
                            }
                            self.proj.houses[hi].more_doors.remove(i - 1);
                        }
                        self.atelier.sig = 0;
                        self.sync_game_project();
                    }
                    // intérieur de la maison : sol, cloisons, meubles, lumières
                    ui.separator();
                    ui.label(RichText::new("Intérieur de la maison").strong());
                    let mut hh = self.proj.houses[hi].clone();
                    let mut ch2 = false;
                    let floor_name = |k: u8| -> String {
                        if (k as usize) < FLOORS.len() { FLOORS[k as usize].to_string() } else { GAME_TEX.iter().filter(|g| g.3 == 3).nth(k as usize - 4).map(|g| format!("{} (jeu)", g.0)).unwrap_or_default() }
                    };
                    egui::ComboBox::from_label("sol").selected_text(floor_name(hh.floor_kind)).show_ui(ui, |ui| {
                        for k in 0..FLOORS.len() as u8 {
                            ch2 |= ui.selectable_value(&mut hh.floor_kind, k, floor_name(k)).changed();
                        }
                        for (j, _) in GAME_TEX.iter().filter(|g| g.3 == 3).enumerate() {
                            let k = 4 + j as u8;
                            ch2 |= ui.selectable_value(&mut hh.floor_kind, k, floor_name(k)).changed();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("étage");
                        let maxf = hh.floors.max(1) - 1;
                        ui.add(egui::DragValue::new(&mut self.atelier.floor).range(0..=maxf));
                        ui.label(RichText::new("(0 = rez-de-chaussée)").small().weak());
                    });
                    ui.horizontal(|ui| {
                        if ui.selectable_label(self.atelier.wall_pick.is_some(), "Tracer une cloison").on_hover_text("Deux clics sur le sol de l'étage choisi : une cloison du sol au plafond (alignée sur les murs à 10° près). Plusieurs pièces par étage. Échap pour finir.").clicked() {
                            self.atelier.wall_pick = if self.atelier.wall_pick.is_some() { None } else { Some((hi, None)) };
                            self.atelier.light_pick = None;
                        }
                        ui.checkbox(&mut self.atelier.wall_door, "avec passage");
                    });
                    let nw = hh.walls.iter().filter(|w| w.floor == self.atelier.floor).count();
                    if nw > 0 {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{nw} cloison(s) à cet étage")).small());
                            if ui.small_button("Retirer la dernière").clicked() {
                                if let Some(k) = hh.walls.iter().rposition(|w| w.floor == self.atelier.floor) {
                                    hh.walls.remove(k);
                                    ch2 = true;
                                }
                            }
                        });
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.selectable_label(self.atelier.light_pick.is_some(), "Poser une lumière").on_hover_text("Clic sur le sol de l'étage choisi : un plafonnier du jeu accroché au plafond au-dessus (dans l'intérieur s'il y en a un). Échap pour finir.").clicked() {
                            self.atelier.light_pick = if self.atelier.light_pick.is_some() { None } else { Some(hi) };
                            self.atelier.wall_pick = None;
                        }
                    });
                    match self.house_mlo(hi) {
                        Some((ti, mi)) => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Meubler l'étage :");
                                for st in ["salon", "cuisine", "bureau", "bar", "magasin"] {
                                    if ui.small_button(st).on_hover_text("Meubles du jeu de ce type de pièce, posés sur le sol de l'étage choisi (dans l'intérieur)").clicked() {
                                        self.push_undo();
                                        self.cur_mlo = Some((ti, mi));
                                        let room = (self.atelier.floor as usize + 1).min(self.proj.ytyps[ti].mlos[mi].rooms.len().saturating_sub(1));
                                        self.status = self.furnish_rooms_inner(st, Some(room));
                                    }
                                }
                            });
                        }
                        None => {
                            ui.label(RichText::new("Meubler : transforme d'abord le bâtiment en intérieur (les meubles vont dans l'intérieur).").small().weak());
                        }
                    }
                    if ch2 {
                        self.push_undo();
                        self.proj.houses[hi] = hh;
                        self.atelier.sig = 0;
                    }
                    ui.horizontal(|ui| {
                        let has_int = h.interior.is_some();
                        if ui.add_enabled(!has_int, egui::Button::new("Transformer en intérieur")).on_hover_text("Une pièce par étage, fenêtres et porte en portails vers l'extérieur (on voit dehors), escalier et trémie entre les étages, vraie porte du jeu à l'entrée, meubles, lumières et ambiance.").clicked() {
                            self.status = self.house_to_interior(hi);
                        }
                        if has_int {
                            ui.label(RichText::new(format!("intérieur : {}", h.interior.clone().unwrap_or_default())).small());
                        }
                    });
                }
            }
            if !self.proj.houses.is_empty() {
                ui.separator();
                ui.label(RichText::new(format!("Bâtiments de l'atelier ({})", self.proj.houses.len())).strong());
                let mut del: Option<usize> = None;
                let mut goto: Option<u64> = None;
                for (i, h) in self.proj.houses.iter().enumerate() {
                    let n = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.archetype.to_lowercase() == h.name).count();
                    ui.horizontal(|ui| {
                        ui.label(format!("{} · {} étage(s) · posé {n} fois", h.name, h.floors));
                        if ui.small_button("Aller voir").clicked() {
                            goto = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.archetype.to_lowercase() == h.name).map(|e| e.id);
                        }
                        if ui.small_button("Supprimer").on_hover_text("Retire le bâtiment et tous ses placements").clicked() {
                            del = Some(i);
                        }
                    });
                }
                if let Some(id) = goto {
                    if let Some(p) = self.proj.find_id(id).and_then(|r| self.proj.world_entity(r)).map(|e| e.pos) {
                        self.goto(p, 60.0);
                        self.set_selection(vec![id]);
                    }
                }
                if let Some(i) = del {
                    self.push_undo();
                    let name = self.proj.houses.remove(i).name;
                    let ids: std::collections::HashSet<u64> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.archetype.to_lowercase() == name).map(|e| e.id).collect();
                    self.proj.delete_ids(&ids);
                    self.clean_selection();
                    self.sync_game_project();
                }
            }
        });
        if !open {
            self.atelier.open = false;
            self.atelier.tracing = false;
        }
    }

    /// Export des bâtiments de l'atelier : modèle détaillé et version lointaine (textures intégrées), collision de
    /// chaque placement (repère du monde), types dans « dw_house_types ». Rend (compte rendu, lignes du fxmanifest).
    pub fn export_houses(&self, stream: &std::path::Path) -> (Vec<String>, String) {
        for f in std::fs::read_dir(stream).into_iter().flatten().flatten() {
            let n = f.file_name().to_string_lossy().to_lowercase();
            if (n.starts_with(HOUSE_PREFIX) || n.starts_with("dw_house_types")) && n.ends_with(".xml") && f.path().is_file() {
                let _ = std::fs::remove_file(f.path());
            }
        }
        let names = crate::terrain::material_names(self.game.world.as_deref());
        let concrete = crate::terrain::coll_type(&names, "CONCRETE", 1);
        let ctypes = [crate::terrain::coll_type(&names, "BRICK", concrete), crate::terrain::coll_type(&names, "GLASS_SHOOT_THROUGH", concrete)];
        let mut archs = Vec::new();
        let (mut n, mut ncol, mut nlit) = (0, 0, 0);
        if self.proj.houses.iter().any(|h| self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).any(|e| e.archetype.to_lowercase() == h.name)) {
            if let Err(e) = crate::sollumz::write_ytd_xml(stream, HOUSE_TXD, &house_textures()) {
                return (vec![format!("Atelier : textures impossibles à écrire ({e}).")], String::new());
            }
        }
        for h in &self.proj.houses {
            let insts: Vec<crate::model::Entity> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.archetype.to_lowercase() == h.name).cloned().collect();
            if insts.is_empty() {
                continue;
            }
            let (model, keys, texs) = house_rmodel(h, false);
            // parties à textures DragoWalker dans le modèle principal ; celles à textures du jeu, un modèle par
            // dictionnaire du jeu (« <nom>_gN »), dont le type d'objet nomme ce dictionnaire : rien n'est copié
            let own: Vec<usize> = (0..model.geoms.len()).filter(|&i| game_tex_of(keys[i]).is_none()).collect();
            let sub = |sel: &[usize]| crate::gta::drawable::RModel { verts: model.verts.clone(), idx: model.idx.clone(), geoms: sel.iter().map(|&i| model.geoms[i].clone()).collect(), embedded_dict: None };
            let own_keys: Vec<u64> = own.iter().map(|&i| keys[i]).collect();
            let Ok(path) = crate::sollumz::write_ydr_xml_ex(stream, &h.name, &sub(&own), &own_keys, &texs, Some(HOUSE_TXD)) else { continue };
            let (bmn, bmx) = h.bbox();
            for (pn, txd) in house_pieces(h) {
                let sel: Vec<usize> = (0..model.geoms.len()).filter(|&i| game_tex_of(keys[i]).map(|g| g.1 == txd).unwrap_or(false)).collect();
                let sk: Vec<u64> = sel.iter().map(|&i| keys[i]).collect();
                let _ = crate::sollumz::write_ydr_xml_named(stream, &pn, &sub(&sel), &sk, &|k| game_tex_of(k).map(|g| g.2.to_string()));
                let (c, r) = ((bmn + bmx) * 0.5, (bmx - bmn).len() * 0.5);
                archs.push(format!(
                    "  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"{HOUSE_LOD}\" />\n   <flags value=\"0\" />\n   <specialAttribute value=\"0\" />\n   <bbMin x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bbMax x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsCentre x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsRadius value=\"{r}\" />\n   <hdTextureDist value=\"60\" />\n   <name>{pn}</name>\n   <textureDictionary>{txd}</textureDictionary>\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>ASSET_TYPE_DRAWABLE</assetType>\n   <assetName>{pn}</assetName>\n   <extensions />\n  </Item>",
                    bmn.x, bmn.y, bmn.z, bmx.x, bmx.y, bmx.z, c.x, c.y, c.z
                ));
            }
            if keys.iter().any(|k| (0..3u8).any(|st| *k == glass_key(st, true))) {
                if night_windows(&path, HOUSE_TXD).unwrap_or(false) {
                    nlit += 1;
                }
            }
            let lod_name = format!("{}_lod", h.name);
            let (lm, lk, lt) = house_rmodel(h, true);
            let _ = crate::sollumz::write_ydr_xml_ex(stream, &lod_name, &lm, &lk, &lt, Some(HOUSE_TXD));
            let (mn, mx) = h.bbox();
            for (nm, lod, child) in [(&h.name, HOUSE_LOD, 0.0), (&lod_name, HOUSE_LOD_FAR, HOUSE_LOD)] {
                let _ = child;
                let (c, r) = ((mn + mx) * 0.5, (mx - mn).len() * 0.5);
                archs.push(format!(
                    "  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"{lod}\" />\n   <flags value=\"0\" />\n   <specialAttribute value=\"0\" />\n   <bbMin x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bbMax x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsCentre x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsRadius value=\"{r}\" />\n   <hdTextureDist value=\"60\" />\n   <name>{nm}</name>\n   <textureDictionary>{HOUSE_TXD}</textureDictionary>\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>ASSET_TYPE_DRAWABLE</assetType>\n   <assetName>{nm}</assetName>\n   <extensions />\n  </Item>",
                    mn.x, mn.y, mn.z, mx.x, mx.y, mx.z, c.x, c.y, c.z
                ));
            }
            // collision de chaque placement, dans le repère du monde
            let (pos, _, parts) = h.mesh(false);
            for (k, e) in insts.iter().enumerate() {
                let world: Vec<V3> = pos.iter().map(|p| e.rot.rotate(*p) + e.pos).collect();
                let mut tris = Vec::new();
                for (_, list, m) in &parts {
                    if *m == 255 {
                        continue; // salissures : pas de collision
                    }
                    for c in list.chunks_exact(3) {
                        tris.push((c[0], c[1], c[2], *m));
                    }
                }
                if std::fs::write(stream.join(format!("{}_col_{k}.ybn.xml", h.name)), crate::roads::mesh_collision_xml(&world, &tris, &ctypes)).is_ok() {
                    ncol += 1;
                }
            }
            n += 1;
        }
        if archs.is_empty() {
            return (Vec::new(), String::new());
        }
        let ytyp = crate::model::Ytyp { name: "dw_house_types".into(), mlos: Vec::new(), visible: true, raw_archetypes: archs };
        let _ = std::fs::write(stream.join("dw_house_types.ytyp.xml"), crate::xmlio::write_ytyp(&ytyp));
        (vec![format!("Atelier : {n} bâtiment(s) avec leur version lointaine, {ncol} collision(s) ; vitres éclairées la nuit dans {nlit} modèle(s).")], "data_file 'DLC_ITYP_REQUEST' 'stream/dw_house_types.ytyp'\n".into())
    }
}

/// Vitres éclairées la nuit : le matériau des vitres « allumées » devient celui que le jeu fait briller la nuit.
/// Rend vrai si le modèle a été modifié.
pub fn night_windows(path: &std::path::Path, txd: &str) -> Result<bool, String> {
    let mut any = false;
    for st in 0..3u8 {
        any |= night_window_one(path, txd, glass_key(st, true))?;
    }
    Ok(any)
}

fn night_window_one(path: &std::path::Path, txd: &str, key: u64) -> Result<bool, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let tn = crate::sollumz::shared_tex_name(txd, key);
    let Some(t) = s.find(&format!("<Name>{tn}</Name>")) else { return Ok(false) };
    // le matériau (Item) qui contient cette texture
    let Some(item_start) = s[..t].rfind("   <Item>\n    <Name>") else { return Ok(false) };
    let item_end = s[t..].find("   </Item>").map(|e| t + e).unwrap_or(t);
    let mut item = s[item_start..item_end].to_string();
    item = item.replacen(&format!("<Name>{}</Name>\n    <FileName>{}.sps</FileName>", "default", "default"), &crate::atelier::night_shader_head(), 1);
    if let Some(p) = item.rfind("    </Parameters>") {
        item.insert_str(p, &night_shader_params());
    }
    let out = format!("{}{}{}", &s[..item_start], item, &s[item_end..]);
    std::fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(true)
}

/// En-tête du matériau des vitres éclairées la nuit (nom et fichier du shader).
pub fn night_shader_head() -> String {
    "<Name>emissivenight</Name>\n    <FileName>emissivenight.sps</FileName>".into()
}

/// Paramètres en plus du matériau des vitres éclairées la nuit.
pub fn night_shader_params() -> String {
    // comme les fenêtres du jeu (dt1_05_office_cheap.ydr, matériau emissivenight) ; intensité **estimée**, plus
    // faible que leurs 16 (fausses fenêtres de bureaux)
    concat!(
        "     <Item name=\"matMaterialColorScale\" type=\"Vector\" x=\"1\" y=\"0\" z=\"0\" w=\"1\" />\n",
        "     <Item name=\"HardAlphaBlend\" type=\"Vector\" x=\"1\" y=\"0\" z=\"0\" w=\"0\" />\n",
        "     <Item name=\"useTessellation\" type=\"Vector\" x=\"0\" y=\"0\" z=\"0\" w=\"0\" />\n",
        "     <Item name=\"emissiveMultiplier\" type=\"Vector\" x=\"8\" y=\"0\" z=\"0\" w=\"0\" />\n"
    )
    .into()
}

/// Escalier d'un bâtiment (le même à chaque étage), dans le repère d'un mur : début du mur, direction le long du mur,
/// direction vers l'intérieur ; trémie (de, à, le long du mur), largeur (de, à, depuis le mur), début des marches,
/// nombre et profondeur des marches.
#[derive(Clone, Copy, Debug)]
pub struct Stair {
    pub a: V3,
    pub dir: V3,
    pub inw: V3,
    pub hole: (f32, f32),
    pub w: (f32, f32),
    pub start: f32,
    pub n: usize,
    pub run: f32,
}

impl Stair {
    /// Point (le long du mur, depuis le mur, hauteur), repère du bâtiment.
    pub fn at(&self, s: f32, w: f32, z: f32) -> V3 {
        let p = self.a + self.dir * s + self.inw * w;
        v3(p.x, p.y, z)
    }

    /// Coins de la trémie à la hauteur z.
    pub fn hole_corners(&self, z: f32) -> [V3; 4] {
        [self.at(self.hole.0, self.w.0, z), self.at(self.hole.1, self.w.0, z), self.at(self.hole.1, self.w.1, z), self.at(self.hole.0, self.w.1, z)]
    }
}

/// Escalier entre deux étages d'un bâtiment de l'atelier : largeur, et trémie au-dessus du haut de l'escalier
/// (part de sa longueur, au plus STAIR_HOLE_MAX m) — **estimé**.
const STAIR_W: f32 = 1.2;
const STAIR_HOLE_PART: f32 = 0.6;
const STAIR_HOLE_MAX: f32 = 3.5;

impl DwApp {
    /// « Transformer en intérieur » pour un bâtiment de l'atelier : une pièce par étage (boîte intérieure du plan),
    /// chaque fenêtre et la porte en portail vers l'extérieur (on voit dehors), une trémie entre deux étages (la coque
    /// y met l'escalier), une vraie porte du jeu à l'entrée, puis meubles, lumières et ambiance comme pour un
    /// bâtiment du jeu. L'intérieur suit le bâtiment quand on le déplace.
    pub fn house_to_interior(&mut self, hi: usize) -> String {
        let h = self.proj.houses[hi].clone();
        let Some(ent) = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.archetype.to_lowercase() == h.name).cloned() else {
            return "Pose d'abord ce bâtiment dans le projet.".into();
        };
        let (bmn, bmx) = h.bbox();
        let rect_area = (bmx.x - bmn.x) * (bmx.y - bmn.y);
        let not_rect = signed_area(&h.foot) < rect_area * 0.95;
        // pièces : une par étage
        let mut rooms: Vec<(String, V3, V3)> = Vec::new();
        for f in 0..h.floors.max(1) {
            let (lo, hi2) = h.floor_room(f);
            rooms.push((format!("etage{}", f + 1), lo, hi2));
        }
        // portails : fenêtres et porte (vers l'extérieur), sur la face intérieure des murs
        let mut portals: Vec<(u32, u32, [V3; 4])> = Vec::new();
        let edges = h.edges();

        for (e, &(a, _, dir, out, _)) in edges.iter().enumerate() {
            let at = |s: f32, zz: f32| {
                let p = a + dir * s - out * WALL_T;
                v3(p.x, p.y, zz)
            };
            for f in 0..h.floors.max(1) {
                for ho in h.holes(e, f) {
                    let c = [at(ho.s0, ho.z0), at(ho.s0, ho.z1), at(ho.s1, ho.z1), at(ho.s1, ho.z0)];
                    portals.push((f as u32 + 1, 0, c));
                }
            }
        }
        // trémies entre étages : le long du mur du bas du plan (y le plus petit), au-dessus du haut de l'escalier
        let mut n_stairs = 0;
        if let Some(st) = h.stair() {
            for f in 0..h.floors.max(1).saturating_sub(1) {
                let z = rooms[f as usize].2.z;
                portals.push((f as u32 + 1, f as u32 + 2, st.hole_corners(z)));
                n_stairs += 1;
            }
        }
        // l'intérieur, posé exactement sur le bâtiment
        self.push_undo();
        let mut name = format!("{}_int", h.name);
        let mut k = 2;
        while self.proj.find_mlo(&name).is_some() {
            name = format!("{}_int{k}", h.name);
            k += 1;
        }
        let mut mrooms = vec![crate::model::Room::new("limbo", V3::ZERO, V3::ZERO)];
        for (n, lo, hi2) in &rooms {
            mrooms.push(crate::model::Room::new(n, *lo, *hi2));
        }
        let mportals: Vec<crate::model::Portal> = portals.iter().map(|(f, t, c)| crate::model::Portal::new(*f, *t, *c)).collect();
        let exits = mportals.iter().filter(|p| p.room_to == 0 || p.room_from == 0).count() as u32;
        let mlo = crate::model::MloArchetype { name: name.clone(), lod_dist: 200.0, flags: 0, mlo_flags: 0, entities: Vec::new(), rooms: mrooms, portals: mportals, entity_sets: Vec::new() };
        // jamais dans le .ytyp d'une ressource MLO importée (recopié tel quel à l'export : l'intérieur n'y serait pas,
        // ni sa porte, ses meubles et ses lumières — vu dans un projet de la personne, 13 nov. 2026)
        let ti = self.own_ytyp();
        self.proj.ytyps[ti].mlos.push(mlo);
        let mi = self.proj.ytyps[ti].mlos.len() - 1;
        self.cur_mlo = Some((ti, mi));
        let yi = self.ensure_target_ymap();
        let mut inst = Entity::new(&name, ent.pos);
        inst.rot = ent.rot;
        inst.mlo = Some(crate::model::MloInstData { num_exit_portals: exits, ..Default::default() });
        inst.lod_dist = 200.0;
        self.proj.mlo_links.retain(|l| l.1 != ent.id);
        self.proj.mlo_links.push((inst.id, ent.id));
        self.proj.mlo_link_yaw.push((inst.id, 0.0));
        self.proj.ymaps[yi].entities.push(inst);
        // ambiance, meubles, lumières
        let tcs = self.game_timecycles();
        let tc = tcs.iter().find(|t| t.to_lowercase().starts_with("int") && ["apart", "house", "living"].iter().any(|k| t.to_lowercase().contains(k))).cloned().unwrap_or_default();
        let light = self.find_game_prop(&["light"], &["ceil", "_ceiling", "v_res_", "v_serv_ct"], v3(0.1, 0.1, 0.02), v3(1.6, 1.6, 0.8), v3(0.6, 0.6, 0.15));
        let mut n_lights = 0;
        for (ri, (_, lo, hi2)) in rooms.iter().enumerate() {
            let ri = ri + 1;
            let area = (hi2.x - lo.x) * (hi2.y - lo.y);
            let kind = if h.shop && ri == 1 { "magasin" } else { crate::convert::guess_room_type(area / 2.0) };
            {
                let r = &mut self.proj.ytyps[ti].mlos[mi].rooms[ri];
                r.timecycle = tc.clone();
            }
            self.furnish_rooms_inner(kind, Some(ri));
            if let Some((ln, _, lmx)) = &light {
                let n = ((area / 25.0).ceil() as usize).max(1);
                let cols = (n as f32).sqrt().ceil() as usize;
                let rows = n.div_ceil(cols);
                for i in 0..n {
                    let (cx, cy) = (i % cols, i / cols);
                    let x = lo.x + (hi2.x - lo.x) * (cx as f32 + 0.5) / cols as f32;
                    let y = lo.y + (hi2.y - lo.y) * (cy as f32 + 0.5) / rows as f32;
                    let mut e = Entity::new(ln, v3(x, y, hi2.z - lmx.z.max(0.0) - 0.02));
                    e.flags = crate::mech::MLO_ENTITY_FLAGS;
                    e.room = ri;
                    self.proj.ytyps[ti].mlos[mi].entities.push(e);
                    n_lights += 1;
                }
            }
        }
        // une vraie porte du jeu par porte du bâtiment, ajustée à l'ouverture exactement comme dehors (même repère :
        // l'intérieur est posé sur le bâtiment) ; les portes posées dehors sont retirées
        let mut placed_doors: Vec<String> = Vec::new();
        let mut hh = h.clone();
        for i in 0..hh.doors().len() {
            let mut d = hh.doors()[i].clone();
            let found = if d.model.is_empty() { self.exterior_door() } else { self.game.world.as_ref().and_then(|w| w.archetypes.get(&crate::gta::crypto::joaat(&d.model)).map(|a| (d.model.clone(), a.bb_min, a.bb_max))) };
            let Some((dn, dmn, dmx)) = found else { continue };
            d.bbox = Some(([dmn.x, dmn.y, dmn.z], [dmx.x, dmx.y, dmx.z]));
            if let Some(old) = d.prop.take() {
                self.proj.delete_ids(&std::iter::once(old).collect());
            }
            hh.put_door(i, d);
            let Some((local, ang, sxy, sz)) = hh.door_fit(i) else { continue };
            let mut e = Entity::new(&dn, local);
            e.rot = Quat::from_z(ang);
            e.scale_xy = sxy;
            e.scale_z = sz;
            e.flags = crate::mech::MLO_ENTITY_FLAGS;
            e.room = 1;
            self.proj.ytyps[ti].mlos[mi].entities.push(e);
            placed_doors.push(dn);
        }
        let door_txt = if placed_doors.is_empty() { "aucune porte du jeu trouvée (jeu non chargé ?)".to_string() } else { format!("porte(s) {}", placed_doors.join(", ")) };
        hh.interior = Some(name.clone());
        self.proj.houses[hi] = hh;
        self.opts.show_rooms = true;
        self.atelier.sig = 0;
        let mut msg = format!(
            "« {name} » : {} pièce(s) (une par étage), {} fenêtre(s) et porte(s) en portails vers l'extérieur, {n_stairs} escalier(s) avec trémie, {door_txt}, {n_lights} lumière(s). Chaque pièce a sa coque et sa collision ; l'intérieur suit le bâtiment.",
            rooms.len(),
            portals.len() - n_stairs
        );
        if not_rect {
            msg += " Attention : l'emprise n'est pas un rectangle, chaque pièce prend le rectangle qui l'entoure (corrige-les dans l'onglet Intérieur).";
        }
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside_poly(foot: &[[f32; 2]], x: f32, y: f32) -> bool {
        let n = foot.len();
        let mut c = false;
        for i in 0..n {
            let (a, b) = (foot[i], foot[(i + 1) % n]);
            if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
                c = !c;
            }
        }
        c
    }

    /// Emprise de forme libre (comme tracée à la main) : rien de l'intérieur ne dépasse des murs.
    #[test]
    fn planchers_et_escalier_dans_les_murs() {
        let foot = vec![[-12.0, -4.0], [6.0, -6.0], [13.0, 1.0], [10.0, 7.0], [-2.0, 5.0], [-11.0, 6.0]];
        let mut h = House::new("dw_bat_t", foot.clone());
        h.floors = 3;
        let st = h.stair().expect("un escalier trouve sa place");
        for q in st.hole_corners(0.0) {
            assert!(inside_poly(&foot, q.x, q.y), "trémie hors des murs : {q:?}");
        }
        let (pos, _, parts) = h.mesh(false);
        for (k, idx, _) in &parts {
            if *k != K_FLOOR {
                continue;
            }
            for &i in idx {
                let p = pos[i as usize];
                // centre du triangle un peu rentré : à l'intérieur de l'emprise (ou sur son bord)
                let near_edge = h.edges().iter().any(|&(a, _, dir, _, len)| {
                    let s = (p - a).dot(dir).clamp(0.0, len);
                    let q = a + dir * s;
                    v3(p.x - q.x, p.y - q.y, 0.0).len() < 0.01
                });
                assert!(near_edge || inside_poly(&foot, p.x, p.y), "plancher hors des murs : {p:?}");
            }
        }
    }
}
