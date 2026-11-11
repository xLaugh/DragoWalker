//! Catalogue de props intégré. Les noms sont les noms d'archétypes du jeu ;
//! les tailles sont ESTIMÉES (dans le jeu elles viennent des fichiers ytyp).

use crate::math::{v3, V3};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Box,
    Tree,
    Pole,
}

pub struct Prop {
    pub name: &'static str,
    pub label: &'static str,
    pub cat: &'static str,
    /// largeur, profondeur, hauteur (m) — estimé
    pub size: [f32; 3],
    pub color: [u8; 3],
    pub shape: Shape,
}

macro_rules! p {
    ($n:expr, $l:expr, $c:expr, [$x:expr, $y:expr, $z:expr], [$r:expr, $g:expr, $b:expr], $s:ident) => {
        Prop { name: $n, label: $l, cat: $c, size: [$x, $y, $z], color: [$r, $g, $b], shape: Shape::$s }
    };
}

pub const CATEGORIES: &[&str] = &["Rue", "Nature", "Barrières", "Intérieur", "Construction", "Véhicules", "Divers"];

pub static PROPS: &[Prop] = &[
    p!("prop_bench_01a", "Banc", "Rue", [1.8, 0.6, 0.9], [140, 100, 60], Box),
    p!("prop_streetlight_01", "Lampadaire", "Rue", [0.4, 0.4, 7.5], [90, 95, 100], Pole),
    p!("prop_bin_01a", "Poubelle", "Rue", [0.6, 0.6, 1.0], [60, 90, 60], Box),
    p!("prop_dumpster_01a", "Benne", "Rue", [2.0, 1.2, 1.4], [40, 90, 50], Box),
    p!("prop_atm_01", "Distributeur", "Rue", [0.8, 0.6, 2.0], [50, 70, 130], Box),
    p!("prop_vend_soda_01", "Distributeur de soda", "Rue", [1.0, 0.8, 1.9], [190, 40, 40], Box),
    p!("prop_busstop_02", "Abribus", "Rue", [4.0, 1.6, 2.6], [120, 130, 140], Box),
    p!("prop_fire_hydrant_1", "Bouche incendie", "Rue", [0.4, 0.4, 0.8], [200, 50, 40], Box),
    p!("prop_parknmeter_01", "Parcmètre", "Rue", [0.3, 0.3, 1.4], [80, 80, 90], Pole),
    p!("prop_tree_pine_01", "Pin", "Nature", [4.0, 4.0, 12.0], [40, 110, 50], Tree),
    p!("prop_palm_med_01b", "Palmier", "Nature", [3.0, 3.0, 10.0], [70, 150, 60], Tree),
    p!("prop_tree_oak_01", "Chêne", "Nature", [7.0, 7.0, 11.0], [60, 130, 50], Tree),
    p!("prop_bush_lrg_04b", "Buisson", "Nature", [2.5, 2.5, 1.8], [50, 120, 45], Box),
    p!("prop_rock_4_c", "Rocher", "Nature", [2.5, 2.0, 1.5], [120, 115, 105], Box),
    p!("prop_plant_01a", "Plante en pot", "Nature", [0.6, 0.6, 1.2], [70, 140, 60], Tree),
    p!("prop_barrier_work05", "Barrière de chantier", "Barrières", [2.2, 0.5, 1.1], [220, 120, 30], Box),
    p!("prop_mp_barrier_02b", "Barrière béton", "Barrières", [3.0, 0.7, 1.0], [170, 170, 165], Box),
    p!("prop_fnclink_02a", "Grillage", "Barrières", [4.0, 0.1, 2.5], [150, 155, 160], Box),
    p!("prop_mp_cone_01", "Cône", "Barrières", [0.4, 0.4, 0.7], [240, 110, 20], Box),
    p!("prop_roadcone02a", "Petit cône", "Barrières", [0.3, 0.3, 0.5], [240, 120, 30], Box),
    p!("prop_bollard_02a", "Potelet", "Barrières", [0.25, 0.25, 0.9], [60, 60, 65], Pole),
    p!("prop_table_03", "Table", "Intérieur", [1.6, 0.9, 0.8], [150, 110, 70], Box),
    p!("prop_chair_01a", "Chaise", "Intérieur", [0.5, 0.5, 0.9], [130, 90, 55], Box),
    p!("prop_couch_01", "Canapé", "Intérieur", [2.2, 0.9, 0.9], [110, 60, 60], Box),
    p!("prop_tv_flat_01", "Télé écran plat", "Intérieur", [1.3, 0.2, 0.8], [30, 30, 35], Box),
    p!("v_res_fridgemodsml", "Frigo", "Intérieur", [0.8, 0.7, 1.8], [220, 220, 225], Box),
    p!("prop_off_desk_01", "Bureau", "Intérieur", [1.6, 0.8, 0.8], [120, 90, 60], Box),
    p!("prop_bar_stool_01", "Tabouret", "Intérieur", [0.4, 0.4, 0.8], [90, 60, 40], Box),
    p!("v_ret_ml_tablea", "Comptoir", "Intérieur", [2.5, 0.8, 1.0], [160, 140, 110], Box),
    p!("prop_container_01a", "Conteneur", "Construction", [6.1, 2.5, 2.6], [40, 90, 150], Box),
    p!("prop_conc_blocks01a", "Blocs béton", "Construction", [2.0, 1.0, 0.8], [160, 160, 155], Box),
    p!("prop_pallet_01a", "Palette", "Construction", [1.2, 1.0, 0.15], [170, 130, 80], Box),
    p!("prop_rub_tyre_01", "Pneu", "Construction", [0.8, 0.8, 0.3], [30, 30, 30], Box),
    p!("prop_gazebo_02", "Tonnelle", "Construction", [3.5, 3.5, 3.0], [210, 210, 205], Box),
    p!("prop_gas_pump_1a", "Pompe à essence", "Véhicules", [1.0, 0.7, 2.2], [200, 60, 50], Box),
    p!("prop_air_lights_02a", "Balise de piste", "Véhicules", [0.3, 0.3, 0.3], [240, 220, 60], Box),
    p!("prop_byard_ramp", "Rampe", "Véhicules", [3.0, 4.0, 1.2], [120, 110, 100], Box),
    p!("prop_alien_egg_01", "Œuf (défaut CodeWalker)", "Divers", [1.0, 1.0, 1.4], [120, 160, 80], Box),
    p!("prop_beach_parasol_02", "Parasol", "Divers", [2.5, 2.5, 2.6], [230, 90, 90], Tree),
    p!("prop_flag_us", "Drapeau", "Divers", [0.3, 0.3, 6.0], [200, 200, 210], Pole),
];

pub fn find(name: &str) -> Option<&'static Prop> {
    PROPS.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

/// Forme d'affichage d'un archétype (catalogue, sinon devinée par le nom).
pub fn shape_for(name: &str) -> Shape {
    if let Some(p) = find(name) {
        return p.shape;
    }
    let n = name.to_ascii_lowercase();
    if n.contains("tree") || n.contains("palm") || n.contains("arbre") {
        Shape::Tree
    } else if n.contains("light") || n.contains("pole") || n.contains("lamp") {
        Shape::Pole
    } else {
        Shape::Box
    }
}

pub fn color_for(name: &str) -> [u8; 3] {
    if let Some(p) = find(name) {
        return p.color;
    }
    let n = name.to_ascii_lowercase();
    if n.starts_with("dw_batiment") || n.starts_with("dw_tour") {
        // couleur stable selon le nom
        let h = n.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
        let g = 150 + (h % 60) as u8;
        return [g, g.saturating_sub((h >> 8) as u8 % 25), g.saturating_sub(10 + (h >> 12) as u8 % 30)];
    }
    let h = n.bytes().fold(5381u32, |a, b| a.wrapping_mul(33) ^ b as u32);
    [110 + (h % 100) as u8, 110 + ((h >> 8) % 100) as u8, 110 + ((h >> 16) % 100) as u8]
}

static GAME_BOXES: std::sync::OnceLock<std::sync::RwLock<std::collections::HashMap<String, (V3, V3)>>> = std::sync::OnceLock::new();

/// Enregistre les tailles réelles (bbMin, bbMax) des types d'objets lus dans le jeu.
pub fn set_game_boxes(m: std::collections::HashMap<String, (V3, V3)>) {
    let lock = GAME_BOXES.get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()));
    if let Ok(mut g) = lock.write() {
        *g = m;
    }
}

static CUSTOM_BOXES: std::sync::OnceLock<std::sync::RwLock<std::collections::HashMap<String, (V3, V3)>>> = std::sync::OnceLock::new();

/// Tailles des modèles personnels (Blender / Sollumz).
pub fn set_custom_boxes(m: std::collections::HashMap<String, (V3, V3)>) {
    let lock = CUSTOM_BOXES.get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()));
    if let Ok(mut g) = lock.write() {
        *g = m;
    }
}

static MLO_BOXES: std::sync::OnceLock<std::sync::RwLock<std::collections::HashMap<String, (V3, V3)>>> = std::sync::OnceLock::new();

/// Tailles des modèles des ressources MLO importées.
pub fn set_mlo_boxes(m: std::collections::HashMap<String, (V3, V3)>) {
    let lock = MLO_BOXES.get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()));
    if let Ok(mut g) = lock.write() {
        *g = m;
    }
}

/// Taille réelle d'un type (modèle personnel d'abord, puis MLO importé, puis jeu), si connue.
pub fn game_box(name: &str) -> Option<(V3, V3)> {
    let n = name.to_ascii_lowercase();
    if let Some(c) = CUSTOM_BOXES.get().and_then(|l| l.read().ok()).and_then(|g| g.get(&n).copied()) {
        return Some(c);
    }
    if let Some(c) = MLO_BOXES.get().and_then(|l| l.read().ok()).and_then(|g| g.get(&n).copied()) {
        return Some(c);
    }
    let g = GAME_BOXES.get()?.read().ok()?;
    g.get(&n).copied()
}

/// Boîte locale (bbMin, bbMax) : réelle si le jeu est chargé, sinon estimée.
pub fn box_for(name: &str) -> (V3, V3) {
    if let Some(b) = game_box(name) {
        return b;
    }
    if let Some(p) = find(name) {
        return (v3(-p.size[0] * 0.5, -p.size[1] * 0.5, 0.0), v3(p.size[0] * 0.5, p.size[1] * 0.5, p.size[2]));
    }
    let n = name.to_ascii_lowercase();
    // bâtiments de la carte d'exemple : dw_batiment_LxPxH
    if let Some(rest) = n.strip_prefix("dw_batiment_") {
        let parts: Vec<f32> = rest.split('x').filter_map(|s| s.parse().ok()).collect();
        if parts.len() == 3 {
            return (v3(-parts[0] * 0.5, -parts[1] * 0.5, 0.0), v3(parts[0] * 0.5, parts[1] * 0.5, parts[2]));
        }
    }
    match shape_for(name) {
        Shape::Tree => (v3(-2.0, -2.0, 0.0), v3(2.0, 2.0, 9.0)),
        Shape::Pole => (v3(-0.2, -0.2, 0.0), v3(0.2, 0.2, 5.0)),
        Shape::Box => (v3(-0.5, -0.5, 0.0), v3(0.5, 0.5, 1.0)),
    }
}

/// Recherche « floue » d'un prop par mot (utilisée par l'assistant).
pub fn guess(word: &str) -> Option<&'static Prop> {
    let w = word.to_lowercase();
    if let Some(p) = find(&w) {
        return Some(p);
    }
    let syn: &[(&str, &str)] = &[
        ("arbre", "prop_tree_oak_01"),
        ("arbres", "prop_tree_oak_01"),
        ("pin", "prop_tree_pine_01"),
        ("sapin", "prop_tree_pine_01"),
        ("palmier", "prop_palm_med_01b"),
        ("banc", "prop_bench_01a"),
        ("lampadaire", "prop_streetlight_01"),
        ("lampe", "prop_streetlight_01"),
        ("poubelle", "prop_bin_01a"),
        ("benne", "prop_dumpster_01a"),
        ("cone", "prop_mp_cone_01"),
        ("cône", "prop_mp_cone_01"),
        ("barriere", "prop_barrier_work05"),
        ("barrière", "prop_barrier_work05"),
        ("beton", "prop_mp_barrier_02b"),
        ("béton", "prop_mp_barrier_02b"),
        ("grillage", "prop_fnclink_02a"),
        ("cloture", "prop_fnclink_02a"),
        ("clôture", "prop_fnclink_02a"),
        ("table", "prop_table_03"),
        ("chaise", "prop_chair_01a"),
        ("canape", "prop_couch_01"),
        ("canapé", "prop_couch_01"),
        ("tele", "prop_tv_flat_01"),
        ("télé", "prop_tv_flat_01"),
        ("frigo", "v_res_fridgemodsml"),
        ("bureau", "prop_off_desk_01"),
        ("tabouret", "prop_bar_stool_01"),
        ("comptoir", "v_ret_ml_tablea"),
        ("conteneur", "prop_container_01a"),
        ("container", "prop_container_01a"),
        ("palette", "prop_pallet_01a"),
        ("pneu", "prop_rub_tyre_01"),
        ("buisson", "prop_bush_lrg_04b"),
        ("rocher", "prop_rock_4_c"),
        ("pierre", "prop_rock_4_c"),
        ("plante", "prop_plant_01a"),
        ("atm", "prop_atm_01"),
        ("distributeur", "prop_atm_01"),
        ("abribus", "prop_busstop_02"),
        ("pompe", "prop_gas_pump_1a"),
        ("rampe", "prop_byard_ramp"),
        ("parasol", "prop_beach_parasol_02"),
        ("drapeau", "prop_flag_us"),
        ("tonnelle", "prop_gazebo_02"),
        ("potelet", "prop_bollard_02a"),
        ("oeuf", "prop_alien_egg_01"),
        ("œuf", "prop_alien_egg_01"),
    ];
    for (k, v) in syn {
        if w == *k || w.trim_end_matches('s') == *k || w.starts_with(k) {
            return find(v);
        }
    }
    PROPS.iter().find(|p| p.label.to_lowercase().contains(&w) || p.name.contains(&w))
}
