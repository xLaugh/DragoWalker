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
}

pub const KINDS: [RoadKind; 5] = [
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
    },
];

/// Longueur de route couverte par une répétition de la texture de chaussée : un tiret de 3 m tous les 12 m.
pub const ROAD_TEX_LEN: f32 = 12.0;
/// Pas entre deux points de la courbe d'une route (m).
pub const STEP: f32 = 1.5;
/// Pente maximale d'une route (12 %) — **estimé**.
pub const MAX_GRADE: f32 = 0.12;

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
    pub fn build(kind: u8, pts: &[V3], height: impl Fn(f32, f32) -> Option<f32>) -> Option<Road> {
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
        // pente limitée dans les deux sens
        for i in 1..z.len() {
            let m = (dist[i] - dist[i - 1]) * MAX_GRADE;
            z[i] = z[i].clamp(z[i - 1] - m, z[i - 1] + m);
        }
        for i in (0..z.len() - 1).rev() {
            let m = (dist[i + 1] - dist[i]) * MAX_GRADE;
            z[i] = z[i].clamp(z[i + 1] - m, z[i + 1] + m);
        }
        let samples = xy.iter().enumerate().map(|(i, p)| [p[0], p[1], z[i], dist[i]]).collect();
        Some(Road { kind, pts: p2, samples, gaps: Vec::new(), traffic: RoadTraffic::default() })
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
        for o in others.iter_mut() {
            let ohw = o.kind().width * 0.5;
            let o_ext = o.ext();
            let olen = o.length();
            let dirt = self.kind().dirt && o.kind().dirt;
            // extrémités de la nouvelle route posées sur l'autre
            for idx in [0usize, n - 1] {
                let s = self.samples[idx];
                let Some((d, p, along, tan)) = o.nearest_point(s[0], s[1]) else { continue };
                if d > ohw + 0.5 {
                    continue;
                }
                cons.push((idx, p.z));
                if along < 1.0 || along > olen - 1.0 {
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
                let my_cross = self.kind().sidewalk > 0.0;
                let o_cross = o.kind().sidewalk > 0.0;
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
                js.push(Junction { pos: [p.x, p.y, p.z], r: hw.max(ohw) * 1.15 + 0.5, dirt, arms, corners, control: 0, props: Vec::new() });
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
                    let (mc, oc) = (self.kind().sidewalk > 0.0, o.kind().sidewalk > 0.0);
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
                    js.push(Junction { pos: [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, z], r: hw.max(ohw) * 1.15 + 0.5, dirt, arms, corners, control: 0, props: Vec::new() });
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
        // repère de chaque point : droite = tangente tournée de -90°
        let frames: Vec<(V3, V3, f32)> = (0..n)
            .map(|i| {
                let a = self.samples[i.saturating_sub(1)];
                let b = self.samples[(i + 1).min(n - 1)];
                let t = v3(b[0] - a[0], b[1] - a[1], 0.0).norm();
                let s = self.samples[i];
                (v3(s[0] - origin.x, s[1] - origin.y, s[2]), v3(t.y, -t.x, 0.0), s[3])
            })
            .collect();
        // une bande : colonnes (décalage, hauteur, u) de gauche à droite, v = distance / longueur
        let gaps = self.gaps.clone();
        let samples_d: Vec<f32> = self.samples.iter().map(|s| s[3]).collect();
        // side : None = chaussée (jamais ouverte), Some(-1 / 1) = bord gauche / droit
        let mut strip = |cols: &[(f32, f32, f32)], vlen: f32, key: u64, coll: u8, side: Option<f32>| {
            let base = pos.len() as u32;
            for (p, r, d) in &frames {
                for (off, dz, u) in cols {
                    pos.push(*p + *r * *off + v3(0.0, 0.0, *dz));
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
            for s in 0..(n as u32 - 1) {
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
        let lift = 0.08;
        // chaussée : par défaut, comme avant, la texture DragoWalker du type de route (goudron usé et marquages
        // peints, 12 m le long) ; avec « Vraies textures du jeu », goudron du jeu uni (tous les 6 m) et marquages
        // en bandes à part, 6 mm au-dessus
        let game_tex = crate::groundtex::enabled();
        if game_tex {
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
        } else {
            let sh = k.shoulder;
            let key = crate::terrain::material_key(k.shoulder_mat as usize);
            strip(&[(-hw - sh, -0.5, (-hw - sh) / 6.0), (-hw - sh, lift - 0.04, (-hw - sh) / 6.0 + 0.1), (-hw, lift, -hw / 6.0 + 0.1)], 6.0, key, 2, Some(-1.0));
            strip(&[(hw, lift, hw / 6.0), (hw + sh, lift - 0.04, (hw + sh) / 6.0), (hw + sh, -0.5, (hw + sh) / 6.0 + 0.1)], 6.0, key, 2, Some(1.0));
        }
        for l in k.lines.iter().filter(|_| game_tex) {
            let base = pos.len() as u32;
            let (a, b) = (l.off - l.w * 0.5, l.off + l.w * 0.5);
            for (p, r, d) in &frames {
                for (off, u) in [(a, 0.0f32), (b, 1.0)] {
                    pos.push(*p + *r * off + v3(0.0, 0.0, lift + LINE_LIFT));
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
                // pas de marquage dans un carrefour (là où les trottoirs s'ouvrent)
                if gaps.iter().any(|g| mid >= g[0] && mid <= g[1]) {
                    continue;
                }
                let a0 = base + s * 2;
                idx.extend_from_slice(&[a0, a0 + 1, a0 + 3, a0, a0 + 3, a0 + 2]);
            }
            parts.push((if l.yellow { LINE_YELLOW_KEY } else { LINE_WHITE_KEY }, idx, NO_COLLISION));
        }
        (pos, uv, parts)
    }
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
    let mut quad = |pos: &mut Vec<V3>, uv: &mut Vec<[f32; 2]>, idx: &mut Vec<u32>, q: [V3; 4], u: [[f32; 2]; 4]| {
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
            let (d0, d1) = (a.len - 3.4, a.len - 0.4);
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
        let mut tri = |pos: &mut Vec<V3>, uv: &mut Vec<[f32; 2]>, idx: &mut Vec<u32>, p: [V3; 3], want: V3| {
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
    let road = &texs.iter().find(|t| t.0 == road_tex_key(kind)).unwrap().1;
    let side = &texs.iter().find(|t| t.0 == SIDEWALK_KEY).unwrap().1;
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
            v
        })
        .clone()
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
            let _ = writeln!(s, "     <Item>\n      <Type value=\"{t}\" />\n      <ProceduralID value=\"0\" />\n      <RoomID value=\"{room}\" />\n      <PedDensity value=\"0\" />\n      <Flags>NONE</Flags>\n      <MaterialColourIndex value=\"0\" />\n      <Unk value=\"0\" />\n     </Item>");
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
