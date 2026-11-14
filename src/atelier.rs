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
/// Toile d'auvent (rayures claires, teintée), métal (garde-corps, gouttières), pierre du soubassement.
pub const K_AWNING: u64 = 0xD7E3_0000_0000_0500;
pub const K_METAL: u64 = 0xD7E3_0000_0000_0501;
pub const K_PLINTH: u64 = 0xD7E3_0000_0000_0502;
/// Surface d'un miroir (coque modelable) : à l'export, matériau « mirror_default » du jeu.
pub const K_MIRROR: u64 = 0xD7E3_0000_0000_0503;

/// Texture DragoWalker `base` teinte de la couleur `rgb` (le motif garde ses nuances).
pub fn tint_key(base: u64, rgb: [u8; 3]) -> u64 {
    // les 32 bits du bas nomment la texture à l'export : empreinte FNV de la base et de la couleur (deux couleurs ne s'y
    // confondent plus, ni avec une texture non teinte) ; la base et la couleur sont retrouvées par le registre
    let mut h: u32 = 0x811C_9DC5;
    for b in (base & 0xFFFF).to_le_bytes().iter().take(2).chain(rgb.iter()) {
        h = (h ^ *b as u32).wrapping_mul(16777619);
    }
    let h = h | 0x8000_0000;
    let key = 0xD7E5_0000_0000_0000 | (base & 0xFFFF) << 32 | h as u64;
    if let Ok(mut r) = TINTS.write() {
        r.insert(key, rgb);
    }
    key
}

static TINTS: std::sync::RwLock<std::collections::BTreeMap<u64, [u8; 3]>> = std::sync::RwLock::new(std::collections::BTreeMap::new());

/// Texture de base et couleur d'une clé teintée.
pub fn tint_of(key: u64) -> Option<(u64, [u8; 3])> {
    if key >> 48 != 0xD7E5 {
        return None;
    }
    let rgb = *TINTS.read().ok()?.get(&key)?;
    Some((0xD7E3_0000_0000_0000 | (key >> 32 & 0xFFFF), rgb))
}

/// Recolore une texture : luminance de chaque point rapportée à la moyenne, fois la couleur.
fn tinted(t: &TexData, rgb: [u8; 3]) -> TexData {
    let d = &t.levels[0];
    let lum = |i: usize| 0.3 * d[i] as f32 + 0.59 * d[i + 1] as f32 + 0.11 * d[i + 2] as f32;
    let n = d.len() / 4;
    let avg = ((0..n).map(|k| lum(k * 4)).sum::<f32>() / n.max(1) as f32).max(1.0);
    let mut out = Vec::with_capacity(d.len());
    for k in 0..n {
        let f = lum(k * 4) / avg;
        for c in 0..3 {
            out.push((rgb[c] as f32 * f).clamp(0.0, 255.0) as u8);
        }
        out.push(255);
    }
    crate::roads::with_mips(t.w, t.h, out)
}

/// Couleurs proposées pour les façades et les auvents : (nom, couleur).
pub const COLORS: [(&str, [u8; 3]); 10] = [
    ("Blanc cassé", [232, 226, 210]),
    ("Sable", [214, 190, 146]),
    ("Ocre", [196, 146, 74]),
    ("Saumon", [212, 140, 118]),
    ("Brique", [160, 72, 52]),
    ("Vert sauge", [150, 168, 132]),
    ("Bleu pâle", [150, 176, 196]),
    ("Gris", [150, 150, 148]),
    ("Anthracite", [70, 72, 76]),
    ("Rouge", [150, 40, 36]),
];

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
/// Rampes des escaliers et perrons : collision marquée « escalier » du jeu (à l'export seulement).
pub const C_STAIRS: u8 = 2;
/// Pas de collision (salissures, marches : la rampe de l'escalier les remplace).
pub const C_NONE: u8 = 255;

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
    /// porte d'entrée fermée à clé
    #[serde(default)]
    pub door_locked: bool,
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
    /// escalier : genre (STAIR_KINDS), centre choisi d'un clic (sinon placé tout seul), quarts de tour
    #[serde(default)]
    pub stair_kind: u8,
    #[serde(default)]
    pub stair_at: Option<[f32; 2]>,
    #[serde(default)]
    pub stair_rot: u8,
    /// escalier jusqu'au toit-terrasse (toit plat), avec un édicule de sortie
    #[serde(default)]
    pub roof_access: bool,
    /// balcons, auvents, gouttières, climatiseurs
    #[serde(default)]
    pub details: Details,
    /// façade et couleur de chaque étage (vide ou absent : comme le bâtiment)
    #[serde(default)]
    pub looks: Vec<FloorLook>,
    /// couleur de la façade (textures DragoWalker seulement)
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// rez-de-chaussée surélevé (m) : soubassement, et perron à marches devant chaque porte
    #[serde(default)]
    pub lift: f32,
    /// objets du jeu posés sur la façade (climatiseurs), qui suivent le bâtiment
    #[serde(default)]
    pub attached: Vec<u64>,
    /// portes et climatiseurs des autres placements du même bâtiment (copies)
    #[serde(default)]
    pub copies: Vec<CopyProps>,
    /// bâtiment de décoration : on n'y entre pas (portes fermées à clé, pas de planchers, d'escalier ni d'intérieur)
    #[serde(default)]
    pub decor: bool,
}

/// Détails de façade.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Details {
    /// balcons aux étages : 0 aucun, 1 côté de l'entrée, 2 tous les côtés
    #[serde(default)]
    pub balconies: u8,
    /// auvents au-dessus des portes et des vitrines, et leur couleur
    #[serde(default)]
    pub awnings: bool,
    #[serde(default = "awning_default")]
    pub awning_color: [u8; 3],
    /// gouttières et descentes aux angles
    #[serde(default)]
    pub gutters: bool,
    /// part des fenêtres avec un climatiseur du jeu sous l'appui (0..1)
    #[serde(default)]
    pub ac: f32,
}

fn awning_default() -> [u8; 3] {
    [150, 40, 36]
}

impl Default for Details {
    fn default() -> Self {
        Details { balconies: 0, awnings: false, awning_color: awning_default(), gutters: false, ac: 0.0 }
    }
}

/// Aspect d'un étage : façade DragoWalker (FACADES) ou du jeu (1 + numéro dans GAME_TEX), couleur.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FloorLook {
    #[serde(default)]
    pub facade: Option<u8>,
    #[serde(default)]
    pub game: u8,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}

/// Point du repère d'un bâtiment dans le monde, pour un placement (position, rotation, échelle de l'objet).
pub fn place_at(b: &Entity, local: V3) -> V3 {
    b.pos + b.rot.rotate(v3(local.x * b.scale_xy, local.y * b.scale_xy, local.z * b.scale_z))
}

/// Objets du jeu d'un placement de plus d'un bâtiment (copie) : ses portes et ses climatiseurs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CopyProps {
    /// objet du bâtiment (placement)
    pub placement: u64,
    #[serde(default)]
    pub doors: Vec<u64>,
    #[serde(default)]
    pub ac: Vec<u64>,
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
    /// fermée à clé en jeu (elle ne s'ouvre pas)
    #[serde(default)]
    pub locked: bool,
}

/// Portes du jeu proposées (relevées par la sonde DW_ARCH_GREP : portes posées dehors par le jeu, puis portes
/// d'intérieur v_ilev_*, à garder pour les intérieurs) : (modèle, description).
pub const DOOR_MODELS: &[(&str, &str)] = &[
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
    ("prop_gar_door_01", "porte de garage basculante (3 × 2,5 m)"),
    ("prop_gar_door_02", "porte de garage basculante, autre teinte (3 × 2,5 m)"),
    ("prop_gar_door_05", "porte de garage (3,4 × 3,3 m)"),
    ("prop_com_gar_door_01", "porte de garage commerciale (4 × 4 m)"),
    ("prop_gar_door_04", "grande porte de garage (4,8 × 4,7 m)"),
];

/// Portes de garage du jeu (relevées par la sonde DW_ARCH_GREP), de la plus petite à la plus grande.
pub const GARAGE_DOORS: [&str; 5] = ["prop_gar_door_01", "prop_gar_door_02", "prop_gar_door_05", "prop_com_gar_door_01", "prop_gar_door_04"];

/// Climatiseurs du jeu : muraux (sous une fenêtre) et de toit (relevés par la sonde, les plus posés par le jeu).
pub const AC_WALL: [&str; 2] = ["prop_aircon_s_05a", "prop_aircon_s_02a"];
pub const AC_ROOF: &str = "prop_aircon_m_04";

/// Modèles de bâtiment prêts à l'emploi : (nom, largeur, profondeur).
pub const PRESETS: [(&str, f32, f32); 4] = [("Maison de banlieue", 11.0, 8.0), ("Petit immeuble", 14.0, 11.0), ("Commerce", 12.0, 10.0), ("Entrepôt", 30.0, 20.0)];

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
pub(crate) struct Mesher {
    pub(crate) pos: Vec<V3>,
    pub(crate) uv: Vec<[f32; 2]>,
    pub(crate) parts: Vec<(u64, Vec<u32>, u8)>,
}

impl Mesher {
    pub(crate) fn part(&mut self, key: u64, coll: u8) -> usize {
        if let Some(i) = self.parts.iter().position(|p| p.0 == key && p.2 == coll) {
            return i;
        }
        self.parts.push((key, Vec::new(), coll));
        self.parts.len() - 1
    }

    /// Quadrilatère tourné vers `want` (ordre inversé s'il regarde de l'autre côté), texture en mètres / 3.
    pub(crate) fn quad(&mut self, q: [V3; 4], want: V3, key: u64, coll: u8) {
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
    pub(crate) fn tri(&mut self, t: [V3; 3], want: V3, key: u64, coll: u8) {
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
pub(crate) fn clip_half(p: &[[f32; 2]], f: &dyn Fn([f32; 2]) -> f32) -> Vec<[f32; 2]> {
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
        House { name: name.to_string(), foot, floors: 2, floor_h: 3.2, roof: 0, facade: 1, win_w: 1.2, win_h: 1.5, win_step: 3.0, shop: false, door_edge: 0, door_t: 0.5, lit: 0.6, interior: None, door_prop: None, door_locked: false, door_box: None, game_facade: 0, game_roof: 0, grime: 0.0, grime_kind: 10, door_model: String::new(), more_doors: Vec::new(), floor_kind: 0, walls: Vec::new(), win_style: 0, win_off: Vec::new(), win_add: Vec::new(), stair_kind: 0, stair_at: None, stair_rot: 0, roof_access: false, details: Details::default(), looks: Vec::new(), color: None, lift: 0.0, attached: Vec::new(), copies: Vec::new(), decor: false }
    }

    pub fn height(&self) -> f32 {
        self.floors.max(1) as f32 * self.floor_h
    }

    /// Texture de la façade : celle du jeu choisie, sinon celle de DragoWalker.
    pub fn facade_key(&self) -> u64 {
        match self.game_facade {
            0 => self.color.map(|c| tint_key(k_facade(self.facade), c)).unwrap_or(k_facade(self.facade)),
            g => game_key(g as usize - 1),
        }
    }

    /// Texture de la façade de l'étage `f` : celle de l'étage si elle est choisie, sinon celle du bâtiment ; teinte
    /// (de l'étage, sinon du bâtiment) sur les textures DragoWalker.
    pub fn facade_key_at(&self, f: u8) -> u64 {
        let look = self.looks.get(f as usize).copied().unwrap_or_default();
        if look.game > 0 {
            return game_key(look.game as usize - 1);
        }
        let color = look.color.or(self.color);
        let base = match look.facade {
            Some(i) => k_facade(i),
            None if self.game_facade > 0 => return game_key(self.game_facade as usize - 1),
            None => k_facade(self.facade),
        };
        match color {
            Some(c) => tint_key(base, c),
            None => base,
        }
    }

    /// Façade de la version lointaine : celle du bâtiment si c'est une texture DragoWalker, sinon la plus proche de
    /// DragoWalker (le dictionnaire du bâtiment n'a pas les textures du jeu).
    pub fn lod_facade_key(&self) -> u64 {
        let base = k_facade(self.facade);
        match self.color {
            Some(c) => tint_key(base, c),
            None => base,
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
            // balcon : la fenêtre devient une porte-fenêtre
            let french = kind == 0 && self.balcony_on(e, f);
            for k in 0..n {
                let c = start + k as f32 * self.win_step;
                let h = if french {
                    Hole { s0: c - ww * 0.5, s1: c + ww * 0.5, z0: z0 + 0.05, z1: z0 + (self.floor_h - 0.5).min(2.3), kind, style: self.win_style }
                } else {
                    Hole { s0: c - ww * 0.5, s1: c + ww * 0.5, z0: z0 + sill, z1: z0 + sill + wh, kind, style: self.win_style }
                };
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
        // version lointaine : textures DragoWalker seulement (son dictionnaire ne contient pas celles du jeu)
        let fk = if lod { self.lod_facade_key() } else { self.facade_key() };
        let tk = if lod { K_ROOF_TILE } else { self.tile_key() };
        let fk_top = if lod { fk } else { self.facade_key_at(self.floors.max(1) - 1) };
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
                let fk = self.facade_key_at(f);
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
                            // la même vitre vue de dedans (le jeu ne dessine une face que d'un côté) ; pas pour un
                            // bâtiment de décoration (on n'y entre pas)
                            let qi = [at(ho.s0, GLASS_INSET + 0.005, ho.z0), at(ho.s1, GLASS_INSET + 0.005, ho.z0), at(ho.s1, GLASS_INSET + 0.005, ho.z1), at(ho.s0, GLASS_INSET + 0.005, ho.z1)];
                            if !self.decor {
                                m.quad_uv(qi, uv, out * -1.0, glass_key(ho.style, false), C_GLASS);
                            }
                        }
                    }
                }
                rect(&mut m, cur, len, z0, z1);
            }
            // acrotère d'un toit plat : face extérieure, face intérieure, dessus
            let fk = self.facade_key_at(self.floors.max(1) - 1);
            if self.roof == 0 {
                m.quad([at(0.0, 0.0, h), at(len, 0.0, h), at(len, 0.0, h + PARAPET), at(0.0, 0.0, h + PARAPET)], out, fk, C_WALL);
                m.quad([at(0.0, WALL_T, h), at(len, WALL_T, h), at(len, WALL_T, h + PARAPET), at(0.0, WALL_T, h + PARAPET)], out * -1.0, fk, C_WALL);
                m.quad([at(0.0, 0.0, h + PARAPET), at(len, 0.0, h + PARAPET), at(len, WALL_T, h + PARAPET), at(0.0, WALL_T, h + PARAPET)], up, fk, C_WALL);
            }
        }
        // angles rentrants : les deux murs s'arrêtent au coin, un carré d'épaisseur de mur restait vide sur toute la
        // hauteur (on voyait dehors) : un poteau le comble
        if !lod {
            let ne = edges.len();
            for k in 0..ne {
                let (_, _, d1, o1, _) = edges[(k + ne - 1) % ne];
                let (v, _, d2, o2, _) = edges[k];
                if d1.cross(d2).z >= -1e-3 {
                    continue; // angle saillant ou plat
                }
                let (i1, i2) = (o1 * -1.0, o2 * -1.0);
                let ztop = if self.roof == 0 { h + PARAPET } else { h };
                let p = |t: f32, u: f32, zz: f32| { let q = v + i1 * t + i2 * u; v3(q.x, q.y, zz) };
                for (a, b, n, key) in [((WALL_T, 0.0), (WALL_T, WALL_T), i1, K_INNER), ((0.0, WALL_T), (WALL_T, WALL_T), i2, K_INNER), ((0.0, 0.0), (0.0, WALL_T), i1 * -1.0, fk_top), ((0.0, 0.0), (WALL_T, 0.0), i2 * -1.0, fk_top)] {
                    m.quad([p(a.0, a.1, 0.0), p(b.0, b.1, 0.0), p(b.0, b.1, ztop), p(a.0, a.1, ztop)], n, key, C_WALL);
                }
                m.quad([p(0.0, 0.0, ztop), p(WALL_T, 0.0, ztop), p(WALL_T, WALL_T, ztop), p(0.0, WALL_T, ztop)], up, fk_top, C_WALL);
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
        // « Transformer en intérieur ») : la maison se visite même sans intérieur ; rien de tout cela pour un bâtiment
        // de décoration (moins de triangles, on n'y entre pas)
        if !lod && !self.decor {
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
                    for pc in House::without_hole(stair, &tri) {
                        for k in 1..pc.len() - 1 {
                            let (p0, p1, p2) = (pc[0], pc[k], pc[k + 1]);
                            m.tri([v3(p0[0], p0[1], z), v3(p1[0], p1[1], z), v3(p2[0], p2[1], z)], up, self.floor_key(), C_WALL);
                            m.tri([v3(p0[0], p0[1], z - SLAB_T), v3(p1[0], p1[1], z - SLAB_T), v3(p2[0], p2[1], z - SLAB_T)], up * -1.0, K_INNER, C_WALL);
                        }
                    }
                }
                let Some(st) = stair else { continue };
                // bords de la trémie (tournés vers son milieu)
                for (p, q, n) in st.hole_edges() {
                    m.quad([z_at(p, z - SLAB_T), z_at(q, z - SLAB_T), z_at(q, z), z_at(p, z)], n, K_INNER, C_WALL);
                }
                self.mesh_hole_extras(&mut m, &st, z, z - SLAB_T);
            }
            // marches de chaque volée (sans collision : la rampe de stair_ramps les remplace, le personnage montait
            // mal les marches une à une)
            if let Some(st) = stair {
                let steps = st.steps();
                let n = steps.len();
                for (z0, z) in self.stair_levels() {
                    let rise = (z - z0) / n as f32;
                    for (i, (q, _)) in steps.iter().enumerate() {
                        let (zl, zh) = (z0 + i as f32 * rise, z0 + (i + 1) as f32 * rise);
                        let p = |k: usize, zz: f32| st.at(q[k][0], q[k][1], zz);
                        let mid = (p(0, zh) + p(1, zh) + p(2, zh) + p(3, zh)) * 0.25;
                        let fwd = (p(3, zh) + p(2, zh)) * 0.5 - (p(0, zh) + p(1, zh)) * 0.5;
                        m.quad([p(0, zl), p(1, zl), p(1, zh), p(0, zh)], fwd * -1.0, self.floor_key(), C_NONE);
                        m.quad([p(0, zh), p(1, zh), p(2, zh), p(3, zh)], up, self.floor_key(), C_NONE);
                        // joues : un escalier plein jusqu'au sol de la volée
                        for (a, b) in [(1usize, 2usize), (3, 0)] {
                            let side = (p(a, zh) + p(b, zh)) * 0.5 - mid;
                            m.quad([p(a, z0), p(b, z0), p(b, zh), p(a, zh)], side, K_INNER, C_WALL);
                        }
                    }
                    if st.kind == 2 {
                        // fût central
                        let c = st.at(st.start + SPIRAL_R, st.w.0 + SPIRAL_R, 0.0);
                        for k in 0..8 {
                            let (a0, a1) = (k as f32 * std::f32::consts::TAU / 8.0, (k + 1) as f32 * std::f32::consts::TAU / 8.0);
                            let (pa, pb) = (c + v3(a0.cos(), a0.sin(), 0.0) * SPIRAL_CORE, c + v3(a1.cos(), a1.sin(), 0.0) * SPIRAL_CORE);
                            m.quad([z_at(pa, z0), z_at(pb, z0), z_at(pb, z + 1.0), z_at(pa, z + 1.0)], (pa + pb) * 0.5 - c, K_INNER, C_WALL);
                        }
                    }
                }
            }
        }
        // cloisons : du sol au plafond de leur étage, avec un passage au milieu si demandé
        if !lod && !self.decor {
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
        let roof_stair = if self.roof_access && self.roof == 0 && !lod { self.stair() } else { None };
        if !lod && !self.decor {
            for t in triangulate(&self.foot) {
                let tri: Vec<[f32; 2]> = t.iter().map(|&k| self.foot[k]).collect();
                for pc in House::without_hole(roof_stair, &tri) {
                    for k in 1..pc.len() - 1 {
                        m.tri([v3(pc[0][0], pc[0][1], h - 0.02), v3(pc[k][0], pc[k][1], h - 0.02), v3(pc[k + 1][0], pc[k + 1][1], h - 0.02)], up * -1.0, K_INNER, C_WALL);
                    }
                }
            }
            if let Some(st) = roof_stair {
                self.roof_hut(&mut m, &st, fk_top);
                let za = |p: V3, zz: f32| v3(p.x, p.y, zz);
                for (p, q, n) in st.hole_edges() {
                    m.quad([za(p, h - 0.02), za(q, h - 0.02), za(q, h + ROOF_TOP), za(p, h + ROOF_TOP)], n, K_INNER, C_WALL);
                }
                self.mesh_hole_extras(&mut m, &st, h + ROOF_TOP, h - 0.02);
            }
        }
        // toit
        let p3 = |p: [f32; 2], zz: f32| v3(p[0], p[1], zz);
        match self.roof {
            0 => {
                for t in triangulate(&self.foot) {
                    let tri: Vec<[f32; 2]> = t.iter().map(|&k| self.foot[k]).collect();
                    for pc in House::without_hole(roof_stair, &tri) {
                        for k in 1..pc.len() - 1 {
                            m.tri([p3(pc[0], h + ROOF_TOP), p3(pc[k], h + ROOF_TOP), p3(pc[k + 1], h + ROOF_TOP)], up, K_ROOF_FLAT, C_WALL);
                        }
                    }
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
                        m.tri([p3(half[t[0]], rz(half[t[0]])), p3(half[t[1]], rz(half[t[1]])), p3(half[t[2]], rz(half[t[2]]))], up, tk, C_WALL);
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
                        m.quad(q, out, fk_top, C_WALL);
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
                    m.quad([c00, c10, r2, r1], up, tk, C_WALL);
                    m.quad([c11, c01, r1, r2], up, tk, C_WALL);
                    m.tri([c10, c11, r2], up, tk, C_WALL);
                    m.tri([c01, c00, r1], up, tk, C_WALL);
                } else {
                    let n = self.foot.len() as f32;
                    let g = v3(self.foot.iter().map(|p| p[0]).sum::<f32>() / n, self.foot.iter().map(|p| p[1]).sum::<f32>() / n, h + ROOF_SLOPE * hu.min(hv));
                    for &(a, b, _, _, _) in &edges {
                        m.tri([z(a, h), z(b, h), g], up, tk, C_WALL);
                    }
                }
            }
        }
        if !lod {
            self.mesh_details(&mut m);
        }
        // rez-de-chaussée surélevé : tout monte, soubassement dessous, perron devant chaque porte
        if self.lift > 0.01 {
            for p in m.pos.iter_mut() {
                p.z += self.lift;
            }
            for &(a, _, dir, out, len) in &edges {
                let at = |s: f32, zz: f32| z(a + dir * s, zz);
                m.quad([at(0.0, 0.0), at(len, 0.0), at(len, self.lift), at(0.0, self.lift)], out, K_PLINTH, C_WALL);
            }
            if !lod {
                for q in self.stoops() {
                    let (bot, top) = (q[0].z, q[2].z);
                    let _ = bot;
                    m.quad(q, up, K_PLINTH, C_NONE);
                    let _ = top;
                }
                for q in self.garage_ramps() {
                    m.quad(q, up, K_PLINTH, C_WALL);
                    let g = |p: V3| v3(p.x, p.y, 0.0);
                    let side = q[1] - q[0];
                    m.tri([g(q[1]), g(q[2]), q[2]], side, K_PLINTH, C_WALL);
                    m.tri([g(q[0]), g(q[3]), q[3]], side * -1.0, K_PLINTH, C_WALL);
                }
                for (q, side, riser) in self.stoop_sides() {
                    // contremarches sans collision : la rampe du perron la remplace
                    m.quad(q, side, K_PLINTH, if riser { C_NONE } else { C_WALL });
                }
            }
        }
        (m.pos, m.uv, m.parts)
    }

    /// Climatiseurs : (modèle, point d'accroche, direction vers l'extérieur, au mur ?) ; au mur, le point est sur la
    /// façade au haut de l'appareil (sous l'appui de la fenêtre) ; sur le toit, au pied de l'appareil. Repère du
    /// bâtiment, rez-de-chaussée surélevé compris. Toujours les mêmes pour un bâtiment donné.
    pub fn ac_spots(&self) -> Vec<(&'static str, V3, V3, bool)> {
        let mut v = Vec::new();
        if self.details.ac <= 0.0 {
            return v;
        }
        let mut seed: u32 = self.name.bytes().fold(0x9E37_79B9u32, |h, b| (h ^ b as u32).wrapping_mul(16777619)) | 1;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % 10_000) as f32 / 10_000.0
        };
        for (e, &(a, _, dir, out, _)) in self.edges().iter().enumerate() {
            for f in 0..self.floors.max(1) {
                for ho in self.holes(e, f) {
                    let r = rnd();
                    if ho.kind != 0 || self.balcony_on(e, f) || r >= self.details.ac || ho.z0 - f as f32 * self.floor_h < 0.9 {
                        continue;
                    }
                    let p = a + dir * ((ho.s0 + ho.s1) * 0.5);
                    v.push((AC_WALL[(r * 1000.0) as usize % AC_WALL.len()], v3(p.x, p.y, self.lift + ho.z0 - 0.12), out, true));
                }
            }
        }
        // toit plat : un gros appareil par 120 m² environ, sur une grille, loin de la sortie de l'escalier
        if self.roof == 0 {
            let (mn, mx) = self.bbox();
            let n = ((signed_area(&self.foot) * self.details.ac / 120.0).round() as usize).clamp(1, 12);
            let st = if self.roof_access { self.stair() } else { None };
            let mut k = 0;
            let cols = (n as f32).sqrt().ceil() as usize;
            for i in 0..cols * cols {
                if k >= n {
                    break;
                }
                let (cx, cy) = (i % cols, i / cols);
                let p = v3(mn.x + (mx.x - mn.x) * (cx as f32 + 0.5) / cols as f32, mn.y + (mx.y - mn.y) * (cy as f32 + 0.5) / cols as f32, self.lift + self.height() + ROOF_TOP);
                let near_hut = st.map(|st| { let c = st.hole_corners(0.0); let ctr = (c[0] + c[2]) * 0.5; v3(p.x - ctr.x, p.y - ctr.y, 0.0).len() < st.len.max(st.depth) * 0.5 + 2.5 }).unwrap_or(false);
                let ok = [v3(-1.6, -1.6, 0.0), v3(1.6, -1.6, 0.0), v3(1.6, 1.6, 0.0), v3(-1.6, 1.6, 0.0)].iter().all(|d| self.inside_walls(p + *d));
                if ok && !near_hut {
                    v.push((AC_ROOF, p, v3(1.0, 0.0, 0.0), false));
                    k += 1;
                }
            }
        }
        v
    }

    /// Réglages d'un modèle prêt à l'emploi (PRESETS) ; la porte de garage est ajoutée par l'appelant.
    pub fn apply_preset(&mut self, k: usize) {
        let colors = |i: usize| Some(COLORS[i].1);
        match k {
            0 => {
                self.floors = 2;
                self.floor_h = 3.0;
                self.roof = 1;
                self.facade = 3;
                self.color = colors(1);
                self.lift = 0.5;
                self.details.gutters = true;
                self.win_style = 3;
                self.stair_kind = 1;
            }
            1 => {
                self.floors = 5;
                self.floor_h = 3.0;
                self.roof = 0;
                self.facade = 1;
                self.color = colors(2);
                self.looks = vec![FloorLook { facade: Some(2), game: 0, color: colors(7) }];
                self.details.balconies = 1;
                self.details.ac = 0.3;
                self.details.gutters = true;
                self.roof_access = true;
                self.win_style = 1;
            }
            2 => {
                self.floors = 2;
                self.floor_h = 3.6;
                self.roof = 0;
                self.facade = 1;
                self.color = colors(3);
                self.shop = true;
                self.details.awnings = true;
                self.details.awning_color = COLORS[9].1;
                self.details.ac = 0.25;
                self.roof_access = true;
            }
            _ => {
                self.floors = 1;
                self.floor_h = 6.0;
                self.roof = 1;
                self.facade = 2;
                self.color = colors(7);
                self.win_w = 2.4;
                self.win_h = 1.0;
                self.win_step = 6.0;
                self.details.gutters = true;
                self.details.ac = 0.2;
            }
        }
    }

    /// Coque intérieure de l'étage `f` : ce qu'on voit de dedans (faces intérieures des murs, embrasures, vitres vues
    /// de dedans, sol, plafond, escalier, cloisons, garde-corps), décalée d'1 cm vers la pièce. Elle est posée dans
    /// l'intérieur (objet de la pièce) : dedans, le jeu ne dessine le bâtiment (objet du monde) qu'à travers les
    /// passages ; avec elle, les fenêtres n'ont plus besoin de passage (leurs vitres sont opaques) et il ne reste
    /// que les portes et les trémies, comme les MLO FiveM (3 à 13 passages ; analyse du 14 nov. 2026). Les textures
    /// du jeu sont remplacées par les plus proches de DragoWalker (le dictionnaire du bâtiment n'a pas celles du jeu).
    pub fn inside_mesh(&self, f: u8) -> (Vec<V3>, Vec<[f32; 2]>, Vec<(u64, Vec<u32>, u8)>) {
        let (pos, uv, parts) = self.mesh(false);
        let edges = self.edges();
        let nf = self.floors.max(1);
        let h = self.height();
        let inside_poly = |p: V3| -> bool {
            let n = self.foot.len();
            let mut c = false;
            for i in 0..n {
                let (a, b) = (self.foot[i], self.foot[(i + 1) % n]);
                if (a[1] > p.y) != (b[1] > p.y) && p.x < (b[0] - a[0]) * (p.y - a[1]) / (b[1] - a[1]) + a[0] {
                    c = !c;
                }
            }
            c
        };
        let fallback = |k: u64| -> u64 {
            match game_tex_of(k) {
                None => k,
                Some((_, _, _, 0)) => k_facade(self.facade),
                Some((_, _, _, 1)) => K_ROOF_TILE,
                Some((_, _, _, 3)) => K_FLOOR,
                Some(_) => K_INNER,
            }
        };
        let (mut op, mut ou, mut oparts): (Vec<V3>, Vec<[f32; 2]>, Vec<(u64, Vec<u32>, u8)>) = (Vec::new(), Vec::new(), Vec::new());
        for (key, idx, coll) in &parts {
            if *coll == C_INVISIBLE || is_cutout(*key) {
                continue;
            }
            let key = fallback(*key);
            let mut kept: Vec<u32> = Vec::new();
            for t in idx.chunks_exact(3) {
                let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
                let n = (b - a).cross(c - a);
                if n.len() < 1e-8 {
                    continue;
                }
                let n = n.norm();
                let ctr = (a + b + c) * (1.0 / 3.0);
                let zr = ctr.z - self.lift;
                if zr < -0.01 || zr > h + 0.01 {
                    continue; // soubassement, perron, toit
                }
                let tf = (((zr + 0.01) / self.floor_h).floor().max(0.0) as u8).min(nf - 1);
                if tf != f || !inside_poly(ctr) {
                    continue;
                }
                // faces tournées vers dehors près de la façade (nu du mur, vitre vue de dehors, panneau de porte)
                let near_out = edges.iter().any(|&(e0, _, dir, out, len)| {
                    let s = (ctr - e0).dot(dir);
                    let d = (ctr - e0).dot(out * -1.0);
                    s > -0.05 && s < len + 0.05 && d >= -0.01 && d < GLASS_INSET + 0.02 && n.dot(out) > 0.5
                });
                if near_out {
                    continue;
                }
                let base = op.len() as u32;
                for &vi in t {
                    op.push(pos[vi as usize] + n * 0.01);
                    ou.push(uv[vi as usize]);
                }
                kept.extend_from_slice(&[base, base + 1, base + 2]);
            }
            if kept.is_empty() {
                continue;
            }
            match oparts.iter_mut().find(|p| p.0 == key) {
                Some(p) => p.1.extend(kept),
                None => oparts.push((key, kept, C_NONE)),
            }
        }
        (op, ou, oparts)
    }

    /// Nom du modèle de la coque intérieure de l'étage `f`.
    pub fn inside_name(&self, f: u8) -> String {
        format!("{}_in{f}", self.name)
    }

    /// Balcons sur ce côté à cet étage ?
    pub fn balcony_on(&self, e: usize, f: u8) -> bool {
        f >= 1 && match self.details.balconies {
            1 => e == self.door_edge.min(self.foot.len().saturating_sub(1)),
            2 => true,
            _ => false,
        }
    }

    /// Balcons, auvents, gouttières (repère du bâtiment, avant le rez-de-chaussée surélevé).
    fn mesh_details(&self, m: &mut Mesher) {
        let up = v3(0.0, 0.0, 1.0);
        let edges = self.edges();
        let h = self.height();
        let z = |p: V3, zz: f32| v3(p.x, p.y, zz);
        // pavé (boîte) le long d'un côté : de s0 à s1, de o0 à o1 vers l'extérieur, de za à zb
        let boxq = |m: &mut Mesher, a: V3, dir: V3, out: V3, (s0, s1): (f32, f32), (o0, o1): (f32, f32), (za, zb): (f32, f32), key: u64, coll: u8| {
            let p = |s: f32, o: f32, zz: f32| z(a + dir * s + out * o, zz);
            m.quad([p(s0, o1, za), p(s1, o1, za), p(s1, o1, zb), p(s0, o1, zb)], out, key, coll);
            m.quad([p(s0, o0, za), p(s1, o0, za), p(s1, o0, zb), p(s0, o0, zb)], out * -1.0, key, coll);
            m.quad([p(s0, o0, zb), p(s1, o0, zb), p(s1, o1, zb), p(s0, o1, zb)], up, key, coll);
            m.quad([p(s0, o0, za), p(s1, o0, za), p(s1, o1, za), p(s0, o1, za)], up * -1.0, key, coll);
            m.quad([p(s0, o0, za), p(s0, o1, za), p(s0, o1, zb), p(s0, o0, zb)], dir * -1.0, key, coll);
            m.quad([p(s1, o0, za), p(s1, o1, za), p(s1, o1, zb), p(s1, o0, zb)], dir, key, coll);
        };
        for (e, &(a, _, dir, out, len)) in edges.iter().enumerate() {
            for f in 0..self.floors.max(1) {
                let zf = f as f32 * self.floor_h;
                for ho in self.holes(e, f) {
                    // balcon devant chaque porte-fenêtre : dalle, garde-corps à barreaux
                    if ho.kind == 0 && self.balcony_on(e, f) {
                        let (s0, s1) = ((ho.s0 - BALCONY_SIDE).max(0.05), (ho.s1 + BALCONY_SIDE).min(len - 0.05));
                        boxq(m, a, dir, out, (s0, s1), (0.0, BALCONY_D), (zf - 0.15, zf + 0.02), K_PLINTH, C_WALL);
                        let rail_top = zf + 0.02 + RAIL_H;
                        boxq(m, a, dir, out, (s0, s1), (BALCONY_D - 0.05, BALCONY_D), (rail_top - 0.05, rail_top), K_METAL, C_WALL);
                        for (ss, o_range) in [(s0, (0.0, BALCONY_D)), (s1 - 0.05, (0.0, BALCONY_D))] {
                            boxq(m, a, dir, out, (ss, ss + 0.05), o_range, (rail_top - 0.05, rail_top), K_METAL, C_WALL);
                        }
                        // barreaux : façade du balcon et côtés
                        let mut ss = s0 + 0.02;
                        while ss < s1 - 0.02 {
                            boxq(m, a, dir, out, (ss, ss + 0.025), (BALCONY_D - 0.04, BALCONY_D - 0.015), (zf + 0.02, rail_top - 0.05), K_METAL, C_NONE);
                            ss += 0.14;
                        }
                        let mut oo = 0.1;
                        while oo < BALCONY_D - 0.05 {
                            for sx in [s0 + 0.012, s1 - 0.037] {
                                boxq(m, a, dir, out, (sx, sx + 0.025), (oo, oo + 0.025), (zf + 0.02, rail_top - 0.05), K_METAL, C_NONE);
                            }
                            oo += 0.14;
                        }
                        // côtés pleins pour la collision (le personnage ne tombe pas)
                        for ss in [s0, s1] {
                            let p = |o: f32, zz: f32| z(a + dir * ss + out * o, zz);
                            m.quad([p(0.0, zf), p(BALCONY_D, zf), p(BALCONY_D, rail_top), p(0.0, rail_top)], dir, K_METAL, C_INVISIBLE);
                        }
                        let p = |s: f32, zz: f32| z(a + dir * s + out * BALCONY_D, zz);
                        m.quad([p(s0, zf), p(s1, zf), p(s1, rail_top), p(s0, rail_top)], out, K_METAL, C_INVISIBLE);
                    }
                    // auvent au-dessus des portes et des vitrines
                    if self.details.awnings && f == 0 && ho.kind != 0 {
                        let key = tint_key(K_AWNING, self.details.awning_color);
                        let (s0, s1) = (ho.s0 - 0.25, ho.s1 + 0.25);
                        let (zt, zb) = ((ho.z1 + 0.55).min(self.floor_h - 0.1), (ho.z1 + 0.55).min(self.floor_h - 0.1) - 0.55);
                        let p = |s: f32, o: f32, zz: f32| z(a + dir * s + out * o, zz);
                        m.quad([p(s0, 0.0, zt), p(s1, 0.0, zt), p(s1, AWNING_D, zb), p(s0, AWNING_D, zb)], up + out * 0.5, key, C_WALL);
                        m.quad([p(s0, 0.0, zt), p(s1, 0.0, zt), p(s1, AWNING_D, zb), p(s0, AWNING_D, zb)], (up + out * 0.5) * -1.0, key, C_NONE);
                        m.quad([p(s0, AWNING_D, zb - 0.25), p(s1, AWNING_D, zb - 0.25), p(s1, AWNING_D, zb), p(s0, AWNING_D, zb)], out, key, C_NONE);
                        m.quad([p(s0, AWNING_D, zb - 0.25), p(s1, AWNING_D, zb - 0.25), p(s1, AWNING_D, zb), p(s0, AWNING_D, zb)], out * -1.0, key, C_NONE);
                        for ss in [s0, s1] {
                            let (pa, pb, pc) = (p(ss, 0.0, zt), p(ss, AWNING_D, zb), p(ss, AWNING_D, zb - 0.25));
                            m.tri([pa, pb, pc], dir, key, C_NONE);
                            m.tri([pa, pb, pc], dir * -1.0, key, C_NONE);
                        }
                    }
                }
            }
            // gouttière le long de la corniche (toits en pente) et descente à chaque angle
            if self.details.gutters {
                if self.roof != 0 {
                    boxq(m, a, dir, out, (0.0, len), (0.02, 0.16), (h - 0.14, h), K_METAL, C_NONE);
                }
                let top = if self.roof == 0 { h + PARAPET - 0.1 } else { h };
                boxq(m, a, dir, out, (0.05, 0.15), (0.02, 0.12), (0.0, top), K_METAL, C_NONE);
            }
        }
    }

    /// Perrons : dessus des marches et du palier devant chaque porte (repère du bâtiment, rez-de-chaussée surélevé
    /// compris), du bas vers le haut.
    pub fn stoops(&self) -> Vec<[V3; 4]> {
        let mut out = Vec::new();
        if self.lift <= 0.01 {
            return out;
        }
        let n = (self.lift / 0.17).ceil().max(1.0) as usize;
        let rise = self.lift / n as f32;
        let ne = self.foot.len();
        for i in 0..self.doors().len() {
            if self.is_garage(i) {
                continue;
            }
            let Some((c0, dir, o, w, _)) = self.door_frame(i) else { continue };
            let _ = ne;
            let (s0, s1) = (-0.4, w + 0.4);
            let p = |s: f32, d: f32, zz: f32| { let q = c0 + dir * s + o * d; v3(q.x, q.y, zz) };
            // palier (profondeur STOOP_D) puis marches vers l'extérieur
            let top = self.lift + FLOOR_LIFT;
            out.push([p(s0, 0.0, top), p(s1, 0.0, top), p(s1, STOOP_D, top), p(s0, STOOP_D, top)]);
            for k in 0..n.saturating_sub(1) {
                let zz = top - (k + 1) as f32 * rise;
                let (d0, d1) = (STOOP_D + k as f32 * 0.3, STOOP_D + (k + 1) as f32 * 0.3);
                out.push([p(s0, d0, zz), p(s1, d0, zz), p(s1, d1, zz), p(s0, d1, zz)]);
            }
        }
        out
    }

    /// Porte de garage ?
    pub fn is_garage(&self, i: usize) -> bool {
        self.doors().get(i).map(|d| GARAGE_DOORS.contains(&d.model.as_str())).unwrap_or(false)
    }

    /// Rampes d'accès des portes de garage d'un rez-de-chaussée surélevé (pente GARAGE_SLOPE), du sol au seuil.
    pub fn garage_ramps(&self) -> Vec<[V3; 4]> {
        let mut v = Vec::new();
        if self.lift <= 0.01 {
            return v;
        }
        let top = self.lift + FLOOR_LIFT;
        let len = top / GARAGE_SLOPE;
        for i in 0..self.doors().len() {
            if !self.is_garage(i) {
                continue;
            }
            let Some((c0, dir, o, w, _)) = self.door_frame(i) else { continue };
            let p = |s: f32, d: f32, zz: f32| { let q = c0 + dir * s + o * d; v3(q.x, q.y, zz) };
            v.push([p(-0.3, len, 0.0), p(w + 0.3, len, 0.0), p(w + 0.3, 0.0, top), p(-0.3, 0.0, top)]);
        }
        v
    }

    /// Côtés et contremarches du perron (quadrilatère, direction de la face).
    fn stoop_sides(&self) -> Vec<([V3; 4], V3, bool)> {
        let mut v = Vec::new();
        for q in self.stoops() {
            let zt = q[0].z;
            let g = |p: V3| v3(p.x, p.y, 0.0);
            let fwd = q[3] - q[0];
            let side = q[1] - q[0];
            v.push(([g(q[3]), g(q[2]), q[2], q[3]], fwd, true));
            v.push(([g(q[0]), g(q[3]), q[3], q[0]], side * -1.0, false));
            v.push(([g(q[1]), g(q[2]), q[2], q[1]], side, false));
            let _ = zt;
        }
        v
    }

    /// Autour d'une trémie, au plancher `z` (dessous `zb`) : palier d'un colimaçon et garde-corps.
    fn mesh_hole_extras(&self, m: &mut Mesher, st: &Stair, z: f32, zb: f32) {
        let up = v3(0.0, 0.0, 1.0);
        let za = |p: V3, zz: f32| v3(p.x, p.y, zz);
        for t in House::spiral_landing(st) {
            m.tri([za(t[0], z), za(t[1], z), za(t[2], z)], up, self.floor_key(), C_WALL);
            m.tri([za(t[0], zb), za(t[1], zb), za(t[2], zb)], up * -1.0, K_INNER, C_WALL);
        }
        for (p, q) in self.stair_rails(st) {
            let len = (q - p).len();
            if len < 0.05 {
                continue;
            }
            let d = (q - p) * (1.0 / len);
            let n = v3(-d.y, d.x, 0.0) * 0.025;
            let top = z + RAIL_H;
            // main courante
            m.quad([za(p + n, top), za(q + n, top), za(q - n, top), za(p - n, top)], up, K_METAL, C_NONE);
            m.quad([za(p + n, top - 0.05), za(q + n, top - 0.05), za(q + n, top), za(p + n, top)], n, K_METAL, C_NONE);
            m.quad([za(p - n, top - 0.05), za(q - n, top - 0.05), za(q - n, top), za(p - n, top)], n * -1.0, K_METAL, C_NONE);
            // barreaux
            let k = (len / 0.14).floor().max(1.0) as usize;
            for i in 0..=k {
                let c = p + d * (len * i as f32 / k as f32);
                let (a, b) = (c - d * 0.0125, c + d * 0.0125);
                m.quad([za(a + n, z), za(b + n, z), za(b + n, top), za(a + n, top)], n, K_METAL, C_NONE);
                m.quad([za(a - n, z), za(b - n, z), za(b - n, top), za(a - n, top)], n * -1.0, K_METAL, C_NONE);
            }
            // collision : un panneau plein (on ne tombe pas dans la trémie)
            m.quad([za(p, z), za(q, z), za(q, top), za(p, top)], n, K_METAL, C_INVISIBLE);
        }
    }

    /// Édicule de sortie sur le toit-terrasse : quatre murs autour de la trémie, une ouverture du côté où arrive
    /// l'escalier, un toit.
    fn roof_hut(&self, m: &mut Mesher, st: &Stair, fk: u64) {
        let up = v3(0.0, 0.0, 1.0);
        let (z0, z1) = (self.height() + ROOF_TOP, self.height() + ROOF_TOP + HUT_H);
        let pad = 0.15;
        let (s0, s1, w0, w1) = (st.hole.0 - pad, st.hole.1 + pad, st.w.0 - pad, st.w.1 + pad);
        let c = st.at((s0 + s1) * 0.5, (w0 + w1) * 0.5, 0.0);
        let ex = st.exit_dir();
        let last = st.steps().last().map(|(q, _)| st.at((q[2][0] + q[3][0]) * 0.5, (q[2][1] + q[3][1]) * 0.5, 0.0)).unwrap_or(c);
        let sides = [(st.at(s0, w0, 0.0), st.at(s1, w0, 0.0)), (st.at(s1, w0, 0.0), st.at(s1, w1, 0.0)), (st.at(s1, w1, 0.0), st.at(s0, w1, 0.0)), (st.at(s0, w1, 0.0), st.at(s0, w0, 0.0))];
        let door_side = (0..4)
            .filter(|&k| self.inside_walls((sides[k].0 + sides[k].1) * 0.5 + ((sides[k].0 + sides[k].1) * 0.5 - c).norm() * 0.6))
            .max_by(|&i, &j| {
                let o = |k: usize| ((sides[k].0 + sides[k].1) * 0.5 - c).norm().dot(ex);
                o(i).partial_cmp(&o(j)).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0);
        let za = |p: V3, z: f32| v3(p.x, p.y, z);
        for (k, &(a, b)) in sides.iter().enumerate() {
            let out = (a + b) * 0.5 - c;
            let len = (b - a).len();
            let d = (b - a) * (1.0 / len.max(1e-4));
            let wall = |m: &mut Mesher, sa: f32, sb: f32, zl: f32, zh: f32| {
                if sb - sa > 0.01 {
                    m.quad([za(a + d * sa, zl), za(a + d * sb, zl), za(a + d * sb, zh), za(a + d * sa, zh)], out, fk, C_WALL);
                    m.quad([za(a + d * sa, zl), za(a + d * sb, zl), za(a + d * sb, zh), za(a + d * sa, zh)], out * -1.0, K_INNER, C_WALL);
                }
            };
            if k == door_side {
                let t = (last - a).dot(d).clamp(HUT_DOOR_W * 0.5, (len - HUT_DOOR_W * 0.5).max(HUT_DOOR_W * 0.5));
                wall(m, 0.0, t - HUT_DOOR_W * 0.5, z0, z1);
                wall(m, t + HUT_DOOR_W * 0.5, len, z0, z1);
                wall(m, t - HUT_DOOR_W * 0.5, t + HUT_DOOR_W * 0.5, z0 + HUT_DOOR_H, z1);
            } else {
                wall(m, 0.0, len, z0, z1);
            }
        }
        let q = [za(sides[0].0, z1), za(sides[1].0, z1), za(sides[2].0, z1), za(sides[3].0, z1)];
        m.quad(q, up, K_ROOF_FLAT, C_WALL);
        m.quad(q, up * -1.0, K_INNER, C_WALL);
    }

    /// Collision des escaliers : une rampe le long des nez de marche, puis le palier du haut jusqu'au plancher (le
    /// jeu fait de même : les escaliers ont une collision en pente). Quadrilatères, repère du bâtiment.
    pub fn stair_ramps(&self) -> Vec<[V3; 4]> {
        let mut v: Vec<[V3; 4]> = self.stair_ramps_raw().into_iter().map(|q| q.map(|p| v3(p.x, p.y, p.z + self.lift))).collect();
        // perrons : une rampe par perron, du sol au palier
        let st = self.stoops();
        // chaque perron : palier puis ses marches (même nombre pour tous ; pas de perron devant un garage)
        let per = if self.lift > 0.01 { (self.lift / 0.17).ceil().max(1.0) as usize } else { 0 };
        for d in 0..if per == 0 { 0 } else { st.len() / per } {
            let land = st[d * per];
            let last = st[d * per + per - 1];
            if per > 1 {
                let low = |p: V3| v3(p.x, p.y, 0.0);
                v.push([low(last[3]), low(last[2]), land[2], land[3]]);
            }
            v.push(land);
        }
        v
    }

    fn stair_ramps_raw(&self) -> Vec<[V3; 4]> {
        let Some(st) = self.stair() else { return Vec::new() };
        let steps = st.steps();
        let n = steps.len();
        let mut out = Vec::new();
        for (z0, z) in self.stair_levels() {
            let rise = (z - z0) / n as f32;
            let top = |i: usize| z0 + (i + 1) as f32 * rise;
            for (i, (q, _)) in steps.iter().enumerate() {
                // par le milieu des contremarches : continue d'une marche à l'autre (palier compris), même pente
                // qu'une marche ; la dernière s'arrête au plancher
                let back = top(i) - rise * 0.5;
                let front = (top(i) + rise * 0.5).min(z);
                let p = |k: usize, zz: f32| st.at(q[k][0], q[k][1], zz);
                out.push([p(0, back), p(1, back), p(2, front), p(3, front)]);
                if i == 0 {
                    // entrée : du sol au nez de la première marche
                    let f = st.at((q[3][0] + q[2][0]) * 0.5, (q[3][1] + q[2][1]) * 0.5, 0.0) - st.at((q[0][0] + q[1][0]) * 0.5, (q[0][1] + q[1][1]) * 0.5, 0.0);
                    let b0 = p(0, z0) - f;
                    let b1 = p(1, z0) - f;
                    out.push([v3(b0.x, b0.y, z0), v3(b1.x, b1.y, z0), p(1, back), p(0, back)]);
                }
            }
        }
        out
    }

    /// Boîte du modèle (repère du bâtiment).
    pub fn bbox(&self) -> (V3, V3) {
        let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, 0.0), v3(f32::MIN, f32::MIN, 0.0));
        for p in &self.foot {
            mn = mn.min(v3(p[0], p[1], 0.0));
            mx = mx.max(v3(p[0], p[1], 0.0));
        }
        let (_, _, _, hu, hv) = self.roof_frame();
        mx.z = self.lift + self.height() + if self.roof == 0 { PARAPET + if self.roof_access { HUT_H } else { 0.0 } } else { ROOF_SLOPE * hu.max(hv) + 0.1 };
        // ce qui dépasse des murs : balcons, auvents, perrons et rampes de garage
        let out = if self.details.balconies > 0 { BALCONY_D } else { 0.0 }.max(if self.details.awnings { AWNING_D } else { 0.0 }).max(if self.lift > 0.01 { STOOP_D + 0.3 * (self.lift / 0.17).ceil() + self.lift / GARAGE_SLOPE } else { 0.0 }) + 0.2;
        mn.x -= out;
        mn.y -= out;
        mx.x += out;
        mx.y += out;
        (mn, mx)
    }

    /// Escalier qui monte de l'étage `f` à l'étage du dessus, le long du mur du bas du plan (y le plus petit) :
    /// trémie (x0, x1, y0, y1), début des marches, nombre de marches, profondeur d'une marche.
    pub fn stair(&self) -> Option<Stair> {
        if self.decor {
            return None; // on n'entre pas dans un bâtiment de décoration
        }
        let n = (self.floor_h / 0.18).ceil().max(2.0) as usize;
        let edges = self.edges();
        let ne = edges.len();
        let doors = self.doors();
        let kind = self.stair_kind.min(2);
        // emplacement choisi d'un clic : centré sur le point, tourné de quarts de tour (sinon placé tout seul)
        if let Some(c) = self.stair_at {
            for run in [0.28f32, 0.24, 0.2] {
                let (len, depth) = Stair::size(kind, n, run);
                let ang = self.stair_rot as f32 * std::f32::consts::FRAC_PI_2;
                let dir = v3(ang.cos(), ang.sin(), 0.0);
                let inw = v3(-dir.y, dir.x, 0.0);
                let a = v3(c[0], c[1], 0.0) - dir * (len * 0.5) - inw * (depth * 0.5);
                let mut st = Stair::new(self, kind, a, dir, inw, 0.0, 0.0, n, run);
                st.clicked = true;
                if st.corners().iter().all(|q| self.inside_walls(*q)) {
                    return Some(st);
                }
            }
        }
        // murs essayés : droits (le long d'un axe) d'abord, puis les plus longs
        let mut order: Vec<usize> = (0..ne).collect();
        let axis = |k: usize| edges[k].2.x.abs().max(edges[k].2.y.abs()) > 0.999;
        order.sort_by(|&i, &j| axis(j).cmp(&axis(i)).then(edges[j].4.partial_cmp(&edges[i].4).unwrap_or(std::cmp::Ordering::Equal)));
        for run in [0.28f32, 0.24, 0.2] {
            let (full, _) = Stair::size(kind, n, run);
            for &e in &order {
                let (a, _, dir, out, len) = edges[e];
                let inw = out * -1.0;
                let (s0, w0) = (WALL_T + 0.3, WALL_T + 0.02);
                let s1 = s0 + full;
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
                let st = Stair::new(self, kind, a, dir, inw, s0, w0, n, run);
                if st.corners().iter().all(|q| self.inside_walls(*q)) {
                    return Some(st);
                }
            }
        }
        None
    }

    /// Point (repère du bâtiment) dans l'emprise, à au moins l'épaisseur des murs de chaque mur ?
    pub fn inside_walls(&self, p: V3) -> bool {
        let ne = self.foot.len();
        let mut c = false;
        for i in 0..ne {
            let (a, b) = (self.foot[i], self.foot[(i + 1) % ne]);
            if (a[1] > p.y) != (b[1] > p.y) && p.x < (b[0] - a[0]) * (p.y - a[1]) / (b[1] - a[1]) + a[0] {
                c = !c;
            }
        }
        c && self.edges().iter().all(|&(a, _, dir, _, len)| {
            let s = (p - a).dot(dir).clamp(0.0, len);
            let q = a + dir * s;
            v3(p.x - q.x, p.y - q.y, 0.0).len() >= WALL_T - 0.01
        })
    }

    /// Volées d'escalier : (bas, haut) de chacune, repère du bâtiment. Une par étage, plus une vers le toit-terrasse
    /// si on peut y monter.
    pub fn stair_levels(&self) -> Vec<(f32, f32)> {
        let mut v: Vec<(f32, f32)> = (1..self.floors.max(1)).map(|f| ((f - 1) as f32 * self.floor_h + if f == 1 { FLOOR_LIFT } else { 0.0 }, f as f32 * self.floor_h)).collect();
        if self.roof_access && self.roof == 0 {
            let f = self.floors.max(1) - 1;
            v.push((f as f32 * self.floor_h + if f == 0 { FLOOR_LIFT } else { 0.0 }, self.height() + ROOF_TOP));
        }
        v
    }

    /// Morceaux convexes d'un polygone sans la trémie de l'escalier.
    fn without_hole(st: Option<Stair>, poly: &[[f32; 2]]) -> Vec<Vec<[f32; 2]>> {
        let Some(st) = st else { return vec![poly.to_vec()] };
        let u = move |p: [f32; 2]| (v3(p[0], p[1], 0.0) - st.a).dot(st.dir);
        let w = move |p: [f32; 2]| (v3(p[0], p[1], 0.0) - st.a).dot(st.inw);
        let mut pieces = vec![poly.to_vec()];
        for (h0, h1, w0, w1) in st.rects() {
            pieces = pieces
                .into_iter()
                .flat_map(|pc| {
                    let mid = clip_half(&clip_half(&pc, &|p| u(p) - h0), &|p| h1 - u(p));
                    vec![clip_half(&pc, &|p| h0 - u(p)), clip_half(&pc, &|p| u(p) - h1), clip_half(&mid, &|p| w0 - w(p)), clip_half(&mid, &|p| w(p) - w1)]
                })
                .filter(|p| p.len() >= 3)
                .collect();
        }
        pieces
    }

    /// Ce qui reste de plancher dans la trémie d'un colimaçon : les coins du carré hors du cercle et le coin entre la
    /// dernière et la première marche (palier d'arrivée). Triangles, repère du bâtiment, z = 0.
    fn spiral_landing(st: &Stair) -> Vec<[V3; 3]> {
        let mut v = Vec::new();
        if st.kind != 2 {
            return v;
        }
        let c = st.at(st.start + SPIRAL_R, st.w.0 + SPIRAL_R, 0.0);
        let ring = |a: f32, r: f32| c + (st.dir * a.cos() + st.inw * a.sin()) * r;
        let sq = |a: f32| { let (x, y) = (a.cos(), a.sin()); let k = SPIRAL_R / x.abs().max(y.abs()); c + (st.dir * x + st.inw * y) * k };
        let k = 72;
        for i in 0..k {
            let (a0, a1) = (i as f32 * std::f32::consts::TAU / k as f32, (i + 1) as f32 * std::f32::consts::TAU / k as f32);
            v.push([ring(a0, SPIRAL_R), sq(a0), sq(a1)]);
            v.push([ring(a0, SPIRAL_R), sq(a1), ring(a1, SPIRAL_R)]);
        }
        // coin d'arrivée : de la fin de la dernière marche au début de la première
        let (e0, e1) = (-std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * 0.9, -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU);
        for i in 0..6 {
            let (a0, a1) = (e0 + (e1 - e0) * i as f32 / 6.0, e0 + (e1 - e0) * (i + 1) as f32 / 6.0);
            v.push([c, ring(a0, SPIRAL_R), ring(a1, SPIRAL_R)]);
        }
        v
    }

    /// Garde-corps autour de la trémie à la hauteur z (repère du bâtiment) : (début, fin) de chaque morceau, sans le
    /// côté où l'escalier arrive ni les côtés contre un mur. Pour un colimaçon : l'arc au-dessus des marches et le
    /// bord du palier au-dessus de la première marche.
    fn stair_rails(&self, st: &Stair) -> Vec<(V3, V3)> {
        let mut v = Vec::new();
        if st.kind == 2 {
            let c = st.at(st.start + SPIRAL_R, st.w.0 + SPIRAL_R, 0.0);
            let ring = |a: f32| c + (st.dir * a.cos() + st.inw * a.sin()) * SPIRAL_R;
            let (a0, a1) = (-std::f32::consts::FRAC_PI_2, -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * 0.9);
            let k = 20;
            for i in 0..k {
                v.push((ring(a0 + (a1 - a0) * i as f32 / k as f32), ring(a0 + (a1 - a0) * (i + 1) as f32 / k as f32)));
            }
            v.push((c, ring(a0)));
            return v;
        }
        let last = st.steps().last().map(|(q, _)| st.at((q[2][0] + q[3][0]) * 0.5, (q[2][1] + q[3][1]) * 0.5, 0.0));
        for (p, q, n) in st.hole_edges() {
            let mid = (p + q) * 0.5;
            if let Some(l) = last {
                // côté d'arrivée : le bout de la dernière marche est dessus
                let d = (q - p).norm();
                let t = (l - p).dot(d);
                if t > -0.05 && t < (q - p).len() + 0.05 && (p + d * t - l).len() < 0.05 {
                    continue;
                }
            }
            // contre un mur : rien derrière ce côté
            if !self.inside_walls(mid - n * 0.4) {
                continue;
            }
            v.push((p, q));
        }
        v
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
        let mut v = vec![DoorSpec { edge: self.door_edge, t: self.door_t, model: self.door_model.clone(), prop: self.door_prop, bbox: self.door_box, locked: self.door_locked }];
        v.extend(self.more_doors.iter().cloned());
        v
    }

    /// La porte `i` est-elle fermée à clé en jeu ? (toutes, pour un bâtiment de décoration)
    pub fn door_locked_at(&self, i: usize) -> bool {
        self.decor || self.doors().get(i).map(|d| d.locked).unwrap_or(false)
    }

    /// Remplace la porte `i`.
    pub fn put_door(&mut self, i: usize, d: DoorSpec) {
        if i == 0 {
            self.door_edge = d.edge;
            self.door_t = d.t;
            self.door_model = d.model;
            self.door_prop = d.prop;
            self.door_box = d.bbox;
            self.door_locked = d.locked;
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
        let p = c0 - dir * (mn.x * sxy) + inward * (WALL_T * 0.5 - mid_y) + v3(0.0, 0.0, self.lift + FLOOR_LIFT - mn.z * sz);
        Some((p, dir.y.atan2(dir.x), sxy, sz))
    }

    /// Pièces de l'intérieur (nom, bas, haut ; repère du bâtiment) et portails entre elles (passages des cloisons,
    /// bouts de cloison laissés ouverts). Chaque étage est coupé par ses cloisons droites (à 3° près) qui traversent au
    /// moins la moitié de la pièce où elles tombent ; les autres cloisons restent dans la pièce.
    pub fn interior_rooms(&self) -> (Vec<(String, V3, V3)>, Vec<[V3; 4]>) {
        let mut rooms = Vec::new();
        let mut inner = Vec::new();
        for f in 0..self.floors.max(1) {
            let (lo, hi) = self.floor_room(f);
            let (z0, z1) = (lo.z, hi.z);
            let mut rects: Vec<(f32, f32, f32, f32)> = vec![(lo.x, hi.x, lo.y, hi.y)];
            for w in self.walls.iter().filter(|w| w.floor == f) {
                let (dx, dy) = (w.b[0] - w.a[0], w.b[1] - w.a[1]);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 0.5 {
                    continue;
                }
                let vertical = dx.abs() < len * 0.05; // x constant
                let horizontal = dy.abs() < len * 0.05;
                if !vertical && !horizontal {
                    continue;
                }
                let m = [(w.a[0] + w.b[0]) * 0.5, (w.a[1] + w.b[1]) * 0.5];
                let Some(ri) = rects.iter().position(|r| m[0] > r.0 && m[0] < r.1 && m[1] > r.2 && m[1] < r.3) else { continue };
                let r = rects[ri];
                // (position de la cloison, début et fin le long de la cloison, étendue de la pièce le long de la cloison)
                let (c, p0, p1, r0, r1) = if vertical { (m[0], w.a[1].min(w.b[1]), w.a[1].max(w.b[1]), r.2, r.3) } else { (m[1], w.a[0].min(w.b[0]), w.a[0].max(w.b[0]), r.0, r.1) };
                if (p1.min(r1) - p0.max(r0)) < 0.5 * (r1 - r0) {
                    continue;
                }
                let (na, nb) = if vertical { ((r.0, c - PART_T * 0.5, r.2, r.3), (c + PART_T * 0.5, r.1, r.2, r.3)) } else { ((r.0, r.1, r.2, c - PART_T * 0.5), (r.0, r.1, c + PART_T * 0.5, r.3)) };
                if (na.1 - na.0) < 0.8 || (na.3 - na.2) < 0.8 || (nb.1 - nb.0) < 0.8 || (nb.3 - nb.2) < 0.8 {
                    continue;
                }
                rects[ri] = na;
                rects.push(nb);
                // portail dans le plan de la cloison, de a0 à a1 le long, de za à zb
                let mut portal = |a0: f32, a1: f32, za: f32, zb: f32| {
                    if a1 - a0 < 0.3 {
                        return;
                    }
                    let pt = |t: f32, zz: f32| if vertical { v3(c, t, zz) } else { v3(t, c, zz) };
                    inner.push([pt(a0, za), pt(a0, zb), pt(a1, zb), pt(a1, za)]);
                };
                // bouts restés ouverts
                portal(r0, p0.max(r0), z0, z1);
                portal(p1.min(r1), r1, z0, z1);
                if w.door && len > PART_DOOR_W + 0.4 {
                    let mid = (p0 + p1) * 0.5;
                    portal(mid - PART_DOOR_W * 0.5, mid + PART_DOOR_W * 0.5, z0, (z0 + PART_DOOR_H).min(z1));
                }
            }
            let many = rects.len() > 1;
            for (k, r) in rects.iter().enumerate() {
                let name = if many { format!("etage{}_piece{}", f + 1, k + 1) } else { format!("etage{}", f + 1) };
                rooms.push((name, v3(r.0, r.2, z0), v3(r.1, r.3, z1)));
            }
        }
        (rooms, inner)
    }

    /// Collision des pièces de l'intérieur de ce bâtiment (pièce, triangle ; repère du bâtiment) : sol et plafond de
    /// chaque pièce, limités à l'emprise réelle et sans les trémies, et pas de murs (ceux du bâtiment sont dans sa
    /// propre collision). La boîte de la pièce, elle, fermait le creux d'une maison en L de murs invisibles.
    pub fn room_collision(&self, rooms: &[crate::model::Room]) -> Vec<(usize, [V3; 3])> {
        let mut out = Vec::new();
        let tris = triangulate(&self.foot);
        let st = self.stair();
        let nf = self.floors.max(1);
        for (ri, r) in rooms.iter().enumerate().skip(1) {
            let f = (((r.bb_min.z - self.lift - 0.02) / self.floor_h).round().max(0.0) as u8).min(nf - 1);
            let zf = self.lift + f as f32 * self.floor_h + if f == 0 { FLOOR_LIFT } else { 0.0 };
            let zc = self.lift + if f + 1 >= nf { self.height() - 0.02 } else { (f + 1) as f32 * self.floor_h - SLAB_T };
            let (x0, x1, y0, y1) = (r.bb_min.x, r.bb_max.x, r.bb_min.y, r.bb_max.y);
            let ceil_hole = f + 1 < nf || (self.roof_access && self.roof == 0);
            for t in &tris {
                let tri: Vec<[f32; 2]> = t.iter().map(|&k| self.foot[k]).collect();
                let pc = clip_half(&clip_half(&clip_half(&clip_half(&tri, &|p| p[0] - x0), &|p| x1 - p[0]), &|p| p[1] - y0), &|p| y1 - p[1]);
                if pc.len() < 3 {
                    continue;
                }
                for (z, hole, down) in [(zf, f > 0, false), (zc, ceil_hole, true)] {
                    for q in House::without_hole(if hole { st } else { None }, &pc) {
                        for k in 1..q.len() - 1 {
                            let (a, b, c) = (v3(q[0][0], q[0][1], z), v3(q[k][0], q[k][1], z), v3(q[k + 1][0], q[k + 1][1], z));
                            out.push((ri, if down { [a, c, b] } else { [a, b, c] }));
                        }
                    }
                }
            }
            // palier du colimaçon dans la trémie
            if f > 0 {
                if let Some(st) = st {
                    for t in House::spiral_landing(&st) {
                        let za = |p: V3| v3(p.x, p.y, zf);
                        if t.iter().all(|p| p.x >= x0 - 0.01 && p.x <= x1 + 0.01 && p.y >= y0 - 0.01 && p.y <= y1 + 0.01) {
                            out.push((ri, [za(t[0]), za(t[1]), za(t[2])]));
                        }
                    }
                }
            }
        }
        out
    }

    /// Pièce de l'étage `f` (boîte intérieure du plan, repère du bâtiment).
    pub fn floor_room(&self, f: u8) -> (V3, V3) {
        let (mn, mx) = self.bbox();
        let z0 = self.lift + f as f32 * self.floor_h;
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
pub fn house_textures() -> &'static [(u64, TexData)] {
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
            v.push((K_AWNING, tex(128, |x, y| {
                // rayures de 16 cm, claires et un peu plus sombres (la couleur vient de la teinte)
                let g = 0.96 + hash(x, y, 30) * 0.04;
                if (x / 14) % 2 == 0 { [235.0 * g, 235.0 * g, 235.0 * g] } else { [175.0 * g, 175.0 * g, 175.0 * g] }
            })));
            v.push((K_METAL, tex(64, |x, y| {
                let g = 0.9 + hash(x, y, 31) * 0.1;
                [58.0 * g, 60.0 * g, 64.0 * g]
            })));
            v.push((K_PLINTH, tex(n, |x, y| {
                // pierre : blocs de 40 × 25 cm
                let row = y / 21;
                let off = if row % 2 == 0 { 0 } else { 17 };
                let joint = y % 21 == 0 || (x + off) % 34 == 0;
                let g = 0.85 + hash((x + off) / 34, row, 32) * 0.15 + hash(x, y, 33) * 0.05;
                if joint { [120.0, 116.0, 108.0] } else { [176.0 * g, 170.0 * g, 156.0 * g] }
            })));
            v.push((K_MIRROR, tex(64, |x, y| {
                // reflet gris-bleu (aperçu ; en jeu, le miroir reflète la pièce)
                let g = 0.85 + (x + y) as f32 / 128.0 * 0.15;
                [170.0 * g, 182.0 * g, 192.0 * g]
            })));
            v.push((K_DOOR, tex(64, |x, y| {
                let panel = (x > 8 && x < 56) && ((y > 6 && y < 28) || (y > 34 && y < 58));
                let g = 0.9 + hash(x, y, 13) * 0.1;
                if panel { [96.0 * g, 62.0 * g, 40.0 * g] } else { [120.0 * g, 80.0 * g, 52.0 * g] }
            })));
            v
        })
}

/// Texture teinte (gardée en mémoire : la recalculer à chaque modèle coûtait cher).
pub(crate) fn tinted_cached(key: u64) -> Option<TexData> {
    static CACHE: std::sync::Mutex<Option<std::collections::HashMap<u64, TexData>>> = std::sync::Mutex::new(None);
    let mut c = CACHE.lock().ok()?;
    let map = c.get_or_insert_with(Default::default);
    if let Some(t) = map.get(&key) {
        return Some(t.clone());
    }
    let (base, rgb) = tint_of(key)?;
    let b = house_textures().iter().find(|t| t.0 == base)?;
    let t = tinted(&b.1, rgb);
    map.insert(key, t.clone());
    Some(t)
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
    mesh_rmodel(h, pos, uv, parts)
}

/// Coque intérieure de l'étage `f`, prête pour la carte graphique ou l'export.
pub fn house_inside_rmodel(h: &House, f: u8) -> (crate::gta::drawable::RModel, Vec<u64>, Vec<(u64, TexData)>) {
    let (pos, uv, parts) = h.inside_mesh(f);
    mesh_rmodel(h, pos, uv, parts)
}

fn mesh_rmodel(h: &House, pos: Vec<V3>, uv: Vec<[f32; 2]>, parts: Vec<(u64, Vec<u32>, u8)>) -> (crate::gta::drawable::RModel, Vec<u64>, Vec<(u64, TexData)>) {
    // collision seulement : rien à dessiner
    let parts: Vec<(u64, Vec<u32>, u8)> = parts.into_iter().filter(|p| p.2 != C_INVISIBLE).collect();
    // seulement les textures dont ce modèle se sert
    let used: std::collections::HashSet<u64> = parts.iter().map(|p| p.0).collect();
    let mut texs: Vec<(u64, TexData)> = house_textures().iter().filter(|t| used.contains(&t.0)).cloned().collect();
    // textures teintes : faites depuis leur texture de base
    for k in &used {
        if tint_of(*k).is_some() && !texs.iter().any(|t| t.0 == *k) {
            if let Some(t) = tinted_cached(*k) {
                texs.push((*k, t));
            }
        }
    }
    // textures du jeu utilisées : lues dans le jeu ; sans le jeu, celle de DragoWalker la plus proche (façade, tuiles),
    // une texture vide pour une salissure
    for (k, _, _) in &parts {
        if let Some((_, _, _, kind)) = game_tex_of(*k) {
            if texs.iter().any(|t| t.0 == *k) {
                continue;
            }
            let t = game_loaded(*k).unwrap_or_else(|| match kind {
                0 => house_textures().iter().find(|t| t.0 == k_facade(h.facade)).map(|t| t.1.clone()).unwrap(),
                1 => house_textures().iter().find(|t| t.0 == K_ROOF_TILE).map(|t| t.1.clone()).unwrap(),
                3 => house_textures().iter().find(|t| t.0 == K_FLOOR).map(|t| t.1.clone()).unwrap(),
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
    /// escalier à placer d'un clic sur le sol (bâtiment)
    pub stair_pick: Option<usize>,
    /// étage dont on règle l'aspect
    pub look_floor: u8,
    /// pas d'annulation déjà pris pour le geste en cours
    pub undo_taken: bool,
    /// signature de chaque bâtiment envoyé à la vue
    pub sigs: std::collections::HashMap<String, u64>,
}

impl DwApp {
    /// Bâtiment de l'atelier porté par un objet, s'il y en a un.
    pub fn house_of(&self, archetype: &str) -> Option<usize> {
        let a = archetype.to_lowercase();
        self.proj.houses.iter().position(|h| h.name == a)
    }

    fn new_house_name(&self) -> String {
        let mut n = 1;
        while self.proj.houses.iter().any(|h| h.name == format!("{HOUSE_PREFIX}{n}")) || self.proj.find_mlo(&format!("{HOUSE_PREFIX}{n}_int")).is_some() || self.proj.ymaps.iter().any(|y| y.entities.iter().any(|e| e.archetype.eq_ignore_ascii_case(&format!("{HOUSE_PREFIX}{n}")))) {
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
        if let Some(hi) = self.atelier.stair_pick {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.atelier.stair_pick = None;
                return;
            }
            if resp.clicked_by(PointerButton::Primary) {
                let Some(mouse) = ctx.input(|i| i.pointer.interact_pos()) else { return };
                let Some(p) = self.house_floor_hit(hi, 0, mouse) else {
                    self.status = "Clique sur le sol du rez-de-chaussée, à l'intérieur du bâtiment.".into();
                    return;
                };
                self.push_undo();
                let old = self.proj.houses[hi].stair_at;
                self.proj.houses[hi].stair_at = Some(p);
                let ok = self.proj.houses[hi].stair().map(|st| st.clicked).unwrap_or(false);
                if ok {
                    self.atelier.stair_pick = None;
                    self.atelier.sig = 0;
                    self.status = "Escalier placé (centré sur le point, même place à chaque étage). « Tourner » pour changer son sens.".into();
                } else {
                    self.proj.houses[hi].stair_at = old;
                    self.status = "L'escalier ne tient pas à cet endroit (trop près d'un mur) : clique plus au milieu, ou tourne-le.".into();
                }
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
                                self.proj.houses[hi].more_doors.push(DoorSpec { edge, t, model, prop: None, bbox: None, locked: false });
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
        let z = z - h.lift;
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
        let sc = v3(1.0 / b.scale_xy.max(1e-3), 1.0 / b.scale_xy.max(1e-3), 1.0 / b.scale_z.max(1e-3));
        let (lo, ld) = (v3(lo.x * sc.x, lo.y * sc.y, lo.z * sc.z), v3(ld.x * sc.x, ld.y * sc.y, ld.z * sc.z));
        let zf = h.lift + f as f32 * h.floor_h + if f == 0 { FLOOR_LIFT } else { 0.0 };
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
        let ceil = h.lift + if f + 1 >= h.floors.max(1) { h.height() - 0.02 } else { (f + 1) as f32 * h.floor_h - SLAB_T };
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
        let sc = v3(1.0 / b.scale_xy.max(1e-3), 1.0 / b.scale_xy.max(1e-3), 1.0 / b.scale_z.max(1e-3));
        let (lo, ld) = (v3(lo.x * sc.x, lo.y * sc.y, lo.z * sc.z), v3(ld.x * sc.x, ld.y * sc.y, ld.z * sc.z));
        let top = h.lift + h.height();
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
        // aussi pour un bâtiment devenu intérieur : sa porte reste un objet du monde, la seule qui s'affiche en jeu
        // (retour de la personne, 13 nov. 2026 : la porte d'une entité d'intérieur disparaissait)
        let h = self.proj.houses[hi].clone();
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
                if !placed.contains(&h.name) {
                    continue;
                }
                for (i, d) in h.doors().iter().enumerate() {
                    // bâtiment transformé avant le 13 nov. 2026 : sa porte avait été mise dans l'intérieur (invisible)
                    if d.prop.map(|p| !ids.contains(&p)).unwrap_or(h.interior.is_some()) {
                        lost.push((hi, i));
                    }
                }
            }
        }
        for (hi, i) in lost {
            // déjà reposée par un appel précédent de cette boucle (house_set_door rappelle house_doors_follow)
            let Some(mut d) = self.proj.houses.get(hi).and_then(|h| h.doors().get(i).cloned()) else { continue };
            if d.prop.map(|p| self.proj.ymaps.iter().any(|y| y.entities.iter().any(|e| e.id == p))).unwrap_or(false) {
                continue;
            }
            d.prop = None;
            self.proj.houses[hi].put_door(i, d);
            let _ = self.house_set_door(hi, i, None);
        }
        self.house_doors_out_of_mlo();
        self.house_props_sync();
        self.house_cleanup();
        self.house_copies_sync();
        self.house_ensure_shells();
        let mut moves: Vec<(u64, V3, Quat, f32, f32)> = Vec::new();
        for h in &self.proj.houses {
            let Some(b) = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.archetype.to_lowercase() == h.name) else { continue };
            for (i, d) in h.doors().iter().enumerate() {
                let Some(did) = d.prop else { continue };
                let Some((local, ang, sxy, sz)) = h.door_fit(i) else { continue };
                moves.push((did, place_at(b, local), b.rot.mul(Quat::from_z(ang)).normalized(), sxy, sz));
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

    /// Retire de l'intérieur d'un bâtiment les portes qui y avaient été mises par « Transformer en intérieur » (avant le
    /// 13 nov. 2026) quand la porte du monde est là : en jeu, seule la porte du monde s'affiche et s'ouvre.
    fn house_doors_out_of_mlo(&mut self) {
        let ids: std::collections::HashSet<u64> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter().map(|e| e.id)).collect();
        let mut todo: Vec<(usize, usize, Vec<V3>)> = Vec::new();
        for (hi, h) in self.proj.houses.iter().enumerate() {
            if h.interior.is_none() {
                continue;
            }
            let Some((ti, mi)) = self.house_mlo(hi) else { continue };
            let spots: Vec<V3> = h.doors().iter().enumerate().filter(|(_, d)| d.prop.map(|p| ids.contains(&p)).unwrap_or(false)).filter_map(|(i, _)| h.door_fit(i).map(|f| f.0)).collect();
            if !spots.is_empty() {
                todo.push((ti, mi, spots));
            }
        }
        for (ti, mi, spots) in todo {
            self.proj.ytyps[ti].mlos[mi].entities.retain(|e| !(e.archetype.to_lowercase().contains("door") && spots.iter().any(|s| (e.pos - *s).len() < 0.8)));
        }
    }

    /// Climatiseurs du jeu de chaque bâtiment (objets du projet) : posés, retirés et replacés pour suivre le bâtiment
    /// et ses réglages.
    fn house_props_sync(&mut self) {
        let Some(w) = self.game.world.as_ref() else { return };
        let mut boxes: std::collections::HashMap<&'static str, (V3, V3)> = std::collections::HashMap::new();
        for n in AC_WALL.iter().copied().chain(std::iter::once(AC_ROOF)) {
            if let Some(a) = w.archetypes.get(&crate::gta::crypto::joaat(n)) {
                boxes.insert(n, (a.bb_min, a.bb_max));
            }
        }
        let ids: std::collections::HashSet<u64> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter().map(|e| e.id)).collect();
        for hi in 0..self.proj.houses.len() {
            let h = self.proj.houses[hi].clone();
            let Some(b) = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.archetype.to_lowercase() == h.name).cloned() else { continue };
            let spots: Vec<_> = h.ac_spots().into_iter().filter(|s| boxes.contains_key(s.0)).collect();
            // (objet, modèle) actuels
            let cur: Vec<(u64, String)> = h.attached.iter().filter(|id| ids.contains(id)).filter_map(|id| self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).find(|e| e.id == *id).map(|e| (e.id, e.archetype.clone()))).collect();
            let same = cur.len() == h.attached.len() && cur.len() == spots.len() && cur.iter().zip(&spots).all(|(c, s)| c.1 == s.0);
            if !same {
                let old: std::collections::HashSet<u64> = h.attached.iter().copied().collect();
                if !old.is_empty() {
                    self.proj.delete_ids(&old);
                }
                let yi = self.ensure_target_ymap();
                let mut new_ids = Vec::new();
                for sp in &spots {
                    let mut e = Entity::new(sp.0, b.pos);
                    e.lod_dist = 120.0;
                    new_ids.push(e.id);
                    self.proj.ymaps[yi].entities.push(e);
                }
                self.proj.houses[hi].attached = new_ids;
            }
            // placement : au mur, le dos de l'appareil contre la façade (son Y vers l'extérieur), le haut sous l'appui ;
            // sur le toit, posé au milieu de son emplacement
            let list = self.proj.houses[hi].attached.clone();
            for (id, sp) in list.iter().zip(&spots) {
                let (mn, mx) = boxes[sp.0];
                let yaw = sp.2.y.atan2(sp.2.x) - std::f32::consts::FRAC_PI_2;
                let q = Quat::from_z(yaw);
                let c = v3((mn.x + mx.x) * 0.5, (mn.y + mx.y) * 0.5, 0.0);
                let local = if sp.3 {
                    let depth = mx.y - mn.y;
                    sp.1 + sp.2 * (depth * 0.5 + 0.02) - q.rotate(v3(c.x, c.y, mx.z))
                } else {
                    sp.1 - q.rotate(v3(c.x, c.y, mn.z))
                };
                let (p, r) = (place_at(&b, local), b.rot.mul(q).normalized());
                for y in self.proj.ymaps.iter_mut() {
                    if let Some(e) = y.entities.iter_mut().find(|e| e.id == *id) {
                        if (e.pos - p).len() > 1e-3 || (e.rot.w - r.w).abs() + (e.rot.z - r.z).abs() > 1e-4 {
                            e.pos = p;
                            e.rot = r;
                        }
                    }
                }
            }
        }
    }

    /// Chaque bâtiment transformé en intérieur a sa coque intérieure par étage dans son intérieur (objet de la
    /// première pièce de l'étage, à l'origine du bâtiment) ; celles d'un étage disparu partent.
    pub fn house_ensure_shells(&mut self) {
        for hi in 0..self.proj.houses.len() {
            let h = self.proj.houses[hi].clone();
            if h.interior.is_none() {
                continue;
            }
            let Some((ti, mi)) = self.house_mlo(hi) else { continue };
            let prefix = format!("{}_in", h.name);
            let m = &mut self.proj.ytyps[ti].mlos[mi];
            let nf = h.floors.max(1);
            m.entities.retain(|e| {
                let a = e.archetype.to_lowercase();
                !(a.starts_with(&prefix) && a[prefix.len()..].parse::<u8>().map(|f| f >= nf).unwrap_or(false))
            });
            for f in 0..nf {
                let nm = h.inside_name(f);
                if m.entities.iter().any(|e| e.archetype.eq_ignore_ascii_case(&nm)) {
                    continue;
                }
                let zf = h.floor_room(f).0.z;
                let room = (1..m.rooms.len()).find(|&r| (m.rooms[r].bb_min.z - zf).abs() < 0.3).unwrap_or(1.min(m.rooms.len().saturating_sub(1)));
                if room == 0 {
                    continue;
                }
                let mut e = Entity::new(&nm, V3::ZERO);
                e.flags = crate::mech::MLO_ENTITY_FLAGS;
                e.room = room;
                m.entities.push(e);
            }
        }
    }

    /// Bâtiment qui n'est plus posé nulle part (supprimé avec Suppr) : ses portes, climatiseurs et objets de copies
    /// partent aussi (ils restaient dans la carte et étaient exportés). Intérieur supprimé : le bâtiment n'en a plus.
    fn house_cleanup(&mut self) {
        let placed: std::collections::HashSet<String> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter().map(|e| e.archetype.to_lowercase())).collect();
        let mut gone: std::collections::HashSet<u64> = std::collections::HashSet::new();
        for hi in 0..self.proj.houses.len() {
            if let Some(n) = self.proj.houses[hi].interior.clone() {
                if self.proj.find_mlo(&n).is_none() {
                    self.proj.houses[hi].interior = None;
                    self.atelier.sig = 0;
                }
            }
            if placed.contains(&self.proj.houses[hi].name) {
                continue;
            }
            let h = &mut self.proj.houses[hi];
            gone.extend(h.attached.drain(..));
            for c in h.copies.drain(..) {
                gone.extend(c.doors);
                gone.extend(c.ac);
            }
            for i in 0..h.doors().len() {
                if let Some(mut d) = h.doors().get(i).cloned() {
                    if let Some(p) = d.prop.take() {
                        gone.insert(p);
                        h.put_door(i, d);
                    }
                }
            }
        }
        if !gone.is_empty() {
            self.proj.delete_ids(&gone);
        }
    }

    /// Les autres placements d'un bâtiment (copies) ont aussi leurs portes et leurs climatiseurs, rangés dans
    /// House.copies ; ceux d'une copie supprimée partent. L'intérieur, lui, reste celui du premier placement.
    fn house_copies_sync(&mut self) {
        if self.game.world.is_none() {
            return;
        }
        let mut index: std::collections::HashMap<u64, (usize, usize)> = std::collections::HashMap::new();
        for (yi, y) in self.proj.ymaps.iter().enumerate() {
            for (ei, e) in y.entities.iter().enumerate() {
                index.insert(e.id, (yi, ei));
            }
        }
        let mut gone: std::collections::HashSet<u64> = std::collections::HashSet::new();
        let mut create: Vec<(usize, usize, bool, usize, String)> = Vec::new(); // (bâtiment, copie, porte ?, rang, modèle)
        let mut moves: Vec<(u64, V3, Quat)> = Vec::new();
        let boxes = |n: &str| self.game.world.as_ref().and_then(|w| w.archetypes.get(&crate::gta::crypto::joaat(n)).map(|a| (a.bb_min, a.bb_max)));
        for hi in 0..self.proj.houses.len() {
            let h = &self.proj.houses[hi];
            let places: Vec<Entity> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.archetype.to_lowercase() == h.name).cloned().collect();
            let extra: Vec<&Entity> = places.iter().skip(1).collect();
            // copies disparues
            let keep: std::collections::HashSet<u64> = extra.iter().map(|e| e.id).collect();
            let mut copies = h.copies.clone();
            for c in copies.iter().filter(|c| !keep.contains(&c.placement)) {
                gone.extend(c.doors.iter().copied());
                gone.extend(c.ac.iter().copied());
            }
            copies.retain(|c| keep.contains(&c.placement));
            // modèles voulus : ceux des portes et climatiseurs du premier placement
            let door_models: Vec<String> = h.doors().iter().map(|d| d.prop.and_then(|p| index.get(&p)).map(|&(yi, ei)| self.proj.ymaps[yi].entities[ei].archetype.clone()).unwrap_or_default()).collect();
            let spots = h.ac_spots();
            for b in &extra {
                let ci = match copies.iter().position(|c| c.placement == b.id) {
                    Some(i) => i,
                    None => {
                        copies.push(CopyProps { placement: b.id, ..Default::default() });
                        copies.len() - 1
                    }
                };
                let c = &mut copies[ci];
                // objets disparus ou modèles changés : refaits
                let ok = |ids: &Vec<u64>, models: &[String]| ids.len() == models.len() && ids.iter().zip(models).all(|(id, m)| index.get(id).map(|&(yi, ei)| self.proj.ymaps[yi].entities[ei].archetype.eq_ignore_ascii_case(m)).unwrap_or(false));
                let wanted_doors: Vec<String> = door_models.iter().filter(|m| !m.is_empty()).cloned().collect();
                if !ok(&c.doors, &wanted_doors) {
                    gone.extend(c.doors.drain(..));
                    for (k, m) in wanted_doors.iter().enumerate() {
                        create.push((hi, ci, true, k, m.clone()));
                    }
                }
                let wanted_ac: Vec<String> = spots.iter().filter(|sp| boxes(sp.0).is_some()).map(|sp| sp.0.to_string()).collect();
                if !ok(&c.ac, &wanted_ac) {
                    gone.extend(c.ac.drain(..));
                    for (k, m) in wanted_ac.iter().enumerate() {
                        create.push((hi, ci, false, k, m.clone()));
                    }
                }
                // placement des objets existants
                let mut di = 0;
                for (i, m) in door_models.iter().enumerate() {
                    if m.is_empty() {
                        continue;
                    }
                    if let (Some(&id), Some((local, ang, _, _))) = (c.doors.get(di), h.door_fit(i)) {
                        moves.push((id, place_at(b, local), b.rot.mul(Quat::from_z(ang)).normalized()));
                    }
                    di += 1;
                }
                for (k, sp) in spots.iter().filter(|sp| boxes(sp.0).is_some()).enumerate() {
                    let Some(&id) = c.ac.get(k) else { continue };
                    let (mn, mx) = boxes(sp.0).unwrap();
                    let q = Quat::from_z(sp.2.y.atan2(sp.2.x) - std::f32::consts::FRAC_PI_2);
                    let cc = v3((mn.x + mx.x) * 0.5, (mn.y + mx.y) * 0.5, 0.0);
                    let local = if sp.3 { sp.1 + sp.2 * ((mx.y - mn.y) * 0.5 + 0.02) - q.rotate(v3(cc.x, cc.y, mx.z)) } else { sp.1 - q.rotate(v3(cc.x, cc.y, mn.z)) };
                    moves.push((id, place_at(b, local), b.rot.mul(q).normalized()));
                }
            }
            self.proj.houses[hi].copies = copies;
        }
        if !gone.is_empty() {
            self.proj.delete_ids(&gone);
        }
        if !create.is_empty() {
            let yi = self.ensure_target_ymap();
            for (hi, ci, door, _, m) in create {
                let mut e = Entity::new(&m, V3::ZERO);
                e.lod_dist = if door { 100.0 } else { 120.0 };
                let id = e.id;
                self.proj.ymaps[yi].entities.push(e);
                let c = &mut self.proj.houses[hi].copies[ci];
                if door { c.doors.push(id) } else { c.ac.push(id) }
            }
            // placés au prochain passage (toutes les 10 images)
        }
        for (id, p, q) in moves {
            if let Some(&(yi, ei)) = index.get(&id) {
                let e = &mut self.proj.ymaps[yi].entities[ei];
                if (e.pos - p).len() > 1e-3 || (e.rot.w - q.w).abs() + (e.rot.z - q.z).abs() + (e.rot.x - q.x).abs() + (e.rot.y - q.y).abs() > 1e-4 {
                    e.pos = p;
                    e.rot = q;
                }
            }
        }
    }

    /// Refait l'intérieur d'un bâtiment (après un changement d'étages, de fenêtres, de cloisons ou d'escalier) en
    /// gardant ses meubles et ses lumières, rangés dans la pièce où ils tombent.
    pub fn house_redo_interior(&mut self, hi: usize) -> String {
        let Some((ti, mi)) = self.house_mlo(hi) else {
            self.proj.houses[hi].interior = None;
            return self.house_to_interior(hi);
        };
        self.push_undo();
        let old = self.proj.ytyps[ti].mlos.remove(mi);
        let shell_prefix = format!("{}_in", self.proj.houses[hi].name);
        let keep: Vec<Entity> = old.entities.iter().filter(|e| { let a = e.archetype.to_lowercase(); !a.starts_with("dw_shell") && !a.starts_with(&shell_prefix) }).cloned().collect();
        let inst: std::collections::HashSet<u64> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.mlo.is_some() && e.archetype.eq_ignore_ascii_case(&old.name)).map(|e| e.id).collect();
        self.proj.mlo_links.retain(|l| !inst.contains(&l.0));
        self.proj.mlo_link_yaw.retain(|l| !inst.contains(&l.0));
        self.proj.delete_ids(&inst);
        self.proj.houses[hi].interior = None;
        self.cur_mlo = None;
        let msg = self.house_to_interior(hi);
        if let Some((ti, mi)) = self.house_mlo(hi) {
            let m = &mut self.proj.ytyps[ti].mlos[mi];
            let rooms = m.rooms.clone();
            m.entities = keep
                .into_iter()
                .map(|mut e| {
                    e.room = (1..rooms.len()).find(|&r| { let x = &rooms[r]; e.pos.x >= x.bb_min.x - 0.1 && e.pos.x <= x.bb_max.x + 0.1 && e.pos.y >= x.bb_min.y - 0.1 && e.pos.y <= x.bb_max.y + 0.1 && e.pos.z >= x.bb_min.z - 0.5 && e.pos.z <= x.bb_max.z + 0.5 }).unwrap_or(1.min(rooms.len().saturating_sub(1)));
                    e
                })
                .collect();
        }
        format!("Intérieur refait (meubles et lumières gardés). {msg}")
    }

    /// Ajoute une porte de garage au bâtiment : la plus grande du jeu qui tient sous le plafond du rez-de-chaussée, au
    /// quart du côté de l'entrée ; à déplacer ensuite d'un clic.
    pub fn house_add_garage(&mut self, hi: usize) -> String {
        let h = self.proj.houses[hi].clone();
        let Some(w) = self.game.world.as_ref() else { return "Charge le jeu pour poser une porte de garage du jeu.".into() };
        let room = h.floor_h - 0.35;
        let best = GARAGE_DOORS.iter().rev().filter_map(|n| w.archetypes.get(&crate::gta::crypto::joaat(n)).map(|a| (*n, a.bb_max - a.bb_min))).find(|(_, d)| d.z <= room && d.x.max(d.y) <= h.edges().get(h.door_edge).map(|e| e.4 - 1.5).unwrap_or(0.0));
        let Some((n, _)) = best else { return "Aucune porte de garage du jeu ne tient sur ce côté sous ce plafond (monte la hauteur d'étage).".into() };
        let t = if h.door_t > 0.5 { 0.25 } else { 0.75 };
        self.proj.houses[hi].more_doors.push(DoorSpec { edge: h.door_edge, t, model: n.to_string(), prop: None, bbox: None, locked: false });
        let i = self.proj.houses[hi].more_doors.len();
        let msg = self.house_set_door(hi, i, Some(n.to_string()));
        format!("Garage : {msg} « Déplacer » puis clic sur un mur pour la mettre ailleurs.")
    }

    /// Crée un bâtiment prêt à l'emploi (PRESETS) au centre de la vue.
    pub fn create_preset(&mut self, k: usize) -> String {
        let (_, w, d) = PRESETS[k.min(PRESETS.len() - 1)];
        let c = self.cam.target;
        let z = self.mouse_ground.map(|g| g.z).unwrap_or(c.z);
        let pts = [v3(c.x - w * 0.5, c.y - d * 0.5, z), v3(c.x + w * 0.5, c.y - d * 0.5, z), v3(c.x + w * 0.5, c.y + d * 0.5, z), v3(c.x - w * 0.5, c.y + d * 0.5, z)];
        let n0 = self.proj.houses.len();
        let msg = self.create_house(&pts);
        if self.proj.houses.len() == n0 {
            return msg;
        }
        let hi = self.proj.houses.len() - 1;
        self.proj.houses[hi].apply_preset(k);
        let mut extra = String::new();
        if k == 0 || k == 3 {
            extra = format!(" {}", self.house_add_garage(hi));
        }
        self.house_doors_follow();
        self.atelier.sig = 0;
        format!("{} « {} » : règle-le ensuite comme tu veux.{extra}", PRESETS[k.min(PRESETS.len() - 1)].0, self.proj.houses[hi].name)
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
        // signature (sérialisation de chaque bâtiment) : à chaque image seulement quand l'atelier est ouvert
        if !self.atelier.open && self.frame % 15 != 0 && self.atelier.sig != 0 {
            return;
        }
        let gsig = std::sync::Arc::as_ptr(&gpu) as usize as u64;
        let sigs: Vec<u64> = self.proj.houses.iter().map(|h| {
            use std::hash::{Hash, Hasher};
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            gsig.hash(&mut hs);
            serde_json::to_string(h).unwrap_or_default().hash(&mut hs);
            hs.finish() | 1
        }).collect();
        if self.atelier.sig == 0 {
            self.atelier.sigs.clear();
        }
        let changed: Vec<usize> = (0..self.proj.houses.len()).filter(|&i| self.atelier.sigs.get(&self.proj.houses[i].name) != Some(&sigs[i])).collect();
        self.atelier.sig = 1;
        if changed.is_empty() {
            return;
        }
        if let Ok(mut g) = gpu.lock() {
            for &i in &changed {
                let h = &self.proj.houses[i];
                let (model, keys, texs) = house_rmodel(h, false);
                g.add_custom(crate::gta::loader::Loaded { arch: crate::gta::crypto::joaat(&h.name), model: Some(model), part_keys: keys, layer_keys: Vec::new(), textures: texs });
                if h.interior.is_some() {
                    for f in 0..h.floors.max(1) {
                        let (m, k, t) = house_inside_rmodel(h, f);
                        g.add_custom(crate::gta::loader::Loaded { arch: crate::gta::crypto::joaat(&h.inside_name(f)), model: Some(m), part_keys: k, layer_keys: Vec::new(), textures: t });
                    }
                }
                self.atelier.sigs.insert(h.name.clone(), sigs[i]);
            }
        }
        let mut boxes: std::collections::HashMap<String, (V3, V3)> = self.proj.houses.iter().map(|h| (h.name.clone(), h.bbox())).collect();
        for h in self.proj.houses.iter().filter(|h| h.interior.is_some()) {
            for f in 0..h.floors.max(1) {
                boxes.insert(h.inside_name(f), h.bbox());
            }
        }
        crate::catalog::set_house_boxes(boxes);
    }

    /// Fenêtre de l'atelier.
    pub fn atelier_window(&mut self, ctx: &egui::Context) {
        if !self.atelier.open {
            return;
        }
        if !ctx.input(|i| i.pointer.any_down()) {
            self.atelier.undo_taken = false;
        }
        // indices de bâtiment des outils encore valables (après une suppression ou une annulation)
        let n = self.proj.houses.len();
        if self.atelier.win_pick.map(|p| p.0 >= n).unwrap_or(false) { self.atelier.win_pick = None; }
        if self.atelier.stair_pick.map(|p| p >= n).unwrap_or(false) { self.atelier.stair_pick = None; }
        if self.atelier.wall_pick.map(|p| p.0 >= n).unwrap_or(false) { self.atelier.wall_pick = None; }
        if self.atelier.light_pick.map(|p| p >= n).unwrap_or(false) { self.atelier.light_pick = None; }
        if self.atelier.door_pick.map(|p| p.0 >= n).unwrap_or(false) { self.atelier.door_pick = None; }
        let mut open = true;
        egui::Window::new("Atelier de bâtiment").open(&mut open).default_width(330.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(ctx.screen_rect().height() * 0.8).show(ui, |ui| {
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
            ui.horizontal_wrapped(|ui| {
                ui.label("Modèles prêts :");
                for (k, (n, w, d)) in PRESETS.iter().enumerate() {
                    if ui.small_button(*n).on_hover_text(format!("{w:.0} × {d:.0} m au centre de la vue, déjà réglé (étages, toit, façade, couleurs, détails) : ajuste-le ensuite")).clicked() {
                        self.status = self.create_preset(k);
                    }
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
                    let nplaces = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.archetype.to_lowercase() == h.name).count();
                    if nplaces > 1 && h.interior.is_some() {
                        ui.label(RichText::new(format!("Posé {nplaces} fois : portes et climatiseurs sur chaque copie, mais l'intérieur seulement sur la première (un intérieur ne se partage pas).")).small().color(Color32::from_rgb(230, 170, 90)));
                    }
                    if self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).any(|e| e.archetype.to_lowercase() == h.name && ((e.scale_xy - 1.0).abs() > 1e-3 || (e.scale_z - 1.0).abs() > 1e-3)) && h.interior.is_some() {
                        ui.label(RichText::new("Bâtiment agrandi : son intérieur, lui, ne s'agrandit pas (le jeu ne met pas d'échelle aux intérieurs) ; change plutôt l'emprise ou les étages.").small().color(Color32::from_rgb(230, 170, 90)));
                    }
                    let mut ch = false;
                    ch |= ui.checkbox(&mut h.decor, "Bâtiment de décoration (on n'y entre pas)").on_hover_text("Portes du jeu fermées à clé, pas de planchers, d'escalier, de cloisons ni d'intérieur : un décor plus léger. Décoche pour pouvoir y entrer.").changed();
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
                    // couleurs et façade par étage
                    egui::CollapsingHeader::new(RichText::new("Couleurs et façade par étage").strong()).id_salt("looks").show(ui, |ui| {
                        let color_row = |ui: &mut egui::Ui, c: &mut Option<[u8; 3]>, id: &str| -> bool {
                            let mut changed = false;
                            ui.horizontal_wrapped(|ui| {
                                if ui.selectable_label(c.is_none(), "sans").clicked() {
                                    *c = None;
                                    changed = true;
                                }
                                for (n, rgb) in COLORS.iter() {
                                    let col = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                                    let r = ui.add(egui::Button::new(RichText::new("■").color(col).size(16.0)).selected(*c == Some(*rgb)).min_size(egui::vec2(20.0, 18.0))).on_hover_text(*n);
                                    if r.clicked() {
                                        *c = Some(*rgb);
                                        changed = true;
                                    }
                                }
                                let mut v = c.unwrap_or([200, 200, 200]);
                                if ui.color_edit_button_srgb(&mut v).on_hover_text(format!("Autre couleur ({id})")).changed() {
                                    *c = Some(v);
                                    changed = true;
                                }
                            });
                            changed
                        };
                        ui.label(RichText::new("Couleur de la façade (textures DragoWalker ; une façade du jeu garde sa couleur)").small());
                        ch |= color_row(ui, &mut h.color, "bâtiment");
                        ui.horizontal(|ui| {
                            ui.label("étage");
                            ui.add(egui::DragValue::new(&mut self.atelier.look_floor).range(0..=h.floors.max(1) - 1));
                            ui.label(RichText::new("(0 = rez-de-chaussée)").small().weak());
                        });
                        let lf = (self.atelier.look_floor as usize).min(h.floors.max(1) as usize - 1);
                        if h.looks.len() <= lf {
                            h.looks.resize(lf + 1, FloorLook::default());
                        }
                        let mut look = h.looks[lf];
                        let cur = if look.game > 0 { format!("{} (jeu)", GAME_TEX[look.game as usize - 1].0) } else { look.facade.map(|i| FACADES[i.min(3) as usize].to_string()).unwrap_or("comme le bâtiment".into()) };
                        egui::ComboBox::from_label("façade de l'étage").selected_text(cur).show_ui(ui, |ui| {
                            if ui.selectable_label(look.facade.is_none() && look.game == 0, "comme le bâtiment").clicked() {
                                look.facade = None;
                                look.game = 0;
                            }
                            for (i, n) in FACADES.iter().enumerate() {
                                if ui.selectable_label(look.facade == Some(i as u8) && look.game == 0, *n).clicked() {
                                    look.facade = Some(i as u8);
                                    look.game = 0;
                                }
                            }
                            for (i, g) in GAME_TEX.iter().enumerate().filter(|g| g.1 .3 == 0) {
                                if ui.selectable_label(look.game == i as u8 + 1, format!("{} (jeu)", g.0)).clicked() {
                                    look.game = i as u8 + 1;
                                }
                            }
                        });
                        ui.label(RichText::new("Couleur de cet étage").small());
                        color_row(ui, &mut look.color, "étage");
                        if look != h.looks[lf] {
                            h.looks[lf] = look;
                            ch = true;
                        }
                        while h.looks.last().map(|l| *l == FloorLook::default()).unwrap_or(false) {
                            h.looks.pop();
                        }
                    });
                    // détails de façade
                    egui::CollapsingHeader::new(RichText::new("Détails de façade").strong()).id_salt("details").show(ui, |ui| {
                        egui::ComboBox::from_label("balcons").selected_text(["aucun", "côté de l'entrée", "tous les côtés"][h.details.balconies.min(2) as usize]).show_ui(ui, |ui| {
                            for (i, n) in ["aucun", "côté de l'entrée", "tous les côtés"].iter().enumerate() {
                                ch |= ui.selectable_value(&mut h.details.balconies, i as u8, *n).changed();
                            }
                        }).response.on_hover_text("Aux étages : chaque fenêtre devient une porte-fenêtre, avec une dalle et un garde-corps (collision comprise)");
                        ui.horizontal(|ui| {
                            ch |= ui.checkbox(&mut h.details.awnings, "auvents").on_hover_text("Au-dessus des portes et des vitrines du rez-de-chaussée").changed();
                            if h.details.awnings {
                                let mut c = Some(h.details.awning_color);
                                ui.vertical(|ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        for (n, rgb) in COLORS.iter() {
                                            if ui.add(egui::Button::new(RichText::new("■").color(Color32::from_rgb(rgb[0], rgb[1], rgb[2])).size(16.0)).selected(c == Some(*rgb))).on_hover_text(*n).clicked() {
                                                c = Some(*rgb);
                                            }
                                        }
                                    });
                                });
                                if let Some(c) = c {
                                    if c != h.details.awning_color {
                                        h.details.awning_color = c;
                                        ch = true;
                                    }
                                }
                            }
                        });
                        ch |= ui.checkbox(&mut h.details.gutters, "gouttières et descentes").changed();
                        ch |= ui.add(egui::Slider::new(&mut h.details.ac, 0.0..=1.0).text("climatiseurs")).on_hover_text("Climatiseurs du jeu sous une partie des fenêtres (et sur un toit plat) : de vrais objets du jeu, qui suivent le bâtiment").changed();
                        ch |= ui.add(egui::Slider::new(&mut h.lift, 0.0..=1.5).step_by(0.05).suffix(" m").text("rez-de-chaussée surélevé")).on_hover_text("Le bâtiment monte sur un soubassement en pierre ; un perron à marches devant chaque porte").changed();
                        if h.details.ac > 0.0 && self.game.world.is_none() {
                            ui.label(RichText::new("Les climatiseurs sont des objets du jeu : charge le jeu pour les voir.").small().weak());
                        }
                    });
                    // escalier
                    egui::CollapsingHeader::new(RichText::new("Escalier").strong()).id_salt("stairs").show(ui, |ui| {
                        egui::ComboBox::from_label("genre").selected_text(STAIR_KINDS[h.stair_kind.min(2) as usize]).show_ui(ui, |ui| {
                            for (i, n) in STAIR_KINDS.iter().enumerate() {
                                ch |= ui.selectable_value(&mut h.stair_kind, i as u8, *n).changed();
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            let on = self.atelier.stair_pick.is_some();
                            if ui.selectable_label(on, "Placer d'un clic").on_hover_text("Puis clic sur le sol du rez-de-chaussée : l'escalier est centré sur ce point, au même endroit à chaque étage. Échap pour annuler.").clicked() {
                                self.atelier.stair_pick = if on { None } else { Some(hi) };
                            }
                            if ui.button("Tourner").on_hover_text("Un quart de tour (escalier placé d'un clic)").clicked() {
                                h.stair_rot = (h.stair_rot + 1) % 4;
                                ch = true;
                            }
                            if h.stair_at.is_some() && ui.button("Placement automatique").on_hover_text("Le long d'un mur, choisi par DragoWalker").clicked() {
                                h.stair_at = None;
                                ch = true;
                            }
                        });
                        if h.floors > 1 && h.stair().is_none() {
                            ui.label(RichText::new("Cet escalier ne tient pas dans le bâtiment : essaie un autre genre, un autre endroit ou tourne-le.").small().color(Color32::from_rgb(230, 150, 90)));
                        }
                        if h.roof == 0 {
                            ch |= ui.checkbox(&mut h.roof_access, "monter sur le toit-terrasse").on_hover_text("L'escalier continue jusqu'au toit, qui a un petit édicule de sortie avec une ouverture").changed();
                        }
                    });
                    // portes : modèle du jeu au choix, placées d'un clic sur un mur
                    ui.label(RichText::new("Portes").strong());
                    let models = self.door_models();
                    let mut set_model: Option<(usize, String)> = None;
                    let mut remove: Option<usize> = None;
                    let mut set_locked: Option<(usize, bool)> = None;
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
                            let mut lk = d.locked || h.decor;
                            if ui.add_enabled(!h.decor, egui::Checkbox::new(&mut lk, "fermée à clé")).on_hover_text("En jeu, cette porte ne s'ouvre pas").changed() {
                                set_locked = Some((i, lk));
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
                    if ui.button("+ Porte de garage").on_hover_text("Une porte de garage basculante du jeu (la plus grande qui tient sous le plafond), qui s'ouvre en jeu ; puis « Déplacer » pour la mettre où tu veux").clicked() {
                        self.push_undo();
                        self.status = self.house_add_garage(hi);
                        h = self.proj.houses[hi].clone();
                    }
                    ch |= ui.add(egui::Slider::new(&mut h.lit, 0.0..=1.0).text("fenêtres éclairées la nuit")).on_hover_text("Part des vitres qui s'allument la nuit en jeu (matériau du jeu qui brille la nuit) ; dans la vue, elles ont une teinte chaude.").changed();
                    if ch {
                        // un pas d'annulation par geste (un curseur glissé change à chaque image) ; push_undo marque
                        // aussi le projet modifié (sauvegarde automatique, question à la fermeture)
                        if !self.atelier.undo_taken {
                            self.push_undo();
                            self.atelier.undo_taken = true;
                        }
                        self.proj.houses[hi] = h.clone();
                    }
                    if let Some((i, n)) = set_model {
                        self.push_undo();
                        self.status = self.house_set_door(hi, i, Some(n));
                    }
                    if let Some((i, lk)) = set_locked {
                        self.push_undo();
                        if let Some(mut d) = self.proj.houses[hi].doors().get(i).cloned() {
                            d.locked = lk;
                            self.proj.houses[hi].put_door(i, d);
                        }
                        h = self.proj.houses[hi].clone();
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
                                        let zf = self.proj.houses[hi].floor_room(self.atelier.floor).0.z;
                                        let rooms = &self.proj.ytyps[ti].mlos[mi].rooms;
                                        let room = (1..rooms.len()).find(|&r| (rooms[r].bb_min.z - zf).abs() < 0.3).unwrap_or((self.atelier.floor as usize + 1).min(rooms.len().saturating_sub(1)));
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
                        if ui.add_enabled(!has_int && !h.decor, egui::Button::new("Transformer en intérieur")).on_hover_text("Une pièce par étage (coupée par les cloisons qui la traversent), fenêtres et porte en portails vers l'extérieur (on voit dehors), escalier et trémie entre les étages, vraie porte du jeu à l'entrée, meubles, lumières et ambiance.").clicked() {
                            self.status = self.house_to_interior(hi);
                        }
                        if has_int {
                            ui.label(RichText::new(format!("intérieur : {}", h.interior.clone().unwrap_or_default())).small());
                            if ui.button("Refaire l'intérieur").on_hover_text("Après un changement d'étages, de fenêtres, de cloisons ou d'escalier : pièces et passages refaits, meubles et lumières gardés").clicked() {
                                self.status = self.house_redo_interior(hi);
                            }
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
                    let gone = self.proj.houses.remove(i);
                    let name = gone.name.clone();
                    let mut ids: std::collections::HashSet<u64> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.archetype.to_lowercase() == name).map(|e| e.id).collect();
                    ids.extend(gone.attached.iter().copied());
                    ids.extend(gone.doors().iter().filter_map(|d| d.prop));
                    for c in &gone.copies {
                        ids.extend(c.doors.iter().copied());
                        ids.extend(c.ac.iter().copied());
                    }
                    // son intérieur : type, placement et lien
                    if let Some(n) = gone.interior.clone() {
                        for t in self.proj.ytyps.iter_mut() {
                            t.mlos.retain(|m| !m.name.eq_ignore_ascii_case(&n));
                        }
                        let inst: Vec<u64> = self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).filter(|e| e.mlo.is_some() && e.archetype.eq_ignore_ascii_case(&n)).map(|e| e.id).collect();
                        self.proj.mlo_links.retain(|l| !inst.contains(&l.0));
                        self.proj.mlo_link_yaw.retain(|l| !inst.contains(&l.0));
                        ids.extend(inst);
                        self.cur_mlo = None;
                    }
                    self.proj.delete_ids(&ids);
                    self.clean_selection();
                    self.sync_game_project();
                }
            }
            });
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
            // pas les fichiers de l'intérieur (« dw_bat_N_int… ») : sa collision de pièces est écrite juste avant
            // (export_interior_bounds) ; l'effacer laissait l'intérieur sans pièces en jeu (relecture du 13 nov. 2026)
            if (n.starts_with(HOUSE_PREFIX) || n.starts_with("dw_house_types")) && !n.contains("_int") && n.ends_with(".xml") && f.path().is_file() {
                let _ = std::fs::remove_file(f.path());
            }
        }
        let names = crate::terrain::material_names(self.game.world.as_deref());
        let concrete = crate::terrain::coll_type(&names, "CONCRETE", 1);
        // 0 murs, 1 vitres, 2 escaliers et perrons (béton marqué « escalier » du jeu : FLAG_STAIRS)
        let ctypes = [crate::terrain::coll_type(&names, "BRICK", concrete), crate::terrain::coll_type(&names, "GLASS_SHOOT_THROUGH", concrete), concrete | crate::roads::MAT_STAIRS];
        let mut archs = Vec::new();
        let (mut n, mut ncol, mut nlit) = (0, 0, 0);
        if self.proj.houses.iter().any(|h| self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).any(|e| e.archetype.to_lowercase() == h.name)) {
            // textures DragoWalker, plus les textures teintes des bâtiments posés (couleurs de façade, auvents)
            let mut all = house_textures().to_vec();
            for h in self.proj.houses.iter().filter(|h| self.proj.ymaps.iter().flat_map(|y| y.entities.iter()).any(|e| e.archetype.to_lowercase() == h.name)) {
                let mut models = vec![house_rmodel(h, false).2, house_rmodel(h, true).2];
                if h.interior.is_some() {
                    for f in 0..h.floors.max(1) {
                        models.push(house_inside_rmodel(h, f).2);
                    }
                }
                for m in models {
                    for t in m {
                        if tint_of(t.0).is_some() && !all.iter().any(|x| x.0 == t.0) {
                            all.push(t);
                        }
                    }
                }
            }
            if let Err(e) = crate::sollumz::write_ytd_xml(stream, HOUSE_TXD, &all) {
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
            // coque intérieure de chaque étage (bâtiment transformé en intérieur)
            if h.interior.is_some() {
                for f in 0..h.floors.max(1) {
                    let nm = h.inside_name(f);
                    let (im, ik, it) = house_inside_rmodel(h, f);
                    if im.geoms.is_empty() {
                        continue;
                    }
                    let _ = crate::sollumz::write_ydr_xml_ex(stream, &nm, &im, &ik, &it, Some(HOUSE_TXD));
                    let (mn, mx) = h.bbox();
                    let (c, r) = ((mn + mx) * 0.5, (mx - mn).len() * 0.5);
                    archs.push(format!(
                        "  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"{HOUSE_LOD}\" />\n   <flags value=\"0\" />\n   <specialAttribute value=\"0\" />\n   <bbMin x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bbMax x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsCentre x=\"{}\" y=\"{}\" z=\"{}\" />\n   <bsRadius value=\"{r}\" />\n   <hdTextureDist value=\"60\" />\n   <name>{nm}</name>\n   <textureDictionary>{HOUSE_TXD}</textureDictionary>\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>ASSET_TYPE_DRAWABLE</assetType>\n   <assetName>{nm}</assetName>\n   <extensions />\n  </Item>",
                        mn.x, mn.y, mn.z, mx.x, mx.y, mx.z, c.x, c.y, c.z
                    ));
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
                let world: Vec<V3> = pos.iter().map(|p| place_at(e, *p)).collect();
                let mut tris = Vec::new();
                for (_, list, m) in &parts {
                    if *m == C_NONE {
                        continue; // salissures, marches : pas de collision
                    }
                    // côtés des balcons : collision seulement
                    let mm = if *m == C_INVISIBLE { C_WALL } else { *m };
                    for c in list.chunks_exact(3) {
                        tris.push((c[0], c[1], c[2], mm));
                    }
                }
                let mut world = world;
                for q in h.stair_ramps() {
                    let b = world.len() as u32;
                    world.extend(q.iter().map(|p| place_at(e, *p)));
                    tris.push((b, b + 1, b + 2, C_STAIRS));
                    tris.push((b, b + 2, b + 3, C_STAIRS));
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
    /// 0 droit, 1 en L (palier puis quart de tournant vers l'intérieur), 2 en colimaçon
    pub kind: u8,
    /// longueur (le long du mur) et profondeur de l'emprise de l'escalier
    pub len: f32,
    pub depth: f32,
    /// placé à l'endroit choisi d'un clic (sinon placé tout seul)
    pub clicked: bool,
}

/// Escaliers proposés.
pub const STAIR_KINDS: [&str; 3] = ["Droit", "En L (avec palier)", "En colimaçon"];
/// Rayon d'un escalier en colimaçon, et de son fût central (m) — **estimé**.
const SPIRAL_R: f32 = 1.05;
const SPIRAL_CORE: f32 = 0.12;
/// Balcon : profondeur, débord de chaque côté de la porte-fenêtre ; hauteur du garde-corps ; avancée d'un auvent ;
/// profondeur du palier d'un perron (m) — **estimé**.
const BALCONY_D: f32 = 1.1;
const BALCONY_SIDE: f32 = 0.5;
const RAIL_H: f32 = 1.0;
const AWNING_D: f32 = 1.3;
const STOOP_D: f32 = 1.2;
/// Pente d'une rampe de garage (montée par mètre) — **estimé**.
const GARAGE_SLOPE: f32 = 0.15;
/// Collision sans rien à dessiner (côtés des balcons) : à l'export, collision seulement.
pub const C_INVISIBLE: u8 = 254;
/// Dessus du toit plat au-dessus de la corniche (m).
pub const ROOF_TOP: f32 = 0.05;
/// Édicule de sortie sur le toit-terrasse : hauteur, ouverture (largeur, hauteur) (m) — **estimé**.
const HUT_H: f32 = 2.5;
const HUT_DOOR_W: f32 = 1.0;
const HUT_DOOR_H: f32 = 2.1;

impl Stair {
    /// Taille de l'emprise (longueur, profondeur) d'un escalier de `n` marches de `run` m.
    pub fn size(kind: u8, n: usize, run: f32) -> (f32, f32) {
        match kind {
            1 => {
                let n1 = n / 2;
                let n2 = n - n1 - 1;
                (n1 as f32 * run + STAIR_W, STAIR_W + n2 as f32 * run)
            }
            2 => (2.0 * SPIRAL_R, 2.0 * SPIRAL_R),
            _ => (n as f32 * run, STAIR_W),
        }
    }

    /// Escalier commençant à (s0, w0) du repère (a, dir, inw) ; trémie : pour un escalier droit, dès la première
    /// marche où il reste moins de STAIR_HEADROOM sous le plancher (sinon le personnage s'y cogne et ne monte pas :
    /// retour de la personne, 13 nov. 2026) ; pour les autres, toute l'emprise.
    #[allow(clippy::too_many_arguments)]
    fn new(h: &House, kind: u8, a: V3, dir: V3, inw: V3, s0: f32, w0: f32, n: usize, run: f32) -> Stair {
        let (len, depth) = Stair::size(kind, n, run);
        let hole = if kind == 0 {
            let rise = h.floor_h / n as f32;
            let k = ((h.floor_h - SLAB_T - STAIR_HEADROOM) / rise).floor().max(0.0);
            ((s0 + k * run - STAIR_HOLE_MARGIN).max(s0), s0 + len)
        } else {
            (s0, s0 + len)
        };
        Stair { a, dir, inw, hole, w: (w0, w0 + depth), start: s0, n, run, kind, len, depth, clicked: false }
    }

    /// Coins et milieux de l'emprise (pour vérifier qu'elle tient dans les murs).
    pub fn corners(&self) -> Vec<V3> {
        let (s0, s1, w0, w1) = (self.start, self.start + self.len, self.w.0, self.w.1);
        let (sm, wm) = ((s0 + s1) * 0.5, (w0 + w1) * 0.5);
        [(s0, w0), (s1, w0), (s1, w1), (s0, w1), (sm, w1), (sm, wm), (s0, wm), (s1, wm), (sm, w0)].iter().map(|&(s, w)| self.at(s, w, 0.0)).collect()
    }

    /// Marches, de bas en haut : (contour [arrière gauche, arrière droit, avant droit, avant gauche] en (s, w),
    /// palier ?). L'arrière d'une marche est sa contremarche.
    pub fn steps(&self) -> Vec<([[f32; 2]; 4], bool)> {
        let (s0, w0, run, n) = (self.start, self.w.0, self.run, self.n);
        let mut v = Vec::with_capacity(n);
        match self.kind {
            1 => {
                let n1 = n / 2;
                for i in 0..n1 {
                    let (a, b) = (s0 + i as f32 * run, s0 + (i + 1) as f32 * run);
                    v.push(([[a, w0], [a, w0 + STAIR_W], [b, w0 + STAIR_W], [b, w0]], false));
                }
                let sl = s0 + n1 as f32 * run;
                v.push(([[sl, w0], [sl, w0 + STAIR_W], [sl + STAIR_W, w0 + STAIR_W], [sl + STAIR_W, w0]], true));
                for j in 0..n - n1 - 1 {
                    let (a, b) = (w0 + STAIR_W + j as f32 * run, w0 + STAIR_W + (j + 1) as f32 * run);
                    v.push(([[sl + STAIR_W, a], [sl, a], [sl, b], [sl + STAIR_W, b]], false));
                }
            }
            2 => {
                let (cs, cw) = (s0 + SPIRAL_R, w0 + SPIRAL_R);
                let step = std::f32::consts::TAU * 0.9 / n as f32;
                let pt = |ang: f32, r: f32| [cs + r * ang.cos(), cw + r * ang.sin()];
                for i in 0..n {
                    let (a0, a1) = (-std::f32::consts::FRAC_PI_2 + i as f32 * step, -std::f32::consts::FRAC_PI_2 + (i + 1) as f32 * step);
                    v.push(([pt(a0, SPIRAL_CORE), pt(a0, SPIRAL_R - 0.03), pt(a1, SPIRAL_R - 0.03), pt(a1, SPIRAL_CORE)], false));
                }
            }
            _ => {
                for i in 0..n {
                    let (a, b) = (s0 + i as f32 * run, s0 + (i + 1) as f32 * run);
                    v.push(([[a, w0], [a, w0 + STAIR_W], [b, w0 + STAIR_W], [b, w0]], false));
                }
            }
        }
        v
    }

    /// Sens de la montée au bout de l'escalier (repère du bâtiment).
    pub fn exit_dir(&self) -> V3 {
        let st = self.steps();
        let Some((q, _)) = st.last() else { return self.dir };
        let back = [(q[0][0] + q[1][0]) * 0.5, (q[0][1] + q[1][1]) * 0.5];
        let front = [(q[2][0] + q[3][0]) * 0.5, (q[2][1] + q[3][1]) * 0.5];
        let d = self.at(front[0], front[1], 0.0) - self.at(back[0], back[1], 0.0);
        if d.len() < 1e-4 { self.dir } else { d.norm() }
    }

    /// Point (le long du mur, depuis le mur, hauteur), repère du bâtiment.
    pub fn at(&self, s: f32, w: f32, z: f32) -> V3 {
        let p = self.a + self.dir * s + self.inw * w;
        v3(p.x, p.y, z)
    }

    /// Rectangles de la trémie (s0, s1, w0, w1) : un pour un escalier droit ou en colimaçon, deux pour un escalier
    /// en L (la volée et son palier, puis la seconde volée : le creux du L reste plancher).
    pub fn rects(&self) -> Vec<(f32, f32, f32, f32)> {
        match self.kind {
            1 => {
                let sl = self.start + (self.n / 2) as f32 * self.run;
                vec![(self.start, sl + STAIR_W, self.w.0, self.w.0 + STAIR_W), (sl, sl + STAIR_W, self.w.0 + STAIR_W, self.w.1)]
            }
            _ => vec![(self.hole.0, self.hole.1, self.w.0, self.w.1)],
        }
    }

    /// Coins d'un rectangle de la trémie à la hauteur z.
    pub fn rect_corners(&self, r: (f32, f32, f32, f32), z: f32) -> [V3; 4] {
        [self.at(r.0, r.2, z), self.at(r.1, r.2, z), self.at(r.1, r.3, z), self.at(r.0, r.3, z)]
    }

    /// Bords de la trémie (début, fin, direction vers l'intérieur de la trémie ; repère du bâtiment, z = 0) : les
    /// côtés des rectangles, sans les morceaux communs à deux rectangles (sinon un bout de dalle barrait le passage
    /// entre le palier et la seconde volée du L).
    pub fn hole_edges(&self) -> Vec<(V3, V3, V3)> {
        let rs = self.rects();
        let mut out = Vec::new();
        for (i, r) in rs.iter().enumerate() {
            // (axe constant : 0 = s, 1 = w ; valeur ; début, fin le long ; sens vers l'intérieur du rectangle)
            let sides = [(1u8, r.2, r.0, r.1, 1.0f32), (1, r.3, r.0, r.1, -1.0), (0, r.0, r.2, r.3, 1.0), (0, r.1, r.2, r.3, -1.0)];
            for (ax, val, a0, a1, sg) in sides {
                let mut segs = vec![(a0, a1)];
                for (j, o) in rs.iter().enumerate() {
                    if i == j {
                        continue;
                    }
                    // l'autre rectangle touche ce côté de l'autre bord
                    let (o_val, o_a0, o_a1) = if ax == 1 { (if sg > 0.0 { o.3 } else { o.2 }, o.0, o.1) } else { (if sg > 0.0 { o.1 } else { o.0 }, o.2, o.3) };
                    if (o_val - val).abs() > 1e-3 {
                        continue;
                    }
                    segs = segs.into_iter().flat_map(|(x0, x1)| {
                        let mut v = Vec::new();
                        if o_a0 > x0 { v.push((x0, o_a0.min(x1))); }
                        if o_a1 < x1 { v.push((o_a1.max(x0), x1)); }
                        v.into_iter().filter(|s| s.1 - s.0 > 0.01).collect::<Vec<_>>()
                    }).collect();
                }
                for (x0, x1) in segs {
                    let (p, q, n) = if ax == 1 { (self.at(x0, val, 0.0), self.at(x1, val, 0.0), self.inw * sg) } else { (self.at(val, x0, 0.0), self.at(val, x1, 0.0), self.dir * sg) };
                    out.push((p, q, n));
                }
            }
        }
        out
    }

    /// Coins de la trémie à la hauteur z.
    pub fn hole_corners(&self, z: f32) -> [V3; 4] {
        [self.at(self.hole.0, self.w.0, z), self.at(self.hole.1, self.w.0, z), self.at(self.hole.1, self.w.1, z), self.at(self.hole.0, self.w.1, z)]
    }
}

/// Escalier entre deux étages d'un bâtiment de l'atelier : largeur ; hauteur libre au-dessus d'une marche sans
/// trémie, et marge de la trémie vers le bas de l'escalier (le personnage se penche en montant) — **estimé**.
const STAIR_W: f32 = 1.2;
const STAIR_HEADROOM: f32 = 2.2;
const STAIR_HOLE_MARGIN: f32 = 0.4;

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
        // pièces : une par étage, coupée par les cloisons droites qui la traversent (chaque pièce a son son et sa
        // lumière) ; portails entre elles au passage des cloisons et aux bouts laissés ouverts
        let (rooms, inner) = h.interior_rooms();
        let room_at = |p: V3| -> u32 {
            rooms.iter().position(|(_, lo, hi2)| p.x >= lo.x - 0.05 && p.x <= hi2.x + 0.05 && p.y >= lo.y - 0.05 && p.y <= hi2.y + 0.05 && p.z >= lo.z - 0.3 && p.z <= hi2.z + 0.3).map(|i| i as u32 + 1).unwrap_or(0)
        };
        let mut portals: Vec<(u32, u32, [V3; 4])> = Vec::new();
        for c in &inner {
            let ctr = (c[0] + c[1] + c[2] + c[3]) * 0.25;
            let n = (c[1] - c[0]).cross(c[3] - c[0]).norm();
            let (ra, rb) = (room_at(ctr + n * 0.3), room_at(ctr - n * 0.3));
            if ra != 0 && rb != 0 && ra != rb {
                portals.push((ra, rb, *c));
            }
        }
        // portails : fenêtres et portes (vers l'extérieur), sur la face intérieure des murs ; un seul portail par mur
        // et par pièce (rectangle qui englobe ses ouvertures) : une fenêtre = un portail en donnait ~70 par immeuble
        // pour 200 dans le jeu. Les murs du bâtiment restent entre les fenêtres (la collision des pièces d'un
        // bâtiment de l'atelier n'a pas de murs, voir house_room_collision)
        let mut win_groups: std::collections::BTreeMap<(usize, u32), Vec<[V3; 4]>> = std::collections::BTreeMap::new();
        let edges = h.edges();
        for (e, &(a, _, dir, out, _)) in edges.iter().enumerate() {
            let at = |s: f32, zz: f32| {
                let p = a + dir * s - out * WALL_T;
                v3(p.x, p.y, zz + h.lift)
            };
            for f in 0..h.floors.max(1) {
                // seulement les portes : la coque intérieure montre les fenêtres de dedans (vitres opaques), elles n'ont
                // plus besoin de passage
                for ho in h.holes(e, f).into_iter().filter(|ho| ho.kind == 1) {
                    let c = [at(ho.s0, ho.z0), at(ho.s0, ho.z1), at(ho.s1, ho.z1), at(ho.s1, ho.z0)];
                    let mid = (c[0] + c[2]) * 0.5 - out * 0.4;
                    let mut r = room_at(mid);
                    if r == 0 {
                        // dans l'interstice d'une cloison : la pièce la plus proche à cette hauteur
                        r = rooms.iter().enumerate().filter(|(_, x)| mid.z >= x.1.z - 0.3 && mid.z <= x.2.z + 0.3).min_by(|a, b| {
                            let d = |x: &(String, V3, V3)| v3(mid.x.clamp(x.1.x, x.2.x) - mid.x, mid.y.clamp(x.1.y, x.2.y) - mid.y, 0.0).len();
                            d(a.1).partial_cmp(&d(b.1)).unwrap_or(std::cmp::Ordering::Equal)
                        }).map(|(i, _)| i as u32 + 1).unwrap_or(0);
                    }
                    if r != 0 {
                        // une porte = un passage (pas réunies : deux portes d'un même mur restent séparées)
                        win_groups.entry((e * 1000 + win_groups.len(), r)).or_default().push(c);
                    }
                }
            }
        }
        for ((_, r), cs) in &win_groups {
            let (a, b) = (cs[0][0], cs[0][3]);
            let d = (b - a).norm();
            let along = |p: V3| (p - a).dot(d);
            let (mut s0, mut s1, mut z0, mut z1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for c in cs {
                for p in c {
                    s0 = s0.min(along(*p));
                    s1 = s1.max(along(*p));
                    z0 = z0.min(p.z);
                    z1 = z1.max(p.z);
                }
            }
            let at = |t: f32, zz: f32| { let q = a + d * t; v3(q.x, q.y, zz) };
            portals.push((*r, 0, [at(s0, z0), at(s0, z1), at(s1, z1), at(s1, z0)]));
        }
        // trémies entre étages (et vers le toit-terrasse)
        let mut n_stairs = 0;
        for (st, r) in h.stair().map(|st| st.rects().into_iter().map(move |r| (st, r)).collect::<Vec<_>>()).unwrap_or_default() {
            let hc = st.rect_corners(r, 0.0);
            let ctr = (hc[0] + hc[2]) * 0.5;
            for f in 0..h.floors.max(1) {
                let (lo, hi2) = h.floor_room(f);
                let below = room_at(v3(ctr.x, ctr.y, (lo.z + hi2.z) * 0.5));
                let up_room = if f + 1 < h.floors.max(1) {
                    let (l2, h2) = h.floor_room(f + 1);
                    room_at(v3(ctr.x, ctr.y, (l2.z + h2.z) * 0.5))
                } else if h.roof_access && h.roof == 0 {
                    0
                } else {
                    continue;
                };
                if below != 0 {
                    portals.push((below, up_room, st.rect_corners(r, hi2.z)));
                    n_stairs += 1;
                }
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
        inst.flags = crate::mech::MLO_INSTANCE_FLAGS;
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
        // les portes restent des objets du monde, ajustées à l'ouverture (comme avant la transformation) : une porte
        // mise dans l'intérieur ne s'affichait pas en jeu (retour de la personne, 13 nov. 2026) ; une porte manquante
        // est posée
        let mut placed_doors: Vec<String> = Vec::new();
        let mut hh = h.clone();
        hh.interior = Some(name.clone());
        self.proj.houses[hi] = hh.clone();
        for i in 0..hh.doors().len() {
            let ok = hh.doors()[i].prop.map(|p| self.proj.ymaps.iter().any(|y| y.entities.iter().any(|e| e.id == p))).unwrap_or(false);
            if !ok {
                let _ = self.house_set_door(hi, i, None);
            }
            if let Some(d) = self.proj.houses[hi].doors().get(i) {
                if d.prop.is_some() {
                    placed_doors.push(d.model.clone());
                }
            }
        }
        self.house_doors_follow();
        let door_txt = if placed_doors.is_empty() { "aucune porte du jeu trouvée (jeu non chargé ?)".to_string() } else { format!("porte(s) {}", placed_doors.join(", ")) };
        self.opts.show_rooms = true;
        self.atelier.sig = 0;
        let mut msg = format!(
            "« {name} » : {} pièce(s) (par étage, et selon les cloisons), {} fenêtre(s) et porte(s) en portails vers l'extérieur, {n_stairs} escalier(s) avec trémie, {door_txt}, {n_lights} lumière(s). Chaque pièce a sa coque et sa collision ; l'intérieur suit le bâtiment.",
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

    /// Escalier praticable : assez de hauteur libre au-dessus de chaque marche hors trémie, rampe continue du sol de
    /// l'étage au plancher du dessus.
    #[test]
    fn escalier_praticable() {
        for fh in [2.6f32, 3.2, 4.0, 6.0] {
            let mut h = House::new("dw_bat_t", vec![[0.0, 0.0], [12.0, 0.0], [12.0, 9.0], [0.0, 9.0]]);
            h.floors = 2;
            h.floor_h = fh;
            let st = h.stair().expect("escalier");
            let rise = fh / st.n as f32;
            for i in 0..st.n {
                let front = st.start + i as f32 * st.run;
                if front + st.run + STAIR_HOLE_MARGIN <= st.hole.0 + 1e-3 {
                    assert!(fh - SLAB_T - (i + 1) as f32 * rise >= STAIR_HEADROOM - 1e-3, "marche {i} trop basse sous le plancher ({fh} m)");
                }
            }
            let r = h.stair_ramps();
            let zs: Vec<f32> = r.iter().flat_map(|q| q.iter().map(|p| p.z)).collect();
            assert!((zs.iter().cloned().fold(f32::MAX, f32::min) - FLOOR_LIFT).abs() < 1e-4 && (zs.iter().cloned().fold(0.0, f32::max) - fh).abs() < 1e-4);
            for q in &r {
                // pente de chaque morceau de rampe, de l'arrière à l'avant
                let d = (q[2] + q[3]) * 0.5 - (q[0] + q[1]) * 0.5;
                let slope = d.z.abs() / (d.x * d.x + d.y * d.y).sqrt().max(1e-4);
                assert!(slope < 0.75, "rampe trop raide : {slope}");
            }
        }
    }

    /// Les trois genres d'escalier, placés tout seuls ou d'un clic : dans les murs, marches qui montent régulièrement
    /// jusqu'au plancher, rampe continue ; avec la sortie sur le toit, une volée de plus.
    #[test]
    fn escaliers_au_choix() {
        for kind in 0..3u8 {
            for at in [None, Some([1.0f32, 0.5])] {
                let mut h = House::new("dw_bat_t", vec![[-7.0, -5.0], [7.0, -5.0], [7.0, 5.0], [-7.0, 5.0]]);
                h.floors = 3;
                h.stair_kind = kind;
                h.stair_at = at;
                h.stair_rot = 1;
                let st = h.stair().unwrap_or_else(|| panic!("escalier {kind} {at:?}"));
                for q in st.corners() {
                    assert!(h.inside_walls(q), "escalier {kind} hors des murs");
                }
                assert_eq!(st.steps().len(), st.n);
                let ramps = h.stair_ramps();
                assert!(!ramps.is_empty());
                let top = ramps.iter().flat_map(|q| q.iter().map(|p| p.z)).fold(0.0f32, f32::max);
                assert!((top - 2.0 * h.floor_h).abs() < 1e-3, "haut des rampes {top}");
                h.roof = 0;
                h.roof_access = true;
                assert_eq!(h.stair_levels().len(), 3);
                let (pos, _, _) = h.mesh(false);
                assert!(pos.iter().any(|p| p.z > h.height() + 2.0), "édicule sur le toit");
            }
        }
    }

    /// Escalier en L : deux rectangles de trémie (le creux du L reste plancher), pas de bord entre eux, garde-corps
    /// sans le côté d'arrivée ; collision des pièces sans murs, dans l'emprise, percée aux trémies.
    #[test]
    fn tremies_et_collision_des_pieces() {
        let mut h = House::new("dw_bat_t", vec![[-7.0, -5.0], [7.0, -5.0], [7.0, 5.0], [-7.0, 5.0]]);
        h.floors = 2;
        h.stair_kind = 1;
        let st = h.stair().expect("escalier en L");
        assert_eq!(st.rects().len(), 2);
        let edges = st.hole_edges();
        assert_eq!(edges.len(), 7, "contour en L : 6 côtés dont un en deux morceaux ({})", edges.len());
        let rails = h.stair_rails(&st);
        assert!(!rails.is_empty() && rails.len() < edges.len(), "garde-corps sans l'arrivée ni le mur");
        // pente des rampes : jamais plus d'une marche par marche
        for q in h.stair_ramps() {
            let d = (q[2] + q[3]) * 0.5 - (q[0] + q[1]) * 0.5;
            assert!(d.z.abs() / (d.x * d.x + d.y * d.y).sqrt().max(1e-3) < 0.8);
        }
        let (rooms, _) = h.interior_rooms();
        let mut mr = vec![crate::model::Room::new("limbo", V3::ZERO, V3::ZERO)];
        for (n, lo, hi) in &rooms {
            mr.push(crate::model::Room::new(n, *lo, *hi));
        }
        let col = h.room_collision(&mr);
        assert!(!col.is_empty());
        assert!(col.iter().all(|(_, t)| (t[0].z - t[1].z).abs() < 1e-4 && (t[0].z - t[2].z).abs() < 1e-4), "pas de murs");
        // maison en L : rien hors de l'emprise
        let mut l = House::new("dw_bat_l", vec![[0.0, 0.0], [12.0, 0.0], [12.0, 5.0], [5.0, 5.0], [5.0, 10.0], [0.0, 10.0]]);
        l.floors = 1;
        let (rooms, _) = l.interior_rooms();
        let mut mr = vec![crate::model::Room::new("limbo", V3::ZERO, V3::ZERO)];
        for (n, lo, hi) in &rooms {
            mr.push(crate::model::Room::new(n, *lo, *hi));
        }
        for (_, t) in l.room_collision(&mr) {
            let c = (t[0] + t[1] + t[2]) * (1.0 / 3.0);
            assert!(!(c.x > 5.2 && c.y > 5.2), "sol dans le creux du L : {c:?}");
        }
        // angle rentrant comblé : le modèle a des faces au coin (5, 5) côté intérieur
        let (pos, _, _) = l.mesh(false);
        assert!(pos.iter().any(|p| (p.x - (5.0 - WALL_T)).abs() < 1e-3 && (p.y - (5.0 - WALL_T)).abs() < 1e-3));
        // colimaçon : palier d'arrivée et garde-corps
        h.stair_kind = 2;
        let st = h.stair().expect("colimaçon");
        assert!(!House::spiral_landing(&st).is_empty());
        assert!(!h.stair_rails(&st).is_empty());
    }

    /// Bâtiment de décoration : portes fermées à clé, rien dedans (moins de triangles), pas d'escalier.
    #[test]
    fn batiment_de_decoration() {
        let mut h = House::new("dw_bat_t", vec![[-6.0, -4.0], [6.0, -4.0], [6.0, 4.0], [-6.0, 4.0]]);
        h.floors = 3;
        let full: usize = h.mesh(false).2.iter().map(|p| p.1.len()).sum();
        assert!(!h.door_locked_at(0) && h.stair().is_some());
        h.decor = true;
        let light: usize = h.mesh(false).2.iter().map(|p| p.1.len()).sum();
        assert!(h.door_locked_at(0) && h.stair().is_none() && h.stair_ramps().is_empty());
        assert!(light * 10 < full * 6, "décor plus léger : {light} / {full}");
    }

    /// Coque intérieure : chaque étage a la sienne, dans l'emprise et à sa hauteur, sans les faces de la façade.
    #[test]
    fn coque_interieure_par_etage() {
        let mut h = House::new("dw_bat_t", vec![[-7.0, -5.0], [7.0, -5.0], [7.0, 5.0], [-7.0, 5.0]]);
        h.apply_preset(1);
        h.lift = 0.5;
        let total: usize = (0..h.floors).map(|f| h.inside_mesh(f).2.iter().map(|p| p.1.len()).sum::<usize>()).sum();
        assert!(total > 0);
        for f in 0..h.floors {
            let (pos, _, parts) = h.inside_mesh(f);
            assert!(!parts.is_empty(), "étage {f} vide");
            let z0 = h.lift + f as f32 * h.floor_h - 0.05;
            let z1 = h.lift + (f + 1) as f32 * h.floor_h + 0.05;
            for p in &pos {
                assert!(p.x.abs() <= 7.0 && p.y.abs() <= 5.0, "hors de l'emprise {p:?}");
                assert!(p.z >= z0 - 0.3 && p.z <= z1 + 1.2, "étage {f} : {p:?}");
            }
            assert!(parts.iter().all(|p| game_tex_of(p.0).is_none()), "pas de texture du jeu dans la coque");
        }
    }

    /// Une cloison qui traverse l'étage le coupe en deux pièces, avec un portail à son passage.
    #[test]
    fn cloisons_en_pieces() {
        let mut h = House::new("dw_bat_t", vec![[-6.0, -4.0], [6.0, -4.0], [6.0, 4.0], [-6.0, 4.0]]);
        h.floors = 2;
        h.walls.push(Partition { floor: 0, a: [0.0, -3.7], b: [0.0, 3.7], door: true });
        h.walls.push(Partition { floor: 1, a: [-1.0, 0.0], b: [1.0, 0.0], door: false });
        let (rooms, inner) = h.interior_rooms();
        assert_eq!(rooms.len(), 3, "{rooms:?}");
        assert!(rooms[0].0.starts_with("etage1_piece"));
        assert_eq!(rooms[2].0, "etage2");
        assert_eq!(inner.len(), 1, "un passage dans la cloison");
        let c = inner[0];
        assert!(c.iter().all(|p| p.x.abs() < 1e-4) && (c[1].z - c[0].z - PART_DOOR_H).abs() < 0.3);
    }

    /// Climatiseurs : jamais sous une porte-fenêtre, toujours les mêmes ; détails et couleurs dans le modèle.
    #[test]
    fn details_de_facade() {
        let mut h = House::new("dw_bat_t", vec![[-8.0, -6.0], [8.0, -6.0], [8.0, 6.0], [-8.0, 6.0]]);
        h.apply_preset(1);
        let a = h.ac_spots();
        assert!(!a.is_empty());
        assert_eq!(a.len(), h.ac_spots().len());
        let (_, _, parts) = h.mesh(false);
        assert!(parts.iter().any(|p| tint_of(p.0).is_some()), "façade teinte");
        assert!(parts.iter().any(|p| p.0 == K_METAL), "garde-corps");
        h.lift = 0.6;
        assert!(!h.stoops().is_empty());
        let (pos, _, _) = h.mesh(false);
        assert!(pos.iter().all(|p| p.z >= -1e-3));
        let (mx, _) = (h.bbox().1, 0);
        assert!(pos.iter().all(|p| p.z <= mx.z + 0.01));
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
