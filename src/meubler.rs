//! Intérieurs (onglet Intérieur, pièce choisie) :
//! - **meubler à la main** : catalogue des objets d'intérieur du jeu (canapés, lits, tables, cuisine, bain, bureau,
//!   déco, lumières…) rangés par catégorie, avec recherche et vignettes (celles de la bibliothèque de props), puis
//!   pose à la souris sur le sol de la pièce (molette = tourner, Maj + molette = quart de tour, clic = poser,
//!   Échap ou clic droit = fini) ;
//! - **ambiance par pièce** : les ambiances (timecycle) du jeu avec un libellé français deviné d'après le nom, et
//!   un aperçu dans la vue (teinte et luminosité approchées de l'ambiance, posées sur la pièce) ;
//! - **ambiances du projet** (ambiance.rs) : copiées d'une ambiance du jeu puis réglées, en tête de la liste ;
//! - **moins de portails** : les petits portails voisins vers l'extérieur d'un même mur et d'un même étage sont
//!   réunis en un seul (rectangle englobant), pour rester loin de la réserve « PortalInst » du jeu (check.rs).

use crate::app::DwApp;
use crate::math::{v3, Quat, V3};
use crate::model::{Entity, MloArchetype, Portal};
use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke};
use std::collections::{HashMap, HashSet};

// ------------------------------------------------------------------ catalogue des meubles

/// Catégories du catalogue : nom, mots cherchés dans le nom du modèle (n'importe lequel). Classement par mots du
/// nom — **estimé** (le jeu ne range pas ses objets par catégorie).
pub const FURN_CATS: [(&str, &[&str]); 12] = [
    ("Tout", &[]),
    ("Canapés / fauteuils", &["sofa", "couch", "armchair", "settee", "lounger", "chaise_l", "recliner", "beanbag"]),
    ("Lits", &["bed_", "_bed", "beds", "mattress", "bunk", "cot_", "crib"]),
    ("Tables", &["table", "coffeetab", "dining", "counter_", "bar_top", "island"]),
    ("Chaises", &["chair", "stool", "bench", "seat", "pew"]),
    ("Cuisine", &["kitchen", "fridge", "cooker", "oven", "microwave", "kettle", "toaster", "sink", "hob_", "dishwash", "coffee_mac", "plate", "mug", "pan_", "pot_", "blender", "freezer", "kit_"]),
    ("Salle de bain", &["toilet", "bath", "shower", "basin", "towel", "loo_", "wc_", "soap", "toothbrush", "urinal", "washbasin", "bidet"]),
    ("Bureau", &["desk", "office", "filing", "filecab", "computer", "monitor", "laptop", "printer", "copier", "keyboard", "phone", "whiteboard", "binder", "folder"]),
    ("Rangement", &["shelf", "shelves", "cabinet", "wardrobe", "drawer", "cupboard", "locker", "dresser", "bookcase", "rack", "chest_"]),
    ("Électronique", &["tv_", "_tv", "television", "speaker", "radio", "console", "hifi", "stereo", "fan_", "_fan", "amp_", "dvd"]),
    ("Déco", &["vase", "frame", "art_", "statue", "rug", "carpet", "curtain", "clock", "poster", "pic_", "painting", "candle", "ornament", "plant", "flower", "cushion", "pillow", "blind", "mirror", "sculpt", "bowl"]),
    ("Lumières", &["light", "lamp", "chandelier", "bulb", "lantern", "neon", "sconce"]),
];

/// Mots exclus du catalogue (versions lointaines, morceaux cassés, objets de rue…) — **estimé**.
const FURN_EXCLUDE: [&str; 16] = ["lod", "_dam", "broken", "_frag", "street", "traffic", "_ext", "ext_", "proxy", "shadow", "_col", "collision", "dummy", "scaff", "veh_", "billboard"];
/// Mots d'un nom de modèle « …door… » qui n'est pas une porte de passage mais celle d'un meuble (armoire, placard,
/// frigo, four, casier, coffre…) : jamais rattaché à un portail ni ouvert par dw_doors.lua — **estimé**.
pub const FURN_DOOR_EXCLUDE: [&str; 16] = ["wardrobe", "cabinet", "cupboard", "fridge", "freezer", "locker", "oven", "microwave", "dishwash", "drawer", "shower", "cooker", "dresser", "safe", "chest", "_veh"];
/// Plus grande taille d'un objet d'intérieur (largeur, profondeur, hauteur ; m) — **estimé**.
const FURN_MAX: [f32; 3] = [6.0, 6.0, 4.0];
/// Vignettes du catalogue (pixels).
const FURN_CELL: f32 = 66.0;

/// Mots français cherchés aussi en anglais dans les noms du jeu.
const FR_WORDS: [(&str, &[&str]); 24] = [
    ("canap", &["sofa", "couch"]),
    ("fauteuil", &["armchair", "chair"]),
    ("lit", &["bed"]),
    ("chaise", &["chair"]),
    ("tabouret", &["stool"]),
    ("banc", &["bench"]),
    ("lampe", &["lamp", "light"]),
    ("lumi", &["light", "lamp"]),
    ("frigo", &["fridge"]),
    ("four", &["oven", "cooker"]),
    ("évier", &["sink"]),
    ("evier", &["sink"]),
    ("toilette", &["toilet"]),
    ("douche", &["shower"]),
    ("baignoire", &["bath"]),
    ("bureau", &["desk", "office"]),
    ("étagère", &["shelf"]),
    ("etagere", &["shelf"]),
    ("armoire", &["wardrobe", "cabinet"]),
    ("télé", &["tv"]),
    ("tele", &["tv"]),
    ("plante", &["plant"]),
    ("tapis", &["rug", "carpet"]),
    ("ordinateur", &["computer", "laptop", "monitor"]),
];

/// Un objet du catalogue : modèle, nom, catégories (bits), taille.
#[derive(Clone)]
pub struct FurnItem {
    pub hash: u32,
    pub name: String,
    pub cats: u16,
    pub size: V3,
}

/// Catégories (bits) d'un nom de modèle ; 0 = pas un objet d'intérieur.
pub fn furn_cats(name: &str, size: V3) -> u16 {
    let n = name.to_lowercase();
    // objets « props » du jeu : prop_…, p_…, v_… (intérieurs du jeu de base) ou …_prop_… (DLC)
    if !(n.starts_with("prop_") || n.starts_with("p_") || n.starts_with("v_") || n.contains("_prop_")) {
        return 0;
    }
    if FURN_EXCLUDE.iter().any(|x| n.contains(x)) {
        return 0;
    }
    if size.x > FURN_MAX[0] || size.y > FURN_MAX[1] || size.z > FURN_MAX[2] || size.x.max(size.y).max(size.z) < 0.05 {
        return 0;
    }
    let mut bits = 0u16;
    for (i, (_, words)) in FURN_CATS.iter().enumerate().skip(1) {
        if words.iter().any(|w| n.contains(w)) {
            bits |= 1 << i;
        }
    }
    bits
}

/// Vrai si le nom correspond à la recherche (mots séparés par des espaces, tous présents ; mots français traduits).
pub fn furn_matches(name: &str, search: &str) -> bool {
    let n = name.to_lowercase();
    search.split_whitespace().all(|w| {
        let w = w.to_lowercase();
        n.contains(&w) || FR_WORDS.iter().any(|(fr, en)| w.starts_with(fr) && en.iter().any(|e| n.contains(e)))
    })
}

/// Pose en cours : l'objet suit la souris sur le sol de la pièce.
#[derive(Clone, Debug)]
pub struct FurnPlace {
    pub arch: String,
    pub yaw_deg: f32,
    pub placed: usize,
}

/// Point visé pendant la pose : (point du monde, pièce sous la souris (0 = hors des pièces), point dans l'intérieur).
pub type FurnAt = (V3, usize, V3);

/// Une ambiance du jeu : nom brut, libellé français deviné, teinte et luminosité de l'aperçu, vraies valeurs du jeu
/// ou table approchée.
#[derive(Clone, Debug)]
pub struct Ambiance {
    pub name: String,
    pub label: &'static str,
    pub tint: [f32; 3],
    pub bright: f32,
    pub from_game: bool,
    pub interior: bool,
}

#[derive(Default)]
pub struct FurnState {
    /// pièce choisie dans l'onglet Intérieur (ambiance, meubles)
    pub room: Option<usize>,
    pub cat: usize,
    pub search: String,
    /// tous les objets d'intérieur du jeu (fait une fois)
    pub items: Option<Vec<FurnItem>>,
    /// vue filtrée : (catégorie, recherche) → indices dans `items`
    pub view_key: (usize, String),
    pub view: Vec<usize>,
    pub place: Option<FurnPlace>,
    pub at: Option<FurnAt>,
    /// ambiances du jeu (lues une fois)
    pub ambs: Option<Vec<Ambiance>>,
    pub amb_search: String,
    pub amb_all: bool,
    /// ambiance survolée dans la liste (aperçu)
    pub amb_hover: Option<String>,
    /// aperçu de l'ambiance de la pièce choisie dans la vue
    pub amb_preview: bool,
    /// dernier « Réduire les portails » : compte rendu
    pub merge_report: String,
    /// faire défiler l'onglet jusqu'à la pièce choisie (bouton « Choisir » d'une pièce)
    pub scroll_to: bool,
    /// jeu dont viennent `items` et `ambs` (adresse du monde chargé) : un autre jeu chargé les refait
    pub world_key: usize,
    /// aperçu de « Réduire les portails » : (signature des intérieurs, avant, après), refait seulement s'ils changent
    pub merge_cache: std::cell::Cell<(u64, usize, usize)>,
    /// ambiance du projet choisie pour la régler (indice dans proj.ambiances)
    pub pamb_sel: Option<usize>,
    /// nom en cours de saisie pour « Renommer »
    pub pamb_rename: String,
    /// réglage en cours (glissière tenue, sélecteur de couleur ouvert) : une seule annulation pour tout le geste
    pub pamb_editing: bool,
}

// ------------------------------------------------------------------ ambiances

/// Libellé des ambiances du projet dans la liste.
pub const PROJ_AMB_LABEL: &str = "Du projet";

/// Familles d'ambiance : libellé français, mots du nom, teinte et luminosité approchées (utilisées quand le jeu
/// n'en donne pas) — **estimé**.
const AMB_FAMILIES: [(&str, &[&str], [f32; 3], f32); 17] = [
    ("Garage", &["garage", "carmod", "lsc", "impexp", "autoshop", "mod_shop"], [0.92, 0.96, 1.05], 0.85),
    ("Bureau", &["office", "_off", "corp", "ofc", "fib", "exec", "iaa", "lawyer"], [0.96, 1.0, 1.06], 1.05),
    ("Néon / coloré", &["neon", "arcade", "casino", "disco", "pink", "purple", "_red", "red_", "blue_", "green_", "rave"], [0.95, 0.62, 1.2], 0.75),
    ("Sombre", &["dark", "black", "cellar", "basement", "sewer", "tunnel", "cave", "mine", "dim", "blackout", "nolight", "nightvision", "shadow"], [0.85, 0.9, 1.0], 0.45),
    ("Entrepôt / atelier", &["warehouse", "wh_", "ware", "factory", "hangar", "bunker", "lab", "meth", "weed", "coke", "facility", "biker", "silo", "plant"], [0.98, 0.96, 0.84], 0.75),
    ("Magasin", &["shop", "store", "clothes", "barber", "tattoo", "ammu", "gunstore", "247", "liquor", "ltd", "boutique", "mall"], [1.0, 1.0, 1.02], 1.15),
    ("Bar / club", &["_bar", "bar_", "bar0", "bar1", "bar2", "bars", "club", "nightclub", "strip", "pub_", "tequila", "bahama", "lounge", "vip"], [1.12, 0.78, 0.92], 0.65),
    ("Restaurant / cuisine", &["kitchen", "diner", "restaurant", "cafe", "burger", "food", "pizza", "cluckin"], [1.08, 0.98, 0.84], 1.0),
    ("Hôpital / clinique", &["hosp", "clinic", "morgue", "med", "doctor", "surgery"], [0.92, 1.04, 1.06], 1.15),
    ("Police / prison", &["police", "cop", "prison", "jail", "cell", "sheriff"], [0.92, 0.96, 1.06], 0.9),
    ("Église", &["church", "chapel"], [1.06, 0.98, 0.88], 0.85),
    ("Banque / coffre", &["bank", "vault", "pacific", "fleeca"], [1.02, 1.0, 0.92], 0.95),
    ("Métro / gare", &["metro", "subway", "station", "train"], [0.96, 1.0, 0.94], 0.8),
    ("Salle de sport", &["gym", "fitness"], [0.98, 1.0, 1.02], 1.05),
    ("Studio / musique", &["studio", "recording", "music", "record"], [1.0, 0.86, 1.04], 0.75),
    ("Appartement / maison", &["apart", "apa_", "apt", "house", "home", "franklin", "michael", "trevor", "lester", "motel", "hotel", "res_", "_res", "flat", "living", "bedroom", "mansion", "penthouse", "safehouse", "trailer"], [1.06, 0.97, 0.86], 1.0),
    ("Intérieur (autre)", &["int_", "int0", "interior", "_int"], [1.0, 0.98, 0.95], 0.95),
];

/// Libellé français et teinte approchée d'une ambiance d'après son nom.
pub fn amb_family(name: &str) -> (&'static str, [f32; 3], f32) {
    let n = name.to_lowercase();
    for (label, words, tint, bright) in AMB_FAMILIES.iter() {
        if words.iter().any(|w| n.contains(w)) {
            return (label, *tint, *bright);
        }
    }
    if n.starts_with("int") || n.starts_with("v_") {
        return ("Intérieur (autre)", [1.0, 0.98, 0.95], 0.95);
    }
    ("Autre", [1.0, 1.0, 1.0], 1.0)
}

/// Ambiance d'intérieur (pour la liste courte) : nom en « int… », « v_… », « …_int… », ou famille reconnue.
pub fn amb_is_interior(name: &str) -> bool {
    let n = name.to_lowercase();
    n.starts_with("int") || n.starts_with("v_") || n.contains("_int") || n.contains("interior") || !matches!(amb_family(name).0, "Autre")
}

/// Modificateurs des fichiers timecycle_mods*.xml du jeu : nom → (paramètre, première valeur). Format :
/// `<modifier name="int_x" numMods=".." userFlags=".."><paramètre>v1 v2</paramètre>…</modifier>`.
pub fn parse_tc_mods(xml: &str) -> Vec<(String, Vec<(String, f32)>)> {
    let mut out = Vec::new();
    for part in xml.split("<modifier name=\"").skip(1) {
        let Some(e) = part.find('"') else { continue };
        let name = part[..e].to_string();
        let body = part.split("</modifier>").next().unwrap_or("");
        let body = body.split_once('>').map(|x| x.1).unwrap_or("");
        let mut vals = Vec::new();
        let mut rest = body;
        while let Some(s) = rest.find('<') {
            let after = &rest[s + 1..];
            if after.starts_with('/') || after.starts_with('!') {
                rest = &after[1..];
                continue;
            }
            let Some(gt) = after.find('>') else { break };
            let tag = after[..gt].split_whitespace().next().unwrap_or("").trim_end_matches('/').to_string();
            let txt = &after[gt + 1..];
            let end = txt.find('<').unwrap_or(txt.len());
            if let Some(v) = txt[..end].split_whitespace().next().and_then(|x| x.parse::<f32>().ok()) {
                vals.push((tag, v));
            }
            rest = &txt[end..];
        }
        out.push((name, vals));
    }
    out
}

/// Teinte et luminosité de l'aperçu à partir des valeurs du jeu : exposition (postfx_exposure, en crans), couleur
/// et intensité de la lumière artificielle de l'intérieur (light_artificial_int_down_col_r/g/b, …_intensity),
/// saturation (postfx_desaturation : malgré son nom, 1 = normal et 0 = noir et blanc, ex. CAMERA_BW = 0 dans le
/// jeu). Approximation : le vrai rendu du jeu mélange bien d'autres paramètres.
pub fn amb_from_values(vals: &[(String, f32)]) -> Option<([f32; 3], f32)> {
    let get = |k: &str| vals.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|v| v.1);
    let exposure = get("postfx_exposure");
    let col = match (get("light_artificial_int_down_col_r"), get("light_artificial_int_down_col_g"), get("light_artificial_int_down_col_b")) {
        (Some(r), Some(g), Some(b)) => Some([r, g, b]),
        _ => None,
    };
    let inten = get("light_artificial_int_down_intensity");
    let sat = get("postfx_desaturation");
    if exposure.is_none() && col.is_none() && inten.is_none() {
        return None;
    }
    let mut tint = [1.0f32; 3];
    if let Some(c) = col {
        let m = (c[0] + c[1] + c[2]) / 3.0;
        if m > 0.01 {
            tint = [c[0] / m, c[1] / m, c[2] / m];
        }
    }
    if let Some(s) = sat {
        let s = s.clamp(0.0, 2.0);
        for t in tint.iter_mut() {
            *t = 1.0 + (*t - 1.0) * s;
        }
    }
    let mut bright = 1.0f32;
    if let Some(e) = exposure {
        bright *= 2f32.powf(e.clamp(-3.0, 3.0) * 0.5);
    }
    if let Some(i) = inten {
        // intensité du jeu autour de 1 à 4 dans les intérieurs : rapportée à 2 — **estimé**
        bright *= (0.55 + 0.45 * (i / 2.0)).clamp(0.35, 1.5);
    }
    Some((tint.map(|t| t.clamp(0.4, 1.8)), bright.clamp(0.25, 1.8)))
}

// ------------------------------------------------------------------ portails

/// Écart maximal entre deux portails réunis (m).
pub const MERGE_GAP: f32 = 1.5;
/// Surface au-delà de laquelle un portail n'est plus « petit » (m²) — **estimé** (une fenêtre, une porte double).
pub const MERGE_MAX_AREA: f32 = 9.0;
/// Message quand on veut changer l'intérieur d'une ressource MLO importée.
pub const IMPORTED_MLO_LOCKED: &str = "Intérieur d'une ressource MLO importée : ses fichiers sont recopiés tels quels à l'export, donc meubles, ambiances et portails changés ici ne seraient pas exportés. Crée ton propre intérieur (onglet Intérieur) pour le meubler.";
/// Bas d'un portail à moins de cette hauteur du sol de sa pièce : c'est une porte (m) — **estimé**.
const FLOOR_DOOR_EPS: f32 = 0.15;
/// Écart au plan du mur pour dire que deux portails sont sur le même mur (m) — **estimé**.
const MERGE_PLANE_EPS: f32 = 0.12;

/// Repère d'un portail de mur : normale, axe horizontal le long du mur, rectangle (u0, z0, u1, z1) sur ces axes.
fn wall_frame(c: &[V3; 4]) -> Option<(V3, V3, [f32; 4])> {
    let n = (c[1] - c[0]).cross(c[3] - c[0]);
    if n.len() < 1e-4 {
        return None;
    }
    let n = n.norm();
    if n.z.abs() > 0.3 {
        return None; // trémie (sol, plafond) : pas un mur
    }
    let u = v3(-n.y, n.x, 0.0).norm();
    Some((n, u, rect_on(c, u)))
}

fn rect_on(c: &[V3; 4], u: V3) -> [f32; 4] {
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in c {
        let a = p.dot(u);
        r[0] = r[0].min(a);
        r[1] = r[1].min(p.z);
        r[2] = r[2].max(a);
        r[3] = r[3].max(p.z);
    }
    r
}

/// Distance entre deux rectangles (u0, z0, u1, z1).
fn rect_gap(a: [f32; 4], b: [f32; 4]) -> f32 {
    let du = (b[0] - a[2]).max(a[0] - b[2]).max(0.0);
    let dz = (b[1] - a[3]).max(a[1] - b[3]).max(0.0);
    (du * du + dz * dz).sqrt()
}

fn rect_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] - 0.01 && b[0] < a[2] - 0.01 && a[1] < b[3] - 0.01 && b[1] < a[3] - 0.01
}

/// Réunit les petits portails voisins vers l'extérieur (pièce 0) d'un intérieur : même pièce (donc même étage),
/// même mur (coplanaires), écart inférieur à `gap` ; le portail réuni est le rectangle qui les englobe, à la place
/// du premier. Ceux qui portent une porte (MloArchetype::door_portal, comme portal_attached) restent séparés, et
/// une réunion qui recouvrirait l'un d'eux est abandonnée. Rend les nouveaux portails et le nombre de réunions.
pub fn merge_exit_portals(m: &MloArchetype, gap: f32) -> (Vec<Portal>, usize) {
    let np = m.portals.len();
    // portails qui portent une porte (même règle que portal_attached, en un seul passage sur les objets)
    let mut locked = vec![false; np];
    for ei in 0..m.entities.len() {
        if let Some(pi) = m.door_portal(ei) {
            locked[pi] = true;
        }
    }
    // passage au ras du sol de sa pièce (porte d'entrée sans objet porte, comme celle de build_mlo) : jamais réuni
    // avec une fenêtre (le rectangle réuni descendrait jusqu'au sol entre les deux)
    for (i, p) in m.portals.iter().enumerate() {
        let room = if p.room_from == 0 { p.room_to } else { p.room_from } as usize;
        if let Some(r) = m.rooms.get(room).filter(|_| room > 0) {
            let zl = p.corners.iter().map(|c| c.z).fold(f32::MAX, f32::min);
            if (zl - r.bb_min.z).abs() < FLOOR_DOOR_EPS {
                locked[i] = true;
            }
        }
    }
    let frames: Vec<Option<(V3, V3, [f32; 4])>> = m.portals.iter().map(|p| wall_frame(&p.corners)).collect();
    let area = |p: &Portal| (p.corners[1] - p.corners[0]).cross(p.corners[3] - p.corners[0]).len();
    let cand: Vec<bool> = (0..np)
        .map(|i| {
            let p = &m.portals[i];
            (p.room_from == 0) != (p.room_to == 0) && !locked[i] && frames[i].is_some() && area(p) <= MERGE_MAX_AREA
        })
        .collect();
    let coplanar = |i: usize, j: usize| -> bool {
        let (Some((ni, _, _)), Some((nj, _, _))) = (frames[i], frames[j]) else { return false };
        ni.dot(nj).abs() > 0.995 && (ni.dot(m.portals[j].corners[0]) - ni.dot(m.portals[i].corners[0])).abs() < MERGE_PLANE_EPS
    };
    // groupes (union-find) : même sens et même pièce, même mur, assez proches
    let mut parent: Vec<usize> = (0..np).collect();
    fn root(p: &mut Vec<usize>, i: usize) -> usize {
        let mut i = i;
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for i in 0..np {
        if !cand[i] {
            continue;
        }
        for j in i + 1..np {
            if !cand[j] {
                continue;
            }
            let (a, b) = (&m.portals[i], &m.portals[j]);
            if (a.room_from, a.room_to) != (b.room_from, b.room_to) || !coplanar(i, j) {
                continue;
            }
            let u = frames[i].map(|f| f.1).unwrap_or(V3::ZERO);
            if rect_gap(rect_on(&a.corners, u), rect_on(&b.corners, u)) < gap {
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                if ri != rj {
                    parent[rj.max(ri)] = ri.min(rj);
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..np {
        if cand[i] {
            let r = root(&mut parent, i);
            groups.entry(r).or_default().push(i);
        }
    }
    let mut replace: HashMap<usize, Portal> = HashMap::new();
    let mut drop: HashSet<usize> = HashSet::new();
    let mut merged = 0;
    let mut keys: Vec<usize> = groups.keys().copied().collect();
    keys.sort();
    for k in keys {
        let g = &groups[&k];
        if g.len() < 2 {
            continue;
        }
        let first = g[0];
        let Some((n0, u, _)) = frames[first] else { continue };
        let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for &i in g {
            let q = rect_on(&m.portals[i].corners, u);
            r = [r[0].min(q[0]), r[1].min(q[1]), r[2].max(q[2]), r[3].max(q[3])];
        }
        // ne pas recouvrir un portail gardé à part (porte) ni un autre portail du même mur hors du groupe
        let covers = (0..np).filter(|j| !g.contains(j)).any(|j| coplanar(first, j) && rect_overlap(r, rect_on(&m.portals[j].corners, u)));
        if covers {
            continue;
        }
        // coins sur le plan du premier portail, dans le même sens de rotation que lui
        let o = m.portals[first].corners[0];
        let at = |a: f32, z: f32| o + u * (a - o.dot(u)) + v3(0.0, 0.0, z - o.z);
        let mut c = [at(r[0], r[1]), at(r[0], r[3]), at(r[2], r[3]), at(r[2], r[1])];
        if (c[1] - c[0]).cross(c[3] - c[0]).dot(n0) < 0.0 {
            c = [at(r[2], r[1]), at(r[2], r[3]), at(r[0], r[3]), at(r[0], r[1])];
        }
        let mut p = m.portals[first].clone();
        p.corners = c;
        // ouvertures d'origine gardées : la coque et la collision ne percent qu'elles (shell.rs)
        p.openings = g.iter().flat_map(|&i| m.portals[i].holes()).collect();
        replace.insert(first, p);
        drop.extend(g.iter().skip(1).copied());
        merged += 1;
    }
    let out = (0..np).filter(|i| !drop.contains(i)).map(|i| replace.remove(&i).unwrap_or_else(|| m.portals[i].clone())).collect();
    (out, merged)
}

// ------------------------------------------------------------------ dans l'application

impl DwApp {
    /// Tous les objets d'intérieur du jeu (fait une fois, le jeu chargé).
    fn furn_items(&mut self) -> &Vec<FurnItem> {
        self.furn_check_world();
        if self.furn.items.is_none() {
            let mut v = Vec::new();
            if let Some(w) = self.game.world.as_ref() {
                for (h, a) in &w.archetypes {
                    if a.mlo {
                        continue;
                    }
                    let Some(name) = w.names.get(h) else { continue };
                    let size = a.bb_max - a.bb_min;
                    let cats = furn_cats(name, size);
                    if cats != 0 {
                        v.push(FurnItem { hash: *h, name: name.clone(), cats, size });
                    }
                }
                v.sort_by(|a, b| a.name.cmp(&b.name));
                self.furn.items = Some(v);
            } else {
                // jeu pas encore chargé : rien gardé, la liste sera faite au chargement
                static EMPTY: Vec<FurnItem> = Vec::new();
                return &EMPTY;
            }
        }
        self.furn.items.as_ref().unwrap()
    }

    /// Jeu rechargé (ou retiré) depuis la liste des meubles et des ambiances : elles sont refaites.
    fn furn_check_world(&mut self) {
        let key = self.game.world.as_deref().map(|w| w as *const crate::gta::world::GameWorld as usize).unwrap_or(0);
        if key != self.furn.world_key {
            self.furn.world_key = key;
            self.furn.items = None;
            self.furn.ambs = None;
            self.furn.view.clear();
            self.furn.view_key = (usize::MAX, String::new());
        }
    }

    fn furn_refresh_view(&mut self) {
        self.furn_check_world();
        let key = (self.furn.cat, self.furn.search.clone());
        // même filtre sur la même liste : rien à refaire (même quand il ne donne rien)
        if self.furn.items.is_some() && key == self.furn.view_key {
            return;
        }
        let cat = self.furn.cat;
        let search = self.furn.search.clone();
        let items = self.furn_items();
        let view: Vec<usize> = items.iter().enumerate().filter(|(_, it)| (cat == 0 || it.cats & (1 << cat) != 0) && furn_matches(&it.name, &search)).map(|(i, _)| i).collect();
        self.furn.view = view;
        self.furn.view_key = key;
    }

    /// Ambiances du jeu (lues une fois), avec libellé et teinte de l'aperçu.
    pub fn ambiances(&mut self) -> &Vec<Ambiance> {
        self.furn_check_world();
        if self.furn.ambs.is_none() {
            static EMPTY: Vec<Ambiance> = Vec::new();
            let Some(w) = self.game.world.as_ref() else { return &EMPTY };
            let mut map: std::collections::BTreeMap<String, Vec<(String, f32)>> = Default::default();
            for f in w.vfs.files.iter().filter(|f| f.name.starts_with("timecycle_mods") && f.name.ends_with(".xml")) {
                let Some(d) = w.vfs.read_binary(f, &w.keys) else { continue };
                for (n, vals) in parse_tc_mods(&String::from_utf8_lossy(&d)) {
                    map.entry(n).or_insert(vals);
                }
            }
            let mut v: Vec<Ambiance> = map
                .into_iter()
                .map(|(name, vals)| {
                    let (label, ftint, fbright) = amb_family(&name);
                    let (tint, bright, from_game) = match amb_from_values(&vals) {
                        Some((t, b)) => (t, b, true),
                        None => (ftint, fbright, false),
                    };
                    let interior = amb_is_interior(&name);
                    Ambiance { name, label, tint, bright, from_game, interior }
                })
                .collect();
            v.sort_by_key(|a| a.name.to_lowercase());
            self.furn.ambs = Some(v);
        }
        self.furn.ambs.as_ref().unwrap()
    }

    /// Tous les modificateurs d'une ambiance du jeu, avec ses deux valeurs et ses drapeaux (relus dans les fichiers
    /// timecycle_mods*.xml, le premier fichier qui la contient l'emporte, comme pour la liste).
    pub fn game_tc_full(&self, name: &str) -> Option<(u32, Vec<crate::ambiance::TcMod>)> {
        let w = self.game.world.as_ref()?;
        for f in w.vfs.files.iter().filter(|f| f.name.starts_with("timecycle_mods") && f.name.ends_with(".xml")) {
            let Some(d) = w.vfs.read_binary(f, &w.keys) else { continue };
            if let Some((_, flags, mods)) = crate::ambiance::parse_tc_full(&String::from_utf8_lossy(&d)).into_iter().find(|m| m.0.eq_ignore_ascii_case(name)) {
                return Some((flags, mods));
            }
        }
        None
    }

    /// Ambiance du projet portant ce nom dans le jeu (« dw_… »).
    pub fn proj_amb(&self, name: &str) -> Option<&crate::ambiance::ProjAmbiance> {
        if name.is_empty() {
            return None;
        }
        self.proj.ambiances.iter().find(|a| a.game_name().eq_ignore_ascii_case(name))
    }

    /// Teinte et luminosité de l'aperçu pour un nom d'ambiance (table approchée si le jeu ne la donne pas). Ambiance
    /// du projet : d'après ses valeurs, sinon celles de son ambiance de départ.
    pub fn amb_look(&self, name: &str) -> ([f32; 3], f32, bool) {
        let mut name = name;
        if let Some(p) = self.proj_amb(name) {
            if let Some((t, b)) = amb_from_values(&p.values()) {
                return (t, b, true);
            }
            if !p.base.is_empty() {
                name = &p.base;
            }
        }
        if let Some(a) = self.furn.ambs.as_ref().and_then(|v| v.iter().find(|a| a.name.eq_ignore_ascii_case(name))) {
            return (a.tint, a.bright, a.from_game);
        }
        let (_, t, b) = amb_family(name);
        (t, b, false)
    }

    /// Partie « pièce choisie » de l'onglet Intérieur : ambiance, meubles à la main, portails.
    pub fn interior_room_tools(&mut self, ui: &mut egui::Ui) {
        let Some((t, m)) = self.cur_mlo else { return };
        let Some(mlo) = self.proj.ytyps.get(t).and_then(|y| y.mlos.get(m)) else { return };
        let names: Vec<String> = mlo.rooms.iter().map(|r| r.name.clone()).collect();
        if names.len() <= 1 {
            return;
        }
        if self.furn.room.map(|r| r == 0 || r >= names.len()).unwrap_or(true) {
            self.furn.room = Some(1);
        }
        let ri = self.furn.room.unwrap_or(1);
        ui.separator();
        if std::mem::take(&mut self.furn.scroll_to) {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Pièce choisie").strong());
            egui::ComboBox::from_id_salt("furn_room").selected_text(format!("{ri} · {}", names[ri])).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate().skip(1) {
                    ui.selectable_value(&mut self.furn.room, Some(i), format!("{i} · {n}"));
                }
            });
        });
        let ri = self.furn.room.unwrap_or(1);
        if self.mlo_ytyp_imported(t) {
            ui.label(RichText::new(IMPORTED_MLO_LOCKED).small().color(Color32::from_rgb(255, 190, 110)));
            return;
        }
        self.ambiance_ui(ui, t, m, ri);
        self.furniture_ui(ui);
        self.portals_ui(ui, Some((t, m)));
    }

    fn ambiance_ui(&mut self, ui: &mut egui::Ui, t: usize, m: usize, ri: usize) {
        let has_game = self.game.world.is_some();
        let cur = self.proj.ytyps[t].mlos[m].rooms[ri].timecycle.clone();
        let mut set: Option<String> = None;
        let mut all = false;
        self.furn.amb_hover = None;
        egui::CollapsingHeader::new(RichText::new("Ambiance de la pièce").strong()).id_salt("amb_hdr").default_open(true).show(ui, |ui| {
            let (label, _, _) = amb_family(&cur);
            ui.horizontal_wrapped(|ui| {
                ui.label("Actuelle :");
                if cur.is_empty() {
                    ui.label(RichText::new("aucune (celle du dehors)").weak());
                } else {
                    ui.label(RichText::new(&cur).strong().color(Color32::from_rgb(255, 200, 90)));
                    ui.label(RichText::new(format!("({label})")).weak());
                }
                if !cur.is_empty() && ui.small_button("Retirer").clicked() {
                    set = Some(String::new());
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.furn.amb_preview, "Aperçu dans la vue").on_hover_text("La pièce choisie prend dans la vue une teinte et une luminosité proches de son ambiance (survol de la liste : celle survolée)");
                ui.checkbox(&mut self.furn.amb_all, "Toutes les ambiances").on_hover_text("Sinon seulement celles des intérieurs");
            });
            if !has_game {
                ui.label(RichText::new("Charge le jeu (onglet Jeu) pour avoir la liste de ses ambiances ; sinon écris le nom dans la liste des pièces.").small().weak());
            } else {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(egui::TextEdit::singleline(&mut self.furn.amb_search).hint_text("garage, office, bar, apart, dark…").desired_width(200.0));
                });
            }
            let search = self.furn.amb_search.to_lowercase();
            let show_all = self.furn.amb_all;
            let matches = |a: &Ambiance| search.is_empty() || a.name.to_lowercase().contains(&search) || a.label.to_lowercase().contains(&search);
            // ambiances du projet en tête
            let mut list: Vec<Ambiance> = self
                .proj
                .ambiances
                .iter()
                .map(|p| {
                    let name = p.game_name();
                    let (tint, bright, _) = self.amb_look(&name);
                    Ambiance { name, label: PROJ_AMB_LABEL, tint, bright, from_game: true, interior: true }
                })
                .filter(|a| matches(a))
                .collect();
            let n_proj = list.len();
            if has_game {
                list.extend(self.ambiances().iter().filter(|a| (show_all || a.interior) && matches(a)).cloned());
            }
            if list.is_empty() {
                return;
            }
            ui.label(RichText::new(if n_proj > 0 { format!("{} ambiance(s), dont {n_proj} du projet", list.len()) } else { format!("{} ambiance(s)", list.len()) }).small().weak());
            let mut hover = None;
            egui::ScrollArea::vertical().id_salt("amb_list").max_height(170.0).min_scrolled_height(170.0).show_rows(ui, 18.0, list.len(), |ui, range| {
                for a in &list[range] {
                    ui.horizontal(|ui| {
                        // pastille : teinte et luminosité de l'aperçu
                        let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                        let k = a.bright.min(1.4) * 170.0;
                        let col = Color32::from_rgb((a.tint[0] * k).min(255.0) as u8, (a.tint[1] * k).min(255.0) as u8, (a.tint[2] * k).min(255.0) as u8);
                        ui.painter().rect_filled(r, 3.0, col);
                        let on = a.name.eq_ignore_ascii_case(&cur);
                        let txt = if a.label == PROJ_AMB_LABEL { RichText::new(format!("{} · {}", a.label, a.name)).color(Color32::from_rgb(140, 210, 255)) } else { RichText::new(format!("{} · {}", a.label, a.name)) };
                        let resp = ui.selectable_label(on, txt).on_hover_text(if a.label == PROJ_AMB_LABEL { "Ambiance du projet (réglée plus bas, écrite dans dw_timecycmod.xml à l'export) — aperçu approché" } else if a.from_game { "Aperçu d'après les valeurs du jeu (exposition, lumière de l'intérieur) — approché" } else { "Aperçu approché d'après le nom (le jeu ne donne pas de lumière pour cette ambiance)" });
                        if resp.hovered() {
                            hover = Some(a.name.clone());
                        }
                        if resp.clicked() {
                            set = Some(a.name.clone());
                        }
                    });
                }
            });
            self.furn.amb_hover = hover;
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!cur.is_empty(), egui::Button::new("Appliquer à toutes les pièces")).on_hover_text("Toutes les pièces de cet intérieur prennent l'ambiance de la pièce choisie").clicked() {
                    all = true;
                }
            });
            ui.label(RichText::new("L'aperçu est approché : le jeu mélange aussi le ciel, le brouillard et les reflets.").small().weak());
        });
        if let Some(tc) = set {
            self.push_undo();
            self.proj.ytyps[t].mlos[m].rooms[ri].timecycle = tc.clone();
            self.status = if tc.is_empty() { format!("Pièce {ri} : ambiance retirée.") } else { format!("Pièce {ri} : ambiance « {tc} » ({}).", self.amb_label(&tc)) };
        }
        if all {
            let tc = self.proj.ytyps[t].mlos[m].rooms[ri].timecycle.clone();
            self.push_undo();
            let mut n = 0;
            for r in self.proj.ytyps[t].mlos[m].rooms.iter_mut().skip(1) {
                r.timecycle = tc.clone();
                n += 1;
            }
            self.status = format!("Ambiance « {tc} » appliquée aux {n} pièces.");
        }
        self.project_ambiance_ui(ui, t, m, ri);
    }

    /// Libellé d'une ambiance : « Du projet » ou la famille devinée d'après le nom.
    pub fn amb_label(&self, name: &str) -> &'static str {
        if self.proj_amb(name).is_some() {
            PROJ_AMB_LABEL
        } else {
            amb_family(name).0
        }
    }

    /// Bloc « Ambiances du projet » : créer depuis l'ambiance de la pièce, régler, renommer, supprimer.
    fn project_ambiance_ui(&mut self, ui: &mut egui::Ui, t: usize, m: usize, ri: usize) {
        use crate::ambiance::{self as amb, ProjAmbiance};
        enum Act {
            Create,
            Rename(usize, String),
            Use(usize),
            Delete(usize),
            Reset(usize),
        }
        let has_game = self.game.world.is_some();
        let cur = self.proj.ytyps[t].mlos[m].rooms[ri].timecycle.clone();
        if self.furn.pamb_sel.is_some_and(|i| i >= self.proj.ambiances.len()) {
            self.furn.pamb_sel = None;
        }
        let sel = self.furn.pamb_sel;
        let mut act: Option<Act> = None;
        let mut edit: Option<ProjAmbiance> = sel.map(|i| self.proj.ambiances[i].clone());
        let mut new_sel = sel;
        let uses: Vec<usize> = self.proj.ambiances.iter().map(|a| a.game_name()).map(|gn| self.proj.ytyps.iter().flat_map(|y| y.mlos.iter()).flat_map(|m| m.rooms.iter()).filter(|r| r.timecycle.eq_ignore_ascii_case(&gn)).count()).collect();
        egui::CollapsingHeader::new(RichText::new("Ambiances du projet").strong()).id_salt("pamb_hdr").default_open(true).show(ui, |ui| {
            ui.label(RichText::new("Ta propre ambiance : copie de toutes les valeurs d'une ambiance du jeu, puis réglages. Écrite à l'export dans dw_timecycmod.xml (déclarée dans le fxmanifest).").small().weak());
            let from = if cur.is_empty() { "aucune ambiance : partie de rien".to_string() } else { format!("copie de « {cur} »") };
            if ui.button("Créer depuis l'ambiance de cette pièce").on_hover_text(format!("{from} ; la pièce prend la nouvelle ambiance")).clicked() {
                act = Some(Act::Create);
            }
            for (i, a) in self.proj.ambiances.iter().enumerate() {
                let txt = format!("{} · {} · {} pièce(s)", a.name, a.game_name(), uses[i]);
                if ui.selectable_label(sel == Some(i), txt).on_hover_text("Choisir pour la régler").clicked() {
                    new_sel = if sel == Some(i) { None } else { Some(i) };
                }
            }
            let (Some(i), Some(e)) = (sel, edit.as_mut()) else { return };
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&e.name).strong().color(Color32::from_rgb(140, 210, 255)));
                ui.label(RichText::new(format!("(dans le jeu : {})", e.game_name())).weak());
            });
            ui.label(RichText::new(if e.base.is_empty() { format!("{} modificateur(s) ; pas d'ambiance du jeu de départ", e.mods.len()) } else { format!("{} modificateur(s) ; départ : {}", e.mods.len(), e.base) }).small().weak());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.furn.pamb_rename).desired_width(150.0));
                if ui.add_enabled(!self.furn.pamb_rename.trim().is_empty() && self.furn.pamb_rename.trim() != e.name, egui::Button::new("Renommer")).on_hover_text("Les pièces qui l'utilisent suivent").clicked() {
                    act = Some(Act::Rename(i, self.furn.pamb_rename.trim().to_string()));
                }
            });
            egui::Grid::new("pamb_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("Couleur de la lumière").on_hover_text("Lumière artificielle de l'intérieur, vers le bas et vers le haut (light_artificial_int_down_col / up_col)");
                let mut c = e.color();
                if ui.color_edit_button_rgb(&mut c).changed() {
                    e.set_color(c);
                }
                ui.end_row();
                for r in amb::REGLAGES.iter() {
                    ui.label(r.label).on_hover_text(r.help);
                    let mut v = e.get(r.params[0]).unwrap_or(r.default);
                    if ui.add(egui::Slider::new(&mut v, r.min..=r.max).max_decimals(2)).on_hover_text(r.help).changed() {
                        for p in r.params {
                            e.set(p, v);
                        }
                    }
                    ui.end_row();
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!cur.eq_ignore_ascii_case(&e.game_name()), egui::Button::new("Utiliser pour cette pièce")).clicked() {
                    act = Some(Act::Use(i));
                }
                if ui.add_enabled(has_game && !e.base.is_empty(), egui::Button::new("Revenir aux valeurs du jeu")).on_hover_text("Recopie toutes les valeurs de l'ambiance de départ").clicked() {
                    act = Some(Act::Reset(i));
                }
                if ui.button("Supprimer").on_hover_text("Les pièces qui l'utilisent reprennent son ambiance de départ").clicked() {
                    act = Some(Act::Delete(i));
                }
            });
            ui.label(RichText::new("Réglages approchés : le jeu mélange aussi le ciel, le brouillard et les reflets. Les autres valeurs copiées restent telles quelles.").small().weak());
        });
        if new_sel != sel {
            self.furn.pamb_sel = new_sel;
            self.furn.pamb_rename = new_sel.map(|i| self.proj.ambiances[i].name.clone()).unwrap_or_default();
            return;
        }
        // réglages : une annulation par geste (glissière tenue, couleur choisie)
        if let (Some(i), Some(e)) = (sel, edit) {
            if self.proj.ambiances[i] != e {
                if !self.furn.pamb_editing {
                    self.push_undo();
                    self.furn.pamb_editing = true;
                }
                self.proj.ambiances[i] = e;
            } else if !ui.input(|inp| inp.pointer.any_down()) {
                self.furn.pamb_editing = false;
            }
        }
        let Some(act) = act else { return };
        let room_set = |s: &mut Self, old: &str, new: &str| {
            let mut n = 0;
            for r in s.proj.ytyps.iter_mut().flat_map(|y| y.mlos.iter_mut()).flat_map(|m| m.rooms.iter_mut()) {
                if r.timecycle.eq_ignore_ascii_case(old) {
                    r.timecycle = new.to_string();
                    n += 1;
                }
                if r.secondary_timecycle.eq_ignore_ascii_case(old) {
                    r.secondary_timecycle = new.to_string();
                }
            }
            n
        };
        match act {
            Act::Create => {
                let (name, base, flags, mods, copied) = if let Some(p) = self.proj_amb(&cur) {
                    (format!("{} copie", p.name), p.base.clone(), p.user_flags, p.mods.clone(), true)
                } else if cur.is_empty() {
                    ("ambiance".to_string(), String::new(), 0, Vec::new(), false)
                } else {
                    match self.game_tc_full(&cur) {
                        Some((f, mods)) => (cur.clone(), cur.clone(), f, mods, true),
                        None => (cur.clone(), cur.clone(), 0, Vec::new(), false),
                    }
                };
                let name = amb::unique_name(&self.proj.ambiances, &name);
                let a = ProjAmbiance { name, base, user_flags: flags, mods };
                let gn = a.game_name();
                let n = a.mods.len();
                self.push_undo();
                self.furn.pamb_rename = a.name.clone();
                self.proj.ambiances.push(a);
                self.proj.ytyps[t].mlos[m].rooms[ri].timecycle = gn.clone();
                self.furn.pamb_sel = Some(self.proj.ambiances.len() - 1);
                self.status = if copied {
                    format!("Ambiance du projet « {gn} » créée ({n} valeur(s) copiée(s)) ; la pièce {ri} la prend. Règle-la dans « Ambiances du projet ».")
                } else if cur.is_empty() || !has_game {
                    format!("Ambiance du projet « {gn} » créée sans valeurs de départ{} ; la pièce {ri} la prend.", if cur.is_empty() { "" } else { " (charge le jeu pour copier celles de l'ambiance de la pièce)" })
                } else {
                    format!("Ambiance du projet « {gn} » créée : « {cur} » introuvable dans le jeu, aucune valeur copiée ; la pièce {ri} la prend.")
                };
            }
            Act::Rename(i, wanted) => {
                let old = self.proj.ambiances[i].game_name();
                let others: Vec<ProjAmbiance> = self.proj.ambiances.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, a)| a.clone()).collect();
                let name = amb::unique_name(&others, &wanted);
                self.push_undo();
                self.proj.ambiances[i].name = name.clone();
                let gn = self.proj.ambiances[i].game_name();
                let n = room_set(self, &old, &gn);
                self.furn.pamb_rename = name;
                self.status = format!("Ambiance renommée « {gn} » ({n} pièce(s) suivent).");
            }
            Act::Use(i) => {
                let gn = self.proj.ambiances[i].game_name();
                self.push_undo();
                self.proj.ytyps[t].mlos[m].rooms[ri].timecycle = gn.clone();
                self.status = format!("Pièce {ri} : ambiance du projet « {gn} ».");
            }
            Act::Reset(i) => {
                let base = self.proj.ambiances[i].base.clone();
                match self.game_tc_full(&base) {
                    Some((f, mods)) => {
                        self.push_undo();
                        let a = &mut self.proj.ambiances[i];
                        a.user_flags = f;
                        a.mods = mods;
                        self.status = format!("« {} » : valeurs de « {base} » recopiées.", a.game_name());
                    }
                    None => self.status = format!("« {base} » introuvable dans les fichiers du jeu."),
                }
            }
            Act::Delete(i) => {
                let a = self.proj.ambiances[i].clone();
                self.push_undo();
                self.proj.ambiances.remove(i);
                let n = room_set(self, &a.game_name(), &a.base);
                self.furn.pamb_sel = None;
                self.status = format!("Ambiance du projet « {} » supprimée ; {n} pièce(s) reprennent « {} ».", a.game_name(), if a.base.is_empty() { "aucune" } else { &a.base });
            }
        }
    }

    fn furniture_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("Meubler à la main").strong()).id_salt("furn_hdr").default_open(true).show(ui, |ui| {
            if self.game.world.is_none() {
                ui.label(RichText::new("Charge le jeu (onglet Jeu) pour avoir ses meubles. « Meubler automatiquement » marche sans.").small().weak());
                return;
            }
            ui.horizontal_wrapped(|ui| {
                for (i, (n, _)) in FURN_CATS.iter().enumerate() {
                    let on = self.furn.cat == i;
                    let txt = if on { RichText::new(*n).color(Color32::BLACK).small() } else { RichText::new(*n).small() };
                    if ui.add(egui::Button::new(txt).fill(if on { Color32::from_rgb(255, 170, 60) } else { Color32::from_gray(45) }).corner_radius(8.0)).clicked() {
                        self.furn.cat = i;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("🔍");
                ui.add(egui::TextEdit::singleline(&mut self.furn.search).hint_text("canapé, lit, lamp, v_res…").desired_width(200.0));
            });
            self.furn_refresh_view();
            let total = self.furn.view.len();
            ui.label(RichText::new(format!("{total} objet(s) · clic sur une image : il suit la souris dans la pièce (molette = tourner, Maj + molette = quart de tour, clic = poser, Échap = fini)")).small().weak());
            let w = ui.available_width().max(FURN_CELL * 2.0);
            let cols = ((w / (FURN_CELL + 6.0)).floor() as usize).max(1);
            let rows = total.div_ceil(cols);
            let row_h = FURN_CELL + 18.0;
            let mut todo: Vec<u32> = Vec::new();
            let mut pick: Option<String> = None;
            let placing = self.furn.place.as_ref().map(|p| p.arch.to_lowercase());
            egui::ScrollArea::vertical().id_salt("furn_grid").max_height(row_h * 3.2).min_scrolled_height(row_h * 3.2).auto_shrink([false, true]).show_rows(ui, row_h, rows, |ui, range| {
                for r in range {
                    ui.horizontal(|ui| {
                        for c in 0..cols {
                            let k = r * cols + c;
                            if k >= total {
                                break;
                            }
                            let Some(it) = self.furn.items.as_ref().and_then(|v| v.get(self.furn.view[k])).cloned() else { continue };
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(FURN_CELL + 2.0, row_h - 2.0), Sense::click());
                            let img = egui::Rect::from_min_size(rect.min + egui::vec2(1.0, 1.0), egui::vec2(FURN_CELL, FURN_CELL));
                            let p = ui.painter_at(rect);
                            p.rect_filled(img, 4.0, if resp.hovered() { Color32::from_rgb(44, 50, 60) } else { Color32::from_rgb(30, 33, 39) });
                            match self.library.thumbs.get_mut(&it.hash) {
                                Some(th) => {
                                    th.last = self.library.frame;
                                    crate::library::draw_thumb(&p, img.shrink(4.0), th);
                                }
                                None if self.library.missing.contains(&it.hash) => {
                                    p.text(img.center(), egui::Align2::CENTER_CENTER, "pas d'aperçu", egui::FontId::proportional(9.0), Color32::GRAY);
                                }
                                None => {
                                    todo.push(it.hash);
                                    p.text(img.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(14.0), Color32::GRAY);
                                }
                            }
                            if placing.as_deref() == Some(it.name.to_lowercase().as_str()) {
                                p.rect_stroke(img, 4.0, Stroke::new(2.0_f32, Color32::from_rgb(120, 255, 140)), egui::StrokeKind::Inside);
                            }
                            let short: String = if it.name.chars().count() > 12 { format!("{}…", it.name.chars().take(11).collect::<String>()) } else { it.name.clone() };
                            p.text(egui::pos2(img.center().x, img.max.y + 2.0), egui::Align2::CENTER_TOP, short, egui::FontId::proportional(10.0), Color32::from_gray(190));
                            let resp = resp.on_hover_text(format!("{}\n{:.2} × {:.2} × {:.2} m", it.name, it.size.x, it.size.y, it.size.z));
                            if resp.clicked() {
                                pick = Some(it.name.clone());
                            }
                        }
                    });
                }
            });
            if !todo.is_empty() {
                self.library_make_thumbs(&todo);
                ui.ctx().request_repaint();
            }
            if let Some(n) = pick {
                self.furn_start(&n);
            }
            if let Some(p) = self.furn.place.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Pose : {} · {:.0}° · {} posé(s)", p.arch, p.yaw_deg, p.placed)).color(Color32::from_rgb(120, 255, 140)));
                    if ui.small_button("Finir").clicked() {
                        self.furn_stop();
                    }
                });
            }
        });
    }

    /// Commence la pose d'un meuble : il suit la souris sur le sol de la pièce.
    pub fn furn_start(&mut self, arch: &str) {
        let yaw = self.furn.place.as_ref().map(|p| p.yaw_deg).unwrap_or(0.0);
        self.group_place = None;
        self.furn.place = Some(FurnPlace { arch: arch.to_string(), yaw_deg: yaw, placed: 0 });
        self.status = format!("« {arch} » suit la souris dans la pièce : molette = tourner, Maj + molette = quart de tour, clic = poser, Échap = fini.");
    }

    pub fn furn_stop(&mut self) {
        if let Some(p) = self.furn.place.take() {
            self.status = format!("Pose de « {} » finie ({} posé(s)).", p.arch, p.placed);
        }
        self.furn.at = None;
    }

    /// Point visé sur le sol de la pièce choisie (ou de la pièce sous la souris) : plan horizontal à la hauteur de
    /// son sol, puis la pièce de cet étage qui contient le point.
    pub fn furn_aim(&self, s: Pos2) -> Option<FurnAt> {
        let (t, m) = self.cur_mlo?;
        let mlo = self.proj.ytyps.get(t)?.mlos.get(m)?;
        let (p, q) = self.proj.mlo_placement(&mlo.name)?;
        let floor_room = self.furn.room.filter(|r| *r > 0 && *r < mlo.rooms.len()).or_else(|| self.mouse_ground.and_then(|g| self.mlo_at(g)).filter(|x| x.0 == t && x.1 == m).map(|x| x.2))?;
        let r = &mlo.rooms[floor_room];
        let zf = r.bb_min.z;
        let c = (r.bb_min + r.bb_max) * 0.5;
        let wz = (p + q.rotate(v3(c.x, c.y, zf))).z;
        let g = self.cam.ground_point(s, wz)?;
        let local = q.inverse().rotate(g - p);
        let local = v3(local.x, local.y, zf);
        let room = mlo.rooms.iter().enumerate().skip(1).find(|(_, r)| (r.bb_min.z - zf).abs() < 0.5 && local.x >= r.bb_min.x && local.x <= r.bb_max.x && local.y >= r.bb_min.y && local.y <= r.bb_max.y).map(|(i, _)| i).unwrap_or(0);
        Some((p + q.rotate(local), room, local))
    }

    /// Hauteur de pose dans la pièce : le sol, ou le plafond pour un plafonnier (nom en « ceil »).
    fn furn_z(&self, arch: &str, room: usize, floor_z: f32) -> f32 {
        let Some((t, m)) = self.cur_mlo else { return floor_z };
        let n = arch.to_lowercase();
        if n.contains("ceil") {
            if let Some(r) = self.proj.ytyps[t].mlos[m].rooms.get(room) {
                let (_, mx) = crate::catalog::box_for(arch);
                return r.bb_max.z - mx.z.max(0.0) - 0.02;
            }
        }
        floor_z
    }

    /// Pose le meuble en cours au point visé ; rend l'identifiant.
    pub fn furn_place_at(&mut self, at: FurnAt) -> Option<u64> {
        let (_, room, local) = at;
        let pl = self.furn.place.clone()?;
        let (t, m) = self.cur_mlo?;
        if self.mlo_ytyp_imported(t) {
            self.status = IMPORTED_MLO_LOCKED.into();
            self.furn_stop();
            return None;
        }
        if room == 0 {
            self.status = "Hors des pièces de l'intérieur : vise le sol d'une pièce.".into();
            return None;
        }
        self.push_undo();
        let z = self.furn_z(&pl.arch, room, local.z);
        let mut e = Entity::new(&pl.arch, v3(local.x, local.y, z));
        e.flags = crate::mech::MLO_ENTITY_FLAGS;
        e.room = room;
        e.rot = Quat::from_z(pl.yaw_deg.to_radians());
        let id = e.id;
        self.proj.ytyps[t].mlos[m].entities.push(e);
        if let Some(p) = self.furn.place.as_mut() {
            p.placed += 1;
        }
        let rn = self.proj.ytyps[t].mlos[m].rooms[room].name.clone();
        self.status = format!("« {} » posé dans la pièce {room} « {rn} », tourné de {:.0}° ({} posé(s)). Clic pour un autre, Échap pour finir.", pl.arch, pl.yaw_deg, pl.placed + 1);
        Some(id)
    }

    /// Souris pendant la pose d'un meuble ; rend vrai si l'entrée est prise.
    pub fn furn_input(&mut self, ctx: &egui::Context, resp: &egui::Response, hover: Option<Pos2>) -> bool {
        if self.furn.place.is_none() {
            self.furn.at = None;
            return false;
        }
        if self.cur_mlo.is_none() {
            self.furn_stop();
            return false;
        }
        let (esc, scroll, shift) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, egui::Key::Escape), i.raw_scroll_delta.y + i.raw_scroll_delta.x, i.modifiers.shift));
        if esc || resp.clicked_by(egui::PointerButton::Secondary) {
            self.furn_stop();
            return true;
        }
        if resp.hovered() && scroll != 0.0 {
            if let Some(p) = self.furn.place.as_mut() {
                p.yaw_deg = crate::groups::next_yaw(p.yaw_deg, scroll, shift);
            }
        }
        self.furn.at = hover.and_then(|s| self.furn_aim(s));
        if resp.clicked_by(egui::PointerButton::Primary) {
            if let Some(at) = self.furn.at {
                self.furn_place_at(at);
            }
        }
        true
    }

    /// Le meuble en cours, là où il serait posé (dans le monde) : (type, position, orientation, pose possible —
    /// faux hors des pièces).
    pub(crate) fn furn_ghost_world(&self) -> Option<(String, V3, Quat, bool)> {
        let pl = self.furn.place.as_ref()?;
        let (_, room, local) = self.furn.at?;
        let (t, m) = self.cur_mlo?;
        let (p, q) = self.proj.mlo_placement(&self.proj.ytyps[t].mlos[m].name)?;
        let z = self.furn_z(&pl.arch, room, local.z);
        Some((pl.arch.clone(), p + q.rotate(v3(local.x, local.y, z)), q.mul(Quat::from_z(pl.yaw_deg.to_radians())).normalized(), room > 0))
    }

    /// Point d'ancrage et nom du meuble en cours sous la souris. Le modèle (teinté en rouge hors des pièces) et,
    /// s'il n'est pas encore chargé, son cadre sont l'aperçu de pose commun (view.rs pose_ghost / draw_ghost_frames) :
    /// pas de second cadre ici.
    pub fn draw_furn_ghost(&self, painter: &egui::Painter) {
        let Some((a, p, _, ok)) = self.furn_ghost_world() else { return };
        let room = self.furn.at.map(|x| x.1).unwrap_or(0);
        if let Some((sp, _)) = self.cam.project(p) {
            let col = if ok { Color32::from_rgb(120, 255, 140) } else { Color32::from_rgb(255, 120, 100) };
            let yaw = self.furn.place.as_ref().map(|p| p.yaw_deg).unwrap_or(0.0);
            let where_ = if ok { format!("pièce {room}") } else { "hors des pièces".into() };
            painter.circle_stroke(sp, 6.0, Stroke::new(2.0_f32, col));
            painter.text(sp + egui::vec2(12.0, -12.0), egui::Align2::LEFT_BOTTOM, format!("{a} · {yaw:.0}° · {where_}"), egui::FontId::proportional(13.0), col);
        }
    }

    /// Aperçu de l'ambiance de la pièce choisie (ou de celle survolée dans la liste) : la pièce, vue à l'écran, est
    /// assombrie ou éclaircie et teintée — **approché** (calque par-dessus la vue, pas le vrai rendu du jeu).
    pub fn draw_ambiance_preview(&self, painter: &egui::Painter, rect: egui::Rect) {
        if self.tab != crate::app::Tab::Interior {
            return;
        }
        let Some((t, m)) = self.cur_mlo else { return };
        let Some(ri) = self.furn.room else { return };
        let Some(mlo) = self.proj.ytyps.get(t).and_then(|y| y.mlos.get(m)) else { return };
        let Some(r) = mlo.rooms.get(ri) else { return };
        let name = match (&self.furn.amb_hover, self.furn.amb_preview) {
            (Some(h), _) => h.clone(),
            (None, true) if !r.timecycle.is_empty() => r.timecycle.clone(),
            _ => return,
        };
        let Some((p, q)) = self.proj.mlo_placement(&mlo.name) else { return };
        let (tint, bright, from_game) = self.amb_look(&name);
        // silhouette de la pièce à l'écran (enveloppe convexe de ses 8 coins) ; caméra dedans : toute la vue
        let mut pts = Vec::new();
        let mut behind = false;
        let cam_local = q.inverse().rotate(self.cam.eye - p);
        let inside = cam_local.x >= r.bb_min.x && cam_local.x <= r.bb_max.x && cam_local.y >= r.bb_min.y && cam_local.y <= r.bb_max.y && cam_local.z >= r.bb_min.z && cam_local.z <= r.bb_max.z;
        for i in 0..8 {
            let c = v3(if i & 1 == 0 { r.bb_min.x } else { r.bb_max.x }, if i & 2 == 0 { r.bb_min.y } else { r.bb_max.y }, if i & 4 == 0 { r.bb_min.z } else { r.bb_max.z });
            match self.cam.project(p + q.rotate(c)) {
                Some((s, _)) => pts.push(s),
                None => behind = true,
            }
        }
        let poly: Vec<Pos2> = if inside || behind || pts.len() < 3 { vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()] } else { convex_hull(pts) };
        for (col, a) in amb_layers(tint, bright) {
            painter.add(egui::Shape::convex_polygon(poly.clone(), Color32::from_rgba_unmultiplied(col[0], col[1], col[2], a), Stroke::NONE));
        }
        let proj = self.proj_amb(&name).is_some();
        let label = format!("Aperçu de l'ambiance « {name} » ({}) — {}", self.amb_label(&name), if proj { "d'après ses réglages, approché" } else if from_game { "d'après les valeurs du jeu, approché" } else { "approché d'après le nom" });
        let pos = rect.center_bottom() + egui::vec2(0.0, -40.0);
        let galley = painter.layout_no_wrap(label, egui::FontId::proportional(13.0), Color32::from_rgb(255, 220, 140));
        let br = egui::Rect::from_center_size(pos, galley.size() + egui::vec2(12.0, 6.0));
        painter.rect_filled(br, 4.0, Color32::from_black_alpha(180));
        painter.galley(br.min + egui::vec2(6.0, 3.0), galley, Color32::WHITE);
    }

    /// Bloc « Portails » : nombre, et « Réduire les portails » avec l'avant / après.
    pub fn portals_ui(&mut self, ui: &mut egui::Ui, only: Option<(usize, usize)>) {
        let (before, after) = self.portal_merge_preview(only);
        let pools = crate::check::game_pools(self.game.world.as_deref());
        let limit = pools.iter().find(|p| p.0 == "PortalInst").map(|p| p.1).unwrap_or(200);
        let total: usize = self.proj.ytyps.iter().flat_map(|t| t.mlos.iter()).map(|m| m.portals.len()).sum();
        let near = total * 2 > limit as usize;
        ui.horizontal_wrapped(|ui| {
            let col = if near { Color32::from_rgb(255, 160, 80) } else { Color32::from_gray(200) };
            ui.label(RichText::new(format!("Portails : {total} dans le projet (réserve du jeu « PortalInst » : {limit})")).color(col));
            let txt = if after < before { format!("Réduire les portails ({before} » {after})") } else { "Réduire les portails".to_string() };
            let b = egui::Button::new(if near { RichText::new(txt).strong() } else { RichText::new(txt) });
            if ui.add_enabled(after < before, b).on_hover_text(format!("Réunit les petits portails voisins vers l'extérieur d'un même mur et d'un même étage (écart < {MERGE_GAP} m) en un seul rectangle de visibilité ; les murs restent percés seulement aux ouvertures d'origine. Seulement les intérieurs créés dans DragoWalker (pas les bâtiments de l'atelier) ; ceux qui ont une porte, et les passages au ras du sol, restent séparés.")).clicked() {
                self.status = self.reduce_portals(only);
                self.furn.merge_report = self.status.clone();
            }
        });
        if !self.furn.merge_report.is_empty() {
            ui.label(RichText::new(&self.furn.merge_report).small().color(Color32::from_rgb(120, 220, 120)));
        }
    }

    /// Intérieurs concernés par la réduction : ceux vraiment créés dans DragoWalker (shell_mlos : ni MLO importé, ni
    /// intérieur du jeu, ni bâtiment de l'atelier, qui gère ses portails lui-même).
    fn own_mlos(&self, only: Option<(usize, usize)>) -> Vec<(usize, usize)> {
        self.shell_mlos().into_iter().filter(|&x| only.map(|o| o == x).unwrap_or(true)).collect()
    }

    /// Nombre de portails avant / après une réduction (sans rien changer). Gardé tant que les intérieurs concernés
    /// (pièces, portails, objets) ne changent pas : le bloc est dessiné à chaque image.
    pub fn portal_merge_preview(&self, only: Option<(usize, usize)>) -> (usize, usize) {
        use std::hash::{Hash, Hasher};
        let list = self.own_mlos(only);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        only.hash(&mut h);
        for &(ti, mi) in &list {
            let m = &self.proj.ytyps[ti].mlos[mi];
            (ti, mi, m.portals.len(), m.entities.len(), m.rooms.len()).hash(&mut h);
            for p in &m.portals {
                (p.room_from, p.room_to, p.openings.len()).hash(&mut h);
                for c in p.corners {
                    (c.x.to_bits(), c.y.to_bits(), c.z.to_bits()).hash(&mut h);
                }
            }
            for e in &m.entities {
                e.archetype.hash(&mut h);
                (e.pos.x.to_bits(), e.pos.y.to_bits(), e.pos.z.to_bits()).hash(&mut h);
            }
            for r in &m.rooms {
                r.bb_min.z.to_bits().hash(&mut h);
            }
        }
        let sig = h.finish() | 1;
        let c = self.furn.merge_cache.get();
        if c.0 == sig {
            return (c.1, c.2);
        }
        let (mut b, mut a) = (0, 0);
        for (ti, mi) in list {
            let m = &self.proj.ytyps[ti].mlos[mi];
            b += m.portals.len();
            a += merge_exit_portals(m, MERGE_GAP).0.len();
        }
        self.furn.merge_cache.set((sig, b, a));
        (b, a)
    }

    /// « Réduire les portails » : réunit les petits portails voisins vers l'extérieur ; compte rendu avant / après.
    pub fn reduce_portals(&mut self, only: Option<(usize, usize)>) -> String {
        let list = self.own_mlos(only);
        let (b, a) = self.portal_merge_preview(only);
        if a >= b {
            return "Aucun portail à réunir (pas de petits portails voisins vers l'extérieur sur un même mur).".into();
        }
        self.push_undo();
        let mut parts = Vec::new();
        for (ti, mi) in list {
            let m = &self.proj.ytyps[ti].mlos[mi];
            let (np, merged) = merge_exit_portals(m, MERGE_GAP);
            if merged == 0 {
                continue;
            }
            let before = m.portals.len();
            let name = m.name.clone();
            parts.push(format!("« {name} » {before} » {}", np.len()));
            self.proj.ytyps[ti].mlos[mi].portals = np;
        }
        let total: usize = self.proj.ytyps.iter().flat_map(|t| t.mlos.iter()).map(|m| m.portals.len()).sum();
        format!("Portails réunis : {b} » {a} ({} de moins) — {} ; {total} portail(s) dans le projet.", b - a, parts.join(", "))
    }

    /// Mode test DW_TEST_MEUBLER=1 : un intérieur 2 × 1 au centre de la vue, ambiance « bar » en aperçu, deux
    /// meubles posés à la souris (dont un quart de tour), puis des fenêtres ajoutées et réunies.
    pub fn meubler_test_tick(&mut self) {
        static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if self.frame != 110 || std::env::var("DW_TEST_MEUBLER").is_err() || DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        self.create_mlo("dw_test_meubles", 2, 1, 6.0, 5.0, 3.0);
        let Some((t, m)) = self.cur_mlo else { return };
        self.tab = crate::app::Tab::Interior;
        self.furn.room = Some(1);
        // catalogue
        self.furn_refresh_view();
        let n_all = self.furn.view.len();
        let mut per = Vec::new();
        for c in 1..FURN_CATS.len() {
            let n = self.furn.items.as_ref().map(|v| v.iter().filter(|it| it.cats & (1 << c) != 0).count()).unwrap_or(0);
            per.push(format!("{} {n}", FURN_CATS[c].0));
        }
        println!("catalogue : {n_all} objets d'intérieur ; {}", per.join(", "));
        self.furn.cat = 1;
        self.furn.search = "canap".into();
        self.furn_refresh_view();
        let sofas: Vec<String> = self.furn.view.iter().take(6).filter_map(|i| self.furn.items.as_ref().map(|v| v[*i].name.clone())).collect();
        println!("recherche « canap » (canapés) : {} : {:?}", self.furn.view.len(), sofas);
        // ambiances
        let n = self.ambiances().len();
        let ambs = self.ambiances().clone();
        let game = ambs.iter().filter(|a| a.from_game).count();
        println!("ambiances : {n} dont {} d'intérieur, {game} avec des valeurs lues", ambs.iter().filter(|a| a.interior).count());
        for a in ambs.iter().filter(|a| a.interior).step_by((ambs.len() / 12).max(1)).take(12) {
            println!("   {} · {} · teinte {:.2} {:.2} {:.2} lum {:.2} {}", a.name, a.label, a.tint[0], a.tint[1], a.tint[2], a.bright, if a.from_game { "(jeu)" } else { "(approché)" });
        }
        let want = std::env::var("DW_TEST_AMB").unwrap_or_default();
        let bar = ambs.iter().find(|a| !want.is_empty() && a.name.eq_ignore_ascii_case(&want)).or_else(|| ambs.iter().find(|a| a.interior && a.from_game && a.label == "Bar / club")).or_else(|| ambs.iter().find(|a| a.interior)).map(|a| a.name.clone()).unwrap_or_else(|| "int_bar_test".into());
        self.proj.ytyps[t].mlos[m].rooms[1].timecycle = bar.clone();
        self.furn.amb_preview = true;
        println!("pièce 1 : ambiance « {bar} » ({})", amb_family(&bar).0);
        // pose à la souris : visée au centre de la vue, puis un point calculé dans chaque pièce
        let arch = sofas.first().cloned().unwrap_or_else(|| "prop_couch_01".into());
        self.furn_start(&arch);
        let name = self.proj.ytyps[t].mlos[m].name.clone();
        if let Some((p, q)) = self.proj.mlo_placement(&name) {
            for (k, ri) in [1usize, 2].iter().enumerate() {
                let r = self.proj.ytyps[t].mlos[m].rooms[*ri].clone();
                let c = (r.bb_min + r.bb_max) * 0.5;
                let w = p + q.rotate(v3(c.x, c.y, r.bb_min.z));
                if k == 1 {
                    if let Some(pl) = self.furn.place.as_mut() {
                        pl.yaw_deg = crate::groups::next_yaw(pl.yaw_deg, 1.0, true);
                    }
                }
                let at = self.cam.project(w).and_then(|(s, _)| self.furn_aim(s));
                match at {
                    Some(at) => {
                        let id = self.furn_place_at(at);
                        let e = id.and_then(|id| self.proj.ytyps[t].mlos[m].entities.iter().find(|e| e.id == id).cloned());
                        println!("visée pièce {ri} : pièce trouvée {} ; posé {:?}", at.1, e.map(|e| (e.archetype, e.room, (e.pos.x * 10.0).round() / 10.0, (e.pos.y * 10.0).round() / 10.0, e.pos.z, e.rot.heading_deg().round(), e.flags)));
                    }
                    None => println!("visée pièce {ri} : hors de la vue"),
                }
            }
        }
        // portails : quatre fenêtres côte à côte sur le mur nord de la pièce 2 (écart 0,6 m)
        {
            let mlo = &mut self.proj.ytyps[t].mlos[m];
            let r = mlo.rooms[2].clone();
            let y = r.bb_max.y;
            for i in 0..4 {
                let x0 = r.bb_min.x + 0.5 + i as f32 * 1.4;
                mlo.portals.push(Portal::new(2, 0, [v3(x0, y, 1.0), v3(x0, y, 2.2), v3(x0 + 0.8, y, 2.2), v3(x0 + 0.8, y, 1.0)]));
            }
        }
        // DW_TEST_PORTALS_N=n : n petites fenêtres en plus (rangées de 10 sur les murs nord et sud), laissées telles
        // quelles pour voir le bouton du contrôle avant l'export (DW_TEST_CHECK=1)
        let many: usize = std::env::var("DW_TEST_PORTALS_N").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        {
            let mlo = &mut self.proj.ytyps[t].mlos[m];
            for k in 0..many {
                let room = 1 + (k / 30) % 2;
                let (row, col) = ((k % 30) / 10, k % 10);
                let r = mlo.rooms[room].clone();
                let y = if (k / 60) % 2 == 0 { r.bb_max.y } else { r.bb_min.y };
                let x0 = r.bb_min.x + 0.1 + col as f32 * 0.58;
                let z0 = 0.3 + row as f32 * 0.8;
                mlo.portals.push(Portal::new(room as u32, 0, [v3(x0, y, z0), v3(x0, y, z0 + 0.6), v3(x0 + 0.4, y, z0 + 0.6), v3(x0 + 0.4, y, z0)]));
            }
        }
        let (b, a) = self.portal_merge_preview(Some((t, m)));
        println!("portails avant / après (aperçu) : {b} » {a}");
        if many == 0 {
            println!("{}", self.reduce_portals(Some((t, m))));
            self.furn.merge_report = self.status.clone();
        }
        // vue : de haut, coupée au-dessus du sol pour voir dedans ; le meuble suit encore la souris (centre)
        if let Some((p, _)) = self.proj.mlo_placement(&name) {
            self.cam.target = p + v3(0.0, 0.0, 1.0);
            self.cam.dist = 16.0;
            if let Some(pitch) = std::env::var("DW_TEST_PITCH").ok().and_then(|v| v.parse::<f32>().ok()) {
                self.cam.pitch = pitch;
            }
            self.cam.snap();
        }
        self.game.cut_on = true;
        self.game.cut_h = 2.4;
        self.opts.show_rooms = std::env::var("DW_TEST_ROOMS_BOX").is_ok();
        self.furn.scroll_to = true;
    }
}

/// Calques de l'aperçu d'ambiance : assombrir (noir) ou éclaircir (blanc), puis teinter ; (couleur, opacité).
pub fn amb_layers(tint: [f32; 3], bright: f32) -> Vec<([u8; 3], u8)> {
    let mut v = Vec::new();
    if bright < 1.0 {
        v.push(([0, 0, 0], ((1.0 - bright) * 255.0 * 0.9).clamp(0.0, 200.0) as u8));
    } else if bright > 1.0 {
        v.push(([255, 255, 255], ((bright - 1.0) * 255.0 * 0.35).clamp(0.0, 90.0) as u8));
    }
    let m = tint.iter().cloned().fold(0.0f32, f32::max).max(0.01);
    let col = tint.map(|c| (c / m * 255.0).clamp(0.0, 255.0) as u8);
    let sat = tint.iter().cloned().fold(0.0f32, f32::max) - tint.iter().cloned().fold(f32::MAX, f32::min);
    let a = (sat * 255.0 * 0.45).clamp(0.0, 110.0) as u8;
    if a > 2 {
        v.push((col, a));
    }
    v
}

/// Enveloppe convexe (ordre des aiguilles, à l'écran) — algorithme de la chaîne monotone.
pub fn convex_hull(mut p: Vec<Pos2>) -> Vec<Pos2> {
    p.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal).then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal)));
    p.dedup_by(|a, b| (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: Pos2, a: Pos2, b: Pos2| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut lower: Vec<Pos2> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<Pos2> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{build_mlo, Portal};

    fn win(room: u32, x0: f32, w: f32, y: f32, z0: f32, z1: f32) -> Portal {
        Portal::new(room, 0, [v3(x0, y, z0), v3(x0, y, z1), v3(x0 + w, y, z1), v3(x0 + w, y, z0)])
    }

    fn area(p: &Portal) -> f32 {
        (p.corners[1] - p.corners[0]).cross(p.corners[3] - p.corners[0]).len()
    }

    #[test]
    fn fenetres_voisines_reunies() {
        let mut m = build_mlo("t", 2, 1, 6.0, 5.0, 3.0);
        let n0 = m.portals.len(); // 1 entre pièces + 1 entrée
        // mur nord de la pièce 2 (x 0..6, y = 2.5) : trois fenêtres à 1 m l'une de l'autre, une quatrième à 3 m
        for x in [0.5f32, 1.8, 3.1] {
            m.portals.push(win(2, x, 0.3, 2.5, 1.0, 2.2));
        }
        m.portals.push(win(2, 0.5, 0.3, 2.5, 1.0, 2.2)); // doublon exact : réuni aussi
        m.portals.push(win(2, 5.5, 0.3, 2.5, 1.0, 2.2)); // trop loin (écart 2,1 m)
        let (out, merged) = merge_exit_portals(&m, MERGE_GAP);
        assert_eq!(merged, 1);
        assert_eq!(out.len(), n0 + 2);
        // le rectangle réuni englobe les trois premières fenêtres, dans le plan du mur et dans le même sens
        let big = &out[n0];
        let xs: Vec<f32> = big.corners.iter().map(|c| c.x).collect();
        assert!((xs.iter().cloned().fold(f32::MAX, f32::min) - 0.5).abs() < 1e-4);
        assert!((xs.iter().cloned().fold(f32::MIN, f32::max) - 3.4).abs() < 1e-4);
        assert!(big.corners.iter().all(|c| (c.y - 2.5).abs() < 1e-4));
        let n_old = (m.portals[n0].corners[1] - m.portals[n0].corners[0]).cross(m.portals[n0].corners[3] - m.portals[n0].corners[0]);
        let n_new = (big.corners[1] - big.corners[0]).cross(big.corners[3] - big.corners[0]);
        assert!(n_old.dot(n_new) > 0.0);
        assert!((area(big) - 2.9 * 1.2).abs() < 1e-3);
        assert_eq!((big.room_from, big.room_to), (2, 0));
        // la fenêtre lointaine reste
        assert!(out.iter().any(|p| (p.corners[0].x - 5.5).abs() < 1e-4));
    }

    #[test]
    fn murs_pieces_et_portes_separes() {
        let mut m = build_mlo("t", 2, 1, 6.0, 5.0, 3.0);
        let n0 = m.portals.len();
        // même mur, mais pièces différentes : jamais réunis
        m.portals.push(win(1, -2.0, 0.6, 2.5, 1.0, 2.2));
        m.portals.push(win(2, -1.0, 0.6, 2.5, 1.0, 2.2));
        // même pièce, murs différents (nord et est) : jamais réunis
        m.portals.push(win(2, 4.0, 0.6, 2.5, 1.0, 2.2));
        m.portals.push(Portal::new(2, 0, [v3(6.0, 1.0, 1.0), v3(6.0, 1.0, 2.2), v3(6.0, 1.6, 2.2), v3(6.0, 1.6, 1.0)]));
        // trémie (horizontale) : jamais réunie
        m.portals.push(Portal::new(2, 0, [v3(4.8, 2.0, 3.0), v3(5.4, 2.0, 3.0), v3(5.4, 2.4, 3.0), v3(4.8, 2.4, 3.0)]));
        let (out, merged) = merge_exit_portals(&m, MERGE_GAP);
        assert_eq!(merged, 0);
        assert_eq!(out.len(), n0 + 5);
        // une porte rattachée à une fenêtre : elle reste à part, et la réunion des deux autres qui la recouvrirait
        // est abandonnée
        // (passages surélevés de 0,5 m : au ras du sol, ils ne sont jamais réunis, voir plus bas)
        let mut m = build_mlo("t", 1, 1, 8.0, 5.0, 3.0);
        let n0 = m.portals.len();
        m.portals.push(win(1, -2.0, 0.8, 2.5, 0.5, 2.2));
        m.portals.push(win(1, -1.0, 0.8, 2.5, 0.5, 2.2)); // la porte est ici
        m.portals.push(win(1, 0.0, 0.8, 2.5, 0.5, 2.2));
        let mut door = Entity::new("prop_magenta_door", v3(-1.0, 2.5, 0.5));
        door.room = 1;
        m.entities.push(door);
        assert_eq!(m.door_portal(0), Some(n0 + 1));
        let (out, merged) = merge_exit_portals(&m, MERGE_GAP);
        assert_eq!(merged, 0, "{:?}", out.iter().map(|p| p.corners[0]).collect::<Vec<_>>());
        assert_eq!(out.len(), n0 + 3);
        // sans la porte : les trois réunis
        m.entities.clear();
        let (out, merged) = merge_exit_portals(&m, MERGE_GAP);
        assert_eq!(merged, 1);
        assert_eq!(out.len(), n0 + 1);
        // une porte d'armoire près d'une fenêtre n'est pas une porte de passage : pas rattachée, réunion faite
        let mut e = Entity::new("prop_wardrobe_door_01", v3(-1.0, 2.5, 0.5));
        e.room = 1;
        m.entities.push(e);
        assert_eq!(m.door_portal(0), None);
        assert_eq!(merge_exit_portals(&m, MERGE_GAP).1, 1);
    }

    #[test]
    fn porte_d_entree_et_ouvertures_gardees() {
        // entrée de build_mlo (au ras du sol, sans objet porte) et une fenêtre juste à côté : pas réunies
        let mut m = build_mlo("t", 1, 1, 8.0, 5.0, 3.0);
        let n0 = m.portals.len();
        let entry = m.portals[n0 - 1].clone();
        let ex = entry.corners.iter().map(|c| c.x).fold(f32::MIN, f32::max);
        let y = entry.corners[0].y;
        m.portals.push(Portal::new(1, 0, [v3(ex + 0.3, y, 1.0), v3(ex + 0.3, y, 2.0), v3(ex + 1.0, y, 2.0), v3(ex + 1.0, y, 1.0)]));
        assert_eq!(merge_exit_portals(&m, MERGE_GAP).1, 0);
        // deux fenêtres réunies : le portail réuni garde les deux ouvertures, la coque ne perce qu'elles
        let mut m = build_mlo("t", 1, 1, 8.0, 5.0, 3.0);
        m.portals.push(win(1, -3.0, 0.6, 2.5, 1.0, 2.0));
        m.portals.push(win(1, -1.6, 0.6, 2.5, 1.0, 2.0));
        let (out, k) = merge_exit_portals(&m, MERGE_GAP);
        assert_eq!(k, 1);
        let merged = out.iter().find(|p| !p.openings.is_empty()).expect("portail réuni");
        assert_eq!(merged.openings.len(), 2);
        m.portals = out;
        // mur nord (y = 2,5) : surface pleine = mur entier moins les deux fenêtres (0,6 × 1 m chacune), pas moins
        // le rectangle réuni (2 × 1 m)
        let wall: f32 = crate::shell::room_quads(&m, 1)
            .iter()
            .filter(|q| q.0 == 1 && q.1.iter().all(|c| (c.y - 2.5).abs() < 1e-4))
            .map(|q| (q.1[1] - q.1[0]).cross(q.1[3] - q.1[0]).len())
            .sum();
        assert!((wall - (8.0 * 3.0 - 2.0 * 0.6)).abs() < 1e-3, "mur {wall}");
        // réunir encore (déjà réuni) garde les ouvertures d'origine
        let (out2, _) = merge_exit_portals(&m, MERGE_GAP);
        assert_eq!(out2.iter().map(|p| p.openings.len()).max(), Some(2));
    }

    #[test]
    fn grands_portails_et_ecart() {
        let mut m = build_mlo("t", 1, 1, 12.0, 5.0, 4.0);
        let n0 = m.portals.len();
        // deux baies de 4 × 3 m (12 m², pas « petites ») côte à côte
        m.portals.push(win(1, -6.0, 4.0, 2.5, 0.0, 3.0));
        m.portals.push(win(1, -1.5, 4.0, 2.5, 0.0, 3.0));
        assert_eq!(merge_exit_portals(&m, MERGE_GAP).1, 0);
        // deux fenêtres à 1,6 m : pas réunies ; à 1,4 m : réunies
        m.portals.truncate(n0);
        m.portals.push(win(1, -6.0, 1.0, 2.5, 1.0, 2.0));
        m.portals.push(win(1, -3.4, 1.0, 2.5, 1.0, 2.0));
        assert_eq!(merge_exit_portals(&m, MERGE_GAP).1, 0);
        m.portals.pop();
        m.portals.push(win(1, -3.6, 1.0, 2.5, 1.0, 2.0));
        let (out, k) = merge_exit_portals(&m, MERGE_GAP);
        assert_eq!(k, 1);
        assert_eq!(out.len(), n0 + 1);
        // sens inverse (portail de l'extérieur vers la pièce, coins dans l'autre sens) : réunis, sens gardé
        m.portals.truncate(n0);
        let rev = |p: Portal| Portal::new(0, 1, [p.corners[3], p.corners[2], p.corners[1], p.corners[0]]);
        m.portals.push(rev(win(1, -6.0, 1.0, 2.5, 1.0, 2.0)));
        m.portals.push(rev(win(1, -4.5, 1.0, 2.5, 1.0, 2.0)));
        let (out, k) = merge_exit_portals(&m, MERGE_GAP);
        assert_eq!(k, 1);
        let p = &out[n0];
        let n_old = (m.portals[n0].corners[1] - m.portals[n0].corners[0]).cross(m.portals[n0].corners[3] - m.portals[n0].corners[0]);
        let n_new = (p.corners[1] - p.corners[0]).cross(p.corners[3] - p.corners[0]);
        assert!(n_old.dot(n_new) > 0.0);
        assert!((area(p) - 2.5).abs() < 1e-3);
    }

    #[test]
    fn libelles_et_valeurs_des_ambiances() {
        assert_eq!(amb_family("int_garage_02").0, "Garage");
        assert_eq!(amb_family("v_strip3").0, "Bar / club");
        assert_eq!(amb_family("int_Barber1").0, "Magasin");
        assert_eq!(amb_family("int_bar_tequila").0, "Bar / club");
        assert_eq!(amb_family("int_office_lobby").0, "Bureau");
        assert_eq!(amb_family("mp_apa_int_dark").0, "Sombre");
        assert!(amb_is_interior("int_Barber1"));
        assert!(!amb_is_interior("CAMERA_secuirity"));
        let xml = r#"<timecycle_modifier_data><modifier name="int_test" numMods="3" userFlags="0">
            <postfx_exposure>-1.000 -1.000</postfx_exposure>
            <light_artificial_int_down_col_r>1.000 1.000</light_artificial_int_down_col_r>
            <light_artificial_int_down_col_g>0.500 0.500</light_artificial_int_down_col_g>
            <light_artificial_int_down_col_b>0.500 0.500</light_artificial_int_down_col_b>
        </modifier><modifier name="vide" numMods="0" userFlags="0"></modifier></timecycle_modifier_data>"#;
        let mods = parse_tc_mods(xml);
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].0, "int_test");
        assert_eq!(mods[0].1.len(), 4);
        let (tint, bright) = amb_from_values(&mods[0].1).unwrap();
        assert!(tint[0] > tint[1] && (tint[1] - tint[2]).abs() < 1e-4);
        assert!(bright < 1.0);
        assert!(amb_from_values(&mods[1].1).is_none());
        // postfx_desaturation : 1 = couleurs gardées, 0 = gris
        let mut v = mods[0].1.clone();
        v.push(("postfx_desaturation".into(), 1.0));
        assert_eq!(amb_from_values(&v).unwrap().0, tint);
        v.last_mut().unwrap().1 = 0.0;
        assert!(amb_from_values(&v).unwrap().0.iter().all(|t| (t - 1.0).abs() < 1e-4));
        // calques : plus sombre = noir plus opaque
        let dark = amb_layers([1.0, 1.0, 1.0], 0.5);
        assert_eq!(dark.len(), 1);
        assert_eq!(dark[0].0, [0, 0, 0]);
        assert!(amb_layers([1.3, 0.8, 0.9], 1.0).iter().any(|l| l.0[0] == 255 && l.1 > 0));
    }

    #[test]
    fn catalogue_des_meubles() {
        let s = v3(2.0, 0.9, 0.8);
        assert!(furn_cats("prop_couch_01", s) & (1 << 1) != 0);
        assert!(furn_cats("v_res_tre_bed2", s) & (1 << 2) != 0);
        assert_eq!(furn_cats("prop_couch_01_lod", s), 0);
        assert_eq!(furn_cats("dt1_01_build", s), 0);
        assert_eq!(furn_cats("prop_table_03", v3(20.0, 2.0, 1.0)), 0);
        assert!(furn_matches("prop_couch_01", "canapé"));
        assert!(furn_matches("v_res_m_lampstand", "lampe"));
        assert!(!furn_matches("prop_table_03", "lit"));
        let h = convex_hull(vec![Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0), Pos2::new(0.5, 0.5), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)]);
        assert_eq!(h.len(), 4);
    }
}
