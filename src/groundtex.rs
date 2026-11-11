//! Vraies textures du sol de GTA V pour le terrain et les routes.
//!
//! Les textures viennent des dictionnaires partagés du jeu (x64g.rpf\levels\gta5\generic\gtxd.rpf), relevés
//! par la sonde DW_TXD_GREP : globalroads.ytd (herbe, terre, gravier, trottoir, goudron), globaldesertterrain.ytd
//! (sable), globalcanyonterrain.ytd (roche). Dans l'éditeur, elles sont lues dans les fichiers du jeu installé ;
//! à l'export, rien n'est copié : chaque morceau de terrain ou de route qui les utilise devient un type d'objet
//! dont le dictionnaire de textures (textureDictionary du .ytyp) est celui du jeu, et ses matériaux nomment la
//! texture du jeu. Le jeu les charge lui-même (comme pour un objet du jeu).

use crate::gta::drawable::TexData;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

/// Une texture du jeu : (clé de texture DragoWalker, dictionnaire du jeu, nom de la texture).
pub struct GameTex {
    pub key: u64,
    pub label: &'static str,
    pub txd: &'static str,
    pub tex: &'static str,
}

/// Correspondance matériau DragoWalker → texture du jeu (noms relevés dans les .ytd du jeu).
pub const MAP: [GameTex; 10] = [
    GameTex { key: 0xD7E2_0000_0000_0000, label: "Sable", txd: "globaldesertterrain", tex: "cs_rsn_sl_dessand_01" },
    GameTex { key: 0xD7E2_0000_0000_0001, label: "Terre", txd: "globalroads", tex: "im_ground_dirt001" },
    GameTex { key: 0xD7E2_0000_0000_0002, label: "Herbe", txd: "globalroads", tex: "im_grass03" },
    GameTex { key: 0xD7E2_0000_0000_0003, label: "Roche", txd: "globalcanyonterrain", tex: "cs_sl_rsn_canyon_rock_01" },
    GameTex { key: 0xD7E2_0000_0000_0004, label: "Béton", txd: "globalroads", tex: "dc_sidewalk004" },
    GameTex { key: 0xD7E2_0000_0000_0005, label: "Goudron", txd: "globalroads", tex: "dc_parking_tarmac_01" },
    GameTex { key: 0xD7E2_0000_0000_0006, label: "Route", txd: "globalroads", tex: "dc_parking_tarmac_02" },
    GameTex { key: crate::roads::SIDEWALK_KEY, label: "Trottoir", txd: "globalroads", tex: "im_sidewalk003" },
    GameTex { key: crate::roads::ASPHALT_KEY, label: "Chaussée", txd: "globalroads", tex: "dc_parking_tarmac_01" },
    GameTex { key: crate::roads::DIRT_PLAIN_KEY, label: "Chemin de terre", txd: "globalroads", tex: "im_ground_dirt001" },
];

/// Dictionnaires du jeu utilisés (dans l'ordre : sert de numéro de groupe à l'export).
pub const TXDS: [&str; 3] = ["globalroads", "globaldesertterrain", "globalcanyonterrain"];

// désactivé par défaut : les textures du jeu se répètent visiblement sur un grand terrain (retour de la
// personne, 11 nov. 2026) ; celles de DragoWalker sont faites pour ne pas se répéter
static ENABLED: AtomicBool = AtomicBool::new(false);
static LOADED: RwLock<Option<HashMap<u64, TexData>>> = RwLock::new(None);

/// Réglage « textures du jeu » (gardé dans les réglages de DragoWalker).
pub fn init_setting() {
    let s = crate::app::load_settings();
    ENABLED.store(s["game_ground_textures_v2"].as_bool().unwrap_or(false), Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    crate::app::save_setting("game_ground_textures_v2", serde_json::json!(on));
}

/// Les textures du jeu sont-elles lues (jeu chargé et dictionnaires trouvés) ?
pub fn loaded() -> bool {
    LOADED.read().map(|g| g.as_ref().map(|m| !m.is_empty()).unwrap_or(false)).unwrap_or(false)
}

/// Lit les textures dans les fichiers du jeu (appelé quand le jeu est chargé). Rend le nombre trouvé.
pub fn load(w: &crate::gta::world::GameWorld) -> usize {
    let mut out = HashMap::new();
    let mut cache: HashMap<&str, Option<crate::gta::rpf::Resource>> = HashMap::new();
    for g in &MAP {
        let res = cache.entry(g.txd).or_insert_with(|| {
            let file = format!("{}.ytd", g.txd);
            w.vfs.files.iter().filter(|f| f.name.eq_ignore_ascii_case(&file)).last().and_then(|f| w.vfs.read_resource(f, &w.keys))
        });
        if let Some(r) = res {
            if let Some(t) = crate::gta::drawable::find_texture(r, 0, crate::gta::crypto::joaat(g.tex), 512) {
                out.insert(g.key, t);
            }
        }
    }
    let n = out.len();
    if let Ok(mut l) = LOADED.write() {
        *l = Some(out);
    }
    n
}

/// Texture du jeu qui remplace cette clé à l'export (si le réglage est actif).
pub fn game_ref(key: u64) -> Option<&'static GameTex> {
    if !enabled() {
        return None;
    }
    MAP.iter().find(|g| g.key == key)
}

/// Remplace, dans une liste de textures pour la vue, celles qui ont une vraie texture du jeu.
pub fn apply(texs: &mut [(u64, TexData)]) {
    if !enabled() {
        return;
    }
    let Ok(l) = LOADED.read() else { return };
    let Some(m) = l.as_ref() else { return };
    for (k, t) in texs.iter_mut() {
        if let Some(g) = m.get(k) {
            *t = g.clone();
        }
    }
}

/// Texture du jeu d'une clé (pour les aperçus), si elle est lue.
pub fn get(key: u64) -> Option<TexData> {
    if !enabled() {
        return None;
    }
    LOADED.read().ok()?.as_ref()?.get(&key).cloned()
}
