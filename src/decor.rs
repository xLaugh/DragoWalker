//! Aménagements tout prêts : une pièce d'intérieur meublée d'un clic selon son type (salon, chambre, cuisine,
//! bureau, bar, supérette…) et un style (moderne, classique, luxe, industriel), avec des variantes (« Varier » tire
//! d'autres meubles et une autre disposition).
//!
//! Les meubles sont de vrais objets du jeu, posés avec leur vraie taille (boîte du modèle) : contre les murs, dos
//! au mur, sans se chevaucher, en laissant libre le passage devant chaque porte ; ils vont par groupes (canapé,
//! table basse, télé en face, tapis ; lit, tables de chevet et lampes ; bureau, fauteuil et écran ; table et chaises
//! autour…). Convention du jeu vérifiée dans la vue : la face avant d'un meuble regarde vers -Y à cap 0 (canapés,
//! lits, armoires, frigos, chaises…), sauf exceptions notées dans `FRONT_FIX`.
//!
//! Avec « ambiance assortie », la pièce prend une ambiance (timecycle) du jeu qui lui va et, si l'intérieur a une
//! coque modelable, un sol et des lumières de la couleur du style.

use crate::math::{v3, Quat, Rng, V3};

/// Types de pièce : (nom, icône).
pub const KINDS: [(&str, &str); 25] = [
    ("Salon", egui_phosphor::regular::COUCH),
    ("Chambre", egui_phosphor::regular::BED),
    ("Cuisine", egui_phosphor::regular::COOKING_POT),
    ("Salle à manger", egui_phosphor::regular::FORK_KNIFE),
    ("Salle de bain", egui_phosphor::regular::BATHTUB),
    ("Bureau", egui_phosphor::regular::DESKTOP),
    ("Open space", egui_phosphor::regular::USERS_THREE),
    ("Salle de réunion", egui_phosphor::regular::PRESENTATION_CHART),
    ("Accueil", egui_phosphor::regular::BELL_SIMPLE),
    ("Bar", egui_phosphor::regular::BEER_STEIN),
    ("Supérette", egui_phosphor::regular::SHOPPING_CART),
    ("Salle de sport", egui_phosphor::regular::BARBELL),
    ("Entrepôt", egui_phosphor::regular::PACKAGE),
    ("Garage", egui_phosphor::regular::WRENCH),
    ("Cellule", egui_phosphor::regular::LOCK),
    ("Vestiaire", egui_phosphor::regular::COAT_HANGER),
    ("Hôpital", egui_phosphor::regular::FIRST_AID_KIT),
    ("Restaurant", egui_phosphor::regular::HAMBURGER),
    ("Coiffeur", egui_phosphor::regular::SCISSORS),
    ("Armurerie", egui_phosphor::regular::CROSSHAIR),
    ("Boîte de nuit", egui_phosphor::regular::MUSIC_NOTES),
    ("Labo", egui_phosphor::regular::FLASK),
    ("Buanderie", egui_phosphor::regular::WASHING_MACHINE),
    ("Chambre d'enfant", egui_phosphor::regular::BABY),
    ("Toilettes", egui_phosphor::regular::TOILET),
];
pub const K_SALON: usize = 0;
pub const K_CHAMBRE: usize = 1;
pub const K_CUISINE: usize = 2;
pub const K_SAM: usize = 3;
pub const K_SDB: usize = 4;
pub const K_BUREAU: usize = 5;
pub const K_OPEN: usize = 6;
pub const K_REUNION: usize = 7;
pub const K_ACCUEIL: usize = 8;
pub const K_BAR: usize = 9;
pub const K_SHOP: usize = 10;
pub const K_GYM: usize = 11;
pub const K_ENTREPOT: usize = 12;
pub const K_GARAGE: usize = 13;
pub const K_CELLULE: usize = 14;
pub const K_VESTIAIRE: usize = 15;
pub const K_HOPITAL: usize = 16;
pub const K_RESTO: usize = 17;
pub const K_COIFFEUR: usize = 18;
pub const K_ARMURERIE: usize = 19;
pub const K_CLUB: usize = 20;
pub const K_LABO: usize = 21;
pub const K_BUANDERIE: usize = 22;
pub const K_ENFANT: usize = 23;
pub const K_WC: usize = 24;

/// Styles : 0 = au hasard parmi ceux qui vont au type de pièce.
pub const THEMES: [&str; 5] = ["Au hasard", "Moderne", "Classique", "Luxe", "Industriel"];

/// Styles qui vont à un type de pièce (indices de THEMES, sans 0).
pub fn themes_for(kind: usize) -> &'static [usize] {
    match kind {
        K_BAR => &[1, 3, 4],
        K_SHOP | K_GYM | K_VESTIAIRE | K_ARMURERIE | K_BUANDERIE | K_WC => &[1, 4],
        K_ENTREPOT | K_GARAGE | K_CELLULE | K_LABO => &[4],
        K_SDB | K_CUISINE => &[1, 2, 3],
        K_OPEN | K_REUNION | K_ACCUEIL | K_COIFFEUR | K_CLUB => &[1, 3],
        K_HOPITAL => &[1],
        K_RESTO => &[1, 2, 4],
        K_ENFANT => &[1, 2],
        _ => &[1, 2, 3],
    }
}

/// Type de pièce deviné d'après son nom (« cuisine », « kitchen », « office »…), sinon None. Les mots les plus
/// précis d'abord (« salon de coiffure » avant « salon »).
pub fn kind_from_name(name: &str) -> Option<usize> {
    let n = name.to_lowercase();
    let t: [(&[&str], usize); 25] = [
        (&["coiff", "barber", "hair"], K_COIFFEUR),
        (&["enfant", "kid", "child", "bebe", "bébé", "nursery"], K_ENFANT),
        (&["cellule", "cell", "jail", "prison", "garde_a_vue", "garde à vue"], K_CELLULE),
        (&["vestiaire", "locker", "changing"], K_VESTIAIRE),
        (&["hopital", "hôpital", "hospital", "medic", "médic", "soin", "infirm", "clinic"], K_HOPITAL),
        (&["restaurant", "resto", "fastfood", "fast_food", "diner", "burger", "pizz", "snack"], K_RESTO),
        (&["armurerie", "ammu", "armory", "armoury", "gunshop", "gun_shop", "weapon"], K_ARMURERIE),
        (&["boite", "boîte", "nightclub", "night_club", "disco", "club"], K_CLUB),
        (&["labo", "lab", "meth", "coke", "weed"], K_LABO),
        (&["buanderie", "laundry", "lingerie"], K_BUANDERIE),
        (&["toilette", "wc", "restroom", "lavatory"], K_WC),
        (&["salon", "living", "lounge", "sejour", "séjour"], K_SALON),
        (&["chambre", "bed", "dorm"], K_CHAMBRE),
        (&["cuisine", "kitchen", "kit"], K_CUISINE),
        (&["manger", "dining", "dine"], K_SAM),
        (&["bain", "bath", "douche", "shower"], K_SDB),
        (&["open"], K_OPEN),
        (&["reunion", "réunion", "meeting", "conf"], K_REUNION),
        (&["bureau", "office", "desk"], K_BUREAU),
        (&["accueil", "lobby", "recep", "hall"], K_ACCUEIL),
        (&["bar", "pub"], K_BAR),
        (&["shop", "magasin", "superette", "supérette", "store", "247"], K_SHOP),
        (&["gym", "sport", "fitness"], K_GYM),
        (&["entrepot", "entrepôt", "warehouse", "stock", "reserve", "réserve"], K_ENTREPOT),
        (&["garage"], K_GARAGE),
    ];
    t.iter().find(|(w, _)| w.iter().any(|w| n.contains(w))).map(|(_, k)| *k)
}

/// Anciens noms de style (assistant IA, atelier) → type de pièce.
pub fn kind_from_style(style: &str) -> usize {
    match style {
        "bureau" => K_BUREAU,
        "bar" => K_BAR,
        "magasin" => K_SHOP,
        "cuisine" => K_CUISINE,
        "chambre" => K_CHAMBRE,
        s => kind_from_name(s).unwrap_or(K_SALON),
    }
}

/// Ambiances du jeu (timecycle) par type et style — noms relus dans le jeu (catalogue des ambiances).
fn ambiances(kind: usize, theme: usize) -> &'static [&'static str] {
    match (kind, theme) {
        (K_CHAMBRE | K_ENFANT, _) => &["MP_H_01_Bedroom", "MP_H_01_New_Bedroom"],
        (K_SALON | K_CUISINE | K_SAM | K_SDB, 3) => &["MPApartHigh", "DLC_mp2025_02_MANSION"],
        (K_SALON | K_CUISINE | K_SAM | K_SDB, 2) => &["MPApart_H_01", "Mp_apart_mid"],
        (K_SALON | K_CUISINE | K_SAM | K_SDB, _) => &["Mp_apart_mid", "DLC_mp2025_01_Apt_Low"],
        (K_BUREAU | K_OPEN, 3) => &["mp_exec_office_03", "mp_exec_office_05"],
        (K_BUREAU | K_OPEN, _) => &["mp_exec_office_01", "mp_exec_office_02", "DLC_mp2024_02_Gen_Office"],
        (K_REUNION, _) => &["mp_x17dlc_facility_conference"],
        (K_ACCUEIL, _) => &["int_office_Lobby", "int_office_LobbyHall"],
        (K_BAR, 3) => &["v_michael_lounge", "DLC_MpSecurity_Studio_Lounge"],
        (K_BAR, 4) => &["v_rockclub", "gunclub"],
        (K_BAR, _) => &["v_bahama", "DLC_mp2025_02_MANSION_BAR"],
        (K_SHOP, _) => &["Shop247", "INT_smshop"],
        (K_GYM, _) => &["Mp_Stilts_gym", "MPApart_H_01_gym"],
        (K_ENTREPOT, _) => &["WAREHOUSE", "mp_bkr_ware01"],
        (K_GARAGE, _) => &["Garage", "MP_MedGarage"],
        (K_CELLULE, _) => &["PoliceStation", "DLC_mp2024_01_BountyHunter_cell"],
        (K_VESTIAIRE, _) => &["MP_casino_BACK_Changing", "PoliceStation"],
        (K_HOPITAL, _) => &["int_Hospital_Blue", "int_hospital_small"],
        (K_RESTO, _) => &["MPApart_H_01", "Mp_apart_mid"],
        (K_COIFFEUR, _) => &["int_Barber1", "INT_posh_hairdresser"],
        (K_ARMURERIE, _) => &["gunstore", "gunshop"],
        (K_CLUB, _) => &["v_rockclub", "v_bahama"],
        (K_LABO, _) => &["int_methlab_small", "mp_x17dlc_lab", "MP_Heist_Tuner_MethLab"],
        (K_BUANDERIE, _) => &["MP_casino_BACK_Laundry"],
        _ => &[],
    }
}

/// Sol de la coque modelable (design::FLOOR_TEX : parquet, carrelage, moquette, béton ciré) et couleur des
/// lumières posées, par type et style — choix de DragoWalker.
fn look(kind: usize, theme: usize) -> (u8, [u8; 3]) {
    match kind {
        K_CUISINE | K_SDB | K_WC | K_SHOP | K_HOPITAL | K_BUANDERIE | K_VESTIAIRE | K_COIFFEUR => (1, [245, 248, 255]),
        K_BUREAU | K_OPEN | K_REUNION | K_ACCUEIL => (if theme == 3 { 0 } else { 2 }, [240, 244, 255]),
        K_BAR | K_CLUB => (if theme == 4 { 3 } else { 0 }, [255, 176, 110]),
        K_RESTO => (if theme == 4 { 1 } else { 0 }, [255, 214, 170]),
        K_GYM | K_CELLULE | K_ARMURERIE | K_LABO => (3, [240, 244, 255]),
        K_ENTREPOT | K_GARAGE => (3, [228, 234, 240]),
        K_ENFANT => (2, [255, 230, 196]),
        _ => (if theme == 3 { 3 } else { 0 }, [255, 224, 184]),
    }
}

/// Rôles des meubles (une liste de modèles par style).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum R {
    Sofa,
    Armchair,
    Coffee,
    TvStand,
    Tv,
    Rug,
    FloorLamp,
    Plant,
    Shelf,
    Bed,
    Night,
    TableLamp,
    Wardrobe,
    Dresser,
    DineTable,
    DineChair,
    Desk,
    OffChair,
    Visitor,
    Cabinet,
    Monitor,
    BigSofa,
    Sideboard,
}

/// Modèles d'un rôle pour un style (1 moderne, 2 classique, 3 luxe, 4 industriel).
fn pool(r: R, theme: usize) -> &'static [&'static str] {
    use R::*;
    match (r, theme) {
        (Sofa, 2) => &["v_res_m_h_sofa", "sf_prop_sf_sofa_chefield_01a", "prop_couch_04"],
        (Sofa, 3) => &["m25_2_prop_m52_sofa_01a", "m25_1_prop_m51_sofa_01a"],
        (Sofa, 4) => &["bkr_prop_clubhouse_sofa_01a", "v_club_officesofa"],
        (Sofa, _) => &["v_res_tre_sofa_s", "prop_couch_lg_06", "v_res_j_sofa", "v_res_d_sofa"],
        (Armchair, 2) => &["v_res_m_armchair", "v_res_d_armchair", "prop_armchair_01"],
        (Armchair, 3) => &["m25_2_prop_m52_armchair_02", "m25_2_prop_m52_armchair_01"],
        (Armchair, 4) => &["bkr_prop_clubhouse_armchair_01a"],
        (Armchair, _) => &["m25_2_prop_m52_armchair_01", "v_res_fh_easychair", "v_corp_lazychair"],
        (Coffee, 2) => &["v_res_d_coffeetable"],
        (Coffee, 3) => &["m25_1_prop_m51_coffeetable_01a"],
        (Coffee, 4) => &["prop_fbi3_coffee_table"],
        (Coffee, _) => &["v_res_j_coffeetable", "v_res_fh_coftablea", "prop_fbi3_coffee_table"],
        (TvStand, 2) => &["v_res_tre_tvstand_tall"],
        (TvStand, 3) => &["m25_1_prop_m51_tvstand_01a"],
        (TvStand, _) => &["v_res_tre_tvstand"],
        (Tv, 3) => &["prop_tv_flat_michael"],
        (Tv, _) => &["prop_tv_flat_02", "prop_tv_flat_michael"],
        (Rug, 2) => &["v_16_study_rug"],
        (Rug, 3) => &["m25_2_prop_m52_rug_round_02b", "m24_1_prop_m41_rug_01a"],
        (Rug, _) => &["v_72_carpet01", "m25_2_prop_m52_rug_round_01a"],
        (FloorLamp, 2) => &["v_res_m_lampstand"],
        (FloorLamp, 3) => &["m25_2_prop_m52_c_floorlamp", "m25_2_prop_m52_r_floorlamp"],
        (FloorLamp, _) => &["v_res_m_lampstand", "m25_2_prop_m52_r_floorlamp"],
        (Plant, 3) => &["m25_2_prop_m52_art_plant_01a", "m25_2_prop_m52_art_plant_06a", "m25_2_prop_m52_art_plant_07a", "m25_2_prop_m52_art_plant_04a"],
        (Plant, 2) => &["prop_plant_int_01b", "prop_plant_int_02b", "prop_pot_plant_05b"],
        (Plant, _) => &["prop_plant_int_02a", "prop_plant_int_03a", "prop_plant_int_01b", "m25_2_prop_m52_art_plant_04a"],
        (Shelf, _) => &["v_res_tre_smallbookshelf"],
        (Bed, 2) => &["v_res_d_bed", "v_res_msonbed_s"],
        (Bed, 3) => &["m25_1_prop_m51_bed_01a", "m25_1_prop_m51_bed_02a"],
        (Bed, _) => &["v_res_tre_bed2", "v_res_tre_bed1"],
        (Night, 2) => &["v_res_mdbedtable", "v_res_mbbedtable"],
        (Night, 3) => &["m25_1_prop_m51_bedsidetable_01a", "m25_1_prop_m51_bedsidetable_02a"],
        (Night, _) => &["v_res_tre_bedsidetable", "v_res_tre_bedsidetableb"],
        (TableLamp, 2) => &["v_res_d_lampa"],
        (TableLamp, 3) => &["v_res_mtblelampmod"],
        (TableLamp, _) => &["v_res_mdbedlamp", "v_res_j_tablelamp2"],
        (Wardrobe, 3) => &["m25_1_prop_m51_wardrobe_01a"],
        (Wardrobe, _) => &["v_res_tre_wardrobe"],
        (Dresser, 2) => &["v_res_d_dressingtable", "v_res_mbdresser"],
        (Dresser, _) => &["v_res_mbdresser"],
        (DineTable, 2) => &["prop_table_04", "v_res_tre_table2"],
        (DineTable, 3) => &["m25_1_prop_m51_dinetable_01a"],
        (DineTable, 4) => &["prop_table_06", "prop_table_03b"],
        (DineTable, _) => &["v_res_tre_table2", "v_res_tre_table001"],
        (DineChair, 2) => &["v_res_m_dinechair", "v_res_j_dinechair"],
        (DineChair, 3) => &["m25_1_prop_m51_dinechair_01a"],
        (DineChair, 4) => &["bkr_prop_clubhouse_chair_01", "prop_chair_08"],
        (DineChair, _) => &["m25_2_prop_m52_diningchair_01a", "v_res_tre_chair"],
        (Desk, 2) => &["v_41_desk01", "prop_office_desk_01"],
        (Desk, 3) => &["m25_2_prop_m52_desk_01a"],
        (Desk, _) => &["v_corp_officedesk_5", "v_ind_dc_desk03", "m25_2_prop_m52_desk_01a"],
        (OffChair, 2) => &["v_res_tre_officechair", "ex_prop_offchair_exec_01"],
        (OffChair, 3) => &["ex_prop_offchair_exec_01", "m24_1_prop_m41_officechair_01a"],
        (OffChair, _) => &["prop_off_chair_04", "prop_off_chair_05", "v_corp_offchair"],
        (Visitor, 2) => &["v_corp_bk_chair3", "v_res_study_chair"],
        (Visitor, 3) => &["m25_2_prop_m52_armchair_01"],
        (Visitor, _) => &["v_corp_sidechair", "prop_chair_08"],
        (Cabinet, 2) => &["v_corp_filecabdark02"],
        (Cabinet, _) => &["v_corp_filecabtall", "v_corp_filecablow"],
        (Monitor, _) => &["prop_monitor_w_large", "prop_monitor_01b"],
        (BigSofa, 3) => &["m25_2_prop_m52_sofa_01a"],
        (BigSofa, _) => &["v_club_officesofa"],
        (Sideboard, 2) => &["v_res_tre_sideboard", "v_res_mbdresser"],
        (Sideboard, 3) => &["m25_2_prop_m52_sideboard_01b"],
        (Sideboard, _) => &["apa_mp_h_04_liv_sideboard", "v_res_tre_sideboard"],
    }
}

/// Meubles dont la face avant n'est pas vers -Y à cap 0 : (modèle, angle de la face avant en degrés) — relevé dans
/// la vue (bureau en bois : on s'assoit de l'autre côté ; baignoire : posée le long du mur).
const FRONT_FIX: [(&str, f32); 3] = [("v_41_desk01", 180.0), ("v_res_mbath", 90.0), ("ex_receptiondesk01x", 90.0)];

/// Hauteur du plan de travail quand la boîte du modèle dépasse (crédence, robinet) : (modèle, hauteur en m).
const TOP_H: [(&str, f32); 2] = [("v_9_kitchen_unit", 0.92), ("imp_carwareoffice_kitch", 0.92)];

/// Passage à laisser libre devant un meuble qu'on ouvre ou qu'on utilise (armoire, frigo, évier, WC…), en m ;
/// 0 pour ceux dont les abords sont gérés par leur groupe (canapé, lit, bureau, table, comptoir).
fn auto_clear(name: &str) -> f32 {
    const K: [&str; 26] = [
        "wardrobe", "dresser", "dressing", "sideboard", "filecab", "cabinet", "shelf", "shelves", "fridge", "cooler", "sink", "oven", "hob", "cooker", "kitch", "toilet", "towel", "tvstand", "vend",
        "copier", "watercooler", "toolchest", "jukebox", "rack", "mirror", "compressor",
    ];
    if !K.iter().any(|k| name.contains(k)) {
        0.0
    } else if name.contains("toilet") || name.contains("towel") || name.contains("mirror") {
        0.55
    } else {
        0.7
    }
}

/// Bureau (meuble de travail avec ordinateur) ? Les modèles des aménagements, et tout « desk » sauf comptoirs.
pub fn is_desk(name: &str) -> bool {
    let n = name.to_lowercase();
    (1..=4).any(|t| pool(R::Desk, t).contains(&n.as_str())) || (n.contains("desk") && !n.contains("reception") && !n.contains("dj_") && !n.contains("frontdesk") && !n.contains("lobby"))
}

/// Objets à plat (tapis) : ni dans la grille d'encombrement, ni sur les autres.
fn is_flat(name: &str) -> bool {
    name.contains("rug") || name.contains("carpet")
}

/// Taille et coin bas (repère du modèle) : (taille, coin min).
pub type Dims = (V3, V3);

/// Un meuble posé : modèle, position de son origine (repère de l'intérieur), cap (radians).
#[derive(Clone, Debug)]
pub struct Placed {
    pub arch: String,
    pub pos: V3,
    pub yaw: f32,
}

/// Lumière proposée (repère de l'intérieur) : position au plafond.
#[derive(Clone, Debug)]
pub struct LightOut {
    pub p: V3,
    pub color: [u8; 3],
}

/// Résultat d'un aménagement.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub items: Vec<Placed>,
    pub lights: Vec<LightOut>,
    pub ambiance: Option<&'static str>,
    pub floor: u8,
    pub theme: usize,
}

/// Meuble posé dans le plan : centre de la boîte au sol, cap, largeur (x local), profondeur (y local), hauteur,
/// direction de sa face avant.
#[derive(Clone, Copy, Debug)]
struct Pose {
    c: [f32; 2],
    #[allow(dead_code)]
    yaw: f32,
    w: f32,
    d: f32,
    h: f32,
    z: f32,
    front: [f32; 2],
}

impl Pose {
    fn right(&self) -> [f32; 2] {
        [-self.front[1], self.front[0]]
    }
}

/// Pièce à meubler : contour (repère de l'intérieur), sol, hauteur, passages (segments au sol : portes ;
/// fenêtres à part), objets déjà là (centre, demi-tailles, cap).
pub struct RoomIn {
    pub poly: Vec<[f32; 2]>,
    pub z0: f32,
    pub h: f32,
    pub doors: Vec<[[f32; 2]; 2]>,
    pub windows: Vec<[[f32; 2]; 2]>,
    pub keep: Vec<KeepIn>,
}

/// Objet déjà dans la pièce, gardé : modèle, centre de sa boîte au sol, demi-tailles, cap (repère de l'intérieur).
pub struct KeepIn {
    pub name: String,
    pub c: [f32; 2],
    pub hs: [f32; 2],
    pub yaw: f32,
}

const CELL: f32 = 0.1;
/// Écart laissé au mur (épaisseur de la coque, plinthes) et entre meubles (m) — choix de DragoWalker.
const WALL_GAP: f32 = 0.05;
/// Recul du dos d'un meuble posé contre un mur (m).
const WALL_BACK: f32 = 0.07;
const PAD: f32 = 0.04;
/// Passage laissé libre devant une porte (rayon, m) et devant une fenêtre pour les meubles hauts.
const DOOR_CLEAR: f32 = 1.05;
const WINDOW_CLEAR: f32 = 0.45;

struct Grid {
    x0: f32,
    y0: f32,
    nx: usize,
    ny: usize,
    /// 1 = hors de la pièce, mur ou passage (testé sans marge) ; 2 = libre seulement pour les meubles bas (fenêtre) ;
    /// 3 = meuble (testé avec la marge PAD) ; 4 = dégagement d'un meuble (passage devant un frigo, autour d'une
    /// table…), libre seulement pour les tapis
    c: Vec<u8>,
}

impl Grid {
    #[allow(dead_code)]
    fn idx(&self, x: f32, y: f32) -> Option<usize> {
        let i = ((x - self.x0) / CELL).floor();
        let j = ((y - self.y0) / CELL).floor();
        if i < 0.0 || j < 0.0 || i as usize >= self.nx || j as usize >= self.ny {
            return None;
        }
        Some(j as usize * self.nx + i as usize)
    }
    fn center(&self, k: usize) -> [f32; 2] {
        [self.x0 + ((k % self.nx) as f32 + 0.5) * CELL, self.y0 + ((k / self.nx) as f32 + 0.5) * CELL]
    }
}

fn seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-9 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    let (qx, qy) = (a[0] + dx * t - p[0], a[1] + dy * t - p[1]);
    (qx * qx + qy * qy).sqrt()
}

fn in_poly(pts: &[[f32; 2]], x: f32, y: f32) -> bool {
    let mut inside = false;
    let n = pts.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (pts[i], pts[j]);
        if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Point dans la boîte orientée (centre, demi-tailles, cap) agrandie de `pad`.
fn in_obb(p: [f32; 2], c: [f32; 2], hw: f32, hd: f32, yaw: f32, pad: f32) -> bool {
    let (s, co) = yaw.sin_cos();
    let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
    let lx = dx * co + dy * s;
    let ly = -dx * s + dy * co;
    lx.abs() <= hw + pad && ly.abs() <= hd + pad
}

/// Mur de la pièce : extrémités, direction, normale vers l'intérieur, longueur.
#[derive(Clone, Copy)]
struct Wall {
    a: [f32; 2],
    u: [f32; 2],
    n: [f32; 2],
    len: f32,
}

/// Où mettre un meuble contre un mur.
#[derive(Clone, Copy)]
enum Pref {
    /// mur le plus long, loin des portes
    Main,
    /// n'importe où (au hasard, un peu loin des portes)
    Any,
    /// mur dont la normale regarde dans cette direction, au plus près de ce point
    Facing([f32; 2], [f32; 2]),
    /// au plus près de ce point
    Near([f32; 2]),
    /// au plus loin de ce point
    Far([f32; 2]),
}

struct Layout<'a> {
    room: &'a RoomIn,
    g: Grid,
    rng: Rng,
    walls: Vec<Wall>,
    dims: &'a dyn Fn(&str) -> Option<Dims>,
    out: Vec<Placed>,
    theme: usize,
    /// meubles posés au sol (et objets posés dessus) : modèle, pose — pour la décoration
    placed: Vec<(String, Pose)>,
    /// objets accrochés aux murs : bouts au sol, bas et haut (m au-dessus du sol)
    hung: Vec<([f32; 2], [f32; 2], f32, f32)>,
}

impl<'a> Layout<'a> {
    fn new(room: &'a RoomIn, seed: u64, theme: usize, dims: &'a dyn Fn(&str) -> Option<Dims>) -> Layout<'a> {
        let (mut mn, mut mx) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in &room.poly {
            mn = [mn[0].min(p[0]), mn[1].min(p[1])];
            mx = [mx[0].max(p[0]), mx[1].max(p[1])];
        }
        let nx = (((mx[0] - mn[0]) / CELL).ceil() as usize).clamp(1, 2000);
        let ny = (((mx[1] - mn[1]) / CELL).ceil() as usize).clamp(1, 2000);
        let mut g = Grid { x0: mn[0], y0: mn[1], nx, ny, c: vec![0; nx * ny] };
        let n = room.poly.len();
        // murs, normales vers l'intérieur (d'après un point juste à côté du milieu)
        let mut walls = Vec::new();
        for i in 0..n {
            let (a, b) = (room.poly[i], room.poly[(i + 1) % n]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 0.3 {
                continue;
            }
            let u = [dx / len, dy / len];
            let mut nn = [-u[1], u[0]];
            let m = [a[0] + dx * 0.5 + nn[0] * 0.05, a[1] + dy * 0.5 + nn[1] * 0.05];
            if !in_poly(&room.poly, m[0], m[1]) {
                nn = [-nn[0], -nn[1]];
            }
            walls.push(Wall { a, u, n: nn, len });
        }
        for k in 0..g.c.len() {
            let p = g.center(k);
            let near_wall = (0..n).any(|i| seg_dist(p, room.poly[i], room.poly[(i + 1) % n]) < WALL_GAP);
            if !in_poly(&room.poly, p[0], p[1]) || near_wall || room.doors.iter().any(|d| seg_dist(p, d[0], d[1]) < DOOR_CLEAR) {
                g.c[k] = 1;
            } else if room.windows.iter().any(|w| seg_dist(p, w[0], w[1]) < WINDOW_CLEAR) {
                g.c[k] = 2;
            }
            if g.c[k] == 0 && room.keep.iter().any(|k| in_obb(p, k.c, k.hs[0], k.hs[1], k.yaw, 0.0)) {
                g.c[k] = 3;
            }
        }
        let mut l = Layout { room, g, rng: Rng::new(seed), walls, dims, out: Vec::new(), theme, placed: Vec::new(), hung: Vec::new() };
        // les objets gardés comptent pour la décoration (pas de tableau derrière une armoire gardée)
        for k in &room.keep {
            if let Some(p) = l.kept(&[k.name.as_str()]) {
                l.placed.push((k.name.clone(), p));
            }
        }
        l
    }

    fn pick(&mut self, r: R) -> &'static str {
        let p = pool(r, self.theme);
        p[(self.rng.next_u32() as usize) % p.len()]
    }

    fn dims_of(&self, name: &str) -> Dims {
        (self.dims)(name).unwrap_or((v3(0.8, 0.6, 0.9), v3(-0.4, -0.3, 0.0)))
    }

    fn front_fix(name: &str) -> f32 {
        FRONT_FIX.iter().find(|(n, _)| *n == name).map(|(_, a)| a.to_radians()).unwrap_or(0.0)
    }

    /// Cap qui tourne la face avant du meuble vers `dir`.
    fn yaw_for(name: &str, dir: [f32; 2]) -> f32 {
        // face avant à cap 0 : rotation de (0,-1) par l'angle de FRONT_FIX ; (0,-1) tourné de θ = (sin θ, -cos θ)
        dir[0].atan2(-dir[1]) - Self::front_fix(name)
    }

    /// Largeur (le long du mur) et profondeur (vers l'avant) du meuble.
    fn wd(&self, name: &str) -> (f32, f32, f32) {
        let (s, _) = self.dims_of(name);
        let f = Self::front_fix(name);
        if (f.sin()).abs() > 0.7 {
            (s.y, s.x, s.z)
        } else {
            (s.x, s.y, s.z)
        }
    }

    /// Libre pour ce meuble (boîte orientée) ?
    fn fits(&self, c: [f32; 2], name: &str, yaw: f32) -> bool {
        let (s, _) = self.dims_of(name);
        let (hw, hd) = (s.x * 0.5, s.y * 0.5);
        let tall = s.z > 0.95;
        let r = (hw * hw + hd * hd).sqrt() + PAD;
        let (i0, j0) = (((c[0] - r - self.g.x0) / CELL).floor().max(0.0) as usize, ((c[1] - r - self.g.y0) / CELL).floor().max(0.0) as usize);
        let (i1, j1) = ((((c[0] + r - self.g.x0) / CELL).ceil() as usize).min(self.g.nx), (((c[1] + r - self.g.y0) / CELL).ceil() as usize).min(self.g.ny));
        // le meuble entier doit être dans la grille
        for corner in [[-hw, -hd], [hw, -hd], [hw, hd], [-hw, hd]] {
            let (sn, co) = yaw.sin_cos();
            let p = [c[0] + corner[0] * co - corner[1] * sn, c[1] + corner[0] * sn + corner[1] * co];
            if !in_poly(&self.room.poly, p[0], p[1]) {
                return false;
            }
        }
        for j in j0..j1 {
            for i in i0..i1 {
                let k = j * self.g.nx + i;
                let v = self.g.c[k];
                if v == 0 || (v == 2 && !tall) {
                    continue;
                }
                if in_obb(self.g.center(k), c, hw, hd, yaw, if v == 3 { PAD } else { 0.0 }) {
                    return false;
                }
            }
        }
        let clear = auto_clear(name);
        clear <= 0.0 || self.front_free(c, name, yaw, clear)
    }

    /// Le passage devant le meuble (sur `depth` m) est-il libre de meubles et dans la pièce ?
    fn front_free(&self, c: [f32; 2], name: &str, yaw: f32, depth: f32) -> bool {
        let ff = Self::front_fix(name);
        let f = [(yaw + ff).sin(), -(yaw + ff).cos()];
        let r = [-f[1], f[0]];
        let (w, d, _) = self.wd(name);
        let (hw, hd) = (w * 0.5, depth * 0.5);
        let k = d * 0.5 + hd;
        let m = [c[0] + f[0] * k, c[1] + f[1] * k];
        // coins du passage dans la pièce
        for q in [[-1.0f32, -1.0f32], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]] {
            let p = [m[0] + r[0] * q[0] * hw + f[0] * q[1] * hd, m[1] + r[1] * q[0] * hw + f[1] * q[1] * hd];
            if !in_poly(&self.room.poly, p[0], p[1]) {
                return false;
            }
        }
        let rad = hw.hypot(hd);
        let (i0, j0) = (((m[0] - rad - self.g.x0) / CELL).floor().max(0.0) as usize, ((m[1] - rad - self.g.y0) / CELL).floor().max(0.0) as usize);
        let (i1, j1) = ((((m[0] + rad - self.g.x0) / CELL).ceil() as usize).min(self.g.nx), (((m[1] + rad - self.g.y0) / CELL).ceil() as usize).min(self.g.ny));
        for j in j0..j1 {
            for i in i0..i1 {
                let kk = j * self.g.nx + i;
                if self.g.c[kk] != 3 {
                    continue;
                }
                let p = self.g.center(kk);
                let (dx, dy) = (p[0] - m[0], p[1] - m[1]);
                if (dx * r[0] + dy * r[1]).abs() <= hw && (dx * f[0] + dy * f[1]).abs() <= hd {
                    return false;
                }
            }
        }
        true
    }

    fn occupy(&mut self, c: [f32; 2], name: &str, yaw: f32) {
        let (s, _) = self.dims_of(name);
        let (hw, hd) = (s.x * 0.5, s.y * 0.5);
        for k in 0..self.g.c.len() {
            if self.g.c[k] != 1 && in_obb(self.g.center(k), c, hw, hd, yaw, 0.0) {
                self.g.c[k] = 3;
            }
        }
    }

    /// Pose le meuble (centre de sa boîte au sol `c`, cap) ; z : hauteur du dessous (None = le sol).
    fn put(&mut self, name: &str, c: [f32; 2], yaw: f32, z: Option<f32>, occupy: bool) -> Pose {
        let (s, mn) = self.dims_of(name);
        if occupy && !is_flat(name) {
            self.occupy(c, name, yaw);
        }
        let local = v3(mn.x + s.x * 0.5, mn.y + s.y * 0.5, 0.0);
        let q = Quat::from_z(yaw);
        let off = q.rotate(local);
        let zb = z.unwrap_or(self.room.z0);
        // un tapis un peu au-dessus du sol, pour ne pas clignoter avec lui
        let lift = if is_flat(name) { 0.012 } else { 0.0 };
        self.out.push(Placed { arch: name.to_string(), pos: v3(c[0] - off.x, c[1] - off.y, zb - mn.z + lift), yaw });
        let ff = Self::front_fix(name);
        let front = [(yaw + ff).sin(), -(yaw + ff).cos()];
        let (w, d, h) = self.wd(name);
        let h = TOP_H.iter().find(|t| t.0 == name).map(|t| t.1).unwrap_or(h);
        let p = Pose { c, yaw, w, d, h, z: zb, front };
        let clear = auto_clear(name);
        if occupy && clear > 0.0 && z.is_none() {
            self.clear_front(p, clear, 0.0);
        }
        if z.is_none() && !is_flat(name) {
            self.placed.push((name.to_string(), p));
        }
        p
    }

    /// Marque un rectangle (centre, direction « avant », demi-largeur, demi-profondeur) comme dégagement : les
    /// meubles posés ensuite n'y vont pas (seules les cases libres sont marquées).
    fn clear_rect(&mut self, c: [f32; 2], f: [f32; 2], hw: f32, hd: f32) {
        let r = [-f[1], f[0]];
        for k in 0..self.g.c.len() {
            if self.g.c[k] != 0 && self.g.c[k] != 2 {
                continue;
            }
            let p = self.g.center(k);
            let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
            if (dx * r[0] + dy * r[1]).abs() <= hw && (dx * f[0] + dy * f[1]).abs() <= hd {
                self.g.c[k] = 4;
            }
        }
    }

    /// Dégagement devant un meuble, sur `depth` m, aussi large que lui plus `extra` de chaque côté.
    fn clear_front(&mut self, p: Pose, depth: f32, extra: f32) {
        let k = p.d * 0.5 + depth * 0.5;
        self.clear_rect([p.c[0] + p.front[0] * k, p.c[1] + p.front[1] * k], p.front, p.w * 0.5 + extra, depth * 0.5);
    }

    /// Dégagement tout autour d'un meuble (table et ses chaises, lit…), sur `m` m.
    fn clear_around(&mut self, p: Pose, m: f32) {
        self.clear_rect(p.c, p.front, p.w * 0.5 + m, p.d * 0.5 + m);
    }

    /// Contre un mur, dos au mur, face vers la pièce.
    fn on_wall(&mut self, name: &str, pref: Pref) -> Option<Pose> {
        let (_, c, yaw) = *self.wall_spots(name, pref).first()?;
        Some(self.put(name, c, yaw, None, true))
    }

    /// Toutes les places libres contre les murs pour ce meuble, la meilleure d'abord : (note, centre, cap).
    fn wall_spots(&mut self, name: &str, pref: Pref) -> Vec<(f32, [f32; 2], f32)> {
        let (_, d, _) = self.wd(name);
        let mut spots = Vec::new();
        let walls = self.walls.clone();
        let longest = walls.iter().map(|w| w.len).fold(0.0, f32::max);
        for w in &walls {
            let yaw = Self::yaw_for(name, w.n);
            let steps = ((w.len / 0.15) as usize).max(1);
            for s in 0..=steps {
                let t = w.len * s as f32 / steps as f32;
                let c = [w.a[0] + w.u[0] * t + w.n[0] * (d * 0.5 + WALL_BACK), w.a[1] + w.u[1] * t + w.n[1] * (d * 0.5 + WALL_BACK)];
                if !self.fits(c, name, yaw) {
                    continue;
                }
                let door = self.room.doors.iter().map(|dd| seg_dist(c, dd[0], dd[1])).fold(9.0, f32::min);
                let mid = 1.0 - ((t / w.len) - 0.5).abs();
                let score = match pref {
                    Pref::Main => w.len / longest * 3.0 + door.min(2.0) * 0.5 + mid * 2.0,
                    Pref::Any => door.min(2.0) * 0.4 + mid * 0.5,
                    Pref::Facing(dir, p) => {
                        let dot = w.n[0] * dir[0] + w.n[1] * dir[1];
                        dot * 6.0 - ((c[0] - p[0]).hypot(c[1] - p[1])) * 0.5
                    }
                    Pref::Near(p) => -((c[0] - p[0]).hypot(c[1] - p[1])),
                    Pref::Far(p) => (c[0] - p[0]).hypot(c[1] - p[1]),
                } + (self.rng.f() * 0.6);
                spots.push((score, c, yaw));
            }
        }
        spots.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        spots
    }

    /// Place libre contre le mur qui fait face à `p` (dont la normale va dans le sens -dir), dans l'axe de `p`
    /// (décalage latéral jusqu'à `slide` m), entre `dmin` et `dmax` m de `p` : (centre, cap, décalage).
    fn facing_spot(&self, name: &str, p: [f32; 2], dir: [f32; 2], slide: f32, dmin: f32, dmax: f32) -> Option<([f32; 2], f32, f32)> {
        let (_, d, _) = self.wd(name);
        let mut best: Option<(f32, [f32; 2], f32)> = None;
        for w in &self.walls {
            if w.n[0] * dir[0] + w.n[1] * dir[1] > -0.9 {
                continue;
            }
            let t0 = (p[0] - w.a[0]) * w.u[0] + (p[1] - w.a[1]) * w.u[1];
            let mut k = 0.0;
            while k <= slide + 0.001 {
                for s in [1.0f32, -1.0] {
                    let t = t0 + k * s;
                    if t < 0.0 || t > w.len {
                        continue;
                    }
                    let c = [w.a[0] + w.u[0] * t + w.n[0] * (d * 0.5 + WALL_BACK), w.a[1] + w.u[1] * t + w.n[1] * (d * 0.5 + WALL_BACK)];
                    let dist = (c[0] - p[0]).hypot(c[1] - p[1]);
                    if dist < dmin || dist > dmax {
                        continue;
                    }
                    let yaw = Self::yaw_for(name, w.n);
                    if self.fits(c, name, yaw) && best.map(|b| k < b.0).unwrap_or(true) {
                        best = Some((k, c, yaw));
                    }
                }
                k += 0.1;
            }
        }
        best.map(|b| (b.1, b.2, b.0))
    }

    /// Plusieurs meubles côte à côte contre un même mur (frigo, évier, plaque…), dos alignés ; s'ils ne tiennent
    /// pas tous, les derniers sont retirés un à un (avec `all` : tous ou rien). Rend les meubles posés.
    fn line_on_wall(&mut self, names: &[&str], gap: f32, pref: Pref, all: bool) -> Vec<Pose> {
        let walls = self.walls.clone();
        let longest = walls.iter().map(|w| w.len).fold(0.0, f32::max);
        let least = if all { names.len() } else { 1 };
        for count in (least..=names.len()).rev() {
            let set = &names[..count];
            let dims: Vec<(f32, f32, f32)> = set.iter().map(|n| self.wd(n)).collect();
            let total: f32 = dims.iter().map(|d| d.0).sum::<f32>() + gap * (count as f32 - 1.0);
            let mut best: Option<(f32, Vec<([f32; 2], f32)>)> = None;
            for w in &walls {
                if w.len < total + 0.1 {
                    continue;
                }
                let steps = (((w.len - total) / 0.15) as usize).max(1);
                for s in 0..=steps {
                    let t0 = (w.len - total) * s as f32 / steps as f32;
                    let mut t = t0;
                    let mut poses = Vec::new();
                    let mut ok = true;
                    for (n, d) in set.iter().zip(&dims) {
                        let tc = t + d.0 * 0.5;
                        let c = [w.a[0] + w.u[0] * tc + w.n[0] * (d.1 * 0.5 + WALL_BACK), w.a[1] + w.u[1] * tc + w.n[1] * (d.1 * 0.5 + WALL_BACK)];
                        let yaw = Self::yaw_for(n, w.n);
                        if !self.fits(c, n, yaw) {
                            ok = false;
                            break;
                        }
                        poses.push((c, yaw));
                        t += d.0 + gap;
                    }
                    if !ok {
                        continue;
                    }
                    let mid = [w.a[0] + w.u[0] * (t0 + total * 0.5), w.a[1] + w.u[1] * (t0 + total * 0.5)];
                    let door = self.room.doors.iter().map(|dd| seg_dist(mid, dd[0], dd[1])).fold(9.0, f32::min);
                    let score = match pref {
                        Pref::Near(p) => -((mid[0] - p[0]).hypot(mid[1] - p[1])),
                        Pref::Far(p) => (mid[0] - p[0]).hypot(mid[1] - p[1]),
                        _ => w.len / longest * 2.0 + door.min(4.0) * 0.5,
                    } + self.rng.f() * 0.5;
                    if best.as_ref().map(|b| score > b.0).unwrap_or(true) {
                        best = Some((score, poses));
                    }
                }
            }
            if let Some((_, poses)) = best {
                return set.iter().zip(poses).map(|(n, (c, yaw))| self.put(n, c, yaw, None, true)).collect();
            }
        }
        Vec::new()
    }

    /// Dans un coin libre (au hasard parmi les coins), face vers le centre de la pièce.
    fn in_corner(&mut self, name: &str) -> Option<Pose> {
        let (s, _) = self.dims_of(name);
        let r = s.x.max(s.y) * 0.5 + WALL_BACK;
        let n = self.room.poly.len();
        let cen = self.centroid();
        let mut cands = Vec::new();
        for i in 0..n {
            let p = self.room.poly[i];
            let dir = [cen[0] - p[0], cen[1] - p[1]];
            let l = dir[0].hypot(dir[1]).max(0.01);
            let dir = [dir[0] / l, dir[1] / l];
            // bissectrice approchée : vers le centre ; on recule le long jusqu'à tenir
            for k in 0..6 {
                let dd = r * 1.42 + k as f32 * 0.12;
                let c = [p[0] + dir[0] * dd, p[1] + dir[1] * dd];
                let yaw = Self::yaw_for(name, dir);
                if self.fits(c, name, yaw) {
                    cands.push((c, yaw));
                    break;
                }
            }
        }
        if cands.is_empty() {
            return None;
        }
        let (c, yaw) = cands[(self.rng.next_u32() as usize) % cands.len()];
        Some(self.put(name, c, yaw, None, true))
    }

    fn centroid(&self) -> [f32; 2] {
        let n = self.room.poly.len().max(1) as f32;
        let s = self.room.poly.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0], a[1] + p[1]]);
        [s[0] / n, s[1] / n]
    }

    /// Le plus près possible de `c` (spirale), face vers `dir`.
    fn near(&mut self, name: &str, c: [f32; 2], dir: [f32; 2], max_r: f32) -> Option<Pose> {
        let yaw = Self::yaw_for(name, dir);
        let mut r = 0.0;
        while r <= max_r {
            let n = if r == 0.0 { 1 } else { (r * 25.0) as usize };
            for k in 0..n {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                let p = [c[0] + r * a.cos(), c[1] + r * a.sin()];
                if self.fits(p, name, yaw) {
                    return Some(self.put(name, p, yaw, None, true));
                }
            }
            r += 0.1;
        }
        None
    }

    /// Devant `of` (dans le sens de sa face avant), à `gap` m, tourné vers lui (`facing_of`) ou dans le même sens.
    fn in_front(&mut self, of: Pose, name: &str, gap: f32, facing_of: bool) -> Option<Pose> {
        let (_, d, _) = self.wd(name);
        let dist = of.d * 0.5 + gap + d * 0.5;
        let c = [of.c[0] + of.front[0] * dist, of.c[1] + of.front[1] * dist];
        let dir = if facing_of { [-of.front[0], -of.front[1]] } else { of.front };
        let yaw = Self::yaw_for(name, dir);
        if self.fits(c, name, yaw) {
            return Some(self.put(name, c, yaw, None, true));
        }
        None
    }

    /// À côté de `of` (côté droit si side > 0), dos aligné sur le sien, même sens.
    fn beside(&mut self, of: Pose, name: &str, side: f32, gap: f32) -> Option<Pose> {
        let (w, d, _) = self.wd(name);
        let r = of.right();
        let back = [of.c[0] - of.front[0] * of.d * 0.5, of.c[1] - of.front[1] * of.d * 0.5];
        let along = side * (of.w * 0.5 + gap + w * 0.5);
        let c = [back[0] + r[0] * along + of.front[0] * d * 0.5, back[1] + r[1] * along + of.front[1] * d * 0.5];
        let yaw = Self::yaw_for(name, of.front);
        if self.fits(c, name, yaw) {
            return Some(self.put(name, c, yaw, None, true));
        }
        None
    }

    /// Posé sur `of` (z = dessus), décalé de `off` (droite, avant) en m, face vers `dir` (None : comme `of`).
    fn on_top(&mut self, of: Pose, name: &str, off: [f32; 2], dir: Option<[f32; 2]>) -> Pose {
        let r = of.right();
        let c = [of.c[0] + r[0] * off[0] + of.front[0] * off[1], of.c[1] + r[1] * off[0] + of.front[1] * off[1]];
        let yaw = Self::yaw_for(name, dir.unwrap_or(of.front));
        let p = self.put(name, c, yaw, Some(of.z + of.h), false);
        self.placed.push((name.to_string(), p));
        p
    }

    /// Tapis sous un point, aligné sur `dir`, s'il tient dans la pièce (les meubles peuvent être dessus).
    fn rug(&mut self, c: [f32; 2], dir: [f32; 2]) {
        let names = pool(R::Rug, self.theme);
        let start = (self.rng.next_u32() as usize) % names.len();
        for k in 0..names.len() {
            let name = names[(start + k) % names.len()];
            let (s, _) = self.dims_of(name);
            let yaw = Self::yaw_for(name, dir);
            let (sn, co) = yaw.sin_cos();
            let ok = [[-1.0f32, -1.0f32], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]].iter().all(|q| {
                let (lx, ly) = (q[0] * s.x * 0.5, q[1] * s.y * 0.5);
                let p = [c[0] + lx * co - ly * sn, c[1] + lx * sn + ly * co];
                in_poly(&self.room.poly, p[0], p[1]) && self.room.poly.iter().enumerate().all(|(i, a)| seg_dist(p, *a, self.room.poly[(i + 1) % self.room.poly.len()]) > WALL_GAP + 0.1)
            });
            if ok {
                self.put(name, c, yaw, None, false);
                return;
            }
        }
    }

    /// Chaises autour d'une table : le long des grands côtés (et des bouts si `ends`), tournées vers elle.
    fn chairs_around(&mut self, t: Pose, chair: &str, ends: bool, max: usize) -> usize {
        let (cw, cd, _) = self.wd(chair);
        let mut n = 0;
        let r = t.right();
        // la table : w le long de `right`, d le long de `front`
        let per = ((t.w + 0.1) / (cw + 0.15)).floor().max(1.0) as i32;
        for side in [1.0f32, -1.0] {
            for k in 0..per {
                if n >= max {
                    return n;
                }
                let along = (k as f32 - (per - 1) as f32 * 0.5) * (t.w / per as f32);
                let at = |dist: f32| [t.c[0] + r[0] * along + t.front[0] * side * dist, t.c[1] + r[1] * along + t.front[1] * side * dist];
                let dir = [-t.front[0] * side, -t.front[1] * side];
                let yaw = Self::yaw_for(chair, dir);
                if self.fits(at(t.d * 0.5 + cd * 0.5 + PAD + 0.02), chair, yaw) {
                    self.put(chair, at(t.d * 0.5 + cd * 0.5 - 0.12), yaw, None, true);
                    n += 1;
                }
            }
        }
        if ends {
            for side in [1.0f32, -1.0] {
                if n >= max {
                    return n;
                }
                let at = |dist: f32| [t.c[0] + r[0] * side * dist, t.c[1] + r[1] * side * dist];
                let dir = [-r[0] * side, -r[1] * side];
                let yaw = Self::yaw_for(chair, dir);
                if self.fits(at(t.w * 0.5 + cd * 0.5 + PAD + 0.02), chair, yaw) {
                    self.put(chair, at(t.w * 0.5 + cd * 0.5 - 0.1), yaw, None, true);
                    n += 1;
                }
            }
        }
        n
    }

    /// Meuble gardé dans la pièce (pas retiré avant d'aménager) dont le modèle est dans `names` : sa pose, pour
    /// construire autour de lui (le canapé gardé reçoit sa table basse et sa télé…).
    fn kept(&self, names: &[&str]) -> Option<Pose> {
        let k = self.room.keep.iter().find(|k| names.iter().any(|n| n.eq_ignore_ascii_case(&k.name)))?;
        let name = names.iter().find(|n| n.eq_ignore_ascii_case(&k.name)).copied()?;
        let ff = Self::front_fix(name);
        let front = [(k.yaw + ff).sin(), -(k.yaw + ff).cos()];
        let (w, d, h) = self.wd(name);
        Some(Pose { c: k.c, yaw: k.yaw, w, d, h, z: self.room.z0, front })
    }

    /// Meuble gardé d'un rôle (tous styles confondus).
    fn kept_role(&self, r: R) -> Option<Pose> {
        let names: Vec<&str> = (1..=4).flat_map(|t| pool(r, t).iter().copied()).collect();
        self.kept(&names)
    }

    /// Un objet accroché peut-il aller là : point de la paroi `p`, normale `n` vers la pièce, bas à `zb` m du sol ?
    /// Loin des portes et fenêtres, pas derrière un meuble plus haut que son bas, pas sur un autre objet accroché.
    fn can_hang(&self, name: &str, p: [f32; 2], n: [f32; 2], zb: f32) -> bool {
        let (w, _, h) = self.wd(name);
        let u = [-n[1], n[0]];
        let a = [p[0] - u[0] * w * 0.5, p[1] - u[1] * w * 0.5];
        let b = [p[0] + u[0] * w * 0.5, p[1] + u[1] * w * 0.5];
        let top = zb + h;
        if zb < 0.0 || top > self.room.h - 0.12 {
            return false;
        }
        if [a, b].iter().any(|q| !in_poly(&self.room.poly, q[0] + n[0] * 0.08, q[1] + n[1] * 0.08)) {
            return false;
        }
        let near = |s: &[[f32; 2]; 2], m: f32| seg_dist(a, s[0], s[1]) < m || seg_dist(b, s[0], s[1]) < m || seg_dist(s[0], a, b) < m || seg_dist(s[1], a, b) < m;
        if self.room.doors.iter().chain(self.room.windows.iter()).any(|s| near(s, 0.3)) {
            return false;
        }
        // meubles collés à ce mur et plus hauts que le bas de l'objet
        for (_, q) in &self.placed {
            if q.z - self.room.z0 + q.h <= zb - 0.03 {
                continue;
            }
            let r = q.right();
            for k in 0..=6 {
                let t = k as f32 / 6.0;
                let s = [a[0] + (b[0] - a[0]) * t + n[0] * 0.1, a[1] + (b[1] - a[1]) * t + n[1] * 0.1];
                let (dx, dy) = (s[0] - q.c[0], s[1] - q.c[1]);
                if (dx * r[0] + dy * r[1]).abs() <= q.w * 0.5 + 0.15 && (dx * q.front[0] + dy * q.front[1]).abs() <= q.d * 0.5 + 0.35 {
                    return false;
                }
            }
        }
        for (sa, sb, z0, z1) in &self.hung {
            if near(&[*sa, *sb], 0.15) && zb < z1 + 0.1 && top > z0 - 0.1 {
                return false;
            }
        }
        true
    }

    fn hang(&mut self, name: &str, p: [f32; 2], n: [f32; 2], zb: f32) -> Pose {
        let (w, d, h) = self.wd(name);
        let u = [-n[1], n[0]];
        let c = [p[0] + n[0] * (d * 0.5 + 0.02), p[1] + n[1] * (d * 0.5 + 0.02)];
        let yaw = Self::yaw_for(name, n);
        let z = self.room.z0 + zb;
        self.hung.push(([p[0] - u[0] * w * 0.5, p[1] - u[1] * w * 0.5], [p[0] + u[0] * w * 0.5, p[1] + u[1] * w * 0.5], zb, zb + h));
        self.put(name, c, yaw, Some(z), false)
    }

    /// Accroché au-dessus d'un meuble posé contre un mur (tableau au-dessus du canapé, du lit, du buffet) : le
    /// premier modèle de `names` qui tient (pas plus large que le meuble, sous le plafond).
    fn hang_above(&mut self, p: Pose, names: &[&str], gap: f32) -> bool {
        let wp = [p.c[0] - p.front[0] * (p.d * 0.5 + WALL_BACK), p.c[1] - p.front[1] * (p.d * 0.5 + WALL_BACK)];
        let zb0 = p.z - self.room.z0 + p.h + gap;
        for name in names {
            let (w, _, h) = self.wd(name);
            if w > p.w + 0.3 {
                continue;
            }
            let zb = zb0.max(1.55 - h * 0.5);
            if self.can_hang(name, wp, p.front, zb) {
                self.hang(name, wp, p.front, zb);
                return true;
            }
        }
        false
    }

    /// Accroché sur un mur libre, son centre à `zc` m du sol : de préférence au milieu d'un grand mur.
    fn hang_free(&mut self, name: &str, zc: f32) -> bool {
        let (w, _, h) = self.wd(name);
        let zb = zc - h * 0.5;
        let walls = self.walls.clone();
        let mut best: Option<(f32, [f32; 2], [f32; 2])> = None;
        for wl in &walls {
            if wl.len < w + 0.6 {
                continue;
            }
            let steps = ((wl.len / 0.25) as usize).max(1);
            for s in 0..=steps {
                let t = wl.len * s as f32 / steps as f32;
                if t < w * 0.5 + 0.3 || t > wl.len - w * 0.5 - 0.3 {
                    continue;
                }
                let p = [wl.a[0] + wl.u[0] * t, wl.a[1] + wl.u[1] * t];
                if !self.can_hang(name, p, wl.n, zb) {
                    continue;
                }
                let mid = 1.0 - ((t / wl.len) - 0.5).abs() * 2.0;
                let score = mid + wl.len * 0.1 + self.rng.f() * 0.8;
                if best.map(|b| score > b.0).unwrap_or(true) {
                    best = Some((score, p, wl.n));
                }
            }
        }
        let Some((_, p, n)) = best else { return false };
        self.hang(name, p, n, zb);
        true
    }

    /// Suspension au plafond au-dessus d'un meuble (lustre au-dessus de la table), si elle passe au-dessus.
    fn pendant_over(&mut self, p: Pose, at: [f32; 2], name: &str) {
        let (s, _) = self.dims_of(name);
        let zb = self.room.h - s.z;
        if zb < p.z - self.room.z0 + p.h + 0.55 {
            return;
        }
        let yaw = Self::yaw_for(name, p.front);
        let z = self.room.z0 + zb;
        self.put(name, at, yaw, Some(z), false);
    }

    /// État de l'aménagement, pour revenir en arrière si un groupe ne tient pas.
    fn snapshot(&self) -> (usize, Vec<u8>, usize, usize) {
        (self.out.len(), self.g.c.clone(), self.placed.len(), self.hung.len())
    }

    fn restore(&mut self, s: (usize, Vec<u8>, usize, usize)) {
        self.out.truncate(s.0);
        self.g.c = s.1;
        self.placed.truncate(s.2);
        self.hung.truncate(s.3);
    }

    /// Table et ses chaises près de `c` : on essaie les places libres de proche en proche et on garde la première où
    /// au moins `min` chaises tiennent ; le tour de la table reste dégagé.
    fn table_group(&mut self, table: &str, chair: &str, c: [f32; 2], front: [f32; 2], max_r: f32, ends: bool, max: usize, min: usize) -> Option<Pose> {
        let mut r = 0.0;
        let mut tried = 0;
        while r <= max_r && tried < 40 {
            let n = if r == 0.0 { 1 } else { (r * 12.0) as usize };
            for k in 0..n {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                for dir in [front, [-front[1], front[0]]] {
                    let p = [c[0] + r * a.cos(), c[1] + r * a.sin()];
                    let yaw = Self::yaw_for(table, dir);
                    if !self.fits(p, table, yaw) {
                        continue;
                    }
                    tried += 1;
                    let snap = self.snapshot();
                    let t = self.put(table, p, yaw, None, true);
                    if self.chairs_around(t, chair, ends, max) >= min {
                        self.clear_around(t, 0.75);
                        return Some(t);
                    }
                    self.restore(snap);
                }
            }
            r += 0.25;
        }
        None
    }

    /// Grand axe de la pièce (direction unitaire) d'après sa boîte.
    fn long_axis(&self) -> [f32; 2] {
        let w = self.g.nx as f32 * CELL;
        let d = self.g.ny as f32 * CELL;
        if w >= d {
            [1.0, 0.0]
        } else {
            [0.0, 1.0]
        }
    }

    fn area(&self) -> f32 {
        let n = self.room.poly.len();
        let mut a = 0.0;
        for i in 0..n {
            let (p, q) = (self.room.poly[i], self.room.poly[(i + 1) % n]);
            a += p[0] * q[1] - q[0] * p[1];
        }
        (a * 0.5).abs()
    }

    /// Plantes ou petits meubles dans des coins libres.
    fn corners(&mut self, r: R, n: usize) {
        for _ in 0..n {
            let name = self.pick(r);
            if self.in_corner(name).is_none() {
                break;
            }
        }
    }

    /// Plusieurs exemplaires le long des murs (rangements, frigos…), au plus `n`.
    fn along_walls(&mut self, names: &[&str], n: usize, pref: Pref) -> usize {
        let mut k = 0;
        let mut misses = 0;
        let mut i = 0;
        while k < n && misses < names.len() {
            let name = names[i % names.len()];
            i += 1;
            if self.on_wall(name, pref).is_some() {
                k += 1;
                misses = 0;
            } else {
                misses += 1;
            }
        }
        k
    }

    /// Rangées d'un objet au milieu de la pièce (gondoles, palettes, bancs), allées de `aisle` m.
    fn rows(&mut self, names: &[&str], aisle: f32, gap: f32, max: usize) -> usize {
        let ax = self.long_axis();
        // face avant le long du grand axe : la largeur (x local) suit la rangée, en travers de la pièce
        let dir = ax;
        let (w, d) = names.iter().map(|n| self.wd(n)).fold((0.0f32, 0.0f32), |a, s| (a.0.max(s.0), a.1.max(s.1)));
        let mut k = 0;
        let (x0, y0) = (self.g.x0, self.g.y0);
        let (gw, gh) = (self.g.nx as f32 * CELL, self.g.ny as f32 * CELL);
        let (la, lb) = if ax[0] > 0.5 { (gw, gh) } else { (gh, gw) };
        let mut a = 1.0 + d * 0.5;
        while a < la - 0.8 && k < max {
            let k0 = k;
            let mut b = 0.9 + w * 0.5;
            while b < lb - 0.9 && k < max {
                let name = names[k % names.len()];
                let yaw = Self::yaw_for(name, dir);
                let c = if ax[0] > 0.5 { [x0 + a, y0 + b] } else { [x0 + b, y0 + a] };
                if self.fits(c, name, yaw) {
                    self.put(name, c, yaw, None, true);
                    k += 1;
                    b += w + gap;
                } else {
                    b += 0.2;
                }
            }
            // rangée vide (un meuble le long du mur la gêne) : on avance un peu
            a += if k > k0 { d + aisle } else { 0.2 };
        }
        k
    }
}

/// Meuble la pièce : type, style (0 = au hasard), graine (« Varier » en change), tailles des modèles.
pub fn furnish(room: &RoomIn, kind: usize, theme: usize, seed: u64, dims: &dyn Fn(&str) -> Option<Dims>) -> Plan {
    let mut rng = Rng::new(seed ^ 0x9e37_79b9);
    let themes = themes_for(kind);
    let theme = if theme == 0 || !themes.contains(&theme) { themes[(rng.next_u32() as usize) % themes.len()] } else { theme };
    let mut l = Layout::new(room, seed, theme, dims);
    match kind {
        K_SALON => salon(&mut l),
        K_CHAMBRE => chambre(&mut l),
        K_CUISINE => cuisine(&mut l),
        K_SAM => salle_a_manger(&mut l),
        K_SDB => salle_de_bain(&mut l),
        K_BUREAU => bureau(&mut l),
        K_OPEN => open_space(&mut l),
        K_REUNION => reunion(&mut l),
        K_ACCUEIL => accueil(&mut l),
        K_BAR => bar(&mut l),
        K_SHOP => superette(&mut l),
        K_GYM => gym(&mut l),
        K_ENTREPOT => entrepot(&mut l),
        K_CELLULE => cellule(&mut l),
        K_VESTIAIRE => vestiaire(&mut l),
        K_HOPITAL => hopital(&mut l),
        K_RESTO => restaurant(&mut l),
        K_COIFFEUR => coiffeur(&mut l),
        K_ARMURERIE => armurerie(&mut l),
        K_CLUB => club(&mut l),
        K_LABO => labo(&mut l),
        K_BUANDERIE => buanderie(&mut l),
        K_ENFANT => enfant(&mut l),
        K_WC => wc(&mut l),
        _ => garage(&mut l),
    }
    dress(&mut l, kind);
    let ambs = ambiances(kind, theme);
    let ambiance = if ambs.is_empty() { None } else { Some(ambs[(rng.next_u32() as usize) % ambs.len()]) };
    let (floor, color) = look(kind, theme);
    // lumières au plafond : une tous les ~12 m², en grille
    let area = l.area();
    let n = ((area / 12.0).round() as usize).clamp(1, 9);
    let mut lights = Vec::new();
    let (cx, cy) = (l.g.x0, l.g.y0);
    let (w, h) = (l.g.nx as f32 * CELL, l.g.ny as f32 * CELL);
    let cols = ((n as f32 * w / h.max(0.1)).sqrt().round() as usize).clamp(1, n);
    let rows = n.div_ceil(cols);
    for j in 0..rows {
        for i in 0..cols {
            let p = [cx + w * (i as f32 + 0.5) / cols as f32, cy + h * (j as f32 + 0.5) / rows as f32];
            if in_poly(&room.poly, p[0], p[1]) {
                lights.push(LightOut { p: v3(p[0], p[1], room.z0 + room.h - 0.08), color });
            }
        }
    }
    Plan { items: l.out, lights, ambiance, floor, theme }
}

fn salon(l: &mut Layout) {
    let sofa = l.pick(R::Sofa);
    let stand = l.pick(R::TvStand);
    // le canapé contre un mur, et le meuble télé sur le mur d'en face, dans son axe (entre 2 et 5,5 m) : on garde
    // la première place du canapé (la mieux notée) qui laisse une place à la télé
    let kept = l.kept_role(R::Sofa);
    let spots = if kept.is_some() { Vec::new() } else { l.wall_spots(sofa, Pref::Main) };
    let ff = Layout::front_fix(sofa);
    let mut chosen = None;
    let mut best = f32::MIN;
    for &(score, c, yaw) in spots.iter().take(80) {
        let front = [(yaw + ff).sin(), -(yaw + ff).cos()];
        if let Some((tc, ty, k)) = l.facing_spot(stand, c, front, 1.2, 2.0, 5.5) {
            // la télé bien en face compte plus que la place du canapé
            let v = score - k * 2.5;
            if v > best {
                best = v;
                chosen = Some((c, yaw, Some((tc, ty))));
            }
        }
    }
    if chosen.is_none() {
        chosen = spots.first().map(|s| (s.1, s.2, None));
    }
    if let Some(k) = kept {
        chosen = Some((k.c, k.yaw, l.facing_spot(stand, k.c, k.front, 1.2, 2.0, 5.5).map(|t| (t.0, t.1))));
    }
    let s = match chosen {
        Some(_) if kept.is_some() => kept,
        Some((c, yaw, _)) => Some(l.put(sofa, c, yaw, None, true)),
        None => {
            let a = l.pick(R::Armchair);
            l.on_wall(a, Pref::Main)
        }
    };
    let Some(s) = s else {
        l.corners(R::Plant, 2);
        return;
    };
    let tv = match chosen.and_then(|c| c.2) {
        Some((c, yaw)) if l.fits(c, stand, yaw) => Some(l.put(stand, c, yaw, None, true)),
        _ => None,
    };
    if let Some(st) = tv {
        let t = l.pick(R::Tv);
        l.on_top(st, t, [0.0, -0.05], None);
    }
    let coffee = l.pick(R::Coffee);
    let ct = l.in_front(s, coffee, 0.45, false);
    let mid = ct.map(|c| c.c).unwrap_or([s.c[0] + s.front[0] * 1.2, s.c[1] + s.front[1] * 1.2]);
    // tapis sous la table basse
    l.rug(mid, s.front);
    if let Some(ct) = ct {
        let arm = l.pick(R::Armchair);
        let (_, ad, _) = l.wd(arm);
        // pas de télé : un fauteuil en face du canapé, de l'autre côté de la table basse
        if tv.is_none() {
            l.in_front(ct, arm, 0.4, true);
        }
        // fauteuils de part et d'autre de la table basse, tournés vers elle
        let mut sides = [1.0f32, -1.0];
        if l.rng.f() < 0.5 {
            sides.swap(0, 1);
        }
        for (k, sd) in sides.iter().enumerate() {
            if k == 1 && l.rng.f() < 0.4 {
                break;
            }
            let r = ct.right();
            let dist = ct.w * 0.5 + 0.4 + ad * 0.5;
            let c = [ct.c[0] + r[0] * sd * dist, ct.c[1] + r[1] * sd * dist];
            let dir = [-r[0] * sd, -r[1] * sd];
            let yaw = Layout::yaw_for(arm, dir);
            if l.fits(c, arm, yaw) {
                l.put(arm, c, yaw, None, true);
            }
        }
    }
    // lampadaire à côté du canapé
    let lamp = l.pick(R::FloorLamp);
    let side = if l.rng.f() < 0.5 { 1.0 } else { -1.0 };
    if l.beside(s, lamp, side, 0.15).is_none() {
        l.beside(s, lamp, -side, 0.15);
    }
    // le coin salon reste dégagé (entre le canapé et la télé)
    let depth = match tv {
        Some(t) => ((t.c[0] - s.c[0]) * s.front[0] + (t.c[1] - s.c[1]) * s.front[1] - s.d * 0.5 - t.d * 0.5).max(0.5),
        None => 1.8,
    };
    l.clear_front(s, depth, 0.3);
    // bibliothèque sur un autre mur, plantes dans les coins
    if l.area() > 14.0 {
        let sh = l.pick(R::Shelf);
        l.on_wall(sh, Pref::Far(s.c));
    }
    l.corners(R::Plant, 2);
}

fn chambre(l: &mut Layout) {
    let bed = l.pick(R::Bed);
    let Some(b) = l.kept_role(R::Bed).or_else(|| l.on_wall(bed, Pref::Main)) else {
        l.corners(R::Plant, 1);
        return;
    };
    let night = l.pick(R::Night);
    let lamp = l.pick(R::TableLamp);
    for side in [1.0f32, -1.0] {
        if let Some(n) = l.beside(b, night, side, 0.06) {
            l.on_top(n, lamp, [0.0, -0.05], None);
        }
    }
    // tapis au pied du lit
    l.rug([b.c[0] + b.front[0] * (b.d * 0.5 - 0.2), b.c[1] + b.front[1] * (b.d * 0.5 - 0.2)], b.front);
    // passages : au pied du lit et de chaque côté (on doit pouvoir se coucher des deux côtés)
    l.clear_front(b, 0.7, 0.0);
    let r = b.right();
    for side in [1.0f32, -1.0] {
        let k = b.w * 0.5 + 0.3;
        l.clear_rect([b.c[0] + r[0] * side * k, b.c[1] + r[1] * side * k], b.front, 0.3, b.d * 0.5);
    }
    let wd = l.pick(R::Wardrobe);
    l.on_wall(wd, Pref::Far(b.c));
    if l.area() > 11.0 {
        let dr = l.pick(R::Dresser);
        l.on_wall(dr, Pref::Facing([-b.front[0], -b.front[1]], b.c));
    }
    if l.area() > 16.0 {
        let a = l.pick(R::Armchair);
        l.in_corner(a);
    }
    l.corners(R::Plant, 1);
}

fn cuisine(l: &mut Layout) {
    let fridges: &[&str] = match l.theme {
        3 => &["v_res_fridgemoda", "v_res_fridgemodsml"],
        2 => &["prop_fridge_03", "v_res_tre_fridge"],
        _ => &["v_res_fridgemodsml", "prop_fridge_03", "v_res_tre_fridge"],
    };
    let fridge = fridges[(l.rng.next_u32() as usize) % fridges.len()];
    let hob = if l.theme == 2 { "prop_cooker_03" } else { "v_res_ovenhobmod" };
    let sink = "imp_carwareoffice_kitch";
    let counter = "v_9_kitchen_unit";
    // une ligne de cuisine contre un mur : frigo, plan de travail, évier, cuisson (four en colonne en luxe) ;
    // si elle ne tient pas, sans le plan de travail, puis le plus possible
    let mut full = vec![fridge, counter, sink, hob];
    if l.theme == 3 {
        full.insert(1, "v_res_int_oven");
    }
    let mut short = vec![fridge, sink, hob];
    if l.rng.f() < 0.5 {
        full.reverse();
        short.reverse();
    }
    let mut set = full.clone();
    let mut line = l.line_on_wall(&full, 0.0, Pref::Main, true);
    if line.is_empty() {
        set = short.clone();
        line = l.line_on_wall(&short, 0.0, Pref::Main, true);
    }
    if line.is_empty() {
        set = vec![sink, hob, fridge];
        line = l.line_on_wall(&set, 0.0, Pref::Main, false);
    }
    let placed: Vec<&str> = set.iter().take(line.len()).copied().collect();
    if let Some(ci) = placed.iter().position(|n| *n == counter) {
        let c = line[ci];
        l.on_top(c, "prop_microwave_1", [c.w * 0.22, -0.12], None);
        l.on_top(c, "prop_coffee_mac_02", [-c.w * 0.3, -0.18], None);
    }
    for n in [sink, hob, fridge] {
        if !placed.contains(&n) {
            l.on_wall(n, Pref::Any);
        }
    }
    // table et chaises au milieu, si la place le permet (au moins deux chaises)
    let cen = l.centroid();
    let t = l.pick(R::DineTable);
    let ax = l.long_axis();
    let front = [-ax[1], ax[0]];
    let ch = l.pick(R::DineChair);
    l.table_group(t, ch, cen, front, 1.6, false, 4, 2);
    l.corners(R::Plant, 1);
}

fn salle_a_manger(l: &mut Layout) {
    let cen = l.centroid();
    let ax = l.long_axis();
    let front = [-ax[1], ax[0]];
    let t = l.pick(R::DineTable);
    let ch = l.pick(R::DineChair);
    if let Some(tp) = l.kept_role(R::DineTable) {
        l.chairs_around(tp, ch, true, 8);
        l.clear_around(tp, 0.8);
    } else if let Some(tp) = l.table_group(t, ch, cen, front, 1.5, true, 8, 4) {
        l.rug(tp.c, tp.front);
    }
    let sb = l.pick(R::Sideboard);
    l.on_wall(sb, Pref::Main);
    let lamp = l.pick(R::FloorLamp);
    l.in_corner(lamp);
    l.corners(R::Plant, 2);
}

fn salle_de_bain(l: &mut Layout) {
    let mut bath = None;
    if l.area() > 5.5 {
        bath = l.on_wall("v_res_mbath", Pref::Main);
        if let Some(b) = bath {
            l.clear_around(b, 0.12);
            l.clear_front(b, 0.6, 0.0);
        }
    }
    // classique : meuble vasque avec WC attenant ; sinon lavabo, puis WC à côté
    let combo = l.theme == 2 && l.on_wall("v_res_m_sinkunit", Pref::Any).is_some();
    if !combo {
        let sink = if l.theme == 3 || l.rng.f() < 0.5 { "prop_sink_06" } else { "prop_sink_02" };
        let s = l.on_wall(sink, Pref::Any);
        let toilet = if l.rng.f() < 0.5 { "prop_toilet_01" } else { "prop_toilet_02" };
        match s {
            Some(s) => l.on_wall(toilet, Pref::Near(s.c)),
            None => l.on_wall(toilet, Pref::Any),
        };
    }
    match bath {
        Some(b) => l.on_wall("prop_towel_shelf_01", Pref::Near(b.c)),
        None => l.on_wall("prop_towel_shelf_01", Pref::Any),
    };
    l.in_corner("prop_plant_int_04a");
}

/// Meuble posé à `gap` m d'un mur (place pour s'asseoir derrière), face vers le mur (`face_wall`) ou vers la pièce.
fn off_wall(l: &mut Layout, name: &str, gap: f32, face_wall: bool, pref: Pref) -> Option<Pose> {
    let (_, dd, _) = l.wd(name);
    let walls = l.walls.clone();
    let mut best: Option<(f32, [f32; 2], f32)> = None;
    for w in &walls {
        let dir = if face_wall { [-w.n[0], -w.n[1]] } else { w.n };
        let yaw = Layout::yaw_for(name, dir);
        let steps = ((w.len / 0.2) as usize).max(1);
        for s in 0..=steps {
            let t = w.len * s as f32 / steps as f32;
            let off = WALL_BACK + gap + dd * 0.5;
            let c = [w.a[0] + w.u[0] * t + w.n[0] * off, w.a[1] + w.u[1] * t + w.n[1] * off];
            if !l.fits(c, name, yaw) {
                continue;
            }
            let door = l.room.doors.iter().map(|d| seg_dist(c, d[0], d[1])).fold(9.0, f32::min);
            let mid = 1.0 - ((t / w.len) - 0.5).abs();
            let score = match pref {
                Pref::Near(p) => -((c[0] - p[0]).hypot(c[1] - p[1])),
                Pref::Far(p) => (c[0] - p[0]).hypot(c[1] - p[1]),
                _ => door.min(5.0) * 0.5 + mid,
            } + l.rng.f() * 0.4;
            if best.map(|b| score > b.0).unwrap_or(true) {
                best = Some((score, c, yaw));
            }
        }
    }
    let (_, c, yaw) = best?;
    Some(l.put(name, c, yaw, None, true))
}

/// Bureau avec son fauteuil côté mur (le meuble regarde la pièce), écran dessus, chaises de visiteur en face.
fn desk_group(l: &mut Layout, pref: Pref, visitors: usize) -> Option<Pose> {
    let desk = l.pick(R::Desk);
    // le bureau à 1 m du mur (place du fauteuil), face avant vers le mur
    let d = off_wall(l, desk, 1.0, true, pref)?;
    // fauteuil entre le bureau et le mur, tourné vers le bureau
    let chair = l.pick(R::OffChair);
    let (_, cd, _) = l.wd(chair);
    let cc = [d.c[0] + d.front[0] * (d.d * 0.5 + cd * 0.5 - 0.05), d.c[1] + d.front[1] * (d.d * 0.5 + cd * 0.5 - 0.05)];
    let cy = Layout::yaw_for(chair, [-d.front[0], -d.front[1]]);
    l.put(chair, cc, cy, None, true);
    l.clear_front(d, 0.95, 0.15);
    let mon = l.pick(R::Monitor);
    l.on_top(d, mon, [0.0, -d.d * 0.18], Some(d.front));
    // visiteurs de l'autre côté
    let vis = l.pick(R::Visitor);
    let (vw, vd, _) = l.wd(vis);
    for k in 0..visitors {
        let along = (k as f32 - (visitors as f32 - 1.0) * 0.5) * (vw + 0.25);
        let r = d.right();
        let dist = d.d * 0.5 + 0.35 + vd * 0.5;
        let p = [d.c[0] - d.front[0] * dist + r[0] * along, d.c[1] - d.front[1] * dist + r[1] * along];
        let vy = Layout::yaw_for(vis, d.front);
        if l.fits(p, vis, vy) {
            l.put(vis, p, vy, None, true);
        }
    }
    // devant le bureau, côté visiteurs : passage
    let k = d.d * 0.5 + 0.35 + vd + 0.3;
    l.clear_rect([d.c[0] - d.front[0] * k, d.c[1] - d.front[1] * k], d.front, d.w * 0.5 + 0.2, 0.35 + vd * 0.5 + 0.3);
    Some(d)
}

fn bureau(l: &mut Layout) {
    let d = l.kept_role(R::Desk).or_else(|| desk_group(l, Pref::Main, 2));
    let c = d.map(|d| d.c).unwrap_or(l.centroid());
    let cab = l.pick(R::Cabinet);
    l.along_walls(&[cab], 2, Pref::Near(c));
    if l.area() > 16.0 {
        let s = l.pick(R::BigSofa);
        if let Some(sp) = l.on_wall(s, Pref::Far(c)) {
            let ct = l.pick(R::Coffee);
            l.in_front(sp, ct, 0.4, false);
            l.clear_front(sp, 1.2, 0.2);
        }
    } else {
        let sh = l.pick(R::Shelf);
        l.on_wall(sh, Pref::Far(c));
    }
    l.corners(R::Plant, 2);
}

fn open_space(l: &mut Layout) {
    // îlots de bureaux en rangées, fauteuil et écran à chaque poste
    let desk = l.pick(R::Desk);
    let chair = l.pick(R::OffChair);
    let mon = l.pick(R::Monitor);
    let (dw, dd, _) = l.wd(desk);
    let ax = l.long_axis();
    let side = [-ax[1], ax[0]];
    let (x0, y0) = (l.g.x0, l.g.y0);
    let (w, h) = (l.g.nx as f32 * CELL, l.g.ny as f32 * CELL);
    let (la, lb) = if ax[0] > 0.5 { (w, h) } else { (h, w) };
    // une rangée = deux bureaux dos à dos, fauteuils de chaque côté ; on glisse le long de la rangée et on pose un
    // poste dès qu'il tient (bureau et fauteuil), puis on garde libre la place du fauteuil
    let (_, cd, _) = l.wd(chair);
    let pitch = dd * 2.0 + cd * 2.0 + 1.0;
    let mut b = 0.6 + cd + dd;
    while b < lb - 0.6 - cd - dd + 0.01 {
        for s in [-1.0f32, 1.0] {
            let mut a = 0.3 + dw * 0.5;
            while a < la - 0.3 - dw * 0.5 {
                let pb = b + s * (dd * 0.5 + PAD + 0.03);
                let c = if ax[0] > 0.5 { [x0 + a, y0 + pb] } else { [x0 + pb, y0 + a] };
                let dir = [side[0] * s, side[1] * s];
                let yaw = Layout::yaw_for(desk, dir);
                let cc = [c[0] + dir[0] * (dd * 0.5 + cd * 0.5 - 0.05), c[1] + dir[1] * (dd * 0.5 + cd * 0.5 - 0.05)];
                let cy = Layout::yaw_for(chair, [-dir[0], -dir[1]]);
                if l.fits(c, desk, yaw) && l.fits([c[0] + dir[0] * (dd * 0.5 + cd * 0.5 + PAD + 0.02), c[1] + dir[1] * (dd * 0.5 + cd * 0.5 + PAD + 0.02)], chair, cy) {
                    let d = l.put(desk, c, yaw, None, true);
                    l.put(chair, cc, cy, None, true);
                    l.on_top(d, mon, [0.0, -d.d * 0.18], Some(d.front));
                    l.clear_front(d, cd + 0.4, 0.0);
                    a += dw + 0.1;
                } else {
                    a += 0.2;
                }
            }
        }
        b += pitch;
    }
    // pièce étroite : postes contre les murs (bureau dos au mur, fauteuil devant)
    let mut n = l.out.iter().filter(|p| p.arch == desk).count();
    let ff = Layout::front_fix(desk);
    while n < 6 {
        let spots = l.wall_spots(desk, Pref::Any);
        let spot = spots.iter().find(|&&(_, c, yaw)| {
            let f = [(yaw + ff).sin(), -(yaw + ff).cos()];
            let k = dd * 0.5 + cd * 0.5 + PAD + 0.02;
            l.fits([c[0] + f[0] * k, c[1] + f[1] * k], chair, Layout::yaw_for(chair, [-f[0], -f[1]]))
        });
        let Some(&(_, c, yaw)) = spot else { break };
        let d = l.put(desk, c, yaw, None, true);
        let cc = [d.c[0] + d.front[0] * (d.d * 0.5 + cd * 0.5 - 0.05), d.c[1] + d.front[1] * (d.d * 0.5 + cd * 0.5 - 0.05)];
        let cy = Layout::yaw_for(chair, [-d.front[0], -d.front[1]]);
        l.put(chair, cc, cy, None, true);
        l.on_top(d, mon, [0.0, -d.d * 0.18], Some(d.front));
        l.clear_front(d, cd + 0.4, 0.0);
        n += 1;
    }
    l.on_wall("prop_copier_01", Pref::Any);
    l.on_wall("prop_watercooler", Pref::Any);
    let cab = l.pick(R::Cabinet);
    l.along_walls(&[cab], 3, Pref::Any);
    l.corners(R::Plant, 3);
}

fn reunion(l: &mut Layout) {
    let cen = l.centroid();
    let ax = l.long_axis();
    let table = if l.area() > 18.0 { "m24_2_prop_m42_conftable_01a" } else { "v_res_tre_table2" };
    // la longueur de la table (x local) suit le grand axe : sa face avant regarde l'axe court
    let front = [-ax[1], ax[0]];
    if let Some(t) = l.near(table, cen, front, 0.8) {
        // écran au bout de la table, sur le mur d'en face, dans son axe ; puis les chaises
        let r = t.right();
        let mut sides = [r, [-r[0], -r[1]]];
        if l.rng.f() < 0.5 {
            sides.swap(0, 1);
        }
        for dir in sides {
            if let Some((c, yaw, _)) = l.facing_spot("v_res_tre_tvstand", t.c, dir, 0.3, 0.8, 8.0) {
                let st = l.put("v_res_tre_tvstand", c, yaw, None, true);
                l.on_top(st, "prop_tv_flat_michael", [0.0, -0.05], None);
                break;
            }
        }
        let ch = l.pick(R::OffChair);
        l.chairs_around(t, ch, true, 12);
        l.clear_around(t, 0.75);
    }
    let cab = l.pick(R::Cabinet);
    l.on_wall(cab, Pref::Any);
    l.corners(R::Plant, 2);
}

fn accueil(l: &mut Layout) {
    // banque d'accueil au fond, face à l'entrée, l'hôte(sse) entre elle et le mur
    let door = l.room.doors.first().map(|d| [(d[0][0] + d[1][0]) * 0.5, (d[0][1] + d[1][1]) * 0.5]).unwrap_or(l.centroid());
    let counters: [&str; 2] = if l.theme == 2 { ["ex_receptiondesk01x", "ch_chint01_reception_desk"] } else { ["ch_chint01_reception_desk", "ex_receptiondesk01x"] };
    let mut desk = None;
    for name in counters {
        if let Some(d) = off_wall(l, name, 1.0, false, Pref::Far(door)) {
            desk = Some(d);
            break;
        }
    }
    match desk {
        Some(d) => {
            let chair = l.pick(R::OffChair);
            let (_, cd, _) = l.wd(chair);
            let k = d.d * 0.5 + cd * 0.5 + 0.1;
            let cc = [d.c[0] - d.front[0] * k, d.c[1] - d.front[1] * k];
            let cy = Layout::yaw_for(chair, d.front);
            if l.fits(cc, chair, cy) {
                l.put(chair, cc, cy, None, true);
            }
            // derrière : la place de l'hôte(sse) ; devant : l'accueil des visiteurs
            let kb = d.d * 0.5 + 0.5;
            l.clear_rect([d.c[0] - d.front[0] * kb, d.c[1] - d.front[1] * kb], d.front, d.w * 0.5, 0.5);
            l.clear_front(d, 1.2, 0.3);
        }
        None => {
            desk_group(l, Pref::Far(door), 0);
        }
    }
    let s = l.pick(R::BigSofa);
    if let Some(sp) = l.on_wall(s, Pref::Any) {
        let ct = l.pick(R::Coffee);
        if let Some(c) = l.in_front(sp, ct, 0.4, false) {
            l.rug(c.c, sp.front);
        }
        l.clear_front(sp, 1.2, 0.2);
    }
    l.on_wall("prop_watercooler", Pref::Any);
    l.corners(R::Plant, 3);
}

fn bar(l: &mut Layout) {
    // au fond : frigos et étagères à bouteilles en ligne contre le mur principal
    let f = if l.rng.f() < 0.5 { "prop_bar_cooler_03" } else { "prop_bar_fridge_02" };
    let back = l.line_on_wall(&["prop_bar_fridge_03", "v_ret_ml_liqshelfa", "v_ret_ml_liqshelfb", f], 0.05, Pref::Main, false);
    let counters: [&str; 3] = if l.theme == 3 {
        ["ch_chint01_reception_desk", "ex_receptiondesk01x", "v_9_kitchen_unit"]
    } else {
        ["ex_receptiondesk01x", "ch_chint01_reception_desk", "v_9_kitchen_unit"]
    };
    let mut placed = None;
    if let (Some(a), Some(b)) = (back.first().copied(), back.last().copied()) {
        // le comptoir parallèle au mur du fond, 1 m devant les étagères (place du barman), face à la salle
        let n = a.front;
        let u = [-n[1], n[0]];
        let wa = [a.c[0] - n[0] * (a.d * 0.5 + WALL_BACK), a.c[1] - n[1] * (a.d * 0.5 + WALL_BACK)];
        let wb = [b.c[0] - n[0] * (b.d * 0.5 + WALL_BACK), b.c[1] - n[1] * (b.d * 0.5 + WALL_BACK)];
        let mid = [(wa[0] + wb[0]) * 0.5, (wa[1] + wb[1]) * 0.5];
        let deep = back.iter().map(|p| p.d).fold(0.0, f32::max);
        'found: for name in counters {
            let (_, cd, _) = l.wd(name);
            let off = WALL_BACK + deep + 1.0 + cd * 0.5;
            let yaw = Layout::yaw_for(name, n);
            for k in 0..12 {
                let sl = if k % 2 == 0 { k as f32 * 0.1 } else { -(k as f32 + 1.0) * 0.1 };
                let c = [mid[0] + n[0] * off + u[0] * sl, mid[1] + n[1] * off + u[1] * sl];
                if l.fits(c, name, yaw) {
                    let p = l.put(name, c, yaw, None, true);
                    // derrière le comptoir : la place du barman
                    let kb = p.d * 0.5 + 0.5;
                    l.clear_rect([p.c[0] - p.front[0] * kb, p.c[1] - p.front[1] * kb], p.front, p.w * 0.5, 0.5);
                    placed = Some(p);
                    break 'found;
                }
            }
        }
    }
    let stool = match l.theme {
        3 => "v_club_bahbarstool",
        4 => "bkr_prop_biker_barstool_02",
        _ => "prop_bar_stool_01",
    };
    if let Some(p) = placed {
        l.on_top(p, "prop_till_01", [p.w * 0.36, -0.1], Some([-p.front[0], -p.front[1]]));
        // tabourets côté salle, tournés vers le comptoir
        let (sw, sd, _) = l.wd(stool);
        let n = ((p.w - 0.2) / (sw + 0.3)).floor().max(1.0) as i32;
        for k in 0..n {
            let along = (k as f32 - (n - 1) as f32 * 0.5) * (sw + 0.3);
            let r = p.right();
            let c = [p.c[0] + p.front[0] * (p.d * 0.5 + sd * 0.5 + 0.1) + r[0] * along, p.c[1] + p.front[1] * (p.d * 0.5 + sd * 0.5 + 0.1) + r[1] * along];
            let yaw = Layout::yaw_for(stool, [-p.front[0], -p.front[1]]);
            if l.fits(c, stool, yaw) {
                l.put(stool, c, yaw, None, true);
            }
        }
        l.clear_front(p, sd + 0.9, 0.3);
    }
    // banquette loin du comptoir, puis tables hautes avec tabourets (ou chaises) dans la salle
    let cen = l.centroid();
    let s = l.pick(R::BigSofa);
    let far = placed.map(|p| p.c).unwrap_or(cen);
    if let Some(sp) = l.on_wall(s, Pref::Far(far)) {
        let ct = l.pick(R::Coffee);
        l.in_front(sp, ct, 0.35, false);
        l.clear_front(sp, 1.1, 0.2);
    }
    let ht = if l.theme == 4 { "prop_table_06" } else { "ba_prop_int_edgy_table_02" };
    let (n, u) = match placed {
        Some(p) => (p.front, p.right()),
        None => ([0.0, 1.0], [1.0, 0.0]),
    };
    for k in 0..4 {
        let at = [cen[0] + n[0] * 0.8 + u[0] * (k as f32 - 1.5) * 1.7, cen[1] + n[1] * 0.8 + u[1] * (k as f32 - 1.5) * 1.7];
        let Some(t) = l.near(ht, at, n, 0.6) else { continue };
        if l.theme == 4 {
            l.chairs_around(t, "bkr_prop_clubhouse_chair_01", false, 4);
        } else {
            l.chairs_around(t, stool, false, 2);
        }
        l.clear_around(t, 0.55);
    }
    if l.area() > 40.0 {
        l.near("prop_pooltable_02", cen, [1.0, 0.0], 3.0);
    }
    l.on_wall("prop_jukebox_01", Pref::Any);
    l.corners(R::Plant, 1);
}

fn superette(l: &mut Layout) {
    let door = l.room.doors.first().map(|d| [(d[0][0] + d[1][0]) * 0.5, (d[0][1] + d[1][1]) * 0.5]).unwrap_or(l.centroid());
    // caisse près de l'entrée (le caissier derrière, côté mur), frigos au fond, gondoles au milieu
    if let Some(c) = off_wall(l, "p_new_j_counter_02", 0.9, false, Pref::Near(door)) {
        l.on_top(c, "prop_till_01", [0.0, -0.1], Some([-c.front[0], -c.front[1]]));
        let kb = c.d * 0.5 + 0.45;
        l.clear_rect([c.c[0] - c.front[0] * kb, c.c[1] - c.front[1] * kb], c.front, c.w * 0.5, 0.45);
        l.clear_front(c, 0.9, 0.2);
    }
    l.along_walls(&["v_ret_ml_fridge"], 3, Pref::Far(door));
    l.along_walls(&["v_ret_ml_liqshelfa", "v_ret_ml_liqshelfb"], 2, Pref::Any);
    l.rows(&["v_ret_247shelves03"], 1.2, 0.0, 8);
    l.on_wall("prop_vend_soda_01", Pref::Near(door));
    l.on_wall("prop_vend_snak_01", Pref::Near(door));
}

fn gym(l: &mut Layout) {
    let rack = l.on_wall("prop_weight_rack_02", Pref::Main);
    // miroirs près des haltères
    let near = rack.map(|r| r.c).unwrap_or(l.centroid());
    l.on_wall("v_ret_mirror", Pref::Near(near));
    l.on_wall("v_ret_mirror", Pref::Near(near));
    let benches = ["prop_muscle_bench_03", "prop_weight_bench_02", "prop_muscle_bench_01", "prop_muscle_bench_05", "prop_muscle_bench_06"];
    l.rows(&benches, 1.0, 0.7, 6);
    l.on_wall("prop_watercooler", Pref::Any);
    l.corners(R::Plant, 1);
}

fn entrepot(l: &mut Layout) {
    l.along_walls(&["prop_shelves_01", "prop_shelves_03"], 8, Pref::Any);
    // palettes et piles de cartons en rangées, allées entre elles
    let piles = ["prop_boxpile_06a", "prop_pallet_02a", "prop_mb_crate_01a", "prop_boxpile_07d", "prop_rub_boxpile_01"];
    l.rows(&piles, 1.3, 0.25, 12);
}

fn garage(l: &mut Layout) {
    // tout contre les murs : le milieu reste libre pour une voiture
    l.on_wall("prop_toolchest_05", Pref::Main);
    if let Some(t) = l.on_wall("prop_toolchest_02", Pref::Main) {
        l.on_top(t, "v_ind_cs_toolbox1", [0.0, 0.0], None);
    }
    l.on_wall("prop_compressor_01", Pref::Any);
    l.along_walls(&["prop_tyre_rack_01"], 2, Pref::Any);
    l.on_wall("prop_shelves_01", Pref::Any);
    l.in_corner("prop_carjack");
    l.on_wall("imp_prop_impexp_engine_part_01a", Pref::Any);
}

/// Comptoir de service : `back` en ligne contre un mur (étagères, frigos, armoires…), le comptoir parallèle 1 m
/// devant (place du personnel), face à la salle ; caisse dessus, tabourets côté salle. Rend le comptoir.
fn service_counter(l: &mut Layout, back: &[&str], counters: &[&str], pref: Pref, stool: Option<&str>, till: bool) -> Option<Pose> {
    let line = l.line_on_wall(back, 0.05, pref, false);
    let mut placed = None;
    if let (Some(a), Some(b)) = (line.first().copied(), line.last().copied()) {
        let n = a.front;
        let u = [-n[1], n[0]];
        let wa = [a.c[0] - n[0] * (a.d * 0.5 + WALL_BACK), a.c[1] - n[1] * (a.d * 0.5 + WALL_BACK)];
        let wb = [b.c[0] - n[0] * (b.d * 0.5 + WALL_BACK), b.c[1] - n[1] * (b.d * 0.5 + WALL_BACK)];
        let mid = [(wa[0] + wb[0]) * 0.5, (wa[1] + wb[1]) * 0.5];
        let deep = line.iter().map(|p| p.d).fold(0.0, f32::max);
        'found: for name in counters {
            let (_, cd, _) = l.wd(name);
            let off = WALL_BACK + deep + 1.0 + cd * 0.5;
            let yaw = Layout::yaw_for(name, n);
            for k in 0..12 {
                let sl = if k % 2 == 0 { k as f32 * 0.1 } else { -(k as f32 + 1.0) * 0.1 };
                let c = [mid[0] + n[0] * off + u[0] * sl, mid[1] + n[1] * off + u[1] * sl];
                if l.fits(c, name, yaw) {
                    placed = Some(l.put(name, c, yaw, None, true));
                    break 'found;
                }
            }
        }
    }
    if placed.is_none() {
        for name in counters {
            if let Some(p) = off_wall(l, name, 1.0, false, pref) {
                placed = Some(p);
                break;
            }
        }
    }
    let p = placed?;
    // derrière le comptoir : la place du personnel
    let kb = p.d * 0.5 + 0.5;
    l.clear_rect([p.c[0] - p.front[0] * kb, p.c[1] - p.front[1] * kb], p.front, p.w * 0.5, 0.5);
    if till {
        l.on_top(p, "prop_till_01", [p.w * 0.36, -0.1], Some([-p.front[0], -p.front[1]]));
    }
    let mut sd = 0.0;
    if let Some(stool) = stool {
        let (sw, d, _) = l.wd(stool);
        sd = d;
        let n = ((p.w - 0.2) / (sw + 0.3)).floor().max(1.0) as i32;
        for k in 0..n {
            let along = (k as f32 - (n - 1) as f32 * 0.5) * (sw + 0.3);
            let r = p.right();
            let c = [p.c[0] + p.front[0] * (p.d * 0.5 + d * 0.5 + 0.1) + r[0] * along, p.c[1] + p.front[1] * (p.d * 0.5 + d * 0.5 + 0.1) + r[1] * along];
            let yaw = Layout::yaw_for(stool, [-p.front[0], -p.front[1]]);
            if l.fits(c, stool, yaw) {
                l.put(stool, c, yaw, None, true);
            }
        }
    }
    l.clear_front(p, sd + 0.9, 0.3);
    Some(p)
}

fn cellule(l: &mut Layout) {
    let b = l.on_wall("m24_2_prop_m42_bunkerbed_01a", Pref::Main);
    if let Some(b) = b {
        l.clear_front(b, 0.5, 0.0);
    }
    let far = b.map(|b| b.c).unwrap_or(l.centroid());
    match l.on_wall("prop_sink_02", Pref::Far(far)) {
        Some(s) => l.on_wall("prop_toilet_01", Pref::Near(s.c)),
        None => l.on_wall("prop_toilet_01", Pref::Far(far)),
    };
}

fn vestiaire(l: &mut Layout) {
    // rangées de casiers contre les murs, bancs au milieu
    let lockers = ["p_cs_locker_01", "p_cs_locker_02", "p_cs_locker_01", "p_cs_locker_02", "p_cs_locker_01", "p_cs_locker_02", "p_cs_locker_01"];
    l.line_on_wall(&lockers, 0.02, Pref::Main, false);
    l.line_on_wall(&lockers, 0.02, Pref::Any, false);
    if l.theme == 4 {
        l.along_walls(&["bkr_prop_gunlocker_01a"], 2, Pref::Any);
    }
    l.rows(&["prop_air_bench_02"], 1.2, 0.6, 3);
    l.on_wall("v_ret_mirror", Pref::Any);
    l.on_wall("prop_towel_shelf_01", Pref::Any);
}

fn hopital(l: &mut Layout) {
    // lits contre les murs, tête au mur, place autour pour soigner
    let beds = ["v_med_bed1", "v_med_bed2"];
    let n = ((l.area() / 11.0) as usize).clamp(1, 6);
    for i in 0..n {
        let Some(b) = l.on_wall(beds[i % 2], Pref::Any) else { break };
        l.clear_front(b, 0.8, 0.0);
        let r = b.right();
        for side in [1.0f32, -1.0] {
            let k = b.w * 0.5 + 0.35;
            l.clear_rect([b.c[0] + r[0] * side * k, b.c[1] + r[1] * side * k], b.front, 0.35, b.d * 0.5);
        }
    }
    l.on_wall("v_med_cor_wallunita", Pref::Any);
    l.on_wall("prop_sink_02", Pref::Any);
    l.on_wall("m25_2_prop_m52_gurney_01a", Pref::Any);
    l.in_corner("v_med_p_planter");
}

fn restaurant(l: &mut Layout) {
    let (table, chair) = if l.theme == 4 { ("prop_table_03", "prop_table_03_chr") } else { ("prop_table_04", "prop_table_04_chr") };
    // comptoir de commande, la cuisine derrière
    service_counter(l, &["prop_ff_shelves_01", "prop_ff_counter_02", "prop_ff_sink_02"], &["ex_receptiondesk01x", "p_new_j_counter_02", "v_9_kitchen_unit"], Pref::Main, None, true);
    // tables dans la salle, en quinconce
    let (x0, y0) = (l.g.x0, l.g.y0);
    let (w, h) = (l.g.nx as f32 * CELL, l.g.ny as f32 * CELL);
    let mut j = 0;
    let mut y = y0 + 1.4;
    while y < y0 + h - 1.0 {
        let mut x = x0 + 1.4 + if j % 2 == 1 { 1.1 } else { 0.0 };
        while x < x0 + w - 1.0 {
            l.table_group(table, chair, [x, y], [0.0, 1.0], 0.9, true, 4, 2);
            x += 2.4;
        }
        y += 2.3;
        j += 1;
    }
    l.corners(R::Plant, 2);
}

fn coiffeur(l: &mut Layout) {
    let door = l.room.doors.first().map(|d| [(d[0][0] + d[1][0]) * 0.5, (d[0][1] + d[1][1]) * 0.5]).unwrap_or(l.centroid());
    // postes : miroir contre le mur, petit meuble entre deux, fauteuil de coiffure devant chaque miroir
    let names = ["v_ret_mirror", "v_ret_hd_unit1_", "v_ret_mirror", "v_ret_hd_unit1_", "v_ret_mirror", "v_ret_hd_unit1_", "v_ret_mirror"];
    let line = l.line_on_wall(&names, 0.1, Pref::Far(door), false);
    let first = line.first().map(|p| p.c);
    for (i, p) in line.iter().enumerate() {
        if names[i] == "v_ret_mirror" {
            l.in_front(*p, "v_ilev_hd_chair", 0.6, true);
        }
    }
    // bacs à shampoing, accueil avec caisse près de l'entrée, coin d'attente
    l.on_wall("prop_hd_seats_01", Pref::Far(first.unwrap_or(door)));
    if let Some(c) = off_wall(l, "p_new_j_counter_02", 0.9, false, Pref::Near(door)) {
        l.on_top(c, "prop_till_01", [0.0, -0.1], Some([-c.front[0], -c.front[1]]));
        l.clear_front(c, 0.9, 0.2);
    }
    let s = l.pick(R::BigSofa);
    if let Some(sp) = l.on_wall(s, Pref::Near(door)) {
        let ct = l.pick(R::Coffee);
        l.in_front(sp, ct, 0.35, false);
        l.clear_front(sp, 1.1, 0.2);
    }
    l.corners(R::Plant, 2);
}

fn armurerie(l: &mut Layout) {
    // armoires d'armes au fond, vitrines-comptoirs devant (le vendeur entre les deux), râteliers sur les côtés
    let back = ["bkr_prop_gunlocker_01a", "bkr_prop_gunlocker_01a", "bkr_prop_gunlocker_01a", "bkr_prop_gunlocker_01a"];
    if let Some(c) = service_counter(l, &back, &["p_new_j_counter_02", "ex_receptiondesk01x"], Pref::Main, None, true) {
        l.on_top(c, "m24_2_prop_m42_guncase_01a", [-c.w * 0.2, 0.0], None);
    }
    l.along_walls(&["prop_cs_gunrack"], 3, Pref::Any);
}

fn club(l: &mut Layout) {
    // cabine du DJ face à la salle, enceintes de chaque côté, piste de danse libre devant
    let dj = off_wall(l, "sf_prop_sf_dj_desk_01a", 0.9, false, Pref::Main);
    if let Some(d) = dj {
        for side in [1.0f32, -1.0] {
            l.beside(d, "ba_prop_battle_club_speaker_large", side, 0.35);
        }
    }
    let far = dj.map(|d| d.c).unwrap_or(l.centroid());
    // le bar à l'autre bout
    let f = if l.rng.f() < 0.5 { "prop_bar_cooler_03" } else { "prop_bar_fridge_02" };
    let stool = if l.theme == 3 { "v_club_bahbarstool" } else { "prop_bar_stool_01" };
    service_counter(l, &["prop_bar_fridge_03", "v_ret_ml_liqshelfa", "v_ret_ml_liqshelfb", f], &["ch_chint01_reception_desk", "ex_receptiondesk01x", "v_9_kitchen_unit"], Pref::Far(far), Some(stool), true);
    // piste de danse libre devant le DJ
    if let Some(d) = dj {
        let (gw, gh) = (l.g.nx as f32 * CELL, l.g.ny as f32 * CELL);
        let floor = (gw.min(gh) * 0.4).clamp(2.0, 5.0);
        l.clear_front(d, floor, 0.8);
    }
    // carrés VIP : banquettes et tables basses
    for _ in 0..2 {
        let s = l.pick(R::BigSofa);
        let Some(sp) = l.on_wall(s, Pref::Any) else { break };
        let ct = l.pick(R::Coffee);
        l.in_front(sp, ct, 0.35, false);
        l.clear_front(sp, 1.1, 0.2);
    }
    for _ in 0..2 {
        l.in_corner("ba_prop_battle_club_speaker_med");
    }
}

fn labo(l: &mut Layout) {
    let tables = ["bkr_prop_meth_table01a", "bkr_prop_coke_table01a", "bkr_prop_weed_table_01a"];
    let t = tables[(l.rng.next_u32() as usize) % tables.len()];
    l.along_walls(&["prop_shelves_01"], 2, Pref::Any);
    l.rows(&[t], 1.2, 0.5, 6);
    if t == "bkr_prop_meth_table01a" {
        l.in_corner("bkr_prop_meth_phosphorus");
    }
    for _ in 0..2 {
        l.in_corner("v_med_barrel");
    }
}

fn buanderie(l: &mut Layout) {
    l.line_on_wall(&["prop_washer_01", "prop_washer_02", "prop_washer_03", "prop_washer_01", "prop_washer_02"], 0.05, Pref::Main, false);
    l.on_wall("ch_prop_ch_laundry_shelving_02a", Pref::Any);
    l.on_wall("prop_ff_sink_02", Pref::Any);
    let c = l.centroid();
    l.near("m25_1_prop_m51_laundry_trolley_01a", c, [0.0, -1.0], 1.5);
}

fn wc(l: &mut Layout) {
    // toilettes publiques : WC côte à côte contre le mur du fond, lavabos en face, sèche-mains au mur
    let t = if l.theme == 4 { "prop_toilet_02" } else { "prop_toilet_01" };
    let line = l.line_on_wall(&[t, t, t], 0.5, Pref::Main, false);
    let far = line.first().map(|p| p.c).unwrap_or(l.centroid());
    l.line_on_wall(&["prop_sink_02", "prop_sink_02"], 0.3, Pref::Far(far), false);
    l.hang_free("prop_handdry_01", 1.25);
}

fn enfant(l: &mut Layout) {
    let b = l.kept_role(R::Bed).or_else(|| l.on_wall("m24_2_prop_m42_bunkerbed_01a", Pref::Main));
    if let Some(b) = b {
        let night = l.pick(R::Night);
        let lamp = l.pick(R::TableLamp);
        let side = if l.rng.f() < 0.5 { 1.0 } else { -1.0 };
        if let Some(n) = l.beside(b, night, side, 0.06).or_else(|| l.beside(b, night, -side, 0.06)) {
            l.on_top(n, lamp, [0.0, -0.05], None);
        }
        l.rug([b.c[0] + b.front[0] * (b.d * 0.5 + 0.6), b.c[1] + b.front[1] * (b.d * 0.5 + 0.6)], b.front);
        l.clear_front(b, 0.7, 0.0);
    }
    let far = b.map(|b| b.c).unwrap_or(l.centroid());
    let wd = l.pick(R::Wardrobe);
    l.on_wall(wd, Pref::Far(far));
    l.on_wall("v_res_tre_smallbookshelf", Pref::Any);
    for _ in 0..2 {
        l.in_corner("v_corp_facebeanbag");
    }
}

/// Décoration, une fois les meubles posés : objets sur les tables et buffets, tableaux au-dessus du canapé, du
/// lit ou du buffet et sur les murs libres, horloge, appliques, étagère murale, suspension au-dessus de la table.
fn dress(l: &mut Layout, kind: usize) {
    let home = matches!(kind, K_SALON | K_CHAMBRE | K_SAM | K_ENFANT);
    let office = matches!(kind, K_BUREAU | K_ACCUEIL | K_REUNION | K_OPEN);
    let warm = home || office || matches!(kind, K_COIFFEUR | K_RESTO | K_CLUB) || (kind == K_BAR && l.theme != 4);
    let mut art: Vec<&str> = match l.theme {
        1 => vec!["apa_mp_h_acc_artwallm_02", "apa_mp_h_acc_artwallm_03", "apa_mp_h_acc_artwallm_04"],
        3 => vec!["apa_mp_h_acc_artwalll_01", "apa_mp_h_acc_artwalll_02", "apa_mp_h_acc_artwalll_03", "apa_mp_h_acc_artwallm_03", "apa_mp_h_acc_artwallm_04"],
        _ => vec!["m24_1_prop_m41_painting_01a", "m24_1_prop_m41_painting_01b", "m24_1_prop_m41_painting_01c", "m24_1_prop_m41_painting_01d"],
    };
    // ordre au hasard, puis les plus larges d'abord (au-dessus d'un meuble, le plus grand qui tient)
    let k = (l.rng.next_u32() as usize) % art.len();
    art.rotate_left(k);
    let mut by_width = art.clone();
    by_width.sort_by(|a, b| l.wd(b).0.partial_cmp(&l.wd(a).0).unwrap_or(std::cmp::Ordering::Equal));
    let items = l.placed.clone();
    let has = |n: &str, r: R| (1..=4).any(|t| pool(r, t).contains(&n));
    let pick = |l: &mut Layout, v: &[&'static str]| v[(l.rng.next_u32() as usize) % v.len()];
    // objets sur les meubles
    if warm || kind == K_CUISINE {
        for (n, p) in &items {
            let n = n.as_str();
            if has(n, R::Coffee) {
                let o = pick(l, &["m24_2_prop_m42_book_stack_01a", "apa_mp_h_acc_candles_01", "apa_mp_h_acc_vase_05", "apa_mp_h_acc_bowl_ceramic_01"]);
                l.on_top(*p, o, [p.w * 0.15, 0.0], None);
            } else if has(n, R::DineTable) && (home || kind == K_CUISINE) {
                let o = pick(l, &["apa_mp_h_acc_fruitbowl_01", "apa_mp_h_acc_candles_01", "apa_mp_h_acc_vase_05"]);
                l.on_top(*p, o, [0.0, 0.0], None);
            } else if has(n, R::Sideboard) || has(n, R::Dresser) {
                l.on_top(*p, "apa_mp_h_acc_vase_flowers_04", [p.w * 0.28, -p.d * 0.05], None);
                let o = pick(l, &["apa_mp_h_acc_candles_06", "apa_mp_h_acc_dec_sculpt_01", "apa_mp_h_acc_box_trinket_01"]);
                l.on_top(*p, o, [-p.w * 0.3, -p.d * 0.05], None);
            } else if has(n, R::Desk) {
                l.on_top(*p, "m24_2_prop_m42_book_stack_01a", [p.w * 0.32, -p.d * 0.1], None);
            } else if kind == K_ACCUEIL && (n == "ch_chint01_reception_desk" || n == "ex_receptiondesk01x") {
                l.on_top(*p, "apa_mp_h_acc_vase_flowers_04", [-p.w * 0.38, 0.0], None);
            }
        }
    }
    // tableaux au-dessus des meubles bas posés contre un mur
    if warm {
        let mut n_art = 0;
        for (n, p) in &items {
            let n = n.as_str();
            if n_art < 2 && (has(n, R::Sofa) || has(n, R::BigSofa) || has(n, R::Bed) || has(n, R::Sideboard) || n == "m24_2_prop_m42_bunkerbed_01a") && l.hang_above(*p, &by_width, 0.25) {
                n_art += 1;
            }
        }
        let extra = ((l.area() / 16.0) as usize).clamp(1, 2);
        for i in 0..extra {
            let a = art[(i + 1) % art.len()];
            l.hang_free(a, 1.6);
        }
    }
    // horloge
    if matches!(kind, K_CUISINE | K_BUREAU | K_OPEN | K_REUNION | K_ACCUEIL | K_GYM | K_VESTIAIRE | K_HOPITAL | K_RESTO | K_SAM | K_CELLULE) {
        let c = if l.theme == 4 { "prop_game_clock_01" } else { "v_ret_gc_clock" };
        l.hang_free(c, 2.05);
    }
    // appliques
    if matches!(kind, K_BAR | K_CLUB | K_RESTO) || (kind == K_SALON && l.theme == 3) {
        let n = 2 + (l.area() / 20.0) as usize;
        for _ in 0..n.min(6) {
            l.hang_free("m25_1_int_01_wall_light_03", 1.95);
        }
    }
    // étagère murale
    if matches!(kind, K_ENFANT | K_BUREAU) || (kind == K_SALON && l.rng.f() < 0.5) {
        l.hang_free("apa_mp_h_str_shelfwallm_01", 1.75);
    }
    // suspensions au-dessus des tables
    let pend = if l.theme == 3 { "apa_mp_h_lit_lightpendant_05" } else { "apa_mp_h_lit_lightpendant_01" };
    for (n, p) in &items {
        let n = n.as_str();
        if has(n, R::DineTable) && (home || kind == K_CUISINE) {
            l.pendant_over(*p, p.c, pend);
        } else if n == "m24_2_prop_m42_conftable_01a" {
            let r = p.right();
            for s in [-1.0f32, 1.0] {
                l.pendant_over(*p, [p.c[0] + r[0] * p.w * 0.25 * s, p.c[1] + r[1] * p.w * 0.25 * s], "apa_mp_h_lit_lightpendant_01");
            }
        }
    }
}

/// Tous les modèles que les aménagements peuvent poser (contrôle et tailles de secours).
#[cfg_attr(not(test), allow(dead_code))]
pub fn all_models() -> Vec<&'static str> {
    use R::*;
    let mut v: Vec<&'static str> = Vec::new();
    for r in [Sofa, Armchair, Coffee, TvStand, Tv, Rug, FloorLamp, Plant, Shelf, Bed, Night, TableLamp, Wardrobe, Dresser, DineTable, DineChair, Desk, OffChair, Visitor, Cabinet, Monitor, BigSofa, Sideboard] {
        for t in 1..=4 {
            v.extend_from_slice(pool(r, t));
        }
    }
    v.extend_from_slice(&EXTRA_MODELS);
    v.sort();
    v.dedup();
    v
}

#[cfg_attr(not(test), allow(dead_code))]
const EXTRA_MODELS: &[&str] = &[
    "m24_2_prop_m42_bunkerbed_01a", "p_cs_locker_01", "p_cs_locker_02", "bkr_prop_gunlocker_01a", "prop_air_bench_02", "v_med_bed1", "v_med_bed2", "v_med_cor_wallunita", "m25_2_prop_m52_gurney_01a", "v_med_p_planter",
    "prop_table_03", "prop_table_03_chr", "prop_table_04", "prop_table_04_chr", "prop_ff_shelves_01", "prop_ff_counter_02", "prop_ff_sink_02", "v_ret_hd_unit1_", "v_ilev_hd_chair", "prop_hd_seats_01",
    "m24_2_prop_m42_guncase_01a", "prop_cs_gunrack", "sf_prop_sf_dj_desk_01a", "ba_prop_battle_club_speaker_large", "ba_prop_battle_club_speaker_med", "bkr_prop_meth_table01a", "bkr_prop_coke_table01a", "bkr_prop_weed_table_01a", "bkr_prop_meth_phosphorus", "v_med_barrel",
    "prop_washer_01", "prop_washer_02", "prop_washer_03", "ch_prop_ch_laundry_shelving_02a", "m25_1_prop_m51_laundry_trolley_01a", "v_corp_facebeanbag", "v_res_tre_smallbookshelf", "prop_shelves_01", "prop_shelves_03",
    "apa_mp_h_acc_artwallm_02", "apa_mp_h_acc_artwallm_03", "apa_mp_h_acc_artwallm_04", "apa_mp_h_acc_artwalll_01", "apa_mp_h_acc_artwalll_02", "apa_mp_h_acc_artwalll_03", "m24_1_prop_m41_painting_01a", "m24_1_prop_m41_painting_01b", "m24_1_prop_m41_painting_01c", "m24_1_prop_m41_painting_01d",
    "m24_2_prop_m42_book_stack_01a", "apa_mp_h_acc_candles_01", "apa_mp_h_acc_candles_06", "apa_mp_h_acc_vase_05", "apa_mp_h_acc_bowl_ceramic_01", "apa_mp_h_acc_fruitbowl_01", "apa_mp_h_acc_vase_flowers_04", "apa_mp_h_acc_dec_sculpt_01", "apa_mp_h_acc_box_trinket_01",
    "prop_handdry_01", "v_ret_gc_clock", "prop_game_clock_01", "m25_1_int_01_wall_light_03", "apa_mp_h_str_shelfwallm_01", "apa_mp_h_lit_lightpendant_01", "apa_mp_h_lit_lightpendant_05",
    "prop_toolchest_05", "prop_toolchest_02", "v_ind_cs_toolbox1", "prop_compressor_01", "prop_tyre_rack_01", "prop_carjack", "prop_muscle_bench_03", "prop_weight_bench_02", "prop_boxpile_06a", "prop_pallet_02a",
    "imp_carwareoffice_kitch", "v_9_kitchen_unit", "v_res_int_oven", "prop_fridge_03", "ex_receptiondesk01x", "ch_chint01_reception_desk", "p_new_j_counter_02", "imp_prop_impexp_engine_part_01a",
    "v_res_m_sinkunit", "v_res_fridgemoda", "v_res_fridgemodsml", "v_res_tre_fridge", "prop_cs_fridge", "prop_cooker_03", "v_res_ovenhobmod", "prop_microwave_1", "prop_coffee_mac_02", "v_res_fh_kitnstool",
    "v_res_mbath", "prop_toilet_01", "prop_toilet_02", "prop_sink_06", "prop_sink_02", "prop_towel_shelf_01", "prop_plant_int_04a", "prop_copier_01", "prop_watercooler", "m24_2_prop_m42_conftable_01a",
    "prop_tv_flat_michael", "v_res_tre_tvstand", "prop_bar_stool_01", "v_res_m_sinkunit", "v_ret_ml_liqshelfa", "prop_bar_cooler_03", "prop_bar_fridge_02", "prop_bar_fridge_03", "prop_till_01", "v_club_bahbarstool",
    "bkr_prop_biker_barstool_02", "prop_bar_stool_01", "prop_table_06", "ba_prop_int_edgy_table_02", "bkr_prop_clubhouse_chair_01", "prop_pooltable_02", "prop_jukebox_01", "v_ret_ml_fridge", "v_res_fh_kitnstool", "v_ret_ml_liqshelfb",
    "v_ret_247shelves03", "prop_vend_soda_01", "prop_vend_snak_01", "prop_weight_rack_02", "v_ret_mirror",
];

/// Tailles de secours (jeu pas chargé) : (modèle, taille, coin min) relevées dans le jeu.
pub fn fallback_dims(name: &str) -> Option<Dims> {
    FALLBACK.iter().find(|f| f.0 == name).map(|f| (v3(f.1[0], f.1[1], f.1[2]), v3(f.2[0], f.2[1], f.2[2])))
}

include!("decor_sizes.rs");


// ------------------------------------------------------------------ dans l'application

use crate::app::DwApp;
use eframe::egui::{self, RichText};

impl DwApp {
    /// Taille et coin min d'un modèle : lus dans le jeu chargé, sinon la table de secours.
    pub fn decor_dims(&self, name: &str) -> Option<Dims> {
        if let Some(w) = self.game.world.as_ref() {
            if let Some(a) = w.archetypes.get(&crate::gta::crypto::joaat(name)) {
                return Some((a.bb_max - a.bb_min, a.bb_min));
            }
        }
        fallback_dims(name)
    }

    /// Pièce `ri` de l'intérieur (t, m) vue par l'aménageur : contour (coque modelable ou boîte), passages,
    /// objets gardés (ceux pour qui `stays` dit oui).
    fn decor_room_in(&self, t: usize, m: usize, ri: usize, stays: &dyn Fn(&crate::model::Entity) -> bool) -> RoomIn {
        let mlo = &self.proj.ytyps[t].mlos[m];
        let r = &mlo.rooms[ri];
        let droom = if r.free { self.proj.designs.iter().find(|d| d.mlo.eq_ignore_ascii_case(&mlo.name)).and_then(|d| d.rooms.iter().find(|dr| dr.room == r.name)) } else { None };
        // pièce d'un bâtiment de l'atelier coupée par des cloisons en biais ou en L : son emprise exacte
        let house_poly = self.proj.houses.iter().find(|ho| ho.interior.as_deref().map(|n| n.eq_ignore_ascii_case(&mlo.name)).unwrap_or(false)).and_then(|ho| {
            let cells = ho.interior_cells();
            (cells.len() + 1 == mlo.rooms.len()).then(|| cells.get(ri - 1).cloned()).flatten()
        }).filter(|c| c.len() > 1).map(|c| crate::atelier::cells_outline(&c)).filter(|p| p.len() >= 3);
        let (poly, z0, h) = match (droom, house_poly) {
            (Some(dr), _) if dr.pts.len() >= 3 => (dr.pts.clone(), dr.z0, (dr.h - dr.drop).max(2.0)),
            (_, Some(p)) => (p, r.bb_min.z, (r.bb_max.z - r.bb_min.z).max(2.0)),
            _ => (vec![[r.bb_min.x, r.bb_min.y], [r.bb_max.x, r.bb_min.y], [r.bb_max.x, r.bb_max.y], [r.bb_min.x, r.bb_max.y]], r.bb_min.z, (r.bb_max.z - r.bb_min.z).max(2.0)),
        };
        let mut doors = Vec::new();
        let mut windows = Vec::new();
        for p in mlo.portals.iter().filter(|p| p.room_from as usize == ri || p.room_to as usize == ri) {
            let c = &p.corners;
            let zmin = c.iter().map(|v| v.z).fold(f32::MAX, f32::min);
            // un portail à plat (trémie d'escalier) : ses diagonales bloquent
            if c.iter().all(|v| (v.z - zmin).abs() < 0.05) {
                doors.push([[c[0].x, c[0].y], [c[2].x, c[2].y]]);
                doors.push([[c[1].x, c[1].y], [c[3].x, c[3].y]]);
                continue;
            }
            // les deux coins du bas, à plat
            let mut low: Vec<&V3> = c.iter().collect();
            low.sort_by(|a, b| a.z.partial_cmp(&b.z).unwrap_or(std::cmp::Ordering::Equal));
            let seg = [[low[0].x, low[0].y], [low[1].x, low[1].y]];
            if zmin < z0 + 0.4 {
                doors.push(seg);
            } else {
                windows.push(seg);
            }
        }
        let keep = mlo
            .entities
            .iter()
            .filter(|e| e.room == ri && stays(e))
            .filter_map(|e| {
                let (s, mn) = self.decor_dims(&e.archetype)?;
                let c = e.pos + e.rot.rotate(mn + s * 0.5);
                let f = e.rot.rotate(v3(1.0, 0.0, 0.0));
                Some(KeepIn { name: e.archetype.clone(), c: [c.x, c.y], hs: [s.x * 0.5, s.y * 0.5], yaw: f.y.atan2(f.x) })
            })
            .collect();
        RoomIn { poly, z0, h, doors, windows, keep }
    }

    /// Aménage la pièce : retire d'abord ses meubles (sauf avec `keep` ; portes et coques gardées de toute façon),
    /// pose ceux du plan, et avec `assorted` l'ambiance, le sol et les lumières du style. Rend (meubles posés,
    /// message).
    /// Cet objet de la pièce reste-t-il quand on aménage ? Toujours les portes et la coque ; avec `keep` tout ; sinon
    /// les objets choisis dans la vue (sauf si « garder les objets choisis » est décoché).
    fn decor_stays(&self, e: &crate::model::Entity, keep: bool) -> bool {
        keep || crate::model::is_door_model(&e.archetype) || e.archetype.starts_with("dw_") || (!self.furn.decor_drop_sel && self.sel_set.contains(&e.id))
    }

    /// Plan d'aménagement de la pièce sans rien poser (aperçus) : comme `decorate_room` le ferait.
    pub fn decor_plan(&self, t: usize, m: usize, ri: usize, kind: usize, theme: usize, seed: u64, keep: bool) -> (RoomIn, Plan) {
        let room = self.decor_room_in(t, m, ri, &|e| self.decor_stays(e, keep));
        let dims = |n: &str| self.decor_dims(n);
        let plan = furnish(&room, kind, theme, seed, &dims);
        (room, plan)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn decorate_room(&mut self, t: usize, m: usize, ri: usize, kind: usize, theme: usize, seed: u64, keep: bool, assorted: bool) -> (usize, String) {
        if !keep {
            let gone: std::collections::HashSet<u64> = self.proj.ytyps[t].mlos[m].entities.iter().filter(|e| e.room == ri && !self.decor_stays(e, false)).map(|e| e.id).collect();
            self.proj.ytyps[t].mlos[m].entities.retain(|e| !gone.contains(&e.id));
        }
        let room = self.decor_room_in(t, m, ri, &|_| true);
        let dims = |n: &str| self.decor_dims(n);
        let plan = furnish(&room, kind, theme, seed, &dims);
        let n = plan.items.len();
        for it in &plan.items {
            let mut e = crate::model::Entity::new(&it.arch, it.pos);
            e.flags = crate::mech::MLO_ENTITY_FLAGS;
            e.room = ri;
            e.rot = Quat::from_z(it.yaw);
            self.proj.ytyps[t].mlos[m].entities.push(e);
        }
        let mut extra = String::new();
        if assorted {
            if let Some(a) = plan.ambiance {
                self.proj.ytyps[t].mlos[m].rooms[ri].timecycle = a.to_string();
                extra.push_str(&format!(", ambiance « {a} »"));
            }
            // coque modelable : sol et lumières du style, si la pièce n'en a pas déjà
            let (mname, rname, free) = {
                let mlo = &self.proj.ytyps[t].mlos[m];
                (mlo.name.clone(), mlo.rooms[ri].name.clone(), mlo.rooms[ri].free)
            };
            if free && self.proj.designs.iter().any(|d| d.mlo.eq_ignore_ascii_case(&mname)) {
                let poly = room.poly.clone();
                let d = self.design_mut(&mname);
                if let Some(dr) = d.rooms.iter_mut().find(|dr| dr.room == rname) {
                    // un sol repris du jeu ou importé (texture du projet) est gardé
                    if dr.floor < crate::design::CUSTOM_BASE {
                        dr.floor = plan.floor;
                    }
                }
                let has = d.lights.iter().any(|l| in_poly(&poly, l.p[0], l.p[1]) && l.p[2] > room.z0 - 0.5 && l.p[2] < room.z0 + room.h + 0.5);
                if !has {
                    for lo in &plan.lights {
                        d.lights.push(crate::design::DLight { p: [lo.p.x, lo.p.y, lo.p.z], kind: 0, color: lo.color, intensity: 8.0, range: 6.0 });
                    }
                    extra.push_str(&format!(", {} lumière(s)", plan.lights.len()));
                }
                self.shell_sig = 0;
            }
        }
        (n, format!("{} ({}) : {n} meuble(s){extra}.", KINDS[kind].0, THEMES[plan.theme]))
    }

    /// Partie « Aménager » de la pièce choisie : type de pièce, style, Aménager / Varier / toutes les pièces.
    pub fn decor_ui(&mut self, ui: &mut egui::Ui, t: usize, m: usize, ri: usize) {
        let rname = self.proj.ytyps[t].mlos[m].rooms[ri].name.clone();
        // type deviné d'après le nom, une fois par pièce choisie
        if self.furn.decor_guessed != Some(ri) {
            self.furn.decor_guessed = Some(ri);
            if let Some(k) = kind_from_name(&rname) {
                self.furn.decor_kind = k;
            }
        }
        let mut run: Option<bool> = None; // Some(false) : cette pièce ; Some(true) : toutes
        let mut vary = false;
        let mut ideas = false;
        let mut take: Option<u64> = None;
        egui::CollapsingHeader::new(RichText::new(crate::look::label(crate::look::ic::MAGIC_WAND, "Aménager la pièce (tout prêt)")).strong()).id_salt("decor_hdr").default_open(true).show(ui, |ui| {
            ui.label(RichText::new("Meubles et déco du jeu posés avec leur vraie taille, contre les murs, sans boucher les portes ; « Varier » change les meubles et la disposition.").small().weak());
            for (gname, kinds) in KIND_GROUPS {
                ui.label(RichText::new(*gname).small().weak());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                    for &k in kinds.iter() {
                        let (n, icon) = KINDS[k];
                        if ui.add(egui::Button::new(crate::look::label(icon, n)).selected(self.furn.decor_kind == k)).clicked() {
                            self.furn.decor_kind = k;
                        }
                    }
                });
            }
            let kind = self.furn.decor_kind.min(KINDS.len() - 1);
            let themes = themes_for(kind);
            if self.furn.decor_theme != 0 && !themes.contains(&self.furn.decor_theme) {
                self.furn.decor_theme = 0;
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("Style");
                for th in std::iter::once(0).chain(themes.iter().copied()) {
                    if ui.selectable_label(self.furn.decor_theme == th, THEMES[th]).clicked() {
                        self.furn.decor_theme = th;
                    }
                }
            });
            let mut clear = !self.furn.decor_keep;
            if ui.checkbox(&mut clear, "Vider la pièce d'abord").on_hover_text("Retire les meubles de la pièce avant d'aménager (les portes restent)").changed() {
                self.furn.decor_keep = !clear;
            }
            let nsel = self.proj.ytyps[t].mlos[m].entities.iter().filter(|e| e.room == ri && self.sel_set.contains(&e.id)).count();
            let mut keep_sel = !self.furn.decor_drop_sel;
            let label = if nsel > 0 { format!("Garder les objets choisis ({nsel})") } else { "Garder les objets choisis".to_string() };
            if ui
                .checkbox(&mut keep_sel, label)
                .on_hover_text("Choisis dans la vue (clic, Ctrl+clic) les meubles qui te plaisent : ils restent en place, et « Varier » refait le reste autour d'eux (le canapé gardé reçoit sa table basse et sa télé, le lit ses tables de chevet…)")
                .changed()
            {
                self.furn.decor_drop_sel = !keep_sel;
            }
            let mut amb = !self.furn.decor_no_amb;
            if ui.checkbox(&mut amb, "Ambiance assortie (et sol, lumières d'une coque modelable)").changed() {
                self.furn.decor_no_amb = !amb;
            }
            ui.horizontal_wrapped(|ui| {
                if crate::look::accent(ui, crate::look::ic::MAGIC_WAND, &format!("Aménager « {rname} »"), "Meuble et décore cette pièce selon le type et le style choisis").clicked() {
                    run = Some(false);
                }
                if ui.button(crate::look::label(crate::look::ic::SHUFFLE, "Varier")).on_hover_text("Refaire la même pièce avec d'autres meubles et une autre disposition").clicked() {
                    run = Some(false);
                    vary = true;
                }
                if ui.button(crate::look::label(crate::look::ic::SQUARES_FOUR, "3 idées")).on_hover_text("Propose trois aménagements en plan, vus de dessus : clique celui que tu préfères").clicked() {
                    ideas = true;
                }
                if ui.button(crate::look::label(crate::look::ic::STACK, "Toutes les pièces")).on_hover_text("Chaque pièce prend le type deviné d'après son nom (cuisine, chambre, bureau…), sinon le type choisi ici").clicked() {
                    run = Some(true);
                }
            });
            // trois idées : plans vus de dessus, à choisir
            let key = self.decor_prev_key(t, m, ri);
            if ideas {
                self.furn.decor_prev.clear();
                let base = self.furn.decor_seed.wrapping_mul(6364136223846793005).wrapping_add(self.frame.wrapping_mul(1442695040888963407)) | 1;
                for i in 0..3u64 {
                    let seed = base.wrapping_add(i.wrapping_mul(0x9e37_79b9_7f4a_7c15));
                    let (room, plan) = self.decor_plan(t, m, ri, kind, self.furn.decor_theme, seed, self.furn.decor_keep);
                    self.furn.decor_prev.push((seed, room, plan));
                }
                self.furn.decor_prev_key = key;
            }
            if !self.furn.decor_prev.is_empty() && self.furn.decor_prev_key == key {
                ui.label(RichText::new("Clique l'idée qui te plaît (vue de dessus ; orange : assises, brun : tables, vert : lits, gris : rangements).").small().weak());
                ui.horizontal(|ui| {
                    let gap = ui.spacing().item_spacing.x;
                    let w = ((ui.available_width() - gap * 2.0 - 2.0) / 3.0).clamp(50.0, 140.0);
                    for (seed, room, plan) in &self.furn.decor_prev {
                        let r = self.plan_thumb(ui, room, plan, w);
                        if r.on_hover_text(format!("{} : {} objet(s)", THEMES[plan.theme], plan.items.len())).clicked() {
                            take = Some(*seed);
                        }
                    }
                });
            }
        });
        if let Some(seed) = take {
            self.furn.decor_seed = seed;
            self.furn.decor_prev.clear();
            run = Some(false);
        }
        let Some(all) = run else { return };
        if (vary || self.furn.decor_seed == 0) && take.is_none() {
            self.furn.decor_seed = self.furn.decor_seed.wrapping_add(0x5851_f42d).wrapping_mul(6364136223846793005) ^ self.frame;
        }
        self.furn.decor_prev.clear();
        let (kind, theme, seed, keep, assorted) = (self.furn.decor_kind, self.furn.decor_theme, self.furn.decor_seed, self.furn.decor_keep, !self.furn.decor_no_amb);
        self.push_undo();
        if all {
            let n = self.proj.ytyps[t].mlos[m].rooms.len();
            let mut total = 0;
            for r in 1..n {
                let k = kind_from_name(&self.proj.ytyps[t].mlos[m].rooms[r].name).unwrap_or(kind);
                total += self.decorate_room(t, m, r, k, theme, seed ^ (r as u64 * 7919), keep, assorted).0;
            }
            self.status = format!("{} pièce(s) aménagée(s) : {total} objet(s).", n.saturating_sub(1));
        } else {
            self.status = self.decorate_room(t, m, ri, kind, theme, seed, keep, assorted).1;
        }
    }

    /// Clé des idées affichées : elles ne valent que pour cette pièce, ce type, ce style et ces objets.
    pub(crate) fn decor_prev_key(&self, t: usize, m: usize, ri: usize) -> u64 {
        let mlo = &self.proj.ytyps[t].mlos[m];
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100_0000_01b3);
        };
        for v in [t as u64, m as u64, ri as u64, self.furn.decor_kind as u64, self.furn.decor_theme as u64, self.furn.decor_keep as u64, self.furn.decor_drop_sel as u64] {
            mix(v);
        }
        for e in mlo.entities.iter().filter(|e| e.room == ri && !e.archetype.starts_with("dw_")) {
            mix(e.id);
            mix(self.sel_set.contains(&e.id) as u64);
        }
        h
    }

    /// Plan vu de dessus d'une idée d'aménagement : contour, portes, meubles au sol en couleurs.
    fn plan_thumb(&self, ui: &mut egui::Ui, room: &RoomIn, plan: &Plan, w: f32) -> egui::Response {
        let (mut mn, mut mx) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in &room.poly {
            mn = [mn[0].min(p[0]), mn[1].min(p[1])];
            mx = [mx[0].max(p[0]), mx[1].max(p[1])];
        }
        let (rw, rh) = ((mx[0] - mn[0]).max(0.5), (mx[1] - mn[1]).max(0.5));
        let h = (w * rh / rw).clamp(40.0, 160.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
        let painter = ui.painter_at(rect);
        let bg = if resp.hovered() { egui::Color32::from_rgb(52, 46, 40) } else { egui::Color32::from_rgb(34, 36, 42) };
        painter.rect_filled(rect, 4.0, bg);
        let sc = ((w - 8.0) / rw).min((h - 8.0) / rh);
        let off = egui::vec2((w - rw * sc) * 0.5, (h - rh * sc) * 0.5);
        // y vers le haut (nord en haut)
        let to = |x: f32, y: f32| rect.min + off + egui::vec2((x - mn[0]) * sc, (mx[1] - y) * sc);
        let pts: Vec<egui::Pos2> = room.poly.iter().map(|p| to(p[0], p[1])).collect();
        painter.add(egui::Shape::convex_polygon(pts.clone(), egui::Color32::from_rgb(70, 64, 58), egui::Stroke::NONE));
        painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.5_f32, egui::Color32::from_gray(200))));
        for d in &room.doors {
            painter.line_segment([to(d[0][0], d[0][1]), to(d[1][0], d[1][1])], egui::Stroke::new(3.0_f32, crate::look::ACCENT));
        }
        for k in &room.keep {
            self.thumb_box(&painter, &to, k.c, k.hs, k.yaw, egui::Color32::from_rgb(120, 170, 230));
        }
        for it in &plan.items {
            let Some((s, m0)) = self.decor_dims(&it.arch) else { continue };
            if it.pos.z + m0.z > room.z0 + 0.3 {
                continue; // posé sur un meuble ou accroché
            }
            let q = Quat::from_z(it.yaw);
            let c = it.pos + q.rotate(m0 + s * 0.5);
            let n = it.arch.as_str();
            let col = if is_flat(n) {
                egui::Color32::from_rgba_unmultiplied(160, 120, 90, 70)
            } else if ["bed", "gurney"].iter().any(|k| n.contains(k)) {
                egui::Color32::from_rgb(110, 170, 110)
            } else if ["sofa", "chair", "chr", "stool", "couch", "bench", "beanbag", "seats"].iter().any(|k| n.contains(k)) {
                egui::Color32::from_rgb(225, 140, 70)
            } else if ["table", "desk", "counter", "kitch"].iter().any(|k| n.contains(k)) {
                egui::Color32::from_rgb(150, 105, 70)
            } else if s.z > 1.3 {
                egui::Color32::from_rgb(120, 125, 140)
            } else {
                egui::Color32::from_rgb(175, 175, 180)
            };
            self.thumb_box(&painter, &to, [c.x, c.y], [s.x * 0.5, s.y * 0.5], it.yaw, col);
        }
        if resp.hovered() {
            painter.rect_stroke(rect, 4.0, egui::Stroke::new(2.0_f32, crate::look::ACCENT), egui::StrokeKind::Inside);
        }
        resp
    }

    fn thumb_box(&self, painter: &egui::Painter, to: &dyn Fn(f32, f32) -> egui::Pos2, c: [f32; 2], hs: [f32; 2], yaw: f32, col: egui::Color32) {
        let (sn, co) = yaw.sin_cos();
        let pts: Vec<egui::Pos2> = [[-1.0f32, -1.0f32], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
            .iter()
            .map(|q| {
                let (lx, ly) = (q[0] * hs[0], q[1] * hs[1]);
                to(c[0] + lx * co - ly * sn, c[1] + lx * sn + ly * co)
            })
            .collect();
        painter.add(egui::Shape::convex_polygon(pts, col, egui::Stroke::new(0.5_f32, egui::Color32::from_black_alpha(140))));
    }
}

/// Types de pièce rangés par famille, pour les boutons.
const KIND_GROUPS: &[(&str, &[usize])] = &[
    ("Maison", &[K_SALON, K_CHAMBRE, K_ENFANT, K_CUISINE, K_SAM, K_SDB, K_WC, K_BUANDERIE]),
    ("Travail", &[K_BUREAU, K_OPEN, K_REUNION, K_ACCUEIL]),
    ("Commerces et loisirs", &[K_BAR, K_RESTO, K_SHOP, K_COIFFEUR, K_ARMURERIE, K_CLUB, K_GYM]),
    ("Services (police, santé)", &[K_CELLULE, K_VESTIAIRE, K_HOPITAL]),
    ("Atelier et illégal", &[K_GARAGE, K_ENTREPOT, K_LABO]),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f32, d: f32) -> RoomIn {
        RoomIn { poly: vec![[0.0, 0.0], [w, 0.0], [w, d], [0.0, d]], z0: 0.0, h: 3.0, doors: vec![[[w * 0.5 - 0.5, 0.0], [w * 0.5 + 0.5, 0.0]]], windows: vec![], keep: vec![] }
    }

    #[test]
    fn rotation_face_avant() {
        // cap pour regarder vers +X : (0,-1) tourné doit donner (1,0)
        let y = Layout::yaw_for("x", [1.0, 0.0]);
        let r = Quat::from_z(y).rotate(v3(0.0, -1.0, 0.0));
        assert!((r.x - 1.0).abs() < 1e-4 && r.y.abs() < 1e-4, "{r:?}");
    }

    #[test]
    fn tous_les_types_posent_des_meubles_dans_la_piece() {
        let room = rect(7.0, 5.5);
        for k in 0..KINDS.len() {
            for seed in 1..4u64 {
                let p = furnish(&room, k, 0, seed, &fallback_dims);
                assert!(p.items.len() >= 2, "{} : {} meuble(s)", KINDS[k].0, p.items.len());
                for it in &p.items {
                    assert!(it.pos.x > -0.5 && it.pos.x < 7.5 && it.pos.y > -0.5 && it.pos.y < 6.0, "{} : {} hors de la pièce {:?}", KINDS[k].0, it.arch, it.pos);
                }
                assert!(!p.lights.is_empty());
            }
        }
    }

    #[test]
    fn passage_de_la_porte_libre() {
        let room = rect(6.0, 5.0);
        for k in 0..KINDS.len() {
            let p = furnish(&room, k, 0, 7, &fallback_dims);
            for it in p.items.iter().filter(|i| !is_flat(&i.arch)) {
                let d = seg_dist([it.pos.x, it.pos.y], [2.5, 0.0], [3.5, 0.0]);
                assert!(d > 0.7, "{} : {} devant la porte ({d:.2} m)", KINDS[k].0, it.arch);
            }
        }
    }

    #[test]
    fn varier_change_l_amenagement() {
        let room = rect(6.0, 5.0);
        let a = furnish(&room, K_SALON, 0, 1, &fallback_dims);
        let b = furnish(&room, K_SALON, 0, 2, &fallback_dims);
        let sig = |p: &Plan| p.items.iter().map(|i| format!("{}{:.1}{:.1}", i.arch, i.pos.x, i.pos.y)).collect::<Vec<_>>().join("|");
        assert_ne!(sig(&a), sig(&b));
    }

    #[test]
    fn tous_les_modeles_ont_une_taille() {
        for n in all_models() {
            assert!(fallback_dims(n).is_some(), "{n} absent du catalogue du jeu");
        }
    }

    #[test]
    fn types_devines() {
        assert_eq!(kind_from_name("Cuisine"), Some(K_CUISINE));
        assert_eq!(kind_from_name("office_2"), Some(K_BUREAU));
        assert_eq!(kind_from_name("e1_piece1"), None);
        assert_eq!(kind_from_style("magasin"), K_SHOP);
    }
}
