//! Ouvertures dans les murs d'un bâtiment du jeu : portes, fenêtres, baies.
//!
//! Une ouverture est une boîte orientée (largeur × profondeur × hauteur) posée contre un mur. Elle est retirée
//! du modèle par une vraie coupe : chaque triangle est découpé contre les six plans de la boîte (les morceaux
//! dedans disparaissent, ceux dehors restent, avec tous leurs attributs interpolés : normale, couleur, UV…).
//! Si le mur a une épaisseur (une face dehors et une face dedans sur la ligne qui traverse l'ouverture),
//! l'ouverture reçoit des embrasures : jambages à gauche et à droite, linteau, et appui si c'est une fenêtre.
//!
//! Le cœur travaille sur des sommets « en vrac » (liste de nombres + description de leur disposition) pour
//! servir deux usages : le modèle affiché dans la vue et la collision (RModel), et le fichier .ydr.xml lu par
//! CodeWalker à l'export (disposition des sommets lue dans <Layout>, variable d'une partie à l'autre).
//! La lecture et la réécriture du .ydr.xml (read_ydr_xml / write_ydr_xml) et le passage RModel ↔ maillages servent
//! aussi au « mode Blender » (meshedit.rs).

use crate::gta::drawable::{RGeom, RModel, RVert};
use crate::math::{v3, V3};
use serde::{Deserialize, Serialize};

/// Une ouverture, dans le repère du modèle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Opening {
    /// centre de l'ouverture
    pub pos: [f32; 3],
    /// normale horizontale du mur (vers l'extérieur) : x, y
    pub dir: [f32; 2],
    /// largeur, hauteur, profondeur de coupe (m)
    pub w: f32,
    pub h: f32,
    pub depth: f32,
    /// « porte », « fenêtre », « baie »…
    #[serde(default)]
    pub kind: String,
    /// fenêtre : rebord en bas de l'ouverture
    #[serde(default)]
    pub sill: bool,
    /// porte ou fenêtre du jeu posée dans l'ouverture (vide = aucune)
    #[serde(default)]
    pub prop: String,
}

/// Préréglages : (nom, largeur, hauteur, appui, hauteur du bas au-dessus du sol).
pub const PRESETS: [(&str, f32, f32, bool, f32); 4] = [("porte", 0.95, 2.05, false, 0.0), ("porte double", 1.8, 2.2, false, 0.0), ("fenêtre", 1.2, 1.3, true, 0.9), ("baie vitrée", 2.4, 2.2, false, 0.0)];

/// Boîte orientée : centre, axes (droite, normale du mur, haut), demi-tailles.
#[derive(Clone, Copy, Debug)]
pub struct OBox {
    pub c: V3,
    pub r: V3,
    pub n: V3,
    pub u: V3,
    pub hw: f32,
    pub hd: f32,
    pub hh: f32,
}

impl Opening {
    pub fn obox(&self) -> OBox {
        let mut n = v3(self.dir[0], self.dir[1], 0.0).norm();
        if n.len() < 0.5 {
            n = v3(0.0, 1.0, 0.0);
        }
        let u = v3(0.0, 0.0, 1.0);
        let r = u.cross(n).norm();
        OBox { c: v3(self.pos[0], self.pos[1], self.pos[2]), r, n, u, hw: self.w * 0.5, hd: self.depth * 0.5, hh: self.h * 0.5 }
    }
}

impl OBox {
    /// Six plans (normale vers l'extérieur, décalage) : un point p est du côté extérieur si p·normale − décalage > 0.
    fn planes(&self) -> [(V3, f32); 6] {
        let one = |a: V3, h: f32| [(a, a.dot(self.c) + h), (-a, -a.dot(self.c) + h)];
        let (x, y, z) = (one(self.r, self.hw), one(self.n, self.hd), one(self.u, self.hh));
        [x[0], x[1], y[0], y[1], z[0], z[1]]
    }

    pub fn world_corners(&self) -> [V3; 8] {
        let mut w = [V3::ZERO; 8];
        for (i, c) in w.iter_mut().enumerate() {
            let s = |bit: usize, h: f32| if i & bit == 0 { -h } else { h };
            *c = self.c + self.r * s(1, self.hw) + self.n * s(2, self.hd) + self.u * s(4, self.hh);
        }
        w
    }
}

// ------------------------------------------------------------------ sommets en vrac

/// Disposition d'un sommet : une liste de nombres ; `int[i]` : le nombre i est un entier (couleur…).
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub int: Vec<bool>,
    pub pos: usize,
    pub nrm: Option<usize>,
    /// première coordonnée de texture (TexCoord0), si connue
    pub uv: Option<usize>,
    /// tangente (x, y, z, sens), si connue
    pub tan: Option<usize>,
    /// binormale (x, y, z, sens), si connue
    pub bin: Option<usize>,
    /// autres coordonnées de texture (TexCoord1 et suivantes)
    pub uvx: Vec<usize>,
}

pub type Vx = Vec<f32>;

#[derive(Clone, Debug)]
pub struct GMesh {
    pub layout: Layout,
    pub verts: Vec<Vx>,
    pub idx: Vec<u32>,
}

pub fn pos_of(l: &Layout, v: &Vx) -> V3 {
    v3(v[l.pos], v[l.pos + 1], v[l.pos + 2])
}

fn lerp_vx(l: &Layout, a: &Vx, b: &Vx, t: f32) -> Vx {
    let mut o: Vx = a.iter().zip(b.iter()).enumerate().map(|(i, (x, y))| {
        let v = x + (y - x) * t;
        if l.int[i] { v.round() } else { v }
    }).collect();
    if let Some(n) = l.nrm {
        let nn = v3(o[n], o[n + 1], o[n + 2]).norm();
        o[n] = nn.x;
        o[n + 1] = nn.y;
        o[n + 2] = nn.z;
    }
    o
}

const EPS: f32 = 1e-5;

/// Coupe un polygone convexe par un plan : (partie côté extérieur, partie côté intérieur).
fn split_poly(l: &Layout, poly: &[Vx], nrm: V3, d: f32) -> (Vec<Vx>, Vec<Vx>) {
    let s: Vec<f32> = poly.iter().map(|v| pos_of(l, v).dot(nrm) - d).collect();
    let (mut out, mut inn) = (Vec::new(), Vec::new());
    let n = poly.len();
    for i in 0..n {
        let j = (i + 1) % n;
        let (sa, sb) = (s[i], s[j]);
        if sa > EPS {
            out.push(poly[i].clone());
        } else if sa < -EPS {
            inn.push(poly[i].clone());
        } else {
            out.push(poly[i].clone());
            inn.push(poly[i].clone());
        }
        if (sa > EPS && sb < -EPS) || (sa < -EPS && sb > EPS) {
            let m = lerp_vx(l, &poly[i], &poly[j], sa / (sa - sb));
            out.push(m.clone());
            inn.push(m);
        }
    }
    (out, inn)
}

/// Ce qui reste d'un polygone convexe une fois la boîte retirée (liste de polygones convexes).
fn clip_out_box(l: &Layout, poly: &[Vx], b: &OBox) -> Vec<Vec<Vx>> {
    let mut rest = poly.to_vec();
    let mut keep = Vec::new();
    for (nrm, d) in b.planes() {
        if rest.len() < 3 {
            break;
        }
        let (o, i) = split_poly(l, &rest, nrm, d);
        if o.len() >= 3 {
            keep.push(o);
        }
        rest = i;
    }
    keep
}

/// Un plan de la boîte laisse les trois points du même côté (extérieur) : le triangle ne touche pas la boîte.
fn separated(b: &OBox, p: &[V3; 3]) -> bool {
    b.planes().iter().any(|(n, d)| p.iter().all(|q| q.dot(*n) - d >= -1e-6))
}

fn tri_area2(a: V3, b: V3, c: V3) -> f32 {
    (b - a).cross(c - a).len()
}

/// Retire les boîtes du maillage. Rend le nombre de triangles découpés ou supprimés.
pub fn cut_mesh(m: &mut GMesh, boxes: &[OBox]) -> usize {
    let l = m.layout.clone();
    let idx = std::mem::take(&mut m.idx);
    let orig = m.verts.len();
    let mut out = Vec::with_capacity(idx.len());
    let mut changed = 0;
    for t in idx.chunks_exact(3) {
        if t.iter().any(|&k| k as usize >= orig) {
            continue;
        }
        let pts = [pos_of(&l, &m.verts[t[0] as usize]), pos_of(&l, &m.verts[t[1] as usize]), pos_of(&l, &m.verts[t[2] as usize])];
        let touching: Vec<&OBox> = boxes.iter().filter(|b| !separated(b, &pts)).collect();
        if touching.is_empty() {
            out.extend_from_slice(t);
            continue;
        }
        let mut pieces: Vec<Vec<Vx>> = vec![t.iter().map(|&k| m.verts[k as usize].clone()).collect()];
        for b in touching {
            let mut next = Vec::new();
            for pc in &pieces {
                next.extend(clip_out_box(&l, pc, b));
            }
            pieces = next;
            if pieces.is_empty() {
                break;
            }
        }
        changed += 1;
        for pc in pieces {
            let base = m.verts.len() as u32;
            let pp: Vec<V3> = pc.iter().map(|v| pos_of(&l, v)).collect();
            m.verts.extend(pc);
            for i in 1..pp.len() - 1 {
                if tri_area2(pp[0], pp[i], pp[i + 1]) > 1e-9 {
                    out.extend_from_slice(&[base, base + i as u32, base + i as u32 + 1]);
                }
            }
        }
    }
    m.idx = out;
    changed
}

// ------------------------------------------------------------------ embrasures

/// Épaisseur du mur à l'endroit de l'ouverture : profondeurs (le long de la normale, depuis le centre) des
/// surfaces avant et arrière, le maillage qui porte la surface avant et un sommet modèle pour les embrasures.
pub struct WallSpan {
    pub front: f32,
    pub back: f32,
    pub mesh: usize,
    pub template: Vx,
}

fn ray_t(o: V3, d: V3, a: V3, b: V3, c: V3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

/// Mesure le mur (à faire AVANT la coupe : après, il n'y a plus de triangle à l'endroit de l'ouverture).
pub fn wall_span(meshes: &[GMesh], primary: &[usize], b: &OBox) -> Option<WallSpan> {
    let o = b.c - b.n * b.hd;
    let mut hits: Vec<(f32, usize, usize)> = Vec::new();
    for &mi in primary {
        let m = &meshes[mi];
        for (ti, t) in m.idx.chunks_exact(3).enumerate() {
            if t.iter().any(|&k| k as usize >= m.verts.len()) {
                continue;
            }
            let p = |k: u32| pos_of(&m.layout, &m.verts[k as usize]);
            if let Some(tt) = ray_t(o, b.n, p(t[0]), p(t[1]), p(t[2])) {
                if tt < 2.0 * b.hd {
                    hits.push((tt, mi, ti));
                }
            }
        }
    }
    if hits.len() < 2 {
        return None;
    }
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (f, k) = (hits[0], hits[hits.len() - 1]);
    if k.0 - f.0 < 0.05 {
        return None;
    }
    let m = &meshes[f.1];
    let v0 = m.idx[f.2 * 3] as usize;
    Some(WallSpan { front: f.0 - b.hd, back: k.0 - b.hd, mesh: f.1, template: m.verts[v0].clone() })
}

fn push_quad(m: &mut GMesh, template: &Vx, pts: [V3; 4], nrm: V3) {
    let l = m.layout.clone();
    let base = m.verts.len() as u32;
    for p in pts {
        let mut v = template.clone();
        v[l.pos] = p.x;
        v[l.pos + 1] = p.y;
        v[l.pos + 2] = p.z;
        if let Some(n) = l.nrm {
            v[n] = nrm.x;
            v[n + 1] = nrm.y;
            v[n + 2] = nrm.z;
        }
        m.verts.push(v);
    }
    // sens des triangles : la normale géométrique regarde du côté voulu
    let g = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
    if g.dot(nrm) >= 0.0 {
        m.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    } else {
        m.idx.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

/// Ajoute les embrasures (jambages, linteau, appui) entre la face avant et la face arrière du mur.
pub fn add_reveals(meshes: &mut [GMesh], span: &WallSpan, b: &OBox, sill: bool) {
    let (f, k) = (span.front, span.back);
    let p = |r: f32, s: f32, u: f32| b.c + b.r * r + b.n * s + b.u * u;
    let (hw, hh) = (b.hw, b.hh);
    let m = &mut meshes[span.mesh];
    // jambage gauche (normale vers la droite), droit, linteau, appui
    push_quad(m, &span.template, [p(-hw, f, -hh), p(-hw, f, hh), p(-hw, k, hh), p(-hw, k, -hh)], b.r);
    push_quad(m, &span.template, [p(hw, f, -hh), p(hw, f, hh), p(hw, k, hh), p(hw, k, -hh)], -b.r);
    push_quad(m, &span.template, [p(-hw, f, hh), p(hw, f, hh), p(hw, k, hh), p(-hw, k, hh)], -b.u);
    if sill {
        push_quad(m, &span.template, [p(-hw, f, -hh), p(hw, f, -hh), p(hw, k, -hh), p(-hw, k, -hh)], b.u);
    }
}

/// Applique les ouvertures à une liste de maillages (une ouverture après l'autre). `primary` : les maillages qui
/// servent à mesurer le mur (le niveau de détail le plus fin). Rend le nombre de triangles découpés.
pub fn apply_openings(meshes: &mut [GMesh], primary: &[usize], openings: &[Opening]) -> usize {
    let mut changed = 0;
    for o in openings {
        let b = o.obox();
        let span = wall_span(meshes, primary, &b);
        for m in meshes.iter_mut() {
            changed += cut_mesh(m, &[b]);
        }
        if let Some(s) = span {
            add_reveals(meshes, &s, &b, o.sill);
        }
    }
    changed
}

// ------------------------------------------------------------------ modèle de la vue (RModel)

fn rmodel_layout() -> Layout {
    let mut int = vec![false; 14];
    int[6..10].iter_mut().for_each(|x| *x = true);
    Layout { int, pos: 0, nrm: Some(3), uv: Some(10), uvx: vec![12], ..Default::default() }
}

fn to_vx(v: &RVert) -> Vx {
    vec![v.pos[0], v.pos[1], v.pos[2], v.nrm[0], v.nrm[1], v.nrm[2], v.col[0] as f32, v.col[1] as f32, v.col[2] as f32, v.col[3] as f32, v.uv[0], v.uv[1], v.uv1[0], v.uv1[1]]
}

fn from_vx(v: &Vx) -> RVert {
    let c = |x: f32| x.round().clamp(0.0, 255.0) as u8;
    RVert { pos: [v[0], v[1], v[2]], nrm: [v[3], v[4], v[5]], col: [c(v[6]), c(v[7]), c(v[8]), c(v[9])], uv: [v[10], v[11]], uv1: [v[12], v[13]] }
}

/// Les parties d'un modèle de la vue en maillages « en vrac » (un par partie, sommets utilisés seulement).
pub fn rmodel_to_meshes(m: &RModel) -> Vec<GMesh> {
    let layout = rmodel_layout();
    let mut meshes: Vec<GMesh> = Vec::new();
    for g in &m.geoms {
        let end = ((g.first + g.count) as usize).min(m.idx.len());
        let slice = &m.idx[(g.first as usize).min(end)..end];
        let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut verts: Vec<Vx> = Vec::new();
        let mut idx = Vec::with_capacity(slice.len());
        for &i in slice {
            let Some(v) = m.verts.get(i as usize) else { continue };
            let n = *remap.entry(i).or_insert_with(|| {
                verts.push(to_vx(v));
                verts.len() as u32 - 1
            });
            idx.push(n);
        }
        meshes.push(GMesh { layout: layout.clone(), verts, idx });
    }
    meshes
}

/// Remet les maillages (un par partie, dans l'ordre) dans le modèle de la vue : mêmes parties, mêmes matériaux.
pub fn meshes_to_rmodel(m: &mut RModel, meshes: &[GMesh]) {
    let mut verts: Vec<RVert> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let mut geoms: Vec<RGeom> = Vec::new();
    for (g, mesh) in m.geoms.iter().zip(meshes.iter()) {
        let base = verts.len() as u32;
        verts.extend(mesh.verts.iter().map(from_vx));
        let first = idx.len() as u32;
        idx.extend(mesh.idx.iter().map(|i| i + base));
        let mut ng = g.clone();
        ng.first = first;
        ng.count = idx.len() as u32 - first;
        geoms.push(ng);
    }
    m.verts = verts;
    m.idx = idx;
    m.geoms = geoms;
}

/// Applique les ouvertures à un modèle de la vue : mêmes parties, sommets ajoutés. Rend le nombre de triangles
/// découpés.
pub fn apply_to_rmodel(m: &mut RModel, openings: &[Opening]) -> usize {
    if openings.is_empty() {
        return 0;
    }
    let mut meshes = rmodel_to_meshes(m);
    let primary: Vec<usize> = (0..meshes.len()).collect();
    let changed = apply_openings(&mut meshes, &primary, openings);
    meshes_to_rmodel(m, &meshes);
    changed
}

// ------------------------------------------------------------------ fichier .ydr.xml de CodeWalker

/// Nombre de nombres par canal d'un sommet dans le XML de CodeWalker, et s'ils sont entiers.
fn channel(name: &str) -> Option<(usize, bool)> {
    match name {
        "Position" | "Normal" => Some((3, false)),
        "Colour0" | "Colour1" | "Colour2" | "BlendWeights" | "BlendIndices" => Some((4, true)),
        "Tangent" | "Binormal" => Some((4, false)),
        n if n.starts_with("TexCoord") => Some((2, false)),
        _ => None,
    }
}

/// Plus grand nombre de sommets d'une partie : les indices du jeu sont sur 16 bits.
pub const MAX_PART_VERTS: usize = 65535;

/// Position d'un os (ou d'une partie attachée à un os) dans le repère du modèle : (rotation x, y, z, w ;
/// translation).
pub type BoneXf = ([f32; 4], V3);

fn q_rot(q: [f32; 4], v: V3) -> V3 {
    // v + 2 w (u × v) + 2 u × (u × v), comme gta::drawable::quat_rotate
    let u = v3(q[0], q[1], q[2]);
    let uv = u.cross(v);
    let uuv = u.cross(uv);
    v + (uv * q[3] + uuv) * 2.0
}

fn q_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

/// Passe un sommet du repère de sa partie à celui du modèle (`inverse` : l'inverse) : position, normale,
/// tangente et binormale.
fn xf_vx(l: &Layout, v: &mut Vx, xf: &BoneXf, inverse: bool) {
    let (q, t) = *xf;
    let qi = [-q[0], -q[1], -q[2], q[3]];
    let rot = |p: V3| if inverse { q_rot(qi, p) } else { q_rot(q, p) };
    let p = pos_of(l, v);
    let np = if inverse { q_rot(qi, p - t) } else { q_rot(q, p) + t };
    v[l.pos] = np.x;
    v[l.pos + 1] = np.y;
    v[l.pos + 2] = np.z;
    for k in [l.nrm, l.tan, l.bin].into_iter().flatten() {
        let d = rot(v3(v[k], v[k + 1], v[k + 2]));
        v[k] = d.x;
        v[k + 1] = d.y;
        v[k + 2] = d.z;
    }
}

/// Nombre de l'attribut `key` d'une ligne comme `<Rotation x="0" y="0" z="0" w="1" />`.
fn num_attr(line: &str, key: &str) -> Option<f32> {
    let a = line.find(&format!(" {key}=\""))? + key.len() + 3;
    let b = a + line[a..].find('"')?;
    let s = line[a..b].trim();
    s.parse().ok().or(match s.to_ascii_lowercase().as_str() {
        "true" => Some(1.0),
        "false" => Some(0.0),
        _ => None,
    })
}

/// Os du squelette (<Skeleton><Bones>) dans le repère du modèle, chacun composé avec son parent — comme
/// gta::drawable::bone_transforms pour le modèle de la vue.
fn read_skeleton(lines: &[&str]) -> Vec<BoneXf> {
    let ind = |l: &str| l.len() - l.trim_start().len();
    let Some(s0) = lines.iter().position(|l| l.trim() == "<Skeleton>") else { return Vec::new() };
    let Some(s1) = (s0..lines.len()).find(|&k| lines[k].trim() == "</Skeleton>" && ind(lines[k]) == ind(lines[s0])) else { return Vec::new() };
    let Some(b0) = (s0..s1).find(|&k| lines[k].trim() == "<Bones>") else { return Vec::new() };
    let b1 = (b0..s1).find(|&k| lines[k].trim() == "</Bones>" && ind(lines[k]) == ind(lines[b0])).unwrap_or(s1);
    let mut abs: Vec<BoneXf> = Vec::new();
    let mut k = b0 + 1;
    while k < b1 {
        if lines[k].trim() != "<Item>" {
            k += 1;
            continue;
        }
        let ii = ind(lines[k]);
        let end = (k + 1..b1).find(|&j| lines[j].trim() == "</Item>" && ind(lines[j]) == ii).unwrap_or(b1);
        let (mut q, mut t, mut parent) = ([0.0, 0.0, 0.0, 1.0], V3::ZERO, -1i32);
        for l in &lines[k + 1..end] {
            let s = l.trim_start();
            if s.starts_with("<ParentIndex ") {
                parent = num_attr(s, "value").map(|v| v as i32).unwrap_or(-1);
            } else if s.starts_with("<Translation ") {
                if let Some(v) = xyz_attr(s) {
                    t = v;
                }
            } else if s.starts_with("<Rotation ") {
                if let (Some(x), Some(y), Some(z), Some(w)) = (num_attr(s, "x"), num_attr(s, "y"), num_attr(s, "z"), num_attr(s, "w")) {
                    let n = (x * x + y * y + z * z + w * w).sqrt();
                    if n > 1e-6 && n.is_finite() {
                        q = [x / n, y / n, z / n, w / n];
                    }
                }
            }
        }
        let i = abs.len();
        if parent >= 0 && (parent as usize) < i {
            let (pq, pt) = abs[parent as usize];
            abs.push((q_mul(pq, q), pt + q_rot(pq, t)));
        } else {
            abs.push((q, t));
        }
        k = end + 1;
    }
    abs
}

/// Os auquel est attachée la partie de modèle (DrawableModel) qui contient la ligne `gs` : <HasSkin> et
/// <BoneIndex> du modèle, comme gta::drawable::model_bone (rien pour une partie à peau ou sur l'os 0).
fn part_bone(lines: &[&str], gs: usize, bones: &[BoneXf]) -> Option<BoneXf> {
    let ind = |l: &str| l.len() - l.trim_start().len();
    let gind = ind(lines[gs]);
    let g0 = (0..gs).rev().find(|&k| lines[k].trim() == "<Geometries>" && ind(lines[k]) < gind)?;
    let ms = (0..g0).rev().find(|&k| lines[k].trim() == "<Item>" && ind(lines[k]) < ind(lines[g0]))?;
    let val = |tag: &str| (ms..g0).find(|&k| lines[k].trim_start().starts_with(tag)).and_then(|k| num_attr(lines[k], "value"));
    let (skin, bone) = (val("<HasSkin ").unwrap_or(0.0), val("<BoneIndex ").unwrap_or(0.0) as usize);
    if skin != 0.0 || bone == 0 || bone >= bones.len() {
        return None;
    }
    let (q, t) = bones[bone];
    if q == [0.0, 0.0, 0.0, 1.0] && t == V3::ZERO {
        return None;
    }
    Some((q, t))
}

/// Une partie (<Item> avec <VertexBuffer> et <IndexBuffer>) du XML.
pub struct XGeom {
    gs: usize,
    ge: usize,
    vb: usize,
    vd0: usize,
    vd1: usize,
    id0: usize,
    id1: usize,
    /// seule partie de sa liste <Geometries> (vidée, elle est gardée réduite à un point)
    alone: bool,
    /// partie attachée à un os : sommets du XML passés dans le repère du modèle à la lecture, ramenés à l'écriture
    pub bone: Option<BoneXf>,
    /// partie du niveau de détail le plus fin (DrawableModelsHigh)
    pub primary: bool,
    /// sommets dans le repère du modèle (comme le modèle de la vue)
    pub mesh: GMesh,
}

/// Un .ydr.xml de CodeWalker lu partie par partie (les parties de disposition inconnue sont laissées de côté, et
/// notées dans `skipped`).
pub struct YdrXml<'a> {
    lines: Vec<&'a str>,
    pub geoms: Vec<XGeom>,
    /// parties laissées de côté (jamais modifiées) : « partie n (disposition) : raison »
    pub skipped: Vec<String>,
}

/// Lit les parties d'un .ydr.xml de CodeWalker (sommets en vrac, disposition lue dans le <Layout> de chaque
/// partie, quel que soit son type GTAV1, GTAV2…). Les sommets des parties attachées à un os sont passés dans le
/// repère du modèle.
pub fn read_ydr_xml(src: &str) -> Option<YdrXml<'_>> {
    let lines: Vec<&str> = src.lines().collect();
    let ind = |l: &str| l.len() - l.trim_start().len();
    // recherche bornée à [a, b)
    let find = |a: usize, b: usize, what: &str| (a..b.min(lines.len())).find(|&k| lines[k].trim() == what);
    // niveau le plus fin
    let high = lines.iter().position(|l| l.trim() == "<DrawableModelsHigh>").and_then(|a| (a..lines.len()).find(|&k| lines[k].trim() == "</DrawableModelsHigh>").map(|b| (a, b)));
    let bones = read_skeleton(&lines);
    let mut geoms: Vec<XGeom> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut part = 0usize;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != "<VertexBuffer>" {
            i += 1;
            continue;
        }
        let vb = i;
        part += 1;
        let Some(gs) = (0..vb).rev().find(|&k| lines[k].trim() == "<Item>") else { i += 1; continue };
        let gind = ind(lines[gs]);
        let ge = (vb..lines.len()).find(|&k| lines[k].trim() == "</Item>" && ind(lines[k]) == gind)?;
        i = ge + 1;
        let skip = |why: &str, ty: &str, skipped: &mut Vec<String>| skipped.push(format!("partie {part}{ty} : {why}"));
        // tout est cherché dans cette partie : <Data> du VertexBuffer (pas celui de l'IndexBuffer), <Layout> de
        // ce VertexBuffer (pas celui de la partie suivante)
        let Some(vbe) = find(vb, ge, "</VertexBuffer>") else {
            skip("tampon de sommets incomplet", "", &mut skipped);
            continue;
        };
        let (Some(vd0), Some(ib)) = (find(vb, vbe, "<Data>").or_else(|| find(vb, vbe, "<Data2>")), find(vbe, ge, "<IndexBuffer>")) else {
            skip("sommets ou indices absents", "", &mut skipped);
            continue;
        };
        let Some(vd1) = (vd0 + 1..vbe).find(|&k| lines[k].trim().starts_with("</Data")) else {
            skip("sommets incomplets", "", &mut skipped);
            continue;
        };
        let (Some(id0), Some(id1)) = (find(ib, ge, "<Data>"), find(ib, ge, "</Data>")) else {
            skip("indices incomplets", "", &mut skipped);
            continue;
        };
        // disposition des sommets
        let l0 = (vb..vd0).find(|&k| lines[k].trim().starts_with("<Layout"));
        let l1 = l0.and_then(|a| (a..vd0).find(|&k| lines[k].trim() == "</Layout>"));
        let ty = l0.and_then(|k| lines[k].split("type=\"").nth(1)).and_then(|s| s.split('"').next()).map(|t| format!(" ({t})")).unwrap_or_default();
        let (Some(l0), Some(l1)) = (l0, l1) else {
            skip("disposition des sommets absente", &ty, &mut skipped);
            continue;
        };
        let mut layout = Layout::default();
        let mut off = 0usize;
        let (mut pos, mut unknown) = (None, None);
        for k in l0 + 1..l1 {
            let name = lines[k].trim().trim_start_matches('<').split(|c: char| c == ' ' || c == '/' || c == '>').next().unwrap_or("");
            if name.is_empty() {
                continue;
            }
            let Some((n, is_int)) = channel(name) else {
                unknown = Some(name.to_string());
                break;
            };
            match name {
                "Position" => pos = Some(off),
                "Normal" => layout.nrm = Some(off),
                "TexCoord0" => layout.uv = Some(off),
                "Tangent" => layout.tan = Some(off),
                "Binormal" => layout.bin = Some(off),
                t if t.starts_with("TexCoord") => layout.uvx.push(off),
                _ => {}
            }
            layout.int.extend(std::iter::repeat(is_int).take(n));
            off += n;
        }
        if let Some(u) = unknown {
            skip(&format!("composante de sommet inconnue « {u} »"), &ty, &mut skipped);
            continue;
        }
        let Some(pos) = pos else {
            skip("pas de position", &ty, &mut skipped);
            continue;
        };
        layout.pos = pos;
        // sommets et indices
        let mut verts: Vec<Vx> = Vec::new();
        let mut bad = false;
        for l in &lines[vd0 + 1..vd1] {
            if l.trim().is_empty() {
                continue;
            }
            let v: Vx = l.split_whitespace().filter_map(|s| s.parse::<f32>().ok()).collect();
            if v.len() != off {
                bad = true;
                break;
            }
            verts.push(v);
        }
        if bad {
            skip(&format!("sommets qui ne suivent pas la disposition ({off} nombres attendus)"), &ty, &mut skipped);
            continue;
        }
        let bone = part_bone(&lines, gs, &bones);
        if let Some(xf) = bone {
            for v in verts.iter_mut() {
                xf_vx(&layout, v, &xf, false);
            }
        }
        let idx: Vec<u32> = lines[id0 + 1..id1].iter().flat_map(|l| l.split_whitespace().filter_map(|s| s.parse::<u32>().ok())).collect();
        let primary = high.map(|(a, b)| gs > a && ge < b).unwrap_or(true);
        // seule partie de sa liste <Geometries> ?
        let alone = (0..gs)
            .rev()
            .find(|&k| lines[k].trim() == "<Geometries>" && ind(lines[k]) < gind)
            .map(|g0| {
                let g1 = (ge..lines.len()).find(|&k| lines[k].trim() == "</Geometries>" && ind(lines[k]) == ind(lines[g0])).unwrap_or(lines.len());
                (g0..g1).filter(|&k| lines[k].trim() == "<Item>" && ind(lines[k]) == gind).count() <= 1
            })
            .unwrap_or(true);
        geoms.push(XGeom { gs, ge, vb, vd0, vd1, id0, id1, alone, bone, primary, mesh: GMesh { layout, verts, idx } });
    }
    if geoms.is_empty() {
        return None;
    }
    Some(YdrXml { lines, geoms, skipped })
}

/// Nombres x, y, z d'une ligne comme `<BoundingBoxMin x="1" y="2" z="3" />`.
fn xyz_attr(line: &str) -> Option<V3> {
    Some(v3(num_attr(line, "x")?, num_attr(line, "y")?, num_attr(line, "z")?))
}

/// Triangles d'un maillage en morceaux d'au plus `max` sommets : pour chacun, les sommets utilisés (numéros dans
/// le maillage) et les indices renumérotés.
fn split_chunks(m: &GMesh, max: usize) -> Vec<(Vec<u32>, Vec<u32>)> {
    let max = max.max(3);
    let mut out = Vec::new();
    let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let (mut order, mut idx): (Vec<u32>, Vec<u32>) = (Vec::new(), Vec::new());
    for t in m.idx.chunks_exact(3) {
        if t.iter().any(|&k| k as usize >= m.verts.len()) {
            continue;
        }
        let new = (0..3).filter(|&j| !remap.contains_key(&t[j]) && !t[..j].contains(&t[j])).count();
        if order.len() + new > max {
            out.push((std::mem::take(&mut order), std::mem::take(&mut idx)));
            remap.clear();
        }
        for &k in t {
            let n = *remap.entry(k).or_insert_with(|| {
                order.push(k);
                order.len() as u32 - 1
            });
            idx.push(n);
        }
    }
    if !idx.is_empty() {
        out.push((order, idx));
    }
    out
}

/// Réécrit le XML avec les maillages modifiés (`meshes` : un par partie de `x.geoms`, dans l'ordre, repère du
/// modèle). Seules les parties changées sont réécrites (sommets utilisés, renumérotés ; boîte de la partie) :
/// - une partie de plus de 65 535 sommets (indices 16 bits) est découpée en plusieurs parties (même matériau) ;
/// - une partie vidée est retirée (seule de sa liste : réduite à un triangle nul, boîte nulle) ;
/// - si elles dépassent la boîte du modèle entier (en-tête), celle-ci et la sphère sont agrandies.
pub fn write_ydr_xml(x: &YdrXml, meshes: &[GMesh]) -> String {
    write_ydr_xml_max(x, meshes, MAX_PART_VERTS)
}

fn write_ydr_xml_max(x: &YdrXml, meshes: &[GMesh], max_verts: usize) -> String {
    let lines = &x.lines;
    let mut out: Vec<Option<String>> = lines.iter().map(|l| Some(l.to_string())).collect();
    let big = 3.0e38f32;
    let (mut all_mn, mut all_mx) = (v3(big, big, big), v3(-big, -big, -big));
    let pad = |k: usize| -> String { lines.get(k).map(|l| l.chars().take_while(|c| c.is_whitespace()).collect()).unwrap_or_default() };
    for (g, m) in x.geoms.iter().zip(meshes.iter()) {
        if m.idx == g.mesh.idx && m.verts == g.mesh.verts {
            continue;
        }
        let l = &m.layout;
        // sommet écrit : ramené dans le repère de la partie
        let local = |v: &Vx| -> Vx {
            let mut v = v.clone();
            if let Some(xf) = g.bone {
                xf_vx(l, &mut v, &xf, true);
            }
            v
        };
        let fmt = |v: &Vx| -> String { v.iter().enumerate().map(|(i, x)| if l.int[i] { format!("{}", x.round().clamp(0.0, 255.0) as i32) } else { format!("{x}") }).collect::<Vec<_>>().join(" ") };
        let vpad = if g.vd1 > g.vd0 + 1 { pad(g.vd0 + 1) } else { pad(g.vd0) + " " };
        let ipad = pad(g.id0);
        // une copie de la partie par morceau (sommets, indices, boîte)
        let emit = |verts: &[Vx], idx: &[u32], text: &mut Vec<String>| {
            let (mut mn, mut mx) = (v3(big, big, big), v3(-big, -big, -big));
            for v in verts {
                let p = pos_of(l, v);
                mn = mn.min(p);
                mx = mx.max(p);
            }
            for k in g.gs..=g.ge {
                if (k > g.vd0 && k < g.vd1) || (k > g.id0 && k < g.id1) {
                    continue;
                }
                let t = lines[k].trim_start();
                if k < g.vb && t.starts_with("<BoundingBoxMin ") {
                    text.push(format!("{}<BoundingBoxMin x=\"{}\" y=\"{}\" z=\"{}\" w=\"0\" />", pad(k), mn.x, mn.y, mn.z));
                } else if k < g.vb && t.starts_with("<BoundingBoxMax ") {
                    text.push(format!("{}<BoundingBoxMax x=\"{}\" y=\"{}\" z=\"{}\" w=\"0\" />", pad(k), mx.x, mx.y, mx.z));
                } else {
                    text.push(lines[k].to_string());
                }
                if k == g.vd0 {
                    text.extend(verts.iter().map(|v| format!("{vpad}{}", fmt(v))));
                }
                if k == g.id0 {
                    text.extend(idx.chunks(24).map(|c| format!("{ipad} {}", c.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" "))));
                }
            }
        };
        let chunks = split_chunks(m, max_verts);
        let mut text: Vec<String> = Vec::new();
        for (order, idx) in &chunks {
            for &k in order {
                let p = pos_of(l, &m.verts[k as usize]);
                all_mn = all_mn.min(p);
                all_mx = all_mx.max(p);
            }
            let verts: Vec<Vx> = order.iter().map(|&k| local(&m.verts[k as usize])).collect();
            emit(&verts, idx, &mut text);
        }
        if chunks.is_empty() && g.alone {
            // partie vidée, seule de sa liste : un triangle nul sur un de ses sommets (boîte nulle), pour ne pas
            // laisser un modèle sans partie ni une boîte de ±3e38
            match m.verts.first().or(g.mesh.verts.first()) {
                Some(v) => emit(&[local(v)], &[0, 0, 0], &mut text),
                None => continue,
            }
        }
        for o in out.iter_mut().take(g.ge + 1).skip(g.gs) {
            *o = None;
        }
        if !text.is_empty() {
            out[g.gs] = Some(text.join("\n"));
        }
    }
    // en-tête (avant <ShaderGroup>) : boîte et sphère du modèle entier agrandies si besoin
    if all_mn.x <= all_mx.x {
        let header_end = lines.iter().position(|l| l.trim() == "<ShaderGroup>").unwrap_or(0);
        let hmn = (0..header_end).find(|&k| lines[k].trim_start().starts_with("<BoundingBoxMin ")).and_then(|k| xyz_attr(lines[k]).map(|v| (k, v)));
        let hmx = (0..header_end).find(|&k| lines[k].trim_start().starts_with("<BoundingBoxMax ")).and_then(|k| xyz_attr(lines[k]).map(|v| (k, v)));
        if let (Some((kn, mn0)), Some((kx, mx0))) = (hmn, hmx) {
            let (mn, mx) = (mn0.min(all_mn), mx0.max(all_mx));
            if mn != mn0 || mx != mx0 {
                out[kn] = Some(format!("{}<BoundingBoxMin x=\"{}\" y=\"{}\" z=\"{}\" />", pad(kn), mn.x, mn.y, mn.z));
                out[kx] = Some(format!("{}<BoundingBoxMax x=\"{}\" y=\"{}\" z=\"{}\" />", pad(kx), mx.x, mx.y, mx.z));
                let c = (mn + mx) * 0.5;
                for k in 0..header_end {
                    let t = lines[k].trim_start();
                    if t.starts_with("<BoundingSphereCenter ") {
                        out[k] = Some(format!("{}<BoundingSphereCenter x=\"{}\" y=\"{}\" z=\"{}\" />", pad(k), c.x, c.y, c.z));
                    } else if t.starts_with("<BoundingSphereRadius ") {
                        out[k] = Some(format!("{}<BoundingSphereRadius value=\"{}\" />", pad(k), (mx - mn).len() * 0.5));
                    }
                }
            }
        }
    }
    out.into_iter().flatten().collect::<Vec<_>>().join("\n") + "\n"
}

/// Applique les ouvertures à un .ydr.xml lu par CodeWalker : tous les niveaux de détail sont coupés, les
/// embrasures sont ajoutées au niveau le plus fin (DrawableModelsHigh). Une partie dont la disposition des
/// sommets est inconnue est laissée telle quelle. Rend le XML et le nombre de triangles découpés.
pub fn apply_to_ydr_xml(src: &str, openings: &[Opening]) -> Option<(String, usize)> {
    if openings.is_empty() {
        return None;
    }
    let x = read_ydr_xml(src)?;
    let primary: Vec<usize> = x.geoms.iter().enumerate().filter(|(_, g)| g.primary).map(|(i, _)| i).collect();
    let mut meshes: Vec<GMesh> = x.geoms.iter().map(|g| g.mesh.clone()).collect();
    // maillages de layouts différents : la mesure du mur et les embrasures se font part par part
    let mut changed = 0;
    for o in openings {
        let b = o.obox();
        let mut span: Option<WallSpan> = None;
        // le mur se mesure sur les parties du niveau fin (chacune avec sa disposition)
        for &pi in &primary {
            if let Some(s) = wall_span(&meshes[pi..pi + 1], &[0], &b) {
                let better = span.as_ref().map(|c| s.front < c.front).unwrap_or(true);
                if better {
                    span = Some(WallSpan { front: s.front, back: s.back, mesh: pi, template: s.template });
                }
            }
        }
        for m in meshes.iter_mut() {
            changed += cut_mesh(m, &[b]);
        }
        if let Some(s) = span {
            add_reveals(&mut meshes, &s, &b, o.sill);
        }
    }
    Some((write_ydr_xml(&x, &meshes), changed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lay() -> Layout {
        // position, normale, uv
        Layout { int: vec![false; 8], pos: 0, nrm: Some(3), uv: Some(6), ..Default::default() }
    }

    /// Un mur plan dans le plan y = `y`, de x ∈ [-5, 5], z ∈ [0, 4], normale vers +y ; uv = (x, z).
    fn wall(y: f32, ny: f32) -> GMesh {
        let q = [(-5.0, 0.0), (5.0, 0.0), (5.0, 4.0), (-5.0, 4.0)];
        let verts: Vec<Vx> = q.iter().map(|&(x, z)| vec![x, y, z, 0.0, ny, 0.0, x, z]).collect();
        let idx = if ny > 0.0 { vec![0, 1, 2, 0, 2, 3] } else { vec![0, 2, 1, 0, 3, 2] };
        GMesh { layout: lay(), verts, idx }
    }

    fn area(m: &GMesh) -> f32 {
        m.idx.chunks_exact(3).map(|t| tri_area2(pos_of(&m.layout, &m.verts[t[0] as usize]), pos_of(&m.layout, &m.verts[t[1] as usize]), pos_of(&m.layout, &m.verts[t[2] as usize])) * 0.5).sum()
    }

    fn opening(depth: f32, sill: bool) -> Opening {
        Opening { pos: [0.5, 0.0, 1.5], dir: [0.0, 1.0], w: 1.0, h: 2.0, depth, kind: "porte".into(), sill, prop: String::new() }
    }

    #[test]
    fn la_coupe_retire_exactement_l_ouverture_et_interpole() {
        let mut m = wall(0.0, 1.0);
        assert!((area(&m) - 40.0).abs() < 1e-3);
        let b = opening(0.4, false).obox();
        let n = cut_mesh(&mut m, &[b]);
        assert!(n >= 1);
        assert!((area(&m) - 38.0).abs() < 1e-3, "40 − 1 × 2 m² = 38, trouvé {}", area(&m));
        // aucun triangle restant n'a son centre dans l'ouverture (x ∈ [0, 1], z ∈ [0.5, 2.5])
        for t in m.idx.chunks_exact(3) {
            let c = (pos_of(&m.layout, &m.verts[t[0] as usize]) + pos_of(&m.layout, &m.verts[t[1] as usize]) + pos_of(&m.layout, &m.verts[t[2] as usize])) * (1.0 / 3.0);
            assert!(!(c.x > 0.01 && c.x < 0.99 && c.z > 0.51 && c.z < 2.49), "centre dans l'ouverture : {c:?}");
        }
        // les UV des sommets créés suivent la position (uv = x, z) ; normales conservées
        for v in &m.verts {
            assert!((v[6] - v[0]).abs() < 1e-3 && (v[7] - v[2]).abs() < 1e-3, "uv mal interpolées : {v:?}");
            assert!((v[4] - 1.0).abs() < 1e-3);
        }
        // le sens des triangles est conservé (le mur de départ a sa normale géométrique vers −y)
        for t in m.idx.chunks_exact(3) {
            let (a, b, c) = (pos_of(&m.layout, &m.verts[t[0] as usize]), pos_of(&m.layout, &m.verts[t[1] as usize]), pos_of(&m.layout, &m.verts[t[2] as usize]));
            assert!((b - a).cross(c - a).y < 0.0);
        }
    }

    #[test]
    fn un_triangle_loin_de_la_boite_est_laisse_intact() {
        let mut m = wall(0.0, 1.0);
        let b = Opening { pos: [50.0, 0.0, 1.5], ..opening(0.4, false) }.obox();
        let idx = m.idx.clone();
        assert_eq!(cut_mesh(&mut m, &[b]), 0);
        assert_eq!(m.idx, idx);
        assert_eq!(m.verts.len(), 4);
    }

    #[test]
    fn mur_epais_embrasures_et_mur_mince_sans() {
        // deux faces à y = −0.15 (vers −y) et y = +0.15 (vers +y) : un mur de 30 cm
        let mut thick = vec![wall(0.15, 1.0), wall(-0.15, -1.0)];
        let b = opening(0.6, true);
        let before = area(&thick[0]) + area(&thick[1]);
        let n = apply_openings(&mut thick, &[0, 1], &[b.clone()]);
        assert!(n >= 2);
        let after = area(&thick[0]) + area(&thick[1]);
        // deux faces percées (−2 m² chacune) + embrasures : 2 × (2 × 0,3) + 2 × (1 × 0,3) = 1,8 m²
        assert!((after - (before - 4.0 + 1.8)).abs() < 0.02, "aire {after}, attendue {}", before - 4.0 + 1.8);
        // les embrasures sont entre les deux faces et regardent vers l'ouverture
        let added: Vec<[V3; 3]> = thick
            .iter()
            .flat_map(|m| m.idx.chunks_exact(3).map(|t| [pos_of(&m.layout, &m.verts[t[0] as usize]), pos_of(&m.layout, &m.verts[t[1] as usize]), pos_of(&m.layout, &m.verts[t[2] as usize])]))
            .filter(|t| t.iter().all(|p| p.y.abs() < 0.151) && (t[1] - t[0]).cross(t[2] - t[0]).y.abs() < 1e-4)
            .collect();
        assert_eq!(added.len(), 8, "4 quads = 8 triangles");
        // leurs normales regardent vers l'ouverture (jambages : vers le centre en x ; linteau : vers le bas ; appui : vers le haut)
        let centre = v3(0.5, 0.0, 1.5);
        for t in &added {
            let g = (t[1] - t[0]).cross(t[2] - t[0]);
            let mid = (t[0] + t[1] + t[2]) * (1.0 / 3.0);
            assert!(g.dot(centre - mid) > 0.0, "embrasure tournée vers l'extérieur : {t:?}");
        }
        // sans appui : 6 triangles
        let mut t2 = vec![wall(0.15, 1.0), wall(-0.15, -1.0)];
        apply_openings(&mut t2, &[0, 1], &[opening(0.6, false)]);
        let count2: usize = t2.iter().map(|m| m.idx.len() / 3).sum();
        let count1: usize = thick.iter().map(|m| m.idx.len() / 3).sum();
        assert_eq!(count1 - count2, 2);
        // mur mince (une seule face) : pas d'embrasure
        let mut thin = vec![wall(0.0, 1.0)];
        apply_openings(&mut thin, &[0], &[opening(0.6, true)]);
        assert!((area(&thin[0]) - 38.0).abs() < 1e-3);
    }

    #[test]
    fn modele_de_la_vue() {
        let mut m = RModel {
            verts: [(-5.0, 0.0), (5.0, 0.0), (5.0, 4.0), (-5.0, 4.0)].iter().map(|&(x, z)| RVert { pos: [x, 0.0, z], nrm: [0.0, 1.0, 0.0], uv: [x, z], col: [200, 100, 50, 255], uv1: [0.0, 0.0] }).collect(),
            idx: vec![0, 1, 2, 0, 2, 3, 0, 1, 2],
            geoms: vec![RGeom { first: 0, count: 6, tex: 7, cutout: false, blend: 0, glass: false, ..Default::default() }, RGeom { first: 6, count: 3, tex: 8, cutout: true, blend: 0, glass: false, ..Default::default() }],
            embedded_dict: None,
        };
        let n = apply_to_rmodel(&mut m, &[opening(0.4, false)]);
        assert!(n >= 2);
        // les parties gardent leur matériau, leurs plages se suivent et couvrent tous les indices
        assert_eq!(m.geoms.len(), 2);
        assert_eq!((m.geoms[0].tex, m.geoms[1].tex, m.geoms[1].cutout), (7, 8, true));
        assert_eq!(m.geoms[0].first, 0);
        assert_eq!(m.geoms[1].first, m.geoms[0].count);
        assert_eq!((m.geoms[0].count + m.geoms[1].count) as usize, m.idx.len());
        assert!(m.idx.iter().all(|&i| (i as usize) < m.verts.len()));
        assert_eq!(m.verts[0].col, [200, 100, 50, 255]);
    }

    const XML: &str = "<Drawable>
 <DrawableModelsHigh>
  <Item>
   <Geometries>
    <Item>
     <ShaderIndex value=\"0\" />
     <BoundingBoxMin x=\"-5\" y=\"0\" z=\"0\" w=\"0\" />
     <BoundingBoxMax x=\"5\" y=\"0\" z=\"4\" w=\"0\" />
     <VertexBuffer>
      <Flags value=\"0\" />
      <Layout type=\"GTAV1\">
       <Position />
       <Normal />
       <Colour0 />
       <TexCoord0 />
       <Tangent />
      </Layout>
      <Data>
       -5 0 0   0 1 0   200 4 0 255   -5 0   1 0 0 1
       5 0 0   0 1 0   200 4 0 255   5 0   1 0 0 1
       5 0 4   0 1 0   200 4 0 255   5 4   1 0 0 1
       -5 0 4   0 1 0   200 4 0 255   -5 4   1 0 0 1
      </Data>
     </VertexBuffer>
     <IndexBuffer>
      <Data>
       0 1 2 0 2 3
      </Data>
     </IndexBuffer>
    </Item>
    <Item>
     <ShaderIndex value=\"1\" />
     <VertexBuffer>
      <Layout type=\"GTAV1\">
       <Position />
       <Mystere />
      </Layout>
      <Data>
       1 2 3 4
      </Data>
     </VertexBuffer>
     <IndexBuffer>
      <Data>
       0 0 0
      </Data>
     </IndexBuffer>
    </Item>
   </Geometries>
  </Item>
 </DrawableModelsHigh>
</Drawable>";

    #[test]
    fn fichier_xml_de_codewalker() {
        let (out, n) = apply_to_ydr_xml(XML, &[opening(0.4, false)]).expect("xml");
        assert!(n >= 1);
        // la partie inconnue est intacte, la première a plus de sommets et d'indices
        assert!(out.contains("       1 2 3 4\n"));
        assert!(out.contains("<Mystere />"));
        let (l0, l1) = (out.find("<Data>").unwrap(), out.find("</Data>").unwrap());
        let nverts = out[l0..l1].lines().filter(|l| l.trim().split_whitespace().count() == 16).count();
        assert!(nverts > 4, "sommets créés par la coupe : {nverts}");
        // chaque ligne de sommet a 16 nombres (3 + 3 + 4 + 2 + 4) et la couleur reste entière
        for l in out[l0..l1].lines().filter(|l| l.trim().split_whitespace().count() == 16) {
            let t: Vec<&str> = l.split_whitespace().collect();
            for k in 6..10 {
                assert!(t[k].parse::<i32>().is_ok(), "couleur non entière : {l}");
            }
        }
        // les indices écrits renvoient à des sommets existants
        let i0 = out.find("<IndexBuffer>").unwrap();
        let idx_text = &out[i0..];
        let idx_end = idx_text.find("</IndexBuffer>").unwrap();
        let idx: Vec<usize> = idx_text[..idx_end].lines().skip(2).flat_map(|l| l.split_whitespace().filter_map(|s| s.parse().ok()).collect::<Vec<usize>>()).collect();
        assert!(!idx.is_empty() && idx.len() % 3 == 0 && idx.iter().all(|&k| k < nverts));
        // sans ouverture : rien
        assert!(apply_to_ydr_xml(XML, &[]).is_none());
    }
    /// Deux parties : la première en GTAV2 (sans couleur, tangente), la seconde en GTAV1 ; l'IndexBuffer de chaque
    /// partie a aussi son <Data>.
    const XML2: &str = "<Drawable>
 <BoundingBoxMin x=\"-5\" y=\"0\" z=\"0\" />
 <BoundingBoxMax x=\"5\" y=\"0\" z=\"4\" />
 <ShaderGroup>
 </ShaderGroup>
 <DrawableModelsHigh>
  <Item>
   <HasSkin value=\"0\" />
   <BoneIndex value=\"0\" />
   <Geometries>
    <Item>
     <ShaderIndex value=\"0\" />
     <BoundingBoxMin x=\"-5\" y=\"0\" z=\"0\" w=\"0\" />
     <BoundingBoxMax x=\"5\" y=\"0\" z=\"4\" w=\"0\" />
     <VertexBuffer>
      <Flags value=\"0\" />
      <Layout type=\"GTAV2\">
       <Position />
       <Normal />
       <TexCoord0 />
       <TexCoord1 />
       <Tangent />
      </Layout>
      <Data2>
       -5 0 0   0 1 0   -5 0   0 0   1 0 0 1
       5 0 0   0 1 0   5 0   1 0   1 0 0 1
       5 0 4   0 1 0   5 4   1 1   1 0 0 1
       -5 0 4   0 1 0   -5 4   0 1   1 0 0 1
      </Data2>
     </VertexBuffer>
     <IndexBuffer>
      <Data>
       0 1 2 0 2 3
      </Data>
     </IndexBuffer>
    </Item>
    <Item>
     <ShaderIndex value=\"1\" />
     <BoundingBoxMin x=\"20\" y=\"20\" z=\"0\" w=\"0\" />
     <BoundingBoxMax x=\"21\" y=\"21\" z=\"0\" w=\"0\" />
     <VertexBuffer>
      <Layout type=\"GTAV1\">
       <Position />
       <Normal />
       <Colour0 />
       <TexCoord0 />
      </Layout>
      <Data>
       20 20 0   0 0 1   255 0 0 255   0 0
       21 20 0   0 0 1   255 0 0 255   1 0
       21 21 0   0 0 1   255 0 0 255   1 1
      </Data>
     </VertexBuffer>
     <IndexBuffer>
      <Data>
       0 1 2
      </Data>
     </IndexBuffer>
    </Item>
   </Geometries>
  </Item>
 </DrawableModelsHigh>
</Drawable>";

    #[test]
    fn disposition_autre_que_gtav1_lue_et_modifiee() {
        let x = read_ydr_xml(XML2).expect("xml");
        assert!(x.skipped.is_empty(), "{:?}", x.skipped);
        assert_eq!(x.geoms.len(), 2);
        let l = &x.geoms[0].mesh.layout;
        // Position 3 + Normal 3 + TexCoord0 2 + TexCoord1 2 + Tangent 4 (pas la disposition de la partie suivante)
        assert_eq!(l.int.len(), 14);
        assert_eq!((l.pos, l.nrm, l.uv, l.tan, l.uvx.clone()), (0, Some(3), Some(6), Some(10), vec![8]));
        assert_eq!(x.geoms[0].mesh.verts.len(), 4, "sommets lus dans <Data2>, pas dans le <Data> des indices");
        assert_eq!(x.geoms[0].mesh.idx, vec![0, 1, 2, 0, 2, 3]);
        assert_eq!(x.geoms[1].mesh.layout.int.len(), 12);
        // l'ouverture coupe bien la partie GTAV2
        let (out, n) = apply_to_ydr_xml(XML2, &[opening(0.4, false)]).expect("ouvertures");
        assert!(n >= 1);
        let y = read_ydr_xml(&out).expect("relu");
        assert!(y.geoms[0].mesh.verts.len() > 4);
        assert!(y.geoms[0].mesh.verts.iter().all(|v| v.len() == 14));
        assert!(out.contains("<Data2>"));
    }

    #[test]
    fn partie_de_disposition_inconnue_signalee() {
        let x = read_ydr_xml(XML).expect("xml");
        assert_eq!(x.geoms.len(), 1);
        assert_eq!(x.skipped.len(), 1);
        assert!(x.skipped[0].contains("Mystere"), "{:?}", x.skipped);
    }

    #[test]
    fn partie_de_plus_de_65535_sommets_decoupee() {
        // limite abaissée à 4 sommets : les 2 triangles d'un mur coupé (plus de 4 sommets) sont répartis en
        // plusieurs parties, chacune avec ses indices < 4 et sa boîte
        let x = read_ydr_xml(XML).expect("xml");
        let mut meshes: Vec<GMesh> = x.geoms.iter().map(|g| g.mesh.clone()).collect();
        let b = opening(0.4, false).obox();
        cut_mesh(&mut meshes[0], &[b]);
        let tris_before = meshes[0].idx.len() / 3;
        let out = write_ydr_xml_max(&x, &meshes, 4);
        let y = read_ydr_xml(&out).expect("relu");
        assert!(y.geoms.len() > 2, "partie découpée : {} parties", y.geoms.len());
        let mut tris_after = 0;
        for g in &y.geoms {
            assert!(g.mesh.verts.len() <= 4);
            assert!(g.mesh.idx.iter().all(|&k| (k as usize) < g.mesh.verts.len()));
            tris_after += g.mesh.idx.len() / 3;
        }
        assert_eq!(tris_after, tris_before);
        assert_eq!(out.matches("<ShaderIndex value=\"0\" />").count(), y.geoms.len());
    }

    #[test]
    fn partie_videe_retiree_sans_boite_geante() {
        // la seconde partie de XML2 vidée : retirée ; la première vidée aussi : seule de sa liste, réduite à un
        // triangle nul
        let x = read_ydr_xml(XML2).expect("xml");
        let mut meshes: Vec<GMesh> = x.geoms.iter().map(|g| g.mesh.clone()).collect();
        meshes[1].idx.clear();
        let out = write_ydr_xml(&x, &meshes);
        assert!(!out.contains("3e38") && !out.contains("300000000"), "{out}");
        let y = read_ydr_xml(&out).expect("relu");
        assert_eq!(y.geoms.len(), 1);
        assert!(!out.contains("<ShaderIndex value=\"1\" />"));
        // seule partie restante vidée : gardée avec un triangle nul
        let mut m1 = vec![y.geoms[0].mesh.clone()];
        m1[0].idx.clear();
        let out2 = write_ydr_xml(&y, &m1);
        assert!(!out2.contains("e38"), "{out2}");
        let z = read_ydr_xml(&out2).expect("relu");
        assert_eq!(z.geoms.len(), 1);
        assert_eq!(z.geoms[0].mesh.idx, vec![0, 0, 0]);
        assert_eq!(z.geoms[0].mesh.verts.len(), 1);
    }

    #[test]
    fn partie_attachee_a_un_os_repere_du_modele() {
        // os 1 : tourné de 90° autour de z et décalé de (10, 0, 0) ; la partie y est attachée : le XML garde ses
        // sommets dans le repère de la partie, DragoWalker les lit (et les modifie) dans celui du modèle
        let s = (0.5f32).sqrt();
        let xml = XML2
            .replace("<BoneIndex value=\"0\" />", "<BoneIndex value=\"1\" />")
            .replace(" <ShaderGroup>", &format!(" <Skeleton>\n  <Bones>\n   <Item>\n    <ParentIndex value=\"-1\" />\n    <Translation x=\"0\" y=\"0\" z=\"0\" />\n    <Rotation x=\"0\" y=\"0\" z=\"0\" w=\"1\" />\n   </Item>\n   <Item>\n    <ParentIndex value=\"0\" />\n    <Translation x=\"10\" y=\"0\" z=\"0\" />\n    <Rotation x=\"0\" y=\"0\" z=\"{s}\" w=\"{s}\" />\n   </Item>\n  </Bones>\n </Skeleton>\n <ShaderGroup>"));
        assert!(xml.contains("<BoneIndex value=\"1\" />"));
        let x = read_ydr_xml(&xml).expect("xml");
        assert!(x.geoms[0].bone.is_some());
        // (5, 0, 0) de la partie → (10, 5, 0) dans le modèle ; normale (0, 1, 0) → (-1, 0, 0)
        let m = &x.geoms[0].mesh;
        assert!((pos_of(&m.layout, &m.verts[1]) - v3(10.0, 5.0, 0.0)).len() < 1e-4, "{:?}", pos_of(&m.layout, &m.verts[1]));
        assert!((v3(m.verts[1][3], m.verts[1][4], m.verts[1][5]) - v3(-1.0, 0.0, 0.0)).len() < 1e-4);
        // un sommet déplacé dans le repère du modèle revient dans celui de la partie à l'écriture
        let mut meshes: Vec<GMesh> = x.geoms.iter().map(|g| g.mesh.clone()).collect();
        meshes[0].verts[1][2] = 1.0;
        let out = write_ydr_xml(&x, &meshes);
        let row: Vec<f32> = out.lines().skip_while(|l| l.trim() != "<Data2>").nth(2).unwrap().split_whitespace().filter_map(|s| s.parse().ok()).collect();
        assert!((v3(row[0], row[1], row[2]) - v3(5.0, 0.0, 1.0)).len() < 1e-4, "repère de la partie : {row:?}");
        let y = read_ydr_xml(&out).expect("relu");
        assert!((pos_of(&y.geoms[0].mesh.layout, &y.geoms[0].mesh.verts[1]) - v3(10.0, 5.0, 1.0)).len() < 1e-4);
    }
}
