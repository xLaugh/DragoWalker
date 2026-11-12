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
#[derive(Clone, Debug)]
pub struct Layout {
    pub int: Vec<bool>,
    pub pos: usize,
    pub nrm: Option<usize>,
}

pub type Vx = Vec<f32>;

#[derive(Clone, Debug)]
pub struct GMesh {
    pub layout: Layout,
    pub verts: Vec<Vx>,
    pub idx: Vec<u32>,
}

fn pos_of(l: &Layout, v: &Vx) -> V3 {
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
    Layout { int, pos: 0, nrm: Some(3) }
}

fn to_vx(v: &RVert) -> Vx {
    vec![v.pos[0], v.pos[1], v.pos[2], v.nrm[0], v.nrm[1], v.nrm[2], v.col[0] as f32, v.col[1] as f32, v.col[2] as f32, v.col[3] as f32, v.uv[0], v.uv[1], v.uv1[0], v.uv1[1]]
}

fn from_vx(v: &Vx) -> RVert {
    let c = |x: f32| x.round().clamp(0.0, 255.0) as u8;
    RVert { pos: [v[0], v[1], v[2]], nrm: [v[3], v[4], v[5]], col: [c(v[6]), c(v[7]), c(v[8]), c(v[9])], uv: [v[10], v[11]], uv1: [v[12], v[13]] }
}

/// Applique les ouvertures à un modèle de la vue : mêmes parties, sommets ajoutés. Rend le nombre de triangles
/// découpés.
pub fn apply_to_rmodel(m: &mut RModel, openings: &[Opening]) -> usize {
    if openings.is_empty() {
        return 0;
    }
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
    let primary: Vec<usize> = (0..meshes.len()).collect();
    let changed = apply_openings(&mut meshes, &primary, openings);
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

/// Une partie (<Item> avec <VertexBuffer> et <IndexBuffer>) du XML.
struct XGeom {
    gs: usize,
    vb: usize,
    vd0: usize,
    vd1: usize,
    id0: usize,
    id1: usize,
    primary: bool,
    mesh: GMesh,
}

/// Applique les ouvertures à un .ydr.xml lu par CodeWalker : tous les niveaux de détail sont coupés, les
/// embrasures sont ajoutées au niveau le plus fin (DrawableModelsHigh). Une partie dont la disposition des
/// sommets est inconnue est laissée telle quelle. Rend le XML et le nombre de triangles découpés.
pub fn apply_to_ydr_xml(src: &str, openings: &[Opening]) -> Option<(String, usize)> {
    if openings.is_empty() {
        return None;
    }
    let lines: Vec<&str> = src.lines().collect();
    let ind = |l: &str| l.len() - l.trim_start().len();
    let find = |from: usize, what: &str| (from..lines.len()).find(|&k| lines[k].trim() == what);
    // niveau le plus fin
    let high = lines.iter().position(|l| l.trim() == "<DrawableModelsHigh>").and_then(|a| (a..lines.len()).find(|&k| lines[k].trim() == "</DrawableModelsHigh>").map(|b| (a, b)));
    let mut geoms: Vec<XGeom> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() != "<VertexBuffer>" {
            i += 1;
            continue;
        }
        let vb = i;
        let Some(gs) = (0..vb).rev().find(|&k| lines[k].trim() == "<Item>") else { i += 1; continue };
        let gind = ind(lines[gs]);
        let (Some(vd0), Some(ib)) = (find(vb, "<Data>").or_else(|| find(vb, "<Data2>")), find(vb, "<IndexBuffer>")) else { return None };
        let vd1 = (vd0 + 1..lines.len()).find(|&k| lines[k].trim().starts_with("</Data"))?;
        let id0 = find(ib, "<Data>")?;
        let id1 = find(id0, "</Data>")?;
        let ge = (id1..lines.len()).find(|&k| lines[k].trim() == "</Item>" && ind(lines[k]) == gind)?;
        i = ge + 1;
        // disposition des sommets
        let (Some(l0), Some(l1)) = (find(vb, "<Layout type=\"GTAV1\">").or_else(|| (vb..vd0).find(|&k| lines[k].trim().starts_with("<Layout"))), (vb..vd0).find(|&k| lines[k].trim() == "</Layout>")) else { continue };
        let (mut int, mut off) = (Vec::new(), 0usize);
        let (mut pos, mut nrm) = (None, None);
        let mut ok = true;
        for k in l0 + 1..l1 {
            let name = lines[k].trim().trim_start_matches('<').split(|c: char| c == ' ' || c == '/' || c == '>').next().unwrap_or("");
            let Some((n, is_int)) = channel(name) else { ok = false; break };
            match name {
                "Position" => pos = Some(off),
                "Normal" => nrm = Some(off),
                _ => {}
            }
            int.extend(std::iter::repeat(is_int).take(n));
            off += n;
        }
        let Some(pos) = pos else { continue };
        if !ok {
            continue;
        }
        // sommets et indices
        let mut verts: Vec<Vx> = Vec::new();
        for l in &lines[vd0 + 1..vd1] {
            if l.trim().is_empty() {
                continue;
            }
            let v: Vx = l.split_whitespace().filter_map(|s| s.parse::<f32>().ok()).collect();
            if v.len() != off {
                ok = false;
                break;
            }
            verts.push(v);
        }
        if !ok {
            continue;
        }
        let idx: Vec<u32> = lines[id0 + 1..id1].iter().flat_map(|l| l.split_whitespace().filter_map(|s| s.parse::<u32>().ok())).collect();
        let primary = high.map(|(a, b)| gs > a && ge < b).unwrap_or(true);
        geoms.push(XGeom { gs, vb, vd0, vd1, id0, id1, primary, mesh: GMesh { layout: Layout { int, pos, nrm }, verts, idx } });
    }
    if geoms.is_empty() {
        return None;
    }
    let before: Vec<usize> = geoms.iter().map(|g| g.mesh.idx.len()).collect();
    let verts_before: Vec<usize> = geoms.iter().map(|g| g.mesh.verts.len()).collect();
    let primary: Vec<usize> = geoms.iter().enumerate().filter(|(_, g)| g.primary).map(|(i, _)| i).collect();
    let mut meshes: Vec<GMesh> = geoms.iter().map(|g| g.mesh.clone()).collect();
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
    // réécriture : seulement les parties modifiées
    let mut out: Vec<Option<String>> = lines.iter().map(|l| Some(l.to_string())).collect();
    for (gi, g) in geoms.iter().enumerate() {
        let m = &meshes[gi];
        if m.idx.len() == before[gi] && m.verts.len() == verts_before[gi] {
            continue;
        }
        // sommets utilisés, renumérotés
        let mut remap: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut order: Vec<u32> = Vec::new();
        let mut new_idx: Vec<u32> = Vec::new();
        for &k in &m.idx {
            let n = *remap.entry(k).or_insert_with(|| {
                order.push(k);
                order.len() as u32 - 1
            });
            new_idx.push(n);
        }
        let (mut mn, mut mx) = (v3(3.0e38, 3.0e38, 3.0e38), v3(-3.0e38, -3.0e38, -3.0e38));
        for &k in &order {
            let p = pos_of(&m.layout, &m.verts[k as usize]);
            mn = mn.min(p);
            mx = mx.max(p);
        }
        for k in g.gs..g.vb {
            let t = lines[k].trim_start();
            let pad: String = lines[k].chars().take_while(|c| c.is_whitespace()).collect();
            if t.starts_with("<BoundingBoxMin ") {
                out[k] = Some(format!("{pad}<BoundingBoxMin x=\"{}\" y=\"{}\" z=\"{}\" w=\"0\" />", mn.x, mn.y, mn.z));
            } else if t.starts_with("<BoundingBoxMax ") {
                out[k] = Some(format!("{pad}<BoundingBoxMax x=\"{}\" y=\"{}\" z=\"{}\" w=\"0\" />", mx.x, mx.y, mx.z));
            }
        }
        let vpad: String = lines.get(g.vd0 + 1).map(|l| l.chars().take_while(|c| c.is_whitespace()).collect()).unwrap_or_default();
        let fmt = |v: &Vx| -> String {
            v.iter().enumerate().map(|(i, x)| if m.layout.int[i] { format!("{}", x.round().clamp(0.0, 255.0) as i32) } else { format!("{x}") }).collect::<Vec<_>>().join(" ")
        };
        for o in out.iter_mut().take(g.vd1).skip(g.vd0 + 1) {
            *o = None;
        }
        out[g.vd0 + 1] = Some(order.iter().map(|&k| format!("{vpad}{}", fmt(&m.verts[k as usize]))).collect::<Vec<_>>().join("\n"));
        for o in out.iter_mut().take(g.id1).skip(g.id0 + 1) {
            *o = None;
        }
        let ipad: String = lines[g.id0].chars().take_while(|c| c.is_whitespace()).collect();
        out[g.id0 + 1] = Some(new_idx.chunks(24).map(|c| format!("{ipad} {}", c.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" "))).collect::<Vec<_>>().join("\n"));
    }
    let text = out.into_iter().flatten().collect::<Vec<_>>().join("\n") + "\n";
    Some((text, changed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lay() -> Layout {
        // position, normale, uv
        Layout { int: vec![false; 8], pos: 0, nrm: Some(3) }
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
}
