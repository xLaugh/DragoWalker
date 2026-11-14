//! Routes façon GTA V pour l'éditeur de terrain : on clique des points, la route suit une courbe lissée,
//! garde une pente douce, aplanit le terrain dessous et devient un vrai modèle : goudron avec marquages
//! (lignes jaunes au centre, lignes blanches pleines ou en tirets), bordures et trottoirs, plus sa collision.
//! Les valeurs des marquages (largeurs, tirets de 3 m tous les 12 m…) sont **estimées** d'après l'aspect des
//! routes du jeu ; elles ne sont pas lues dans les fichiers du jeu.

use crate::gta::drawable::{TexData, TexFmt};
use crate::math::{v3, V3};
use serde::{Deserialize, Serialize};

/// Un marquage au sol : décalage depuis le centre (m), largeur (m), jaune ou blanc, tirets ou plein.
#[derive(Clone, Copy)]
pub struct Line {
    pub off: f32,
    pub w: f32,
    pub yellow: bool,
    pub dashed: bool,
}

const fn ln(off: f32, w: f32, yellow: bool, dashed: bool) -> Line {
    Line { off, w, yellow, dashed }
}

/// Type de route.
pub struct RoadKind {
    pub name: &'static str,
    pub tip: &'static str,
    /// largeur de la chaussée (m)
    pub width: f32,
    /// largeur de chaque trottoir (0 = pas de trottoir)
    pub sidewalk: f32,
    /// hauteur de la bordure du trottoir (m)
    pub curb: f32,
    /// accotement sans trottoir (m) et sa texture (matériau du terrain)
    pub shoulder: f32,
    pub shoulder_mat: u8,
    pub lines: &'static [Line],
    /// chemin de terre (texture de terre avec traces de pneus, pas de marquage)
    pub dirt: bool,
    /// surface de collision (nom de materials.dat) de la chaussée
    pub coll: &'static str,
    /// passerelle pour piétons : toujours un pont, escaliers aux deux bouts, pas de voitures ni de carrefour
    pub foot: bool,
    /// bretelle d'autoroute : une voie à sens unique (sens du tracé), qui se fond dans l'autoroute sans carrefour
    pub ramp: bool,
}

pub const KINDS: [RoadKind; 7] = [
    RoadKind {
        name: "Rue de ville",
        tip: "2 voies, double ligne jaune au centre, places de stationnement, trottoirs avec bordure (11 m + 2 × 3 m)",
        width: 11.0,
        sidewalk: 3.0,
        curb: 0.18,
        shoulder: 0.0,
        shoulder_mat: 4,
        lines: &[ln(-0.16, 0.12, true, false), ln(0.16, 0.12, true, false), ln(-3.8, 0.12, false, false), ln(3.8, 0.12, false, false)],
        dirt: false,
        coll: "TARMAC",
        foot: false,
        ramp: false,
    },
    RoadKind {
        name: "Avenue",
        tip: "4 voies, double ligne jaune, lignes blanches en tirets entre les voies, trottoirs larges (15,6 m + 2 × 3,5 m)",
        width: 15.6,
        sidewalk: 3.5,
        curb: 0.18,
        shoulder: 0.0,
        shoulder_mat: 4,
        lines: &[ln(-0.16, 0.12, true, false), ln(0.16, 0.12, true, false), ln(-3.9, 0.12, false, true), ln(3.9, 0.12, false, true), ln(-7.45, 0.15, false, false), ln(7.45, 0.15, false, false)],
        dirt: false,
        coll: "TARMAC",
        foot: false,
        ramp: false,
    },
    RoadKind {
        name: "Autoroute",
        tip: "2 × 3 voies, lignes jaunes le long du terre-plein, tirets blancs, bandes d'arrêt, accotements en béton (24 m)",
        width: 24.0,
        sidewalk: 0.0,
        curb: 0.0,
        shoulder: 1.5,
        shoulder_mat: 4,
        lines: &[
            ln(-0.7, 0.15, true, false),
            ln(0.7, 0.15, true, false),
            ln(-4.4, 0.15, false, true),
            ln(4.4, 0.15, false, true),
            ln(-8.1, 0.15, false, true),
            ln(8.1, 0.15, false, true),
            ln(-11.6, 0.2, false, false),
            ln(11.6, 0.2, false, false),
        ],
        dirt: false,
        coll: "TARMAC",
        foot: false,
        ramp: false,
    },
    RoadKind {
        name: "Route de campagne",
        tip: "2 voies étroites, ligne jaune en tirets au centre, lignes blanches de bord, accotements de terre (7,6 m)",
        width: 7.6,
        sidewalk: 0.0,
        curb: 0.0,
        shoulder: 1.2,
        shoulder_mat: 1,
        lines: &[ln(0.0, 0.15, true, true), ln(-3.55, 0.12, false, false), ln(3.55, 0.12, false, false)],
        dirt: false,
        coll: "TARMAC",
        foot: false,
        ramp: false,
    },
    RoadKind {
        name: "Chemin de terre",
        tip: "Piste de terre avec traces de pneus, sans marquage (5 m)",
        width: 5.0,
        sidewalk: 0.0,
        curb: 0.0,
        shoulder: 0.8,
        shoulder_mat: 1,
        lines: &[],
        dirt: true,
        coll: "DIRT_TRACK",
        foot: false,
        ramp: false,
    },
    RoadKind {
        name: "Passerelle",
        tip: "Pont pour piétons au-dessus des routes ou d'un ravin, escaliers aux deux bouts, garde-corps (3 m)",
        width: 3.0,
        sidewalk: 0.0,
        curb: 0.0,
        shoulder: 0.0,
        shoulder_mat: 4,
        lines: &[],
        dirt: false,
        coll: "CONCRETE",
        foot: true,
        ramp: false,
    },
    RoadKind {
        name: "Bretelle d'autoroute",
        tip: "Entrée ou sortie d'autoroute : 1 voie à sens unique (dans le sens du tracé), lignes blanches de bord, accotements en béton ; un bout se pose au bord d'une autoroute (5 m + 2 × 1 m)",
        width: 5.0,
        sidewalk: 0.0,
        curb: 0.0,
        shoulder: 1.0,
        shoulder_mat: 4,
        lines: &[ln(-2.3, 0.15, false, false), ln(2.3, 0.15, false, false)],
        dirt: false,
        coll: "TARMAC",
        foot: false,
        ramp: true,
    },
];

/// Longueur de route couverte par une répétition de la texture de chaussée : un tiret de 3 m tous les 12 m.
pub const ROAD_TEX_LEN: f32 = 12.0;
/// Pas entre deux points de la courbe d'une route (m).
pub const STEP: f32 = 1.5;
/// Pente maximale d'une route par défaut (12 %) — **estimé** ; réglable de GRADE_MIN à GRADE_MAX par route.
pub const MAX_GRADE: f32 = 0.12;
pub const GRADE_MIN: f32 = 0.02;
pub const GRADE_MAX: f32 = 0.25;

fn default_grade() -> f32 {
    MAX_GRADE
}

// Ponts — toutes ces mesures sont **estimées** d'après l'aspect des ponts du jeu, pas lues dans ses fichiers.
/// Hauteur au-dessus du sol à partir de laquelle une route tracée en mode pont devient un pont (m).
pub const BRIDGE_MIN_CLEAR: f32 = 2.5;
/// Épaisseur du tablier sous la chaussée (poutre en caisson) : route, passerelle (m).
pub const DECK_DEPTH: f32 = 1.4;
pub const FOOT_DEPTH: f32 = 0.6;
/// Hauteur libre sous un pont qui passe au-dessus d'une route (gabarit des autoroutes, m).
pub const CLEARANCE: f32 = 5.3;
/// Hauteur libre sous une passerelle qui ne passe au-dessus d'aucune route (m).
pub const FOOT_CLEAR: f32 = 2.5;
/// Écart entre deux piles (m).
pub const PIER_STEP: f32 = 30.0;
/// Garde-corps en béton : hauteur (route, passerelle) et épaisseur (m).
pub const PARAPET_H: f32 = 0.9;
pub const FOOT_PARAPET_H: f32 = 1.1;
pub const PARAPET_W: f32 = 0.3;
/// Marches des escaliers de passerelle : hauteur et profondeur (m).
pub const STEP_RISE: f32 = 0.17;
pub const STEP_RUN: f32 = 0.30;
/// Deux routes dont les hauteurs diffèrent de plus que ça à leur croisement ne se rejoignent pas : l'une passe
/// au-dessus de l'autre (m).
pub const JOIN_DZ: f32 = 3.0;
/// Partie de modèle « structure de pont » (piles, tablier, garde-corps, escaliers) : collision en béton.
pub const BRIDGE_COLL: u8 = 3;
/// Glissières : collision en métal ; îlot de rond-point : collision d'herbe.
pub const RAIL_COLL: u8 = 4;
pub const GRASS_COLL: u8 = 5;
/// Collision des marches des passerelles : béton marqué « escalier » (MAT_STAIRS).
pub const STAIRS_COLL: u8 = 6;

// Tunnels — mesures **estimées** d'après l'aspect des tunnels du jeu.
/// Hauteur libre du tube au-dessus de la chaussée (m).
pub const TUNNEL_H: f32 = 6.0;
/// Terre au-dessus du toit pour qu'une route tracée en mode tunnel passe en tunnel (m).
pub const TUNNEL_COVER: f32 = 2.0;
/// Écart entre deux lumières du tunnel (m).
pub const TUNNEL_LIGHT_STEP: f32 = 12.0;
/// Façade de chaque entrée : débord de chaque côté du tube et au-dessus (m).
pub const PORTAL_SIDE: f32 = 8.0;
pub const PORTAL_TOP: f32 = 5.0;

/// Raccord entre deux types de route (une rue qui prolonge une avenue) : longueur sur laquelle la largeur passe de
/// l'une à l'autre (m) — **estimé**.
pub const TAPER_LEN: f32 = 30.0;

// Équipements — **estimés** d'après les rues du jeu.
/// Écart entre deux lampadaires d'un même côté (m), et sur l'autoroute (au terre-plein).
pub const LAMP_STEP: f32 = 30.0;
pub const LAMP_STEP_HIGHWAY: f32 = 40.0;
/// Lampadaire à un bras (bord de route) et à deux bras (terre-plein d'autoroute), objets du jeu.
pub const LAMP_SINGLE: &str = "prop_streetlight_01";
pub const LAMP_DOUBLE: &str = "prop_streetlight_03d";
/// Glissière : hauteur du bas et du haut de la lisse (au-dessus de l'accotement), écart des poteaux (m).
pub const RAIL_LOW: f32 = 0.45;
pub const RAIL_HIGH: f32 = 0.8;
pub const RAIL_POST_STEP: f32 = 2.0;
/// Rayon par défaut d'un rond-point (axe de la chaussée, m).
pub const RING_RADIUS: f32 = 20.0;

/// Équipements d'une route, posés tout seuls.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Furniture {
    /// lampadaires du jeu le long de la route
    pub lights: bool,
    /// glissières de sécurité (routes sans trottoir)
    pub rails: bool,
    /// décalques au sol : fissures, taches, rustines
    pub decals: bool,
}

impl Furniture {
    /// Réglage par défaut selon le type : lampadaires en ville, sur l'autoroute et les bretelles ; glissières sur
    /// l'autoroute et les bretelles ; décalques sur toutes les routes goudronnées.
    pub fn for_kind(k: &RoadKind) -> Furniture {
        let paved = !k.dirt && !k.foot;
        Furniture { lights: paved && (k.sidewalk > 0.0 || k.ramp || k.width >= 20.0), rails: k.ramp || k.width >= 20.0, decals: paved }
    }
}

/// Clés des textures (dans le même dictionnaire que celles du terrain).
pub fn road_tex_key(kind: usize) -> u64 {
    0xD7E2_0000_0000_0100u64 | kind as u64
}
pub const SIDEWALK_KEY: u64 = 0xD7E2_0000_0000_0110;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Road {
    pub kind: u8,
    /// points cliqués (x, y)
    pub pts: Vec<[f32; 2]>,
    /// points de la courbe : x, y, hauteur de la chaussée, distance depuis le début
    pub samples: Vec<[f32; 4]>,
    /// trottoirs / accotements ouverts (jonctions) : distance de début, de fin, côté (-1 gauche, 1 droite, 0 les deux)
    #[serde(default)]
    pub gaps: Vec<[f32; 3]>,
    /// réglages de circulation
    #[serde(default)]
    pub traffic: RoadTraffic,
    /// pente maximale (0,12 = 12 %)
    #[serde(default = "default_grade")]
    pub grade: f32,
    /// tracée en mode pont (hauteur gardée au-dessus des creux et des routes croisées)
    #[serde(default)]
    pub elevated: bool,
    /// tronçons en pont : distance de début et de fin le long de la route (terrain laissé tel quel dessous)
    #[serde(default)]
    pub spans: Vec<[f32; 2]>,
    /// piles : distance le long de la route et hauteur du sol à leur pied
    #[serde(default)]
    pub piers: Vec<[f32; 2]>,
    /// culées (murs aux deux bouts de chaque tronçon en pont) : distance, hauteur du sol
    #[serde(default)]
    pub abutments: Vec<[f32; 2]>,
    /// passerelle : hauteur du sol au pied de l'escalier du début et de celui de la fin
    #[serde(default)]
    pub stairs: Option<[f32; 2]>,
    /// tracée en mode tunnel (passe sous les reliefs au lieu de les suivre)
    #[serde(default)]
    pub buried: bool,
    /// tronçons en tunnel : distance de début et de fin
    #[serde(default)]
    pub tunnels: Vec<[f32; 2]>,
    /// raccord au début avec une route d'un autre type : (type de l'autre route, longueur du raccord)
    #[serde(default)]
    pub taper: Option<(u8, f32)>,
    /// rond-point : boucle fermée (le dernier point est le premier), sens unique (sens inverse des aiguilles)
    #[serde(default)]
    pub ring: bool,
    /// sens unique dans le sens du tracé (bretelle)
    #[serde(default)]
    pub oneway: bool,
    /// équipements posés tout seuls
    #[serde(default)]
    pub furniture: Furniture,
    /// lampadaires posés par DragoWalker pour cette route (identifiants des objets)
    #[serde(default)]
    pub props: Vec<u64>,
}

/// Carrefour : disque de goudron sans marquage posé sur la jonction de deux routes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Junction {
    pub pos: [f32; 3],
    pub r: f32,
    pub dirt: bool,
    /// branches : une par route qui part du carrefour (deux pour une route qui le traverse)
    #[serde(default)]
    pub arms: Vec<JArm>,
    /// coins de trottoir arrondis entre deux routes à trottoirs
    #[serde(default)]
    pub corners: Vec<JCorner>,
    /// signalisation : 0 automatique, 1 feux, 2 stops, 3 rien (voir signals.rs)
    #[serde(default)]
    pub control: u8,
    /// feux et panneaux posés par DragoWalker pour ce carrefour (identifiants des objets)
    #[serde(default)]
    pub props: Vec<u64>,
}

/// Coin de trottoir arrondi : la route U suit `u` (demi-chaussée `hu`), la route V suit `v` (demi-chaussée `hv`) ;
/// le trottoir remplit le quart de disque de rayon `rc` centré en c + u·(hv+rc) + v·(hu+rc), à la hauteur de la
/// bordure, avec sa bordure en arc côté chaussée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JCorner {
    pub u: [f32; 2],
    pub v: [f32; 2],
    pub hu: f32,
    pub hv: f32,
    pub rc: f32,
    pub curb: f32,
}

/// Branche d'un carrefour : direction (à plat), demi-chaussée, demi-largeur totale (trottoirs compris),
/// longueur couverte depuis le centre, passage piéton.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JArm {
    pub dir: [f32; 2],
    pub hw: f32,
    pub total: f32,
    pub len: f32,
    pub cross: bool,
}

/// Géométrie d'un coin de trottoir entre la route U (axe `u`, demi-chaussée `hu`) et la route V (axe `v`,
/// demi-chaussée `hv`), dans le quart (u, v), quel que soit l'angle entre les routes : le centre du quart de
/// disque est à `rc` des deux bordures (donc au croisement des bords extérieurs des trottoirs quand rc = largeur
/// du trottoir) ; renvoie (centre, normale de U côté v, normale de V côté u, distance le long de U où le trottoir
/// de U reprend, distance le long de V où celui de V reprend).
pub fn corner_geom(u: V3, hu: f32, v: V3, hv: f32, rc: f32) -> Option<(V3, V3, V3, f32, f32)> {
    let nu = (v - u * u.dot(v)).norm();
    let nv = (u - v * u.dot(v)).norm();
    let (db, da) = (v.dot(nu), u.dot(nv));
    if db < 0.25 || da < 0.25 {
        return None; // routes presque parallèles : pas de coin
    }
    let center = u * ((hv + rc) / da) + v * ((hu + rc) / db);
    let tu = (center - nu * rc).dot(u);
    let tv = (center - nv * rc).dot(v);
    Some((center, nu, nv, tu.max(0.0), tv.max(0.0)))
}

/// Réglages de circulation d'une route (écrits dans les points de route .ynd).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoadTraffic {
    /// voitures sur cette route
    pub enabled: bool,
    /// densité 0–15 (YndNode::get_Density 0x06002eee : Flags4 & 15) ; 8 = la plus courante en ville
    pub density: u8,
    /// vitesse 0 lente, 1 normale, 2 rapide, 3 très rapide (YndNode::get_Speed 0x06002ed0 : (LinkCountUnk >> 1) & 3)
    pub speed: u8,
    /// pas de gros véhicules (YndNode::get_NoBigVehicles 0x06002ed4 : Flags0 & 32)
    pub no_big: bool,
}

impl Default for RoadTraffic {
    fn default() -> Self {
        RoadTraffic { enabled: true, density: 8, speed: 1, no_big: false }
    }
}

/// Distance maximale entre deux points cliqués d'une route : longueur d'un lien entre nœuds de route, gardée sur
/// un octet par le jeu, min(255, distance) — YndLink::UpdateLength (0x06002f24).
pub const MAX_LINK: f32 = 255.0;

/// Distance d'accrochage aux extrémités d'une route (m) — **estimé**.
pub const SNAP_END: f32 = 8.0;
/// Longueur sur laquelle la hauteur d'une route se raccorde à celle qu'elle rejoint (m) — **estimé**.
pub const HEIGHT_BLEND: f32 = 25.0;

/// Intersection de deux segments [a,b] et [c,d] (à plat) : paramètres (t, u) dans [0, 1].
fn seg_inter(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> Option<(f32, f32)> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let q = [d[0] - c[0], d[1] - c[1]];
    let den = r[0] * q[1] - r[1] * q[0];
    if den.abs() < 1e-9 {
        return None;
    }
    let ac = [c[0] - a[0], c[1] - a[1]];
    let t = (ac[0] * q[1] - ac[1] * q[0]) / den;
    let u = (ac[0] * r[1] - ac[1] * r[0]) / den;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) { Some((t, u)) } else { None }
}

impl Road {
    pub fn kind(&self) -> &'static RoadKind {
        &KINDS[(self.kind as usize).min(KINDS.len() - 1)]
    }

    /// Demi-largeur totale (chaussée + trottoir ou accotement).
    pub fn half_total(&self) -> f32 {
        let k = self.kind();
        k.width * 0.5 + k.sidewalk.max(k.shoulder)
    }

    pub fn length(&self) -> f32 {
        self.samples.last().map(|s| s[3]).unwrap_or(0.0)
    }

    /// Crée la route à partir des points cliqués : courbe lissée (Catmull-Rom), hauteur du terrain lissée
    /// sur ±15 m puis pente limitée à 12 %.
    pub fn build(kind: u8, pts: &[V3], height: impl Fn(f32, f32) -> Option<f32>, grade: f32, elevated: bool) -> Option<Road> {
        Road::build_ex(kind, pts, height, grade, elevated, false)
    }

    /// Comme build ; `buried` : mode tunnel, la chaussée va en ligne droite (en hauteur) d'un bout à l'autre sous les
    /// reliefs, sauf là où le terrain est plus bas.
    pub fn build_ex(kind: u8, pts: &[V3], height: impl Fn(f32, f32) -> Option<f32>, grade: f32, elevated: bool, buried: bool) -> Option<Road> {
        if pts.len() < 2 {
            return None;
        }
        let p2: Vec<[f32; 2]> = pts.iter().map(|p| [p.x, p.y]).collect();
        let xy = spline(&p2, STEP);
        if xy.len() < 2 {
            return None;
        }
        let mut z: Vec<f32> = Vec::with_capacity(xy.len());
        let mut last = pts[0].z;
        for p in &xy {
            let h = height(p[0], p[1]).unwrap_or(last);
            z.push(h);
            last = h;
        }
        // lissage : moyenne glissante sur ±10 points (≈ ±15 m), trois fois
        for _ in 0..3 {
            let src = z.clone();
            for i in 0..z.len() {
                let a = i.saturating_sub(10);
                let b = (i + 10).min(z.len() - 1);
                z[i] = src[a..=b].iter().sum::<f32>() / (b - a + 1) as f32;
            }
        }
        let mut dist = vec![0.0f32; xy.len()];
        for i in 1..xy.len() {
            dist[i] = dist[i - 1] + ((xy[i][0] - xy[i - 1][0]).powi(2) + (xy[i][1] - xy[i - 1][1]).powi(2)).sqrt();
        }
        // mode pont : la chaussée ne descend pas dans les creux, elle va en ligne droite (en hauteur) d'un bout à
        // l'autre, sauf là où le terrain est plus haut
        let grade = grade.clamp(GRADE_MIN, GRADE_MAX);
        if elevated {
            let (za, zb) = (pts[0].z, pts[pts.len() - 1].z);
            let len = dist[dist.len() - 1].max(1e-3);
            for i in 0..z.len() {
                z[i] = z[i].max(za + (zb - za) * dist[i] / len);
            }
        }
        if buried && !elevated {
            let (za, zb) = (pts[0].z, pts[pts.len() - 1].z);
            let len = dist[dist.len() - 1].max(1e-3);
            for i in 0..z.len() {
                z[i] = z[i].min(za + (zb - za) * dist[i] / len);
            }
        }
        limit_grade(&mut z, &dist, grade);
        let samples = xy.iter().enumerate().map(|(i, p)| [p[0], p[1], z[i], dist[i]]).collect();
        let kd = &KINDS[(kind as usize).min(KINDS.len() - 1)];
        let foot = kd.foot;
        let traffic = RoadTraffic { enabled: !foot, ..RoadTraffic::default() };
        Some(Road {
            kind,
            pts: p2,
            samples,
            gaps: Vec::new(),
            traffic,
            grade,
            elevated: elevated || foot,
            spans: Vec::new(),
            piers: Vec::new(),
            abutments: Vec::new(),
            stairs: None,
            buried: buried && !elevated && !foot,
            tunnels: Vec::new(),
            taper: None,
            ring: false,
            oneway: kd.ramp,
            furniture: Furniture::for_kind(kd),
            props: Vec::new(),
        })
    }

    /// Rond-point : boucle de rayon `radius` (axe de la chaussée) autour de `c`, à la hauteur `z`, parcourue dans
    /// le sens inverse des aiguilles d'une montre (on roule à droite).
    pub fn ring(kind: u8, c: V3, radius: f32, z: f32) -> Road {
        let n = ((std::f32::consts::TAU * radius / STEP).ceil() as usize).max(24);
        let mut samples = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            samples.push([c.x + radius * a.cos(), c.y + radius * a.sin(), z, radius * a]);
        }
        let pts = (0..8).map(|i| {
            let a = i as f32 / 8.0 * std::f32::consts::TAU;
            [c.x + radius * a.cos(), c.y + radius * a.sin()]
        }).collect();
        let kd = &KINDS[(kind as usize).min(KINDS.len() - 1)];
        Road {
            kind,
            pts,
            samples,
            gaps: Vec::new(),
            traffic: RoadTraffic::default(),
            grade: MAX_GRADE,
            elevated: false,
            spans: Vec::new(),
            piers: Vec::new(),
            abutments: Vec::new(),
            stairs: None,
            buried: false,
            tunnels: Vec::new(),
            taper: None,
            ring: true,
            oneway: true,
            furniture: Furniture::for_kind(kd),
            props: Vec::new(),
        }
    }

    /// La même route refaite depuis ses points cliqués et ses réglages (type, pente, pont, tunnel, rond-point), sur
    /// le terrain actuel ; ses bouts posés sur une route déjà refaite (`built`) prennent sa hauteur. Le réseau entier
    /// est refait ainsi quand une route est modifiée ou supprimée (Terrain::rebuild_roads).
    pub fn rebuilt(&self, height: &dyn Fn(f32, f32) -> Option<f32>, built: &[Road]) -> Option<Road> {
        let keep = |mut r: Road| {
            r.traffic = self.traffic.clone();
            r.furniture = self.furniture;
            r.taper = self.taper;
            r.props = self.props.clone();
            r
        };
        if self.ring {
            let n = self.pts.len().max(1) as f32;
            let c = v3(self.pts.iter().map(|p| p[0]).sum::<f32>() / n, self.pts.iter().map(|p| p[1]).sum::<f32>() / n, 0.0);
            let rr = self.pts.first().map(|p| ((p[0] - c.x).powi(2) + (p[1] - c.y).powi(2)).sqrt()).unwrap_or(RING_RADIUS);
            let z = height(c.x, c.y).or(self.samples.first().map(|s| s[2])).unwrap_or(0.0);
            return Some(keep(Road::ring(self.kind, c, rr, z)));
        }
        let n = self.pts.len();
        if n < 2 {
            return None;
        }
        let pts: Vec<V3> = self
            .pts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut z = height(p[0], p[1]).unwrap_or(0.0);
                if i == 0 || i == n - 1 {
                    for o in built {
                        if let Some((d, q, _, _)) = o.nearest_point(p[0], p[1]) {
                            if d <= o.kind().width * 0.5 + 0.5 && (q.z - z).abs() < 4.0 * JOIN_DZ {
                                z = q.z;
                            }
                        }
                    }
                }
                v3(p[0], p[1], z)
            })
            .collect();
        let foot = self.kind().foot;
        let mut r = Road::build_ex(self.kind, &pts, height, self.grade, self.elevated && !foot, self.buried)?;
        if foot {
            r.make_footbridge(height, built);
        } else if self.elevated {
            r.raise_over(built);
        }
        Some(keep(r))
    }

    /// Centre et rayon (axe) d'un rond-point.
    pub fn ring_center(&self) -> (V3, f32) {
        let m = (self.samples.len() - 1).max(1);
        let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
        for s in &self.samples[..m] {
            x += s[0];
            y += s[1];
            z += s[2];
        }
        let c = v3(x / m as f32, y / m as f32, z / m as f32);
        let r = ((self.samples[0][0] - c.x).powi(2) + (self.samples[0][1] - c.y).powi(2)).sqrt();
        (c, r)
    }

    /// Largeurs au point `d` (demi-chaussée, trottoir ou accotement, bordure), raccord compris.
    pub fn dims_at(&self, d: f32) -> (f32, f32, f32) {
        let dims = |k: &RoadKind| (k.width * 0.5, k.sidewalk.max(k.shoulder), k.curb);
        let b = dims(self.kind());
        if let Some((ok, len)) = self.taper {
            if d < len && len > 0.0 && !self.kind().foot {
                let o = dims(&KINDS[(ok as usize).min(KINDS.len() - 1)]);
                let t = (d / len).clamp(0.0, 1.0);
                let t = t * t * (3.0 - 2.0 * t);
                return (o.0 + (b.0 - o.0) * t, o.1 + (b.1 - o.1) * t, o.2 + (b.2 - o.2) * t);
            }
        }
        b
    }

    /// Demi-largeur totale au point `d` (raccord compris).
    pub fn half_at(&self, d: f32) -> f32 {
        let (h, e, _) = self.dims_at(d);
        h + e
    }

    /// Plus grande demi-largeur totale de la route (raccord compris).
    pub fn max_half(&self) -> f32 {
        self.half_total().max(self.half_at(0.0))
    }

    /// Vrai si la distance `d` est dans un tunnel.
    pub fn in_tunnel(&self, d: f32) -> bool {
        self.tunnels.iter().any(|s| d >= s[0] && d <= s[1])
    }

    /// Tronçons en tunnel d'une route tracée en mode tunnel : là où il y a plus de TUNNEL_H + TUNNEL_COVER de terrain
    /// au-dessus de la chaussée ; deux tronçons à moins de 12 m n'en font qu'un, moins de 6 m est ignoré (**estimé**).
    pub fn mark_tunnels(&mut self, ground: impl Fn(f32, f32) -> Option<f32>) {
        let mut out: Vec<[f32; 2]> = Vec::new();
        let mut cur: Option<f32> = None;
        let n = self.samples.len();
        for (i, s) in self.samples.iter().enumerate() {
            let deep = ground(s[0], s[1]).map(|g| g - s[2] > TUNNEL_H + TUNNEL_COVER).unwrap_or(false);
            match (deep, cur) {
                (true, None) => cur = Some(s[3]),
                (false, Some(a)) => {
                    out.push([a, s[3]]);
                    cur = None;
                }
                _ => {}
            }
            if i == n - 1 {
                if let Some(a) = cur {
                    out.push([a, s[3]]);
                }
            }
        }
        let mut merged: Vec<[f32; 2]> = Vec::new();
        for sp in out {
            if let Some(l) = merged.last_mut() {
                if sp[0] - l[1] < 12.0 {
                    l[1] = sp[1];
                    continue;
                }
            }
            merged.push(sp);
        }
        merged.retain(|s| s[1] - s[0] >= 6.0);
        self.tunnels = merged;
    }

    /// Lumières du plafond des tunnels (points, en coordonnées monde) : au milieu du plafond, tous les
    /// TUNNEL_LIGHT_STEP m.
    pub fn tunnel_lights(&self) -> Vec<V3> {
        let mut out = Vec::new();
        for t in &self.tunnels {
            let n = ((t[1] - t[0]) / TUNNEL_LIGHT_STEP).floor().max(1.0) as usize;
            for k in 0..n {
                let d = t[0] + (t[1] - t[0]) * (k as f32 + 0.5) / n as f32;
                let (p, _) = self.at(d);
                out.push(v3(p.x, p.y, p.z + 0.08 + TUNNEL_H - 0.25));
            }
        }
        out
    }

    /// Emplacements des lampadaires : (pied sur l'axe décalé, direction vers la chaussée, lampadaire à deux bras).
    pub fn lamp_spots(&self) -> Vec<(V3, V3, bool)> {
        let k = self.kind();
        let mut out = Vec::new();
        if !self.furniture.lights || k.foot || k.dirt || self.samples.len() < 2 {
            return out;
        }
        let len = self.length();
        let gap = |d: f32, side: f32| self.gaps.iter().any(|g| d >= g[0] - 2.0 && d <= g[1] + 2.0 && (g[2] == 0.0 || g[2] == side));
        let free = |d: f32| !self.in_tunnel(d) && (self.ring || (d > 6.0 && d < len - 6.0));
        if k.width >= 20.0 && !self.ring {
            // autoroute : lampadaires à deux bras sur le terre-plein
            let mut d = LAMP_STEP_HIGHWAY * 0.5;
            while d < len {
                if free(d) && !gap(d, 0.0) {
                    let (p, t) = self.at(d);
                    out.push((p, v3(t.y, -t.x, 0.0), true));
                }
                d += LAMP_STEP_HIGHWAY;
            }
            return out;
        }
        let mut d = LAMP_STEP * 0.5;
        let mut i = 0;
        while d < len {
            let (hw, e, _) = self.dims_at(d);
            // trottoir : près de la bordure, des deux côtés (en quinconce dans une rue, face à face sur une avenue) ;
            // sans trottoir : au bord de l'accotement, côté droit ; rond-point : côté extérieur
            let sides: Vec<f32> = if self.ring {
                vec![1.0]
            } else if k.sidewalk > 0.0 {
                if k.width >= 15.0 { vec![-1.0, 1.0] } else { vec![if i % 2 == 0 { -1.0 } else { 1.0 }] }
            } else {
                vec![1.0]
            };
            for side in sides {
                if !free(d) || gap(d, side) {
                    continue;
                }
                let (p, t) = self.at(d);
                let r = v3(t.y, -t.x, 0.0);
                let off = if k.sidewalk > 0.0 { hw + 0.6 } else { hw + e + 0.5 };
                out.push((p + r * (off * side), r * -side, false));
            }
            i += 1;
            d += if k.sidewalk > 0.0 && k.width < 15.0 { LAMP_STEP * 0.5 } else { LAMP_STEP };
        }
        out
    }

    /// La même route parcourue dans l'autre sens (distances, ouvertures des trottoirs, ponts et escaliers suivent).
    pub fn reversed(&self) -> Road {
        let len = self.length();
        let mut r = self.clone();
        r.pts.reverse();
        r.samples = self.samples.iter().rev().map(|s| [s[0], s[1], s[2], len - s[3]]).collect();
        r.gaps = self.gaps.iter().map(|g| [len - g[1], len - g[0], -g[2]]).collect();
        r.spans = self.spans.iter().rev().map(|s| [len - s[1], len - s[0]]).collect();
        r.piers = self.piers.iter().rev().map(|p| [len - p[0], p[1]]).collect();
        r.abutments = self.abutments.iter().rev().map(|p| [len - p[0], p[1]]).collect();
        r.stairs = self.stairs.map(|s| [s[1], s[0]]);
        r.tunnels = self.tunnels.iter().rev().map(|s| [len - s[1], len - s[0]]).collect();
        r.taper = None;
        r
    }

    /// Début de la route jusqu'à la distance `d` (le point à `d` compris).
    pub fn head_to(&self, d: f32) -> Road {
        let mut r = self.clone();
        let (p, _) = self.at(d);
        r.samples = self.samples.iter().filter(|s| s[3] < d - 1e-3).copied().collect();
        r.samples.push([p.x, p.y, p.z, d]);
        r.gaps = self.gaps.iter().filter(|g| g[0] < d).map(|g| [g[0], g[1].min(d + 1.0), g[2]]).collect();
        r.spans = self.spans.iter().filter(|g| g[0] < d).map(|g| [g[0], g[1].min(d)]).collect();
        r.tunnels = self.tunnels.iter().filter(|g| g[0] < d).map(|g| [g[0], g[1].min(d)]).collect();
        r
    }

    /// Prolonge cette route (déjà construite) par une nouvelle partie qui part de son bout de fin : une seule route,
    /// courbe continue. La courbe est refaite à partir des points cliqués des deux ; seul le dernier tronçon de
    /// l'ancienne route bouge (une courbe de Catmull-Rom ne dépend que des points voisins). Les hauteurs de l'ancienne
    /// partie sont gardées (raccords aux carrefours compris), la nouvelle s'y raccorde sur HEIGHT_BLEND ; les
    /// ouvertures de trottoir sont reportées. Renvoie (route, distance où commence la partie refaite).
    pub fn extended(&self, more: &[V3], height: impl Fn(f32, f32) -> Option<f32>, grade: f32, elevated: bool) -> Option<(Road, f32)> {
        if self.pts.len() < 2 || more.is_empty() {
            return None;
        }
        let mut all: Vec<V3> = self.pts.iter().map(|p| v3(p[0], p[1], self.nearest(p[0], p[1]).map(|n| n.1).unwrap_or(0.0))).collect();
        all.extend_from_slice(more);
        let mut r = Road::build(self.kind, &all, height, grade.max(self.grade), elevated || self.elevated)?;
        // début de la partie refaite : l'avant-dernier point cliqué de l'ancienne route
        let keep = self.pts[self.pts.len() - 2];
        let cut = r.nearest_point(keep[0], keep[1]).map(|n| n.2).unwrap_or(0.0);
        let z_cut_old = self.nearest(keep[0], keep[1]).map(|n| n.1).unwrap_or(0.0);
        let k = r.samples.iter().position(|s| s[3] >= cut).unwrap_or(0);
        let delta = z_cut_old - r.samples[k][2];
        for s in r.samples.iter_mut() {
            if s[3] < cut {
                if let Some((_, z)) = self.nearest(s[0], s[1]) {
                    s[2] = z;
                }
            } else {
                let w = (1.0 - (s[3] - cut) / HEIGHT_BLEND).max(0.0);
                s[2] += delta * w * w * (3.0 - 2.0 * w);
            }
        }
        // ouvertures de trottoir de l'ancienne route, reportées le long de la nouvelle courbe
        let map = |d: f32| {
            let (p, _) = self.at(d);
            r.nearest_point(p.x, p.y).map(|n| n.2).unwrap_or(d)
        };
        r.gaps = self.gaps.iter().map(|g| [map(g[0].max(0.0)), map(g[1].min(self.length())), g[2]]).collect();
        r.traffic = self.traffic.clone();
        // réglages de la route gardés ; ses lampadaires sont repris pour être retirés puis reposés le long de la
        // nouvelle courbe (refresh_furniture), sinon ils resteraient orphelins
        r.furniture = self.furniture;
        r.oneway = self.oneway;
        r.props = self.props.clone();
        Some((r, cut))
    }

    /// Partie de la route à partir de la distance `d0`, distances recomptées depuis 0 (pour la relier aux autres
    /// routes sans refaire les carrefours de la partie d'avant).
    pub fn tail_from(&self, d0: f32) -> Road {
        let mut r = self.clone();
        r.samples = self.samples.iter().filter(|s| s[3] >= d0 - 1e-3).map(|s| [s[0], s[1], s[2], s[3] - d0]).collect();
        r.gaps.clear();
        r.spans.clear();
        r.piers.clear();
        r.abutments.clear();
        r.tunnels.clear();
        r.taper = None;
        r
    }

    /// Point de l'axe à la distance `d` le long de la route (hauteur de la chaussée comprise) et tangente à plat.
    pub fn at(&self, d: f32) -> (V3, V3) {
        let n = self.samples.len();
        let mut k = 0;
        while k + 2 < n && self.samples[k + 1][3] < d {
            k += 1;
        }
        let (a, b) = (self.samples[k], self.samples[(k + 1).min(n - 1)]);
        let f = if (b[3] - a[3]).abs() > 1e-4 { ((d - a[3]) / (b[3] - a[3])).clamp(0.0, 1.0) } else { 0.0 };
        (v3(a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f), v3(b[0] - a[0], b[1] - a[1], 0.0).norm())
    }

    /// Vrai si la distance `d` le long de la route est sur un tronçon en pont.
    pub fn in_span(&self, d: f32) -> bool {
        self.spans.iter().any(|s| d >= s[0] && d <= s[1])
    }

    /// Longueur totale des tronçons en pont.
    pub fn bridge_len(&self) -> f32 {
        self.spans.iter().map(|s| s[1] - s[0]).sum()
    }

    /// Croisements à plat avec les autres routes : (distance le long de cette route, hauteur de l'autre route,
    /// demi-largeur totale de l'autre route).
    fn crossings(&self, others: &[Road]) -> Vec<(f32, f32, f32)> {
        let mut out = Vec::new();
        for o in others {
            for w in self.samples.windows(2) {
                for v in o.samples.windows(2) {
                    if let Some((t, u)) = seg_inter([w[0][0], w[0][1]], [w[1][0], w[1][1]], [v[0][0], v[0][1]], [v[1][0], v[1][1]]) {
                        out.push((w[0][3] + (w[1][3] - w[0][3]) * t, v[0][2] + (v[1][2] - v[0][2]) * u, o.half_total()));
                    }
                }
            }
        }
        out
    }

    /// Mode pont : la chaussée passe au-dessus des routes qu'elle croise (hauteur libre CLEARANCE sous le tablier),
    /// avec des rampes à la pente maximale de part et d'autre. Une route déjà bien plus haute (un autre pont) est
    /// laissée au-dessus.
    pub fn raise_over(&mut self, others: &[Road]) {
        let need_above = CLEARANCE + DECK_DEPTH;
        let n = self.samples.len();
        // hauteur imposée : un plateau au-dessus de la route croisée (sa largeur + 9 m de chaque côté), puis la rampe
        let mut tent = vec![f32::MIN; n];
        for (dc, zo, ohalf) in self.crossings(others) {
            let (p, _) = self.at(dc);
            if zo > p.z + JOIN_DZ {
                continue; // l'autre passe au-dessus
            }
            let need = zo + need_above;
            let plateau = ohalf + 9.0;
            for (i, s) in self.samples.iter().enumerate() {
                tent[i] = tent[i].max(need - self.grade * ((s[3] - dc).abs() - plateau).max(0.0));
            }
        }
        if tent.iter().all(|t| *t == f32::MIN) {
            return;
        }
        // sommet arrondi : moyenne glissante sur ±6 points (± 9 m), sans toucher le dessus du plateau
        let src = tent.clone();
        for i in 0..n {
            let (a, b) = (i.saturating_sub(6), (i + 6).min(n - 1));
            tent[i] = src[a..=b].iter().sum::<f32>() / (b - a + 1) as f32;
        }
        for (s, t) in self.samples.iter_mut().zip(&tent) {
            s[2] = s[2].max(*t);
        }
    }

    /// Passerelle : tablier plat, CLEARANCE sous lui au-dessus des routes croisées, FOOT_CLEAR au-dessus du sol
    /// ailleurs ; escaliers aux deux bouts jusqu'au sol (hauteur du sol aux points cliqués).
    pub fn make_footbridge(&mut self, ground: impl Fn(f32, f32) -> Option<f32>, others: &[Road]) {
        let mut top = f32::MIN;
        for s in &self.samples {
            if let Some(g) = ground(s[0], s[1]) {
                top = top.max(g + FOOT_CLEAR + FOOT_DEPTH);
            }
        }
        for (_, zo, _) in self.crossings(others) {
            top = top.max(zo + CLEARANCE + FOOT_DEPTH);
        }
        if top == f32::MIN {
            top = self.samples[0][2] + FOOT_CLEAR + FOOT_DEPTH;
        }
        for s in self.samples.iter_mut() {
            s[2] = top;
        }
        let (a, b) = (self.samples[0], self.samples[self.samples.len() - 1]);
        let ga = ground(a[0], a[1]).unwrap_or(top - 3.0);
        let gb = ground(b[0], b[1]).unwrap_or(top - 3.0);
        self.stairs = Some([ga, gb]);
        self.spans = vec![[0.0, self.length()]];
    }

    /// Tronçons en pont d'une route tracée en mode pont : là où la chaussée est à plus de BRIDGE_MIN_CLEAR au-dessus
    /// du sol ; deux tronçons séparés de moins de 12 m n'en font qu'un, un tronçon de moins de 6 m est ignoré
    /// (**estimé**).
    pub fn mark_spans(&mut self, ground: impl Fn(f32, f32) -> Option<f32>) {
        if self.kind().foot {
            self.spans = vec![[0.0, self.length()]];
            return;
        }
        let mut spans: Vec<[f32; 2]> = Vec::new();
        let mut cur: Option<f32> = None;
        let n = self.samples.len();
        for (i, s) in self.samples.iter().enumerate() {
            let high = ground(s[0], s[1]).map(|g| s[2] - g > BRIDGE_MIN_CLEAR).unwrap_or(true);
            match (high, cur) {
                (true, None) => cur = Some(s[3]),
                (false, Some(a)) => {
                    spans.push([a, s[3]]);
                    cur = None;
                }
                _ => {}
            }
            if i == n - 1 {
                if let Some(a) = cur {
                    spans.push([a, s[3]]);
                }
            }
        }
        let mut merged: Vec<[f32; 2]> = Vec::new();
        for sp in spans {
            if let Some(l) = merged.last_mut() {
                if sp[0] - l[1] < 12.0 {
                    l[1] = sp[1];
                    continue;
                }
            }
            merged.push(sp);
        }
        merged.retain(|s| s[1] - s[0] >= 6.0);
        self.spans = merged;
    }

    /// Piles tous les ~PIER_STEP m sur chaque tronçon en pont et culées à ses bouts, posées sur le sol. Une pile qui
    /// tomberait sur une route plus basse (`blocked`) est décalée le long du pont jusqu'à 12 m, sinon retirée.
    pub fn place_piers(&mut self, ground: impl Fn(f32, f32) -> Option<f32>, blocked: impl Fn(f32, f32, f32) -> bool) {
        self.piers.clear();
        self.abutments.clear();
        let len = self.length();
        let foot = self.kind().foot;
        for sp in self.spans.clone() {
            let l = sp[1] - sp[0];
            // culées : au bout d'un tronçon qui retrouve le sol (pas au bout d'une passerelle : ses escaliers)
            if !foot {
                for d in [sp[0], sp[1]] {
                    let (p, _) = self.at(d);
                    if let Some(g) = ground(p.x, p.y) {
                        if (d > 0.5 && d < len - 0.5) || p.z - g < BRIDGE_MIN_CLEAR + 2.0 {
                            self.abutments.push([d, g]);
                        }
                    }
                }
            }
            let n = (l / PIER_STEP).round().max(1.0) as usize;
            // passerelle : une pile à chaque bout (sous le haut des escaliers) en plus de celles du milieu
            let ks: Vec<f32> = if foot { (0..=n).map(|k| sp[0] + (l * k as f32 / n as f32).clamp(0.6, l - 0.6)).collect() } else { (1..n).map(|k| sp[0] + l * k as f32 / n as f32).collect() };
            for d0 in ks {
                let mut placed = false;
                for off in [0.0f32, 2.0, -2.0, 4.0, -4.0, 6.0, -6.0, 8.0, -8.0, 10.0, -10.0, 12.0, -12.0] {
                    let d = (d0 + off).clamp(sp[0] + 0.5, sp[1] - 0.5);
                    let (p, _) = self.at(d);
                    let Some(g) = ground(p.x, p.y) else { continue };
                    if p.z - g < 1.0 || blocked(p.x, p.y, p.z) {
                        continue;
                    }
                    self.piers.push([d, g]);
                    placed = true;
                    break;
                }
                let _ = placed;
            }
        }
    }

    /// Point le plus proche de la route : (distance à plat, hauteur de la chaussée à cet endroit).
    pub fn nearest(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let mut best: Option<(f32, f32)> = None;
        for w in self.samples.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l2 = (dx * dx + dy * dy).max(1e-6);
            let t = (((x - a[0]) * dx + (y - a[1]) * dy) / l2).clamp(0.0, 1.0);
            let (px, py) = (a[0] + dx * t, a[1] + dy * t);
            let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
            if best.map(|b| d < b.0).unwrap_or(true) {
                best = Some((d, a[2] + (b[2] - a[2]) * t));
            }
        }
        best
    }

    /// Point de l'axe le plus proche : (distance à plat, point de l'axe, distance le long de la route, tangente).
    pub fn nearest_point(&self, x: f32, y: f32) -> Option<(f32, V3, f32, V3)> {
        let mut best: Option<(f32, V3, f32, V3)> = None;
        for w in self.samples.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l2 = (dx * dx + dy * dy).max(1e-6);
            let t = (((x - a[0]) * dx + (y - a[1]) * dy) / l2).clamp(0.0, 1.0);
            let p = v3(a[0] + dx * t, a[1] + dy * t, a[2] + (b[2] - a[2]) * t);
            let d = ((x - p.x).powi(2) + (y - p.y).powi(2)).sqrt();
            if best.map(|b| d < b.0).unwrap_or(true) {
                best = Some((d, p, a[3] + (b[3] - a[3]) * t, v3(dx, dy, 0.0).norm()));
            }
        }
        best
    }

    /// Demi-largeur totale + 0,5 m (zone à ouvrir chez l'autre route).
    fn ext(&self) -> f32 {
        let k = self.kind();
        k.width * 0.5 + k.sidewalk.max(k.shoulder) + 0.5
    }

    /// Relie cette nouvelle route aux routes existantes : extrémités posées sur une autre route (T ou
    /// prolongement), croisements. La hauteur se raccorde sur 25 m, les trottoirs s'ouvrent à la jonction, et un
    /// carrefour en goudron couvre les marquages. Renvoie les carrefours à ajouter.
    pub fn attach(&mut self, others: &mut [Road]) -> Vec<Junction> {
        let n = self.samples.len();
        if n < 2 {
            return Vec::new();
        }
        let len = self.length();
        let hw = self.kind().width * 0.5;
        let my_ext = self.ext();
        let mut js = Vec::new();
        let mut cons: Vec<(usize, f32)> = Vec::new();
        if self.kind().foot {
            return Vec::new();
        }
        for o in others.iter_mut() {
            if o.kind().foot {
                continue;
            }
            let ohw = o.kind().width * 0.5;
            let o_ext = o.ext();
            let olen = o.length();
            let dirt = self.kind().dirt && o.kind().dirt;
            // extrémités de la nouvelle route posées sur l'autre (un rond-point n'en a pas)
            let ends: &[usize] = if self.ring { &[] } else { &[0, n - 1] };
            for &idx in ends {
                let s = self.samples[idx];
                let Some((d, p, along, tan)) = o.nearest_point(s[0], s[1]) else { continue };
                if d > ohw + 0.5 || (s[2] - p.z).abs() > JOIN_DZ {
                    continue;
                }
                cons.push((idx, p.z));
                // bretelle posée sur une autoroute : elle s'y fond, sans carrefour ni trottoir ouvert
                if self.kind().ramp && o.kind().width >= 20.0 {
                    continue;
                }
                if !o.ring && (along < 1.0 || along > olen - 1.0) {
                    // prolongement bout à bout : rien à ouvrir
                    continue;
                }
                // côté de l'autre route par lequel arrive la nouvelle
                let inner = self.samples[if idx == 0 { 5.min(n - 1) } else { n.saturating_sub(6) }];
                let dir = v3(inner[0] - s[0], inner[1] - s[1], 0.0);
                let right = v3(tan.y, -tan.x, 0.0);
                let side = if dir.dot(right) > 0.0 { 1.0 } else { -1.0 };
                // deux routes à trottoirs : chaque trottoir s'arrête exactement là où commence le coin arrondi
                // (rayon = la plus petite largeur de trottoir) ; sinon ouverture large comme avant
                let rc = self.kind().sidewalk.min(o.kind().sidewalk);
                let din0 = dir.norm();
                let geoms: Vec<(f32, Option<(V3, V3, V3, f32, f32)>)> = [1.0f32, -1.0].iter().map(|sg| (*sg, if rc > 0.0 { corner_geom(tan * *sg, ohw, din0, hw, rc) } else { None })).collect();
                if geoms.iter().all(|g| g.1.is_some()) {
                    // le sens de la nouvelle route à ce bout (pour savoir quel trottoir est de quel côté)
                    let dn = if idx == 0 { din0 } else { din0 * -1.0 };
                    let rn = v3(dn.y, -dn.x, 0.0);
                    for (sg, g) in &geoms {
                        let (_, _, _, tu, tv) = g.unwrap();
                        o.gaps.push(if *sg > 0.0 { [along, along + tu, side] } else { [along - tu, along, side] });
                        let side_n = if (tan * *sg).dot(rn) > 0.0 { 1.0 } else { -1.0 };
                        self.gaps.push(if idx == 0 { [0.0, tv, side_n] } else { [len - tv, len + 1.0, side_n] });
                    }
                } else {
                    o.gaps.push([along - hw - 1.0, along + hw + 1.0, side]);
                    if idx == 0 {
                        self.gaps.push([0.0, o_ext, 0.0]);
                    } else {
                        self.gaps.push([len - o_ext, len + 1.0, 0.0]);
                    }
                }
                // branches : la nouvelle route (vers l'intérieur de la nouvelle route) et l'autre dans ses deux sens
                let din = dir.norm();
                let my_total = self.half_total();
                let o_total = o.half_total();
                // pas de passage piéton en travers de l'anneau d'un rond-point (seulement sur les routes qui y entrent)
                let my_cross = self.kind().sidewalk > 0.0 && !self.ring;
                let o_cross = o.kind().sidewalk > 0.0 && !o.ring;
                let arms = vec![
                    JArm { dir: [din.x, din.y], hw, total: my_total, len: o_ext, cross: my_cross },
                    JArm { dir: [tan.x, tan.y], hw: ohw, total: o_total, len: my_ext, cross: o_cross },
                    JArm { dir: [-tan.x, -tan.y], hw: ohw, total: o_total, len: my_ext, cross: o_cross },
                ];
                let mut corners = Vec::new();
                if geoms.iter().all(|g| g.1.is_some()) {
                    let curb = self.kind().curb.min(o.kind().curb);
                    for sgn in [1.0f32, -1.0] {
                        corners.push(JCorner { u: [tan.x * sgn, tan.y * sgn], v: [din.x, din.y], hu: ohw, hv: hw, rc, curb });
                    }
                }
                // rond-point : priorité à l'anneau, ni feux ni stops
                let control = if o.ring || self.ring { 3 } else { 0 };
                js.push(Junction { pos: [p.x, p.y, p.z], r: hw.max(ohw) * 1.15 + 0.5, dirt, arms, corners, control, props: Vec::new() });
            }
            // croisements au milieu
            for j in 0..n - 1 {
                for k in 0..o.samples.len().saturating_sub(1) {
                    let (a, b) = (self.samples[j], self.samples[j + 1]);
                    let (c, dd) = (o.samples[k], o.samples[k + 1]);
                    let Some((t, u)) = seg_inter([a[0], a[1]], [b[0], b[1]], [c[0], c[1]], [dd[0], dd[1]]) else { continue };
                    let dj = a[3] + (b[3] - a[3]) * t;
                    if dj < ohw + 1.0 || dj > len - ohw - 1.0 {
                        continue; // extrémité : déjà traitée
                    }
                    let dk = c[3] + (dd[3] - c[3]) * u;
                    let z = c[2] + (dd[2] - c[2]) * u;
                    if (a[2] + (b[2] - a[2]) * t - z).abs() > JOIN_DZ {
                        continue; // l'une passe au-dessus de l'autre (pont)
                    }
                    // croisement pile sur un point de la courbe : vu par les deux segments voisins, un seul carrefour
                    let (px, py) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
                    if js.iter().any(|q: &Junction| (q.pos[0] - px).powi(2) + (q.pos[1] - py).powi(2) < 1.0) {
                        continue;
                    }
                    cons.push((if t < 0.5 { j } else { j + 1 }, z));
                    let rc = self.kind().sidewalk.min(o.kind().sidewalk);
                    let ds0 = v3(b[0] - a[0], b[1] - a[1], 0.0).norm();
                    let dk0 = v3(dd[0] - c[0], dd[1] - c[1], 0.0).norm();
                    let quads = [(1.0f32, 1.0f32), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)];
                    let cg: Vec<Option<(V3, V3, V3, f32, f32)>> = quads.iter().map(|(su, sv)| if rc > 0.0 { corner_geom(ds0 * *su, hw, dk0 * *sv, ohw, rc) } else { None }).collect();
                    let corners_ok = cg.iter().all(|g| g.is_some());
                    if corners_ok {
                        let (rs, ro) = (v3(ds0.y, -ds0.x, 0.0), v3(dk0.y, -dk0.x, 0.0));
                        for ((su, sv), g) in quads.iter().zip(&cg) {
                            let (_, _, _, tu, tv) = g.unwrap();
                            let side_s = if (dk0 * *sv).dot(rs) > 0.0 { 1.0 } else { -1.0 };
                            let side_o = if (ds0 * *su).dot(ro) > 0.0 { 1.0 } else { -1.0 };
                            self.gaps.push(if *su > 0.0 { [dj, dj + tu, side_s] } else { [dj - tu, dj, side_s] });
                            o.gaps.push(if *sv > 0.0 { [dk, dk + tv, side_o] } else { [dk - tv, dk, side_o] });
                        }
                    } else {
                        self.gaps.push([dj - o_ext, dj + o_ext, 0.0]);
                        o.gaps.push([dk - my_ext, dk + my_ext, 0.0]);
                    }
                    let ds = v3(b[0] - a[0], b[1] - a[1], 0.0).norm();
                    let dk2 = v3(dd[0] - c[0], dd[1] - c[1], 0.0).norm();
                    let (my_total, o_total) = (self.half_total(), o.half_total());
                    let (mc, oc) = (self.kind().sidewalk > 0.0 && !self.ring, o.kind().sidewalk > 0.0 && !o.ring);
                    let arms = vec![
                        JArm { dir: [ds.x, ds.y], hw, total: my_total, len: o_ext, cross: mc },
                        JArm { dir: [-ds.x, -ds.y], hw, total: my_total, len: o_ext, cross: mc },
                        JArm { dir: [dk2.x, dk2.y], hw: ohw, total: o_total, len: my_ext, cross: oc },
                        JArm { dir: [-dk2.x, -dk2.y], hw: ohw, total: o_total, len: my_ext, cross: oc },
                    ];
                    let mut corners = Vec::new();
                    if corners_ok {
                        let curb = self.kind().curb.min(o.kind().curb);
                        for (su, sv) in [(1.0f32, 1.0f32), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                            corners.push(JCorner { u: [ds.x * su, ds.y * su], v: [dk2.x * sv, dk2.y * sv], hu: hw, hv: ohw, rc, curb });
                        }
                    }
                    let control = if o.ring || self.ring { 3 } else { 0 };
                    js.push(Junction { pos: [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, z], r: hw.max(ohw) * 1.15 + 0.5, dirt, arms, corners, control, props: Vec::new() });
                }
            }
        }
        // hauteur : raccord en douceur vers chaque point imposé
        for (i0, zc) in cons {
            let d0 = self.samples[i0][3];
            let delta = zc - self.samples[i0][2];
            for s in self.samples.iter_mut() {
                let w = (1.0 - (s[3] - d0).abs() / HEIGHT_BLEND).max(0.0);
                s[2] += delta * w * w * (3.0 - 2.0 * w);
            }
            self.samples[i0][2] = zc;
        }
        js
    }

    /// Modèle de la route (sommets relatifs à `origin` en x, y ; z absolu) : parties (texture, indices).
    /// Chaussée un peu au-dessus du terrain aplani (+0,08 m), trottoirs à la hauteur de la bordure, et une
    /// jupe qui descend sous le terrain sur les bords pour qu'aucun trou ne se voie.
    pub fn mesh(&self, origin: V3) -> (Vec<V3>, Vec<[f32; 2]>, Vec<(u64, Vec<u32>, u8)>) {
        let k = self.kind();
        let hw = k.width * 0.5;
        let n = self.samples.len();
        let mut pos: Vec<V3> = Vec::new();
        let mut uv: Vec<[f32; 2]> = Vec::new();
        let mut parts: Vec<(u64, Vec<u32>, u8)> = Vec::new();
        if n < 2 {
            return (pos, uv, parts);
        }
        // repère de chaque point : droite = tangente tournée de -90° (rond-point : boucle, voisins de part et d'autre
        // de la jonction)
        let m = n - 1;
        let frames: Vec<(V3, V3, f32)> = (0..n)
            .map(|i| {
                let (a, b) = if self.ring && m >= 2 {
                    let i = i % m;
                    (self.samples[(i + m - 1) % m], self.samples[(i + 1) % m])
                } else {
                    (self.samples[i.saturating_sub(1)], self.samples[(i + 1).min(n - 1)])
                };
                let t = v3(b[0] - a[0], b[1] - a[1], 0.0).norm();
                let s = self.samples[i];
                (v3(s[0] - origin.x, s[1] - origin.y, s[2]), v3(t.y, -t.x, 0.0), s[3])
            })
            .collect();
        // une bande : colonnes (décalage, hauteur, u) de gauche à droite, v = distance / longueur
        let gaps = self.gaps.clone();
        let samples_d: Vec<f32> = self.samples.iter().map(|s| s[3]).collect();
        // side : None = chaussée (jamais ouverte), Some(-1 / 1) = bord gauche / droit
        let spans = self.spans.clone();
        let tunnels = self.tunnels.clone();
        let in_span = |mid: f32| spans.iter().any(|s| mid >= s[0] && mid <= s[1]);
        let in_tun = |mid: f32| tunnels.iter().any(|s| mid >= s[0] && mid <= s[1]);
        let lift = 0.08 + if k.ramp { 0.025 } else { 0.0 };
        // raccord avec un autre type de route : chaque colonne suit la largeur du moment (chaussée et bord étirés,
        // bordure qui monte ou descend)
        let dims: Vec<(f32, f32, f32)> = frames.iter().map(|f| self.dims_at(f.2)).collect();
        let (bhw, bedge) = (hw, k.sidewalk.max(k.shoulder));
        let tapered = self.taper.is_some() && !k.foot;
        let map_off = |o: f32, i: usize| -> f32 {
            if !tapered {
                return o;
            }
            let (h, e, _) = dims[i];
            let a = o.abs();
            if a <= bhw + 1e-4 {
                o * h / bhw.max(1e-3)
            } else {
                o.signum() * (h + (a - bhw) * if bedge > 1e-3 { e / bedge } else { 1.0 })
            }
        };
        let map_dz = |dz: f32, i: usize| -> f32 {
            if !tapered || k.curb <= 0.0 || dz <= lift + 1e-4 || dz > lift + k.curb + 1e-4 {
                return dz;
            }
            lift + (dz - lift) * dims[i].2 / k.curb
        };
        // filt : segments dessinés (milieu du segment) ; cap : section fermée là où ils commencent et finissent
        let mut strip_ex = |cols: &[(f32, f32, f32)], vlen: f32, key: u64, coll: u8, side: Option<f32>, filt: &dyn Fn(f32) -> bool, cap: bool| {
            let base = pos.len() as u32;
            let lines: Vec<Vec<V3>> = cols.iter().map(|c| offset_line_var(&frames, &(0..n).map(|i| map_off(c.0, i)).collect::<Vec<f32>>())).collect();
            for (i, (_, _, d)) in frames.iter().enumerate() {
                for (c, (_, dz, u)) in cols.iter().enumerate() {
                    pos.push(lines[c][i] + v3(0.0, 0.0, map_dz(*dz, i)));
                    uv.push([*u, *d / vlen]);
                }
            }
            let w = cols.len() as u32;
            let mut idx = Vec::new();
            let open = |s: u32| -> bool {
                let Some(sd) = side else { return false };
                let mid = (samples_d[s as usize] + samples_d[s as usize + 1]) * 0.5;
                gaps.iter().any(|g| mid >= g[0] && mid <= g[1] && (g[2] == 0.0 || g[2] == sd))
            };
            let seg_on = |s: u32| filt((samples_d[s as usize] + samples_d[s as usize + 1]) * 0.5);
            for s in 0..(n as u32 - 1) {
                if !seg_on(s) {
                    continue;
                }
                if cap {
                    // bouts du tronçon : section fermée
                    for (edge, at) in [(s == 0 || !seg_on(s - 1), s), (s + 1 == n as u32 - 1 || !seg_on(s + 1), s + 1)] {
                        if edge {
                            let b = base + at * w;
                            for c in 1..w - 1 {
                                idx.extend_from_slice(&[b, b + c, b + c + 1, b, b + c + 1, b + c]);
                            }
                        }
                    }
                }
                if side.is_none() {
                    for c in 0..w - 1 {
                        let a = base + s * w + c;
                        idx.extend_from_slice(&[a, a + 1, a + w + 1, a, a + w + 1, a + w]);
                    }
                    continue;
                }
                // bout de trottoir (là où il s'ouvre ou reprend) : on ferme la section, pour ne pas voir dessous
                let here = open(s);
                if s + 1 < n as u32 - 1 && here != open(s + 1) {
                    let b = base + (s + 1) * w;
                    for c in 1..w - 1 {
                        idx.extend_from_slice(&[b, b + c, b + c + 1, b, b + c + 1, b + c]);
                    }
                }
                if here {
                    continue;
                }
                for c in 0..w - 1 {
                    let a = base + s * w + c;
                    let b = a + 1;
                    let cc = a + w + 1;
                    let d = a + w;
                    idx.extend_from_slice(&[a, b, cc, a, cc, d]);
                }
            }
            parts.push((key, idx, coll));
        };
        let mut strip = |cols: &[(f32, f32, f32)], vlen: f32, key: u64, coll: u8, side: Option<f32>| strip_ex(cols, vlen, key, coll, side, &|_| true, false);
        // chaussée : par défaut, comme avant, la texture DragoWalker du type de route (goudron usé et marquages
        // peints, 12 m le long) ; avec « Vraies textures du jeu », goudron du jeu uni (tous les 6 m) et marquages
        // en bandes à part, 6 mm au-dessus
        let game_tex = crate::groundtex::enabled();
        if k.foot {
            strip(&[(-hw, lift, -hw / 2.0), (hw, lift, hw / 2.0)], 2.0, SIDEWALK_KEY, 0, None);
        } else if game_tex || self.ring {
            let surf_key = if k.dirt { DIRT_PLAIN_KEY } else { ASPHALT_KEY };
            strip(&[(-hw, lift, -hw / 6.0), (0.0, lift, 0.0), (hw, lift, hw / 6.0)], 6.0, surf_key, 0, None);
        } else {
            strip(&[(-hw, lift, 0.0), (0.0, lift, 0.5), (hw, lift, 1.0)], ROAD_TEX_LEN, road_tex_key(self.kind as usize), 0, None);
        }
        if k.sidewalk > 0.0 {
            let sw = k.sidewalk;
            let c = lift + k.curb;
            // gauche : jupe, bord extérieur, bord de la bordure, pied de la bordure
            strip(&[(-hw - sw, -0.6, (-hw - sw) / 2.0), (-hw - sw, c, (-hw - sw) / 2.0 + 0.3), (-hw, c, -hw / 2.0 + 0.3), (-hw, lift, -hw / 2.0 + 0.39)], 2.0, SIDEWALK_KEY, 1, Some(-1.0));
            strip(&[(hw, lift, hw / 2.0 - 0.09), (hw, c, hw / 2.0), (hw + sw, c, (hw + sw) / 2.0), (hw + sw, -0.6, (hw + sw) / 2.0 + 0.3)], 2.0, SIDEWALK_KEY, 1, Some(1.0));
        } else if !k.foot {
            let sh = k.shoulder;
            let key = crate::terrain::material_key(k.shoulder_mat as usize);
            strip(&[(-hw - sh, -0.5, (-hw - sh) / 6.0), (-hw - sh, lift - 0.04, (-hw - sh) / 6.0 + 0.1), (-hw, lift, -hw / 6.0 + 0.1)], 6.0, key, 2, Some(-1.0));
            strip(&[(hw, lift, hw / 6.0), (hw + sh, lift - 0.04, (hw + sh) / 6.0), (hw + sh, -0.5, (hw + sh) / 6.0 + 0.1)], 6.0, key, 2, Some(1.0));
        }
        if !spans.is_empty() {
            let concrete = crate::terrain::material_key(4);
            // 2 cm plus large que la route : les faces du tablier et des garde-corps ne se superposent pas au bord
            // du trottoir ou de l'accotement (sinon elles se battent à l'écran)
            let wt = self.half_total() + 0.02;
            let depth = if k.foot { FOOT_DEPTH } else { DECK_DEPTH };
            // tablier : poutre en caisson sous la chaussée, côtés droits et dessous (vers l'extérieur)
            strip_ex(&[(wt, lift, 0.0), (wt, -depth, (depth + lift) / 6.0), (-wt, -depth, (depth + lift + 2.0 * wt) / 6.0), (-wt, lift, (2.0 * depth + 2.0 * lift + 2.0 * wt) / 6.0)], 6.0, concrete, BRIDGE_COLL, None, &|m| in_span(m), true);
            // garde-corps en béton sur les deux bords, posés sur le trottoir ou l'accotement
            let base = if k.sidewalk > 0.0 { lift + k.curb } else if k.foot { lift } else { lift - 0.04 };
            let ph = if k.foot { FOOT_PARAPET_H } else { PARAPET_H };
            let top = base + ph;
            strip_ex(&[(-wt, base - 0.3, 0.0), (-wt, top, (ph + 0.3) / 6.0), (-wt + PARAPET_W, top, (ph + 0.6) / 6.0), (-wt + PARAPET_W, base, (2.0 * ph + 0.6) / 6.0)], 6.0, concrete, BRIDGE_COLL, None, &|m| in_span(m), true);
            strip_ex(&[(wt - PARAPET_W, base, 0.0), (wt - PARAPET_W, top, ph / 6.0), (wt, top, (ph + 0.3) / 6.0), (wt, base - 0.3, (2.0 * ph + 0.6) / 6.0)], 6.0, concrete, BRIDGE_COLL, None, &|m| in_span(m), true);
        }
        // tunnels : tube (murs et plafond tournés vers l'intérieur) et bandes lumineuses au plafond
        if !tunnels.is_empty() {
            let concrete = crate::terrain::material_key(4);
            let wt = self.half_total() + 0.02;
            let th = lift + TUNNEL_H;
            strip_ex(&[(wt, -0.6, 0.0), (wt, th, (th + 0.6) / 6.0), (-wt, th, (th + 0.6 + 2.0 * wt) / 6.0), (-wt, -0.6, (2.0 * th + 1.2 + 2.0 * wt) / 6.0)], 6.0, concrete, BRIDGE_COLL, None, &|m| in_tun(m), false);
            strip_ex(&[(0.3, th - 0.05, 0.0), (-0.3, th - 0.05, 1.0)], 4.0, LIGHTSTRIP_KEY, NO_COLLISION, None, &|m| in_tun(m) && m.rem_euclid(TUNNEL_LIGHT_STEP) < 4.0, false);
        }
        // glissières : lisse en métal le long des deux bords (routes sans trottoir), pas sur les ponts ni dans les
        // tunnels, ouvertes aux carrefours
        if self.furniture.rails && k.sidewalk <= 0.0 && !k.foot {
            let x = hw + k.shoulder - 0.25;
            let (lo, hi) = (lift - 0.04 + RAIL_LOW, lift - 0.04 + RAIL_HIGH);
            let keep = |m: f32| !in_span(m) && !in_tun(m);
            strip_ex(&[(-x - 0.06, lo, 0.0), (-x - 0.06, hi, 0.1), (-x, hi, 0.2), (-x, lo, 0.3)], 4.0, METAL_KEY, RAIL_COLL, Some(-1.0), &keep, false);
            strip_ex(&[(x, lo, 0.0), (x, hi, 0.1), (x + 0.06, hi, 0.2), (x + 0.06, lo, 0.3)], 4.0, METAL_KEY, RAIL_COLL, Some(1.0), &keep, false);
        }
        for l in k.lines.iter().filter(|l| (game_tex || self.ring) && !(self.ring && l.yellow)) {
            let base = pos.len() as u32;
            let (a, b) = (l.off - l.w * 0.5, l.off + l.w * 0.5);
            let (la, lb) = (offset_line_var(&frames, &vec![a; n]), offset_line_var(&frames, &vec![b; n]));
            for (i, (_, _, d)) in frames.iter().enumerate() {
                for (q, u) in [(la[i], 0.0f32), (lb[i], 1.0)] {
                    pos.push(q + v3(0.0, 0.0, lift + LINE_LIFT));
                    uv.push([u, *d / 4.0]);
                }
            }
            let mut idx = Vec::new();
            for s in 0..(n as u32 - 1) {
                // tirets de 3 m tous les 12 m
                let mid = (samples_d[s as usize] + samples_d[s as usize + 1]) * 0.5;
                if l.dashed && mid.rem_euclid(ROAD_TEX_LEN) >= 3.0 {
                    continue;
                }
                // pas de marquage dans un carrefour (là où les trottoirs s'ouvrent) ni dans un raccord de largeur
                if gaps.iter().any(|g| mid >= g[0] && mid <= g[1]) || (tapered && mid < self.taper.map(|t| t.1).unwrap_or(0.0)) {
                    continue;
                }
                let a0 = base + s * 2;
                idx.extend_from_slice(&[a0, a0 + 1, a0 + 3, a0, a0 + 3, a0 + 2]);
            }
            parts.push((if l.yellow { LINE_YELLOW_KEY } else { LINE_WHITE_KEY }, idx, NO_COLLISION));
        }
        if !spans.is_empty() {
            let concrete = crate::terrain::material_key(4);
            let wt = self.half_total();
            let depth = if k.foot { FOOT_DEPTH } else { DECK_DEPTH };
            let mut bidx: Vec<u32> = Vec::new();
            let mut sidx: Vec<u32> = Vec::new();
            let o2 = v3(origin.x, origin.y, 0.0);
            // piles : un voile de béton sous le tablier, du sol (enterré de 1 m) au dessous du tablier
            for pr in &self.piers {
                let (p, t) = self.at(pr[0]);
                let r = v3(t.y, -t.x, 0.0);
                let half_across = if k.foot { 0.45 } else { (wt * 0.7).max(1.0) };
                let half_along = if k.foot { 0.45 } else { 0.8 };
                add_box(&mut pos, &mut uv, &mut bidx, p - o2, t, r, half_along, half_across, pr[1] - 1.0, p.z - depth + 0.05);
            }
            // culées : mur sur toute la largeur, du sol (enterré de 1,5 m) jusque sous la chaussée
            for ab in &self.abutments {
                let (p, t) = self.at(ab[0]);
                let r = v3(t.y, -t.x, 0.0);
                add_box(&mut pos, &mut uv, &mut bidx, p - o2, t, r, 0.75, wt + 0.2, ab[1] - 1.5, p.z + lift - 0.02);
            }
            // escaliers des passerelles, vers l'extérieur aux deux bouts
            if let (Some(st), true) = (self.stairs, k.foot) {
                let len = self.length();
                for (d, out_sign, g) in [(0.0f32, -1.0f32, st[0]), (len, 1.0, st[1])] {
                    let (p, t) = self.at(d);
                    let out = t * out_sign;
                    let r = v3(out.y, -out.x, 0.0);
                    let top = p.z + lift;
                    let rise = top - g;
                    if rise < 0.3 {
                        continue;
                    }
                    let steps = (rise / STEP_RISE).ceil() as usize;
                    let h = rise / steps as f32;
                    let p0 = p - o2;
                    let at = |x: f32, s: f32, z: f32| v3(p0.x + out.x * x + r.x * s, p0.y + out.y * x + r.y * s, z);
                    for i in 0..steps {
                        let (x0, x1) = (i as f32 * STEP_RUN, (i + 1) as f32 * STEP_RUN);
                        let (z_hi, z_lo) = (top - i as f32 * h, top - (i + 1) as f32 * h);
                        // contremarche (face vers l'extérieur) puis marche : collision « escalier » du jeu
                        push_quad(&mut pos, &mut uv, &mut sidx, [at(x0, -hw, z_hi), at(x0, hw, z_hi), at(x0, hw, z_lo), at(x0, -hw, z_lo)], out);
                        push_quad(&mut pos, &mut uv, &mut sidx, [at(x0, -hw, z_lo), at(x0, hw, z_lo), at(x1, hw, z_lo), at(x1, -hw, z_lo)], v3(0.0, 0.0, 1.0));
                    }
                    // limons : un seul pan de chaque côté, du sol au garde-corps qui suit la pente (des dalles empilées,
                    // une par marche, se chevauchaient et scintillaient), et le dessus du garde-corps
                    let lx = steps as f32 * STEP_RUN;
                    let (rail_top, rail_end) = (top + FOOT_PARAPET_H, g + h + FOOT_PARAPET_H);
                    for s in [-1.0f32, 1.0] {
                        let e = s * (hw + PARAPET_W);
                        let ei = s * hw;
                        for (ss, want) in [(e, r * s), (ei, r * -s)] {
                            push_quad(&mut pos, &mut uv, &mut bidx, [at(0.0, ss, g - 0.5), at(lx, ss, g - 0.5), at(lx, ss, rail_end), at(0.0, ss, rail_top)], want);
                        }
                        push_quad(&mut pos, &mut uv, &mut bidx, [at(0.0, ei, rail_top), at(lx, ei, rail_end), at(lx, e, rail_end), at(0.0, e, rail_top)], v3(0.0, 0.0, 1.0));
                    }
                    // bout du limon
                    for s in [-1.0f32, 1.0] {
                        let (e, ei) = (s * (hw + PARAPET_W), s * hw);
                        push_quad(&mut pos, &mut uv, &mut bidx, [at(lx, e, g - 0.5), at(lx, ei, g - 0.5), at(lx, ei, g + h + FOOT_PARAPET_H), at(lx, e, g + h + FOOT_PARAPET_H)], out);
                    }
                }
            }
            if !bidx.is_empty() {
                parts.push((concrete, bidx, BRIDGE_COLL));
            }
            if !sidx.is_empty() {
                parts.push((concrete, sidx, STAIRS_COLL));
            }
        }
        let o2 = v3(origin.x, origin.y, 0.0);
        let gap_at = |d: f32, side: f32| self.gaps.iter().any(|g| d >= g[0] && d <= g[1] && (g[2] == 0.0 || g[2] == side));
        // poteaux des glissières
        if self.furniture.rails && k.sidewalk <= 0.0 && !k.foot {
            let mut pidx = Vec::new();
            let x = hw + k.shoulder - 0.25 + 0.14;
            let len = self.length();
            let mut d = 1.0;
            while d < len - 0.5 {
                if !self.in_span(d) && !self.in_tunnel(d) {
                    let (p, t) = self.at(d);
                    let r = v3(t.y, -t.x, 0.0);
                    for side in [-1.0f32, 1.0] {
                        if !gap_at(d, side) {
                            add_box(&mut pos, &mut uv, &mut pidx, p - o2 + r * (x * side), t, r, 0.06, 0.06, p.z - 0.6, p.z + lift - 0.04 + RAIL_HIGH - 0.05);
                        }
                    }
                }
                d += RAIL_POST_STEP;
            }
            if !pidx.is_empty() {
                parts.push((METAL_KEY, pidx, RAIL_COLL));
            }
        }
        // façades des entrées de tunnel : un mur percé du tube, de part et d'autre et au-dessus, et une dalle de toit
        // sur les premiers mètres (cache le terrain ouvert à l'entrée)
        if !self.tunnels.is_empty() {
            let concrete = crate::terrain::material_key(4);
            let mut tidx = Vec::new();
            let wt = self.half_total() + 0.02;
            for t in &self.tunnels {
                for (d, sign) in [(t[0], -1.0f32), (t[1], 1.0)] {
                    let (p, tg) = self.at(d);
                    let out = tg * sign;
                    let r = v3(out.y, -out.x, 0.0);
                    let c = p - o2;
                    let (bot, oh, top) = (p.z - 1.0, p.z + lift + TUNNEL_H, p.z + lift + TUNNEL_H + PORTAL_TOP);
                    let w = wt + PORTAL_SIDE;
                    let at = |a: f32, l: f32, z: f32| v3(c.x + out.x * a + r.x * l, c.y + out.y * a + r.y * l, z);
                    for (l0, l1, z0, z1) in [(-w, -wt, bot, top), (wt, w, bot, top), (-wt, wt, oh, top)] {
                        push_quad(&mut pos, &mut uv, &mut tidx, [at(0.0, l0, z0), at(0.0, l1, z0), at(0.0, l1, z1), at(0.0, l0, z1)], out);
                        push_quad(&mut pos, &mut uv, &mut tidx, [at(-1.0, l0, z0), at(-1.0, l1, z0), at(-1.0, l1, z1), at(-1.0, l0, z1)], out * -1.0);
                    }
                    // dessus de la façade et dalle vers l'intérieur
                    push_quad(&mut pos, &mut uv, &mut tidx, [at(0.0, -w, top), at(0.0, w, top), at(-1.0, w, top), at(-1.0, -w, top)], v3(0.0, 0.0, 1.0));
                    push_quad(&mut pos, &mut uv, &mut tidx, [at(-1.0, -wt - 1.0, oh + 0.6), at(-1.0, wt + 1.0, oh + 0.6), at(-9.0, wt + 1.0, oh + 0.6), at(-9.0, -wt - 1.0, oh + 0.6)], v3(0.0, 0.0, 1.0));
                }
            }
            parts.push((concrete, tidx, BRIDGE_COLL));
        }
        // îlot central d'un rond-point : herbe à la hauteur du trottoir (ou 15 cm), bordure en béton tout autour
        if self.ring {
            let (c, rr) = self.ring_center();
            let ri = (rr - self.half_total()).max(1.0);
            let zi = c.z + lift + if k.curb > 0.0 { k.curb } else { 0.15 };
            let cc = c - o2;
            let seg = 64u32;
            let (mut gidx, mut kidx) = (Vec::new(), Vec::new());
            let base = pos.len() as u32;
            pos.push(v3(cc.x, cc.y, zi));
            uv.push([c.x / 6.0, c.y / 6.0]);
            for i in 0..seg {
                let a = i as f32 / seg as f32 * std::f32::consts::TAU;
                let q = v3(cc.x + ri * a.cos(), cc.y + ri * a.sin(), zi);
                pos.push(q);
                uv.push([(q.x + origin.x) / 6.0, (q.y + origin.y) / 6.0]);
            }
            for i in 0..seg {
                gidx.extend_from_slice(&[base, base + 1 + i, base + 1 + (i + 1) % seg]);
            }
            if k.curb <= 0.0 {
                for i in 0..seg {
                    let (a0, a1) = (i as f32 / seg as f32 * std::f32::consts::TAU, (i + 1) as f32 / seg as f32 * std::f32::consts::TAU);
                    let p0 = v3(cc.x + ri * a0.cos(), cc.y + ri * a0.sin(), 0.0);
                    let p1 = v3(cc.x + ri * a1.cos(), cc.y + ri * a1.sin(), 0.0);
                    let outw = v3((a0 + a1).cos() * 0.5 + (a0 + a1).cos() * 0.5, (a0 + a1).sin(), 0.0);
                    let mid = v3(((a0 + a1) * 0.5).cos(), ((a0 + a1) * 0.5).sin(), 0.0);
                    let _ = outw;
                    push_quad(&mut pos, &mut uv, &mut kidx, [v3(p0.x, p0.y, c.z + lift - 0.05), v3(p1.x, p1.y, c.z + lift - 0.05), v3(p1.x, p1.y, zi), v3(p0.x, p0.y, zi)], mid);
                }
                parts.push((crate::terrain::material_key(4), kidx, 1));
            }
            parts.push((crate::terrain::material_key(2), gidx, GRASS_COLL));
        }
        // décalques au sol : fissures, taches d'huile, rustines, posés au hasard (toujours les mêmes pour une route)
        if self.furniture.decals && !k.dirt && !k.foot {
            let len = self.length();
            let mut seed = (self.samples[0][0].to_bits() ^ self.samples[0][1].to_bits().rotate_left(13)) | 1;
            let mut rnd = || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed % 10_000) as f32 / 10_000.0
            };
            let taper_end = self.taper.map(|t| t.1).unwrap_or(0.0);
            let mut lists: [Vec<u32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
            let mut d = 5.0 + rnd() * 8.0;
            while d < len - 3.0 {
                let kind = (rnd() * 3.0) as usize % 3;
                let lat = (rnd() * 2.0 - 1.0) * (hw - 1.0).max(0.3);
                let (a_len, b_len, ang) = match kind {
                    0 => (1.2 + rnd() * 1.0, 0.35 + rnd() * 0.3, (rnd() - 0.5) * 1.0),
                    1 => (0.6 + rnd() * 0.8, 0.5 + rnd() * 0.6, rnd() * 6.28),
                    _ => (0.8 + rnd() * 1.0, 0.6 + rnd() * 0.7, (rnd() - 0.5) * 0.15),
                };
                if d > taper_end && !gap_at(d, 0.0) && !gap_at(d, -1.0) && !gap_at(d, 1.0) && !self.in_tunnel(d) {
                    let (p, t) = self.at(d);
                    let r = v3(t.y, -t.x, 0.0);
                    let u = t * ang.cos() + r * ang.sin();
                    let v = v3(u.y, -u.x, 0.0);
                    let c = p - o2 + r * lat;
                    let corner = |su: f32, sv: f32| {
                        let q = c + u * (a_len * su) + v * (b_len * sv);
                        let along = d + (u * (a_len * su) + v * (b_len * sv)).dot(t);
                        v3(q.x, q.y, self.at(along.clamp(0.0, len)).0.z + lift + 0.004)
                    };
                    let b = pos.len() as u32;
                    pos.extend_from_slice(&[corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)]);
                    uv.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
                    // tournées vers le haut : (u, v) direct ou non selon le sens de v
                    if u.cross(v).z > 0.0 {
                        lists[kind].extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
                    } else {
                        lists[kind].extend_from_slice(&[b, b + 2, b + 1, b, b + 3, b + 2]);
                    }
                }
                d += 7.0 + rnd() * 14.0;
            }
            for (i, l) in lists.into_iter().enumerate() {
                if !l.is_empty() {
                    parts.push((DECAL_KEYS[i], l, NO_COLLISION));
                }
            }
        }
        (pos, uv, parts)
    }
}

/// Quadrilatère (4 coins dans l'ordre du tour) tourné vers `want` : l'ordre est inversé s'il regarde de l'autre
/// côté. Coordonnées de texture en mètres / 4.
fn push_quad(pos: &mut Vec<V3>, uv: &mut Vec<[f32; 2]>, idx: &mut Vec<u32>, q: [V3; 4], want: V3) {
    let n = (q[1] - q[0]).cross(q[2] - q[0]);
    let q = if n.dot(want) < 0.0 { [q[3], q[2], q[1], q[0]] } else { q };
    // texture projetée sur le plan de la face, en coordonnées du monde (4 m par répétition) : continue d'un morceau à
    // l'autre (chaque morceau repartait de zéro : raccords visibles sur les escaliers et les piles des passerelles)
    let nn = n.norm();
    let b = pos.len() as u32;
    pos.extend_from_slice(&q);
    for p in q {
        uv.push(if nn.z.abs() > 0.7 {
            [p.x / 4.0, p.y / 4.0]
        } else {
            let h = v3(-nn.y, nn.x, 0.0).norm();
            [(p.x * h.x + p.y * h.y) / 4.0, -p.z / 4.0]
        });
    }
    idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
}

/// Boîte posée au point `c` (x, y), orientée par `along` et `right`, de z0 à z1 : quatre côtés et le dessus.
#[allow(clippy::too_many_arguments)]
fn add_box(pos: &mut Vec<V3>, uv: &mut Vec<[f32; 2]>, idx: &mut Vec<u32>, c: V3, along: V3, right: V3, ha: f32, hr: f32, z0: f32, z1: f32) {
    let at = |a: f32, r: f32, z: f32| v3(c.x + along.x * a + right.x * r, c.y + along.y * a + right.y * r, z);
    for (a, r, want) in [(1.0f32, 0.0f32, along), (-1.0, 0.0, along * -1.0), (0.0, 1.0, right), (0.0, -1.0, right * -1.0)] {
        let q = if a != 0.0 {
            [at(a * ha, -hr, z0), at(a * ha, hr, z0), at(a * ha, hr, z1), at(a * ha, -hr, z1)]
        } else {
            [at(-ha, r * hr, z0), at(ha, r * hr, z0), at(ha, r * hr, z1), at(-ha, r * hr, z1)]
        };
        push_quad(pos, uv, idx, q, want);
    }
    push_quad(pos, uv, idx, [at(-ha, -hr, z1), at(ha, -hr, z1), at(ha, hr, z1), at(-ha, hr, z1)], v3(0.0, 0.0, 1.0));
}

/// Pente limitée dans les deux sens : deux points voisins ne diffèrent pas de plus de `grade` × leur écart.
fn limit_grade(z: &mut [f32], dist: &[f32], grade: f32) {
    for i in 1..z.len() {
        let m = (dist[i] - dist[i - 1]) * grade;
        z[i] = z[i].clamp(z[i - 1] - m, z[i - 1] + m);
    }
    for i in (0..z.len().saturating_sub(1)).rev() {
        let m = (dist[i + 1] - dist[i]) * grade;
        z[i] = z[i].clamp(z[i + 1] - m, z[i + 1] + m);
    }
}

/// Début du passage piéton d'une branche (distance au centre du carrefour) : juste après la chaussée de chaque
/// route croisée, sur toute la largeur de la chaussée de la branche. Une route qui arrive en biais repousse le
/// passage (sinon il tombe en travers sur l'autre chaussée) ; à angle droit, c'est le bord de l'autre chaussée,
/// dans l'alignement de son trottoir.
pub fn crosswalk_start(j: &Junction, a: &JArm) -> f32 {
    let mut s: Option<f32> = None;
    for b in &j.arms {
        let cos = a.dir[0] * b.dir[0] + a.dir[1] * b.dir[1];
        let sin = (a.dir[0] * b.dir[1] - a.dir[1] * b.dir[0]).abs();
        if sin < 0.25 {
            continue; // même route (ou presque parallèle)
        }
        let need = (b.hw + a.hw * cos.abs()) / sin + 0.1;
        s = Some(s.map_or(need, |v: f32| v.max(need)));
    }
    s.unwrap_or(a.len - 3.4)
}

/// Bord décalé de `off` (à droite si positif) le long des repères (point, droite, distance) d'une route.
/// Dans un virage plus serré que le décalage, le bord intérieur repartirait en arrière et le trottoir se
/// replierait sur lui-même : tant qu'un point recule par rapport au dernier gardé (dans le sens de la route),
/// il est ramené sur ce point (bord en éventail, sans repli).
fn offset_line_var(frames: &[(V3, V3, f32)], offs: &[f32]) -> Vec<V3> {
    let mut out: Vec<V3> = Vec::with_capacity(frames.len());
    for (i, (p, r, _)) in frames.iter().enumerate() {
        let off = offs[i];
        let q = *p + *r * off;
        if off != 0.0 {
            if let Some(last) = out.last().copied() {
                let t = v3(-r.y, r.x, 0.0);
                let dxy = v3(q.x - last.x, q.y - last.y, 0.0);
                if dxy.dot(t) < 0.01 {
                    out.push(v3(last.x, last.y, q.z));
                    continue;
                }
            }
        }
        out.push(q);
    }
    out
}

/// Disque du carrefour (sommets relatifs à `origin`), à +9,5 cm : au-dessus de la chaussée (+8 cm) pour cacher
/// les marquages, sous les trottoirs ; texture de goudron (ou de terre) sans marquage, répétée tous les 6 m.
pub fn junction_mesh(j: &Junction, origin: V3) -> (Vec<V3>, Vec<[f32; 2]>, Vec<(u64, Vec<u32>, u8)>) {
    if !j.arms.is_empty() {
        return junction_arms_mesh(j, origin, true);
    }
    let c = v3(j.pos[0] - origin.x, j.pos[1] - origin.y, j.pos[2] + 0.095);
    let mut pos = vec![c];
    let mut uv = vec![[j.pos[0] / 6.0, j.pos[1] / 6.0]];
    let seg = 32u32;
    for i in 0..seg {
        let a = i as f32 / seg as f32 * std::f32::consts::TAU;
        let (x, y) = (j.r * a.cos(), j.r * a.sin());
        pos.push(c + v3(x, y, 0.0));
        uv.push([(j.pos[0] + x) / 6.0, (j.pos[1] + y) / 6.0]);
    }
    let mut idx = Vec::new();
    for i in 0..seg {
        idx.extend_from_slice(&[0, 1 + i, 1 + (i + 1) % seg]);
    }
    (pos, uv, vec![(if j.dirt { DIRT_PLAIN_KEY } else { ASPHALT_KEY }, idx, 0)])
}

/// Carrefour en branches : pour chaque route qui en part, un rectangle de goudron uni de toute la largeur de la
/// route (trottoirs compris) jusqu'au bord de l'autre route — il remplit l'espace laissé par les trottoirs
/// ouverts — et, sur les routes à trottoirs, un passage piéton (bandes blanches de 50 cm, 3 m de large) juste
/// avant le bord du carrefour. Goudron à +9,5 cm, passage à +10 cm.
pub fn junction_arms_mesh(j: &Junction, origin: V3, crosswalks: bool) -> (Vec<V3>, Vec<[f32; 2]>, Vec<(u64, Vec<u32>, u8)>) {
    let c = v3(j.pos[0] - origin.x, j.pos[1] - origin.y, j.pos[2]);
    let (mut pos, mut uv) = (Vec::new(), Vec::new());
    let (mut plain, mut zebra) = (Vec::new(), Vec::new());
    let quad = |pos: &mut Vec<V3>, uv: &mut Vec<[f32; 2]>, idx: &mut Vec<u32>, q: [V3; 4], u: [[f32; 2]; 4]| {
        let b = pos.len() as u32;
        pos.extend_from_slice(&q);
        uv.extend_from_slice(&u);
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    };
    for a in &j.arms {
        let d = v3(a.dir[0], a.dir[1], 0.0);
        let r = v3(d.y, -d.x, 0.0);
        let z = v3(0.0, 0.0, 0.095);
        let corners = [c + z - r * a.total, c + z + r * a.total, c + z + r * a.total + d * a.len, c + z - r * a.total + d * a.len];
        let w = |p: V3| [(p.x + origin.x) / 6.0, (p.y + origin.y) / 6.0];
        let u = [w(corners[0]), w(corners[1]), w(corners[2]), w(corners[3])];
        quad(&mut pos, &mut uv, &mut plain, corners, u);
        if crosswalks && a.cross && a.len > 3.6 {
            let z = v3(0.0, 0.0, 0.1);
            let d0 = crosswalk_start(j, a);
            let d1 = d0 + 3.0;
            let q = [c + z - r * a.hw + d * d0, c + z + r * a.hw + d * d0, c + z + r * a.hw + d * d1, c + z - r * a.hw + d * d1];
            let u = [[0.0, 0.0], [a.hw * 2.0, 0.0], [a.hw * 2.0, 1.0], [0.0, 1.0]];
            quad(&mut pos, &mut uv, &mut zebra, q, u);
        }
    }
    // coins de trottoir arrondis : dessus en part de disque à la hauteur de la bordure, bordure en arc côté
    // chaussée ; chaque face une seule fois, dans le bon sens (dessus vers le haut, bordure vers la chaussée)
    let mut side = Vec::new();
    for k in &j.corners {
        let u = v3(k.u[0], k.u[1], 0.0);
        let v = v3(k.v[0], k.v[1], 0.0);
        let Some((cen, nu, nv, _, _)) = corner_geom(u, k.hu, v, k.hv, k.rc) else { continue };
        let top = 0.08 + k.curb;
        let ctr = c + cen;
        let seg = 10;
        let w2 = |p: V3| [(p.x + origin.x) / 2.0, (p.y + origin.y) / 2.0];
        let mut arc = Vec::new();
        for i in 0..=seg {
            let f = i as f32 / seg as f32;
            let d = (nu * -(1.0 - f) + nv * -f).norm();
            arc.push(ctr + d * k.rc);
        }
        let tri = |pos: &mut Vec<V3>, uv: &mut Vec<[f32; 2]>, idx: &mut Vec<u32>, p: [V3; 3], want: V3| {
            let n = (p[1] - p[0]).cross(p[2] - p[0]);
            let p = if n.dot(want) < 0.0 { [p[0], p[2], p[1]] } else { p };
            let b0 = pos.len() as u32;
            for q in p {
                pos.push(q);
                uv.push([w2(q)[0], w2(q)[1] + if q.z < c.z + 0.1 { 0.09 } else { 0.0 }]);
            }
            idx.extend_from_slice(&[b0, b0 + 1, b0 + 2]);
        };
        let up = v3(0.0, 0.0, 1.0);
        let lift_top = v3(0.0, 0.0, top);
        let lift_bot = v3(0.0, 0.0, 0.08);
        // goudron entre le centre du carrefour et l'arrondi (angle obtus des routes en biais)
        let lift_road = v3(0.0, 0.0, 0.095);
        let start = pos.len();
        for w in arc.windows(2) {
            tri(&mut pos, &mut uv, &mut plain, [c + lift_road, w[0] + lift_road, w[1] + lift_road], up);
        }
        // même échelle que le reste du goudron (6 m au lieu de 2 m)
        for t in uv[start..].iter_mut() {
            *t = [t[0] / 3.0, t[1] / 3.0];
        }
        for w in arc.windows(2) {
            tri(&mut pos, &mut uv, &mut side, [ctr + lift_top, w[0] + lift_top, w[1] + lift_top], up);
            let out = ((w[0] + w[1]) * 0.5 - ctr).norm();
            tri(&mut pos, &mut uv, &mut side, [w[0] + lift_top, w[1] + lift_top, w[1] + lift_bot], out);
            tri(&mut pos, &mut uv, &mut side, [w[0] + lift_top, w[1] + lift_bot, w[0] + lift_bot], out);
        }
    }
    let mut parts = vec![(if j.dirt { DIRT_PLAIN_KEY } else { ASPHALT_KEY }, plain, 0u8)];
    if !zebra.is_empty() {
        parts.push((CROSSWALK_KEY, zebra, 0));
    }
    if !side.is_empty() {
        parts.push((SIDEWALK_KEY, side, 1));
    }
    (pos, uv, parts)
}

/// Passage piéton : bandes blanches de 50 cm espacées de 50 cm (la texture couvre 1 m en travers, 3 m le long).
pub const CROSSWALK_KEY: u64 = 0xD7E2_0000_0000_0113;

fn make_crosswalk() -> TexData {
    let n = 256u32;
    let mut d = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let g = hash(x, y, 7) - 0.5;
            let c = if x < n / 2 {
                let f = 0.9 + g * 0.15;
                [232.0 * f, 232.0 * f, 226.0 * f]
            } else {
                let s = 1.0 + g * 0.22;
                [64.0 * s, 65.0 * s, 68.0 * s]
            };
            d.extend_from_slice(&[c[0].clamp(0.0, 255.0) as u8, c[1].clamp(0.0, 255.0) as u8, c[2].clamp(0.0, 255.0) as u8, 255]);
        }
    }
    with_mips(n, n, d)
}

pub const ASPHALT_KEY: u64 = 0xD7E2_0000_0000_0111;
pub const DIRT_PLAIN_KEY: u64 = 0xD7E2_0000_0000_0112;
/// Marquages au sol (bandes à part sur la chaussée) : blanc et jaune.
pub const LINE_WHITE_KEY: u64 = 0xD7E2_0000_0000_0114;
pub const LINE_YELLOW_KEY: u64 = 0xD7E2_0000_0000_0115;
/// Glissières (acier galvanisé), bandes lumineuses des tunnels, décalques (fissure, tache, rustine).
pub const METAL_KEY: u64 = 0xD7E2_0000_0000_0116;
pub const LIGHTSTRIP_KEY: u64 = 0xD7E2_0000_0000_0117;
pub const DECAL_KEYS: [u64; 3] = [0xD7E2_0000_0000_0118, 0xD7E2_0000_0000_0119, 0xD7E2_0000_0000_011A];

/// Texture découpée (transparence par seuil : « cutout ») : les décalques.
pub fn is_cutout(key: u64) -> bool {
    DECAL_KEYS.contains(&key)
}

/// Acier galvanisé, 64 × 64.
fn make_metal() -> TexData {
    let n = 64u32;
    let mut d = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let f = 0.92 + (hash(x, y, 41) - 0.5) * 0.12 + if (y / 8) % 2 == 0 { 0.04 } else { -0.04 };
            d.extend_from_slice(&[(168.0 * f) as u8, (172.0 * f) as u8, (176.0 * f) as u8, 255]);
        }
    }
    with_mips(n, n, d)
}

/// Lumière de tunnel : blanc chaud très clair, 32 × 32.
fn make_lightstrip() -> TexData {
    let n = 32u32;
    let mut d = Vec::with_capacity((n * n * 4) as usize);
    for _ in 0..n * n {
        d.extend_from_slice(&[255, 244, 214, 255]);
    }
    with_mips(n, n, d)
}

/// Décalques (fond transparent) : 0 fissure ramifiée, 1 tache d'huile, 2 rustine de goudron plus sombre.
fn make_decal(kind: usize) -> TexData {
    let n = 128u32;
    let mut d = vec![0u8; (n * n * 4) as usize];
    let mut put = |x: i32, y: i32, c: [u8; 3]| {
        if x >= 0 && y >= 0 && (x as u32) < n && (y as u32) < n {
            let i = ((y as u32 * n + x as u32) * 4) as usize;
            d[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    };
    match kind {
        0 => {
            // fissure : marche au hasard d'un bord à l'autre, avec deux branches
            let crack = [30u8, 31, 33];
            let mut walk = |mut x: f32, mut y: f32, dx: f32, steps: u32, seed: u32| {
                for s in 0..steps {
                    y += (hash(s, seed, 77) - 0.5) * 2.4;
                    x += dx;
                    put(x as i32, y as i32, crack);
                    put(x as i32, y as i32 + 1, crack);
                }
                (x, y)
            };
            walk(2.0, 64.0, 1.0, 124, 1);
            walk(40.0, 64.0, 0.8, 40, 2);
            walk(84.0, 62.0, 0.9, 36, 3);
        }
        1 => {
            // tache : disque bruité, plus sombre au centre
            for y in 0..n {
                for x in 0..n {
                    let (fx, fy) = (x as f32 / n as f32 - 0.5, y as f32 / n as f32 - 0.5);
                    let r = (fx * fx + fy * fy).sqrt() * 2.0 + (hash(x / 6, y / 6, 5) - 0.5) * 0.35;
                    if r < 0.85 {
                        let s = 0.55 + r * 0.35 + (hash(x, y, 9) - 0.5) * 0.1;
                        put(x as i32, y as i32, [(52.0 * s) as u8, (50.0 * s) as u8, (52.0 * s) as u8]);
                    }
                }
            }
        }
        _ => {
            // rustine : rectangle de goudron neuf, bords un peu irréguliers
            for y in 0..n {
                for x in 0..n {
                    let e = (hash(x / 4, y / 4, 13) - 0.5) * 6.0;
                    let inside = (x as f32) > 6.0 + e && (x as f32) < 122.0 + e && (y as f32) > 10.0 + e && (y as f32) < 118.0 + e;
                    if inside {
                        let s = 0.78 + (hash(x, y, 21) - 0.5) * 0.18;
                        put(x as i32, y as i32, [(58.0 * s) as u8, (59.0 * s) as u8, (62.0 * s) as u8]);
                    }
                }
            }
        }
    }
    with_mips(n, n, d)
}
/// Hauteur des marquages au-dessus de la chaussée (m) — **estimé**.
pub const LINE_LIFT: f32 = 0.006;
/// Partie sans collision (marquages : la chaussée dessous a déjà la sienne).
pub const NO_COLLISION: u8 = 255;

/// Peinture de marquage : couleur unie, légèrement usée (64 × 64).
fn make_line(yellow: bool) -> TexData {
    let n = 64u32;
    let mut d = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let f = 0.88 + (hash(x, y, 13) - 0.5) * 0.2;
            let c = if yellow { [226.0 * f, 184.0 * f, 46.0 * f] } else { [232.0 * f, 232.0 * f, 226.0 * f] };
            d.extend_from_slice(&[c[0].clamp(0.0, 255.0) as u8, c[1].clamp(0.0, 255.0) as u8, c[2].clamp(0.0, 255.0) as u8, 255]);
        }
    }
    with_mips(n, n, d)
}

/// Goudron ou terre sans marquage (carrefours), 256 × 256 pour 6 m.
fn make_plain(dirt: bool) -> TexData {
    let n = 256u32;
    let mut d = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let g = hash(x, y, 7) - 0.5;
            let blot = hash(x / 16, y / 16, 91) - 0.5;
            let c = if dirt {
                let s = 1.0 + g * 0.25 + blot * 0.2;
                [128.0 * s, 98.0 * s, 68.0 * s]
            } else {
                let s = 1.0 + g * 0.22 + blot * 0.12 + if hash(x, y, 33) > 0.97 { 0.25 } else { 0.0 };
                [64.0 * s, 65.0 * s, 68.0 * s]
            };
            d.extend_from_slice(&[c[0].clamp(0.0, 255.0) as u8, c[1].clamp(0.0, 255.0) as u8, c[2].clamp(0.0, 255.0) as u8, 255]);
        }
    }
    with_mips(n, n, d)
}

/// Courbe passant par tous les points (Catmull-Rom), un point tous les `step` mètres environ.
pub fn spline(p: &[[f32; 2]], step: f32) -> Vec<[f32; 2]> {
    let n = p.len();
    let mut out = Vec::new();
    if n < 2 {
        return out;
    }
    let at = |i: isize| p[i.clamp(0, n as isize - 1) as usize];
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (at(i as isize - 1), at(i as isize), at(i as isize + 1), at(i as isize + 2));
        let len = ((p2[0] - p1[0]).powi(2) + (p2[1] - p1[1]).powi(2)).sqrt();
        let m = ((len / step).ceil() as usize).max(1);
        for s in 0..m {
            let t = s as f32 / m as f32;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3);
            out.push([f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])]);
        }
    }
    out.push(p[n - 1]);
    out
}

fn hash(x: u32, y: u32, s: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8DA6_B343) ^ y.wrapping_mul(0xD816_3841) ^ s.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Niveaux de détail : moyenne de 2 × 2 pixels jusqu'à 1 pixel de côté.
pub fn with_mips(w: u32, h: u32, d: Vec<u8>) -> TexData {
    let mut levels = vec![d];
    let (mut cw, mut ch) = (w as usize, h as usize);
    while cw > 1 || ch > 1 {
        let prev = levels.last().unwrap();
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let mut o = Vec::with_capacity(nw * nh * 4);
        for y in 0..nh {
            for x in 0..nw {
                for k in 0..4 {
                    let p = |xx: usize, yy: usize| prev[(yy.min(ch - 1) * cw + xx.min(cw - 1)) * 4 + k] as u32;
                    o.push(((p(2 * x, 2 * y) + p(2 * x + 1, 2 * y) + p(2 * x, 2 * y + 1) + p(2 * x + 1, 2 * y + 1)) / 4) as u8);
                }
            }
        }
        levels.push(o);
        cw = nw;
        ch = nh;
    }
    TexData { w, h, fmt: TexFmt::Rgba8, levels }
}

/// Texture de chaussée : u en travers (toute la largeur), v le long (12 m), marquages du type de route.
fn make_road_texture(kind: usize) -> TexData {
    let k = &KINDS[kind];
    let (w, h) = (256u32, 512u32);
    let mut d = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let off = (x as f32 + 0.5) / w as f32 * k.width - k.width * 0.5;
            let along = (y as f32 + 0.5) / h as f32 * ROAD_TEX_LEN;
            let g = hash(x, y, kind as u32 + 7) - 0.5;
            let blot = hash(x / 16, y / 16, 91) - 0.5;
            let mut c = if k.dirt {
                // terre, deux traces de pneus plus sombres et tassées
                let track = ((off.abs() - 1.0).abs() < 0.35) as i32 as f32;
                let base = [128.0, 98.0, 68.0];
                let s = 1.0 + g * 0.25 + blot * 0.2 - track * 0.18;
                [base[0] * s, base[1] * s, base[2] * s]
            } else {
                // goudron gris foncé, gravillons, usure plus sombre au milieu des voies
                let wear = ((off.abs() % 3.7) - 1.85).abs() < 0.5;
                let s = 1.0 + g * 0.22 + blot * 0.12 - if wear { 0.06 } else { 0.0 } + if hash(x, y, 33) > 0.97 { 0.25 } else { 0.0 };
                [64.0 * s, 65.0 * s, 68.0 * s]
            };
            for l in k.lines {
                let on_dash = !l.dashed || along < 3.0;
                if (off - l.off).abs() <= l.w * 0.5 && on_dash {
                    let fade = 0.88 + g * 0.2;
                    c = if l.yellow { [226.0 * fade, 184.0 * fade, 46.0 * fade] } else { [232.0 * fade, 232.0 * fade, 226.0 * fade] };
                }
            }
            d.extend_from_slice(&[c[0].clamp(0.0, 255.0) as u8, c[1].clamp(0.0, 255.0) as u8, c[2].clamp(0.0, 255.0) as u8, 255]);
        }
    }
    with_mips(w, h, d)
}

/// Trottoir : dalles de béton de 1 m (la texture couvre 2 m), joints plus sombres.
fn make_sidewalk_texture() -> TexData {
    let n = 256u32;
    let mut d = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let g = hash(x, y, 501) - 0.5;
            let slab = hash(x / 128, y / 128, 77) - 0.5;
            let joint = x % 128 < 3 || y % 128 < 3;
            let s = 1.0 + g * 0.12 + slab * 0.08 - if joint { 0.25 } else { 0.0 };
            let c = [176.0 * s, 174.0 * s, 168.0 * s];
            d.extend_from_slice(&[c[0].clamp(0.0, 255.0) as u8, c[1].clamp(0.0, 255.0) as u8, c[2].clamp(0.0, 255.0) as u8, 255]);
        }
    }
    with_mips(n, n, d)
}

/// Aperçu d'un type de route vu de dessus (24 m de long) : chaussée et marquages, trottoirs avec bordure ou
/// accotements, un peu d'herbe de chaque côté — à partir des mêmes textures que le modèle.
pub fn preview_image(kind: usize) -> eframe::egui::ColorImage {
    let k = &KINDS[kind.min(KINDS.len() - 1)];
    let texs = road_textures();
    let side = &texs.iter().find(|t| t.0 == SIDEWALK_KEY).unwrap().1;
    // passerelle : dalles de béton comme un trottoir
    let road = if k.foot { side } else { &texs.iter().find(|t| t.0 == road_tex_key(kind)).unwrap().1 };
    let shoulder = crate::terrain::material_preview_texture(k.shoulder_mat as usize);
    let grass = crate::terrain::material_preview_texture(2);
    let hw = k.width * 0.5;
    let edge = k.sidewalk.max(k.shoulder);
    let total = (hw + edge) * 2.0 + 3.0;
    let (w, h) = (240usize, 84usize);
    let sample = |t: &TexData, u: f32, v: f32| -> [u8; 4] {
        let (tw, th) = (t.w as usize, t.h as usize);
        let x = ((u.rem_euclid(1.0)) * tw as f32) as usize % tw;
        let y = ((v.rem_euclid(1.0)) * th as f32) as usize % th;
        let i = (y * tw + x) * 4;
        let l = &t.levels[0];
        [l[i], l[i + 1], l[i + 2], 255]
    };
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let along = x as f32 / w as f32 * 24.0;
            let off = (y as f32 + 0.5) / h as f32 * total - total * 0.5;
            let a = off.abs();
            let c = if a <= hw {
                sample(road, (off + hw) / k.width, along / ROAD_TEX_LEN)
            } else if a <= hw + edge && k.sidewalk > 0.0 {
                if a < hw + 0.18 { [205, 203, 196, 255] } else { sample(side, off / 2.0, along / 2.0) }
            } else if a <= hw + edge {
                sample(&shoulder, off / 6.0, along / 6.0)
            } else {
                sample(&grass, off / 6.0, along / 6.0)
            };
            px.push(eframe::egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]));
        }
    }
    eframe::egui::ColorImage { size: [w, h], pixels: px }
}

/// Toutes les textures des routes (gardées en mémoire).
pub fn road_textures() -> Vec<(u64, TexData)> {
    road_textures_cached().clone()
}

/// Clés des textures des routes (sans copier les images).
pub fn road_texture_keys() -> Vec<u64> {
    road_textures_cached().iter().map(|t| t.0).collect()
}

fn road_textures_cached() -> &'static Vec<(u64, TexData)> {
    static CACHE: std::sync::OnceLock<Vec<(u64, TexData)>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let mut v: Vec<(u64, TexData)> = (0..KINDS.len()).map(|k| (road_tex_key(k), make_road_texture(k))).collect();
            v.push((SIDEWALK_KEY, make_sidewalk_texture()));
            v.push((ASPHALT_KEY, make_plain(false)));
            v.push((DIRT_PLAIN_KEY, make_plain(true)));
            v.push((CROSSWALK_KEY, make_crosswalk()));
            v.push((LINE_WHITE_KEY, make_line(false)));
            v.push((LINE_YELLOW_KEY, make_line(true)));
            v.push((METAL_KEY, make_metal()));
            v.push((LIGHTSTRIP_KEY, make_lightstrip()));
            for (i, k) in DECAL_KEYS.iter().enumerate() {
                v.push((*k, make_decal(i)));
            }
            v
        })
}

/// Sommets d'un .ybn : un triangle désigne ses sommets sur 15 bits (le 16e est un drapeau), donc au plus
/// 32 767 sommets par pièce ; on garde de la marge.
const MAX_COL_VERTS: usize = 32000;
const MAX_COL_TRIS: usize = 20000;

/// Prépare un maillage pour la physique : sommets fusionnés (au millimètre ; un modèle d'affichage en a
/// beaucoup en double, à cause des coutures de textures), triangles plats ou invalides retirés (leur
/// normale serait NaN pour le moteur physique du jeu).
fn clean_collision_mesh(verts: &[V3], tris: &[(u32, u32, u32, u8)]) -> (Vec<V3>, Vec<(u32, u32, u32, u8)>) {
    let mut key_of: std::collections::HashMap<(i64, i64, i64), u32> = std::collections::HashMap::new();
    let mut out_v: Vec<V3> = Vec::new();
    let mut weld: Vec<Option<u32>> = vec![None; verts.len()];
    let mut get = |i: u32, out_v: &mut Vec<V3>| -> Option<u32> {
        let p = *verts.get(i as usize)?;
        if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
            return None;
        }
        if let Some(w) = weld[i as usize] {
            return Some(w);
        }
        let k = ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64, (p.z * 1000.0).round() as i64);
        let w = *key_of.entry(k).or_insert_with(|| {
            out_v.push(p);
            out_v.len() as u32 - 1
        });
        weld[i as usize] = Some(w);
        Some(w)
    };
    let mut out_t = Vec::with_capacity(tris.len());
    for t in tris {
        let (Some(a), Some(b), Some(c)) = (get(t.0, &mut out_v), get(t.1, &mut out_v), get(t.2, &mut out_v)) else { continue };
        if a == b || b == c || a == c {
            continue;
        }
        let n = (out_v[b as usize] - out_v[a as usize]).cross(out_v[c as usize] - out_v[a as usize]);
        if n.len() < 1e-6 {
            continue;
        }
        out_t.push((a, b, c, t.3));
    }
    (out_v, out_t)
}

/// Découpe les triangles en pièces d'au plus MAX_COL_TRIS triangles et MAX_COL_VERTS sommets distincts.
fn collision_chunks(tris: &[(u32, u32, u32, u8)]) -> Vec<&[(u32, u32, u32, u8)]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for (i, t) in tris.iter().enumerate() {
        let new = [t.0, t.1, t.2].iter().filter(|v| !seen.contains(v)).count();
        if i > start && (i - start >= MAX_COL_TRIS || seen.len() + new > MAX_COL_VERTS) {
            out.push(&tris[start..i]);
            start = i;
            seen.clear();
        }
        seen.extend([t.0, t.1, t.2]);
    }
    if start < tris.len() {
        out.push(&tris[start..]);
    }
    out
}

/// Collisions d'un modèle quelconque (triangles dans le monde) au format XML de CodeWalker : une pièce
/// GeometryBVH par paquet d'au plus 20 000 triangles et 32 000 sommets, sommets relatifs au centre de la pièce (GetVertexPos
/// 0x0600410f), marge 0,04, une liste de types de surface `types` (index `m` de chaque triangle).
/// Type de surface marqué « escalier » (bit à ajouter au type) : à l'écriture, le matériau reçoit FLAG_STAIRS
/// (drapeau des matériaux de collision du jeu, nom relevé dans CodeWalker.Core.dll) ; le personnage y monte et
/// descend comme sur un escalier du jeu.
pub const MAT_STAIRS: u32 = 1 << 31;

pub fn mesh_collision_xml(verts: &[V3], tris: &[(u32, u32, u32, u8)], types: &[u32]) -> String {
    let mats: Vec<(u32, u32)> = types.iter().map(|t| (*t, 0)).collect();
    mesh_collision_xml_rooms(verts, tris, &mats)
}

/// Même chose, chaque matériau avec sa pièce d'intérieur : (type de surface, RoomID). Dans une collision
/// d'intérieur, le RoomID de la surface sous le joueur dit au jeu dans quelle pièce il se trouve (0 = dehors).
pub fn mesh_collision_xml_rooms(verts: &[V3], tris: &[(u32, u32, u32, u8)], mats: &[(u32, u32)]) -> String {
    use std::fmt::Write;
    let xyz = |tag: &str, v: V3| format!("<{tag} x=\"{}\" y=\"{}\" z=\"{}\" />", v.x, v.y, v.z);
    let head = |s: &mut String, ind: &str, a: V3, b: V3, margin: f32| {
        let c = (a + b) * 0.5;
        let _ = writeln!(s, "{ind}{}\n{ind}{}\n{ind}{}\n{ind}{}\n{ind}<SphereRadius value=\"{}\" />\n{ind}<Margin value=\"{margin}\" />\n{ind}<Volume value=\"1\" />\n{ind}<Inertia x=\"1\" y=\"1\" z=\"1\" />\n{ind}<MaterialIndex value=\"0\" />\n{ind}<MaterialColourIndex value=\"0\" />\n{ind}<ProceduralID value=\"0\" />\n{ind}<RoomID value=\"0\" />\n{ind}<PedDensity value=\"0\" />\n{ind}<UnkFlags value=\"0\" />\n{ind}<PolyFlags value=\"0\" />\n{ind}<UnkType value=\"1\" />", xyz("BoxMin", a), xyz("BoxMax", b), xyz("BoxCenter", c), xyz("SphereCenter", c), (b - a).len() * 0.5);
    };
    let (verts, tris) = clean_collision_mesh(verts, tris);
    let (verts, tris) = (&verts[..], &tris[..]);
    let (mut mn, mut mx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
    for t in tris {
        for i in [t.0, t.1, t.2] {
            mn = mn.min(verts[i as usize]);
            mx = mx.max(verts[i as usize]);
        }
    }
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<BoundsFile>\n <Bounds type=\"Composite\">\n");
    head(&mut s, "  ", mn, mx, 0.0);
    s.push_str("  <Children>\n");
    for chunk in collision_chunks(tris) {
        let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut order: Vec<u32> = Vec::new();
        let (mut bmn, mut bmx) = (v3(f32::MAX, f32::MAX, f32::MAX), v3(f32::MIN, f32::MIN, f32::MIN));
        for t in chunk {
            for i in [t.0, t.1, t.2] {
                remap.entry(i).or_insert_with(|| {
                    order.push(i);
                    order.len() as u32 - 1
                });
                bmn = bmn.min(verts[i as usize]);
                bmx = bmx.max(verts[i as usize]);
            }
        }
        let gc = (bmn + bmx) * 0.5;
        s.push_str("   <Item type=\"GeometryBVH\">\n");
        head(&mut s, "    ", bmn, bmx, 0.04);
        let _ = writeln!(s, "    <CompositeTransform>\n     1 0 0 0\n     0 1 0 0\n     0 0 1 0\n     0 0 0 1\n    </CompositeTransform>\n    <CompositeFlags1>MAP_WEAPON, MAP_DYNAMIC, MAP_ANIMAL, MAP_COVER, MAP_VEHICLE</CompositeFlags1>\n    <CompositeFlags2>VEHICLE_NOT_BVH, VEHICLE_BVH, PED, RAGDOLL, ANIMAL, ANIMAL_RAGDOLL, OBJECT, PLANT, PROJECTILE, EXPLOSION, FORKLIFT_FORKS, TEST_WEAPON, TEST_CAMERA, TEST_AI, TEST_SCRIPT, TEST_VEHICLE_WHEEL, GLASS</CompositeFlags2>\n    {}\n    <UnkFloat1 value=\"0\" />\n    <UnkFloat2 value=\"0.0025\" />\n    <Materials>", xyz("GeometryCenter", gc));
        for (t, room) in mats {
            let (t, flags) = (t & !MAT_STAIRS, if t & MAT_STAIRS != 0 { "FLAG_STAIRS" } else { "NONE" });
            let _ = writeln!(s, "     <Item>\n      <Type value=\"{t}\" />\n      <ProceduralID value=\"0\" />\n      <RoomID value=\"{room}\" />\n      <PedDensity value=\"0\" />\n      <Flags>{flags}</Flags>\n      <MaterialColourIndex value=\"0\" />\n      <Unk value=\"0\" />\n     </Item>");
        }
        s.push_str("    </Materials>\n    <Vertices>\n");
        for i in &order {
            let p = verts[*i as usize] - gc;
            let _ = writeln!(s, "     {}, {}, {}", p.x, p.y, p.z);
        }
        s.push_str("    </Vertices>\n    <Polygons>\n");
        for t in chunk {
            let _ = writeln!(s, "     <Triangle m=\"{}\" v1=\"{}\" v2=\"{}\" v3=\"{}\" f1=\"0\" f2=\"0\" f3=\"0\" />", t.3, remap[&t.0], remap[&t.1], remap[&t.2]);
        }
        s.push_str("    </Polygons>\n   </Item>\n");
    }
    s.push_str("  </Children>\n </Bounds>\n</BoundsFile>\n");
    s
}

#[cfg(test)]
mod collision_tests {
    use super::*;

    #[test]
    fn soudure_triangles_plats_et_limite_de_sommets() {
        // 30 000 triangles, chacun avec 3 sommets à lui (90 000 sommets) : comme un modèle d'affichage
        let mut verts = Vec::new();
        let mut tris = Vec::new();
        for i in 0..30000u32 {
            let x = (i % 300) as f32;
            let y = (i / 300) as f32;
            let b = verts.len() as u32;
            verts.extend([v3(x, y, 0.0), v3(x + 0.5, y, 0.0), v3(x, y + 0.5, 0.1)]);
            tris.push((b, b + 1, b + 2, 0));
        }
        // doublons exacts d'un sommet, triangle plat, triangle dégénéré, sommet NaN
        let b = verts.len() as u32;
        verts.extend([v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), v3(2.0, 0.0, 0.0), v3(f32::NAN, 0.0, 0.0)]);
        tris.push((b, b + 1, b + 2, 0));
        tris.push((b, b, b + 1, 0));
        tris.push((b, b + 1, b + 3, 0));
        let (cv, ct) = clean_collision_mesh(&verts, &tris);
        assert_eq!(ct.len(), 30000);
        assert!(cv.len() < verts.len());
        for c in collision_chunks(&ct) {
            let n: std::collections::HashSet<u32> = c.iter().flat_map(|t| [t.0, t.1, t.2]).collect();
            assert!(n.len() <= MAX_COL_VERTS && c.len() <= MAX_COL_TRIS);
        }
        let xml = mesh_collision_xml(&verts, &tris, &[1]);
        assert!(!xml.contains("NaN") && !xml.contains("inf"));
        for line in xml.lines().filter(|l| l.contains("<Triangle")) {
            for k in ["v1=\"", "v2=\"", "v3=\""] {
                let v: u32 = line.split(k).nth(1).unwrap().split('"').next().unwrap().parse().unwrap();
                assert!(v < 32768);
            }
        }
    }
}

#[cfg(test)]
mod bridge_tests {
    use super::*;

    fn flat(_x: f32, _y: f32) -> Option<f32> {
        Some(0.0)
    }

    #[test]
    fn pont_au_dessus_d_une_route() {
        let mut low = vec![Road::build(1, &[v3(-150.0, 0.0, 0.0), v3(150.0, 0.0, 0.0)], flat, MAX_GRADE, false).unwrap()];
        let mut r = Road::build(0, &[v3(0.0, -150.0, 0.0), v3(0.0, 150.0, 0.0)], flat, MAX_GRADE, true).unwrap();
        r.raise_over(&low);
        // passe au-dessus : pas de carrefour
        assert!(r.attach(&mut low).is_empty());
        r.mark_spans(flat);
        let (p, _) = r.at(150.0);
        assert!(p.z >= CLEARANCE + DECK_DEPTH - 0.01, "hauteur au croisement {}", p.z);
        assert_eq!(r.spans.len(), 1);
        assert!(r.in_span(150.0) && !r.in_span(5.0));
        // pente respectée, bouts au sol
        for w in r.samples.windows(2) {
            assert!((w[1][2] - w[0][2]).abs() <= (w[1][3] - w[0][3]) * MAX_GRADE + 1e-3);
        }
        assert!(r.samples[0][2].abs() < 0.01);
        // aucune pile sur l'avenue
        let lw = low[0].clone();
        r.place_piers(flat, |x, y, _| lw.nearest(x, y).map(|(d, _)| d < lw.half_total() + 1.0).unwrap_or(false));
        assert!(!r.piers.is_empty() && r.abutments.len() == 2);
        for pr in &r.piers {
            let (q, _) = r.at(pr[0]);
            assert!(q.y.abs() > lw.half_total(), "pile sur l'avenue à {}", q.y);
        }
        let (_, _, parts) = r.mesh(V3::ZERO);
        assert!(parts.iter().any(|p| p.2 == BRIDGE_COLL && !p.1.is_empty()));
    }

    #[test]
    fn passerelle_au_dessus_d_une_route() {
        let low = vec![Road::build(0, &[v3(-60.0, 0.0, 0.0), v3(60.0, 0.0, 0.0)], flat, MAX_GRADE, false).unwrap()];
        let mut f = Road::build(5, &[v3(0.0, -15.0, 0.0), v3(0.0, 15.0, 0.0)], flat, MAX_GRADE, false).unwrap();
        assert!(f.elevated && !f.traffic.enabled);
        f.make_footbridge(flat, &low);
        assert!(f.samples.iter().all(|s| (s[2] - (CLEARANCE + FOOT_DEPTH)).abs() < 0.01));
        assert_eq!(f.stairs, Some([0.0, 0.0]));
        assert_eq!(f.spans, vec![[0.0, f.length()]]);
        // escaliers : le modèle descend jusqu'au sol, au-delà des bouts
        let (pos, _, _) = f.mesh(V3::ZERO);
        assert!(pos.iter().any(|p| p.y < -20.0 && p.z < 0.5));
    }

    #[test]
    fn prolonger_une_route_sans_coude() {
        let old = Road::build(0, &[v3(-120.0, -30.0, 0.0), v3(0.0, 0.0, 0.0)], flat, MAX_GRADE, false).unwrap();
        // dans les deux sens : prolongée par sa fin, ou par son début (route retournée)
        for o in [old.clone(), old.reversed().reversed()] {
            let (r, cut) = o.extended(&[v3(55.0, 95.0, 0.0)], flat, MAX_GRADE, false).unwrap();
            assert_eq!(r.pts.len(), 3);
            assert!(cut < 1.0, "avec 2 points, toute l'ancienne partie est refaite");
            let mut worst = 0.0f32;
            for w in r.samples.windows(3) {
                let a = v3(w[1][0] - w[0][0], w[1][1] - w[0][1], 0.0).norm();
                let b = v3(w[2][0] - w[1][0], w[2][1] - w[1][1], 0.0).norm();
                worst = worst.max(a.dot(b).clamp(-1.0, 1.0).acos().to_degrees());
            }
            assert!(worst < 6.0, "coude de {worst:.1}° entre deux points de la courbe");
        }
        let rr = old.reversed().reversed();
        assert!(old.samples.iter().zip(&rr.samples).all(|(a, b)| (a[3] - b[3]).abs() < 1e-3 && a[0] == b[0]));
    }

    #[test]
    fn pente_reglable() {
        let hill = |x: f32, _y: f32| Some(if x.abs() < 20.0 { 30.0 } else { 0.0 });
        for g in [0.06f32, 0.2] {
            let r = Road::build(3, &[v3(-200.0, 0.0, 0.0), v3(200.0, 0.0, 0.0)], hill, g, false).unwrap();
            for w in r.samples.windows(2) {
                assert!((w[1][2] - w[0][2]).abs() <= (w[1][3] - w[0][3]) * g + 1e-3);
            }
        }
    }
}

#[cfg(test)]
mod roads2_tests {
    use super::*;

    fn flat(_x: f32, _y: f32) -> Option<f32> {
        Some(0.0)
    }

    #[test]
    fn tunnel_sous_une_colline() {
        let hill = |x: f32, _y: f32| Some(if x.abs() < 60.0 { 30.0 } else { 0.0 });
        let mut r = Road::build_ex(0, &[v3(-150.0, 0.0, 0.0), v3(150.0, 0.0, 0.0)], hill, MAX_GRADE, false, true).unwrap();
        // la chaussée reste au niveau des bouts
        assert!(r.samples.iter().all(|s| s[2].abs() < 0.01));
        r.mark_tunnels(hill);
        assert_eq!(r.tunnels.len(), 1);
        let t = r.tunnels[0];
        assert!(t[0] > 80.0 && t[1] < 220.0 && t[1] - t[0] > 100.0, "{t:?}");
        assert!(!r.tunnel_lights().is_empty());
        let (_, _, parts) = r.mesh(V3::ZERO);
        assert!(parts.iter().any(|p| p.0 == LIGHTSTRIP_KEY && !p.1.is_empty()));
    }

    #[test]
    fn rond_point_ferme_et_a_sens_unique() {
        let r = Road::ring(0, v3(0.0, 0.0, 0.0), 20.0, 5.0);
        assert!(r.ring && r.oneway);
        let (a, b) = (r.samples[0], r.samples[r.samples.len() - 1]);
        assert!((a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3);
        let (c, rr) = r.ring_center();
        assert!(c.len() < 0.01 + 5.0 && (rr - 20.0).abs() < 0.01);
        // sens inverse des aiguilles d'une montre
        assert!(r.samples[1][1] > r.samples[0][1]);
        // une route qui arrive dessus : carrefour sans feux
        let mut ring = vec![r];
        let mut a = Road::build(0, &[v3(120.0, 0.0, 5.0), v3(20.0, 0.0, 5.0)], |_, _| Some(5.0), MAX_GRADE, false).unwrap();
        let js = a.attach(&mut ring);
        assert_eq!(js.len(), 1);
        assert_eq!(js[0].control, 3);
    }

    #[test]
    fn raccord_de_largeur() {
        let mut r = Road::build(0, &[v3(0.0, 0.0, 0.0), v3(100.0, 0.0, 0.0)], flat, MAX_GRADE, false).unwrap();
        r.taper = Some((1, TAPER_LEN));
        let (avenue, rue) = (KINDS[1].width * 0.5 + KINDS[1].sidewalk, KINDS[0].width * 0.5 + KINDS[0].sidewalk);
        assert!((r.half_at(0.0) - avenue).abs() < 1e-3);
        assert!((r.half_at(TAPER_LEN + 1.0) - rue).abs() < 1e-3);
        let mid = r.half_at(TAPER_LEN * 0.5);
        assert!(mid < avenue && mid > rue);
    }

    #[test]
    fn bretelle_sur_autoroute_sans_carrefour() {
        let mut hw = vec![Road::build(2, &[v3(-200.0, 0.0, 0.0), v3(200.0, 0.0, 0.0)], flat, MAX_GRADE, false).unwrap()];
        let mut ramp = Road::build(6, &[v3(-100.0, -60.0, 0.0), v3(0.0, -10.2, 0.0)], flat, MAX_GRADE, false).unwrap();
        assert!(ramp.oneway);
        assert!(ramp.attach(&mut hw).is_empty());
        assert!(hw[0].gaps.is_empty());
    }
}
