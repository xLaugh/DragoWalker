//! Végétation au pinceau : on choisit un ensemble (forêt de pins, palmiers, désert, buissons…) et on
//! peint sur le terrain ; DragoWalker pose de vrais objets du jeu, espacés, tournés et de taille variée,
//! jamais sur les routes ni le béton. Ctrl + pinceau retire la végétation.
//! Les noms viennent des types d'objets du jeu les plus placés sur la carte d'origine (sonde DW_ARCH_GREP
//! sur E:\Games\GTAV : prop_bush_med_03 placé 274 595 fois, prop_tree_cedar_s_01 23 726 fois…).

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use crate::model::Entity;

pub struct VegSet {
    pub name: &'static str,
    pub tip: &'static str,
    pub props: &'static [&'static str],
    /// écart minimal entre deux objets (m)
    pub spacing: f32,
    /// distance d'affichage donnée aux objets posés (m) — **estimé**
    pub lod_dist: f32,
    /// pente maximale acceptée (tangente)
    pub max_slope: f32,
}

pub const SETS: [VegSet; 13] = [
    VegSet {
        name: "Forêt de pins",
        tip: "Pins et cèdres de la région de Paleto / Mont Chiliad",
        props: &["prop_tree_pine_02", "prop_tree_pine_01", "prop_tree_cedar_02", "prop_tree_cedar_03", "prop_tree_cedar_04", "prop_w_r_cedar_01", "prop_tree_cedar_s_01", "prop_tree_cedar_s_04", "prop_tree_cedar_s_05"],
        spacing: 9.0,
        lod_dist: 350.0,
        max_slope: 0.9,
    },
    VegSet {
        name: "Feuillus et parc",
        tip: "Chênes, érables, bouleaux, oliviers, ficus de Los Santos",
        props: &["prop_tree_eng_oak_01", "prop_tree_oak_01", "prop_tree_maple_02", "prop_tree_maple_03", "prop_tree_birch_01", "prop_tree_birch_04", "prop_tree_olive_01", "prop_tree_lficus_03", "prop_tree_lficus_05", "prop_tree_jacada_01"],
        spacing: 12.0,
        lod_dist: 300.0,
        max_slope: 0.7,
    },
    VegSet {
        name: "Palmiers",
        tip: "Palmiers de plage et de boulevard",
        props: &["prop_palm_med_01b", "prop_palm_fan_03_c", "prop_palm_fan_03_d", "prop_palm_fan_03_b", "prop_palm_sm_01e", "prop_palm_sm_01d", "prop_palm_huge_01a", "prop_fan_palm_01a"],
        spacing: 9.0,
        lod_dist: 350.0,
        max_slope: 0.6,
    },
    VegSet {
        name: "Désert",
        tip: "Arbres de Josué, cactus, bois de fer, buissons secs (Sandy Shores)",
        props: &["prop_joshua_tree_01a", "prop_joshua_tree_01b", "prop_joshua_tree_01c", "prop_joshua_tree_01d", "prop_cactus_01b", "prop_cactus_01d", "prop_cactus_01e", "prop_cactus_02", "prop_desert_iron_01", "prop_bush_dead_02"],
        spacing: 8.0,
        lod_dist: 220.0,
        max_slope: 0.8,
    },
    VegSet {
        name: "Buissons",
        tip: "Les buissons les plus utilisés de la carte",
        props: &["prop_bush_med_03", "prop_bush_med_02", "prop_bush_lrg_01", "prop_bush_med_06", "prop_bush_med_07", "prop_bush_lrg_01b", "prop_bush_med_05", "prop_bush_lrg_01d"],
        spacing: 4.0,
        lod_dist: 150.0,
        max_slope: 1.0,
    },
    VegSet {
        name: "Herbes et fougères",
        tip: "Touffes d'herbe, fougères, roseaux",
        props: &["prop_veg_grass_01_a", "prop_veg_grass_01_b", "prop_veg_grass_01_c", "prop_veg_grass_02_a", "prop_plant_fern_02a", "prop_plant_fern_02b", "prop_plant_cane_01a", "prop_plant_cane_02a"],
        spacing: 2.5,
        lod_dist: 80.0,
        max_slope: 1.0,
    },
    VegSet {
        name: "Rochers",
        tip: "Rochers de toutes tailles",
        props: &["prop_rock_1_a", "prop_rock_1_b", "prop_rock_1_c", "prop_rock_1_d", "prop_rock_1_f", "prop_rock_3_a", "prop_rock_3_i", "prop_rock_4_big", "prop_rock_4_big2"],
        spacing: 6.0,
        lod_dist: 250.0,
        max_slope: 3.0,
    },
    // variétés ajoutées le 11 nov. 2026 : noms et nombre de placements relevés par la sonde DW_ARCH_GREP
    // (prop_tree_birch_04 5 398 fois, prop_tree_eucalip_01 1 102, prop_tree_cypress_01 840, test_tree_forest_trunk_01 682…)
    VegSet {
        name: "Bouleaux",
        tip: "Bouleaux de toutes tailles (nord de la carte)",
        props: &["prop_tree_birch_01", "prop_tree_birch_02", "prop_tree_birch_03", "prop_tree_birch_03b", "prop_tree_birch_04", "prop_tree_birch_05"],
        spacing: 7.0,
        lod_dist: 300.0,
        max_slope: 0.8,
    },
    VegSet {
        name: "Méditerranéen",
        tip: "Eucalyptus, cyprès, oliviers, mesquites (Vinewood Hills, collines sèches)",
        props: &["prop_tree_eucalip_01", "prop_tree_cypress_01", "prop_tree_olive_01", "prop_tree_mquite_01", "prop_tree_jacada_02"],
        spacing: 11.0,
        lod_dist: 320.0,
        max_slope: 0.8,
    },
    VegSet {
        name: "Arbres morts et tombés",
        tip: "Cèdres morts, troncs couchés, herbes mortes",
        props: &["prop_w_r_cedar_dead", "prop_tree_fallen_pine_01", "prop_tree_fallen_01", "prop_tree_fallen_02", "prop_weeddead_nxg01", "prop_weeddead_nxg02"],
        spacing: 10.0,
        lod_dist: 250.0,
        max_slope: 0.9,
    },
    VegSet {
        name: "Forêt géante",
        tip: "Très grands troncs de forêt ancienne (Raton Canyon) et grands cèdres",
        props: &["test_tree_forest_trunk_01", "test_tree_cedar_trunk_001", "prop_tree_cedar_03", "prop_tree_cedar_04"],
        spacing: 18.0,
        lod_dist: 500.0,
        max_slope: 0.9,
    },
    VegSet {
        name: "Neige",
        tip: "Sapins enneigés de North Yankton",
        props: &["prop_snow_tree_03_i", "prop_snow_tree_04_d", "prop_snow_tree_04_f"],
        spacing: 9.0,
        lod_dist: 350.0,
        max_slope: 1.0,
    },
    VegSet {
        name: "Forêt mixte",
        tip: "Pins, cèdres, chênes, bouleaux, érables et buissons mélangés",
        props: &["prop_tree_pine_02", "prop_tree_cedar_03", "prop_tree_eng_oak_01", "prop_tree_birch_04", "prop_tree_maple_02", "prop_tree_birch_03b", "prop_bush_med_03", "prop_bush_lrg_01"],
        spacing: 9.0,
        lod_dist: 320.0,
        max_slope: 0.9,
    },
];

/// Vrai si ce type d'objet fait partie d'un ensemble de végétation.
/// Écart proposé pour un ensemble (celui qu'on avait avec la densité par défaut de 50 %), arrondi au demi-mètre.
pub fn default_spacing(set: usize) -> f32 {
    let s = &SETS[set.min(SETS.len() - 1)];
    ((s.spacing / 0.5f32.sqrt()) * 2.0).round() / 2.0
}

pub fn is_vegetation(arch: &str) -> bool {
    SETS.iter().any(|s| s.props.iter().any(|p| p.eq_ignore_ascii_case(arch)))
}

impl DwApp {
    /// Objets d'un ensemble présents dans le jeu chargé (tous si le jeu n'est pas chargé).
    pub fn veg_props(&self, set: usize) -> Vec<&'static str> {
        let s = &SETS[set.min(SETS.len() - 1)];
        match self.game.world.as_ref() {
            Some(w) => s.props.iter().copied().filter(|p| w.archetypes.contains_key(&crate::gta::crypto::joaat(p))).collect(),
            None => s.props.to_vec(),
        }
    }

    /// Nombre d'objets de végétation dans le projet.
    pub fn veg_count(&self) -> usize {
        self.proj.ymaps.iter().filter(|y| !y.sample).flat_map(|y| y.entities.iter()).filter(|e| is_vegetation(&e.archetype)).count()
    }

    /// Sème la végétation dans le disque (centre `c`, rayon `r`) : nombre d'essais selon la surface,
    /// l'écart minimal et la densité ; refuse les routes, le goudron, le béton, l'eau et les pentes trop raides.
    pub fn veg_paint(&mut self, c: V3, r: f32) -> usize {
        let set = self.terrain.veg_set.min(SETS.len() - 1);
        let props = self.veg_props(set);
        if props.is_empty() {
            return 0;
        }
        let s = &SETS[set];
        let spacing = if self.terrain.veg_spacing > 0.0 { self.terrain.veg_spacing } else { s.spacing / self.terrain.veg_density.clamp(0.05, 1.0).sqrt() };
        let Some(t) = self.proj.terrain.as_ref() else { return 0 };
        // objets de végétation déjà là, près du disque
        let mut near: Vec<V3> = self
            .proj
            .ymaps
            .iter()
            .flat_map(|y| y.entities.iter())
            .filter(|e| (e.pos.x - c.x).abs() < r + spacing && (e.pos.y - c.y).abs() < r + spacing && is_vegetation(&e.archetype))
            .map(|e| e.pos)
            .collect();
        let area = std::f32::consts::PI * r * r;
        let tries = ((area / (spacing * spacing)) * 1.5).ceil().clamp(1.0, 400.0) as usize;
        let mut new: Vec<Entity> = Vec::new();
        for _ in 0..tries {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let d = r * self.rng.range(0.0, 1.0).sqrt();
            let (x, y) = (c.x + d * a.cos(), c.y + d * a.sin());
            let Some(z) = t.height_at(x, y) else { continue };
            if z < 0.3 {
                continue; // dans l'eau
            }
            if let Some(m) = t.material_at(x, y) {
                if m >= 4 {
                    continue; // béton, goudron, route
                }
            }
            if t.roads.iter().any(|rd| rd.nearest(x, y).map(|(dd, _)| dd < rd.half_total() + 1.0).unwrap_or(false)) {
                continue;
            }
            let slope = {
                let e = 1.0;
                let hx = t.height_at(x + e, y).unwrap_or(z) - t.height_at(x - e, y).unwrap_or(z);
                let hy = t.height_at(x, y + e).unwrap_or(z) - t.height_at(x, y - e).unwrap_or(z);
                ((hx * hx + hy * hy).sqrt()) / (2.0 * e)
            };
            if slope > s.max_slope {
                continue;
            }
            if near.iter().any(|p| (p.x - x).powi(2) + (p.y - y).powi(2) < spacing * spacing) {
                continue;
            }
            let k = (self.rng.next_u32() as usize) % props.len();
            // un peu enfoncé dans le sol pour que le pied ne flotte pas sur une pente
            let mut e = Entity::new(props[k], v3(x, y, z - 0.15 - slope.min(1.0) * 0.3));
            e.rot = Quat::from_z(self.rng.range(0.0, std::f32::consts::TAU));
            let sc = self.rng.range(0.85, 1.2);
            e.scale_xy = sc;
            e.scale_z = sc;
            // distance du type dans le jeu, sinon celle de l'ensemble
            e.lod_dist = self.game.world.as_ref().and_then(|w| w.archetypes.get(&crate::gta::crypto::joaat(props[k]))).map(|a| a.lod_dist).filter(|d| *d > 0.0).unwrap_or(s.lod_dist);
            near.push(e.pos);
            new.push(e);
        }
        let n = new.len();
        if n > 0 {
            let yi = self.ensure_target_ymap();
            self.proj.ymaps[yi].entities.extend(new);
            self.sync_game_project();
        }
        n
    }

    /// Retire la végétation du projet dans le disque.
    pub fn veg_remove(&mut self, c: V3, r: f32) -> usize {
        let mut n = 0;
        for y in self.proj.ymaps.iter_mut().filter(|y| !y.sample) {
            let before = y.entities.len();
            y.entities.retain(|e| !(is_vegetation(&e.archetype) && (e.pos.x - c.x).powi(2) + (e.pos.y - c.y).powi(2) <= r * r));
            n += before - y.entities.len();
        }
        if n > 0 {
            self.clean_selection();
            self.sync_game_project();
        }
        n
    }
}
