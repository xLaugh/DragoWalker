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
        Some(Road { kind, pts: p2, samples })
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
        let mut strip = |cols: &[(f32, f32, f32)], vlen: f32, key: u64, coll: u8| {
            let base = pos.len() as u32;
            for (p, r, d) in &frames {
                for (off, dz, u) in cols {
                    pos.push(*p + *r * *off + v3(0.0, 0.0, *dz));
                    uv.push([*u, *d / vlen]);
                }
            }
            let w = cols.len() as u32;
            let mut idx = Vec::new();
            for s in 0..(n as u32 - 1) {
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
        let road_key = road_tex_key(self.kind as usize);
        strip(&[(-hw, lift, 0.0), (0.0, lift, 0.5), (hw, lift, 1.0)], ROAD_TEX_LEN, road_key, 0);
        if k.sidewalk > 0.0 {
            let sw = k.sidewalk;
            let c = lift + k.curb;
            // gauche : jupe, bord extérieur, bord de la bordure, pied de la bordure
            strip(&[(-hw - sw, -0.6, (-hw - sw) / 2.0), (-hw - sw, c, (-hw - sw) / 2.0 + 0.3), (-hw, c, -hw / 2.0 + 0.3), (-hw, lift, -hw / 2.0 + 0.39)], 2.0, SIDEWALK_KEY, 1);
            strip(&[(hw, lift, hw / 2.0 - 0.09), (hw, c, hw / 2.0), (hw + sw, c, (hw + sw) / 2.0), (hw + sw, -0.6, (hw + sw) / 2.0 + 0.3)], 2.0, SIDEWALK_KEY, 1);
        } else {
            let sh = k.shoulder;
            let key = crate::terrain::material_key(k.shoulder_mat as usize);
            strip(&[(-hw - sh, -0.5, (-hw - sh) / 6.0), (-hw - sh, lift - 0.04, (-hw - sh) / 6.0 + 0.1), (-hw, lift, -hw / 6.0 + 0.1)], 6.0, key, 2);
            strip(&[(hw, lift, hw / 6.0), (hw + sh, lift - 0.04, (hw + sh) / 6.0), (hw + sh, -0.5, (hw + sh) / 6.0 + 0.1)], 6.0, key, 2);
        }
        (pos, uv, parts)
    }
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

/// Toutes les textures des routes (gardées en mémoire).
pub fn road_textures() -> Vec<(u64, TexData)> {
    static CACHE: std::sync::OnceLock<Vec<(u64, TexData)>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let mut v: Vec<(u64, TexData)> = (0..KINDS.len()).map(|k| (road_tex_key(k), make_road_texture(k))).collect();
            v.push((SIDEWALK_KEY, make_sidewalk_texture()));
            v
        })
        .clone()
}

/// Collisions d'un modèle quelconque (triangles dans le monde) au format XML de CodeWalker : une pièce
/// GeometryBVH par paquet de 20 000 triangles, sommets relatifs au centre de la pièce (GetVertexPos
/// 0x0600410f), marge 0,04, une liste de types de surface `types` (index `m` de chaque triangle).
pub fn mesh_collision_xml(verts: &[V3], tris: &[(u32, u32, u32, u8)], types: &[u32]) -> String {
    use std::fmt::Write;
    let xyz = |tag: &str, v: V3| format!("<{tag} x=\"{}\" y=\"{}\" z=\"{}\" />", v.x, v.y, v.z);
    let head = |s: &mut String, ind: &str, a: V3, b: V3, margin: f32| {
        let c = (a + b) * 0.5;
        let _ = writeln!(s, "{ind}{}\n{ind}{}\n{ind}{}\n{ind}{}\n{ind}<SphereRadius value=\"{}\" />\n{ind}<Margin value=\"{margin}\" />\n{ind}<Volume value=\"1\" />\n{ind}<Inertia x=\"1\" y=\"1\" z=\"1\" />\n{ind}<MaterialIndex value=\"0\" />\n{ind}<MaterialColourIndex value=\"0\" />\n{ind}<ProceduralID value=\"0\" />\n{ind}<RoomID value=\"0\" />\n{ind}<PedDensity value=\"0\" />\n{ind}<UnkFlags value=\"0\" />\n{ind}<PolyFlags value=\"0\" />\n{ind}<UnkType value=\"1\" />", xyz("BoxMin", a), xyz("BoxMax", b), xyz("BoxCenter", c), xyz("SphereCenter", c), (b - a).len() * 0.5);
    };
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
    for chunk in tris.chunks(20000) {
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
        for t in types {
            let _ = writeln!(s, "     <Item>\n      <Type value=\"{t}\" />\n      <ProceduralID value=\"0\" />\n      <RoomID value=\"0\" />\n      <PedDensity value=\"0\" />\n      <Flags>NONE</Flags>\n      <MaterialColourIndex value=\"0\" />\n      <Unk value=\"0\" />\n     </Item>");
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
